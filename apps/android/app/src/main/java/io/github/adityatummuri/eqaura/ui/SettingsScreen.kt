package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AiChoice
import io.github.adityatummuri.eqaura.AiSettings
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.Coverage
import io.github.adityatummuri.eqaura.Detail
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.ThemeChoice
import io.github.adityatummuri.eqaura.Tone

@Composable
internal fun SettingsScreen(model: AppModel, onBack: () -> Unit, modifier: Modifier = Modifier) {
    SubScreen("Settings", onBack, modifier) {
        AppearanceCard(model)
        StartupCard(model)
        CoverageCard(model)
        DetailCard(model)
        AssistantCard(model)
        PrivacyCard(model)
        OutputsCard(model)
    }
}

@Composable
private fun SettingsCard(title: String, help: String? = null, content: @Composable ColumnScope.() -> Unit) {
    val c = EqTheme.colors
    EqCard {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = EqType.lead, color = c.text)
            if (help != null) Text(help, style = EqType.small, color = c.text2)
            content()
        }
    }
}

@Composable
private fun AppearanceCard(model: AppModel) {
    val theme by model.observe { it.settings.theme }
    SettingsCard("Appearance", "Match your phone, or always use a light or dark look.") {
        Segmented(
            options = ThemeChoice.entries.map { it.label },
            selected = ThemeChoice.entries.indexOf(theme),
            onSelect = { model.setTheme(ThemeChoice.entries[it]) },
        )
    }
}

@Composable
private fun StartupCard(model: AppModel) {
    val atBoot by model.observe { it.settings.startAtBoot }
    SettingsCard("Startup") {
        SwitchRow(
            title = "Start after restart",
            checked = atBoot,
            onChange = { model.setStartAtBoot(it) },
            detail = "The equalizer comes back by itself when the phone has started, if it was on.",
        )
    }
}

@Composable
private fun CoverageCard(model: AppModel) {
    val coverage by model.observe { it.settings.coverage }
    SettingsCard("Coverage", "Which sound the equalizer is attached to. If one choice does nothing on your phone, try the other.") {
        Coverage.entries.forEach {
            RadioRow(it.label, it.detail, it == coverage, { model.setCoverage(it) })
        }
    }
}

@Composable
private fun DetailCard(model: AppModel) {
    val detail by model.observe { it.settings.detail }
    SettingsCard("Detail", "How finely the curve is followed, against the delay it adds.") {
        Detail.entries.forEach {
            RadioRow(it.label, it.detail, it == detail, { model.setDetail(it) })
        }
    }
}

private fun providerDetail(choice: AiChoice): String = when (choice) {
    AiChoice.Rules -> "Works offline and never sends anything."
    AiChoice.Anthropic -> "Claude models. Needs your own key."
    AiChoice.OpenAi -> "OpenAI or any service with the same interface. Needs your own key."
    AiChoice.Ollama -> "A model that you run on your own computer. No key needed."
}

@Composable
private fun AssistantCard(model: AppModel) {
    val c = EqTheme.colors
    val choice by model.observe { it.settings.ai }
    SettingsCard(
        "Assistant",
        "Choose who turns your words into suggestions. Nothing changes until you apply a suggestion.",
    ) {
        AiChoice.entries.forEach {
            RadioRow(it.label, providerDetail(it), it == choice, { model.setAi(it) })
        }
        if (choice != AiChoice.Rules) {
            Divider(Modifier.padding(vertical = 4.dp))
            ProviderFields(model, choice)
        }
    }
    if (choice == AiChoice.Rules) {
        Text(
            "Built-in rules understand words such as bass, vocals, harsh, muddy, warm and bright.",
            style = EqType.small,
            color = c.text2,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}

@Composable
private fun ProviderFields(model: AppModel, choice: AiChoice) {
    val c = EqTheme.colors
    val all by model.observe { it.settings.aiSettings }
    val stored = all[choice.id] ?: AiSettings()
    val ai by model.observe { it.ai }
    var url by remember(choice) { mutableStateOf(stored.url) }
    var name by remember(choice) { mutableStateOf(stored.model) }
    var key by remember(choice) { mutableStateOf("") }
    var menu by remember { mutableStateOf(false) }
    LaunchedEffect(stored.url) { if (stored.url != url.trim()) url = stored.url }
    LaunchedEffect(stored.model) { if (stored.model != name.trim()) name = stored.model }

    OutlinedTextField(
        value = url,
        onValueChange = {
            url = it
            model.setAiAddress(it)
        },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        label = { Text("Address") },
        placeholder = { Text(choice.defaultUrl) },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
        colors = fieldColors(),
        shape = RoundedCornerShape(14.dp),
    )
    Text(
        if (choice == AiChoice.Ollama) {
            "On a phone, use the address of the computer that runs Ollama, for example http://192.168.1.20:11434/v1"
        } else {
            "Leave it empty to use ${choice.defaultUrl}."
        },
        style = EqType.small,
        color = c.text2,
    )
    OutlinedTextField(
        value = name,
        onValueChange = {
            name = it
            model.setAiModel(it)
        },
        modifier = Modifier.fillMaxWidth(),
        singleLine = true,
        label = { Text("Model") },
        placeholder = { Text("Type a model name") },
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next),
        colors = fieldColors(),
        shape = RoundedCornerShape(14.dp),
    )
    if (ai.models.isNotEmpty()) {
        Box {
            val shape = RoundedCornerShape(12.dp)
            Row(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = 48.dp)
                    .clip(shape)
                    .background(c.surface2)
                    .border(1.dp, c.line, shape)
                    .semantics { contentDescription = "Pick a model from the list of ${ai.models.size}" }
                    .clickable(role = Role.DropdownList) { menu = true }
                    .padding(horizontal = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Pick from ${ai.models.size} models", style = EqType.body, color = c.text, modifier = Modifier.weight(1f))
                Ico(R.drawable.ic_chevron_down, tint = c.text2, size = 20.dp)
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }, containerColor = c.surface) {
                ai.models.forEach { option ->
                    DropdownMenuItem(
                        text = { Text(option, style = EqType.body, color = c.text) },
                        onClick = {
                            menu = false
                            name = option
                            model.setAiModel(option)
                        },
                    )
                }
            }
        }
    }
    if (choice.needsKey) {
        if (ai.hasKey) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Badge("Key saved", tone = Tone.Good, icon = R.drawable.ic_key)
                Box(Modifier.weight(1f))
                EqButton("Remove", { model.setAiKey("") }, kind = ButtonKind.Danger)
            }
        }
        OutlinedTextField(
            value = key,
            onValueChange = { key = it },
            modifier = Modifier.fillMaxWidth(),
            singleLine = true,
            label = { Text(if (ai.hasKey) "Replace the key" else "API key") },
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = {
                if (key.isNotBlank()) {
                    model.setAiKey(key)
                    key = ""
                }
            }),
            colors = fieldColors(),
            shape = RoundedCornerShape(14.dp),
        )
        EqButton(
            "Save key",
            {
                model.setAiKey(key)
                key = ""
            },
            enabled = key.isNotBlank(),
            icon = R.drawable.ic_lock,
        )
        Text(
            "The key is stored encrypted with the Android Keystore. It is never shown again.",
            style = EqType.small,
            color = c.text2,
        )
    }
    EqButton("Check connection", { model.loadModels() }, icon = R.drawable.ic_refresh)
    if (ai.status.isNotEmpty()) {
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Ico(toneIcon(ai.tone), tint = c.tone(ai.tone), size = 20.dp)
            Text(ai.status, style = EqType.small, color = c.tone(ai.tone), modifier = Modifier.weight(1f))
        }
    }
}

@Composable
private fun PrivacyCard(model: AppModel) {
    val send by model.observe { it.settings.sendHearing }
    SettingsCard("Privacy") {
        SwitchRow(
            title = "Share hearing results with the assistant",
            checked = send,
            onChange = { model.setSendHearing(it) },
            detail = "Off by default. When on, the assistant sees your hearing result so it can make better suggestions. Audio never leaves this phone.",
        )
    }
}

@Composable
private fun OutputsCard(model: AppModel) {
    val c = EqTheme.colors
    val devices by model.observe { it.devices }
    val output by model.observe { it.output }
    var forgetting by remember { mutableStateOf<String?>(null) }
    val rows = devices.entries.sortedBy { it.value.name.lowercase() }.sortedBy { it.key != output.id }
    SettingsCard("Remembered outputs", "Each output keeps its own sound. Switching headphones switches the sound.") {
        if (rows.isEmpty()) {
            Text("Outputs you use are remembered here.", style = EqType.small, color = c.text2)
        }
        rows.forEach { (id, device) ->
            val bands = device.profile.filters.size
            val detail = (if (bands == 1) "1 band" else "$bands bands") + (device.preset?.let { "  ·  $it" } ?: "")
            Row(
                Modifier.fillMaxWidth().heightIn(min = 56.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(device.name, style = EqType.bodyStrong, color = c.text)
                    Text(detail, style = EqType.small, color = c.text2)
                }
                if (id == output.id) {
                    Badge("In use", tone = Tone.Good)
                } else {
                    EqButton("Forget", { forgetting = id }, kind = ButtonKind.Ghost)
                }
            }
        }
    }
    forgetting?.let { id ->
        val name = devices[id]?.name.orEmpty()
        ConfirmDialog(
            title = "Forget $name?",
            text = "Its sound is removed from this phone. The next time you use it, it starts flat.",
            action = "Forget",
            onConfirm = {
                model.forgetOutput(id)
                forgetting = null
            },
            onDismiss = { forgetting = null },
            danger = true,
        )
    }
}
