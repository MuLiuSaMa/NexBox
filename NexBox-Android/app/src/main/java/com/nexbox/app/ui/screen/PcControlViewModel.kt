package com.nexbox.app.ui.screen

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.CrosshairStatus
import com.nexbox.app.data.FilterActionResult
import com.nexbox.app.data.FilterSettings
import com.nexbox.app.data.GameModeStatus
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.NexBoxException
import com.nexbox.app.data.OverlayStatus
import com.nexbox.app.data.WsEvent
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.decodeFromJsonElement

/** 主页互联卡第二页上的四个 PC 功能开关 */
enum class PcFeature { GAME_MODE, FILTER, CROSSHAIR, OVERLAY }

/**
 * PC 功能开关面板状态。
 *
 * 用四个独立 Boolean 而不是 Map/Set —— 后者对 Compose 是 unstable 类型，
 * 会让 [Immutable] 失效、破坏重组跳过。
 */
@Immutable
data class PcControlUiState(
    val gameModeOn: Boolean = false,
    val filterOn: Boolean = false,
    val crosshairOn: Boolean = false,
    val overlayOn: Boolean = false,
    /** 同一时间只允许一个开关在写，避免并发点按互相盖状态 */
    val busy: PcFeature? = null,
    /** PC 端版本较旧、没有这个接口（查询 404）时为 true → 该开关置灰并提示升级 */
    val crosshairUnsupported: Boolean = false,
    val overlayUnsupported: Boolean = false,
    val message: String? = null,
    val messageIsError: Boolean = true,
)

/**
 * 调节 PC 端功能的四个开关：游戏模式 / 滤镜 / 准心 / 悬浮框。
 *
 * 从 [HardwareViewModel] 里把游戏模式整段搬了过来 —— 那边原本是「已实现但没有 UI 消费方」
 * 的悬空状态，四个开关归一处管，也避免两个 ViewModel 各拉一份 `gamemode.status`。
 *
 * 数据都是低频的用户驱动状态，2s 轮询一次；PC 端界面上改开关不会推 WS，
 * 只能靠轮询兜底（收到 `action.done` 时再补一次）。
 */
class PcControlViewModel : ViewModel() {

    private val _state = MutableStateFlow(PcControlUiState())
    val state: StateFlow<PcControlUiState> = _state.asStateFlow()

    /** 游戏模式写入：连点只保留最后一次，否则上一轮的延时补读会把新状态盖回去 */
    private var gameModeJob: Job? = null

    init {
        // ① 轮询：只在有会话时跑，解绑立刻停并清空
        viewModelScope.launch {
            NetworkModule.session.session
                .distinctUntilChanged()
                .collectLatest { session ->
                    if (session == null) {
                        _state.value = PcControlUiState()
                        return@collectLatest
                    }
                    while (isActive) {
                        refreshAll()
                        delay(POLL_MS)
                    }
                }
        }

        // ② 事件驱动：建连后立刻补一轮；PC 端执行完动作会广播 action.done，
        //    但那个广播不带结果数据，所以收到后必须自己再查一次
        viewModelScope.launch {
            NetworkModule.statsRepo.events.collect { event ->
                when (event) {
                    is WsEvent.Connected -> refreshAll()
                    is WsEvent.ActionDone -> when {
                        event.key.startsWith("gamemode.") -> refreshGameMode()
                        event.key.startsWith("filter.") -> refreshFilter()
                        event.key == "crosshair.toggle" -> refreshCrosshair()
                        event.key == "overlay.toggle" -> refreshOverlay()
                    }
                    else -> Unit
                }
            }
        }
    }

    // ───────── 对外入口 ─────────

    /** 切换某个 PC 功能。四个都先本地翻面（PC 端都是同步置位），失败再回滚 */
    fun setEnabled(feature: PcFeature, enabled: Boolean) {
        if (_state.value.busy != null) return
        when (feature) {
            PcFeature.GAME_MODE -> setGameMode(enabled)
            PcFeature.FILTER -> setFilter(enabled)
            PcFeature.CROSSHAIR -> setCrosshair(enabled)
            PcFeature.OVERLAY -> setOverlay(enabled)
        }
    }

    fun consumeMessage() = _state.update { it.copy(message = null) }

    override fun onCleared() {
        gameModeJob?.cancel()
        gameModeJob = null
    }

    // ───────── 游戏模式 ─────────

    /**
     * 与 PC 端顶栏那个开关完全等价：开 = 常规档，关 = 默认档。
     *
     * 走 `gamemode.set_preset` —— PC 端扫描线程只认 `preset` / `auto_preset`，
     * `gamemode.set_auto` 改的 `auto_enabled` 是个不起作用的字段（以前手机端误用过它）。
     */
    private fun setGameMode(enabled: Boolean) {
        val preset = if (enabled) "regular" else "default"
        gameModeJob?.cancel()
        gameModeJob = viewModelScope.launch {
            _state.update {
                it.copy(busy = PcFeature.GAME_MODE, message = null, gameModeOn = enabled)
            }
            val result = runCatching {
                NetworkModule.api.action(
                    "gamemode.set_preset",
                    JsonObject(mapOf("preset" to JsonPrimitive(preset))),
                )
            }
            // 写请求一回来就收 busy：后面的事只影响回读，不该锁住开关
            _state.update { it.copy(busy = null) }

            result
                .onSuccess {
                    refreshGameMode()
                    // active / suppressed_count 要等 PC 端下一轮扫描（3s）才翻页
                    delay(SETTLE_MS)
                    refreshGameMode()
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(
                            message = e.toUserMessage(),
                            messageIsError = true,
                            gameModeOn = !enabled,
                        )
                    }
                    refreshGameMode()
                }
        }
    }

    // ───────── 滤镜 ─────────

    private fun setFilter(enabled: Boolean) {
        viewModelScope.launch {
            _state.update { it.copy(busy = PcFeature.FILTER, message = null, filterOn = enabled) }
            val result = runCatching {
                NetworkModule.json.decodeFromJsonElement(
                    FilterActionResult.serializer(),
                    NetworkModule.api.action(if (enabled) "filter.enable" else "filter.disable"),
                )
            }
            _state.update { it.copy(busy = null) }

            result
                .onSuccess { r ->
                    when {
                        // 请求被处理但这次操作没真正发生 —— 绝不能当成成功显示，
                        // 否则会出现「点了没反应但开关显示已开」，立刻回读纠正
                        r.skippedStale -> refreshFilter()
                        // 返回体里带了最新设置，直接用它（权威）
                        r.settings != null -> _state.update { it.copy(filterOn = r.settings.isOn) }
                        else -> refreshFilter()
                    }
                    if (r.degraded) {
                        _state.update { it.copy(message = "滤镜以兼容模式切换，未还原原校色", messageIsError = false) }
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(message = e.toUserMessage(), messageIsError = true, filterOn = !enabled)
                    }
                    refreshFilter()
                }
        }
    }

    // ───────── 准心 / 悬浮框 ─────────

    private fun setCrosshair(enabled: Boolean) {
        viewModelScope.launch {
            _state.update { it.copy(busy = PcFeature.CROSSHAIR, message = null, crosshairOn = enabled) }
            runCatching { NetworkModule.api.action("crosshair.toggle") }
                .onSuccess { refreshCrosshair() }
                .onFailure { e ->
                    _state.update {
                        it.copy(message = e.toUserMessage(), messageIsError = true, crosshairOn = !enabled)
                    }
                    refreshCrosshair()
                }
            _state.update { it.copy(busy = null) }
        }
    }

    private fun setOverlay(enabled: Boolean) {
        viewModelScope.launch {
            _state.update { it.copy(busy = PcFeature.OVERLAY, message = null, overlayOn = enabled) }
            runCatching { NetworkModule.api.action("overlay.toggle") }
                .onSuccess { refreshOverlay() }
                .onFailure { e ->
                    _state.update {
                        it.copy(message = e.toUserMessage(), messageIsError = true, overlayOn = !enabled)
                    }
                    refreshOverlay()
                }
            _state.update { it.copy(busy = null) }
        }
    }

    // ───────── 读状态 ─────────

    private suspend fun refreshAll() {
        refreshGameMode()
        refreshFilter()
        refreshCrosshair()
        refreshOverlay()
    }

    private suspend fun refreshGameMode() {
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                GameModeStatus.serializer(),
                NetworkModule.api.query("gamemode.status"),
            )
        }.onSuccess { gm -> _state.update { it.copy(gameModeOn = gm.isOn) } }
        // 拉不到就保留上一次的值，别让开关在网络抖动时乱跳
    }

    private suspend fun refreshFilter() {
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                FilterSettings.serializer(),
                NetworkModule.api.query("filter.settings"),
            )
        }.onSuccess { f -> _state.update { it.copy(filterOn = f.isOn) } }
    }

    /**
     * 准心状态。PC 端版本较旧时这个查询返回 404 → 标记 unsupported 并**不再重试**；
     * 网络抖动之类的临时失败不算版本问题，下一轮继续。
     */
    private suspend fun refreshCrosshair() {
        if (_state.value.crosshairUnsupported) return
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                CrosshairStatus.serializer(),
                NetworkModule.api.query("crosshair.status"),
            )
        }
            .onSuccess { c -> _state.update { it.copy(crosshairOn = c.enabled) } }
            .onFailure { e ->
                if (e is NexBoxException && e.status == 404) {
                    _state.update { it.copy(crosshairUnsupported = true) }
                }
            }
    }

    private suspend fun refreshOverlay() {
        if (_state.value.overlayUnsupported) return
        if (NetworkModule.session.current() == null) return
        runCatching {
            NetworkModule.json.decodeFromJsonElement(
                OverlayStatus.serializer(),
                NetworkModule.api.query("overlay.status"),
            )
        }
            .onSuccess { o -> _state.update { it.copy(overlayOn = o.active) } }
            .onFailure { e ->
                if (e is NexBoxException && e.status == 404) {
                    _state.update { it.copy(overlayUnsupported = true) }
                }
            }
    }
}

/** 四个开关的轮询节奏：都是内存读，2s 足够跟上 PC 端的手动改动 */
private const val POLL_MS = 2_000L

/** PC 端扫描周期 3s，多等一会再补读才能拿到「已生效」状态 */
private const val SETTLE_MS = 3_500L
