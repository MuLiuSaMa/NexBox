mod advanced;
mod announcement;
mod audio_engine;
mod audio_eq;
mod auto_start;
mod autoclicker;
mod music_api;
mod cpu_scheduler;
mod crosshair;
mod dattorro;
mod delta_force;
mod disk_optimize;
mod external_player;
mod spectrum;
mod display_cache;
mod display_filter;
mod downloader;
mod download_accelerator;
mod smtc;
mod anticheat;
mod game_fps;
mod game_filter;
mod game_launcher;
mod game_mode;
mod game_process_optimize;
mod game_win_key;
mod game_ime_lock;
mod game_ping;
mod gpu_rename;
mod hardware;
mod hardware_report;

mod feature_flags;
mod hotkey;
mod main_window;
mod media_keys;
mod music;
mod network_optimize;
#[allow(dead_code, unused_imports)]
mod netease_lyrics;
mod nvapi;
mod nvidia_driver_download;
mod runtime_repair;
mod optimization;
mod overlay_panel;
mod power_settings;
mod vertical_overlay;
mod vtx_virtualization;

mod sensor;
mod sensor_monitor;
mod shader_cache;
mod pawnio_driver;
mod smart;
mod sponsor;
mod contributor;
mod qq_group;
mod ads;
mod startup_manager;
mod service_manager;
mod system_fonts;
mod context_menu;
mod steam;
mod speedtest;
mod storage_clean;
mod storage_scan;
mod thirdparty_tools;
mod community_tools;
mod tray;
mod uapi;
mod utils;
mod video_bg;
mod wmi_query;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;

/// 主窗口可见性状态（用于最小化/托盘时暂停动态背景视频）
static MAIN_WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

/// 是否以开机自启(--autostart)方式启动（供前端判断加载完成后是否自隐藏到托盘）
static AUTOSTART_MODE: AtomicBool = AtomicBool::new(false);

/// 开机自启(--autostart)模式下前端(WebView2)是否已成功加载就绪。
/// 由 tray::minimize_to_tray（前端 App.tsx 挂载后必然调用）置位，
/// 供启动诊断判断“是否只启动后端、前端未起来”。
pub(crate) static AUTOSTART_FRONTEND_READY: AtomicBool = AtomicBool::new(false);

/// 按需创建竖排悬浮框窗口（不常驻，启用时创建、关闭时销毁）。
/// 创建后 visible(false)，前端渲染完成后调用 `vertical_overlay_ready` 命令 show，避免白屏闪烁。
pub fn ensure_vertical_overlay<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<tauri::WebviewWindow<R>> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let label = "vertical-overlay";
    if let Some(win) = app.get_webview_window(label) {
        return Some(win);
    }

    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(label.into()))
        .title("NexBox Vertical Overlay")
        // 与其它窗口保持一致的 WebView2 参数：禁用 Chromium 自动媒体会话，
        // 避免与 smtc.rs 注册的「新境盒」媒体会话重复（参数必须全窗口一致，否则 WebView2 环境冲突导致窗口创建失败）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling")
        .inner_size(220.0, 400.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .visible(false)
        .maximizable(false)
        .skip_taskbar(true)
        .shadow(false);

    match builder.build() {
        Ok(win) => Some(win),
        Err(e) => {
            log::error!("[Window] 创建 vertical-overlay 失败: {e}");
            None
        }
    }
}

/// 按需创建心境窗口（不常驻，点击主页「心境」卡片时创建并显示）。
/// 加载应用内 /mood 路由（WebviewUrl::App 对 SPA 回退到 index.html）。
pub fn ensure_mood_window<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Option<tauri::WebviewWindow<R>> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    let label = "mood";
    if let Some(win) = app.get_webview_window(label) {
        return Some(win);
    }

    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(label.into()))
        .title("心境")
        // 与其它窗口保持一致的 WebView2 参数：禁用 Chromium 自动媒体会话，
        // 避免与 smtc.rs 注册的「新境盒」媒体会话重复（参数必须全窗口一致，否则 WebView2 环境冲突导致窗口创建失败）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling")
        .inner_size(1000.0, 700.0)
        .resizable(true)
        .center()
        .visible(false);

    match builder.build() {
        Ok(win) => Some(win),
        Err(e) => {
            log::error!("[Window] 创建 mood 窗口失败: {e}");
            None
        }
    }
}

/// 打开心境窗口：不存在则创建，已存在则显示并聚焦。
/// 使用 async 命令（与其它开窗命令保持一致），避免同步命令在阻塞线程操作窗口导致死锁。
#[tauri::command]
async fn open_mood_window(app: tauri::AppHandle) -> Result<(), String> {
    let Some(win) = ensure_mood_window(&app) else {
        return Err("创建心境窗口失败".to_string());
    };
    win.show().map_err(|e| format!("显示心境窗口失败: {}", e))?;
    let _ = win.set_focus();
    Ok(())
}

/// 设置当前进程效能模式（EcoQoS，即任务管理器中的"小绿叶"）。
/// 开启后 Windows 会主动限制该进程的 CPU/功耗，适合后台/最小化状态。
#[cfg(windows)]
fn set_efficiency_mode(enable: bool) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, SetProcessInformation,
        ProcessPowerThrottling,
        PROCESS_POWER_THROTTLING_STATE,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_SET_INFORMATION,
    };
    use windows::Win32::Foundation::GetLastError;

    let pid = std::process::id();
    unsafe {
        match OpenProcess(PROCESS_SET_INFORMATION, false, pid) {
            Ok(handle) => {
                // ControlMask / StateMask 直接用 u32 数值，避免 newtype 隐式转换问题
                // PROCESS_POWER_THROTTLING_EXECUTION_SPEED = 0x1
                const EXECUTION_SPEED: u32 = 0x1;

                let state = PROCESS_POWER_THROTTLING_STATE {
                    Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                    ControlMask: EXECUTION_SPEED,
                    StateMask: if enable { EXECUTION_SPEED } else { 0 },
                };

                match SetProcessInformation(
                    handle,
                    ProcessPowerThrottling,
                    &state as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
                ) {
                    Ok(_) => log::info!(
                        "[EcoQoS] 效能模式 {} 成功 (pid={})",
                        if enable { "开启 (小绿叶)" } else { "关闭" },
                        pid,
                    ),
                    Err(e) => log::error!(
                        "[EcoQoS] SetProcessInformation 失败: {e} (last_error={})",
                        GetLastError().0,
                    ),
                }

                let _ = CloseHandle(handle);
            }
            Err(e) => {
                log::error!("[EcoQoS] OpenProcess 失败: {e} (pid={})", pid);
            }
        }
    }
}

/// 通知前端主窗口可见性变化。仅在状态切换时发送，避免重复事件刷屏。
/// 同时自动切换系统效能模式：隐藏时开启 EcoQoS 降功耗，恢复时关闭。
pub fn emit_main_visibility<R: tauri::Runtime>(app: &tauri::AppHandle<R>, visible: bool) {
    use tauri::Emitter;
    if MAIN_WINDOW_VISIBLE.swap(visible, Ordering::SeqCst) != visible {
        let _ = app.emit("window-visibility-changed", visible);

        #[cfg(windows)]
        {
            if visible {
                set_efficiency_mode(false);
            } else {
                // 延时 3 秒，避免刚最小化又恢复时反复切换
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    if !MAIN_WINDOW_VISIBLE.load(Ordering::SeqCst) {
                        set_efficiency_mode(true);
                    }
                });
            }
        }
    }
}

/// 当前是否处于开机自启(--autostart)模式。
/// 前端在主窗口加载完成后据此决定是否调用 minimize_to_tray 隐藏到托盘。
#[tauri::command]
fn is_autostart_mode() -> bool {
    AUTOSTART_MODE.load(Ordering::SeqCst)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 重复启动时：统一走托盘打开主窗口入口（恢复任务栏、若处于离屏预热则归位到屏幕内）
            crate::tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_os::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::ShortcutState;
                    if event.state == ShortcutState::Pressed {
                        // 全部热键总开关关闭时，忽略所有全局热键
                        if !hotkey::is_hotkeys_enabled() {
                            return;
                        }
                        if shortcut.id() == hotkey::get_overlay_shortcut_id() {
                            let _ = overlay_panel::toggle_overlay(app);
                        } else if shortcut.id() == hotkey::get_crosshair_shortcut_id() {
                            let _ = crosshair::toggle_crosshair_sync(app);
                        } else if shortcut.id() == hotkey::get_filter_shortcut_id() {
                            let _ = display_filter::toggle_filter_sync(app);
                        } else if shortcut.id() == hotkey::get_autoclicker_shortcut_id() {
                            let _ = autoclicker::toggle(app);
                        } else if shortcut.id() == hotkey::get_music_prev_shortcut_id() {
                            hotkey::trigger_music_action(app, "prev");
                        } else if shortcut.id() == hotkey::get_music_next_shortcut_id() {
                            hotkey::trigger_music_action(app, "next");
                        } else if shortcut.id() == hotkey::get_music_playpause_shortcut_id() {
                            hotkey::trigger_music_action(app, "play-pause");
                        } else if shortcut.id() == hotkey::get_lyrics_btn_toggle_shortcut_id() {
                            use tauri::Emitter;
                            let _ = app.emit("lyrics:toggle-hide-unlock-btn", ());
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 初始化显示滤镜的会话监控窗口（捕获系统关机/注销广播，避免退出时 xcalib 报错）
            display_filter::init_session_watch();
            // 载入用户配置的社区工具下载位置
            community_tools::init_community_download_dir(app.handle());
            // 初始化音乐 API 和音频代理
            let app_handle_for_music = app.handle().clone();
            music_api::audio_proxy::set_app_handle(app_handle_for_music.clone());
            tauri::async_runtime::spawn(async move {
                music_api::init_cookie_cache(&app_handle_for_music).await;
                match music_api::audio_proxy::start_audio_proxy().await {
                    Ok(port) => log::info!("[MusicAPI] audio proxy started on port {port}"),
                    Err(e) => log::error!("[MusicAPI] failed to start audio proxy: {e}"),
                }
            });

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            } else {
                // Release 模式：日志写入文件，便于排查开机自启失败问题
                // 日志路径：%LOCALAPPDATA%/NexBox/nexbox.log
                let log_dir = dirs::data_local_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("NexBox");
                let _ = std::fs::create_dir_all(&log_dir);
                let log_path = log_dir.join("nexbox.log");
                if let Ok(log_file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                {
                    let _ = env_logger::Builder::from_env(
                        env_logger::Env::default().default_filter_or("info"),
                    )
                    .target(env_logger::Target::Pipe(Box::new(log_file)))
                    .try_init();
                }
                log::info!(
                    "NexBox v{} 启动 | exe: {:?} | cwd: {:?}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::current_exe().ok(),
                    std::env::current_dir().ok(),
                );
            }
            sensor::start_sensor_process(app);

            // 桌面歌词窗口：安装 WM_MOVING 拦截，拖动时实时钳制窗口在工作区内
            // （碰撞体），任何部分都不允许移出屏幕外或压住任务栏。
            let _ = utils::cursor::install_lyrics_move_clamp(app.handle());

            utils::sys_info::check_and_send_statistics(app);
            overlay_panel::start_hardware_poller();
            hardware_report::start_recording();

            // 初始化 ACE / 反作弊自动检测（读取持久化配置并启动后台任务）
            let app_handle = app.handle().clone();
            let app_handle_anticheat = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = optimization::init_ace_auto_detect(app_handle).await;
                let _ = anticheat::init(app_handle_anticheat).await;
            });

            // 启动时自动应用已保存的 CPU 调度规则
            let app_handle_for_rules = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                cpu_scheduler::apply_all_saved_rules(&app_handle_for_rules).await;
            });

            // 初始化游戏滤镜自动应用（读取持久化配置并启动后台轮询）
            let app_handle_for_game_filter = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = game_filter::init(app_handle_for_game_filter).await;
            });

            // 初始化游戏启动时自动清理内存（读取持久化配置并启动后台轮询）
            let app_handle_for_game_start_clean = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = optimization::init_game_start_clean(app_handle_for_game_start_clean).await;
            });

            // 初始化游戏启动时禁用 Win 键（读取持久化配置并启动后台轮询）
            let app_handle_for_game_win_key = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = game_win_key::init(app_handle_for_game_win_key).await;
            });

            // 初始化游戏启动时锁定输入法（读取持久化配置并启动后台轮询）
            let app_handle_for_game_ime_lock = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = game_ime_lock::init(app_handle_for_game_ime_lock).await;
            });

            // 初始化游戏进程优化（恢复持久化配置，首次预置三角洲；启动自动优化线程）
            let app_handle_for_game_opt = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = game_process_optimize::init(app_handle_for_game_opt).await;
            });

            // 初始化游戏模式（读取持久化配置并启动后台扫描/压制线程）
            let app_handle_for_game_mode = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let _ = game_mode::init(app_handle_for_game_mode).await;
            });

            // 启动时自动清理旧版开机自启残留（计划任务/启动快捷方式），
            // 确保只保留当前的注册表 Run 键启动项
            auto_start::cleanup_legacy_auto_start();

            // Main window: intercept taskbar Close / Alt+F4 → hide instead of destroy，
            // 并通知前端窗口可见性变化（最小化/隐藏到托盘时暂停动态背景视频，降低 CPU 占用）
            if let Some(main_window) = app.get_webview_window("main") {
                let main_clone = main_window.clone();
                main_window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::CloseRequested { api, .. } => {
                            api.prevent_close();
                            main_window::save_current_position(&main_clone.app_handle());
                            let _ = main_clone.hide();
                            emit_main_visibility(&main_clone.app_handle(), false);
                        }
                        tauri::WindowEvent::Resized(_) => {
                            let minimized = main_clone.is_minimized().unwrap_or(false);
                            emit_main_visibility(&main_clone.app_handle(), !minimized);
                        }
                        _ => {}
                    }
                });
            }

            // Tray menu: hide when losing focus (click outside), reset always-on-top
            if let Some(tray_menu) = app.get_webview_window("tray-menu") {
                let menu_clone = tray_menu.clone();
                tray_menu.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        let _ = menu_clone.set_always_on_top(false);
                        let _ = menu_clone.hide();
                    }
                });
            }

            // 显示器信息改为前端按需加载 (get_displays 内部已有缓存逻辑)，
            // 不在启动阶段预填，避免阻塞 WebView 加载导致白屏。

            match tray::init_tray(app.handle()) {
                Ok(_) => log::info!("Tray initialized successfully"),
                Err(e) => log::error!("Failed to initialize tray: {}", e),
            }

            // 开机自启模式（--autostart 参数）：主窗口在 tauri.conf.json 中已设 visible:false。
            // 采用「离屏预热」：将主窗口移到屏幕外并显示，强制 WebView2 初始化加载前端（窗口真实存在
            // 但位于屏幕外且临时跳过任务栏，对用户完全不可见、零弹窗）；前端加载完成后自行调用
            // minimize_to_tray 隐藏到托盘；托盘打开时再由 ensure_main_onscreen 归位到屏幕内。
            let is_autostart = std::env::args().any(|a| a == "--autostart");
            AUTOSTART_MODE.store(is_autostart, Ordering::SeqCst);

            if let Some(main_window) = app.get_webview_window("main") {
                if is_autostart {
                    let _ = main_window.set_skip_taskbar(true);
                    let _ = main_window.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition { x: -30000, y: -30000 },
                    ));
                    let _ = main_window.show();
                    log::info!("开机自启模式：主窗口离屏预热，前端初始化后隐藏到托盘");

                    // 开机自启诊断（只记日志，不自愈）：若一段时间后前端(WebView2)仍未就绪，
                    // 说明出现“只启动后端、前端未起来”的情况，便于从 %LOCALAPPDATA%/NexBox/nexbox.log 定位。
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                        if !AUTOSTART_FRONTEND_READY.load(Ordering::SeqCst) {
                            log::warn!(
                                "[autostart] 前端 10 秒内未就绪：疑似只启动了后端、前端(WebView2)未加载，请检查该机 WebView2 运行时/GPU/登录时序"
                            );
                        }
                    });
                } else {
                    // 正常启动：确保非跳过任务栏、恢复上次位置（无记录则居中）、正常显示
                    let _ = main_window.set_skip_taskbar(false);
                    main_window::restore_position(app.handle());
                    let _ = main_window.show();
                    emit_main_visibility(app.handle(), true);
                    log::info!("正常启动：主窗口已显示");
                }
            }

            // 提前从持久化存储加载悬浮框设置，确保快捷键触发时使用已保存的配置而非默认值
            overlay_panel::try_load_persisted_settings(app.handle());

            // 启动时直接从持久化配置读取用户设置的快捷键并注册，
            // 不再依赖前端启动后覆盖，避免用户自定义热键重启后失效
            let overlay_hotkey = hotkey::load_saved_hotkey(app.handle(), "overlay-hotkey", "Shift+F10");
            let crosshair_hotkey = hotkey::load_saved_hotkey(app.handle(), "crosshair-hotkey", "Shift+F9");
            let filter_hotkey = hotkey::load_saved_hotkey(app.handle(), "filter-hotkey", "Shift+F8");
            let autoclicker_hotkey = hotkey::load_saved_hotkey(app.handle(), "autoclicker-hotkey", "F8");
            let music_prev_hotkey = hotkey::load_saved_hotkey(app.handle(), "music-prev-hotkey", "Alt+[");
            let music_next_hotkey = hotkey::load_saved_hotkey(app.handle(), "music-next-hotkey", "Alt+]");
            let music_playpause_hotkey = hotkey::load_saved_hotkey(app.handle(), "music-playpause-hotkey", "Alt+Space");
            let lyrics_btn_hotkey = hotkey::load_saved_hotkey(app.handle(), "lyrics-btn-hotkey", "");
            hotkey::set_hotkeys_enabled(hotkey::load_saved_hotkeys_enabled(app.handle()));

            // 恢复每个热键的独立开关（在注册热键前设置，使 apply_hotkeys_enabled 能正确判断）
            hotkey::set_overlay_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "overlay-hotkey-enabled"));
            hotkey::set_crosshair_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "crosshair-hotkey-enabled"));
            hotkey::set_filter_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "filter-hotkey-enabled"));
            hotkey::set_autoclicker_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "autoclicker-hotkey-enabled"));
            hotkey::set_music_prev_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "music-prev-hotkey-enabled"));
            hotkey::set_music_next_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "music-next-hotkey-enabled"));
            hotkey::set_music_playpause_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "music-playpause-hotkey-enabled"));
            hotkey::set_lyric_btn_enabled(hotkey::load_saved_hotkey_enabled(app.handle(), "lyrics-btn-hotkey-enabled"));

            let _ = hotkey::init_overlay(app.handle(), &overlay_hotkey);
            let _ = hotkey::init_crosshair(app.handle(), &crosshair_hotkey);
            let _ = hotkey::init_filter(app.handle(), &filter_hotkey);
            let _ = hotkey::init_autoclicker(app.handle(), &autoclicker_hotkey);
            let _ = hotkey::init_music_prev(app.handle(), &music_prev_hotkey);
            let _ = hotkey::init_music_next(app.handle(), &music_next_hotkey);
            let _ = hotkey::init_music_playpause(app.handle(), &music_playpause_hotkey);
            let _ = hotkey::init_lyrics_btn_toggle(app.handle(), &lyrics_btn_hotkey);

            // 若保存的总开关为“关闭”，上面已注册的热键需立即注销以释放按键
            if !hotkey::is_hotkeys_enabled() {
                hotkey::apply_hotkeys_enabled(app.handle(), false);
            }

            // 启动外部客户端播放的后台系统媒体会话轮询
            external_player::start(app.handle().clone());

            // 注册本应用内置播放器的 SMTC 媒体会话（音量浮层/锁屏显示「新境盒」并支持媒体键控制）
            smtc::start(app.handle().clone());

            // 键盘物理媒体键统一捕获（前台聚焦时 WebView2 会抢键，必须用低层钩子在 WebView2 之前接管）
            media_keys::start(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
        announcement::get_announcements,
        announcement::get_important_announcements,
        auto_start::set_nexbox_auto_start,
        auto_start::check_nexbox_auto_start,
        hardware::get_hardware,
        hardware::get_cpu_load,
        hardware::get_gpu_status,
        hardware::get_disk_status,
        hardware::is_nvidia_gpu,
        hardware::get_os_version,
        hardware::get_disk_health_info,
        disk_optimize::optimize_disk,
        music::get_music_files,
        music::import_local_music,
        music::import_local_music_folder,
        music::get_local_lyric,
        music::verify_local_covers,
        music::delete_cover_cache_files,
        // === 音乐播放器 API ===
        music_api::music_search,
        music_api::music_song_url,
        music_api::music_login_qr_key,
        music_api::music_login_qr_create,
        music_api::music_login_qr_check,
        music_api::music_login_status,
        music_api::music_login_cookie,
        music_api::music_logout,
        music_api::music_user_playlist,
        music_api::music_playlist_tracks,
        music_api::music_playlist_tracks_range,
        music_api::music_playlist_info_with_track_ids,
        music_api::music_playlist_detail,
        music_api::music_likelist,
        music_api::music_like,
        music_api::music_playlist_subscribe,
        external_player::external_player_state,
        external_player::external_control,
        smtc::smtc_update_state,
        smtc::smtc_clear,
        music_api::music_lyric,
        music_api::music_song_comments,
        music_api::music_send_comment,
        music_api::music_personalized,
        music_api::music_recommend_songs,
        music_api::music_recommend_resource,
        music_api::music_simi_song,
        music_api::music_artist_search,
        music_api::music_artist_songs,
        music_api::music_artist_detail,
        music_api::music_artist_albums,
        music_api::music_artist_mvs,
        music_api::music_album_detail,
        music_api::music_mv_url,
        music_api::music_playlist_search,
        music_api::music_open_login_window,
        // === 酷狗音乐 API ===
        music_api::kugou_search,
        music_api::kugou_artist_search,
        music_api::kugou_playlist_search,
        music_api::kugou_artist_songs,
        music_api::kugou_song_url,
        music_api::kugou_lyric,
        music_api::kugou_login_status,
        music_api::kugou_login_cookie,
        music_api::kugou_logout,
        music_api::kugou_user_playlists,
        music_api::kugou_playlist_tracks,
        music_api::kugou_playlist_tracks_range,
        music_api::kugou_guess_like,
        music_api::kugou_rank_list,
        music_api::kugou_rank_songs,
        music_api::kugou_like_toggle,
        music_api::kugou_liked_hashes,
        // === QQ 音乐 API ===
        music_api::qq_search,
        music_api::qq_song_url,
        music_api::qq_lyric,
        music_api::qq_login_status,
        music_api::qq_login_cookie,
        music_api::qq_logout,
        music_api::qq_user_playlists,
        music_api::qq_playlist_tracks,
        music_api::qq_playlist_tracks_range,
        music_api::qq_artist_search,
        music_api::qq_artist_songs,
        music_api::qq_playlist_search,
        music_api::qq_rank_list,
        music_api::qq_rank_songs,
        music_api::music_qq_recommend_playlists,
        music_api::qq_liked_hashes,
        music_api::qq_like_toggle,
        // === 多平台管理 ===
        music_api::music_get_login_statuses,
        music_api::music_switch_provider,
        music_api::music_get_playback_source,
        music_api::audio_proxy::cmd_get_proxy_port,
        downloader::download_file,
        downloader::open_system_browser,
        downloader::open_installer,
        downloader::download_update,
        downloader::install_update,
        downloader::delete_download_file,
        downloader::mark_pending_install,
        downloader::clear_pending_install,
        downloader::cancel_download,
        downloader::reset_download_cancel,
        // === 下载加速器（FluxDown 引擎移植） ===
        download_accelerator::accel_start,
        download_accelerator::accel_pause,
        download_accelerator::accel_resume,
        download_accelerator::accel_cancel,
        download_accelerator::accel_list,
        download_accelerator::accel_clear_learned,
        download_accelerator::accel_set_speed_limit,
        download_accelerator::accel_scan_unfinished,
        download_accelerator::accel_open_file,
        download_accelerator::accel_reveal_file,
        optimization::optimize_memory,
        optimization::get_memory_status,
        optimization::kill_wallpaper_engine,
        optimization::flush_dns,
        optimization::clean_temp_files,
        optimization::optimize_privacy_services,
        optimization::optimize_ace_processes,
        optimization::set_high_performance_power_plan,
        optimization::get_memory_limit_options,
        optimization::get_memory_limit_status,
        optimization::set_memory_limit,
        optimization::restore_memory_limit,
        optimization::get_detailed_memory_status,
        optimization::clean_standby_memory,
        optimization::trim_system_working_set,
        optimization::start_auto_clean,
        optimization::stop_auto_clean,
        optimization::get_auto_clean_config,
        optimization::get_game_start_clean_config,
        optimization::set_game_start_clean_config,
        optimization::get_pagefile_status,
        optimization::set_pagefile,
        optimization::boost_delta_force_priority,
        optimization::boost_delta_force_affinity,
        optimization::boost_delta_force_affinity_with_mask,
        optimization::limit_ace_priority,
        optimization::restrict_ace_affinity,
        optimization::restrict_ace_affinity_with_mask,
        optimization::set_ace_efficiency_mode,
        optimization::apply_ace_registry_limits,
        optimization::restore_ace_registry_limits,
        optimization::optimize_all_game_processes,
        optimization::set_ace_auto_detect,
        optimization::get_ace_auto_detect_status,
        optimization::init_ace_auto_detect,
        anticheat::anticheat_get_groups,
        anticheat::anticheat_limit_priority,
        anticheat::anticheat_restrict_affinity,
        anticheat::anticheat_set_efficiency,
        anticheat::anticheat_apply_registry,
        anticheat::anticheat_restore_registry,
        anticheat::anticheat_set_auto_detect,
        anticheat::anticheat_get_auto_detect_status,
        optimization::get_builtin_power_plans,
        optimization::get_system_power_plans,
        optimization::get_active_power_plan,
        optimization::get_laptop_power_lock_status,
        optimization::unlock_laptop_power_plan,
        optimization::import_power_plan,
        optimization::activate_power_plan,
        optimization::import_and_activate_power_plan,
        // === 处理器电源高级设置 ===
        power_settings::get_power_advanced_settings,
        power_settings::set_power_advanced_setting,
        power_settings::unhide_power_advanced_settings,
        optimization::apply_registry_tweak,
        optimization::restore_registry_tweak,
        optimization::batch_apply_registry_tweaks,
        optimization::batch_restore_registry_tweaks,
        optimization::scan_registry_tweaks,
        optimization::disable_windows_update,
        optimization::enable_windows_update,
        optimization::check_windows_update_state,
        optimization::check_pause_update_state,
        optimization::check_defender_state,
        optimization::delete_power_plan,
        optimization::get_peripheral_status,
        optimization::set_peripheral_settings,
        optimization::reset_peripheral_settings,
        optimization::restart_graphics_driver,
        network_optimize::set_tcp_congestion,
        network_optimize::restore_tcp_congestion,
        network_optimize::set_tcp_chimney_off,
        network_optimize::restore_tcp_chimney,
        network_optimize::set_nagle_optimization,
        network_optimize::restore_nagle_optimization,
        network_optimize::set_adapter_power_saving_off,
        network_optimize::restore_adapter_power_saving,
        network_optimize::set_dns_servers,
        network_optimize::restore_dns_servers,
        network_optimize::clear_dns_cache,
        network_optimize::reset_network,
        network_optimize::fix_dhcp,
        network_optimize::check_network_tweak_states,
        network_optimize::batch_network_enable,
        network_optimize::batch_network_disable,
        network_optimize::get_public_ip,
        startup_manager::scan_startup_items,
        startup_manager::disable_startup_item,
        startup_manager::enable_startup_item,
        startup_manager::locate_startup_file,
        startup_manager::find_startup_key_in_registry,
        startup_manager::get_startup_item_icon,
        startup_manager::get_process_icons,
        service_manager::scan_services,
        service_manager::set_service_start_type,
        service_manager::is_app_admin,
        context_menu::scan_context_menu_items,
        context_menu::hide_context_menu_item,
        context_menu::restore_context_menu_item,
        context_menu::scan_this_pc_items,
        context_menu::hide_this_pc_item,
        context_menu::restore_this_pc_item,
        context_menu::scan_drives,
        context_menu::hide_drive,
        context_menu::restore_drive,
        display_filter::get_displays,
        display_filter::set_active_display,
        display_filter::check_gamma_support,
        display_filter::get_filter_settings,
        display_filter::set_filter_settings,
        display_filter::enable_filter,
        display_filter::disable_filter,
        display_filter::toggle_filter,
        display_filter::get_filter_presets,
        display_filter::apply_preset,
        display_filter::get_custom_filter_settings,
        display_filter::save_custom_filter_settings,
        display_filter::export_custom_filter,
        display_filter::get_user_filter_presets,
        display_filter::save_user_filter_preset,
        display_filter::apply_user_filter_preset,
        display_filter::delete_user_filter_preset,
        display_filter::select_icc_file,
        display_filter::import_icc_profile,
        display_filter::get_icc_presets,
        display_filter::apply_icc_preset,
        display_filter::delete_icc_preset,
        display_filter::export_preset_as_icc,
        display_filter::apply_filter_stack,
        display_filter::restore_filter_state,
        game_filter::get_game_filter_status,
        game_filter::set_game_filter_enabled,
        game_filter::add_custom_game,
        game_filter::remove_custom_game,
        // === 高级设置 ===
        advanced::get_storage_sizes,
        advanced::clear_cache,
        advanced::clear_data,
        advanced::restart_app,
        // === 游戏进程优化 ===
        game_process_optimize::get_game_optimize_configs,
        game_process_optimize::save_game_optimize_configs,
        game_process_optimize::optimize_game_priority,
        game_process_optimize::optimize_game_affinity,
        game_process_optimize::apply_game_ifeo,
        game_process_optimize::restore_game_ifeo,
        game_process_optimize::set_game_auto_optimize,
        game_process_optimize::get_game_auto_optimize_status,
        game_process_optimize::select_game_executable,
        game_process_optimize::list_running_processes,
        game_process_optimize::check_game_optimize_admin,
        game_process_optimize::get_affinity_topology,
        game_mode::game_mode_get_config,
        game_mode::game_mode_set_preset,
        game_mode::game_mode_set_manual,
        game_mode::game_mode_set_auto,
        game_mode::game_mode_set_auto_preset,
        game_mode::game_mode_get_status,
        game_win_key::get_game_win_key_status,
        game_win_key::set_game_win_key_enabled,
        game_ime_lock::get_game_ime_lock_status,
        game_ime_lock::set_game_ime_lock_enabled,
        // === EQ 调音命令 ===
        audio_eq::check_virtual_audio_driver,
        audio_eq::install_virtual_audio_driver,
        audio_eq::uninstall_virtual_audio_driver,
        audio_eq::start_eq_engine,
        audio_eq::stop_eq_engine,
        audio_eq::get_eq_engine_status,
        audio_eq::get_eq_presets,
        audio_eq::apply_eq_preset,
        audio_eq::import_eq_preset,
        audio_eq::delete_eq_preset,
        audio_eq::save_eq_preset,
        audio_eq::export_fac_file,
        audio_eq::get_audio_levels,
        audio_eq::get_spectrum,
        audio_eq::get_default_audio_device,
        audio_eq::list_audio_devices,
        audio_eq::update_eq_bands,
        audio_eq::update_eq_preamp,
        audio_eq::update_eq_effects,
        audio_eq::get_eq_effects,
        thirdparty_tools::get_thirdparty_tools,
        thirdparty_tools::get_tool_install_path,
        thirdparty_tools::get_tool_download_path,
        thirdparty_tools::run_tool,
        thirdparty_tools::download_tool,
        thirdparty_tools::open_tool_installer,
        // === 社区工具（GitCode PR） ===
        community_tools::get_community_tools,
        community_tools::get_community_categories,
        community_tools::search_community_tools,
        community_tools::get_community_install_status,
        community_tools::invalidate_community_cache,
        community_tools::gitcode_login_start,
        community_tools::get_gitcode_login_status,
        community_tools::get_gitcode_avatar_data,
        community_tools::gitcode_logout,
        community_tools::submit_community_tool,
        community_tools::delete_community_tool,
        community_tools::get_community_download_dir,
        community_tools::set_community_download_dir,
        community_tools::pick_community_download_dir,
        community_tools::install_community_tool,
        community_tools::open_community_zip,
        community_tools::run_community_tool,
        community_tools::pick_community_package,
        community_tools::list_zip_entry_exes,
        community_tools::pick_community_icon,
        overlay_panel::start_overlay_panel,
        overlay_panel::stop_overlay_panel,
        overlay_panel::get_overlay_panel_status,
        overlay_panel::set_active_gpu_index,
        overlay_panel::get_overlay_hardware_data,
        overlay_panel::update_overlay_settings,
        overlay_panel::toggle_overlay_panel,
        overlay_panel::set_overlay_drag_mode,
        overlay_panel::get_overlay_current_settings,
        overlay_panel::check_drag_mode_status,
        overlay_panel::reset_overlay_position,
        pawnio_driver::check_pawnio_status,
        pawnio_driver::install_pawnio_driver,
        pawnio_driver::uninstall_pawnio_driver,

        vertical_overlay::start_vertical_overlay,
        vertical_overlay::stop_vertical_overlay,
        vertical_overlay::vertical_overlay_ready,
        vertical_overlay::save_vertical_overlay_position,
        vertical_overlay::set_vertical_overlay_click_through,
        vertical_overlay::reset_vertical_overlay_position,
        vertical_overlay::resize_vertical_overlay,

        hardware_report::export_hardware_report,
        hardware_report::get_hardware_recording_status,
        hardware_report::clear_hardware_data,

        sensor::get_lhm_cpu_load,
        sensor::get_lhm_cpu_status,
        sensor::get_lhm_gpu_status,
        sensor::restart_monitor_process,
        sensor_monitor::open_sensor_monitor,
        sensor_monitor::get_all_sensors,

        game_ping::get_current_ping,
        hotkey::get_overlay_hotkey,
        hotkey::set_overlay_hotkey,
        hotkey::get_crosshair_hotkey,
        hotkey::set_crosshair_hotkey,
        hotkey::get_filter_hotkey,
        hotkey::set_filter_hotkey,
        hotkey::get_autoclicker_hotkey,
        hotkey::set_autoclicker_hotkey,
        hotkey::get_music_prev_hotkey,
        hotkey::set_music_prev_hotkey,
        hotkey::get_music_next_hotkey,
        hotkey::set_music_next_hotkey,
        hotkey::get_music_playpause_hotkey,
        hotkey::set_music_playpause_hotkey,
        hotkey::get_lyrics_btn_hotkey,
        hotkey::set_lyrics_btn_hotkey,
        hotkey::set_hotkeys_enabled_cmd,
        hotkey::get_hotkeys_enabled_cmd,
        hotkey::get_overlay_hotkey_enabled,
        hotkey::set_overlay_hotkey_enabled,
        hotkey::get_crosshair_hotkey_enabled,
        hotkey::set_crosshair_hotkey_enabled,
        hotkey::get_filter_hotkey_enabled,
        hotkey::set_filter_hotkey_enabled,
        hotkey::get_autoclicker_hotkey_enabled,
        hotkey::set_autoclicker_hotkey_enabled,
        hotkey::get_music_prev_hotkey_enabled,
        hotkey::set_music_prev_hotkey_enabled,
        hotkey::get_music_next_hotkey_enabled,
        hotkey::set_music_next_hotkey_enabled,
        hotkey::get_music_playpause_hotkey_enabled,
        hotkey::set_music_playpause_hotkey_enabled,
        autoclicker::autoclicker_start,
        autoclicker::autoclicker_stop,
        autoclicker::autoclicker_toggle,
        autoclicker::autoclicker_update,
        autoclicker::autoclicker_get_status,
        crosshair::toggle_crosshair,
        crosshair::get_crosshair_status,
        crosshair::update_crosshair_settings,
        crosshair::get_crosshair_displays,
        crosshair::pick_crosshair_image,
        crosshair::get_preset_crosshair_path,
        crosshair::get_crosshair_presets,

        delta_force::get_delta_passwords,
        delta_force::cache_delta_image,
        delta_force::get_weapon_codes,
        delta_force::get_dlss_model_presets,
        delta_force::apply_dlss_model_preset,
        delta_force::get_dlss_preset_status,
        delta_force::get_delta_maps,
        delta_force::toggle_dlss_indicator,
        delta_force::toggle_dlss_lock,
        delta_force::get_dlss_settings_status,
        delta_force::open_platform_window,
        open_mood_window,
        game_launcher::launch_game,
        game_launcher::search_delta_force_launcher,
        game_launcher::get_default_delta_force_game,
        game_launcher::select_exe_file,
        game_launcher::get_file_icon,
        gpu_rename::get_gpu_info,
        gpu_rename::get_gpu_list,
        gpu_rename::get_gpu_options,
        gpu_rename::apply_gpu_rename,
        gpu_rename::restore_gpu_name,
        video_bg::pick_video_file,
            sponsor::get_sponsors,
            contributor::get_contributors,
        qq_group::get_qq_groups,
        qq_group::get_qq_group_icon,
        ads::get_ads,
        ads::get_ad_image,
        shader_cache::scan_shader_caches,
        shader_cache::clean_shader_cache,
        nvapi::get_nvapi_status,
        nvapi::diagnose_nvapi,
        nvapi::get_nvidia_driver_version,
        nvapi::list_nvidia_settings,
        nvapi::set_nvidia_setting,
        nvapi::reset_nvidia_settings,
        nvapi::list_nvidia_displays,
        nvapi::get_nvidia_display_modes,
        nvapi::set_nvidia_display_resolution,
        nvapi::get_injected_resolutions,
        nvapi::remove_injected_resolution,
        // === NVIDIA 驱动下载 ===
        nvidia_driver_download::fetch_nvidia_drivers,
        nvidia_driver_download::detect_current_nvidia_gpu,
        // === 运行库补全/修复 ===
        runtime_repair::get_runtime_statuses,
        runtime_repair::repair_runtime,
        // === VT-X 虚拟化修复 ===
        vtx_virtualization::check_vtx_virtualization_status,
        vtx_virtualization::fix_vtx_virtualization_popup,
        vtx_virtualization::restore_vtx_virtualization,
            storage_clean::scan_storage_items,
            storage_clean::clean_storage_items,
            storage_clean::empty_recycle_bin_cmd,
            // === 垃圾清理 / 大文件扫描(移植自 light-c-main) ===
            storage_scan::scan_junk_categories,
            storage_scan::scan_junk_category,
            storage_scan::get_junk_categories,
            storage_scan::delete_junk_files,
            storage_scan::scan_large_files,
            storage_scan::cancel_large_file_scan,
            storage_scan::reveal_large_file,
            storage_scan::delete_large_file,
            storage_scan::get_drive_list,
            utils::sys_info::get_system_locale,
            utils::sys_info::get_system_username,
            tray::minimize_to_tray,
            tray::show_window,
            tray::get_close_behavior,
            tray::set_close_behavior,
            tray::get_dont_ask_again,
            tray::set_dont_ask_again,
            tray::exit_app,
            tray::check_update_and_show,
            is_autostart_mode,
            // === MCTier 命令 ===
            utils::cursor::get_cursor_position,
            utils::cursor::set_desktop_lyrics_click_through,
            utils::cursor::clamp_lyrics_window_position,
            utils::cursor::center_lyrics_window,
            system_fonts::get_system_fonts,
            utils::lyrics_btn::show_lyrics_unlock_btn,
            utils::lyrics_btn::hide_lyrics_unlock_btn,
                        utils::lyrics_btn::unlock_lyrics,

        // === CPU 核心调度 ===
        cpu_scheduler::get_cpu_topology,
        cpu_scheduler::get_process_list,
        cpu_scheduler::get_process_affinity,
        cpu_scheduler::set_process_affinity,
        cpu_scheduler::restore_process_affinity,
        cpu_scheduler::get_saved_rules,
        cpu_scheduler::save_rule,
        cpu_scheduler::delete_rule,
        cpu_scheduler::apply_rule_by_name,
        cpu_scheduler::apply_core_isolation,
        cpu_scheduler::restore_core_isolation,
        cpu_scheduler::get_isolation_state,
        cpu_scheduler::get_isolation_rules,
        cpu_scheduler::save_isolation_rule,
        cpu_scheduler::delete_isolation_rule,
        cpu_scheduler::apply_isolation_rule_by_name,

        // === Steam 集成 ===
        steam::get_steam_install_info,
        steam::get_steam_users,
        steam::get_steam_libraries,
        steam::get_steam_games,
        steam::get_steam_all_data,
        steam::launch_steam_client,
        steam::launch_steam_game,
        steam::open_steam_store_page,
        steam::open_game_folder,
        steam::switch_steam_account,
        steam::delete_steam_account,
        steam::uninstall_steam_game,
        steam::format_file_size,
        steam::get_steam_stats,
        steam::get_library_disk_info,
        steam::steam_debug,
        steam::get_steam_user_avatars,

        // === 网络测速 ===
        speedtest::start_speedtest,
        speedtest::stop_speedtest,
        speedtest::is_speedtest_running,
        speedtest::get_speedtest_servers,

        // === UAPI 随机图片 ===
        uapi::get_random_image,
        uapi::save_random_image_bytes,

        // === Windows 隐藏功能开关（移植自 ViVe） ===
        feature_flags::feature_flags_status,
        feature_flags::feature_flags_query,
        feature_flags::feature_flags_set,
        feature_flags::feature_flags_reset,
    ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        match event {
            tauri::RunEvent::ExitRequested { .. } => {
                // 若存在已下载未安装的更新包，退出前异步启动安装向导（不阻塞退出）
                // 后端兜底：静默更新已关闭时不启动安装向导
                if downloader::auto_update_enabled() {
                    if let Some(path) = downloader::take_pending_install() {
                        if std::path::Path::new(&path).exists() {
                            if let Err(e) = downloader::launch_installer_sync(&path) {
                                log::error!("[Update] 退出时自动启动安装向导失败: {e}");
                            }
                        }
                    }
                }
                // 退出流程开始前隐藏所有窗口，避免 WebView2 销毁后闪现原生标题栏
                for label in &["main", "tray-menu", "desktop-lyrics", "lyrics-unlock-btn", "vertical-overlay"] {
                    if let Some(w) = app_handle.get_webview_window(label) {
                        let _ = w.hide();
                    }
                }
            }
            tauri::RunEvent::Exit => {
                // 退出前兜底保存主窗口位置（与竖排悬浮窗 cleanup 对齐）
                main_window::save_current_position(app_handle);
                sensor::stop_sensor_process(app_handle);
                hardware::cleanup_hardware_cache();
                overlay_panel::cleanup(); // 先停后台轮询线程(FPS/传感器)，再恢复 Gamma
                game_win_key::cleanup();
                game_ime_lock::cleanup();
                speedtest::cleanup();
                display_filter::cleanup();
                game_mode::shutdown();
                vertical_overlay::cleanup(app_handle);
                crosshair::cleanup();
                autoclicker::cleanup();
                audio_eq::cleanup();
                tray::cleanup();
                hotkey::cleanup(app_handle);
                nvapi::cleanup();
                hardware_report::stop_recording();
            }
            _ => {}
        }
    });
}
