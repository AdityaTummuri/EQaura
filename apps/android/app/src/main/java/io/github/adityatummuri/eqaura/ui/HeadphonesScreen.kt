package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.CoreError
import io.github.adityatummuri.eqaura.HeadphoneHits
import io.github.adityatummuri.eqaura.HeadphoneRow
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Tone
import java.text.NumberFormat
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

private fun count(n: Int): String = NumberFormat.getIntegerInstance().format(n.toLong())

@Composable
internal fun HeadphonesScreen(model: AppModel, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val correction by model.observe { it.profile.headphone }
    var query by rememberSaveable { mutableStateOf("") }
    var hits by remember { mutableStateOf(HeadphoneHits()) }
    var failed by remember { mutableStateOf(false) }
    var loaded by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current

    LaunchedEffect(query) {
        if (loaded) delay(150)
        try {
            hits = model.searchHeadphones(query)
            failed = false
        } catch (_: CoreError) {
            failed = true
        }
        loaded = true
    }

    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        LazyColumn(
            modifier = Modifier.widthIn(max = MaxContentWidth).fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
        ) {
            item(key = "header") {
                PageHeader(
                    "Headphones",
                    Modifier.padding(bottom = 16.dp),
                    "Pick your model and EQaura corrects its known quirks, so it sounds closer to a neutral reference.",
                )
            }
            item(key = "current") {
                Box(Modifier.padding(bottom = 16.dp)) { CurrentCard(model) }
            }
            item(key = "search") {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.fillMaxWidth().testTag("headphone-search"),
                    singleLine = true,
                    label = { Text("Search headphones") },
                    placeholder = { Text("Brand or model, for example Sennheiser HD 600") },
                    leadingIcon = { Ico(R.drawable.ic_search, tint = c.text2, size = 20.dp) },
                    trailingIcon = {
                        if (query.isNotEmpty()) {
                            EqIconButton(R.drawable.ic_close, "Clear the search", { query = "" })
                        }
                    },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { focus.clearFocus() }),
                    shape = RoundedCornerShape(16.dp),
                    colors = fieldColors(),
                )
            }
            item(key = "count") {
                val text = when {
                    !loaded || failed || hits.total == 0 -> ""
                    query.isBlank() -> "${count(hits.total)} headphones"
                    hits.rows.size < hits.total ->
                        "Showing the first ${count(hits.rows.size)} of ${count(hits.total)} matches. Type more to narrow it."
                    else -> "${count(hits.total)} ${if (hits.total == 1) "match" else "matches"}"
                }
                Text(
                    text,
                    style = EqType.small,
                    color = c.text2,
                    modifier = Modifier.padding(top = 10.dp, bottom = 8.dp, start = 4.dp).heightIn(min = 18.dp),
                )
            }
            if (failed) {
                item(key = "failed") {
                    EmptyState(
                        "The headphone list is not available yet",
                        "You can still use the equalizer as normal. Try restarting the app.",
                        R.drawable.ic_headphones,
                    )
                }
            } else if (loaded && hits.rows.isEmpty()) {
                item(key = "none") {
                    EmptyState(
                        "No headphones match “$query”",
                        "Check the spelling, or try just the brand or a shorter part of the model name.",
                        R.drawable.ic_search,
                    )
                }
            }
            itemsIndexed(hits.rows, key = { _, row -> row.index }) { position, row ->
                HitRow(
                    row = row,
                    inUse = correction?.name == row.name,
                    onClick = { model.pickHeadphone(row.index) },
                    modifier = Modifier.testTag("headphone-row-$position").padding(bottom = 6.dp),
                )
            }
        }
    }
}

@Composable
private fun HitRow(row: HeadphoneRow, inUse: Boolean, onClick: () -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = modifier
            .fillMaxWidth()
            .heightIn(min = 64.dp)
            .clip(shape)
            .background(if (inUse) c.accentSoft else c.surface)
            .border(1.dp, if (inUse) c.accent.copy(alpha = 0.5f) else c.line, shape)
            .semantics { contentDescription = "${row.name}, ${row.form}, ${row.source}${if (inUse) ", in use" else ""}" }
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(row.name, style = EqType.bodyStrong, color = c.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text("${row.source}  ·  ${row.form}", style = EqType.small, color = c.text2, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (inUse) Badge("In use", tone = Tone.Good, icon = R.drawable.ic_check)
    }
}

@Composable
private fun CurrentCard(model: AppModel) {
    val c = EqTheme.colors
    val correction by model.observe { it.profile.headphone }
    val current = correction
    EqCard {
        if (current == null) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Ico(R.drawable.ic_info, tint = c.text2, size = 22.dp)
                Text(
                    "No headphones selected. Choose a model below to apply its correction.",
                    style = EqType.body,
                    color = c.text2,
                    modifier = Modifier.weight(1f),
                )
            }
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Ico(R.drawable.ic_headphones, tint = c.accent, size = 24.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(current.name, style = EqType.lead, color = c.text)
                        if (current.source.isNotEmpty()) Text(current.source, style = EqType.small, color = c.text2)
                    }
                }
                SwitchRow(
                    title = "Correction",
                    checked = current.on,
                    onChange = { model.setHeadphoneOn(it) },
                    detail = if (current.on) "Correction is on" else "Correction is off",
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Strength", style = EqType.small, color = c.text2, modifier = Modifier.weight(1f))
                    Text("${(current.strength * 100).roundToInt()}%", style = EqType.number, color = c.text)
                }
                EqSlider(
                    value = current.strength.toFloat(),
                    onChange = { model.setHeadphoneStrength(it) },
                    onFinish = { model.seal() },
                    range = 0f..1f,
                    label = "Correction strength",
                    valueText = "${(current.strength * 100).roundToInt()} percent",
                    enabled = current.on,
                )
                EqButton("Remove", { model.removeHeadphone() }, kind = ButtonKind.Danger, icon = R.drawable.ic_close)
            }
        }
    }
}
