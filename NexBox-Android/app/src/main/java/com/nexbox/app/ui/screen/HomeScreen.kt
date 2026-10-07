package com.nexbox.app.ui.screen

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.nexbox.app.data.AnnouncementStore
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.ui.MessageToast
import com.nexbox.app.ui.screen.home.AddDeviceDialog
import com.nexbox.app.ui.screen.home.AnnouncementListDialog
import com.nexbox.app.ui.screen.home.FileTransferCard
import com.nexbox.app.ui.screen.home.HomeTopCards
import com.nexbox.app.ui.screen.home.ImportantAnnouncementDialog
import com.nexbox.app.ui.screen.home.LinkPhase
import com.nexbox.app.ui.screen.home.PcLinkCard
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * 主页 = 顶部三小卡（今日人气 / 公告 / 随机一言）+ PC 互联小横板 + 「添加设备」。
 * 原连接页的手动 IP 表单按需求砍掉，扫码与扫描都收敛进添加弹窗；
 * 配对成功后点互联卡进设备详情页（硬件监控 + 游戏模式）。
 */
@Composable
fun HomeScreen(
    vm: ConnectViewModel = viewModel(),
    hw: HardwareViewModel = viewModel(),
    pc: PcControlViewModel = viewModel(),
    ft: FileTransferViewModel = viewModel(),
) {
    val state by vm.state.collectAsStateWithLifecycle()
    val hwState by hw.state.collectAsStateWithLifecycle()
    val pcState by pc.state.collectAsStateWithLifecycle()
    val ftState by ft.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()

    // —— 页面层级：主页 ↔ CPU / GPU 图表详情页 / 文件互传页 ——
    // 存枚举名而不是枚举本身：rememberSaveable 的自动 Saver 对 null 值不友好，字符串最稳。
    var detailKey by rememberSaveable { mutableStateOf("") }
    val detail = HardwareDetailKind.entries.firstOrNull { it.name == detailKey }
    val onTransfer = detailKey == "file_transfer"
    // 断连 / 解绑后不留图表页：里面会全是「—」（互传页靠 REST，断 WS 仍可看，保留）
    LaunchedEffect(state.session, state.wsConnected) {
        if (state.session == null) detailKey = ""
        else if (!state.wsConnected && !onTransfer) detailKey = ""
    }
    BackHandler(enabled = detail != null || onTransfer) { detailKey = "" }

    // —— 弹窗状态 ——
    var showAdd by rememberSaveable { mutableStateOf(false) }
    var addMode by rememberSaveable { mutableStateOf("menu") } // menu | scan
    var showAnnouncements by rememberSaveable { mutableStateOf(false) }
    var importantKey by rememberSaveable { mutableStateOf("") }
    var importantHandled by remember { mutableStateOf(false) }

    // 配对成功（拿到会话）自动关闭添加弹窗
    LaunchedEffect(state.session) {
        if (state.session != null && showAdd) {
            showAdd = false
            addMode = "menu"
        }
    }

    // 公告：进主页刷新一次（AnnouncementStore 内置 10 分钟内存缓存）
    LaunchedEffect(Unit) { AnnouncementStore.refresh() }

    // 重要公告强制弹窗：数据到位后停 1.5s 再弹（对齐 PC 端节奏），一次进页最多弹一条
    val announcements by AnnouncementStore.items.collectAsStateWithLifecycle()
    val confirmed by NetworkModule.homePrefs.confirmedImportant.collectAsStateWithLifecycle(initialValue = emptySet())
    LaunchedEffect(announcements, confirmed) {
        if (importantHandled) return@LaunchedEffect
        val pending = announcements.firstOrNull { it.important && it.confirmKey !in confirmed }
            ?: return@LaunchedEffect
        delay(1500)
        importantHandled = true
        importantKey = pending.confirmKey
    }

    // —— 扫码：扫到内容直接进配对并收起弹窗；取消扫描则留在弹窗 ——
    val scanner = rememberLauncherForActivityResult(ScanContract()) { result ->
        if (result.contents != null) {
            showAdd = false
            addMode = "menu"
            vm.onQrScanned(result.contents)
        }
    }
    val launchScan: () -> Unit = {
        scanner.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("对准 PC 端「手机远程连接」里的二维码")
                .setBeepEnabled(false)
                // 方向交回清单：清单里已把 CaptureActivity 覆盖成 fullSensor 跟随设备
                .setOrientationLocked(false)
        )
    }

    // 进入「扫描设备」页自动开始搜索（只列清单，配对由用户点「手动配对」触发）
    LaunchedEffect(showAdd, addMode) {
        if (showAdd && addMode == "scan") vm.startScan(restart = true)
    }

    // 互联卡相位：只由会话与实时通道决定 —— 搜索/配对过程只显示在添加弹窗里，
    // 未配对时卡片保持故事动画，不被扫描状态打扰
    val phase = when {
        state.session != null && state.wsConnected -> LinkPhase.CONNECTED
        state.session != null -> LinkPhase.PAIRED_OFFLINE
        else -> LinkPhase.IDLE
    }

    Box(Modifier.fillMaxSize()) {
        // 主页 ↔ 详情页转场：进从右滑入、返回反向（detailKey 非空 = 在详情页）
        AnimatedContent(
            targetState = detailKey,
            transitionSpec = {
                val forward = targetState.isNotEmpty()
                if (forward) {
                    (slideInHorizontally(tween(240)) { it / 2 } + fadeIn(tween(240)))
                        .togetherWith(slideOutHorizontally(tween(240)) { -it / 6 } + fadeOut(tween(180)))
                } else {
                    (slideInHorizontally(tween(240)) { -it / 6 } + fadeIn(tween(240)))
                        .togetherWith(slideOutHorizontally(tween(240)) { it / 2 } + fadeOut(tween(180)))
                }
            },
            label = "home-hardware-detail",
            modifier = Modifier.fillMaxSize(),
        ) { key ->
            val kind = HardwareDetailKind.entries.firstOrNull { it.name == key }
            when {
                key == "file_transfer" -> FileTransferScreen(onBack = { detailKey = "" })
                kind != null -> HardwareDetailScreen(kind = kind, state = hwState, onBack = { detailKey = "" })
                else ->
                    HomeContent(
                        state = state,
                        hwState = hwState,
                        controlState = pcState,
                        // 离线（重连中）时把四个开关置灰：没有 WS 就收不到 action.done，
                        // 点了也不会有反馈，禁用比「点了没反应」体验好
                        controlsEnabled = phase == LinkPhase.CONNECTED,
                        phase = phase,
                        transferPending = ftState.incoming.count { !it.acked },
                        onOpenCpuDetail = { detailKey = HardwareDetailKind.CPU.name },
                        onOpenGpuDetail = { detailKey = HardwareDetailKind.GPU.name },
                        onOpenTransfer = { detailKey = "file_transfer" },
                        onMemoryClean = hw::optimizeMemory,
                        onToggleFeature = pc::setEnabled,
                        onUnpair = vm::unpair,
                        onAdd = {
                            addMode = "menu"
                            showAdd = true
                        },
                        onOpenAnnouncements = {
                            // 打开即视为已读：把已读水位推到最新一条
                            val latest = AnnouncementStore.items.value
                                .maxOfOrNull { it.createTime.orEmpty() }
                                .orEmpty()
                            scope.launch { NetworkModule.homePrefs.markAnnouncementsRead(latest) }
                            showAnnouncements = true
                        },
                    )
            }
        }

        // 轻提示：连接页 / 硬件页 / PC 功能开关共用一个通道，按优先级取第一个非空的
        MessageToast(
            message = state.message ?: hwState.message ?: pcState.message,
            isError = when {
                state.message != null -> state.messageIsError
                hwState.message != null -> hwState.messageIsError
                else -> pcState.messageIsError
            },
            onDismiss = {
                when {
                    state.message != null -> vm.consumeMessage()
                    hwState.message != null -> hw.consumeMessage()
                    else -> pc.consumeMessage()
                }
            },
            modifier = Modifier
                .align(Alignment.TopEnd)
                .statusBarsPadding()
                .padding(top = 8.dp, end = 12.dp)
                .fillMaxWidth(0.86f),
        )

        if (showAdd) {
            AddDeviceDialog(
                mode = addMode,
                state = state,
                onScanDevices = { addMode = "scan" }, // 切到扫描页；搜索由下面的 LaunchedEffect 启动
                onPick = vm::requestPairWith,
                onScanQr = launchScan,
                onBackToMenu = { addMode = "menu" },
                onDismiss = { showAdd = false },
            )
        }

        if (showAnnouncements) {
            AnnouncementListDialog(
                items = announcements,
                loading = AnnouncementStore.loading.value,
                onDismiss = { showAnnouncements = false },
            )
        }

        val importantItem = announcements.firstOrNull { it.confirmKey == importantKey }
        if (importantItem != null) {
            ImportantAnnouncementDialog(
                item = importantItem,
                onConfirm = {
                    scope.launch { NetworkModule.homePrefs.confirmImportant(importantItem.confirmKey) }
                    importantKey = ""
                },
            )
        }
    }
}

@Composable
private fun HomeContent(
    state: ConnectUiState,
    hwState: HardwareUiState,
    controlState: PcControlUiState,
    controlsEnabled: Boolean,
    phase: LinkPhase,
    /** PC 发来的待接收文件数（文件互传卡副标题角标） */
    transferPending: Int,
    onOpenCpuDetail: () -> Unit,
    onOpenGpuDetail: () -> Unit,
    onOpenTransfer: () -> Unit,
    onMemoryClean: () -> Unit,
    onToggleFeature: (PcFeature, Boolean) -> Unit,
    onUnpair: () -> Unit,
    onAdd: () -> Unit,
    onOpenAnnouncements: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        TopBar()

        HomeTopCards(onOpenAnnouncements = onOpenAnnouncements)

        // 一整块互联卡：未配对时是配对故事 + 卡内「添加设备」；已配对后顶部显示
        // 名称 / 已连接 + 右上角「断开连接」，主体是**可左右翻页的两页** ——
        // 第一页硬件四宫格（CPU / GPU 格进详情页，内存格点一下做「一键内存优化」），
        // 第二页四个 PC 功能开关。两态结构对称，卡片高度一致。
        PcLinkCard(
            phase = phase,
            pcName = state.pcName.orEmpty().ifBlank { state.session?.deviceName.orEmpty() },
            targetName = state.approvalDeviceName,
            hwState = hwState,
            controlState = controlState,
            controlsEnabled = controlsEnabled,
            unpairBusy = state.busy,
            onAdd = onAdd,
            onOpenCpuDetail = onOpenCpuDetail,
            onOpenGpuDetail = onOpenGpuDetail,
            onMemoryClean = onMemoryClean,
            onToggleFeature = onToggleFeature,
            onUnpair = onUnpair,
        )

        // 文件互传卡：配对完成后出现，点进互传页（收 PC 发来的文件 / 发文件给 PC）
        if (phase != LinkPhase.IDLE) {
            FileTransferCard(
                pendingCount = transferPending,
                connected = phase == LinkPhase.CONNECTED,
                onClick = onOpenTransfer,
            )
        }


        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量：内容从它下面穿过，末尾留出高度不被永久遮住
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

/** 顶部：一行标题 + BETA 角标（连接状态只在互联卡里显示，这里不再重复一个胶囊） */
@Composable
private fun TopBar() {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            "新境盒-安卓端",
            fontSize = 20.sp,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onBackground,
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
