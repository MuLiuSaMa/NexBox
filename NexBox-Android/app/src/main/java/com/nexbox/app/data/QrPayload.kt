package com.nexbox.app.data

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull

/**
 * PC 端二维码载荷。PC 端 `remote-access-dialog.tsx` 生成的是：
 * `{"v":1,"ip":"192.168.x.x","port":54321,"code":"123456"}`
 * —— 注意 `port` 是数字、`code` 是字符串。
 */
@Serializable
data class QrPairing(
    val v: Int = 1,
    val ip: String,
    val port: Int,
    val code: String,
)

object QrPayload {

    private val IPV4 = Regex("""(\d{1,3}(?:\.\d{1,3}){3})""")
    private val SIX_DIGITS = Regex("""(?<!\d)(\d{6})(?!\d)""")
    private val PORT = Regex("""(?<!\d)(\d{2,5})(?!\d)""")

    /** 解析二维码文本；识别不了返回 null，由调用方给提示 */
    fun parse(raw: String): QrPairing? {
        val text = raw.trim()
        if (text.isEmpty()) return null
        return fromJson(text) ?: fromLoose(text)
    }

    private fun fromJson(text: String): QrPairing? {
        val obj = runCatching {
            NetworkModule.json.parseToJsonElement(text) as? JsonObject
        }.getOrNull() ?: return null

        val ip = (obj["ip"] as? JsonPrimitive)?.contentOrNull?.trim()
        val port = (obj["port"] as? JsonPrimitive)?.intOrNull
        val code = (obj["code"] as? JsonPrimitive)?.contentOrNull?.trim()
        val version = (obj["v"] as? JsonPrimitive)?.intOrNull ?: 1

        if (ip.isNullOrBlank() || port == null || port !in 1..65535 || code.isNullOrBlank()) return null
        return QrPairing(version, ip, port, code)
    }

    /**
     * 兜底：兼容手工生成或其它工具产出的二维码，
     * 只要文本里同时出现 IPv4、端口、6 位配对码就认。
     */
    private fun fromLoose(text: String): QrPairing? {
        val ip = IPV4.find(text)?.groupValues?.get(1) ?: return null
        val rest = text.replace(ip, " ")
        val code = SIX_DIGITS.find(rest)?.groupValues?.get(1) ?: return null
        val port = PORT.find(rest)?.groupValues?.get(1)?.toIntOrNull()?.takeIf { it in 1..65535 } ?: return null
        return QrPairing(1, ip, port, code)
    }
}
