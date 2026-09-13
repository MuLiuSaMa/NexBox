//! 喊话爆闪：麦克风分贝阈值触发模拟按键
//!
//! 开关开启时启动监听线程：
//! - WASAPI 捕获默认麦克风（eCapture/eConsole），实时计算 RMS → dBFS
//! - 每 2s 轮询三角洲行动进程（DeltaForceClient-Win64-Shipping）
//! - 分贝超阈值 + 冷却结束 + 游戏运行时，SendInput 按一次爆闪键（扫描码模式，
//!   游戏对纯 VK 注入常不识别）
//! - 以约 20Hz 节流向前端 emit `voice-strobe-level` { db, triggered }
//!
//! 线程生命周期沿用 game_win_key 的 GENERATION 代次控制模式，
//! 配置持久化到 voice_strobe.json。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use log::{info, warn};
use sysinfo::System;
use tauri::Emitter;
use tauri_plugin_store::StoreExt;

#[cfg(windows)]
use windows::Win32::Media::Audio as wa;

// ─── 配置 ───

fn default_key() -> String {
    "U".to_string()
}
fn default_threshold() -> i32 {
    -30
}
fn default_cooldown() -> u64 {
    0
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct VoiceStrobeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_key")]
    pub key: String,
    #[serde(default = "default_threshold")]
    pub threshold_db: i32,
    #[serde(default = "default_cooldown")]
    pub cooldown_ms: u64,
    #[serde(default)]
    pub device_id: String,
}

impl Default for VoiceStrobeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key: default_key(),
            threshold_db: default_threshold(),
            cooldown_ms: default_cooldown(),
            device_id: String::new(),
        }
    }
}

// ─── 全局状态 ───

/// 功能开关是否开启（内存态）
static ENABLED: AtomicBool = AtomicBool::new(false);
/// 代次：开关切换时 +1，通知旧监听线程退出
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 三角洲行动是否正在运行（监听线程周期性刷新）
static GAME_RUNNING: AtomicBool = AtomicBool::new(false);
/// 运行时配置（None 表示尚未初始化，取默认值）
static CONFIG: RwLock<Option<VoiceStrobeConfig>> = RwLock::new(None);
/// 监听线程句柄
static THREAD_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// 三角洲行动进程名（与 game_filter 内置名单一致）
const DELTA_FORCE_PROCESS: &str = "DeltaForceClient-Win64-Shipping";

fn get_config() -> VoiceStrobeConfig {
    CONFIG
        .read()
        .unwrap()
        .clone()
        .unwrap_or_else(VoiceStrobeConfig::default)
}

// ─── 配置持久化 ───

fn load_persisted_config(app: &tauri::AppHandle) -> VoiceStrobeConfig {
    match app.store("voice_strobe.json") {
        Ok(store) => {
            if let Some(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value::<VoiceStrobeConfig>(value) {
                    return config;
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open voice_strobe store: {}", e);
        }
    }
    VoiceStrobeConfig::default()
}

fn save_persisted_config(app: &tauri::AppHandle, config: &VoiceStrobeConfig) {
    match app.store("voice_strobe.json") {
        Ok(store) => {
            store.set("config", serde_json::to_value(config).unwrap());
            if let Err(e) = store.save() {
                log::error!("Failed to save voice_strobe config: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to open voice_strobe store for saving: {}", e);
        }
    }
}

// ─── 按键名 → 虚拟键码 ───

/// 支持 A-Z、0-9、F1-F24、方向键、Space/Tab/Enter 等常用键以及
/// Ctrl/Shift/Alt/Meta 修饰键单独绑定；鼠标键由 mouse_button_for_key 处理。
/// token 格式与前端 any-key 录制器保持一致
#[cfg(windows)]
fn vk_for_key(name: &str) -> Option<u32> {
    let n = name.trim();
    if n.len() == 1 {
        let c = n.chars().next().unwrap().to_ascii_uppercase();
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u32);
        }
        return match c {
            '[' => Some(0xDB),
            ']' => Some(0xDD),
            _ => None,
        };
    }
    let up = n.to_ascii_uppercase();
    // 修饰键单独绑定
    match up.as_str() {
        "CTRL" | "CONTROL" => return Some(0x11),
        "SHIFT" => return Some(0x10),
        "ALT" => return Some(0x12),
        "META" => return Some(0x5B), // VK_LWIN
        _ => {}
    }
    // F1-F24: VK_F1 = 0x70 连续排列
    if up.len() >= 2 && up.starts_with('F') {
        if let Ok(num) = up[1..].parse::<u32>() {
            if (1..=24).contains(&num) {
                return Some(0x6F + num);
            }
        }
    }
    match up.as_str() {
        "SPACE" => Some(0x20),
        "TAB" => Some(0x09),
        "ENTER" => Some(0x0D),
        "BACKSPACE" => Some(0x08),
        "DELETE" => Some(0x2E),
        "INSERT" => Some(0x2D),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "CAPSLOCK" => Some(0x14),
        "NUMLOCK" => Some(0x90),
        "SCROLLLOCK" => Some(0x91),
        "PAUSE" => Some(0x13),
        "PRINTSCREEN" => Some(0x2C),
        "LEFT" => Some(0x25),
        "UP" => Some(0x26),
        "RIGHT" => Some(0x27),
        "DOWN" => Some(0x28),
        "MINUS" => Some(0xBD),
        "EQUAL" => Some(0xBB),
        "BACKSLASH" => Some(0xDC),
        "SEMICOLON" => Some(0xBA),
        "QUOTE" => Some(0xDE),
        "COMMA" => Some(0xBC),
        "PERIOD" => Some(0xBE),
        "SLASH" => Some(0xBF),
        "BACKQUOTE" => Some(0xC0),
        _ => None,
    }
}

/// 触发目标：键盘 VK 或鼠标按钮（0=左键 1=右键 2=中键 3=侧键1 4=侧键2）
#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
enum TriggerTarget {
    Keyboard(u32),
    Mouse(u16),
}

/// 解析爆闪键配置：鼠标 token（MouseLeft/Right/Middle/X1/X2）或键盘 token
#[cfg(windows)]
fn resolve_trigger(name: &str) -> Option<TriggerTarget> {
    match name.trim() {
        "MouseLeft" => Some(TriggerTarget::Mouse(0)),
        "MouseRight" => Some(TriggerTarget::Mouse(1)),
        "MouseMiddle" => Some(TriggerTarget::Mouse(2)),
        "MouseX1" => Some(TriggerTarget::Mouse(3)),
        "MouseX2" => Some(TriggerTarget::Mouse(4)),
        k => vk_for_key(k).map(TriggerTarget::Keyboard),
    }
}

// ─── SendInput 模拟按键 ───

#[cfg(windows)]
fn press_key_by_name(name: &str) {
    match resolve_trigger(name) {
        Some(TriggerTarget::Keyboard(vk)) => press_keyboard_key(vk),
        Some(TriggerTarget::Mouse(btn)) => press_mouse_button(btn),
        None => warn!("喊话爆闪: 未识别的按键名 '{}'", name),
    }
}

/// 鼠标按钮按下再松开（左/右/中/侧键，侧键通过 XDOWN/XUP + mouseData 标识）
#[cfg(windows)]
fn press_mouse_button(button: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT,
    };

    const XBUTTON1: u32 = 0x0001;
    const XBUTTON2: u32 = 0x0002;

    let (down_flags, up_flags, mouse_data) = match button {
        0 => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, 0),
        1 => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, 0),
        2 => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, 0),
        3 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON1),
        4 => (MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, XBUTTON2),
        _ => return,
    };

    let make_input = |dw_flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
                      mouse_data: u32| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: mouse_data,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        SendInput(
            &[make_input(down_flags, mouse_data)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
    thread::sleep(Duration::from_millis(30));
    unsafe {
        SendInput(
            &[make_input(up_flags, mouse_data)],
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

/// 键盘按键按下再松开（扫描码模式，游戏对纯 VK 注入常不识别）
#[cfg(windows)]
fn press_keyboard_key(vk: u32) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MapVirtualKeyW, MAPVK_VK_TO_VSC, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    };

    // 方向键/翻页/编辑区键为扩展键，需要 EXTENDEDKEY 标志
    let extended = matches!(
        vk,
        0x25..=0x28 | 0x21 | 0x22 | 0x23 | 0x24 | 0x2C | 0x2D | 0x2E
    );
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC) } as u16;

    let mut down_flags = KEYEVENTF_SCANCODE;
    if extended {
        down_flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let up_flags: KEYBD_EVENT_FLAGS = down_flags | KEYEVENTF_KEYUP;

    let make_input = |dw_flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: scan,
                dwFlags: dw_flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    unsafe {
        SendInput(&[make_input(down_flags)], std::mem::size_of::<INPUT>() as i32);
    }
    thread::sleep(Duration::from_millis(30));
    unsafe {
        SendInput(&[make_input(up_flags)], std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(not(windows))]
fn press_key_by_name(_name: &str) {}

// ─── WASAPI 麦克风捕获 ───

#[cfg(windows)]
const BUFFER_FLAG_SILENT: u32 = 0x2;
#[cfg(windows)]
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
#[cfg(windows)]
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
#[cfg(windows)]
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: windows::core::GUID = windows::core::GUID::from_values(
    0x00000003, 0x0000, 0x0010,
    [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
);

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
enum SampleFormat {
    Float32,
    Pcm16,
    Pcm32,
    Other,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct FormatInfo {
    sample_format: SampleFormat,
    sample_rate: u32,
    channels: u16,
    block_align: u16,
}

#[cfg(windows)]
fn parse_format(wfx: &wa::WAVEFORMATEX) -> FormatInfo {
    let sample_format = if wfx.wFormatTag == WAVE_FORMAT_EXTENSIBLE {
        let ext_ptr = wfx as *const wa::WAVEFORMATEX as *const wa::WAVEFORMATEXTENSIBLE;
        let sub_format = unsafe { std::ptr::addr_of!((*ext_ptr).SubFormat).read_unaligned() };
        if sub_format == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT {
            SampleFormat::Float32
        } else {
            match unsafe { (*ext_ptr).Format.wBitsPerSample } {
                16 => SampleFormat::Pcm16,
                32 => SampleFormat::Pcm32,
                _ => SampleFormat::Other,
            }
        }
    } else if wfx.wFormatTag == WAVE_FORMAT_IEEE_FLOAT {
        SampleFormat::Float32
    } else {
        match wfx.wBitsPerSample {
            16 => SampleFormat::Pcm16,
            32 => SampleFormat::Pcm32,
            _ => SampleFormat::Other,
        }
    };

    FormatInfo {
        sample_format,
        sample_rate: wfx.nSamplesPerSec,
        channels: wfx.nChannels,
        block_align: wfx.nBlockAlign,
    }
}

/// 一次已启动的麦克风捕获会话
#[cfg(windows)]
struct CaptureSession {
    audio_client: wa::IAudioClient,
    capture_client: wa::IAudioCaptureClient,
    fmt: FormatInfo,
    /// 会话绑定的设备 ID（空 = 系统默认麦克风）
    device_id: String,
}

#[cfg(windows)]
impl Drop for CaptureSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.audio_client.Stop();
        }
    }
}

#[cfg(windows)]
impl CaptureSession {
    fn new(device_id: &str) -> Result<Self, String> {
        use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

        unsafe {
            let enumerator: wa::IMMDeviceEnumerator = CoCreateInstance(
                &wa::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
            .map_err(|e| format!("CoCreateInstance failed: {}", e))?;

            // 空 = 系统默认麦克风；否则按设备 ID 打开指定设备
            let device = if device_id.is_empty() {
                enumerator
                    .GetDefaultAudioEndpoint(wa::eCapture, wa::eConsole)
                    .map_err(|e| format!("GetDefaultAudioEndpoint(eCapture) failed: {}", e))?
            } else {
                let id_hstring = windows::core::HSTRING::from(device_id);
                enumerator
                    .GetDevice(&id_hstring)
                    .map_err(|e| format!("GetDevice({}) failed: {}", device_id, e))?
            };

            let audio_client: wa::IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| format!("Activate failed: {}", e))?;

            let mix_format_ptr = audio_client
                .GetMixFormat()
                .map_err(|e| format!("GetMixFormat failed: {}", e))?;
            let fmt = parse_format(unsafe { &*mix_format_ptr });

            audio_client
                .Initialize(wa::AUDCLNT_SHAREMODE_SHARED, 0, 0, 0, mix_format_ptr, None)
                .map_err(|e| format!("Initialize failed: {}", e))?;

            let capture_client: wa::IAudioCaptureClient = audio_client
                .GetService()
                .map_err(|e| format!("GetService failed: {}", e))?;

            CoTaskMemFree(Some(mix_format_ptr as *const _));

            audio_client
                .Start()
                .map_err(|e| format!("Start failed: {}", e))?;

            Ok(Self {
                audio_client,
                capture_client,
                fmt,
                device_id: device_id.to_string(),
            })
        }
    }

    /// 读取一个可用音频包并计算 RMS；无数据返回 Ok(None)，出错返回 Err（需重连）
    fn read_batch_rms(&self) -> Result<Option<f64>, String> {
        unsafe {
            let packet = self
                .capture_client
                .GetNextPacketSize()
                .map_err(|e| format!("GetNextPacketSize failed: {}", e))?;
            if packet == 0 {
                return Ok(None);
            }

            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames = 0u32;
            let mut flags = 0u32;
            self.capture_client
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .map_err(|e| format!("GetBuffer failed: {}", e))?;

            let channels = self.fmt.channels as usize;
            let total = frames as usize * channels;
            let rms = if flags & BUFFER_FLAG_SILENT != 0 || total == 0 {
                0.0
            } else {
                let bytes_per_sample = (self.fmt.block_align as usize / channels).max(1);
                let byte_len = total * bytes_per_sample;
                let slice = std::slice::from_raw_parts(data, byte_len);
                compute_rms(slice, self.fmt.sample_format)
            };

            self.capture_client
                .ReleaseBuffer(frames)
                .map_err(|e| format!("ReleaseBuffer failed: {}", e))?;

            Ok(Some(rms))
        }
    }
}

#[cfg(windows)]
fn compute_rms(data: &[u8], format: SampleFormat) -> f64 {
    let (sum_sq, count) = match format {
        SampleFormat::Float32 => {
            let samples: &[f32] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };
            let mut s = 0.0f64;
            for &v in samples {
                s += (v as f64) * (v as f64);
            }
            (s, samples.len())
        }
        SampleFormat::Pcm16 => {
            let samples: &[i16] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const i16, data.len() / 2) };
            let mut s = 0.0f64;
            for &v in samples {
                let f = v as f64 / 32768.0;
                s += f * f;
            }
            (s, samples.len())
        }
        SampleFormat::Pcm32 => {
            let samples: &[i32] =
                unsafe { std::slice::from_raw_parts(data.as_ptr() as *const i32, data.len() / 4) };
            let mut s = 0.0f64;
            for &v in samples {
                let f = v as f64 / 2147483648.0;
                s += f * f;
            }
            (s, samples.len())
        }
        SampleFormat::Other => return 0.0,
    };
    if count == 0 {
        return 0.0;
    }
    (sum_sq / count as f64).max(0.0).sqrt()
}

/// RMS（0..1）→ dBFS，全静音记 -100
fn rms_to_db(rms: f64) -> f64 {
    if rms <= 1e-7 {
        -100.0
    } else {
        (20.0 * rms.log10()).clamp(-100.0, 0.0)
    }
}

// ─── 三角洲行动进程检测 ───

fn delta_force_running(system: &mut System) -> bool {
    system.refresh_processes();
    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let lowered = name.to_ascii_lowercase();
        let stripped = lowered.strip_suffix(".exe").unwrap_or(&lowered);
        if stripped == DELTA_FORCE_PROCESS.to_ascii_lowercase() {
            return true;
        }
    }
    false
}

// ─── 监听线程 ───

fn monitor_loop(gen: u64, app: tauri::AppHandle) {
    #[cfg(windows)]
    {
        // COM 初始化；RPC_E_CHANGED_MODE 时沿用已初始化的套间即可
        let _ = unsafe { windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        ) };
    }

    info!("喊话爆闪: 监听线程已启动 (gen={})", gen);

    let mut system = System::new();
    let mut last_game_check = Instant::now() - Duration::from_secs(3);
    let mut last_emit = Instant::now() - Duration::from_millis(200);
    let mut last_trigger = Instant::now() - Duration::from_secs(3600);
    let mut last_err_log = Instant::now() - Duration::from_secs(10);
    #[cfg(windows)]
    let mut session: Option<CaptureSession> = None;

    while GENERATION.load(Ordering::Relaxed) == gen && ENABLED.load(Ordering::Relaxed) {
        // 1. 三角洲行动进程检测（2s 周期）
        if last_game_check.elapsed() >= Duration::from_secs(2) {
            last_game_check = Instant::now();
            GAME_RUNNING.store(delta_force_running(&mut system), Ordering::Relaxed);
        }

        // 2. 读取音频并计算本批 RMS
        #[cfg(windows)]
        let mut batch_rms: Option<f64> = None;
        #[cfg(windows)]
        {
            // 用户切换了麦克风设备：丢弃旧会话，下次迭代按新设备重建
            if let Some(s) = session.as_ref() {
                if s.device_id != get_config().device_id {
                    info!("喊话爆闪: 检测到麦克风设备切换，重建捕获会话");
                    session = None;
                }
            }

            if session.is_none() {
                let wanted_device = get_config().device_id;
                match CaptureSession::new(&wanted_device) {
                    Ok(s) => {
                        info!(
                            "喊话爆闪: 麦克风捕获已建立 ({}Hz {}ch, {:?})",
                            s.fmt.sample_rate, s.fmt.channels, s.fmt.sample_format
                        );
                        session = Some(s);
                    }
                    Err(e) => {
                        if last_err_log.elapsed() >= Duration::from_secs(5) {
                            last_err_log = Instant::now();
                            warn!("喊话爆闪: 建立麦克风捕获失败，2s 后重试: {}", e);
                        }
                        thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                }
            }

            let mut capture_failed = false;
            if let Some(s) = session.as_ref() {
                for _ in 0..64 {
                    match s.read_batch_rms() {
                        Ok(Some(rms)) => {
                            batch_rms = Some(match batch_rms {
                                Some(prev) => prev.max(rms),
                                None => rms,
                            });
                        }
                        Ok(None) => break,
                        Err(e) => {
                            if last_err_log.elapsed() >= Duration::from_secs(5) {
                                last_err_log = Instant::now();
                                warn!("喊话爆闪: 捕获中断，2s 后重连: {}", e);
                            }
                            capture_failed = true;
                            break;
                        }
                    }
                }
            }
            if capture_failed {
                session = None;
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        }
        #[cfg(not(windows))]
        let batch_rms: Option<f64> = None;

        let db = batch_rms.map(rms_to_db).unwrap_or(-100.0);

        // 3. 触发判定：超阈值 + 游戏运行 + 冷却结束
        let cfg = get_config();
        let mut triggered = false;
        if batch_rms.is_some()
            && db >= cfg.threshold_db as f64
            && GAME_RUNNING.load(Ordering::Relaxed)
            && last_trigger.elapsed() >= Duration::from_millis(cfg.cooldown_ms)
        {
            press_key_by_name(&cfg.key);
            last_trigger = Instant::now();
            triggered = true;
            info!("喊话爆闪: 触发按键 '{}' (db={:.1})", cfg.key, db);
        }

        // 4. 音量事件节流 ~20Hz（触发时立即推送一次）
        if triggered || last_emit.elapsed() >= Duration::from_millis(50) {
            last_emit = Instant::now();
            let _ = app.emit(
                "voice-strobe-level",
                serde_json::json!({ "db": db, "triggered": triggered }),
            );
        }

        thread::sleep(Duration::from_millis(10));
    }

    GAME_RUNNING.store(false, Ordering::Relaxed);

    #[cfg(windows)]
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }
    info!("喊话爆闪: 监听线程已退出");
}

// ─── 线程启停 ───

fn apply_enabled(app: &tauri::AppHandle, enabled: bool) {
    if enabled == ENABLED.load(Ordering::Relaxed) {
        return;
    }
    ENABLED.store(enabled, Ordering::Relaxed);

    if enabled {
        let gen = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        let app_handle = app.clone();
        let handle = thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                monitor_loop(gen, app_handle)
            }));
        });
        *THREAD_HANDLE.lock().unwrap() = Some(handle);
        info!("喊话爆闪: 已开启");
    } else {
        GAME_RUNNING.store(false, Ordering::Relaxed);
        // 监听线程检测到 ENABLED=false 后自行退出（~10ms 内）
        info!("喊话爆闪: 已关闭");
    }
}

// ─── 初始化 / 清理 ───

/// 应用启动时调用：恢复持久化配置（键位/阈值/冷却/设备），
/// 但开关一律不自动开启——确保喊话爆闪默认关闭，需用户在页面手动开启。
pub async fn init(app: tauri::AppHandle) -> Result<(), String> {
    let mut config = load_persisted_config(&app);
    // 旧版本默认冷却为 1500ms，未手动修改过的遗留值统一迁移为 0（无冷却）
    if config.cooldown_ms == 1500 {
        config.cooldown_ms = 0;
        save_persisted_config(&app, &config);
    }
    // 确保默认关闭：即使上次持久化了 enabled=true，启动时也不自动开启，
    // 并清除该遗留标记，避免下次读取残留
    if config.enabled {
        config.enabled = false;
        save_persisted_config(&app, &config);
    }
    *CONFIG.write().unwrap() = Some(config.clone());
    Ok(())
}

/// 应用退出时清理：停止监听线程
pub fn cleanup() {
    ENABLED.store(false, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
    GAME_RUNNING.store(false, Ordering::Relaxed);
    if let Some(handle) = THREAD_HANDLE.lock().unwrap().take() {
        let _ = handle.join();
    }
}

// ─── Tauri 命令 ───

/// 枚举系统可用的麦克风（捕获）设备
#[derive(serde::Serialize, Clone, Debug)]
pub struct VoiceStrobeDevice {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub fn voice_strobe_list_devices() -> Result<Vec<VoiceStrobeDevice>, String> {
    #[cfg(windows)]
    {
        use windows::Win32::Media::Audio as wa;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
            STGM,
        };
        use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY};
        use windows::core::{GUID, PCWSTR};

        /// PKEY_Device_FriendlyName = {A45C254E-DF1C-4EFD-8020-67D146A850E0}, pid=14
        const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
            fmtid: GUID::from_values(
                0xA45C254E, 0xDF1C, 0x4EFD,
                [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0],
            ),
            pid: 14,
        };

        unsafe {
            // 命令可能运行在未初始化 COM 的线程，幂等初始化（已初始化时忽略）
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: wa::IMMDeviceEnumerator = CoCreateInstance(
                &wa::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            )
            .map_err(|e| format!("CoCreateInstance failed: {}", e))?;

            let collection = enumerator
                .EnumAudioEndpoints(wa::eCapture, wa::DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("EnumAudioEndpoints(eCapture) failed: {}", e))?;

            let count = collection
                .GetCount()
                .map_err(|e| format!("GetCount failed: {}", e))?;

            let mut devices = Vec::new();
            for i in 0..count {
                let Ok(device) = collection.Item(i) else { continue };
                let id = match device.GetId() {
                    Ok(pwstr) => {
                        let s = PCWSTR(pwstr.as_ptr()).to_string().unwrap_or_default();
                        CoTaskMemFree(Some(pwstr.as_ptr() as *const _));
                        s
                    }
                    Err(_) => continue,
                };
                if id.is_empty() {
                    continue;
                }
                let name = device
                    .OpenPropertyStore(STGM(0))
                    .ok()
                    .and_then(|store: IPropertyStore| store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME).ok())
                    .map(|prop| prop.to_string())
                    .unwrap_or_else(|| format!("麦克风 {}", devices.len() + 1));
                devices.push(VoiceStrobeDevice { id, name });
            }
            Ok(devices)
        }
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

/// 开关切换：启动/停止监听线程并持久化
#[tauri::command]
pub async fn voice_strobe_set_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let cfg = {
        let mut guard = CONFIG.write().unwrap();
        let cfg = guard.get_or_insert_with(VoiceStrobeConfig::default);
        cfg.enabled = enabled;
        cfg.clone()
    };
    save_persisted_config(&app, &cfg);
    apply_enabled(&app, enabled);
    Ok(())
}

/// 获取状态（开关 + 当前配置 + 游戏运行标记）
#[tauri::command]
pub fn voice_strobe_get_status() -> VoiceStrobeStatus {
    let cfg = get_config();
    VoiceStrobeStatus {
        enabled: ENABLED.load(Ordering::Relaxed),
        key: cfg.key,
        threshold_db: cfg.threshold_db,
        cooldown_ms: cfg.cooldown_ms,
        game_running: GAME_RUNNING.load(Ordering::Relaxed),
        device_id: cfg.device_id,
    }
}

#[derive(serde::Serialize)]
pub struct VoiceStrobeStatus {
    pub enabled: bool,
    pub key: String,
    pub threshold_db: i32,
    pub cooldown_ms: u64,
    pub game_running: bool,
    pub device_id: String,
}

/// 热更新配置（按键/阈值/冷却/麦克风设备），运行中立即生效
#[tauri::command]
pub async fn voice_strobe_update_config(
    app: tauri::AppHandle,
    key: String,
    threshold_db: i32,
    cooldown_ms: u64,
    device_id: Option<String>,
) -> Result<(), String> {
    let key = key.trim().to_string();
    #[cfg(windows)]
    if resolve_trigger(&key).is_none() {
        return Err(format!("不支持的按键: {}", key));
    }

    let cfg = {
        let mut guard = CONFIG.write().unwrap();
        let cfg = guard.get_or_insert_with(VoiceStrobeConfig::default);
        cfg.key = key;
        cfg.threshold_db = threshold_db.clamp(-60, 0);
        cfg.cooldown_ms = cooldown_ms.clamp(0, 5000);
        if let Some(id) = device_id {
            cfg.device_id = id.trim().to_string();
        }
        cfg.clone()
    };
    save_persisted_config(&app, &cfg);
    Ok(())
}
