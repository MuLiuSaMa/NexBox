package com.nexbox.app.data

import android.os.SystemClock
import java.io.IOException
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import okhttp3.Call
import okhttp3.Callback
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response

/**
 * 三角洲行动：每日密码 + 改枪码平台。
 *
 * 与 PC 端 `src/pages/DeltaForcePage.tsx` 打的是同一批公网接口，字段按那边已验证过的
 * 响应结构对齐，多余字段（`icon`、`reported`）由 `ignoreUnknownKeys` 忽略。
 * 数据全部来自站外，不走 [NexBoxClient] 的鉴权信封 —— 没配对 PC 也能用。
 */
@Serializable
data class DeltaPasswordItem(val name: String = "", val password: String = "")

/** 每日密码的全部地图，顺序即 PC 端/悬浮框的展示顺序；悬浮框的地图选择页按它逐张开关 */
val DELTA_MAPS = listOf("零号大坝", "长弓溪谷", "巴克什", "航天基地", "潮汐监狱", "AZ3")

@Serializable
data class DeltaCategory(
    val id: Int = 0,
    val name: String = "",
    @SerialName("loadout_count") val loadoutCount: Int = 0,
)

@Serializable
data class DeltaWeapon(@SerialName("weapon_name") val weaponName: String = "", val count: Int = 0)

@Serializable
data class DeltaLoadout(
    val id: Int = 0,
    @SerialName("category_id") val categoryId: Int = 0,
    @SerialName("weapon_name") val weaponName: String = "",
    val code: String = "",
    val description: String = "",
    /** 读取用 Double：后端偶尔回小数时不至于整页解析失败，展示时再取整 */
    val cost: Double = 0.0,
    val author: String = "",
    val likes: Int = 0,
    val status: String = "",
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("category_name") val categoryName: String = "",
)

@Serializable
data class DeltaLoadoutPage(
    val data: List<DeltaLoadout> = emptyList(),
    val total: Int = 0,
    val page: Int = 1,
    @SerialName("totalPages") val totalPages: Int = 1,
)

/** 投稿体。作者留空由服务端记为「匿名」，长度上限与 PC 端表单一致 */
@Serializable
data class DeltaLoadoutDraft(
    @SerialName("category_id") val categoryId: Int,
    @SerialName("weapon_name") val weaponName: String,
    val code: String,
    val cost: Long,
    val description: String,
    val author: String,
)

/** 列表查询条件。后端只认这四个参数（PC 端那个 sort 状态是死代码，实测无效） */
data class DeltaLoadoutQuery(
    val categoryId: Int? = null,
    val weapon: String = "",
    val search: String = "",
    val page: Int = 1,
)

@Serializable
private data class DeltaPasswordResponse(val status: String = "", val data: List<DeltaPasswordItem> = emptyList())

@Serializable
private data class DeltaLikeResponse(val id: Int = 0, val likes: Int = 0)

object DeltaForceStore {

    private const val API_BASE = "https://df.nexbox.top"
    private const val PASSWORD_PRIMARY = "https://i.elaina.vin/api/%E4%B8%89%E8%A7%92%E6%B4%B2/%E5%AF%86%E7%A0%81/"
    private const val PASSWORD_BACKUP = "https://api.s0o1.com/API/sjz/mm/"
    private const val ATTEMPTS = 3
    private const val PASSWORD_CACHE_MS = 60_000L
    private val BACKOFF_MS = longArrayOf(500L, 1_500L)
    private val JSON_TYPE = "application/json".toMediaType()
    private val CATEGORY_LIST = ListSerializer(DeltaCategory.serializer())
    private val WEAPON_LIST = ListSerializer(DeltaWeapon.serializer())

    /** 备用密码接口是纯文本，逐行配对：`1. 地图名` 起头，下一行 `每日密码：xxxx` */
    private val MAP_LINE = Regex("""^\s*\d+\.\s*(.+?)\s*$""")
    private val PASSWORD_LINE = Regex("""每日密码\s*[:：]\s*(\S+)""")

    @Volatile
    private var cachedPasswords: List<DeltaPasswordItem> = emptyList()

    @Volatile
    private var passwordCachedAt = 0L

    /**
     * 每日密码。主接口失败或返回空就退到备用文本接口 —— 与 PC 端 Rust 的
     * `fetch_delta_passwords_from_api` 同一条链路，两端看到的密码始终一致。
     */
    suspend fun fetchPasswords(force: Boolean = false): List<DeltaPasswordItem> {
        if (!force && cachedPasswords.isNotEmpty() &&
            SystemClock.elapsedRealtime() - passwordCachedAt < PASSWORD_CACHE_MS
        ) {
            return cachedPasswords
        }
        var last: Throwable? = null
        repeat(ATTEMPTS) { attempt ->
            try {
                val items = primaryPasswords().ifEmpty { backupPasswords() }
                if (items.isEmpty()) throw IOException("密码接口暂时没有返回数据")
                cachedPasswords = items
                passwordCachedAt = SystemClock.elapsedRealtime()
                return items
            } catch (e: Throwable) {
                if (e is CancellationException) throw e
                last = e
                // 主备各算一次尝试；只有网关抖动值得再退避重跑
                if (e is DeltaApiException && !e.retryable) throw e
                if (attempt < ATTEMPTS - 1) delay(BACKOFF_MS[attempt.coerceAtMost(BACKOFF_MS.lastIndex)])
            }
        }
        throw last ?: IOException("无法获取每日密码")
    }

    suspend fun fetchCategories(): List<DeltaCategory> = decode(CATEGORY_LIST, "$API_BASE/api/categories")

    suspend fun fetchWeapons(categoryId: Int): List<DeltaWeapon> =
        decode(WEAPON_LIST, "$API_BASE/api/weapons/$categoryId")

    suspend fun fetchLoadouts(query: DeltaLoadoutQuery): DeltaLoadoutPage =
        decode(DeltaLoadoutPage.serializer(), loadoutUrl(query).toString())

    /** 点赞，回服务端最新的点赞数 */
    suspend fun like(loadoutId: Int): Int {
        val body = postEmpty("/api/loadouts/$loadoutId/like")
        return NetworkModule.json.decodeFromString(DeltaLikeResponse.serializer(), body).likes
    }

    suspend fun report(loadoutId: Int) {
        postEmpty("/api/loadouts/$loadoutId/report")
    }

    suspend fun submit(draft: DeltaLoadoutDraft) {
        val body = NetworkModule.json.encodeToString(DeltaLoadoutDraft.serializer(), draft)
        execute(
            Request.Builder()
                .url("$API_BASE/api/loadouts")
                .post(body.toRequestBody(JSON_TYPE))
                .build(),
        )
    }

    // ───────────────────────── 内部 ─────────────────────────

    /**
     * 主接口。挂了或格式不对时返回空列表而不是抛出去 —— Rust 那边就是 `Option`，
     * 主接口一挂就换备用文本接口；这里直接抛的话备胎永远没机会上场。
     */
    private suspend fun primaryPasswords(): List<DeltaPasswordItem> = try {
        val text = execute(Request.Builder().url(PASSWORD_PRIMARY).build())
        val parsed = decodeText(DeltaPasswordResponse.serializer(), text)
        if (parsed.status == "success") parsed.data else emptyList()
    } catch (e: CancellationException) {
        throw e
    }

    private suspend fun backupPasswords(): List<DeltaPasswordItem> =
        parseBackupPasswords(execute(Request.Builder().url(PASSWORD_BACKUP).build()))

    private fun parseBackupPasswords(text: String): List<DeltaPasswordItem> {
        val items = mutableListOf<DeltaPasswordItem>()
        var name: String? = null
        var password = ""

        fun flush() {
            val mapName = name
            if (mapName != null && password.isNotEmpty()) items += DeltaPasswordItem(mapName, password)
            password = ""
        }

        for (line in text.lineSequence()) {
            val mapMatch = MAP_LINE.matchEntire(line)
            if (mapMatch != null) {
                flush()
                name = mapMatch.groupValues[1]
                continue
            }
            if (password.isEmpty()) {
                PASSWORD_LINE.find(line)?.groupValues?.getOrNull(1)?.let { password = it }
            }
        }
        flush()
        return items
    }

    private fun loadoutUrl(query: DeltaLoadoutQuery): HttpUrl {
        val builder = "$API_BASE/api/loadouts".toHttpUrl().newBuilder()
        // 中文武器名与关键字必须走 addQueryParameter 编码，拼字符串会直接 400
        query.categoryId?.let { builder.addQueryParameter("category_id", it.toString()) }
        if (query.weapon.isNotBlank()) builder.addQueryParameter("weapon_name", query.weapon)
        if (query.search.isNotBlank()) builder.addQueryParameter("search", query.search.trim())
        builder.addQueryParameter("page", query.page.toString())
        return builder.build()
    }

    private suspend fun <T> decode(serializer: DeserializationStrategy<T>, url: String): T =
        decodeText(serializer, execute(Request.Builder().url(url).build()))

    private fun <T> decodeText(serializer: DeserializationStrategy<T>, text: String): T = try {
        NetworkModule.json.decodeFromString(serializer, text)
    } catch (e: Exception) {
        // 网关挂了会回一整个 HTML 页，把序列化异常原样抛出去用户根本看不懂
        throw IOException("接口返回格式异常，请稍后重试")
    }

    /** 与 PC 端 `fetch(path, { method: "POST" })` 对齐：不带 body，也不带 Content-Type */
    private suspend fun postEmpty(path: String): String = execute(
        Request.Builder()
            .url(API_BASE + path)
            .post(ByteArray(0).toRequestBody(null))
            .build(),
    )

    /**
     * 异步发请求，协程取消时同步取消底层 [Call]（对应 PC 端的 AbortController）。
     * 已取消的调用不再 resume，免得把「用户已经不想要这个结果」当成错误冒给上层。
     */
    private suspend fun execute(request: Request): String = suspendCancellableCoroutine { cont ->
        val call = NetworkModule.client.newCall(request)
        cont.invokeOnCancellation { call.cancel() }
        call.enqueue(
            object : Callback {
                override fun onFailure(call: Call, e: IOException) {
                    if (cont.isActive) cont.resumeWithException(e)
                }

                override fun onResponse(call: Call, response: Response) {
                    response.use { resp ->
                        if (!resp.isSuccessful) {
                            if (cont.isActive) cont.resumeWithException(DeltaApiException(resp.code))
                            return
                        }
                        val body = runCatching { resp.body?.string() }.getOrNull()
                        if (body == null) {
                            if (cont.isActive) cont.resumeWithException(IOException("接口返回内容为空"))
                        } else if (cont.isActive) {
                            cont.resume(body)
                        }
                    }
                }
            },
        )
    }
}

/**
 * 三角洲接口的 HTTP 错误。5xx / 429 归为抖动（[DeltaForceStore] 会自动退避重跑），
 * 其余状态码重试也是白搭，直接把文案交给 UI。
 */
class DeltaApiException(val httpCode: Int) : IOException(
    if (httpCode in 500..599 || httpCode == 429) {
        "服务暂时不可用（HTTP $httpCode），稍后自动重试"
    } else {
        "服务返回异常（HTTP $httpCode）"
    },
) {
    val retryable: Boolean get() = httpCode in 500..599 || httpCode == 429
}
