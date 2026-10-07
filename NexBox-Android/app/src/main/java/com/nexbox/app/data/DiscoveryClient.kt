package com.nexbox.app.data

import android.content.Context
import android.net.wifi.WifiManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.NetworkInterface
import java.net.SocketTimeoutException

/**
 * 局域网设备发现。与 PC 端 `remote_access/discovery.rs` 严格对齐：
 * - 固定 UDP 端口 [DISCOVERY_PORT] = 45689
 * - 探测包需包含 magic 串 [PROBE] = "nexbox.discover"
 * - PC 端单播回应 `{service,name,version,ip,tcpPort,controlEnabled,monitorEnabled}`
 *
 * 部分路由器/系统会丢弃广播，需持 [WifiManager.MulticastLock]；
 * 扫描失败不影响主流程，用户仍可扫码或手输 IP。
 */
class DiscoveryClient {

    /**
     * 扫描并**渐进式**返回结果：每发现一台新设备就发一次列表，
     * 界面可以边扫边显示，不用等超时结束。
     */
    fun scan(timeoutMs: Long = 2_500): Flow<List<DiscoveryReply>> = flow {
        val socket = DatagramSocket().apply {
            broadcast = true
            soTimeout = 250
            reuseAddress = true
        }
        val lock = acquireMulticastLock()
        try {
            val probe = PROBE.toByteArray()
            broadcastTargets().forEach { target ->
                runCatching {
                    socket.send(DatagramPacket(probe, probe.size, target, DISCOVERY_PORT))
                }
            }

            val found = LinkedHashMap<String, DiscoveryReply>()
            val deadline = System.currentTimeMillis() + timeoutMs
            val buffer = ByteArray(2048)

            while (System.currentTimeMillis() < deadline) {
                val packet = DatagramPacket(buffer, buffer.size)
                try {
                    socket.receive(packet)
                } catch (_: SocketTimeoutException) {
                    continue
                }
                val text = String(packet.data, 0, packet.length)
                val reply = runCatching {
                    NetworkModule.json.decodeFromString(DiscoveryReply.serializer(), text)
                }.getOrNull() ?: continue
                if (reply.service != "nexbox") continue

                // 同一台机器可能从多个网卡回应，用 ip:tcpPort 去重
                val key = "${reply.ip}:${reply.tcpPort}"
                if (found.put(key, reply) == null) emit(found.values.toList())
            }
        } finally {
            runCatching { socket.close() }
            runCatching { lock?.release() }
        }
    }.flowOn(Dispatchers.IO)

    /** 枚举各网卡的子网广播地址；只发 255.255.255.255 在部分路由器上不可达 */
    private fun broadcastTargets(): List<InetAddress> {
        val targets = mutableListOf<InetAddress>()
        runCatching {
            NetworkInterface.getNetworkInterfaces().toList().forEach { nif ->
                if (!nif.isUp || nif.isLoopback) return@forEach
                nif.interfaceAddresses.forEach { addr ->
                    addr.broadcast?.let { targets += it }
                }
            }
        }
        targets += InetAddress.getByName("255.255.255.255")
        return targets.distinct()
    }

    private fun acquireMulticastLock(): WifiManager.MulticastLock? {
        val wifi = NetworkModule.appContext
            .getSystemService(Context.WIFI_SERVICE) as? WifiManager ?: return null
        return runCatching {
            wifi.createMulticastLock("nexbox-discovery").apply {
                setReferenceCounted(false)
                acquire()
            }
        }.getOrNull()
    }

    companion object {
        const val DISCOVERY_PORT = 45689
        const val PROBE = "nexbox.discover"
    }
}
