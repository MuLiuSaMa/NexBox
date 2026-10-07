package com.nexbox.app.ui.screen

import androidx.activity.compose.BackHandler
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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Casino
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Done
import androidx.compose.material.icons.rounded.Favorite
import androidx.compose.material.icons.rounded.FavoriteBorder
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.Map
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.nexbox.app.data.DeltaLoadout
import com.nexbox.app.data.DeltaPasswordItem
import com.nexbox.app.data.DeltaWeapon
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.MessageToast
import com.nexbox.app.ui.theme.Accent
import com.nexbox.app.ui.theme.Hairline
import com.nexbox.app.ui.theme.TextSecondary

/**
 * 三角洲页：顶部每日密码，下面改枪码平台。数据源与 PC 端一致（直连公网，不依赖是否连上电脑）。
 *
 * 整页只有一个 LazyColumn，筛选卡与改枪码卡都摊成条目 —— 卡里再嵌一个可滚动容器，
 * 手势会先滚内层、滚到底才轮到外层，列表也拿不到回收复用。
 */
@Composable
fun DeltaForceScreen(vm: DeltaForceViewModel = viewModel()) {
    val state by vm.state.collectAsStateWithLifecycle()
    val listState = rememberLazyListState()
    // 随机装备整页开关：纯 UI 状态，不走 VM（页面本身不依赖三角洲数据）
    var rouletteOpen by remember { mutableStateOf(false) }

    // 轮询跟着页面走：VM 挂在 Activity 上，切到别的标签它不会销毁
    DisposableEffect(Unit) {
        vm.setActive(true)
        onDispose { vm.setActive(false) }
    }
    // 翻页后回到顶部，否则新页内容在屏幕外，看着像没翻动
    LaunchedEffect(state.query.page) { listState.scrollToItem(0) }
    // 投稿页里返回优先退回列表，而不是把整个 App 退掉
    BackHandler(enabled = state.uploadOpen) { vm.closeUpload() }

    Box(Modifier.fillMaxSize()) {
        AnimatedContent(
            targetState = state.uploadOpen,
            transitionSpec = {
                // targetState 为 true 表示正在进投稿页：新页从右侧进、旧页向左让
                val dir = if (targetState) 1 else -1
                (slideInHorizontally(tween(220)) { it / 3 * dir } + fadeIn(tween(220)))
                    .togetherWith(slideOutHorizontally(tween(180)) { -it / 3 * dir } + fadeOut(tween(180)))
            },
            label = "delta-upload",
        ) { opened ->
            if (opened) {
                UploadFormScreen(state = state, vm = vm)
            } else {
                DeltaForceList(state = state, vm = vm, listState = listState, onOpenRoulette = { rouletteOpen = true })
            }
        }

        // 随机装备整页覆盖层：与投稿页同款转场，盖在列表之上、轻提示之下
        AnimatedContent(
            targetState = rouletteOpen,
            transitionSpec = {
                val dir = if (targetState) 1 else -1
                (slideInHorizontally(tween(220)) { it / 3 * dir } + fadeIn(tween(220)))
                    .togetherWith(slideOutHorizontally(tween(180)) { -it / 3 * dir } + fadeOut(tween(180)))
            },
            label = "delta-roulette",
        ) { open ->
            if (open) {
                DeltaRouletteScreen(onClose = { rouletteOpen = false })
            } else {
                Box(Modifier.fillMaxSize())
            }
        }

        MessageToast(
            message = state.message,
            isError = state.messageIsError,
            onDismiss = vm::consumeMessage,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .statusBarsPadding()
                .padding(top = 8.dp, end = 12.dp)
                .fillMaxWidth(0.86f),
        )
    }
}

/** 列表主体：一张密码卡 + 一行功能入口 + 一张筛选卡 + 若干改枪码卡，全在同一个 LazyColumn 里 */
@Composable
private fun DeltaForceList(
    state: DeltaUiState,
    vm: DeltaForceViewModel,
    listState: LazyListState,
    onOpenRoulette: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding(),
    ) {
        Text(
            "三角洲专区",
            fontSize = 20.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onBackground,
            modifier = Modifier.padding(start = 18.dp, top = 14.dp, bottom = 14.dp),
        )

        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(start = 18.dp, end = 18.dp, bottom = 18.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            item(key = "password") { PasswordCard(state, vm) }
            item(key = "features") { FeatureRow(onOpenRoulette = onOpenRoulette) }
            item(key = "loadout-header") { LoadoutHeader(state, vm) }
            items(state.loadouts, key = { "loadout-${it.id}" }, contentType = { "loadout" }) { item ->
                LoadoutCard(
                    item = item,
                    isCopied = state.copiedId == item.id,
                    isLiked = item.id in state.likedIds,
                    isReported = item.id in state.reportedIds,
                    onCopied = { vm.markCopied(item.id) },
                    onLike = { vm.like(item) },
                    onReport = { vm.report(item) },
                )
            }

            if (state.totalPages > 1) {
                item(key = "pager") { PagerRow(state, vm) }
            }

            // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
            item(key = "nav-bar-space") {
                Spacer(Modifier.navigationBarsPadding().height(96.dp))
            }
        }
    }
}

// ───────────────────────── 每日密码 ─────────────────────────

@Composable
private fun PasswordCard(state: DeltaUiState, vm: DeltaForceViewModel) {
    val clipboard = LocalClipboardManager.current

    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                IconBadge(Icons.Rounded.Lock, Accent, size = 40.dp, iconSize = 21.dp)
                Text(
                    "每日密码",
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }

            when {
                state.passwords.isEmpty() && state.passwordsLoading -> LoadingLine("正在获取每日密码…")

                state.passwords.isEmpty() && state.passwordError != null -> ErrorLine(
                    text = state.passwordError.orEmpty(),
                    onRetry = { vm.refreshPasswords(force = true) },
                )

                state.passwords.isEmpty() -> Text("暂无密码数据", fontSize = 12.sp, color = TextSecondary)

                else -> PasswordGrid(
                    items = state.passwords,
                    onCopy = { item ->
                        clipboard.setText(AnnotatedString(item.password))
                        vm.markCopiedPassword(item.name)
                    },
                )
            }

            // 已有数据时接口抖一下，只在底部留一行小字，不把用户正在看的密码顶掉
            if (state.passwords.isNotEmpty() && state.passwordError != null) {
                Text(state.passwordError.orEmpty(), fontSize = 11.sp, color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

/**
 * 密码块单行铺开：6 张一行（多出来的自动换行），手机上正好放今日全部地图。
 * 每张块很窄，字号整体压小一档；不显示「复制」字样，点按静默复制。
 */
@Composable
private fun PasswordGrid(items: List<DeltaPasswordItem>, onCopy: (DeltaPasswordItem) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        items.chunked(6).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                row.forEach { item ->
                    PasswordTile(
                        item = item,
                        onClick = { onCopy(item) },
                        modifier = Modifier.weight(1f),
                    )
                }
                // 不足 6 张时补空位，保证每张密码块的宽度与整行一致
                repeat(6 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun PasswordTile(
    item: DeltaPasswordItem,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(10.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
            .clickable(onClick = onClick)
            .padding(horizontal = 3.dp, vertical = 11.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        Text(
            item.name,
            fontSize = 9.sp,
            fontWeight = FontWeight.Medium,
            color = TextSecondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Text(
            item.password,
            fontSize = 15.sp,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.primary,
        )
    }
}

// ───────────────────────── 功能入口 ─────────────────────────

/**
 * 密码卡下方的一行功能入口：随机装备（应用内页）/ 官方地图（系统浏览器打开官网工具）。
 */
@Composable
private fun FeatureRow(onOpenRoulette: () -> Unit) {
    val uriHandler = LocalUriHandler.current

    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
        Row(
            Modifier.padding(horizontal = 10.dp, vertical = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            FeatureTile(
                modifier = Modifier.weight(1f),
                icon = Icons.Rounded.Casino,
                title = "随机装备",
                desc = "随机生成完整装备",
                onClick = onOpenRoulette,
            )
            FeatureTile(
                modifier = Modifier.weight(1f),
                icon = Icons.Rounded.Map,
                title = "官方地图",
                desc = "物资点 / 撤离点",
                onClick = {
                    // 无浏览器等异常直接吞掉：入口卡上没有展示错误的地方
                    runCatching { uriHandler.openUri("https://df.qq.com/cp/a20240729directory/") }
                },
            )
        }
    }
}

@Composable
private fun FeatureTile(
    modifier: Modifier,
    icon: ImageVector,
    title: String,
    desc: String,
    onClick: () -> Unit,
) {
    Column(
        modifier = modifier
            .clip(RoundedCornerShape(12.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 2.dp, vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(5.dp),
    ) {
        IconBadge(icon, Accent, size = 38.dp, iconSize = 20.dp)
        Text(
            title,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Text(
            desc,
            fontSize = 9.5.sp,
            color = TextSecondary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

// ───────────────────────── 改枪码平台 ─────────────────────────

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LoadoutHeader(state: DeltaUiState, vm: DeltaForceViewModel) {
    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    Text(
                        "改枪码",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Text(
                        if (state.total > 0) "共 ${state.total} 条" else "点码块即可复制",
                        fontSize = 11.5.sp,
                        color = TextSecondary,
                    )
                }
                TextButton("上传改枪码", enabled = state.categories.isNotEmpty()) { vm.openUpload() }
            }

            // 分类接口挂了就把整条筛选藏掉：列表本身照样能看，留个空筛选条更让人以为坏了
            if (state.categories.isNotEmpty()) {
                FlowRow(
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    FilterPill(text = "全部", selected = state.query.categoryId == null) {
                        vm.selectCategory(null)
                    }
                    state.categories.forEach { cat ->
                        FilterPill(
                            text = cat.name,
                            count = cat.loadoutCount,
                            // 选中具体武器时高亮交给武器按钮，胶囊退回分类态
                            selected = state.query.categoryId == cat.id && state.query.weapon.isEmpty(),
                        ) { vm.selectCategory(cat.id) }
                    }
                }

                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    OutlinedTextField(
                        value = state.searchInput,
                        onValueChange = vm::onSearchText,
                        placeholder = { Text("搜索武器…", fontSize = 12.5.sp) },
                        singleLine = true,
                        textStyle = MaterialTheme.typography.bodyMedium.copy(fontSize = 12.5.sp),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(12.dp),
                    )
                    if (state.weapons.isNotEmpty()) {
                        WeaponMenu(weapons = state.weapons, selected = state.query.weapon, onSelect = vm::selectWeapon)
                    }
                }
            }

            when {
                state.listLoading && state.loadouts.isEmpty() -> LoadingLine("正在获取改枪码…")

                state.loadouts.isEmpty() && state.listError != null -> ErrorLine(
                    text = state.listError.orEmpty(),
                    onRetry = { vm.refreshList() },
                )

                state.loadouts.isEmpty() -> Column {
                    Text("暂无改枪码", fontSize = 12.sp, color = TextSecondary)
                    Text("换个筛选条件试试", fontSize = 11.sp, color = TextSecondary)
                }
            }
        }
    }
}

@Composable
private fun FilterPill(text: String, selected: Boolean, count: Int = 0, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .clip(CircleShape)
            .background(
                if (selected) MaterialTheme.colorScheme.primary
                else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f),
            )
            .clickable(onClick = onClick)
            .padding(horizontal = 13.dp, vertical = 7.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text,
                fontSize = 12.5.sp,
                fontWeight = FontWeight.Medium,
                color = if (selected) MaterialTheme.colorScheme.onPrimary else TextSecondary,
            )
            if (count > 0) {
                Spacer(Modifier.width(4.dp))
                Text(
                    "$count",
                    fontSize = 10.5.sp,
                    color = if (selected) MaterialTheme.colorScheme.onPrimary
                    else TextSecondary.copy(alpha = 0.75f),
                )
            }
        }
    }
}

@Composable
private fun WeaponMenu(weapons: List<DeltaWeapon>, selected: String, onSelect: (String) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        FilterPill(text = selected.ifEmpty { "全部武器" }, selected = selected.isNotEmpty()) { expanded = true }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            DropdownMenuItem(
                text = { Text("全部武器", fontSize = 13.sp) },
                onClick = {
                    expanded = false
                    onSelect("")
                },
            )
            weapons.forEach { weapon ->
                DropdownMenuItem(
                    text = {
                        Text(
                            "${weapon.weaponName} (${weapon.count})",
                            fontSize = 13.sp,
                            color = if (weapon.weaponName == selected) MaterialTheme.colorScheme.primary
                            else MaterialTheme.colorScheme.onSurface,
                        )
                    },
                    onClick = {
                        expanded = false
                        onSelect(weapon.weaponName)
                    },
                )
            }
        }
    }
}

@Composable
private fun LoadoutCard(
    item: DeltaLoadout,
    isCopied: Boolean,
    isLiked: Boolean,
    isReported: Boolean,
    onCopied: () -> Unit,
    onLike: () -> Unit,
    onReport: () -> Unit,
) {
    val clipboard = LocalClipboardManager.current

    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(9.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    item.weaponName,
                    fontSize = 14.5.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    item.categoryName,
                    fontSize = 10.5.sp,
                    color = MaterialTheme.colorScheme.primary,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.14f))
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    "💰 ${groupThousands(item.cost.toLong())}",
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    color = TextSecondary,
                )
            }

            // 整块可点：改枪码要换到游戏里粘贴，复制就是这条内容唯一的用途
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f))
                    .clickable {
                        clipboard.setText(AnnotatedString(item.code))
                        onCopied()
                    }
                    .padding(11.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    item.code,
                    fontSize = 12.sp,
                    fontFamily = FontFamily.Monospace,
                    lineHeight = 18.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        if (isCopied) Icons.Rounded.Done else Icons.Rounded.ContentCopy,
                        contentDescription = null,
                        tint = if (isCopied) MaterialTheme.colorScheme.primary else TextSecondary,
                        modifier = Modifier.size(13.dp),
                    )
                    Spacer(Modifier.width(5.dp))
                    Text(
                        if (isCopied) "已复制" else "复制改枪码",
                        fontSize = 11.sp,
                        color = if (isCopied) MaterialTheme.colorScheme.primary else TextSecondary,
                    )
                }
            }

            if (item.description.isNotBlank()) {
                Text(item.description, fontSize = 12.sp, lineHeight = 17.sp, color = TextSecondary)
            }

            Box(Modifier.fillMaxWidth().height(1.dp).background(Hairline))

            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "${item.author.ifBlank { "匿名" }} · ${item.createdAt.take(10)}",
                    fontSize = 11.sp,
                    color = TextSecondary,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                TextButton(
                    text = if (isReported) "已反馈" else "无法使用？",
                    enabled = !isReported,
                    color = if (isReported) MaterialTheme.colorScheme.primary else TextSecondary,
                    onClick = onReport,
                )
                Spacer(Modifier.width(2.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .clickable(enabled = !isLiked, onClick = onLike)
                        .padding(horizontal = 6.dp, vertical = 5.dp),
                ) {
                    Icon(
                        if (isLiked) Icons.Rounded.Favorite else Icons.Rounded.FavoriteBorder,
                        contentDescription = "点赞",
                        tint = if (isLiked) MaterialTheme.colorScheme.primary else TextSecondary,
                        modifier = Modifier.size(15.dp),
                    )
                    Text(
                        "${item.likes}",
                        fontSize = 11.5.sp,
                        color = if (isLiked) MaterialTheme.colorScheme.primary else TextSecondary,
                    )
                }
            }
        }
    }
}

@Composable
private fun PagerRow(state: DeltaUiState, vm: DeltaForceViewModel) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.Center,
    ) {
        PageButton(text = "上一页", enabled = state.query.page > 1) { vm.setPage(state.query.page - 1) }
        Text(
            "${state.query.page} / ${state.totalPages}",
            fontSize = 12.sp,
            color = TextSecondary,
            modifier = Modifier.padding(horizontal = 16.dp),
        )
        PageButton(text = "下一页", enabled = state.query.page < state.totalPages) { vm.setPage(state.query.page + 1) }
    }
}

@Composable
private fun PageButton(text: String, enabled: Boolean, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = if (enabled) 0.6f else 0.25f))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 9.dp),
    ) {
        Text(
            text,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
            color = if (enabled) MaterialTheme.colorScheme.onSurface else TextSecondary.copy(alpha = 0.5f),
        )
    }
}

/** 卡片右上角那种轻量文字按钮（刷新 / 上传 / 举报） */
@Composable
private fun TextButton(
    text: String,
    enabled: Boolean = true,
    color: Color = MaterialTheme.colorScheme.primary,
    onClick: () -> Unit,
) {
    Text(
        text,
        fontSize = 12.sp,
        fontWeight = FontWeight.Medium,
        color = if (enabled) color else color.copy(alpha = 0.4f),
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 6.dp),
    )
}

@Composable
private fun LoadingLine(text: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
        Text(text, fontSize = 12.sp, color = TextSecondary)
    }
}

@Composable
private fun ErrorLine(text: String, onRetry: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text,
            fontSize = 12.sp,
            lineHeight = 16.sp,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.weight(1f),
        )
        TextButton("重试", onClick = onRetry)
    }
}

/** 千分位手工分组：String.format 在部分 locale 下会把分隔符输出成点号 */
private fun groupThousands(value: Long): String {
    val digits = value.toString()
    val builder = StringBuilder()
    digits.reversed().forEachIndexed { index, char ->
        if (index > 0 && index % 3 == 0) builder.append(',')
        builder.append(char)
    }
    return builder.reverse().toString()
}
