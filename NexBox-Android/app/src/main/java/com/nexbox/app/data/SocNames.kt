package com.nexbox.app.data

/**
 * SoC 代号 → 商用名（骁龙 / 天玑 / 麒麟 …）。
 *
 * `Build.SOC_MODEL` 给的是部件号（SM7435、MT6991），用户认识的是发布会上的名字。
 * 表里只收有公开资料能核对的对应关系，**宁可退回显示原始代号也不猜**：
 * 骁龙 7 系本身就命名混乱（SM7435 是 7s Gen 2、SM7550 才是 7 Gen 3、SM7635 的
 * -AB/-AC 两个后缀还分别是 7s Gen 3 和 7s Gen 4），猜错比不显示更容易误导人。
 */
object SocNames {

    /** 完全匹配（含修订后缀）：同一无数字在不同后缀上会跨到不同商用名的，必须走这里 */
    private val EXACT = mapOf(
        "SM8350-AC" to "骁龙 888+",
        "SM8250-AB" to "骁龙 865+",
        "SM8150-AC" to "骁龙 855+",
        "SM7325-AE" to "骁龙 778G+",
        "SM7635-AC" to "骁龙 7s Gen 4",
        "SM6375-AC" to "骁龙 6s Gen 3",
        "SM6115-AC" to "骁龙 6s 4G Gen 1",
        // 联发科：后缀字母属于型号本身，不是修订版
        "MT6983-V" to "天玑 9000+",
        "MT6985-Z" to "天玑 9200+",
        "MT6895-T" to "天玑 8100",
        "MT6895-Z" to "天玑 8100 Ultra",
        "MT6896-T" to "天玑 8250",
        "MT6893-Z" to "天玑 1300",
        // 老高通的后缀变体（SD678 = SM6150-AC、730G = SM7150-AB、768G = SM7250-AC）
        "SM6150-AC" to "骁龙 678",
        "SM7150-AB" to "骁龙 730G",
        "SM7250-AC" to "骁龙 768G",
        "SM6225-AD" to "骁龙 685",
        "SM7325-AZ" to "骁龙 782G",
        // 麒麟的后缀字母也属于型号本身（9000E / 9000L / 9010S）
        "KIRIN9000-E" to "麒麟 9000E",
        "KIRIN9000-L" to "麒麟 9000L",
        "KIRIN9010-S" to "麒麟 9010S",
    )

    /** 基号匹配：去掉修订后缀（SM8650-AB / -AC / -Q-AB 都是 8 Gen 3） */
    private val BASE = mapOf(
        // 2026 旗舰：部件号来自高通公布的命名（快科技），天玑 9600 Pro 来自 Geekbench 实机（vivo V2610）
        "SM8950" to "第六代骁龙 8 至尊版",
        "SM8975" to "第六代骁龙 8 超级至尊版",
        "MT6995" to "天玑 9600 Pro",
        "SM8850" to "第五代骁龙 8 至尊版",
        "SM8750" to "骁龙 8 至尊版",
        "SM8735" to "骁龙 8s Gen 4",
        "SM8650" to "骁龙 8 Gen 3",
        "SM8635" to "骁龙 8s Gen 3",
        "SM8550" to "骁龙 8 Gen 2",
        "SM8475" to "骁龙 8+ Gen 1",
        "SM8450" to "骁龙 8 Gen 1",
        "SM8350" to "骁龙 888",
        "SM8250" to "骁龙 865",
        "SM8150" to "骁龙 855",
        "SDM845" to "骁龙 845",
        "MSM8998" to "骁龙 835",
        "SM7750" to "骁龙 7 Gen 4",
        "SM7675" to "骁龙 7+ Gen 3",
        "SM7635" to "骁龙 7s Gen 3",
        "SM7550" to "骁龙 7 Gen 3",
        "SM7475" to "骁龙 7+ Gen 2",
        "SM7450" to "骁龙 7 Gen 1",
        "SM7435" to "骁龙 7s Gen 2",
        "SM7350" to "骁龙 780G",
        // 778G / 778G+ / 782G 同为 SM7325 基号的不同后缀，未细分的变体统一标到 778G
        "SM7325" to "骁龙 778G",
        "SM6650" to "骁龙 6 Gen 4",
        "SM6475" to "骁龙 6 Gen 3",
        "SM4635" to "骁龙 4s Gen 2",
        "MT6991" to "天玑 9400",
        "MT6989" to "天玑 9300",
        "MT6985" to "天玑 9200",
        "MT6983" to "天玑 9000",
        "MT6899" to "天玑 8400",
        "MT6897" to "天玑 8300",
        "MT6896" to "天玑 8200",
        "MT6895" to "天玑 8000",
        "MT6893" to "天玑 1200",
        "MT6886" to "天玑 7200",
        "MT6878" to "天玑 7300",
        // 华为麒麟：鸿蒙/EMUI 固件普遍不带 ro.soc.model（或只写裸家族名 "Kirin"），
        // 型号要从 ro.board.platform / ro.hardware 上的 kirinXXXX 拿
        "KIRIN9020" to "麒麟 9020",
        "KIRIN9010" to "麒麟 9010",
        "KIRIN9000" to "麒麟 9000",
        "KIRIN990" to "麒麟 990",
        "KIRIN985" to "麒麟 985",
        "KIRIN980" to "麒麟 980",
        "KIRIN970" to "麒麟 970",
        "KIRIN960" to "麒麟 960",
        "KIRIN820" to "麒麟 820",
        "KIRIN810" to "麒麟 810",
        "KIRIN8000" to "麒麟 8000",
        "KIRIN710" to "麒麟 710",
        // 高通 4G/中端老平台：Android 11 及以下没有 SOC_MODEL，部件号
        // 只能从 cpuinfo Hardware / soc0 machine / ro.board.platform 拿
        "SDM845" to "骁龙 845",
        "SDM710" to "骁龙 710",
        "SDM670" to "骁龙 670",
        "SDM660" to "骁龙 660",
        "SDM636" to "骁龙 636",
        "SDM625" to "骁龙 625",
        "SM6125" to "骁龙 665",
        "SM6150" to "骁龙 675",
        "SM6115" to "骁龙 662",
        "SM6225" to "骁龙 680",
        "SM6325" to "骁龙 695",
        "SM7125" to "骁龙 720G",
        "SM7150" to "骁龙 730",
        "SM4250" to "骁龙 460",
        "SM4350" to "骁龙 480",
        "MSM8996" to "骁龙 820",
        // 联发科老平台，部件号来自 ro.board.platform / ro.mediatek.platform
        "MT6883" to "天玑 1000",
        "MT6877" to "天玑 900",
        "MT6875" to "天玑 820",
        "MT6873" to "天玑 800",
        "MT6853" to "天玑 720",
        "MT6833" to "天玑 700",
        "MT6765" to "Helio P35",
        "MT6762" to "Helio P22",
        // 天玑 9500：Geekbench 实机部件号 MT6993（vivo X300 Pro 等）
        "MT6993" to "天玑 9500",
        // 骁龙 6/4 系中低端的公开部件号（回退链从平台属性拿到时用）
        "SM6450" to "骁龙 6 Gen 1",
        "SM4375" to "骁龙 4 Gen 1",
        "SM4450" to "骁龙 4 Gen 2",
        // Helio：Geekbench / 内核资料核对的几个常见型号
        "MT6789" to "Helio G99",
        "MT6781" to "Helio G96",
    )

    /**
     * 部件号/平台代号 → 商用名，按整串精确匹配（S5E、XRING 这类数字不在尾部的
     * 部件号套不进部件号正则，平台代号则根本没有数字）。只收公开资料能核对的。
     */
    private val CODE = mapOf(
        // 高通平台代号：老固件 ro.board.platform 上经常给代号而非部件号；
        // 一码多芯的（bengal 同时覆盖 460/662/480）宁可放弃也不猜
        "SUN" to "骁龙 8 至尊版",
        "PINEAPPLE" to "骁龙 8 Gen 3",
        "PARROT" to "骁龙 7s Gen 2",
        "LAHAINA" to "骁龙 888",
        "TARO" to "骁龙 8 Gen 1",
        "KONA" to "骁龙 865",
        "MSMNILE" to "骁龙 855",
        "LITO" to "骁龙 730",
        "ATOLL" to "骁龙 675",
        "TRINKET" to "骁龙 665",
        "SDMMAGPIE" to "骁龙 660",
        // 三星猎户座：ro.soc.model 给的是 S5E 部件号（TechInsights 拆解 / NamuWiki 核对，
        // 2000 系规律是倒数第二位对应代际：25→2200、35→2300、45→2400、55→2500、65→2600）
        "S5E9925" to "Exynos 2200",
        "S5E9935" to "Exynos 2300",
        "S5E9945" to "Exynos 2400",
        "S5E9955" to "Exynos 2500",
        "S5E9965" to "Exynos 2600",
        "S5E8825" to "Exynos 1280",
        "S5E8535" to "Exynos 1330",
    )

    /** 已经写了商用名的固件，只把厂商前缀换成本地叫法 */
    private val ALREADY_NAMED = listOf(
        Regex("(?i)^dimensity[\\s_-]*(.*)") to "天玑 ",
        Regex("(?i)^snapdragon[\\s_-]*(.*)") to "骁龙 ",
        Regex("(?i)^(?:his)?kirin[\\s_-]*(.*)") to "麒麟 ",
        Regex("(?i)^exynos[\\s_-]*(.*)") to "Exynos ",
        Regex("(?i)^tensor[\\s_-]*(.*)") to "Google Tensor ",
        // 展锐的 T7xx/T8xx 本身就是商品名，只补个厂商前缀
        Regex("(?i)^(t[68]\\d{2})$") to "展锐 ",
    )

    /**
     * SoC 厂商名的中文写法。`Build.SOC_MANUFACTURER` 给的是注册名，高通中端芯片普遍报
     * “QTI”（Qualcomm Technologies 的缩写），直接显示会让用户以为是杂牌。
     */
    fun vendorName(vendor: String?): String? {
        val raw = vendor?.trim()?.takeIf { it.isNotEmpty() && !it.equals("unknown", true) } ?: return null
        val key = raw.lowercase().filter { it.isLetterOrDigit() }
        return when {
            key.startsWith("qualcomm") || key == "qti" || key.startsWith("qcom") -> "高通"
            key.startsWith("mediatek") || key == "mtk" -> "联发科"
            key.startsWith("samsung") || key.startsWith("exynos") -> "三星"
            key.startsWith("apple") -> "苹果"
            key.startsWith("hisilicon") || key.startsWith("huawei") || key.startsWith("kirin") -> "华为海思"
            key.startsWith("xiaomi") || key.startsWith("xring") -> "小米玄戒"
            key.startsWith("unisoc") || key.startsWith("spreadtrum") -> "紫光展锐"
            key.startsWith("google") || key.startsWith("tensor") -> "Google"
            key.startsWith("intel") -> "英特尔"
            key.startsWith("amd") -> "AMD"
            else -> raw
        }
    }

    /** 高通平台代号（与 CODE 里高通段对应）：ro.soc.manufacturer 缺失时认厂商用 */
    private val QUALCOMM_CODENAMES =
        setOf("sun", "pineapple", "parrot", "lahaina", "taro", "kona", "msmnile",
            "lito", "atoll", "trinket", "sdmmagpie")

    /**
     * 从部件号/平台代号反推厂商。Android 11 及以下没有 `ro.soc.manufacturer`
     * （与 [displayName] 的缺口同源），这时厂商只能从型号本身认。
     */
    fun vendorFromModel(socModel: String?): String? {
        val key = socModel?.trim()?.lowercase()?.filter { it.isLetterOrDigit() } ?: return null
        if (key.isEmpty()) return null
        return when {
            key.startsWith("kirin") || key.startsWith("hisi") || Regex("^hi\\d").containsMatchIn(key) -> "华为海思"
            key.startsWith("xring") -> "小米玄戒"
            key.startsWith("sm") || key.startsWith("sdm") || key.startsWith("msm") ||
                key.startsWith("qcom") || key.startsWith("qti") || key in QUALCOMM_CODENAMES -> "高通"
            Regex("^mt\\d").containsMatchIn(key) || key.startsWith("mediatek") -> "联发科"
            key.startsWith("exynos") || key.startsWith("universal") || key.startsWith("s5e") -> "三星"
            Regex("^(sc|sp|ums)\\d").containsMatchIn(key) || Regex("^t[678]\\d\\d$").containsMatchIn(key) -> "紫光展锐"
            else -> null
        }
    }

    /**
     * @param socModel [android.os.Build.SOC_MODEL] 之类的原始代号
     * @return 商用名；识别不出返回 null，由调用方退回显示原始代号
     */
    fun displayName(socModel: String?): String? {
        val raw = socModel?.trim()?.takeIf { it.isNotEmpty() } ?: return null

        // 0) 小米玄戒：型号用字母 O（O1/O3），固件可能写 "XRING O1"、"xringo1"，统一成 "玄戒 O1"
        Regex("(?i)^xring[\\s_-]*(o\\d.*)").matchEntire(raw)?.let {
            return "玄戒 ${it.groupValues[1].trim().uppercase()}"
        }

        // 1) 固件已经直接写了商用名（"Dimensity 9300"、"Kirin9000"），只换前缀
        ALREADY_NAMED.forEach { (regex, prefix) ->
            regex.matchEntire(raw)?.let { match ->
                val tail = match.groupValues[1].trim()
                return if (tail.isEmpty()) null else prefix + tail
            }
        }

        val flat = raw.uppercase().filter { it.isLetterOrDigit() }

        // 2) 高通平台代号（"atoll"、"parrot"）：老固件只有代号没有部件号
        CODE[flat]?.let { return it }

        // 3) 部件号：拆成基号（SM7435）+ 修订后缀（AB）。
        //    数字限 3~4 位，否则 SM8750-3-AB 会把基号错拆成 SM87503
        val match = Regex("^([A-Z]+\\d{3,4})([A-Z0-9]*)$").find(flat) ?: return null
        val base = match.groupValues[1]
        val suffix = match.groupValues[2]
        if (suffix.isNotEmpty()) {
            // 后缀完整的精确匹配优先：SM7635-AB 与 SM7635-AC 是两个不同的商用名
            EXACT["$base-$suffix"]?.let { return it }
            // 联发科的字母后缀属于型号本身（MT6895ZB → 先按 MT6895Z 查）
            if (suffix.length > 1) EXACT["$base-${suffix[0]}"]?.let { return it }
        }
        return BASE[base]
    }
}
