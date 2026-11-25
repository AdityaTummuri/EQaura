package io.github.adityatummuri.eqaura.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.EarBar
import io.github.adityatummuri.eqaura.HearingStage
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.freqText
import io.github.adityatummuri.eqaura.gainText
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.roundToInt

@Composable
internal fun HearingScreen(model: AppModel, modifier: Modifier = Modifier) {
    val stage by model.observe { it.hearing.stage }
    BackHandler(enabled = stage != HearingStage.Intro) { model.cancelHearing() }
    when (stage) {
        HearingStage.Intro -> IntroPage(model, modifier)
        HearingStage.Testing -> TestingPage(model, modifier)
        HearingStage.Result -> ResultPage(model, modifier)
    }
}

@Composable
private fun IntroPage(model: AppModel, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    ScreenColumn(modifier) {
        PageHeader("Hearing test", subtitle = "A short listening check that tunes the sound to your ears.")
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                Text("Find out how you hear, one ear at a time", style = EqType.title, color = c.text)
                Text(
                    "EQaura plays short, quiet tones at a few pitches. You say whether you can hear each one. " +
                        "It then suggests a gentle correction that you can switch on or off at any time.",
                    style = EqType.body,
                    color = c.text2,
                )
                Check(R.drawable.ic_hearing, "Pick a quiet room", "Background noise makes quiet tones hard to hear and will skew the result.")
                Check(
                    R.drawable.ic_headphones,
                    "Wear the headphones you normally listen with",
                    "The result is for this pair. Connect them first, so the tones play through them.",
                )
                Check(
                    R.drawable.ic_volume,
                    "Turn the volume down low before you start",
                    "Tones begin very quietly and slowly get louder. Stop at once if anything is uncomfortable.",
                )
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Ico(R.drawable.ic_info, tint = c.text2, size = 20.dp, modifier = Modifier.padding(top = 1.dp))
                    Text(
                        "This is not a medical test. It cannot find or rule out hearing problems. " +
                            "If you notice hearing loss, ringing or pain, please see a doctor or an audiologist.",
                        style = EqType.small,
                        color = c.text2,
                        modifier = Modifier.weight(1f),
                    )
                }
                EqButton(
                    "Start the test",
                    { model.startHearing() },
                    Modifier.fillMaxWidth().testTag("hearing-start"),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_play,
                )
                Text("About 5 minutes", style = EqType.small, color = c.text3, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
            }
        }
        StoredResult(model)
    }
}

@Composable
private fun Check(icon: Int, title: String, body: String) {
    val c = EqTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(
            Modifier.size(36.dp).background(c.accentSoft, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Ico(icon, tint = c.accent, size = 20.dp)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = EqType.bodyStrong, color = c.text)
            Text(body, style = EqType.small, color = c.text2)
        }
    }
}

@Composable
private fun StoredResult(model: AppModel) {
    val c = EqTheme.colors
    val layer by model.observe { it.profile.hearing }
    val bars by model.observe { it.hearing.bars }
    val current = layer ?: return
    EqCard {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Your result", style = EqType.lead, color = c.text)
            Text("Taller bars mean that ear needed more volume at that pitch.", style = EqType.small, color = c.text2)
            if (bars.isNotEmpty()) {
                EarLegend()
                HearingBars(bars)
            }
            SwitchRow(
                title = "Hearing correction",
                checked = current.on,
                onChange = { model.setHearingOn(it) },
                detail = if (current.on) "Applied to everything you play" else "Off",
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Strength", style = EqType.small, color = c.text2, modifier = Modifier.weight(1f))
                Text("${(current.strength * 100).roundToInt()}%", style = EqType.number, color = c.text)
            }
            EqSlider(
                value = current.strength.toFloat(),
                onChange = { model.setHearingStrength(it) },
                onFinish = { model.seal() },
                range = 0f..1f,
                label = "Hearing correction strength",
                valueText = "${(current.strength * 100).roundToInt()} percent",
                enabled = current.on,
            )
            Text(
                "Start low. A half strength correction is gentler and often sounds more natural.",
                style = EqType.small,
                color = c.text2,
            )
            EqButton("Clear result", { model.removeHearing() }, kind = ButtonKind.Danger, icon = R.drawable.ic_trash)
        }
    }
}

@Composable
private fun KeepScreenOn() {
    val view = LocalView.current
    DisposableEffect(view) {
        view.keepScreenOn = true
        onDispose { view.keepScreenOn = false }
    }
}

@Composable
private fun TestingPage(model: AppModel, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val ui by model.observe { it.hearing }
    val step = ui.step
    KeepScreenOn()
    ScreenColumn(modifier) {
        PageHeader("Hearing test", subtitle = "Listen for a quiet tone, then answer.")
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Row(verticalAlignment = Alignment.Top) {
                    Column(Modifier.weight(1f)) {
                        Text("Now testing", style = EqType.small, color = c.text2)
                        Text(
                            if (step == null || step.left) "Left ear" else "Right ear",
                            style = EqType.title,
                            color = c.text,
                        )
                    }
                    Column(horizontalAlignment = Alignment.End) {
                        Text("Tone", style = EqType.small, color = c.text2)
                        Text(
                            step?.let { it.label.ifEmpty { freqText(it.freq) } }.orEmpty(),
                            style = EqType.title,
                            color = c.text,
                        )
                    }
                }
                val progress = (step?.progress ?: 0f).coerceIn(0f, 1f)
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Row {
                        Text("Progress", style = EqType.small, color = c.text2, modifier = Modifier.weight(1f))
                        Text("${(progress * 100).roundToInt()}%", style = EqType.number, color = c.text)
                    }
                    Box(
                        Modifier
                            .fillMaxWidth()
                            .height(8.dp)
                            .clip(CircleShape)
                            .background(c.surface3)
                            .semantics {
                                contentDescription = "Test progress"
                                progressBarRangeInfo = ProgressBarRangeInfo(progress, 0f..1f)
                            },
                    ) {
                        Box(Modifier.fillMaxWidth(progress).fillMaxHeight().background(c.accent, CircleShape))
                    }
                }
                Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally) {
                    PulseIndicator(ui.playing)
                    Text(
                        if (ui.playing) "Playing a tone" else "Waiting for your answer",
                        style = EqType.small,
                        color = c.text2,
                    )
                }
                Text("Did you hear the tone?", style = EqType.lead, color = c.text, modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Center)
                EqButton(
                    "I hear it",
                    { model.answerHearing(true) },
                    Modifier.fillMaxWidth().heightIn(min = 72.dp).testTag("hearing-yes"),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_check,
                )
                EqButton(
                    "I don't hear it",
                    { model.answerHearing(false) },
                    Modifier.fillMaxWidth().heightIn(min = 72.dp).testTag("hearing-no"),
                    icon = R.drawable.ic_close,
                )
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally)) {
                    EqButton("Play again", { model.repeatTone() }, kind = ButtonKind.Ghost, icon = R.drawable.ic_play)
                    EqButton("Stop", { model.cancelHearing() }, Modifier.testTag("hearing-stop"), kind = ButtonKind.Ghost, icon = R.drawable.ic_stop)
                }
            }
        }
    }
}

@Composable
private fun PulseIndicator(playing: Boolean) {
    val c = EqTheme.colors
    Box(Modifier.size(132.dp), contentAlignment = Alignment.Center) {
        if (playing) {
            Waves(Modifier.size(132.dp))
        } else {
            Box(Modifier.size(84.dp).background(c.surface3, CircleShape))
        }
        Box(
            Modifier.size(68.dp).background(if (playing) c.accent else c.surface2, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Ico(R.drawable.ic_volume, tint = if (playing) c.accentOn else c.text2, size = 30.dp)
        }
    }
}

@Composable
private fun Waves(modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val transition = rememberInfiniteTransition(label = "pulse")
    val wave by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1100, easing = FastOutSlowInEasing), RepeatMode.Restart),
        label = "wave",
    )
    Box(
        modifier.drawBehind {
            val base = 34.dp.toPx()
            drawCircle(c.accent.copy(alpha = 0.30f * (1f - wave)), base + 30.dp.toPx() * wave)
            drawCircle(c.accent.copy(alpha = 0.16f), base + 8.dp.toPx())
        },
    )
}

@Composable
private fun ResultPage(model: AppModel, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val bars by model.observe { it.hearing.bars }
    ScreenColumn(modifier) {
        PageHeader("Your result", subtitle = "Taller bars mean that ear needed more volume at that pitch.")
        EqCard {
            Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
                EarLegend()
                HearingBars(bars)
                Text(
                    "The correction starts at full strength. You can lower it afterwards, and a half strength correction often sounds more natural.",
                    style = EqType.small,
                    color = c.text2,
                )
                EqButton(
                    "Use this result",
                    { model.applyHearing() },
                    Modifier.fillMaxWidth().testTag("hearing-apply"),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_check,
                )
                EqButton("Discard", { model.cancelHearing() }, Modifier.fillMaxWidth().testTag("hearing-discard"), kind = ButtonKind.Ghost)
            }
        }
    }
}

@Composable
private fun EarLegend() {
    val c = EqTheme.colors
    Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        listOf("Left ear" to c.band(1), "Right ear" to c.band(3)).forEach { (label, color) ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.size(10.dp).background(color, RoundedCornerShape(2.dp)))
                Text(label, style = EqType.caption, color = c.text2)
            }
        }
    }
}

private fun shortLabel(label: String): String = label.replace(" kHz", "k").replace(" Hz", "")

@Composable
private fun HearingBars(bars: List<EarBar>, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val top = max(6f, bars.maxOfOrNull { max(abs(it.left), abs(it.right)) } ?: 0f)
    val description = bars.joinToString(". ") {
        if (it.tested) {
            "${it.label}: left ear ${gainText(it.left.toDouble())}, right ear ${gainText(it.right.toDouble())}"
        } else {
            "${it.label}: not tested"
        }
    }
    val label = EqType.caption.copy(fontSize = fixedSp(10f))
    val left = c.band(1)
    val right = c.band(3)
    Column(modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = "Hearing result. $description" }) {
        Row(Modifier.fillMaxWidth().height(150.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            bars.forEach { bar ->
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .drawBehind {
                            val w = size.width * 0.38f
                            val base = size.height
                            drawLine(c.gridStrong, Offset(0f, base), Offset(size.width, base), 1.dp.toPx())
                            if (bar.tested) {
                                val hl = (max(bar.left, 0f) / top).coerceIn(0f, 1f) * (size.height - 4.dp.toPx())
                                val hr = (max(bar.right, 0f) / top).coerceIn(0f, 1f) * (size.height - 4.dp.toPx())
                                val gap = size.width * 0.04f
                                val x0 = size.width / 2 - gap / 2 - w
                                val x1 = size.width / 2 + gap / 2
                                val radius = CornerRadius(3.dp.toPx())
                                drawRoundRect(left, Offset(x0, base - hl - 1.dp.toPx()), Size(w, hl + 1.dp.toPx()), radius)
                                drawRoundRect(right, Offset(x1, base - hr - 1.dp.toPx()), Size(w, hr + 1.dp.toPx()), radius)
                            }
                        },
                )
            }
        }
        Row(Modifier.fillMaxWidth().padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            bars.forEach { bar ->
                Text(
                    shortLabel(bar.label),
                    style = label,
                    color = if (bar.tested) c.text2 else c.text3,
                    textAlign = TextAlign.Center,
                    maxLines = 1,
                    softWrap = false,
                    modifier = Modifier.weight(1f),
                )
            }
        }
        if (bars.any { !it.tested }) {
            Text("Pitches that were not tested show no bar.", style = EqType.caption, color = c.text3, modifier = Modifier.padding(top = 8.dp))
        }
    }
}
