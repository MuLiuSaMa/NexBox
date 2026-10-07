package com.nexbox.app.data

import android.content.ContentValues
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okio.BufferedSink
import java.io.File
import java.io.IOException
import java.io.InputStream
import java.util.Locale

/**
 * PC 端 REST 客户端。所有响应统一走 `{ok,data}` / `{ok:false,error}` 信封，
 * 非 2xx 直接抛 [NexBoxException]，状态码语义见 [httpErrorMessage]。
 */
class NexBoxClient(private val sessionStore: SessionStore) {

    // ───────────────────────── 免鉴权 ─────────────────────────

    /** 探活：不需要 token，用来在配对前确认地址可达、PC 端远程控制是否开启 */
    suspend fun info(host: String, port: Int): InfoData =
        call("GET", "http://$host:$port/api/info").let {
            NetworkModule.json.decodeFromJsonElement(InfoData.serializer(), it)
        }

    /** 用 6 位配对码换长期令牌。device_name 按契约是蛇形字段；device_uid 让 PC 端对同一台手机去重 */
    suspend fun pair(host: String, port: Int, code: String, deviceUid: String): PairData {
        val payload = NetworkModule.json.encodeToString(
            PairRequest.serializer(),
            PairRequest(
                code = code,
                deviceName = Build.MODEL ?: "Android",
                deviceUid = deviceUid,
            ),
        )
        val data = call("POST", "http://$host:$port/api/pair", payload)
        return NetworkModule.json.decodeFromJsonElement(PairData.serializer(), data)
    }

    /**
     * 免配对码：向 PC 端发起配对请求，PC 端会弹确认框，
     * 用户在电脑上点「允许」后由 [pairRequestStatus] 轮询取走令牌。
     */
    suspend fun requestPair(host: String, port: Int, deviceName: String, deviceUid: String): PairRequestCreated {
        val payload = NetworkModule.json.encodeToString(
            PairRequestBody.serializer(),
            PairRequestBody(deviceName = deviceName, deviceUid = deviceUid),
        )
        val data = call("POST", "http://$host:$port/api/pair/request", payload)
        return NetworkModule.json.decodeFromJsonElement(PairRequestCreated.serializer(), data)
    }

    /** 轮询配对请求的审批结果 */
    suspend fun pairRequestStatus(host: String, port: Int, requestId: String): PairRequestStatusData {
        val data = call("GET", "http://$host:$port/api/pair/request/$requestId")
        return NetworkModule.json.decodeFromJsonElement(PairRequestStatusData.serializer(), data)
    }

    /** 放弃本次请求（退出页面时调用，PC 端会撤掉待审批项） */
    suspend fun cancelPairRequest(host: String, port: Int, requestId: String) {
        runCatching { call("DELETE", "http://$host:$port/api/pair/request/$requestId") }
    }

    // ───────────────────────── 需鉴权 ─────────────────────────

    suspend fun capabilities(): CapabilitiesData {
        val data = authed("GET", "/api/capabilities")
        return NetworkModule.json.decodeFromJsonElement(CapabilitiesData.serializer(), data)
    }

    suspend fun query(key: String, args: JsonObject = JsonObject(emptyMap())): JsonElement {
        val payload = NetworkModule.json.encodeToString(
            QueryRequest.serializer(),
            QueryRequest(key = key, args = args),
        )
        return authed("POST", "/api/query", payload)
    }

    suspend fun action(
        key: String,
        args: JsonObject = JsonObject(emptyMap()),
        confirm: Boolean = false,
    ): JsonElement {
        val payload = NetworkModule.json.encodeToString(
            ActionRequest.serializer(),
            ActionRequest(key = key, args = args, confirm = confirm),
        )
        return authed("POST", "/api/action", payload)
    }

    /** 自助解绑：撤销本机令牌（成功后再清本地会话） */
    suspend fun unpair() {
        authed("DELETE", "/api/pair")
    }

    // ───────────────────────── 文件互传（文件篮） ─────────────────────────

    /** GET /api/transfer/files —— incoming=PC 发来的待接收，outgoing=本机已发送 */
    suspend fun transferList(): TransferLists {
        val data = authed("GET", "/api/transfer/files")
        return NetworkModule.json.decodeFromJsonElement(TransferLists.serializer(), data)
    }

    /** POST /api/transfer/files/:id/ack —— 下载完成回执，PC 端条目变「已接收」 */
    suspend fun transferAck(fileId: String) {
        authed("POST", "/api/transfer/files/$fileId/ack")
    }

    /** DELETE /api/transfer/files/:id —— 只能删自己发送的条目（PC 端来件归 PC 管） */
    suspend fun transferRemove(fileId: String) {
        authed("DELETE", "/api/transfer/files/$fileId")
    }

    /** POST /api/transfer/files/:id/progress —— 下载时上报进度/速度，PC 端列表同步显示（调用方节流） */
    suspend fun transferProgressReport(fileId: String, done: Long, total: Long, speedBps: Double) {
        val payload = NetworkModule.json.encodeToString(
            TransferProgress.serializer(),
            TransferProgress(done = done, total = total, speed = speedBps),
        )
        authed("POST", "/api/transfer/files/$fileId/progress", payload)
    }

    /**
     * POST /api/transfer/files —— multipart 上传单个文件到 PC 暂存区。
     * 流式边读边发（不整块进内存），[onProgress] 回调 (已传字节, 总字节)（总字节未知时为 -1）。
     */
    suspend fun transferUpload(uri: Uri, onProgress: (Long, Long) -> Unit): TransferFile =
        withContext(Dispatchers.IO) {
            val s = sessionStore.current()
                ?: throw NexBoxException(401, message = "尚未配对，请先连接 PC 端")
            val resolver = NetworkModule.appContext.contentResolver
            val (displayName, declaredSize) = queryFileInfo(uri)
            val safeName = displayName?.takeIf { it.isNotBlank() } ?: "file-${System.currentTimeMillis()}"

            val requestBody = object : RequestBody() {
                override fun contentType(): okhttp3.MediaType? =
                    resolver.getType(uri)?.toMediaType() ?: "application/octet-stream".toMediaType()
                override fun contentLength(): Long = declaredSize ?: -1L
                override fun writeTo(sink: BufferedSink) {
                    resolver.openInputStream(uri)?.use { input ->
                        copyUpload(input, sink, declaredSize ?: -1L, onProgress)
                    } ?: throw IOException("无法读取所选文件，可能已被移动或删除")
                }
            }

            val body = MultipartBody.Builder()
                .setType(MultipartBody.FORM)
                .addFormDataPart("file", safeName, requestBody)
                .build()
            val request = Request.Builder()
                .url(s.baseUrl + "/api/transfer/files")
                .header("Authorization", "Bearer ${s.token}")
                .post(body)
                .build()

            NetworkModule.transferClient.newCall(request).execute().use { resp ->
                val data = parseEnvelope(resp.isSuccessful, resp.code, resp.body?.string().orEmpty())
                // 返回 {added:[TransferFile]}，本次只传一个文件取第一个
                val arr = data.jsonObject["added"]?.jsonArray
                    ?: throw NexBoxException(resp.code, message = "PC 端返回数据异常")
                val first = arr.firstOrNull()
                    ?: throw NexBoxException(resp.code, message = "PC 端没有接收该文件")
                NetworkModule.json.decodeFromJsonElement(TransferFile.serializer(), first)
            }
        }

    /**
     * GET /api/transfer/files/:id/download —— 流式下载 PC 发来的文件，
     * 落到系统下载文件夹的 NexBox 子目录。返回 (保存位置描述, 可打开的 content Uri)。
     */
    suspend fun transferDownload(fileId: String, fileName: String, onProgress: (Long, Long) -> Unit): Pair<String, String> =
        withContext(Dispatchers.IO) {
            val s = sessionStore.current()
                ?: throw NexBoxException(401, message = "尚未配对，请先连接 PC 端")
            val request = Request.Builder()
                .url(s.baseUrl + "/api/transfer/files/$fileId/download")
                .header("Authorization", "Bearer ${s.token}")
                .get()
                .build()

            NetworkModule.transferClient.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) {
                    parseEnvelope(false, resp.code, resp.body?.string().orEmpty())
                }
                val body = resp.body ?: throw IOException("PC 端返回了空数据")
                val total = resp.headers["Content-Length"]?.toLongOrNull() ?: -1L
                body.byteStream().use { input ->
                    saveIntoDownloads(fileName, input, total, onProgress)
                }
            }
        }

    // ───────────────────────── 内部实现 ─────────────────────────

    private suspend fun authed(method: String, path: String, body: String? = null): JsonElement {
        val s = sessionStore.current()
            ?: throw NexBoxException(401, message = "尚未配对，请先连接 PC 端")
        return call(method, s.baseUrl + path, body, s.token)
    }

    private suspend fun call(
        method: String,
        url: String,
        body: String? = null,
        token: String? = null,
    ): JsonElement = withContext(Dispatchers.IO) {
        val builder = Request.Builder().url(url)
        when (method) {
            "GET" -> builder.get()
            "POST" -> builder.post((body ?: "{}").toRequestBody(JSON_MEDIA_TYPE))
            "DELETE" -> builder.delete()
            else -> error("unsupported method: $method")
        }
        if (!token.isNullOrBlank()) builder.header("Authorization", "Bearer $token")

        NetworkModule.client.newCall(builder.build()).execute().use { resp ->
            val text = resp.body?.string().orEmpty()
            parseEnvelope(resp.isSuccessful, resp.code, text)
        }
    }

    /** 解析 `{ok,data}` / `{ok:false,error}` 信封；失败抛 [NexBoxException] */
    private fun parseEnvelope(successful: Boolean, httpCode: Int, text: String): JsonElement {
        val envelope = runCatching {
            NetworkModule.json.decodeFromString(Envelope.serializer(JsonElement.serializer()), text)
        }.getOrNull()
        if (!successful) {
            // 优先用 PC 端给的具体原因，没有则按状态码给中文文案
            val serverMsg = envelope?.error?.message?.takeIf { it.isNotBlank() }
            val fallback = when (httpCode) {
                403 -> if (serverMsg?.contains("limit", true) == true) {
                    "该 PC 端已配对的设备数已达上限（10 台），请先在 PC 端移除旧设备"
                } else null
                else -> serverMsg
            }
            throw NexBoxException(httpCode, envelope?.error?.codeText(), httpErrorMessage(httpCode, fallback))
        }
        if (envelope == null) throw NexBoxException(httpCode, message = "PC 端返回了无法解析的数据")
        if (!envelope.ok) {
            throw NexBoxException(
                httpCode,
                envelope.error?.codeText(),
                envelope.error?.message ?: "PC 端拒绝了该请求",
            )
        }
        return envelope.data ?: JsonObject(emptyMap())
    }

    // ───────────────────────── 文件互传内部工具 ─────────────────────────

    /** 查选择器返回的 uri 的显示名与大小（大小可能是 -1 / null，表示未知） */
    internal fun queryFileInfo(uri: Uri): Pair<String?, Long?> {
        val resolver = NetworkModule.appContext.contentResolver
        var name: String? = null
        var size: Long? = null
        runCatching {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
        }.getOrNull()?.use { c ->
            if (c.moveToFirst()) {
                val ni = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                val si = c.getColumnIndex(OpenableColumns.SIZE)
                if (ni >= 0) name = c.getString(ni)
                if (si >= 0 && !c.isNull(si)) {
                    val v = c.getLong(si)
                    if (v > 0) size = v
                }
            }
        }
        val finalName = name?.takeIf { it.isNotBlank() } ?: uri.lastPathSegment
        return finalName to size
    }

    /** 上传写流：边读边发并回调进度 */
    private fun copyUpload(input: InputStream, sink: BufferedSink, total: Long, onProgress: (Long, Long) -> Unit) {
        val buf = ByteArray(64 * 1024)
        var written = 0L
        while (true) {
            val n = input.read(buf)
            if (n < 0) break
            sink.write(buf, 0, n)
            written += n
            onProgress(written, total)
        }
    }

    /** 下载读流：边收边写并回调进度 */
    private fun copyDownload(input: InputStream, out: java.io.OutputStream, total: Long, onProgress: (Long, Long) -> Unit) {
        val buf = ByteArray(64 * 1024)
        var written = 0L
        while (true) {
            val n = input.read(buf)
            if (n < 0) break
            out.write(buf, 0, n)
            written += n
            onProgress(written, total)
        }
        out.flush()
    }

    /**
     * 把下载流落到系统下载文件夹的 NexBox 子目录。
     * 返回 (位置描述, 可打开的 Uri)：API 29+ 走 MediaStore（免权限）；
     * 更老版本有存储权限走公共下载目录，否则退回应用私有目录（Uri 走 FileProvider）。
     */
    private fun saveIntoDownloads(
        displayName: String,
        input: InputStream,
        total: Long,
        onProgress: (Long, Long) -> Unit,
    ): Pair<String, String> {
        val context = NetworkModule.appContext
        val safeName = displayName.ifBlank { "file-${System.currentTimeMillis()}" }

        if (Build.VERSION.SDK_INT >= 29) {
            val values = ContentValues().apply {
                put(MediaStore.MediaColumns.DISPLAY_NAME, safeName)
                put(MediaStore.MediaColumns.MIME_TYPE, guessMime(safeName))
                put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/NexBox")
                put(MediaStore.MediaColumns.IS_PENDING, 1)
            }
            val resolver = context.contentResolver
            val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                ?: throw IOException("无法创建下载文件")
            try {
                resolver.openOutputStream(uri)?.use { out ->
                    copyDownload(input, out, total, onProgress)
                } ?: throw IOException("无法写入下载文件")
                values.clear()
                values.put(MediaStore.MediaColumns.IS_PENDING, 0)
                resolver.update(uri, values, null, null)
            } catch (t: Throwable) {
                runCatching { resolver.delete(uri, null, null) }
                throw t
            }
            return "Download/NexBox/$safeName" to uri.toString()
        }

        val publicDir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "NexBox")
        val hasLegacyPermission = context.checkSelfPermission(android.Manifest.permission.WRITE_EXTERNAL_STORAGE) ==
            android.content.pm.PackageManager.PERMISSION_GRANTED
        val dir = if (hasLegacyPermission) {
            publicDir
        } else {
            context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: context.filesDir
        }
        dir.mkdirs()
        val target = uniqueFile(dir, safeName)
        target.outputStream().use { out ->
            copyDownload(input, out, total, onProgress)
        }
        val uri = androidx.core.content.FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            target,
        )
        val display = if (dir == publicDir) "Download/NexBox/${target.name}" else target.absolutePath
        return display to uri.toString()
    }

    /** MediaStore 插入 / 打开文件需要 MIME 类型，按扩展名给一个常见映射 */
    internal fun guessMime(name: String): String = when (name.substringAfterLast('.', "").lowercase(Locale.ROOT)) {
        "jpg", "jpeg" -> "image/jpeg"
        "png" -> "image/png"
        "gif" -> "image/gif"
        "webp" -> "image/webp"
        "mp4" -> "video/mp4"
        "mp3" -> "audio/mpeg"
        "flac" -> "audio/flac"
        "pdf" -> "application/pdf"
        "zip" -> "application/zip"
        "apk" -> "application/vnd.android.package-archive"
        else -> "application/octet-stream"
    }

    /** 公共目录写入需要手动去重：a.txt → a (1).txt */
    private fun uniqueFile(dir: File, name: String): File {
        var f = File(dir, name)
        if (!f.exists()) return f
        val stem = name.substringBeforeLast('.', missingDelimiterValue = name)
        val ext = name.substringAfterLast('.', missingDelimiterValue = "")
        var n = 1
        while (f.exists()) {
            f = if (ext.isEmpty()) File(dir, "$stem ($n)") else File(dir, "$stem ($n).$ext")
            n++
        }
        return f
    }

    private companion object {
        val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
    }
}
