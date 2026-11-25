package io.github.adityatummuri.eqaura

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.Looper

enum class OutputKind { Speaker, Wired, Usb, Bluetooth, Other }

/** Where the sound goes. [id] stays the same for the same model. */
data class Output(val id: String, val name: String, val kind: OutputKind)

class Outputs(context: Context, private val changed: () -> Unit) {
    private val audio = context.getSystemService(AudioManager::class.java)

    private val callback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) = later()
        override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) = later()
    }
    private val handler = Handler(Looper.getMainLooper())
    private val notify = Runnable { changed() }

    init {
        audio.registerAudioDeviceCallback(callback, handler)
    }

    /** Routing follows a moment after a device appears. */
    private fun later() {
        handler.removeCallbacks(notify)
        changed()
        handler.postDelayed(notify, 700)
    }

    /** The rate effects run at on this phone. */
    val rate: Double
        get() = audio.getProperty(AudioManager.PROPERTY_OUTPUT_SAMPLE_RATE)?.toDoubleOrNull()
            ?.takeIf { it in 8_000.0..384_000.0 } ?: 48_000.0

    fun current(): Output {
        val all = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).filter { rank(it.type) < HIDDEN }
        val routed = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val music = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).build()
            audio.getAudioDevicesForAttributes(music).firstOrNull { rank(it.type) < HIDDEN }
        } else {
            null
        }
        return describe(routed ?: all.minByOrNull { rank(it.type) })
    }

    private fun describe(d: AudioDeviceInfo?): Output {
        val kind = when (d?.let { rank(it.type) }) {
            0 -> OutputKind.Bluetooth
            1 -> OutputKind.Usb
            2 -> OutputKind.Wired
            4 -> OutputKind.Speaker
            null -> OutputKind.Speaker
            else -> OutputKind.Other
        }
        // Built-in outputs report the phone's own model as their name.
        val model = d?.productName?.toString()?.trim().orEmpty()
        return when (kind) {
            OutputKind.Speaker -> Output("speaker", "Phone speaker", kind)
            OutputKind.Wired -> Output("wired", "Wired headphones", kind)
            OutputKind.Usb -> Output("usb:$model", model.ifEmpty { "USB audio" }, kind)
            OutputKind.Bluetooth -> Output("bt:$model", model.ifEmpty { "Bluetooth" }, kind)
            OutputKind.Other -> Output("other:${d?.type}", model.ifEmpty { "Other output" }, kind)
        }
    }

    private companion object {
        const val HIDDEN = 9

        /** The order in which Android prefers outputs for music. */
        fun rank(type: Int): Int = when (type) {
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
            AudioDeviceInfo.TYPE_BLE_HEADSET,
            AudioDeviceInfo.TYPE_BLE_SPEAKER,
            AudioDeviceInfo.TYPE_HEARING_AID,
            -> 0
            AudioDeviceInfo.TYPE_USB_HEADSET,
            AudioDeviceInfo.TYPE_USB_DEVICE,
            AudioDeviceInfo.TYPE_USB_ACCESSORY,
            -> 1
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
            AudioDeviceInfo.TYPE_WIRED_HEADSET,
            AudioDeviceInfo.TYPE_LINE_ANALOG,
            -> 2
            AudioDeviceInfo.TYPE_HDMI,
            AudioDeviceInfo.TYPE_DOCK,
            AudioDeviceInfo.TYPE_AUX_LINE,
            AudioDeviceInfo.TYPE_LINE_DIGITAL,
            -> 3
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> 4
            else -> HIDDEN
        }
    }
}
