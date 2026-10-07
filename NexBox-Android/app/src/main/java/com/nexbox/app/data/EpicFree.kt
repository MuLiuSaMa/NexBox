package com.nexbox.app.data

import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import okhttp3.Request

/**
 * Epic 限免游戏（「喜加一」）。与 PC 端 `EpicFreePage` 用同一个聚合接口，
 * 字段按那边已验证过的响应结构对齐；多余字段由 `ignoreUnknownKeys` 忽略。
 */
@Serializable
data class EpicGame(
    val id: String = "",
    val title: String = "",
    /** 封面图地址，交给 Coil 加载 */
    val cover: String = "",
    val description: String = "",
    @SerialName("original_price_desc") val originalPriceDesc: String = "",
    @SerialName("is_free_now") val isFreeNow: Boolean = false,
    /** 免费截止日期的可读文本（接口已格式化好，不再自己拼时区） */
    @SerialName("free_end") val freeEnd: String = "",
    /** 领取跳转地址 */
    val link: String = "",
)

@Serializable
private data class EpicResponse(
    val code: Int? = null,
    val message: String = "",
    val data: List<EpicGame> = emptyList(),
)

/**
 * 站外公开接口，和 PC 端无关：不配对也能用，所以不走 [NexBoxClient] 的鉴权信封。
 * 复用同一份 OkHttpClient / Json 配置，避免再开一个连接池。
 */
object EpicFreeStore {

    private const val API = "https://uapis.cn/api/v1/game/epic-free"
    private const val ATTEMPTS = 3
    private val BACKOFF_MS = longArrayOf(500L, 1_500L)

    /**
     * 拉取限免列表。`uapis.cn` 挂在 Cloudflare 后面，520/521/522 这类网关错误实测
     * 隔几百毫秒再打一次常常就好，所以只对可重试错误退避重跑；其余错误直接抛出。
     */
    suspend fun fetch(): List<EpicGame> = withContext(Dispatchers.IO) {
        var last: Throwable? = null
        repeat(ATTEMPTS) { attempt ->
            try {
                return@withContext requestOnce()
            } catch (e: Throwable) {
                last = e
                if (e !is EpicApiException || !e.retryable) throw e
                if (attempt < ATTEMPTS - 1) {
                    Thread.sleep(BACKOFF_MS[attempt.coerceAtMost(BACKOFF_MS.lastIndex)])
                }
            }
        }
        throw last ?: IOException("获取限免列表失败")
    }

    private fun requestOnce(): List<EpicGame> {
        val request = Request.Builder().url(API).build()
        return NetworkModule.client.newCall(request).execute().use { response ->
            if (!response.isSuccessful) throw EpicApiException(response.code)
            val body = response.body?.string() ?: throw IOException("返回内容为空")
            val parsed = NetworkModule.json.decodeFromString(EpicResponse.serializer(), body)
            // 只留真正限免中的项：个别数据源会把已结束的活动一起带回来
            parsed.data.filter { it.title.isNotBlank() && it.isFreeNow }
        }
    }
}

/**
 * 限免接口的 HTTP 错误。文案不写「接口返回」这种术语，并明确告诉用户会自行恢复；
 * 5xx / 429 归为抖动（重试有意义），其余状态码重试也是白搭。
 */
class EpicApiException(val httpCode: Int) : IOException(
    if (httpCode in 500..599 || httpCode == 429) {
        "限免接口暂时不可用（HTTP $httpCode），稍后自动重试"
    } else {
        "限免接口返回异常（HTTP $httpCode）"
    },
) {
    val retryable: Boolean get() = httpCode in 500..599 || httpCode == 429
}
