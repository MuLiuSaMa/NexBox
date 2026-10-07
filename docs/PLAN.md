# NexBox PC ↔ 安卓远程桌面（局域网 1080p60）

## Summary
- 在现有配对、令牌、局域网发现和 WebSocket 基础上，新增 Android 原生远程桌面，PC 作为视频与输入主机。
- 分两阶段交付：阶段 A 只实现画面观看并达到性能门槛；阶段 B 再开放鼠标、键盘和触控操作。
- 目标为同局域网 1080p/60fps，接通时间不超过 3 秒，稳定网络下玻璃到玻璃延迟 p50 ≤ 80ms、p95 ≤ 120ms。
- 单会话、无音频、无公网中继、正常桌面限定；已配对设备在 PC 总开关开启后可直接观看和控制。

## Implementation Changes
- **会话模型**：在 `src-tauri/src/remote_access` 增加远程桌面会话管理器，同一时刻只允许一台设备连接；其他设备返回 `SESSION_BUSY`，同设备断线 10 秒内可重连恢复，超时或被撤销后释放捕获、编码、WebRTC 和输入状态。
- **信令**：复用现有鉴权 WebSocket。Android 先发 `remote.session.start`，收到 `remote.session.ready` 后创建 Offer，PC 返回 Answer；直接使用局域网 Host ICE，不依赖 STUN/TURN，并增加 NexBox 程序级 UDP 防火墙放行规则。
- **视频链路**：PC 使用 `webrtc-rs 0.21` 发送 H.264，Android 使用现有 MCTier 已验证的 `io.github.webrtc-sdk:android:144.7559.14` 解码到 `SurfaceViewRenderer`。
- **画面采集**：使用 `windows-capture 2.0.1` 的 Windows Graphics Capture 捕获单台显示器，失败时回退 DXGI Desktop Duplication；PC 弹窗负责选择显示器，手机只显示当前显示器。
- **编码与自适应**：优先使用 Media Foundation 硬件 H.264 MFT，失败时回退系统软件 MFT；D3D11 Video Processor 完成缩放和 BGRA 到 NV12 转换。使用 WebRTC GCC 估算带宽，以估算值的 75%–90% 调整码率，并按 `1080p60 → 900p60 → 720p30` 自动降档。
- **编码策略**：起始档位为 1080p60、8–25 Mbps；900p60 使用 4–8 Mbps；720p30 使用 1.5–4 Mbps。每 2 秒插入 IDR，并在新观看者加入、分辨率切换、网络恢复、显示器切换和屏幕从锁屏恢复时立即请求 IDR。
- **输入控制**：默认只观看，手机点击“控制”后才发送输入；PC 现有“远程控制”总开关是最终授权边界。使用 `nexbox-input-fast` 非可靠通道发送指针和滚轮，使用 `nexbox-input-reliable` 可靠通道发送按键、鼠标按键、文本和模式切换。
- **输入映射**：支持直接触控、虚拟触控板、双指滚动、长按右键和软键盘；坐标按当前显示器物理像素归一化后映射到 Windows 虚拟桌面，通过 `SendInput` 注入。会话关闭、通道断开或停止时强制释放所有已按下按键和鼠标键。
- **Android 界面**：主页增加“远程桌面”入口和独立全屏页面，默认横屏沉浸显示；提供自动/清晰/流畅、适配/原始尺寸、触控/触控板、软键盘和退出控制。App 进入后台 5 秒后停止会话，避免后台持续解码或被控。
- **PC 状态提示**：新增置顶 `remote-status` 小窗，显示设备名、观看/控制状态、分辨率、FPS、码率和停止按钮；使用 `WDA_EXCLUDEFROMCAPTURE` 防止状态栏出现在自己的画面中。会话期间注册独立全局停止快捷键 `Ctrl+Alt+Shift+F12`，不受其他热键总开关影响。
- **依赖与兼容**：PC 增加 `webrtc`、`windows-capture` 及对应 Media Foundation/Capture 特性；两者使用 Rust 2024 edition，因此将项目 MSRV 从 1.77.2 提升到 1.85。

## Interfaces
- `GET /api/info` 增加 `features: ["remote.screen.v1", "remote.input.v1"]`，旧 Android 端据此隐藏入口。
- `/api/capabilities` 增加 `remote_desktop` 分组，包含 `screen.displays` 查询和 `screen.select_display` 动作。
- WebSocket 新增消息：`remote.session.start/ready/quality/stop`、`remote.session.state`、`remote.stats`、`remote.busy`、`webrtc.offer/answer`；状态值为 `starting/streaming/paused/stopped/error`。
- 错误码统一为 `NOT_PAIRED`、`SERVICE_DISABLED`、`SESSION_BUSY`、`NO_DISPLAY`、`CAPTURE_FAILED`、`ENCODER_FAILED`、`SECURE_DESKTOP`、`UNSUPPORTED`。
- PC Tauri 命令新增 `cmd_remote_screen_status`、`cmd_remote_screen_select_display`、`cmd_remote_screen_stop`；显示器选择和质量偏好持久化到 `remote-screen.json`。
- Android 新增远程桌面数据模型、信令解析、WebRTC 会话仓库、输入事件序列化器和全屏 Compose 页面。

## Test Plan
- Rust 单元测试覆盖信令状态机、单会话抢占、重连宽限、显示器 ID 映射、负坐标及高 DPI 输入换算、断线释放按键、幂等停止。
- Windows 集成测试覆盖 WGC/DXGI、硬件/软件编码器、光标与边框设置、全屏/无边框游戏、显示器切换、锁屏恢复和受保护内容失败提示。
- Android 测试覆盖信令解析、H.264 Surface 渲染、旋转与后台切换、直接触控、触控板、双指滚动、长按右键、软键盘和断线重连。
- 端到端测试覆盖扫码配对、首次观看、控制授权、撤销设备、关闭服务、退出 PC 进程、切换网速和模拟丢包。
- 性能验收要求：1080p60 在 20 Mbps 局域网达到 p50 ≤ 80ms、p95 ≤ 120ms；降至 5 Mbps 后 5 秒内进入 720p30；连续运行 30 分钟无内存持续增长或残留输入。
- 回归验证现有远程监控、精选功能控制、文件互传、设备发现和六个语言包的文案与入口。

## Assumptions
- 目标平台为 Windows 10 22H2+ 与现有 Android App；不新增浏览器、iOS、公网中继或语音链路。
- 不拼接多显示器，不支持 UAC、锁屏、Ctrl+Alt+Del 和其他 Windows 安全桌面；遇到时暂停并提示。
- 不改变现有“远程控制”总开关默认关闭的策略；开启后已配对设备无需每次电脑确认即可控制，但电脑端始终显示置顶状态并可一键停止。
- 首版不做剪贴板同步、文件拖拽、独占本地键鼠、唤醒休眠电脑或反作弊绕过；受保护窗口和反作弊游戏可能无法捕获或注入。
