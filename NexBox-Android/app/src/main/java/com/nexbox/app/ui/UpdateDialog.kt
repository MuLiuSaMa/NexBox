package com.nexbox.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.nexbox.app.update.UpdateManager
import kotlinx.coroutines.delay

/**
 * 更新弹窗：检查中 / 发现新版本 / 下载进度 / 待安装 / 失败，一个弹窗按状态机切换内容。
 *
 * 样式对齐 AccentPickerDialog：全屏遮罩内居中圆角卡片（走 [Dialog] 自身窗口，
 * 才能压住内容层之外的悬浮导航条）。
 *
 * 文案对齐 PC 端 UpdateModal：发现新版本 / 更新日志 / 下载 / 取消 / 下载中 xx%。
 * 下载完成自动拉起安装；若系统要「未知来源」授权，授权返回后自动续装。
 */
@Composable
fun UpdateDialog() {
    val state = UpdateManager.state
    if (!UpdateManager.showDialog) return

    // 进入 Downloaded 稍等一拍（让「下载完成」可见）就自动拉起安装。
    // 被授权页打断时由 UpdateManager.onHostResumed() 续上，这里不重复触发。
    LaunchedEffect(state) {
        if (state is UpdateManager.State.Downloaded) {
            delay(600)
            UpdateManager.install()
        }
    }

    Dialog(
        onDismissRequest = { UpdateManager.dismiss() },
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 18.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(22.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(22.dp))
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                when (val s = state) {
                    is UpdateManager.State.Checking -> {
                        Text(
                            "检查更新",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(16.dp),
                                strokeWidth = 2.dp,
                            )
                            Spacer(Modifier.width(10.dp))
                            Text(
                                "正在连接 GitCode 更新源…",
                                fontSize = 13.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        OutlinedButton(
                            onClick = { UpdateManager.dismiss() },
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(12.dp),
                        ) { Text("取消") }
                    }

                    is UpdateManager.State.Available -> {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "发现新版本",
                                fontSize = 16.sp,
                                fontWeight = FontWeight.SemiBold,
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                            Spacer(Modifier.width(8.dp))
                            Text(
                                "v${s.info.version}",
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Bold,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(6.dp))
                                    .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.14f))
                                    .padding(horizontal = 7.dp, vertical = 3.dp),
                            )
                        }
                        Text(
                            "更新日志",
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        ChangelogBody(s.info.changelog)
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedButton(
                                onClick = { UpdateManager.dismiss() },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                            ) { Text("取消") }
                            Button(
                                onClick = { UpdateManager.startDownload() },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                            ) { Text("下载", fontWeight = FontWeight.SemiBold) }
                        }
                    }

                    is UpdateManager.State.Downloading -> {
                        Text(
                            "下载更新 v${s.info.version}",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            if (s.progress in 1..99) {
                                LinearProgressIndicator(
                                    progress = { s.progress / 100f },
                                    modifier = Modifier.fillMaxWidth(),
                                )
                            } else {
                                // 0%（刚起手）或总大小未知时用不确定态，避免一条死 0% 条
                                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                            }
                            Text(
                                if (s.progress in 1..99) "下载中 ${s.progress}%" else "下载中…",
                                fontSize = 12.5.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        OutlinedButton(
                            onClick = { UpdateManager.cancelDownload() },
                            modifier = Modifier.fillMaxWidth(),
                            shape = RoundedCornerShape(12.dp),
                        ) { Text("取消下载") }
                    }

                    is UpdateManager.State.Downloaded -> {
                        Text(
                            "下载完成",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "安装包已就绪，正在拉起安装。\n若系统弹出「未知来源」授权，请允许后自动继续。",
                            fontSize = 13.sp,
                            lineHeight = 19.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedButton(
                                onClick = { UpdateManager.dismiss() },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                            ) { Text("稍后") }
                            Button(
                                onClick = { UpdateManager.install() },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                            ) { Text("立即安装", fontWeight = FontWeight.SemiBold) }
                        }
                    }

                    is UpdateManager.State.Failed -> {
                        Text(
                            "更新失败",
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            s.message,
                            fontSize = 13.sp,
                            lineHeight = 19.sp,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            OutlinedButton(
                                onClick = { UpdateManager.dismiss() },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                            ) { Text("关闭") }
                            Button(
                                onClick = {
                                    if (s.info != null) UpdateManager.startDownload() else UpdateManager.manualCheck()
                                },
                                modifier = Modifier.weight(1f),
                                shape = RoundedCornerShape(12.dp),
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = MaterialTheme.colorScheme.primary,
                                ),
                            ) { Text("重试", fontWeight = FontWeight.SemiBold) }
                        }
                    }

                    // UpToDate / Idle 不在弹窗里展示：已是最新走 toast，空闲没有可展示的内容
                    else -> Unit
                }
            }
        }
    }
}

/** 更新日志正文：限高滚动，换行按 GitCode release body 的原始排版呈现 */
@Composable
private fun ChangelogBody(text: String) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(max = 220.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Text(
            text,
            fontSize = 12.5.sp,
            lineHeight = 19.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
