package com.nexbox.app.ui.screen

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.nexbox.app.data.SpeedStage
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.MessageToast
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.FillColor
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.MetricMemory
import com.nexbox.app.ui.theme.TempWarn
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.ceil
import kotlin.math.roundToInt

/**
 * 网络测速页（工具页进入）。
 *
 * 测的是**本机网络**，与 PC 端无关：数据全部由 [SpeedTestEngine] 在本地跑出来，
 * 指标与算法对齐 PC 端（延迟 / 抖动 / 丢包 / 下载 / 上传），页面带下载与上传两条实时曲线。
 *
 * 曲线上传/下载用的是主题色：主题里没有 PC 端那种绿，下载取品牌色、上传取指标紫。
 */
@Composable
fun SpeedTestScreen(vm: SpeedTestViewModel = viewModel(), onBack: () -> Unit) {
    val state by vm.state.collectAsStateWithLifecycle()

    BackHandler { onBack() }
    // 离开页面必须停测：viewModel() 挂在 Activity 上，切 tab 不会销毁它，
    // 不停的话测速会在后台继续跑，白白吃流量
    DisposableEffect(Unit) { onDispose { vm.stop() } }

    Box(Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 18.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            DetailTopBar(
                title = "网络测速",
                onBack = onBack,
                trailingText = stageLabel(state.stage),
            )

            // ── 下载 / 上传大数字 ──
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                SpeedMeter(
                    label = "下载",
                    value = state.downloadMbps,
                    color = Accent,
                    active = state.stage == SpeedStage.DOWNLOAD || state.stage == SpeedStage.DONE,
                    // 延迟阶段还没轮到下载，显示「—」
                    measured = state.stage == SpeedStage.DOWNLOAD ||
                        state.stage == SpeedStage.UPLOAD ||
                        state.stage == SpeedStage.DONE,
                    modifier = Modifier.weight(1f),
                )
                SpeedMeter(
                    label = "上传",
                    value = state.uploadMbps,
                    color = MetricMemory,
                    active = state.stage == SpeedStage.UPLOAD || state.stage == SpeedStage.DONE,
                    // 上传排在最后：下载跑完之前它一直是「—」，不是不测
                    measured = state.stage == SpeedStage.UPLOAD || state.stage == SpeedStage.DONE,
                    modifier = Modifier.weight(1f),
                )
            }

            // ── 实时速率曲线 ──
            val peak = maxOf(
                state.downloadPoints.maxOrNull() ?: 0f,
                state.uploadPoints.maxOrNull() ?: 0f,
            )
            AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
                Column(
                    Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        LegendDot(Accent, "下载")
                        Spacer(Modifier.width(12.dp))
                        LegendDot(MetricMemory, "上传")
                        Spacer(Modifier.weight(1f))
                        Text(
                            if (peak > 0f) "峰值 ${formatSpeed(peak.toDouble())} Mbps" else "实时速率",
                            fontSize = 11.sp,
                            color = TextSecondary,
                        )
                    }
                    SpeedChart(
                        download = state.downloadPoints,
                        upload = state.uploadPoints,
                        stage = state.stage,
                        modifier = Modifier.fillMaxWidth().height(150.dp),
                    )
                }
            }

            // ── 延迟 / 抖动 / 丢包 ──
            val measured = state.stage != SpeedStage.IDLE
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                SpeedMetricCell(
                    label = "延迟",
                    value = state.pingMs,
                    unit = "ms",
                    measured = measured,
                    modifier = Modifier.weight(1f),
                )
                SpeedMetricCell(
                    label = "抖动",
                    value = state.jitterMs,
                    unit = "ms",
                    measured = measured,
                    modifier = Modifier.weight(1f),
                )
                SpeedMetricCell(
                    label = "丢包",
                    value = state.packetLossPct,
                    unit = "%",
                    measured = measured,
                    // 丢包 0 是有意义的读数，不能像延迟那样把 0 显示成「—」
                    showZero = true,
                    modifier = Modifier.weight(1f),
                )
            }

            // ── 测速服务器 ──
            AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
                Column(
                    Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        "测速服务器",
                        fontSize = 13.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.85f),
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        vm.servers.forEach { server ->
                            FilterChip(
                                selected = state.selectedServer == server.id,
                                onClick = { vm.selectServer(server.id) },
                                enabled = !state.running,
                                label = { Text(server.shortName, fontSize = 11.5.sp, maxLines = 1) },
                            )
                        }
                    }
                    Text(
                        "上传与延迟统一走南航节点；下载失败会自动切换到备用源。",
                        fontSize = 10.5.sp,
                        color = TextSecondary,
                        lineHeight = 15.sp,
                    )
                }
            }

            // ── 开始 / 停止 ──
            Button(
                onClick = { if (state.running) vm.stop() else vm.start() },
                modifier = Modifier.fillMaxWidth().height(48.dp),
                shape = RoundedCornerShape(14.dp),
                colors = if (state.running) {
                    ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)
                } else {
                    ButtonDefaults.buttonColors()
                },
            ) {
                Text(
                    if (state.running) "停止" else "开始测速",
                    fontSize = 15.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }

            // 跑完却一个字节都没拿到时，把引擎给的具体原因显示出来，别让人对着 0 猜
            if (state.stage == SpeedStage.DONE && state.note.isNotBlank()) {
                Text(
                    "${state.note}，换个节点再试。",
                    fontSize = 11.sp,
                    color = TempWarn,
                    lineHeight = 16.sp,
                )
            }

            Text(
                "测的是本机网络，与电脑无关。全程约 15 秒，会消耗较多流量，建议在 Wi-Fi 下进行。",
                fontSize = 11.sp,
                color = TextSecondary,
                lineHeight = 16.sp,
            )

            Spacer(Modifier.height(8.dp))
            // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
            Spacer(Modifier.navigationBarsPadding().height(96.dp))
        }

        MessageToast(
            message = state.error,
            isError = true,
            onDismiss = vm::consumeError,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .statusBarsPadding()
                .padding(top = 8.dp, end = 12.dp)
                .fillMaxWidth(0.86f),
        )
    }
}

// ───────────────────────── 组件 ─────────────────────────

/**
 * 下载 / 上传大数字。
 * - [active]：是不是正在测这一项，不是就压暗
 * - [measured]：这项是否已经测到过数据。没测到一律显示「—」——
 *   否则下载阶段上传那栏会显示 0，看着像「不测上传」
 */
@Composable
private fun SpeedMeter(
    label: String,
    value: Double,
    color: Color,
    active: Boolean,
    measured: Boolean,
    modifier: Modifier = Modifier,
) {
    AppCard(modifier = modifier, shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier
                        .size(6.dp)
                        .clip(CircleShape)
                        .background(if (active) color else color.copy(alpha = 0.35f)),
                )
                Spacer(Modifier.width(6.dp))
                Text(
                    label,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = TextSecondary,
                )
            }
            Row(verticalAlignment = Alignment.Bottom) {
                Text(
                    if (measured) formatSpeed(value) else "—",
                    fontSize = 32.sp,
                    fontWeight = FontWeight.Bold,
                    color = if (active) {
                        MaterialTheme.colorScheme.onSurface
                    } else {
                        TextSecondary
                    },
                )
                Text(
                    "Mbps",
                    fontSize = 11.sp,
                    color = TextSecondary,
                    modifier = Modifier.padding(start = 3.dp, bottom = 6.dp),
                )
            }
        }
    }
}

/** 延迟 / 抖动 / 丢包的小格 */
@Composable
private fun SpeedMetricCell(
    label: String,
    value: Double,
    unit: String,
    measured: Boolean,
    modifier: Modifier = Modifier,
    showZero: Boolean = false,
) {
    AppCard(modifier = modifier, shape = RoundedCornerShape(14.dp)) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(label, fontSize = 11.sp, color = TextSecondary)
            Row(verticalAlignment = Alignment.Bottom) {
                Text(
                    when {
                        !measured -> "—"
                        showZero -> value.roundToInt().toString()
                        value <= 0.0 -> "—"
                        else -> oneDecimal(value)
                    },
                    fontSize = 18.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    unit,
                    fontSize = 10.sp,
                    color = TextSecondary,
                    modifier = Modifier.padding(start = 2.dp, bottom = 2.dp),
                )
            }
        }
    }
}

/** 曲线图例的小圆点 + 文字 */
@Composable
private fun LegendDot(color: Color, label: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(7.dp).clip(CircleShape).background(color))
        Spacer(Modifier.width(5.dp))
        Text(label, fontSize = 11.sp, color = TextSecondary)
    }
}

/**
 * 下载 / 上传实时速率双曲线。
 *
 * 单序列、Y 轴写死 0–100 的 `UsageChart`（硬件详情页那个）用不上，所以这里单独写：
 * 双序列、Float、Y 轴按两条曲线的最大值自适应。
 * Path 用 remember 复用，避免 80ms 一帧反复分配对象。
 */
@Composable
private fun SpeedChart(
    download: List<Float>,
    upload: List<Float>,
    stage: SpeedStage,
    modifier: Modifier = Modifier,
) {
    // Canvas 的绘制 lambda 不是组合作用域，语义色要先在外面取好
    val gridColor = Hairline
    val backgroundColor = FillColor
    val downloadColor = Accent
    val uploadColor = MetricMemory
    val linePath = remember { Path() }
    val fillPath = remember { Path() }

    Canvas(
        modifier = modifier
            .clip(RoundedCornerShape(10.dp))
            .background(backgroundColor),
    ) {
        val w = size.width
        val h = size.height

        // Y 轴上限：取两条曲线的峰值再留 15% 余量，向上取整到 10；全空时给 10 的底
        val rawMax = maxOf(download.maxOrNull() ?: 0f, upload.maxOrNull() ?: 0f)
        val maxY = maxOf(ceil(rawMax * 1.15f / 10f) * 10f, 10f)

        // 横向参考线
        listOf(0f, 0.25f, 0.5f, 0.75f, 1f).forEach { fraction ->
            val y = h * fraction
            drawLine(gridColor, Offset(0f, y), Offset(w, y), strokeWidth = 1.dp.toPx())
        }

        // 下载曲线：下载阶段起可见；上传曲线：上传阶段起可见（与 PC 端一致）
        val showDownload = stage == SpeedStage.DOWNLOAD ||
            stage == SpeedStage.UPLOAD ||
            stage == SpeedStage.DONE
        val showUpload = stage == SpeedStage.UPLOAD || stage == SpeedStage.DONE

        if (showDownload) {
            drawSeries(download, downloadColor, linePath, fillPath, w, h, maxY)
        }
        if (showUpload) {
            drawSeries(upload, uploadColor, linePath, fillPath, w, h, maxY)
        }
    }
}

/** 画一条速率曲线：面积渐变 + 折线 + 最新点高亮；只有一个点时画个孤点 */
private fun DrawScope.drawSeries(
    values: List<Float>,
    color: Color,
    linePath: Path,
    fillPath: Path,
    width: Float,
    height: Float,
    maxY: Float,
) {
    if (values.isEmpty()) return

    fun pointX(index: Int): Float =
        if (values.size == 1) width / 2f else width / (values.size - 1) * index

    fun pointY(value: Float): Float =
        height * (1f - (value / maxY).coerceIn(0f, 1f))

    if (values.size == 1) {
        drawCircle(color, radius = 3.dp.toPx(), center = Offset(pointX(0), pointY(values[0])))
        return
    }

    linePath.reset()
    fillPath.reset()
    values.forEachIndexed { index, value ->
        val x = pointX(index)
        val y = pointY(value)
        if (index == 0) {
            linePath.moveTo(x, y)
            fillPath.moveTo(x, height)
            fillPath.lineTo(x, y)
        } else {
            linePath.lineTo(x, y)
            fillPath.lineTo(x, y)
        }
    }
    fillPath.lineTo(pointX(values.lastIndex), height)
    fillPath.close()

    drawPath(
        fillPath,
        brush = Brush.verticalGradient(
            colors = listOf(color.copy(alpha = 0.28f), color.copy(alpha = 0f)),
        ),
    )
    drawPath(
        linePath,
        color = color,
        style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
    )
    // 最新点高亮，一眼看到当前速率
    drawCircle(
        color,
        radius = 3.5.dp.toPx(),
        center = Offset(pointX(values.lastIndex), pointY(values.last())),
    )
}

// ───────────────────────── 格式化 ─────────────────────────

private fun stageLabel(stage: SpeedStage): String = when (stage) {
    SpeedStage.IDLE -> "未开始"
    SpeedStage.PING -> "延迟测试中"
    SpeedStage.DOWNLOAD -> "下载测试中"
    SpeedStage.UPLOAD -> "上传测试中"
    SpeedStage.DONE -> "已完成"
}

/** 速率：小数值保留有效位，避免出现 0.0 / 12.0 这种读不出信息的写法 */
private fun formatSpeed(v: Double): String = when {
    v <= 0.0 -> "0"
    v < 1.0 -> twoDecimal(v)
    v < 100.0 -> oneDecimal(v)
    else -> v.roundToInt().toString()
}

/** 保留小数不用 String.format：部分 locale 会把小数点输出成逗号 */
private fun oneDecimal(v: Double): String = (kotlin.math.round(v * 10) / 10).toString()
private fun twoDecimal(v: Double): String = (kotlin.math.round(v * 100) / 100).toString()
