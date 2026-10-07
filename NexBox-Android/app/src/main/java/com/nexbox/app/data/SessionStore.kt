package com.nexbox.app.data

import android.content.Context
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import java.util.UUID

private val Context.sessionDataStore: DataStore<Preferences> by preferencesDataStore(name = "nexbox_session")

/**
 * 会话持久化：host / port / token / deviceId / deviceName。
 * 令牌只落在应用私有目录，不写日志；解绑或 401 时立即清除。
 */
class SessionStore(context: Context) {

    private val app = context.applicationContext
    private val store = app.sessionDataStore

    val session: Flow<Session?> = store.data.map { p ->
        val host = p[KEY_HOST]
        val token = p[KEY_TOKEN]
        if (host.isNullOrBlank() || token.isNullOrBlank()) {
            null
        } else {
            Session(
                host = host,
                port = p[KEY_PORT] ?: DEFAULT_PORT,
                token = token,
                deviceId = p[KEY_DEVICE_ID].orEmpty(),
                deviceName = p[KEY_DEVICE_NAME].orEmpty(),
            )
        }
    }

    suspend fun current(): Session? = session.first()

    /**
     * 稳定设备身份：配对时随 device_uid 上报，PC 端据此识别同一台手机做去重，避免重复已配对设备。
     *
     * 优先用 ANDROID_ID：同一签名密钥下它跨卸载重装不变，而 DataStore 会随卸载被清空。
     * 只用安装 UUID 时，每重装一次包 PC 端就多出一条重复的已配对设备。
     */
    suspend fun installId(): String {
        androidId()?.let { return it }
        store.data.first()[KEY_INSTALL_UUID]?.let { return it }
        val fresh = UUID.randomUUID().toString()
        store.edit { it[KEY_INSTALL_UUID] = fresh }
        return fresh
    }

    /** 个别老机型/ROM 上 ANDROID_ID 会固定返回同一个坏值，那种情况退回安装 UUID。 */
    private fun androidId(): String? = runCatching {
        Settings.Secure.getString(app.contentResolver, Settings.Secure.ANDROID_ID)
    }
        .getOrNull()
        ?.trim()
        ?.takeUnless { it.isEmpty() || it == ANDROID_ID_BUG_VALUE }

    suspend fun save(value: Session) {
        store.edit { p ->
            p[KEY_HOST] = value.host
            p[KEY_PORT] = value.port
            p[KEY_TOKEN] = value.token
            p[KEY_DEVICE_ID] = value.deviceId
            p[KEY_DEVICE_NAME] = value.deviceName
        }
    }

    suspend fun clear() {
        // 解绑只清会话，install_uuid 必须跨解绑保留，否则设备身份会变、PC 端又会新建一条
        val keep = store.data.first()[KEY_INSTALL_UUID]
        store.edit { p ->
            p.clear()
            keep?.let { p[KEY_INSTALL_UUID] = it }
        }
    }

    companion object {
        /** PC 端 HTTP 端口是随机的，这里只是手动输入框的占位初值 */
        const val DEFAULT_PORT = 8080

        /** 部分机型上所有应用读到同一个值，拿它当身份会把多台手机判成一台 */
        private const val ANDROID_ID_BUG_VALUE = "9774d56d682e549c"

        private val KEY_HOST = stringPreferencesKey("host")
        private val KEY_PORT = intPreferencesKey("port")
        private val KEY_TOKEN = stringPreferencesKey("token")
        private val KEY_DEVICE_ID = stringPreferencesKey("device_id")
        private val KEY_DEVICE_NAME = stringPreferencesKey("device_name")
        private val KEY_INSTALL_UUID = stringPreferencesKey("install_uuid")
    }
}
