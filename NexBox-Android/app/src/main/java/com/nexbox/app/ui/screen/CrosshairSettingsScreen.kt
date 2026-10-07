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
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import com.nexbox.app.data.CROSSHAIR_COLORS
import com.nexbox.app.data.CrosshairPreset
import com.nexbox.app.data.CrosshairStore
import com.nexbox.app.data.CrosshairStyles
import com.nexbox.app.overlay.HudOverlayService
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.FillColor
import com.nexbox.app.ui.theme.TextSecondary
import com.nexbox.app.ui.theme.parseHexColor
import kotlin.math.roundToInt

/**
 * 辅助准心设置页：启用（含悬浮窗权限引导）+ 样式/颜色 + 大小/粗细/透明度 + 位置偏移。
 *
 * 所有项直接写 [CrosshairStore]，服务里的准心面板读的就是这些状态，改动即时生效。
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CrosshairSettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var showSaveDialog by remember { mutableStateOf(false) }
    var presetName by remember { mutableStateOf("") }
    /** 待确认删除的预设：点了 ✕ 先弹确认，防止挤到误删 */
    var deleteTarget by remember { mutableStateOf<CrosshairPreset?>(null) }
    BackHandler { onBack() }

    // 通知权限：Android 13+ 前台服务的常驻通知要它。用户不给也不影响准心，所以不阻塞启动
    val notificationPermission =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
    fun launchCrosshair() {
        CrosshairStore.setEnabled(context, true)
        HudOverlayService.refresh(context)
        val granted = ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.POST_NOTIFICATIONS,
        ) == PackageManager.PERMISSION_GRANTED
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && !granted) {
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    val overlaySettings =
        rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
            if (Settings.canDrawOverlays(context)) launchCrosshair()
        }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        DetailTopBar(title = "辅助准心", onBack = onBack)

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Row(
                modifier = Modifier.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                IconBadge(Icons.Rounded.GpsFixed, Accent, size = 40.dp, iconSize = 21.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        "启用辅助准心",
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "屏幕正中悬浮准星，触摸穿透不影响游戏操作",
                        fontSize = 11.5.sp,
                        color = TextSecondary,
                    )
                }
                Switch(
                    checked = CrosshairStore.enabled,
                    onCheckedChange = { checked ->
                        if (checked) {
                            if (Settings.canDrawOverlays(context)) {
                                launchCrosshair()
                            } else {
                                overlaySettings.launch(
                                    Intent(
                                        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                        Uri.parse("package:${context.packageName}"),
                                    ),
                                )
                            }
                        } else {
                            CrosshairStore.setEnabled(context, false)
                            HudOverlayService.refresh(context)
                        }
                    },
                )
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SectionTitle("样式")
                // 用 FlowRow 而非 Row：窄屏 / 大字体下挤不下的那颗芯片整颗换到第二行。
                // 写在 Row 里时最后一颗只分到两字宽，「圆圈」会被竖排拆成两行。
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    CrosshairStyles.ALL.forEach { style ->
                        FilterChip(
                            selected = CrosshairStore.style == style,
                            onClick = { CrosshairStore.setStyle(context, style) },
                            label = { Text(CrosshairStyles.label(style), maxLines = 1) },
                        )
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "颜色",
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        modifier = Modifier.weight(1f),
                    )
                    CROSSHAIR_COLORS.forEach { hex ->
                        val selected = CrosshairStore.colorHex.equals(hex, ignoreCase = true)
                        androidx.compose.foundation.Canvas(
                            modifier = Modifier
                                .size(26.dp)
                                .clip(CircleShape)
                                .clickable { CrosshairStore.setColor(context, hex) },
                        ) {
                            drawCircle(parseHexColor(hex) ?: Color.Transparent)
                            // 浅色卡片上白色色块会隐身，统一描一圈细边
                            drawCircle(
                                color = Color.Black.copy(alpha = 0.15f),
                                radius = size.minDimension / 2f,
                                style = androidx.compose.ui.graphics.drawscope.Stroke(width = 1.dp.toPx()),
                            )
                            if (selected) {
                                drawCircle(
                                    color = Color.White,
                                    radius = size.minDimension / 2f,
                                    style = androidx.compose.ui.graphics.drawscope.Stroke(width = 2.dp.toPx()),
                                )
                            }
                        }
                        Spacer(Modifier.size(8.dp))
                    }
                }
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SectionTitle("参数")
                SliderRow(
                    title = "大小",
                    valueText = "${CrosshairStore.sizeDp} dp",
                    value = CrosshairStore.sizeDp.toFloat(),
                    range = CrosshairStore.SIZE_MIN.toFloat()..CrosshairStore.SIZE_MAX.toFloat(),
                    steps = CrosshairStore.SIZE_MAX - CrosshairStore.SIZE_MIN - 1,
                    onChange = { CrosshairStore.setSize(context, it.roundToInt()) },
                )
                SliderRow(
                    title = "粗细",
                    valueText = "${CrosshairStore.thicknessDp} dp",
                    value = CrosshairStore.thicknessDp.toFloat(),
                    range = CrosshairStore.THICKNESS_MIN.toFloat()..CrosshairStore.THICKNESS_MAX.toFloat(),
                    steps = CrosshairStore.THICKNESS_MAX - CrosshairStore.THICKNESS_MIN - 1,
                    onChange = { CrosshairStore.setThickness(context, it.roundToInt()) },
                )
                SliderRow(
                    title = "不透明度",
                    valueText = "${CrosshairStore.opacityPercent}%",
                    value = CrosshairStore.opacityPercent.toFloat(),
                    range = CrosshairStore.OPACITY_MIN.toFloat()..CrosshairStore.OPACITY_MAX.toFloat(),
                    steps = (CrosshairStore.OPACITY_MAX - CrosshairStore.OPACITY_MIN) / 5 - 1,
                    onChange = { CrosshairStore.setOpacity(context, it.roundToInt()) },
                )
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SectionTitle("预设")
                if (CrosshairStore.presets.isEmpty()) {
                    Text(
                        "调好样式与参数后保存为预设，下次一键套用。",
                        fontSize = 11.sp,
                        color = TextSecondary,
                    )
                } else {
                    FlowRow(
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        CrosshairStore.presets.forEach { preset ->
                            Row(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(FillColor)
                                    .clickable { CrosshairStore.applyPreset(context, preset) }
                                    .padding(start = 14.dp, end = 6.dp, top = 9.dp, bottom = 9.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    preset.name,
                                    fontSize = 13.sp,
                                    fontWeight = FontWeight.Medium,
                                    color = MaterialTheme.colorScheme.onSurface,
                                )
                                Spacer(Modifier.width(6.dp))
                                Icon(
                                    Icons.Rounded.Close,
                                    contentDescription = "删除预设 ${preset.name}",
                                    tint = TextSecondary,
                                    modifier = Modifier
                                        .size(26.dp)
                                        .clip(CircleShape)
                                        .clickable { deleteTarget = preset }
                                        .padding(6.dp),
                                )
                            }
                        }
                    }
                }
                OutlinedButton(
                    onClick = { showSaveDialog = true },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                ) { Text("保存为预设") }
            }
        }

        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }

    if (showSaveDialog) {
        AlertDialog(
            onDismissRequest = { showSaveDialog = false },
            title = { Text("保存为预设") },
            text = {
                OutlinedTextField(
                    value = presetName,
                    onValueChange = { presetName = it },
                    placeholder = { Text("预设名称") },
                    singleLine = true,
                )
            },
            confirmButton = {
                TextButton(
                    enabled = presetName.isNotBlank(),
                    onClick = {
                        CrosshairStore.savePreset(context, presetName)
                        presetName = ""
                        showSaveDialog = false
                    },
                ) { Text("保存") }
            },
            dismissButton = {
                TextButton(onClick = { showSaveDialog = false }) { Text("取消") }
            },
        )
    }

    deleteTarget?.let { target ->
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            title = { Text("删除预设") },
            text = { Text("确认删除「${target.name}」？删除后无法恢复。") },
            confirmButton = {
                TextButton(onClick = {
                    CrosshairStore.deletePreset(context, target.name)
                    deleteTarget = null
                }) { Text("删除") }
            },
            dismissButton = {
                TextButton(onClick = { deleteTarget = null }) { Text("取消") }
            },
        )
    }
}

/** 分组标题：13sp 半粗，与其他设置页同一层级 */
@Composable
private fun SectionTitle(title: String) {
    Text(
        title,
        fontSize = 13.sp,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

/** 滑杆行：标题 + 当前值文本 */
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