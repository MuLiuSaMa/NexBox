package com.nexbox.app.ui.screen

import android.os.Build
import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.DiscoveryReply
import com.nexbox.app.data.InfoData
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.QrPayload
import com.nexbox.app.data.Session
import com.nexbox.app.data.WsEvent
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/** 连接页/设置页共用的界面状态 */
@Immutable
data class ConnectUiState(
    /** 已保存的会话，非空表示已配对 */
    val session: Session? = null,
    val host: String = "",
    val port: String = "",
    val code: String = "",
    /** 正在执行连接/配对/解绑等操作 */
    val busy: Boolean = false,
    /** 一次性提示（错误或成功），展示后由 [ConnectViewModel.consumeMessage] 清掉 */
    val message: String? = null,
    val messageIsError: Boolean = true,
    /** 探活结果，用来提前告诉用户"PC 端远程控制没开" */
    val info: InfoData? = null,
    /** 正在自动搜索局域网设备 */
    val scanning: Boolean = false,
    /** 搜索到的设备（可控制的排在前面） */
    val discovered: List<DiscoveryReply> = emptyList(),
    /** 已发出配对请求，正在等 PC 端用户点「允许」 */
    val awaitingApproval: Boolean = false,
    val approvalDeviceName: String? = null,
    /** 来自 WS meta 的 PC 主机名 */
    val pcName: String? = null,
    val wsConnected: Boolean = false,
) {
    val connected: Boolean get() = session != null && wsConnected
}

/**
 * 连接流程：探活 → 配对 → 持久化 → 建立 WS。
 * 三种入口（手输 / 搜索 / 扫码）最终都收敛到 [pair]。
 */
class ConnectViewModel : ViewModel() {

    private val _state = MutableStateFlow(ConnectUiState())
    val state: StateFlow<ConnectUiState> = _state.asStateFlow()

    private var autoJob: Job? = null

    init {
        // 连接状态以 StateFlow 为准，而不是靠 WsEvent.Connected 事件：
        // 仓库在 App 启动时就建连，UI 订阅晚一步会错过那次事件
        viewModelScope.launch {
            NetworkModule.statsRepo.connected.collect { up ->
                _state.update { it.copy(wsConnected = up) }
            }
        }
        // 会话变化 → 刷新状态并（重）建 WebSocket
        viewModelScope.launch {
            NetworkModule.session.session
                .distinctUntilChanged()
                .collectLatest { session ->
                    _state.update { it.copy(session = session) }
                    if (session == null) {
                        _state.update { it.copy(wsConnected = false, pcName = null) }
                        return@collectLatest
                    }
                    // 订阅全局唯一的 WS（由 StatsRepository 持有），
                    // 硬件面板读的是同一条连接，不重复建连
                    NetworkModule.statsRepo.events.collect(::onWsEvent)
                }
        }
        // 预填上次输入的地址；已配对但通道未通就主动催一次端口刷新。
        // 注意：这里不再自动扫描配对 —— 搜索/配对过程只发生在添加弹窗里，
        // 主页卡片未配对时保持故事动画，不被扫描状态打扰
        viewModelScope.launch {
            val s = NetworkModule.session.current()
            if (s != null) {
                _state.update { it.copy(host = s.host, port = s.port.toString()) }
                // 已配对但通道未通：可能 PC 重启换了端口，主动催一次端口刷新，缩短恢复时间
                if (!_state.value.wsConnected) NetworkModule.reconnect.refreshOnce()
            }
        }
    }

    private suspend fun onWsEvent(event: WsEvent) {
        when (event) {
            is WsEvent.Connected -> _state.update { it.copy(wsConnected = true) }
            is WsEvent.Disconnected -> _state.update {
                it.copy(wsConnected = false, message = event.reason, messageIsError = true)
            }
            is WsEvent.Unauthorized -> {
                // 令牌被 PC 端撤销：清会话退回连接页
                NetworkModule.session.clear()
                _state.update {
                    it.copy(
                        wsConnected = false,
                        message = "登录状态已失效，请重新配对",
                        messageIsError = true,
                    )
                }
            }
            is WsEvent.Stats -> {
                val name = event.meta?.computerName
                if (!name.isNullOrBlank() && name != _state.value.pcName) {
                    _state.update { it.copy(pcName = name) }
                }
            }
            is WsEvent.ActionDone -> Unit // 控制页据此重拉 query，本轮先忽略
            is WsEvent.TransferUpdate -> Unit // 文件互传列表刷新由 FileTransferViewModel 处理
            is WsEvent.Unknown -> Unit
        }
    }

    fun onHostChange(v: String) = _state.update { it.copy(host = v.trim()) }
    fun onPortChange(v: String) = _state.update { it.copy(port = v.filter(Char::isDigit)) }
    fun onCodeChange(v: String) = _state.update { it.copy(code = v.filter(Char::isDigit).take(6)) }
    fun consumeMessage() = _state.update { it.copy(message = null) }

    /** 手动输入连接：先探活，再配对 */
    fun connectManual() {
        val s = _state.value
        val port = s.port.toIntOrNull()
        when {
            s.host.isBlank() -> return warn("请填写 PC 端 IP 地址")
            port == null || port !in 1..65535 -> return warn("端口需为 1-65535 的数字")
            s.code.length != 6 -> return warn("配对码是 PC 端显示的 6 位数字")
        }
        pair(s.host, port, s.code)
    }

    /** 探活：不需要配对码，用于确认地址可达（设置页的「测试连接」） */
    fun probe() {
        val session = _state.value.session
        val host = session?.host ?: _state.value.host
        val port = session?.port ?: _state.value.port.toIntOrNull()
        if (host.isBlank()) return warn("请填写 PC 端 IP 地址")
        if (port == null || port !in 1..65535) return warn("端口需为 1-65535 的数字")

        viewModelScope.launch {
            _state.update { it.copy(busy = true, message = null) }
            runCatching { NetworkModule.api.info(host, port) }
                .onSuccess { info ->
                    _state.update {
                        it.copy(
                            busy = false,
                            info = info,
                            message = if (info.controlEnabled) {
                                "已连上 ${info.lanName.orEmpty().ifBlank { "$host:$port" }}（服务 v${info.version.orEmpty()}）"
                            } else {
                                "已连上 PC 端，但远程控制未开启，请在 PC 端打开后重试"
                            },
                            messageIsError = !info.controlEnabled,
                        )
                    }
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(busy = false, message = e.toUserMessage(), messageIsError = true)
                    }
                }
        }
    }

    /**
     * 扫码结果：PC 端二维码里已经带了 ip / port / code，
     * 所以直接配对，用户不需要手输配对码。
     */
    fun onQrScanned(raw: String?) {
        if (raw.isNullOrBlank()) return // 用户取消扫描
        val parsed = QrPayload.parse(raw)
        if (parsed == null) {
            warn("无法识别该二维码，请扫描 PC 端「手机远程连接」弹窗里显示的二维码")
            return
        }
        _state.update {
            it.copy(host = parsed.ip, port = parsed.port.toString(), code = parsed.code)
        }
        pair(parsed.ip, parsed.port, parsed.code)
    }

    /** 扫码入口解析出的地址直接配对 */
    fun pairFromQr(host: String, port: Int, code: String) {
        _state.update { it.copy(host = host, port = port.toString(), code = code) }
        pair(host, port, code)
    }

    // ───────── 扫描设备（只列清单，不自动配对）→ 手动点「配对」→ 等 PC 端确认 ─────────

    /**
     * 搜索同一 Wi-Fi 下的新境盒 PC 端，把结果列进弹窗 —— **不会自动发起配对**，
     * 配对永远由用户点设备行的「手动配对」触发（[requestPairWith]）。
     */
    fun startScan(restart: Boolean = false) {
        if (_state.value.session != null) return
        if (_state.value.scanning && !restart) return

        autoJob?.cancel()
        _state.update {
            it.copy(
                scanning = true,
                discovered = emptyList(),
                awaitingApproval = false,
                approvalDeviceName = null,
                message = null,
            )
        }

        autoJob = viewModelScope.launch {
            try {
                NetworkModule.discovery.scan(timeoutMs = if (restart) 3_000 else 2_500)
                    .collect { list ->
                        _state.update { s ->
                            s.copy(discovered = list.sortedByDescending { it.controlEnabled })
                        }
                    }
            } catch (_: Throwable) {
                // 广播失败静默，退化为扫码
            } finally {
                _state.update { it.copy(scanning = false) }
            }
        }
    }

    /** 向指定设备发起配对请求并轮询审批结果 */
    private suspend fun requestApproval(device: DiscoveryReply) {
        val host = device.ip.orEmpty()
        val port = device.tcpPort
        val name = device.name.orEmpty().ifBlank { "新境盒 PC 端" }
        if (host.isBlank() || port !in 1..65535) return

        _state.update {
            it.copy(
                awaitingApproval = true,
                approvalDeviceName = name,
                host = host,
                port = port.toString(),
                message = "已向「$name」发送配对请求，请在电脑上点「允许」",
                messageIsError = false,
            )
        }

        val created = runCatching {
            NetworkModule.api.requestPair(
                host,
                port,
                Build.MODEL ?: "Android",
                NetworkModule.session.installId(),
            )
        }.getOrElse { e ->
            _state.update {
                it.copy(awaitingApproval = false, message = e.toUserMessage(), messageIsError = true)
            }
            return
        }

        val deadline = System.currentTimeMillis() + APPROVAL_TIMEOUT_MS
        while (currentCoroutineContext().isActive && System.currentTimeMillis() < deadline) {
            delay(APPROVAL_POLL_MS)
            val st = runCatching {
                NetworkModule.api.pairRequestStatus(host, port, created.requestId)
            }.getOrNull() ?: continue

            when (st.status) {
                "approved" -> {
                    val token = st.token
                    if (token.isNullOrBlank()) continue
                    NetworkModule.session.save(
                        Session(
                            host = host,
                            port = port,
                            token = token,
                            deviceId = st.deviceId.orEmpty(),
                            deviceName = name,
                        )
                    )
                    _state.update {
                        it.copy(
                            awaitingApproval = false,
                            message = "已连接到 $name",
                            messageIsError = false,
                        )
                    }
                    return
                }

                "denied" -> {
                    _state.update {
                        it.copy(awaitingApproval = false, message = "PC 端拒绝了本次配对请求", messageIsError = true)
                    }
                    return
                }

                "expired", "unknown" -> {
                    _state.update {
                        it.copy(awaitingApproval = false, message = "配对请求已失效，请重新扫描", messageIsError = true)
                    }
                    return
                }
            }
        }

        // 超时：撤掉 PC 端那条待审批项，避免残留
        NetworkModule.api.cancelPairRequest(host, port, created.requestId)
        _state.update {
            it.copy(awaitingApproval = false, message = "等待 PC 端确认超时，请重新扫描", messageIsError = true)
        }
    }

    /** 手动点击某个搜索到的设备发起配对：随时可点，换设备时取消旧的等待、发起新请求 */
    fun requestPairWith(device: DiscoveryReply) {
        autoJob?.cancel()
        autoJob = viewModelScope.launch { requestApproval(device) }
    }

    fun stopAutoPair() {
        autoJob?.cancel()
        autoJob = null
        _state.update { it.copy(scanning = false, awaitingApproval = false) }
    }

    /** 解绑：先通知 PC 端撤销令牌，再清本地会话 */
    fun unpair() {
        viewModelScope.launch {
            _state.update { it.copy(busy = true, message = null) }
            val s = _state.value.session
            if (s != null) {
                runCatching { NetworkModule.api.unpair() }
                    .onFailure { e ->
                        // PC 端不可达也要允许本地退出，否则用户会卡死
                        _state.update { it.copy(message = "PC 端解绑失败：${e.toUserMessage()}，已清除本地登录", messageIsError = true) }
                    }
            }
            NetworkModule.session.clear()
            _state.update { it.copy(busy = false, info = null, code = "") }
        }
    }

    fun clearAddress() = _state.update { it.copy(host = "", port = "", code = "", info = null) }

    private fun pair(host: String, port: Int, code: String) {
        viewModelScope.launch {
            _state.update { it.copy(busy = true, message = null) }
            runCatching {
                // 先探活，能提前把"远程控制没开"这类问题说清楚
                val info = NetworkModule.api.info(host, port)
                if (!info.controlEnabled) {
                    error("PC 端远程控制未开启，请先在 PC 端打开")
                }
                NetworkModule.api.pair(host, port, code, NetworkModule.session.installId()) to info
            }
                .onSuccess { (paired, info) ->
                    NetworkModule.session.save(
                        Session(
                            host = host,
                            port = port,
                            token = paired.token,
                            deviceId = paired.deviceId,
                            deviceName = info.lanName.orEmpty(),
                        )
                    )
                    _state.update {
                        it.copy(
                            busy = false,
                            code = "",
                            info = info,
                            message = "已连接到 ${info.lanName.orEmpty().ifBlank { "$host:$port" }}",
                            messageIsError = false,
                        )
                    }
                }
                .onFailure { e ->
                    _state.update { it.copy(busy = false, message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    private fun warn(text: String) =
        _state.update { it.copy(message = text, messageIsError = true) }

    private companion object {
        /** PC 端待审批请求有效期 120s，这边多等一点再放弃 */
        const val APPROVAL_TIMEOUT_MS = 125_000L
        const val APPROVAL_POLL_MS = 2_000L
    }
}
