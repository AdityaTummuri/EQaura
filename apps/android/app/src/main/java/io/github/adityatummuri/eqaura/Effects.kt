package io.github.adityatummuri.eqaura

import android.media.audiofx.AudioEffect
import android.media.audiofx.DynamicsProcessing
import android.util.Log

/** What the effects should sound like. */
data class Sound(val bands: EngineBands, val detail: Detail)

enum class EffectState {
    Off,
    Active,

    /** Per app, and no player has announced itself yet. */
    Waiting,

    /** Another equalizer app holds the effect. */
    Taken,

    /** This phone refused the effect. */
    Failed,
}

/** One DynamicsProcessing effect on one audio session. */
class Fx(session: Int, sound: Sound) : AutoCloseable {
    private val bands = sound.bands
    private val detail = sound.detail
    private var now = bands
    private val effect: DynamicsProcessing

    init {
        val count = bands.cutoffs.size
        val config = DynamicsProcessing.Config.Builder(
            DynamicsProcessing.VARIANT_FAVOR_FREQUENCY_RESOLUTION, CHANNELS,
            true, count, false, 0, false, 0, false,
        ).setPreferredFrameDuration(detail.frameMs)
            .setInputGainAllChannelsTo(bands.preampDb)
        for (channel in 0 until CHANNELS) {
            val eq = DynamicsProcessing.Eq(true, true, count)
            gains(bands, channel).forEachIndexed { i, gain ->
                eq.setBand(i, DynamicsProcessing.EqBand(true, bands.cutoffs[i], gain))
            }
            config.setPreEqByChannelIndex(channel, eq)
        }
        effect = DynamicsProcessing(PRIORITY, session, config.build())
        try {
            check(effect.setEnabled(true) == AudioEffect.SUCCESS) { "not enabled" }
        } catch (e: RuntimeException) {
            effect.release()
            throw e
        }
    }

    private fun gains(b: EngineBands, channel: Int) = if (channel == 0) b.left else b.right

    val inControl get() = effect.hasControl() && effect.enabled

    /** False when a different band layout needs a new effect. */
    fun update(sound: Sound): Boolean {
        val new = sound.bands
        if (sound.detail != detail || new.cutoffs != now.cutoffs) return false
        if (new == now) return true
        // Quieter first, so a change never passes through a loud moment.
        val quieter = new.preampDb <= now.preampDb
        if (quieter) effect.setInputGainAllChannelsTo(new.preampDb)
        for (channel in 0 until CHANNELS) {
            val old = gains(now, channel)
            gains(new, channel).forEachIndexed { i, gain ->
                if (gain != old[i]) {
                    effect.setPreEqBandByChannelIndex(
                        channel, i, DynamicsProcessing.EqBand(true, new.cutoffs[i], gain),
                    )
                }
            }
        }
        if (!quieter) effect.setInputGainAllChannelsTo(new.preampDb)
        now = new
        return true
    }

    override fun close() {
        try {
            effect.release()
        } catch (e: RuntimeException) {
            Log.w(TAG, "release", e)
        }
    }

    private companion object {
        const val CHANNELS = 2

        /** Ahead of the equalizers that players bring along. */
        const val PRIORITY = 1000
    }
}

/** All effects of the app. Not thread-safe: use from one thread. */
class Effects {
    private val active = mutableMapOf<Int, Fx>()
    private val announced = linkedSetOf<Int>()
    private var sound: Sound? = null
    private var coverage = Coverage.AllApps
    private var failed = false

    /** Makes [sound] audible; `null` removes every effect. */
    fun apply(sound: Sound?, coverage: Coverage): EffectState {
        if (coverage != this.coverage) releaseAll()
        this.sound = sound
        this.coverage = coverage
        return sync()
    }

    fun open(session: Int): EffectState {
        if (session != GLOBAL) {
            announced.remove(session)
            announced.add(session)
            // Players that never say goodbye must not pile up.
            while (announced.size > MAX_SESSIONS) {
                val oldest = announced.first()
                announced.remove(oldest)
                active.remove(oldest)?.close()
            }
        }
        return sync()
    }

    fun close(session: Int): EffectState {
        announced.remove(session)
        if (session != GLOBAL) active.remove(session)?.close()
        return sync()
    }

    /** Also repairs effects that the audio system dropped. */
    fun sync(): EffectState {
        val sound = this.sound
        if (sound == null) {
            releaseAll()
            return EffectState.Off
        }
        val wanted = if (coverage == Coverage.AllApps) setOf(GLOBAL) else announced.toSet()
        (active.keys - wanted).forEach { active.remove(it)?.close() }
        failed = false
        wanted.forEach { keep(it, sound) }
        return when {
            active.values.any { it.inControlOrNull() == true } -> EffectState.Active
            active.isNotEmpty() -> EffectState.Taken
            failed -> EffectState.Failed
            wanted.isEmpty() -> EffectState.Waiting
            else -> EffectState.Failed
        }
    }

    private fun Fx.inControlOrNull(): Boolean? = try {
        inControl
    } catch (_: RuntimeException) {
        null
    }

    private fun keep(session: Int, sound: Sound) {
        val old = active[session]
        if (old != null) {
            val kept = try {
                // An effect that lost its engine throws or reports it is off.
                old.inControlOrNull() != null && old.update(sound)
            } catch (e: RuntimeException) {
                Log.w(TAG, "update", e)
                false
            }
            if (kept) return
            active.remove(session)
            old.close()
        }
        try {
            active[session] = Fx(session, sound)
        } catch (e: RuntimeException) {
            // Thrown for sessions that are gone, and on phones without the effect.
            Log.w(TAG, "create on $session", e)
            if (session != GLOBAL) announced.remove(session)
            failed = true
        }
    }

    private fun releaseAll() {
        active.values.forEach(Fx::close)
        active.clear()
    }

    private companion object {
        /** The session that carries everything the phone plays. */
        const val GLOBAL = 0
        const val MAX_SESSIONS = 24
    }
}
