# NexBox 架构说明

> 本文描述 NexBox 桌面端的整体架构、模块边界、关键数据流与扩展方式。最后更新：2026-11-09。

## 1. 总览

NexBox 是基于 Tauri 2 的 Windows 桌面应用，采用「React 前端 + Rust 服务层 + Windows 原生能力」的分层结构。硬件传感器由独立的 .NET 子进程采集，下载能力由独立 Rust crate 提供，安装器、卸载器与 MSIX 打包各自独立。

```
┌────────────────────────────────────────────────────┐
│ 表现层  React 19 + TypeScript + Vite + Chakra UI    │
│ 主窗口 / 悬浮面板 / 准星 / 灵动岛 / 桌面歌词          │
└────────────────────────┬───────────────────────────┘
                         │ Tauri IPC（invoke / event）
┌────────────────────────▼───────────────────────────┐
│ 桥接层  Tauri 2.12（命令注册、窗口、托盘、全局热键）  │
└────────────────────────┬───────────────────────────┘
                         │
┌────────────────────────▼───────────────────────────┐
│ 服务层  Rust（硬件、优化、显示、游戏、音乐、存储）    │
└───────────┬───────────────────────────┬────────────┘
            │                           │
┌───────────▼────────────┐  ┌───────────▼────────────┐
│ Windows 原生接口        │  │ NexBoxMonitor.exe      │
│ 注册表 / WMI / NVML     │  │ .NET 4.8 + LHM         │
│ NVAPI / Win32 API       │  │ JSON over stdin/stdout │
└─────────────────────────┘  └────────────────────────┘
```

设计原则：

- **本地优先**：核心功能不依赖云端，不上传用户隐私数据。
- **故障隔离**：硬件监控放在独立子进程，崩溃或卡死不影响主进程，并支持自动重启。
- **按域拆分**：Rust 后端按硬件、优化、显示、游戏、音乐、存储等领域拆分模块，避免单文件膨胀。
- **原生优先**：需要底层能力的场景直接调用 Windows API，而不是通过中间层转发。

## 2. 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 19、TypeScript 5.8、Vite 6、Chakra UI 2、Zustand 5、React Router 7、i18next |
| 桌面框架 | Tauri 2.12 |
| 后端 | Rust（2021 edition）、Tokio、reqwest、serde |
| 硬件监控 | .NET Framework 4.8、LibreHardwareMonitorLib |
| Windows 集成 | windows-sys、windows-core、winreg、WMI、NVML、NVIDIA NVAPI SDK |
| 下载引擎 | nx-dl-engine（独立 workspace crate，2024 edition） |
| 构建与测试 | Cargo、npm/Vite、GitHub Actions |

## 3. 分层设计

### 3.1 表现层（`src/`）

- `src/pages/`：60+ 个功能页面，按业务域划分（硬件监控、系统优化、显示滤镜、游戏工具、音乐播放器等）。
- `src/components/`：可复用 UI 组件与业务组件。
- `src/contexts/`：主题、背景、字体、音乐、更新等跨页面状态。
- `src/stores/`：Zustand 运行时状态。
- `src/hooks/`：窗口同步、桌面歌词、快捷键等自定义 Hook。
- `src/lib/`、`src/config/`：Tauri 调用封装、常量与配置。
- `src/locales/`：简体中文、繁體中文、English、Français、日本語、Deutsch 六套语言包。

除主窗口外，以下界面使用独立的 `WebviewWindow`，按需创建并设置透明、置顶、鼠标穿透等属性：

| 窗口 | 用途 |
| --- | --- |
| `desktop-lyrics` | 桌面歌词 |
| `overlay-panel` | 游戏内叠加面板 |
| `vertical-overlay` | 竖排叠加面板 |
| `crosshair` | 准星叠加 |
| `dynamic-island` | 灵动岛 |
| `sensor-monitor` | 全量传感器监控窗口 |

### 3.2 桥接层（`src-tauri/src/lib.rs`）

- 统一在 `invoke_handler` 中注册 500+ 个 Tauri 命令，前端通过 `invoke` 调用。
- 使用 Tauri 事件推送传感器数据、下载进度、窗口状态等实时信息。
- 集成 `single-instance`、`global-shortcut`、`store`、`dialog`、`opener`、`shell`、`log` 等插件。
- 负责主窗口、托盘、窗口显隐、单实例唤醒与应用退出清理。
- 权限在 `src-tauri/capabilities/default.json` 中按最小必要原则声明。

### 3.3 服务层（`src-tauri/src/`）

| 领域 | 主要模块 |
| --- | --- |
| 硬件与传感器 | `hardware.rs`、`sensor.rs`、`sensor_monitor.rs`、`smart.rs`、`jedec.rs`、`nvapi.rs`、`wmi_query.rs`、`game_fps.rs`、`game_ping.rs` |
| 系统优化 | `optimization.rs`、`startup_manager.rs`、`service_manager.rs`、`power_settings.rs`、`network_optimize.rs`、`disk_optimize.rs`、`runtime_repair.rs`、`vtx_virtualization.rs`、`shader_cache.rs`、`pso_cache.rs`、`mouse_poll.rs` |
| 显示与叠加 | `display_filter.rs`、`display_cache.rs`、`crosshair.rs`、`overlay_panel.rs`、`vertical_overlay.rs`、`island_window.rs`、`game_filter.rs` |
| 游戏与社区 | `delta_force.rs`、`df_stats.rs`、`game_launcher.rs`、`steam.rs`、`anticheat.rs`、`cpu_scheduler.rs`、`game_process_optimize.rs` |
| 音乐与音频 | `music.rs`、`music_api/`、`audio_engine.rs`、`audio_eq.rs`、`spectrum.rs`、`smtc.rs`、`netease_lyrics.rs`、`external_player.rs` |
| 存储与下载 | `storage_scan/`、`storage_clean.rs`、`downloader.rs`、`download_accelerator.rs`、`nx-dl-engine` |
| 应用基础 | `main_window.rs`、`tray.rs`、`hotkey.rs`、`auto_start.rs`、`context_menu.rs`、`feature_flags.rs`、`announcement.rs` |

模块之间通过 Rust 函数调用协作；耗时任务使用 `tokio::spawn` 或异步命令执行，避免阻塞主线程。

### 3.4 系统集成层

- 注册表读写：`winreg`，用于系统优化、启动项、GPU 改名、上下文菜单等。
- WMI 查询：`wmi`，用于主板、BIOS、内存、显卡等硬件信息。
- 显卡监控：NVML 读取 NVIDIA GPU 状态；NVAPI（`R560-developer/`）用于更底层的显卡能力。
- Win32 API：`windows-sys` / `windows-core`，用于窗口、显示器、电源、外设、鼠标轮询率等。
- 底层驱动：部分硬件读写通过 PawnIO 驱动完成，安装包内置 `PawnIO_setup.exe`。
- 需要管理员权限的操作在命令入口处校验，并通过系统 API 执行；注册表类优化配套 `aq_registry_restore/` 恢复脚本。

### 3.5 硬件监控子进程（`monitor/`）

`NexBoxMonitor.exe` 是基于 .NET Framework 4.8 与 LibreHardwareMonitorLib 的独立传感器进程：

- 启动：Rust 端 `sensor::start_sensor_process` 在主进程就绪后拉起子进程。
- 协议：Rust 通过 stdin 写入 JSON 读取命令，子进程通过 stdout 按行输出 JSON 结果。
- 生命周期：Rust 端负责超时保护、异常重启（带冷却与最大重试次数）、退出时终止子进程。
- 隔离原因：LibreHardwareMonitorLib 依赖 .NET 运行时与底层驱动访问，独立进程可避免其崩溃、GC 或驱动异常拖垮主应用。

### 3.6 下载引擎（`src-tauri/nx-dl-engine/`）

- 作为独立 crate 加入 Cargo workspace，使用 2024 edition，隔离新语法对主工程的影响。
- 负责分片调度、分片建议、代理配置、限速与下载状态管理。
- 主工程通过 `download_accelerator.rs` 调用，前端只负责展示进度与控制。

### 3.7 交付层

| 目录 | 职责 |
| --- | --- |
| `src-tauri/` | 主应用（React 前端 + Rust 后端 + 打包资源） |
| `installer/` | 安装程序（独立 React + Tauri 工程） |
| `uninstaller/` | 卸载程序（独立 React + Tauri 工程） |
| `msix/` | Microsoft Store 的 MSIX 打包脚本与配置 |
| `monitor/` | 硬件监控子进程（C#） |
| `power-plans/`、`aq_registry*/` | 内置电源方案与注册表优化/恢复脚本 |

## 4. 关键数据流

### 4.1 硬件监控

```
页面组件 → invoke("get_cpu_load") → hardware.rs → sensor.rs
        → NexBoxMonitor.exe stdin → LibreHardwareMonitorLib
        → stdout JSON → sensor.rs → 页面状态更新
```

传感器数据按需读取，Rust 端带超时；子进程异常时自动重启，连续失败后返回可读错误而不是卡死界面。

### 4.2 系统优化

```
页面组件 → invoke("optimize_*") → 对应 Rust 模块 → 注册表 / WMI / 系统服务 / Win32 API
        → 返回执行结果 → 页面提示
```

写操作前校验权限与参数；注册表类操作保留恢复脚本，支持一键还原。

### 4.3 音乐播放

```
页面组件 → invoke("music_*") → music_api → 各平台 HTTP API
        → 播放地址 / 歌词 / 歌单 → 音频引擎 → 桌面歌词窗口（事件同步）
```

音乐平台登录态、歌单与播放状态由前端 store 与 Rust 端共同维护；桌面歌词是独立透明窗口，通过事件与主窗口同步。

### 4.4 叠加窗口

准星、叠加面板、灵动岛、桌面歌词均为独立 `WebviewWindow`：

- 窗口属性：透明、无边框、置顶、跳过任务栏，按需启用鼠标穿透。
- 显隐控制：全局热键或主窗口开关触发，Rust 端负责窗口创建、定位与层级。
- 数据更新：主窗口通过 Tauri 事件推送 FPS、CPU、GPU、内存等指标。

## 5. 目录结构

```
NexBox/
├── src/                      # React 前端
│   ├── pages/                # 功能页面
│   ├── components/           # UI 与业务组件
│   ├── contexts/             # 跨页面状态
│   ├── hooks/                # 自定义 Hook
│   ├── stores/               # Zustand 状态
│   ├── lib/                  # Tauri 调用与工具函数
│   ├── locales/              # 六语言包
│   └── config/               # 前端配置
├── src-tauri/
│   ├── src/                  # Rust 服务层与 Tauri 命令
│   ├── nx-dl-engine/         # 独立下载引擎 crate
│   ├── capabilities/         # Tauri 权限声明
│   └── resources/            # 打包资源
├── monitor/                  # .NET 硬件监控子进程
├── installer/                # 安装程序
├── uninstaller/              # 卸载程序
├── msix/                     # Microsoft Store 打包
├── power-plans/              # 电源方案
├── aq_registry/              # 优化注册表脚本
├── aq_registry_restore/      # 注册表恢复脚本
├── docs/                     # 功能与架构文档
└── .github/workflows/        # CI
```

## 6. 状态与持久化

- **运行时状态**：Zustand（`src/stores/`）与 React Context（主题、背景、字体、音乐、更新）。
- **持久化设置**：`tauri-plugin-store` 写入本地 JSON；部分轻量开关在 WebView `localStorage` 中保留兜底值。
- **命名约定**：设置键使用 kebab-case，例如 `island-liquid-glass-enabled`。
- **多窗口同步**：主窗口与叠加窗口通过 Tauri 事件同步状态，避免各自维护一份配置。

## 7. 安全与权限

- Tauri capabilities 按最小权限声明，未使用的系统能力不开放。
- Release 构建关闭 DevTools，避免通过 WebView 调试接口调用本地 IPC 命令。
- 核心功能离线可用；联网功能包括音乐平台、公告、更新检查、网速测试与匿名版本统计。
- 需要管理员权限的系统优化操作在命令入口校验，失败时返回明确错误。
- 项目使用 GPL-3.0 许可证，源码可审计。

## 8. 测试与质量

- Rust 端内置 269 个单元测试，覆盖下载分片调度与代理配置、Winapp2 规则解析/检测/路径展开/深度扫描、音乐平台 API、特性开关、BSOD 日志、JEDEC、显示缓存、显示滤镜等核心逻辑。
- 运行全部测试：

```bash
cargo test --workspace --manifest-path src-tauri/Cargo.toml
```

- CI（`.github/workflows/ci.yml`）在 push / PR 时执行前端构建、Rust `cargo check` 与 `cargo test`。
- 前端以 TypeScript 类型检查与 Vite 构建作为基础质量门禁。

## 9. 扩展指引

1. **新增后端能力**：在 `src-tauri/src/` 新建模块，在 `lib.rs` 中声明 `mod`，并把命令注册进 `invoke_handler`。
2. **新增页面**：在 `src/pages/` 创建页面组件，在 `src/App.tsx` 添加路由，并同步六套语言包。
3. **新增设置项**：使用 kebab-case 键名，通过 `tauri-plugin-store` 持久化；需要即时生效的设置走事件通知，不重启应用。
4. **新增窗口**：使用 `WebviewWindowBuilder` 创建独立窗口，按需设置透明、置顶与鼠标穿透属性。
5. **提交前检查**：运行 `cargo test --workspace --manifest-path src-tauri/Cargo.toml` 与 `npm run build`。
