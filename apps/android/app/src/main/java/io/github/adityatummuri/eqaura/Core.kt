package io.github.adityatummuri.eqaura

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNamingStrategy
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

// The same shapes as in the Rust core.

@Serializable
enum class Kind(val label: String, val hasGain: Boolean) {
    @SerialName("peaking") Peak("Peak", true),
    @SerialName("low_shelf") LowShelf("Low shelf", true),
    @SerialName("high_shelf") HighShelf("High shelf", true),
    @SerialName("low_pass") LowPass("Low pass", false),
    @SerialName("high_pass") HighPass("High pass", false),
    @SerialName("notch") Notch("Notch", false),
}

@Serializable
enum class Ear(val label: String) {
    @SerialName("both") Both("Both"),
    @SerialName("left") Left("Left"),
    @SerialName("right") Right("Right"),
}

@Serializable
data class Filter(
    @SerialName("type") val kind: Kind = Kind.Peak,
    val freqHz: Double,
    val gainDb: Double = 0.0,
    val q: Double = 1.0,
    val on: Boolean = true,
    val channel: Ear = Ear.Both,
)

@Serializable
data class Preamp(val auto: Boolean = true, val db: Double = 0.0)

@Serializable
data class Correction(
    val name: String,
    val source: String = "",
    val on: Boolean = true,
    val strength: Double = 1.0,
    val filters: List<Filter>,
)

@Serializable
data class HearingLayer(
    val on: Boolean = true,
    val strength: Double = 1.0,
    val left: List<Filter>,
    val right: List<Filter>,
)

@Serializable
data class Profile(
    val schema: Int = 1,
    val name: String = "Untitled",
    val preamp: Preamp = Preamp(),
    val filters: List<Filter> = emptyList(),
    val headphone: Correction? = null,
    val hearing: HearingLayer? = null,
)

@Serializable
enum class Severity {
    @SerialName("info") Info,
    @SerialName("caution") Caution,
    @SerialName("danger") Danger,
}

@Serializable
data class Warning(val severity: Severity, val title: String, val detail: String, val band: Int? = null)

/** Curves hold [Core.POINTS] values in dB, spread evenly on a log scale from 20 Hz to 20 kHz. */
@Serializable
data class Analysis(
    val preampDb: Double = 0.0,
    val netPeakDb: Double = 0.0,
    val left: List<Float> = emptyList(),
    /** Only present when the right ear differs. */
    val right: List<Float>? = null,
    /** Headphone and hearing layers alone. */
    val correction: List<Float>? = null,
    /** The selected band alone. */
    val selected: List<Float>? = null,
    val sliders: List<Float> = List(10) { 0f },
    val warnings: List<Warning> = emptyList(),
)

@Serializable
data class EngineBands(
    val preampDb: Float,
    val cutoffs: List<Float>,
    val left: List<Float>,
    val right: List<Float>,
)

@Serializable
data class Proposal(
    val reason: String,
    val lines: List<String>,
    val notes: List<String>,
    val after: List<Filter>,
)

@Serializable
data class HeadphoneRow(val index: Int, val name: String, val source: String, val form: String)

@Serializable
data class HeadphoneHits(val total: Int = 0, val rows: List<HeadphoneRow> = emptyList())

@Serializable
data class EarBar(val label: String, val left: Float, val right: Float, val tested: Boolean)

@Serializable
data class HearingStep(
    val done: Boolean,
    val progress: Float,
    val left: Boolean = true,
    val freq: Double = 0.0,
    val db: Double = 0.0,
    val label: String = "",
    val bars: List<EarBar> = emptyList(),
    val layer: HearingLayer? = null,
)

@Serializable
data class Imported(val profile: Profile, val skipped: Int)

@Serializable
data class AiTarget(val provider: String, val url: String, val model: String, val key: String?)

/** A message from the core that is fit to show. */
class CoreError(message: String) : Exception(message)

/** The Rust core. Calls are quick except [ask] and [models], which use the network. */
object Core {
    const val POINTS = 160

    @OptIn(ExperimentalSerializationApi::class)
    val json = Json {
        namingStrategy = JsonNamingStrategy.SnakeCase
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
    }

    init {
        System.loadLibrary("eqaura")
    }

    @JvmStatic
    private external fun call(method: String, args: String): String

    @JvmStatic
    external fun tone(left: Boolean, freq: Double, db: Double, rate: Double): FloatArray

    private fun raw(method: String, args: String): JsonElement {
        val reply = json.parseToJsonElement(call(method, args)).jsonObject
        reply["error"]?.let { throw CoreError(it.jsonPrimitive.content) }
        return reply.getValue("ok")
    }

    private inline fun <reified A, reified R> run(method: String, args: A): R =
        json.decodeFromJsonElement(raw(method, json.encodeToString(args)))

    @Serializable
    private class AnalyzeArgs(val profile: Profile, val points: Int, val selected: Int?)

    fun analyze(profile: Profile, selected: Int?): Analysis =
        run("analyze", AnalyzeArgs(profile, POINTS, selected))

    @Serializable
    private class EngineArgs(val profile: Profile, val block: Int, val rate: Double, val flat: Boolean)

    fun engine(profile: Profile, block: Int, rate: Double, flat: Boolean): EngineBands =
        run("engine", EngineArgs(profile, block, rate, flat))

    @Serializable
    private class SuggestArgs(val words: String, val profile: Profile)

    /** Built-in rules. `null` when the words were not understood. */
    fun suggest(words: String, profile: Profile): Proposal? = run("suggest", SuggestArgs(words, profile))

    @Serializable
    private class AskArgs(
        val ai: AiTarget,
        val words: String,
        val device: String,
        val sendHearing: Boolean,
        val earlier: List<List<String>>,
        val profile: Profile,
    )

    fun ask(
        ai: AiTarget,
        words: String,
        device: String,
        sendHearing: Boolean,
        earlier: List<Pair<String, String>>,
        profile: Profile,
    ): Proposal =
        run("ask", AskArgs(ai, words, device, sendHearing, earlier.map { listOf(it.first, it.second) }, profile))

    fun models(ai: AiTarget): List<String> = run("models", ai)

    @Serializable
    private class SendsArgs(val profile: Profile, val device: String, val sendHearing: Boolean)

    fun sends(profile: Profile, device: String, sendHearing: Boolean): String =
        run("sends", SendsArgs(profile, device, sendHearing))

    @Serializable
    private class SearchArgs(val query: String, val limit: Int)

    fun search(query: String, limit: Int): HeadphoneHits = run("search", SearchArgs(query, limit))

    @Serializable
    private class IndexArgs(val index: Int)

    fun headphone(index: Int): Correction = run("headphone", IndexArgs(index))

    @Serializable
    private class HearingArgs(val answers: List<Boolean>)

    fun hearing(answers: List<Boolean>): HearingStep = run("hearing", HearingArgs(answers))

    @Serializable
    private class LayerArgs(val layer: HearingLayer)

    fun earBars(layer: HearingLayer): List<EarBar> = run("ear_bars", LayerArgs(layer))

    @Serializable
    private class ImportArgs(val name: String, val text: String)

    fun import(name: String, text: String): Imported = run("import", ImportArgs(name, text))

    @Serializable
    private class ProfileArgs(val profile: Profile)

    fun export(profile: Profile): String = run("export", ProfileArgs(profile))

    @Serializable
    private class SliderArgs(val filters: List<Filter>, val index: Int, val gainDb: Double)

    fun slider(filters: List<Filter>, index: Int, gainDb: Double): List<Filter> =
        run("slider", SliderArgs(filters, index, gainDb))

    @Serializable
    private class TextArgs(val text: String)

    /** Understands "1.2k", "1,2 kHz" and "-3 dB". */
    fun number(text: String): Double? = run("number", TextArgs(text))

    fun builtin(): List<Profile> = json.decodeFromJsonElement(raw("builtin", "{}"))
}
