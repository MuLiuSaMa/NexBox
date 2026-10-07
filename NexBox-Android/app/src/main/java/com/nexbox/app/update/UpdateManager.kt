package com.nexbox.app.update

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.FileProvider
import com.nexbox.app.data.NetworkModule
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import java.io.File

/**
 * 更新流程编排（状态机），对齐 PC 端 update-context 的两条路径：
 * - 自动检查：启动后静默执行，失败 / 已是最新都不打扰，发现新版本才弹窗；
 * - 手动检查：设置页「检查更新」触发，过程在弹窗可见，失败给错误弹窗，已是最新用 toast。
 *
 * 安装走系统安装器（FileProvider + ACTION_VIEW）；未授予「安装未知应用」时先跳授权页，
 * 回到前台后由 [onHostResumed] 自动续装。
 */
object UpdateManager {

    sealed interface State {
        /** 空闲：从未检查过，或上次检查已是最新 */
        data object Idle : State

        /** 正在检查（仅手动检查进入；自动检查保持 Idle，避免设置页闪状态） */
        data object Checking : State

        /** 已是最新（手动检查的结果） */
        data object UpToDate : State

        /** 发现新版本，等待用户决定 */
        data class Available(val info: UpdateChecker.UpdateInfo) : State

        /** 下载中，progress 0..100 */
        data class Downloading(val info: UpdateChecker.UpdateInfo, val progress: Int) : State

        /** 下载完成，等待 / 正在安装 */
        data class Downloaded(val info: UpdateChecker.UpdateInfo, val file: File) : State

        /** 检查或下载失败；info 非空说明是下载失败，可原地重试 */
        data class Failed(val message: String, val info: UpdateChecker.UpdateInfo? = null) : State
    }

    var state by mutableStateOf<State>(State.Idle)
        private set

    /** 更新弹窗可见性：自动检查发现新版本、手动检查、从设置页重新拉起都置 true */
    var showDialog by mutableStateOf(false)
        private set

    /** 轻提示（已是最新等），由根布局的 MessageToast 消费，展示完调 [clearToast] 收起 */
    var toast by mutableStateOf<String?>(null)
        private set

    fun clearToast() {
        toast = null
    }

    /** 「安装未知应用」授权跳转后置位，回到前台时若已授权则自动续装 */
    private var pendingInstall = false

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var checkJob: Job? = null
    private var downloadJob: Job? = null

    /**
     * 启动后的静默检查：失败 / 已是最新都保持安静，发现新版本才把弹窗拉起来。
     * 已有检查或下载在进行时不重复发起。
     */
    fun autoCheck() {
        if (checkJob?.isActive == true || downloadJob?.isActive == true) return
        checkJob = scope.launch {
            val info = runCatching { UpdateChecker.fetchLatest() }.getOrNull() ?: return@launch
            if (!UpdateChecker.isNewer(info.version)) return@launch
            state = State.Available(info)
            showDialog = true
        }
    }

    /** 设置页手动检查：有进行中的流程就只把弹窗拉起来，否则真正发一次检查 */
    fun manualCheck() {
        when (state) {
            is State.Checking, is State.Available, is State.Downloading, is State.Downloaded, is State.Failed -> {
                showDialog = true
                return
            }
            State.Idle, State.UpToDate -> Unit
        }
        checkJob?.cancel()
        checkJob = scope.launch {
            state = State.Checking
            showDialog = true
            val info = try {
                UpdateChecker.fetchLatest()
            } catch (e: Exception) {
                if (e is CancellationException) throw e
                state = State.Failed(e.message ?: "网络异常，请稍后重试")
                return@launch
            }
            if (info == null) {
                state = State.Failed("未在更新源找到可安装的 APK，请稍后重试")
                return@launch
            }
            if (!UpdateChecker.isNewer(info.version)) {
                state = State.UpToDate
                showDialog = false
                toast = "已是最新版本 v${info.version}"
                return@launch
            }
            state = State.Available(info)
        }
    }

    /** 开始下载（可用态与失败重试共用入口） */
    fun startDownload() {
        val info = when (val s = state) {
            is State.Available -> s.info
            is State.Failed -> s.info
            else -> return
        } ?: return
        downloadJob?.cancel()
        downloadJob = scope.launch {
            state = State.Downloading(info, 0)
            try {
                val file = UpdateDownloader.download(NetworkModule.appContext, info.apkUrl) { pct ->
                    val s = state
                    if (s is State.Downloading) state = s.copy(progress = pct)
                }
                state = State.Downloaded(info, file)
            } catch (e: CancellationException) {
                // 用户取消：回到「可下载」而不是失败，弹窗里可以直接再点下载
                state = State.Available(info)
                throw e
            } catch (e: Exception) {
                state = State.Failed(e.message ?: "下载失败，请稍后重试", info)
            }
        }
    }

    /** 取消下载：断流、清理临时文件，状态由下载协程回滚到可用 */
    fun cancelDownload() {
        downloadJob?.cancel()
        downloadJob = null
    }

    /**
     * 拉起系统安装器。未授予「安装未知应用」权限时改为跳转授权页，
     * 授权回到前台后由 [onHostResumed] 自动续装。
     */
    fun install() {
        val context = NetworkModule.appContext
        val file = (state as? State.Downloaded)?.file ?: return
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            !context.packageManager.canRequestPackageInstalls()
        ) {
            pendingInstall = true
            runCatching {
                context.startActivity(
                    Intent(
                        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                        Uri.parse("package:${context.packageName}"),
                    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }.onFailure {
                pendingInstall = false
                toast = "请先在系统设置中允许本应用安装未知应用"
            }
            return
        }
        launchInstaller(context, file)
    }

    /** 宿主 Activity ON_RESUME 回调：刚授权完「安装未知应用」就自动续装 */
    fun onHostResumed() {
        if (!pendingInstall) return
        val context = NetworkModule.appContext
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O ||
            context.packageManager.canRequestPackageInstalls()
        ) {
            (state as? State.Downloaded)?.let { launchInstaller(context, it.file) }
        }
    }

    private fun launchInstaller(context: android.content.Context, file: File) {
        pendingInstall = false
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
        runCatching {
            context.startActivity(
                Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK),
            )
        }.onFailure { toast = "无法启动安装器，请重试" }
    }

    /** 关闭弹窗（各状态共用的退出路径）：检查中 / 下载中随之取消，其余状态保留待设置页展示 */
    fun dismiss() {
        showDialog = false
        when (state) {
            is State.Checking -> {
                checkJob?.cancel()
                state = State.Idle
            }
            is State.Downloading -> cancelDownload() // 下载协程会把状态回滚到 Available
            is State.Failed -> state = State.Idle
            // Available / Downloaded 保留：设置页还能看到「发现 vX.Y.Z」/「待安装」
            else -> Unit
        }
    }
}
