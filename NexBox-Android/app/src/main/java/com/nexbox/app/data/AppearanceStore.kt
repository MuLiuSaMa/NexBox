package com.nexbox.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/** 默认主题色：新境盒品牌蓝 */
const val DefaultAccentHex = "#4C8DFF"

/** 主题模式。存枚举名，改名/加项时旧值落回 [ThemeMode.System] 而不是崩 */
enum class ThemeMode(val label: String) {
    System("跟随系统"),
    Light("浅色"),
    Dark("深色"),
}

/** 外观设置：主题模式、主题色与「液态玻璃卡片」开关，SharedPreferences 持久化，重启保留 */
object AppearanceStore {
    private const val PREFS_NAME = "nexbox_appearance"
    private const val KEY_GLASS_CARDS = "glass_cards"
    private const val KEY_THEME_MODE = "theme_mode"
    private const val KEY_ACCENT = "accent_color"

    /** 液态玻璃卡片开关：开 = 所有卡片玻璃化，关 = 普通 surface 卡片 */
    var glassEnabled by mutableStateOf(false)
        private set

    var themeMode by mutableStateOf(ThemeMode.System)
        private set

    /** 主题色，存 `#RRGGBB` 字符串：比存 int 好在日志里能直接读，也不会被 alpha 坑 */
    var accent by mutableStateOf(DefaultAccentHex)
        private set

    fun init(context: Context) {
        val p = prefs(context)
        glassEnabled = p.getBoolean(KEY_GLASS_CARDS, false)
        themeMode = p.getString(KEY_THEME_MODE, null)
            ?.let { saved -> ThemeMode.entries.firstOrNull { it.name == saved } }
            ?: ThemeMode.System
        accent = p.getString(KEY_ACCENT, null)?.takeIf { isHexColor(it) } ?: DefaultAccentHex
    }

    fun setGlassEnabled(context: Context, enabled: Boolean) {
        glassEnabled = enabled
        prefs(context).edit().putBoolean(KEY_GLASS_CARDS, enabled).apply()
    }

    fun setThemeMode(context: Context, mode: ThemeMode) {
        themeMode = mode
        prefs(context).edit().putString(KEY_THEME_MODE, mode.name).apply()
    }

    fun setAccent(context: Context, hex: String) {
        if (!isHexColor(hex)) return
        accent = hex.uppercase()
        prefs(context).edit().putString(KEY_ACCENT, accent).apply()
    }

    private fun isHexColor(value: String): Boolean =
        value.length == 7 && value.startsWith('#') &&
            value.substring(1).all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}
