package com.nexbox.app.data

/**
 * 三角洲行动 · 随机装备生成数据，由 tools/convert-roulette-data.mjs 从 PC 端
 * src/data/delta-roulette 目录下的 .ts 数据文件自动转换（复刻自 delta-roulette 工具）。
 * 手改无意义——重新跑脚本会覆盖；数据源更新时改 PC 端再转一次。
 */

data class RouletteWeapon(
    val objectName: String,
    val pic: String,
    val fireMode: String,
    val caliber: String,
    val ammoPic: String?,
    val selectedAmmoGrade: Int,
)

/** 头盔 / 护甲共用：名称 + 图 + 防护等级 */
data class RouletteGear(
    val objectName: String,
    val pic: String,
    val protectLevel: Int,
)

data class RouletteMap(
    val id: Int,
    val map: String,
    /** 常规 / 机密 / 绝密 / 永夜 */
    val difficulty: String,
    val pic: String,
)

data class RouletteOperator(
    val id: Int,
    val name: String,
    val pic: String,
)

val rouletteWeapons: List<RouletteWeapon> = listOf(
    RouletteWeapon("SVD狙击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000004.png", "单发", "7.62x54", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x54mm.png", 6),
    RouletteWeapon("SV-98狙击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18060000007.png", "单发", "7.62x54", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x54mm.png", 5),
    RouletteWeapon("M16A4突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000014.png", "连发", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 5),
    RouletteWeapon("AK-12突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000018.png", "全自动", "5.45x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.45x39mm.png", 5),
    RouletteWeapon("PKM通用机枪", "https://playerhub.df.qq.com/playerhub/60004/object/18040000001.png", "全自动", "7.62x54", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x54mm.png", 4),
    RouletteWeapon("AKM突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000006.png", "全自动", "7.62x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x39mm.png", 5),
    RouletteWeapon("M1014霰弹枪", "https://playerhub.df.qq.com/playerhub/60004/object/18030000001.png", "单发", "12", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_12-Gauge.png", 2),
    RouletteWeapon("R93狙击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18060000008.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 6),
    RouletteWeapon("QSZ92G", "https://playerhub.df.qq.com/playerhub/60004/object/18070000002.png", "单发", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 1),
    RouletteWeapon("G3战斗步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000023.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 3),
    RouletteWeapon("SKS射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000006.png", "单发", "7.62x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x39mm.png", 3),
    RouletteWeapon("MK4冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000012.png", "连发", "4.6x30", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_4.6x30.png", 4),
    RouletteWeapon("QCQ171冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000011.png", "全自动", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 3),
    RouletteWeapon("MP7冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000010.png", "单发", "4.6x30", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_4.6x30mm.png", 4),
    RouletteWeapon("AUG突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000015.png", "全自动", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 4),
    RouletteWeapon("G17", "https://playerhub.df.qq.com/playerhub/60004/object/18070000010.png", "单发", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 2),
    RouletteWeapon("M249轻机枪", "https://playerhub.df.qq.com/playerhub/60004/object/18040000002.png", "全自动", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 2),
    RouletteWeapon("K416突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000013.png", "单发", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 3),
    RouletteWeapon("腾龙突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000038.png", "全自动", "5.8x42", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.8x42mm.png", 4),
    RouletteWeapon("AS Val突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000037.png", "单发", "9x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x39mm.png", 4),
    RouletteWeapon("93R", "https://playerhub.df.qq.com/playerhub/60004/object/18070000006.png", "连发", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 4),
    RouletteWeapon("勇士冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000009.png", "全自动", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 4),
    RouletteWeapon("M250通用机枪", "https://playerhub.df.qq.com/playerhub/60004/object/18040000003.png", "全自动", "6.8x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_6.8x51mm.png", 5),
    RouletteWeapon("M700狙击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18060000009.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 6),
    RouletteWeapon("M7战斗步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000016.png", "全自动", "6.8x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_6.8x51mm.png", 5),
    RouletteWeapon("SR9射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000008.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 2),
    RouletteWeapon("SG552突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000017.png", "全自动", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 5),
    RouletteWeapon("PSG-1射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000031.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 6),
    RouletteWeapon("QBZ95-1突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000008.png", "单发", "5.8x42", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.8x42mm.png", 4),
    RouletteWeapon("Vector冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000003.png", "连发", ".45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_45-ACP.png", 1),
    RouletteWeapon("SCAR-H战斗步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000021.png", "全自动", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 4),
    RouletteWeapon("M4A1突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000001.png", "全自动", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 4),
    RouletteWeapon("M1911", "https://playerhub.df.qq.com/playerhub/60004/object/18070000033.png", "单发", ".45 ACP", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_45-ACP.png", 2),
    RouletteWeapon("Marlin杠杆步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000032.png", "单发", ".45-70", "https://playerhub.df.qq.com/playerhub/60004/object/p_37290300001.png", 4),
    RouletteWeapon("M14射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000005.png", "全自动", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 2),
    RouletteWeapon("S12K霰弹枪", "https://playerhub.df.qq.com/playerhub/60004/object/18030000002.png", "单发", "12", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_12-Gauge.png", 1),
    RouletteWeapon("沙漠之鹰", "https://playerhub.df.qq.com/playerhub/60004/object/18070000004.png", "单发", ".50", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_50-AE.png", 3),
    RouletteWeapon("野牛冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000005.png", "单发", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 2),
    RouletteWeapon("M870霰弹枪", "https://playerhub.df.qq.com/playerhub/60004/object/18030000004.png", "单发", "12", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_12-Gauge.png", 3),
    RouletteWeapon(".357左轮", "https://playerhub.df.qq.com/playerhub/60004/object/18070000003.png", "单发", ".357", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_357-Magnum.png", 3),
    RouletteWeapon("AWM狙击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18060000011.png", "单发", ".338", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_338.png", 6),
    RouletteWeapon("K437突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000040.png", "单发", "7.62x35", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_300BLK.png", 5),
    RouletteWeapon("VSS射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000003.png", "全自动", "9x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x39mm.png", 4),
    RouletteWeapon("AKS-74U突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000010.png", "全自动", "5.45x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.45x39mm.png", 4),
    RouletteWeapon("P90冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000002.png", "全自动", "5.7x28", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.7x28mm.png", 4),
    RouletteWeapon("CAR-15突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000031.png", "单发", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 5),
    RouletteWeapon("SR-3M紧凑突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000008.png", "单发", "9x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x39mm.png", 5),
    RouletteWeapon("PTR-32突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000024.png", "全自动", "7.62x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x39mm.png", 5),
    RouletteWeapon("UZI冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000004.png", "全自动", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 1),
    RouletteWeapon("725双管霰弹枪", "https://playerhub.df.qq.com/playerhub/60004/object/18030000005.png", "单发", "12", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_12-Gauge.png", 1),
    RouletteWeapon("QJB201轻机枪", "https://playerhub.df.qq.com/playerhub/60004/object/18040000004.png", "全自动", "5.8x42", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.8x42mm.png", 3),
    RouletteWeapon("Mini-14射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000002.png", "单发", "5.56x45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.56x45mm.png", 5),
    RouletteWeapon("SR-25射手步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18050000007.png", "单发", "7.62x51", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_7.62x51mm.png", 6),
    RouletteWeapon("SMG-45冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000006.png", "全自动", ".45", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_45-ACP.png", 4),
    RouletteWeapon("ASh-12战斗步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000012.png", "全自动", "12.7x55", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_12.7x55mm.png", 5),
    RouletteWeapon("G18", "https://playerhub.df.qq.com/playerhub/60004/object/18070000005.png", "全自动", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 3),
    RouletteWeapon("KC17突击步枪", "https://playerhub.df.qq.com/playerhub/60004/object/18010000042.png", "单发", "5.45x39", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_5.45x39mm.png", 3),
    RouletteWeapon("MP5冲锋枪", "https://playerhub.df.qq.com/playerhub/60004/object/18020000001.png", "连发", "9x19", "https://playerhub.df.qq.com/playerhub/60004/object/gun/ammo/p_9x19mm.png", 1),
)

val rouletteArmors: List<RouletteGear> = listOf(
    RouletteGear("金刚防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050006001.png", 6),
    RouletteGear("DT-AVS防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050004003.png", 4),
    RouletteGear("泰坦防弹装甲", "https://playerhub.df.qq.com/playerhub/60004/object/11050006004.png", 6),
    RouletteGear("HMP特勤防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050004007.png", 4),
    RouletteGear("特里克MAS2.0装甲", "https://playerhub.df.qq.com/playerhub/60004/object/11050006003.png", 6),
    RouletteGear("Hvk快拆防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050003002.png", 3),
    RouletteGear("Hvk-2防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050005002.png", 5),
    RouletteGear("射手战术背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050003004.png", 3),
    RouletteGear("重型突击背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050005004.png", 5),
    RouletteGear("制式防弹背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050003001.png", 3),
    RouletteGear("MK-2战术背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050004004.png", 4),
    RouletteGear("突击手防弹背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050004002.png", 4),
    RouletteGear("通用战术背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050002004.png", 2),
    RouletteGear("摩托马甲", "https://playerhub.df.qq.com/playerhub/60004/object/11050001001.png", 1),
    RouletteGear("TG战术防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050002003.png", 2),
    RouletteGear("FS复合防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050005003.png", 5),
    RouletteGear("HT战术背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050002002.png", 2),
    RouletteGear("精英防弹背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050005001.png", 5),
    RouletteGear("安保防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050001002.png", 1),
    RouletteGear("TG-H防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050003003.png", 3),
    RouletteGear("轻型防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050001004.png", 1),
    RouletteGear("简易防刺服", "https://playerhub.df.qq.com/playerhub/60004/object/11050002001.png", 2),
    RouletteGear("HA-2防弹装甲", "https://playerhub.df.qq.com/playerhub/60004/object/11050006002.png", 6),
    RouletteGear("尼龙防弹衣", "https://playerhub.df.qq.com/playerhub/60004/object/11050001003.png", 1),
    RouletteGear("武士防弹背心", "https://playerhub.df.qq.com/playerhub/60004/object/11050004001.png", 4),
)

val rouletteHelmets: List<RouletteGear> = listOf(
    RouletteGear("DICH-1战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005003.png", 5),
    RouletteGear("复古摩托头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010002001.png", 2),
    RouletteGear("H70 夜视精英头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010006007.png", 6),
    RouletteGear("GN 久战重型夜视头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005010.png", 5),
    RouletteGear("MC201 头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010003004.png", 3),
    RouletteGear("H01 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010002002.png", 2),
    RouletteGear("GN 重型头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005004.png", 5),
    RouletteGear("D6 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010004001.png", 4),
    RouletteGear("安保头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010001002.png", 1),
    RouletteGear("DAS 防弹头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010003003.png", 3),
    RouletteGear("H07 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010003002.png", 3),
    RouletteGear("GT1 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010004004.png", 4),
    RouletteGear("DRO 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010002003.png", 2),
    RouletteGear("MC防弹头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010002004.png", 2),
    RouletteGear("H70 精英头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010006002.png", 6),
    RouletteGear("GT5 指挥官头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010006004.png", 6),
    RouletteGear("H09 防暴头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005002.png", 5),
    RouletteGear("防暴头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010003001.png", 3),
    RouletteGear("奔尼帽", "https://playerhub.df.qq.com/playerhub/60004/object/11010001003.png", 1),
    RouletteGear("户外棒球帽", "https://playerhub.df.qq.com/playerhub/60004/object/11010001004.png", 1),
    RouletteGear("GN 重型夜视头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005009.png", 5),
    RouletteGear("DICH-9重型头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010006003.png", 6),
    RouletteGear("老式钢盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010001001.png", 1),
    RouletteGear("Mask-1铁壁头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010005001.png", 5),
    RouletteGear("MHS 战术头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010004002.png", 4),
    RouletteGear("DICH 训练头盔", "https://playerhub.df.qq.com/playerhub/60004/object/11010004003.png", 4),
)

val rouletteMaps: List<RouletteMap> = listOf(
    RouletteMap(1, "零号大坝", "机密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-jimi.jpg"),
    RouletteMap(1, "零号大坝", "常规", "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-changgui.jpg"),
    RouletteMap(5, "潮汐监狱", "绝密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/cxjy-juemi.jpg"),
    RouletteMap(4, "航天基地", "绝密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/htjd-juemi.jpg"),
    RouletteMap(3, "巴克什", "绝密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/bks-juemi.jpg"),
    RouletteMap(2, "长弓溪谷", "常规", "https://eo.oss.hengj.cn/one/DfGame/MapImages/cgxg-changgui.jpg"),
    RouletteMap(2, "长弓溪谷", "机密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/cgxg-jimi.jpg"),
    RouletteMap(3, "巴克什", "机密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/bks-jimi.jpg"),
    RouletteMap(1, "零号大坝", "永夜", "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-yongye.jpg"),
    RouletteMap(4, "航天基地", "机密", "https://eo.oss.hengj.cn/one/DfGame/MapImages/htjd-jimi.jpg"),
)

val rouletteOperators: List<RouletteOperator> = listOf(
    RouletteOperator(30, "红狼", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000030.png"),
    RouletteOperator(25, "威龙", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000025.png"),
    RouletteOperator(40, "银翼", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000040.png"),
    RouletteOperator(38, "无名", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000038.png"),
    RouletteOperator(36, "蛊", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000036.png"),
    RouletteOperator(39, "疾风", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000039.png"),
    RouletteOperator(26, "骇爪", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000026.png"),
    RouletteOperator(28, "露娜", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000028.png"),
    RouletteOperator(41, "比特", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000041.png"),
    RouletteOperator(29, "牧羊人", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000029.png"),
    RouletteOperator(45, "蝶", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000045.png"),
    RouletteOperator(37, "深蓝", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000037.png"),
    RouletteOperator(27, "蜂医", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000027.png"),
    RouletteOperator(35, "乌鲁鲁", "https://playerhub.df.qq.com/playerhub/60004/object/p_88000000035.png"),
)

val rouletteTasks: List<String> = listOf(
    "寸土寸金：使用1X2安全箱",
    "原始宣告：开局交全部道具",
    "钢铁扳机：单次开枪必须清空弹匣",
    "痛觉麻痹：每分钟吃一颗止疼药",
    "化身太阳：枪械所有可装备战术道具的槽位全部安装爆闪手电",
    "心灵感应：全局闭麦并自言自语",
    "喋喋不休：话痨状态开麦",
    "危险快递：全局不搜索容器",
    "抱头鼠窜：丢包撤离点开放后使用丢包撤离",
    "空仓赴约：不带子弹进图",
    "腰射狂徒：枪械改装至最高腰射",
    "轻装上阵：允许自选防具，但不可大于3级",
    "无医可求：不带手术包进图",
    "拾荒专家：拾取所有搜索到的物品",
    "咬紧牙关：击杀第一个敌方干员前不使用任何药品、针剂及维修套件",
    "沉默是金：全局闭麦",
    "以彼之道：与第一个击杀的AI互换所有装备",
    "探天之瞳：枪械使用可装备的最高倍镜",
    "明镜如水：全局不使用烟雾弹",
    "战略撤退：接取局内任务后第一时间主动放弃",
    "两袖清风：撤离成功时收益为0",
    "战备封锁：卡战备进图",
    "负重特训：蛋白粉负重进图",
    "痛觉免疫：全局不使用手术包",
    "焰舞豁免：不可击杀任何阵营的喷火兵",
    "重力桎梏：全局不可跳跃",
    "玻璃大炮：允许自选武器与弹药，但只可装备1级防具",
    "举步维艰：全局不可疾跑",
    "神秘来客：全局不开大",
    "黑暗交易：击杀一名敌方干员后必须与其互换装备，然后直接撤离",
)

/** 按武器图 URL 中的 /1807 段判断手枪（「不用手枪」过滤用） */
fun isPistolWeapon(pic: String): Boolean = pic.contains("/1807")

/** 绝密/永夜 难度（「只玩绝密」过滤用） */
fun isClassifiedDifficulty(difficulty: String): Boolean =
    difficulty == "绝密" || difficulty == "永夜"
