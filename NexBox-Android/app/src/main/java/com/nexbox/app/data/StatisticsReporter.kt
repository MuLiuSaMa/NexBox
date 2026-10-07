package com.nexbox.app.data

import android.content.Context
import android.util.Log
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import com.nexbox.app.BuildConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

private val Context.statisticsDataStore: DataStore<Preferences> by preferencesDataStore(name = "nexbox_statistics")

/** 上报体：与 PC 端 sys_info.rs 完全同构（version 前缀 PC 为 box-、安卓为 boxad-） */
@Serializable
private data class StatisticsDto(val version: String, val os: String)

/**
 * 首次启动统计：与 PC 端 sys_info.rs 同口径，启动后上报一次到 mc.sjtu.cn。
 * 标志落在独立 DataStore（对应 PC 的 statistics-sent.json）；
 * 与 PC 一致：无论请求成败都写标志，每台机器只尝试一次，失败静默不重试。
 */
object StatisticsReporter {

    private const val TAG = "Statistics"
    private const val URL = "https://mc.sjtu.cn/api-sjmcl/statistics"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    private val KEY_SENT = booleanPreferencesKey("statistics_sent")

    /** Application.onCreate 调用；后台协程执行，不阻塞启动 */
    fun maybeReport(context: Context) {
        val app = context.applicationContext
        scope.launch {
            val alreadySent = runCatching {
                app.statisticsDataStore.data.first()[KEY_SENT] == true
            }.getOrDefault(false)
            if (alreadySent) {
                Log.i(TAG, "Statistics already sent, skipping")
                return@launch
            }

            val version = "boxad-${BuildConfig.VERSION_NAME}"
            Log.i(TAG, "Sending statistics: version=$version, os=android")
            runCatching {
                val body = NetworkModule.json
                    .encodeToString(StatisticsDto(version = version, os = "android"))
                    .toRequestBody("application/json".toMediaType())
                val request = Request.Builder().url(URL).post(body).build()
                NetworkModule.client.newCall(request).execute().use { resp ->
                    Log.i(TAG, "Statistics sent, status: ${resp.code}")
                }
            }.onFailure {
                Log.e(TAG, "Failed to send statistics: ${it.message}")
            }

            runCatching { app.statisticsDataStore.edit { it[KEY_SENT] = true } }
                .onFailure { Log.e(TAG, "Failed to write statistics flag: ${it.message}") }
        }
    }
}
