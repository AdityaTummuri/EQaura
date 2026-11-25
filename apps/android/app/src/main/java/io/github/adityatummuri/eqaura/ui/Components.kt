package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextFieldColors
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Tone
import io.github.adityatummuri.eqaura.UiState
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

internal val MaxContentWidth = 640.dp

internal val CardShape = RoundedCornerShape(18.dp)
internal val ButtonShape = RoundedCornerShape(14.dp)

/** Follows one part of the state, so a screen only redraws when that part changes. */
@Composable
internal fun <T> AppModel.observe(select: (UiState) -> T): State<T> {
    val flow = remember(this) { state.map { select(it) }.distinctUntilChanged() }
    return flow.collectAsStateWithLifecycle(snapshot(select))
}

private fun <T> AppModel.snapshot(select: (UiState) -> T): T = select(state.value)

/** Text that must keep its size, such as labels inside a graph. */
@Composable
internal fun fixedSp(size: Float): TextUnit = with(LocalDensity.current) { size.dp.toSp() }

/** Limits how much the system font size can grow in a tight place. */
@Composable
internal fun ClampFont(max: Float, content: @Composable () -> Unit) {
    val d = LocalDensity.current
    val capped = remember(d.density, d.fontScale) { Density(d.density, minOf(d.fontScale, max)) }
    CompositionLocalProvider(LocalDensity provides capped) { content() }
}

@Composable
internal fun Ico(icon: Int, modifier: Modifier = Modifier, tint: Color = EqTheme.colors.text2, size: Dp = 24.dp) {
    Icon(painter = painterResource(icon), contentDescription = null, modifier = modifier.size(size), tint = tint)
}

@Composable
internal fun EqCard(
    modifier: Modifier = Modifier,
    padding: Dp = 16.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val c = EqTheme.colors
    Column(
        modifier = modifier
            .fillMaxWidth()
            .clip(CardShape)
            .background(c.surface)
            .border(1.dp, c.line, CardShape)
            .padding(padding),
        content = content,
    )
}

@Composable
internal fun Divider(modifier: Modifier = Modifier) {
    HorizontalDivider(modifier = modifier, thickness = 1.dp, color = EqTheme.colors.line)
}

internal enum class ButtonKind { Primary, Secondary, Ghost, Danger }

@Composable
internal fun EqButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    kind: ButtonKind = ButtonKind.Secondary,
    icon: Int? = null,
    enabled: Boolean = true,
) {
    val c = EqTheme.colors
    val background = when (kind) {
        ButtonKind.Primary -> c.accent
        ButtonKind.Secondary -> c.surface2
        else -> Color.Transparent
    }
    val content = when (kind) {
        ButtonKind.Primary -> c.accentOn
        ButtonKind.Danger -> c.danger
        else -> c.text
    }
    val edge = when (kind) {
        ButtonKind.Secondary -> c.lineStrong
        ButtonKind.Danger -> c.danger.copy(alpha = 0.5f)
        else -> Color.Transparent
    }
    Row(
        modifier = modifier
            .alpha(if (enabled) 1f else 0.4f)
            .heightIn(min = 48.dp)
            .clip(ButtonShape)
            .background(background)
            .border(1.dp, edge, ButtonShape)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick)
            .padding(horizontal = 18.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
    ) {
        if (icon != null) Ico(icon, tint = content, size = 20.dp)
        Text(text, style = EqType.label, color = content, textAlign = TextAlign.Center)
    }
}

@Composable
internal fun EqIconButton(
    icon: Int,
    description: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    tint: Color = EqTheme.colors.text,
    background: Color = Color.Transparent,
) {
    Box(
        modifier = modifier
            .size(48.dp)
            .alpha(if (enabled) 1f else 0.4f)
            .clip(CircleShape)
            .background(background)
            .semantics { contentDescription = description }
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Ico(icon, tint = tint)
    }
}

@Composable
internal fun SectionTitle(text: String, modifier: Modifier = Modifier) {
    Text(
        text = text,
        style = EqType.caption,
        color = EqTheme.colors.text2,
        modifier = modifier.semantics { heading() },
    )
}

@Composable
internal fun Badge(text: String, modifier: Modifier = Modifier, tone: Tone = Tone.Neutral, icon: Int? = null) {
    val c = EqTheme.colors
    val color = c.tone(tone)
    Row(
        modifier = modifier
            .clip(CircleShape)
            .background(if (tone == Tone.Neutral) c.surface3 else c.toneSoft(tone))
            .padding(horizontal = 10.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        if (icon != null) Ico(icon, tint = color, size = 14.dp)
        Text(text, style = EqType.caption, color = color)
    }
}

@Composable
internal fun EqSwitch(checked: Boolean, onChange: ((Boolean) -> Unit)?) {
    val c = EqTheme.colors
    Switch(
        checked = checked,
        onCheckedChange = onChange,
        colors = SwitchDefaults.colors(
            checkedThumbColor = c.accentOn,
            checkedTrackColor = c.accent,
            checkedBorderColor = c.accent,
            uncheckedThumbColor = c.text2,
            uncheckedTrackColor = c.surface3,
            uncheckedBorderColor = c.lineStrong,
        ),
    )
}

@Composable
internal fun SwitchRow(
    title: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    detail: String? = null,
) {
    val c = EqTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .toggleable(value = checked, role = Role.Switch, onValueChange = onChange)
            .padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = EqType.bodyStrong, color = c.text)
            if (detail != null) Text(detail, style = EqType.small, color = c.text2)
        }
        EqSwitch(checked, null)
    }
}

@Composable
internal fun RadioRow(
    title: String,
    detail: String?,
    selected: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = EqTheme.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clip(RoundedCornerShape(12.dp))
            .selectable(selected = selected, role = Role.RadioButton, onClick = onClick)
            .padding(vertical = 8.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Box(
            Modifier
                .size(22.dp)
                .border(2.dp, if (selected) c.accent else c.lineStrong, CircleShape)
                .padding(5.dp)
                .background(if (selected) c.accent else Color.Transparent, CircleShape),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(title, style = if (selected) EqType.bodyStrong else EqType.body, color = c.text)
            if (detail != null) Text(detail, style = EqType.small, color = c.text2)
        }
    }
}

/** A row of choices. Pass [tags] to give each one a test tag. */
@Composable
internal fun Segmented(
    options: List<String>,
    selected: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
    icons: List<Int>? = null,
    tags: List<String>? = null,
) {
    val c = EqTheme.colors
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(shape)
            .background(c.surface3)
            .padding(3.dp),
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        options.forEachIndexed { i, label ->
            val on = i == selected
            Row(
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = 44.dp)
                    .clip(RoundedCornerShape(11.dp))
                    .background(if (on) c.surface else Color.Transparent)
                    .then(if (tags != null) Modifier.testTag(tags[i]) else Modifier)
                    .selectable(selected = on, role = Role.RadioButton, onClick = { onSelect(i) })
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally),
            ) {
                if (icons != null) Ico(icons[i], tint = if (on) c.text else c.text2, size = 18.dp)
                Text(
                    label,
                    style = if (on) EqType.bodyStrong else EqType.body,
                    color = if (on) c.text else c.text2,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

@Composable
internal fun fieldColors(): TextFieldColors = EqTheme.colors.let { c ->
    OutlinedTextFieldDefaults.colors(
        focusedTextColor = c.text,
        unfocusedTextColor = c.text,
        focusedContainerColor = c.surface,
        unfocusedContainerColor = c.surface,
        cursorColor = c.accent,
        focusedBorderColor = c.accent,
        unfocusedBorderColor = c.lineStrong,
        focusedPlaceholderColor = c.text3,
        unfocusedPlaceholderColor = c.text3,
        focusedLabelColor = c.accent,
        unfocusedLabelColor = c.text2,
    )
}

@Composable
private fun DialogButton(text: String, onClick: () -> Unit, enabled: Boolean = true, danger: Boolean = false) {
    val c = EqTheme.colors
    TextButton(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = if (danger) c.danger else c.accent),
    ) {
        Text(text, style = EqType.label)
    }
}

@Composable
internal fun EqDialog(
    title: String,
    onDismiss: () -> Unit,
    confirm: @Composable () -> Unit,
    content: @Composable () -> Unit,
) {
    val c = EqTheme.colors
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = c.surface,
        titleContentColor = c.text,
        textContentColor = c.text2,
        title = { Text(title, style = EqType.title) },
        text = content,
        confirmButton = confirm,
        dismissButton = { DialogButton("Cancel", onDismiss) },
    )
}

@Composable
internal fun ConfirmDialog(
    title: String,
    text: String,
    action: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    danger: Boolean = false,
) {
    EqDialog(
        title = title,
        onDismiss = onDismiss,
        confirm = { DialogButton(action, onConfirm, danger = danger) },
    ) {
        Text(text, style = EqType.body)
    }
}

/** One line of text. [onDone] gets the text as typed. */
@Composable
internal fun TextDialog(
    title: String,
    initial: String,
    action: String,
    onDone: (String) -> Unit,
    onDismiss: () -> Unit,
    hint: String? = null,
    allowEmpty: Boolean = false,
) {
    val c = EqTheme.colors
    var value by remember { mutableStateOf(TextFieldValue(initial, TextRange(0, initial.length))) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) {
        try {
            focus.requestFocus()
        } catch (_: IllegalStateException) {
            // The field is not on screen yet; the user can tap it.
        }
    }
    val ready = allowEmpty || value.text.isNotBlank()
    EqDialog(
        title = title,
        onDismiss = onDismiss,
        confirm = { DialogButton(action, { onDone(value.text) }, enabled = ready) },
    ) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = value,
                onValueChange = { value = it },
                modifier = Modifier.fillMaxWidth().focusRequester(focus),
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { if (ready) onDone(value.text) }),
                colors = fieldColors(),
            )
            if (hint != null) Text(hint, style = EqType.small, color = c.text2)
        }
    }
}

/** A horizontal slider. Taps and drags set the value; screen readers can set it too. */
@Composable
internal fun EqSlider(
    value: Float,
    onChange: (Float) -> Unit,
    onFinish: () -> Unit,
    range: ClosedFloatingPointRange<Float>,
    label: String,
    valueText: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    origin: Float? = null,
) {
    val c = EqTheme.colors
    val change by rememberUpdatedState(onChange)
    val finish by rememberUpdatedState(onFinish)
    val bounds by rememberUpdatedState(range)
    var width by remember { mutableIntStateOf(1) }
    val edge = with(LocalDensity.current) { 14.dp.toPx() }
    fun fraction(v: Float) = ((v - bounds.start) / (bounds.endInclusive - bounds.start)).coerceIn(0f, 1f)
    fun set(x: Float) {
        val f = ((x - edge) / (width - 2 * edge).coerceAtLeast(1f)).coerceIn(0f, 1f)
        change(bounds.start + f * (bounds.endInclusive - bounds.start))
    }
    val active = if (enabled) c.accent else c.text3
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(48.dp)
            .onSizeChanged { width = it.width }
            .semantics {
                contentDescription = label
                stateDescription = valueText
                progressBarRangeInfo = ProgressBarRangeInfo(value.coerceIn(bounds.start, bounds.endInclusive), bounds)
                if (enabled) {
                    setProgress { target ->
                        change(target.coerceIn(bounds.start, bounds.endInclusive))
                        finish()
                        true
                    }
                }
            }
            .pointerInput(enabled) {
                if (!enabled) return@pointerInput
                detectTapGestures(onTap = { set(it.x); finish() })
            }
            .pointerInput(enabled) {
                if (!enabled) return@pointerInput
                detectHorizontalDragGestures(
                    onDragStart = { set(it.x) },
                    onDragEnd = { finish() },
                    onDragCancel = { finish() },
                    onHorizontalDrag = { pointer, _ ->
                        set(pointer.position.x)
                        pointer.consume()
                    },
                )
            }
            .drawBehind {
                val w = size.width - 2 * edge
                val y = size.height / 2
                val groove = 4.dp.toPx()
                drawLine(c.surface3, Offset(edge, y), Offset(edge + w, y), groove, StrokeCap.Round)
                val start = edge + w * fraction(origin ?: bounds.start)
                val end = edge + w * fraction(value)
                drawLine(active, Offset(start, y), Offset(end, y), groove, StrokeCap.Round)
                drawCircle(c.surface, 11.dp.toPx(), Offset(end, y))
                drawCircle(active, 11.dp.toPx(), Offset(end, y), style = Stroke(2.dp.toPx()))
            },
    )
}

/** A named number with nudge buttons, a slider and tap to type. */
@Composable
internal fun ParamRow(
    label: String,
    valueText: String,
    onType: () -> Unit,
    onMinus: () -> Unit,
    onPlus: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable (Modifier) -> Unit,
) {
    val c = EqTheme.colors
    Column(modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, style = EqType.small, color = c.text2, modifier = Modifier.weight(1f))
            Text(
                text = valueText,
                style = EqType.number,
                color = c.text,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .alpha(if (enabled) 1f else 0.5f)
                    .heightIn(min = 48.dp)
                    .widthIn(min = 96.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(c.surface2)
                    .border(1.dp, c.line, RoundedCornerShape(12.dp))
                    .semantics { contentDescription = "$label, $valueText. Tap to type a value" }
                    .clickable(enabled = enabled, role = Role.Button, onClick = onType)
                    .padding(horizontal = 14.dp, vertical = 12.dp),
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            EqIconButton(R.drawable.ic_minus, "Lower $label", onMinus, enabled = enabled)
            content(Modifier.weight(1f))
            EqIconButton(R.drawable.ic_plus, "Raise $label", onPlus, enabled = enabled)
        }
    }
}

@Composable
internal fun StatusRow(
    tone: Tone,
    title: String,
    detail: String,
    modifier: Modifier = Modifier,
    action: String? = null,
    onAction: () -> Unit = {},
) {
    val c = EqTheme.colors
    Column(modifier.fillMaxWidth().padding(vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            Ico(toneIcon(tone), tint = c.tone(tone), size = 22.dp, modifier = Modifier.padding(top = 1.dp))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(title, style = EqType.bodyStrong, color = c.text)
                Text(detail, style = EqType.small, color = c.text2)
            }
        }
        if (action != null) {
            EqButton(action, onAction, Modifier.padding(start = 34.dp))
        }
    }
}

@Composable
internal fun EmptyState(title: String, body: String, icon: Int, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    Column(
        modifier = modifier.fillMaxWidth().padding(vertical = 28.dp, horizontal = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Ico(icon, tint = c.text3, size = 28.dp)
        Text(title, style = EqType.lead, color = c.text, textAlign = TextAlign.Center)
        Text(body, style = EqType.small, color = c.text2, textAlign = TextAlign.Center)
    }
}

/** A page that scrolls, with a readable width on big screens. */
@Composable
internal fun ScreenColumn(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = modifier.fillMaxSize().verticalScroll(rememberScrollState()),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(
            modifier = Modifier
                .widthIn(max = MaxContentWidth)
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
            content = content,
        )
    }
}

@Composable
internal fun PageHeader(title: String, modifier: Modifier = Modifier, subtitle: String? = null) {
    val c = EqTheme.colors
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(title, style = EqType.display, color = c.text, modifier = Modifier.semantics { heading() })
        if (subtitle != null) Text(subtitle, style = EqType.body, color = c.text2)
    }
}

/** A page with a back button, for the pages under More. */
@Composable
internal fun SubScreen(
    title: String,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    ScreenColumn(modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            EqIconButton(R.drawable.ic_chevron_left, "Back", onBack)
            Spacer(Modifier.width(4.dp))
            Text(title, style = EqType.title, color = EqTheme.colors.text, modifier = Modifier.semantics { heading() })
        }
        content()
    }
}

@Composable
internal fun Dot(color: Color, modifier: Modifier = Modifier, size: Dp = 8.dp) {
    Box(modifier.size(size).background(color, CircleShape))
}
