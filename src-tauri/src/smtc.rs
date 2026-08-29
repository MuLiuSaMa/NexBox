//! 内部播放器 SMTC（System Media Transport Controls）集成
//!
//! 把 NexBox 内置音乐播放器的播放信息注册到 Windows 系统媒体传输控制：
//! - 音量浮层 / 锁屏显示「新境盒」+ 封面 + 歌名 + 歌手 + 进度条
//! - 系统媒体键（播放/暂停/上一曲/下一曲）与浮层拖动进度控制内部播放器
//!
//! 与 external_player.rs（读取外部客户端会话）相反，本模块是注册**本应用自己**
//! 的媒体会话。Tauri 是无包身份的桌面应用，不能用 UWP 的 GetForCurrentView()，
//! 必须通过 ISystemMediaTransportControlsInterop::GetForWindow(hwnd) 获取会话
//! （参考 windows-rs issue #3734 的完整可用写法）。
//!
//! 注意：WebView2(Chromium) 会为 <audio> 播放自动注册系统媒体会话（显示「未知应用」），
//! 与本站点会话重复。因此所有窗口统一配置 additionalBrowserArgs
//! `--disable-features=MediaSessionService,HardwareMediaKeyHandling` 禁用它，
//! 只保留本会话（显示名「新境盒」）。参数必须全窗口一致，否则 WebView2 环境冲突。

#[cfg(not(target_os = "windows"))]
mod imp {
    /// 非 Windows 平台 no-op
    pub fn start(_app: tauri::AppHandle) {}
    pub fn update(_state: super::SmtcState) {}
    pub fn clear() {}
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::{mpsc, Mutex, OnceLock};

    use tauri::{AppHandle, Emitter, Manager};
    use windows::core::{Interface, IInspectable};
    use windows::Foundation::{TimeSpan, TypedEventHandler};
    use windows::Media::{
        MediaPlaybackStatus, MediaPlaybackType,
        PlaybackPositionChangeRequestedEventArgs,
        SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
        SystemMediaTransportControlsDisplayUpdater,
        SystemMediaTransportControlsTimelineProperties,
    };
    use windows::Storage::Streams::RandomAccessStreamReference;
    use windows::Storage::StorageFile;
    use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;

    use base64::Engine;
    use crate::smtc::SmtcState;

    /// 100ns tick / 毫秒
    const TICK_PER_MS: i64 = 10_000;
    /// 封面下载 UA（与 cover 代理一致）
    const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    /// 同一封面来源解析失败后的重试退避
    const COVER_FAIL_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);
    /// SMTC 缩略图目标最大边长（系统浮层/锁屏显示远小于此）
    const THUMBNAIL_MAX_PX: u32 = 1024;
    /// 原始封面超过该字节数才解码缩图（≤1MB 的封面尺寸已合理，免解码开销）
    const THUMBNAIL_RESIZE_THRESHOLD: usize = 1024 * 1024;

    /// 控制台会话句柄（全局唯一）
    static CTRL: Mutex<Option<SystemMediaTransportControls>> = Mutex::new(None);
    /// 供 WinRT 事件回调线程向前端 emit 使用
    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// SMTC 更新命令：经常驻工作线程串行处理（避免前端每秒 push 时反复 spawn 线程，
    /// 线程栈/内核对象频繁创建销毁会导致进程提交内存持续上涨——"播放时内存一直涨"的根因之一）
    enum SmtcCmd {
        Update(SmtcState),
        Clear,
    }

    static SMTC_TX: Mutex<Option<mpsc::Sender<SmtcCmd>>> = Mutex::new(None);

    /// 获取（或创建）SMTC 常驻工作线程的发送端。线程启动一次即常驻，
    /// 后续 update/clear 只发消息，零线程创建开销。
    fn ensure_worker() -> mpsc::Sender<SmtcCmd> {
        let mut guard = SMTC_TX.lock().unwrap();
        if let Some(tx) = guard.as_ref() {
            return tx.clone();
        }
        let (tx, rx) = mpsc::channel::<SmtcCmd>();
        std::thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    SmtcCmd::Update(state) => do_update(state),
                    SmtcCmd::Clear => do_clear(),
                }
            }
        });
        *guard = Some(tx.clone());
        tx
    }

    /// 缓存：元数据键（title|artist|album），判断歌曲是否变化
    static LAST_TRACK: Mutex<Option<String>> = Mutex::new(None);
    /// 缓存：最近一次成功设置的封面来源（data URI / 代理 URL / file:// 路径）。
    /// 失败时不记录 → 下次 update 自动重试（带 COVER_FAIL_BACKOFF 退避），直到成功。
    static LAST_COVER_KEY: Mutex<Option<String>> = Mutex::new(None);
    /// 封面解析失败退避：(封面来源, 最近失败时刻)。前端每秒推送一次播放状态，
    /// 失败若无退避会以每秒一次的频率重复下载大图并刷警告日志。
    static LAST_COVER_FAIL: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

    /// 启动时注册本应用媒体会话（在 Tauri setup 中调用一次）
    pub fn start(app: AppHandle) {
        // SMTC 浮层顶部显示的应用名取自绑定窗口的标题，统一改为中文品牌名「新境盒」
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_title("新境盒");
        }
        let _ = APP.set(app.clone());

        std::thread::spawn(move || {
            // 未打包桌面应用：系统通过「开始菜单快捷方式」把进程 AppUserModelID 解析为显示名。
            // 不创建快捷方式时浮层会显示「未知应用」；创建指向本 exe 的「新境盒.lnk」后即可显示「新境盒」。
            // （进程不设置显式 AUMID，保持默认 exe 路径，与快捷方式默认 AUMID 一致才能匹配。）
            if let Err(e) = ensure_start_menu_shortcut() {
                log::warn!("[SMTC] 创建开始菜单快捷方式失败（可能仍显示未知应用）: {e}");
            }
            if let Err(e) = init_controls(&app) {
                log::warn!("[SMTC] 初始化本应用媒体会话失败: {e}");
                return;
            }
            log::info!("[SMTC] 本应用媒体会话初始化成功");
        });
    }

    /// 幂等创建开始菜单快捷方式「新境盒.lnk」（指向当前 exe），供 Windows 解析 SMTC 显示名。
    /// 已存在且指向同一 exe 时跳过；否则覆盖重建。
    fn ensure_start_menu_shortcut() -> Result<(), String> {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, IPersistFile};
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        let exe_str = exe.to_string_lossy().to_string();

        // 快捷方式目录：%APPDATA%\Microsoft\Windows\Start Menu\Programs\NexBox\
        let appdata = std::env::var("APPDATA").map_err(|_| "APPDATA 环境变量缺失".to_string())?;
        let dir = std::path::Path::new(&appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("NexBox");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建快捷方式目录失败: {e}"))?;
        let lnk = dir.join("新境盒.lnk");

        // 已存在且指向同一 exe 则跳过（避免每次启动重建）
        if let Ok(existing) = std::fs::read_to_string(&lnk) {
            if existing.contains(&exe_str) {
                return Ok(());
            }
        }

        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("CoCreateInstance(ShellLink): {e}"))?;
            let exe_w: Vec<u16> = exe_str.encode_utf16().chain(std::iter::once(0)).collect();
            let desc_w: Vec<u16> = "新境盒".encode_utf16().chain(std::iter::once(0)).collect();
            link.SetPath(windows::core::PCWSTR(exe_w.as_ptr()))
                .map_err(|e| format!("SetPath: {e}"))?;
            link.SetDescription(windows::core::PCWSTR(desc_w.as_ptr()))
                .map_err(|e| format!("SetDescription: {e}"))?;
            let persist: IPersistFile = link.cast().map_err(|e| format!("cast IPersistFile: {e}"))?;
            let lnk_str = lnk.to_string_lossy().to_string();
            let lnk_w: Vec<u16> = lnk_str.encode_utf16().chain(std::iter::once(0)).collect();
            persist
                .Save(windows::core::PCWSTR(lnk_w.as_ptr()), true)
                .map_err(|e| format!("Save 快捷方式: {e}"))?;
        }
        log::info!("[SMTC] 已创建开始菜单快捷方式: {}", lnk.display());
        Ok(())
    }

    /// 初始化 SMTC 会话：Interop GetForWindow 绑定主窗口 + 注册按键/进度事件
    fn init_controls(app: &AppHandle) -> Result<(), String> {
        // WinRT 需要线程具备 COM 公寓初始化
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
        }

        let window = app
            .get_webview_window("main")
            .ok_or_else(|| "主窗口不存在".to_string())?;
        let hwnd = window
            .hwnd()
            .map_err(|e| format!("获取主窗口 HWND 失败: {e}"))?;
        // Tauri hwnd() 返回的元组结构 `.0` 即裸窗口句柄
        let hwnd = windows::Win32::Foundation::HWND(hwnd.0 as *mut core::ffi::c_void);

        // 通过 Interop 激活工厂获取 ISystemMediaTransportControlsInterop 并绑定窗口
        let interop: ISystemMediaTransportControlsInterop =
            windows::core::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()
                .map_err(|e| format!("获取 SMTC Interop 工厂失败: {e}"))?;
        let controls: SystemMediaTransportControls = unsafe {
            interop
                .GetForWindow::<windows::Win32::Foundation::HWND, IInspectable>(hwnd)
                .map_err(|e| format!("GetForWindow 失败: {e}"))?
        }
        .cast()
        .map_err(|e| format!("SMTC 会话类型转换失败: {e}"))?;

        // 初始禁用：未播放时不显示在系统浮层（否则空会话会显示 exe 路径等无意义内容）；
        // 首次播放推送状态时由 update() 启用。
        let _ = controls.SetIsEnabled(false);
        let _ = controls.SetIsPlayEnabled(true);
        let _ = controls.SetIsPauseEnabled(true);
        let _ = controls.SetIsNextEnabled(true);
        let _ = controls.SetIsPreviousEnabled(true);

        // 系统媒体键 / 浮层按钮按下 → 转发给前端
        let btn_handler = TypedEventHandler::<SystemMediaTransportControls, SystemMediaTransportControlsButtonPressedEventArgs>::new(
            move |_sender, args| {
                let button = args
                    .as_ref()
                    .and_then(|a| a.Button().ok())
                    .unwrap_or(SystemMediaTransportControlsButton::Play);
                let action = match button {
                    SystemMediaTransportControlsButton::Play => "play-pause",
                    SystemMediaTransportControlsButton::Pause => "play-pause",
                    SystemMediaTransportControlsButton::Stop => "stop",
                    SystemMediaTransportControlsButton::Next => "next",
                    SystemMediaTransportControlsButton::Previous => "prev",
                    _ => return Ok(()),
                };
                if let Some(app) = APP.get() {
                    let _ = app.emit("smtc:control", serde_json::json!({ "action": action }));
                }
                Ok(())
            },
        );
        // 浮层拖动进度 → 转发给前端
        let seek_handler =
            TypedEventHandler::<SystemMediaTransportControls, PlaybackPositionChangeRequestedEventArgs>::new(
                move |_sender, args| {
                    let position_ms = args
                        .as_ref()
                        .and_then(|a| a.RequestedPlaybackPosition().ok())
                        .map(|t| t.Duration / TICK_PER_MS)
                        .unwrap_or(0);
                    if let Some(app) = APP.get() {
                        let _ = app.emit(
                            "smtc:control",
                            serde_json::json!({ "action": "seek", "positionMs": position_ms }),
                        );
                    }
                    Ok(())
                },
            );
        let _btn_token = controls
            .ButtonPressed(&btn_handler)
            .map_err(|e| format!("注册 ButtonPressed 失败: {e}"))?;
        let _seek_token = controls
            .PlaybackPositionChangeRequested(&seek_handler)
            .map_err(|e| format!("注册 PlaybackPositionChangeRequested 失败: {e}"))?;
        // 持有 handler 避免委托被释放导致事件失效
        let _ = (btn_handler, seek_handler);

        *CTRL.lock().unwrap() = Some(controls);
        Ok(())
    }

    /// 前端推送最新播放状态（标题/进度/播放状态变化时调用）
    pub fn update(state: SmtcState) {
        let _ = ensure_worker().send(SmtcCmd::Update(state));
    }

    fn do_update(state: SmtcState) {
        let controls = match CTRL.lock().unwrap().as_ref() {
            Some(c) => c.clone(),
            None => return,
        };

            // 有播放内容时启用会话（未播放时禁用，避免空会话显示在浮层）
            let _ = controls.SetIsEnabled(true);

            let track_key = format!("{}|{}|{}", state.title, state.artist, state.album);
            let cover_src = state.cover.clone().unwrap_or_default();

            // 歌曲变化 → 重建元数据（歌名/歌手/专辑）；封面来源变化 → 重新设置封面。
            // 两者相互独立：封面失败时不记录 LAST_COVER_KEY，退避 30 秒后自动重试。
            let track_changed = LAST_TRACK.lock().unwrap().as_deref() != Some(track_key.as_str());
            // 同一封面来源 30 秒内失败过则暂不重试（退避），避免每秒推送触发重复下载
            let cover_backoff_active = match LAST_COVER_FAIL.lock().unwrap().as_ref() {
                Some((key, at)) => key == &cover_src && at.elapsed() < COVER_FAIL_BACKOFF,
                None => false,
            };
            let cover_changed = !cover_src.is_empty()
                && LAST_COVER_KEY.lock().unwrap().as_deref() != Some(cover_src.as_str())
                && !cover_backoff_active;

            if track_changed || cover_changed {
                let updater: Option<SystemMediaTransportControlsDisplayUpdater> =
                    match controls.DisplayUpdater() {
                        Ok(u) => Some(u),
                        Err(e) => {
                            log::warn!("[SMTC] 获取 DisplayUpdater 失败: {e}");
                            None
                        }
                    };
                if let Some(updater) = updater.as_ref() {
                    if track_changed {
                        if let Err(e) = update_metadata(updater, &state, &track_key) {
                            log::warn!("[SMTC] 更新元数据失败: {e}");
                        } else {
                            *LAST_TRACK.lock().unwrap() = Some(track_key);
                        }
                    }
                    if cover_changed {
                        match resolve_cover_bytes(&cover_src) {
                            Some(bytes) => match set_cover_thumbnail(updater, &bytes) {
                                Ok(()) => {
                                    log::info!("[SMTC] 封面已设置 ({} 字节)", bytes.len());
                                    *LAST_COVER_KEY.lock().unwrap() = Some(cover_src);
                                    *LAST_COVER_FAIL.lock().unwrap() = None;
                                }
                                Err(e) => {
                                    log::warn!("[SMTC] 设置封面失败: {e}");
                                    *LAST_COVER_FAIL.lock().unwrap() =
                                        Some((cover_src.clone(), std::time::Instant::now()));
                                }
                            },
                            None => {
                                log::warn!("[SMTC] 封面来源无法解析: {cover_src}");
                                *LAST_COVER_FAIL.lock().unwrap() =
                                    Some((cover_src.clone(), std::time::Instant::now()));
                            }
                        }
                    }
                    if let Err(e) = updater.Update() {
                        log::warn!("[SMTC] 提交元数据失败: {e}");
                    }
                }
            }

            // 播放状态
            let status = if state.playing {
                MediaPlaybackStatus::Playing
            } else {
                MediaPlaybackStatus::Paused
            };
            if let Err(e) = controls.SetPlaybackStatus(status) {
                log::warn!("[SMTC] 设置播放状态失败: {e}");
            }

            // 按钮可用态
            let _ = controls.SetIsPlayEnabled(!state.playing);
            let _ = controls.SetIsPauseEnabled(state.playing);
            let _ = controls.SetIsNextEnabled(true);
            let _ = controls.SetIsPreviousEnabled(true);

            // 进度/时长（TimeSpan 100ns tick）
            if state.duration_ms > 0 || state.playing {
                let dur_ms = state.duration_ms.max(0);
                let pos_ms = state.position_ms.max(0);
                match SystemMediaTransportControlsTimelineProperties::new() {
                    Ok(timeline) => {
                        let _ = timeline.SetStartTime(TimeSpan { Duration: 0 });
                        let _ = timeline.SetMinSeekTime(TimeSpan { Duration: 0 });
                        let _ = timeline.SetMaxSeekTime(TimeSpan { Duration: dur_ms * TICK_PER_MS });
                        let _ = timeline.SetEndTime(TimeSpan { Duration: dur_ms * TICK_PER_MS });
                        let _ = timeline.SetPosition(TimeSpan { Duration: pos_ms * TICK_PER_MS });
                        if let Err(e) = controls.UpdateTimelineProperties(&timeline) {
                            log::warn!("[SMTC] 更新进度失败: {e}");
                        }
                    }
                    Err(e) => log::warn!("[SMTC] 创建 TimelineProperties 失败: {e}"),
                }
            }
    }

    /// 停止播放/无歌时调用：会话置 Stopped + 禁用，从浮层消失
    pub fn clear() {
        let _ = ensure_worker().send(SmtcCmd::Clear);
    }

    fn do_clear() {
        let controls = match CTRL.lock().unwrap().as_ref() {
            Some(c) => c.clone(),
            None => return,
        };
        let _ = controls.SetPlaybackStatus(MediaPlaybackStatus::Stopped);
        let _ = controls.SetIsEnabled(false);
        if let Ok(updater) = controls.DisplayUpdater() {
            let _ = updater.ClearAll();
        }
        *LAST_TRACK.lock().unwrap() = None;
        *LAST_COVER_KEY.lock().unwrap() = None;
        *LAST_COVER_FAIL.lock().unwrap() = None;
    }

    /// 更新 DisplayUpdater 元数据（仅在歌曲变化时调用，封面由调用方单独处理）
    fn update_metadata(
        updater: &SystemMediaTransportControlsDisplayUpdater,
        state: &SmtcState,
        track_key: &str,
    ) -> Result<(), String> {
        updater
            .SetType(MediaPlaybackType::Music)
            .map_err(|e| e.to_string())?;
        let music = updater.MusicProperties().map_err(|e| e.to_string())?;
        music.SetTitle(&windows::core::HSTRING::from(state.title.as_str()))
            .map_err(|e| e.to_string())?;
        music.SetArtist(&windows::core::HSTRING::from(state.artist.as_str()))
            .map_err(|e| e.to_string())?;
        music.SetAlbumTitle(&windows::core::HSTRING::from(state.album.as_str()))
            .map_err(|e| e.to_string())?;
        log::info!("[SMTC] 更新元数据: {track_key}");
        Ok(())
    }

    /// 解析封面来源为图片字节（过大时先缩图）：
    /// - `data:` base64 data URI → 直接解码
    /// - `http(s)://` URL → 后端 reqwest 下载（带防盗链 Referer，无 CORS 限制）
    /// - `file://` 本地路径 → 直接读文件（本地导入歌曲的封面缓存）
    fn resolve_cover_bytes(cover: &str) -> Option<Vec<u8>> {
        let raw = if cover.starts_with("data:") {
            decode_data_uri(cover)?
        } else if let Some(path) = cover.strip_prefix("file://") {
            std::fs::read(path).ok()?
        } else if cover.starts_with("http://") || cover.starts_with("https://") {
            download_cover(cover)?
        } else {
            return None;
        };
        let bytes = normalize_cover_thumbnail(raw);
        // SMTC 缩略图大小上限保护（缩图失败时的兜底）
        if bytes.len() > 5 * 1024 * 1024 {
            log::warn!("[SMTC] 封面过大且无法缩图 ({} 字节): {cover}", bytes.len());
            return None;
        }
        Some(bytes)
    }

    /// 下载网络封面（带防盗链 Referer，按域名判断；QQ 封面含 y.qq.com/qpic.cn/gtimg.cn）
    fn download_cover(cover: &str) -> Option<Vec<u8>> {
        let referer = if cover.contains("qq.com")
            || cover.contains("qpic.cn")
            || cover.contains("gtimg.cn")
        {
            "https://y.qq.com/"
        } else if cover.contains("kugou.com") {
            "https://www.kugou.com/"
        } else if cover.contains("migu.cn") || cover.contains("miguvideo.com") {
            "https://music.migu.cn/"
        } else {
            "https://music.163.com/"
        };
        let client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[SMTC] 构建封面下载客户端失败: {e}");
                return None;
            }
        };
        let resp = match client
            .get(cover)
            .header("User-Agent", UA)
            .header("Referer", referer)
            .send()
        {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[SMTC] 封面下载请求失败: {e} (url={cover})");
                return None;
            }
        };
        if !resp.status().is_success() {
            log::warn!("[SMTC] 封面下载非 200: {} (url={cover})", resp.status());
            return None;
        }
        let bytes = match resp.bytes() {
            Ok(b) => b.to_vec(),
            Err(e) => {
                log::warn!("[SMTC] 封面读取失败: {e}");
                return None;
            }
        };
        Some(bytes)
    }

    /// 过大封面缩图为最大边 THUMBNAIL_MAX_PX 的 JPEG（网易云高清封面可达 5MB+，
    /// 超出 SMTC 缩略图大小上限且系统浮层/锁屏显示用不到原图）。
    /// 解码/编码失败或尺寸已足够小时原样返回（交由调用方大小上限兜底）；
    /// 输出固定 JPEG（RGBA 透明通道被丢弃，对系统缩略图可接受）。
    fn normalize_cover_thumbnail(data: Vec<u8>) -> Vec<u8> {
        use image::{GenericImageView, ImageEncoder};
        if data.len() <= THUMBNAIL_RESIZE_THRESHOLD {
            return data;
        }
        let Ok(img) = image::load_from_memory(&data) else {
            log::warn!("[SMTC] 封面解码失败，按原始字节处理 ({} 字节)", data.len());
            return data;
        };
        let (w, h) = img.dimensions();
        if w.max(h) <= THUMBNAIL_MAX_PX {
            return data;
        }
        let resized = img
            .resize(THUMBNAIL_MAX_PX, THUMBNAIL_MAX_PX, image::imageops::FilterType::Triangle)
            .to_rgb8();
        let mut out = Vec::new();
        let mut writer = std::io::Cursor::new(&mut out);
        let encode = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 85)
            .write_image(resized.as_raw(), resized.width(), resized.height(), image::ExtendedColorType::Rgb8);
        match encode {
            Ok(()) if !out.is_empty() => out,
            _ => data,
        }
    }

    /// 解析 base64 data URI（data:image/xxx;base64,....）为原始字节
    fn decode_data_uri(uri: &str) -> Option<Vec<u8>> {
        let idx = uri.find("base64,")?;
        let b64 = &uri[idx + "base64,".len()..];
        base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()
    }

    /// 把封面字节写入临时文件，通过 RandomAccessStreamReference::CreateFromFile 设为 SMTC 缩略图。
    /// （InMemoryRandomAccessStream 方式在 SMTC 上不可靠、不显示，写临时文件更稳定。）
    fn set_cover_thumbnail(
        updater: &SystemMediaTransportControlsDisplayUpdater,
        bytes: &[u8],
    ) -> Result<(), String> {
        let path = std::env::temp_dir().join("nexbox_smtc_cover.jpg");
        std::fs::write(&path, bytes).map_err(|e| format!("写临时封面文件失败: {e}"))?;
        let path_str = path.to_string_lossy().to_string();
        let h = windows::core::HSTRING::from(path_str);
        let file = StorageFile::GetFileFromPathAsync(&h)
            .map_err(|e| format!("GetFileFromPathAsync: {e}"))?
            .get()
            .map_err(|e| format!("GetFileFromPathAsync get: {e}"))?;
        let reference = RandomAccessStreamReference::CreateFromFile(&file)
            .map_err(|e| format!("CreateFromFile: {e}"))?;
        updater
            .SetThumbnail(&reference)
            .map_err(|e| format!("SetThumbnail: {e}"))
    }
}

/// 前端推送的 SMTC 状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtcState {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// 封面来源：base64 data URI / http(s) URL / file:// 本地路径；空 = 不更新封面
    pub cover: Option<String>,
    pub playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
}

/// 启动后台注册本应用媒体会话（在 setup 中调用）
pub fn start(app: tauri::AppHandle) {
    imp::start(app);
}

#[tauri::command]
pub fn smtc_update_state(state: SmtcState) {
    // 「设置 → 高级 → 键盘媒体键控制」关闭时：不向系统推送/启用本应用媒体会话。
    // 启用中的 SMTC 会话会让系统把物理媒体键路由给新境盒（前端又因开关不响应），
    // 导致其他音乐软件收不到媒体键；此处直接忽略推送，保持会话禁用。
    if !crate::media_keys::control_enabled() {
        return;
    }
    imp::update(state);
}

#[tauri::command]
pub fn smtc_clear() {
    imp::clear();
}
