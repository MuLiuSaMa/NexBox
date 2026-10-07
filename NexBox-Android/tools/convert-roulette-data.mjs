// 一次性转换脚本：把 PC 端 src/data/delta-roulette/*.ts 的数据转成 Kotlin 常量。
// 用法：node tools/convert-roulette-data.mjs
import { readFileSync, writeFileSync } from "node:fs";

const SRC = "D:/NexBox/src/data/delta-roulette";
const OUT = "D:/NexBox/NexBox-Android/app/src/main/java/com/nexbox/app/data/DeltaRouletteData.kt";

// TS 文件里数组是合法的 JS 表达式（无引号键 + 尾逗号），截取后直接 eval
function extractArray(file) {
  const text = readFileSync(`${SRC}/${file}`, "utf8");
  const start = text.indexOf("[", text.indexOf("="));
  const end = text.lastIndexOf("]");
  return new Function(`return ${text.slice(start, end + 1)}`)();
}

const weapons = extractArray("weapons.ts");
const armors = extractArray("armors.ts");
const helmets = extractArray("helmets.ts");
const maps = extractArray("maps.ts");
const operators = extractArray("operators.ts");
const tasks = extractArray("tasks.ts");

const s = (v) => JSON.stringify(v ?? "");

const weaponLines = weapons
  .map(
    (w) =>
      `    RouletteWeapon(${s(w.objectName)}, ${s(w.pic)}, ${s(w.fireMode)}, ${s(w.caliber)}, ` +
      `${w.ammoPic ? s(w.ammoPic) : "null"}, ${w.selectedAmmoGrade}),`,
  )
  .join("\n");
const armorLines = armors
  .map((a) => `    RouletteGear(${s(a.objectName)}, ${s(a.pic)}, ${a.protectLevel}),`)
  .join("\n");
const helmetLines = helmets
  .map((h) => `    RouletteGear(${s(h.objectName)}, ${s(h.pic)}, ${h.protectLevel}),`)
  .join("\n");
const mapLines = maps
  .map((m) => `    RouletteMap(${m.id}, ${s(m.map)}, ${s(m.difficulty)}, ${s(m.pic)}),`)
  .join("\n");
const operatorLines = operators
  .map((o) => `    RouletteOperator(${o.id}, ${s(o.name)}, ${s(o.pic)}),`)
  .join("\n");
const taskLines = tasks.map((t) => `    ${s(t)},`).join("\n");

const kotlin = `package com.nexbox.app.data

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
${weaponLines}
)

val rouletteArmors: List<RouletteGear> = listOf(
${armorLines}
)

val rouletteHelmets: List<RouletteGear> = listOf(
${helmetLines}
)

val rouletteMaps: List<RouletteMap> = listOf(
${mapLines}
)

val rouletteOperators: List<RouletteOperator> = listOf(
${operatorLines}
)

val rouletteTasks: List<String> = listOf(
${taskLines}
)

/** 按武器图 URL 中的 /1807 段判断手枪（「不用手枪」过滤用） */
fun isPistolWeapon(pic: String): Boolean = pic.contains("/1807")

/** 绝密/永夜 难度（「只玩绝密」过滤用） */
fun isClassifiedDifficulty(difficulty: String): Boolean =
    difficulty == "绝密" || difficulty == "永夜"
`;

writeFileSync(OUT, kotlin);
console.log(
  `weapons=${weapons.length} armors=${armors.length} helmets=${helmets.length} maps=${maps.length} operators=${operators.length} tasks=${tasks.length} -> ${OUT}`,
);
