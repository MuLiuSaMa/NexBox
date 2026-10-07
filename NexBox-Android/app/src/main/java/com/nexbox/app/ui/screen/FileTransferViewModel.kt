package com.nexbox.app.ui.screen

import android.content.Intent
import android.net.Uri
import android.os.SystemClock
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.TransferFile
import com.nexbox.app.data.WsEvent
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 单个进行中下载的进度（按 fileId 隔离，支持多文件并发互不干扰） */
data class DownloadUi(
    /** 0..1；<0 表示总量未知 */
    val progress: Float = -1f,
    /** 实时速度（字节/秒） */
    val speed: Long = 0,
)

/** 单个进行中上传的进度（按 uri 隔离，多文件并发） */
data class UploadUi(
    val name: String,
    /** 0..1；<0 表示总量未知 */
    val progress: Float = -1f,
    /** 实时速度（字节/秒） */
    val speed: Long = 0,
)

data class FileTransferUiState(
    /** PC 发来的待接收（to_device），点「下载」才落到本机 */
    val incoming: List<TransferFile> = emptyList(),
    /** 本机已发送（from_device），PC 端另存为后变「已接收」 */
    val sent: List<TransferFile> = emptyList(),
    val loading: Boolean = false,
    /** 进行中的上传：uri → 进度（多文件并发，各一张进度卡） */
    val uploads: Map<String, UploadUi> = emptyMap(),
    /** 进行中的下载：fileId → 进度/速度（可多文件并发，各行只动自己的进度条） */
    val downloads: Map<String, DownloadUi> = emptyMap(),
    /** 本机已下载：fileId → 保存位置描述 */
    val savedLocations: Map<String, String> = emptyMap(),
    /** 本机已下载：fileId → 可打开的 content Uri（字符串形式） */
    val savedUris: Map<String, String> = emptyMap(),
    val message: String? = null,
    val messageIsError: Boolean = true,
)

/**
 * 文件互传（文件篮模式，单个列表混排两个方向）。
 * - 「添加文件」即上传到 PC 暂存区，PC 端用户手动另存为；
 * - PC 发来的文件点「下载」流式存进系统下载文件夹并回执 ack，可「打开 / 打开所在位置」；
 * - 下载时按 500ms 节流向 PC 上报进度/速度，两端都看得到传输状态；
 * - 列表刷新由 WS `transfer.update` 帧驱动（PC 添加/删除/回执），进场时主动拉一次。
 */
class FileTransferViewModel : ViewModel() {

    private val api = NetworkModule.api

    private val _state = MutableStateFlow(FileTransferUiState())
    val state: StateFlow<FileTransferUiState> = _state.asStateFlow()

    init {
        // PC 端有互传变更（添加/删除/已另存为回执）→ 立刻刷新列表
        viewModelScope.launch {
            NetworkModule.statsRepo.events.collect { event ->
                if (event is WsEvent.TransferUpdate) refresh()
            }
        }
    }

    /** 拉一次列表；无会话时清空 */
    fun refresh() {
        viewModelScope.launch {
            if (NetworkModule.session.current() == null) {
                _state.update { it.copy(incoming = emptyList(), sent = emptyList()) }
                return@launch
            }
            _state.update { it.copy(loading = true) }
            runCatching { api.transferList() }
                .onSuccess { lists ->
                    _state.update {
                        it.copy(incoming = lists.incoming, sent = lists.outgoing, loading = false)
                    }
                }
                .onFailure { e ->
                    _state.update { it.copy(loading = false, message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    /** 添加文件 = 并发上传到 PC 暂存区（多文件同时传）；失败的跳过并汇总提示 */
    fun addFiles(uris: List<Uri>) {
        if (uris.isEmpty()) return
        viewModelScope.launch {
            val results = uris.map { uri -> async { uploadOne(uri) } }.awaitAll()
            val okCount = results.count { it == null }
            val lastError = results.filterNotNull().firstOrNull()
            _state.update {
                it.copy(
                    message = when {
                        okCount > 0 && lastError == null -> "已发送 $okCount 个文件"
                        okCount > 0 -> "已发送 $okCount 个，部分失败：$lastError"
                        else -> lastError ?: "发送失败"
                    },
                    messageIsError = okCount == 0,
                )
            }
        }
    }

    /** 上传单个文件；返回 null 表示成功，否则为错误文案 */
    private suspend fun uploadOne(uri: Uri): String? {
        val key = uri.toString()
        // 同一个文件不重复上传
        if (_state.value.uploads.containsKey(key)) return null
        val (name, size) = api.queryFileInfo(uri)
        val displayName = name?.takeIf { it.isNotBlank() } ?: "文件"
        val meter = SpeedMeter()
        _state.update {
            it.copy(uploads = it.uploads + (key to UploadUi(displayName, if (size != null) 0f else -1f)))
        }
        return try {
            api.transferUpload(uri) { done, total ->
                val speed = meter.tick(done)
                _state.update { s ->
                    val cur = s.uploads[key] ?: return@update s
                    s.copy(uploads = s.uploads + (key to cur.copy(
                        progress = if (total > 0) (done.toFloat() / total).coerceIn(0f, 1f) else -1f,
                        speed = speed,
                    )))
                }
            }
            _state.update { it.copy(uploads = it.uploads - key) }
            refresh() // 传完立刻刷新，列表出现新条目
            null
        } catch (e: Throwable) {
            _state.update { it.copy(uploads = it.uploads - key) }
            e.toUserMessage()
        }
    }

    /** 流式下载 PC 发来的文件到系统下载文件夹，完成后回执 ack。可多文件并发 */
    fun download(file: TransferFile) {
        viewModelScope.launch {
            val meter = SpeedMeter()
            var lastReport = 0L
            // 已在下载中的条目不重复发起
            if (_state.value.downloads.containsKey(file.id)) return@launch
            _state.update { it.copy(downloads = it.downloads + (file.id to DownloadUi())) }
            try {
                val (where, uriStr) = api.transferDownload(file.id, file.name) { done, total ->
                    val speed = meter.tick(done)
                    _state.update { s ->
                        s.copy(downloads = s.downloads + (file.id to DownloadUi(
                            progress = if (total > 0) (done.toFloat() / total).coerceIn(0f, 1f) else -1f,
                            speed = speed,
                        )))
                    }
                    // 进度/速度节流上报给 PC 端（500ms 一次），让电脑上也看得到「手机接收中」
                    val now = SystemClock.elapsedRealtime()
                    if (now - lastReport >= 500) {
                        lastReport = now
                        viewModelScope.launch(Dispatchers.IO) {
                            runCatching { api.transferProgressReport(file.id, done, total, speed.toDouble()) }
                        }
                    }
                }
                // 回执失败不影响文件已落盘：列表下次刷新会对齐
                runCatching { api.transferAck(file.id) }
                _state.update {
                    it.copy(
                        downloads = it.downloads - file.id,
                        savedLocations = it.savedLocations + (file.id to where),
                        savedUris = it.savedUris + (file.id to uriStr),
                        message = "已保存到 $where",
                        messageIsError = false,
                    )
                }
                refresh()
            } catch (e: Throwable) {
                _state.update {
                    it.copy(downloads = it.downloads - file.id, message = e.toUserMessage(), messageIsError = true)
                }
            }
        }
    }

    /** 用系统应用打开已下载的文件 */
    fun openFile(file: TransferFile) {
        val context = NetworkModule.appContext
        val uriStr = _state.value.savedUris[file.id] ?: return
        val uri = Uri.parse(uriStr)
        val mime = context.contentResolver.getType(uri) ?: api.guessMime(file.name)
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mime)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        runCatching { context.startActivity(intent) }
            .onFailure {
                _state.update { s -> s.copy(message = "没有可以打开此文件的应用", messageIsError = true) }
            }
    }

    /** 打开文件所在位置：跳到系统「下载」管理页（互传文件都落在 Download/NexBox） */
    fun openDownloadFolder() {
        val context = NetworkModule.appContext
        val intent = Intent(android.app.DownloadManager.ACTION_VIEW_DOWNLOADS)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        runCatching { context.startActivity(intent) }
            .onFailure {
                _state.update { s -> s.copy(message = "无法打开下载文件夹", messageIsError = true) }
            }
    }

    /** 清除列表条目（来件/去件都可以清，两端共享同一份列表） */
    fun removeFile(file: TransferFile) {
        viewModelScope.launch {
            runCatching { api.transferRemove(file.id) }
                .onSuccess { refresh() }
                .onFailure { e ->
                    _state.update { it.copy(message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    fun consumeMessage() = _state.update { it.copy(message = null) }
}

/** 简单速度表：按进度回调计算字节/秒，约 200ms 更新一次，未到期沿用上次速度 */
private class SpeedMeter {
    private var lastAt = 0L
    private var lastBytes = 0L
    private var lastSpeed = 0L

    fun tick(done: Long): Long {
        val now = SystemClock.elapsedRealtime()
        val dt = (now - lastAt) / 1000.0
        if (lastAt > 0L && dt > 0.2) {
            lastSpeed = ((done - lastBytes) / dt).toLong().coerceAtLeast(0)
        }
        if (lastAt == 0L || dt > 0.2) {
            lastAt = now
            lastBytes = done
        }
        return lastSpeed
    }
}
