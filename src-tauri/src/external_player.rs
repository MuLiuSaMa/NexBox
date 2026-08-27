//! 外部客户端播放（SMTC 接管）
//!
//! 读取 Windows 系统媒体会话（`GlobalSystemMediaTransportControls`），把用户在任意外部
//! 音乐/视频客户端里正在播放的内容（封面/歌名/歌手/进度）实时推送给前端，并允许前端
//! 发送 播放/暂停/上一曲/下一曲/拖动进度 控制命令。
//!
//! 参考 eIsland 的 SMTC 实现思路，但在 Tauri 中直接复用 `windows` crate 的 WinRT 绑定，
//! 无需自带 C# Native DLL。相关 API 用法与 `netease_lyrics.rs` 保持一致。

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn start(_app: tauri::AppHandle) {}
    pub fn current_state() -> Option<crate::external_player::ExternalPlayback> { None }
    pub fn control(_action: &str, _value_ms: i64) {}
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Sender};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    use base64::Engine;
    use tauri::{AppHandle, Emitter};
    use windows::Media::Control::{
        CurrentSessionChangedEventArgs,
        GlobalSystemMediaTransportControlsSession,
        GlobalSystemMediaTransportControlsSessionManager,
        GlobalSystemMediaTransportControlsSessionMediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        SessionsChangedEventArgs,
    };
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::Streams::DataReader;

    use crate::external_player::ExternalPlayback;

    const TICK_PER_MS: i64 = 10_000;

    /// 仅接管音乐客户端的媒体会话（依据 SourceAppUserModelId 子串匹配，小写）。
    /// 抖音 App（明文 douyin）、浏览器（chrome/edge/firefox）等非音乐客户端不在列，
    /// 避免把视频/直播内容顶到灵动岛。
    const MUSIC_SOURCE_KEYS: &[&str] = &[
        // 网易云音乐
        "cloudmusic", "netease", "orpheus", "music.163", "163music",
        // QQ 音乐
        "qqmusic", "tencentmusic",
        // 酷狗 / 酷我
        "kugou", "kuwo",
        // Spotify / Apple Music / Zune(Groove)Music
        "spotify", "applemusic", "zunemusic", "groovemusic", "musicui",
        // 汽水音乐（抖音音乐）
        "sodamusic", "qishui", "douyinmusic",
    ];

    /// 控制命令通道
    struct ControlMsg {
        action: String,
        value_ms: i64,
    }

    static CTRL_TX: OnceLock<Sender<ControlMsg>> = OnceLock::new();
    static CURRENT: Mutex<Option<ExternalPlayback>> = Mutex::new(None);

    /// 启动后台轮询线程（在 Tauri setup 中调用一次）
    pub fn start(app: AppHandle) {
        thread::spawn(move || run_loop(app));
    }

    /// 读取最近一次缓存的播放状态（供前端进入页面时同步）
    pub fn current_state() -> Option<ExternalPlayback> {
        CURRENT.lock().ok().and_then(|s| s.clone())
    }

    /// 发送控制命令到外部客户端（非阻塞，由后台线程执行）
    pub fn control(action: &str, value_ms: i64) {
        if let Some(tx) = CTRL_TX.get() {
            let _ = tx.send(ControlMsg { action: action.to_string(), value_ms });
        }
    }

    /// 媒体键转发入口（键盘媒体键开关关闭、且焦点在新境盒时由低层钩子调用）：
    /// 把播放/暂停等命令直接发给当前外部音乐客户端的 SMTC 会话，
    /// 绕过 WebView2 聚焦时对 WM_APPCOMMAND 的吞噬
    pub fn forward_media_key(action: &str) {
        match CTRL_TX.get() {
            Some(tx) => {
                let _ = tx.send(ControlMsg { action: action.to_string(), value_ms: 0 });
            }
            None => log::warn!("[MediaKeys] 外部会话控制通道未就绪，丢弃媒体键转发: {action}"),
        }
    }

    fn run_loop(app: AppHandle) {
        // WinRT 需要线程具备 COM 公寓初始化（后台线程不会自动初始化）。
        // 不初始化时 RequestAsync 会直接失败，导致外部播放永不接管。
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_MULTITHREADED,
            );
        }

        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .and_then(|task| task.get())
        {
            Ok(m) => m,
            Err(e) => {
                log::warn!("初始化系统媒体会话管理器失败: {e}");
                return;
            }
        };
        log::info!("外部客户端播放（SMTC）管理器初始化成功");

        // 关键：GSTM 只有在订阅了会话变更事件后，GetSessions() 才会被系统填充。
        // 仅靠轮询 GetSessions()/GetCurrentSession() 会一直得到 0 个会话（各处实测）。
        // 因此订阅 CurrentSessionChanged / SessionsChanged，事件触发时立刻重新探测。
        let session_dirty = Arc::new(AtomicBool::new(false));

        let cur_handler =
            TypedEventHandler::<GlobalSystemMediaTransportControlsSessionManager, CurrentSessionChangedEventArgs>::new({
                let dirty = session_dirty.clone();
                move |_sender, _args| {
                    dirty.store(true, Ordering::SeqCst);
                    Ok(())
                }
            });
        let sess_handler =
            TypedEventHandler::<GlobalSystemMediaTransportControlsSessionManager, SessionsChangedEventArgs>::new({
                let dirty = session_dirty.clone();
                move |_sender, _args| {
                    dirty.store(true, Ordering::SeqCst);
                    Ok(())
                }
            });
        // 持有 handler 与订阅 token，避免委托被释放导致事件失效
        let _cur_token = manager.CurrentSessionChanged(&cur_handler);
        let _sess_token = manager.SessionsChanged(&sess_handler);
        let _ = (cur_handler, sess_handler);

        // 启动时枚举一次当前媒体会话，用于诊断 AUMID 与白名单匹配
        log_session_snapshot(&manager);

        let (tx, rx) = mpsc::channel::<ControlMsg>();
        let _ = CTRL_TX.set(tx);

        let mut current_session: Option<GlobalSystemMediaTransportControlsSession> = None;
        let mut cover_key: Option<String> = None;
        let mut last_poll = Instant::now();
        // 连续几拍没检测到才隐藏，避免偶发读取失败导致灵动岛开关闪烁
        let mut empty_streak: u32 = 0;
        const EMPTY_THRESHOLD: u32 = 3;

        loop {
            // 处理积压的控制命令
            while let Ok(msg) = rx.try_recv() {
                let mut target = current_session.as_ref().cloned();
                if target.is_none() {
                    // 兜底：尚未轮询到会话（如刚启动/刚切歌）时现场探测一次
                    if pick_session(&manager, &mut current_session).is_some() {
                        target = current_session.clone();
                    }
                }
                match target.as_ref() {
                    Some(session) => {
                        let aumid = session
                            .SourceAppUserModelId()
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        log::info!("[MediaKeys] 转发 {} → 会话 [{aumid}]", msg.action);
                        handle_control(session, &msg.action, msg.value_ms);
                    }
                    None => log::warn!(
                        "[MediaKeys] 无可用外部音乐会话，丢弃媒体键命令: {}（确认其他播放器正在运行且属于音乐白名单）",
                        msg.action
                    ),
                }
            }

            // 会话变更事件置脏，或多长时间未轮询，则重新探测并推送状态
            let due = last_poll.elapsed() >= Duration::from_millis(1000);
            let dirty = session_dirty.swap(false, Ordering::SeqCst);
            if due || dirty {
                last_poll = Instant::now();
                let state = poll(&manager, &mut current_session, &mut cover_key);
                if state.is_some() {
                    empty_streak = 0;
                } else {
                    empty_streak = empty_streak.saturating_add(1);
                }
                // 有实际状态、或仍在防抖窗口内时，维持当前显示；只有持续为空才清空
                let emit_state = if state.is_some() {
                    Some(state)
                } else if empty_streak >= EMPTY_THRESHOLD {
                    Some(None)
                } else {
                    None // 保持上一次状态，防抖
                };
                if let Some(out) = emit_state {
                    let mut guard = CURRENT.lock().unwrap();
                    *guard = out.clone();
                    drop(guard);
                    let _ = app.emit("external-player:state", out);
                }
            }

            thread::sleep(Duration::from_millis(200));
        }
    }

    /// 枚举并打印当前媒体会话（诊断用）
    fn log_session_snapshot(manager: &GlobalSystemMediaTransportControlsSessionManager) {
        let Ok(sessions) = manager.GetSessions() else {
            log::warn!("[SMTC] GetSessions 失败");
            return;
        };
        let size = sessions.Size().unwrap_or(0);
        log::info!("[SMTC] 当前媒体会话数: {size}");
        for i in 0..size {
            if let Ok(s) = sessions.GetAt(i) {
                let id = s.SourceAppUserModelId().map(|v| v.to_string()).unwrap_or_default();
                log::info!(
                    "[SMTC] 会话[{i}] AUMID={id} 命中白名单={}",
                    if is_music_source(&s) { "是" } else { "否" }
                );
            }
        }
    }

    fn handle_control(session: &GlobalSystemMediaTransportControlsSession, action: &str, value_ms: i64) {
        let _ = match action {
            "play-pause" => session.TryTogglePlayPauseAsync().and_then(|t| t.get()),
            "prev" => session.TrySkipPreviousAsync().and_then(|t| t.get()),
            "next" => session.TrySkipNextAsync().and_then(|t| t.get()),
            "seek" => {
                let target = value_ms * TICK_PER_MS;
                let r = session.TryChangePlaybackPositionAsync(target).and_then(|t| t.get());
                match &r {
                    Ok(true) => log::info!("[SMTC] seek 成功: {value_ms}ms"),
                    Ok(false) => log::warn!("[SMTC] seek 被平台拒绝: {value_ms}ms"),
                    Err(e) => log::warn!("[SMTC] seek 失败: {e} ({value_ms}ms)"),
                }
                r
            }
            _ => return,
        };
    }

    /// 轮询并构造一次播放状态快照
    fn poll(
        manager: &GlobalSystemMediaTransportControlsSessionManager,
        current_session: &mut Option<GlobalSystemMediaTransportControlsSession>,
        cover_key: &mut Option<String>,
    ) -> Option<ExternalPlayback> {
        let session = pick_session(manager, current_session)?;
        let media = session.TryGetMediaPropertiesAsync().and_then(|t| t.get()).ok()?;

        let title = media.Title().map(|v| v.to_string()).unwrap_or_default();
        if title.is_empty() {
            *cover_key = None;
            return None;
        }

        let artist = media.Artist().map(|v| v.to_string()).unwrap_or_default();
        let album = media.AlbumTitle().map(|v| v.to_string()).unwrap_or_default();

        let is_playing =
            matches!(session.GetPlaybackInfo().ok().and_then(|i| i.PlaybackStatus().ok()),
                Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing));

        // 进度/时长：TimeSpan 以 100ns tick 计，换算为毫秒
        let (position_ms, duration_ms) = match session.GetTimelineProperties() {
            Ok(timeline) => {
                let pos = timeline.Position().map(|p| p.Duration.max(0)).unwrap_or(0) / TICK_PER_MS;
                let dur = timeline.EndTime().map(|d| d.Duration.max(0)).unwrap_or(0) / TICK_PER_MS;
                (pos, dur)
            }
            Err(_) => (0, 0),
        };

        let source_app_id = session.SourceAppUserModelId().map(|v| v.to_string()).unwrap_or_default();

        // 封面：仅当曲目变化时重新拉取，避免高频重复下载
        let current_key = format!("{title}|{artist}|{album}");
        if cover_key.as_deref() != Some(current_key.as_str()) {
            log::info!(
                "[SMTC] 接管会话 AUMID={source_app_id} 歌曲={title} - {artist} 进度={position_ms}/{duration_ms}ms 播放={is_playing}"
            );
        }
        let cover = if cover_key.as_deref() != Some(current_key.as_str()) {
            let c = read_cover(&media);
            *cover_key = Some(current_key.clone());
            c
        } else {
            current_state().and_then(|s| s.track).and_then(|t| t.cover)
        };

        Some(ExternalPlayback {
            track: Some(crate::external_player::ExternalTrack {
                title,
                artist,
                album,
                cover,
                source_app_id,
            }),
            is_playing,
            position_ms,
            duration_ms,
        })
    }

    /// 选定当前播放会话：仅接受音乐客户端，优先「正在播放且有标题」的会话。
    fn pick_session(
        manager: &GlobalSystemMediaTransportControlsSessionManager,
        current_session: &mut Option<GlobalSystemMediaTransportControlsSession>,
    ) -> Option<GlobalSystemMediaTransportControlsSession> {
        // 当前系统会话是音乐客户端即采用（避免接管抖音/浏览器等非音乐会话）
        if let Ok(s) = manager.GetCurrentSession() {
            if is_music_source(&s) && has_title(&s) && !is_self_session(&s) {
                *current_session = Some(s.clone());
                return Some(s);
            }
        }

        // 遍历会话表：优先「正在播放的音乐客户端」，其次「有标题的音乐客户端」
        if let Ok(sessions) = manager.GetSessions() {
            let size = sessions.Size().unwrap_or(0);
            let mut fallback: Option<GlobalSystemMediaTransportControlsSession> = None;
            for i in 0..size {
                if let Ok(s) = sessions.GetAt(i) {
                    if !is_music_source(&s) || !has_title(&s) || is_self_session(&s) {
                        continue;
                    }
                    if is_playing(&s) {
                        *current_session = Some(s.clone());
                        return Some(s);
                    }
                    if fallback.is_none() {
                        fallback = Some(s);
                    }
                }
            }
            if let Some(s) = fallback {
                *current_session = Some(s.clone());
                return Some(s);
            }
            if size > 0 {
                // 打印所有存在但未能接管的会话 AUMID，便于把网易云等客户端补进白名单
                let ids: Vec<String> = (0..size)
                    .filter_map(|i| sessions.GetAt(i).ok())
                    .filter_map(|s| s.SourceAppUserModelId().map(|v| v.to_string()).ok())
                    .collect();
                log::debug!("[SMTC] 检测到 {size} 个媒体会话但未接管；AUMID 列表: {ids:?}");
            }
        }

        *current_session = None;
        None
    }

    /// 是否为受支持的音乐客户端会话
    fn is_music_source(session: &GlobalSystemMediaTransportControlsSession) -> bool {
        let id = session
            .SourceAppUserModelId()
            .map(|v| v.to_string().to_lowercase())
            .unwrap_or_default();
        MUSIC_SOURCE_KEYS.iter().any(|k| id.contains(k))
    }

    /// 是否为 NexBox 自身注册的 SMTC 会话（避免内部播放时被外部监控误接管）。
    /// 未打包桌面应用经 Interop 注册的会话，SourceAppUserModelId 常为 exe 全路径。
    fn is_self_session(session: &GlobalSystemMediaTransportControlsSession) -> bool {
        let id = session
            .SourceAppUserModelId()
            .map(|v| v.to_string().to_lowercase())
            .unwrap_or_default();
        if id.is_empty() {
            return false;
        }
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        !exe.is_empty() && (id == exe || id.contains(&exe) || exe.contains(&id))
    }

    fn is_playing(session: &GlobalSystemMediaTransportControlsSession) -> bool {
        matches!(session.GetPlaybackInfo().ok().and_then(|i| i.PlaybackStatus().ok()),
            Some(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing))
    }

    fn has_title(session: &GlobalSystemMediaTransportControlsSession) -> bool {
        session
            .TryGetMediaPropertiesAsync()
            .and_then(|t| t.get())
            .map(|m| !m.Title().map(|v| v.to_string()).unwrap_or_default().is_empty())
            .unwrap_or(false)
    }

    /// 读取会话缩略图并编码为 data URI
    fn read_cover(media: &GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
        let thumb = media.Thumbnail().ok()?.clone();
        let stream = thumb.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()?;
        if size == 0 || size > 5 * 1024 * 1024 {
            return None;
        }

        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size as u32).ok()?.get().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;

        // 依据文件头嗅探图片类型（SMTC 缩略图一般是 png/jpeg）
        let mime = if bytes.starts_with(&[0xFF, 0xD8]) {
            "image/jpeg"
        } else {
            "image/png"
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Some(format!("data:{mime};base64,{b64}"))
    }
}

/// 外部播放曲目信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover: Option<String>,
    pub source_app_id: String,
}

/// 外部播放状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPlayback {
    pub track: Option<ExternalTrack>,
    pub is_playing: bool,
    pub position_ms: i64,
    pub duration_ms: i64,
}

/// 启动后台轮询（在 setup 中调用）
pub fn start(app: tauri::AppHandle) {
    imp::start(app);
}

#[tauri::command]
pub fn external_player_state() -> Option<ExternalPlayback> {
    imp::current_state()
}

#[tauri::command]
pub fn external_control(action: String, value_ms: Option<i64>) {
    imp::control(&action, value_ms.unwrap_or(0));
}

/// 媒体键转发入口（仅 Windows；由 media_keys 低层钩子调用）
#[cfg(target_os = "windows")]
pub fn forward_media_key(action: &str) {
    imp::forward_media_key(action);
}

#[cfg(not(target_os = "windows"))]
pub fn forward_media_key(_action: &str) {}