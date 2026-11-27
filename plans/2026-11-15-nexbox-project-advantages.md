# NexBox 现状优势盘点（只读调研）

日期：2026-11-15
类型：只读调研 / 结论归档，不含代码改动

## 目标

以只读方式梳理 NexBox 当前工程的实际体量与能力边界，输出可直接用于对外介绍、答辩或 README 提炼的「优势清单」，并标注每条优势在仓库中的证据位置与潜在短板。

## 调研方式

只使用 list_dir / glob / grep / read_file（run_code 内嵌 meow 读取），未运行任何命令、未修改任何代码。统计口径均为仓库内现有文件，非估算。

## 现状事实（带证据）

### 1. 体量

| 维度 | 实测 | 证据 |
| --- | --- | --- |
| 功能页面 | 62 个 `.tsx`，共 57,326 行 | `src/pages/` |
| 复用组件 | 76 个 | `src/components/` |
| 前端路由 | 59 处 Route 定义 | `src/App.tsx`（556 行） |
| Rust 源文件 | 84 个，共 70,063 行 | `src-tauri/src/` |
| Rust 模块 | 86 个 `mod` 声明 | `src-tauri/src/lib.rs` |
| Tauri 命令 | 639 条（去重后仍 639） | `src-tauri/src/lib.rs:649` 起 `generate_handler!` |
| Rust 依赖条目 | 82 条 | `src-tauri/Cargo.toml` |
| 单元测试 | 281 个 `#[test]` / `#[tokio::test]` | `src-tauri/` 全树，其中 162 个在 `nx-dl-engine` |
| 语言包 | 6 套（zh / zh-TW / en / fr / ja / de） | `src/locales/` |
| 前端状态层 | Zustand + 7 个 Context + 12 个 Hook | `src/stores/`、`src/contexts/`、`src/hooks/` |

### 2. 架构分层优势

- **进程级故障隔离**：硬件传感器跑在独立 .NET 4.8 子进程 `NexBoxMonitor.exe`（`monitor/Program.cs`，依赖 LibreHardwareMonitorLib），Rust 端 `sensor.rs` 用 stdin/stdout JSON 协议 + 超时 + 冷却重启保护主进程（`ARCHITECTURE.md:103-110`）。
- **下载引擎独立 crate**：`src-tauri/nx-dl-engine/` 用 edition 2024 单列 workspace member，隔离新语法风险，自带 162 个测试、11 个源文件，主工程经 `download_accelerator.rs` 调用（`src-tauri/Cargo.toml:12-14`）。
- **交付层四套独立工程**：`src-tauri/`（主应用）、`installer/`、`uninstaller/`、`msix/`（Microsoft Store 打包），另有一条 `tauri.store.conf.json` 走 Store 专用构建（`package.json:10,17`）。
- **底层能力直连**：NVAPI DRS（`src-tauri/src/nvapi.rs`）、WMI（`wmi_query.rs`）、winreg、PawnIO 驱动（`pawnio_driver.rs`）、`windows-sys` / `webview2-com`（WebView 内存层级控制），不靠中间层转发。

### 3. 功能覆盖面优势（相对同类的差异化）

- **一站式**：硬件监控 + 20+ 项系统优化（含 120 项 Windows 深度调整、Winapp2 规则清理）+ 显示滤镜/准星/叠加面板 + 游戏专区 + 五平台音乐播放器，替代玩家通常要装的 7~8 个工具（`README.md:44-167`）。
- **叠加窗口体系**：`desktop-lyrics` / `overlay-panel` / `vertical-overlay` / `crosshair` / `dynamic-island` / `sensor-monitor` 六类独立 `WebviewWindow`，透明、置顶、鼠标穿透按需（`ARCHITECTURE.md:61-70`）。
- **MFT 级存储扫描**：直接读 `$MFT` + USN 枚举（`storage_scan/big_files_engine/mft_core.rs`），共 17 个文件的分层扫描/删除/安全常量，比逐目录遍历快一个量级。
- **PC ↔ 安卓生态闭环**：`remote_access/` 11 个模块（配对码 + 设备令牌哈希 + 常量时间比较 + 局域网 UDP 发现 45689 + 剪贴板同步 + 音乐远程控制 + 文件互传），并配 `NexBox-Android/` 原生客户端（Kotlin + Compose）。
- **可还原的安全设计**：注册表类优化配 `aq_registry_restore/` 恢复脚本；`capabilities/default.json` 仅声明 31 条最小权限，涉及 7 个插件（`ARCHITECTURE.md:203-209`）。

### 4. 工程质量优势

- CI 双 job（`web` → `tauri`，后者 `needs: web`）：`npm ci` + `npm run build` + `cargo check` + `cargo test --workspace`（`.github/workflows/ci.yml:9-33`）。
- 已被 Git 历史验证的稳定迭代节奏：10.x 系列连续发版，近期提交包含 Tauri 2.12 升级、磁盘测速、CPU 改名、86 项优化移植等（`git log`: `582c6d1`、`bbf74e8`）。
- 文档与功能同步：`docs/` 11 篇专题文档 + `ARCHITECTURE.md` + `README.md` 全功能清单。
- 全本地优先：核心功能离线可用，仅音乐/公告/更新/测速/匿名统计联网（`ARCHITECTURE.md:207`）。

## 潜在短板（同一口径下的另一面）

1. `lib.rs` 1,389 行、639 条命令集中在单一 `invoke_handler`，读写成本随时间线性上升——可考虑按域拆分 `register!` 宏或子模块 `all()` 聚合。
2. `eslint` 存在约 149 条历史遗留错误，未纳入 CI（`.github/workflows/ci.yml` 只跑 build/check/test），前端口径的门禁只有类型检查 + 构建。
3. Windows 专属能力占比极高（WMI / NVAPI / 注册表 / PawnIO），跨平台与可移植性天然受限；`Cargo.toml` 无 `[target.'cfg(windows)']` 以外的分层。
4. 单人主维护 + GPL-3.0，社区贡献门槛受 Windows 环境与管理员权限调试限制。
5. 部分模块存在 `.bak` / `.txt` 残留（如 `src-tauri/src/netease_lyrics.rs.bak`、`optimization.txt`），长期看会干扰代码搜索。

## 后续可选动作（本计划不实施）

- 把上述「现状事实」表压缩成 README「核心亮点」数据化版本（替换现有定性描述）。
- 给 `lib.rs` 命令注册做按域拆分（需先加一个注册顺序一致性测试）。
- 补齐 `eslint` 与 CI 的取舍决策：要么修 149 条错误后入 CI，要么在 `eslint.config.mjs` 显式降级为 warning 并留注释说明。

## 验证方式

本次为只读调研，未产生改动，验证即「结论可复现」：

```powershell
# 命令数量（应为 639）
(Select-String -Path src-tauri/src/lib.rs -Pattern '^\s+[a-z_0-9]+' ).Count

# 测试数量（应为 281）
(Select-String -Path src-tauri -Include *.rs -Pattern '#\[test\]|#\[tokio::test\]' -Recurse).Count
```

若上述数字与文档不一致（例如新增了模块），以仓库实测为准并更新本文件对应表格。
