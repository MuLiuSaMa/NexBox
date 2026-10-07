package com.nexbox.app.ui.screen

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
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
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import com.nexbox.app.ui.AppCard
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.nexbox.app.data.CpuIdentity
import com.nexbox.app.data.GpuIdentity
import com.nexbox.app.data.HardwareIdentity
import com.nexbox.app.data.HardwareSnapshot
import com.nexbox.app.data.MemoryStatus
import com.nexbox.app.ui.theme.FillColor
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.Danger
import com.nexbox.app.ui.theme.MetricCpu
import com.nexbox.app.ui.theme.MetricGpu
import com.nexbox.app.ui.theme.TempWarn
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.roundToInt

/** 主页 CPU / GPU 卡片点进去的详情页类型 */
enum class HardwareDetailKind { CPU, GPU }

/**
 * CPU / GPU 详情页。
 *
 * 实时数据来自 WS 每秒一帧的硬件快照（[HardwareUiState.snapshot]），
 * 趋势图吃 [HardwareUiState.cpuHistory] / [HardwareUiState.gpuHistory] 的滚动采样，
 * 型号信息来自 `hw.info`（[HardwareUiState.identity]，老 PC 端为 null 时降级不显示）。
 * 任何传感器都可能缺失：一律显示「—」，不用 0 冒充。
 */
@Composable
fun HardwareDetailScreen(
    kind: HardwareDetailKind,
    state: HardwareUiState,
    onBack: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        DetailTopBar(
            title = if (kind == HardwareDetailKind.CPU) "CPU 详情" else "GPU 详情",
            onBack = onBack,
            trailingText = "实时刷新",
        )

        when (kind) {
            HardwareDetailKind.CPU -> CpuDetailBody(state)
            HardwareDetailKind.GPU -> GpuDetailBody(state)
        }

        Text(
            "数据来自 PC 端传感器（每秒刷新），读不到的项目显示「—」；趋势为最近 90 秒占用率。",
            fontSize = 11.sp,
            color = TextSecondary,
            lineHeight = 16.sp,
        )
        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

// ───────────────────────── CPU ─────────────────────────

@Composable
private fun CpuDetailBody(state: HardwareUiState) {
    val snap = state.snapshot

    UsageHeroCard(
        title = "CPU 占用",
        percent = snap?.cpuUsage,
        history = state.cpuHistory,
        color = MetricCpu,
    )

    StatGrid(
        listOf(
            StatItem("温度", fmtTemp(snap?.cpuTemp), tempTint(snap?.cpuTemp)),
            StatItem("功耗", fmtWatt(snap?.cpuPower?.let { it.roundToInt() })),
            StatItem("频率", fmtClock(snap?.cpuClock)),
            StatItem("电压", fmtVoltage(snap?.cpuVoltage)),
            StatItem("风扇转速", fmtFan(snap?.cpuFanSpeed)),
            StatItem("内存占用", state.memory.percentOrDash()),
        ),
    )

    DetailSectionLabel("型号信息")
    CpuIdentityCard(state.identity?.cpu)
}

/** CPU 型号卡：名称 + 核心线程 / 频率 / 插槽 / 缓存 */
@Composable
private fun CpuIdentityCard(cpu: CpuIdentity?) {
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        if (cpu == null) {
            Column(Modifier.padding(14.dp)) {
                Text("暂无型号信息", fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = TextSecondary)
                Text(
                    "型号信息需要较新版本的 PC 端提供，升级后自动显示。",
                    fontSize = 11.sp,
                    color = TextSecondary,
                    lineHeight = 16.sp,
                )
            }
        } else {
            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(9.dp)) {
                Text(
                    cpu.name.ifBlank { "未知 CPU" },
                    fontSize = 15.sp,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                    lineHeight = 20.sp,
                )
                SpecRow("厂商", cpu.manufacturer.ifBlank { null })
                SpecRow(
                    "核心 / 线程",
                    if (cpu.cores != null || cpu.threads != null) {
                        "${cpu.cores ?: "—"} / ${cpu.threads ?: "—"}"
                    } else null,
                )
                SpecRow("最大频率", cpu.maxClockMhz?.let { fmtClock(it) })
                SpecRow("插槽", cpu.socket.ifBlank { null })
                SpecRow("L3 缓存", cpu.l3CacheKb?.let { fmtCacheKb(it) })
            }
        }
    }
}

// ───────────────────────── GPU ─────────────────────────

/**
 * GPU 详情：只展示**当前使用中**的那块显卡（WS 快照里的 gpu_* 字段本就是
 * active_gpu_index 那块卡的读数），不铺多显卡列表。
 */
@Composable
private fun GpuDetailBody(state: HardwareUiState) {
    val snap = state.snapshot
    val activeName = snap?.gpuSensors?.getOrNull(snap.activeGpuIndex)?.name
    val gpuIdentity = matchGpuIdentity(state.identity, activeName, snap?.activeGpuIndex ?: 0)

    // 显卡名：型号信息里的名字更规整，缺失时退回传感器（LHM）里的名字
    val displayName = gpuIdentity?.name?.takeIf { it.isNotBlank() }
        ?: activeName?.takeIf { it.isNotBlank() }
        ?: "当前显卡"

    UsageHeroCard(
        title = "GPU 占用",
        percent = snap?.gpuUsage,
        history = state.gpuHistory,
        color = MetricGpu,
    )

    VramCard(snap)

    StatGrid(
        listOf(
            StatItem("温度", fmtTemp(snap?.gpuTemp), tempTint(snap?.gpuTemp)),
            StatItem("功耗", fmtWatt(snap?.gpuPower)),
            StatItem("核心频率", fmtClock(snap?.gpuClock)),
            StatItem("显存频率", fmtClock(snap?.gpuMemoryClock)),
            StatItem("电压", fmtVoltage(snap?.gpuVoltage)),
            StatItem("风扇转速", fmtFan(snap?.gpuFanSpeed)),
        ),
    )

    DetailSectionLabel("型号信息")
    GpuIdentityCard(name = displayName, gpu = gpuIdentity)
}

/** 显存占用条：已用 / 总量 + 占比条 */
@Composable
private fun VramCard(snap: HardwareSnapshot?) {
    val used = snap?.gpuVramUsed
    val total = snap?.gpuVramTotal
    val percent = if (used != null && total != null && total > 0) {
        ((used.toDouble() / total) * 100).roundToInt()
    } else null

    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("显存", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = TextSecondary)
                Spacer(Modifier.weight(1f))
                Text(
                    if (used != null && total != null) {
                        "${oneDecimal(used / 1024.0)} / ${oneDecimal(total / 1024.0)} GB"
                    } else "—",
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            DetailBar(percent = percent, color = MetricGpu)
        }
    }
}

/** 显卡型号卡：名称 + 厂商 / 显存 / 驱动 / 分辨率 */
@Composable
private fun GpuIdentityCard(name: String, gpu: GpuIdentity?) {
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(9.dp)) {
            Text(
                name,
                fontSize = 15.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurface,
                lineHeight = 20.sp,
            )
            if (gpu == null) {
                Text(
                    "型号详情需要较新版本的 PC 端提供，升级后自动显示。",
                    fontSize = 11.sp,
                    color = TextSecondary,
                    lineHeight = 16.sp,
                )
            } else {
                SpecRow("厂商", gpu.vendor.takeIf { it.isNotBlank() && it != "Unknown" })
                SpecRow("显存容量", gpu.memoryGb?.let { "${oneDecimal(it)} GB" })
                SpecRow("显存类型", gpu.videoMemoryType?.takeIf { it.isNotBlank() })
                SpecRow("驱动版本", gpu.driverVersion.takeIf { it.isNotBlank() })
                SpecRow("驱动日期", gpu.driverDate.takeIf { it.isNotBlank() })
                SpecRow("分辨率", formatResolution(gpu))
            }
        }
    }
}

/** 型号信息行见同文件的共享组件 [SpecRow]（配置页本机信息卡也在用） */

// ───────────────────────── 通用组件 ─────────────────────────

/** 顶部大数字 + 最近 90 秒占用率趋势 */
@Composable
private fun UsageHeroCard(
    title: String,
    percent: Int?,
    history: List<Int?>,
    color: Color,
) {
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(6.dp).clip(CircleShape).background(color))
                Spacer(Modifier.width(6.dp))
                Text(title, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = TextSecondary)
                Spacer(Modifier.weight(1f))
                Text("近 90 秒", fontSize = 11.sp, color = TextSecondary)
            }

            Row(verticalAlignment = Alignment.Bottom) {
                Text(
                    percent?.let { "$it" } ?: "—",
                    fontSize = 44.sp,
                    fontWeight = FontWeight.Bold,
                    color = if (percent == null) TextSecondary else MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    "%",
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Medium,
                    color = TextSecondary,
                    modifier = Modifier.padding(bottom = 6.dp),
                )
            }

            UsageChart(values = history, color = color, modifier = Modifier.fillMaxWidth().height(96.dp))
        }
    }
}

/**
 * 占用率趋势折线：空值（刚连上还没采到点、传感器缺失）按段断开，
 * 不做插值——插出来的线会被当成真实读数。
 */
@Composable
private fun UsageChart(values: List<Int?>, color: Color, modifier: Modifier = Modifier) {
    // Canvas 的绘制 lambda 不是组合作用域，语义色要先在外部取好
    val gridLine = Hairline
    Canvas(
        modifier = modifier
            .clip(RoundedCornerShape(10.dp))
            .background(FillColor),
    ) {
        val w = size.width
        val h = size.height

        // 0 / 50 / 100 参考线
        listOf(0f, 0.5f, 1f).forEach { f ->
            val y = h * (1f - f)
            drawLine(
                gridLine,
                start = androidx.compose.ui.geometry.Offset(0f, y),
                end = androidx.compose.ui.geometry.Offset(w, y),
                strokeWidth = 1.dp.toPx(),
            )
        }

        if (values.size < 2) return@Canvas
        val step = w / (values.size - 1)

        // 按连续非空分段
        val runs = mutableListOf<IntRange>()
        var start = -1
        values.forEachIndexed { i, v ->
            if (v != null) {
                if (start < 0) start = i
            } else if (start >= 0) {
                runs.add(start until i)
                start = -1
            }
        }
        if (start >= 0) runs.add(start..values.lastIndex)

        fun pointX(i: Int): Float = step * i
        fun pointY(v: Int): Float = h * (1f - v.coerceIn(0, 100) / 100f)

        for (run in runs) {
            if (run.count() >= 2) {
                val linePath = Path()
                val fillPath = Path()
                run.forEachIndexed { k, idx ->
                    val x = pointX(idx)
                    val y = pointY(values[idx] ?: 0)
                    if (k == 0) {
                        linePath.moveTo(x, y)
                        fillPath.moveTo(x, h)
                        fillPath.lineTo(x, y)
                    } else {
                        linePath.lineTo(x, y)
                        fillPath.lineTo(x, y)
                    }
                }
                fillPath.lineTo(pointX(run.last), h)
                fillPath.close()

                drawPath(
                    fillPath,
                    brush = Brush.verticalGradient(
                        colors = listOf(color.copy(alpha = 0.30f), color.copy(alpha = 0f)),
                    ),
                )
                drawPath(
                    linePath,
                    color = color,
                    style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
                )
            } else {
                // 孤点也要看得见
                val idx = run.first
                drawCircle(
                    color = color,
                    radius = 2.5.dp.toPx(),
                    center = androidx.compose.ui.geometry.Offset(pointX(idx), pointY(values[idx] ?: 0)),
                )
            }
        }
    }
}

/** 一行两列的指标格子 */
private data class StatItem(val label: String, val value: String, val valueColor: Color? = null)

@Composable
private fun StatGrid(items: List<StatItem>, columns: Int = 2) {
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        items.chunked(columns).forEach { rowItems ->
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                rowItems.forEach { item -> StatCell(item, Modifier.weight(1f)) }
                // 尾行不足整列时补空位，格子不串位
                repeat(columns - rowItems.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun StatCell(item: StatItem, modifier: Modifier = Modifier) {
    AppCard(
        modifier = modifier,
        shape = RoundedCornerShape(14.dp),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(item.label, fontSize = 11.sp, color = TextSecondary)
            Text(
                item.value,
                fontSize = 16.sp,
                fontWeight = FontWeight.SemiBold,
                color = item.valueColor ?: MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

@Composable
private fun DetailSectionLabel(text: String) {
    Text(
        text,
        fontSize = 13.sp,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.85f),
        modifier = Modifier.padding(top = 2.dp),
    )
}

/** 占用条：percent 为空只留底槽 */
@Composable
private fun DetailBar(percent: Int?, color: Color) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(5.dp)
            .clip(RoundedCornerShape(50))
            .background(FillColor),
    ) {
        if (percent != null) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(percent.coerceIn(0, 100) / 100f)
                    .fillMaxHeight()
                    .clip(RoundedCornerShape(50))
                    .background(color),
            )
        }
    }
}

// ───────────────────────── 型号匹配 / 格式化 ─────────────────────────

/**
 * 把 WS 快照里「当前 GPU」（LibreHardwareMonitor 的硬件名）对到 `hw.info` 的型号条目。
 * 两个来源的名称写法可能略有出入（前后缀厂商名、括号等），先精确后模糊包含；
 * 都对不上时按索引兜底，只有一块显卡时直接用它。
 */
private fun matchGpuIdentity(
    identity: HardwareIdentity?,
    activeName: String?,
    activeIndex: Int,
): GpuIdentity? {
    val gpus = identity?.gpus.orEmpty()
    if (gpus.isEmpty()) return null

    val key = activeName?.let { normalizeGpuName(it) }.orEmpty()
    if (key.isNotBlank()) {
        gpus.firstOrNull { normalizeGpuName(it.name) == key }?.let { return it }
        gpus.firstOrNull {
            val n = normalizeGpuName(it.name)
            n.isNotBlank() && (n.contains(key) || key.contains(n))
        }?.let { return it }
    }
    return gpus.getOrNull(activeIndex) ?: if (gpus.size == 1) gpus[0] else null
}

/** 名称归一化：忽略大小写与非字母数字，方便「NVIDIA NVIDIA GeForce…」这类写法对上 */
private fun normalizeGpuName(name: String): String =
    name.lowercase().filter { it.isLetterOrDigit() }

/** "2560 × 1440 @ 165Hz"；只有分辨率没有刷新率就省略后半段 */
private fun formatResolution(gpu: GpuIdentity): String? {
    val w = gpu.resolutionWidth
    val h = gpu.resolutionHeight
    if (w == null || h == null || w <= 0 || h <= 0) return null
    val base = "$w × $h"
    val hz = gpu.refreshRate?.takeIf { it > 0 }
    return if (hz != null) "$base @ ${hz}Hz" else base
}

private fun MemoryStatus?.percentOrDash(): String = this?.percent?.let { "$it%" } ?: "—"

/** 温度配色：80°C 起提示，90°C 起报警（与主页卡片同标准） */
@Composable
private fun tempTint(t: Double?): Color = when {
    t == null -> TextSecondary
    t >= 90.0 -> Danger
    t >= 80.0 -> TempWarn
    else -> TextSecondary
}

private fun fmtTemp(t: Double?): String = t?.let { "${it.roundToInt()}°C" } ?: "—"

private fun fmtWatt(w: Int?): String = w?.let { "${it}W" } ?: "—"

/** 频率：≥1000 MHz 折成 GHz 显示，和 PC 端悬浮框的读数习惯一致 */
private fun fmtClock(mhz: Int?): String = when {
    mhz == null -> "—"
    mhz >= 1000 -> "${twoDecimal(mhz / 1000.0)} GHz"
    else -> "$mhz MHz"
}

/** 缓存：≥1024 KB 折成 MB */
private fun fmtCacheKb(kb: Int): String = when {
    kb >= 1024 -> "${oneDecimal(kb / 1024.0)} MB"
    kb > 0 -> "$kb KB"
    else -> "—"
}

private fun fmtVoltage(v: Double?): String = v?.let { "${threeDecimal(it)}V" } ?: "—"

private fun fmtFan(rpm: Int?): String = rpm?.let { "$it RPM" } ?: "—"

/** 保留小数不用 String.format：部分地区会把小数点输出成逗号 */
private fun oneDecimal(v: Double): String = (kotlin.math.round(v * 10) / 10).toString()
private fun twoDecimal(v: Double): String = (kotlin.math.round(v * 100) / 100).toString()
private fun threeDecimal(v: Double): String = (kotlin.math.round(v * 1000) / 1000).toString()
