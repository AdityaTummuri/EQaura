package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AiChoice
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.ChatRow
import io.github.adityatummuri.eqaura.ProposalState
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Tone

private val Examples = listOf("More bass", "Vocals sound far away", "Less harsh", "Make it warmer")

private const val HeaderItems = 2

@Composable
internal fun AssistantScreen(model: AppModel, onOpenSettings: () -> Unit, modifier: Modifier = Modifier) {
    val c = EqTheme.colors
    val chat by model.observe { it.chat }
    val provider by model.observe { it.settings.ai }
    val busy = chat.any { it.tone == Tone.Busy }
    var text by rememberSaveable { mutableStateOf("") }
    val listState = rememberLazyListState()
    LaunchedEffect(chat.size) {
        if (chat.isNotEmpty()) listState.animateScrollToItem(HeaderItems + chat.size - 1)
    }

    fun submit() {
        if (text.isNotBlank() && !busy) {
            model.send(text)
            text = ""
        }
    }

    Box(modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(Modifier.widthIn(max = MaxContentWidth).fillMaxSize()) {
            LazyColumn(
                state = listState,
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                item(key = "header") {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text("Assistant", style = EqType.display, color = c.text, modifier = Modifier.weight(1f))
                            EqButton(
                                "Clear chat",
                                { model.clearChat() },
                                kind = ButtonKind.Ghost,
                                icon = R.drawable.ic_trash,
                                enabled = chat.isNotEmpty() && !busy,
                            )
                        }
                        Text(
                            "Describe the sound you want in plain English. It suggests changes, and you decide.",
                            style = EqType.body,
                            color = c.text2,
                        )
                    }
                }
                item(key = "sends") { SendsPanel(model, provider, onOpenSettings) }
                if (chat.isEmpty()) {
                    item(key = "empty") { EmptyChat { if (!busy) model.send(it) } }
                }
                items(chat, key = { it.id }) { row ->
                    ChatItem(row, onApply = { model.applyProposal(row.id) }, onStop = { model.stopAsking() })
                }
            }
            Row(
                Modifier.fillMaxWidth().background(c.bg).padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    modifier = Modifier.weight(1f).testTag("assistant-input"),
                    placeholder = { Text(if (busy) "Waiting for a reply" else "Describe what you want to change") },
                    maxLines = 4,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Send),
                    keyboardActions = KeyboardActions(onSend = { submit() }),
                    shape = RoundedCornerShape(24.dp),
                    colors = fieldColors(),
                )
                val ready = text.isNotBlank() && !busy
                EqIconButton(
                    icon = R.drawable.ic_send,
                    description = "Send",
                    onClick = { submit() },
                    modifier = Modifier.padding(bottom = 4.dp).testTag("assistant-send"),
                    enabled = ready,
                    tint = c.accentOn,
                    background = c.accent,
                )
            }
        }
    }
}

@Composable
private fun SendsPanel(model: AppModel, provider: AiChoice, onOpenSettings: () -> Unit) {
    val c = EqTheme.colors
    var open by rememberSaveable { mutableStateOf(false) }
    val sends by model.observe { it.ai.sends }
    LaunchedEffect(open) { if (open) model.refreshSends() }
    val local = provider == AiChoice.Rules
    EqCard(padding = 0.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .semantics { stateDescription = if (open) "Expanded" else "Collapsed" }
                .clickable(role = Role.Button, onClickLabel = if (open) "Hide what gets sent" else "Show what gets sent") { open = !open }
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Ico(R.drawable.ic_eye, tint = c.text2, size = 20.dp)
            Column(Modifier.weight(1f)) {
                Text("What gets sent", style = EqType.bodyStrong, color = c.text)
                Text(
                    if (local) "Nothing leaves this phone" else "Open to see exactly what is sent",
                    style = EqType.small,
                    color = c.text2,
                )
            }
            Ico(if (open) R.drawable.ic_chevron_up else R.drawable.ic_chevron_down, tint = c.text2, size = 20.dp)
        }
        if (open) {
            Divider()
            SelectionContainer {
                Text(
                    sends.ifEmpty { "Nothing is sent. The built-in rules run on this phone." },
                    style = EqType.mono,
                    color = c.text,
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                )
            }
        }
        Divider()
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Ico(R.drawable.ic_sparkle, tint = c.accent, size = 20.dp)
            Text(
                if (local) "Built-in rules" else provider.label,
                style = EqType.small,
                color = c.text,
                modifier = Modifier.weight(1f).padding(vertical = 8.dp),
            )
            EqButton("Change", onOpenSettings, kind = ButtonKind.Ghost)
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EmptyChat(onExample: (String) -> Unit) {
    val c = EqTheme.colors
    Column(
        Modifier.fillMaxWidth().padding(top = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Ask for a change in your own words", style = EqType.lead, color = c.text, textAlign = TextAlign.Center)
        Text(
            "For example: “Vocals sound harsh” or “I want more bass for movies”. " +
                "You will see exactly what would change before anything is applied.",
            style = EqType.small,
            color = c.text2,
            textAlign = TextAlign.Center,
        )
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Examples.forEach { example ->
                Text(
                    example,
                    style = EqType.body,
                    color = c.text,
                    modifier = Modifier
                        .heightIn(min = 48.dp)
                        .clip(CircleShape)
                        .background(c.surface)
                        .border(1.dp, c.lineStrong, CircleShape)
                        .clickable(role = Role.Button) { onExample(example) }
                        .padding(horizontal = 18.dp, vertical = 12.dp),
                )
            }
        }
    }
}

@Composable
private fun ChatItem(row: ChatRow, onApply: () -> Unit, onStop: () -> Unit) {
    val c = EqTheme.colors
    when {
        row.fromUser -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
            Text(
                row.text,
                style = EqType.body,
                color = c.text,
                modifier = Modifier
                    .widthIn(max = 480.dp)
                    .clip(RoundedCornerShape(20.dp, 20.dp, 6.dp, 20.dp))
                    .background(c.accentSoft)
                    .padding(horizontal = 16.dp, vertical = 12.dp),
            )
        }
        row.tone == Tone.Busy -> Row(
            Modifier
                .fillMaxWidth()
                .semantics { liveRegion = LiveRegionMode.Polite },
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CircularProgressIndicator(Modifier.size(22.dp), color = c.accent, strokeWidth = 2.5.dp)
            Text(
                "Thinking about your request. Nothing changes until you apply a suggestion.",
                style = EqType.small,
                color = c.text2,
                modifier = Modifier.weight(1f),
            )
            EqButton("Stop", onStop, kind = ButtonKind.Ghost, icon = R.drawable.ic_stop)
        }
        row.tone == Tone.Danger -> Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(c.toneSoft(Tone.Danger))
                .padding(14.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Ico(R.drawable.ic_alert, tint = c.danger, size = 20.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Something went wrong", style = EqType.smallStrong, color = c.danger)
                Text(row.text, style = EqType.small, color = c.text)
            }
        }
        else -> Reply(row, onApply)
    }
}

@Composable
private fun Reply(row: ChatRow, onApply: () -> Unit) {
    val c = EqTheme.colors
    val lines = row.text.lines()
    val shape = RoundedCornerShape(6.dp, 20.dp, 20.dp, 20.dp)
    Column(
        Modifier
            .widthIn(max = 520.dp)
            .clip(shape)
            .background(c.surface)
            .border(1.dp, c.line, shape)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(lines.firstOrNull().orEmpty(), style = EqType.body, color = c.text)
        lines.drop(1).filter { it.isNotBlank() }.forEach { Text(it, style = EqType.small, color = c.text2) }
        if (row.lines.isNotEmpty()) {
            Divider()
            SectionTitle("Proposed changes")
            row.lines.forEach { change ->
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.padding(top = 8.dp).size(6.dp).background(c.accent, CircleShape))
                    Text(change, style = EqType.body, color = c.text, modifier = Modifier.weight(1f))
                }
            }
        }
        when (row.proposal) {
            ProposalState.None -> Unit
            ProposalState.Open -> EqButton(
                "Apply",
                onApply,
                Modifier.testTag("proposal-apply"),
                kind = ButtonKind.Primary,
                icon = R.drawable.ic_check,
            )
            ProposalState.Applied -> Badge("Applied", tone = Tone.Good, icon = R.drawable.ic_check)
            ProposalState.Undone -> EqButton(
                "Undone — Apply again",
                onApply,
                Modifier.testTag("proposal-apply"),
                kind = ButtonKind.Primary,
                icon = R.drawable.ic_redo,
            )
        }
        if (row.origin.isNotEmpty()) Text(row.origin, style = EqType.caption, color = c.text3)
    }
}
