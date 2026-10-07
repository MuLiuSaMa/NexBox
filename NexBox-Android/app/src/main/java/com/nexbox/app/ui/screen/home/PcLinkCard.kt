package com.nexbox.app.ui.screen.home

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.FilterAlt
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material.icons.rounded.PictureInPicture
import androidx.compose.material.icons.rounded.SportsEsports
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.nexbox.app.data.DiskStatus
import com.nexbox.app.data.HardwareSnapshot
import com.nexbox.app.data.MemoryStatus
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.screen.HardwareUiState
import com.nexbox.app.ui.screen.MetricBar
import com.nexbox.app.ui.screen.PcControlUiState
import com.nexbox.app.ui.screen.PcFeature
import com.nexbox.app.ui.screen.tempColor
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.MetricCpu
import com.nexbox.app.ui.theme.MetricGpu
import com.nexbox.app.ui.theme.MetricMemory
import com.nexbox.app.ui.theme.MetricStorage
import com.nexbox.app.ui.theme.TempWarn
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.roundToInt

/** 互联卡的状态机：未配对 / 搜索中 / 等待电脑确认 / 已配对但通道断开 / 已连接 */
enum class LinkPhase { IDLE, SEARCHING, AWAITING, PAIRED_OFFLINE, CONNECTED }

/** 配对成功的对勾颜色 */
private val SuccessGreen = Color(0xFF3FBF6F)

/**
 * 两种形态里「一行文字」的统一高度。
 *
 * 卡片上下高度一致靠的是两边总高相等：
 * 未配对 = 主体(170) + 一行文字 + 按钮(40)；已配对 = 一行文字 + 主体。
 * 把文字行钉成固定高度，是为了不让「连接前后大小不一样」重新出现 ——
 * 否则它俩的高度会取决于字体行高与状态胶囊的 padding，改个字号就悄悄错开。
 */
private val CardTextRowHeight = 26.dp

/**
 * 已配对态主体（硬件四宫格）的高度。
 *
 * = 未配对态主体(170) + 「添加设备」按钮(40) + 一处间距(10)。
 * 因为「断开连接」挪到了顶部行里，已配对态底部不再有按钮，这 50dp 就补给面板，
 * 让两种形态的卡片总高仍然相等。改顶部行结构时记得一起核算这个数。
 */
private val PairedGridHeight = 220.dp

/**
 * 第二页开关格子的高度。
 *
 * 刻意比格子宽度（一行 3 个约 91dp）小，做成**横向长方形**而不是方格子。
 * 两行各 70dp 用 `SpaceEvenly` 分布在 220dp 里，上下留白均匀 ——
 * 主体高度不动，所以第一页的四宫格不会跟着变形，卡片总高也仍然和未配对态一致。
 */
private val PcTileHeight = 70.dp

/**
 * PC 互联卡：一整块卡片，两种形态结构对称、卡片高度一致。
 * - **连接前**：循环播放完整配对故事 ——
 *   开场电脑就在正中间放大、屏幕上是一个标准二维码（三个定位角 + 数据点阵）；
 *   手机从底部升到屏幕上，射出光锥扫码（扫描线从上往下扫过二维码，扫过的模块点亮）；
 *   扫完两台设备向左右小幅错开（电脑不缩放），中间画出「配对成功」对勾 + 粒子迸发，
 *   淡出进入下一轮。真实状态由下方文字行说明。
 * - **连接后**：顶部一行是电脑名称 + 「已连接」状态 + 右上角「断开连接」，
 *   主体是**可左右翻页的两页**（高度都是 [PairedGridHeight]，卡片总高因此不变）：
 *   第一页是硬件四宫格（CPU / GPU 格进图表详情页，内存格点一下执行「一键内存优化」），
 *   第二页是四个 PC 功能开关（游戏模式 / 滤镜 / 准心 / 悬浮框）。
 *
 * 两态都是「主体 + 一行文字 + 一个按钮」的等价高度，所以卡片上下高度完全一致 ——
 * 改任何一边的结构时，记得让另一边保持同样的骨架，否则又会出现「连接前后大小不一样」。
 */
@Composable
fun PcLinkCard(
    phase: LinkPhase,
    pcName: String,
    targetName: String?,
    hwState: HardwareUiState,
    controlState: PcControlUiState,
    controlsEnabled: Boolean,
    unpairBusy: Boolean,
    onAdd: () -> Unit,
    onOpenCpuDetail: () -> Unit,
    onOpenGpuDetail: () -> Unit,
    onMemoryClean: () -> Unit,
    onToggleFeature: (PcFeature, Boolean) -> Unit,
    onUnpair: () -> Unit,
    modifier: Modifier = Modifier,
) {
    // 只有未配对时整卡是个大按钮（点哪都开「添加设备」）；
    // 已配对后整卡不再跳转，跳转交给卡内各个格子
    val cardClick: (() -> Unit)? = if (phase == LinkPhase.IDLE) onAdd else null

    AppCard(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .then(if (cardClick != null) Modifier.clickable(onClick = cardClick) else Modifier),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(
            Modifier.padding(horizontal = 16.dp, vertical = 14.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (phase == LinkPhase.IDLE) {
                LinkScene(phase = phase, modifier = Modifier.fillMaxWidth().height(170.dp))

                // 文字与按钮整体上移一点，不贴卡片底部
                Column(
                    Modifier.offset(y = (-5).dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    IdleHint()

                    // 未配对：添加设备入口放在卡片里，圆角矩形小按钮居中
                    Button(
                        onClick = onAdd,
                        modifier = Modifier.align(Alignment.CenterHorizontally),
                        shape = RoundedCornerShape(12.dp),
                        contentPadding = PaddingValues(horizontal = 24.dp, vertical = 10.dp),
                    ) {
                        Icon(Icons.Rounded.Add, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(6.dp))
                        Text("添加设备", fontWeight = FontWeight.SemiBold)
                    }
                }
            } else {
                // 已配对：顶部一行是「名称 + 已连接胶囊 + 断开连接（右上角）」，
                // 主体是硬件四宫格；面板高度已补足底部按钮消失的部分，卡片总高不变
                LinkHeader(
                    phase = phase,
                    pcName = pcName,
                    targetName = targetName,
                    unpairBusy = unpairBusy,
                    onUnpair = onUnpair,
                )
                PcPagesPanel(
                    hwState = hwState,
                    controlState = controlState,
                    controlsEnabled = controlsEnabled,
                    onCpuDetail = onOpenCpuDetail,
                    onGpuDetail = onOpenGpuDetail,
                    onMemoryClean = onMemoryClean,
                    onToggleFeature = onToggleFeature,
                    modifier = Modifier.fillMaxWidth().height(PairedGridHeight),
                )
            }
        }
    }
}

/** 已配对卡片的顶部行：左边电脑名称，右边「已连接 / 重连中」胶囊 + 右上角「断开连接」 */
@Composable
private fun LinkHeader(
    phase: LinkPhase,
    pcName: String,
    targetName: String?,
    unpairBusy: Boolean,
    onUnpair: () -> Unit,
) {
    val connected = phase == LinkPhase.CONNECTED
    val name = when (phase) {
        LinkPhase.SEARCHING, LinkPhase.AWAITING -> targetName.orEmpty().ifBlank { "我的电脑" }
        else -> pcName.ifBlank { "我的电脑" }
    }
    Row(
        modifier = Modifier.fillMaxWidth().height(CardTextRowHeight),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            name,
            fontSize = 13.5.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        LinkStatusChip(connected)
        Spacer(Modifier.width(8.dp))
        UnpairButton(busy = unpairBusy, onClick = onUnpair)
    }
}

/**
 * 卡片右上角的「断开连接」。
 * 压成 22dp 高的小号描边按钮 —— 必须显式限高，否则 Material3 Button 自带的
 * 40dp 最小高度会把 [CardTextRowHeight] 撑破，卡片就又和未配对态不一样高了。
 */
@Composable
private fun UnpairButton(busy: Boolean, onClick: () -> Unit) {
    OutlinedButton(
        onClick = onClick,
        enabled = !busy,
        modifier = Modifier.height(22.dp),
        shape = RoundedCornerShape(50),
        contentPadding = PaddingValues(horizontal = 10.dp, vertical = 0.dp),
    ) {
        Text(
            if (busy) "断开中…" else "断开连接",
            fontSize = 11.5.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
        )
    }
}

/** 顶部状态胶囊：已连接（主题色）/ 重连中（警示色） */
@Composable
private fun LinkStatusChip(connected: Boolean) {
    val color = if (connected) Accent else TempWarn
    Row(
        modifier = Modifier
            .clip(RoundedCornerShape(50))
            .background(color.copy(alpha = 0.14f))
            .padding(horizontal = 8.dp, vertical = 3.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(6.dp).clip(CircleShape).background(color))
        Spacer(Modifier.width(5.dp))
        Text(if (connected) "已连接" else "重连中", fontSize = 10.5.sp, color = color)
    }
}

// ───────────────────────── 卡内硬件状态 ─────────────────────────

// ───────────────────────── 卡内两页：硬件状态 / PC 功能开关 ─────────────────────────

/**
 * 已配对卡片的主体：左右两页，可横向滑动翻页。
 *
 * 高度严格等于 [PairedGridHeight]，两页都 `fillMaxSize()`；翻页圆点用 Box 叠在底部，
 * **不参与 Column 测量** —— 这样卡片总高与未配对态仍然一致。
 *
 * 手势说明：横向拖动由 Pager 接管，卡内格子的 `clickable` 只消费 tap，两者不冲突；
 * 与外面 `HomeContent` 的纵向滚动轴向正交，也不冲突。
 */
@Composable
private fun PcPagesPanel(
    hwState: HardwareUiState,
    controlState: PcControlUiState,
    controlsEnabled: Boolean,
    onCpuDetail: () -> Unit,
    onGpuDetail: () -> Unit,
    onMemoryClean: () -> Unit,
    onToggleFeature: (PcFeature, Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val pager = rememberPagerState(pageCount = { 2 })
    Box(modifier) {
        HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { page ->
            if (page == 0) {
                HardwareMiniGrid(
                    state = hwState,
                    onCpuDetail = onCpuDetail,
                    onGpuDetail = onGpuDetail,
                    onMemoryClean = onMemoryClean,
                    modifier = Modifier.fillMaxSize(),
                )
            } else {
                PcToggleGrid(
                    state = controlState,
                    enabled = controlsEnabled,
                    onToggle = onToggleFeature,
                    modifier = Modifier.fillMaxSize(),
                )
            }
        }
        // 页码点叠在卡片下边缘上：往下移出主体区、落进卡片的 padding 里，正好压在边框内侧。
        // 不占高度，也不加 clickable —— 否则这一小片会把翻页手势吃掉
        PagerDots(
            current = pager.currentPage,
            count = 2,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .offset(y = 12.dp),
        )
    }
}

/** 翻页指示：两个小圆点，当前页用主题色 */
@Composable
private fun PagerDots(current: Int, count: Int, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        repeat(count) { index ->
            val on = index == current
            Box(
                Modifier
                    .size(if (on) 6.dp else 5.dp)
                    .clip(CircleShape)
                    .background(
                        if (on) Accent
                        else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.4f),
                    ),
            )
        }
    }
}

/**
 * 第二页：四个 PC 功能开关，2×2 网格。
 * 与第一页的指标格同构（`weight(1f).fillMaxHeight()`），所以两页高度天然一致。
 */
@Composable
private fun PcToggleGrid(
    state: PcControlUiState,
    enabled: Boolean,
    onToggle: (PcFeature, Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier, verticalArrangement = Arrangement.SpaceEvenly) {
        // 第一行 3 个
        Row(Modifier.height(PcTileHeight), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PcToggleTile(
                label = "游戏模式",
                icon = Icons.Rounded.SportsEsports,
                on = state.gameModeOn,
                busy = state.busy == PcFeature.GAME_MODE,
                enabled = enabled,
                unsupported = false,
                onClick = { onToggle(PcFeature.GAME_MODE, !state.gameModeOn) },
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
            PcToggleTile(
                label = "滤镜",
                icon = Icons.Rounded.FilterAlt,
                on = state.filterOn,
                busy = state.busy == PcFeature.FILTER,
                enabled = enabled,
                unsupported = false,
                onClick = { onToggle(PcFeature.FILTER, !state.filterOn) },
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
            PcToggleTile(
                label = "准心",
                icon = Icons.Rounded.GpsFixed,
                on = state.crosshairOn,
                busy = state.busy == PcFeature.CROSSHAIR,
                enabled = enabled && !state.crosshairUnsupported,
                unsupported = state.crosshairUnsupported,
                onClick = { onToggle(PcFeature.CROSSHAIR, !state.crosshairOn) },
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
        }
        // 第二行只剩一个，后面补两个空位保持列对齐
        Row(Modifier.height(PcTileHeight), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PcToggleTile(
                label = "悬浮框",
                icon = Icons.Rounded.PictureInPicture,
                on = state.overlayOn,
                busy = state.busy == PcFeature.OVERLAY,
                enabled = enabled && !state.overlayUnsupported,
                unsupported = state.overlayUnsupported,
                onClick = { onToggle(PcFeature.OVERLAY, !state.overlayOn) },
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
            Spacer(Modifier.weight(1f))
            Spacer(Modifier.weight(1f))
        }
    }
}

/**
 * 单个功能开关：图标 + 文字，开着时**整块填充主题色**（图标与文字转 onPrimary）。
 * 关闭态用与第一页指标格一致的浅色底，两页观感统一。
 */
@Composable
private fun PcToggleTile(
    label: String,
    icon: ImageVector,
    on: Boolean,
    busy: Boolean,
    enabled: Boolean,
    unsupported: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val shape = RoundedCornerShape(12.dp)
    val container = when {
        !enabled -> MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.22f)
        on -> accent
        else -> MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.35f)
    }
    // 实心主题色底上必须用 onPrimary：浅色主题下主题色是深蓝，白字才立得住
    val contentColor = when {
        !enabled -> TextSecondary
        on -> MaterialTheme.colorScheme.onPrimary
        else -> TextSecondary
    }
    Column(
        modifier = modifier
            .clip(shape)
            .background(container)
            .then(if (enabled && !busy) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 8.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        // 图标与文字横着排一行 —— 一行要放 3 个，竖排就挤不下了
        Row(verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, contentDescription = null, tint = contentColor, modifier = Modifier.size(17.dp))
            Spacer(Modifier.width(4.dp))
            Text(
                label,
                fontSize = 11.5.sp,
                fontWeight = if (on) FontWeight.SemiBold else FontWeight.Normal,
                color = contentColor,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        if (unsupported) {
            Spacer(Modifier.height(2.dp))
            Text("需升级 PC 端", fontSize = 9.sp, color = TextSecondary, maxLines = 1)
        }
    }
}

/**
 * 卡内硬件状态四宫格（CPU / GPU / 内存 / 存储）。
 * 数值全部可空 —— 传感器缺失或还没回数据时显示「—」，不用 0 顶替。
 * 数据与图表详情页同源（[HardwareUiState]），不额外开连接。
 *
 * 交互：CPU / GPU 格点进各自的图表详情页；内存格点一下执行 PC 端「一键内存优化」
 * （执行中锁住防连点）；存储格暂无动作，不可点。
 */
@Composable
private fun HardwareMiniGrid(
    state: HardwareUiState,
    onCpuDetail: () -> Unit,
    onGpuDetail: () -> Unit,
    onMemoryClean: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val snap = state.snapshot
    // 内存优先用 mem.status（sysinfo 直读），缺失时退回 WS 的 LHM 传感器值
    val memoryPercent = state.memory?.percent ?: snap?.memoryUsage?.roundToInt()

    Column(modifier, verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MiniMetric(
                label = "CPU",
                color = MetricCpu,
                percent = snap?.cpuUsage,
                temp = snap?.cpuTemp,
                footnote = snap?.cpuPower?.let { "功耗 ${it.roundToInt()}W" } ?: "—",
                hint = "详情 ›",
                onClick = onCpuDetail,
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
            MiniMetric(
                label = "GPU",
                color = MetricGpu,
                percent = snap?.gpuUsage,
                temp = snap?.gpuTemp,
                footnote = gpuFootnote(snap),
                hint = "详情 ›",
                onClick = onGpuDetail,
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
        }
        Row(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            MiniMetric(
                label = "内存",
                color = MetricMemory,
                percent = memoryPercent,
                temp = null,
                footnote = memoryFootnote(state.memory),
                hint = if (state.memoryBusy) "清理中…" else "点击清理 ›",
                onClick = onMemoryClean.takeIf { !state.memoryBusy },
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
            MiniMetric(
                label = "存储",
                color = MetricStorage,
                percent = state.disk?.usagePercent?.roundToInt(),
                temp = snap?.ssdTemp,
                footnote = diskFootnote(state.disk),
                hint = null,
                onClick = null,
                modifier = Modifier.weight(1f).fillMaxHeight(),
            )
        }
    }
}

/** 卡内紧凑指标格：圆点 + 标签 + 右上温度 / 大数字占用率 / 占用条 / 脚注 + 右侧动作提示 */
@Composable
private fun MiniMetric(
    label: String,
    color: Color,
    percent: Int?,
    temp: Double?,
    footnote: String,
    modifier: Modifier = Modifier,
    hint: String? = null,
    onClick: (() -> Unit)? = null,
) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(12.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.35f))
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 10.dp, vertical = 10.dp),
        verticalArrangement = Arrangement.SpaceBetween,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(color))
            Spacer(Modifier.width(5.dp))
            Text(
                label,
                fontSize = 11.sp,
                fontWeight = FontWeight.SemiBold,
                color = TextSecondary,
                maxLines = 1,
            )
            Spacer(Modifier.weight(1f))
            if (temp != null) {
                Text(
                    "${temp.roundToInt()}°C",
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Medium,
                    color = tempColor(temp),
                )
            }
        }

        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                percent?.let { "$it" } ?: "—",
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
                color = if (percent == null) TextSecondary else MaterialTheme.colorScheme.onSurface,
            )
            Text(
                "%",
                fontSize = 11.sp,
                color = TextSecondary,
                modifier = Modifier.padding(bottom = 3.dp),
            )
        }

        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            MetricBar(percent = percent, color = color)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    footnote,
                    fontSize = 9.5.sp,
                    color = TextSecondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (hint != null) {
                    Text(
                        hint,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Medium,
                        color = color,
                        maxLines = 1,
                    )
                }
            }
        }
    }
}

/** GPU 脚注：优先显存占用，缺失时退回功耗 */
private fun gpuFootnote(snap: HardwareSnapshot?): String {
    if (snap == null) return "—"
    val used = snap.gpuVramUsed
    val total = snap.gpuVramTotal
    if (used != null && total != null && total > 0) {
        return "显存 ${oneDecimal(used / 1024.0)} / ${oneDecimal(total / 1024.0)} GB"
    }
    return snap.gpuPower?.let { "功耗 ${it}W" } ?: "—"
}

/** 内存脚注：已用 / 总量 */
private fun memoryFootnote(m: MemoryStatus?): String {
    val used = m?.usedGb
    val total = m?.totalGb
    if (used == null || total == null) return "—"
    return "已用 ${oneDecimal(used)} / ${oneDecimal(total)} GB"
}

/** 存储脚注：已用 / 总量 */
private fun diskFootnote(d: DiskStatus?): String {
    if (d == null) return "—"
    return "已用 ${oneDecimal(d.usedGb)} / ${oneDecimal(d.totalGb)} GB"
}

/** 保留一位小数，不用 String.format，避免某些地区把小数点输出成逗号 */
private fun oneDecimal(v: Double): String = (kotlin.math.round(v * 10) / 10).toString()

/** 时间轴分段：t 落在 [a,b) 内归一化，段外夹边 */
private fun seg(t: Float, a: Float, b: Float): Float = ((t - a) / (b - a)).coerceIn(0f, 1f)

/** 三次缓入缓出 */
private fun ease01(f: Float): Float = if (f < 0.5f) 4f * f * f * f else 1f - ((-2f * f + 2f).pow3() / 2f)

private fun Float.pow3(): Float = this * this * this

/** 0→1→0 三角包络 */
private fun sin01(f: Float): Float = 1f - kotlin.math.abs(f * 2f - 1f)

private fun lerpF(a: Float, b: Float, f: Float): Float = a + (b - a) * f

/**
 * 循环故事动画（4.2s 一轮）：
 * 0.00–0.28 手机从底部升到屏幕上 → 0.28–0.58 扫码（光锥 + QR 扫描线）→
 * 0.58–0.75 左右小幅错开（电脑平移不缩放）→ 0.75–0.92 对勾出现 + 迸发 →
 * 0.92–1.00 淡出衔接下一轮。
 *
 * 只有未配对（[LinkPhase.IDLE]）时才会被 [PcLinkCard] 调用：已配对后卡片主体换成了
 * [HardwareMiniGrid]，所以下面 connected / offline 的静态信息图分支实际不再走到 ——
 * 保留仅作兜底（万一别处传进已配对相位，仍能画出静止画面，而不是留一片空屏）。
 */
@Composable
private fun LinkScene(phase: LinkPhase, modifier: Modifier = Modifier) {
    // 语义色是 @Composable getter，先在组合期取值，绘制块里只用普通 Color
    val accent = Accent
    val warn = TempWarn
    val onSurface = MaterialTheme.colorScheme.onSurface

    val transition = rememberInfiniteTransition(label = "link-story")
    val t by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(4_200, easing = LinearEasing)),
        label = "story-progress",
    )

    Canvas(modifier) {
        val connected = phase == LinkPhase.CONNECTED
        val offline = phase == LinkPhase.PAIRED_OFFLINE

        val color = when (phase) {
            LinkPhase.CONNECTED, LinkPhase.AWAITING -> accent
            LinkPhase.SEARCHING -> accent.copy(alpha = 0.9f)
            LinkPhase.PAIRED_OFFLINE -> warn
            else -> accent
        }

        // ── 静态信息卡：连接后/离线，不读任何动画值，绘制停摆 ──
        if (connected || offline) {
            drawStaticInfo(color, onSurface, connected)
            return@Canvas
        }

        // ── 循环故事时间轴 ──
        val env = minOf(seg(t, 0f, 0.05f), 1f - seg(t, 0.94f, 1f)) // 循环接缝淡入淡出
        val rise = ease01(seg(t, 0.02f, 0.28f))                    // 手机从底部升上来
        val scan = seg(t, 0.28f, 0.58f)                            // 扫码
        val separate = ease01(seg(t, 0.58f, 0.75f))                // 左右小幅错开
        fun Color.e(): Color = copy(alpha = alpha * env)

        val w = size.width
        val h = size.height
        val cx = w / 2

        // ── 电脑：开场就在中央放大；扫完只向右平移一小段，不缩放 ──
        val phoneW = 40.dp.toPx()
        val phoneH = 64.dp.toPx()
        val monW = 150.dp.toPx()
        val monH = 104.dp.toPx()
        val pcCy = h * 0.34f
        val pcShift = separate * 72.dp.toPx()    // 电脑向右的小位移
        val phoneShift = separate * 72.dp.toPx() // 手机向左的小位移
        val monCx = cx + pcShift
        val monCy = pcCy
        val monLeft = monCx - monW / 2
        val monTop = monCy - monH / 2

        // ── 手机：升到屏幕正中（二维码中间），扫完向左小幅移开 ──
        val phoneCx = cx - phoneShift
        val phoneScanCy = monCy
        val phoneCy = lerpF(h + 30.dp.toPx(), phoneScanCy, rise)
        val phoneLeft = phoneCx - phoneW / 2
        val phoneTop = phoneCy - phoneH / 2

        // ── 机身（微透明底，二维码透出来）──
        drawRoundRect(
            color.e().copy(alpha = 0.07f),
            topLeft = Offset(monLeft, monTop),
            size = Size(monW, monH),
            cornerRadius = CornerRadius(6.dp.toPx(), 6.dp.toPx()),
            style = Fill,
        )
        drawRoundRect(
            color.e(),
            topLeft = Offset(monLeft, monTop),
            size = Size(monW, monH),
            cornerRadius = CornerRadius(6.dp.toPx(), 6.dp.toPx()),
            style = Stroke(width = 2.dp.toPx()),
        )
        val standTop = monTop + monH
        drawLine(color.e(), Offset(monCx, standTop), Offset(monCx, standTop + 8.dp.toPx()), 3.dp.toPx(), StrokeCap.Round)
        drawLine(
            color.e(),
            Offset(monCx - 13.dp.toPx(), standTop + 8.dp.toPx()),
            Offset(monCx + 13.dp.toPx(), standTop + 8.dp.toPx()),
            3.dp.toPx(), StrokeCap.Round,
        )

        // ── 标准二维码：正方形、屏幕正中；手机升到二维码中间扫它 ──
        val checkProg = seg(t, 0.56f, 0.70f) // 扫完瞬间：二维码淡出、两块屏幕上弹对勾
        val qrSide = minOf(monW, monH * 0.58f)
        val qrLeft = monCx - qrSide / 2
        val qrTop = monTop + (monH - qrSide) / 2
        val qrRight = qrLeft + qrSide
        val qrBottom = qrTop + qrSide
        val finderS = qrSide * 0.32f
        val qrAlphaMul = 1f - checkProg

        fun finder(fx: Float, fy: Float) {
            // 外框
            drawRoundRect(
                color.copy(alpha = color.alpha * env * qrAlphaMul),
                topLeft = Offset(fx, fy),
                size = Size(finderS, finderS),
                cornerRadius = CornerRadius(2.dp.toPx(), 2.dp.toPx()),
                style = Stroke(width = 2.dp.toPx()),
            )
            // 中心实心块
            drawRoundRect(
                color.copy(alpha = color.alpha * env * qrAlphaMul),
                topLeft = Offset(fx + finderS * 0.30f, fy + finderS * 0.30f),
                size = Size(finderS * 0.40f, finderS * 0.40f),
                cornerRadius = CornerRadius(1.dp.toPx(), 1.dp.toPx()),
            )
        }
        finder(qrLeft, qrTop)                            // 左上
        finder(qrRight - finderS, qrTop)                 // 右上
        finder(qrLeft, qrBottom - finderS)               // 左下

        // 数据点阵：7×7 完整网格，只避开三个定位角（不缺格，二维码才是完整的）
        val gridN = 7
        val cell = qrSide / gridN
        val finderRects = listOf(
            Offset(qrLeft, qrTop), Offset(qrRight - finderS, qrTop), Offset(qrLeft, qrBottom - finderS),
        )
        for (r in 0 until gridN) {
            for (c in 0 until gridN) {
                val cellL = qrLeft + c * cell
                val cellT = qrTop + r * cell
                val inFinder = finderRects.any { f ->
                    cellL < f.x + finderS + 1.dp.toPx() && cellL + cell > f.x - 1.dp.toPx() &&
                        cellT < f.y + finderS + 1.dp.toPx() && cellT + cell > f.y - 1.dp.toPx()
                }
                if (inFinder) continue
                // 扫描线扫过的模块点亮
                val scanned = scan <= 0f || cellT + cell * 0.5f <= qrTop + qrSide * scan
                drawRoundRect(
                    color.e().copy(alpha = (if (scanned) 0.95f else 0.4f) * qrAlphaMul),
                    topLeft = Offset(cellL + cell * 0.22f, cellT + cell * 0.22f),
                    size = Size(cell * 0.56f, cell * 0.56f),
                    cornerRadius = CornerRadius(1.dp.toPx(), 1.dp.toPx()),
                )
            }
        }

        // ── 扫描线（明显的亮线 + 光带）+ 取景框角标 ──
        val scanY = qrTop + qrSide * scan
        if (scan > 0f && scan < 1f) {
            drawRoundRect(
                color.e().copy(alpha = 0.22f),
                topLeft = Offset(qrLeft, (scanY - 5.dp.toPx()).coerceAtLeast(qrTop)),
                size = Size(qrSide, 10.dp.toPx()),
                cornerRadius = CornerRadius(3.dp.toPx(), 3.dp.toPx()),
            )
            drawLine(
                color.e(),
                Offset(qrLeft + 2.dp.toPx(), scanY),
                Offset(qrRight - 2.dp.toPx(), scanY),
                strokeWidth = 2.2.dp.toPx(),
                cap = StrokeCap.Round,
            )
            // 四角取景框：扫码时出现
            val ba = minOf(scan * 4f, 1f)
            val bl = 9.dp.toPx()
            val bInset = 3.dp.toPx()
            val bx0 = qrLeft - bInset
            val by0 = qrTop - bInset
            val bx1 = qrRight + bInset
            val by1 = qrBottom + bInset
            fun bracket(cx2: Float, cy2: Float, dx: Float, dy: Float) {
                drawLine(
                    color.e().copy(alpha = ba),
                    Offset(cx2, cy2), Offset(cx2 + dx * bl, cy2),
                    strokeWidth = 2.dp.toPx(), cap = StrokeCap.Round,
                )
                drawLine(
                    color.e().copy(alpha = ba),
                    Offset(cx2, cy2), Offset(cx2, cy2 + dy * bl),
                    strokeWidth = 2.dp.toPx(), cap = StrokeCap.Round,
                )
            }
            bracket(bx0, by0, 1f, 1f)
            bracket(bx1, by0, -1f, 1f)
            bracket(bx0, by1, 1f, -1f)
            bracket(bx1, by1, -1f, -1f)
            // 手机 → 扫描线 的光束：明确是"手机在扫"
            drawLine(
                color.e().copy(alpha = 0.55f),
                Offset(phoneCx, phoneTop + 3.dp.toPx()),
                Offset(phoneCx, scanY),
                strokeWidth = 1.4.dp.toPx(),
                cap = StrokeCap.Round,
            )
        }

        // ── 手机本体（屏幕里画它自己的扫描画面）──
        drawPhone(
            color.e(),
            screenGlow = 0.15f + scan * 0.35f,
            topLeft = Offset(phoneLeft, phoneTop),
            width = phoneW,
            height = phoneH,
            scanFrac = if (scan > 0f && scan < 1f) scan else null,
        )

        // ── 扫完：电脑屏幕和手机屏幕上各弹出一个绿色对勾（二维码已淡出）──
        if (checkProg > 0f) {
            // 电脑屏：正中间
            drawCheckBadge(
                cx = monCx,
                cy = monCy,
                r = monH * 0.22f,
                prog = checkProg,
                fade = env,
            )
            // 手机屏：正中
            drawCheckBadge(
                cx = phoneCx,
                cy = phoneCy,
                r = 13.dp.toPx(),
                prog = checkProg,
                fade = env,
            )
        }
    }
}

/** 绿色对勾徽章：随 [prog] 弹入（缩放）+ 白色对勾落笔，[fade] 控制循环末尾淡出 */
private fun DrawScope.drawCheckBadge(cx: Float, cy: Float, r: Float, prog: Float, fade: Float) {
    val rr = r * (0.4f + 0.6f * ease01(prog))
    drawCircle(SuccessGreen.copy(alpha = fade), radius = rr, center = Offset(cx, cy))
    val checkT = ((prog - 0.45f) / 0.55f).coerceIn(0f, 1f)
    if (checkT <= 0f) return
    val p0 = Offset(cx - rr * 0.42f, cy + rr * 0.05f)
    val p1 = Offset(cx - rr * 0.08f, cy + rr * 0.34f)
    val p2 = Offset(cx + rr * 0.46f, cy - rr * 0.30f)
    val f1 = (checkT / 0.45f).coerceIn(0f, 1f)
    val f2 = ((checkT - 0.45f) / 0.55f).coerceIn(0f, 1f)
    drawLine(
        Color.White.copy(alpha = fade),
        p0, Offset(lerpF(p0.x, p1.x, f1), lerpF(p0.y, p1.y, f1)),
        2.dp.toPx(), StrokeCap.Round,
    )
    if (f2 > 0f) {
        drawLine(
            Color.White.copy(alpha = fade),
            p1, Offset(lerpF(p1.x, p2.x, f2), lerpF(p1.y, p2.y, f2)),
            2.dp.toPx(), StrokeCap.Round,
        )
    }
}

/** 连接后/离线的静态信息卡：直线 + 手机 + 显示器（屏幕上是"信息行"），零动画 */
private fun DrawScope.drawStaticInfo(color: Color, onSurface: Color, connected: Boolean) {
    val cy = size.height * 0.58f
    val phoneW = 26.dp.toPx()
    val phoneH = 44.dp.toPx()
    val phoneLeft = 30.dp.toPx()
    val monW = 48.dp.toPx()
    val monH = 32.dp.toPx()
    val monLeft = size.width - 30.dp.toPx() - monW
    val monTop = cy - monH / 2 - 6.dp.toPx()

    drawLine(
        color.copy(alpha = if (connected) 0.7f else 0.5f),
        Offset(phoneLeft + phoneW + 12.dp.toPx(), cy),
        Offset(monLeft - 10.dp.toPx(), cy),
        strokeWidth = 2.dp.toPx(),
        cap = StrokeCap.Round,
    )
    drawPhone(
        color = color,
        screenGlow = if (connected) 0.3f else 0.12f,
        topLeft = Offset(phoneLeft, cy - phoneH / 2),
        width = phoneW,
        height = phoneH,
    )
    drawRoundRect(
        color = color.copy(alpha = if (connected) 0.16f else 0.08f),
        topLeft = Offset(monLeft, monTop),
        size = Size(monW, monH),
        cornerRadius = CornerRadius(5.dp.toPx(), 5.dp.toPx()),
        style = Fill,
    )
    drawRoundRect(
        color = color,
        topLeft = Offset(monLeft, monTop),
        size = Size(monW, monH),
        cornerRadius = CornerRadius(5.dp.toPx(), 5.dp.toPx()),
        style = Stroke(width = 1.8.dp.toPx()),
    )
    val monCx = monLeft + monW / 2
    drawLine(color, Offset(monCx, monTop + monH), Offset(monCx, monTop + monH + 7.dp.toPx()), 2.5.dp.toPx(), StrokeCap.Round)
    drawLine(
        color,
        Offset(monCx - 11.dp.toPx(), monTop + monH + 7.dp.toPx()),
        Offset(monCx + 11.dp.toPx(), monTop + monH + 7.dp.toPx()),
        2.5.dp.toPx(), StrokeCap.Round,
    )
    val innerLeft = monLeft + 6.dp.toPx()
    val innerTop = monTop + 7.dp.toPx()
    val innerW = monW - 12.dp.toPx()
    drawRoundRect(
        color = if (connected) color else onSurface.copy(alpha = 0.15f),
        topLeft = Offset(innerLeft, innerTop),
        size = Size(innerW * 0.62f, 3.dp.toPx()),
        cornerRadius = CornerRadius(1.5.dp.toPx(), 1.5.dp.toPx()),
    )
    drawRoundRect(
        color = if (connected) color.copy(alpha = 0.6f) else onSurface.copy(alpha = 0.15f),
        topLeft = Offset(innerLeft, innerTop + 7.dp.toPx()),
        size = Size(innerW * 0.4f, 3.dp.toPx()),
        cornerRadius = CornerRadius(1.5.dp.toPx(), 1.5.dp.toPx()),
    )
}

/** 手机矢量：圆角机身 + 亮屏 + （扫码时的）迷你取景器 + 听筒 + home 点 */
private fun DrawScope.drawPhone(
    color: Color,
    screenGlow: Float,
    topLeft: Offset,
    width: Float,
    height: Float,
    scanFrac: Float? = null,
) {
    val corner = CornerRadius(6.dp.toPx(), 6.dp.toPx())
    drawRoundRect(
        color = color.copy(alpha = 0.08f),
        topLeft = topLeft,
        size = Size(width, height),
        cornerRadius = corner,
        style = Fill,
    )
    drawRoundRect(
        color = color,
        topLeft = topLeft,
        size = Size(width, height),
        cornerRadius = corner,
        style = Stroke(width = 1.8.dp.toPx()),
    )
    drawRoundRect(
        color = color.copy(alpha = screenGlow.coerceIn(0f, 0.6f)),
        topLeft = Offset(topLeft.x + 4.dp.toPx(), topLeft.y + 9.dp.toPx()),
        size = Size(width - 8.dp.toPx(), height - 16.dp.toPx()),
        cornerRadius = CornerRadius(3.dp.toPx(), 3.dp.toPx()),
        style = Fill,
    )
    // 扫描画面：迷你取景器（小 QR 点阵 + 扫描线），手机自己"正在扫"的画面
    if (scanFrac != null) {
        val vLeft = topLeft.x + 5.dp.toPx()
        val vTop = topLeft.y + 11.dp.toPx()
        val vW = width - 10.dp.toPx()
        val vH = height - 18.dp.toPx()
        for (r in 0 until 4) {
            for (c in 0 until 3) {
                if ((r * 5 + c * 3) % 4 == 0) continue
                drawRoundRect(
                    color = color.copy(alpha = 0.45f),
                    topLeft = Offset(vLeft + c * vW / 3 + vW / 12, vTop + r * vH / 4 + vH / 10),
                    size = Size(vW / 6, vH / 8),
                    cornerRadius = CornerRadius(0.8.dp.toPx(), 0.8.dp.toPx()),
                )
            }
        }
        val ly = vTop + vH * scanFrac
        drawLine(
            color,
            Offset(vLeft, ly),
            Offset(vLeft + vW, ly),
            strokeWidth = 1.2.dp.toPx(),
            cap = StrokeCap.Round,
        )
    }
    drawLine(
        color,
        Offset(topLeft.x + width * 0.35f, topLeft.y + 5.dp.toPx()),
        Offset(topLeft.x + width * 0.65f, topLeft.y + 5.dp.toPx()),
        strokeWidth = 1.6.dp.toPx(),
        cap = StrokeCap.Round,
    )
    drawCircle(
        color,
        radius = 1.5.dp.toPx(),
        center = Offset(topLeft.x + width / 2, topLeft.y + height - 4.dp.toPx()),
    )
}

/** 未配对时卡片底部的一行引导语（已配对后卡片走 [LinkHeader]，不再经过这里） */
@Composable
private fun IdleHint() {
    Box(
        modifier = Modifier.fillMaxWidth().height(CardTextRowHeight),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            "连接到新境盒PC端",
            fontSize = 13.5.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
            textAlign = TextAlign.Center,
        )
    }
}
