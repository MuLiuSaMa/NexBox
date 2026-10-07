package com.nexbox.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.toArgb

/*
 * 调色板分两层：
 * 1. 下面这批 `private/internal` 常量是**原始色**，只给 [Theme] 组 colorScheme 用；
 * 2. 界面里读的是同名语义色（[TextSecondary]、[Hairline] 等），它们是从 colorScheme
 *    取值的 `@Composable` 属性 —— 浅色模式下自动换成墨色系，调用点不用改。
 *
 * 之所以保留语义色这一层，是因为全站有七十多处直接写 `color = TextSecondary`：
 * 让名字跟着主题走，比把每个调用点展开成 `MaterialTheme.colorScheme.xxx` 好维护得多。
 */

// ───────── 深色（原配色，保持逐字一致） ─────────
internal val DarkPageBgTop = Color(0xFF14141F)
internal val DarkPageBg = Color(0xFF0A0A10)
internal val DarkPanel = Color(0xFF20202F)
internal val DarkPanelHigh = Color(0xFF2B2B40)
internal val DarkTextPrimary = Color(0xFFFFFFFF)
internal val DarkTextSecondary = Color(0xB3FFFFFF)
internal val DarkHairline = Color(0x1FFFFFFF)
internal val DarkAccent = Color(0xFF4C8DFF)
internal val DarkOnAccent = Color(0xFF04182F)
internal val DarkDanger = Color(0xFFCF6679)
internal val DarkMetricGpu = Color(0xFF45D0E6)
internal val DarkMetricMemory = Color(0xFF9B8CFF)
internal val DarkMetricStorage = Color(0xFFFFB454)

// ───────── 浅色 ─────────
// 页面要比卡片深一档：白卡压在近白页面上，关着玻璃看不出边界，开着玻璃更像「全透明」
internal val LightPageBgTop = Color(0xFFF7F8FC)
internal val LightPageBg = Color(0xFFEBEDF5)
internal val LightPanel = Color(0xFFFFFFFF)
internal val LightPanelHigh = Color(0xFFE7E9F2)
internal val LightTextPrimary = Color(0xFF14151C)
internal val LightTextSecondary = Color(0x9940434E)
internal val LightHairline = Color(0x14000000)
// 品牌蓝要压深：0xFF4C8DFF 在白底上只有约 2.6:1，小字直接糊掉
internal val LightAccent = Color(0xFF1F63D2)
internal val LightOnAccent = Color(0xFFFFFFFF)
internal val LightDanger = Color(0xFFB3261E)
internal val LightMetricGpu = Color(0xFF0C7C8E)
internal val LightMetricMemory = Color(0xFF5B4BD6)
internal val LightMetricStorage = Color(0xFFA96A00)

/** 纯黑：开屏页与背景图遮罩用，不随主题变 */
val Black = Color(0xFF000000)

// ───────── 玻璃层：刻意不随主题换色 ─────────
// Haze 的折射与模糊吃的是卡片背后的真实内容，深浅色下用同一套参数才是同一种「透」。
// 给浅色配高 alpha 的白 tint 等于在背景上盖一块白底板，那就不是玻璃了。
val GlassTint = Color.White.copy(alpha = 0.05f)
val GlassBorder = Color.White.copy(alpha = 0.14f)
val NavTint = Color.White.copy(alpha = 0.04f)
val NavBorder = Color.White.copy(alpha = 0.16f)
val NavSolidBorder = Color.White.copy(alpha = 0.08f)

/**
 * 主题相关的「非 colorScheme」表面色：页面渐变、背景图遮罩与弱填充面。
 * 这些没有 Material3 的槽位可放，所以单独一份，由 [NexBoxTheme] 按模式提供。
 */
@Immutable
data class AppPalette(
    val isDark: Boolean,
    /** 页面渐变顶端 */
    val pageTop: Color,
    /** 页面渐变底端 */
    val pageBottom: Color,
    /** 自定义背景图上的一层薄遮罩：保证文字压上去还读得清 */
    val imageScrim: Color,
    /** 弱填充：进度条底槽、图表底这类「比卡片再深/浅一档」的面 */
    val fill: Color,
)

internal val DarkPalette = AppPalette(
    isDark = true,
    pageTop = DarkPageBgTop,
    pageBottom = DarkPageBg,
    imageScrim = Black.copy(alpha = 0.18f),
    fill = Color.White.copy(alpha = 0.06f),
)

internal val LightPalette = AppPalette(
    isDark = false,
    pageTop = LightPageBgTop,
    pageBottom = LightPageBg,
    // 浅色下用白雾压一层，深色背景图才不会把黑字吃掉
    imageScrim = Color.White.copy(alpha = 0.52f),
    fill = Black.copy(alpha = 0.06f),
)

val LocalAppPalette = staticCompositionLocalOf { DarkPalette }

/** 当前是不是深色模式。要区分深浅色时才用它，普通颜色请直接用语义色 */
@Composable
fun isDarkTheme(): Boolean = LocalAppPalette.current.isDark

// ───────── 语义色：跟随主题模式 ─────────

/** 主文字 */
val TextPrimary: Color
    @Composable get() = MaterialTheme.colorScheme.onBackground

/** 次要文字（说明、标签、数值单位） */
val TextSecondary: Color
    @Composable get() = MaterialTheme.colorScheme.onSurfaceVariant

/** 分隔线与细描边 */
val Hairline: Color
    @Composable get() = MaterialTheme.colorScheme.outlineVariant

/** 页面渐变的两端 */
val PageBg: Color
    @Composable get() = LocalAppPalette.current.pageBottom

val PageBgTop: Color
    @Composable get() = LocalAppPalette.current.pageTop

/** 弱填充面：进度条底槽、趋势图底 */
val FillColor: Color
    @Composable get() = LocalAppPalette.current.fill

/** 品牌蓝：等价于 colorScheme.primary，写界面时用这个更短 */
val Accent: Color
    @Composable get() = MaterialTheme.colorScheme.primary

/** 错误 / 断连提示色 */
val Danger: Color
    @Composable get() = if (isDarkTheme()) DarkDanger else LightDanger

// ───────── 硬件指标配色 ─────────
// 主题仍是蓝；其余指标走同冷色系做区分，存储用暖色收尾，避免四张卡糊成一片
val MetricCpu: Color
    @Composable get() = Accent

val MetricGpu: Color
    @Composable get() = if (isDarkTheme()) DarkMetricGpu else LightMetricGpu

val MetricMemory: Color
    @Composable get() = if (isDarkTheme()) DarkMetricMemory else LightMetricMemory

val MetricStorage: Color
    @Composable get() = if (isDarkTheme()) DarkMetricStorage else LightMetricStorage

/** 温度偏高（尚未危险）时的提示色 */
val TempWarn: Color
    @Composable get() = MetricStorage

// ───────── 主题色（用户可改） ─────────

/**
 * 预设色板：橙黄绿青蓝紫各一个，排成一行的量。
 * 都挑的中亮度、中饱和的色：深色模式下能直接用，浅色模式下 [accentForLight] 再压一档，
 * 两种模式都不会出现「白字压白底」。
 */
val AccentPresets: List<String> = listOf(
    "#F97316", "#EAB308", "#22C55E", "#06B6D4", "#4C8DFF", "#8B5CF6",
)

/**
 * `#RRGGBB` → Color。非法值返回 null，交给调用方兜默认色。
 *
 * 必须走 [Color] 的 Int/Long 工厂函数：`Color(ULong)` 是原始打包构造器，低 6 位是色彩
 * 空间索引，把 ARGB 直接 or 成 ULong 会被读成 index 63，随后任何 luminance/convert 调用
 * 都抛 ArrayIndexOutOfBoundsException。
 */
fun parseHexColor(hex: String): Color? {
    if (hex.length != 7 || !hex.startsWith('#')) return null
    val rgb = hex.substring(1).toIntOrNull(16) ?: return null
    return Color(0xFF000000.toInt() or rgb)
}

/** Color → `#RRGGBB`，用于回显与持久化 */
fun Color.toHexColorString(): String = "#%06X".format(toArgb() and 0xFFFFFF)

/**
 * 浅色模式下的主题色：饱和度抬到 0.35、明度压到 0.72。
 * 用户挑的多半是为深色背景准备的亮色，直接放到白底上小字对比度不够（品牌蓝实测只有 2.6:1）；
 * 压到这一档后约 5.5:1，白底上的小字才立得住。
 */
fun accentForLight(base: Color): Color {
    val hsv = FloatArray(3)
    android.graphics.Color.colorToHSV(base.toArgb(), hsv)
    hsv[1] = kotlin.math.max(hsv[1], 0.35f)
    hsv[2] = kotlin.math.min(hsv[2], 0.72f)
    return Color(android.graphics.Color.HSVToColor(hsv))
}

/** 主题色上面压的文字色：亮色配近黑，暗色配白 */
fun accentContentColor(base: Color): Color =
    if (base.luminance() > 0.55f) Color(0xFF04182F) else Color.White
