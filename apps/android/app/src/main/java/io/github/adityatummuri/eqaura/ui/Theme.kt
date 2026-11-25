package io.github.adityatummuri.eqaura.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.adityatummuri.eqaura.R
import io.github.adityatummuri.eqaura.ThemeChoice
import io.github.adityatummuri.eqaura.Tone

/** The design tokens of EQaura, the same as on the desktop. */
@Immutable
internal class EqColors(
    val dark: Boolean,
    val bg: Color,
    val rail: Color,
    val surface: Color,
    val surface2: Color,
    val surface3: Color,
    val sunken: Color,
    val scrim: Color,
    val line: Color,
    val lineStrong: Color,
    val grid: Color,
    val gridStrong: Color,
    val text: Color,
    val text2: Color,
    val text3: Color,
    val accent: Color,
    val accentPress: Color,
    val accentOn: Color,
    val accentLine: Color,
    val good: Color,
    val warn: Color,
    val danger: Color,
    val busy: Color,
    val onBand: Color,
    val rightEar: Color,
    val bands: List<Color>,
) {
    val accentSoft: Color get() = accent.copy(alpha = if (dark) 0.15f else 0.10f)
}

internal val LightColors = EqColors(
    dark = false,
    bg = Color(0xFFF2F0EB),
    rail = Color(0xFFE9E6DF),
    surface = Color(0xFFFFFFFF),
    surface2 = Color(0xFFF5F3EE),
    surface3 = Color(0xFFE9E6DF),
    sunken = Color(0xFFFBFAF7),
    scrim = Color(0x661B1A17),
    line = Color(0xFFE2DED5),
    lineStrong = Color(0xFFC9C4B8),
    grid = Color(0xFFEEEBE4),
    gridStrong = Color(0xFFD8D4C9),
    text = Color(0xFF1B1A17),
    text2 = Color(0xFF55524A),
    text3 = Color(0xFF65625A),
    accent = Color(0xFFA84A08),
    accentPress = Color(0xFF8E3E05),
    accentOn = Color(0xFFFFFFFF),
    accentLine = Color(0xFFD1600C),
    good = Color(0xFF17693F),
    warn = Color(0xFF7F5D00),
    danger = Color(0xFFAB2A36),
    busy = Color(0xFF1F5CA6),
    onBand = Color(0xFFFFFFFF),
    rightEar = Color(0xFF2B6FCB),
    bands = listOf(
        Color(0xFF12806F), Color(0xFF2B6FCB), Color(0xFF6B4FC4), Color(0xFFBE3F83),
        Color(0xFF387F18), Color(0xFFC03C4F), Color(0xFF0F7690), Color(0xFF7D6812),
    ),
)

internal val DarkColors = EqColors(
    dark = true,
    bg = Color(0xFF131312),
    rail = Color(0xFF0E0E0D),
    surface = Color(0xFF1A1A18),
    surface2 = Color(0xFF232321),
    surface3 = Color(0xFF2E2E2B),
    sunken = Color(0xFF0F0F0E),
    scrim = Color(0xA8000000),
    line = Color(0xFF2A2A27),
    lineStrong = Color(0xFF3E3E3A),
    grid = Color(0xFF1C1C1A),
    gridStrong = Color(0xFF2D2D2A),
    text = Color(0xFFEDEBE5),
    text2 = Color(0xFFABA89F),
    text3 = Color(0xFF9C998F),
    accent = Color(0xFFF0903E),
    accentPress = Color(0xFFDB7F30),
    accentOn = Color(0xFF1C0F03),
    accentLine = Color(0xFFF2994A),
    good = Color(0xFF5FC48E),
    warn = Color(0xFFE3BE45),
    danger = Color(0xFFF5808A),
    busy = Color(0xFF7DB4F2),
    onBand = Color(0xFF0E0E0D),
    rightEar = Color(0xFF8FB8F0),
    bands = listOf(
        Color(0xFF4DBFAE), Color(0xFF69A6F0), Color(0xFFA78DEB), Color(0xFFE889B8),
        Color(0xFF8CCB62), Color(0xFFE67A86), Color(0xFF55C3DD), Color(0xFFCDBA6E),
    ),
)

internal val LocalEqColors = staticCompositionLocalOf { LightColors }

internal object EqTheme {
    val colors: EqColors
        @Composable
        @ReadOnlyComposable
        get() = LocalEqColors.current
}

/** Eight calm hues for bands. Every band also has a number. */
internal fun EqColors.band(i: Int): Color = bands[i.mod(bands.size)]

internal fun EqColors.tone(t: Tone): Color = when (t) {
    Tone.Neutral -> text2
    Tone.Good -> good
    Tone.Warning -> warn
    Tone.Danger -> danger
    Tone.Busy -> busy
}

internal fun EqColors.toneSoft(t: Tone): Color = tone(t).copy(alpha = if (dark) 0.16f else 0.12f)

internal fun toneIcon(t: Tone): Int = when (t) {
    Tone.Neutral -> R.drawable.ic_info
    Tone.Good -> R.drawable.ic_check_circle
    Tone.Warning -> R.drawable.ic_warning
    Tone.Danger -> R.drawable.ic_alert
    Tone.Busy -> R.drawable.ic_refresh
}

/** Text styles. Sizes follow the user's font size setting. */
internal object EqType {
    val display = TextStyle(fontSize = 28.sp, lineHeight = 34.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.3).sp)
    val title = TextStyle(fontSize = 20.sp, lineHeight = 26.sp, fontWeight = FontWeight.Bold, letterSpacing = (-0.2).sp)
    val lead = TextStyle(fontSize = 16.sp, lineHeight = 22.sp, fontWeight = FontWeight.SemiBold)
    val body = TextStyle(fontSize = 15.sp, lineHeight = 21.sp)
    val bodyStrong = TextStyle(fontSize = 15.sp, lineHeight = 21.sp, fontWeight = FontWeight.SemiBold)
    val small = TextStyle(fontSize = 13.sp, lineHeight = 18.sp)
    val smallStrong = TextStyle(fontSize = 13.sp, lineHeight = 18.sp, fontWeight = FontWeight.SemiBold)
    val caption = TextStyle(fontSize = 12.sp, lineHeight = 16.sp, fontWeight = FontWeight.Medium)
    val label = TextStyle(fontSize = 15.sp, lineHeight = 20.sp, fontWeight = FontWeight.SemiBold)
    val number = TextStyle(fontSize = 15.sp, lineHeight = 21.sp, fontWeight = FontWeight.SemiBold, fontFeatureSettings = "tnum")
    val mono = TextStyle(fontSize = 12.sp, lineHeight = 17.sp, fontFamily = FontFamily.Monospace)
}

private val EqShapes = Shapes(
    extraSmall = RoundedCornerShape(6.dp),
    small = RoundedCornerShape(10.dp),
    medium = RoundedCornerShape(14.dp),
    large = RoundedCornerShape(20.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

private fun schemeOf(c: EqColors): ColorScheme = if (c.dark) {
    darkColorScheme(
        primary = c.accent,
        onPrimary = c.accentOn,
        primaryContainer = c.accentSoft,
        onPrimaryContainer = c.accent,
        secondary = c.text2,
        onSecondary = c.bg,
        secondaryContainer = c.surface3,
        onSecondaryContainer = c.text,
        tertiary = c.busy,
        background = c.bg,
        onBackground = c.text,
        surface = c.surface,
        onSurface = c.text,
        surfaceVariant = c.surface2,
        onSurfaceVariant = c.text2,
        outline = c.lineStrong,
        outlineVariant = c.line,
        error = c.danger,
        onError = c.bg,
        scrim = c.scrim,
        surfaceContainerLowest = c.sunken,
        surfaceContainerLow = c.surface,
        surfaceContainer = c.surface,
        surfaceContainerHigh = c.surface2,
        surfaceContainerHighest = c.surface3,
    )
} else {
    lightColorScheme(
        primary = c.accent,
        onPrimary = c.accentOn,
        primaryContainer = c.accentSoft,
        onPrimaryContainer = c.accent,
        secondary = c.text2,
        onSecondary = c.bg,
        secondaryContainer = c.surface3,
        onSecondaryContainer = c.text,
        tertiary = c.busy,
        background = c.bg,
        onBackground = c.text,
        surface = c.surface,
        onSurface = c.text,
        surfaceVariant = c.surface2,
        onSurfaceVariant = c.text2,
        outline = c.lineStrong,
        outlineVariant = c.line,
        error = c.danger,
        onError = c.bg,
        scrim = c.scrim,
        surfaceContainerLowest = c.sunken,
        surfaceContainerLow = c.surface,
        surfaceContainer = c.surface,
        surfaceContainerHigh = c.surface2,
        surfaceContainerHighest = c.surface3,
    )
}

@Composable
internal fun isDark(choice: ThemeChoice): Boolean = when (choice) {
    ThemeChoice.System -> isSystemInDarkTheme()
    ThemeChoice.Light -> false
    ThemeChoice.Dark -> true
}

@Composable
internal fun ProvideEqTheme(dark: Boolean, content: @Composable () -> Unit) {
    val colors = if (dark) DarkColors else LightColors
    val scheme = remember(dark) { schemeOf(colors) }
    CompositionLocalProvider(LocalEqColors provides colors) {
        MaterialTheme(colorScheme = scheme, shapes = EqShapes) {
            CompositionLocalProvider(LocalContentColor provides colors.text) { content() }
        }
    }
}
