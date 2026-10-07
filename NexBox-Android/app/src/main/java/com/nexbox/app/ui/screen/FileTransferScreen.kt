package com.nexbox.app.ui.screen

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.TransferFile
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.TextSecondary
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

private val TIME_FMT = SimpleDateFormat("M/d HH:mm", Locale.getDefault())

private fun fmtSize(n: Long): String = when {
    n < 1024 -> "$n B"
    n < 1024L * 1024 -> String.format(Locale.getDefault(), "%.1f KB", n / 1024.0)
    n < 1024L * 1024 * 1024 -> String.format(Locale.getDefault(), "%.1f MB", n / 1024.0 / 1024.0)
    else -> String.format(Locale.getDefault(), "%.2f GB", n / 1024.0 / 1024.0 / 1024.0)
}

private fun fmtSpeed(bps: Long): String = if (bps > 0) "${fmtSize(bps)}/s" else "…"

/** 单个列表行：混排两个方向，带一个方向小标签 */
private data class TransferRow(val file: TransferFile, val fromPc: Boolean)

/**
 * 文件互传页（主页「文件互传」卡片进入）：单个文件列表混排两个方向 ——
 * 「来自电脑」点「下载」流式存进 Download/NexBox，完成后可打开 / 打开所在位置；
 * 「发给电脑」添加即上传到 PC 暂存区，由 PC 端用户手动另存为。进度与速度实时显示。
 */
@Composable
fun FileTransferScreen(vm: FileTransferViewModel = viewModel(), onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()
    val connected by NetworkModule.statsRepo.connected.collectAsStateWithLifecycle()

    // 进场拉一次列表；WS 变更帧由 VM 兜底刷新
    LaunchedEffect(Unit) { vm.refresh() }

    // 添加文件：系统选择器任意类型，选完立即上传
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (!uris.isNullOrEmpty()) vm.addFiles(uris)
    }
    // API 29+ MediaStore 免权限；更老版本下载前先要一次存储权限（拒绝则客户端自动退回私有目录）
    var pendingDownload by remember { mutableStateOf<TransferFile?>(null) }
    val legacyPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { _ ->
        val file = pendingDownload
        pendingDownload = null
        if (file != null) vm.download(file)
    }
    val requestDownload: (TransferFile) -> Unit = { f ->
        if (Build.VERSION.SDK_INT >= 29) {
            vm.download(f)
        } else {
            val granted = ContextCompat.checkSelfPermission(
                NetworkModule.appContext,
                Manifest.permission.WRITE_EXTERNAL_STORAGE,
            ) == PackageManager.PERMISSION_GRANTED
            if (granted) vm.download(f) else {
                pendingDownload = f
                legacyPermission.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE)
            }
        }
    }

    // 单列表：两个方向按时间混排，新的在前
    val rows = remember(state.incoming, state.sent) {
        (state.incoming.map { TransferRow(it, fromPc = true) } + state.sent.map { TransferRow(it, fromPc = false) })
            .sortedByDescending { it.file.createdAt }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        DetailTopBar(
            title = "文件互传",
            onBack = onBack,
            trailingText = if (connected) "已连接" else "未连接",
        )

        Text(
            "通过局域网直接传输，文件不经过外部服务器；PC 端在「手机远程连接」弹窗的设备页收发文件。",
            fontSize = 11.sp,
            color = TextSecondary,
            lineHeight = 16.sp,
        )

        // 添加文件入口
        AppCard(
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.clickable { picker.launch(arrayOf("*/*")) },
        ) {
            Row(
                Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.Center,
            ) {
                Icon(Icons.Rounded.Add, contentDescription = null, tint = Accent, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text("添加文件", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Accent)
            }
        }

        // 上传进度卡（多文件并发，每个文件一张卡）
        state.uploads.values.forEach { up ->
            TransferProgressCard(title = "正在发送 ${up.name}", progress = up.progress, speed = up.speed)
        }

        if (rows.isEmpty() && state.uploads.isEmpty()) {
            EmptyCard(
                title = if (state.loading) "正在读取…" else "还没有文件",
                hint = "点「添加文件」选择要传的文件，会立即通过局域网传到 PC 端；PC 端发来的文件也会出现在这里。",
            )
        } else {
            rows.forEach { row ->
                if (row.fromPc) {
                    IncomingRow(state, row.file, vm, onDownload = requestDownload)
                } else {
                    SentRow(row.file, vm)
                }
            }
        }

        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

// ───────────────────────── 行 ─────────────────────────

@Composable
private fun IncomingRow(
    state: FileTransferUiState,
    file: TransferFile,
    vm: FileTransferViewModel,
    onDownload: (TransferFile) -> Unit,
) {
    // 每个文件自己的进度条（多文件并发互不干扰）
    val dl = state.downloads[file.id]
    val savedWhere = state.savedLocations[file.id]
    val done = file.acked || savedWhere != null

    AppCard(shape = RoundedCornerShape(12.dp)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                DirectionTag(fromPc = true)
                Spacer(Modifier.width(8.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        file.name,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        "${fmtSize(file.size)} · ${TIME_FMT.format(Date(file.createdAt))}",
                        fontSize = 11.sp,
                        color = TextSecondary,
                    )
                }
                when {
                    dl != null -> Text(
                        "${if (dl.progress >= 0f) "${(dl.progress * 100).toInt()}% · " else ""}${fmtSpeed(dl.speed)}",
                        fontSize = 11.sp,
                        color = Accent,
                        fontWeight = FontWeight.Medium,
                    )
                    done -> Text(
                        "已保存",
                        fontSize = 11.sp,
                        color = MaterialTheme.colorScheme.primary,
                        fontWeight = FontWeight.Medium,
                    )
                    else -> SmallActionButton(text = "下载", primary = true) { onDownload(file) }
                }
                // 来件也可以清除（两端共享同一份列表，删了 PC 端也会消失）
                IconButton(onClick = { vm.removeFile(file) }, modifier = Modifier.size(30.dp)) {
                    Icon(
                        Icons.Rounded.Close,
                        contentDescription = "清除",
                        tint = TextSecondary,
                        modifier = Modifier.size(17.dp),
                    )
                }
            }
            if (dl != null) {
                Spacer(Modifier.height(6.dp))
                if (dl.progress >= 0f) {
                    LinearProgressIndicator(progress = { dl.progress }, modifier = Modifier.fillMaxWidth(), color = Accent)
                } else {
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth(), color = Accent)
                }
            }
            if (done) {
                val where = savedWhere ?: file.savedPath
                if (!where.isNullOrBlank()) {
                    Spacer(Modifier.height(4.dp))
                    Text(
                        "已保存到 $where",
                        fontSize = 10.5.sp,
                        color = TextSecondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Spacer(Modifier.height(6.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    // 「打开」只在本次会话里有 Uri 时可用；重启后仍可用「所在位置」到下载文件夹找到文件
                    if (state.savedUris[file.id] != null) {
                        SmallActionButton(text = "打开", primary = false) { vm.openFile(file) }
                    }
                    SmallActionButton(text = "打开所在位置", primary = false) { vm.openDownloadFolder() }
                }
            }
        }
    }
}

@Composable
private fun SentRow(file: TransferFile, vm: FileTransferViewModel) {
    AppCard(shape = RoundedCornerShape(12.dp)) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            DirectionTag(fromPc = false)
            Spacer(Modifier.width(8.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    file.name,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    "${fmtSize(file.size)} · ${TIME_FMT.format(Date(file.createdAt))}",
                    fontSize = 11.sp,
                    color = TextSecondary,
                )
            }
            Text(
                if (file.acked) "已接收" else "待 PC 另存",
                fontSize = 11.sp,
                fontWeight = FontWeight.Medium,
                color = if (file.acked) MaterialTheme.colorScheme.primary else TextSecondary,
            )
            IconButton(onClick = { vm.removeFile(file) }, modifier = Modifier.size(32.dp)) {
                Icon(
                    Icons.Rounded.Close,
                    contentDescription = "删除",
                    tint = TextSecondary,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

// ───────────────────────── 通用小块 ─────────────────────────

/** 方向小标签：来自电脑 / 发给电脑 */
@Composable
private fun DirectionTag(fromPc: Boolean) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f))
            .padding(horizontal = 5.dp, vertical = 2.dp),
    ) {
        Text(
            if (fromPc) "来自电脑" else "发给电脑",
            fontSize = 9.5.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.primary,
        )
    }
}

/** 小号操作按钮：primary 走实心主题色，否则描边 */
@Composable
private fun SmallActionButton(text: String, primary: Boolean, onClick: () -> Unit) {
    if (primary) {
        Button(
            onClick = onClick,
            modifier = Modifier.height(26.dp),
            shape = RoundedCornerShape(50),
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 0.dp),
        ) {
            Text(text, fontSize = 11.sp, maxLines = 1)
        }
    } else {
        OutlinedButton(
            onClick = onClick,
            modifier = Modifier.height(26.dp),
            shape = RoundedCornerShape(50),
            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 0.dp),
        ) {
            Text(text, fontSize = 11.sp, maxLines = 1, color = MaterialTheme.colorScheme.onSurface)
        }
    }
}

/** 上传 / 下载共用的进度卡 */
@Composable
private fun TransferProgressCard(title: String, progress: Float, speed: Long) {
    AppCard(shape = RoundedCornerShape(12.dp)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    title,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Text(
                    buildString {
                        if (progress >= 0f) append("${(progress * 100).toInt()}%")
                        if (speed > 0) {
                            if (isNotEmpty()) append(" · ")
                            append(fmtSpeed(speed))
                        }
                    },
                    fontSize = 11.sp,
                    color = Accent,
                    fontWeight = FontWeight.Medium,
                )
            }
            Spacer(Modifier.height(6.dp))
            if (progress >= 0f) {
                LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth(), color = Accent)
            } else {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth(), color = Accent)
            }
        }
    }
}

@Composable
private fun EmptyCard(title: String, hint: String) {
    AppCard(shape = RoundedCornerShape(12.dp)) {
        Column(
            Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 22.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(title, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.onSurface)
            Spacer(Modifier.height(5.dp))
            Text(
                hint,
                fontSize = 11.sp,
                color = TextSecondary,
                lineHeight = 16.sp,
                textAlign = TextAlign.Center,
            )
        }
    }
}
