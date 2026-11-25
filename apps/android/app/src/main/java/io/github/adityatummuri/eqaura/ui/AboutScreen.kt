package io.github.adityatummuri.eqaura.ui

import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.BuildConfig
import io.github.adityatummuri.eqaura.R
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private const val Repo = "https://github.com/AdityaTummuri/EQaura"

private class Credit(val name: String, val license: String, val detail: String)

private val Credits = listOf(
    Credit("EQaura", "MIT", "Copyright (c) 2026 Aditya. Source code on GitHub."),
    Credit(
        "AutoEq",
        "MIT",
        "Headphone corrections. Copyright (c) 2018-2022 Jaakko Pasanen. Measurements by oratory1990, crinacle, " +
            "Rtings, Innerfidelity, Headphone.com, HypetheSonics, Super Review and others.",
    ),
    Credit("Audio EQ Cookbook", "Public formulas", "Filter formulas by Robert Bristow-Johnson."),
    Credit("Jetpack Compose and Kotlin", "Apache-2.0", "The toolkit and the language this interface is made with, by Google and JetBrains."),
    Credit("Rust libraries", "MIT, Apache-2.0 and similar", "The core of the equalizer. Choose Licenses for the full list."),
)

private val MissingLicenses = listOf(
    "The full list of licenses is not part of this build.",
    "",
    "EQaura is MIT licensed. The libraries it uses are listed in the project on GitHub:",
    Repo,
)

private fun loadLicenses(context: Context): List<String> = try {
    context.assets.open("THIRD-PARTY-LICENSES.txt").use { it.readBytes().decodeToString().lines() }
} catch (_: IOException) {
    MissingLicenses
}

@Composable
internal fun AboutScreen(onBack: () -> Unit, modifier: Modifier = Modifier) {
    var licenses by rememberSaveable { mutableStateOf(false) }
    BackHandler(enabled = licenses) { licenses = false }
    if (licenses) {
        LicensesPage({ licenses = false }, modifier)
    } else {
        AboutPage(onBack, { licenses = true }, modifier)
    }
}

@Composable
private fun AboutPage(onBack: () -> Unit, onLicenses: () -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val links = LocalUriHandler.current
    fun openLink(url: String) {
        try {
            links.openUri(url)
        } catch (_: IllegalArgumentException) {
            // Nothing on this phone can open a link.
        }
    }
    SubScreen("About", onBack, modifier) {
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Ico(R.drawable.ic_notification, tint = c.accent, size = 44.dp)
                    Column {
                        Text("EQaura", style = EqType.display, color = c.text)
                        Text("Version ${BuildConfig.VERSION_NAME}  ·  MIT license", style = EqType.small, color = c.text2)
                    }
                }
                Text(
                    "A system-wide equalizer for Android. Free and open source, with no account and no telemetry.",
                    style = EqType.body,
                    color = c.text2,
                )
                EqButton(
                    "View on GitHub",
                    { openLink(Repo) },
                    Modifier.fillMaxWidth(),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_external,
                )
                EqButton("Report a problem", { openLink("$Repo/issues") }, Modifier.fillMaxWidth(), icon = R.drawable.ic_external)
            }
        }
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Credits", style = EqType.lead, color = c.text, modifier = Modifier.semantics { heading() })
                Text("EQaura stands on the work of these projects and people.", style = EqType.small, color = c.text2)
                Credits.forEach { credit ->
                    Divider()
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(credit.name, style = EqType.bodyStrong, color = c.text)
                        Text(credit.license, style = EqType.caption, color = c.text2)
                        Text(credit.detail, style = EqType.small, color = c.text2)
                    }
                }
                EqButton("Licenses", onLicenses, Modifier.fillMaxWidth(), icon = R.drawable.ic_chevron_right)
            }
        }
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Privacy", style = EqType.lead, color = c.text, modifier = Modifier.semantics { heading() })
                Text(
                    "No account, no telemetry, no server. Settings are plain files on this phone.",
                    style = EqType.small,
                    color = c.text2,
                )
                Text(
                    "The assistant's built-in rules run offline. A request to an AI provider goes out only when you press send, " +
                        "straight from this phone to the provider you chose. What gets sent shows the exact text.",
                    style = EqType.small,
                    color = c.text2,
                )
                Text(
                    "API keys are encrypted with the Android Keystore, travel only to the address you set, and are never " +
                        "written to logs or settings.",
                    style = EqType.small,
                    color = c.text2,
                )
            }
        }
        Text(
            "The hearing test is not a medical test. Start at a low volume. Loud sound can damage your hearing.",
            style = EqType.small,
            color = c.text2,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}

@Composable
private fun LicensesPage(onBack: () -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val context = LocalContext.current
    val lines by produceState<List<String>?>(null) {
        value = withContext(Dispatchers.IO) { loadLicenses(context) }
    }
    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(Modifier.widthIn(max = MaxContentWidth).fillMaxSize()) {
            Row(Modifier.padding(start = 16.dp, top = 16.dp, end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                EqIconButton(R.drawable.ic_chevron_left, "Back to About", onBack)
                Text("Licenses", style = EqType.title, color = c.text, modifier = Modifier.semantics { heading() })
            }
            LazyColumn(
                modifier = Modifier.fillMaxWidth().weight(1f),
                contentPadding = PaddingValues(16.dp),
            ) {
                val shown = lines
                if (shown == null) {
                    item(key = "loading") { Text("Loading", style = EqType.small, color = c.text2) }
                } else {
                    items(shown) { line ->
                        Text(line.ifEmpty { " " }, style = EqType.mono, color = c.text)
                    }
                }
            }
        }
    }
}
