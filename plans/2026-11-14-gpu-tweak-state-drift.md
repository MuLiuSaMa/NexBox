# 修复显卡优化项「重启后开关自己变」的问题

日期：2026-11-14
状态：待确认（plan 模式，未改动任何代码）

## 目标

用户只开启「锁定 NVIDIA P-State 0」和「强制启用 AMD Shader Cache」两项，重启 NexBox 后开关状态会自己变化（一会儿 NVIDIA 那项变关，一会儿 AMD 那项变关）。要做到：

1. 开关状态与注册表真实状态一致，且**重启应用后不再自行翻转**。
2. 关闭某一项时，不能把另一项仍然需要的注册表值一起删掉。
3. NVIDIA / AMD / Intel 的优化项只写到自己厂商的显卡驱动键上，不再无脑写 `0000` / `0001`。

## 现状与根因（已核对代码与 .reg 文件）

### 1. 两个优化项共用同一个注册表值，恢复时互相删除（主因）

- `aq_registry/lockPState0.reg:8,11` 写 `DisableDynamicPstate=dword:00000001`
- `aq_registry/amdPowerOptimization.reg:11,23` **也写** `DisableDynamicPstate=dword:00000001`
- 两者的 restore 文件都是删除该值：
  - `aq_registry_restore/lockPState0.restore.reg:4,7` → `"DisableDynamicPstate"=-`
  - `aq_registry_restore/amdPowerOptimization.restore.reg:9,21` → `"DisableDynamicPstate"=-`

`restore_registry_tweak`（`src-tauri/src/optimization.rs:4706`）无条件执行 restore 文件里的删除，不看还有没有别的优化项在用这个值。所以只要用户关过 AMD 功耗优化（或点过「全部恢复」），NVIDIA P-State 那项的注册表值就被删掉，下次扫描自然变成关。

同类跨项冲突（脚本比对 206 个 .reg 得到，非猜测）：

| 注册表值 | 涉及的优化项 | 关系 |
| --- | --- | --- |
| `DisableDynamicPstate`（0000/0001） | lockPState0(2 条)、amdPowerOptimization(20 条) | lockPState0 是 amdPowerOptimization 的真子集 |
| `RotatingLockScreenEnabled` | disableAdsPromotional(19 条)、disableLockScreenSpotlight(1 条) | 后者是前者的真子集 |
| `AITEnable` | disableAppCompat(4 条)、disableAppImpactTelemetry(1 条)、disableAppImpactTelemetryAgent(1 条) | 后两者内容**完全相同**，且都是 disableAppCompat 的子集 |
| `DisableUAR` | disableAppCompat(4 条)、disableStepRecorder(1 条) | 后者是前者的真子集 |

补充事实：`disableAppImpactTelemetry.reg` 与 `disableAppImpactTelemetryAgent.reg` 逐字节等价（同一个键、同一个 `AITEnable=dword:00000000`），restore 也等价。这是纯粹的重复项，两个开关必然永远同步，属于 UI 冗余。

### 2. 扫描是「全条目 AND」，且写死了 0000/0001 两个键

`scan_reg_content`（`src-tauri/src/optimization.rs:4498`）要求 .reg 里**每一条**都匹配才算 applied；键打不开时（`open_reg_key_readonly` 返回 Err）直接判 false。

而 12 个显卡类 .reg 全部硬编码 `\{4d36e968-e325-11ce-bfc1-08002be10318}\0000` 和 `\0001`：
`amdPowerOptimization`、`disableClockGating`、`disableDriverLog`、`disableGpuPowerMgmt`、`disableHDCP`、`disableMiracastOverlay`、`disableWriteCombining`、`intelGraphicsOptimization`、`lockPState0`、`lowLatencyThreshold`、`perCpuDPC`、`shaderCache`（后者是 `0000\UMD` / `0001\UMD`）。

真实机器上显卡驱动键的编号不一定是 0000/0001（多显卡、核显+独显、驱动重装后编号会往后排，也可能只剩一个）。只要有一个键不存在或值被驱动重写，整项就判为「未优化」。这解释了「同一台机器上开关状态飘忽」。

### 3. NVIDIA / AMD / Intel 项写到所有显卡键上

`apply_reg_content`（`src-tauri/src/optimization.rs:4346`）照抄 .reg 路径写入，不区分厂商。于是「AMD Shader Cache」会往 NVIDIA 的 `0000\UMD` 里塞 `ShaderCache=0x32`，「Intel 显卡优化」会往独显键里写 `Display1_DisableAsyncFlips`。既无效，也让状态判断更容易被别的驱动行为干扰。

### 4. 前端每次进页面都用扫描结果覆盖本地记录

`src/pages/SystemOptimizerPage.tsx:344-392`：初始化时 `scanned ?? saved`，扫描成功就把结果写回 store。扫描口径一变（上面 1/2/3 任一原因），用户看到的开关就跟着变，本地记录也一起被改写。

### 5. 批量应用/恢复并发执行，同键写入顺序不确定

`batch_apply_reg_files`（`src-tauri/src/optimization.rs:4732`）按 CPU 数分块并发跑 `apply_reg_content`。lockPState0 与 amdPowerOptimization 写同一个值，谁最后落地不确定；批量恢复时更是随机删。

## 改动方案

### 步骤 0：先合并重复项（用户要求，优先做）

判定标准：**只有「注册表条目集合完全相同」才算同一个项目，可以直接合并**；「A 是 B 的子集」不算相同，不能合并（合并会丢掉 A 单独开关的能力）。

按此标准，206 个 .reg 里只有一对真正重复：

- `disableAppImpactTelemetry` 与 `disableAppImpactTelemetryAgent`：同键 `HKLM\SOFTWARE\Policies\Microsoft\Windows\AppCompat`、同值 `AITEnable=dword:00000000`，restore 同为删除。**合并为一项**。

具体改动：

1. `src/config/system-optimizer.ts:638-647`：删除 `disableAppImpactTelemetryAgent` 整个条目，保留 `disableAppImpactTelemetry`（`id`/`regName` 不变，避免老用户 store 里的状态失效）。
2. 把保留项的文案改成能覆盖两者语义：`src/locales/{zh,zh-TW,en,ja,de,fr}.json`
   - `systemOptimizer.privacy.disableAppImpactTelemetry` → 「禁用应用影响遥测（含代理）」
   - `...Desc` → 「禁用 AITEnable 策略，同时关闭 AIT 代理」
   - 删除 `disableAppImpactTelemetryAgent` / `...Desc` 两个键（6 个语言包各 2 条）。
3. 删除 `aq_registry/disableAppImpactTelemetryAgent.reg` 与 `aq_registry_restore/disableAppImpactTelemetryAgent.restore.reg`。
   - 打包无需改配置：`src-tauri/tauri.conf.json:63-64` 与 `installer/pack-payload.ps1:52-64` 都是整目录通配拷贝。
4. 兼容旧状态：`src/pages/SystemOptimizerPage.tsx` 初始化时，若 store 里存在 `disableAppImpactTelemetryAgent` 键，把它的值并入 `disableAppImpactTelemetry`（任一为 true 即 true），然后删除旧键。

**不合并**的三组（子集关系，保留独立开关，靠步骤 3 的引用计数解决互删）：

- `lockPState0` ⊂ `amdPowerOptimization`：一个是 NVIDIA 单项、一个是 AMD 全套 10 个值，语义与厂商都不同，合并会让用户没法只锁 P-State。
- `disableLockScreenSpotlight` ⊂ `disableAdsPromotional`：前者只关锁屏聚焦，后者关全部推广，粒度不同。
- `disableStepRecorder` ⊂ `disableAppCompat`：前者只关步骤记录器，后者连兼容性引擎和 PCA 一起关，风险等级也不同（low vs medium）。

> 注：`lockPState0` 与 `amdPowerOptimization` 共用 `DisableDynamicPstate` 属于**值级重叠**，不是项目重复。步骤 1 按厂商拆分写入目标后，NVIDIA 项只写 NVIDIA 键、AMD 项只写 AMD 键，物理上就不再冲突；步骤 3 的引用计数作为双保险。

### 步骤 1：给 .reg 引入「厂商 + 动态显卡键」占位（后端）

文件：`src-tauri/src/optimization.rs`

- 新增 `fn display_adapter_keys(vendor: Option<&str>) -> Vec<String>`：
  - 枚举 `HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}` 下形如 `00xx` 的子键；
  - 读 `MatchingDeviceId`（形如 `PCI\VEN_10DE&DEV_...`）判定厂商，`VEN_10DE`=nvidia、`VEN_1002`=amd、`VEN_8086`=intel；
  - 跳过 `DriverDesc` 含 `basic display` 的占位设备（与 `src-tauri/src/gpu_rename.rs:129-154` 的既有判定口径保持一致）；
  - 返回匹配厂商的全部子键名（可能 0 个、1 个或多个）。
- 新增 `fn expand_reg_content(content: &str, vendor: Option<&str>) -> String`：
  - 把 .reg 中的 `\{4d36e968-...}\0000`、`\0001` 这类固定编号段替换为实际枚举到的每个键，逐键展开；
  - 保留 `\UMD` 等后缀（`shaderCache` 需要）；
  - 厂商无匹配键时返回空内容（视为「本机无此厂商显卡」）。
- `apply_registry_tweak` / `restore_registry_tweak` / `batch_apply_reg_files` / `scan_registry_tweaks` 全部改为先 `expand_reg_content` 再走原有逻辑。
- 厂商来源：在 `src/config/system-optimizer.ts` 的 `OptimizerItem` 增加 `vendor?: "nvidia" | "amd" | "intel"`，前端调用时随 `name` 一起传 `vendor`；后端命令签名加 `vendor: Option<String>`，缺省 `None` 表示不做厂商过滤（兼容其它页面调用 `apply_registry_tweak` 的地方，如 `src/pages/WindowsUpdatePage.tsx:102`）。

### 步骤 2：扫描口径改为「至少一个真实显卡键匹配」

文件：`src-tauri/src/optimization.rs`（`scan_reg_content`，4498 行）

- 展开后按「键」分组：同一优化项在多个显卡键上的条目视为**或**关系（任一显卡键全部匹配即算 applied），同一键内的多个值仍是**与**关系。
- 厂商无匹配显卡键时返回 `applied=false`，并在结果里带上 `available=false`，供前端置灰而不是显示成「关」。
- `TweakScanResult`（4556 行）增加 `available: bool` 字段。

### 步骤 3：恢复时做引用计数，别删别人还在用的值

文件：`src-tauri/src/optimization.rs`

- 新增 `fn other_tweaks_using_value(name: &str, key: &str, value: &str) -> bool`：
  - 遍历 `aq_registry/*.reg`（排除自身），展开厂商键后，判断是否有别的项也写同一个 `键+值名+目标数据`；
  - 若有，再读注册表判断该项当前是否处于 applied（复用步骤 2 的判定）。
- `restore_registry_tweak` 与批量恢复在遇到 `"X"=-` 时，先调用上面的判断：**仍被其它已启用项占用则跳过删除**，其余照常删。
- 冲突清单（步骤 1 表格里那 4 组）加单元测试覆盖。

### 步骤 4：批量写入串行化同键项

文件：`src-tauri/src/optimization.rs`（`batch_apply_reg_files`，4732 行）

- 先按「展开后涉及的注册表键集合」分组，有交集的项归到同一 worker 串行执行，无交集的才并发。
- 保留现有 UAC 兜底逻辑（`elevated_reg_import` / `merge_reg_files`）不变。

### 步骤 5：前端不再让扫描结果无脑覆盖用户意图

文件：`src/pages/SystemOptimizerPage.tsx`

- `buildScanStates`（282 行）接收 `available`，不可用项在 `OptimizeCard` 上置灰 + 提示「本机未检测到该厂商显卡」，不计入 `optimizedCount`。
- 初始化（344-392 行）：扫描结果与本地记录不一致时，以扫描为准更新 UI，但**不覆盖 store 里用户显式操作过的项**；给 store 增加 `system_optimizer_user_set`（记录用户手动开关过的 id 集合），仅这些 id 在下次扫描不一致时弹一次提示「检测到 X 已被系统/驱动还原，是否重新应用？」，而不是静默翻转。
- `handleBatchDisable`（590 行）走步骤 3 的引用计数恢复，避免「全部恢复」把别的项一起废掉。

### 步骤 6：文案与配置

- `src/config/system-optimizer.ts`：给 12 个显卡项补 `vendor` 字段（nvidia 9 项、amd 2 项、intel 1 项）。
- `src/locales/{zh,zh-TW,en,ja,de,fr}.json` 的 `systemOptimizer` 段新增：
  - `vendorUnavailable`: 「本机未检测到该厂商显卡」
  - `stateDriftDetected`: 「检测到 {{name}} 已被系统还原，是否重新应用？」
  - `reapply`: 「重新应用」

## 风险

- **注册表写入范围变化**：改成按厂商动态枚举后，写入的键集合与旧版不同。旧版误写到其它厂商键上的残留值不会被自动清理，需要一次性迁移：首次启动检测到旧版写入痕迹时，按旧路径删除误写值（仅限本项目写入的已知值名）。
- **驱动重写**：NVIDIA/AMD 驱动更新或 `restart_graphics_driver` 后可能重写这些值，属于系统行为，前端只能提示重新应用，不能保证永久生效。
- **引用计数的性能**：恢复时要遍历 206 个 .reg 并展开，需缓存展开结果（进程内 `OnceLock<HashMap>`），避免每次恢复都做磁盘 IO。
- **`available=false` 的判定**：`MatchingDeviceId` 缺失的机器（极少数精简系统）会退化成「无可用显卡」，需保留按 `DriverDesc` 关键词兜底判定。
- 本方案不改动 `aq_registry/*.reg` 的原始值语义，只改「写到哪些键」和「什么时候删」，降低回归面。

## 验证方式

1. `cd src-tauri; cargo check`，再 `cargo test`（新增单测：厂商枚举、键展开、扫描或/与口径、引用计数跳过删除）。
2. `npx tsc --noEmit -p tsconfig.json`。
3. 合并项验证：隐私分类下只剩一个「禁用应用影响遥测（含代理）」，开关一次 `reg query "HKLM\SOFTWARE\Policies\Microsoft\Windows\AppCompat" /v AITEnable` 应为 0，关掉后该值消失；老用户 store 里带 `disableAppImpactTelemetryAgent` 的记录能正确迁移。
4. 手工回归（需管理员权限运行）：
   - 只开「锁定 NVIDIA P-State 0」+「强制启用 AMD Shader Cache」，`reg query` 确认值落在正确厂商的键上；
   - 关闭重启 NexBox 三次，两个开关都保持开启；
   - 开 AMD 功耗优化后再关掉它，`reg query` 确认 `DisableDynamicPstate` 仍在（P-State 项不受影响）；
   - 点「全部恢复」，确认所有值被清理且开关全关；
   - 单显卡机器（只有独显 / 只有核显）各测一遍，确认不再出现「键不存在导致整项判 false」。
4. 用 `reg query "HKLM\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}" /s` 对照实际键编号，验证动态枚举结果。