package io.github.adityatummuri.eqaura

import android.content.Context
import android.os.Build
import android.util.Log
import java.io.File
import java.io.IOException
import java.util.concurrent.Executors
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

const val TAG = "EQaura"

enum class Tone { Neutral, Good, Warning, Danger, Busy }

/** A short message at the bottom of the screen. */
data class Toast(val text: String, val tone: Tone, val canUndo: Boolean)

data class Preset(val profile: Profile, val builtin: Boolean) {
    val name get() = profile.name
}

enum class ProposalState { None, Open, Applied, Undone }

data class ChatRow(
    val id: Long,
    val fromUser: Boolean,
    val text: String,
    val tone: Tone = Tone.Neutral,
    /** "Built-in rules" or the model's name. */
    val origin: String = "",
    /** What the suggestion changes, one line each. */
    val lines: List<String> = emptyList(),
    val proposal: ProposalState = ProposalState.None,
    val before: List<Filter> = emptyList(),
    val after: List<Filter> = emptyList(),
)

enum class HearingStage { Intro, Testing, Result }

data class HearingUi(
    val stage: HearingStage = HearingStage.Intro,
    val step: HearingStep? = null,
    val playing: Boolean = false,
    /** The measured or stored compensation per frequency. */
    val bars: List<EarBar> = emptyList(),
)

data class AiUi(
    val models: List<String> = emptyList(),
    val status: String = "",
    val tone: Tone = Tone.Neutral,
    val hasKey: Boolean = false,
    /** Exactly what a request would send. */
    val sends: String = "",
)

data class UiState(
    val settings: Settings,
    val output: Output,
    val profile: Profile = Profile(),
    /** The preset this sound came from. */
    val preset: String? = null,
    val dirty: Boolean = false,
    val analysis: Analysis = Analysis(),
    val effects: EffectState = EffectState.Off,
    val comparing: Boolean = false,
    val selected: Int? = null,
    val canUndo: Boolean = false,
    val canRedo: Boolean = false,
    val presets: List<Preset> = emptyList(),
    val devices: Map<String, DeviceState> = emptyMap(),
    val hearing: HearingUi = HearingUi(),
    val chat: List<ChatRow> = emptyList(),
    val ai: AiUi = AiUi(),
)

/** Undo and redo. Edits that share a key become one step until [seal]. */
class History {
    private val undo = ArrayDeque<Profile>()
    private val redo = ArrayDeque<Profile>()
    private var key: String? = null

    val canUndo get() = undo.isNotEmpty()
    val canRedo get() = redo.isNotEmpty()

    fun record(current: Profile, key: String?) {
        if (key != null && key == this.key) return
        this.key = key
        redo.clear()
        undo.addLast(current)
        if (undo.size > DEPTH) undo.removeFirst()
    }

    fun seal() {
        key = null
    }

    fun undo(current: Profile): Profile? {
        seal()
        return undo.removeLastOrNull()?.also { redo.addLast(current) }
    }

    fun redo(current: Profile): Profile? {
        seal()
        return redo.removeLastOrNull()?.also { undo.addLast(current) }
    }

    fun clear() {
        seal()
        undo.clear()
        redo.clear()
    }

    private companion object {
        const val DEPTH = 200
    }
}

/**
 * The app's state and everything the interface can do. One instance lives as
 * long as the process and is shared by the screens and the service. Call its
 * functions on the main thread.
 */
class AppModel(private val context: Context, dir: File = context.filesDir) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "effects") }.asCoroutineDispatcher()
    private val disk = Executors.newSingleThreadExecutor { Thread(it, "disk") }.asCoroutineDispatcher()

    private val store = Store(dir)
    private val keys = Keys(context)
    private val effects = Effects()
    private val player = TonePlayer()
    private val history = History()
    private val outputs = Outputs(context, ::outputChanged)

    private val toastFlow = MutableSharedFlow<Toast>(0, 4, BufferOverflow.DROP_OLDEST)
    val toasts: SharedFlow<Toast> = toastFlow.asSharedFlow()

    private val builtin = Core.builtin().map { Preset(it, true) }
    private var own = store.presets()

    private val mutable = MutableStateFlow(
        UiState(settings = store.settings(), output = outputs.current(), devices = store.devices()),
    )
    val state: StateFlow<UiState> = mutable.asStateFlow()
    private val now get() = mutable.value

    private data class Request(val profile: Profile?, val flat: Boolean, val coverage: Coverage, val detail: Detail)

    private val sound = MutableStateFlow<Request?>(null)
    private val drawing = MutableStateFlow<Pair<Profile, Int?>?>(null)

    private var saveJob: Job? = null
    private var testJob: Job? = null
    private var askJob: Job? = null
    private var modelsJob: Job? = null
    private var answers = emptyList<Boolean>()
    private var measured: HearingLayer? = null
    private var chatIds = 0L

    init {
        refreshPresets()
        loadOutput(now.output)
        scope.launch(worker) { sound.collect { if (it != null) play(it) } }
        scope.launch(Dispatchers.Default) {
            drawing.collect { request ->
                val (profile, selected) = request ?: return@collect
                val analysis = Core.analyze(profile, selected)
                mutable.update { it.copy(analysis = analysis) }
            }
        }
        refreshAi()
    }

    // ---- sound ----

    private fun play(r: Request) {
        val state = try {
            val bands = r.profile?.let { Core.engine(it, r.detail.block, outputs.rate, r.flat) }
            effects.apply(bands?.let { Sound(it, r.detail) }, r.coverage)
        } catch (e: CoreError) {
            Log.e(TAG, "engine", e)
            EffectState.Failed
        }
        mutable.update { it.copy(effects = state) }
    }

    /** Runs on the thread that owns the effects. */
    private fun onEffects(f: Effects.() -> EffectState) {
        scope.launch(worker) {
            val state = effects.f()
            mutable.update { it.copy(effects = state) }
        }
    }

    fun sessionOpened(id: Int) = onEffects { open(id) }

    fun sessionClosed(id: Int) = onEffects { close(id) }

    fun checkEffects() = onEffects { sync() }

    /** Makes the current state audible, draws it and remembers it. */
    private fun refresh() {
        val s = now
        // The hearing test must not be equalized.
        val on = s.settings.enabled && s.hearing.stage != HearingStage.Testing
        sound.value = Request(
            s.profile.takeIf { on }, s.comparing, s.settings.coverage, s.settings.detail,
        )
        drawing.value = s.profile to s.selected
        val source = s.presets.find { it.name == s.preset }?.profile
        val dirty = if (source != null) {
            source.filters != s.profile.filters || source.preamp != s.profile.preamp
        } else {
            s.profile.filters.isNotEmpty() || !s.profile.preamp.auto
        }
        val device = DeviceState(s.output.name, s.profile, s.preset)
        val devices = if (s.devices[s.output.id] == device) s.devices else s.devices + (s.output.id to device)
        if (devices !== s.devices) saveSoon()
        val chat = s.chat.map { row ->
            when {
                row.proposal == ProposalState.Applied && row.before == s.profile.filters ->
                    row.copy(proposal = ProposalState.Undone)
                row.proposal == ProposalState.Undone && row.after == s.profile.filters ->
                    row.copy(proposal = ProposalState.Applied)
                else -> row
            }
        }
        mutable.update {
            it.copy(
                dirty = dirty,
                devices = devices,
                chat = chat,
                canUndo = history.canUndo,
                canRedo = history.canRedo,
            )
        }
    }

    /** Changes the profile as one undoable step, or as part of the step named [key]. */
    private fun edit(key: String? = null, change: (Profile) -> Profile) {
        val next = change(now.profile)
        if (next == now.profile) return
        history.record(now.profile, key)
        mutable.update { it.copy(profile = next, comparing = false) }
        refresh()
    }

    /** Ends a drag or another continuous change, so the next one is its own undo step. */
    fun seal() = history.seal()

    fun setEnabled(on: Boolean) {
        if (on == now.settings.enabled) return
        changeSettings { it.copy(enabled = on) }
        if (on) EqService.start(context) else EqService.stop(context)
    }

    /** The app came to the screen: bring back what Android may have stopped. */
    fun appShown() {
        outputChanged()
        if (now.settings.enabled) {
            EqService.start(context)
            checkEffects()
        }
    }

    /** Listen without the filters, at the same volume. */
    fun setComparing(on: Boolean) {
        mutable.update { it.copy(comparing = on) }
        refresh()
    }

    fun undo() {
        val p = history.undo(now.profile) ?: return
        jump(p)
    }

    fun redo() {
        val p = history.redo(now.profile) ?: return
        jump(p)
    }

    private fun jump(p: Profile) {
        mutable.update {
            it.copy(
                profile = p,
                comparing = false,
                selected = it.selected?.takeIf { i -> i < p.filters.size },
            )
        }
        refresh()
        refreshBars()
    }

    // ---- bands ----

    fun select(band: Int?) {
        mutable.update { it.copy(selected = band?.takeIf { i -> i in it.profile.filters.indices }) }
        drawing.value = now.profile to now.selected
    }

    /** Continuous changes such as drags pass a [key] and end with [seal]. */
    fun changeBand(i: Int, key: String? = null, change: (Filter) -> Filter) {
        if (i !in now.profile.filters.indices) return
        edit(key) { p ->
            p.copy(filters = p.filters.mapIndexed { n, f -> if (n == i) change(f) else f })
        }
    }

    fun dragBand(i: Int, freqHz: Double, gainDb: Double) = changeBand(i, "drag $i") { f ->
        f.copy(
            freqHz = roundFreq(freqHz),
            gainDb = if (f.kind.hasGain) roundGain(gainDb) else f.gainDb,
        )
    }

    fun addBand(freqHz: Double = 1000.0, gainDb: Double = 0.0, kind: Kind = Kind.Peak) {
        seal()
        mutable.update { it.copy(selected = it.profile.filters.size) }
        val q = if (kind == Kind.Peak) 1.0 else 0.71
        edit { p -> p.copy(filters = p.filters + Filter(kind, roundFreq(freqHz), roundGain(gainDb), q)) }
    }

    fun removeBand(i: Int) {
        seal()
        mutable.update { it.copy(selected = null) }
        edit { p -> p.copy(filters = p.filters.filterIndexed { n, _ -> n != i }) }
    }

    fun clearBands() {
        seal()
        mutable.update { it.copy(selected = null, preset = null) }
        edit { p -> p.copy(filters = emptyList()) }
        toast("All bands removed", canUndo = true)
    }

    /** Reads a typed number; `null` when it is not one. */
    fun number(text: String): Double? = Core.number(text)

    fun setSlider(i: Int, gainDb: Float) = edit("slider $i") { p ->
        p.copy(filters = Core.slider(p.filters, i, roundGain(gainDb.toDouble())))
    }

    fun setSliderView(on: Boolean) = changeSettings { it.copy(sliderView = on) }

    fun setAutoPreamp(on: Boolean) {
        seal()
        // Starting from the automatic value avoids a sudden jump in volume.
        edit { p -> p.copy(preamp = Preamp(on, roundGain(now.analysis.preampDb))) }
    }

    fun setPreamp(db: Double) = edit("preamp") { p -> p.copy(preamp = Preamp(false, roundGain(db))) }

    // ---- presets ----

    private fun refreshPresets() {
        val all = builtin + own.sortedBy { it.name.lowercase() }.map { Preset(it, false) }
        mutable.update { it.copy(presets = all) }
    }

    fun loadPreset(name: String) {
        val p = now.presets.find { it.name == name }?.profile ?: return
        seal()
        mutable.update { it.copy(selected = null, preset = name) }
        val before = now.profile
        edit { it.copy(filters = p.filters, preamp = p.preamp, name = p.name) }
        if (before == now.profile) refresh() else toast("Loaded $name", canUndo = true)
    }

    /** True when the name can be used. */
    fun savePreset(name: String): Boolean {
        val clean = name.trim().take(80)
        if (clean.isEmpty()) return false
        if (builtin.any { it.name == clean }) {
            toast("A built-in preset already has this name", Tone.Warning)
            return false
        }
        val preset = now.profile.copy(name = clean, headphone = null, hearing = null)
        own = own.filter { it.name != clean } + preset
        mutable.update { it.copy(preset = clean, profile = it.profile.copy(name = clean)) }
        presetsChanged()
        toast("Saved $clean", Tone.Good)
        return true
    }

    fun renamePreset(old: String, name: String): Boolean {
        val clean = name.trim().take(80)
        if (clean.isEmpty()) return false
        if (clean != old && now.presets.any { it.name == clean }) {
            toast("Another preset already has this name", Tone.Warning)
            return false
        }
        own = own.map { if (it.name == old) it.copy(name = clean) else it }
        mutable.update { s ->
            s.copy(
                preset = if (s.preset == old) clean else s.preset,
                devices = s.devices.mapValues { (_, d) -> if (d.preset == old) d.copy(preset = clean) else d },
            )
        }
        presetsChanged()
        return true
    }

    fun deletePreset(name: String) {
        own = own.filter { it.name != name }
        mutable.update { it.copy(preset = it.preset.takeIf { p -> p != name }) }
        presetsChanged()
        toast("Deleted $name")
    }

    private fun presetsChanged() {
        refreshPresets()
        val list = own
        save("The presets could not be saved") { savePresets(list) }
        refresh()
    }

    fun importText(name: String, text: String) {
        val imported = try {
            Core.import(name, text)
        } catch (e: CoreError) {
            toast(e.message.orEmpty(), Tone.Warning)
            return
        }
        seal()
        mutable.update { it.copy(selected = null, preset = null) }
        edit { it.copy(filters = imported.profile.filters, preamp = imported.profile.preamp) }
        if (imported.skipped > 0) {
            toast("Imported $name. ${imported.skipped} lines are not supported and were left out", Tone.Warning, true)
        } else {
            toast("Imported $name", Tone.Good, true)
        }
    }

    /** Equalizer APO text that other equalizers can read. */
    fun exportText(): String = Core.export(now.profile)

    // ---- outputs ----

    private fun outputChanged() {
        val output = outputs.current()
        if (output == now.output) return
        loadOutput(output)
        if (now.settings.enabled) EqService.start(context)
        toast("Switched to ${output.name}")
    }

    private fun loadOutput(output: Output) {
        history.clear()
        cancelHearing()
        measured = null
        mutable.update { s ->
            // A new output starts flat; what another one needs may not suit it.
            val saved = s.devices[output.id]
            s.copy(
                output = output,
                profile = saved?.profile ?: Profile(),
                preset = saved?.preset,
                selected = null,
                comparing = false,
                hearing = HearingUi(),
            )
        }
        refresh()
        refreshBars()
    }

    fun forgetOutput(id: String) {
        if (id == now.output.id) return
        mutable.update { it.copy(devices = it.devices - id) }
        saveSoon()
    }

    // ---- headphones ----

    suspend fun searchHeadphones(query: String): HeadphoneHits =
        withContext(Dispatchers.Default) { Core.search(query, MAX_RESULTS) }

    fun pickHeadphone(index: Int) {
        val correction = try {
            Core.headphone(index)
        } catch (e: CoreError) {
            toast(e.message.orEmpty(), Tone.Danger)
            return
        }
        seal()
        edit { it.copy(headphone = correction) }
        toast("Corrected for ${correction.name}", Tone.Good, true)
    }

    fun setHeadphoneOn(on: Boolean) {
        seal()
        edit { it.copy(headphone = it.headphone?.copy(on = on)) }
    }

    fun setHeadphoneStrength(strength: Float) = edit("headphone") {
        it.copy(headphone = it.headphone?.copy(strength = roundStrength(strength)))
    }

    fun removeHeadphone() {
        seal()
        edit { it.copy(headphone = null) }
    }

    // ---- hearing test ----

    fun startHearing() {
        seal()
        answers = emptyList()
        setHearing { it.copy(stage = HearingStage.Testing, step = Core.hearing(answers), playing = false) }
        mutable.update { it.copy(comparing = false) }
        refresh()
        // Time for the effect to step aside before the first tone.
        playTone(900)
    }

    private fun playTone(wait: Long) {
        testJob?.cancel()
        testJob = scope.launch {
            delay(wait)
            val step = now.hearing.step?.takeIf { !it.done } ?: return@launch
            try {
                val length = player.play(step)
                setHearing { it.copy(playing = true) }
                delay(length)
                player.stop()
                setHearing { it.copy(playing = false) }
            } catch (e: RuntimeException) {
                if (e is kotlinx.coroutines.CancellationException) throw e
                Log.w(TAG, "tone", e)
                cancelHearing()
                toast("The tone could not be played", Tone.Danger)
            }
        }
    }

    /** Plays the current tone again. */
    fun repeatTone() = playTone(0)

    fun answerHearing(heard: Boolean) {
        if (now.hearing.stage != HearingStage.Testing) return
        testJob?.cancel()
        player.stop()
        answers = answers + heard
        val step = Core.hearing(answers)
        if (step.done) {
            measured = step.layer
            setHearing { HearingUi(HearingStage.Result, step, false, step.bars) }
            refresh()
        } else {
            setHearing { it.copy(step = step, playing = false) }
            playTone(450)
        }
    }

    /** Leaves the test or its result. */
    fun cancelHearing() {
        testJob?.cancel()
        player.stop()
        if (now.hearing.stage == HearingStage.Intro) return
        setHearing { HearingUi() }
        refresh()
        refreshBars()
    }

    fun applyHearing() {
        val layer = measured ?: return
        seal()
        edit { it.copy(hearing = layer.copy(strength = it.hearing?.strength ?: 1.0)) }
        cancelHearing()
        refreshBars()
        if (layer.left.isEmpty() && layer.right.isEmpty()) {
            toast("Both ears hear evenly, so nothing needs to change", Tone.Good)
        } else {
            toast("Hearing compensation is on", Tone.Good, true)
        }
    }

    fun setHearingOn(on: Boolean) {
        seal()
        edit { it.copy(hearing = it.hearing?.copy(on = on)) }
    }

    fun setHearingStrength(strength: Float) = edit("hearing") {
        it.copy(hearing = it.hearing?.copy(strength = roundStrength(strength)))
    }

    fun removeHearing() {
        seal()
        edit { it.copy(hearing = null) }
        refreshBars()
    }

    private fun setHearing(f: (HearingUi) -> HearingUi) = mutable.update { it.copy(hearing = f(it.hearing)) }

    private fun refreshBars() {
        if (now.hearing.stage != HearingStage.Intro) return
        val bars = now.profile.hearing?.let(Core::earBars).orEmpty()
        setHearing { it.copy(bars = bars) }
    }

    // ---- assistant ----

    private fun target(): AiTarget? {
        val s = now.settings
        if (s.ai == AiChoice.Rules) return null
        val a = s.aiSettings[s.ai.id] ?: AiSettings()
        return AiTarget(s.ai.id, a.url, a.model, keys.get(s.ai.id))
    }

    fun send(words: String) {
        val text = words.trim().take(2000)
        if (text.isEmpty() || askJob?.isActive == true) return
        val earlier = earlierChat()
        addChat(ChatRow(nextId(), true, text))
        val profile = now.profile
        val target = target()
        if (target == null) {
            answerWithRules(text, profile, null)
            return
        }
        val busy = ChatRow(nextId(), false, "Thinking", Tone.Busy, target.model)
        addChat(busy)
        val device = now.output.name
        val sendHearing = now.settings.sendHearing
        askJob = scope.launch {
            val result = runCatching {
                withContext(Dispatchers.IO) { Core.ask(target, text, device, sendHearing, earlier, profile) }
            }
            removeChat(busy.id)
            val answer = result.getOrElse { e ->
                if (e !is CoreError) throw e
                answerWithRules(text, profile, e.message)
                return@launch
            }
            propose(answer, target.model, profile.filters)
        }
    }

    fun stopAsking() {
        if (askJob?.isActive != true) return
        askJob?.cancel()
        mutable.update { s -> s.copy(chat = s.chat.filter { it.tone != Tone.Busy }) }
    }

    private fun earlierChat(): List<Pair<String, String>> {
        val out = mutableListOf<Pair<String, String>>()
        var question: String? = null
        for (row in now.chat) {
            if (row.fromUser) {
                question = row.text
            } else if (row.tone != Tone.Danger && question != null) {
                out += question to row.text
                question = null
            }
        }
        return out
    }

    private fun answerWithRules(words: String, profile: Profile, problem: String?) {
        if (problem != null) addChat(ChatRow(nextId(), false, problem, Tone.Danger))
        val answer = try {
            Core.suggest(words, profile)
        } catch (e: CoreError) {
            addChat(ChatRow(nextId(), false, e.message.orEmpty(), Tone.Danger, RULES))
            return
        }
        if (answer != null) {
            propose(answer, RULES, profile.filters)
        } else if (problem == null) {
            val hint = "I did not understand that. Try words like bass, vocals, harsh, muddy, warm or bright."
            addChat(ChatRow(nextId(), false, hint, origin = RULES))
        }
    }

    private fun propose(p: Proposal, origin: String, before: List<Filter>) {
        val text = (listOf(p.reason) + p.notes.map { "Note: $it" }).joinToString("\n")
        val state = if (p.lines.isEmpty()) ProposalState.None else ProposalState.Open
        addChat(ChatRow(nextId(), false, text, Tone.Neutral, origin, p.lines, state, before, p.after))
    }

    fun applyProposal(id: Long) {
        val row = now.chat.find { it.id == id } ?: return
        if (row.proposal == ProposalState.None || row.proposal == ProposalState.Applied) return
        if (row.before != now.profile.filters) {
            toast("Your bands changed since this suggestion. Ask again for a new one", Tone.Warning)
            return
        }
        seal()
        mutable.update { s ->
            s.copy(
                selected = null,
                chat = s.chat.map { if (it.id == id) it.copy(proposal = ProposalState.Applied) else it },
            )
        }
        edit { it.copy(filters = row.after) }
        toast("Applied", Tone.Good, true)
    }

    fun clearChat() {
        stopAsking()
        mutable.update { it.copy(chat = emptyList()) }
    }

    private fun nextId() = ++chatIds
    private fun addChat(row: ChatRow) = mutable.update { it.copy(chat = it.chat + row) }
    private fun removeChat(id: Long) = mutable.update { s -> s.copy(chat = s.chat.filter { it.id != id }) }

    // ---- settings ----

    private fun changeSettings(f: (Settings) -> Settings) {
        val next = f(now.settings)
        if (next == now.settings) return
        mutable.update { it.copy(settings = next) }
        saveSoon()
        refresh()
    }

    fun setTheme(t: ThemeChoice) = changeSettings { it.copy(theme = t) }
    fun setStartAtBoot(on: Boolean) = changeSettings { it.copy(startAtBoot = on) }
    fun setCoverage(c: Coverage) = changeSettings { it.copy(coverage = c) }
    fun setDetail(d: Detail) = changeSettings { it.copy(detail = d) }
    fun setWelcomed() = changeSettings { it.copy(welcomed = true) }

    fun setSendHearing(on: Boolean) {
        changeSettings { it.copy(sendHearing = on) }
        refreshSends()
    }

    fun setAi(choice: AiChoice) {
        modelsJob?.cancel()
        changeSettings { it.copy(ai = choice) }
        refreshAi()
    }

    fun setAiAddress(url: String) = changeAi { it.copy(url = url.trim()) }
    fun setAiModel(model: String) = changeAi { it.copy(model = model.trim()) }

    private fun changeAi(f: (AiSettings) -> AiSettings) = changeSettings { s ->
        s.copy(aiSettings = s.aiSettings + (s.ai.id to f(s.aiSettings[s.ai.id] ?: AiSettings())))
    }

    /** An empty key removes the stored one. */
    fun setAiKey(key: String) {
        val id = now.settings.ai.id
        if (key.isBlank()) {
            keys.clear(id)
            toast("API key removed")
        } else if (keys.set(id, key.trim())) {
            toast("API key saved in the Android Keystore", Tone.Good)
        } else {
            toast("This phone's key store refused the key", Tone.Danger)
        }
        refreshAi()
    }

    private fun refreshAi() {
        mutable.update { it.copy(ai = AiUi(hasKey = keys.has(it.settings.ai.id))) }
        refreshSends()
    }

    /** Updates the "What gets sent" text. */
    fun refreshSends() {
        val s = now
        val text = if (s.settings.ai == AiChoice.Rules) {
            "Nothing. The built-in rules run on this phone."
        } else {
            "Your message, your last few messages in this chat, and:\n" +
                Core.sends(s.profile, s.output.name, s.settings.sendHearing).trimEnd()
        }
        mutable.update { it.copy(ai = it.ai.copy(sends = text)) }
    }

    /** Checks the connection and lists what the provider offers. */
    fun loadModels() {
        val target = target() ?: return
        setAiStatus("Loading models", Tone.Busy)
        modelsJob?.cancel()
        modelsJob = scope.launch {
            val result = runCatching { withContext(Dispatchers.IO) { Core.models(target) } }
            val models = result.getOrElse { e ->
                if (e !is CoreError) throw e
                setAiStatus(e.message.orEmpty(), Tone.Danger)
                return@launch
            }
            if (models.isEmpty()) {
                setAiStatus("The provider lists no models. Type a model name instead.", Tone.Warning)
                return@launch
            }
            if (now.settings.aiSettings[target.provider]?.model.isNullOrEmpty()) setAiModel(models.first())
            mutable.update { it.copy(ai = it.ai.copy(models = models)) }
            setAiStatus("Connected. ${models.size} models available.", Tone.Good)
        }
    }

    private fun setAiStatus(text: String, tone: Tone) =
        mutable.update { it.copy(ai = it.ai.copy(status = text, tone = tone)) }

    fun diagnostics(): String {
        val s = now
        return buildString {
            appendLine("EQaura ${BuildConfig.VERSION_NAME} on Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})")
            appendLine("Phone: ${Build.MANUFACTURER} ${Build.MODEL}, ${Build.SUPPORTED_ABIS.joinToString()}")
            appendLine("Enabled: ${s.settings.enabled}, effect: ${s.effects}")
            appendLine("Coverage: ${s.settings.coverage}, detail: ${s.settings.detail}, rate: ${outputs.rate.roundToInt()} Hz")
            appendLine("Output: ${s.output.kind}")
            appendLine("Bands: ${s.profile.filters.size}")
            appendLine("Headphone layer: ${s.profile.headphone?.on}, hearing layer: ${s.profile.hearing?.on}")
            append("Assistant: ${s.settings.ai.id}")
        }
    }

    // ---- saving ----

    private fun saveSoon() {
        if (saveJob?.isActive == true) return
        saveJob = scope.launch {
            delay(400)
            saveNow()
        }
    }

    fun saveNow() {
        saveJob?.cancel()
        val s = now
        save("The settings could not be saved") {
            saveSettings(s.settings)
            saveDevices(s.devices)
        }
    }

    private fun save(problem: String, write: Store.() -> Unit) {
        scope.launch(disk) {
            try {
                store.write()
            } catch (e: IOException) {
                Log.e(TAG, problem, e)
                toast(problem, Tone.Danger)
            }
        }
    }

    fun toast(text: String, tone: Tone = Tone.Neutral, canUndo: Boolean = false) {
        toastFlow.tryEmit(Toast(text, tone, canUndo))
    }

    private companion object {
        const val RULES = "Built-in rules"

        /** Rows handed to the headphone list at once. Typing narrows it further. */
        const val MAX_RESULTS = 200
    }
}

/** Three significant digits: 1234 becomes 1230, 56.78 becomes 56.8. */
fun roundFreq(f: Double): Double {
    if (f <= 0 || !f.isFinite()) return 1000.0
    val scale = 10.0.pow(2 - floor(log10(f)))
    return Math.round(f * scale) / scale
}

fun roundGain(db: Double): Double = if (db.isFinite()) Math.round(db * 10) / 10.0 else 0.0

fun roundStrength(v: Float): Double = (v * 100).roundToInt() / 100.0

/** "63 Hz", "1.25 kHz" */
fun freqText(hz: Double): String =
    if (hz >= 1000) "${trimmed(hz / 1000, 2)} kHz" else "${trimmed(hz, if (hz < 100) 1 else 0)} Hz"

/** "+3.5 dB", with a true minus sign. */
fun gainText(db: Double): String {
    val v = trimmed(abs(db), 1)
    return if (v == "0") "0 dB" else "${if (db < 0) "−" else "+"}$v dB"
}

fun qText(q: Double): String = trimmed(q, 2)

private fun trimmed(v: Double, places: Int): String {
    val s = String.format(java.util.Locale.ROOT, "%.${places}f", v)
    return if ('.' in s) s.trimEnd('0').trimEnd('.') else s
}
