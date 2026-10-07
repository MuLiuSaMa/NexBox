package com.nexbox.app.overlay

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.os.Build
import android.os.IBinder
import android.provider.Settings
import android.util.DisplayMetrics
import android.util.Log
import android.view.Gravity
import android.view.WindowManager
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.ComposeView
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner
import com.nexbox.app.BuildConfig
import com.nexbox.app.MainActivity
import com.nexbox.app.R
import com.nexbox.app.data.AppearanceStore
import com.nexbox.app.data.CrosshairStore
import com.nexbox.app.data.CrosshairStyles
import com.nexbox.app.data.DeltaForceStore
import com.nexbox.app.data.DeltaPasswordItem
import com.nexbox.app.data.LocalDeviceReader
import com.nexbox.app.data.OverlayStore
import com.nexbox.app.data.CROSSHAIR_COLORS
import com.nexbox.app.ui.theme.DarkDanger
import com.nexbox.app.ui.theme.DarkMetricGpu
import com.nexbox.app.ui.theme.DarkMetricMemory
import com.nexbox.app.ui.theme.DarkMetricStorage
import com.nexbox.app.ui.theme.DarkPanel
import com.nexbox.app.ui.theme.MiSans
import com.nexbox.app.ui.theme.parseHexColor
import kotlin.math.max
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * 屏幕悬浮框：前台服务 + WindowManager 悬浮窗口，显示 CPU 占用/温度与 GPU 占用（温度）。
 *
 * 三件事决定了这里的实现方式：
 * - **必须前台服务**：targetSdk 36 上悬浮窗脱离前台服务活不过几分钟；用的是
 *   `specialUse` 类型（Android 14 起自定义用途只能走它），通知栏有一条常驻通知。
 * - **窗口用 Compose 画**：服务里没有 Activity，ComposeView 需要手动补
 *   Lifecycle / ViewModelStore / SavedState 三个 ViewTreeOwner，[OverlayViewOwner] 就是干这个的。
 * - **读数来自 [LocalDeviceReader]**：GPU 占用在这台机器上被 SELinux 锁死，拿不到就显示「—」，
 *   不编造数字（详见该文件里 gpuUsageNow 的说明）。
 */
class HudOverlayService : Service() {

    companion object {
        private const val CHANNEL_ID = "hud_overlay"
        private const val NOTIFICATION_ID = 1021
        private const val ACTION_STOP = "com.nexbox.app.action.STOP_HUD"

        /** 设置页「重置位置」按钮通过这个 action 让运行中的服务立刻摆回默认落点 */
        const val ACTION_RESET_POSITION = "com.nexbox.app.action.RESET_HUD_POSITION"

        private const val POLL_MS = 1_000L

        /** 每日密码拉取周期。Store 自带 60s 缓存，这里兜底节流 */
        private const val PASSWORD_POLL_MS = 60_000L

        /** 默认落点与状态栏的间距（dp）：默认位置必须整个躲在状态栏下面 */
        private const val STATUS_BAR_GAP_DP = 12

        /** 悬浮框与屏幕边缘的最小距离（dp）：默认落点与旋转后的贴边都用它 */
        private const val EDGE_MARGIN_DP = 10

        /** 常驻 flags：不抢焦点（不挡输入法）；坐标从屏幕左上角算，便于自己夹边界 */
        private val BASE_WINDOW_FLAGS =
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS

        /** 进程内是否正在跑。界面靠它判断「开关开着但服务没了」（进程被杀过） */
        @Volatile
        var isRunning = false
            private set

        fun start(context: Context) {
            ContextCompat.startForegroundService(context, Intent(context, HudOverlayService::class.java))
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, HudOverlayService::class.java))
        }

        /** 悬浮框 / 辅助准心任一开着就保持服务，全关才停。各设置页开关后调用 */
        fun refresh(context: Context) {
            if (OverlayStore.enabled || CrosshairStore.enabled) start(context) else stop(context)
        }
    }

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val owner = OverlayViewOwner()
    private val crosshairOwner = OverlayViewOwner()

    private var windowManager: WindowManager? = null
    private var rootView: ComposeView? = null
    private var params: WindowManager.LayoutParams? = null

    private var crosshairRoot: ComposeView? = null
    private var crosshairParams: WindowManager.LayoutParams? = null

    private var sensorJob: Job? = null
    private var passwordJob: Job? = null

    private var sample by mutableStateOf(HudSample(null, null, null, null))

    /** 三角洲每日密码：独立协程按分钟拉取（[DeltaForceStore] 内部自带 60s 缓存） */
    private var passwords by mutableStateOf<List<DeltaPasswordItem>>(emptyList())

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        startForegroundCompat()
        isRunning = true
        // 两个窗口（悬浮框 / 准心）的挂与拆跟着 Store 状态走：设置页改开关，这里自动响应，
        // 不需要设置页知道窗口细节。轮询也在这里按需启停（只有准心时零轮询）。
        // 这个收集器必须固定在主线程：Lifecycle/SavedState 的注册与 WindowManager.addView
        // 都有主线程检查，跑在 scope 的 Default 上会直接 IllegalStateException 闪退。
        // 单次挂/拆各自 runCatching：任何一步失败都不许杀掉收集循环——
        // 收集循环一死，Store 的后续变化就无人响应，开关表现为「关不上」
        scope.launch(Dispatchers.Main) {
            snapshotFlow { OverlayStore.enabled to CrosshairStore.enabled }
                .collect { (hud, crosshair) ->
                    if (hud) {
                        runCatching { attachOverlay() }.onFailure {
                            OverlayStore.setEnabled(this@HudOverlayService, false)
                        }
                    } else {
                        runCatching { detachOverlay() }
                    }
                    if (crosshair) {
                        runCatching { attachCrosshair() }.onFailure {
                            CrosshairStore.setEnabled(this@HudOverlayService, false)
                        }
                    } else {
                        runCatching { detachCrosshair() }
                    }
                    syncPolling()
                    if (!OverlayStore.enabled && !CrosshairStore.enabled) stopSelf()
                }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                // 通知栏的「关闭」按钮 = 服务总闸：两个功能一起落回关闭
                OverlayStore.setEnabled(this, false)
                CrosshairStore.setEnabled(this, false)
                stopSelf()
            }
            ACTION_RESET_POSITION -> {
                // 设置页的「重置位置」：丢掉存档坐标，立刻摆回默认落点
                OverlayStore.clearPosition(this)
                clampIntoScreen(reposition = true)
            }
        }
        return START_NOT_STICKY
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        // 旋转/折叠屏后原来的坐标可能整块跑到屏幕外，按新尺寸夹回来
        clampIntoScreen(reposition = true)
    }

    override fun onDestroy() {
        scope.cancel()
        detachOverlay()
        detachCrosshair()
        isRunning = false
        // 开关状态不在这里改写：服务被杀后由工具页/设置页的 heal 逻辑补拉起，
        // 界面开关与屏幕事实在那一步重新对齐
        super.onDestroy()
    }

    // ───────────────────────── 前台通知 ─────────────────────────

    private fun createChannel() {
        val manager = getSystemService(NotificationManager::class.java) ?: return
        if (manager.getNotificationChannel(CHANNEL_ID) != null) return
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "悬浮框", NotificationManager.IMPORTANCE_LOW).apply {
                description = "在屏幕上悬浮显示本机 CPU / GPU 状态"
                setShowBadge(false)
            },
        )
    }

    private fun startForegroundCompat() {
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    private fun buildNotification(): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, HudOverlayService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_hud_notification)
            .setContentTitle("悬浮窗已开启")
            .setContentText("悬浮框 / 辅助准心运行中")
            .setContentIntent(open)
            .addAction(0, "全部关闭", stop)
            .setOngoing(true)
            .setShowWhen(false)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .build()
    }

    // ───────────────────────── 悬浮窗口 ─────────────────────────

    private fun attachOverlay() {
        if (rootView != null) return
        // 权限被撤销（用户在系统设置里关掉）时不要硬加窗口，直接收摊
        if (!Settings.canDrawOverlays(this)) {
            stopSelf()
            return
        }
        val wm = getSystemService(WindowManager::class.java) ?: run { stopSelf(); return }
        val view = ComposeView(this).apply {
            setViewTreeLifecycleOwner(owner)
            setViewTreeViewModelStoreOwner(owner)
            setViewTreeSavedStateRegistryOwner(owner)
            setContent {
                HudPanel(
                    sample = sample,
                    passwords = passwords,
                    onDrag = ::onDrag,
                    onDragEnd = ::persistPosition,
                    onLockChanged = ::applyLock,
                    onPanelWidthChanged = ::applyNaturalWidth,
                    maxWidthPx = hudCapWidthPx(),
                )
            }
        }
        owner.moveToResumed()

        val layout = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            BASE_WINDOW_FLAGS,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
        }

        windowManager = wm
        rootView = view
        params = layout
        // 首帧就要摆对位置：addView 之前把坐标定死（存档或默认），否则会先在 (0,0) 闪一下
        // 再跳到默认点。视图还没 layout 量不到真实尺寸，用标称值估算，layout 完成后
        // clampIntoScreen 会用真实尺寸再校一遍，偏差只有一个像素级的小跳。
        if (OverlayStore.hasPosition()) {
            layout.x = OverlayStore.posX
            layout.y = OverlayStore.posY
        } else {
            applyDefaultPosition(
                layout,
                estW = dpToPx(if (OverlayStore.horizontal) HUD_WIDTH_DP * 2 - 10 else HUD_WIDTH_DP),
                estH = dpToPx(if (OverlayStore.horizontal) HUD_HEIGHT_DP / 2 else HUD_HEIGHT_DP),
            )
        }
        runCatching { wm.addView(view, layout) }.onFailure {
            // 权限刚被撤销时 addView 会抛异常；清理干净让开关和界面对得上
            rootView = null
            params = null
            stopSelf()
            return
        }
        // 加进窗口树后才量得到真实尺寸，这时再定初始位置；锁定态也按存档摆好
        view.post {
            clampIntoScreen(reposition = true)
            applyLock(OverlayStore.locked)
            logHudLayout("attached")
        }
    }

    private fun detachOverlay() {
        val view = rootView ?: return
        runCatching { windowManager?.removeView(view) }
        rootView = null
        params = null
        owner.onDetached()
    }

    // ───────────────────────── 辅助准心窗口 ─────────────────────────

    /**
     * 准心窗口：小画布 + gravity CENTER 定位（天然屏幕居中），x/y 只承担偏移量。
     * flags 恒为「不抢焦点 + 触摸穿透」——准心只显示，绝不拦截游戏里的任何操作。
     */
    private fun attachCrosshair() {
        if (crosshairRoot != null) return
        if (!Settings.canDrawOverlays(this)) return
        // 写回字段：只开准心（HUD 关）时这里是 null，不写回的话 detachCrosshair
        // 会因 windowManager 为 null 跳过 removeView，窗口泄漏成「关不上」
        val wm = windowManager ?: getSystemService(WindowManager::class.java) ?: return
        windowManager = wm
        val view = ComposeView(this).apply {
            setViewTreeLifecycleOwner(crosshairOwner)
            setViewTreeViewModelStoreOwner(crosshairOwner)
            setViewTreeSavedStateRegistryOwner(crosshairOwner)
            setContent { CrosshairPanel() }
        }
        crosshairOwner.moveToResumed()

        val layout = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            // 没有 NO_LIMITS：准心以屏幕几何中心定位，不越界
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT,
        ).apply {
            gravity = Gravity.CENTER
        }

        crosshairRoot = view
        crosshairParams = layout
        runCatching { wm.addView(view, layout) }.onFailure {
            crosshairRoot = null
            crosshairParams = null
        }
    }

    private fun detachCrosshair() {
        val view = crosshairRoot ?: return
        val wm = windowManager ?: getSystemService(WindowManager::class.java)
        runCatching { wm?.removeView(view) }
        crosshairRoot = null
        crosshairParams = null
        crosshairOwner.onDetached()
    }

    /** 拖动：把位移加到窗口坐标上，并夹在屏幕内（卡片可以贴边但不会跑出去） */
    private fun onDrag(dx: Float, dy: Float) {
        val layout = params ?: return
        layout.x += dx.roundToInt()
        layout.y += dy.roundToInt()
        // clampIntoScreen 内部已经 updateView()：这里再调一次就是每个运动事件对半透明浮层
        // 刷两遍 relayout，两遍之间表面被重新合成，肉眼就是拖动时的闪烁
        clampIntoScreen(reposition = false)
    }

    private fun persistPosition() {
        val layout = params ?: return
        OverlayStore.savePosition(this, layout.x, layout.y)
        logHudLayout("dragEnd")
    }

    /**
     * 锁定切换：锁定时整块窗口不拦截触摸（点它能点穿到下面的应用，彻底防误触），
     * 解锁恢复可拖动。由面板组合内的 LaunchedEffect 在锁定状态变化时调到主线程执行。
     */
    private fun applyLock(locked: Boolean) {
        val layout = params ?: return
        layout.flags = if (locked) {
            BASE_WINDOW_FLAGS or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
        } else {
            BASE_WINDOW_FLAGS
        }
        updateView()
    }

    /**
     * 把窗口夹进屏幕。`reposition = true` 表示首次摆放或旋转后重摆：
     * 没存过位置时用默认落点（[applyDefaultPosition]），存过就沿用并把越界坐标拉回来。
     */
    private fun clampIntoScreen(reposition: Boolean) {
        val layout = params ?: return
        val (screenW, screenH) = screenSize()
        val viewW = rootView?.width?.takeIf { it > 0 } ?: dpToPx(HUD_WIDTH_DP)
        val viewH = rootView?.height?.takeIf { it > 0 } ?: dpToPx(HUD_HEIGHT_DP)
        if (reposition && !OverlayStore.hasPosition()) {
            applyDefaultPosition(layout, estW = viewW, estH = viewH)
        }
        layout.x = layout.x.coerceIn(0, (screenW - viewW).coerceAtLeast(0))
        layout.y = layout.y.coerceIn(0, (screenH - viewH).coerceAtLeast(0))
        updateView()
    }

    /**
     * 默认落点：**左右居中、状态栏下方**。
     * FLAG_LAYOUT_NO_LIMITS 下 y=0 是屏幕物理顶（状态栏底下），所以必须加状态栏高度偏移，
     * 不然默认位置整个压在状态栏里；拖动不受此限制，用户想放哪放哪。
     */
    private fun applyDefaultPosition(layout: WindowManager.LayoutParams, estW: Int, estH: Int) {
        val (screenW, screenH) = screenSize()
        layout.x = ((screenW - estW) / 2).coerceAtLeast(dpToPx(EDGE_MARGIN_DP))
        layout.y = statusBarHeightPx() + dpToPx(STATUS_BAR_GAP_DP)
        // 屏幕特别矮（分屏/小窗）时的兜底：默认点不许被挤出屏幕底
        if (layout.y > screenH - estH) layout.y = (screenH - estH).coerceAtLeast(0)
    }

    /** 状态栏高度：NO_LIMITS 模式下坐标从物理顶开始，默认落点要按它偏移 */
    private fun statusBarHeightPx(): Int {
        val id = resources.getIdentifier("status_bar_height", "dimen", "android")
        return if (id > 0) resources.getDimensionPixelSize(id) else dpToPx(28)
    }

    private fun updateView() {
        val view = rootView ?: return
        val layout = params ?: return
        runCatching { windowManager?.updateViewLayout(view, layout) }
    }

    private fun screenSize(): Pair<Int, Int> {
        val wm = windowManager ?: return dpToPx(400) to dpToPx(800)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val bounds = wm.currentWindowMetrics.bounds
            bounds.width() to bounds.height()
        } else {
            val metrics = DisplayMetrics()
            @Suppress("DEPRECATION")
            wm.defaultDisplay.getRealMetrics(metrics)
            metrics.widthPixels to metrics.heightPixels
        }
    }

    /**
     * 临时诊断（仅 debug 构建）：折行点到底由谁定的，已经靠推断错过两轮，
     * 先把「窗口帧、实测尺寸、系统给的可用宽度」打出来再动手。
     * 抓法：adb logcat -s NexBoxHud
     */
    private fun logHudLayout(from: String) {
        if (!BuildConfig.DEBUG) return
        val layout = params ?: return
        val (screenW, screenH) = screenSize()
        Log.i(
            "NexBoxHud",
            "$from x=${layout.x} y=${layout.y} frame=${layout.width}x${layout.height} " +
                "view=${rootView?.width ?: 0}x${rootView?.height ?: 0} " +
                "screen=${screenW}x$screenH " +
                "availW=${screenW - layout.x} density=${resources.displayMetrics.density}",
        )
    }

    private fun dpToPx(dp: Int): Int = (dp * resources.displayMetrics.density).roundToInt()

    /** 悬浮框允许占到的最大宽度：屏宽减去两侧留白。再宽就把右圆角顶出屏幕了 */
    private fun hudCapWidthPx(): Int =
        (screenSize().first - 2 * dpToPx(EDGE_MARGIN_DP)).coerceAtLeast(0)

    /**
     * 横排把帧宽定成面板的自然宽度（全部项目排成一行所需的宽度）。
     *
     * `WRAP_CONTENT` 是可调尺寸窗口，系统按「屏宽 − x」给测量矩形，而且帧一旦缩小就不
     * 回弹，所以项目再多 pill 也只长到屏幕右边缘，看着像固定长度。显式帧宽不受 x 影响，
     * 项目增减都能立刻反映成长度变化；面板那边已经把宽度封顶到 `hudCapWidthPx()`，
     * 所以帧不会超过屏幕，两端圆角都在画面内，放不下的项目从右侧裁掉。
     */
    private fun applyNaturalWidth(panelWpx: Int) {
        val layout = params ?: return
        if (panelWpx <= 0) return
        // 竖排一行一项、宽度本来就窄，继续按内容自适应
        val wanted =
            if (OverlayStore.horizontal) panelWpx.coerceAtMost(hudCapWidthPx())
            else WindowManager.LayoutParams.WRAP_CONTENT
        if (layout.width == wanted) return
        layout.width = wanted
        logHudLayout("width->$wanted")
        // onSizeChanged 处在 layout 阶段，updateViewLayout 必须 post 出去。
        // 生效后再测一次得到的宽度与帧宽相等，直接 return，不会来回刷
        rootView?.post { clampIntoScreen(reposition = false) }
    }

    // ───────────────────────── 采样 ─────────────────────────

    /** HUD 开着才跑采样轮询；只有准心时零轮询（准心是纯静态绘制） */
    private fun syncPolling() {
        if (OverlayStore.enabled) {
            if (sensorJob == null) sensorJob = scope.launch { sensorLoop() }
            if (passwordJob == null) passwordJob = scope.launch { passwordLoop() }
        } else {
            sensorJob?.cancel()
            sensorJob = null
            passwordJob?.cancel()
            passwordJob = null
        }
    }

    private suspend fun sensorLoop() {
        // scope.cancel() 后 isActive 翻 false，循环自然退出
        while (scope.isActive) {
            val next = HudSample(
                cpuUsage = LocalDeviceReader.cpuUsageNow(),
                cpuTemp = LocalDeviceReader.cpuTempNow(),
                gpuTemp = LocalDeviceReader.gpuTempNow(),
                memUsage = LocalDeviceReader.memUsageNow(this@HudOverlayService),
            )
            // Compose 快照状态支持跨线程写入，这里不切主线程，省一次往返还避免抖动
            sample = next
            delay(POLL_MS)
        }
    }

    private suspend fun passwordLoop() {
        while (scope.isActive) {
            runCatching { DeltaForceStore.fetchPasswords() }.onSuccess { items ->
                if (items.isNotEmpty()) passwords = items
            }
            delay(PASSWORD_POLL_MS)
        }
    }

}

/**
 * HUD 的一张读数。任何一项拿不到就是 null，UI 显示「—」。
 * GPU 没有占用字段：kgsl / devfreq 节点被 SELinux 全锁，免 root 拿不到（本机实测连 shell 都拒），
 * 与其常年挂一个「—」不如不显示，GPU 行只报温度。
 */
private data class HudSample(
    val cpuUsage: Int?,
    val cpuTemp: Double?,
    val gpuTemp: Double?,
    /** 内存占用率（used/total） */
    val memUsage: Int?,
)

/** 悬浮框标称尺寸（dp）：只用于首次摆放与夹边界时估算，真实尺寸以布局结果为凖 */
private const val HUD_WIDTH_DP = 150
private const val HUD_HEIGHT_DP = 62

/**
 * 服务里的 Compose 宿主生命周期。
 *
 * ComposeView 挂进窗口树时会向上找 Lifecycle / ViewModelStore / SavedStateRegistry 三个
 * Owner，服务没有 Activity 提供它们，所以这里造一个最简实现：创建时直接推到 RESUMED
 * （悬浮框没有前后台之分），销毁时走到 DESTROYED 并清掉 ViewModelStore。
 */
private class OverlayViewOwner : LifecycleOwner, ViewModelStoreOwner, SavedStateRegistryOwner {
    private val registry = LifecycleRegistry(this)
    private val savedStateController = SavedStateRegistryController.create(this)

    /** performRestore 一辈子只能调一次；窗口拆了重挂时靠这个标记跳过第二次 */
    private var restored = false

    override val lifecycle: Lifecycle get() = registry
    override val viewModelStore = ViewModelStore()
    override val savedStateRegistry: SavedStateRegistry get() = savedStateController.savedStateRegistry

    fun moveToResumed() {
        if (!restored) {
            savedStateController.performRestore(null)
            restored = true
        }
        registry.currentState = Lifecycle.State.RESUMED
    }

    /** 拆窗口 ≠ 销毁 owner：只退到 CREATED（绝不走 DESTROYED），下一个窗口直接复用 */
    fun onDetached() {
        registry.currentState = Lifecycle.State.CREATED
        viewModelStore.clear()
    }
}

/**
 * 悬浮框本体：指标行（CPU 占用+温度 / GPU 温度 / 刷新率）+ 三角洲每日密码，
 * 整块可拖动（锁定时禁拖且触摸穿透）。
 *
 * 所有外观参数（显示项/不透明度/大小/横竖）直接读 [OverlayStore]——它是 Compose 状态，
 * 设置页一改这里立刻重组。配色固定走深色面板，不跟随 App 的深浅色主题——悬浮框是
 * 盖在**别人家界面**上的，浅色卡片压在白色页面上会直接看不见。
 */
@Composable
private fun HudPanel(
    sample: HudSample,
    passwords: List<DeltaPasswordItem>,
    onDrag: (Float, Float) -> Unit,
    onDragEnd: () -> Unit,
    onLockChanged: (Boolean) -> Unit,
    /** 面板自然宽度（px）：横排靠它把窗口帧撑到「全部项目排一行」所需的宽度 */
    onPanelWidthChanged: (Int) -> Unit,
    /** 面板可占的最大宽度（px）= 屏宽减两侧留白，超出部分裁掉以保住两端圆角 */
    maxWidthPx: Int,
) {
    val locked = OverlayStore.locked
    LaunchedEffect(locked) { onLockChanged(locked) }

    // 缩放系数：字号、间距、圆角、定宽数字列全部等比，改大小不会破排版
    val s = OverlayStore.scalePercent / 100f
    // 不透明度只作用于背景与描边：文字永远完全不透明，低透明度下读数依然清晰
    val opacity = OverlayStore.opacityPercent / 100f
    val horizontal = OverlayStore.horizontal
    val showCpuUsage = OverlayStore.showCpuUsage
    val showCpuTemp = OverlayStore.showCpuTemp
    val showGpuTemp = OverlayStore.showGpuTemp
    val cpuColor = parseHexColor(AppearanceStore.accent) ?: Color(0xFF4C8DFF)

    val panel = Modifier
        .clip(RoundedCornerShape((14 * s).dp))
        .background(DarkPanel.copy(alpha = opacity))
        .border(1.dp, Color.White.copy(alpha = 0.16f * opacity), RoundedCornerShape((14 * s).dp))
        .pointerInput(locked) {
            // 锁定时不挂拖动手势：整块窗口也被 NOT_TOUCHABLE 点穿，这里只是双保险
            if (!locked) {
                detectDragGestures(
                    onDragEnd = onDragEnd,
                    onDragCancel = onDragEnd,
                    onDrag = { change, drag ->
                        change.consume()
                        onDrag(drag.x, drag.y)
                    },
                )
            }
        }
        // 横排：clip/背景内侧按无限宽排单行，节点自身尺寸交给外侧的 widthIn 封顶。
        // 顺序不能反 —— widthIn 放在 clip 内侧的话，圆角会按未封顶的宽度画，右半截跑出屏幕
        .then(if (horizontal) Modifier.wrapContentWidth(unbounded = true) else Modifier)
        // 在无限宽约束内侧量，量到的才是「全部项目排一行」的自然宽度（含左右内边距）。
        // 放到 Column 外侧量的话，帧宽定死之后上报值恒等于帧宽，项目变多也测不出来
        .onSizeChanged { onPanelWidthChanged(it.width) }
        .padding(horizontal = (12 * s).dp, vertical = (9 * s).dp)

    // 指标与每日密码共用同一份内容：竖排一行一项，横排全部塞进同一条单行 Row
    val items: @Composable () -> Unit = {
        if (showCpuUsage || showCpuTemp) {
            HudRow(
                label = "CPU",
                color = cpuColor,
                usage = sample.cpuUsage,
                temp = sample.cpuTemp,
                showUsage = showCpuUsage,
                showTemp = showCpuTemp,
                scale = s,
            )
        }
        if (showGpuTemp) {
            HudRow(
                label = "GPU",
                color = DarkMetricGpu,
                usage = null,
                temp = sample.gpuTemp,
                showUsage = false,
                showTemp = true,
                scale = s,
            )
        }
        if (OverlayStore.showMemory) {
            HudRow(
                label = "内存",
                color = DarkMetricMemory,
                usage = sample.memUsage,
                temp = null,
                showUsage = true,
                showTemp = false,
                scale = s,
            )
        }
        if (OverlayStore.showDeltaPasswords) {
            val visible = passwords.filter { it.name in OverlayStore.selectedMaps }
            if (visible.isEmpty()) {
                PasswordPlaceholder(scale = s)
            } else {
                visible.forEach { item -> PasswordRow(item.name, item.password, s, compact = horizontal) }
            }
        }
    }

    // 横排把整块宽度封顶到「屏宽 − 两侧留白」：项目排不下时从右侧裁掉，
    // 但两端圆角始终落在屏幕内，不会被顶出去变成方头条
    Column(
        modifier = (if (horizontal) {
            Modifier.widthIn(max = with(LocalDensity.current) { maxWidthPx.toDp() })
        } else {
            Modifier
        })
            .then(panel),
        verticalArrangement = Arrangement.spacedBy((6 * s).dp),
    ) {
        if (horizontal) {
            // 横排：强制单行，有几个项目就排几个，宽度跟着内容走。
            // 超出屏幕的部分直接被裁掉（不折行 = 行数恒定，拖动时不会再改行数闪烁）
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy((12 * s).dp),
            ) { items() }
        } else {
            items()
        }
    }
}

/**
 * 一行读数：圆点 + 名称 + 占用率 + 温度。数字列定宽右对齐，数值跳动时排版不会抖。
 * 整行按内容自适应宽度，不撑满面板——面板宽度由最宽的一行决定，撑满会在标签和
 * 数字之间留出一大块空白。
 */
@Composable
private fun HudRow(
    label: String,
    color: Color,
    usage: Int?,
    temp: Double?,
    showUsage: Boolean,
    showTemp: Boolean,
    scale: Float,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size((6 * scale).dp).clip(CircleShape).background(color))
        Spacer(Modifier.width((6 * scale).dp))
        Text(
            label,
            fontSize = (11 * scale).sp,
            fontWeight = FontWeight.Medium,
            color = Color.White,
            fontFamily = HudFontFamily,
        )
        Spacer(Modifier.width((10 * scale).dp))
        if (showUsage) {
            Text(
                usage?.let { "$it%" } ?: "—",
                fontSize = (12.5 * scale).sp,
                fontWeight = FontWeight.SemiBold,
                color = Color.White,
                textAlign = TextAlign.End,
                fontFamily = HudFontFamily,
                maxLines = 1,
                modifier = Modifier.width((34 * scale).dp),
            )
        }
        if (showTemp) {
            Text(
                temp?.let { "${it.roundToInt()}°C" } ?: "—",
                fontSize = (11.5 * scale).sp,
                color = hudTempColor(temp),
                textAlign = TextAlign.End,
                fontFamily = HudFontFamily,
                maxLines = 1,
                modifier = Modifier.width((38 * scale).dp),
            )
        }
    }
}

/** 密码还没拉到（刚开启/接口抖动）时的占位小字 */
@Composable
private fun PasswordPlaceholder(scale: Float) {
    Text(
        "密码获取中…",
        fontSize = (10.5 * scale).sp,
        color = Color.White.copy(alpha = 0.75f),
        fontFamily = HudFontFamily,
    )
}

/**
 * 每日密码行：地图名 + 密码，整行按内容自适应，不靠 weight 把密码推到面板右缘——
 * 那样会让面板被最宽的一行定死宽度。[compact]（横排）省掉圆点，排进指标那一行更紧凑。
 */
@Composable
private fun PasswordRow(name: String, code: String, scale: Float, compact: Boolean = false) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (!compact) {
            Box(Modifier.size((6 * scale).dp).clip(CircleShape).background(DarkMetricStorage))
            Spacer(Modifier.width((6 * scale).dp))
        }
        Text(
            name,
            fontSize = (10.5 * scale).sp,
            fontWeight = FontWeight.Medium,
            color = Color.White,
            fontFamily = HudFontFamily,
            maxLines = 1,
        )
        Spacer(Modifier.width(((if (compact) 4 else 6) * scale).dp))
        Text(
            code,
            fontSize = ((if (compact) 11f else 11.5f) * scale).sp,
            fontWeight = FontWeight.SemiBold,
            color = Color.White,
            fontFamily = HudFontFamily,
            maxLines = 1,
        )
    }
}

/** 悬浮框字体：MiSans 与 App 全局一致（res/font 打包的同一家族，服务里没有主题上下文所以要显式给） */
private val HudFontFamily = MiSans

/** 温度配色沿用配置页的手机口径（48 偏高 / 58 危险）；常态与缺省「—」都是纯白，和占用率一致 */
private fun hudTempColor(temp: Double?): Color = when {
    temp == null -> Color.White
    temp >= 58.0 -> DarkDanger
    temp >= 48.0 -> DarkMetricStorage
    else -> Color.White
}
/**
 * 辅助准心：小画布 Canvas 绘制，窗口 gravity CENTER + 偏移量定位。
 * 全部参数直接读 [CrosshairStore]——设置页一改这里立刻重绘。
 * 窗口永远触摸穿透（NOT_TOUCHABLE），准心只显示、绝不拦截游戏操作。
 */
@Composable
private fun CrosshairPanel() {
    val color = (parseHexColor(CrosshairStore.colorHex) ?: Color(0xFF22C55E))
        .copy(alpha = CrosshairStore.opacityPercent / 100f)
    val size = CrosshairStore.sizeDp.dp
    val thickness = CrosshairStore.thicknessDp.dp
    // 画布给足描边与圆帽的余量，避免粗线/圆圈被裁边
    val canvas = size + thickness * 2 + 10.dp

    Canvas(modifier = Modifier.size(canvas)) {
        val center = center
        val strokeW = thickness.toPx()
        val half = size.toPx() / 2f
        when (CrosshairStore.style) {
            CrosshairStyles.DOT -> drawCircle(color = color, radius = half)

            CrosshairStyles.CIRCLE -> drawCircle(
                color = color,
                radius = half,
                style = Stroke(width = strokeW, cap = StrokeCap.Round),
            )

            CrosshairStyles.CROSS, CrosshairStyles.CROSS_DOT -> {
                // 中心留空隙：圆点准心不留（点本身就是中心），十字带点留大一些给点让位
                val gap = if (CrosshairStore.style == CrosshairStyles.CROSS_DOT) {
                    half * 0.34f
                } else {
                    half * 0.12f
                }
                drawLine(color, Offset(center.x - half, center.y), Offset(center.x - gap, center.y), strokeW, StrokeCap.Round)
                drawLine(color, Offset(center.x + gap, center.y), Offset(center.x + half, center.y), strokeW, StrokeCap.Round)
                drawLine(color, Offset(center.x, center.y - half), Offset(center.x, center.y - gap), strokeW, StrokeCap.Round)
                drawLine(color, Offset(center.x, center.y + gap), Offset(center.x, center.y + half), strokeW, StrokeCap.Round)
                if (CrosshairStore.style == CrosshairStyles.CROSS_DOT) {
                    drawCircle(color = color, radius = max(strokeW, 2.dp.toPx()))
                }
            }
        }
    }
}
