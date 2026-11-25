package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.Analysis
import io.github.adityatummuri.eqaura.Filter
import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.ln
import kotlin.math.max
import kotlin.math.roundToInt

private val Steps = floatArrayOf(12f, 18f, 24f, 36f, 48f, 72f, 120f, 240f)

/** The smallest range that shows [db] with a little headroom. */
internal fun rangeFor(db: Float): Float {
    val need = abs(db) * 1.04f
    return Steps.firstOrNull { it >= need } ?: Steps.last()
}

internal fun nextRange(range: Float): Float = Steps.firstOrNull { it > range } ?: range

private val LogSpan = ln(1000.0)

internal fun xOf(freq: Double): Double = ln(freq / 20.0) / LogSpan

internal fun freqOf(x: Double): Double = 20.0 * exp(x * LogSpan)

private fun yOf(db: Float, range: Float): Float = 0.5f - db / (2f * range)

/** "+3", "−4.5", "0" */
internal fun dbLabel(db: Float): String {
    val tenths = (abs(db) * 10).roundToInt()
    if (tenths == 0) return "0"
    val text = if (tenths % 10 == 0) "${tenths / 10}" else "${tenths / 10}.${tenths % 10}"
    return (if (db < 0) "−" else "+") + text
}

private val GridX: List<Pair<Double, Boolean>> = buildList {
    for (decade in listOf(10.0, 100.0, 1000.0, 10000.0)) {
        for (k in 1..9) {
            val f = decade * k
            if (f in 20.0..20000.0) add(f to (f == 100.0 || f == 1000.0 || f == 10000.0))
        }
    }
}

private val LabelX = listOf(
    50.0 to "50", 100.0 to "100", 200.0 to "200", 500.0 to "500",
    1000.0 to "1k", 2000.0 to "2k", 5000.0 to "5k", 10000.0 to "10k",
)

private val PadLeft = 44.dp
private val PadTop = 12.dp
private val PadRight = 14.dp
private val PadBottom = 26.dp

/** How tall the graph's own curves are allowed to be, so that zooming only follows what is shown. */
private fun wantedRange(filters: List<Filter>, a: Analysis): Float {
    var top = 0f
    fun take(v: Float) {
        // High and low pass filters fall without end; they should not zoom out the graph.
        if (v.isNaN() || v <= -60f) return
        val m = if (v > 0f) v else -v * 0.999f
        if (m > top) top = m
    }
    a.left.forEach { take(it) }
    a.right?.forEach { take(it) }
    filters.forEach { if (it.kind.hasGain) take(it.gainDb.toFloat()) }
    val steep = filters.any { it.on && !it.kind.hasGain }
    val want = if (steep) {
        filters.filter { it.kind.hasGain }.maxOfOrNull { abs(it.gainDb).toFloat() } ?: 0f
    } else {
        top
    }
    return rangeFor(want)
}

private fun dotCenter(f: Filter, left: Float, top: Float, w: Float, h: Float, range: Float): Offset {
    val xf = xOf(f.freqHz).toFloat()
    val x = if (xf.isNaN()) 0.5f else xf.coerceIn(0f, 1f)
    val gain = if (f.kind.hasGain) f.gainDb.toFloat() else 0f
    val yf = yOf(if (gain.isNaN()) 0f else gain, range).coerceIn(0f, 1f)
    return Offset(left + w * x, top + h * yf)
}

private fun curvePath(values: List<Float>, left: Float, top: Float, w: Float, h: Float, range: Float, fill: Boolean): Path {
    val path = Path()
    val n = max(values.size - 1, 1)
    val zero = top + h * yOf(0f, range)
    values.forEachIndexed { i, v ->
        val x = left + w * i / n
        val y = (top + h * yOf(if (v.isNaN()) 0f else v, range)).coerceIn(top - h * 0.5f, top + h * 1.5f)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
    }
    if (fill && values.isNotEmpty()) {
        path.lineTo(left + w, zero)
        path.lineTo(left, zero)
        path.close()
    }
    return path
}

private class GraphInput(
    val filters: List<Filter>,
    val selected: Int?,
    val onSelect: (Int?) -> Unit,
    val onDrag: (Int, Double, Double) -> Unit,
    val onDragEnd: () -> Unit,
    val onAdd: (Double, Double) -> Unit,
    val onQ: (Int, Double) -> Unit,
)

/**
 * Log frequency across, dB up. Drag a dot to move a band, tap to select, double tap
 * empty space to add a band, pinch to change the selected band's Q.
 */
@Composable
internal fun EqGraph(
    filters: List<Filter>,
    analysis: Analysis,
    selected: Int?,
    muted: Boolean,
    flagged: Set<Int>,
    description: String,
    onSelect: (Int?) -> Unit,
    onDrag: (Int, Double, Double) -> Unit,
    onDragEnd: () -> Unit,
    onAdd: (Double, Double) -> Unit,
    onQ: (Int, Double) -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = EqTheme.colors
    val density = LocalDensity.current
    val measurer = rememberTextMeasurer()
    val wanted = remember(filters, analysis) { wantedRange(filters, analysis) }
    val range = remember { mutableFloatStateOf(wanted) }
    val dragging = remember { mutableStateOf(false) }
    // The scale only follows the sound while nothing is being dragged.
    LaunchedEffect(wanted, dragging.value) { if (!dragging.value) range.floatValue = wanted }

    val labelStyle = remember(c, density) { TextStyle(color = c.text3, fontSize = with(density) { 10.dp.toSp() }) }
    val numberStyle = remember(density) {
        TextStyle(fontSize = with(density) { 11.dp.toSp() }, fontWeight = FontWeight.Bold)
    }
    val xLabels = remember(labelStyle) { LabelX.map { it.first to measurer.measure(it.second, labelStyle) } }
    val r = range.floatValue
    val yLabels = remember(r, labelStyle) { (-3..3).map { measurer.measure(dbLabel(r / 4f * it), labelStyle) } }
    val numbers = remember(filters.size, numberStyle) { List(filters.size) { measurer.measure("${it + 1}", numberStyle) } }

    val input = rememberUpdatedState(GraphInput(filters, selected, onSelect, onDrag, onDragEnd, onAdd, onQ))

    Box(
        modifier = modifier
            .testTag("graph")
            .semantics { contentDescription = description }
            .clip(RoundedCornerShape(16.dp))
            .background(c.sunken)
            .pointerInput(Unit) {
                val hit = 28.dp.toPx()
                val doubleTapGap = 40.dp.toPx()
                val slop = viewConfiguration.touchSlop
                var lastTap = 0L
                var lastTapAt = Offset.Zero
                awaitEachGesture {
                    val left = PadLeft.toPx()
                    val top = PadTop.toPx()
                    val w = (size.width - left - PadRight.toPx()).coerceAtLeast(1f)
                    val h = (size.height - top - PadBottom.toPx()).coerceAtLeast(1f)
                    val down = awaitFirstDown(requireUnconsumed = false)
                    val start = input.value
                    var target = -1
                    var best = hit
                    start.filters.forEachIndexed { i, f ->
                        val d = (dotCenter(f, left, top, w, h, range.floatValue) - down.position).getDistance()
                        if (d < best) {
                            best = d
                            target = i
                        }
                    }
                    if (target >= 0) {
                        down.consume()
                        start.onSelect(target)
                    }
                    val grab = if (target >= 0) {
                        dotCenter(start.filters[target], left, top, w, h, range.floatValue) - down.position
                    } else {
                        Offset.Zero
                    }
                    var moved = false
                    var pinching = false
                    var baseDistance = 1f
                    var baseQ = 1.0
                    var lastGrow = 0L
                    while (true) {
                        val event = awaitPointerEvent()
                        if (target < 0 && event.changes.any { it.isConsumed }) return@awaitEachGesture
                        val pressed = event.changes.filter { it.pressed }
                        if (pressed.isEmpty()) break
                        val now = input.value
                        if (pressed.size >= 2) {
                            val sel = now.selected
                            if (sel != null && sel in now.filters.indices) {
                                val d = max((pressed[0].position - pressed[1].position).getDistance(), 1f)
                                if (!pinching) {
                                    pinching = true
                                    baseDistance = d
                                    baseQ = now.filters[sel].q
                                } else {
                                    now.onQ(sel, baseQ * baseDistance / d)
                                }
                            }
                            event.changes.forEach { it.consume() }
                            continue
                        }
                        if (pinching) {
                            event.changes.forEach { it.consume() }
                            continue
                        }
                        val pointer = pressed[0]
                        if (!moved && (pointer.position - down.position).getDistance() > slop) {
                            moved = true
                            if (target >= 0) dragging.value = true
                        }
                        if (moved && target >= 0 && target < now.filters.size) {
                            val tx = pointer.position.x + grab.x
                            val ty = pointer.position.y + grab.y
                            val xf = ((tx - left) / w).coerceIn(0f, 1f)
                            var gain = (0.5f - (ty - top) / h) * 2f * range.floatValue
                            if (now.filters[target].kind.hasGain) {
                                if (abs(gain) > range.floatValue && pointer.uptimeMillis - lastGrow > 350) {
                                    range.floatValue = nextRange(range.floatValue)
                                    lastGrow = pointer.uptimeMillis
                                }
                                gain = gain.coerceIn(-range.floatValue, range.floatValue)
                            }
                            now.onDrag(target, freqOf(xf.toDouble()), gain.toDouble())
                        }
                        if (target >= 0) pointer.consume()
                    }
                    if (target >= 0 && (moved || pinching)) {
                        dragging.value = false
                        input.value.onDragEnd()
                    }
                    if (target < 0 && !moved && !pinching) {
                        val p = down.position
                        val inPlot = p.x in left..(left + w) && p.y in top..(top + h)
                        if (inPlot && down.uptimeMillis - lastTap < 300 && (p - lastTapAt).getDistance() < doubleTapGap) {
                            lastTap = 0L
                            val gain = ((0.5f - (p.y - top) / h) * 2f * range.floatValue).coerceIn(-range.floatValue, range.floatValue)
                            input.value.onAdd(freqOf(((p.x - left) / w).toDouble()), gain.toDouble())
                        } else {
                            lastTap = down.uptimeMillis
                            lastTapAt = p
                            input.value.onSelect(null)
                        }
                    } else if (target < 0 && pinching) {
                        input.value.onDragEnd()
                    }
                }
            },
    ) {
        // Grid, labels and curves. They only change with the sound, so they get a layer of their own.
        Spacer(
            Modifier
                .fillMaxSize()
                .graphicsLayer()
                .drawWithCache {
                    val left = PadLeft.toPx()
                    val top = PadTop.toPx()
                    val w = (size.width - left - PadRight.toPx()).coerceAtLeast(1f)
                    val h = (size.height - top - PadBottom.toPx()).coerceAtLeast(1f)
                    val line = if (muted) c.text3 else c.accentLine
                    val total = curvePath(analysis.left, left, top, w, h, r, false)
                    val area = curvePath(analysis.left, left, top, w, h, r, true)
                    val right = analysis.right?.let { curvePath(it, left, top, w, h, r, false) }
                    val correction = analysis.correction?.let { curvePath(it, left, top, w, h, r, false) }
                    val own = if (selected != null) analysis.selected?.let { curvePath(it, left, top, w, h, r, false) } else null
                    val fillBrush = Brush.verticalGradient(
                        colors = listOf(line.copy(alpha = if (muted) 0.10f else if (c.dark) 0.24f else 0.22f), line.copy(alpha = 0f)),
                        startY = top,
                        endY = top + h,
                    )
                    val hair = 1.dp.toPx()
                    val dash = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 5.dp.toPx()))
                    onDrawBehind {
                        for (k in -3..3) {
                            val y = top + h * yOf(k * r / 4f, r)
                            drawLine(if (k == 0) c.gridStrong else c.grid, Offset(left, y), Offset(left + w, y), hair)
                        }
                        for ((f, strong) in GridX) {
                            val x = left + w * xOf(f).toFloat()
                            drawLine(if (strong) c.gridStrong else c.grid, Offset(x, top), Offset(x, top + h), hair)
                        }
                        for ((f, text) in xLabels) {
                            val x = left + w * xOf(f).toFloat() - text.size.width / 2f
                            drawText(text, topLeft = Offset(x, top + h + 6.dp.toPx()))
                        }
                        yLabels.forEachIndexed { i, text ->
                            val y = top + h * yOf((i - 3) * r / 4f, r) - text.size.height / 2f
                            drawText(
                                text,
                                color = if (i == 3) c.text2 else c.text3,
                                topLeft = Offset(left - 8.dp.toPx() - text.size.width, y),
                            )
                        }
                        clipRect(left, top, left + w, top + h) {
                            if (correction != null) {
                                drawPath(
                                    correction,
                                    c.text3.copy(alpha = 0.8f),
                                    style = Stroke(1.25.dp.toPx(), cap = StrokeCap.Round, pathEffect = dash),
                                )
                            }
                            drawPath(area, fillBrush)
                            if (own != null && selected != null) {
                                drawPath(
                                    own,
                                    if (muted) c.text3.copy(alpha = 0.6f) else c.band(selected),
                                    style = Stroke(1.5.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
                                )
                            }
                            if (right != null) {
                                drawPath(
                                    right,
                                    if (muted) c.text3.copy(alpha = 0.7f) else c.rightEar,
                                    style = Stroke(2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
                                )
                            }
                            drawPath(total, line, style = Stroke(2.25.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round))
                        }
                    }
                },
        )
        // Band dots change while dragging, so they are drawn apart from the curves.
        Spacer(
            Modifier
                .fillMaxSize()
                .drawBehind {
                    val left = PadLeft.toPx()
                    val top = PadTop.toPx()
                    val w = (size.width - left - PadRight.toPx()).coerceAtLeast(1f)
                    val h = (size.height - top - PadBottom.toPx()).coerceAtLeast(1f)
                    val rr = range.floatValue
                    if (selected != null && selected in filters.indices) {
                        val x = dotCenter(filters[selected], left, top, w, h, rr).x
                        val guide = if (muted) c.text3.copy(alpha = 0.25f) else c.band(selected).copy(alpha = 0.4f)
                        drawLine(guide, Offset(x, top), Offset(x, top + h), 1.dp.toPx())
                    }
                    filters.forEachIndexed { i, f ->
                        val center = dotCenter(f, left, top, w, h, rr)
                        val color = c.band(i)
                        val isSelected = i == selected
                        val radius = (if (isSelected) 12.dp else 10.dp).toPx()
                        if (isSelected) {
                            drawCircle(color.copy(alpha = 0.16f), 20.dp.toPx(), center)
                            drawCircle(
                                color.copy(alpha = if (muted) 0.5f else 0.9f),
                                20.dp.toPx(),
                                center,
                                style = Stroke(1.5.dp.toPx()),
                            )
                        }
                        if (i in flagged && !isSelected) {
                            drawCircle(c.warn, radius + 5.dp.toPx(), center, style = Stroke(1.5.dp.toPx()))
                        }
                        val face: Color = when {
                            !f.on -> c.sunken
                            muted -> color.copy(alpha = 0.55f)
                            else -> color
                        }
                        drawCircle(face, radius, center)
                        drawCircle(if (f.on) c.sunken else color, radius, center, style = Stroke(2.dp.toPx()))
                        numbers.getOrNull(i)?.let { n ->
                            drawText(
                                n,
                                color = if (f.on) c.onBand else color,
                                topLeft = Offset(center.x - n.size.width / 2f, center.y - n.size.height / 2f),
                            )
                        }
                    }
                },
        )
    }
}

/** Describes the graph for screen readers. Editing is done with the band list. */
internal fun graphDescription(filters: List<Filter>, selected: Int?): String = buildString {
    append("Equalizer graph from 20 hertz to 20 kilohertz. ")
    append(
        when (filters.size) {
            0 -> "No bands. "
            1 -> "One band. "
            else -> "${filters.size} bands. "
        },
    )
    if (selected != null && selected in filters.indices) append("Band ${selected + 1} is selected. ")
    append("Use the band list below to edit bands with buttons.")
}
