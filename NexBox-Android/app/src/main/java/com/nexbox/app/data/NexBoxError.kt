package com.nexbox.app.data

import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException

/** PC 端 HTTP 状态语义 → 用户可读中文文案 */
class NexBoxException(
    val status: Int,
    val apiCode: String? = null,
    override val message: String,
) : Exception(message) {

    /** 令牌失效：调用方应清会话并退回连接页 */
    val isUnauthorized: Boolean get() = status == 401
}

fun httpErrorMessage(status: Int, fallback: String? = null): String = when (status) {
    401 -> "登录状态已失效，请重新配对"
    403 -> "配对码错误或已过期，请在 PC 端刷新后重试"
    404 -> "没有找到新境盒 PC 端，请确认已开启远程控制"
    408 -> "连接超时，请确认手机与 PC 在同一 Wi-Fi"
    428 -> "该操作需要二次确认"
    429 -> "尝试过于频繁已被临时锁定，请稍后再试"
    500 -> fallback ?: "PC 端执行失败"
    else -> fallback ?: "请求失败（HTTP $status）"
}

/** 把网络层异常收敛成一句能给用户看的中文 */
fun Throwable.toUserMessage(): String = when (this) {
    is NexBoxException -> message
    is SocketTimeoutException -> "连接超时，请确认手机与 PC 在同一 Wi-Fi"
    is UnknownHostException -> "无法解析该地址，请检查 IP 是否正确"
    is ConnectException -> "无法连接到该地址，请确认 PC 端已开启远程控制"
    is IOException -> "网络异常：${message ?: "连接中断"}"
    else -> message ?: "未知错误"
}
