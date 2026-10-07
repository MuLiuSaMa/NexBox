package com.nexbox.app.update

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import java.io.File
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

/**
 * 更新包下载：流式写盘 + 百分比回调，取消协程即断流。
 *
 * GitCode 下载链路的两个坑（实测）：
 * - browser_download_url 会 302 到签名 CDN，auth_key 只有约 60 秒有效期，
 *   OkHttp 默认自动跟随重定向即可，不要缓存重定向后的地址；
 * - 对下载 URL 发 HEAD 会 401，所以不做预检直接流式 GET，
 *   总大小从响应 Content-Length 取（release API 的资产没有 size 字段）。
 */
object UpdateDownloader {

    private val client: OkHttpClient by lazy {
        OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .writeTimeout(60, TimeUnit.SECONDS)
            .build()
    }

    private const val APK_NAME = "nexbox-update.apk"
    private const val PART_SUFFIX = ".part"

    /** 应用私有 Download 目录（file_paths.xml 的 app_downloads 已覆盖，可直接经 FileProvider 分享） */
    fun apkFile(context: Context): File {
        val dir = context.getExternalFilesDir("Download") ?: context.filesDir
        return File(dir, APK_NAME)
    }

    /**
     * 下载 [url] 到应用私有 Download 目录。[onProgress] 回调整数百分比（0..100）；
     * Content-Length 未知时不回调，UI 自行退回不确定态。完成后校验落盘字节数与
     * Content-Length 一致，不一致即抛错（PC 端 downloader.rs 同样的完整性策略）。
     */
    suspend fun download(context: Context, url: String, onProgress: (Int) -> Unit): File =
        withContext(Dispatchers.IO) {
            val dest = apkFile(context)
            dest.parentFile?.mkdirs()
            // 先写临时文件再原子替换：中断后不会留下半截 APK 被误当成完整安装包
            val tmp = File(dest.parentFile, APK_NAME + PART_SUFFIX)
            if (dest.exists()) dest.delete()

            val call = client.newCall(Request.Builder().url(url).build())
            try {
                call.await().use { resp ->
                    if (!resp.isSuccessful) throw IOException("下载失败：HTTP ${resp.code}")
                    val body = resp.body ?: throw IOException("下载失败：响应内容为空")
                    val total = body.contentLength()
                    var read = 0L
                    var lastPct = -1
                    body.byteStream().use { input ->
                        tmp.outputStream().use { out ->
                            val buf = ByteArray(64 * 1024)
                            while (true) {
                                currentCoroutineContext().ensureActive()
                                val n = input.read(buf)
                                if (n < 0) break
                                out.write(buf, 0, n)
                                read += n
                                if (total > 0) {
                                    val pct = (read * 100 / total).toInt()
                                    if (pct != lastPct) {
                                        lastPct = pct
                                        onProgress(pct.coerceIn(0, 100))
                                    }
                                }
                            }
                            out.flush()
                        }
                    }
                    if (total > 0 && read != total) {
                        throw IOException("下载不完整：$read / $total 字节")
                    }
                }
                if (!tmp.renameTo(dest)) {
                    tmp.copyTo(dest, overwrite = true)
                    tmp.delete()
                }
                dest
            } catch (e: Exception) {
                tmp.delete()
                throw e
            }
        }

    /** enqueue + 挂起：取消协程会同时 cancel 底层 call（断开连接、停止读流） */
    private suspend fun Call.await(): Response = suspendCancellableCoroutine { cont ->
        cont.invokeOnCancellation { cancel() }
        enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (cont.isActive) cont.resumeWithException(e)
            }

            override fun onResponse(call: Call, response: Response) {
                if (cont.isActive) cont.resume(response) else response.close()
            }
        })
    }
}
