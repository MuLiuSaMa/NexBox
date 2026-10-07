package com.nexbox.app.data

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

/**
 * 屏幕悬浮框（HUD）设置：开关、位置与外观（显示项/不透明度/大小/横竖/锁定），
 * SharedPreferences 持久化。设置页与服务共同读写这里，全是 Compose 状态，
 * 改动即时反映到悬浮框（服务里的 Compose 直接读这些字段，改完即重组）。
 *
 * 服务被系统回收时 [com.nexbox.app.overlay.HudOverlayService] 会在 onDestroy 把
 * 开关落回 false，保证界面状态与屏幕上的事实一致。
 */
object OverlayStore {
    private const val PREFS_NAME = "nexbox_overlay"
    private const val KEY_ENABLED = "enabled"
    private const val KEY_X = "pos_x"
    private const val KEY_Y = "pos_y"
    private const val KEY_LOCKED = "locked"
    private const val KEY_OPACITY = "opacity"
    private const val KEY_SCALE = "scale"
    private const val KEY_HORIZONTAL = "horizontal"
    private const val KEY_SHOW_CPU_USAGE = "show_cpu_usage"
    private const val KEY_SHOW_CPU_TEMP = "show_cpu_temp"
    private const val KEY_SHOW_GPU_TEMP = "show_gpu_temp"
    private const val KEY_SHOW_MEMORY = "show_memory"
    private const val KEY_SHOW_DELTA = "show_delta_passwords"
    private const val KEY_SELECTED_MAPS = "selected_maps"

    /** 不透明度/大小的取值范围（百分比）。不透明度 0 = 背景全透只剩文字，由用户自己决定 */
    const val OPACITY_MIN = 0
    const val OPACITY_MAX = 100
    const val SCALE_MIN = 70
    const val SCALE_MAX = 160

    /** 悬浮框未定位过的哨兵值：位置从没存过时用默认落点 */
    const val NO_POSITION = Int.MIN_VALUE

    var enabled by mutableStateOf(false)
        private set

    var posX by mutableStateOf(NO_POSITION)
        private set

    var posY by mutableStateOf(NO_POSITION)
        private set

    /** 锁定：禁止拖动，且悬浮框整体不拦截触摸（点它能点穿到下面的应用） */
    var locked by mutableStateOf(false)
        private set

    /** 不透明度百分比（[OPACITY_MIN]~[OPACITY_MAX]），作用于整个悬浮框 */
    var opacityPercent by mutableStateOf(85)
        private set

    /** 缩放百分比（[SCALE_MIN]~[SCALE_MAX]），等比作用于字号、内边距与圆角 */
    var scalePercent by mutableStateOf(100)
        private set

    /** false = 竖排（两行堆叠），true = 横排（单行并排） */
    var horizontal by mutableStateOf(false)
        private set

    var showCpuUsage by mutableStateOf(true)
        private set

    var showCpuTemp by mutableStateOf(true)
        private set

    var showGpuTemp by mutableStateOf(true)
        private set

    /** 内存占用率（used / total，与系统设置同口径） */
    var showMemory by mutableStateOf(true)
        private set

    /** 三角洲每日密码大类：开 = 悬浮框显示已选地图的密码 */
    var showDeltaPasswords by mutableStateOf(true)
        private set

    /** 已选地图集合（[DELTA_MAPS] 的子集），空集 = 大类开着但一张都不显示 */
    var selectedMaps by mutableStateOf(DELTA_MAPS.toSet())
        private set

    fun init(context: Context) {
        val p = prefs(context)
        enabled = p.getBoolean(KEY_ENABLED, false)
        posX = p.getInt(KEY_X, NO_POSITION)
        posY = p.getInt(KEY_Y, NO_POSITION)
        locked = p.getBoolean(KEY_LOCKED, false)
        opacityPercent = p.getInt(KEY_OPACITY, 85).coerceIn(OPACITY_MIN, OPACITY_MAX)
        scalePercent = p.getInt(KEY_SCALE, 100).coerceIn(SCALE_MIN, SCALE_MAX)
        horizontal = p.getBoolean(KEY_HORIZONTAL, false)
        showCpuUsage = p.getBoolean(KEY_SHOW_CPU_USAGE, true)
        showCpuTemp = p.getBoolean(KEY_SHOW_CPU_TEMP, true)
        showGpuTemp = p.getBoolean(KEY_SHOW_GPU_TEMP, true)
        showMemory = p.getBoolean(KEY_SHOW_MEMORY, true)
        showDeltaPasswords = p.getBoolean(KEY_SHOW_DELTA, true)
        selectedMaps = p.getStringSet(KEY_SELECTED_MAPS, null)
            ?.let { saved -> DELTA_MAPS.filter { it in saved }.toSet() }
            ?: DELTA_MAPS.toSet()
    }

    fun setEnabled(context: Context, enabled: Boolean) {
        this.enabled = enabled
        prefs(context).edit().putBoolean(KEY_ENABLED, enabled).apply()
    }

    fun savePosition(context: Context, x: Int, y: Int) {
        posX = x
        posY = y
        prefs(context).edit().putInt(KEY_X, x).putInt(KEY_Y, y).apply()
    }

    /** 有没有存过位置。没存过时服务按屏幕尺寸算默认落点，而不是用 (0,0) 贴左上角 */
    fun hasPosition(): Boolean = posX != NO_POSITION && posY != NO_POSITION

    /** 清掉保存的位置：下次摆放回到默认落点（左右居中、状态栏下方）。「重置位置」按钮用 */
    fun clearPosition(context: Context) {
        posX = NO_POSITION
        posY = NO_POSITION
        prefs(context).edit().remove(KEY_X).remove(KEY_Y).apply()
    }

    fun setLocked(context: Context, locked: Boolean) {
        this.locked = locked
        prefs(context).edit().putBoolean(KEY_LOCKED, locked).apply()
    }

    fun setOpacity(context: Context, percent: Int) {
        opacityPercent = percent.coerceIn(OPACITY_MIN, OPACITY_MAX)
        prefs(context).edit().putInt(KEY_OPACITY, opacityPercent).apply()
    }

    fun setScale(context: Context, percent: Int) {
        scalePercent = percent.coerceIn(SCALE_MIN, SCALE_MAX)
        prefs(context).edit().putInt(KEY_SCALE, scalePercent).apply()
    }

    fun setHorizontal(context: Context, horizontal: Boolean) {
        this.horizontal = horizontal
        prefs(context).edit().putBoolean(KEY_HORIZONTAL, horizontal).apply()
    }

    fun setShowCpuUsage(context: Context, value: Boolean) {
        showCpuUsage = value
        prefs(context).edit().putBoolean(KEY_SHOW_CPU_USAGE, value).apply()
    }

    fun setShowCpuTemp(context: Context, value: Boolean) {
        showCpuTemp = value
        prefs(context).edit().putBoolean(KEY_SHOW_CPU_TEMP, value).apply()
    }

    fun setShowGpuTemp(context: Context, value: Boolean) {
        showGpuTemp = value
        prefs(context).edit().putBoolean(KEY_SHOW_GPU_TEMP, value).apply()
    }

    fun setShowMemory(context: Context, value: Boolean) {
        showMemory = value
        prefs(context).edit().putBoolean(KEY_SHOW_MEMORY, value).apply()
    }

    fun setShowDeltaPasswords(context: Context, value: Boolean) {
        showDeltaPasswords = value
        prefs(context).edit().putBoolean(KEY_SHOW_DELTA, value).apply()
    }

    fun setSelectedMaps(context: Context, maps: Set<String>) {
        selectedMaps = maps
        prefs(context).edit().putStringSet(KEY_SELECTED_MAPS, maps).apply()
    }

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
}