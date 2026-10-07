package com.nexbox.app.data

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

/**
 * 实时数据总闸。
 *
 * [StatsSocket.events] 是冷流：每被 collect 一次就会新建一条 WebSocket。
 * 连接页与硬件面板都要用 stats，若各自订阅就会开出两条长连接（PC 端也会多记一台设备），
 * 所以这里把它收敛成进程内唯一的一份：内部只跑一条连接，对外广播 [events]，
 * 并把面板要的硬件快照解析成 [hardware]，省掉一轮 REST 轮询。
 */
class StatsRepository(
    private val socket: StatsSocket,
    private val sessionStore: SessionStore,
) {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /**
     * replay = 0：不要回放历史事件。
     * 否则解绑换机后，新订阅者会立刻收到上一台机器的 `Connected`，误判成「已连接」。
     * 需要「当前状态」的消费方读 [hardware] 这类 StateFlow，而不是靠事件回放。
     */
    private val _events = MutableSharedFlow<WsEvent>(
        replay = 0,
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val events: SharedFlow<WsEvent> = _events.asSharedFlow()

    private val _hardware = MutableStateFlow<HardwareSnapshot?>(null)

    /** PC 端每 ~1s 推一帧，直接驱动面板，不必再轮询 */
    val hardware: StateFlow<HardwareSnapshot?> = _hardware.asStateFlow()

    private val _connected = MutableStateFlow(false)

    /**
     * 实时通道是否已建立。
     *
     * 之所以用 StateFlow 而不是只发 `WsEvent.Connected` 事件：本仓库在 App 启动时就建连，
     * 而 UI 要晚一步才订阅；只靠事件的话，UI 会错过那次 `Connected`，界面永远停在「未连接」。
     * StateFlow 会把当前值补发给后到的订阅者，从根上避免这个竞态。
     */
    val connected: StateFlow<Boolean> = _connected.asStateFlow()

    private var running = false

    /** 幂等启动：重复调用不会开出第二条连接 */
    @Synchronized
    fun start() {
        if (running) return
        running = true

        scope.launch {
            // 必须用 collectLatest：socket.events() 是永不结束的无限重连流，
            // 普通 collect 会被它永久阻塞在内层，导致换端口/重配后外层收不到新 session、
            // WS 卡在旧连接上不再重建（表现为功能页 REST 正常、主页一直「正在建立实时数据通道」）。
            // collectLatest 在 session 变化时取消上一轮连接并用新会话立即重建。
            sessionStore.session.distinctUntilChanged().collectLatest { session ->
                // 换设备/解绑后旧机器的数据必须立刻清掉，否则面板会显示上一台的读数
                _hardware.value = null
                _connected.value = false
                if (session == null) return@collectLatest

                socket.events()
                    .catch { /* events() 内部已做退避重连，这里只防未捕获异常打断整个 pump */ }
                    .collect { event ->
                        when (event) {
                            is WsEvent.Connected -> _connected.value = true
                            is WsEvent.Disconnected, is WsEvent.Unauthorized -> _connected.value = false
                            is WsEvent.Stats -> _hardware.value = HardwareSnapshot.from(event.payload)
                            is WsEvent.ActionDone, is WsEvent.TransferUpdate, is WsEvent.Unknown -> Unit
                        }
                        _events.tryEmit(event)
                    }
            }
        }
    }
}
