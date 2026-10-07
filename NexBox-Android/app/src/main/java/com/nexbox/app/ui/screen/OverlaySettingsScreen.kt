package com.nexbox.app.ui.screen

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.rounded.PictureInPicture
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.nexbox.app.data.DELTA_MAPS
import com.nexbox.app.data.OverlayStore
import com.nexbox.app.overlay.HudOverlayService
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.roundToInt

/**
 * 悬浮框设置页：开关（含权限引导）+ 显示项 + 不透明度/大小/布局 + 锁定。
 *
 * 所有项直接写 [OverlayStore]，悬浮框服务里的 Compose 读的就是这些状态，
 * 改动即时生效，不需要「保存」按钮，也不需要重启服务。
 */
@Composable
fun OverlaySettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var mapsOpen by remember { mutableStateOf(false) }
    // 子页（地图选择）优先吃掉返回键，再才是退出设置页
    BackHandler { if (mapsOpen) mapsOpen = false else onBack() }
    if (mapsOpen) {
        OverlayMapsScreen(onBack = { mapsOpen = false })
        return
    }

    // 通知权限：Android 13+ 前台服务的常驻通知要它。用户不给也不影响悬浮框，所以不阻塞启动
    val notificationPermission =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    fun launchOverlay() {
        OverlayStore.setEnabled(context, true)
        HudOverlayService.start(context)
        val granted = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !granted) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    // 悬浮窗权限是系统级的，只能把人送到设置页；回来后查到授权了才真正启动
    val overlaySettings =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
            if (Settings.canDrawOverlays(context)) launchOverlay()
        }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        // 卡片之间的缝隙：不写这个四张卡会糊成一块
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        DetailTopBar(title = "悬浮框设置", onBack = onBack)
        Spacer(Modifier.height(12.dp))

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(11.dp)) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    IconBadge(Icons.Rounded.PictureInPicture, Accent, size = 40.dp, iconSize = 21.dp)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        Text(
                            "启用悬浮框",
                            fontSize = 14.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "屏幕悬浮显示 CPU / GPU 实时状态",
                            fontSize = 11.5.sp,
                            color = TextSecondary,
                        )
                    }
                    Switch(
                        checked = OverlayStore.enabled,
                        onCheckedChange = { checked ->
                            if (checked) {
                                if (Settings.canDrawOverlays(context)) {
                                    launchOverlay()
                                } else {
                                    overlaySettings.launch(
                                        Intent(
                                            Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                            Uri.parse("package:${context.packageName}"),
                                        ),
                                    )
                                }
                            } else {
                                OverlayStore.setEnabled(context, false)
                                // 全关才停服务：辅助准心可能还开着
                                HudOverlayService.refresh(context)
                            }
                        },
                    )
                }
                if (OverlayStore.enabled) {
                    Text(
                        "通知栏有一条常驻通知，点它上面的「关闭悬浮框」也能关掉。",
                        fontSize = 11.sp,
                        color = TextSecondary,
                    )
                }
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                SectionTitle("显示项")
                // 每一项都可以关，但至少留一项，不然悬浮框成了空壳
                val items = listOf(
                    DisplayItem("CPU 占用", OverlayStore.showCpuUsage) { OverlayStore.setShowCpuUsage(context, it) },
                    DisplayItem("CPU 温度", OverlayStore.showCpuTemp) { OverlayStore.setShowCpuTemp(context, it) },
                    DisplayItem("GPU 温度", OverlayStore.showGpuTemp) { OverlayStore.setShowGpuTemp(context, it) },
                    DisplayItem("内存占用", OverlayStore.showMemory) { OverlayStore.setShowMemory(context, it) },
                    DisplayItem("三角洲每日密码", OverlayStore.showDeltaPasswords) {
                        OverlayStore.setShowDeltaPasswords(context, it)
                    },
                )
                items.forEach { item ->
                    val othersOn = items.count { it.checked } > if (item.checked) 1 else 0
                    ToggleRow(
                        title = item.title,
                        checked = item.checked,
                        canTurnOff = othersOn,
                        onChange = item.onChange,
                    )
                }
                // 地图选择子页入口：跟 PC 端一样按地图单独开关
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .clickable { mapsOpen = true }
                        .padding(vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(
                            "选择地图",
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "悬浮框只显示已选地图的每日密码",
                            fontSize = 11.sp,
                            color = TextSecondary,
                        )
                    }
                    Text(
                        "已选 ${OverlayStore.selectedMaps.size}/${DELTA_MAPS.size}",
                        fontSize = 12.sp,
                        color = TextSecondary,
                    )
                    Spacer(Modifier.width(6.dp))
                    Icon(
                        Icons.AutoMirrored.Rounded.ArrowForward,
                        contentDescription = "选择地图",
                        tint = TextSecondary,
                        modifier = Modifier.size(14.dp),
                    )
                }
                Text(
                    "GPU 占用与逐帧 FPS 被系统限制读取（需 root），故不提供。",
                    fontSize = 11.sp,
                    color = TextSecondary,
                )
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SectionTitle("外观")
                SliderRow(
                    title = "不透明度",
                    valueText = "${OverlayStore.opacityPercent}%",
                    value = OverlayStore.opacityPercent.toFloat(),
                    range = OverlayStore.OPACITY_MIN.toFloat()..OverlayStore.OPACITY_MAX.toFloat(),
                    steps = 19,
                    onChange = { OverlayStore.setOpacity(context, it.roundToInt()) },
                )
                SliderRow(
                    title = "大小",
                    valueText = "${OverlayStore.scalePercent}%",
                    value = OverlayStore.scalePercent.toFloat(),
                    range = OverlayStore.SCALE_MIN.toFloat()..OverlayStore.SCALE_MAX.toFloat(),
                    steps = 8,
                    onChange = { OverlayStore.setScale(context, it.roundToInt()) },
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "布局",
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    FilterChip(
                        selected = !OverlayStore.horizontal,
                        onClick = { OverlayStore.setHorizontal(context, false) },
                        label = { Text("竖排") },
                    )
                    Spacer(Modifier.width(8.dp))
                    FilterChip(
                        selected = OverlayStore.horizontal,
                        onClick = { OverlayStore.setHorizontal(context, true) },
                        label = { Text("横排") },
                    )
                }
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(11.dp)) {
                SectionTitle("位置")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        Text(
                            "锁定位置",
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "锁定后悬浮框不可拖动，触摸直接穿透到下层应用",
                            fontSize = 11.5.sp,
                            color = TextSecondary,
                        )
                    }
                    Switch(
                        checked = OverlayStore.locked,
                        onCheckedChange = { OverlayStore.setLocked(context, it) },
                    )
                }
                OutlinedButton(
                    onClick = {
                        OverlayStore.clearPosition(context)
                        // 服务在跑就顺手让它立刻摆回默认点；没在跑的话下次开启自然用默认
                        if (HudOverlayService.isRunning) {
                            context.startService(
                                Intent(context, HudOverlayService::class.java)
                                    .setAction(HudOverlayService.ACTION_RESET_POSITION),
                            )
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                ) { Text("重置位置") }
            }
        }

        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

/** 分组标题：13sp 半粗，与设置页的分节同一层级 */
@Composable
private fun SectionTitle(title: String) {
    Text(
        title,
        fontSize = 13.sp,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

/** 显示项开关行。最后一项在显示时不允许关掉（canTurnOff = false 时开关禁用），防止框变空壳 */
@Composable
private fun ToggleRow(title: String, checked: Boolean, canTurnOff: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            title,
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.weight(1f),
        )
        Switch(
            checked = checked,
            // 关不掉的场合：已经是最后一项还想去关它
            enabled = !checked || canTurnOff,
            onCheckedChange = onChange,
        )
    }
}

/** 滑杆行：标题 + 当前百分比，离散档位让数值始终是整数 */
@Composable
private fun SliderRow(
    title: String,
    valueText: String,
    value: Float,
    range: ClosedFloatingPointRange<Float>,
    steps: Int,
    onChange: (Float) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                title,
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.weight(1f),
            )
            Text(valueText, fontSize = 12.sp, color = TextSecondary)
        }
        Slider(
            value = value,
            onValueChange = onChange,
            valueRange = range,
            steps = steps,
        )
    }
}

/** 显示项开关的数据行：标题 + 当前态 + 写回 */
private class DisplayItem(
    val title: String,
    val checked: Boolean,
    val onChange: (Boolean) -> Unit,
)

/**
 * 地图选择子页：跟 PC 端一致，按地图单独开关每日密码。
 * 全不选也不拦——「三角洲每日密码」大类开关才是显示与否的总闸。
 */
@Composable
private fun OverlayMapsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    BackHandler { onBack() }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
    ) {
        DetailTopBar(title = "选择地图", onBack = onBack)
        Spacer(Modifier.height(12.dp))

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                SectionTitle("每日密码地图")
                DELTA_MAPS.forEach { map ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            map,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurface,
                            modifier = Modifier.weight(1f),
                        )
                        Switch(
                            checked = map in OverlayStore.selectedMaps,
                            onCheckedChange = { on ->
                                val next = if (on) {
                                    OverlayStore.selectedMaps + map
                                } else {
                                    OverlayStore.selectedMaps - map
                                }
                                OverlayStore.setSelectedMaps(context, next)
                            },
                        )
                    }
                }
                Text(
                    "悬浮框只显示已勾选地图的密码；密码每分钟自动更新。",
                    fontSize = 11.sp,
                    color = TextSecondary,
                )
            }
        }

        Spacer(Modifier.height(8.dp))
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}