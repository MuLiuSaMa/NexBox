package com.nexbox.app.data

import android.annotation.SuppressLint
import android.app.ActivityManager
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.hardware.display.DisplayManager
import android.os.BatteryManager
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.os.SystemClock
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLDisplay
import android.opengl.GLES20
import android.system.Os
import android.util.DisplayMetrics
import android.view.Display
import androidx.core.content.ContextCompat
import java.io.File
import java.util.Locale
import kotlin.math.roundToInt
import kotlin.math.abs
import kotlin.math.sqrt
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * 读本机（手机自己）的配置信息。全部走免权限的系统 API，不联网、不申请任何权限。
 *
 * 分两个入口：
 * - [readSpec]：静态规格，进页扫一次即可，字段不会自己变；
 * - [readRuntime]：会变化的少量运行值，配置页按 1 秒轮询。
 *
 * 每个取值各自 `runCatching`：某个 sysfs 节点被 SELinux 挡了只该让那一行消失，
 * 不该把整页拖成空白。
 */
object LocalDeviceReader {

    private const val CPU_ROOT = "/sys/devices/system/cpu"
    private const val CPU_ONLINE = "$CPU_ROOT/online"
    private const val GIB = 1024.0 * 1024.0 * 1024.0

    /** 存储卡片的口径基准：十进制 GB，与标称容量同基，已用 + 剩余才能与总量对账（RAM 仍用 GIB） */
    private const val GB = 1000.0 * 1000.0 * 1000.0

    /** 常见标称容量档位（十进制 GB）：物理容量贴近哪一档就报哪一档 */
    private val MARKETING_GB = intArrayOf(8, 16, 32, 64, 128, 256, 512, 1024, 2048)

    /**
     * 可能是真闪存上文件系统的挂载点。/storage、/apex、/dev 这类 fuse/内存/派生
     * 挂载一概不进候选，否则会把 /data、/system 重复计入。
     */
    private val MOUNT_CANDIDATES = listOf(
        "/data", "/system", "/system_ext", "/vendor", "/product", "/odm", "/oem",
        "/metadata", "/cache", "/firmware", "/bt_firmware", "/efs", "/persist", "/mnt/vendor/persist",
    )

    /** 超宽带特性名：对应常量 FEATURE_UWB 要 API 34，这里直接写特性名字符串，低版本只会读到 false */
    private const val FEATURE_UWB_NAME = "android.hardware.uwb"

    private val CPU_DIR = Regex("^cpu\\d+$")

    private const val THERMAL_ROOT = "/sys/class/thermal"

    /** 超过这个间隔说明中间根本没在采（切走页面 / 长时间后台），占用率的窗口不可信 */
    private const val USAGE_WINDOW_MAX_MS = 10_000L
    private const val USAGE_EMA_ALPHA = 0.4
    private const val POWER_MIN_W = 0.05
    private const val POWER_MAX_W = 40.0

    private val ZONE_DIR = Regex("^thermal_zone\\d+$")

    /** cpuidle 的每个空闲状态目录：cpu0/cpuidle/state0、state1… */
    private val IDLE_STATE_DIR = Regex("^state\\d+$")

    /** 这些 zone 量的是电源路径 / modem / 摄像头 / 虚拟调节点，不是芯片温度 */
    private val ZONE_IGNORE =
        Regex("step|bcl|ibat|socd|virtual|^pa\\d*$|sdr|mmw|mdmss|nspss|camera|^pm\\d|battery|usb|quiet")

    @Volatile
    private var zones: ThermalZones? = null

    private val emptyZones = ThermalZones(emptyList(), emptyList())

    /** 上一次采样的墙钟 + 各核 idle 驻留累计值 */
    @Volatile
    private var usageBaseline: Pair<Long, Long>? = null

    @Volatile
    private var usageSmoothed: Double? = null

    /**
     * `cpuidle/stateN/time` 的单位换算。内核文档写毫秒，本机实测是微秒
     * （累计值折算下来正好等于开机时长），判一次就锁存。
     */
    @Volatile
    private var idleUnitDivisor = 1.0

    /** 库仑计基线：估算整机功耗用 */
    @Volatile
    private var chargeBaseline: Pair<Long, Long>? = null

    /** 点「重新扫描」时清掉传感器缓存（换 kernel、内核重载后能重新发现节点） */
    fun resetSensors() {
        zones = null
        usageBaseline = null
        usageSmoothed = null
        chargeBaseline = null
        idleUnitDivisor = 1.0
    }

    /** 特性开关 → 展示名。只列用户看得懂的项，顺序即展示顺序 */
    @Suppress("DEPRECATION")
    private val FEATURE_LABELS = listOf(
        PackageManager.FEATURE_FINGERPRINT to "指纹",
        PackageManager.FEATURE_NFC to "NFC",
        PackageManager.FEATURE_BLUETOOTH to "蓝牙",
        PackageManager.FEATURE_BLUETOOTH_LE to "低功耗蓝牙",
        FEATURE_UWB_NAME to "超宽带",
        PackageManager.FEATURE_LOCATION to "定位",
        PackageManager.FEATURE_TELEPHONY to "通话",
        PackageManager.FEATURE_CAMERA_FRONT to "前置摄像头",
        PackageManager.FEATURE_CAMERA_FLASH to "闪光灯",
        PackageManager.FEATURE_SENSOR_ACCELEROMETER to "加速度计",
        PackageManager.FEATURE_SENSOR_GYROSCOPE to "陀螺仪",
        PackageManager.FEATURE_SENSOR_BAROMETER to "气压计",
        PackageManager.FEATURE_USB_HOST to "USB Host",
        PackageManager.FEATURE_GAMEPAD to "游戏手柄",
    )

    /** 静态规格：整份在 IO 线程上组装，含若干次文件列举与属性反射 */
    suspend fun readSpec(context: Context): DeviceSpec = withContext(Dispatchers.IO) {
        val app = context.applicationContext
        val display = displayOf(app)
        val metrics = realMetrics(display)
        val native = nativePanelSize(display)
        val soc = socModel()
        DeviceSpec(
            marketName = prop("ro.product.marketname") ?: tidy(Build.MODEL),
            brand = tidy(Build.BRAND),
            manufacturer = tidy(Build.MANUFACTURER),
            model = tidy(Build.MODEL),
            deviceCode = listOfNotNull(tidy(Build.DEVICE), tidy(Build.PRODUCT), tidy(Build.BOARD))
                .distinct().joinToString(" / ").ifBlank { null },
            androidRelease = tidy(Build.VERSION.RELEASE),
            sdkInt = Build.VERSION.SDK_INT,
            securityPatch = tidy(Build.VERSION.SECURITY_PATCH),
            buildDisplay = tidy(Build.DISPLAY),
            romLabel = romLabel(),
            kernelVersion = kernelVersion(),
            is64Bit = Build.SUPPORTED_64_BIT_ABIS?.isNotEmpty() == true,
            abis = Build.SUPPORTED_ABIS?.distinct()?.joinToString(" / ")?.ifBlank { null },
            socVendor = SocNames.vendorName(socVendor()) ?: SocNames.vendorFromModel(soc),
            socModel = soc,
            socMarketingName = SocNames.displayName(soc),
            cpuCores = cpuCoreCount(),
            glEsVersion = glesVersion(app),
            vulkanSupported = hasFeature(app, PackageManager.FEATURE_VULKAN_HARDWARE_LEVEL),
            gpuDriver = gpuDriver(),
            gpuName = gpuRendererName(),
            ramTotal = nominalRamText(memoryInfo(app)?.totalMem),
            storageTotal = storageTotalText(),
            screenResolution = resolutionText(native, metrics),
            screenInches = screenInches(native, metrics),
            maxRefreshRate = maxRefreshRateText(app),
            screenDpi = metrics?.densityDpi?.takeIf { it > 0 },
            hdrSupported = runCatching { display?.isHdr == true }.getOrDefault(false),
            supportedFeatures = FEATURE_LABELS.mapNotNull { (id, name) ->
                if (hasFeature(app, id)) name else null
            },
        )
    }

    /** 轮询字段：每项都是一次轻量调用（内存/磁盘 stat、一次粘性广播、十几个小 sysfs 文件） */
    suspend fun readRuntime(context: Context): DeviceRuntime = withContext(Dispatchers.IO) {
        val app = context.applicationContext
        // 广播只取一次，下面几项共用
        val battery = batteryStatus(app)
        val cores = coresOnline()
        // 存储三项（总量/剩余/已用）共用一个口径：总量 = 标称容量（与规格页一致），
        // 剩余 = /data 可用，已用 = 总量 - 剩余（系统/固件占用计入已用，与系统设置口径一致），
        // 保证已用 + 剩余 = 总量 严格成立
        val volume = statFs(Environment.getDataDirectory().path)
        val totalBytes = nominalTotalBytes()
        val freeBytes = volume?.let { it.availableBlocksLong * it.blockSizeLong }
        // 先化成 0.1 GB 的整数再回字符串：剩余四舍五入后，已用按同一精度补齐，不会差 0.1
        val totalTenths = totalBytes?.let { Math.round(it / GB * 10) }
        val freeTenths = freeBytes?.let { Math.round(it / GB * 10) }
        val power = batteryPowerWatt(app, battery)
        DeviceRuntime(
            coresOnline = cores,
            ramAvailable = gbText(memoryInfo(app)?.availMem, decimals = 1),
            storageFree = freeTenths?.let { String.format(Locale.US, "%.1f GB", it / 10.0) },
            storageUsed = if (totalTenths == null || freeTenths == null) null
                else String.format(Locale.US, "%.1f GB", (totalTenths - freeTenths) / 10.0),
            storageUsedPercent = if (totalBytes != null && freeBytes != null && totalBytes > 0) {
                (((totalBytes - freeBytes) * 100.0) / totalBytes).roundToInt().coerceIn(0, 100)
            } else null,
            batteryPercent = batteryPercent(battery)
                ?: batteryProperty(app, BatteryManager.BATTERY_PROPERTY_CAPACITY)?.takeIf { it in 0..100 },
            chargingLabel = chargingLabel(battery),
            batteryVoltage = battery
                ?.getIntExtra(BatteryManager.EXTRA_VOLTAGE, 0)
                ?.takeIf { it > 0 }
                ?.let { String.format(Locale.US, "%.2f V", it / 1000.0) },
            cpuTempC = maxZoneTemp(thermalZones().cpu),
            gpuTempC = maxZoneTemp(thermalZones().gpu),
            cpuUsagePercent = cpuUsagePercent(cores),
            gpuUsagePercent = null,
            cpuClock = cpuClockText(),
            batteryPower = power?.let { (watt, _) -> String.format(Locale.US, "%.1f W", watt) },
            batteryPowerEstimated = power?.second == true,
            batteryTempC = battery
                ?.getIntExtra(BatteryManager.EXTRA_TEMPERATURE, 0)
                ?.takeIf { it > 0 }
                ?.let { it / 10.0 },
        )
    }

    // ───────────────────────── 取值细节 ─────────────────────────

    /** 候选属性：值里自带 ROM 名的最可信，顺序即优先级（新名在前，避免被历史名抢先） */
    private val ROM_TEXT_PROPS = listOf(
        "ro.vivo.os.build.display.id",
        "ro.build.display.id",
        "ro.coloros.version",
        "ro.build.version.opporom",
        "ro.build.version.realmeui",
        "ro.magic.ui.version",
        "ro.honor.os.version",
        "ro.build.version.emui",
        "ro.harmonyos.version",
        "ro.build.version.smartos",
        "ro.vivo.os.name",
    )

    /** 已知定制 ROM 名，顺序即优先级：OriginOS 排在 Funtouch 前，HyperOS 排在 MIUI 前 */
    private val ROM_KEYWORDS = listOf(
        "HyperOS", "OriginOS", "Funtouch", "ColorOS", "OxygenOS", "HydrogenOS",
        "realme UI", "MagicOS", "Magic", "HarmonyOS", "EMUI",
        "RedMagic", "MyOS", "Flyme", "Nothing", "Smartisan", "MIUI",
    )

    /**
     * 定制系统标签。厂商私有属性又多又不统一，这里收敛成四步，避免把编码值或历史值直接吐给用户：
     * 1. 三星 oneui 属性是编码整数（80500 = One UI 8.5），必须解码；
     * 2. 扫一遍候选属性，**值里自带 ROM 名**的（"OriginOS 5"、"ColorOS 14"）最可信，直接采用；
     *    这一步同时兜住了「属性名对不上但值里有名字」的机型，而且不会拼出乱码（没关键字就跳过）；
     * 3. 都没有再按品牌拼「名 + 版本号」；
     * 4. 欧加系还剩品牌兜底（[oplusDisplayLabel]）：私有属性全灭时用公开 API Build.DISPLAY 解析。
     * 其余全部为空则返回 null 让整行隐藏，不写「原生系统」瞎猜。
     */
    private fun romLabel(): String? {
        // 鸿蒙最先判：hw_sc.build.platform.version 只在鸿蒙上有值（EMUI/海外固件没有），
        // 而 EMUI 属性在鸿蒙双框架机上也存在（值还是 EmotionUI_x），后判会被它抢走
        harmonyLabel()?.let { return it }
        oneUiLabel()?.let { return it }

        ROM_TEXT_PROPS.forEach { key ->
            val raw = prop(key) ?: return@forEach
            ROM_KEYWORDS.firstOrNull { raw.contains(it, ignoreCase = true) }?.let { kw ->
                return romText(raw, kw)
            }
        }

        // 澎湃：固件版本号形如 OS3.0.304.0.xxx（实测 Redmi Note 13 Pro 只有这个能认出来）
        if (isXiaomiFamily()) {
            Regex("^OS(\\d+)").find(tidy(Build.VERSION.INCREMENTAL).orEmpty())?.let {
                return "HyperOS ${it.groupValues[1]}"
            }
        }
        prop("ro.miuiDescriptor.version")?.let { return "HyperOS ${pretty(it)}" }
        prop("ro.miui.ui.version.name")?.let { return "MIUI ${pretty(it)}" }
        vivoLabel()?.let { return it }
        prop("ro.build.version.emui")?.let { return emuiLabel(it) }
        // 欧加系三家共用底座但商品名不同：OPPO/一加国行=ColorOS、一加海外=OxygenOS、realme=realme UI
        oplusLabel()?.let { return it }
        prop("ro.build.version.realmeui")?.let { return label("realme UI", it) }
        prop("ro.build.version.magic")?.let { return magicLabel(it) }
        prop("msc.config.magic.version")?.let { return magicLabel(it) }
        prop("ro.magic.ui.version")?.let { return label("MagicOS", it) }
        prop("ro.build.version.smartos")?.let { return label("Smartisan OS", it) }
        // 欧加系兜底：SystemProperties 反射被挡或 opporom 一族属性缺失时，改用公开 API Build.DISPLAY
        oplusDisplayLabel()?.let { return it }
        return null
    }

    /**
     * 欧加系（OPPO / 一加 / realme）。三家固件共享欧加底座，版本号都来自
     * ro.build.version.opporom，但对外商品名必须按品牌和销售区域区分：
     * OPPO=ColorOS；一加国行=ColorOS、海外=OxygenOS（OxygenOS 13 起两系合并，
     * 版本号同源，海外判定用 ro.vendor.oplus.regionmark=CN/EUEX/IN/RU，取不到按国行）；
     * realme=realme UI。合并前的老氧 OS 固件带 ro.oxygen.version，优先采用。
     * opporom 之外再试 oplusrom / vendor 副本：个别固件上主属性缺失或改名时还有冗余。
     */
    private fun oplusLabel(): String? =
        oplusVersionLabel(prop("ro.build.version.opporom"))
            ?: oplusVersionLabel(prop("ro.build.version.oplusrom"))
            ?: prop("ro.oxygen.version")?.let { label("OxygenOS", it) }
            ?: oplusVersionLabel(prop("ro.vendor.build.version.opporom"))
            ?: prop("ro.coloros.version")?.let { ver -> oplusVersionLabel(ver) ?: label("ColorOS", ver) }

    private fun oplusVersionLabel(raw: String?): String? {
        val version = raw?.trim()?.removePrefix("V")?.removePrefix("v")?.takeIf { it.isNotEmpty() } ?: return null
        if (isBrand("realme")) return label("realme UI", version)
        if (isBrand("oneplus") && !oplusRegionChina()) return label("OxygenOS", version)
        return label("ColorOS", version)
    }

    /** 欧加系品牌判定：BRAND / MANUFACTURER / PRODUCT 任一命中即可 */
    private fun isBrand(vararg names: String): Boolean {
        val b = "${Build.BRAND} ${Build.MANUFACTURER} ${Build.PRODUCT}".lowercase()
        return names.any { it in b }
    }

    private fun oplusRegionChina(): Boolean {
        val region = prop("ro.vendor.oplus.regionmark")
            ?: prop("ro.product.oplus.regionmark")
            ?: prop("ro.vendor.oppo.version")
            ?: return true // 读不到区域属性按国行算：这台应用的用户以国行为主
        return region.equals("CN", true)
    }

    /** 欧加系 display.id 的版本段：PHS110_15.0.2.501(CN01) → 版本 15.0.2.501、区域 CN */
    private val OPLUS_DISPLAY_VERSION = Regex("_V?(\\d{1,2}(?:\\.\\d+){1,3})(?:\\(([A-Za-z]{2})\\d*\\))?")

    private fun isOplusFamily(): Boolean = isBrand("oppo", "oneplus", "realme")

    /**
     * 欧加系最后兜底：前面的链路全靠 SystemProperties 反射，个别固件上 opporom 一族
     * 属性就是没有（或反射被挡），用户看到的反馈就是「本机获取不到 ColorOS」——整行消失。
     * 公开 API Build.DISPLAY 在欧加三家固件上是固定格式「型号_版本号(区域码)」
     * （PHS110_15.0.2.501(CN01)、CPH2611_15.0.0.402(EX01)），版本号与 opporom 同源，
     * 括号里的区域码还能修正一加的 ColorOS/OxygenOS 之分（无区域码按国行）。
     */
    private fun oplusDisplayLabel(): String? {
        if (!isOplusFamily()) return null
        val groups = tidy(Build.DISPLAY)?.let { OPLUS_DISPLAY_VERSION.find(it) }?.groupValues
        if (groups != null && groups[1].isNotEmpty()) {
            val region = groups[2].uppercase(Locale.US)
            return when {
                isBrand("realme") -> label("realme UI", groups[1])
                isBrand("oneplus") && region.isNotEmpty() && !region.startsWith("CN") ->
                    label("OxygenOS", groups[1])
                else -> label("ColorOS", groups[1])
            }
        }
        // display.id 也不是标准格式（老版 ColorOS 11/12 是 PEEM00_11_A.16 这类）：
        // 品牌对得上就只报家族名，版本号不编，比整行消失强
        return when {
            isBrand("realme") -> "realme UI"
            isBrand("oneplus") && !oplusRegionChina() -> "OxygenOS"
            else -> "ColorOS"
        }
    }

    /** 荣耀 2022 年（MagicOS 7）起把 Magic UI 更名 MagicOS；属性版本号连续，老版本要还原旧名 */
    private fun magicLabel(version: String): String? {
        val major = version.trim().toDoubleOrNull()?.toInt() ?: 0
        return if (major in 1..6) label("Magic UI", version) else label("MagicOS", version)
    }

    /**
     * 三星 One UI 的版本号是编码整数而不是文本：60100 → 6.1、60101 → 6.1.1、80500 → 8.5。
     * 直接把属性值贴出来就是用户反馈的「One UI 80500」。
     */
    private fun oneUiLabel(): String? {
        val code = prop("ro.build.version.oneui")?.toIntOrNull() ?: return null
        val major = code / 10000
        if (major <= 0) return null
        val minor = (code / 100) % 100
        val patch = code % 100
        return buildString {
            append("One UI $major")
            if (minor > 0) append(".$minor")
            if (patch > 0) append(".$patch")
        }
    }

    /**
     * vivo：`ro.vivo.os.name` 在跑 OriginOS 的机器上可能仍是历史值 Funtouch，
     * 而 `ro.vivo.os.version` 又常等于 Android 大版本，拼起来就是用户反馈的「Funtouch 16.0」。
     * 所以版本号和 Android 版本对得上时不拼版本号，只留名字；OriginOS 的正确识别交给关键字扫描那一步。
     */
    private fun vivoLabel(): String? {
        val name = prop("ro.vivo.os.name") ?: return null
        val version = prop("ro.vivo.os.version")
        val release = tidy(Build.VERSION.RELEASE)
        val looksLikeAndroid = version == null || release.isNullOrEmpty() ||
            version.startsWith(release)
        return if (looksLikeAndroid) pretty(name) else label(pretty(name), version)
    }

    /**
     * 鸿蒙版本：hw_sc.build.platform.version（如 "4.2.0"）是社区通用的鸿蒙判定属性，
     * 只在鸿蒙（含能跑安卓应用的双框架 2~4）上有值，EMUI 与海外固件上为空。
     * 尾段 ".0" 去掉 —— 华为对外就写 HarmonyOS 3、HarmonyOS 4.2，不写 3.0.0。
     */
    private fun harmonyLabel(): String? {
        val raw = prop("hw_sc.build.platform.version") ?: return null
        if (raw.firstOrNull()?.isDigit() != true) return null
        val segments = raw.split('.')
        val kept = segments.dropLastWhile { it == "0" }.ifEmpty { listOf(segments.first()) }
        return "HarmonyOS ${kept.joinToString(".")}"
    }

    /**
     * 海外 EMUI 与国行鸿蒙同代同源、只是品牌名不同：官方对 Mate X6 的写法就是
     * "HarmonyOS 4.3 / EMUI 15" 双标（Pura 70 海外 EMUI 14.2 = 国行鸿蒙 4.2）。
     * 认出同代关系后把国行名标在前面，用户不用再猜 "EMUI 15 是个什么版本"。
     * 12 及更早的 EMUI 没有可靠公开的对应关系，不标。
     */
    private val EMUI_HARMONY_TWINS = mapOf(
        "14.2" to "HarmonyOS 4.2",
        "15" to "HarmonyOS 4.3",
    )

    /**
     * EMUI 的属性值多写旧称 "EmotionUI_15.0.0"（EMUI 本来就是 EmotionUI 的缩写），
     * 直接拼前缀会读出 "EMUI EmotionUI 15.0.0"；剥掉新旧称再统一拼，命中同代表的
     * 附上鸿蒙名（"HarmonyOS 4.3（EMUI 15）"）。
     */
    private fun emuiLabel(raw: String): String? {
        val version = pretty(raw).replace(Regex("(?i)(?:emotionui|emui)"), "").trim()
        if (version.isEmpty()) return "EMUI"
        val twin = emuiHarmonyTwin(version)
        if (twin == null) return "EMUI $version"
        // 括号里只留有效段（15.0.0 → 15），副标题一行放得下
        val short = version.split('.').dropLastWhile { it == "0" }.joinToString(".").ifEmpty { version }
        return "$twin（EMUI $short）"
    }

    /** EMUI 版本号逐级去尾段找同代鸿蒙（15.0.1 → 15.0 → 15 命中） */
    private fun emuiHarmonyTwin(version: String): String? {
        val segments = version.split('.')
        for (keep in segments.size downTo 1) {
            EMUI_HARMONY_TWINS[segments.take(keep).joinToString(".")]?.let { return it }
        }
        return null
    }

    /** 值里已经写了 ROM 名：只把下划线换成空格，不重复拼前缀 */
    private fun romText(raw: String, keyword: String): String {
        val text = pretty(raw)
        return if (text.contains(keyword, ignoreCase = true)) text else "$keyword $text"
    }

    /** 小米系品牌判定：只拿它给 incremental 的 HyperOS 推定做限定，避免误伤其他厂商 */
    private fun isXiaomiFamily(): Boolean {
        val brand = "${Build.BRAND} ${Build.MANUFACTURER}".lowercase()
        return listOf("xiaomi", "redmi", "poco", "blackshark").any { it in brand }
    }

    /** 值为空时整行隐藏，"unknown"/"default" 这类占位串也视为没有 */
    private fun tidy(value: String?): String? =
        value?.trim()?.takeIf { it.isNotEmpty() && !it.equals("unknown", true) && !it.equals("default", true) }

    /** sysui 风格属性常带下划线分隔（Emui_4.0），换成可读空格 */
    private fun pretty(raw: String): String = raw.replace('_', ' ').trim()

    /** 前缀已经在值里就别重复拼（"OriginOS_15" → "OriginOS 15"，不是 "OriginOS OriginOS 15"） */
    private fun label(prefix: String, raw: String?): String? {
        val name = pretty(prefix)
        val value = tidy(raw)?.let(::pretty) ?: return name
        return if (value.contains(name, ignoreCase = true)) value else "$name $value"
    }

    /** Linux 内核版本只留 x.y.z 主干，后面一长串构建号没有阅读价值 */
    private fun kernelVersion(): String? =
        tidy(System.getProperty("os.version"))?.substringBefore('-')

    /**
     * SoC 原始代号。`Build.SOC_MODEL`（Android 12+）覆盖新机，但有两个已知缺口：
     * - Android 11 及以下根本没有这个属性（老 EMUI / OriginOS 机器整卡消失）；
     * - 部分华为固件把它写成裸家族名 "Kirin"，不带具体型号。
     * 两条路都落空时按可信度继续找：cpuinfo Hardware 与 soc0 machine（高通老内核直接写
     * 部件号）、ro.board.platform（麒麟给 kirin980、高通给平台代号）、厂商私有属性、
     * 主板 / 硬件值。能映射出商用名的候选最可信；都没有再退部件号样子的原始值。
     */
    private fun socModel(): String? {
        val primary = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            tidy(Build.SOC_MODEL) ?: prop("ro.soc.model")
        } else {
            prop("ro.soc.model")
        }
        // 只报家族名（不含数字）说明厂商没写实，继续往下找具体型号
        if (primary != null && primary.any { it.isDigit() }) return primary
        val candidates = listOfNotNull(
            cpuinfoHardware(),
            soc0Machine(),
            prop("ro.board.platform"),
            prop("ro.vendor.qti.soc_name"),
            prop("ro.mediatek.platform"),
            prop("ro.product.board"),
            tidy(Build.BOARD),
            tidy(Build.HARDWARE),
        ).distinct()
        candidates.firstOrNull { SocNames.displayName(it) != null }?.let { return it }
        // 没有能认出的：部件号样子的原始代号（"MT6785"）也比裸家族名有信息量
        return candidates.firstOrNull { it.any { c -> c.isDigit() } } ?: primary
    }

    /** cpuinfo Hardware / soc0 machine 值开头的厂商前缀，剥掉后剩芯片部件号 */
    private val CHIP_VENDOR_PREFIX = Regex(
        "(?i)^(qualcomm technologies,? inc\\.?|qualcomm|qti|hisi(?:licon)?|huawei|mediatek)\\s+",
    )

    /**
     * /proc/cpuinfo 的 Hardware 行：内核 4.x 上高通写 "Qualcomm Technologies, Inc SDM660"、
     * 海思写芯片名；内核 5.x 起大多删掉了，读不到就交给下一个来源。
     */
    private fun cpuinfoHardware(): String? = runCatching {
        File("/proc/cpuinfo").useLines { lines ->
            lines.firstOrNull { it.startsWith("Hardware", ignoreCase = true) }
        }?.substringAfter(':')?.trim()?.takeIf { it.isNotEmpty() }
    }.getOrNull()?.let { CHIP_VENDOR_PREFIX.replace(it, "").trim().takeIf { v -> v.isNotEmpty() } }

    /** 高通平台的 /sys/devices/soc0/machine，同样写 "Qualcomm Technologies, Inc. SM6150"；被 SELinux 挡就算了 */
    private fun soc0Machine(): String? = readSysfs("/sys/devices/soc0/machine")
        ?.let { CHIP_VENDOR_PREFIX.replace(it, "").trim().takeIf { v -> v.isNotEmpty() } }

    private fun socVendor(): String? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        tidy(Build.SOC_MANUFACTURER) ?: prop("ro.soc.manufacturer")
    } else {
        prop("ro.soc.manufacturer")
    }

    /**
     * 核心总数。省电模式下 `availableProcessors` 会跟着下线核心少报，
     * 所以再用 /sys 下的 cpuN 目录数取个较大值。
     */
    private fun cpuCoreCount(): Int {
        val fromSys = runCatching {
            File(CPU_ROOT).listFiles()?.count { f -> f.isDirectory && CPU_DIR.matches(f.name) }
        }.getOrNull() ?: 0
        return maxOf(Runtime.getRuntime().availableProcessors(), fromSys)
    }

    /** /sys/devices/system/cpu/online 形如 "0-7" 或 "0-3,5-7"，数出在线核心个数 */
    private fun coresOnline(): Int? = runCatching {
        val text = File(CPU_ONLINE).readText().trim().ifBlank { return null }
        text.split(',').sumOf { part ->
            val bounds = part.split('-')
            if (bounds.size == 2) {
                val from = bounds[0].trim().toIntOrNull() ?: return null
                val to = bounds[1].trim().toIntOrNull() ?: return null
                (to - from + 1).coerceAtLeast(0)
            } else {
                1
            }
        }.takeIf { it > 0 }
    }.getOrNull()

    /** GLES 版本：系统特性里的 reqGlEsVersion 高 16 位是主版本、低 16 位是次版本 */
    private fun glesVersion(context: Context): String? = runCatching {
        val max = context.packageManager.systemAvailableFeatures.maxOfOrNull { it.reqGlEsVersion } ?: 0
        if (max <= 0) return null
        val major = (max ushr 16) and 0xFFFF
        val minor = max and 0xFFFF
        "OpenGL ES $major.$minor"
    }.getOrNull()

    /** GPU 驱动族：两个属性各机型命中情况不同，映射成用户认识的名字 */
    private fun gpuDriver(): String? {
        val raw = prop("vendor.graphics.gl") ?: prop("ro.hardware.egl") ?: return null
        val v = raw.lowercase()
        return when {
            "adreno" in v -> "Adreno"
            "mali" in v -> "Mali"
            "powervr" in v || "pvr" in v || "rogue" in v -> "PowerVR"
            "panvk" in v || "panfrost" in v -> "Mali (Panfrost)"
            "virtio" in v -> "virtio-GPU"
            else -> pretty(raw).replaceFirstChar { it.uppercase() }
        }
    }

    private fun memoryInfo(context: Context): ActivityManager.MemoryInfo? = runCatching {
        val am = context.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager ?: return null
        ActivityManager.MemoryInfo().also { am.getMemoryInfo(it) }
    }.getOrNull()

    /** GL 查询结果缓存：驱动不会热更，一个进程只查一次 */
    @Volatile
    private var glRenderer: String? = null

    /**
     * GPU 精确型号：离屏 EGL 上下文里的 GL_RENDERER（"Adreno (TM) 810" → "Adreno 810"）。
     * 这是免权限拿到精确 GPU 名的唯一来源——kgsl 的 sysfs 节点被 SELinux 全挡。
     * 查询建 1×1 pbuffer、查完即毁；display 是进程级单例且 HWUI 在共用，**不能 eglTerminate**。
     */
    private fun gpuRendererName(): String? {
        glRenderer?.let { return it }
        val name = runCatching {
            val display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
            if (display == EGL14.EGL_NO_DISPLAY) return@runCatching null
            val version = IntArray(2)
            if (!EGL14.eglInitialize(display, version, 0, version, 1)) return@runCatching null
            val config = chooseEglPbufferConfig(display) ?: return@runCatching null
            val context = EGL14.eglCreateContext(
                display, config, EGL14.EGL_NO_CONTEXT,
                intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE), 0,
            )
            if (context == EGL14.EGL_NO_CONTEXT) return@runCatching null
            try {
                val surface = EGL14.eglCreatePbufferSurface(
                    display, config,
                    intArrayOf(EGL14.EGL_WIDTH, 1, EGL14.EGL_HEIGHT, 1, EGL14.EGL_NONE), 0,
                )
                if (surface == EGL14.EGL_NO_SURFACE) return@runCatching null
                try {
                    if (!EGL14.eglMakeCurrent(display, surface, surface, context)) return@runCatching null
                    val renderer = GLES20.glGetString(GLES20.GL_RENDERER)
                    EGL14.eglMakeCurrent(
                        display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT,
                    )
                    renderer
                } finally {
                    EGL14.eglDestroySurface(display, surface)
                }
            } finally {
                EGL14.eglDestroyContext(display, context)
            }
        }.getOrNull()
            ?.replace("(TM)", "", ignoreCase = true)
            ?.replace("™", "")
            ?.trim()
            ?.replace(Regex("\\s+"), " ")
            ?.takeIf { it.isNotEmpty() }
        // 只缓存成功值：个别设备查不到时，下次「重新扫描」还有机会
        if (name != null) glRenderer = name
        return name
    }

    /** 选一个支持 pbuffer 的 ES2 配置；拿不到就整个 GL 查询降级 */
    private fun chooseEglPbufferConfig(display: EGLDisplay): EGLConfig? {
        val attribs = intArrayOf(
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_PBUFFER_BIT,
            EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT,
            EGL14.EGL_RED_SIZE, 8, EGL14.EGL_GREEN_SIZE, 8, EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 0,
            EGL14.EGL_NONE,
        )
        val configs = arrayOfNulls<EGLConfig>(1)
        val num = IntArray(1)
        return if (EGL14.eglChooseConfig(display, attribs, 0, configs, 0, 1, num, 0) && num[0] > 0) {
            configs[0]
        } else {
            null
        }
    }

    private fun statFs(path: String): StatFs? = runCatching { StatFs(path) }.getOrNull()

    /**
     * 机身存储总量（标称口径）。StatFs(/data) 只有用户数据分区（128GB 机型通常只剩
     * 105GB 出头），/sys/block 的块设备节点又被 SELinux 挡死（本机连 shell 都读不到），
     * 所以物理容量按「块设备 → statvfs 求和」取，吸附到常见标称档位后输出；
     * 都读不到才退回 /data 实际容量。规格页与存储卡片的总量/已用/剩余共用这一个口径。
     */
    private fun storageTotalText(): String? =
        nominalTotalBytes()?.let { "${(it / GB).roundToInt()} GB" }

    /** 标称总量（字节）：物理容量吸附常见档位（±10%），异形容量按十进制 GB 取整；物理量不可得时退 /data 实际容量 */
    private fun nominalTotalBytes(): Long? {
        val physical = flashTotalBytes() ?: mountTotalBytes()
            ?: return statFs(Environment.getDataDirectory().path)?.let { it.blockCountLong * it.blockSizeLong }
        val gb = physical / GB
        val nearest = MARKETING_GB.minByOrNull { abs(gb - it) }
        val snapped = nearest?.takeIf { abs(gb - nearest) / nearest <= 0.10 } ?: gb.roundToInt()
        return snapped.toLong() * 1_000_000_000L
    }

    /**
     * 机身闪存物理容量：累加 /sys/block 下真实块设备的扇区数（内核恒按 512B 扇区上报）。
     * eMMC 是一整块 mmcblk0；UFS 按逻辑单元拆成 sda、sdb… 多个盘，求和才是全容量。
     * 排除 device-mapper/loop/zram 虚拟盘与可移动介质（SD 卡、U 盘），读不到回 null。
     */
    private fun flashTotalBytes(): Long? = runCatching {
        File("/sys/block").listFiles()
            ?.filter { dir ->
                val name = dir.name
                if (!dir.isDirectory || name.startsWith("dm-") || name.startsWith("loop") ||
                    name.startsWith("zram") || name.startsWith("ram")
                ) return@filter false
                // mmcblk0 恒为内置 eMMC；其余盘需确认非可移动介质，removable 读不到就宁缺
                if (name == "mmcblk0") return@filter true
                readSysfs("${dir.path}/removable")?.toIntOrNull() == 0
            }
            ?.sumOf { readSysfsLong("${it.path}/size") ?: 0L }
            ?.let { sectors -> sectors * 512L }
            ?.takeIf { it >= 8_000_000_000L } // 起码像块 8GB 的闪存，防虚拟盘碎片污染求和
    }.getOrNull()

    /**
     * 真实文件系统容量求和：对候选挂载点逐个 statvfs。这条链路不走文件读取，应用域放行
     * （本机实测各挂载点全部可读，而 /sys/block 连 shell 都被拒）。
     * f_fsid 相同 = 同一文件系统（system-as-root 下 /system 是 / 的 bind 挂载），只计一次。
     * 固件分区（xbl/modem 等）不在任何挂载点上，求和比物理容量小几个 GB，交给档位吸附消化。
     */
    private fun mountTotalBytes(): Long? = runCatching {
        val seen = HashSet<String>()
        var total = 0L
        for (path in MOUNT_CANDIDATES) {
            val fs = runCatching { Os.statvfs(path) }.getOrNull() ?: continue
            if (fs.f_frsize <= 0L) continue
            val bytes = fs.f_blocks * fs.f_frsize
            // 过滤伪文件系统与异常值：闪存上的单个文件系统不会小于 1MB 也不会超过 2TB
            if (bytes < 1_000_000L || bytes > 2_000_000_000_000L) continue
            if (!seen.add(if (fs.f_fsid == 0L) "path:$path" else "fs:${fs.f_fsid}")) continue
            total += bytes
        }
        total.takeIf { it >= 8_000_000_000L }
    }.getOrNull()

    /** GB 文本。容量类整数感觉更好，剩余量保留一位小数免得常年显示 0 GB */
    private fun gbText(bytes: Long?, decimals: Int): String? {
        val value = bytes ?: return null
        if (value <= 0L) return null
        val gb = value / GIB
        return if (decimals <= 0) "${gb.roundToInt()} GB"
        else String.format(Locale.US, "%.${decimals}f GB", gb)
    }

    /** 内存标称档位（GiB）：内存芯片按 2^n×通道数组合，标称容量数值上就是 GiB */
    private val RAM_TIERS_GIB =
        doubleArrayOf(1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0, 12.0, 16.0, 18.0, 24.0, 32.0)

    /**
     * 运行内存标称值。`ActivityManager.totalMem` 是内核可用的 MemTotal —— 扣掉了
     * 固件/基带/相机等预留，本机（8GB 的 Redmi Note 13 Pro）只有 7.13 GiB，直接
     * 取整就成了用户反馈的「比实际少 1G」（12GB 显示 11、8GB 显示 7）。
     * MemTotal 严格小于物理容量、而物理容量恒等于标称档位，所以向上吸附最近
     * 档位即精确还原标称值，无需容差 —— 与机身存储的档位吸附同一思路。
     */
    private fun nominalRamText(totalMem: Long?): String? {
        val gib = (totalMem ?: return null) / GIB
        if (gib <= 0.0) return null
        val tier = RAM_TIERS_GIB.firstOrNull { it >= gib } ?: return "${Math.round(gib)} GB"
        return if (tier % 1.0 == 0.0) "${tier.toInt()} GB" else "$tier GB"
    }

    /**
     * 取默认屏。**两条来源必须分开 catch**：Application context 上 `Context.getDisplay()`
     * 会直接抛 UnsupportedOperationException，放在同一个 runCatching 里会把后面的
     * DisplayManager 回退一起吃掉，导致分辨率 / 尺寸 / 刷新率三项同时消失。
     */
    private fun displayOf(context: Context): Display? {
        val fromContext = runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) context.display else null
        }.getOrNull()
        if (fromContext != null) return fromContext
        return runCatching {
            @Suppress("DEPRECATION")
            (context.getSystemService(Context.DISPLAY_SERVICE) as? DisplayManager)
                ?.getDisplay(Display.DEFAULT_DISPLAY)
        }.getOrNull()
    }

    /**
     * 面板原生像素：取所有 `supportedModes` 里面积最大的一档。
     *
     * 不能直接用 `metrics.widthPixels/heightPixels`：小米等 ROM 的「分辨率」设置会把
     * 渲染分辨率降到 1080P，此时 realMetrics 给的是**渲染**尺寸（1080×2400），而 xdpi 仍是
     * 面板真实密度 526，两者一除就得到用户反馈的「6.67 英寸显示成 5 英寸」。
     * 像素与 dpi 必须同一基准，所以一律用面板原生尺寸。
     */
    private fun nativePanelSize(display: Display?): Pair<Int, Int> {
        val modes = runCatching { display?.supportedModes }.getOrNull()
        val best = modes?.maxByOrNull { it.physicalWidth.toLong() * it.physicalHeight }
        val w = best?.physicalWidth ?: 0
        val h = best?.physicalHeight ?: 0
        return if (w > 0 && h > 0) w to h else 0 to 0
    }

    /** 物理分辨率：面板原生像素（不受渲染档位与窗口/分屏影响），退回 getRealMetrics */
    private fun resolutionText(native: Pair<Int, Int>, metrics: DisplayMetrics?): String? {
        val w = if (native.first > 0) native.first else metrics?.widthPixels ?: 0
        val h = if (native.second > 0) native.second else metrics?.heightPixels ?: 0
        if (w <= 0 || h <= 0) return null
        // 统一按「竖 × 横」（长边 × 短边）输出，与参数表习惯一致，也不受当前横竖屏影响
        return "${maxOf(w, h)} × ${minOf(w, h)}"
    }

    /**
     * 物理屏幕参数。必须用 getRealMetrics：分屏/小窗下 displayMetrics 拿的是窗口尺寸，
     * 会把 2K 屏读成半屏。
     */
    @Suppress("DEPRECATION")
    private fun realMetrics(display: Display?): DisplayMetrics? = runCatching {
        display ?: return null
        DisplayMetrics().also { display.getRealMetrics(it) }
    }.getOrNull()

    /**
     * 屏幕对角线。公开 API 里没有返回毫米尺寸的接口，只能用 `getRealMetrics` 的 xdpi/ydpi
     * （面板每英寸真实像素）去除以**面板原生像素**。结果不在手机/平板的合理区间就当读不到，
     * 不编一个假尺寸。
     */
    private fun screenInches(native: Pair<Int, Int>, metrics: DisplayMetrics?): String? {
        metrics ?: return null
        val w = if (native.first > 0) native.first else metrics.widthPixels
        val h = if (native.second > 0) native.second else metrics.heightPixels
        if (w <= 0 || h <= 0) return null
        // 两个轴要么都用物理密度，要么都用 densityDpi：混用会让对角线偏掉一大截
        val physical = metrics.xdpi > 100f && metrics.ydpi > 100f
        val horizontal = if (physical) w / metrics.xdpi else w * 25.4f / metrics.densityDpi
        val vertical = if (physical) h / metrics.ydpi else h * 25.4f / metrics.densityDpi
        if (horizontal <= 0f || vertical <= 0f) return null
        val inch = sqrt(horizontal * horizontal + vertical * vertical)
        if (inch < 2.0f || inch > 20.0f) return null
        return String.format(Locale.US, "%.1f 英寸", inch)
    }

    /**
     * 面板最高支持刷新率：取所有 `supportedModes` 里的最大值。
     *
     * 不能用 `display.mode.refreshRate`——那是**当前生效**的档位，国产 ROM 的智能刷新率
     * 与省电策略会把它压到 60/90，用户看到的就是「我 144 的屏显示 120、120 的屏显示 60」。
     * 规格页要的是硬件上限，与实时档位分开两个字段。
     */
    private fun maxRefreshRateText(context: Context): String? = runCatching {
        val modes = displayOf(context)?.supportedModes ?: return null
        val hz = modes.maxOfOrNull { it.refreshRate } ?: return null
        hz.roundToInt().takeIf { it > 0 }?.let { "$it Hz" }
    }.getOrNull()

    /** 容量类属性的兜底：读不到时 getIntProperty 返回负数，滤掉 */
    private fun batteryProperty(context: Context, which: Int): Int? = runCatching {
        (context.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager)
            ?.getIntProperty(which)
            ?.takeIf { it >= 0 }
    }.getOrNull()

    /**
     * 电池快照：拿 ACTION_BATTERY_CHANGED 的粘性广播。receiver 传 null 只是取回
     * 最近一次广播，不会真的注册回调；系统广播免权限，API 33+ 要显式给导出标志。
     */
    private fun batteryStatus(context: Context): Intent? = runCatching {
        ContextCompat.registerReceiver(
            context,
            null,
            IntentFilter(Intent.ACTION_BATTERY_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
    }.getOrNull()

    /** 电量百分比：EXTRA_LEVEL / EXTRA_SCALE 比 BatteryManager 的容量属性更准（能拿到小数取整） */
    private fun batteryPercent(intent: Intent?): Int? {
        intent ?: return null
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, -1)
        if (level < 0 || scale <= 0) return null
        return (level * 100f / scale).roundToInt().coerceIn(0, 100)
    }

    /** 充电状态带插入方式（交流 / USB / 无线），插入方式只在电池广播里，BatteryManager 没有对应属性 */
    private fun chargingLabel(intent: Intent?): String? {
        intent ?: return null
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        if (status == BatteryManager.BATTERY_STATUS_FULL) return "已充满"
        return when (intent.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0)) {
            BatteryManager.BATTERY_PLUGGED_WIRELESS -> "无线充电中"
            BatteryManager.BATTERY_PLUGGED_AC -> "交流充电中"
            BatteryManager.BATTERY_PLUGGED_USB -> "USB 充电中"
            0 -> when (status) {
                BatteryManager.BATTERY_STATUS_NOT_CHARGING -> "已插电未充电"
                BatteryManager.BATTERY_STATUS_DISCHARGING -> "放电中"
                else -> null
            }
            else -> "充电中"
        }
    }

    // ───────────────────────── 温度 / 占用 / 频率 / 功耗 ─────────────────────────

    /**
     * 温度区快照。`type` 不会自己变，本机 76 个 zone 每轮都重扫一遍纯属浪费，
     * 所以扫一次缓存起来，点「重新扫描」时由 [resetSensors] 清掉。
     */
    private class ThermalZones(val cpu: List<File>, val gpu: List<File>)

    private fun thermalZones(): ThermalZones {
        zones?.let { return it }
        val found = runCatching {
            val cpu = mutableListOf<File>()
            val gpu = mutableListOf<File>()
            val soc = mutableListOf<File>()
            File(THERMAL_ROOT).listFiles { f -> f.isDirectory && ZONE_DIR.matches(f.name) }
                .orEmpty()
                .forEach { dir ->
                    val type = readSysfs("${dir.path}/type")?.lowercase()?.removePrefix("qcom,") ?: return@forEach
                    if (type.isBlank() || ZONE_IGNORE.containsMatchIn(type)) return@forEach
                    when {
                        // qcom 的 "gpu-0"、mtk 的 "mtkgpu"、海思直接叫 "gpu"（Pura 80 实测），按包含认
                        "gpu" in type -> gpu += dir
                        // 有 cpu 字样的直接认；麒麟的 CPU 簇叫 cluster0/1/2（Pura 80 实测，没有
                        // cpu 字样）、三星大小核叫 big_core/little_core，按真实命名收
                        "cpu" in type || "cluster" in type ||
                            ("core" in type && Regex("big|little|mid").containsMatchIn(type)) -> cpu += dir
                        // 海思老内核（麒麟 980 时代）整机温度区 soc-thermal，与 qcom aoss 一样兜底；
                        // pa/shell/charger/bat_raw/npu/isp 等外设与协处理器区分类不中自然丢弃
                        "aoss" in type || "soc" in type -> soc += dir
                    }
                }
            ThermalZones(cpu.ifEmpty { soc }, gpu)
        }.getOrNull() ?: return emptyZones
        // 一个都没扫到时不缓存：换 ROM、内核重载之后还有机会读到
        if (found.cpu.isNotEmpty() || found.gpu.isNotEmpty()) zones = found
        return found
    }

    /** 毫摄氏度 → 摄氏度。断开源回 -273000、策略伪 zone 回 0，都当没有这个传感器 */
    private fun readZoneTempC(dir: File): Double? =
        readSysfsLong("${dir.path}/temp")?.let { it / 1000.0 }?.takeIf { it in 1.0..150.0 }

    /** 组内取最高而不是平均：一个 80℃ 的热点不该被七个冷核抹平成 30℃ */
    private fun maxZoneTemp(dirs: List<File>): Double? = dirs.mapNotNull { readZoneTempC(it) }.maxOrNull()

    /**
     * 供悬浮窗服务独立使用：占用与温度的轻量读数。类型归属与 readRuntime 完全一致，
     * 但只读需要的两项，避免服务为拿两个值去拉起整套电池/功耗广播。
     */
    internal fun cpuUsageNow(): Int? = cpuUsagePercent(coresOnline())

    internal fun cpuTempNow(): Double? = maxZoneTemp(thermalZones().cpu)

    internal fun gpuTempNow(): Double? = maxZoneTemp(thermalZones().gpu)

    /** 内存占用率：(总内存 - 可用内存) / 总内存。availMem 已含系统可回收部分，口径与系统设置一致 */
    internal fun memUsageNow(context: Context): Int? = runCatching {
        val info = memoryInfo(context) ?: return null
        if (info.totalMem <= 0L) return null
        (((info.totalMem - info.availMem) * 100.0) / info.totalMem).roundToInt().coerceIn(0, 100)
    }.getOrNull()

    /**
     * CPU 占用率：1 −（各核 cpuidle 空闲驻留之和）÷（在线核数 × 墙钟差）。
     *
     * 为什么不用看起来更直接的 `time_in_state`：本机 kernel 只在**离开某个频率档时**才结算
     * 该档的驻留时间，满载钉在最高频时当前档根本不累加 —— 实测八线程压满 4 秒总驻留只涨了
     * 3344ms（空载是 3313ms），占用率会永远卡在 10%。`/proc/stat` 对普通应用又是
     * Permission denied，所以退到 cpuidle：一个核任一时刻只处在一个 idle 状态，
     * 跨核跨状态求和不会重复计时。
     *
     * 首轮没有基线只能回 null（UI 给 `—`），拿 0 冒充「空闲」会让人以为机器很凉。
     */
    internal fun cpuUsagePercent(coresOnline: Int?): Int? = runCatching {
        val now = SystemClock.elapsedRealtime()
        val idle = idleResidency()
        val previous = usageBaseline
        usageBaseline = now to idle
        if (idle <= 0L) return null
        val cores = coresOnline?.takeIf { it > 0 } ?: cpuDirs().size.takeIf { it > 0 } ?: return null
        // 累计空闲量不可能超过「核数 × 开机时长」，超出两倍就说明单位是微秒，判一次就锁存
        if (idleUnitDivisor == 1.0 && idle > now * cores * 2) idleUnitDivisor = 1000.0
        val (prevNow, prevIdle) = previous ?: return null
        val elapsed = now - prevNow
        if (elapsed <= 0L || elapsed > USAGE_WINDOW_MAX_MS) {
            // 切走页面或长时间后台之后再回来：窗口中间根本没采样，比值不可信，重置基线
            usageSmoothed = null
            return null
        }
        val idleDelta = (idle - prevIdle) / idleUnitDivisor
        // 计数被重置（重启）时是负数，这一轮不出数
        if (idleDelta < 0.0) {
            usageSmoothed = null
            return null
        }
        val coreTimeMs = elapsed.toDouble() * cores
        val raw = ((coreTimeMs - idleDelta) / coreTimeMs * 100.0).coerceIn(0.0, 100.0)
        // 一秒一跳的进度条太神经质，叠一层指数平滑
        val smoothed = usageSmoothed?.let { it + USAGE_EMA_ALPHA * (raw - it) } ?: raw
        usageSmoothed = smoothed
        smoothed.roundToInt().coerceIn(0, 100)
    }.getOrNull()

    /**
     * Σ 每核每个 idle 状态的累计驻留时间。单位由 [idleUnitDivisor] 归一到毫秒，
     * 这里返回原始累计值，让调用方自己判单位。
     */
    private fun idleResidency(): Long = cpuDirs().sumOf { core ->
        File(core, "cpuidle").listFiles { f -> f.isDirectory && IDLE_STATE_DIR.matches(f.name) }
            .orEmpty()
            .sumOf { state -> readSysfsLong("${state.path}/time") ?: 0L }
    }

    /** 只显示最高频 cluster 的当前频率：脚注一行放不下八个核 */
    private fun cpuClockText(): String? = runCatching {
        val pairs = cpuDirs().mapNotNull { dir ->
            val max = readSysfsLong("${dir.path}/cpufreq/cpuinfo_max_freq") ?: return@mapNotNull null
            val current = readSysfsLong("${dir.path}/cpufreq/scaling_cur_freq") ?: return@mapNotNull null
            max to current
        }
        if (pairs.isEmpty()) return null
        val top = pairs.maxOf { it.first }
        val current = pairs.filter { it.first == top }.maxOf { it.second }
        if (current <= 0L) null else String.format(Locale.US, "%.2f GHz", current / 1_000_000.0)
    }.getOrNull()

    private fun cpuDirs(): List<File> =
        File(CPU_ROOT).listFiles { f -> f.isDirectory && CPU_DIR.matches(f.name) }
            .orEmpty()
            .sortedBy { f -> f.name.removePrefix("cpu").toIntOrNull() ?: Int.MAX_VALUE }

    /**
     * 电池端**整机**功率。充电时整行不显示 —— 那时电流是充进去的，
     * 当负载功率报出来会大得离谱。
     *
     * 优先 `CURRENT_NOW × 电压`；部分 ROM 这个属性常年回 0 或回一个静态上限，
     * 所以必须校验区间，不合格才退到库仑计差值（并标成估算）。
     */
    private fun batteryPowerWatt(context: Context, intent: Intent?): Pair<Double, Boolean>? {
        intent ?: return null
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        if (status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL) return null
        if (intent.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) != 0) return null
        val millivolts = intent.getIntExtra(BatteryManager.EXTRA_VOLTAGE, 0).takeIf { it > 0 } ?: return null
        val volts = millivolts / 1000.0

        val microAmps = runCatching {
            (context.getSystemService(Context.BATTERY_SERVICE) as? BatteryManager)
                ?.getIntProperty(BatteryManager.BATTERY_PROPERTY_CURRENT_NOW)
        }.getOrNull() ?: 0
        // 有的 ROM 放电给负号；为 0 说明这个属性压根没开放
        val instantaneous = volts * abs(microAmps) / 1_000_000.0
        if (instantaneous in POWER_MIN_W..POWER_MAX_W) return instantaneous to false

        val counter = batteryProperty(context, BatteryManager.BATTERY_PROPERTY_CHARGE_COUNTER) ?: return null
        val now = SystemClock.elapsedRealtime()
        val previous = chargeBaseline
        chargeBaseline = now to counter.toLong()
        val (prevNow, prevCounter) = previous ?: return null
        val hours = (now - prevNow) / 3_600_000.0
        // 窗口太短的话库仑计分辨率不够，算出来是抖的
        if (hours < 0.005) return null
        val amps = (prevCounter - counter) / 1_000_000.0 / hours
        return (volts * amps).takeIf { it in POWER_MIN_W..POWER_MAX_W }?.to(true)
    }

    /** 读一个 sysfs 小文件：被 SELinux 挡、文件不存在、内容为空都统一回 null */
    private fun readSysfs(path: String): String? = runCatching {
        File(path).readText().trim().takeIf { it.isNotEmpty() }
    }.getOrNull()

    private fun readSysfsLong(path: String): Long? = readSysfs(path)?.toLongOrNull()

    /**
     * 系统属性读取。`SystemProperties` 属未公开 API，高 targetSdk 上可能被非 SDK 接口限制
     * 直接挡掉，反射失败就返回 null，由调用方降级到公开 API（Build.*）或整行隐藏。
     */
    @SuppressLint("PrivateApi")
    private fun prop(key: String): String? = runCatching {
        Class.forName("android.os.SystemProperties")
            .getMethod("get", String::class.java)
            .invoke(null, key) as? String
    }.getOrNull()?.let { tidy(it) }

    private fun hasFeature(context: Context, id: String): Boolean =
        runCatching { context.packageManager.hasSystemFeature(id) }.getOrDefault(false)
}
