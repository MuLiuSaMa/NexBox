package com.nexbox.app.ui.screen

import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.GpsFixed
import androidx.compose.material.icons.rounded.PictureInPicture
import androidx.compose.material.icons.rounded.Speed
import androidx.compose.material.icons.rounded.SportsEsports
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import com.nexbox.app.ui.AppCard
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import coil3.compose.AsyncImage
import coil3.request.ImageRequest
import coil3.request.crossfade
import com.nexbox.app.data.EpicGame
import com.nexbox.app.data.CrosshairStore
import com.nexbox.app.data.OverlayStore
import com.nexbox.app.overlay.HudOverlayService
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.MetricGpu
import com.nexbox.app.ui.theme.MetricMemory
import com.nexbox.app.ui.theme.TextSecondary

/** 工具页内的一级页面：列表 / 网络测速 / 心境 / 悬浮框设置 / 辅助准心设置 */
private enum class ToolsPage { List, SpeedTest, Mood, OverlaySettings, CrosshairSettings }

/**
 * 工具页：只放与 PC 连接无关的站外小工具，一个功能一张卡。
 *
 * 原先这里按 `GET /api/capabilities` 生成 PC 能力清单，已按要求整体移除
 * （PC 侧的接口与白名单没动，要恢复只要把卡片列表接回来）。
 */
@Composable
fun FunctionScreen(epicVm: EpicFreeViewModel = viewModel()) {
    var page by remember { mutableStateOf(ToolsPage.List) }

    AnimatedContent(
        targetState = page,
        transitionSpec = {
            // targetState 为 List 表示正在回列表：新页从右侧进、旧页向左让
            val dir = if (targetState == ToolsPage.List) -1 else 1
            (slideInHorizontally(tween(220)) { it / 3 * dir } + fadeIn(tween(220)))
                .togetherWith(slideOutHorizontally(tween(180)) { -it / 3 * dir } + fadeOut(tween(180)))
        },
        label = "tools-page",
    ) { current ->
        when (current) {
            ToolsPage.List -> ToolsList(
                onOpenSpeedTest = { page = ToolsPage.SpeedTest },
                onOpenMood = { page = ToolsPage.Mood },
                onOpenOverlaySettings = { page = ToolsPage.OverlaySettings },
                onOpenCrosshairSettings = { page = ToolsPage.CrosshairSettings },
                epicVm = epicVm,
            )
            ToolsPage.SpeedTest -> SpeedTestScreen(onBack = { page = ToolsPage.List })
            ToolsPage.Mood -> MoodScreen(onBack = { page = ToolsPage.List })
            ToolsPage.OverlaySettings -> OverlaySettingsScreen(onBack = { page = ToolsPage.List })
            ToolsPage.CrosshairSettings -> CrosshairSettingsScreen(onBack = { page = ToolsPage.List })
        }
    }
}

@Composable
private fun ToolsList(
    onOpenSpeedTest: () -> Unit,
    onOpenMood: () -> Unit,
    onOpenOverlaySettings: () -> Unit,
    onOpenCrosshairSettings: () -> Unit,
    epicVm: EpicFreeViewModel,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding(),
    ) {
        Text(
            "工具",
            fontSize = 20.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier.padding(start = 18.dp, top = 14.dp, bottom = 14.dp),
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 18.dp, end = 18.dp, bottom = 18.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            item(key = "hud-overlay") { HudOverlayCard(onOpen = onOpenOverlaySettings) }
            item(key = "crosshair") { CrosshairCard(onOpen = onOpenCrosshairSettings) }
            item(key = "speed-test") { SpeedTestCard(onOpen = onOpenSpeedTest) }
            item(key = "mood") { MoodCard(onOpen = onOpenMood) }
            item(key = "epic-free") { EpicFreeCard(vm = epicVm) }

            // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
            item(key = "nav-bar-space") {
                Spacer(Modifier.navigationBarsPadding().height(96.dp))
            }
        }
    }
}

/**
 * 悬浮框入口卡：整卡点进设置页，开关与外观项都收在 [OverlaySettingsScreen] 里。
 * 卡上只留一个「是否开着」的状态副标题，和自动补启动（进程被杀后开关还在的情况）。
 */
@Composable
private fun HudOverlayCard(onOpen: () -> Unit) {
    val context = LocalContext.current

    // 进程被杀过之后回到本页：开关还开着但服务没了，权限还在就补起来
    LaunchedEffect(Unit) {
        if (OverlayStore.enabled && !HudOverlayService.isRunning && Settings.canDrawOverlays(context)) {
            HudOverlayService.start(context)
        }
    }
    AppCard(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .clickable(onClick = onOpen),
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            modifier = Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            IconBadge(Icons.Rounded.PictureInPicture, Accent, size = 40.dp, iconSize = 21.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    "悬浮框",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    if (OverlayStore.enabled) "已开启 · CPU / GPU 实时状态悬浮显示" else "已关闭 · 点击进入设置",
                    fontSize = 11.5.sp,
                    color = TextSecondary,
                )
            }
            Icon(
                Icons.AutoMirrored.Rounded.ArrowForward,
                contentDescription = "打开设置",
                tint = TextSecondary,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

/**
 * 网络测速入口卡：整卡点进测速页。
 * 测的是本机网络，与 PC 连接无关，所以放在工具页而不是主页。
 */
@Composable
private fun SpeedTestCard(onOpen: () -> Unit) {
    AppCard(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .clickable(onClick = onOpen),
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            modifier = Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            IconBadge(Icons.Rounded.Speed, Accent, size = 40.dp, iconSize = 21.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    "网络测速",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    "测试本机下载 / 上传 / 延迟，约 15 秒",
                    fontSize = 11.5.sp,
                    color = TextSecondary,
                )
            }
            Icon(
                Icons.AutoMirrored.Rounded.ArrowForward,
                contentDescription = "打开测速",
                tint = TextSecondary,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

/**
 * 辅助准心入口卡：整卡点进设置页。heal 逻辑与悬浮框卡对称——
 * 进程被杀后回到本页，开关还开着但服务没了就补拉起。
 */
@Composable
private fun CrosshairCard(onOpen: () -> Unit) {
    val context = LocalContext.current
    LaunchedEffect(Unit) {
        if (CrosshairStore.enabled && !HudOverlayService.isRunning && Settings.canDrawOverlays(context)) {
            HudOverlayService.start(context)
        }
    }
    AppCard(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .clickable(onClick = onOpen),
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            modifier = Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            IconBadge(Icons.Rounded.GpsFixed, MetricMemory, size = 40.dp, iconSize = 21.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    "辅助准心",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    if (CrosshairStore.enabled) "已开启 · 屏幕准星悬浮显示" else "已关闭 · 点击进入设置",
                    fontSize = 11.5.sp,
                    color = TextSecondary,
                )
            }
            Icon(
                Icons.AutoMirrored.Rounded.ArrowForward,
                contentDescription = "打开设置",
                tint = TextSecondary,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

/** 心境卡：点击在 App 内嵌打开，不跳系统浏览器 */
@Composable
private fun MoodCard(onOpen: () -> Unit) {
    AppCard(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .clickable(onClick = onOpen),
        shape = RoundedCornerShape(16.dp),
    ) {
        Row(
            modifier = Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            IconBadge(Icons.Rounded.Favorite, Accent, size = 40.dp, iconSize = 21.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(
                    "心境",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    "每一颗心 都值得被听见",
                    fontSize = 11.5.sp,
                    color = TextSecondary,
                )
            }
            Icon(
                Icons.AutoMirrored.Rounded.ArrowForward,
                contentDescription = "打开",
                tint = TextSecondary,
                modifier = Modifier.size(16.dp),
            )
        }
    }
}

/** Epic 喜加一卡：本周限免列表，与 PC 端同一个数据源与过滤条件 */
@Composable
private fun EpicFreeCard(vm: EpicFreeViewModel) {
    val state by vm.state.collectAsStateWithLifecycle()
    val uriHandler = LocalUriHandler.current
    LaunchedEffect(Unit) { vm.refresh() }

    AppCard(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(11.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                IconBadge(Icons.Rounded.SportsEsports, MetricGpu, size = 40.dp, iconSize = 21.dp)
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        "Epic 喜加一",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        "本周限免游戏，领取后永久入库",
                        fontSize = 11.5.sp,
                        color = TextSecondary,
                    )
                }
                if (!state.loading) {
                    Text(
                        "刷新",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { vm.refresh(force = true) }
                            .padding(horizontal = 8.dp, vertical = 6.dp),
                    )
                }
            }

            when {
                state.loading && state.games.isEmpty() -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                    Text("正在获取限免列表…", fontSize = 12.sp, color = TextSecondary)
                }

                state.error != null && state.games.isEmpty() -> Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(
                        state.error.orEmpty(),
                        fontSize = 12.sp,
                        lineHeight = 16.sp,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.weight(1f),
                    )
                    // 接口抖动时给个手动兜底（底层已经自动退避重试过三轮）
                    Text(
                        "重试",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { vm.refresh(force = true) }
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                    )
                }

                state.games.isEmpty() -> Text("本周没有限免游戏", fontSize = 12.sp, color = TextSecondary)

                else -> state.games.take(3).forEach { game ->
                    EpicGameRow(game = game, onOpen = { runCatching { uriHandler.openUri(game.link) } })
                }
            }
        }
    }
}

@Composable
private fun EpicGameRow(game: EpicGame, onOpen: () -> Unit) {
    val context = LocalContext.current
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        AsyncImage(
            // Epic 的 CDN 在国内要几秒才回，crossfade 让图到了淡入，比“啪”地跳出来不像卡住
            model = ImageRequest.Builder(context).data(game.cover).crossfade(durationMillis = 260).build(),
            contentDescription = null,
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .size(width = 52.dp, height = 70.dp)
                .clip(RoundedCornerShape(8.dp))
                .background(Hairline),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                game.title,
                fontSize = 13.sp,
                fontWeight = FontWeight.Medium,
                lineHeight = 17.sp,
                maxLines = 2,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                listOfNotNull(
                    game.freeEnd.takeIf { it.isNotBlank() }?.let { "截止 $it" },
                    game.originalPriceDesc.takeIf { it.isNotBlank() },
                ).joinToString(" · "),
                fontSize = 11.sp,
                color = TextSecondary,
                maxLines = 1,
            )
        }
        Text(
            "领取",
            fontSize = 12.sp,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .clickable(enabled = game.link.isNotBlank(), onClick = onOpen)
                .padding(horizontal = 8.dp, vertical = 6.dp),
        )
    }
}
