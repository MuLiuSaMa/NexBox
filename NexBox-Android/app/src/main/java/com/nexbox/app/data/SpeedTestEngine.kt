package com.nexbox.app.data

import android.util.Log
import androidx.compose.runtime.Immutable
import java.io.IOException
import java.net.Inet6Address
import java.net.InetAddress
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicLong
import kotlin.math.sqrt
import kotlin.random.Random
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.channelFlow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.joinAll
import kotlinx.coroutines.launch
import okhttp3.Call
import okhttp3.ConnectionPool
import okhttp3.Dns
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody

/**
 * 本机网络测速（纯本地实现，不依赖 PC 端）。
 *
 * 算法与指标逐字对齐 PC 端 `src-tauri/src/speedtest.rs`：
 * 延迟/抖动/丢包 → 下载 → 上传，三个阶段共约 15 秒。
 * 测的是**这台安卓设备自己的网络**，与电脑无关。
 */

/** 测速阶段 */
enum class SpeedStage { IDLE, PING, DOWNLOAD, UPLOAD, DONE }

/**
 * 一帧测速进度。字段与 PC 端的 `SpeedTestProgress` 对齐，
 * 只有总进度 `progress_pct` 没有带过来 —— UI 直接按 [stage] 推（5 / 50 / 90 / 100）。
 */
@Immutable
data class SpeedTestProgress(
    val stage: SpeedStage,
    /** 延迟 ms */
    val pingMs: Double = 0.0,
    /** 抖动 ms（RTT 标准差） */
    val jitterMs: Double = 0.0,
    /** 丢包率 % */
    val packetLossPct: Double = 0.0,
    /** 实时下载速率 Mbps */
    val downloadMbps: Double = 0.0,
    /** 实时上传速率 Mbps */
    val uploadMbps: Double = 0.0,
    val message: String = "",
)

/** 一个测速节点。下载源按顺序轮转，前面的失败就自动切到后面的备用源 */
data class SpeedTestServer(
    val id: String,
    val name: String,
    /** 选择芯片上显示的短名，全称太长放不下 */
    val shortName: String,
    val downloadUrls: List<String>,
    val uploadUrls: List<String>,
    val pingUrls: List<String>,
)

/**
 * 内置测速节点，与 PC 端同一批（纯国内）。
 *
 * 按 PC 端源码注释：浙大节点常返回 502、中科大拒绝非浏览器客户端，
 * 实际上只有南航稳定；上传与延迟也只有南航的 `empty.php` 对外开放，
 * 所以三个节点的 upload/ping 都统一指向南航。
 * 每个节点的下载列表尾部都挂了 https 的大厂 CDN 备用源，保证下载始终可测。
 */
object SpeedTestServers {

    const val DEFAULT_ID = "nuaa"

    val ALL: List<SpeedTestServer> = listOf(
        SpeedTestServer(
            id = "nuaa",
            name = "南京航空航天大学",
            shortName = "南航",
            downloadUrls = listOf(
                "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=50",
                "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=10",
                "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
                "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
                "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=10",
            ),
            uploadUrls = listOf(NUAA_EMPTY),
            pingUrls = listOf(NUAA_EMPTY),
        ),
        SpeedTestServer(
            id = "zju",
            name = "浙江大学",
            shortName = "浙大",
            downloadUrls = listOf(
                "http://speedtest.zju.edu.cn/garbage.php",
                "http://speedtest.zju.edu.cn/1000M",
                "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
                "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
                "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
            ),
            uploadUrls = listOf(NUAA_EMPTY),
            pingUrls = listOf(NUAA_EMPTY),
        ),
        SpeedTestServer(
            id = "ustc",
            name = "中国科学技术大学",
            shortName = "中科大",
            downloadUrls = listOf(
                "https://test.ustc.edu.cn/backend/garbage.php",
                "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
                "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
                "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
            ),
            uploadUrls = listOf(NUAA_EMPTY),
            pingUrls = listOf(NUAA_EMPTY),
        ),
    )

    fun find(id: String): SpeedTestServer = ALL.firstOrNull { it.id == id } ?: ALL.first()
}

private const val NUAA_EMPTY = "http://speed.nuaa.edu.cn/backend/empty.php"

// ───────── 算法参数（与 PC 端逐字对齐，不要随手改） ─────────

/** 延迟测试轮数 */
private const val PING_COUNT = 8
/** 每轮之间的间隔，模拟真实 ping 节奏 */
private const val PING_INTERVAL_MS = 150L
/** 单次延迟请求超时 */
private const val PING_TIMEOUT_SECS = 3L
private const val DEFAULT_THREADS = 16
private const val DEFAULT_DURATION_SECS = 6
/** 采样间隔 */
private const val PROGRESS_INTERVAL_MS = 80L
/** 指数移动平均平滑系数 */
private const val EMA_ALPHA = 0.4
/** 下载预热期：TCP 慢启动期间速率虚高，这段时间既不发前端也不计入最终值 */
private const val WARMUP_MS = 800.0
/** 上传预热期：并发上传会先在本地 TCP 缓冲区堆积，需要更长时间才回落 */
private const val UPLOAD_WARMUP_MS = 1200.0
/** 上传单次请求的数据块大小 */
private const val UPLOAD_CHUNK_SIZE = 256 * 1024
private const val UPLOAD_TIMEOUT_SECS = 5L
private const val DOWNLOAD_TIMEOUT_SECS = 8L
/** 单个下载源失败后，等一小会儿再换下一个源重试 */
private const val FAILOVER_DELAY_MS = 300L
private const val READ_BUFFER_SIZE = 64 * 1024
private const val UPLOAD_MEDIA_TYPE = "application/octet-stream"

/** 记录失败原因用的阶段键 */
private const val STAGE_DOWNLOAD = "download"
private const val STAGE_UPLOAD = "upload"

/** 诊断日志 TAG，排查「没有测到数据」时用 `adb logcat -s NexBoxSpeed` */
private const val TAG = "NexBoxSpeed"

/**
 * 解析域名时把 IPv6 排在前面。
 *
 * 这是踩了很久的坑：南航 `speed.nuaa.edu.cn` 的 **IPv4 入口（202.119.64.6）是一台 nginx**，
 * 它把 `empty.php` / `garbage.php` 当**静态文件**返回 —— 状态码 200 但内容是 449 字节的源码
 * （带 ETag / Last-Modified），POST 更是直接 405 Method Not Allowed。
 * 结果就是：上传速率恒为 0，下载也只能靠 CDN 备用源撑着。
 * 真正执行 PHP 的测速后端只挂在 **IPv6 入口**（`curl -6` 实测 200 / Apache）。
 *
 * 所以这里把 IPv6 排前面。网络不支持 IPv6 时 OkHttp 会自己回退到 IPv4，不会更差。
 */
private val preferIpv6Dns = object : Dns {
    override fun lookup(hostname: String): List<InetAddress> =
        Dns.SYSTEM.lookup(hostname).sortedBy { if (it is Inet6Address) 0 else 1 }
}

/**
 * 测速专用 OkHttpClient。
 *
 * 不复用 `NetworkModule.client`：那是给 PC 端 REST 用的（read 15s、默认 5 个空闲连接），
 * 对 16 并发的吞吐测试偏紧。这里把连接池放大、整体超时关掉（总时长由引擎自己按阶段算），
 * 单请求超时在各阶段用 `call.timeout()` 临时设。
 */
private val speedTestClient: OkHttpClient by lazy {
    OkHttpClient.Builder()
        .connectTimeout(5, TimeUnit.SECONDS)
        .readTimeout(10, TimeUnit.SECONDS)
        .writeTimeout(10, TimeUnit.SECONDS)
        // 关掉整体调用超时：下载/上传的时长由 deadline 控制，不能在这里被砍
        .callTimeout(0, TimeUnit.MILLISECONDS)
        .connectionPool(ConnectionPool(16, 5, TimeUnit.MINUTES))
        .dns(preferIpv6Dns)
        // 统一禁用压缩。
        // OkHttp 默认给每个请求带 `Accept-Encoding: gzip`，而测速节点对压缩请求
        // 可能返回解不完的响应；顺带这也让「按线上字节计数」成立。
        .addInterceptor { chain ->
            val request = chain.request().newBuilder()
                .header("Accept-Encoding", "identity")
                .build()
            chain.proceed(request)
        }
        .build()
}

/**
 * 一次测速的会话，负责「能被立刻叫停」。
 *
 * 这是 Kotlin 侧的坑：协程的 `cancel()` **打断不了阻塞在 `Call.execute()` 上的线程**，
 * 必须显式 `call.cancel()`，否则点停止之后 IO 线程要一直等到读超时才释放。
 * 所以所有在飞的请求都登记进来，取消时逐个 cancel。
 */
private class SpeedTestSession {

    @Volatile
    private var stopped = false

    private val calls: MutableSet<Call> =
        Collections.newSetFromMap(ConcurrentHashMap<Call, Boolean>())

    /** 各阶段最后一次失败原因，用于跑完后在页面上说明「为什么没数据」 */
    private val errors = ConcurrentHashMap<String, String>()

    /** 用户主动停止（或页面退出）：worker 循环条件会立刻失败 */
    val isStopped: Boolean get() = stopped

    fun noteError(stage: String, message: String) {
        errors[stage] = message
    }

    fun errorOf(stage: String): String? = errors[stage]

    fun register(call: Call) {
        // 已经在停的过程中还来注册，直接掐掉，避免漏网请求继续跑
        if (stopped) call.cancel() else calls.add(call)
    }

    fun unregister(call: Call) {
        calls.remove(call)
    }

    /**
     * 只中断在飞请求、不置 [isStopped]。
     * 阶段自然结束（deadline 到了）时用它，让还卡在 read 上的 worker 立刻退出，
     * 不至于把总时长拖到读超时。用户取消请用 [stop]。
     */
    fun cancelInFlight() {
        calls.forEach { runCatching { it.cancel() } }
        calls.clear()
    }

    /** 用户取消：置位 + 掐掉所有在飞请求 */
    fun stop() {
        stopped = true
        cancelInFlight()
    }
}

object SpeedTestEngine {

    /**
     * 跑一次完整测速。返回的 Flow 边测边吐进度，取消收集即中止测速。
     *
     * 用 `channelFlow` + CONFLATED：80ms 一帧，UI 跟不上就丢旧帧，不让采样被反压。
     */
    fun start(
        serverId: String = SpeedTestServers.DEFAULT_ID,
        threads: Int = DEFAULT_THREADS,
        durationSecs: Int = DEFAULT_DURATION_SECS,
    ): Flow<SpeedTestProgress> = channelFlow {
        val server = SpeedTestServers.find(serverId)
        val session = SpeedTestSession()
        val durationMs = durationSecs * 1000L
        Log.d(TAG, "开始测速 server=${server.id} threads=$threads duration=${durationSecs}s")
        try {
            // ① 延迟 / 抖动 / 丢包
            send(SpeedTestProgress(SpeedStage.PING, message = "正在测试延迟…"))
            val ping = pingPhase(server, session)
            Log.d(TAG, "延迟完成 ping=${ping.ping}ms jitter=${ping.jitter} loss=${ping.loss}%")
            if (session.isStopped) return@channelFlow

            // ② 下载
            val afterPing = SpeedTestProgress(
                stage = SpeedStage.DOWNLOAD,
                pingMs = ping.ping,
                jitterMs = ping.jitter,
                packetLossPct = ping.loss,
                message = "正在测试下载…",
            )
            send(afterPing)
            val download = downloadPhase(server, session, threads, durationMs, afterPing)
            Log.d(TAG, "下载完成 ${download} Mbps（${session.errorOf(STAGE_DOWNLOAD) ?: "无错误"}）")
            if (session.isStopped) return@channelFlow

            // ③ 上传
            val afterDownload = afterPing.copy(
                stage = SpeedStage.UPLOAD,
                downloadMbps = download,
                message = "正在测试上传…",
            )
            send(afterDownload)
            val upload = uploadPhase(server, session, threads, durationMs, afterDownload)
            Log.d(TAG, "上传完成 ${upload} Mbps（${session.errorOf(STAGE_UPLOAD) ?: "无错误"}）")

            send(
                afterDownload.copy(
                    stage = SpeedStage.DONE,
                    uploadMbps = upload,
                    message = doneMessage(download, upload, session),
                ),
            )
        } finally {
            // 收集方取消（点停止 / 退出页面 / VM 清理）时的兜底，保证 IO 线程立刻释放
            session.stop()
        }
    }
        .buffer(Channel.CONFLATED)
        .flowOn(Dispatchers.IO)

    /**
     * 跑完后的说明文字：哪一项没拿到数据就把**具体原因**带上，
     * 免得用户对着 0 猜是「没测」还是「测不到」。全部正常时返回空串，页面不显示。
     */
    private fun doneMessage(
        download: Double,
        upload: Double,
        session: SpeedTestSession,
    ): String {
        val problems = buildList {
            if (download <= 0.0) {
                add("下载（${session.errorOf(STAGE_DOWNLOAD) ?: "节点没有响应"}）")
            }
            if (upload <= 0.0) {
                add("上传（${session.errorOf(STAGE_UPLOAD) ?: "节点没有响应"}）")
            }
        }
        return if (problems.isEmpty()) "" else "${problems.joinToString("、")}没有拿到数据"
    }

    // ───────────────────────── 延迟 / 抖动 / 丢包 ─────────────────────────

    private data class PingResult(val ping: Double, val jitter: Double, val loss: Double)

    /**
     * 对 `empty.php` 发 HTTP GET 测 RTT（不是 ICMP）。
     * 每轮取多个 URL 里的最小 RTT，8 轮后去掉前后各 25% 求平均 = 延迟，
     * 抖动取有效样本的标准差，丢包率 = 失败请求数 / 总请求数。
     */
    private suspend fun ProducerScope<SpeedTestProgress>.pingPhase(
        server: SpeedTestServer,
        session: SpeedTestSession,
    ): PingResult {
        val rtts = ArrayList<Double>()
        var failures = 0

        repeat(PING_COUNT) {
            if (session.isStopped) return@repeat
            var minRtt = Double.MAX_VALUE
            for (url in server.pingUrls) {
                if (session.isStopped) break
                val begin = System.nanoTime()
                val ok = runCatching { pingOnce(url, session) }.getOrDefault(false)
                if (ok) {
                    minRtt = minOf(minRtt, (System.nanoTime() - begin) / 1_000_000.0)
                } else {
                    failures++
                }
            }
            if (minRtt != Double.MAX_VALUE) rtts.add(minRtt)
            delay(PING_INTERVAL_MS)
            // 每轮推一次，让阶段徽章有「正在测」的反馈
            send(SpeedTestProgress(SpeedStage.PING, message = "正在测试延迟…"))
        }

        val total = rtts.size + failures
        val loss = if (total > 0) failures.toDouble() / total * 100.0 else 0.0
        // 一个都没成功：不冒充 0，交给上层显示「—」
        if (rtts.isEmpty()) return PingResult(0.0, 0.0, loss)

        val sorted = rtts.sorted()
        val trim = maxOf(sorted.size / 4, 1)
        val effective =
            if (sorted.size > trim * 2) sorted.subList(trim, sorted.size - trim) else sorted
        val ping = effective.average()
        val variance = effective.sumOf { (it - ping) * (it - ping) } / effective.size
        return PingResult(ping, sqrt(variance), loss)
    }

    private fun pingOnce(url: String, session: SpeedTestSession): Boolean {
        val call = speedTestClient.newCall(
            Request.Builder()
                .url(url)
                // 显式声明不压缩：否则 OkHttp 透明解压，计时会把解压开销算进 RTT
                .header("Accept-Encoding", "identity")
                .build(),
        )
        call.timeout().timeout(PING_TIMEOUT_SECS, TimeUnit.SECONDS)
        session.register(call)
        return try {
            call.execute().use { resp ->
                // 消费响应体，确保连接真正建立
                resp.body?.bytes()
                resp.isSuccessful
            }
        } finally {
            session.unregister(call)
        }
    }

    // ───────────────────────── 下载 ─────────────────────────

    private suspend fun ProducerScope<SpeedTestProgress>.downloadPhase(
        server: SpeedTestServer,
        session: SpeedTestSession,
        threads: Int,
        durationMs: Long,
        base: SpeedTestProgress,
    ): Double {
        val counter = AtomicLong()
        val deadlineNs = System.nanoTime() + durationMs * 1_000_000

        return coroutineScope {
            val workers = List(threads) { index ->
                launch(Dispatchers.IO) {
                    downloadWorker(server.downloadUrls, deadlineNs, counter, session, index)
                }
            }
            try {
                val mbps = sampleLoop(
                    counter = counter,
                    durationMs = durationMs,
                    warmupMs = WARMUP_MS,
                    stage = SpeedStage.DOWNLOAD,
                    base = base,
                    session = session,
                    writeDownload = true,
                )
                workers.joinAll()
                mbps
            } finally {
                // 兜底：被取消时 sampleLoop 里的收尾走不到（取消是异常路径），
                // 这里再掐一次在飞请求，否则 worker 要一直卡到读超时才释放，
                // 表现为「点了停止，流量还在跑十来秒」
                session.cancelInFlight()
            }
        }
    }

    private suspend fun downloadWorker(
        urls: List<String>,
        deadlineNs: Long,
        counter: AtomicLong,
        session: SpeedTestSession,
        startIndex: Int,
    ) {
        if (urls.isEmpty()) return
        var index = startIndex % urls.size
        val buffer = ByteArray(READ_BUFFER_SIZE)

        while (!session.isStopped && System.nanoTime() < deadlineNs) {
            val call = speedTestClient.newCall(
                Request.Builder()
                    .url(urls[index])
                    .header("Accept-Encoding", "identity")
                    .build(),
            )
            call.timeout().timeout(DOWNLOAD_TIMEOUT_SECS, TimeUnit.SECONDS)
            session.register(call)
            try {
                call.execute().use { resp ->
                    if (!resp.isSuccessful) throw IOException("HTTP ${resp.code}")
                    val source = resp.body?.source() ?: throw IOException("响应体为空")
                    var received = 0L
                    while (!session.isStopped && System.nanoTime() < deadlineNs) {
                        val read = source.read(buffer)
                        if (read <= 0) break
                        received += read
                        counter.addAndGet(read.toLong())
                    }
                    // 返回 200 却一个字节都不给 —— 南航节点会间歇性这样（实测）。
                    // 这种情况必须当成失败去换下一个源，否则 worker 会一直死磕同一个源，
                    // 速率永远是 0，看起来就像「没有测到数据」。
                    // 注意：deadline 到了导致的 0 字节不算失败，那是正常收尾。
                    if (received == 0L && !session.isStopped && System.nanoTime() < deadlineNs) {
                        throw IOException("节点返回空内容")
                    }
                }
            } catch (e: IOException) {
                session.noteError(STAGE_DOWNLOAD, e.message ?: "请求失败")
                Log.w(TAG, "下载源失败 ${urls[index]} -> ${e.message}")
                // 单个源失败：换下一个源（尾部就是 https 的 CDN 备用源），稍等再试
                if (!session.isStopped && System.nanoTime() < deadlineNs) {
                    index = (index + 1) % urls.size
                    delay(FAILOVER_DELAY_MS)
                }
            } finally {
                session.unregister(call)
            }
        }
    }

    // ───────────────────────── 上传 ─────────────────────────

    /**
     * 256KB 随机数据块，全局只生成一次并复用。
     * RequestBody 只读它、可以被 16 个 worker 同时 POST，
     * 避免 PC 端那种「每轮分配一块」在手机上反复触发 GC。
     */
    private val uploadChunk: ByteArray by lazy {
        ByteArray(UPLOAD_CHUNK_SIZE).also { Random.nextBytes(it) }
    }

    private suspend fun ProducerScope<SpeedTestProgress>.uploadPhase(
        server: SpeedTestServer,
        session: SpeedTestSession,
        threads: Int,
        durationMs: Long,
        base: SpeedTestProgress,
    ): Double {
        val counter = AtomicLong()
        val deadlineNs = System.nanoTime() + durationMs * 1_000_000

        return coroutineScope {
            val workers = List(threads) { index ->
                launch(Dispatchers.IO) {
                    uploadWorker(server.uploadUrls, deadlineNs, counter, session, index)
                }
            }
            try {
                val mbps = sampleLoop(
                    counter = counter,
                    durationMs = durationMs,
                    warmupMs = UPLOAD_WARMUP_MS,
                    stage = SpeedStage.UPLOAD,
                    base = base,
                    session = session,
                    writeDownload = false,
                )
                workers.joinAll()
                mbps
            } finally {
                // 同下载阶段：取消路径下的兜底，别让 worker 卡在读超时上
                session.cancelInFlight()
            }
        }
    }

    private suspend fun uploadWorker(
        urls: List<String>,
        deadlineNs: Long,
        counter: AtomicLong,
        session: SpeedTestSession,
        startIndex: Int,
    ) {
        if (urls.isEmpty()) return
        val url = urls[startIndex % urls.size]
        val body: RequestBody = uploadChunk.toRequestBody(UPLOAD_MEDIA_TYPE.toMediaType())

        while (!session.isStopped && System.nanoTime() < deadlineNs) {
            val call = speedTestClient.newCall(
                Request.Builder().url(url).post(body).build(),
            )
            call.timeout().timeout(UPLOAD_TIMEOUT_SECS, TimeUnit.SECONDS)
            session.register(call)
            val t0 = System.nanoTime()
            try {
                val response = call.execute()
                val code = response.code
                val ok = response.use { it.isSuccessful }
                val ms = (System.nanoTime() - t0) / 1_000_000
                // 只有请求成功（数据真的发出去了）才计数，
                // 否则服务器不可达时会把本地缓冲区里从未发出的数据也算进去
                if (ok) {
                    counter.addAndGet(UPLOAD_CHUNK_SIZE.toLong())
                } else {
                    session.noteError(STAGE_UPLOAD, "服务器返回 HTTP $code")
                    Log.w(TAG, "上传非2xx code=$code ${ms}ms")
                }
            } catch (e: IOException) {
                // 这一轮不计入；记下原因，跑完一个字节都没有时给用户看
                session.noteError(STAGE_UPLOAD, e.message ?: "请求失败")
                Log.w(TAG, "上传异常 ${(System.nanoTime() - t0) / 1_000_000}ms -> ${e.message}")
            } finally {
                session.unregister(call)
            }
        }
    }

    // ───────────────────────── 采样（下载 / 上传共用） ─────────────────────────

    /**
     * 每 [PROGRESS_INTERVAL_MS] 采一次共享计数，算瞬时速率并做 EMA 平滑；
     * 预热期内的样本既不发前端也不计入最终值。返回本阶段的最终速率（Mbps）。
     */
    private suspend fun ProducerScope<SpeedTestProgress>.sampleLoop(
        counter: AtomicLong,
        durationMs: Long,
        warmupMs: Double,
        stage: SpeedStage,
        base: SpeedTestProgress,
        session: SpeedTestSession,
        writeDownload: Boolean,
    ): Double {
        val startNs = System.nanoTime()
        var lastBytes = 0L
        var lastElapsed = 0.0
        var smooth = 0.0
        // 进入稳定期那一刻的字节基线，用于最终值只统计稳定期
        var stableBase: Long? = null

        while (!session.isStopped) {
            if ((System.nanoTime() - startNs) / 1e6 >= durationMs) break
            delay(PROGRESS_INTERVAL_MS)

            val current = counter.get()
            val elapsed = (System.nanoTime() - startNs) / 1e9
            val deltaSecs = elapsed - lastElapsed
            if (deltaSecs > 0) {
                val instantMbps = (current - lastBytes) * 8.0 / 1_000_000.0 / deltaSecs
                val elapsedMs = elapsed * 1000.0
                if (elapsedMs >= warmupMs && stableBase == null) stableBase = lastBytes
                smooth =
                    if (smooth <= 0.0) instantMbps
                    else smooth * (1 - EMA_ALPHA) + instantMbps * EMA_ALPHA
                if (elapsedMs >= warmupMs) {
                    send(
                        base.copy(
                            stage = stage,
                            downloadMbps = if (writeDownload) smooth else base.downloadMbps,
                            uploadMbps = if (writeDownload) base.uploadMbps else smooth,
                        ),
                    )
                }
            }
            lastBytes = current
            lastElapsed = elapsed
        }

        // 阶段收尾：掐掉还卡在 read 上的请求，worker 立刻退出，不把总时长拖到读超时
        session.cancelInFlight()

        val elapsed = (System.nanoTime() - startNs) / 1e9
        val totalBytes = counter.get()
        Log.d(TAG, "$stage 采样结束 用时=${elapsed}s 累计字节=$totalBytes 稳定基线=$stableBase")
        val baseBytes = stableBase
        if (baseBytes != null) {
            val stableSecs = (elapsed - warmupMs / 1000.0).coerceAtLeast(0.1)
            val stableBytes = totalBytes - baseBytes
            if (stableBytes > 0) return stableBytes * 8.0 / 1_000_000.0 / stableSecs
        }
        // 兜底：全时段平均
        if (elapsed <= 0.0) return 0.0
        return totalBytes * 8.0 / 1_000_000.0 / elapsed
    }
}
