package com.nexbox.app.ui.theme

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.core.view.WindowCompat
import com.nexbox.app.data.AppearanceStore
import com.nexbox.app.data.ThemeMode

private val DarkColors = darkColorScheme(
    primary = DarkAccent,
    onPrimary = DarkOnAccent,
    secondary = DarkAccent,
    onSecondary = DarkOnAccent,
    background = DarkPageBg,
    onBackground = DarkTextPrimary,
    surface = DarkPanel,
    onSurface = DarkTextPrimary,
    surfaceVariant = DarkPanelHigh,
    onSurfaceVariant = DarkTextSecondary,
    outlineVariant = DarkHairline,
    error = DarkDanger,
    onError = DarkTextPrimary,
)

private val LightColors = lightColorScheme(
    primary = LightAccent,
    onPrimary = LightOnAccent,
    secondary = LightAccent,
    onSecondary = LightOnAccent,
    background = LightPageBg,
    onBackground = LightTextPrimary,
    surface = LightPanel,
    onSurface = LightTextPrimary,
    surfaceVariant = LightPanelHigh,
    onSurfaceVariant = LightTextSecondary,
    outlineVariant = LightHairline,
    error = LightDanger,
    onError = LightOnAccent,
)

/**
 * 应用主题。[mode] 默认取设置页选的「主题模式」，跟随系统时按系统深色走。
 *
 * 主题色同样由设置页选：深色直接用原色，浅色先压暗一档再用；主色上的文字色
 * 按亮度自动配黑或白，所以挑荧光色也不会出现白字压白底。
 */
@Composable
fun NexBoxTheme(mode: ThemeMode = AppearanceStore.themeMode, content: @Composable () -> Unit) {
    val dark = when (mode) {
        ThemeMode.System -> isSystemInDarkTheme()
        ThemeMode.Light -> false
        ThemeMode.Dark -> true
    }
    val accent = parseHexColor(AppearanceStore.accent) ?: DarkAccent

    ApplySystemBarAppearance(dark)

    val colorScheme = if (dark) DarkColors.withAccent(accent) else LightColors.withAccent(accentForLight(accent))

    CompositionLocalProvider(LocalAppPalette provides if (dark) DarkPalette else LightPalette) {
        MaterialTheme(
            colorScheme = colorScheme,
            typography = NexBoxTypography(),
            shapes = Shapes(
                extraSmall = RoundedCornerShape(8.dp),
                small = RoundedCornerShape(12.dp),
                medium = RoundedCornerShape(16.dp),
                large = RoundedCornerShape(20.dp),
                extraLarge = RoundedCornerShape(28.dp),
            ),
            content = content,
        )
    }
}

/** 换主色时连带换它上面的文字色；`secondary` 全应用都当主色用，所以一起改 */
private fun ColorScheme.withAccent(primary: Color): ColorScheme {
    val on = accentContentColor(primary)
    return copy(
        primary = primary,
        onPrimary = on,
        secondary = primary,
        onSecondary = on,
    )
}

/**
 * 系统栏图标配色：浅色主题下必须换成深色图标，否则白色状态栏图标压在白底上完全看不见。
 * edge-to-edge 由 MainActivity 统一开，这里只管图标颜色。
 */
@Composable
private fun ApplySystemBarAppearance(dark: Boolean) {
    val view = LocalView.current
    if (view.isInEditMode) return
    val controller = remember(view) {
        view.context.findActivity()?.window?.let { WindowCompat.getInsetsController(it, view) }
    } ?: return
    SideEffect {
        controller.isAppearanceLightStatusBars = !dark
        controller.isAppearanceLightNavigationBars = !dark
    }
}

private tailrec fun Context.findActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.findActivity()
    else -> null
}
