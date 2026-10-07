package com.nexbox.app.ui.screen

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.SpeedStage
import com.nexbox.app.data.SpeedTestEngine
import com.nexbox.app.data.SpeedTestProgress
import com.nexbox.app.data.SpeedTestServers
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * 网络测速页面的状态。
 *
 * 数值都可空语义 —— 还没测到或全失败时用 0 表示「没有」，UI 层负责显示成「—」，
 * 不要把 0 直接当成「速率是零」展示。
 */
@Immutable
data class SpeedTestUiState(
    val stage: SpeedStage = SpeedStage.IDLE,
    val pingMs: Double = 0.0,
    val jitterMs: Double = 0.0,
    val packetLossPct: Double = 0.0,
    val downloadMbps: Double = 0.0,
    val uploadMbps: Double = 0.0,
    val selectedServer: String = SpeedTestServers.DEFAULT_ID,
    /** 实时曲线采样点（Mbps），上限 [MAX_POINTS] */
    val downloadPoints: List<Float> = emptyList(),
    val uploadPoints: List<Float> = emptyList(),
    /** 跑完后的说明：哪一项没拿到数据、具体什么原因（引擎给的），正常时为空 */
    val note: String = "",
    val error: String? = null,
) {
    val running: Boolean
        get() = stage == SpeedStage.PING ||
            stage == SpeedStage.DOWNLOAD ||
            stage == SpeedStage.UPLOAD
}

/**
 * 网络测速。测的是本机网络，数据全部来自 [SpeedTestEngine]，不经过 PC 端。
 *
 * 注意 `viewModel()` 解析到的是 Activity 级的 ViewModelStore，
 * 切到别的 tab 并不会销毁它 —— 所以页面离开时必须调 [stop]，否则测速会在后台继续跑流量。
 */
class SpeedTestViewModel : ViewModel() {

    private val _state = MutableStateFlow(SpeedTestUiState())
    val state: StateFlow<SpeedTestUiState> = _state.asStateFlow()

    val servers = SpeedTestServers.ALL

    private var runJob: Job? = null

    fun selectServer(id: String) {
        if (_state.value.running) return
        _state.update { it.copy(selectedServer = id) }
    }

    fun start() {
        if (_state.value.running) return
        val server = _state.value.selectedServer
        // 每次开始都清掉上一轮的数字与曲线，避免新旧数据混在一起
        _state.value = SpeedTestUiState(stage = SpeedStage.PING, selectedServer = server)

        runJob = viewModelScope.launch {
            runCatching {
                SpeedTestEngine.start(server).collect { progress ->
                    _state.update { reduce(it, progress) }
                }
            }.onFailure { e ->
                // 用户点停止会取消这个 job，那是正常流程，不能当成错误弹出来
                if (e is CancellationException) throw e
                _state.update {
                    it.copy(stage = SpeedStage.IDLE, error = "测速失败：${e.message ?: "网络异常"}")
                }
            }
        }
    }

    /** 停止测速。已经测到的数字保留，阶段落回完成态 */
    fun stop() {
        if (!_state.value.running) return
        runJob?.cancel()
        runJob = null
        _state.update { it.copy(stage = SpeedStage.DONE, error = null) }
    }

    fun consumeError() = _state.update { it.copy(error = null) }

    override fun onCleared() {
        runJob?.cancel()
        runJob = null
    }

    private fun reduce(state: SpeedTestUiState, progress: SpeedTestProgress): SpeedTestUiState {
        val enteredStage = state.stage != progress.stage
        // 与 PC 端一致：进入某阶段时清掉该阶段曲线，另一条保留
        var download =
            if (enteredStage && progress.stage == SpeedStage.DOWNLOAD) emptyList()
            else state.downloadPoints
        var upload =
            if (enteredStage && progress.stage == SpeedStage.UPLOAD) emptyList()
            else state.uploadPoints

        if (progress.stage == SpeedStage.DOWNLOAD && progress.downloadMbps > 0) {
            download = download.appendCapped(progress.downloadMbps.toFloat())
        }
        if (progress.stage == SpeedStage.UPLOAD && progress.uploadMbps > 0) {
            upload = upload.appendCapped(progress.uploadMbps.toFloat())
        }

        return state.copy(
            stage = progress.stage,
            pingMs = progress.pingMs,
            jitterMs = progress.jitterMs,
            packetLossPct = progress.packetLossPct,
            downloadMbps = progress.downloadMbps,
            uploadMbps = progress.uploadMbps,
            downloadPoints = download,
            uploadPoints = upload,
            // 说明文字只在跑完那一帧有意义，其余阶段清空
            note = if (progress.stage == SpeedStage.DONE) progress.message else "",
            error = null,
        )
    }
}

/** 曲线窗口：80ms 一帧，120 点约 9.6 秒，够覆盖单阶段又不会无限增长（与 PC 端一致） */
private const val MAX_POINTS = 120

private fun List<Float>.appendCapped(value: Float): List<Float> =
    (if (size >= MAX_POINTS) drop(size - MAX_POINTS + 1) else this) + value
