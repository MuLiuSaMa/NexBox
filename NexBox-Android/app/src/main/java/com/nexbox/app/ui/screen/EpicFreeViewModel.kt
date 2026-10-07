package com.nexbox.app.ui.screen

import android.app.Application
import androidx.compose.runtime.Immutable
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import coil3.ImageLoader
import coil3.request.ImageRequest
import com.nexbox.app.data.EpicFreeStore
import com.nexbox.app.data.EpicGame
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * Epic 喜加一列表。[games] 为空且 [loaded] 为真表示「本周确实没有限免」，
 * 与 [error]（拉取失败）是两回事，不能混成一个空态。
 */
@Immutable
data class EpicUiState(
    val loading: Boolean = false,
    val loaded: Boolean = false,
    val games: List<EpicGame> = emptyList(),
    val error: String? = null,
)

/** 站外接口，与 PC 端连接状态无关，所以独立一个 VM；接口抖动由 [EpicFreeStore] 自己退避重试 */
class EpicFreeViewModel(app: Application) : AndroidViewModel(app) {

    private val _state = MutableStateFlow(EpicUiState())
    val state: StateFlow<EpicUiState> = _state.asStateFlow()

    fun refresh(force: Boolean = false) {
        val s = _state.value
        if (s.loading || (s.loaded && !force)) return
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            runCatching { EpicFreeStore.fetch() }
                .onSuccess { games ->
                    _state.update { it.copy(loading = false, loaded = true, games = games, error = null) }
                    prefetchCovers(games)
                }
                .onFailure { e ->
                    // 只记错误、不动 games：一次接口抖动不该把刚看到过的限免信息抹掉，
                    // UI 只在列表为空时才把错误顶上去
                    _state.update {
                        it.copy(loading = false, loaded = true, error = e.toUserMessage())
                    }
                }
        }
    }

    /**
     * 列表一到手就把要展示的封面预取进 Coil 缓存。Epic 的 CDN 在国内本来就要几秒，
     * 等卡片组合完再发请求会白等一帧，提前发起能缩短那段「只有灰块」的时间。
     */
    private fun prefetchCovers(games: List<EpicGame>) {
        val ctx = getApplication<Application>()
        runCatching {
            // Coil 3 没有 Coil 2 那个 `Coil` 单例；这个工厂函数返回全局共享的 loader，
            // 与 AsyncImage 内部用的是同一个，预取结果才能直接被卡片命中
            val loader = ImageLoader(ctx)
            games.asSequence()
                .map { it.cover }
                .filter { it.isNotBlank() }
                .take(PREFETCH_COVERS)
                .forEach { url -> loader.enqueue(ImageRequest.Builder(ctx).data(url).build()) }
        }
    }

    private companion object {
        const val PREFETCH_COVERS = 3
    }
}
