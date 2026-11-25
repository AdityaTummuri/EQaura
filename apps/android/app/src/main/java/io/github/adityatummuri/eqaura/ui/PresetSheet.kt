package io.github.adityatummuri.eqaura.ui

import android.content.ContentResolver
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.adityatummuri.eqaura.AppModel
import io.github.adityatummuri.eqaura.CoreError
import io.github.adityatummuri.eqaura.Preset
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.Tone
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private const val MaxImportBytes = 1_048_576

private sealed interface Picked {
    data class Loaded(val name: String, val body: String) : Picked

    data class Problem(val message: String) : Picked
}

private sealed interface SheetDialog {
    data object SaveAs : SheetDialog

    data class Replace(val name: String) : SheetDialog

    data class Rename(val name: String) : SheetDialog

    data class Delete(val name: String) : SheetDialog
}

private fun readImport(resolver: ContentResolver, uri: Uri): Picked = try {
    val shown = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
        if (cursor.moveToFirst()) cursor.getString(0) else null
    }
    val name = (shown ?: "Imported preset").substringBeforeLast('.').ifBlank { "Imported preset" }
    val stream = resolver.openInputStream(uri)
    if (stream == null) {
        Picked.Problem("The file could not be opened")
    } else {
        stream.use { input ->
            val buffer = ByteArray(MaxImportBytes + 1)
            var n = 0
            while (n < buffer.size) {
                val read = input.read(buffer, n, buffer.size - n)
                if (read < 0) break
                n += read
            }
            if (n > MaxImportBytes) {
                Picked.Problem("That file is too large for a preset")
            } else {
                Picked.Loaded(name, buffer.decodeToString(0, n))
            }
        }
    }
} catch (_: IOException) {
    Picked.Problem("The file could not be read")
} catch (_: SecurityException) {
    Picked.Problem("EQaura is not allowed to read that file")
}

private fun writeExport(resolver: ContentResolver, uri: Uri, text: String): Boolean = try {
    resolver.openOutputStream(uri)?.use { it.write(text.encodeToByteArray()) } != null
} catch (_: IOException) {
    false
} catch (_: SecurityException) {
    false
}

private fun fileSafe(name: String): String = name.map { if (it.isLetterOrDigit() || it in " -_") it else '_' }.joinToString("")

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
internal fun PresetSheet(model: AppModel, onDismiss: () -> Unit) {
    val c = EqTheme.colors
    val presets by model.observe { it.presets }
    val current by model.observe { it.preset }
    val dirty by model.observe { it.dirty }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var dialog by remember { mutableStateOf<SheetDialog?>(null) }

    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            scope.launch {
                when (val picked = withContext(Dispatchers.IO) { readImport(context.contentResolver, uri) }) {
                    is Picked.Loaded -> {
                        model.importText(picked.name, picked.body)
                        onDismiss()
                    }
                    is Picked.Problem -> model.toast(picked.message, Tone.Warning)
                }
            }
        }
    }
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/plain")) { uri ->
        if (uri != null) {
            val text = try {
                model.exportText()
            } catch (e: CoreError) {
                model.toast(e.message.orEmpty(), Tone.Danger)
                null
            }
            if (text != null) {
                scope.launch {
                    val ok = withContext(Dispatchers.IO) { writeExport(context.contentResolver, uri, text) }
                    if (ok) model.toast("Exported", Tone.Good) else model.toast("The file could not be written", Tone.Danger)
                }
            }
        }
    }

    val own = presets.filter { !it.builtin }
    val builtin = presets.filter { it.builtin }
    val currentIsOwn = own.any { it.name == current }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = c.bg,
        contentColor = c.text,
        dragHandle = { BottomSheetDefaults.DragHandle(color = c.lineStrong) },
    ) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Presets", style = EqType.title, color = c.text)
                if (current != null && dirty) Badge("edited", tone = Tone.Warning)
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (currentIsOwn) {
                    EqButton(
                        "Save",
                        { current?.let { model.savePreset(it) } },
                        kind = ButtonKind.Primary,
                        icon = R.drawable.ic_save,
                        enabled = dirty,
                    )
                }
                EqButton(
                    "Save as",
                    { dialog = SheetDialog.SaveAs },
                    kind = if (currentIsOwn) ButtonKind.Secondary else ButtonKind.Primary,
                    icon = R.drawable.ic_copy,
                )
                EqButton("Import file", { importer.launch(arrayOf("*/*")) }, icon = R.drawable.ic_import)
                EqButton(
                    "Export",
                    { exporter.launch("${fileSafe(current ?: "EQaura preset")}.txt") },
                    icon = R.drawable.ic_export,
                )
            }
            LazyColumn(Modifier.weight(1f, fill = false), contentPadding = PaddingValues(bottom = 24.dp)) {
                item(key = "head-own") { SectionTitle("Yours", Modifier.padding(top = 8.dp, bottom = 4.dp)) }
                if (own.isEmpty()) {
                    item(key = "own-empty") {
                        Text(
                            "Presets you save appear here. Choose Save as to keep the current sound.",
                            style = EqType.small,
                            color = c.text2,
                            modifier = Modifier.padding(bottom = 8.dp),
                        )
                    }
                }
                items(own, key = { "own:" + it.name }) { p ->
                    PresetRow(
                        preset = p,
                        selected = p.name == current,
                        edited = p.name == current && dirty,
                        onLoad = {
                            model.loadPreset(p.name)
                            onDismiss()
                        },
                        onRename = { dialog = SheetDialog.Rename(p.name) },
                        onDelete = { dialog = SheetDialog.Delete(p.name) },
                    )
                }
                item(key = "head-builtin") { SectionTitle("Built in", Modifier.padding(top = 16.dp, bottom = 4.dp)) }
                items(builtin, key = { "builtin:" + it.name }) { p ->
                    PresetRow(
                        preset = p,
                        selected = p.name == current,
                        edited = p.name == current && dirty,
                        onLoad = {
                            model.loadPreset(p.name)
                            onDismiss()
                        },
                    )
                }
            }
        }
    }

    when (val d = dialog) {
        null -> Unit
        SheetDialog.SaveAs -> TextDialog(
            title = "Save as",
            initial = if (currentIsOwn) current.orEmpty() else "",
            action = "Save",
            hint = "Give this sound a name.",
            onDismiss = { dialog = null },
            onDone = { name ->
                val clean = name.trim()
                if (own.any { it.name == clean } && clean != current) {
                    dialog = SheetDialog.Replace(clean)
                } else if (model.savePreset(clean)) {
                    dialog = null
                    onDismiss()
                }
            },
        )
        is SheetDialog.Replace -> ConfirmDialog(
            title = "Replace ${d.name}?",
            text = "A preset with this name already exists. It will be replaced by the current sound.",
            action = "Replace",
            onConfirm = {
                if (model.savePreset(d.name)) {
                    dialog = null
                    onDismiss()
                }
            },
            onDismiss = { dialog = null },
            danger = true,
        )
        is SheetDialog.Rename -> TextDialog(
            title = "Rename preset",
            initial = d.name,
            action = "Rename",
            onDismiss = { dialog = null },
            onDone = { name -> if (model.renamePreset(d.name, name)) dialog = null },
        )
        is SheetDialog.Delete -> ConfirmDialog(
            title = "Delete ${d.name}?",
            text = "This preset is removed from this phone. Sounds already using it keep their bands.",
            action = "Delete",
            onConfirm = {
                model.deletePreset(d.name)
                dialog = null
            },
            onDismiss = { dialog = null },
            danger = true,
        )
    }
}

@Composable
private fun PresetRow(
    preset: Preset,
    selected: Boolean,
    edited: Boolean,
    onLoad: () -> Unit,
    onRename: (() -> Unit)? = null,
    onDelete: (() -> Unit)? = null,
) {
    val c = EqTheme.colors
    val n = preset.profile.filters.size
    val detail = when (n) {
        0 -> "No changes"
        1 -> "1 band"
        else -> "$n bands"
    }
    val shape = RoundedCornerShape(14.dp)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 56.dp)
            .clip(shape)
            .background(if (selected) c.accentSoft else Color.Transparent)
            .semantics {
                contentDescription = "${preset.name}, $detail${if (selected) ", loaded" else ""}${if (edited) ", edited" else ""}"
            }
            .clickable(role = Role.Button, onClick = onLoad)
            .padding(start = 12.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Ico(R.drawable.ic_presets, tint = if (selected) c.accent else c.text3, size = 20.dp)
        Column(Modifier.weight(1f).padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(preset.name, style = EqType.bodyStrong, color = c.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(
                if (selected) "$detail  ·  loaded${if (edited) ", edited" else ""}" else detail,
                style = EqType.small,
                color = c.text2,
            )
        }
        if (onRename != null) EqIconButton(R.drawable.ic_edit, "Rename ${preset.name}", onRename)
        if (onDelete != null) EqIconButton(R.drawable.ic_trash, "Delete ${preset.name}", onDelete, tint = c.danger)
    }
}
