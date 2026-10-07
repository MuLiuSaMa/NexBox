package com.nexbox.app.ui.screen

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.DiskStatus
import com.nexbox.app.data.HardwareIdentity
import com.nexbox.app.data.HardwareSnapshot
import com.nexbox.app.data.MemOptimizeResult
import com.nexbox.app.data.MemoryStatus
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.NexBoxException
import com.nexbox.app.data.WsEvent
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.decodeFromJsonElement

/**
 * 硬件面板状态。
 * 所有数值都可空——机器上没有对应传感器（比如核显没有独立温度）时 UI 统一显示「—」，
 * 不能用 0 冒充，否则会把「读不到」误读成「占用为零」。
 *
 * @Immutable：只用 copy 换新值、从不原地改；标注后 Compose 才能跳过没变化的子树，
 * 否则硬件面板每秒一帧的刷新会拖着整棵首页重组（页面切换会掉帧）。
 */
@Immutable
data class HardwareUiState(
    val snapshot: HardwareSnapshot? = null,
    val memory: MemoryStatus? = null,
    val disk: DiskStatus? = null,
    /** PC 端版本较旧、还没实现 disk.status（查询返回 404）时为 true */
    val diskUnsupported: Boolean = false,
    /** 一键内存优化执行中：锁住内存卡片防止连点 */
    val memoryBusy: Boolean = false,
    /** CPU / GPU 静态型号信息（`hw.info`，进详情页展示用；老 PC 端为 null） */
    val identity: HardwareIdentity? = null,
    /** CPU / GPU 占用率的滚动采样（每秒一点），详情页画趋势线用 */
    val cpuHistory: List<Int?> = emptyList(),
    val gpuHistory: List<Int?> = emptyList(),
    val message: String? = null,
    /** [message] 是否按错误样式展示（成功提示走普通样式） */
    val messageIsError: Boolean = true,
)

/**
 * 硬件面板。
 *
 * 数据来源分两路，各取所长：
 * 1. CPU / GPU / 内存的占用与温度 —— 直接吃 WS stats 帧（PC 端每秒推一次），实时且零额外请求；
 * 2. 内存 / 磁盘的容量明细 —— REST 轮询，两者开销差很多所以分频：
 *    `mem.status` 只是一次 sysinfo 内存读取（2s），`disk.status` 要枚举全部磁盘（12s）。
 *
 * 游戏模式已搬到 [PcControlViewModel]（主页卡片第二页的开关面板），这里不再管。
 */
class HardwareViewModel : ViewModel() {

    private val _state = MutableStateFlow(HardwareUiState())
    val state: StateFlow<HardwareUiState> = _state.asStateFlow()

    /** 轮询计数：每 tick 刷内存，到倍数才刷磁盘 */
    private var tick = 0

    /** 型号信息已拿到（或确认 PC 端没有该接口），不再重复请求 */
    private var identityDone = false

    init {
        // ① 实时快照：不轮询，跟着 WS 走；顺手滚动记一份占用率历史给详情页画趋势
        viewModelScope.launch {
            NetworkModule.statsRepo.hardware.collect { snap ->
                _state.update { s ->
                    s.copy(
                        snapshot = snap,
                        // 换机/解绑后 snap 归 null，历史一并清掉，不能把上一台机器的曲线留给新机器
                        cpuHistory = if (snap == null) emptyList() else s.cpuHistory.appendCapped(snap.cpuUsage),
                        gpuHistory = if (snap == null) emptyList() else s.gpuHistory.appendCapped(snap.gpuUsage),
                    )
                }
            }
        }

        // ②③ 容量明细 + 游戏模式：只在有会话时轮询，解绑立刻停
        viewModelScope.launch {
            NetworkModule.session.session
                .distinctUntilChanged()
                .collectLatest { session ->
                    if (session == null) {
                        _state.update {
                            it.copy(
                                memory = null,
                                disk = null,
                                diskUnsupported = false,
                                identity = null,
                                cpuHistory = emptyList(),
                                gpuHistory = emptyList(),
                            )
                        }
                        return@collectLatest
                    }
                    tick = 0
                    identityDone = false
                    while (isActive) {
                        refreshIdentity()
                        refreshMemory()
                        if (tick % DISK_EVERY_N == 0) refreshDisk()
                        tick++
                        delay(POLL_MS)
                    }
                }
        }

        // ④ 事件驱动：WS 建连后立刻补一轮，PC 端执行完动作会广播 action.done，
        // 借此让另一端的改动（包括本机的「一键内存优化」）当场反映到卡片上
        viewModelScope.launch {
            NetworkModule.statsRepo.events.collect { event ->
                when (event) {
                    is WsEvent.Connected -> refreshMemory()
                    is WsEvent.ActionDone -> when (event.key) {
                        "mem.optimize", "sys.clean_temp" -> refreshMemory()
                        else -> Unit
                    }
                    else -> Unit
                }
            }
        }
    }

    fun consumeMessage() = _state.update { it.copy(message = null) }

    // ───────── 一键内存优化 ─────────

    /**
     * 一键清理 PC 端内存（`mem.optimize`）：主页点内存卡片直接触发，不设确认弹窗。
     * PC 端做的是清待机缓存 + 收紧工作集这类轻操作，可重复执行，直接给结果提示即可。
     */
    fun optimizeMemory() {
        if (_state.value.memoryBusy) return
        viewModelScope.launch {
            _state.update { it.copy(memoryBusy = true, message = null) }
            runCatching {
                NetworkModule.json.decodeFromJsonElement(
                    MemOptimizeResult.serializer(),
                    NetworkModule.api.action("mem.optimize"),
                )
            }
                .onSuccess { r ->
                    _state.update {
                        it.copy(
                            memoryBusy = false,
                            message = r.message?.takeIf { m -> m.isNotBlank() }
                                ?: if (r.freedMb > 0) "内存优化完成，释放约 ${r.freedMb} MB" else "内存优化完成",
                            messageIsError = false,
                        )
                    }
                    refreshMemory()
                }
                .onFailure { e ->
                    _state.update { it.copy(memoryBusy = false, message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    // ───────── 内部实现 ─────────

    private suspend fun refreshMemory() {
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                MemoryStatus.serializer(),
                NetworkModule.api.query("mem.status"),
            )
        }.onSuccess { m -> _state.update { it.copy(memory = m) } }
        // 拉不到就保留上一次的值，避免网络抖动时数字闪成「—」
    }

    private suspend fun refreshDisk() {
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                DiskStatus.serializer(),
                NetworkModule.api.query("disk.status"),
            )
        }
            .onSuccess { d -> _state.update { it.copy(disk = d, diskUnsupported = false) } }
            .onFailure { e ->
                // 只有「PC 端不认识这个查询」（404）才算版本旧；
                // 网络不通不该让提示一直挂着
                val unsupported = e is NexBoxException && e.status == 404
                _state.update { it.copy(diskUnsupported = unsupported) }
            }
    }

    /**
     * CPU / GPU 型号信息。静态数据（PC 端有缓存），拿到一次就不再请求。
     * - 成功 → 置 [identityDone]，后续 tick 直接跳过；
     * - PC 端版本较旧不认识 `hw.info`（404）→ 同样收手，详情页把型号区块降级；
     * - 网络抖动之类的临时失败 → 下一轮 tick 重试。
     */
    private suspend fun refreshIdentity() {
        if (identityDone || NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                HardwareIdentity.serializer(),
                NetworkModule.api.query("hw.info"),
            )
        }
            .onSuccess { id ->
                identityDone = true
                _state.update { it.copy(identity = id) }
            }
            .onFailure { e ->
                if (e is NexBoxException && e.status == 404) identityDone = true
            }
    }

    private companion object {
        /** 内存轮询节奏：2s 足够跟上体感，又不像 WS 每秒那样白扣 PC 端开销 */
        const val POLL_MS = 2_000L

        /** 磁盘枚举比内存读取贵得多，3 轮（~6s）一次 */
        const val DISK_EVERY_N = 3
    }
}

/** 趋势线窗口：WS 每秒一帧，90 点约等于一分半，够看趋势又不吃内存 */
private const val HISTORY_SAMPLES = 90

/** 追加一个采样点并把窗口截断到 [HISTORY_SAMPLES] */
private fun List<Int?>.appendCapped(v: Int?): List<Int?> =
    (if (size >= HISTORY_SAMPLES) drop(size - HISTORY_SAMPLES + 1) else this) + v
