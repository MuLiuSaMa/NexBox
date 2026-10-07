package com.nexbox.app.ui.screen

import android.app.Application
import androidx.compose.runtime.Immutable
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.DeviceRuntime
import com.nexbox.app.data.DeviceSpec
import com.nexbox.app.data.LocalDeviceReader
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * 配置页状态。
 *
 * [spec] 为 null 表示静态规格还没读完（首帧），UI 走「正在读取」文案而不是铺一排空卡片。
 * 全部字段零权限、纯本机来源，所以不需要任何失败提示通道；单项读不到由读取侧置 null，
 * 交给 [SpecRow] 隐行。
 */
@Immutable
data class ConfigUiState(
    val spec: DeviceSpec? = null,
    val runtime: DeviceRuntime? = null,
    /** GPU 温度最近 [ConfigViewModel.GPU_HISTORY_LEN] 秒的采样，给性能卡的温度折线图用 */
    val gpuTempHistory: List<Double> = emptyList(),
    /** 静态规格上次扫描完成的时刻（HH:mm:ss） */
    val readAt: String? = null,
    /** 正在扫描静态规格：锁住「重新扫描」按钮防连点 */
    val scanning: Boolean = false,
)

/**
 * 读本机配置信息。
 *
 * 分两档频率：静态规格（机型 / SoC / 容量 / 特性）扫一次就缓存，这些字段不会自己变，
 * 反复解析 /sys 与系统属性纯属浪费 IO；会变的只有在线核心数、可用内存、存储剩余、
 * 当前刷新率、电量，按 1 秒轮询。
 *
 * 轮询由 [setActive] 开关控制：VM 作用域挂在 Activity 上，切到别的底部标签它不会销毁，
 * 不做 start/stop 就会一直在后台采样耗电。
 */
class ConfigViewModel(app: Application) : AndroidViewModel(app) {

    private val _state = MutableStateFlow(ConfigUiState())
    val state: StateFlow<ConfigUiState> = _state.asStateFlow()

    private var pollJob: Job? = null

    /** 静态规格已尝试过（成功与否）：失败也不在轮询里反复重扫，交给「重新扫描」按钮 */
    private var specAttempted = false

    /** 配置页可见时传 true，离开传 false */
    fun setActive(active: Boolean) {
        if (!active) {
            pollJob?.cancel()
            pollJob = null
            return
        }
        if (pollJob?.isActive == true) return
        pollJob = viewModelScope.launch {
            while (isActive) {
                if (!specAttempted) refreshSpec()
                runCatching { LocalDeviceReader.readRuntime(getApplication<Application>()) }
                    .onSuccess { runtime ->
                        _state.update { current ->
                            current.copy(
                                runtime = runtime,
                                // GPU 温度按秒入队：折线图要的是趋势，只收真样本，空轮不推进时间轴
                                gpuTempHistory = runtime.gpuTempC
                                    ?.let { t -> (current.gpuTempHistory + t).takeLast(GPU_HISTORY_LEN) }
                                    ?: current.gpuTempHistory,
                            )
                        }
                    }
                delay(POLL_MS)
            }
        }
    }

    /** 手动重扫静态规格 */
    fun reloadSpec() {
        if (_state.value.scanning) return
        // 传感器缓存一起丢：换 kernel、加散热背夹之后 thermal zone 表可能就变了
        LocalDeviceReader.resetSensors()
        viewModelScope.launch { refreshSpec() }
    }

    private suspend fun refreshSpec() {
        _state.update { it.copy(scanning = true) }
        runCatching { LocalDeviceReader.readSpec(getApplication<Application>()) }
            .onSuccess { spec ->
                _state.update {
                    it.copy(spec = spec, scanning = false, readAt = nowText())
                }
            }
            .onFailure {
                _state.update { it.copy(scanning = false) }
            }
        specAttempted = true
    }

    /** 静态扫描完成时刻。格式器每次现建，不当静态字段——用户中途切地区也不会拿着旧格式 */
    private fun nowText(): String = SimpleDateFormat("HH:mm:ss", Locale.getDefault()).format(Date())

    private companion object {
        const val POLL_MS = 1_000L

        /** GPU 温度折线的窗口长度（采样个数，1 秒一个） */
        const val GPU_HISTORY_LEN = 60
    }
}
