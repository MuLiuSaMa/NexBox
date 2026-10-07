package com.nexbox.app.ui.screen.home

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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.QrCodeScanner
import androidx.compose.material.icons.rounded.Radar
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.nexbox.app.data.DiscoveryReply
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.screen.ConnectUiState
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.TextSecondary

/**
 * 添加设备弹窗：只有「扫描设备」和「扫码」两个入口（按需求砍掉手动 IP 表单）。
 * 扫描态不是卡片 —— 就是一份朴素的设备列表，点哪台配哪台；
 * 状态（搜索中/等待确认）压成一行小字。弹窗关掉后搜索/等待继续在后台跑，
 * 主页互联卡的相位动画会同步反映。
 */
@Composable
fun AddDeviceDialog(
    mode: String,
    state: ConnectUiState,
    onScanDevices: () -> Unit,
    onPick: (DiscoveryReply) -> Unit,
    onScanQr: () -> Unit,
    onBackToMenu: () -> Unit,
    onDismiss: () -> Unit,
) {
    Dialog(
        onDismissRequest = onDismiss,
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
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                if (mode == "scan") {
                    ScanBody(state, onPick, onScanDevices, onBackToMenu)
                } else {
                    MenuBody(onScanDevices, onScanQr, onDismiss)
                }
                // 弹窗窗口不消费手势区，留一点底部余量避免贴住导航条
                Spacer(Modifier.navigationBarsPadding().height(0.dp))
            }
        }
    }
}

@Composable
private fun MenuBody(onScanDevices: () -> Unit, onScanQr: () -> Unit, onDismiss: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "添加设备",
                fontSize = 16.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text("把同一局域网里的新境盒 PC 端连到这台手机", fontSize = 11.5.sp, color = TextSecondary)
        }

        OptionRow(
            icon = Icons.Rounded.Radar,
            title = "扫描设备",
            desc = "自动搜索同一 Wi-Fi 下的 PC 端",
            onClick = onScanDevices,
        )
        OptionRow(
            icon = Icons.Rounded.QrCodeScanner,
            title = "扫码连接",
            desc = "扫 PC 端「手机远程连接」里的二维码",
            onClick = onScanQr,
        )

        OutlinedButton(
            onClick = onDismiss,
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(12.dp),
        ) { Text("取消") }
    }
}

@Composable
private fun OptionRow(icon: androidx.compose.ui.graphics.vector.ImageVector, title: String, desc: String, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
            .clickable(onClick = onClick)
            .padding(14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(40.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = title, tint = Accent, modifier = Modifier.size(20.dp))
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, fontSize = 14.sp, fontWeight = FontWeight.Medium, color = MaterialTheme.colorScheme.onSurface)
            Text(desc, fontSize = 11.sp, color = TextSecondary, lineHeight = 15.sp)
        }
    }
}

/**
 * 扫描态：不是卡片 —— 一行状态小字 + 朴素的设备列表，点设备直接配对。
 */
@Composable
private fun ScanBody(
    state: ConnectUiState,
    onPick: (DiscoveryReply) -> Unit,
    onRescan: () -> Unit,
    onBackToMenu: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "扫描设备",
                fontSize = 16.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onBackToMenu) { Text("返回") }
        }

        // 状态压成一行小字
        when {
            state.awaitingApproval -> ScanHint(
                busy = true,
                text = "已向「${state.approvalDeviceName.orEmpty()}」发送配对请求，请在电脑上点「允许」",
            )
            state.scanning -> ScanHint(busy = true, text = "正在搜索局域网内的电脑…")
            state.discovered.isEmpty() -> ScanHint(
                busy = false,
                text = "未发现设备，请确认 PC 端已打开「远程控制」开关",
            )
        }

        // 朴素设备列表：设备名 + 手动配对按钮
        state.discovered.forEachIndexed { index, device ->
            DeviceRow(device = device, onPair = { onPick(device) })
            if (index != state.discovered.lastIndex) {
                HorizontalDivider(thickness = 0.5.dp, color = Hairline)
            }
        }

        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center) {
            TextButton(onClick = onRescan, enabled = !state.awaitingApproval) {
                Text(if (state.awaitingApproval) "等待电脑确认中…" else "重新扫描")
            }
        }
    }
}

@Composable
private fun ScanHint(busy: Boolean, text: String) {
    Row(
        modifier = Modifier.padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (busy) {
            CircularProgressIndicator(modifier = Modifier.size(12.dp), strokeWidth = 1.8.dp)
            Spacer(Modifier.width(8.dp))
        }
        Text(text, fontSize = 12.sp, color = TextSecondary, lineHeight = 16.sp)
    }
}

@Composable
private fun DeviceRow(device: DiscoveryReply, onPair: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onPair)
            .padding(vertical = 12.dp, horizontal = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(
                device.name.orEmpty().ifBlank { "未知设备" },
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                buildString {
                    append(device.ip.orEmpty())
                    append(':')
                    append(device.tcpPort)
                    if (!device.controlEnabled) append("  ·  仅监控")
                },
                fontSize = 11.5.sp,
                color = TextSecondary,
            )
        }
        // 手动配对按钮：随时可点，换设备时覆盖之前的等待
        OutlinedButton(
            onClick = onPair,
            shape = RoundedCornerShape(10.dp),
            contentPadding = PaddingValues(horizontal = 14.dp, vertical = 6.dp),
        ) {
            Text(
                "手动配对",
                fontSize = 12.5.sp,
                fontWeight = FontWeight.Medium,
            )
        }
    }
}
