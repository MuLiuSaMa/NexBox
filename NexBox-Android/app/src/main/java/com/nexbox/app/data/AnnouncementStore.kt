package com.nexbox.app.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import okhttp3.Request

private val Context.homeDataStore: DataStore<Preferences> by preferencesDataStore(name = "nexbox_home")

/** 今天的日期串（yyyy-MM-dd），今日人气按天重置用它当 key */
fun todayKey(): String = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date())

/**
 * 一条公告。字段名与 PC 端 announcement.rs 解析的 notice.json 完全同源
 * （https://gitee.com/muliuawa/nexbox/raw/master/notice.json）。
 */
@Serializable
data class Announcement(
    val title: String? = null,
    val content: String? = null,
    val important: Boolean = false,
    @SerialName("create_time") val createTime: String? = null,
) {
    /** 与 PC 端 ImportantAnnouncementModal 相同的确认键：标题_时间 */
    val confirmKey: String get() = "${title.orEmpty()}_${createTime.orEmpty()}"
}

@Serializable
private data class AnnouncementFeedDto(
    val version: Long = 0,
    @SerialName("announce_list") val announceList: List<Announcement> = emptyList(),
)

/**
 * 主页轻量偏好：今日人气、公告已读水位、已确认的重要公告。
 * 与会话存储分开一个 DataStore 文件，互不干扰无效写。
 */
class HomePrefs(context: Context) {

    private val store = context.applicationContext.homeDataStore

    /** 今日人气：(日期, 值)；没生成过时日期为 null */
    val popularity: kotlinx.coroutines.flow.Flow<Pair<String?, Int>> = store.data.map { p ->
        p[KEY_POPULARITY_DATE] to (p[KEY_POPULARITY_VALUE] ?: -1)
    }

    /**
     * 取今日人气：今天已生成过就返回原值，否则生成 0~100 并落盘。
     * 并发点击由 DataStore 单写者串行化，两次点击只会生成一次。
     */
    suspend fun popularityToday(today: String = todayKey()): Int {
        val p = store.data.first()
        val savedDate = p[KEY_POPULARITY_DATE]
        val saved = p[KEY_POPULARITY_VALUE] ?: -1
        if (savedDate == today && saved in 0..100) return saved
        val fresh = (0..100).random()
        store.edit {
            it[KEY_POPULARITY_DATE] = today
            it[KEY_POPULARITY_VALUE] = fresh
        }
        return fresh
    }

    /** 公告已读水位：存已读到的最新 create_time，早于它的都算未读（口径与 PC 端一致） */
    val announcementsRead: kotlinx.coroutines.flow.Flow<String> = store.data.map { it[KEY_ANNOUNCE_READ].orEmpty() }

    suspend fun markAnnouncementsRead(latestCreateTime: String) {
        if (latestCreateTime.isBlank()) return
        store.edit { it[KEY_ANNOUNCE_READ] = latestCreateTime }
    }

    /** 已确认过的重要公告（confirmKey 集合） */
    val confirmedImportant: kotlinx.coroutines.flow.Flow<Set<String>> =
        store.data.map { it[KEY_IMPORTANT_CONFIRMED].orEmpty().toSet() }

    suspend fun confirmImportant(key: String) {
        store.edit { it[KEY_IMPORTANT_CONFIRMED] = (it[KEY_IMPORTANT_CONFIRMED].orEmpty() + key) }
    }

    private companion object {
        val KEY_POPULARITY_DATE = stringPreferencesKey("popularity_date")
        val KEY_POPULARITY_VALUE = intPreferencesKey("popularity_value")
        val KEY_ANNOUNCE_READ = stringPreferencesKey("announce_read_time")
        val KEY_IMPORTANT_CONFIRMED = stringSetPreferencesKey("important_confirmed")
    }
}

/**
 * 公告数据源：gitee 上的 notice.json → 内存 → 磁盘缓存（filesDir）→ 空列表。
 * 抓取口径与 PC 端 announcement.rs 相同：失败不打断 UI，主页显示上一次的内容。
 */
object AnnouncementStore {

    private const val URL = "https://gitee.com/muliuawa/nexbox/raw/master/notice.json"
    private const val CACHE_FILE = "announcement_cache.json"

    /** 10 分钟内存缓存：主页每次进来都调 refresh，不必每次都真发请求 */
    private const val CACHE_MS = 10 * 60_000L

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val _items = MutableStateFlow<List<Announcement>>(emptyList())
    val items: StateFlow<List<Announcement>> = _items.asStateFlow()

    private val _loading = MutableStateFlow(false)
    val loading: StateFlow<Boolean> = _loading.asStateFlow()

    @Volatile
    private var lastFetchAt = 0L

    /** 进主页时调用；有内存缓存且未过期直接返回，强制刷新传 force */
    fun refresh(force: Boolean = false) {
        if (_loading.value) return
        val now = System.currentTimeMillis()
        if (!force && _items.value.isNotEmpty() && now - lastFetchAt < CACHE_MS) return

        scope.launch {
            _loading.value = true
            runCatching {
                val request = Request.Builder().url(URL).build()
                NetworkModule.client.newCall(request).execute().use { resp ->
                    val body = resp.body?.string().orEmpty()
                    if (!resp.isSuccessful || body.isBlank()) error("HTTP ${resp.code}")
                    body
                }
            }.onSuccess { raw ->
                val feed = runCatching { NetworkModule.json.decodeFromString<AnnouncementFeedDto>(raw) }.getOrNull()
                if (feed != null) {
                    _items.value = feed.announceList
                    lastFetchAt = System.currentTimeMillis()
                    // 原文落盘当离线缓存；写失败无所谓，下次联网再补
                    runCatching { cacheFile().writeText(raw) }
                }
            }.onFailure {
                // 断网兜底：内存还没有内容时读磁盘缓存
                if (_items.value.isEmpty()) loadCache()
            }
            _loading.value = false
        }
    }

    private fun loadCache() {
        val raw = runCatching { cacheFile().readText() }.getOrNull() ?: return
        val feed = runCatching { Json.decodeFromString<AnnouncementFeedDto>(raw) }.getOrNull() ?: return
        _items.value = feed.announceList
    }

    private fun cacheFile(): File = File(NetworkModule.appContext.filesDir, CACHE_FILE)
}
