package com.nexbox.app.ui.screen.home

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.nexbox.app.data.Announcement
import com.nexbox.app.data.AnnouncementStore
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.todayKey
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Danger
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.random.Random
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * 主页顶部三张小卡：今日人气 / 公告 / 随机一言。
 * 形态与 PC 端 HomePage.tsx 顶部那一排 pill 对齐（label 在上、数值在下），
 * 一排三列等高；点击各自触发玩法，弹窗由主页层负责挂载。
 */
@Composable
fun HomeTopCards(
    onOpenAnnouncements: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
        TodayPopularityCard(Modifier.weight(1f).fillMaxHeight())
        Spacer(Modifier.width(10.dp))
        AnnouncementCard(Modifier.weight(1f).fillMaxHeight(), onClick = onOpenAnnouncements)
        Spacer(Modifier.width(10.dp))
        RandomQuoteCard(Modifier.weight(1f).fillMaxHeight())
    }
}

/** 三张卡共用的壳：label 在上，内容在下，整卡可点 */
@Composable
private fun StatCard(
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    AppCard(
        modifier = modifier
            .clip(RoundedCornerShape(16.dp))
            .clickable(onClick = onClick),
        shape = RoundedCornerShape(16.dp),
    ) {
        Column(
            Modifier
                .fillMaxSize()
                .padding(horizontal = 12.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(label, fontSize = 10.sp, color = TextSecondary)
            content()
        }
    }
}

// ───────────────────────── 今日人气 ─────────────────────────

/** 与 PC 端 TodayPopularity 一致的紫色数值（暗色 #b794f4） */
private val PopularityPurple = Color(0xFFB794F4)

/**
 * 今日人气：本机整活数 —— 没点过显示 ?，点一下生成 0~100，当天固定、次日重置。
 * 生成瞬间数字做一个 1→1.3→1 的弹跳（PC 端同款 keyframes）。
 */
@Composable
private fun TodayPopularityCard(modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var value by rememberSaveable { mutableIntStateOf(-1) }
    var loaded by rememberSaveable { mutableStateOf(false) }
    val scale = remember { Animatable(1f) }

    LaunchedEffect(Unit) {
        val (date, saved) = NetworkModule.homePrefs.popularity.first()
        if (date == todayKey() && saved in 0..100) value = saved
        loaded = true
    }

    StatCard(label = "今日人气", onClick = {
        scope.launch {
            val fresh = NetworkModule.homePrefs.popularityToday()
            value = fresh
            // 只在拿到数的时候弹一下；当天再点返回同一个数，弹跳也照做（有反馈感）
            scale.snapTo(1f)
            scale.animateTo(1.3f, tween(300))
            scale.animateTo(1f, tween(300))
        }
    }, modifier = modifier) {
        if (!loaded) {
            Text(" ", fontSize = 22.sp, fontWeight = FontWeight.Bold)
        } else {
            Text(
                if (value in 0..100) value.toString() else "?",
                fontSize = 22.sp,
                fontWeight = FontWeight.Bold,
                color = PopularityPurple,
                modifier = Modifier.graphicsLayer {
                    scaleX = scale.value
                    scaleY = scale.value
                },
            )
        }
    }
}

// ───────────────────────── 公告 ─────────────────────────

/**
 * 公告卡：大数字 = 条数；有未读（create_time 晚于本地已读水位）时右上红点。
 * 口径与 PC 端 AnnouncementCard 一致。
 */
@Composable
private fun AnnouncementCard(modifier: Modifier = Modifier, onClick: () -> Unit) {
    val items by AnnouncementStore.items.collectAsStateWithLifecycle()
    val loading by AnnouncementStore.loading.collectAsStateWithLifecycle()
    val readMark by NetworkModule.homePrefs.announcementsRead
        .collectAsStateWithLifecycle(initialValue = "")
    val unread = items.any { (it.createTime ?: "") > readMark }

    StatCard(label = "公告", onClick = onClick, modifier = modifier) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (loading && items.isEmpty()) "…" else items.size.toString(),
                fontSize = 22.sp,
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Spacer(Modifier.width(6.dp))
            if (unread) Box(Modifier.size(7.dp).clip(CircleShape).background(Danger))
        }
    }
}

/** 公告列表弹窗：标题 + 「重要」红标 + 时间 + 正文，滚动查看；空态「暂无公告」 */
@Composable
fun AnnouncementListDialog(
    items: List<Announcement>,
    loading: Boolean,
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
                    .heightIn(max = 600.dp)
                    .clip(RoundedCornerShape(22.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "公告",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = onDismiss) { Text("关闭") }
                }

                when {
                    loading && items.isEmpty() -> Box(
                        Modifier.fillMaxWidth().height(120.dp),
                        contentAlignment = Alignment.Center,
                    ) { CircularProgressIndicator(modifier = Modifier.size(22.dp), strokeWidth = 2.5.dp) }

                    items.isEmpty() -> Box(
                        Modifier.fillMaxWidth().height(120.dp),
                        contentAlignment = Alignment.Center,
                    ) { Text("暂无公告", fontSize = 13.sp, color = TextSecondary) }

                    else -> Column(
                        Modifier
                            .verticalScroll(rememberScrollState())
                            .weight(1f, fill = false),
                        verticalArrangement = Arrangement.spacedBy(14.dp),
                    ) {
                        items.forEach { AnnouncementRow(it) }
                    }
                }

                // 弹窗窗口不消费手势区，留一点底部余量避免贴住导航条
                Spacer(Modifier.navigationBarsPadding().height(0.dp))
            }
        }
    }
}

@Composable
private fun AnnouncementRow(item: Announcement) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                item.title.orEmpty().ifBlank { "公告" },
                fontSize = 14.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
            )
            if (item.important) {
                Spacer(Modifier.width(6.dp))
                Text(
                    "重要",
                    fontSize = 9.sp,
                    fontWeight = FontWeight.Bold,
                    color = Color.White,
                    modifier = Modifier
                        .clip(RoundedCornerShape(5.dp))
                        .background(Danger)
                        .padding(horizontal = 5.dp, vertical = 1.dp),
                )
            }
        }
        item.createTime?.takeIf { it.isNotBlank() }?.let {
            Text(it, fontSize = 10.5.sp, color = TextSecondary)
        }
        item.content?.takeIf { it.isNotBlank() }?.let {
            Text(
                it,
                fontSize = 12.5.sp,
                lineHeight = 19.sp,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.9f),
            )
        }
    }
}

/**
 * 重要公告强制弹窗：对齐 PC 端 ImportantAnnouncementModal ——
 * 不能点外部/返回键关掉，只能点「好的」确认。
 */
@Composable
fun ImportantAnnouncementDialog(
    item: Announcement,
    onConfirm: () -> Unit,
) {
    Dialog(
        onDismissRequest = { /* 强制弹窗：不允许点外部关闭 */ },
        properties = DialogProperties(
            usePlatformDefaultWidth = false,
            dismissOnClickOutside = false,
            dismissOnBackPress = false,
        ),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 24.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(22.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        item.title.orEmpty().ifBlank { "重要公告" },
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        "重要",
                        fontSize = 9.sp,
                        fontWeight = FontWeight.Bold,
                        color = Color.White,
                        modifier = Modifier
                            .clip(RoundedCornerShape(5.dp))
                            .background(Danger)
                            .padding(horizontal = 5.dp, vertical = 1.dp),
                    )
                }
                Text(
                    item.content.orEmpty(),
                    fontSize = 13.sp,
                    lineHeight = 20.sp,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.92f),
                    modifier = Modifier
                        .heightIn(max = 320.dp)
                        .verticalScroll(rememberScrollState()),
                )
                Button(
                    onClick = onConfirm,
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp),
                ) { Text("好的", fontWeight = FontWeight.SemiBold) }
                Spacer(Modifier.navigationBarsPadding().height(0.dp))
            }
        }
    }
}

// ───────────────────────── 随机一言 ─────────────────────────

/** 随机一言：语料与 PC 端 RandomQuote 同源，点击随机换一条 */
@Composable
private fun RandomQuoteCard(modifier: Modifier = Modifier) {
    var index by rememberSaveable { mutableIntStateOf(Random.nextInt(Quotes.size)) }
    val quote = Quotes.at(index)

    StatCard(label = "随机一言", onClick = {
        var next = index
        while (next == index) next = Random.nextInt(Quotes.size)
        index = next
    }, modifier = modifier) {
        Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                quote.text,
                fontSize = 10.sp,
                lineHeight = 14.sp,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text(
                "-${quote.author}",
                fontSize = 9.sp,
                color = TextSecondary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.End,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}
