package io.github.adityatummuri.eqaura.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.displayCutout
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.BuildConfig
import io.github.adityatummuri.eqaura.HearingStage
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Toast
import kotlinx.coroutines.delay

private const val Equalizer = 0
private const val Headphones = 1
private const val Hearing = 2
private const val Assistant = 3
private const val More = 4

private class Dest(val label: String, val icon: Int, val tag: String)

private val Dests = listOf(
    Dest("Equalizer", R.drawable.ic_eq, "nav-equalizer"),
    Dest("Headphones", R.drawable.ic_headphones, "nav-headphones"),
    Dest("Hearing", R.drawable.ic_hearing, "nav-hearing"),
    Dest("Assistant", R.drawable.ic_assistant, "nav-assistant"),
    Dest("More", R.drawable.ic_more, "nav-more"),
)

@Composable
internal fun AppRoot(model: AppModel) {
    val welcomed by model.observe { it.settings.welcomed }
    Box(Modifier.fillMaxSize().background(EqTheme.colors.bg)) {
        if (welcomed) {
            MainShell(model)
        } else {
            WelcomeScreen(model)
            ToastHost(model, Modifier.align(Alignment.BottomCenter).navigationBarsPadding())
        }
    }
}

@Composable
private fun MainShell(model: AppModel) {
    var dest by rememberSaveable { mutableIntStateOf(Equalizer) }
    var sub by rememberSaveable { mutableIntStateOf(0) }
    val imeOpen = WindowInsets.ime.getBottom(LocalDensity.current) > 0
    val bars = WindowInsets.systemBars.union(WindowInsets.displayCutout).asPaddingValues()
    val dir = LocalLayoutDirection.current

    BackHandler(enabled = dest != Equalizer) {
        if (dest == More && sub != 0) sub = 0 else dest = Equalizer
    }
    // Leaving the hearing page ends a test that is still running.
    LaunchedEffect(dest) {
        if (dest != Hearing && model.state.value.hearing.stage != HearingStage.Intro) model.cancelHearing()
    }

    val go: (Int, Int) -> Unit = { d, s ->
        dest = d
        sub = s
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        if (maxWidth >= 600.dp) {
            Row(Modifier.fillMaxSize()) {
                SideRail(dest, maxHeight < 480.dp, bars, dir) { go(it, 0) }
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .padding(
                            top = bars.calculateTopPadding(),
                            end = bars.calculateEndPadding(dir),
                            bottom = if (imeOpen) 0.dp else bars.calculateBottomPadding(),
                        )
                        .imePadding(),
                ) {
                    Screens(model, dest, sub, go)
                    ToastHost(model, Modifier.align(Alignment.BottomCenter))
                }
            }
        } else {
            Column(Modifier.fillMaxSize()) {
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .padding(
                            start = bars.calculateStartPadding(dir),
                            top = bars.calculateTopPadding(),
                            end = bars.calculateEndPadding(dir),
                        )
                        .imePadding(),
                ) {
                    Screens(model, dest, sub, go)
                    ToastHost(model, Modifier.align(Alignment.BottomCenter))
                }
                if (!imeOpen) BottomBar(dest) { go(it, 0) }
            }
        }
    }
}

@Composable
private fun Screens(model: AppModel, dest: Int, sub: Int, go: (Int, Int) -> Unit) {
    val holder = rememberSaveableStateHolder()
    Crossfade(targetState = dest, animationSpec = tween(120), label = "page") { d ->
        holder.SaveableStateProvider(d) {
            when (d) {
                Equalizer -> EqualizerScreen(model, onOpenSystem = { go(More, 2) })
                Headphones -> HeadphonesScreen(model)
                Hearing -> HearingScreen(model)
                Assistant -> AssistantScreen(model, onOpenSettings = { go(More, 1) })
                else -> when (sub) {
                    1 -> SettingsScreen(model, onBack = { go(More, 0) })
                    2 -> SystemScreen(model, onBack = { go(More, 0) })
                    3 -> AboutScreen(onBack = { go(More, 0) })
                    else -> MoreScreen { go(More, it) }
                }
            }
        }
    }
}

@Composable
private fun BottomBar(selected: Int, onSelect: (Int) -> Unit) {
    val c = EqTheme.colors
    Column {
        Divider()
        ClampFont(1.25f) {
            NavigationBar(containerColor = c.surface, tonalElevation = 0.dp) {
                Dests.forEachIndexed { i, d ->
                    NavigationBarItem(
                        selected = i == selected,
                        onClick = { onSelect(i) },
                        icon = { Icon(painterResource(d.icon), contentDescription = null) },
                        label = { Text(d.label, style = EqType.caption, maxLines = 1) },
                        alwaysShowLabel = true,
                        modifier = Modifier.testTag(d.tag),
                    )
                }
            }
        }
    }
}

@Composable
private fun SideRail(
    selected: Int,
    compact: Boolean,
    bars: PaddingValues,
    dir: LayoutDirection,
    onSelect: (Int) -> Unit,
) {
    val c = EqTheme.colors
    Column(
        Modifier
            .fillMaxHeight()
            .background(c.rail)
            .padding(
                start = bars.calculateStartPadding(dir),
                top = bars.calculateTopPadding(),
                bottom = bars.calculateBottomPadding(),
            )
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 8.dp, vertical = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        ClampFont(1.25f) {
            Dests.forEachIndexed { i, d ->
                NavigationRailItem(
                    selected = i == selected,
                    onClick = { onSelect(i) },
                    icon = { Icon(painterResource(d.icon), contentDescription = null) },
                    label = { Text(d.label, style = EqType.caption, maxLines = 1) },
                    alwaysShowLabel = !compact,
                    modifier = Modifier.testTag(d.tag),
                )
            }
        }
    }
}

@Composable
private fun MoreScreen(onOpen: (Int) -> Unit) {
    val c = EqTheme.colors
    ScreenColumn {
        PageHeader("More")
        EqCard(padding = 4.dp) {
            MoreRow(R.drawable.ic_settings, "Settings", "Look, coverage, assistant and remembered outputs", Modifier.testTag("more-settings")) {
                onOpen(1)
            }
            Divider()
            MoreRow(R.drawable.ic_system, "System status", "Is the equalizer working, and what can get in its way", Modifier.testTag("more-system")) {
                onOpen(2)
            }
            Divider()
            MoreRow(R.drawable.ic_info, "About", "Version, credits and licenses", Modifier.testTag("more-about")) {
                onOpen(3)
            }
        }
        Text("EQaura ${BuildConfig.VERSION_NAME}", style = EqType.caption, color = c.text3, modifier = Modifier.padding(horizontal = 4.dp))
    }
}

@Composable
private fun MoreRow(icon: Int, title: String, detail: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val c = EqTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clip(RoundedCornerShape(14.dp))
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Ico(icon, tint = c.text2)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = EqType.bodyStrong, color = c.text)
            Text(detail, style = EqType.small, color = c.text2)
        }
        Ico(R.drawable.ic_chevron_right, tint = c.text3, size = 20.dp)
    }
}

private class Shown(val id: Long, val toast: Toast)

@Composable
private fun ToastHost(model: AppModel, modifier: Modifier = Modifier) {
    var shown by remember { mutableStateOf<Shown?>(null) }
    var visible by remember { mutableStateOf(false) }
    LaunchedEffect(model) {
        var n = 0L
        model.toasts.collect { shown = Shown(++n, it) }
    }
    LaunchedEffect(shown?.id) {
        val current = shown ?: return@LaunchedEffect
        visible = true
        delay(if (current.toast.canUndo) 6000L else 3500L)
        visible = false
    }
    AnimatedVisibility(
        visible = visible,
        modifier = modifier,
        enter = fadeIn() + slideInVertically { it / 2 },
        exit = fadeOut() + slideOutVertically { it / 2 },
    ) {
        shown?.let { s ->
            ToastBar(
                toast = s.toast,
                onUndo = {
                    model.undo()
                    visible = false
                },
                onDismiss = { visible = false },
            )
        }
    }
}

@Composable
private fun ToastBar(toast: Toast, onUndo: () -> Unit, onDismiss: () -> Unit) {
    val c = EqTheme.colors
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier = Modifier
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .widthIn(max = 560.dp)
            .fillMaxWidth()
            .shadow(6.dp, shape)
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.lineStrong, shape)
            .semantics { liveRegion = LiveRegionMode.Polite }
            .clickable(onClickLabel = "Dismiss", onClick = onDismiss)
            .heightIn(min = 52.dp)
            .padding(start = 14.dp, end = if (toast.canUndo) 4.dp else 14.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Ico(toneIcon(toast.tone), tint = c.tone(toast.tone), size = 20.dp)
        Text(toast.text, style = EqType.small, color = c.text, modifier = Modifier.weight(1f))
        if (toast.canUndo) EqButton("Undo", onUndo, kind = ButtonKind.Ghost, icon = R.drawable.ic_undo)
    }
}
