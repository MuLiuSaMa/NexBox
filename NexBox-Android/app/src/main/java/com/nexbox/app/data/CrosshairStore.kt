package com.nexbox.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/** 准心样式常量。全部存字符串，新增样式旧值不会崩 */
object CrosshairStyles {
    const val DOT = "dot"
    const val CROSS = "cross"
    const val CROSS_DOT = "crossdot"
    const val CIRCLE = "circle"

    /** 全部样式，顺序即设置页展示顺序 */
    val ALL = listOf(DOT, CROSS, CROSS_DOT, CIRCLE)

    fun label(style: String): String = when (style) {
        DOT -> "圆点"
        CROSS -> "十字"
        CROSS_DOT -> "十字带点"
        CIRCLE -> "圆圈"
        else -> style
    }
}

/** 经典准星六色（绿/红/白/青/黄/品红），设置页圆点选择用 */
val CROSSHAIR_COLORS = listOf("#22C55E", "#EF4444", "#FFFFFF", "#22D3EE", "#EAB308", "#EC4899")

/** 保存为预设：一份完整的准心外观快照。不含位置——准心恒为屏幕居中 */
@Serializable
data class CrosshairPreset(
    val name: String,
    val style: String,
    val colorHex: String,
    val sizeDp: Int,
    val thicknessDp: Int,
    val opacityPercent: Int,
)

/**
 * 辅助准心设置：样式、颜色、大小、粗细、透明度与用户保存的预设，SharedPreferences 持久化。
 *
 * 全部是 Compose 状态：设置页改动即时反映到悬浮窗口（服务里的 Compose 直接读这些字段）。
 * 服务被系统回收时不改写这里的开关——由工具页/设置页的「heal」逻辑在回到页面时补拉起服务。
 */
object CrosshairStore {
    private const val PREFS_NAME = "nexbox_crosshair"
    private const val KEY_ENABLED = "enabled"
    private const val KEY_STYLE = "style"
    private const val KEY_COLOR = "color"
    private const val KEY_SIZE = "size"
    private const val KEY_THICKNESS = "thickness"
    private const val KEY_OPACITY = "opacity"
    private const val KEY_PRESETS = "presets"

    const val SIZE_MIN = 2
    const val SIZE_MAX = 20
    const val THICKNESS_MIN = 1
    const val THICKNESS_MAX = 8
    const val OPACITY_MIN = 20
    const val OPACITY_MAX = 100

    var enabled by mutableStateOf(false)
        private set

    var style by mutableStateOf(CrosshairStyles.CROSS_DOT)
        private set

    var colorHex by mutableStateOf("#22C55E")
        private set

    var sizeDp by mutableStateOf(18)
        private set

    var thicknessDp by mutableStateOf(2)
        private set

    var opacityPercent by mutableStateOf(100)
        private set

    /** 用户保存的预设，按保存顺序排列；同名保存即覆盖 */
    var presets by mutableStateOf<List<CrosshairPreset>>(emptyList())
        private set

    private val json = Json { ignoreUnknownKeys = true }

    fun init(context: Context) {
        val p = prefs(context)
        enabled = p.getBoolean(KEY_ENABLED, false)
        style = p.getString(KEY_STYLE, null)?.takeIf { it in CrosshairStyles.ALL }
            ?: CrosshairStyles.CROSS_DOT
        colorHex = p.getString(KEY_COLOR, null)?.takeIf(::isHexColor) ?: "#22C55E"
        sizeDp = p.getInt(KEY_SIZE, 18).coerceIn(SIZE_MIN, SIZE_MAX)
        thicknessDp = p.getInt(KEY_THICKNESS, 2).coerceIn(THICKNESS_MIN, THICKNESS_MAX)
        opacityPercent = p.getInt(KEY_OPACITY, 100).coerceIn(OPACITY_MIN, OPACITY_MAX)
        // 损坏/写了一半的 JSON 直接当没有预设，不让一个坏档拖崩设置页
        presets = p.getString(KEY_PRESETS, null)
            ?.let { raw -> runCatching { json.decodeFromString<List<CrosshairPreset>>(raw) }.getOrNull() }
            ?.filter { it.style in CrosshairStyles.ALL }
            ?: emptyList()
    }

    fun setEnabled(context: Context, enabled: Boolean) {
        this.enabled = enabled
        prefs(context).edit().putBoolean(KEY_ENABLED, enabled).apply()
    }

    fun setStyle(context: Context, style: String) {
        if (style !in CrosshairStyles.ALL) return
        this.style = style
        prefs(context).edit().putString(KEY_STYLE, style).apply()
    }

    fun setColor(context: Context, hex: String) {
        if (!isHexColor(hex)) return
        colorHex = hex.uppercase()
        prefs(context).edit().putString(KEY_COLOR, colorHex).apply()
    }

    fun setSize(context: Context, value: Int) {
        sizeDp = value.coerceIn(SIZE_MIN, SIZE_MAX)
        prefs(context).edit().putInt(KEY_SIZE, sizeDp).apply()
    }

    fun setThickness(context: Context, value: Int) {
        thicknessDp = value.coerceIn(THICKNESS_MIN, THICKNESS_MAX)
        prefs(context).edit().putInt(KEY_THICKNESS, thicknessDp).apply()
    }

    fun setOpacity(context: Context, percent: Int) {
        opacityPercent = percent.coerceIn(OPACITY_MIN, OPACITY_MAX)
        prefs(context).edit().putInt(KEY_OPACITY, opacityPercent).apply()
    }

    /** 把当前外观存成同名预设；已存在同名则覆盖更新 */
    fun savePreset(context: Context, name: String) {
        val trimmed = name.trim()
        if (trimmed.isEmpty()) return
        val preset = CrosshairPreset(trimmed, style, colorHex, sizeDp, thicknessDp, opacityPercent)
        presets = presets.filter { it.name != trimmed } + preset
        persistPresets(context)
    }

    fun deletePreset(context: Context, name: String) {
        presets = presets.filter { it.name != name }
        persistPresets(context)
    }

    /** 套用预设：字段逐个夹回合法区间，并整体落盘——否则重启后会回到套用前的样子 */
    fun applyPreset(context: Context, preset: CrosshairPreset) {
        if (preset.style !in CrosshairStyles.ALL) return
        style = preset.style
        colorHex = if (isHexColor(preset.colorHex)) preset.colorHex else colorHex
        sizeDp = preset.sizeDp.coerceIn(SIZE_MIN, SIZE_MAX)
        thicknessDp = preset.thicknessDp.coerceIn(THICKNESS_MIN, THICKNESS_MAX)
        opacityPercent = preset.opacityPercent.coerceIn(OPACITY_MIN, OPACITY_MAX)
        prefs(context).edit()
            .putString(KEY_STYLE, style)
            .putString(KEY_COLOR, colorHex)
            .putInt(KEY_SIZE, sizeDp)
            .putInt(KEY_THICKNESS, thicknessDp)
            .putInt(KEY_OPACITY, opacityPercent)
            .apply()
    }

    private fun persistPresets(context: Context) {
        prefs(context).edit().putString(KEY_PRESETS, json.encodeToString(presets)).apply()
    }

    private fun isHexColor(value: String): Boolean =
        value.length == 7 && value.startsWith('#') &&
            value.substring(1).all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}