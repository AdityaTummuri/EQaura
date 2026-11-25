package io.github.adityatummuri.eqaura.ui

import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.R

private const val NotificationPermission = "android.permission.POST_NOTIFICATIONS"

@Composable
internal fun WelcomeScreen(model: AppModel, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val context = LocalContext.current

    fun finish() {
        model.setEnabled(true)
        model.setWelcomed()
    }

    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { _ -> finish() }
    val needsPermission = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        ContextCompat.checkSelfPermission(context, NotificationPermission) != PackageManager.PERMISSION_GRANTED

    Box(
        modifier
            .fillMaxSize()
            .background(c.bg)
            .windowInsetsPadding(WindowInsets.systemBars.union(WindowInsets.displayCutout)),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier
                .widthIn(max = 480.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            Box(Modifier.size(88.dp).background(c.accentSoft, CircleShape), contentAlignment = Alignment.Center) {
                Ico(R.drawable.ic_notification, tint = c.accent, size = 48.dp)
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("EQaura", style = EqType.display, color = c.text, modifier = Modifier.semantics { heading() })
                Text(
                    "Shape the sound of everything you play.",
                    style = EqType.lead,
                    color = c.text2,
                    textAlign = TextAlign.Center,
                )
            }
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Point(R.drawable.ic_eq, "One equalizer for music, video, games and calls.")
                Point(R.drawable.ic_headphones, "Pick your headphones, or say what you want to hear.")
                Point(R.drawable.ic_lock, "Nothing leaves this phone unless you set up an assistant.")
            }
            if (needsPermission) {
                Text(
                    "Android will ask to show a notification. It needs one to keep the equalizer running.",
                    style = EqType.small,
                    color = c.text3,
                    textAlign = TextAlign.Center,
                )
            }
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                EqButton(
                    "Turn on",
                    { if (needsPermission) request.launch(NotificationPermission) else finish() },
                    Modifier.fillMaxWidth().testTag("welcome-start"),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_power,
                )
                EqButton(
                    "Not now",
                    { model.setWelcomed() },
                    Modifier.fillMaxWidth().testTag("welcome-skip"),
                    kind = ButtonKind.Ghost,
                )
            }
        }
    }
}

@Composable
private fun Point(icon: Int, text: String) {
    val c = EqTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
        Ico(icon, tint = c.accent, size = 24.dp)
        Text(text, style = EqType.body, color = c.text, modifier = Modifier.weight(1f))
    }
}
