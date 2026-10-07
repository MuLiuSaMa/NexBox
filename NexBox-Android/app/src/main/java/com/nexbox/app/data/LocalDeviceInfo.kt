package com.nexbox.app.data

import androidx.compose.runtime.Immutable

/**
 * 本机静态规格：进配置页扫一次就缓存，不参与轮询。
 *
 * 字段一律「已格式化的字符串 + 可空」：单位换算放在读取侧做好，UI 只管显示；
 * 读不到的项给 null，让 [com.nexbox.app.ui.screen.SpecRow] 把整行隐掉。
 * 用 0 或空串冒充会让「这台机器确实没有」和「这次没读到」长得一样，看不出区别。
 */
@Immutable
data class DeviceSpec(
    /** 产品市场名（如「小米 14」），拿不到时退回 Build.MODEL */
    val marketName: String? = null,
    val brand: String? = null,
    val manufacturer: String? = null,
    val model: String? = null,
    /** 设备代号 / 产品名 / 主板，去重后拼一行 */
    val deviceCode: String? = null,

    val androidRelease: String? = null,
    val sdkInt: Int = 0,
    val securityPatch: String? = null,
    /** Build.DISPLAY，厂商的固件版本号 */
    val buildDisplay: String? = null,
    /** 定制系统：HyperOS / MIUI / OriginOS / EMUI / ColorOS …，识别不出为 null */
    val romLabel: String? = null,
    val kernelVersion: String? = null,
    val is64Bit: Boolean = false,
    /** 指令集，如 "arm64-v8a / armeabi-v7a" */
    val abis: String? = null,

    val socVendor: String? = null,
    /** 原始代号，如 SM7435-AB */
    val socModel: String? = null,
    /** 商用名，如「骁龙 7s Gen 2」；[SocNames] 认不出时为 null，UI 退回显示原始代号 */
    val socMarketingName: String? = null,
    /** 核心总数（含离线核心） */
    val cpuCores: Int = 0,
    val glEsVersion: String? = null,
    val vulkanSupported: Boolean = false,
    /** GPU 驱动族：Adreno / Mali / PowerVR …，读不到就隐藏 */
    val gpuDriver: String? = null,
    /** GPU 精确型号（GL_RENDERER 查得，如「Adreno 810」），读不到就退回驱动族 */
    val gpuName: String? = null,

    val ramTotal: String? = null,
    /** 标称机身容量（如「128 GB」）：块设备物理容量/各文件系统求和吸附档位，都读不到才退回数据分区 */
    val storageTotal: String? = null,
    /** 物理分辨率（不是窗口尺寸） */
    val screenResolution: String? = null,
    /** 屏幕对角线，按面板真实 dpi 折算（如「6.4 英寸」） */
    val screenInches: String? = null,
    /**
     * 面板最高支持刷新率。不能拿当前生效模式：智能刷新率/省电会把它压到 60、90，
     * 实测就有 144Hz 屏显示 120、120Hz 屏显示 60 的反馈。规格页只报硬件上限。
     */
    val maxRefreshRate: String? = null,
    val screenDpi: Int? = null,
    val hdrSupported: Boolean = false,
    /** 只装「支持」的特性，UI 拼成一行 */
    val supportedFeatures: List<String> = emptyList(),
)

/**
 * 本机会自然变化的少量运行值：配置页按 1 秒轮询刷新。
 *
 * 温度与占用走 sysfs。以前这里写着「Android 10+ 用 SELinux 挡住了普通应用的
 * `/sys/.../cpufreq`、`thermal_zone` 节点，读了也是空」，实测不成立 —— untrusted_app 域
 * 在本机（HyperOS 3 / 骁龙 7s Gen 2）能读 thermal zone 的 type 与 temp、
 * `cpufreq/{scaling_cur_freq,cpuinfo_max_freq}`、`cpuN/cpuidle/stateN/time` 和 `cpu/online`，
 * 所以这两项收了。但**能不能读每台机器都不一样**（取决于 kernel 与厂商策略），
 * 每个字段各自降级 null 让整行消失，别写死「一定能读到」。
 *
 * 三条踩过的坑记在这儿：
 * - 断开的温度区回 `-273000`，策略伪 zone 回 `0`，两个都要滤掉；
 * - 占用**不能**用 `time_in_state`：本机 kernel 只在换档时结算，八线程压满 4 秒总驻留
 *   只涨 3344ms（空载 3313ms），占用率会永远卡在 10%。改用 cpuidle 反推；
 * - `cpuidle/stateN/time` 的单位内核文档写毫秒、本机实际是微秒，按累计量对开机时长的
 *   倍数判一次并锁存。
 *
 * 仍然拿不到的：GPU 占用（kgsl 与 devfreq 节点连 shell 都被拒，得 root）、
 * 以及 power_supply 目录下的 sysfs（对应用拒绝，所以功率只能走框架 API 拿电池端整机值）。
 */
@Immutable
data class DeviceRuntime(
    /** 当前在线核心数（省电模式会关核，所以和 [DeviceSpec.cpuCores] 可能不等） */
    val coresOnline: Int? = null,
    val ramAvailable: String? = null,
    val storageFree: String? = null,
    /** 已用存储 = 标称总量 - 剩余（系统/固件占用计入已用），十进制 GB 口径，已用 + 剩余 = 总量严格成立 */
    val storageUsed: String? = null,
    /** 存储已用百分比 = (总量 - 剩余) / 总量，给底部存储卡的进度条用 */
    val storageUsedPercent: Int? = null,
    val batteryPercent: Int? = null,
    val chargingLabel: String? = null,
    val batteryVoltage: String? = null,

    /** CPU 温度：所有 cpu/cpuss 温度区里的最高值。取最高不取平均 —— 一个热点不该被七个冷核抹平 */
    val cpuTempC: Double? = null,
    val gpuTempC: Double? = null,
    /** CPU 占用：1 −（各核 cpuidle 空闲驻留之和 ÷ 在线核数 × 墙钟差）。首轮采样必为 null */
    val cpuUsagePercent: Int? = null,
    /** 预留：需要 root 才能读到，绝大多数机器上是 null，字段留着以便换机后直接可用 */
    val gpuUsagePercent: Int? = null,
    /** 最高频 cluster 的当前频率 */
    val cpuClock: String? = null,
    /** 电池端**整机**功率，不是 CPU/GPU 分项；充电中不显示（那时是充电输入而不是负载） */
    val batteryPower: String? = null,
    /** true 表示这个瓦数来自库仑计差值估算，UI 要加「约」字 */
    val batteryPowerEstimated: Boolean = false,
    val batteryTempC: Double? = null,
)
