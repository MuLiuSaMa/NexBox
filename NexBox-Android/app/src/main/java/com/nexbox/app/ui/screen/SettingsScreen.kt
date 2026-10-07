package com.nexbox.app.ui.screen

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.BlurOn
import androidx.compose.material.icons.rounded.Colorize
import androidx.compose.material.icons.rounded.Done
import androidx.compose.material.icons.rounded.Image
import androidx.compose.material.icons.rounded.Palette
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import com.nexbox.app.BuildConfig
import com.nexbox.app.data.AppearanceStore
import com.nexbox.app.data.BackgroundStore
import com.nexbox.app.data.DefaultAccentHex
import com.nexbox.app.data.ThemeMode
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.AccentPresets
import com.nexbox.app.ui.theme.Danger
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.PageBg
import com.nexbox.app.ui.theme.PageBgTop
import com.nexbox.app.ui.theme.TextSecondary
import com.nexbox.app.ui.theme.accentContentColor
import com.nexbox.app.ui.theme.parseHexColor
import com.nexbox.app.update.UpdateManager

/** 设置页：主题模式 / 主题色 / 外观 / 液态玻璃 / 关于 */
@Composable
fun SettingsScreen() {
    val context = LocalContext.current
    var pickingAccent by remember { mutableStateOf(false) }

    SettingsList(onOpenPicker = { pickingAccent = true })

    if (pickingAccent) {
        AccentPickerDialog(
            initialHex = AppearanceStore.accent,
            defaultHex = DefaultAccentHex,
            onApply = { hex ->
                AppearanceStore.setAccent(context, hex)
                pickingAccent = false
            },
            onDismiss = { pickingAccent = false },
        )
    }
}

@Composable
private fun SettingsList(onOpenPicker: () -> Unit) {
    val context = LocalContext.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Spacer(Modifier.height(2.dp))
        Text(
            "设置",
            fontSize = 20.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onBackground,
        )

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SectionHead(Icons.Rounded.Palette, "主题模式")
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    ThemeMode.entries.forEach { mode ->
                        ModeChip(
                            label = mode.label,
                            selected = AppearanceStore.themeMode == mode,
                            modifier = Modifier.weight(1f),
                        ) { AppearanceStore.setThemeMode(context, mode) }
                    }
                }
                Text(
                    "跟随系统时按系统的深色设置切换",
                    fontSize = 11.5.sp,
                    color = TextSecondary,
                )
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SectionHead(Icons.Rounded.Colorize, "主题色")
                // 色块尺寸按可用宽度算，但**封顶 44dp**：
                // 平板 / 横屏下卡片很宽，纯 weight(1f) 会把这一排色块撑成一堆大圆饼。
                BoxWithConstraints {
                    val gap = 10.dp
                    // 末尾还有一个方形调色盘入口，一起参与均分
                    val count = AccentPresets.size + 1
                    val swatch = minOf((maxWidth - gap * (count - 1)) / count, 44.dp)
                    Row(horizontalArrangement = Arrangement.spacedBy(gap)) {
                        AccentPresets.forEach { hex ->
                            val color = parseHexColor(hex)
                            if (color != null) {
                                AccentSwatch(
                                    color = color,
                                    selected = hex.equals(AppearanceStore.accent, ignoreCase = true),
                                    modifier = Modifier.size(swatch),
                                ) { AppearanceStore.setAccent(context, hex) }
                            }
                        }
                        // 方形是为了和圆形的预设区分开：它是入口，不是颜色
                        PaletteSquare(modifier = Modifier.size(swatch), onClick = onOpenPicker)
                    }
                }
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SectionHead(Icons.Rounded.Image, "外观")
                BackgroundPicker()
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                SectionHead(Icons.Rounded.BlurOn, "液态玻璃")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                        Text(
                            "玻璃质感卡片",
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                        Text(
                            "开启后卡片与底部导航栏变为液态玻璃，导航栏为胶囊式",
                            fontSize = 11.5.sp,
                            color = TextSecondary,
                        )
                    }
                    Switch(
                        checked = AppearanceStore.glassEnabled,
                        onCheckedChange = { AppearanceStore.setGlassEnabled(context, it) },
                    )
                }
            }
        }

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                SectionHead(icon = null, title = "关于")
                AppNameRow()
                HorizontalDivider(color = Hairline)
                InfoRow("版本", "v${BuildConfig.VERSION_NAME}")
                UpdateRow()
                InfoRow("作者", "木流")
                SiteRow()
                OpenSourceRow()
            }
        }

        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

/** 卡片标题：小图标 + 13sp 标题，和硬件页的分节保持同一层级 */
@Composable
private fun SectionHead(icon: ImageVector?, title: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = Accent, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(7.dp))
        }
        Text(
            title,
            fontSize = 13.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

@Composable
private fun ModeChip(label: String, selected: Boolean, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(12.dp))
            .background(
                if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.16f)
                else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f),
            )
            .border(
                width = if (selected) 1.dp else 0.dp,
                color = if (selected) MaterialTheme.colorScheme.primary else Hairline,
                shape = RoundedCornerShape(12.dp),
            )
            .clickable(onClick = onClick)
            .padding(vertical = 11.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = 12.5.sp,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            color = if (selected) MaterialTheme.colorScheme.primary else TextSecondary,
        )
    }
}

/** 背景图选择：相册选图后拷进应用私有目录，重启仍然生效 */
@Composable
private fun BackgroundPicker() {
    val context = LocalContext.current
    val bgFile = BackgroundStore.file
    // 选图读不到数据（云端图片、相册权限失效等）时要有可见反馈，
    // 否则点了「更换背景图」界面一动不动，用户只会认为没生效
    var bgError by remember { mutableStateOf<String?>(null) }
    val pickBackground = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia(),
    ) { uri ->
        if (uri == null) {
            bgError = null
        } else {
            bgError = if (BackgroundStore.setFromUri(context, uri)) {
                null
            } else {
                "读取所选图片失败，背景未更换。图片若存放在云端，请先下载到本机再试"
            }
        }
    }

    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            Modifier
                .size(width = 84.dp, height = 52.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(Brush.verticalGradient(listOf(PageBgTop, PageBg))),
        ) {
            if (bgFile != null) {
                AsyncImage(
                    model = bgFile,
                    contentDescription = null,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                if (bgFile != null) "自定义背景" else "默认背景",
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                if (bgFile != null) "整应用生效" else "跟随主题模式的渐变色",
                fontSize = 11.5.sp,
                color = TextSecondary,
            )
        }
    }

    OutlinedButton(
        onClick = {
            pickBackground.launch(
                PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly),
            )
        },
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(12.dp),
    ) { Text(if (bgFile != null) "更换背景图" else "选择背景图") }

    if (bgFile != null) {
        OutlinedButton(
            onClick = {
                BackgroundStore.clear()
                bgError = null
            },
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(12.dp),
        ) { Text("恢复默认背景") }
    }

    bgError?.let { Text(it, fontSize = 11.5.sp, color = Danger) }
}

@Composable
private fun AppNameRow() {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            "新境盒-安卓端",
            fontSize = 14.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.width(7.dp))
        Box(
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.16f))
                .padding(horizontal = 6.dp, vertical = 2.dp),
        ) {
            Text(
                "BETA",
                fontSize = 10.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun SiteRow() {
    val uriHandler = LocalUriHandler.current
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("官网", fontSize = 13.sp, color = TextSecondary)
        Spacer(Modifier.weight(1f))
        Text(
            "nexbox.cn",
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable { uriHandler.openUri("https://nexbox.cn") }
                .padding(horizontal = 4.dp, vertical = 2.dp),
        )
    }
}

/** 开源致谢：液态玻璃效果来自 Kyant0 的 AndroidLiquidGlass（backdrop 库）。 */
@Composable
private fun OpenSourceRow() {
    val uriHandler = LocalUriHandler.current
    HorizontalDivider(color = Hairline)
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Text("开源项目", fontSize = 13.sp, color = TextSecondary)
        Text(
            "Kyant0/AndroidLiquidGlass",
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable {
                    uriHandler.openUri("https://github.com/Kyant0/AndroidLiquidGlass/tree/android")
                }
                .padding(horizontal = 4.dp, vertical = 2.dp),
        )
        Text(
            "使用了该项目的液态玻璃效果",
            fontSize = 11.5.sp,
            color = TextSecondary,
        )
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, fontSize = 13.sp, color = TextSecondary)
        Spacer(Modifier.weight(1f))
        Text(
            value,
            fontSize = 13.sp,
            color = MaterialTheme.colorScheme.onSurface,
            fontWeight = FontWeight.Medium,
        )
    }
}

/**
 * 「检查更新」行：右侧跟随更新流程状态变化；空闲 / 已最新时可点发起手动检查，
 * 其余状态点了只是把进行中的更新弹窗重新拉起来。
 */
@Composable
private fun UpdateRow() {
    val s = UpdateManager.state
    val value: String
    val clickable: Boolean
    when (s) {
        is UpdateManager.State.Checking -> { value = "检查中…"; clickable = false }
        is UpdateManager.State.Available -> { value = "发现 v${s.info.version}"; clickable = true }
        is UpdateManager.State.Downloading -> { value = "下载中 ${s.progress}%"; clickable = false }
        is UpdateManager.State.Downloaded -> { value = "待安装"; clickable = true }
        is UpdateManager.State.Failed -> { value = "失败，点我重试"; clickable = true }
        UpdateManager.State.Idle, UpdateManager.State.UpToDate -> { value = "检查更新"; clickable = true }
    }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("检查更新", fontSize = 13.sp, color = TextSecondary)
        Spacer(Modifier.weight(1f))
        Text(
            value,
            fontSize = 13.sp,
            color = if (clickable) MaterialTheme.colorScheme.primary else TextSecondary,
            fontWeight = FontWeight.Medium,
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable(enabled = clickable) { UpdateManager.manualCheck() }
                .padding(horizontal = 4.dp, vertical = 2.dp),
        )
    }
}

/** 预设色块：选中时套一圈跟随主题的描边，深浅色背景上都看得见 */
@Composable
private fun AccentSwatch(
    color: Color,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    Box(
        modifier = modifier
            .border(
                width = if (selected) 2.dp else 0.dp,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.85f),
                shape = CircleShape,
            )
            .padding(if (selected) 3.dp else 0.dp)
            .clip(CircleShape)
            .background(color)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        if (selected) {
            Icon(
                Icons.Rounded.Done,
                contentDescription = "已选",
                tint = accentContentColor(color),
                modifier = Modifier.size(15.dp),
            )
        }
    }
}

/** 调色盘入口：方形 + 彩虹渐变，和圆形的预设色块一眼分开 */
@Composable
private fun PaletteSquare(modifier: Modifier = Modifier, onClick: () -> Unit) {
    val spectrum = AccentPresets.mapNotNull { parseHexColor(it) }
    Box(
        modifier = modifier
            .clip(RoundedCornerShape(9.dp))
            .background(Brush.linearGradient(spectrum))
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(9.dp))
            .clickable(onClickLabel = "打开调色盘", onClick = onClick),
    )
}
