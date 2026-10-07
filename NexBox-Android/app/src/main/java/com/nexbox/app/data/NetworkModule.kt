package com.nexbox.app.data

import android.content.Context
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit

/**
 * 轻量单例容器（方案明确不引 Hilt/Koin）：
 * 全局一份 Json / OkHttpClient，以及依赖 [SessionStore] 的 API 客户端。
 */
object NetworkModule {

    val json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = false
        isLenient = true
    }

    lateinit var session: SessionStore
        private set

    lateinit var appContext: Context
        private set

    /** REST：连接/读超时短一些，配对失败要快速反馈 */
    val client: OkHttpClient by lazy {
        OkHttpClient.Builder()
            .connectTimeout(5, TimeUnit.SECONDS)
            .readTimeout(15, TimeUnit.SECONDS)
            .writeTimeout(15, TimeUnit.SECONDS)
            .build()
    }

    /** WebSocket：长连接，读超时置 0，靠 ping 保活 */
    val socketClient: OkHttpClient by lazy {
        client.newBuilder()
            .readTimeout(0, TimeUnit.MILLISECONDS)
            .pingInterval(20, TimeUnit.SECONDS)
            .build()
    }

    /**
     * 文件互传：传输期间单个 socket 读写可能远超普通请求，放宽超时；
     * 读超时仍保留 —— 内网里持续传输的流不会静默卡死，卡住就该报错。
     */
    val transferClient: OkHttpClient by lazy {
        client.newBuilder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .writeTimeout(60, TimeUnit.SECONDS)
            .build()
    }

    val api: NexBoxClient by lazy { NexBoxClient(session) }
    val stats: StatsSocket by lazy { StatsSocket(session) }
    val discovery: DiscoveryClient by lazy { DiscoveryClient() }


    /** 主页偏好：今日人气 / 公告已读水位 / 重要公告确认 */
    val homePrefs: HomePrefs by lazy { HomePrefs(appContext) }

    /** 全局唯一实时数据源：连接页与硬件面板共用，避免开出两条 WebSocket */
    val statsRepo: StatsRepository by lazy { StatsRepository(stats, session) }

    /** 端口漂移救援：PC 换端口后自动找回新端口、复用 token 重连，不必重新配对 */
    val reconnect: ReconnectCoordinator by lazy {
        ReconnectCoordinator(session, discovery, api, statsRepo.connected)
    }

    fun init(context: Context) {
        appContext = context.applicationContext
        session = SessionStore(appContext)
        // 有会话就常驻保持 WS，这样进主页时硬件面板已经有数据，不用等首帧
        statsRepo.start()
        reconnect.start()
    }
}
