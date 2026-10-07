package com.nexbox.app.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * 端口漂移自动重连。
 *
 * PC 端 HTTP 端口每次启动是随机的，而 UDP 发现端口固定（45689）。当实时通道长时间连不上
 * （且不是 401 令牌失效）时，用发现拿回 PC 当前的 tcpPort，只更新 host/port、复用原 token
 * 重建 WebSocket，避免用户被迫重新配对——也就从根源上停止了 PC 端「重复已配对设备」的增长。
 *
 * 匹配「同一台 PC」：IP 优先、名称兜底。若新端口探活后仍拿不到有效令牌，
 * [StatsSocket] 会收到 401 并走既有 [WsEvent.Unauthorized] 清会话逻辑（说明 PC 端确实移除了该设备）。
 */
class ReconnectCoordinator(
    private val sessionStore: SessionStore,
    private val discovery: DiscoveryClient,
    private val api: NexBoxClient,
    private val connected: StateFlow<Boolean>,
) {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var running = false

    @Synchronized
    fun start() {
        if (running) return
        running = true

        scope.launch {
            // 会话存在 + 通道断开时进入救援循环；一旦连上或会话变化，collectLatest 会取消本轮
            combine(sessionStore.session, connected) { s, up -> s to up }
                .distinctUntilChanged()
                .collectLatest { (session, up) ->
                    if (session == null || up) return@collectLatest
                    while (currentCoroutineContext().isActive) {
                        delay(RESCUE_INTERVAL_MS)
                        if (connected.value) break
                        val current = sessionStore.current() ?: break
                        runCatching { refreshPort(current) }
                    }
                }
        }
    }

    /** 供 UI 在前台/进入连接页时主动催一次端口刷新 */
    fun refreshOnce() {
        scope.launch {
            val session = sessionStore.current() ?: return@launch
            if (connected.value) return@launch
            runCatching { refreshPort(session) }
        }
    }

    /** 用 UDP 发现找回原 PC 的当前端口，命中且端口/IP 变了才落库（token/deviceId 不变） */
    private suspend fun refreshPort(session: Session) {
        var match: DiscoveryReply? = null
        discovery.scan(timeoutMs = SCAN_TIMEOUT_MS).collect { list ->
            if (match != null) return@collect
            match = list.firstOrNull { it.ip == session.host && it.tcpPort in 1..65535 }
                ?: list.firstOrNull {
                    session.deviceName.isNotBlank() &&
                        it.name == session.deviceName &&
                        it.tcpPort in 1..65535
                }
        }

        val reply = match ?: return
        val newIp = reply.ip ?: return
        val newPort = reply.tcpPort
        // 端口和 IP 都没变，交给 WS 自身退避重连，不必落库触发重建
        if (newIp == session.host && newPort == session.port) return

        val info = runCatching { api.info(newIp, newPort) }.getOrNull() ?: return
        if (!info.controlEnabled) return

        sessionStore.save(session.copy(host = newIp, port = newPort))
    }

    private companion object {
        /** 断开后先等这段时间再判定为端口漂移，给 WS 常规重连留机会 */
        const val RESCUE_INTERVAL_MS = 6_000L
        const val SCAN_TIMEOUT_MS = 2_500L
    }
}
