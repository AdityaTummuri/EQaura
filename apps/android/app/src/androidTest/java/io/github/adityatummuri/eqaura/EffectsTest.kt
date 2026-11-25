package io.github.adityatummuri.eqaura

import android.Manifest
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.media.audiofx.Visualizer
import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.rule.GrantPermissionRule
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ln
import kotlin.math.sin
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Plays tones through the real audio system and measures what comes out. */
@RunWith(AndroidJUnit4::class)
class EffectsTest {
    @get:Rule
    val permission: GrantPermissionRule = GrantPermissionRule.grant(Manifest.permission.RECORD_AUDIO)

    private val effects = Effects()
    private val flat = Profile()
    private val shaped = Profile(
        filters = listOf(
            Filter(Kind.LowShelf, 105.0, 5.0, 0.7),
            Filter(Kind.Peak, 1000.0, -6.0, 1.4),
            Filter(Kind.Peak, 3200.0, 4.0, 2.0),
            Filter(Kind.HighShelf, 9000.0, -5.0, 0.7),
        ),
    )

    @Before
    fun volume() {
        val audio = InstrumentationRegistry.getInstrumentation().targetContext
            .getSystemService(AudioManager::class.java)
        val loudest = audio.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        audio.setStreamVolume(AudioManager.STREAM_MUSIC, loudest, 0)
    }

    @After
    fun cleanUp() {
        effects.apply(null, Coverage.AllApps)
    }

    private fun sound(p: Profile, detail: Detail, compare: Boolean = false) =
        Sound(Core.engine(p, detail.block, RATE.toDouble(), compare), detail)

    /** A steady tone in one or both ears. */
    private fun play(freq: Double, left: Boolean = true, right: Boolean = true): AudioTrack {
        // A whole number of periods loops without a click.
        val data = FloatArray(RATE * 2)
        for (i in 0 until RATE) {
            val v = (0.25 * sin(2 * PI * freq * i / RATE)).toFloat()
            if (left) data[2 * i] = v
            if (right) data[2 * i + 1] = v
        }
        val track = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build())
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                    .setSampleRate(RATE)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                    .build(),
            )
            .setTransferMode(AudioTrack.MODE_STATIC)
            .setBufferSizeInBytes(data.size * Float.SIZE_BYTES)
            .build()
        track.write(data, 0, data.size, AudioTrack.WRITE_BLOCKING)
        track.setLoopPoints(0, RATE, -1)
        track.play()
        return track
    }

    /** Loudness of everything the device plays, in dB. */
    private fun measure(): Double {
        val v = Visualizer(0)
        try {
            v.enabled = false
            v.scalingMode = Visualizer.SCALING_MODE_AS_PLAYED
            v.measurementMode = Visualizer.MEASUREMENT_MODE_PEAK_RMS
            v.enabled = true
            Thread.sleep(900)
            val m = Visualizer.MeasurementPeakRms()
            assertEquals(Visualizer.SUCCESS, v.getMeasurementPeakRms(m))
            return m.mRms / 100.0
        } finally {
            v.release()
        }
    }

    private fun expected(p: Profile, freq: Double): Double {
        val a = Core.analyze(p, null)
        val x = ln(freq / 20.0) / ln(1000.0) * (Core.POINTS - 1)
        val i = x.toInt()
        return a.left[i] + (a.left[i + 1] - a.left[i]) * (x - i) + a.preampDb
    }

    @Test
    fun soundFollowsTheCurve() {
        val problems = mutableListOf<String>()
        for (detail in Detail.entries) {
            for (freq in listOf(60.0, 250.0, 1000.0, 3200.0, 12_000.0)) {
                val track = play(freq)
                try {
                    effects.open(track.audioSessionId)
                    assertEquals(EffectState.Active, effects.apply(sound(flat, detail), Coverage.PerApp))
                    val before = measure()
                    assertTrue("silence: $before", before > -60)
                    // Also proves that a running effect takes new settings.
                    assertEquals(EffectState.Active, effects.apply(sound(shaped, detail), Coverage.PerApp))
                    val change = measure() - before
                    val want = expected(shaped, freq)
                    val line = "$detail $freq Hz: measured %.2f dB, curve %.2f dB".format(change, want)
                    Log.i(TAG, line)
                    // Wide bins cannot follow the steepest bass, so leave them more room.
                    val room = if (freq < 100 && detail != Detail.FineBass) 1.5 else 0.8
                    if (abs(change - want) >= room) problems += line
                } finally {
                    effects.close(track.audioSessionId)
                    track.release()
                }
            }
        }
        assertTrue(problems.joinToString("\n"), problems.isEmpty())
    }

    @Test
    fun earsAreIndependent() {
        val p = Profile(
            preamp = Preamp(false, 0.0),
            filters = listOf(Filter(Kind.Peak, 1000.0, -8.0, 1.0, channel = Ear.Left)),
        )
        for (left in listOf(true, false)) {
            val track = play(1000.0, left, !left)
            try {
                effects.open(track.audioSessionId)
                effects.apply(sound(flat, Detail.Balanced), Coverage.PerApp)
                val before = measure()
                effects.apply(sound(p, Detail.Balanced), Coverage.PerApp)
                assertEquals(if (left) -8.0 else 0.0, measure() - before, 0.8)
            } finally {
                effects.close(track.audioSessionId)
                track.release()
            }
        }
    }

    @Test
    fun compareKeepsOnlyThePreamp() {
        val track = play(1000.0)
        try {
            effects.open(track.audioSessionId)
            effects.apply(sound(flat, Detail.Balanced), Coverage.PerApp)
            val before = measure()
            effects.apply(sound(shaped, Detail.Balanced, compare = true), Coverage.PerApp)
            assertEquals(Core.analyze(shaped, null).preampDb, measure() - before, 0.5)
        } finally {
            track.release()
        }
    }

    @Test
    fun oneEffectForAllApps() {
        val track = play(1000.0)
        try {
            assertEquals(EffectState.Active, effects.apply(sound(flat, Detail.Balanced), Coverage.AllApps))
            assertEquals(EffectState.Active, effects.apply(sound(shaped, Detail.LowDelay), Coverage.AllApps))
            assertEquals(EffectState.Waiting, effects.apply(sound(shaped, Detail.LowDelay), Coverage.PerApp))
            assertEquals(EffectState.Off, effects.apply(null, Coverage.PerApp))
        } finally {
            track.release()
        }
    }

    @Test
    fun oddSessionsDoNoHarm() {
        effects.apply(sound(shaped, Detail.Balanced), Coverage.PerApp)
        for (id in listOf(0, -4, 123_456_789)) effects.open(id)
        effects.close(987_654_321)
        effects.sync()
    }

    private companion object {
        const val RATE = 48_000
    }
}
