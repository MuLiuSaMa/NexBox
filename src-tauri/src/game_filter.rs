//! 游戏启动时自动应用滤镜模块
//!
//! 后台每 2.5 秒轮询一次系统进程，当检测到内置/自定义名单中的游戏进程运行时：
//! - 自动开启当前选中的滤镜（复用 `display_filter::apply_filter_to_display`）
//! 当所有名单内游戏进程退出时：
//! - 自动恢复默认显示（仅关闭由自动任务开启的滤镜，不误关用户手动开启的滤镜）
//!
//! 参考 `optimization.rs` 的 ACE 自动检测模式（generation 代次控制线程生命周期 +
//! `app.store` 持久化配置）。

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use sysinfo::System;
use tauri_plugin_store::StoreExt;

use crate::display_filter;

// ─── 内置热门游戏名单（与 docs/game-auto-filter.md 保持一致） ───
// 进程名不区分大小写、无需 .exe 后缀；命中任意一个进程即触发

pub const BUILTIN_GAMES: &[(&str, &[&str])] = &[
    // 射击 / FPS
    ("三角洲行动", &["DeltaForceClient-Win64-Shipping"]),
    ("暗区突围无限", &["ABInfinite", "ABInfinite-Win64-Shipping"]),
    ("漫威争锋", &["Marvel-Win64-Shipping"]),
    ("潜行者 2", &["Stalker2-Win64-Shipping"]),
    ("绝地潜兵 2", &["helldivers2"]),
    ("无畏契约", &["VALORANT", "VALORANT-Win64-Shipping"]),
    ("CS2", &["cs2"]),
    ("CS:GO", &["csgo"]),
    ("APEX 英雄", &["r5apex"]),
    ("绝地求生 PUBG", &["TslGame"]),
    ("使命召唤：战区", &["cod", "ModernWarfare"]),
    ("守望先锋", &["Overwatch", "Overwatch2"]),
    ("堡垒之夜", &["FortniteClient-Win64-Shipping"]),
    ("彩虹六号：围攻", &["RainbowSix", "RainbowSix_BE"]),
    ("逃离塔科夫", &["EscapeFromTarkov"]),
    ("战地系列", &["bf1", "bfv", "bf2042"]),
    ("全境封锁", &["TheDivision", "TheDivision2"]),
    ("命运 2", &["destiny2"]),
    ("猎杀对决", &["HuntGame"]),
    ("星球大战：前线", &["starwarsbattlefrontii"]),
    ("光环：无限", &["HaloInfinite"]),
    ("泰坦陨落 2", &["Titanfall2"]),
    ("求生之路 2", &["left4dead2"]),
    ("地球防卫军 5", &["EDF5"]),
    ("无主之地 3", &["Borderlands3"]),
    ("无主之地 4", &["Borderlands4"]),
    ("毁灭战士：永恒", &["DOOMEternal"]),
    ("毁灭战士（2016）", &["DOOM"]),
    ("孤岛惊魂 6", &["farcry6"]),
    ("狙击精英 5", &["SniperElite5"]),
    ("深岩银河", &["DeepRockGalactic"]),
    ("行星边际 2", &["PlanetSide2"]),
    ("地铁：离去", &["MetroExodus"]),
    ("死亡循环", &["Deathloop"]),
    ("光环：士官长合集", &["MCC-Win64-Shipping"]),
    ("孤岛危机 3", &["Crysis3"]),
    ("生化危机 4 重制", &["re4"]),
    // MOBA / 对战
    ("英雄联盟", &["LeagueClient", "League of Legends"]),
    ("DOTA 2", &["dota2"]),
    ("王者荣耀 PC 版", &["HonorOfKings"]),
    ("决战！平安京", &["OnmyojiArena"]),
    ("虚荣", &["Vainglory"]),
    // 开放世界 / RPG
    ("崩坏：星穹铁道", &["StarRail"]),
    ("原神", &["GenshinImpact", "YuanShen"]),
    ("绝区零", &["ZenlessZoneZero"]),
    ("鸣潮", &["WutheringWaves"]),
    ("黑神话：悟空", &["b1-Win64-Shipping"]),
    ("艾尔登法环", &["eldenring"]),
    ("赛博朋克 2077", &["Cyberpunk2077"]),
    ("GTA V", &["GTA5"]),
    ("荒野大镖客 2", &["RDR2"]),
    ("巫师 3", &["witcher3"]),
    ("博德之门 3", &["bg3", "bg3_dx11"]),
    ("上古卷轴 5", &["SkyrimSE", "TESV"]),
    ("辐射 4", &["Fallout4"]),
    ("星空", &["Starfield"]),
    ("刺客信条系列", &["ACOdyssey", "ACValhalla", "AC Syndicate"]),
    ("塞尔达（模拟器）", &["ryujinx", "yuzu"]),
    ("幻兽帕鲁", &["Palworld"]),
    ("流放之路", &["PathOfExile", "PathOfExile_x64"]),
    ("暗黑破坏神 4", &["Diablo IV"]),
    ("魔兽世界", &["Wow", "WowClassic"]),
    ("最终幻想 14", &["ffxiv_dx11"]),
    ("命运方舟", &["LostArk"]),
    ("星际战甲", &["Warframe"]),
    ("怪物猎人", &["MonsterHunterWorld", "MonsterHunterRise"]),
    ("怪物猎人：荒野", &["MonsterHunterWilds"]),
    ("只狼：影逝二度", &["sekiro"]),
    ("对马岛之魂", &["GhostOfTsushima"]),
    ("匹诺曹的谎言", &["LiesOfP"]),
    ("堕落之主", &["LordsOfTheFallen"]),
    ("卧龙：苍天陨落", &["WoLong"]),
    ("龙之信条 2", &["Dragon's Dogma 2"]),
    ("最终幻想 7 重制版", &["FF7R"]),
    ("最终幻想 16", &["ff16"]),
    ("霍格沃茨之遗", &["HogwartsLegacy"]),
    ("原子之心", &["AtomicHeart"]),
    ("遗迹 2", &["Remnant2"]),
    ("仁王 2", &["nioh2"]),
    ("神界：原罪 2", &["DivinityOriginalSin2"]),
    ("极乐迪斯科", &["DiscoElysium"]),
    ("天国拯救 2", &["KingdomCome"]),
    ("如龙 8", &["Yakuza8"]),
    ("真三国无双：起源", &["DynastyWarriorsOrigins"]),
    ("艾尔登法环：夜王", &["EldenRingNightreign"]),
    // 动作 / 竞速 / 其他
    ("永劫无间", &["NarakaBladepoint"]),
    ("地平线 6", &["ForzaHorizon6"]),
    ("地平线 5", &["ForzaHorizon5"]),
    ("地平线 4", &["ForzaHorizon4"]),
    ("尘埃拉力赛 2.0", &["dirt2"]),
    ("欧洲卡车模拟 2", &["eurotrucks2"]),
    ("双人成行", &["It Takes Two"]),
    ("胡闹厨房 2", &["Overcooked2"]),
    ("泰拉瑞亚", &["Terraria"]),
    ("我的世界", &["javaw", "java", "Minecraft.Windows"]),
    ("星露谷物语", &["StardewValley"]),
    ("缺氧", &["OxygenNotIncluded"]),
    ("环世界", &["RimWorld"]),
    ("城市：天际线", &["Cities"]),
    ("文明 6", &["CivilizationVI"]),
    ("全面战争：战锤 3", &["warhammer3"]),
    ("帝国时代 4", &["AgeOfEmpires4"]),
    ("三国：全面战争", &["ThreeKingdoms"]),
    ("糖豆人", &["FallGuys_client"]),
    ("模拟人生 4", &["TS4_x64"]),
    ("中国式家长", &["ChineseParents"]),
    ("太吾绘卷", &["Taiwu"]),
    ("鬼谷八荒", &["TaleOfImmortal"]),
    ("戴森球计划", &["DysonSphereProgram"]),
    // 动作 / 冒险
    ("战神", &["GodOfWar"]),
    ("战神：诸神黄昏", &["GoWR"]),
    ("漫威蜘蛛侠", &["MarvelsSpiderMan"]),
    ("漫威蜘蛛侠 2", &["Spider-Man2"]),
    ("地平线：零之曙光", &["HorizonZeroDawn"]),
    ("地平线：西之绝境", &["HorizonForbiddenWest"]),
    ("最后生还者 1", &["TheLastOfUs"]),
    ("神秘海域：盗贼遗产", &["Uncharted4"]),
    ("古墓丽影：暗影", &["ShadowOfTheTombRaider"]),
    ("星球大战绝地：幸存者", &["StarWarsJediSurvivor"]),
    ("星球大战绝地：陨落的武士团", &["StarWarsJediFallenOrder"]),
    ("蝙蝠侠：阿卡姆骑士", &["BatmanArkhamKnight"]),
    ("消逝的光芒 2", &["DyingLight2"]),
    ("往日不再", &["Days Gone"]),
    ("死亡空间", &["DeadSpace"]),
    ("心灵杀手 2", &["AlanWake2"]),
    ("控制", &["Control"]),
    ("羞辱 2", &["Dishonored2"]),
    ("看门狗 2", &["WatchDogs2"]),
    ("生化危机：村庄", &["re8"]),
    ("双影奇境", &["SplitFiction"]),
    ("死亡搁浅", &["DeathStranding"]),
    ("刺客信条：影", &["ACShadows"]),
    // 策略 / 模拟
    ("文明 7", &["CivilizationVII"]),
    ("群星", &["Stellaris"]),
    ("十字军之王 3", &["CK3"]),
    ("维多利亚 3", &["Victoria3"]),
    ("钢铁雄心 4", &["HOI4"]),
    ("城市：天际线 2", &["Cities2"]),
    ("冰汽时代 2", &["Frostpunk2"]),
    ("战锤 40K：星际战士 2", &["Warhammer40KSpaceMarine2"]),
    ("微软飞行模拟 2024", &["FlightSimulator"]),
    ("极限竞速：Motorsport", &["ForzaMotorsport"]),
    ("F1 24", &["F1_24"]),
    ("模拟农场 25", &["FarmingSimulator25"]),
    ("双点医院", &["TwoPointHospital"]),
    ("帝国时代 2：决定版", &["AoE2"]),
    ("全面战争：战锤 2", &["warhammer2"]),
    // 生存 / 合作
    ("英灵神殿", &["valheim"]),
    ("森林之子", &["SonsOfTheForest"]),
    ("夜族崛起", &["VRising"]),
    ("盗贼之海", &["SeaOfThieves"]),
    ("火箭联盟", &["RocketLeague"]),
    ("黎明杀机", &["DeadByDaylight"]),
    ("第五人格", &["IdentityV"]),
    ("动物派对", &["PartyAnimals"]),
    ("在我们中间", &["AmongUs"]),
    ("方舟：生存进化", &["ShooterGame"]),
    ("无人深空", &["NMS"]),
    ("人类一败涂地", &["HumanFallFlat"]),
    // 独立 / Roguelike
    ("哈迪斯 2", &["Hades2"]),
    ("吸血鬼幸存者", &["VampireSurvivors"]),
    ("咩咩启示录", &["CultOfTheLamb"]),
    ("潜水员戴夫", &["DaveTheDiver"]),
    ("巴拉特罗", &["Balatro"]),
    ("动物井", &["AnimalWell"]),
    ("死亡细胞", &["DeadCells"]),
    ("空洞骑士", &["HollowKnight"]),
    ("灵魂面甲", &["Soulmask"]),
    // 国产 / 其他
    ("燕云十六声", &["yysls"]),
    ("无限暖暖", &["InfinityNikki"]),
    ("尘白禁区", &["Snowbreak"]),
    ("暖雪", &["WarmSnow"]),
    ("剑网 3", &["JX3"]),
    ("逆水寒", &["nsh"]),
    ("仙剑奇侠传 7", &["Pal7", "Pal7-Win64-Shipping"]),
    ("幻塔", &["TowerOfFantasy"]),
    ("荒野乱斗", &["BrawlStars"]),
    ("古剑奇谭 3", &["Gujian3"]),
    ("卡拉彼丘", &["Strinova"]),
    // ─── 腾讯 (Tencent) ───
    ("逆战未来", &["NZM"]),
    ("逆战", &["NZ", "NZLauncher"]),
    ("穿越火线", &["CrossFire", "CF"]),
    ("地下城与勇士", &["DNF", "DNFCHINA"]),
    ("天涯明月刀", &["wuxia", "wuxia_client"]),
    ("剑灵", &["client"]),
    ("QQ飞车", &["GameApp"]),
    // ─── 米哈游 (miHoYo) ───
    ("崩坏 3", &["BH3"]),
    // ─── 完美世界 (Perfect World) ───
    ("诛仙世界", &["ZXSJ"]),
    ("完美世界（端游）", &["elementclient"]),
    // ─── 网易 (NetEase) ───
    ("梦幻西游", &["mhmain", "xyqsvc"]),
    ("新倩女幽魂", &["XQN"]),
    ("大话西游 2", &["xy2"]),
    // ─── 国产游戏（盛趣/世纪天成/畅游/腾讯/网易等） ───
    ("冒险岛", &["MapleStory"]),
    ("龙之谷", &["DragonNest"]),
    ("永恒之塔", &["Aion"]),
    ("泡泡堂", &["BNB"]),
    ("热血传奇", &["Mir2"]),
    ("传奇世界", &["Woool"]),
    ("跑跑卡丁车", &["KartRider"]),
    ("天龙八部", &["TLBB"]),
    ("使命召唤OL", &["codol"]),
    ("战争雷霆", &["aces", "aces_x64"]),
    ("反恐精英OL", &["cso"]),
    ("斗战神", &["Asura"]),
    ("荒野行动", &["hyxd"]),
    ("天下3", &["tw2launch", "tw2"]),
    ("我的世界中国版", &["MinecraftLauncher"]),
    ("光·遇", &["Sky"]),
    // 射击 / FPS
    ("生化危机 2 重制版", &["re2"]),
    ("生化危机 3 重制版", &["re3"]),
    ("生化危机 7", &["re7"]),
    ("战术小队", &["SquadGame"]),
    ("孤岛惊魂 5", &["farcry5"]),
    ("孤岛惊魂：原始杀戮", &["farCryPrimal"]),
    ("正当防卫 4", &["JustCause4"]),
    ("幽灵行者", &["GhostRunner"]),
    // RPG / 开放世界
    ("暗黑破坏神 2 重制版", &["D2R"]),
    ("黑暗之魂 3", &["DarkSouls3"]),
    ("黑暗之魂 2", &["DarkSoulsII"]),
    ("黑暗之魂：重制版", &["DARK SOULS REMASTERED"]),
    ("噬血代码", &["CodeVein"]),
    ("尼尔：机械纪元", &["NieRAutomata"]),
    ("破晓传说", &["Tales of Arise"]),
    ("绯红结系", &["Scarlet Nexus"]),
    ("底特律：化身为人", &["DetroitBecomeHuman"]),
    ("质量效应：传奇版", &["MassEffectLE"]),
    ("龙腾世纪：审判", &["DragonAgeInquisition"]),
    ("恐怖黎明", &["GrimDawn"]),
    ("女神异闻录 5 皇家版", &["P5R"]),
    ("女神异闻录 3 Reload", &["P3R"]),
    // 生存 / 合作
    ("木筏求生", &["Raft"]),
    ("森林", &["TheForest"]),
    ("绿色地狱", &["GreenHell"]),
    ("深海迷航", &["Subnautica"]),
    ("幸福工厂", &["Satisfactory"]),
    ("异星工厂", &["Factorio"]),
    ("恐鬼症", &["Phasmophobia"]),
    ("僵尸毁灭工程", &["ProjectZomboid", "ProjectZomboid64"]),
    ("腐蚀", &["rust"]),
    ("七日杀", &["7DaysToDie"]),
    ("地心护核者", &["CoreKeeper"]),
    // 策略 / 模拟 / 经营
    ("过山车之星", &["PlanetCoaster"]),
    ("动物园之星", &["PlanetZoo"]),
    ("侏罗纪世界：进化 2", &["JurassicWorldEvolution2"]),
    ("幽浮 2", &["XCOM2"]),
    ("亿万僵尸", &["TheyAreBillions"]),
    ("饥荒", &["dontstarve"]),
    ("饥荒联机版", &["dontstarvetogether"]),
    ("坎巴拉太空计划", &["KSP"]),
    ("传送门 2", &["portal2"]),
    ("半条命 2", &["hl2"]),
    ("欧陆风云 4", &["eu4"]),
    ("纪元 1800", &["Anno1800"]),
    ("海岛大亨 6", &["Tropico6"]),
    ("暗黑地牢", &["DarkestDungeon"]),
    // 动作 / 冒险
    ("逃生 2", &["Outlast2"]),
    ("地狱之刃 2", &["Hellblade2"]),
    ("星刃", &["StellarBlade"]),
    ("鬼泣 5", &["DevilMayCry5"]),
    ("师父", &["Sifu"]),
    ("掠食", &["Prey"]),
    ("德军总部：新秩序", &["WolfNewOrder"]),
    // 独立 / Roguelike
    ("杀戮尖塔", &["SlayTheSpire"]),
    ("以撒的结合", &["isaac-ng", "isaac"]),
    ("蔚蓝", &["Celeste"]),
    ("奥日与黑暗森林", &["ori"]),
    ("挺进地牢", &["EnterTheGungeon"]),
    ("星界边境", &["starbound"]),
    ("山羊模拟器", &["GoatSimulator"]),
    ("胡闹搬家", &["MovingOut"]),
    // 竞速 / 体育 / 对战
    ("极品飞车：不羁", &["NFSUnbound"]),
    ("极品飞车：热度", &["NFSHeat"]),
    ("NBA 2K24", &["NBA2K24"]),
    ("实况足球 eFootball", &["eFootball"]),
    ("风暴英雄", &["HeroesOfTheStorm"]),
    ("星际争霸 2", &["SC2"]),
    // 其他补充
    ("最终幻想 15", &["FFXV"]),
    ("迷失", &["Stray"]),
    ("异形：隔离", &["AlienIsolation"]),
    ("辐射 76", &["Fallout76"]),
    ("波西亚时光", &["Portia"]),
    ("火炬之光 2", &["Torchlight2"]),
    ("洛奇英雄传", &["Vindictus"]),
    ("黑色沙漠", &["BlackDesert64"]),
    ("奇异人生", &["LifeIsStrange"]),
    ("瘟疫传说：无罪", &["APlagueTale"]),
    ("拳皇 15", &["KOFXV"]),
    ("街头霸王 6", &["StreetFighter6"]),
    ("真人快打 11", &["MK11"]),
    ("猎人：荒野的召唤", &["theHunter"]),
    ("米塔", &["MiSideFull"]),
    ("在奇境", &["inZOI"]),
];

/// 轮询间隔（秒）
const POLL_INTERVAL_SECS: u64 = 2;

// ─── 全局状态 ───

/// 开关是否开启（内存态，供轮询线程与状态查询读取）
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 代次：开关切换时 +1，通知旧轮询线程退出
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 自动滤镜归属槽（替换原 AUTO_FILTER_ON 布尔）：记录哪个自动会话在哪台显示器上
/// 以哪个版本开启了滤镜、处于什么状态。条件登记/条件弃权/条件恢复见
/// [`display_filter::AutoOwnership`]。**锁顺序：归属锁 → 设备拓扑锁/状态锁 → 操作锁**，
/// 绝不反向（display_filter 的状态/操作锁路径从不回调归属锁）。
static AUTO_OWNERSHIP: Mutex<Option<display_filter::AutoOwnership>> = Mutex::new(None);
/// 自动会话计数器：每次自动开启尝试生成新会话标识。
static AUTO_SESSION: AtomicU64 = AtomicU64::new(0);
/// 自定义游戏名单内存缓存（None = 尚未从 store 加载）
static CUSTOM_GAMES: Mutex<Option<Vec<CustomGame>>> = Mutex::new(None);

// ─── 数据结构 ───

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CustomGame {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub process_names: Vec<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct GameFilterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub custom_games: Vec<CustomGame>,
}

/// 返回给前端的单个游戏条目（内置 + 自定义合并）
#[derive(serde::Serialize, Clone)]
pub struct GameEntry {
    pub id: String,
    pub name: String,
    pub process_names: Vec<String>,
    /// 是否内置名单（内置不可删除）
    pub is_builtin: bool,
}

#[derive(serde::Serialize)]
pub struct GameFilterStatus {
    pub enabled: bool,
    pub games: Vec<GameEntry>,
}

// ─── 配置持久化 ───

async fn load_persisted_config(app: &tauri::AppHandle) -> GameFilterConfig {
    match app.store("game_filter.json") {
        Ok(store) => {
            if let Some(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value::<GameFilterConfig>(value) {
                    return config;
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open game_filter store: {}", e);
        }
    }
    GameFilterConfig::default()
}

async fn save_persisted_config(app: &tauri::AppHandle, config: &GameFilterConfig) {
    match app.store("game_filter.json") {
        Ok(store) => {
            store.set("config", serde_json::to_value(config).unwrap());
            if let Err(e) = store.save() {
                log::error!("Failed to save game_filter config: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to open game_filter store for saving: {}", e);
        }
    }
}

/// 合并内置 + 自定义名单，返回前端条目列表
fn merge_games() -> Vec<GameEntry> {
    let mut games: Vec<GameEntry> = BUILTIN_GAMES
        .iter()
        .map(|(name, procs)| GameEntry {
            id: format!("builtin_{}", name),
            name: (*name).to_string(),
            process_names: procs.iter().map(|s| (*s).to_string()).collect(),
            is_builtin: true,
        })
        .collect();

    if let Ok(lock) = CUSTOM_GAMES.lock() {
        if let Some(custom) = lock.as_ref() {
            games.extend(custom.iter().map(|g| GameEntry {
                id: g.id.clone(),
                name: g.name.clone(),
                process_names: g.process_names.clone(),
                is_builtin: false,
            }));
        }
    }
    games
}

// ─── 进程匹配 ───

/// 去除 .exe 后缀（大小写不敏感）
fn strip_exe_suffix(s: &str) -> &str {
    if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
        &s[..s.len() - 4]
    } else {
        s
    }
}

/// 进程名与名单条目匹配（大小写不敏感、兼容带/不带 .exe）
fn process_matches(process_name: &str, entry_names: &[String]) -> bool {
    let proc = strip_exe_suffix(process_name);
    entry_names
        .iter()
        .any(|n| strip_exe_suffix(n).eq_ignore_ascii_case(proc))
}

/// 检测是否有名单内游戏在运行（复用 System 实例，避免每次重建）
/// 供 game_win_key 模块复用同一份游戏名单
pub(crate) fn any_game_running(system: &System) -> bool {
    !running_game_pids(system).is_empty()
}

/// 返回当前正在运行的滤镜名单游戏进程 PID 集合（内置 + 自定义名单）
/// 供 game_mode 模块复用同一份名单来豁免游戏进程
pub(crate) fn running_game_pids(system: &System) -> HashSet<u32> {
    let games = merge_games();
    let mut pids = HashSet::new();
    if games.is_empty() {
        return pids;
    }
    for (_, process) in system.processes() {
        let name = process.name().to_string();
        if games.iter().any(|g| process_matches(&name, &g.process_names)) {
            pids.insert(process.pid().as_u32());
        }
    }
    pids
}

// ─── 自动应用 / 恢复滤镜 ───

/// 检测到游戏启动：自动开启当前选中的滤镜。
/// `expected_generation`：当前轮询线程启动时的控制代次。登记在控制锁内将
/// `expected_generation` 与锁内当前代次比较——关闭后重开时旧代次即使当前
/// enabled=true 也会被拒绝。
fn auto_apply_filter(expected_generation: u64) {
    let idx = display_filter::get_active_index();

    let Some((idx, session, operation_generation)) = display_filter::auto_register_owned(
        idx,
        expected_generation,
        &AUTO_SESSION,
        &AUTO_OWNERSHIP,
    ) else {
        log::info!(
            "游戏滤镜自动应用[{}]: 条件开启被拒绝（功能已关闭/代次过期/滤镜已开启/退出中），自动任务不介入",
            idx
        );
        return;
    };

    match display_filter::apply_filter_to_display_if_current(idx, operation_generation) {
        Ok(display_filter::RunResult::Executed(())) => {
            // 条件登记：只有归属仍属于本会话、且操作版本未被用户推进，才置 Applied。
            // 旧任务晚到成功不得登记成当前自动会话；用户已手动接管时条件弃权。
            if display_filter::auto_mark_applied(&AUTO_OWNERSHIP, session, &display_filter::global_ops()) {
                log::info!("游戏滤镜自动应用[{}]: 检测到游戏运行，已自动开启当前滤镜", idx);
            } else {
                log::info!("游戏滤镜自动应用[{}]: 应用完成但归属已被新会话接管/用户已接管，不登记", idx);
            }
        }
        Ok(display_filter::RunResult::SkippedStale) => {
            // 已被更晚意图接管：只释放**自己的**记录，不得清除新会话的归属。
            display_filter::auto_release_if_owned(&AUTO_OWNERSHIP, session);
            log::info!("游戏滤镜自动应用[{}]: 应用已过期，跳过", idx);
        }
        Err(e) => {
            // 统一失败回滚（条件）：仅当归属/版本仍属本任务时原子关闭并走
            // 版本化串行恢复。按结果条件收尾（成功清记录由调用方统一处理）：
            // - Restored/DegradedCleared/NoRestoreNeeded：条件完成恢复，清归属。
            // - Superseded（关闭未提交/已过期）：条件弃权，不改新意图、不写屏。
            // - RestoreFailed：保留 Restoring 记录 + restore_pending 供逐轮重试，
            //   不得无条件清除。
            match display_filter::auto_rollback_failed_apply_global(&AUTO_OWNERSHIP, session) {
                display_filter::RollbackOutcome::Restored => {
                    display_filter::auto_finish_restore(&AUTO_OWNERSHIP, session);
                    log::info!("游戏滤镜自动应用[{}]: 应用失败，已精确恢复默认显示", idx);
                }
                display_filter::RollbackOutcome::DegradedCleared => {
                    display_filter::auto_finish_restore(&AUTO_OWNERSHIP, session);
                    log::warn!("游戏滤镜自动应用[{}]: 应用失败，已降级清除滤镜", idx);
                }
                display_filter::RollbackOutcome::NoRestoreNeeded => {
                    display_filter::auto_finish_restore(&AUTO_OWNERSHIP, session);
                    log::info!("游戏滤镜自动应用[{}]: 应用失败，无待恢复记录", idx);
                }
                display_filter::RollbackOutcome::Superseded => {
                    // 已被更晚意图接管：只释放自己的记录，不得清除新会话的归属。
                    display_filter::auto_release_if_owned(&AUTO_OWNERSHIP, session);
                }
                display_filter::RollbackOutcome::RestoreFailed(re) => {
                    log::error!(
                        "游戏滤镜自动应用[{}]: 应用失败，回滚恢复也失败（保留重试责任）: {}",
                        idx, re
                    );
                }
            }
            log::error!("游戏滤镜自动应用[{}]: 应用滤镜失败: {}", idx, e);
        }
    }
}

/// 游戏退出：若滤镜由自动任务开启，则恢复**原目标显示器**的默认显示。
fn auto_restore_filter() {
    // 会话标识：只处理当前归属所属的会话。
    let session = {
        let slot = AUTO_OWNERSHIP.lock().unwrap();
        match slot.as_ref() {
            Some(rec) => rec.session,
            None => return, // 无归属（滤镜非自动开启）：不干预
        }
    };
    // 归属核对 + 拓扑失效检测 + 原子条件关闭：全部在同一次归属锁持有内完成；
    // 恢复永远针对登记时的目标显示器，不再用恢复时的 get_active_index() 替代。
    match display_filter::auto_restore_decision_global(&AUTO_OWNERSHIP, session) {
        display_filter::AutoRestoreDecision::Proceed { display_idx, restore_generation } => {
            match display_filter::restore_display_default_if_current(display_idx, restore_generation) {
                Ok(display_filter::RunResult::Executed(_)) => {
                    display_filter::auto_finish_restore(&AUTO_OWNERSHIP, session);
                    log::info!("游戏滤镜自动恢复[{}]: 游戏已退出，已恢复默认显示", display_idx);
                }
                Ok(display_filter::RunResult::SkippedStale) => {
                    // 用户在恢复执行前接管（版本被推进）：丢弃旧归属，不覆盖用户选择。
                    display_filter::auto_release_if_owned(&AUTO_OWNERSHIP, session);
                    log::info!("游戏滤镜自动恢复[{}]: 恢复已过期（用户已接管），停止", display_idx);
                }
                Err(e) => {
                    // 恢复失败：保留 Restoring 归属（含 restore_generation）供逐轮重试；
                    // restore_pending 由恢复路径保留。
                    log::error!("游戏滤镜自动恢复[{}]: 恢复默认显示失败，保留重试责任: {}", display_idx, e);
                }
            }
        }
        display_filter::AutoRestoreDecision::Superseded => {
            log::info!("游戏滤镜自动恢复: 用户已接管滤镜状态，自动恢复弃权");
        }
        display_filter::AutoRestoreDecision::TargetGone => {
            log::warn!("游戏滤镜自动恢复: 目标显示器拓扑已变化（重排/移除），明确退出，不写任何显示器");
        }
        display_filter::AutoRestoreDecision::NotOwned => {
            log::info!("游戏滤镜自动恢复: 无归属或已被新自动会话接管，跳过");
        }
    }
}

/// 恢复责任重试：存在 Restoring 归属时逐轮重试版本化恢复，
/// 直到成功 / 用户接管（SkippedStale → 丢弃旧归属）/ 目标失效。
fn auto_retry_pending_restore() {
    let (session, display_idx, restore_generation) = {
        let slot = AUTO_OWNERSHIP.lock().unwrap();
        match slot.as_ref() {
            Some(rec) if rec.state == display_filter::AutoSessionState::Restoring => (
                rec.session,
                rec.display_idx,
                match rec.restore_generation {
                    Some(g) => g,
                    None => return,
                },
            ),
            _ => return,
        }
    };
    match display_filter::restore_display_default_if_current(display_idx, restore_generation) {
        Ok(display_filter::RunResult::Executed(_)) => {
            display_filter::auto_finish_restore(&AUTO_OWNERSHIP, session);
            log::info!("游戏滤镜自动恢复[{}]: 重试恢复成功", display_idx);
        }
        Ok(display_filter::RunResult::SkippedStale) => {
            // 用户已接管：停止旧重试，丢弃旧归属，不写屏。
            display_filter::auto_release_if_owned(&AUTO_OWNERSHIP, session);
            log::info!("游戏滤镜自动恢复[{}]: 重试已过期（用户已接管），停止", display_idx);
        }
        Err(e) => {
            // 仍失败：保留 Restoring 归属与 restore_pending，下轮继续重试。
            log::error!("游戏滤镜自动恢复[{}]: 重试恢复仍失败: {}", display_idx, e);
        }
    }
}

// ─── 后台轮询线程 ───

/// 轮询边缘决策（纯函数，便于测试）：只在“无游戏 → 有游戏”边缘自动开启；
/// 只要有任一名单内游戏仍在运行（running 保持 true），**不得提前恢复**——
/// 只有最后一个名单内游戏退出（true → false）才触发恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PollEdge {
    None,
    GameStarted,
    AllGamesExited,
}

fn poll_edge(prev_running: bool, now_running: bool) -> PollEdge {
    match (prev_running, now_running) {
        (false, true) => PollEdge::GameStarted,
        (true, false) => PollEdge::AllGamesExited,
        _ => PollEdge::None,
    }
}

fn game_filter_loop(generation: u64) {
    let mut system = System::new();
    let mut game_running = false;

    loop {
        // 控制状态读取在锁内完成（enabled + 代次一致时继续；否则退出）。
        let (still_enabled, still_current) = {
            let guard = display_filter::auto_registration_guard();
            (guard.enabled, guard.generation == generation)
        };
        if !still_current {
            break;
        }
        thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
        let (still_enabled, still_current) = {
            let guard = display_filter::auto_registration_guard();
            (guard.enabled, guard.generation == generation)
        };
        if !still_current {
            break;
        }
        if !still_enabled {
            // 归属处置已由关闭命令 set_game_filter_enabled 明确执行；
            // 轮询线程只需停止轮询并退出（代次检查已保证不复活）。
            game_running = false;
            continue;
        }

        system.refresh_processes();
        let running = any_game_running(&system);

        match poll_edge(game_running, running) {
            PollEdge::GameStarted => auto_apply_filter(generation),
            PollEdge::AllGamesExited => auto_restore_filter(),
            PollEdge::None => {}
        }
        game_running = running;
        // 上轮恢复失败的责任重试（仍会检查用户是否已接管）。
        auto_retry_pending_restore();
    }
}

// ─── 初始化 / 启动 ───

/// 应用启动时调用：恢复持久化配置并启动轮询线程
pub async fn init(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_persisted_config(&app).await;

    // 初始化内存缓存
    {
        let mut lock = CUSTOM_GAMES.lock().map_err(|e| e.to_string())?;
        *lock = Some(config.custom_games.clone());
    }

    // 控制状态初始化也在同一把控制锁内更新（与运行时开关命令一致）。
    // 注意：startup 时若配置为 false 且锁内当前已 false，则返回 None（无需启动线程）。
    let started = display_filter::auto_update_control_state(&AUTO_OWNERSHIP, config.enabled);
    // 展示值同步（仅状态查询用；登记依据完全走控制锁）。
    ENABLED.store(config.enabled, Ordering::Relaxed);

    if config.enabled {
        let gen = match started {
            Some((_, gen)) => gen,
            None => display_filter::auto_current_generation(),
        };
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| game_filter_loop(gen));
        });
        log::info!("游戏滤镜自动应用: 已根据持久化配置启动轮询");
    }
    Ok(())
}

// ─── Tauri 命令 ───

/// 获取开关状态 + 内置/自定义名单
#[tauri::command]
pub async fn get_game_filter_status(_app: tauri::AppHandle) -> Result<GameFilterStatus, String> {
    Ok(GameFilterStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        games: merge_games(),
    })
}

/// 开关切换：开启启动轮询线程，关闭停止（代次 +1 使旧线程退出）。
/// 控制状态（enabled + generation）的更新在**同一控制锁边界**内完成，
/// 与自动登记互斥；旧线程即使在锁外预检查后到达，也会在锁内因代次/启用
/// 状态不符而被拒绝登记。
#[tauri::command]
pub async fn set_game_filter_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    // 锁内检查当前状态、更新 enabled/generation、处置归属。
    let Some((keep_restoring, gen)) = display_filter::auto_update_control_state(&AUTO_OWNERSHIP, enabled)
    else {
        return Ok(());
    };
    // 展示值同步（仅状态查询用；登记依据完全走控制锁）。
    ENABLED.store(enabled, Ordering::Relaxed);

    // 关闭时若存在未完成的自动恢复（Restoring），安排恢复重试（锁已释放）。
    if !enabled && keep_restoring {
        log::info!("游戏滤镜自动应用: 关闭时存在未完成的自动恢复，安排恢复重试");
        auto_retry_pending_restore();
    }

    // 持久化（锁外 await）
    let config = GameFilterConfig {
        enabled,
        custom_games: {
            let lock = CUSTOM_GAMES.lock().map_err(|e| e.to_string())?;
            lock.as_ref().cloned().unwrap_or_default()
        },
    };
    save_persisted_config(&app, &config).await;

    if enabled {
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| game_filter_loop(gen));
        });
        log::info!("游戏滤镜自动应用: 已开启");
    } else {
        log::info!("游戏滤镜自动应用: 已关闭");
    }
    Ok(())
}

/// 添加自定义游戏
#[tauri::command]
pub async fn add_custom_game(
    app: tauri::AppHandle,
    name: String,
    process_names: Vec<String>,
) -> Result<GameFilterStatus, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("游戏名称不能为空".to_string());
    }
    let procs: Vec<String> = process_names
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if procs.is_empty() {
        return Err("至少填写一个进程名".to_string());
    }

    let mut custom = {
        let mut lock = CUSTOM_GAMES.lock().map_err(|e| e.to_string())?;
        let list = lock.get_or_insert_with(Vec::new);
        list.push(CustomGame {
            id: format!("custom_{}", chrono::Utc::now().timestamp_millis()),
            name: name.clone(),
            process_names: procs,
        });
        list.clone()
    };

    // 持久化
    let config = GameFilterConfig {
        enabled: ENABLED.load(Ordering::Relaxed),
        custom_games: std::mem::take(&mut custom),
    };
    save_persisted_config(&app, &config).await;
    log::info!("游戏滤镜自动应用: 已添加自定义游戏 {}", name);

    Ok(GameFilterStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        games: merge_games(),
    })
}

/// 删除自定义游戏
#[tauri::command]
pub async fn remove_custom_game(
    app: tauri::AppHandle,
    id: String,
) -> Result<GameFilterStatus, String> {
    let removed = {
        let mut lock = CUSTOM_GAMES.lock().map_err(|e| e.to_string())?;
        let list = lock.get_or_insert_with(Vec::new);
        let before = list.len();
        list.retain(|g| g.id != id);
        list.len() != before
    };
    if !removed {
        return Err("未找到要删除的自定义游戏".to_string());
    }

    let custom = {
        let lock = CUSTOM_GAMES.lock().map_err(|e| e.to_string())?;
        lock.as_ref().cloned().unwrap_or_default()
    };
    let config = GameFilterConfig {
        enabled: ENABLED.load(Ordering::Relaxed),
        custom_games: custom,
    };
    save_persisted_config(&app, &config).await;
    log::info!("游戏滤镜自动应用: 已删除自定义游戏 id={}", id);

    Ok(GameFilterStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        games: merge_games(),
    })
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    // ─── 多游戏运行时的轮询边缘决策 ───
    // 名单内多个游戏同时运行时 running 保持 true；只有最后一个游戏退出
    // （true → false）才触发恢复，不得提前恢复。
    #[test]
    fn poll_edge_multi_game_no_early_restore() {
        assert_eq!(poll_edge(false, false), PollEdge::None, "无游戏：不动作");
        assert_eq!(poll_edge(false, true), PollEdge::GameStarted, "首个游戏启动：自动开启");
        // 多个游戏同时运行：running 保持 true → 不提前恢复。
        assert_eq!(poll_edge(true, true), PollEdge::None, "仍有游戏运行：不得提前恢复");
        assert_eq!(poll_edge(true, false), PollEdge::AllGamesExited, "最后一个游戏退出：恢复");
    }

    // ─── 功能关闭时的归属处置 ───
    // Applied/Applying 彻底释放自动控制（效果保留，沿用旧版清标志语义；在途 apply
    // 完成时 auto_mark_applied 因槽位为 None 拒绝登记，不留残留、不误删新会话）；
    // Restoring 保留（恢复责任不随开关消失）。用与生产共用的
    // auto_update_control_state_with 验证关闭处置。
    #[test]
    fn disable_keeps_restoring_but_drops_other_ownership() {
        let make = |session, state, restore_gen| display_filter::AutoOwnership {
            session,
            display_idx: 0,
            device_name: "MON0".to_string(),
            display_count: 1,
            operation_generation: 7,
            state,
            restore_generation: restore_gen,
        };

        let applying: display_filter::AutoOwnershipSlot =
            std::sync::Mutex::new(Some(make(1, display_filter::AutoSessionState::Applying, None)));
        let state = std::sync::Mutex::new(display_filter::AutoControlState { enabled: true, generation: 1 });
        let guard = state.lock().unwrap();
        let (keep_restoring, _) = display_filter::auto_update_control_state_with(guard, &applying, false)
            .expect("状态变化必须返回更新");
        assert!(!keep_restoring, "Applying 释放自动控制");
        assert!(applying.lock().unwrap().is_none(), "关闭后不得残留 Applying 记录");

        let applied: display_filter::AutoOwnershipSlot =
            std::sync::Mutex::new(Some(make(1, display_filter::AutoSessionState::Applied, None)));
        let state = std::sync::Mutex::new(display_filter::AutoControlState { enabled: true, generation: 1 });
        let guard = state.lock().unwrap();
        let (keep_restoring, _) = display_filter::auto_update_control_state_with(guard, &applied, false)
            .expect("状态变化必须返回更新");
        assert!(!keep_restoring, "Applied 释放自动控制");
        assert!(applied.lock().unwrap().is_none(), "已生效效果保留，但自动控制权移交用户");

        let none: display_filter::AutoOwnershipSlot = std::sync::Mutex::new(None);
        let state = std::sync::Mutex::new(display_filter::AutoControlState { enabled: true, generation: 1 });
        let guard = state.lock().unwrap();
        let (keep_restoring, _) = display_filter::auto_update_control_state_with(guard, &none, false)
            .expect("状态变化必须返回更新");
        assert!(!keep_restoring, "空槽位无需处置");

        let restoring: display_filter::AutoOwnershipSlot =
            std::sync::Mutex::new(Some(make(1, display_filter::AutoSessionState::Restoring, Some(9))));
        let state = std::sync::Mutex::new(display_filter::AutoControlState { enabled: true, generation: 1 });
        let guard = state.lock().unwrap();
        let (keep_restoring, _) = display_filter::auto_update_control_state_with(guard, &restoring, false)
            .expect("状态变化必须返回更新");
        assert!(keep_restoring, "Restoring 归属必须保留");
        assert!(restoring.lock().unwrap().is_some(), "不得在恢复完成前删除唯一归属记录");
    }
}
