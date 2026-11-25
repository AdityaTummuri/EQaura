package io.github.adityatummuri.eqaura

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack

/** Plays one hearing test tone at a time. */
class TonePlayer {
    private var track: AudioTrack? = null

    /** Returns how long the tone lasts, in milliseconds. */
    fun play(step: HearingStep): Long {
        stop()
        val samples = Core.tone(step.left, step.freq, step.db, RATE.toDouble())
        check(samples.isNotEmpty()) { "empty tone" }
        val t = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                    .setSampleRate(RATE)
                    .setChannelMask(AudioFormat.CHANNEL_OUT_STEREO)
                    .build(),
            )
            .setTransferMode(AudioTrack.MODE_STATIC)
            .setBufferSizeInBytes(samples.size * Float.SIZE_BYTES)
            .build()
        t.write(samples, 0, samples.size, AudioTrack.WRITE_BLOCKING)
        t.play()
        track = t
        return samples.size / 2 * 1000L / RATE
    }

    fun stop() {
        track?.release()
        track = null
    }

    private companion object {
        const val RATE = 48_000
    }
}
