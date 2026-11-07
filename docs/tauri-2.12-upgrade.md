# Tauri 2.12 升级记录

2026-11-07 完成，Tauri 核心 2.11.1 → 2.12.5。升级前后的 API 完全兼容，没有代码改动，只动了依赖声明。

## 为什么升

2.11.1 与 2.12 之间有三类值得拿到的改动：

安全修复（最主要）：

- GHSA-w28w-mhc8-qvjv：channel / invoke 响应队列按 webview 隔离。此前任意 webview 可以读到其他 webview 队列里的响应
- runtime ACL deny 改为按 local / remote 上下文分别限定，不再一刀切拦全部 origin

Windows 相关修复（与本项目用法一一对应）：

- `unstable` feature 下的 Alt-Tab 焦点问题（本项目启用了该 feature）
- 无边框窗口拖动时产生假 `Focused` 事件（本项目 `decorations: false` 自绘标题栏）
- `asset://` 协议修复多段 206 响应，且加载移出事件循环线程（本项目全站用 convertFileSrc 喂图：广告图、三角洲图片、NVIDIA 缩略图、外设图标、QQ 群头像）
- NSIS 安装包在静默 / 被动模式下恢复 `allowDowngrades: false` 的降级拦截（影响自研更新器走安装包的链路）
- 窗口关闭时 channel / listener 泄漏修复；`Image::rgba` 跳过冗余拷贝（托盘图标）

其他：`asset://` 移出事件循环线程属于性能改进，大图加载不再和窗口拖动 / 输入抢线程。

## 改动清单

`src-tauri/Cargo.toml`：

| 项 | 旧 | 新 | 说明 |
|---|---|---|---|
| `tauri` | 2.10 | 2.12 | lock 实际解析到 2.12.5 |
| `tauri-build` | 2.5 | 2.6 | lock 实际解析到 2.7.4 |
| `rust-version` | 1.77.2 | 1.90 | 2.12 的 MSRV（2.12 起 MSRV 政策为 stable - 3） |
| `webview2-com` | 0.38 | 0.39 | 必须与 wry 同版本，见下文 |
| `windows-core` | 0.61 | 0.62 | 同上 |

`package.json`：

- `@tauri-apps/api` ^2.10.1 → ^2.12.0（lock 实际 2.12.4）
- `@tauri-apps/cli` ^2.10.1 → ^2.12.0（lock 实际 2.12.4）
- 删除 `@tauri-apps/plugin-process`：本就无人引用的死依赖（Rust 侧从未安装该插件，前端也无 import），且 2.12 起进程退出进了核心 API（`@tauri-apps/api/app` 的 `exit()`）

锁定文件：`Cargo.lock`（定向 `cargo update -p tauri -p tauri-build`，只动 tauri 生态，不碰其他 crate）、`package-lock.json`。

随 tauri 一起更新的关键传递依赖：wry 0.55.1 → 0.57.1、tao 0.35.2 → 0.37.2、tauri-runtime-wry 2.12.3、tray-icon 0.25.1、muda 0.20、window-vibrancy 0.8.1、webview2-com-sys 0.39.1。

## webview2-com 必须跟 wry 对齐的原因

`src-tauri/src/lib.rs` 的 `set_main_webview_memory_level` 通过 `win.with_webview` 拿 wry 的 controller，再 cast 到 `ICoreWebView2_19` 调 `SetMemoryUsageTargetLevel`（隐藏到托盘时降低 WebView2 内存目标）。这要求直接依赖的 `webview2-com` / `windows-core` 版本与 wry 编译所用版本完全一致，否则 `cast` 的 trait 与目标类型来自不同版本、无法通过编译。wry 0.57 对应 webview2-com 0.39 + windows-core 0.62。将来再升 tauri / wry 时，先查 wry 的依赖版本，同步改这两行。

## 注意事项

- 本机与 CI 均满足 MSRV：本机 rustc 1.97.1，CI 用 `dtolnay/rust-toolchain@stable`。其他开发机需要 Rust ≥ 1.90
- Tauri 2.12 起官方放弃 Windows 7（WebView2 新安装器在 Win7 上崩）。本项目目标 Win10+，无影响
- 2.12 的 Android 相关破坏性变更（`$VIDEO` 路径、Gradle 模板）不适用本仓库（安卓端是独立 Kotlin 仓库）
- 升级后 DevTools 配置、托盘、global-shortcut、single-instance、store 等插件 API 无任何变化；`tauri.conf.json` 未动

## 验证

- `cargo check`：通过（tauri 全栈 + webview2-com 0.39 互操作编译无误）
- `tsc --noEmit`：通过
- `vite build`：通过（chunk 体积警告为升级前已存在）

## 回滚

单次提交内 revert 即可：`Cargo.toml`、`Cargo.lock`、`package.json`、`package-lock.json` 四个文件恢复原样后 `npm install` + `cargo check`。

## 同日其他依赖升级（2026-11-07 第二批）

工具链与 Rust 依赖：

- rustc 1.97.1 → 1.99.1（rustup 自更新到 1.29.3 时报 os error 183 链接错误，仅管理器自身问题，不影响工具链）
- 全量 `cargo update`：259 个包 semver 兼容更新，`cargo check` 通过

JS 依赖（semver 兼容批次）：react / react-dom 19.3.1、vite 6.4.5、@tauri-apps 插件 5 个（shell 2.6.0、dialog 2.8.4、opener 2.7.2、store 2.5.0、clipboard-manager 2.5.0）、i18next 26.7.2、react-router-dom 7.18.5、prettier 3.9.10 等。

JS 大版本（本次已做）：

- framer-motion 11.18.2 → 14.10.1：v14 收紧 `Variants` 类型（`ease: string` 不再接受）。MusicPage 4 个变体常量补 `Variants` 类型注解即可，共修 9 处报错，无行为变化
- lucide-react 0.487.0 → 1.70.0：1.0 移除全部品牌图标。项目只用了 `Chrome`（Edge/Chrome 优化项），已换成 `Globe`；注册表 id 里的 "Chrome" 字样与图标无关，未动
- @vitejs/plugin-react 4.7.0 → 5.2.0：6.x 全系 peer 锁死 `vite ^8`，当前 vite 6 只能到 5.x

JS 大版本（本次未做，原因如下）：

- eslint 10：装不上。eslint-config-next 16 硬依赖 `next` 包本体（本项目是 Vite，没有 next）；config-next 15 的 peer 只支持 eslint ≤9。已回退 eslint 9.39.4 + config-next 15.5.29
- 发现 `npm run lint` 在本次升级前就是坏的：`next` 从未出现在 lock 里，config-next 的 parser require 链一直是断的；`.husky` 无 pre-commit 钩子，不影响提交。修复方向二选一：去掉 eslint-config-next、用 @typescript-eslint + react-hooks 等插件手写 flat config（顺带解锁 eslint 10），或等 config-next 16 提供 next 之外的 parser 路径

遗留待办（按收益排序）：

1. vite 6 → 8 + @vitejs/plugin-react 6（解锁插件新特性）
2. eslint flat config 重写 + eslint 10（顺带修好休眠的 lint）
3. Rust 大版本：reqwest 0.11 → 0.12、axum 0.7 → 0.8（路由 `:param` → `{param}`）、sysinfo 0.30 → 新版（CPU 组件 API 变化大）、windows 0.58 → 0.62、tokio-tungstenite、rand 0.8 → 0.9、dirs 5 → 6。均无安全/性能紧迫性
4. 明确不做：@chakra-ui/react 2 → 3（等于重写全站样式层）

## Tauri 3 评估结论

3.0 仍在 alpha（alpha.5，2026-10），`tauri migrate` 不支持 2 → 3 自动迁移，且本项目用的 `unstable` / `tray-icon` / `protocol-asset` feature 在 3.0 里全部要挪到 runtime crate，迁移有实际工作量。运行时性能 / 内存与 2.x 基本无差异（新选项 `tauri-runtime-cef` 是打包整套 Chromium，体积内存只会更大，与本需求相反）。结论：等 3.0 进 beta 且 migrate 工具完善后再评估，当前留在 2.x。

参考：[Tauri 2.12 发布说明](https://v2.tauri.app/blog/tauri-2.12) | [核心 changelog](https://v2.tauri.app/release/tauri/) | [3.0 迁移指南](https://v3.tauri.app/de/start/migrate/from-tauri-2)
