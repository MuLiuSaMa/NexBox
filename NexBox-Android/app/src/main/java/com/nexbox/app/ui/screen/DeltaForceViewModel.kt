package com.nexbox.app.ui.screen

import android.app.Application
import androidx.compose.runtime.Immutable
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.DeltaCategory
import com.nexbox.app.data.DeltaForceStore
import com.nexbox.app.data.DeltaLoadout
import com.nexbox.app.data.DeltaLoadoutDraft
import com.nexbox.app.data.DeltaLoadoutQuery
import com.nexbox.app.data.DeltaPasswordItem
import com.nexbox.app.data.DeltaWeapon
import com.nexbox.app.data.toUserMessage
import java.util.concurrent.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * 三角洲页状态。
 *
 * [passwords] / 列表在拉取失败时都**保留上一次的数据**只记错误：接口抖一下不该把
 * 用户正在看的东西抹掉，UI 只在列表为空时才把错误顶上去（与 Epic 喜加一同一个取舍）。
 */
@Immutable
data class DeltaUiState(
    // 进页面就会起首拉一次，默认置 true：否则首帧会闪一下「暂无数据」，像坏了
    val passwords: List<DeltaPasswordItem> = emptyList(),
    val passwordsLoading: Boolean = true,
    val passwordError: String? = null,

    val categories: List<DeltaCategory> = emptyList(),
    val weapons: List<DeltaWeapon> = emptyList(),
    val query: DeltaLoadoutQuery = DeltaLoadoutQuery(),
    /** 搜索框的原始输入：与 query.search（已防抖生效的关键字）分开，输入过程不被网络节奏打断 */
    val searchInput: String = "",
    val loadouts: List<DeltaLoadout> = emptyList(),
    val total: Int = 0,
    val totalPages: Int = 1,
    val listLoading: Boolean = true,
    val listError: String? = null,

    /** 点赞/举报只在本次会话内去重，和 PC 端一样不落本地存储 */
    val likedIds: Set<Int> = emptySet(),
    val reportedIds: Set<Int> = emptySet(),
    val copiedId: Int? = null,
    /** 密码格按地图名记「已复制」：密码本身没有 id，用下标会和改枪码的 id 撞车 */
    val copiedPassword: String? = null,

    val uploadOpen: Boolean = false,
    val uploadCategoryId: Int? = null,
    val uploadWeapon: String = "",
    val uploadWeapons: List<DeltaWeapon> = emptyList(),
    val uploadCode: String = "",
    val uploadDescription: String = "",
    val uploadCost: String = "",
    val uploadAuthor: String = "",
    val submitting: Boolean = false,
    val uploadError: String? = null,

    val message: String? = null,
    val messageIsError: Boolean = false,
)

/**
 * 三角洲页：每日密码 + 改枪码平台。
 *
 * 数据全部来自站外公网，与 PC 端连接状态无关，所以独立一个 VM，不碰 [com.nexbox.app.data.NexBoxClient]。
 */
class DeltaForceViewModel(app: Application) : AndroidViewModel(app) {

    private val _state = MutableStateFlow(DeltaUiState())
    val state: StateFlow<DeltaUiState> = _state.asStateFlow()

    private var pollJob: Job? = null
    private var passwordJob: Job? = null
    private var listJob: Job? = null
    private var filterWeaponsJob: Job? = null
    private var uploadWeaponsJob: Job? = null
    private var searchJob: Job? = null
    private var copiedJob: Job? = null

    /**
     * 页面可见时传 true，离开传 false。
     * VM 挂在 Activity 上，切走标签它不会销毁，不停掉轮询就会一直在后台打公网。
     */
    fun setActive(active: Boolean) {
        if (!active) {
            pollJob?.cancel()
            pollJob = null
            return
        }
        if (pollJob?.isActive == true) return
        pollJob = viewModelScope.launch {
            bootstrap()
            while (isActive) {
                delay(POLL_MS)
                refreshPasswords()
                // 投稿填到一半时别动列表：整屏内容突然换掉会把人看的那条顶走
                if (!_state.value.submitting) refreshList(silent = true)
            }
        }
    }

    private fun bootstrap() {
        refreshPasswords()
        loadCategories()
        refreshList()
    }

    // ───────────────────────── 每日密码 ─────────────────────────

    fun refreshPasswords(force: Boolean = false) {
        if (passwordJob?.isActive == true) return
        passwordJob = viewModelScope.launch {
            if (_state.value.passwords.isEmpty()) {
                _state.update { it.copy(passwordsLoading = true, passwordError = null) }
            }
            runCatching { DeltaForceStore.fetchPasswords(force) }
                .onSuccess { items ->
                    _state.update { it.copy(passwords = items, passwordsLoading = false, passwordError = null) }
                }
                .onFailure { e ->
                    if (e is CancellationException) return@onFailure
                    _state.update { it.copy(passwordsLoading = false, passwordError = e.toUserMessage()) }
                }
        }
    }

    // ───────────────────────── 分类 / 武器 / 列表 ─────────────────────────

    private fun loadCategories() {
        if (_state.value.categories.isNotEmpty()) return
        viewModelScope.launch {
            // 分类只喂筛选条，拉不到就整条隐藏，列表本身照常能看，不单独报错吓人
            runCatching { DeltaForceStore.fetchCategories() }
                .onSuccess { cats -> _state.update { it.copy(categories = cats) } }
        }
    }

    fun selectCategory(id: Int?) {
        filterWeaponsJob?.cancel()
        _state.update { it.copy(query = it.query.copy(categoryId = id, weapon = "", page = 1), weapons = emptyList()) }
        if (id != null) {
            filterWeaponsJob = viewModelScope.launch {
                runCatching { DeltaForceStore.fetchWeapons(id) }
                    .onSuccess { list ->
                        // 回来时可能又切了分类，回写前对一下，否则武器下拉会挂到别的分类上
                        if (_state.value.query.categoryId == id) _state.update { it.copy(weapons = list) }
                    }
            }
        }
        refreshList()
    }

    fun selectWeapon(weapon: String) {
        _state.update { it.copy(query = it.query.copy(weapon = weapon, page = 1)) }
        refreshList()
    }

    /** 输入即防抖，300ms 内的连续按键只发一次请求（同 PC 端 searchTimer） */
    fun onSearchText(text: String) {
        _state.update { it.copy(searchInput = text) }
        searchJob?.cancel()
        searchJob = viewModelScope.launch {
            delay(SEARCH_DEBOUNCE_MS)
            val keyword = text.trim()
            if (_state.value.query.search == keyword) return@launch
            _state.update { it.copy(query = it.query.copy(search = keyword, page = 1)) }
            refreshList()
        }
    }

    fun setPage(page: Int) {
        val target = page.coerceIn(1, _state.value.totalPages)
        if (_state.value.query.page == target) return
        _state.update { it.copy(query = it.query.copy(page = target)) }
        refreshList()
    }

    /**
     * 拉列表。[silent] 用于轮询：不置 loading，否则每分钟整屏闪一次转圈。
     *
     * 取消旧请求只是省流量，拦不住已经恢复执行的协程往下写状态，
     * 所以成功/失败回调都要再对一次发起时的 query 快照。
     */
    fun refreshList(silent: Boolean = false) {
        listJob?.cancel()
        val requested = _state.value.query
        if (!silent) _state.update { it.copy(listLoading = true, listError = null) }
        listJob = viewModelScope.launch {
            runCatching { DeltaForceStore.fetchLoadouts(requested) }
                .onSuccess { page ->
                    if (_state.value.query != requested) return@onSuccess
                    _state.update {
                        it.copy(
                            loadouts = page.data,
                            total = page.total,
                            totalPages = page.totalPages.coerceAtLeast(1),
                            listLoading = false,
                            listError = null,
                        )
                    }
                }
                .onFailure { e ->
                    if (e is CancellationException) return@onFailure
                    if (_state.value.query != requested) return@onFailure
                    _state.update { it.copy(listLoading = false, listError = e.toUserMessage()) }
                }
        }
    }

    // ───────────────────────── 复制 / 点赞 / 举报 ─────────────────────────

    /** 剪贴板由 UI 侧写（要拿 LocalClipboardManager），这里只管「已复制」徽标的 2 秒窗口 */
    fun markCopied(id: Int) {
        copiedJob?.cancel()
        _state.update { it.copy(copiedId = id, copiedPassword = null) }
        copiedJob = viewModelScope.launch {
            delay(COPIED_HINT_MS)
            _state.update { if (it.copiedId == id) it.copy(copiedId = null) else it }
        }
    }

    fun markCopiedPassword(name: String) {
        copiedJob?.cancel()
        _state.update { it.copy(copiedPassword = name, copiedId = null) }
        copiedJob = viewModelScope.launch {
            delay(COPIED_HINT_MS)
            _state.update { if (it.copiedPassword == name) it.copy(copiedPassword = null) else it }
        }
    }

    fun like(item: DeltaLoadout) {
        val id = item.id
        if (id in _state.value.likedIds) return
        viewModelScope.launch {
            runCatching { DeltaForceStore.like(id) }
                .onSuccess { likes ->
                    _state.update { s ->
                        s.copy(
                            likedIds = s.likedIds + id,
                            loadouts = s.loadouts.map { if (it.id == id) it.copy(likes = likes) else it },
                        )
                    }
                }
                .onFailure { e -> showMessage("点赞失败：${e.toUserMessage()}", isError = true) }
        }
    }

    fun report(item: DeltaLoadout) {
        val id = item.id
        if (id in _state.value.reportedIds) return
        viewModelScope.launch {
            runCatching { DeltaForceStore.report(id) }
                .onSuccess {
                    _state.update { it.copy(reportedIds = it.reportedIds + id) }
                    showMessage("已报告，管理员会跟进这条改枪码")
                }
                .onFailure { e -> showMessage("举报失败：${e.toUserMessage()}", isError = true) }
        }
    }

    // ───────────────────────── 投稿 ─────────────────────────

    fun openUpload() {
        val first = _state.value.categories.firstOrNull()
        _state.update {
            it.copy(
                uploadOpen = true,
                uploadError = null,
                uploadCategoryId = first?.id,
                uploadWeapon = "",
                uploadWeapons = emptyList(),
                uploadCode = "",
                uploadDescription = "",
                uploadCost = "",
                uploadAuthor = "",
            )
        }
        first?.let { loadUploadWeapons(it.id) }
    }

    fun closeUpload() {
        _state.update { it.copy(uploadOpen = false) }
    }

    fun selectUploadWeapon(weapon: String) {
        _state.update { it.copy(uploadWeapon = weapon) }
    }

    fun selectUploadCategory(id: Int?) {
        uploadWeaponsJob?.cancel()
        _state.update { it.copy(uploadCategoryId = id, uploadWeapon = "", uploadWeapons = emptyList()) }
        if (id != null) loadUploadWeapons(id)
    }

    private fun loadUploadWeapons(categoryId: Int) {
        uploadWeaponsJob?.cancel()
        uploadWeaponsJob = viewModelScope.launch {
            runCatching { DeltaForceStore.fetchWeapons(categoryId) }
                .onSuccess { list ->
                    if (_state.value.uploadCategoryId == categoryId) {
                        _state.update { it.copy(uploadWeapons = list) }
                    }
                }
        }
    }

    fun onUploadCode(text: String) = _state.update { it.copy(uploadCode = text.take(CODE_MAX)) }
    fun onUploadDescription(text: String) = _state.update { it.copy(uploadDescription = text.take(DESC_MAX)) }
    fun onUploadCost(text: String) = _state.update { it.copy(uploadCost = text.filter(Char::isDigit).take(COST_MAX)) }
    fun onUploadAuthor(text: String) = _state.update { it.copy(uploadAuthor = text.take(AUTHOR_MAX)) }

    /** 校验规则与 PC 端上传弹窗逐条对齐 */
    fun submitUpload() {
        val s = _state.value
        if (s.submitting) return
        val categoryId = s.uploadCategoryId
        if (categoryId == null) {
            _state.update { it.copy(uploadError = "请选择分类") }
            return
        }
        if (s.uploadWeapon.isBlank()) {
            _state.update { it.copy(uploadError = "请选择武器") }
            return
        }
        if (s.uploadCode.isBlank()) {
            _state.update { it.copy(uploadError = "请输入改枪码") }
            return
        }
        val cost = s.uploadCost.toLongOrNull() ?: 0L
        if (cost <= 0L) {
            _state.update { it.copy(uploadError = "请输入有效的改枪费用") }
            return
        }

        _state.update { it.copy(submitting = true, uploadError = null) }
        viewModelScope.launch {
            runCatching {
                DeltaForceStore.submit(
                    DeltaLoadoutDraft(
                        categoryId = categoryId,
                        weaponName = s.uploadWeapon,
                        code = s.uploadCode.trim(),
                        cost = cost,
                        description = s.uploadDescription.trim(),
                        author = s.uploadAuthor.trim().ifEmpty { "匿名" },
                    ),
                )
            }
                .onSuccess {
                    _state.update { it.copy(submitting = false, uploadOpen = false) }
                    showMessage("已提交，等待管理员审核")
                    // 回第 1 页重拉：新投稿要过审核，看不到也只提示提交成功，不假装已上架
                    _state.update { it.copy(query = it.query.copy(page = 1)) }
                    refreshList()
                }
                .onFailure { e ->
                    _state.update {
                        it.copy(submitting = false, uploadError = "提交失败：${e.toUserMessage()}")
                    }
                }
        }
    }

    // ───────────────────────── 轻提示 ─────────────────────────

    private fun showMessage(text: String, isError: Boolean = false) {
        _state.update { it.copy(message = text, messageIsError = isError) }
    }

    fun consumeMessage() {
        _state.update { it.copy(message = null) }
    }

    private companion object {
        const val POLL_MS = 60_000L
        const val SEARCH_DEBOUNCE_MS = 300L
        const val COPIED_HINT_MS = 1_500L
        const val CODE_MAX = 500
        const val DESC_MAX = 200
        const val COST_MAX = 12
        const val AUTHOR_MAX = 20
    }
}
