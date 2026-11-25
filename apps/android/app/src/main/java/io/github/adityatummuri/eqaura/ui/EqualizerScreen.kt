package io.github.adityatummuri.eqaura.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectVerticalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.Ear
import io.github.adityatummuri.eqaura.EffectState
import io.github.adityatummuri.eqaura.Filter
import io.github.adityatummuri.eqaura.Kind
import io.github.adityatummuri.eqaura.OutputKind
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Severity
import io.github.adityatummuri.eqaura.Tone
import io.github.adityatummuri.eqaura.freqText
import io.github.adityatummuri.eqaura.gainText
import io.github.adityatummuri.eqaura.qText
import io.github.adityatummuri.eqaura.roundFreq
import io.github.adityatummuri.eqaura.roundGain
import java.util.Locale
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.pow
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

private val SliderLabels = listOf("31", "62", "125", "250", "500", "1k", "2k", "4k", "8k", "16k")
private val LnQ = ln(200.0)

@Composable
internal fun EqualizerScreen(model: AppModel, onOpenSystem: () -> Unit, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier.fillMaxHeight().fillMaxWidth()) {
        if (maxWidth >= 720.dp && maxWidth > maxHeight) {
            SideBySide(model, onOpenSystem)
        } else {
            Stacked(model, onOpenSystem)
        }
    }
}

@Composable
private fun Stacked(model: AppModel, onOpenSystem: () -> Unit) {
    val sliderView by model.observe { it.settings.sliderView }
    ScreenColumn {
        TopArea(model, onOpenSystem)
        Toolbar(model)
        ViewSwitch(model, sliderView)
        if (sliderView) {
            SlidersCard(model, fill = false)
        } else {
            GraphCard(model, fill = false)
            BandsCard(model)
        }
        PreampCard(model)
        WarningsCard(model)
    }
}

@Composable
private fun SideBySide(model: AppModel, onOpenSystem: () -> Unit) {
    val sliderView by model.observe { it.settings.sliderView }
    Row(
        Modifier.fillMaxHeight().fillMaxWidth().padding(16.dp),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Column(Modifier.weight(1.25f).fillMaxHeight(), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Toolbar(model)
            if (sliderView) {
                SlidersCard(model, fill = true, modifier = Modifier.weight(1f))
            } else {
                GraphCard(model, fill = true, modifier = Modifier.weight(1f))
            }
        }
        Column(
            Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            TopArea(model, onOpenSystem)
            ViewSwitch(model, sliderView)
            if (!sliderView) BandsCard(model)
            PreampCard(model)
            WarningsCard(model)
        }
    }
}

// ---- top area ----

internal fun outputIcon(kind: OutputKind): Int = when (kind) {
    OutputKind.Speaker -> R.drawable.ic_speaker
    OutputKind.Wired -> R.drawable.ic_headphones
    OutputKind.Usb -> R.drawable.ic_usb
    OutputKind.Bluetooth -> R.drawable.ic_bluetooth
    OutputKind.Other -> R.drawable.ic_device
}

@Composable
private fun TopArea(model: AppModel, onOpenSystem: () -> Unit) {
    val enabled by model.observe { it.settings.enabled }
    val effects by model.observe { it.effects }
    val output by model.observe { it.output }
    val c = EqTheme.colors
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            PowerButton(enabled, { model.setEnabled(it) }, Modifier.testTag("power"))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Ico(outputIcon(output.kind), tint = c.text2, size = 20.dp)
                    Text(
                        output.name,
                        style = EqType.lead,
                        color = c.text,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                EffectStatus(if (enabled) effects else EffectState.Off, onOpenSystem)
            }
        }
        PresetChip(model)
    }
}

@Composable
private fun PowerButton(on: Boolean, onToggle: (Boolean) -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val face by animateColorAsState(if (on) c.accent else c.surface3, label = "power")
    Box(
        modifier = modifier
            .size(76.dp)
            .drawBehind {
                if (on) {
                    drawCircle(
                        Brush.radialGradient(listOf(c.accent.copy(alpha = 0.30f), Color.Transparent)),
                        radius = size.minDimension * 0.75f,
                    )
                }
            }
            .padding(6.dp)
            .clip(CircleShape)
            .background(face)
            .border(1.dp, if (on) c.accent else c.lineStrong, CircleShape)
            .semantics { contentDescription = "Equalizer power" }
            .toggleable(value = on, role = Role.Switch, onValueChange = onToggle),
        contentAlignment = Alignment.Center,
    ) {
        Ico(R.drawable.ic_power, tint = if (on) c.accentOn else c.text2, size = 30.dp)
    }
}

@Composable
private fun EffectStatus(state: EffectState, onOpenSystem: () -> Unit) {
    val c = EqTheme.colors
    val (text, tone) = when (state) {
        EffectState.Off -> "Off" to Tone.Neutral
        EffectState.Active -> "Active" to Tone.Good
        EffectState.Waiting -> "Waiting for a player" to Tone.Warning
        EffectState.Taken -> "Another equalizer has control" to Tone.Warning
        EffectState.Failed -> "This phone refused the effect" to Tone.Danger
    }
    val leads = state == EffectState.Waiting || state == EffectState.Taken || state == EffectState.Failed
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = if (leads) 48.dp else 24.dp)
            .then(if (leads) Modifier.clickable(role = Role.Button, onClickLabel = "Open system status", onClick = onOpenSystem) else Modifier),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Dot(c.tone(tone), size = 8.dp)
        Text(text, style = EqType.small, color = c.tone(tone), modifier = Modifier.weight(1f, fill = false))
        if (leads) Ico(R.drawable.ic_chevron_right, tint = c.text2, size = 18.dp)
    }
}

@Composable
private fun PresetChip(model: AppModel) {
    val c = EqTheme.colors
    var open by rememberSaveable { mutableStateOf(false) }
    val preset by model.observe { it.preset }
    val dirty by model.observe { it.dirty }
    val name = preset ?: if (dirty) "Custom sound" else "Flat"
    val shape = RoundedCornerShape(16.dp)
    Row(
        modifier = Modifier
            .testTag("preset-chip")
            .fillMaxWidth()
            .heightIn(min = 52.dp)
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.line, shape)
            .semantics { contentDescription = "Preset: $name${if (preset != null && dirty) ", edited" else ""}. Opens the preset list" }
            .clickable(role = Role.Button) { open = true }
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Ico(R.drawable.ic_presets, tint = c.text2, size = 20.dp)
        Text(name, style = EqType.bodyStrong, color = c.text, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
        if (preset != null && dirty) Badge("edited", tone = Tone.Warning)
        Spacer(Modifier.weight(1f))
        Ico(R.drawable.ic_chevron_down, tint = c.text2, size = 20.dp)
    }
    if (open) PresetSheet(model) { open = false }
}

// ---- toolbar ----

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Toolbar(model: AppModel) {
    val c = EqTheme.colors
    val canUndo by model.observe { it.canUndo }
    val canRedo by model.observe { it.canRedo }
    val comparing by model.observe { it.comparing }
    val peak by model.observe { it.analysis.netPeakDb }
    FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        EqIconButton(R.drawable.ic_undo, "Undo", { model.undo() }, Modifier.testTag("undo"), enabled = canUndo)
        EqIconButton(R.drawable.ic_redo, "Redo", { model.redo() }, Modifier.testTag("redo"), enabled = canRedo)
        CompareButton(comparing, { model.setComparing(!comparing) }, Modifier.testTag("compare"))
        Box(Modifier.weight(1f).heightIn(min = 48.dp).padding(start = 8.dp), contentAlignment = Alignment.CenterEnd) {
            Text(
                text = "Peak ${gainText(peak)}",
                style = EqType.small,
                color = if (peak > 0.05) c.danger else c.text2,
                fontWeight = if (peak > 0.05) FontWeight.SemiBold else FontWeight.Normal,
                textAlign = TextAlign.End,
            )
        }
    }
}

@Composable
private fun CompareButton(on: Boolean, onToggle: () -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    Row(
        modifier = modifier
            .heightIn(min = 48.dp)
            .clip(ButtonShape)
            .background(if (on) c.accentSoft else c.surface2)
            .border(1.dp, if (on) c.accent else c.lineStrong, ButtonShape)
            .semantics { contentDescription = "Compare with the flat sound" }
            .toggleable(value = on, role = Role.Switch, onValueChange = { onToggle() })
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Ico(R.drawable.ic_compare, tint = if (on) c.accent else c.text, size = 20.dp)
        Text(if (on) "Comparing" else "Compare", style = EqType.label, color = if (on) c.accent else c.text)
    }
}

@Composable
private fun ViewSwitch(model: AppModel, sliderView: Boolean) {
    Segmented(
        options = listOf("Curve", "Sliders"),
        selected = if (sliderView) 1 else 0,
        onSelect = { model.setSliderView(it == 1) },
        icons = listOf(R.drawable.ic_curve, R.drawable.ic_sliders),
        tags = listOf("view-curve", "view-sliders"),
    )
}

// ---- graph ----

private fun Severity.tone(): Tone = when (this) {
    Severity.Info -> Tone.Busy
    Severity.Caution -> Tone.Warning
    Severity.Danger -> Tone.Danger
}

private fun roundQ(q: Double): Double = Math.round(q.coerceIn(0.05, 100.0) * 1000) / 1000.0

@Composable
private fun GraphCard(model: AppModel, fill: Boolean, modifier: Modifier = Modifier) {
    val filters by model.observe { it.profile.filters }
    val analysis by model.observe { it.analysis }
    val selected by model.observe { it.selected }
    val enabled by model.observe { it.settings.enabled }
    val comparing by model.observe { it.comparing }
    val flagged = remember(analysis.warnings) {
        analysis.warnings.filter { it.severity != Severity.Info }.mapNotNull { it.band }.toSet()
    }
    EqCard(modifier, padding = 12.dp) {
        InfoStrip(filters, selected, enabled, comparing)
        Spacer(Modifier.height(8.dp))
        EqGraph(
            filters = filters,
            analysis = analysis,
            selected = selected,
            muted = !enabled || comparing,
            flagged = flagged,
            description = graphDescription(filters, selected),
            onSelect = { model.select(it) },
            onDrag = { i, freq, gain -> model.dragBand(i, freq, gain) },
            onDragEnd = { model.seal() },
            onAdd = { freq, gain -> model.addBand(freq, gain) },
            onQ = { i, q -> model.changeBand(i, "q $i") { it.copy(q = roundQ(q)) } },
            modifier = if (fill) Modifier.weight(1f).fillMaxWidth() else Modifier.fillMaxWidth().height(300.dp),
        )
        Spacer(Modifier.height(8.dp))
        Legend(analysis.right != null, analysis.correction != null, analysis.selected != null && selected != null, selected, enabled && !comparing)
    }
}

@Composable
private fun InfoStrip(filters: List<Filter>, selected: Int?, enabled: Boolean, comparing: Boolean) {
    val c = EqTheme.colors
    val picked = selected?.let { i -> filters.getOrNull(i)?.let { i to it } }
    val band = picked?.second
    val text = when {
        !enabled -> "Equalizer is off: the sound is unchanged"
        comparing -> "Comparing: you hear the flat sound"
        picked != null -> buildString {
            append("Band ${picked.first + 1}  ·  ${picked.second.kind.label}  ·  ${freqText(picked.second.freqHz)}")
            if (picked.second.kind.hasGain) append("  ·  ${gainText(picked.second.gainDb)}")
            append("  ·  Q ${qText(picked.second.q)}")
        }
        filters.isEmpty() -> "Double tap the graph to add a band"
        else -> "Drag a dot to shape the sound. Pinch to change its width."
    }
    val muted = !enabled || comparing
    Row(
        Modifier.fillMaxWidth().heightIn(min = 28.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (muted) Ico(if (enabled) R.drawable.ic_compare else R.drawable.ic_power, tint = c.text2, size = 18.dp)
        Text(
            text,
            style = if (band != null && !muted) EqType.smallStrong else EqType.small,
            color = if (band != null && !muted) c.text else c.text2,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Legend(right: Boolean, correction: Boolean, own: Boolean, selected: Int?, live: Boolean) {
    val c = EqTheme.colors
    FlowRow(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        LegendItem(if (live) c.accentLine else c.text3, if (right) "Left ear" else "Total")
        if (right) LegendItem(c.rightEar, "Right ear")
        if (correction) LegendItem(c.text3, "Correction", thin = true)
        if (own && selected != null) LegendItem(c.band(selected), "Selected band")
    }
}

@Composable
private fun LegendItem(color: Color, text: String, thin: Boolean = false) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.width(14.dp).height(if (thin) 1.dp else 2.dp).background(color, CircleShape))
        Text(text, style = EqType.caption, color = EqTheme.colors.text2)
    }
}

// ---- sliders ----

@Composable
private fun SlidersCard(model: AppModel, fill: Boolean, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val sliders by model.observe { it.analysis.sliders }
    val enabled by model.observe { it.settings.enabled }
    val comparing by model.observe { it.comparing }
    var active by remember { mutableStateOf(false) }
    var range by remember { mutableFloatStateOf(12f) }
    val top = sliders.maxOfOrNull { abs(it) } ?: 0f
    LaunchedEffect(top, active) { if (!active) range = if (top > 12f) rangeFor(top) else 12f }
    EqCard(modifier, padding = 8.dp) {
        Row(
            modifier = if (fill) Modifier.fillMaxWidth().weight(1f) else Modifier.fillMaxWidth().height(280.dp),
        ) {
            sliders.take(SliderLabels.size).forEachIndexed { i, gain ->
                SliderColumn(
                    index = i,
                    label = SliderLabels[i],
                    value = gain,
                    range = range,
                    muted = !enabled || comparing,
                    onStart = { active = true },
                    onChange = { model.setSlider(i, it) },
                    onFinish = {
                        model.seal()
                        active = false
                    },
                    modifier = Modifier.weight(1f),
                )
            }
        }
        Text(
            "Range ±${range.roundToInt()} dB. Drag a slider, or tap where you want it.",
            style = EqType.caption,
            color = c.text3,
            modifier = Modifier.padding(start = 8.dp, top = 6.dp),
        )
    }
}

@Composable
private fun SliderColumn(
    index: Int,
    label: String,
    value: Float,
    range: Float,
    muted: Boolean,
    onStart: () -> Unit,
    onChange: (Float) -> Unit,
    onFinish: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = EqTheme.colors
    var live by remember { mutableFloatStateOf(Float.NaN) }
    var touching by remember { mutableStateOf(false) }
    LaunchedEffect(value, touching, live) {
        if (!touching) {
            delay(150)
            live = Float.NaN
        }
    }
    val rangeNow by rememberUpdatedState(range)
    val start by rememberUpdatedState(onStart)
    val change by rememberUpdatedState(onChange)
    val finish by rememberUpdatedState(onFinish)
    var height by remember { mutableFloatStateOf(1f) }
    val shown = (if (live.isNaN()) value else live).coerceIn(-range, range)
    val edge = with(LocalDensity.current) { 14.dp.toPx() }
    fun set(y: Float) {
        val f = ((y - edge) / (height - 2 * edge).coerceAtLeast(1f)).coerceIn(0f, 1f)
        val v = rangeNow - f * 2f * rangeNow
        live = v
        change(v)
    }
    val fixed = EqType.caption.copy(fontSize = fixedSp(11f))
    val active = if (muted) c.text3 else c.accent
    Column(modifier.fillMaxHeight(), horizontalAlignment = Alignment.CenterHorizontally) {
        Text(
            dbLabel(shown),
            style = fixed,
            color = if (abs(shown) < 0.05f) c.text2 else c.text,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier.padding(vertical = 6.dp),
        )
        Box(
            Modifier
                .testTag("slider-$index")
                .weight(1f)
                .fillMaxWidth()
                .onSizeChanged { height = it.height.toFloat() }
                .semantics {
                    contentDescription = "$label hertz band"
                    stateDescription = gainText(shown.toDouble())
                    progressBarRangeInfo = ProgressBarRangeInfo(shown, -range..range)
                    setProgress { target ->
                        change(target.coerceIn(-rangeNow, rangeNow))
                        finish()
                        true
                    }
                }
                .pointerInput(Unit) {
                    detectTapGestures(onTap = {
                        set(it.y)
                        finish()
                    })
                }
                .pointerInput(Unit) {
                    detectVerticalDragGestures(
                        onDragStart = {
                            touching = true
                            start()
                            set(it.y)
                        },
                        onDragEnd = {
                            touching = false
                            finish()
                        },
                        onDragCancel = {
                            touching = false
                            finish()
                        },
                        onVerticalDrag = { pointer, _ ->
                            set(pointer.position.y)
                            pointer.consume()
                        },
                    )
                }
                .drawBehind {
                    val x = size.width / 2
                    val h = size.height - 2 * edge
                    val groove = 4.dp.toPx()
                    val zero = edge + h / 2
                    val thumb = edge + h * (1f - (shown + range) / (2f * range))
                    drawLine(c.surface3, Offset(x, edge), Offset(x, edge + h), groove, StrokeCap.Round)
                    drawLine(c.gridStrong, Offset(x - 9.dp.toPx(), zero), Offset(x + 9.dp.toPx(), zero), 1.dp.toPx())
                    drawLine(active, Offset(x, zero), Offset(x, thumb), groove, StrokeCap.Round)
                    val tw = 26.dp.toPx()
                    val th = 14.dp.toPx()
                    drawRoundRect(c.surface, Offset(x - tw / 2, thumb - th / 2), Size(tw, th), CornerRadius(th / 2))
                    drawRoundRect(
                        active,
                        Offset(x - tw / 2, thumb - th / 2),
                        Size(tw, th),
                        CornerRadius(th / 2),
                        style = Stroke(1.5.dp.toPx()),
                    )
                },
        )
        Text(
            label,
            style = fixed,
            color = c.text2,
            maxLines = 1,
            softWrap = false,
            modifier = Modifier.padding(vertical = 6.dp),
        )
    }
}

// ---- bands ----

@Composable
private fun BandsCard(model: AppModel) {
    val c = EqTheme.colors
    val filters by model.observe { it.profile.filters }
    val selected by model.observe { it.selected }
    val warnings by model.observe { it.analysis.warnings }
    val flagged = remember(warnings) {
        warnings.filter { it.severity != Severity.Info }.mapNotNull { it.band }.toSet()
    }
    EqCard {
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Bands", style = EqType.lead, color = c.text)
                Text("${filters.size}", style = EqType.lead, color = c.text3)
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                EqButton(
                    "Add band",
                    { model.addBand() },
                    Modifier.weight(1f).testTag("add-band"),
                    kind = ButtonKind.Primary,
                    icon = R.drawable.ic_plus,
                )
                EqButton("Clear", { model.clearBands() }, kind = ButtonKind.Ghost, enabled = filters.isNotEmpty())
            }
            if (filters.isNotEmpty()) BandChips(filters, selected, flagged) { model.select(it) }
            val sel = selected
            val band = sel?.let { filters.getOrNull(it) }
            if (sel != null && band != null) {
                Divider()
                BandEditor(model, sel, band)
            } else {
                Text(
                    if (filters.isEmpty()) "No bands yet. Choose Add band, or double tap the graph." else "Select a band above, or tap one on the graph, to edit it.",
                    style = EqType.small,
                    color = c.text2,
                )
            }
        }
    }
}

@Composable
private fun BandChips(filters: List<Filter>, selected: Int?, flagged: Set<Int>, onSelect: (Int) -> Unit) {
    val state = rememberLazyListState()
    LaunchedEffect(selected) {
        if (selected != null && state.layoutInfo.visibleItemsInfo.none { it.index == selected }) {
            state.animateScrollToItem(selected)
        }
    }
    LazyRow(state = state, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        items(count = filters.size, key = { it }) { i ->
            BandChip(i, filters[i], i == selected, i in flagged) { onSelect(i) }
        }
    }
}

@Composable
private fun BandChip(index: Int, f: Filter, selected: Boolean, flagged: Boolean, onClick: () -> Unit) {
    val c = EqTheme.colors
    val color = c.band(index)
    val shape = RoundedCornerShape(14.dp)
    Box(
        modifier = Modifier
            .testTag("band-$index")
            .size(48.dp)
            .clip(shape)
            .background(if (selected) color.copy(alpha = 0.16f) else Color.Transparent)
            .then(if (selected) Modifier.border(2.dp, color, shape) else Modifier)
            .semantics {
                contentDescription = "Band ${index + 1}, ${freqText(f.freqHz)}${if (f.on) "" else ", off"}"
            }
            .selectable(selected = selected, role = Role.Tab, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier
                .size(28.dp)
                .background(if (f.on) color else Color.Transparent, CircleShape)
                .border(2.dp, color, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                "${index + 1}",
                style = EqType.caption.copy(fontSize = fixedSp(12f), fontWeight = FontWeight.Bold),
                color = if (f.on) c.onBand else color,
            )
        }
        if (flagged) Dot(c.warn, Modifier.align(Alignment.TopEnd).padding(7.dp), size = 8.dp)
    }
}

private enum class Field(val title: String, val hint: String) {
    Freq("Frequency", "For example 120, 1.2k or 1.2 kHz"),
    Gain("Gain", "For example 3, -2.5 or 4 dB"),
    Q("Width (Q)", "A higher number is a narrower band. For example 1.4"),
}

private fun plain(v: Double, places: Int): String {
    val s = String.format(Locale.ROOT, "%.${places}f", v)
    return if ('.' in s) s.trimEnd('0').trimEnd('.') else s
}

@Composable
private fun BandEditor(model: AppModel, i: Int, f: Filter) {
    val c = EqTheme.colors
    var typing by remember { mutableStateOf<Field?>(null) }
    var menu by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(12.dp)

    fun nudgeFreq(dir: Int) = model.changeBand(i, "nf $i") {
        it.copy(freqHz = roundFreq((it.freqHz * 2.0.pow(dir / 12.0)).coerceIn(10.0, 24000.0)))
    }

    fun nudgeGain(dir: Int) = model.changeBand(i, "ng $i") {
        it.copy(gainDb = roundGain((it.gainDb + dir * 0.5).coerceIn(-60.0, 60.0)))
    }

    fun nudgeQ(dir: Int) = model.changeBand(i, "nq $i") { it.copy(q = roundQ(it.q * 1.1.pow(dir))) }

    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Dot(c.band(i), size = 12.dp)
            Text("Band ${i + 1}", style = EqType.lead, color = c.text, modifier = Modifier.weight(1f))
            Row(
                Modifier
                    .heightIn(min = 48.dp)
                    .clip(shape)
                    .semantics { contentDescription = "Band ${i + 1} on" }
                    .toggleable(value = f.on, role = Role.Switch) { v -> model.changeBand(i) { it.copy(on = v) } }
                    .padding(horizontal = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("On", style = EqType.small, color = c.text2)
                EqSwitch(f.on, null)
            }
            EqIconButton(R.drawable.ic_trash, "Remove band ${i + 1}", { model.removeBand(i) }, Modifier.testTag("delete-band"), tint = c.danger)
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Type", style = EqType.small, color = c.text2)
            Box {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .clip(shape)
                        .background(c.surface2)
                        .border(1.dp, c.line, shape)
                        .semantics { contentDescription = "Filter type, ${f.kind.label}" }
                        .clickable(role = Role.DropdownList) { menu = true }
                        .padding(horizontal = 14.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(f.kind.label, style = EqType.body, color = c.text, modifier = Modifier.weight(1f))
                    Ico(R.drawable.ic_chevron_down, tint = c.text2, size = 20.dp)
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }, containerColor = c.surface) {
                    Kind.entries.forEach { k ->
                        DropdownMenuItem(
                            text = { Text(k.label, style = EqType.body, color = c.text) },
                            onClick = {
                                menu = false
                                model.changeBand(i) { it.copy(kind = k) }
                            },
                        )
                    }
                }
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Ear", style = EqType.small, color = c.text2)
            Segmented(
                options = Ear.entries.map { it.label },
                selected = Ear.entries.indexOf(f.channel),
                onSelect = { n -> model.changeBand(i) { it.copy(channel = Ear.entries[n]) } },
            )
        }
        ParamRow(
            label = "Frequency",
            valueText = freqText(f.freqHz),
            onType = { typing = Field.Freq },
            onMinus = { nudgeFreq(-1) },
            onPlus = { nudgeFreq(1) },
        ) { m ->
            EqSlider(
                value = xOf(f.freqHz).toFloat().coerceIn(0f, 1f),
                onChange = { v ->
                    model.changeBand(i, "f $i") { it.copy(freqHz = roundFreq(freqOf(v.toDouble()))) }
                },
                onFinish = { model.seal() },
                range = 0f..1f,
                label = "Frequency",
                valueText = freqText(f.freqHz),
                modifier = m,
            )
        }
        ParamRow(
            label = "Gain",
            valueText = if (f.kind.hasGain) gainText(f.gainDb) else "Not used",
            onType = { typing = Field.Gain },
            onMinus = { nudgeGain(-1) },
            onPlus = { nudgeGain(1) },
            enabled = f.kind.hasGain,
        ) { m ->
            EqSlider(
                value = f.gainDb.toFloat().coerceIn(-18f, 18f),
                onChange = { v -> model.changeBand(i, "g $i") { it.copy(gainDb = roundGain(v.toDouble())) } },
                onFinish = { model.seal() },
                range = -18f..18f,
                label = "Gain",
                valueText = gainText(f.gainDb),
                modifier = m,
                enabled = f.kind.hasGain,
                origin = 0f,
            )
        }
        ParamRow(
            label = "Width (Q)",
            valueText = qText(f.q),
            onType = { typing = Field.Q },
            onMinus = { nudgeQ(-1) },
            onPlus = { nudgeQ(1) },
        ) { m ->
            EqSlider(
                value = (ln(f.q.coerceIn(0.1, 20.0) / 0.1) / LnQ).toFloat(),
                onChange = { v -> model.changeBand(i, "q $i") { it.copy(q = roundQ(0.1 * exp(v.toDouble() * LnQ))) } },
                onFinish = { model.seal() },
                range = 0f..1f,
                label = "Width, Q",
                valueText = qText(f.q),
                modifier = m,
            )
        }
    }
    typing?.let { field ->
        TextDialog(
            title = field.title,
            initial = when (field) {
                Field.Freq -> if (f.freqHz >= 1000) plain(f.freqHz / 1000, 2) + "k" else plain(f.freqHz, 1)
                Field.Gain -> plain(f.gainDb, 1)
                Field.Q -> plain(f.q, 3)
            },
            action = "Set",
            hint = field.hint,
            onDismiss = { typing = null },
            onDone = { text ->
                val v = model.number(text)
                if (v == null || !v.isFinite()) {
                    model.toast("That is not a number", Tone.Warning)
                } else {
                    model.seal()
                    model.changeBand(i) {
                        when (field) {
                            Field.Freq -> it.copy(freqHz = roundFreq(v.coerceIn(10.0, 24000.0)))
                            Field.Gain -> it.copy(gainDb = roundGain(v.coerceIn(-60.0, 60.0)))
                            Field.Q -> it.copy(q = roundQ(v))
                        }
                    }
                }
                typing = null
            },
        )
    }
}

// ---- preamp and advice ----

@Composable
private fun PreampCard(model: AppModel) {
    val c = EqTheme.colors
    val preamp by model.observe { it.profile.preamp }
    val autoDb by model.observe { it.analysis.preampDb }
    var typing by remember { mutableStateOf(false) }
    EqCard {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Preamp", style = EqType.lead, color = c.text)
            SwitchRow(
                title = "Automatic",
                checked = preamp.auto,
                onChange = { model.setAutoPreamp(it) },
                modifier = Modifier.testTag("preamp-auto"),
                detail = if (preamp.auto) {
                    "${gainText(autoDb)}. Makes room for boosts so they do not distort."
                } else {
                    "Set by hand. Watch the peak when you boost."
                },
            )
            if (!preamp.auto) {
                ParamRow(
                    label = "Preamp",
                    valueText = gainText(preamp.db),
                    onType = { typing = true },
                    onMinus = { model.setPreamp((preamp.db - 0.5).coerceIn(-30.0, 12.0)) },
                    onPlus = { model.setPreamp((preamp.db + 0.5).coerceIn(-30.0, 12.0)) },
                ) { m ->
                    EqSlider(
                        value = preamp.db.toFloat().coerceIn(-30f, 12f),
                        onChange = { model.setPreamp(it.toDouble()) },
                        onFinish = { model.seal() },
                        range = -30f..12f,
                        label = "Preamp",
                        valueText = gainText(preamp.db),
                        modifier = m,
                        origin = 0f,
                    )
                }
            }
        }
    }
    if (typing) {
        TextDialog(
            title = "Preamp",
            initial = plain(preamp.db, 1),
            action = "Set",
            hint = "From -30 to 12 dB. For example -6",
            onDismiss = { typing = false },
            onDone = { text ->
                val v = model.number(text)
                if (v == null || !v.isFinite()) {
                    model.toast("That is not a number", Tone.Warning)
                } else {
                    model.seal()
                    model.setPreamp(v.coerceIn(-30.0, 12.0))
                    model.seal()
                }
                typing = false
            },
        )
    }
}

@Composable
private fun WarningsCard(model: AppModel) {
    val c = EqTheme.colors
    val warnings by model.observe { it.analysis.warnings }
    if (warnings.isEmpty()) return
    EqCard {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Suggestions", style = EqType.lead, color = c.text)
            Text("Advice only, nothing is blocked.", style = EqType.small, color = c.text2)
            warnings.forEach { w ->
                val tone = w.severity.tone()
                val band = w.band
                Row(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = 48.dp)
                        .clip(RoundedCornerShape(12.dp))
                        .then(
                            if (band != null) {
                                Modifier.clickable(role = Role.Button, onClickLabel = "Select band ${band + 1}") { model.select(band) }
                            } else {
                                Modifier
                            },
                        )
                        .padding(vertical = 8.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Ico(toneIcon(tone), tint = c.tone(tone), size = 20.dp, modifier = Modifier.padding(top = 1.dp))
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(w.title, style = EqType.bodyStrong, color = c.text)
                        Text(w.detail, style = EqType.small, color = c.text2)
                    }
                }
            }
        }
    }
}
