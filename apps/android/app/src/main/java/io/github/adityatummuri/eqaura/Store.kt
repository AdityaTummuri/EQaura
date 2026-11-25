package io.github.adityatummuri.eqaura

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import android.util.Base64
import androidx.core.content.edit
import java.io.File
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable

@Serializable
enum class ThemeChoice(val label: String) { System("System"), Light("Light"), Dark("Dark") }

@Serializable
enum class Coverage(val label: String, val detail: String) {
    AllApps("All apps", "One equalizer for the whole phone. Works with most phones."),
    PerApp("Per app", "Attaches to each player that announces itself. Use it if All apps does nothing."),
}

/** How finely the curve is followed, against the delay it adds. */
@Serializable
enum class Detail(val label: String, val detail: String, val block: Int, val frameMs: Float) {
    LowDelay("Low delay", "About 20 ms. Best for games. Bass is less precise.", 1024, 20f),
    Balanced("Balanced", "About 40 ms. Right for most listening.", 2048, 40f),
    FineBass("Fine bass", "About 85 ms. Follows narrow bass bands best.", 4096, 80f),
}

@Serializable
enum class AiChoice(val id: String, val label: String, val needsKey: Boolean, val defaultUrl: String) {
    Rules("rules", "Built-in rules", false, ""),
    Anthropic("anthropic", "Anthropic", true, "https://api.anthropic.com"),
    OpenAi("openai", "OpenAI-compatible", true, "https://api.openai.com/v1"),
    Ollama("ollama", "Ollama", false, "http://localhost:11434/v1"),
}

@Serializable
data class AiSettings(val url: String = "", val model: String = "")

@Serializable
data class Settings(
    val theme: ThemeChoice = ThemeChoice.System,
    val enabled: Boolean = false,
    val startAtBoot: Boolean = true,
    val coverage: Coverage = Coverage.AllApps,
    val detail: Detail = Detail.Balanced,
    val sliderView: Boolean = false,
    val ai: AiChoice = AiChoice.Rules,
    /** Address and model per provider, so switching back keeps them. */
    val aiSettings: Map<String, AiSettings> = emptyMap(),
    val sendHearing: Boolean = false,
    val welcomed: Boolean = false,
)

@Serializable
data class DeviceState(val name: String, val profile: Profile, val preset: String? = null)

@Serializable
private data class Devices(val devices: Map<String, DeviceState> = emptyMap())

@Serializable
private data class Presets(val presets: List<Profile> = emptyList())

/** Files EQaura keeps on this phone. A half-written file is never left behind. */
class Store(private val dir: File) {
    private inline fun <reified T> read(name: String): T? {
        val file = File(dir, name)
        return try {
            Core.json.decodeFromString<T>(AtomicFile(file).readFully().decodeToString())
        } catch (_: IOException) {
            null
        } catch (_: IllegalArgumentException) {
            // Kept for a rescue instead of being overwritten.
            file.renameTo(File(dir, "$name.broken"))
            null
        }
    }

    private inline fun <reified T> write(name: String, value: T) {
        dir.mkdirs()
        val file = AtomicFile(File(dir, name))
        val out = file.startWrite()
        try {
            out.write(Core.json.encodeToString(value).encodeToByteArray())
            file.finishWrite(out)
        } catch (e: IOException) {
            file.failWrite(out)
            throw e
        } catch (e: SerializationException) {
            file.failWrite(out)
            throw IOException(e)
        }
    }

    fun settings(): Settings = read("settings.json") ?: Settings()
    fun saveSettings(s: Settings) = write("settings.json", s)

    fun devices(): Map<String, DeviceState> = read<Devices>("devices.json")?.devices.orEmpty()
    fun saveDevices(d: Map<String, DeviceState>) = write("devices.json", Devices(d))

    fun presets(): List<Profile> = read<Presets>("presets.json")?.presets.orEmpty()
    fun savePresets(p: List<Profile>) = write("presets.json", Presets(p))
}

/** API keys, encrypted with a key that never leaves the Android Keystore. */
class Keys(context: Context) {
    private val prefs = context.getSharedPreferences("keys", Context.MODE_PRIVATE)

    private fun secret(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }

    /** False when this phone's key store refused. */
    fun set(provider: String, key: String): Boolean = try {
        val cipher = Cipher.getInstance(CIPHER).apply { init(Cipher.ENCRYPT_MODE, secret()) }
        val data = cipher.iv + cipher.doFinal(key.encodeToByteArray())
        prefs.edit { putString(provider, Base64.encodeToString(data, Base64.NO_WRAP)) }
        true
    } catch (_: GeneralSecurityException) {
        false
    }

    fun get(provider: String): String? = try {
        prefs.getString(provider, null)?.let { text ->
            val data = Base64.decode(text, Base64.NO_WRAP)
            val cipher = Cipher.getInstance(CIPHER)
            cipher.init(Cipher.DECRYPT_MODE, secret(), GCMParameterSpec(128, data, 0, IV))
            cipher.doFinal(data, IV, data.size - IV).decodeToString()
        }
    } catch (_: GeneralSecurityException) {
        null
    } catch (_: IllegalArgumentException) {
        null
    }

    fun has(provider: String) = prefs.contains(provider)

    fun clear(provider: String) = prefs.edit { remove(provider) }

    private companion object {
        const val ALIAS = "eqaura"
        const val CIPHER = "AES/GCM/NoPadding"
        const val IV = 12
    }
}
