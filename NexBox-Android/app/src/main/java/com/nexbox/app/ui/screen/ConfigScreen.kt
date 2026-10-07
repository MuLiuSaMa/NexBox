package com.nexbox.app.ui.screen

import android.content.Intent
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
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
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.rounded.Android
import androidx.compose.material.icons.rounded.BatteryChargingFull
import androidx.compose.material.icons.rounded.BatteryFull
import androidx.compose.material.icons.rounded.DeveloperBoard
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.Monitor
import androidx.compose.material.icons.rounded.PhoneAndroid
import androidx.compose.material.icons.rounded.Speed
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import com.nexbox.app.ui.AppCard
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.nexbox.app.data.DeviceRuntime
import com.nexbox.app.data.DeviceSpec
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.FillColor
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.MetricCpu
import com.nexbox.app.ui.theme.MetricGpu
import com.nexbox.app.ui.theme.MetricMemory
import com.nexbox.app.ui.theme.MetricStorage
import com.nexbox.app.ui.theme.TempWarn
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.roundToInt

/**
 * 手机 SoC 的温度阈值。PC 那套 80/90°C 在这儿永远不亮（本机 idle 实测 34~37°C），
 * 等于没有配色；48/58 已经接近皮肤温控上限，亮起来是有意义的。
 */
private const val SOC_TEMP_WARN = 48.0
private const val SOC_TEMP_DANGER = 58.0

/**
 * 配置页：本机（手机）配置信息。
 *
 * 默认只铺常用的几项（处理器 / 运存 / 存储 / 屏幕 / 刷新率 / 尺寸 / 系统 / 电池），
 * 点顶部机型卡才进详情页看全量参数——全量清单有四十来行，直接摊在第一屏既难找也难看。
 */
@Composable
fun ConfigScreen(vm: ConfigViewModel = viewModel()) {
    val state by vm.state.collectAsStateWithLifecycle()
    var showDetail by remember { mutableStateOf(false) }

    // 轮询跟着页面走：VM 挂在 Activity 上，切到别的标签不会销毁，不主动停就会一直在后台采样
    DisposableEffect(Unit) {
        vm.setActive(true)
        onDispose { vm.setActive(false) }
    }
    // 详情页里返回优先退回顾览，别直接把整个 App 退掉
    BackHandler(enabled = showDetail) { showDetail = false }

    AnimatedContent(
        targetState = showDetail,
        transitionSpec = {
            // targetState 为 true 表示正在进详情：新页从右侧进、旧页向左让
            val dir = if (targetState) 1 else -1
            (slideInHorizontally(tween(220)) { it / 3 * dir } + fadeIn(tween(220)))
                .togetherWith(slideOutHorizontally(tween(180)) { -it / 3 * dir } + fadeOut(tween(180)))
        },
        label = "config-page",
    ) { detail ->
        if (detail) {
            ConfigDetailScreen(state = state, onBack = { showDetail = false })
        } else {
            ConfigOverviewScreen(
                state = state,
                onRescan = vm::reloadSpec,
                onOpenDetail = { showDetail = true },
            )
        }
    }
}

// ───────────────────────── 概览 ─────────────────────────

@Composable
private fun ConfigOverviewScreen(
    state: ConfigUiState,
    onRescan: () -> Unit,
    onOpenDetail: () -> Unit,
) {
    val spec = state.spec
    val runtime = state.runtime
    val context = LocalContext.current

    // 应用沙箱里清不了别的 App 的缓存，能做的只有把用户送到系统自己的存储页
    val openStorageSettings: () -> Unit = {
        val candidates = listOf(
            Intent(Settings.ACTION_INTERNAL_STORAGE_SETTINGS),
            Intent(Settings.ACTION_APPLICATION_SETTINGS),
        )
        for (intent in candidates) {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            if (runCatching { context.startActivity(intent) }.isSuccess) break
        }
    }

    PageColumn {
        Spacer(Modifier.height(2.dp))
        Text(
            "配置",
            fontSize = 20.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onBackground,
        )

        // 机型卡：整块可点，进详情看全量参数
        AppCard(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .clickable(enabled = spec != null, onClick = onOpenDetail),
            shape = RoundedCornerShape(16.dp),
        ) {
            Row(
                modifier = Modifier.padding(14.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                IconBadge(Icons.Rounded.PhoneAndroid, Accent, size = 40.dp, iconSize = 21.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        spec?.marketName ?: "本机",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        listOfNotNull(
                            spec?.brand,
                            spec?.androidRelease?.let { "Android $it" },
                            spec?.socMarketingName ?: spec?.socModel,
                        ).joinToString(" · ").ifBlank { "正在读取本机信息…" },
                        fontSize = 11.5.sp,
                        color = TextSecondary,
                    )
                }
                Icon(
                    Icons.AutoMirrored.Rounded.ArrowForward,
                    contentDescription = "查看完整参数",
                    tint = TextSecondary,
                    modifier = Modifier.size(16.dp),
                )
            }
        }

        // 常用项：两列卡片，读不到的项不占位
        val tiles = overviewTiles(spec, runtime)
        tiles.chunked(2).forEach { row ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                row.forEach { tile ->
                    Box(Modifier.weight(1f)) { OverviewTile(tile) }
                }
                // 奇数项时补个占位，保证最后一个是半宽而不是通栏
                if (row.size == 1) Spacer(Modifier.weight(1f))
            }
        }

        // 本机性能：温度 / 占用 / 频率 / 整机功耗。读不到的机器整卡不出现
        LocalPerfCard(runtime = runtime, gpuName = spec?.gpuName, gpuTempHistory = state.gpuTempHistory)

        // 屏幕三项（分辨率 / 尺寸 / 刷新率）同一块屏的信息，合成一张通栏卡横向摆
        ScreenCard(spec = spec)

        // 存储占最下面一张通栏卡：带进度条和操作按钮，塞进两列网格不好展
        StorageCard(spec = spec, runtime = runtime, onOpenSettings = openStorageSettings)

        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                "点上方机型卡看完整参数（指令集、GPU 驱动、安全补丁等）",
                fontSize = 11.sp,
                color = TextSecondary,
                lineHeight = 15.sp,
                modifier = Modifier.weight(1f),
            )
            // 静态项极少变，但换机 / 调试时想强制重读·   
            Text(
                if (state.scanning) "扫描中…" else "重新扫描",
                fontSize = 12.sp,
                color = if (state.scanning) TextSecondary else MaterialTheme.colorScheme.primary,
                modifier = Modifier
                    .padding(start = 10.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .clickable(enabled = !state.scanning, onClick = onRescan)
                    .padding(horizontal = 6.dp, vertical = 3.dp),
            )
        }
    }
}

/** 概览格子 */
private class Tile(
    val icon: ImageVector,
    val label: String,
    val value: String?,
    val tint: Color,
    val hint: String? = null,
)

/** 只列用户关心的常用项；值为 null 的项直接不出现 */
@Composable
private fun overviewTiles(spec: DeviceSpec?, runtime: DeviceRuntime?): List<Tile> {
    val soc = spec?.socMarketingName ?: spec?.socModel
    return listOf(
        Tile(Icons.Rounded.DeveloperBoard, "处理器", soc, MetricCpu, spec?.socVendor),
        Tile(
            Icons.Rounded.Memory, "运行内存", spec?.ramTotal, MetricMemory,
            runtime?.ramAvailable?.let { "可用 $it" },
        ),
        Tile(
            Icons.Rounded.Android, "系统", spec?.androidRelease?.let { "Android $it" }, Accent,
            spec?.romLabel,
        ),
        // 充电中换充电图标，一眼能看出是否在插电
        Tile(
            if (runtime?.chargingLabel?.contains("充电中") == true) {
                Icons.Rounded.BatteryChargingFull
            } else {
                Icons.Rounded.BatteryFull
            },
            "电池", runtime?.batteryPercent?.let { "$it%" }, TempWarn,
            runtime?.chargingLabel,
        ),
    ).filter { !it.value.isNullOrBlank() }
}

@Composable
private fun OverviewTile(tile: Tile) {
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(
            modifier = Modifier.padding(13.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                IconBadge(tile.icon, tile.tint, size = 26.dp, iconSize = 15.dp)
                Text(tile.label, fontSize = 11.5.sp, color = TextSecondary)
            }
            Text(
                tile.value.orEmpty(),
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
            )
            tile.hint?.let {
                Text(it, fontSize = 11.sp, color = TextSecondary, maxLines = 1)
            }
        }
    }
}

/**
 * 本机性能卡：CPU 温度 / 占用 / 频率，GPU 型号 / 温度趋势，外加电池端整机功耗。
 *
 * 读数全部来自 sysfs 与电池框架，能不能读到逐台机器都不一样（看 kernel 和厂商策略）：
 * 三项主读数一个都没有就整卡不出现，单项缺失只让那一格退回「—」，不铺一张空卡。
 * GPU 的占用与频率 sysfs 被 SELinux 全锁（kgsl / devfreq 连 shell 都被拒），免 root
 * 拿不到，所以 GPU 列只放拿得到的：型号（GL_RENDERER 查得）+ 温度折线。
 */
@Composable
private fun LocalPerfCard(
    runtime: DeviceRuntime?,
    gpuName: String? = null,
    gpuTempHistory: List<Double> = emptyList(),
) {
    val r = runtime ?: return
    if (r.cpuTempC == null && r.cpuUsagePercent == null && r.gpuTempC == null && gpuName == null) return

    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
        Column {
            Row(
                modifier = Modifier.padding(start = 14.dp, end = 14.dp, top = 14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconBadge(Icons.Rounded.Speed, MetricCpu, size = 26.dp, iconSize = 15.dp)
                Spacer(Modifier.width(8.dp))
                Text(
                    "性能",
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }

            Row {
                MetricGauge(
                    label = "CPU",
                    color = MetricCpu,
                    percent = r.cpuUsagePercent,
                    temp = r.cpuTempC,
                    footnote = r.cpuClock,
                    tempWarn = SOC_TEMP_WARN,
                    tempDanger = SOC_TEMP_DANGER,
                    modifier = Modifier.weight(1f),
                )
                if (r.gpuTempC != null || gpuName != null) {
                    GpuPanel(
                        name = gpuName,
                        temp = r.gpuTempC,
                        history = gpuTempHistory,
                        modifier = Modifier.weight(1f),
                    )
                }
            }

            // 充电时 LocalDeviceReader 直接不给功率（那时是充进去的电），这一行就整行消失
            val power = r.batteryPower
            if (power != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(start = 14.dp, end = 14.dp, bottom = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("整机功耗（电池端）", fontSize = 11.sp, color = TextSecondary)
                    Spacer(Modifier.weight(1f))
                    Text(
                        if (r.batteryPowerEstimated) "约 $power" else power,
                        fontSize = 11.5.sp,
                        fontWeight = FontWeight.Medium,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }
    }
}

/**
 * GPU 面板：精确型号（离屏 EGL 查 GL_RENDERER）+ 温度折线。
 * 头部小温度、脚注峰值与折线图共用同一条 1 秒采样序列。
 */
@Composable
private fun GpuPanel(
    name: String?,
    temp: Double?,
    history: List<Double>,
    modifier: Modifier = Modifier,
) {
    Column(modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(MetricGpu))
            Spacer(Modifier.width(6.dp))
            Text(
                "GPU",
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                color = TextSecondary,
            )
            Spacer(Modifier.weight(1f))
            if (temp != null) {
                Text(
                    "${temp.roundToInt()}°C",
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    color = tempColor(temp, SOC_TEMP_WARN, SOC_TEMP_DANGER),
                )
            }
        }
        Text(
            name ?: "—",
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
            color = if (name == null) TextSecondary else MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
        )
        GpuTempChart(history, MetricGpu)
        Text(
            history.maxOrNull()?.let { "峰值 ${it.roundToInt()}°C" } ?: "温度采样中…",
            fontSize = 11.sp,
            color = TextSecondary,
            maxLines = 1,
        )
    }
}

/**
 * 温度折线：秒级采样，右端是最新值。量程自适应并上下各留 15% 余量——
 * 不留余量的话温度走平的时候线会贴死边框。
 */
@Composable
private fun GpuTempChart(history: List<Double>, color: Color, modifier: Modifier = Modifier) {
    if (history.size < 2) {
        // 样本不足（刚进页面）：先画一条与 MetricBar 同语言的空底槽
        Box(
            modifier
                .fillMaxWidth()
                .height(24.dp)
                .clip(RoundedCornerShape(50))
                .background(FillColor),
        )
        return
    }
    val lo0 = history.min()
    val hi0 = history.max()
    val pad = ((hi0 - lo0) * 0.15).coerceAtLeast(1.0)
    val lo = lo0 - pad
    val span = (hi0 - lo0) + pad * 2
    Canvas(modifier.fillMaxWidth().height(24.dp)) {
        val step = size.width / (history.size - 1)
        fun yOf(v: Double): Float =
            (size.height * (1f - ((v - lo) / span).toFloat())).coerceIn(0f, size.height)
        val line = Path()
        val area = Path()
        history.forEachIndexed { index, value ->
            val x = index * step
            val y = yOf(value)
            if (index == 0) {
                line.moveTo(x, y)
                area.moveTo(x, size.height)
                area.lineTo(x, y)
            } else {
                line.lineTo(x, y)
                area.lineTo(x, y)
            }
        }
        area.lineTo(size.width, size.height)
        area.close()
        drawPath(
            area,
            Brush.verticalGradient(
                0f to color.copy(alpha = 0.28f),
                1f to color.copy(alpha = 0.04f),
            ),
        )
        drawPath(
            line,
            color,
            style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round),
        )
    }
}

/**
 * 屏幕卡：分辨率 / 尺寸 / 刷新率横向三列。三项都是同一块屏的参数，
 * 拆成三个格子会把它们分散到不同行，看不出是一组。
 *
 * 刷新率只报面板上限：实时档位会被智能刷新率拉到 60/90，放在规格页里就是误导。
 */
@Composable
private fun ScreenCard(spec: DeviceSpec?) {
    val resolution = spec?.screenResolution
    val inches = spec?.screenInches
    val maxRate = spec?.maxRefreshRate
    // 全读不到（极老的固件或被限制）时整张卡不出现，不铺一个空壳
    if (resolution == null && inches == null && maxRate == null) return

    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(11.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                IconBadge(Icons.Rounded.Monitor, MetricGpu, size = 26.dp, iconSize = 15.dp)
                Text(
                    "屏幕",
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            // 三列两端对齐：首列贴左、末列贴右、中间居中，间隔严格相等，右侧不留空档
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                StatCell("分辨率", resolution)
                StatCell("尺寸", inches)
                // 面板上限：读当前生效档位会把 144Hz 屏报成 120、120Hz 屏报成 60
                StatCell("刷新率", maxRate)
            }
        }
    }
}

/**
 * 本机存储大卡：进度条 + 已用/剩余/总量 + 跳系统存储设置。
 *
 * 安卓不让第三方应用清别的 App 的缓存，所以这张卡只做「看清还剩多少 + 把人送到系统页」，
 * 不在本机造一个其实清不动东西的假按钮。
 */
@Composable
private fun StorageCard(spec: DeviceSpec?, runtime: DeviceRuntime?, onOpenSettings: () -> Unit) {
    val percent = runtime?.storageUsedPercent
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(11.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                IconBadge(Icons.Rounded.Storage, MetricStorage, size = 26.dp, iconSize = 15.dp)
                Text(
                    "机身存储",
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Spacer(Modifier.weight(1f))
                percent?.let { Text("已用 $it%", fontSize = 11.5.sp, color = TextSecondary) }
            }
            UsageBar(percent = percent, color = MetricStorage)
            // 与屏幕卡同款排布（SpaceBetween 三列）：已用 → 剩余 → 总量，
            // 间隔相等、右侧不留空档，两张相邻通栏卡的左右两列上下对齐
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                StatCell("已用", runtime?.storageUsed)
                StatCell("剩余", runtime?.storageFree)
                StatCell("总量", spec?.storageTotal)
            }
            OutlinedButton(
                onClick = onOpenSettings,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp),
            ) { Text("打开系统存储设置") }
        }
    }
}

/** 占用条：percent 为 null 时只留底槽不画进度，与主页硬件卡同一口径 */
@Composable
private fun UsageBar(percent: Int?, color: Color) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(6.dp)
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

@Composable
private fun StatCell(label: String, value: String?, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(label, fontSize = 11.sp, color = TextSecondary)
        Text(
            value ?: "—",
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}

/** 图标底座：淡色圆角块 + 同色图标（工具页的功能卡也在用） */
@Composable
internal fun IconBadge(icon: ImageVector, tint: Color, size: Dp, iconSize: Dp) {
    Box(
        modifier = Modifier
            .size(size)
            .clip(RoundedCornerShape(percent = 32))
            .background(tint.copy(alpha = 0.16f)),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = tint, modifier = Modifier.size(iconSize))
    }
}

// ───────────────────────── 详情 ─────────────────────────

/** 全量参数：按类别分组，读不到的行整行隐藏 */
@Composable
private fun ConfigDetailScreen(state: ConfigUiState, onBack: () -> Unit) {
    val spec = state.spec
    val runtime = state.runtime

    PageColumn(spacing = 12.dp) {
        DetailTopBar(title = "完整参数", onBack = onBack, trailingText = state.readAt?.let { "读取于 $it" })

        SectionCard(title = "设备") {
            SpecRow("产品名称", spec?.marketName)
            SpecRow("品牌 / 制造商", joined(spec?.brand, spec?.manufacturer))
            SpecRow("型号", spec?.model)
            SpecRow("设备代号", spec?.deviceCode)
        }

        SectionCard(title = "系统") {
            SpecRow("Android 版本", spec?.androidRelease)
            SpecRow("API 级别", spec?.sdkInt?.takeIf { it > 0 }?.toString())
            SpecRow("安全补丁", spec?.securityPatch)
            SpecRow("固件版本", spec?.buildDisplay)
            SpecRow("定制系统", spec?.romLabel)
            SpecRow("内核版本", spec?.kernelVersion)
            SpecRow("系统架构", spec?.let { if (it.is64Bit) "64 位" else "32 位" })
        }

        SectionCard(title = "处理器与显卡") {
            SpecRow("处理器", spec?.socMarketingName ?: spec?.socModel)
            // 商用名是查表映射出来的，代号一并留着方便核对
            if (spec?.socMarketingName != null) SpecRow("型号代号", spec.socModel)
            SpecRow("SoC 厂商", spec?.socVendor)
            SpecRow(
                "CPU",
                spec?.cpuCores?.takeIf { it > 0 }?.let { cores ->
                    appended("$cores 核", runtime?.coresOnline?.let { "在线 $it 核" })
                },
            )
            SpecRow("指令集", spec?.abis)
            SpecRow("图形接口", spec?.glEsVersion)
            SpecRow("GPU 型号", spec?.gpuName)
            SpecRow("Vulkan", spec?.let { if (it.vulkanSupported) "支持" else "不支持" })
            SpecRow("GPU 驱动", spec?.gpuDriver)
        }

        SectionCard(title = "内存与存储") {
            SpecRow("运行内存", appended(spec?.ramTotal, runtime?.ramAvailable?.let { "可用 $it" }))
            SpecRow("机身存储", appended(spec?.storageTotal, runtime?.storageFree?.let { "剩余 $it" }))
        }

        SectionCard(title = "屏幕与电池") {
            SpecRow("分辨率", spec?.screenResolution)
            SpecRow("屏幕尺寸", spec?.screenInches)
            SpecRow("像素密度", spec?.screenDpi?.let { "$it dpi" })
            SpecRow("刷新率", spec?.maxRefreshRate)
            SpecRow("HDR", spec?.let { if (it.hdrSupported) "支持" else "不支持" })
            SpecRow("电量", runtime?.batteryPercent?.let { "$it%" })
            SpecRow("充电状态", runtime?.chargingLabel)
            SpecRow("电池电压", runtime?.batteryVoltage)
        }

        val features = spec?.supportedFeatures.orEmpty()
        if (features.isNotEmpty()) {
            SectionCard(title = "支持的特性") {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        Modifier
                            .size(5.dp)
                            .clip(CircleShape)
                            .background(Accent),
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        features.joinToString(" / "),
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Medium,
                        lineHeight = 17.sp,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                }
            }
        }

        Text(
            "处理器商用名由代号查表映射，个别未收录的型号会直接显示原始代号。",
            fontSize = 11.sp,
            color = TextSecondary,
            lineHeight = 15.sp,
        )
    }
}

/** 概览与详情共用的页面骨架：状态栏留白 + 横向 18dp 内边距 + 底部玻璃条余量 */
@Composable
private fun PageColumn(spacing: Dp = 12.dp, content: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(spacing),
    ) {
        content()
        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

/** 详情页统一的分组卡片：小标题 + 内容，间距与设置页保持一致 */
@Composable
private fun SectionCard(title: String, content: @Composable ColumnScope.() -> Unit) {
    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(9.dp)) {
            Text(
                title,
                fontSize = 13.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
            )
            HorizontalDivider(color = Hairline)
            content()
        }
    }
}

/** 两段值拼成 "8 GB（可用 4.3 GB）"；任一段缺失就只留另一段，两段都没有则为 null（整行隐藏） */
private fun appended(main: String?, extra: String?): String? = when {
    main == null && extra == null -> null
    main == null -> extra
    extra == null -> main
    else -> "$main（$extra）"
}

/** "品牌 / 制造商" 这类并列值：空的段跳过，全空为 null */
private fun joined(vararg parts: String?): String? =
    parts.filterNotNull().filter { it.isNotBlank() }.distinct().joinToString(" / ").ifBlank { null }
