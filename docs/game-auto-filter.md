# 游戏滤镜自动应用名单（内置清单）

> 本文档与 `src-tauri/src/game_filter.rs` 中的 `BUILTIN_GAMES` 保持一致。

## 功能简介

后台每 2 秒轮询一次系统进程，当检测到内置/自定义名单中的游戏进程运行时：
- 自动开启当前选中的滤镜（复用 `display_filter::apply_filter_to_display`）；
- 当所有名单内游戏进程退出时，自动恢复默认显示（仅关闭由自动任务开启的滤镜，不误关用户手动开启的滤镜）。

进程状态需连续两次轮询确认，且两次自动切换之间至少间隔 10 秒；恢复失败会按指数退避重试，最多 5 次后停止自动重试并保留原始校色数据。

进程名匹配规则：**不区分大小写、无需 `.exe` 后缀、精确匹配**（命中任一个进程即触发）。

该名单同样被「竞技档豁免」与「窗口键识别」功能复用。

## 内置名单

### 射击 / FPS
- 三角洲行动（DeltaForceClient-Win64-Shipping）
- 暗区突围无限（ABInfinite / ABInfinite-Win64-Shipping）
- 漫威争锋（Marvel-Win64-Shipping）
- 潜行者 2（Stalker2-Win64-Shipping）
- 绝地潜兵 2（helldivers2）
- 无畏契约（VALORANT / VALORANT-Win64-Shipping）
- CS2（cs2）
- CS:GO（csgo）
- APEX 英雄（r5apex）
- 绝地求生 PUBG（TslGame）
- 使命召唤：战区（cod / ModernWarfare）
- 守望先锋（Overwatch / Overwatch2）
- 堡垒之夜（FortniteClient-Win64-Shipping）
- 彩虹六号：围攻（RainbowSix / RainbowSix_BE）
- 逃离塔科夫（EscapeFromTarkov）
- 战地系列（bf1 / bfv / bf2042）
- 全境封锁（TheDivision / TheDivision2）
- 命运 2（destiny2）
- 猎杀对决（HuntGame）
- 星球大战：前线（starwarsbattlefrontii）
- 光环：无限（HaloInfinite）
- 泰坦陨落 2（Titanfall2）
- 求生之路 2（left4dead2）
- 地球防卫军 5（EDF5）
- 无主之地 3（Borderlands3）
- 无主之地 4（Borderlands4）
- 毁灭战士：永恒（DOOMEternal）
- 毁灭战士（2016）（DOOM）
- 孤岛惊魂 6（farcry6）
- 狙击精英 5（SniperElite5）
- 深岩银河（DeepRockGalactic）
- 行星边际 2（PlanetSide2）
- 地铁：离去（MetroExodus）
- 死亡循环（Deathloop）
- 光环：士官长合集（MCC-Win64-Shipping）
- 孤岛危机 3（Crysis3）
- 生化危机 4 重制（re4）
- 穿越火线（CrossFire / CF）
- 逆战未来（NZM）
- 逆战（NZ / NZLauncher）
- 使命召唤OL（codol）
- 战争雷霆（aces / aces_x64）
- 反恐精英OL（cso）
- 生化危机 2 重制版（re2）
- 生化危机 3 重制版（re3）
- 生化危机 7（re7）
- 战术小队（SquadGame）
- 孤岛惊魂 5（farcry5）
- 孤岛惊魂：原始杀戮（farCryPrimal）
- 正当防卫 4（JustCause4）
- 幽灵行者（GhostRunner）

### MOBA / 对战
- 英雄联盟（LeagueClient / League of Legends）
- DOTA 2（dota2）
- 王者荣耀 PC 版（HonorOfKings）
- 决战！平安京（OnmyojiArena）
- 虚荣（Vainglory）
- 风暴英雄（HeroesOfTheStorm）
- 星际争霸 2（SC2）

### 开放世界 / RPG
- 崩坏：星穹铁道（StarRail）
- 原神（GenshinImpact / YuanShen）
- 绝区零（ZenlessZoneZero）
- 鸣潮（WutheringWaves）
- 黑神话：悟空（b1-Win64-Shipping）
- 艾尔登法环（eldenring）
- 赛博朋克 2077（Cyberpunk2077）
- GTA V（GTA5）
- 荒野大镖客 2（RDR2）
- 巫师 3（witcher3）
- 博德之门 3（bg3 / bg3_dx11）
- 上古卷轴 5（SkyrimSE / TESV）
- 辐射 4（Fallout4）
- 星空（Starfield）
- 刺客信条系列（ACOdyssey / ACValhalla / AC Syndicate）
- 塞尔达（模拟器）（ryujinx / yuzu）
- 幻兽帕鲁（Palworld）
- 流放之路（PathOfExile / PathOfExile_x64）
- 暗黑破坏神 4（Diablo IV）
- 魔兽世界（Wow / WowClassic）
- 最终幻想 14（ffxiv_dx11）
- 命运方舟（LostArk）
- 星际战甲（Warframe）
- 怪物猎人（MonsterHunterWorld / MonsterHunterRise）
- 怪物猎人：荒野（MonsterHunterWilds）
- 只狼：影逝二度（sekiro）
- 对马岛之魂（GhostOfTsushima）
- 匹诺曹的谎言（LiesOfP）
- 堕落之主（LordsOfTheFallen）
- 卧龙：苍天陨落（WoLong）
- 龙之信条 2（Dragon's Dogma 2）
- 最终幻想 7 重制版（FF7R）
- 最终幻想 16（ff16）
- 霍格沃茨之遗（HogwartsLegacy）
- 原子之心（AtomicHeart）
- 遗迹 2（Remnant2）
- 仁王 2（nioh2）
- 神界：原罪 2（DivinityOriginalSin2）
- 极乐迪斯科（DiscoElysium）
- 天国拯救 2（KingdomCome）
- 如龙 8（Yakuza8）
- 真三国无双：起源（DynastyWarriorsOrigins）
- 艾尔登法环：夜王（EldenRingNightreign）
- 暗黑破坏神 2 重制版（D2R）
- 黑暗之魂 3（DarkSouls3）
- 黑暗之魂 2（DarkSoulsII）
- 黑暗之魂：重制版（DARK SOULS REMASTERED）
- 噬血代码（CodeVein）
- 尼尔：机械纪元（NieRAutomata）
- 破晓传说（Tales of Arise）
- 绯红结系（Scarlet Nexus）
- 底特律：化身为人（DetroitBecomeHuman）
- 质量效应：传奇版（MassEffectLE）
- 龙腾世纪：审判（DragonAgeInquisition）
- 恐怖黎明（GrimDawn）
- 女神异闻录 5 皇家版（P5R）
- 女神异闻录 3 Reload（P3R）

### 动作 / 竞速 / 其他
- 永劫无间（NarakaBladepoint）
- 地平线 6（ForzaHorizon6）
- 地平线 5（ForzaHorizon5）
- 地平线 4（ForzaHorizon4）
- 尘埃拉力赛 2.0（dirt2）
- 欧洲卡车模拟 2（eurotrucks2）
- 双人成行（It Takes Two）
- 胡闹厨房 2（Overcooked2）
- 泰拉瑞亚（Terraria）
- 我的世界（javaw / java / Minecraft.Windows）
- 星露谷物语（StardewValley）
- 缺氧（OxygenNotIncluded）
- 环世界（RimWorld）
- 城市：天际线（Cities）
- 文明 6（CivilizationVI）
- 全面战争：战锤 3（warhammer3）
- 帝国时代 4（AgeOfEmpires4）
- 三国：全面战争（ThreeKingdoms）
- 糖豆人（FallGuys_client）
- 模拟人生 4（TS4_x64）
- 中国式家长（ChineseParents）
- 太吾绘卷（Taiwu）
- 鬼谷八荒（TaleOfImmortal）
- 戴森球计划（DysonSphereProgram）

### 动作 / 冒险
- 战神（GodOfWar）
- 战神：诸神黄昏（GoWR）
- 漫威蜘蛛侠（MarvelsSpiderMan）
- 漫威蜘蛛侠 2（Spider-Man2）
- 地平线：零之曙光（HorizonZeroDawn）
- 地平线：西之绝境（HorizonForbiddenWest）
- 最后生还者 1（TheLastOfUs）
- 神秘海域：盗贼遗产（Uncharted4）
- 古墓丽影：暗影（ShadowOfTheTombRaider）
- 星球大战绝地：幸存者（StarWarsJediSurvivor）
- 星球大战绝地：陨落的武士团（StarWarsJediFallenOrder）
- 蝙蝠侠：阿卡姆骑士（BatmanArkhamKnight）
- 消逝的光芒 2（DyingLight2）
- 往日不再（Days Gone）
- 死亡空间（DeadSpace）
- 心灵杀手 2（AlanWake2）
- 控制（Control）
- 羞辱 2（Dishonored2）
- 看门狗 2（WatchDogs2）
- 生化危机：村庄（re8）
- 双影奇境（SplitFiction）
- 死亡搁浅（DeathStranding）
- 刺客信条：影（ACShadows）
- 逃生 2（Outlast2）
- 地狱之刃 2（Hellblade2）
- 星刃（StellarBlade）
- 鬼泣 5（DevilMayCry5）
- 师父（Sifu）
- 掠食（Prey）
- 德军总部：新秩序（WolfNewOrder）

### 策略 / 模拟
- 文明 7（CivilizationVII）
- 群星（Stellaris）
- 十字军之王 3（CK3）
- 维多利亚 3（Victoria3）
- 钢铁雄心 4（HOI4）
- 城市：天际线 2（Cities2）
- 冰汽时代 2（Frostpunk2）
- 战锤 40K：星际战士 2（Warhammer40KSpaceMarine2）
- 微软飞行模拟 2024（FlightSimulator）
- 极限竞速：Motorsport（ForzaMotorsport）
- F1 24（F1_24）
- 模拟农场 25（FarmingSimulator25）
- 双点医院（TwoPointHospital）
- 帝国时代 2：决定版（AoE2）
- 全面战争：战锤 2（warhammer2）
- 过山车之星（PlanetCoaster）
- 动物园之星（PlanetZoo）
- 侏罗纪世界：进化 2（JurassicWorldEvolution2）
- 幽浮 2（XCOM2）
- 亿万僵尸（TheyAreBillions）
- 饥荒（dontstarve）
- 饥荒联机版（dontstarvetogether）
- 坎巴拉太空计划（KSP）
- 传送门 2（portal2）
- 半条命 2（hl2）
- 欧陆风云 4（eu4）
- 纪元 1800（Anno1800）
- 海岛大亨 6（Tropico6）
- 暗黑地牢（DarkestDungeon）

### 生存 / 合作
- 英灵神殿（valheim）
- 森林之子（SonsOfTheForest）
- 夜族崛起（VRising）
- 盗贼之海（SeaOfThieves）
- 火箭联盟（RocketLeague）
- 黎明杀机（DeadByDaylight）
- 第五人格（IdentityV）
- 动物派对（PartyAnimals）
- 在我们中间（AmongUs）
- 方舟：生存进化（ShooterGame）
- 无人深空（NMS）
- 人类一败涂地（HumanFallFlat）
- 木筏求生（Raft）
- 森林（TheForest）
- 绿色地狱（GreenHell）
- 深海迷航（Subnautica）
- 幸福工厂（Satisfactory）
- 异星工厂（Factorio）
- 恐鬼症（Phasmophobia）
- 僵尸毁灭工程（ProjectZomboid / ProjectZomboid64）
- 腐蚀（rust）
- 七日杀（7DaysToDie）
- 地心护核者（CoreKeeper）

### 独立 / Roguelike
- 哈迪斯 2（Hades2）
- 吸血鬼幸存者（VampireSurvivors）
- 咩咩启示录（CultOfTheLamb）
- 潜水员戴夫（DaveTheDiver）
- 巴拉特罗（Balatro）
- 动物井（AnimalWell）
- 死亡细胞（DeadCells）
- 空洞骑士（HollowKnight）
- 灵魂面甲（Soulmask）
- 杀戮尖塔（SlayTheSpire）
- 以撒的结合（isaac-ng / isaac）
- 蔚蓝（Celeste）
- 奥日与黑暗森林（ori）
- 挺进地牢（EnterTheGungeon）
- 星界边境（starbound）
- 山羊模拟器（GoatSimulator）
- 胡闹搬家（MovingOut）

### 竞速 / 体育
- 极品飞车：不羁（NFSUnbound）
- 极品飞车：热度（NFSHeat）
- NBA 2K24（NBA2K24）
- 实况足球 eFootball（eFootball）

### 国产 / 其他
- 燕云十六声（yysls）
- 无限暖暖（InfinityNikki）
- 尘白禁区（Snowbreak）
- 暖雪（WarmSnow）
- 剑网 3（JX3）
- 逆水寒（nsh）
- 仙剑奇侠传 7（Pal7 / Pal7-Win64-Shipping）
- 幻塔（TowerOfFantasy）
- 荒野乱斗（BrawlStars）
- 古剑奇谭 3（Gujian3）
- 卡拉彼丘（Strinova）
- 地下城与勇士（DNF / DNFCHINA）
- 天涯明月刀（wuxia / wuxia_client）
- 剑灵（client）
- QQ飞车（GameApp）
- 崩坏 3（BH3）
- 诛仙世界（ZXSJ）
- 完美世界（端游）（elementclient）
- 梦幻西游（mhmain / xyqsvc）
- 新倩女幽魂（XQN）
- 大话西游 2（xy2）
- 冒险岛（MapleStory）
- 龙之谷（DragonNest）
- 永恒之塔（Aion）
- 泡泡堂（BNB）
- 热血传奇（Mir2）
- 传奇世界（Woool）
- 跑跑卡丁车（KartRider）
- 天龙八部（TLBB）
- 斗战神（Asura）
- 荒野行动（hyxd）
- 天下3（tw2launch / tw2）
- 我的世界中国版（MinecraftLauncher）
- 光·遇（Sky）

### 其他补充
- 最终幻想 15（FFXV）
- 迷失（Stray）
- 异形：隔离（AlienIsolation）
- 辐射 76（Fallout76）
- 波西亚时光（Portia）
- 火炬之光 2（Torchlight2）
- 洛奇英雄传（Vindictus）
- 黑色沙漠（BlackDesert64）
- 奇异人生（LifeIsStrange）
- 瘟疫传说：无罪（APlagueTale）
- 拳皇 15（KOFXV）
- 街头霸王 6（StreetFighter6）
- 真人快打 11（MK11）
- 猎人：荒野的召唤（theHunter）
- 米塔（MiSideFull）
- 在奇境（inZOI）

## 自定义游戏

除内置名单外，用户可在「滤镜设置 - 游戏滤镜」页手动添加自定义游戏（名称 + 进程名），
自定义名单持久化保存在 `game_filter.json` 中，与内置名单合并参与检测。

## 维护说明

- 内置名单以 `src-tauri/src/game_filter.rs` 的 `BUILTIN_GAMES` 为准；
- 修改内置名单后请同步更新本文档，保持两者一致；
- 进程名采用精确匹配（忽略大小写 / `.exe` 后缀），若游戏更新后进程名变更导致检测不到，请及时修正本表。
