package com.nexbox.app.data

import androidx.compose.runtime.Immutable
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.intOrNull
import java.net.URLEncoder
import kotlin.math.roundToInt

/**
 * PC 端接口数据模型。字段与 `NexBox_Android_原生远程端` 方案里的接口契约严格对齐，
 * 注意 `device_name` / `computer_name` 是蛇形命名，其余为 camelCase。
 */

/** 统一响应信封：成功 `{ok:true,data:{...}}`；失败 `{ok:false,error:{code,message}}` */
@Serializable
data class Envelope<T>(
    val ok: Boolean = false,
    val data: T? = null,
    val error: ApiErrorBody? = null,
)

@Serializable
data class ApiErrorBody(
    /** PC 端返回的是数字状态码，但保留 JsonElement 以容忍字符串形式 */
    val code: JsonElement? = null,
    val message: String? = null,
) {
    fun codeText(): String? = when (val c = code) {
        null -> null
        is JsonPrimitive -> c.contentOrNull
        else -> null
    }
}

/** GET /api/info —— 探活，不需要鉴权 */
@Immutable
@Serializable
data class InfoData(
    val online: Boolean = false,
    val service: String? = null,
    val version: String? = null,
    val controlEnabled: Boolean = false,
    val pairingActive: Boolean = false,
    val lanName: String? = null,
)

/** POST /api/pair */
@Serializable
data class PairRequest(
    val code: String,
    @SerialName("device_name") val deviceName: String,
    /** 手机端稳定身份，PC 端据此对同一台手机去重，避免重复已配对设备 */
    @SerialName("device_uid") val deviceUid: String = "",
)

@Serializable
data class PairData(
    val token: String,
    val deviceId: String,
)

// ───────── 请求配对（免配对码，PC 端人工批准） ─────────

/** POST /api/pair/request 请求体 */
@Serializable
data class PairRequestBody(
    @SerialName("device_name") val deviceName: String,
    /** 手机端稳定身份，PC 端据此对同一台手机去重，避免重复已配对设备 */
    @SerialName("device_uid") val deviceUid: String = "",
)

/** POST /api/pair/request 响应 */
@Serializable
data class PairRequestCreated(
    val requestId: String,
    val expiresAt: Long = 0,
)

/** GET /api/pair/request/:id 响应。status: pending | approved | denied | expired | unknown */
@Serializable
data class PairRequestStatusData(
    val status: String,
    val token: String? = null,
    val deviceId: String? = null,
)

/** GET /api/capabilities —— 控制页按此数据驱动渲染，不硬编码任何动作 */
@Serializable
data class CapabilitiesData(
    val groups: List<CapabilityGroup> = emptyList(),
)

@Serializable
data class CapabilityGroup(
    val id: String,
    val title: String,
    val queries: List<CapabilityQuery> = emptyList(),
    val actions: List<CapabilityAction> = emptyList(),
)

@Serializable
data class CapabilityQuery(
    val key: String,
    val title: String,
)

@Serializable
data class CapabilityAction(
    val key: String,
    val title: String,
    val needsConfirm: Boolean = false,
    val params: List<CapabilityParam> = emptyList(),
)

@Serializable
data class CapabilityParam(
    val name: String,
    /** string / bool / int / number 等，由 PC 端给出 */
    val type: String = "string",
    val required: Boolean = false,
    /** 有枚举值时渲染为下拉，而非自由输入 */
    @SerialName("enum") val enumValues: List<String>? = null,
)

/** POST /api/query */
@Serializable
data class QueryRequest(
    val key: String,
    val args: JsonObject = JsonObject(emptyMap()),
)

/** POST /api/action —— needsConfirm 的动作必须带 confirm=true */
@Serializable
data class ActionRequest(
    val key: String,
    val args: JsonObject = JsonObject(emptyMap()),
    val confirm: Boolean = false,
)

/**
 * WebSocket 帧。
 * 注意：PC 端把 `meta` 塞进了 `data` 内部（`{type:"stats", data:{...字段, meta:{...}}}`），
 * 不是同级字段，所以这里 `data` 用 JsonObject 收，meta 由 [WsFrame.statsMeta] 取。
 */
@Serializable
data class WsFrame(
    val type: String,
    val data: JsonObject? = null,
    val key: String? = null,
) {
    /** 从 data.meta 里取主机名与时间戳 */
    fun statsMeta(): WsMeta? = data?.get("meta")?.let { el ->
        runCatching { NetworkModule.json.decodeFromJsonElement(WsMeta.serializer(), el) }.getOrNull()
    }

    /** data 里除 meta 之外的真正硬件字段 */
    fun statsPayload(): Map<String, JsonElement> =
        data?.filterKeys { it != "meta" }.orEmpty()
}

@Serializable
data class WsMeta(
    @SerialName("computer_name") val computerName: String? = null,
    val ts: Long? = null,
)

/** UDP 发现应答（camelCase） */
@Immutable
@Serializable
data class DiscoveryReply(
    val service: String? = null,
    val name: String? = null,
    val version: String? = null,
    val ip: String? = null,
    val tcpPort: Int = 0,
    val controlEnabled: Boolean = false,
    val monitorEnabled: Boolean = false,
)

/** 已配对会话，持久化在 DataStore */
@Immutable
@Serializable
data class Session(
    val host: String,
    val port: Int,
    val token: String,
    val deviceId: String,
    val deviceName: String,
) {
    val baseUrl: String get() = "http://$host:$port"
    // 令牌是 base64，可能含 '+' '/' '='；放进 URL query 必须百分号编码。
    // 否则 PC 端按表单规则把 '+' 解成空格 → 令牌哈希不匹配 → WS 401，
    // 表现为「已配对，正在建立实时数据通道…」一直连不上、要反复重新配对。
    val wsUrl: String get() = "ws://$host:$port/api/ws?token=${URLEncoder.encode(token, "UTF-8")}"
    val display: String get() = "$host:$port"
}

// ───────────────────────── 硬件面板 ─────────────────────────

private fun JsonElement?.asDoubleOrNull(): Double? = (this as? JsonPrimitive)?.doubleOrNull
private fun JsonElement?.asIntOrNull(): Int? = (this as? JsonPrimitive)?.intOrNull

/**
 * WS `stats` 帧解析出的硬件快照。
 *
 * PC 端推的是 `OverlayHardwareData`（snake_case）。任何字段都可能是 null ——
 * 取决于机器上有没有对应传感器（比如没有独显就没有 gpu_*），所以这里全部可空，
 * UI 侧统一按「—」降级，不要用 0 冒充。
 *
 * @Immutable：字段全为 val 且从不原地改，用 copy 换新值；
 * 不标注的话里面的 List 会被 Compose 判成 unstable，硬件面板每秒刷新会拖着整页一起重组。
 */
@Immutable
data class HardwareSnapshot(
    /** CPU 占用百分比 0-100 */
    val cpuUsage: Int? = null,
    /** CPU 温度 °C */
    val cpuTemp: Double? = null,
    /** CPU 封装功耗 W */
    val cpuPower: Double? = null,
    /** CPU 频率 MHz */
    val cpuClock: Int? = null,
    /** CPU 电压 V */
    val cpuVoltage: Double? = null,
    /** CPU 风扇转速 RPM */
    val cpuFanSpeed: Int? = null,
    /** GPU 占用百分比 0-100 */
    val gpuUsage: Int? = null,
    /** GPU 温度 °C */
    val gpuTemp: Double? = null,
    /** GPU 功耗 W */
    val gpuPower: Int? = null,
    /** GPU 核心频率 MHz */
    val gpuClock: Int? = null,
    /** GPU 显存频率 MHz */
    val gpuMemoryClock: Int? = null,
    /** GPU 电压 V */
    val gpuVoltage: Double? = null,
    /** GPU 风扇转速 RPM */
    val gpuFanSpeed: Int? = null,
    /** 显存已用 / 总量（MB） */
    val gpuVramUsed: Int? = null,
    val gpuVramTotal: Int? = null,
    /** 多显卡机器上每块 GPU 的传感器明细 */
    val gpuSensors: List<GpuSensor> = emptyList(),
    /** 当前选中的 GPU 索引（对应 [gpuSensors]） */
    val activeGpuIndex: Int = 0,
    /** 内存占用百分比 0-100（来自 LHML Memory Load 传感器） */
    val memoryUsage: Double? = null,
    /** SSD 温度 °C */
    val ssdTemp: Double? = null,
    /** 实时下载 / 上传速率 KB/s */
    val netDown: Double? = null,
    val netUp: Double? = null,
) {
    companion object {
        fun from(payload: Map<String, JsonElement>): HardwareSnapshot = HardwareSnapshot(
            cpuUsage = payload["cpu_usage"].asIntOrNull(),
            cpuTemp = payload["cpu_temp"].asDoubleOrNull(),
            cpuPower = payload["cpu_power"].asDoubleOrNull(),
            cpuClock = payload["cpu_clock"].asIntOrNull(),
            cpuVoltage = payload["cpu_voltage"].asDoubleOrNull(),
            cpuFanSpeed = payload["cpu_fan_speed"].asIntOrNull(),
            gpuUsage = payload["gpu_usage"].asIntOrNull(),
            gpuTemp = payload["gpu_temp"].asDoubleOrNull(),
            gpuPower = payload["gpu_power"].asIntOrNull(),
            gpuClock = payload["gpu_clock"].asIntOrNull(),
            gpuMemoryClock = payload["gpu_memory_clock"].asIntOrNull(),
            gpuVoltage = payload["gpu_voltage"].asDoubleOrNull(),
            gpuFanSpeed = payload["gpu_fan_speed"].asIntOrNull(),
            gpuVramUsed = payload["gpu_vram_used"].asIntOrNull(),
            gpuVramTotal = payload["gpu_vram_total"].asIntOrNull(),
            gpuSensors = payload["gpu_sensors"].asGpuSensorList(),
            activeGpuIndex = payload["active_gpu_index"].asIntOrNull() ?: 0,
            memoryUsage = payload["memory_usage"].asDoubleOrNull(),
            ssdTemp = payload["ssd_temp"].asDoubleOrNull(),
            netDown = payload["net_down_speed"].asDoubleOrNull(),
            netUp = payload["net_up_speed"].asDoubleOrNull(),
        )
    }
}

/**
 * 单块 GPU 的传感器明细（WS `stats` 帧里的 `gpu_sensors` 数组元素）。
 * 与 [HardwareSnapshot] 同理：任何传感器都可能缺失，字段一律可空，UI 显示「—」。
 */
@Immutable
@Serializable
data class GpuSensor(
    val name: String = "",
    @SerialName("hardware_type") val hardwareType: String? = null,
    /** 温度 °C */
    val temperature: Double? = null,
    /** 占用百分比 0-100 */
    val usage: Int? = null,
    /** 风扇转速 RPM */
    @SerialName("fan_speed") val fanSpeed: Int? = null,
    /** 功耗 W */
    val power: Int? = null,
    /** 核心频率 MHz */
    val clock: Int? = null,
    /** 显存频率 MHz */
    @SerialName("memory_clock") val memoryClock: Int? = null,
    /** 显存已用 / 总量（MB） */
    @SerialName("vram_used") val vramUsed: Int? = null,
    @SerialName("vram_total") val vramTotal: Int? = null,
    /** 电压 V */
    val voltage: Double? = null,
)

/** `gpu_sensors` 是对象数组，逐个解；单条解析失败只丢那一条，不拖垮整帧 */
private fun JsonElement?.asGpuSensorList(): List<GpuSensor> {
    val arr = this as? JsonArray ?: return emptyList()
    return arr.mapNotNull { el ->
        runCatching { NetworkModule.json.decodeFromJsonElement(GpuSensor.serializer(), el) }.getOrNull()
    }
}

/**
 * `mem.optimize`（一键内存优化）的返回：优化前后内存对比与释放量。
 * 字段来自 PC 端 `optimization::OptimizationResult`，单位 MB。
 */
@Immutable
@Serializable
data class MemOptimizeResult(
    val success: Boolean = true,
    val message: String? = null,
    @SerialName("freed_mb") val freedMb: Long = 0,
)

/**
 * `hw.info` —— CPU / GPU 的静态型号信息（名称、驱动、显存类型等）。
 * 只在进详情页时拉一次，不轮询；PC 端版本较旧（404）时整块型号信息降级不显示。
 * 字段一律可空/空串，读不到就交给 UI 显示「—」。
 */
@Immutable
@Serializable
data class HardwareIdentity(
    val cpu: CpuIdentity? = null,
    val gpus: List<GpuIdentity> = emptyList(),
)

@Immutable
@Serializable
data class CpuIdentity(
    val name: String = "",
    val manufacturer: String = "",
    /** 核心 / 线程数 */
    val cores: Int? = null,
    val threads: Int? = null,
    /** 最大频率 MHz */
    @SerialName("max_clock_speed") val maxClockMhz: Int? = null,
    val socket: String = "",
    /** L3 缓存 KB */
    @SerialName("l3_cache_size") val l3CacheKb: Int? = null,
)

@Immutable
@Serializable
data class GpuIdentity(
    val name: String = "",
    /** NVIDIA / AMD / Intel / Unknown */
    val vendor: String = "",
    /** 显存总量 GB（核显读不到时为 null） */
    @SerialName("memory_gb") val memoryGb: Double? = null,
    @SerialName("driver_version") val driverVersion: String = "",
    @SerialName("driver_date") val driverDate: String = "",
    /** 显存类型，如 GDDR6X */
    @SerialName("video_memory_type") val videoMemoryType: String? = null,
    @SerialName("resolution_width") val resolutionWidth: Int? = null,
    @SerialName("resolution_height") val resolutionHeight: Int? = null,
    /** 刷新率 Hz */
    @SerialName("refresh_rate") val refreshRate: Int? = null,
)

/**
 * `mem.status` —— 比 WS 里的 memory_usage 多了总量/已用，用来把卡片写具体。
 *
 * **单位是 MB，不是字节**：PC 端 `optimization::get_memory_info()` 里已经先 `/1024/1024`，
 * 按字节换算会把 32 GB 内存算成 `0.0 GB`。[toGb] 顺带兼容直接给字节的形式。
 * 字段一律可空，读不到就交给 UI 显示「—」，不要用 0 冒充。
 */
@Immutable
@Serializable
data class MemoryStatus(
    val total: Long? = null,
    val available: Long? = null,
    val used: Long? = null,
    @SerialName("usage_percent") val usagePercent: Double? = null,
) {
    val totalGb: Double? get() = total.toGb()

    /** PC 端某些版本不给 used，用 总量 - 可用 反算 */
    val usedGb: Double?
        get() = (used ?: total?.let { t -> available?.let { a -> t - a } }).toGb()

    /** 占用百分比：优先 usage_percent，缺失时用 已用/总量 反算 */
    val percent: Int?
        get() = usagePercent?.let { it.roundToInt() }
            ?: run {
                val u = usedGb
                val t = totalGb
                if (u != null && t != null && t > 0.0) ((u / t) * 100).roundToInt() else null
            }
}

/**
 * 1 GiB。
 * 注意不要写成 `MemoryStatus` 的 private companion ——
 * kotlinx.serialization 会自己生成一个 `Companion`，手写的私有伴生对象会把它挡掉，
 * 导致 `MemoryStatus.serializer()` 编译不过。
 */
private const val GIB = 1024.0 * 1024.0 * 1024.0

/**
 * 字节量级的下限：整机内存最少也有 1 GB，换成字节就是 1.07e9；
 * 而 MB 量级算到 1 TB 内存才 1e6，两者用 1e9 一分就开，不会误判。
 */
private const val BYTE_LIKE_THRESHOLD = 1_000_000_000L

/** MB → GB（PC 端回的就是 MB，商就是 1024，不是 1024²） */
private const val MB_PER_GB = 1024.0

private fun Long?.toGb(): Double? {
    val v = this ?: return null
    if (v <= 0L) return null
    return if (v >= BYTE_LIKE_THRESHOLD) v / GIB else v / MB_PER_GB
}

/**
 * `disk.status` —— 所有磁盘的合计占用。
 * 该查询是后加的：PC 端版本较旧时会返回「unknown query」，
 * 上层据此把存储卡片降级为「—」并给一句升级提示。
 */
@Immutable
@Serializable
data class DiskStatus(
    val name: String = "",
    @SerialName("total_gb") val totalGb: Double = 0.0,
    @SerialName("available_gb") val availableGb: Double = 0.0,
    @SerialName("used_gb") val usedGb: Double = 0.0,
    @SerialName("usage_percent") val usagePercent: Double = 0.0,
)

/** `gamemode.status` */
@Immutable
@Serializable
data class GameModeStatus(
    /** 用户选定的档位：default / regular / competitive */
    val preset: String = "default",
    /** 当前实际生效的档位 */
    @SerialName("effective_preset") val effectivePreset: String = "default",
    @SerialName("manual_enabled") val manualEnabled: Boolean = false,
    @SerialName("auto_enabled") val autoEnabled: Boolean = false,
    /** 是否正在压制后台进程 */
    val active: Boolean = false,
    @SerialName("suppressed_count") val suppressedCount: Int = 0,
    @SerialName("game_running") val gameRunning: Boolean = false,
) {
    val presetLabel: String get() = label(preset)
    val effectiveLabel: String get() = label(effectivePreset)

    /**
     * 开关语义：与 PC 端顶栏一致，非 default 档即「开」（开 = 常规）。
     * 只看 [preset] 不看 effectivePreset —— effectivePreset 要等 PC 端下一轮扫描才翻页，
     * 拿它当 checked 会让刚点下去的开关弹回去。
     */
    val isOn: Boolean get() = preset != "default"

    private fun label(v: String): String = when (v) {
        "regular" -> "常规"
        "competitive" -> "竞技"
        else -> "关闭"
    }
}

/** `filter.settings` —— PC 端显示滤镜的权威状态来源 */
@Immutable
@Serializable
data class FilterSettings(
    /** 滤镜当前是否开启 */
    @SerialName("is_active") val isActive: Boolean = false,
    val temperature: Int = 0,
    val brightness: Int = 0,
    val contrast: Int = 0,
    val saturation: Int = 0,
    val mode: Int = 0,
) {
    /** 开关语义：filter.enable / filter.disable 控制的就是 [isActive] */
    val isOn: Boolean get() = isActive
}

/**
 * `filter.enable` / `filter.disable` 的返回。
 *
 * 有两个字段不能当摆设：
 * - [skippedStale] 为 true 表示「请求被处理了，但这次操作没真正发生」，
 *   **不能据此显示成已开/已关**，必须回读 [settings] 纠正；
 * - [degraded] 为 true 表示滤镜是以兼容模式恢复的，未必还原了原来的校色。
 */
@Immutable
@Serializable
data class FilterActionResult(
    val success: Boolean = false,
    val message: String = "",
    val degraded: Boolean = false,
    @SerialName("skipped_stale") val skippedStale: Boolean = false,
    /** 返回体里带的最新设置，含权威的 is_active */
    val settings: FilterSettings? = null,
)

/** `crosshair.status` —— PC 端准心开关 */
@Immutable
@Serializable
data class CrosshairStatus(val enabled: Boolean = false)

/** `overlay.status` —— PC 端悬浮框开关 */
@Immutable
@Serializable
data class OverlayStatus(val active: Boolean = false)

// ───────────────────────── 文件互传（文件篮） ─────────────────────────

/**
 * 单个互传文件条目（PC 端 `transfer::TransferFile`）。
 * 文件篮模式：添加方把文件放进列表，接收方手动取 —— 本机视角里
 * [direction] 为 `to_device` 的是 PC 发来的待接收文件，`from_device` 是本机已发送的。
 */
@Immutable
@Serializable
data class TransferFile(
    val id: String,
    val name: String,
    val size: Long = 0,
    val direction: String = "",
    @SerialName("created_at") val createdAt: Long = 0,
    /** 接收方已确认（本机下载完成回执 / PC 端已另存为） */
    val acked: Boolean = false,
    /** PC 端「另存为」后的位置（仅 from_device 有意义） */
    @SerialName("saved_path") val savedPath: String? = null,
)

/** GET /api/transfer/files —— incoming=PC 发来的待接收，outgoing=本机已发送 */
@Immutable
@Serializable
data class TransferLists(
    val incoming: List<TransferFile> = emptyList(),
    val outgoing: List<TransferFile> = emptyList(),
)

/** POST /api/transfer/files/:id/progress —— 下载进度上报体 */
@Serializable
data class TransferProgress(
    val done: Long,
    val total: Long,
    val speed: Double,
)
