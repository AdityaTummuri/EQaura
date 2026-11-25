package io.github.adityatummuri.eqaura.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.Coverage
import io.github.adityatummuri.eqaura.EffectState
import io.github.adityatummuri.eqaura.OutputKind
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Tone

private const val NOTIFICATIONS = "android.permission.POST_NOTIFICATIONS"

private fun notificationsAllowed(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
        ContextCompat.checkSelfPermission(context, NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

private fun batteryUnrestricted(context: Context): Boolean =
    context.getSystemService(PowerManager::class.java)?.isIgnoringBatteryOptimizations(context.packageName) ?: false

private fun open(context: Context, model: AppModel, intent: Intent) {
    try {
        context.startActivity(intent)
    } catch (_: ActivityNotFoundException) {
        model.toast("This phone has no screen for that", Tone.Warning)
    }
}

private fun outputText(kind: OutputKind): String = when (kind) {
    OutputKind.Speaker -> "The phone's own speaker."
    OutputKind.Wired -> "Wired headphones or a line out."
    OutputKind.Usb -> "A USB audio device."
    OutputKind.Bluetooth -> "A Bluetooth device."
    OutputKind.Other -> "Another kind of output."
}

private class EffectInfo(val tone: Tone, val title: String, val detail: String)

private fun effectInfo(state: EffectState): EffectInfo = when (state) {
    EffectState.Off -> EffectInfo(Tone.Neutral, "The equalizer is off", "Turn it on to hear your changes.")
    EffectState.Active -> EffectInfo(Tone.Good, "The equalizer is working", "Everything this phone plays goes through it.")
    EffectState.Waiting -> EffectInfo(
        Tone.Warning,
        "Waiting for a player",
        "In Per app mode the equalizer attaches when a player starts. Play a song or a video. If nothing changes, try All apps.",
    )
    EffectState.Taken -> EffectInfo(
        Tone.Warning,
        "Another equalizer has control",
        "An equalizer from your phone's maker, Dolby or a music player holds the effect. " +
            "Turn it off in its own settings, or try the other coverage.",
    )
    EffectState.Failed -> EffectInfo(
        Tone.Danger,
        "This phone refused the effect",
        "The phone does not allow it, or its audio system stopped it. " +
            "Try the other coverage, then switch the equalizer off and on.",
    )
}

@Composable
internal fun SystemScreen(model: AppModel, onBack: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val enabled by model.observe { it.settings.enabled }
    val effects by model.observe { it.effects }
    val coverage by model.observe { it.settings.coverage }
    val output by model.observe { it.output }
    var notifications by remember { mutableStateOf(notificationsAllowed(context)) }
    var battery by remember { mutableStateOf(batteryUnrestricted(context)) }
    var asked by remember { mutableStateOf(false) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        notifications = notificationsAllowed(context)
        if (!granted) asked = true
    }
    LifecycleResumeEffect(Unit) {
        notifications = notificationsAllowed(context)
        battery = batteryUnrestricted(context)
        model.checkEffects()
        onPauseOrDispose { }
    }

    val state = if (enabled) effects else EffectState.Off
    val other = if (coverage == Coverage.AllApps) Coverage.PerApp else Coverage.AllApps

    SubScreen("System status", onBack, modifier) {
        Text(
            "Whether the equalizer is really working on this phone, and what can get in its way.",
            style = EqType.body,
            color = EqTheme.colors.text2,
        )
        EqCard(padding = 4.dp) {
            Column(Modifier.padding(horizontal = 12.dp)) {
                val info = effectInfo(state)
                val offState = state == EffectState.Off
                val trouble = state == EffectState.Waiting || state == EffectState.Taken || state == EffectState.Failed
                StatusRow(
                    tone = info.tone,
                    title = info.title,
                    detail = info.detail,
                    action = when {
                        offState -> "Turn on"
                        trouble -> "Try ${other.label}"
                        else -> null
                    },
                    onAction = {
                        if (offState) model.setEnabled(true) else model.setCoverage(other)
                    },
                )
                Divider()
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    StatusRow(
                        tone = if (notifications) Tone.Good else Tone.Warning,
                        title = if (notifications) "Notifications are on" else "Notifications are off",
                        detail = "While the equalizer is on, Android needs a small notification to keep it running. " +
                            "Without it Android may stop the equalizer.",
                        action = when {
                            notifications -> null
                            asked -> "Open notification settings"
                            else -> "Allow notifications"
                        },
                        onAction = {
                            if (asked) {
                                val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                                    .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
                                open(context, model, intent)
                            } else {
                                request.launch(NOTIFICATIONS)
                            }
                        },
                    )
                    Divider()
                }
                StatusRow(
                    tone = if (battery) Tone.Good else Tone.Warning,
                    title = if (battery) "Battery saving does not limit EQaura" else "Battery saving may stop the equalizer",
                    detail = "Some phones stop apps in the background to save power. If the sound goes back to normal after a while, " +
                        "open the battery settings and let EQaura run without limits.",
                    action = if (battery) null else "Open battery settings",
                    onAction = { open(context, model, Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) },
                )
                Divider()
                StatusRow(
                    tone = Tone.Neutral,
                    title = "Other equalizers and Dolby",
                    detail = "Dolby Atmos, Wavelet or an equalizer inside a music player can work against EQaura. " +
                        "If the sound seems wrong, turn them off.",
                )
                Divider()
                StatusRow(
                    tone = Tone.Neutral,
                    title = "Output: ${output.name}",
                    detail = outputText(output.kind) + " Each output keeps its own sound.",
                    action = "Check again",
                    onAction = { model.checkEffects() },
                )
            }
        }
        EqButton(
            "Share diagnostics",
            {
                val send = Intent(Intent.ACTION_SEND)
                    .setType("text/plain")
                    .putExtra(Intent.EXTRA_SUBJECT, "EQaura diagnostics")
                    .putExtra(Intent.EXTRA_TEXT, model.diagnostics())
                open(context, model, Intent.createChooser(send, "Share diagnostics"))
            },
            icon = R.drawable.ic_export,
        )
        Text(
            "Diagnostics are a few lines of text for a bug report. They contain no audio and no keys.",
            style = EqType.small,
            color = EqTheme.colors.text2,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}
