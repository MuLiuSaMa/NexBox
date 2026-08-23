// 三角洲行动 · 随机装备生成 — 地图数据（复刻自 delta-roulette 工具）

export type RouletteDifficulty = "常规" | "机密" | "绝密" | "永夜";

export interface RouletteMap {
  id: number;
  map: string;
  difficulty: RouletteDifficulty;
  pic: string;
}

export const maps: RouletteMap[] = [
  { id: 1, map: "零号大坝", difficulty: "机密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-jimi.jpg" },
  { id: 1, map: "零号大坝", difficulty: "常规", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-changgui.jpg" },
  { id: 5, map: "潮汐监狱", difficulty: "绝密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/cxjy-juemi.jpg" },
  { id: 4, map: "航天基地", difficulty: "绝密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/htjd-juemi.jpg" },
  { id: 3, map: "巴克什", difficulty: "绝密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/bks-juemi.jpg" },
  { id: 2, map: "长弓溪谷", difficulty: "常规", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/cgxg-changgui.jpg" },
  { id: 2, map: "长弓溪谷", difficulty: "机密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/cgxg-jimi.jpg" },
  { id: 3, map: "巴克什", difficulty: "机密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/bks-jimi.jpg" },
  { id: 1, map: "零号大坝", difficulty: "永夜", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/lhdb-yongye.jpg" },
  { id: 4, map: "航天基地", difficulty: "机密", pic: "https://eo.oss.hengj.cn/one/DfGame/MapImages/htjd-jimi.jpg" },
];

/** 绝密/永夜 难度过滤（「只玩绝密」选项） */
export function isClassifiedDifficulty(d: RouletteDifficulty): boolean {
  return d === "绝密" || d === "永夜";
}