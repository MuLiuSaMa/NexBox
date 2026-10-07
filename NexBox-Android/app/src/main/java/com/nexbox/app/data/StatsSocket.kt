package com.nexbox.app.data

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.isActive
import kotlinx.serialization.json.JsonElement
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener

/** WS 事件。PC 端每 ~1s 推一帧 stats，动作执行完广播 action.done */
sealed interface WsEvent {
    /** 硬件快照：payload 已剔除 meta */
    data class Stats(val payload: Map<String, JsonElement>, val meta: WsMeta?) : WsEvent

    /** 有动作在 PC 端执行完成，多端状态同步用（可据此重拉 query） */
    data class ActionDone(val key: String) : WsEvent

    /** PC 端文件互传列表有变化（添加/删除/已接收回执），触发本机刷新互传列表 */
    data object TransferUpdate : WsEvent


    data object Connected : WsEvent

    data class Disconnected(val reason: String?) : WsEvent

    /** 令牌失效：调用方应清会话并退回连接页 */
    data object Unauthorized : WsEvent

    data class Unknown(val type: String) : WsEvent
}

/**
 * WebSocket 客户端：`ws://{ip}:{port}/api/ws?token={token}`。
 * 断线按 1s → 2s → 5s 指数退避重连；令牌失效则不再重连，交由上层处理。
 */
class StatsSocket(private val sessionStore: SessionStore) {


    fun events(): Flow<WsEvent> = flow {
        var attempt = 0
        var unauthorized = false

        while (currentCoroutineContext().isActive && !unauthorized) {
            val session = sessionStore.current() ?: break

            var connectedOnce = false
            var failure: Throwable? = null
            try {
                connectOnce(session).collect { event ->
                    if (event is WsEvent.Connected) {
                        connectedOnce = true
                        attempt = 0
                    }
                    if (event is WsEvent.Unauthorized) unauthorized = true
                    emit(event)
                }
            } catch (e: CancellationException) {
                throw e
            } catch (t: Throwable) {
                failure = t
            }

            if (unauthorized || !currentCoroutineContext().isActive) break

            val wait = BACKOFF_MS[attempt.coerceAtMost(BACKOFF_MS.lastIndex)]
            attempt++
            val head = failure?.message ?: if (connectedOnce) "连接已断开" else "无法建立连接"
            emit(WsEvent.Disconnected("$head，${wait / 1000}s 后重试"))
            delay(wait)
        }
    }

    private fun connectOnce(session: Session): Flow<WsEvent> = callbackFlow {
        val request = Request.Builder().url(session.wsUrl).build()

        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {                trySend(WsEvent.Connected)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                trySend(parse(text))
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                channel.close()
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (response?.code == 401) trySend(WsEvent.Unauthorized)
                channel.close(t)
            }
        }

        val socket = NetworkModule.socketClient.newWebSocket(request, listener)
        awaitClose { socket.cancel() }
    }

    private fun parse(text: String): WsEvent {
        val frame = runCatching {
            NetworkModule.json.decodeFromString(WsFrame.serializer(), text)
        }.getOrNull() ?: return WsEvent.Unknown("无法解析的帧")

        return when (frame.type) {
            "stats" -> WsEvent.Stats(frame.statsPayload(), frame.statsMeta())
            "action.done" -> WsEvent.ActionDone(frame.key.orEmpty())
            "transfer.update" -> WsEvent.TransferUpdate
            else -> WsEvent.Unknown(frame.type)
        }
    }

    private companion object {
        val BACKOFF_MS = longArrayOf(1_000, 2_000, 5_000)
    }
}
