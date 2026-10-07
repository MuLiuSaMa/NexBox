package com.nexbox.app.update

import com.nexbox.app.BuildConfig
import com.nexbox.app.data.NetworkModule
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import okhttp3.Request
import java.io.IOException

/**
 * GitCode 更新源检查，流程对齐 PC 端（update-checker.ts）：
 * - 端点 /releases/latest，只读秘钥走 Authorization 头（不放 URL，避免落进日志），
 *   用途只是抬高匿名请求的限流额度；
 * - GitCode 的 release 资产没有 size 字段、没有 published_at；上传的安装包 type 是
 *   "attach"，平台还会自动附加 4 份源码归档（type "source"），必须筛掉。
 */
object UpdateChecker {

    private const val API_BASE = "https://api.gitcode.com/api/v5"
    private const val OWNER = "MuLiuSaMa"
    private const val REPO = "NexBox-Android-Update"

    /** 只读个人秘钥（项目/仓库都只有只读权限），避免匿名请求被限流 */
    private const val TOKEN = "sR5F7YQRrVckQD_87N_4YP2S"

    @Serializable
    private data class GitCodeAsset(
        val name: String = "",
        val browser_download_url: String = "",
        val type: String = "",
    )

    @Serializable
    private data class GitCodeRelease(
        val tag_name: String = "",
        val name: String = "",
        val body: String? = null,
        val assets: List<GitCodeAsset> = emptyList(),
    )

    /** 检查结果：只留 UI 需要的字段，资产已筛成唯一可安装的 APK */
    data class UpdateInfo(
        val version: String,
        val changelog: String,
        val apkUrl: String,
    )

    /**
     * 拉取最新 release 并挑出 APK 资产；没有可安装的 APK 时返回 null，
     * 网络 / 解析异常原样抛出，由调用方决定静默还是弹窗提示。
     */
    suspend fun fetchLatest(): UpdateInfo? = withContext(Dispatchers.IO) {
        val req = Request.Builder()
            .url("$API_BASE/repos/$OWNER/$REPO/releases/latest")
            .header("Authorization", "Bearer $TOKEN")
            .build()
        NetworkModule.client.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) throw IOException("GitCode 响应异常：HTTP ${resp.code}")
            val payload = resp.body?.string() ?: throw IOException("GitCode 响应为空")
            val release = NetworkModule.json.decodeFromString(GitCodeRelease.serializer(), payload)
            val apk = release.pickApk() ?: return@withContext null
            UpdateInfo(
                version = cleanVersion(release.tag_name),
                changelog = release.body?.trim().orEmpty().ifEmpty { "暂无更新日志" },
                apkUrl = apk.browser_download_url,
            )
        }
    }

    /** [latest] 是否比当前版本新；tag 兼容 v0.1.1 / boxad-0.1.0 / 0.1.0 三种写法 */
    fun isNewer(latest: String, current: String = BuildConfig.VERSION_NAME): Boolean {
        val l = parseVersion(latest) ?: return false
        val c = parseVersion(current) ?: return false
        for (i in 0 until maxOf(l.size, c.size)) {
            val a = l.getOrElse(i) { 0 }
            val b = c.getOrElse(i) { 0 }
            if (a != b) return a > b
        }
        return false
    }

    /** 去掉 v / boxad- / box- 前缀，仅用于展示 */
    fun cleanVersion(tag: String): String =
        tag.trim().removePrefix("boxad-").removePrefix("box-").removePrefix("v").removePrefix("V")

    private fun GitCodeRelease.pickApk(): GitCodeAsset? {
        val apks = assets.filter { it.type == "attach" && it.name.endsWith(".apk", ignoreCase = true) }
        // 命名规范是 nexbox-android-X.X.X.apk；规范之外的（如历史遗留 nexbox-beta.apk）也兜底接受
        return apks.firstOrNull { it.name.startsWith("nexbox-android", ignoreCase = true) }
            ?: apks.firstOrNull()
    }

    /** "v0.1.1" → [0,1,1]；解析失败返回 null（比较时视为不更新，不弹窗） */
    private fun parseVersion(raw: String): List<Int>? {
        val cleaned = cleanVersion(raw)
        if (cleaned.isEmpty()) return null
        val segments = cleaned.split('.').map { seg -> seg.takeWhile { it.isDigit() } }
        if (segments.any { it.isEmpty() }) return null
        return segments.map { it.toInt() }
    }
}
