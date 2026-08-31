use std::sync::atomic::{AtomicBool, AtomicI64, AtomicPtr, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager};

static OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static OVERLAY_HANDLE: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
static DRAG_MODE: AtomicBool = AtomicBool::new(false);
static POSITION_CHANGED: AtomicBool = AtomicBool::new(false);
static BACKGROUND_POLLER_ACTIVE: AtomicBool = AtomicBool::new(false);

// ═══ 网络时间同步 ═══
// NET_TIME_OFFSET_MS = 网络标准时间 - 本地系统时间（毫秒）。
// 悬浮框时间 = 本地时间 + 该偏移量，从而得到不依赖系统时区的网络标准时间。
static NET_TIME_OFFSET_MS: AtomicI64 = AtomicI64::new(i64::MIN);
static NET_TIME_SYNC_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 获取网络时间偏移量（毫秒）。若尚未同步成功，返回 None。
fn get_net_offset_ms() -> Option<i64> {
    let v = NET_TIME_OFFSET_MS.load(Ordering::SeqCst);
    if v == i64::MIN { None } else { Some(v) }
}

/// 后台线程周期性从 HTTP 响应头 Date 字段同步网络时间。
/// 从多个端点轮流尝试，任意一个成功即可。
fn start_net_time_sync() {
    if NET_TIME_SYNC_ACTIVE.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        let endpoints = [
            "https://www.baidu.com",
            "https://www.taobao.com",
            "https://www.qq.com",
            "https://cloud.tencent.com",
        ];
        // 每 30 分钟同步一次；初始立即同步一次
        loop {
            for url in &endpoints {
                if let Some(server_date) = fetch_http_date(url) {
                    let local_before = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    let server_ms = server_date * 1000;
                    // 粗略补偿网络往返（取请求发出到收到之间的中点）：
                    // 这里简单用响应到达时刻 - 单程估计（200ms）
                    let offset = server_ms - (local_before - 200);
                    NET_TIME_OFFSET_MS.store(offset, Ordering::SeqCst);
                    log::info!("overlay: 网络时间同步成功 offset_ms={} (endpoint={})", offset, url);
                    break;
                }
            }
            // 等待 30 分钟
            for _ in 0..(30 * 60) {
                if !NET_TIME_SYNC_ACTIVE.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

/// 请求 URL，从响应头 Date 解析 Unix 秒。失败返回 None。
fn fetch_http_date(url: &str) -> Option<i64> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let resp = client.get(url).send().ok()?;
    let date_header = resp.headers().get("date")?.to_str().ok()?;
    // HTTP Date 格式: Wed, 21 Oct 2015 07:28:00 GMT（RFC 7231）
    chrono::DateTime::parse_from_rfc2822(date_header)
        .ok()
        .map(|dt| dt.timestamp())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DisplayItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

pub type DisplayItems = Vec<DisplayItem>;

fn default_style() -> String {
    "default".to_string()
}

fn default_font() -> String {
    "Microsoft YaHei".to_string()
}

fn default_font_size() -> u32 {
    13
}

fn default_item_width() -> u32 {
    130
}

fn default_font_color() -> String {
    "#ffffff".to_string()
}

fn default_display_items() -> DisplayItems {
        vec![
            DisplayItem { id: "time".to_string(), label: "时间".to_string(), enabled: false },
            DisplayItem { id: "fps".to_string(), label: "FPS".to_string(), enabled: false },
            DisplayItem { id: "fps_1low".to_string(), label: "1% Low".to_string(), enabled: false },
            DisplayItem { id: "fps_01low".to_string(), label: "0.1% Low".to_string(), enabled: false },
            DisplayItem { id: "cpu_temp".to_string(), label: "CPU温度".to_string(), enabled: false },
            DisplayItem { id: "cpu_usage".to_string(), label: "CPU占用".to_string(), enabled: true },
            DisplayItem { id: "cpu_fan_speed".to_string(), label: "CPU风扇转速".to_string(), enabled: false },
            DisplayItem { id: "cpu_clock".to_string(), label: "CPU频率".to_string(), enabled: false },
            DisplayItem { id: "cpu_voltage".to_string(), label: "CPU电压".to_string(), enabled: false },
            DisplayItem { id: "cpu_power".to_string(), label: "CPU功耗".to_string(), enabled: false },
            DisplayItem { id: "gpu_temp".to_string(), label: "GPU温度".to_string(), enabled: true },
            DisplayItem { id: "gpu_usage".to_string(), label: "GPU占用".to_string(), enabled: true },
            DisplayItem { id: "gpu_fan_speed".to_string(), label: "GPU风扇转速".to_string(), enabled: false },
            DisplayItem { id: "gpu_power".to_string(), label: "GPU功耗".to_string(), enabled: false },
            DisplayItem { id: "gpu_clock".to_string(), label: "GPU频率".to_string(), enabled: false },
            DisplayItem { id: "gpu_voltage".to_string(), label: "GPU电压".to_string(), enabled: false },
            DisplayItem { id: "gpu_vram".to_string(), label: "GPU显存占用".to_string(), enabled: false },
            DisplayItem { id: "gpu_memory_clock".to_string(), label: "GPU显存频率".to_string(), enabled: false },
            DisplayItem { id: "memory_usage".to_string(), label: "内存占用".to_string(), enabled: true },
            DisplayItem { id: "ssd_temp".to_string(), label: "硬盘温度".to_string(), enabled: false },
            DisplayItem { id: "game_ping".to_string(), label: "游戏延迟".to_string(), enabled: true },
            DisplayItem { id: "delta_password".to_string(), label: "三角洲密码".to_string(), enabled: false },
        ]
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CustomOverlayItem {
    pub id: String,
    pub text: String,
    pub color: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OverlaySettings {
    #[serde(default = "default_display_items")]
    pub display_items: DisplayItems,
    #[serde(default)]
    pub custom_items: Vec<CustomOverlayItem>,
    pub opacity: u8,
    #[serde(default = "default_style")]
    pub style: String,
    #[serde(default = "default_font")]
    pub font: String,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default = "default_item_width")]
    pub item_width: u32,
    #[serde(default = "default_font_color")]
    pub font_color: String,
    #[serde(default)]
    pub position_x: Option<i32>,
    #[serde(default)]
    pub position_y: Option<i32>,
    /// 竖排悬浮框独立位置（与 Win32 悬浮框的 position_x/y 解耦，互不影响）
    #[serde(default)]
    pub vertical_position_x: Option<i32>,
    #[serde(default)]
    pub vertical_position_y: Option<i32>,
    /// 三角洲密码选中的地图名称列表（空 = 显示全部）
    #[serde(default)]
    pub delta_password_maps: Vec<String>,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            display_items: default_display_items(),
            custom_items: Vec::new(),
            opacity: 200,
            style: "default".to_string(),
            font: "Microsoft YaHei".to_string(),
            font_size: 13,
            item_width: 130,
            font_color: "#ffffff".to_string(),
            position_x: None,
            position_y: None,
            vertical_position_x: None,
            vertical_position_y: None,
            delta_password_maps: Vec::new(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct OverlayResult {
    pub success: bool,
    pub message: String,
}

/// 单个 GPU 的传感器数据
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct GpuSensorData {
    pub name: String,
    pub hardware_type: String,
    pub temperature: Option<f64>,
    pub usage: Option<u32>,
    pub fan_speed: Option<u32>,
    pub power: Option<u32>,
    pub clock: Option<u32>,
    pub memory_clock: Option<u32>,
    pub vram_used: Option<u32>,
    pub vram_total: Option<u32>,
    pub voltage: Option<f64>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct OverlayHardwareData {
    fps: Option<u32>,
    fps_1low: Option<u32>,
    fps_01low: Option<u32>,
    cpu_usage: Option<u16>,
    cpu_temp: Option<f64>,
    cpu_clock: Option<u32>,
    gpu_temp: Option<f64>,
    gpu_usage: Option<u32>,
    memory_usage: Option<f64>,
    delta_password: Option<String>,
    game_ping: Option<u32>,
    cpu_fan_speed: Option<u32>,
    gpu_fan_speed: Option<u32>,
    gpu_power: Option<u32>,
    gpu_clock: Option<u32>,
    gpu_vram_used: Option<u32>,
    gpu_vram_total: Option<u32>,
    gpu_memory_clock: Option<u32>,
    cpu_voltage: Option<f64>,
    gpu_voltage: Option<f64>,
    cpu_power: Option<f64>,
    ssd_temp: Option<f64>,
    /// 网络标准时间偏移量（毫秒）：net_time = 本地时间 + 该偏移。None 表示尚未同步成功
    pub net_time_offset_ms: Option<i64>,
    /// 所有 GPU 的传感器数据（支持多 GPU 切换）
    pub gpu_sensors: Vec<GpuSensorData>,
    /// 当前选中的 GPU 索引
    pub active_gpu_index: usize,
}

impl Default for OverlayHardwareData {
    fn default() -> Self {
        Self {
            fps: None,
            fps_1low: None,
            fps_01low: None,
            cpu_usage: None,
            cpu_temp: None,
            cpu_clock: None,
            gpu_temp: None,
            gpu_usage: None,
            memory_usage: None,
            delta_password: None,
            game_ping: None,
            cpu_fan_speed: None,
            gpu_fan_speed: None,
            gpu_power: None,
            gpu_clock: None,
            gpu_vram_used: None,
            gpu_vram_total: None,
            gpu_memory_clock: None,
            cpu_voltage: None,
            gpu_voltage: None,
            cpu_power: None,
            ssd_temp: None,
            net_time_offset_ms: None,
            gpu_sensors: Vec::new(),
            active_gpu_index: 0,
        }
    }
}

pub static CURRENT_SETTINGS: Mutex<Option<OverlaySettings>> = Mutex::new(None);
pub static CURRENT_HARDWARE_DATA: Mutex<Option<OverlayHardwareData>> = Mutex::new(None);
pub static SETTINGS_LOADED_FROM_STORE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 供托盘悬停面板读取的硬件数据快照（只读自常驻轮询缓存，不触发额外采样）。
#[derive(serde::Serialize, Clone, Default)]
pub struct HoverSnapshot {
    pub cpu_usage: Option<u16>,
    pub cpu_temp: Option<f64>,
    pub cpu_clock: Option<u32>,
    pub cpu_fan_speed: Option<u32>,
    pub cpu_voltage: Option<f64>,
    pub cpu_power: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub gpu_usage: Option<u32>,
    pub gpu_clock: Option<u32>,
    pub gpu_fan_speed: Option<u32>,
    pub gpu_voltage: Option<f64>,
    pub gpu_power: Option<u32>,
    pub gpu_vram_used: Option<u32>,
    pub gpu_vram_total: Option<u32>,
    pub gpu_memory_clock: Option<u32>,
    pub memory_usage: Option<f64>,
    pub ssd_temp: Option<f64>,
}

/// 读取当前轮询缓存中的硬件数据（None 表示尚无数据，如启动早期）。
pub fn current_hover_snapshot() -> Option<HoverSnapshot> {
    CURRENT_HARDWARE_DATA.lock().unwrap().clone().map(|d| HoverSnapshot {
        cpu_usage: d.cpu_usage,
        cpu_temp: d.cpu_temp,
        cpu_clock: d.cpu_clock,
        cpu_fan_speed: d.cpu_fan_speed,
        cpu_voltage: d.cpu_voltage,
        cpu_power: d.cpu_power,
        gpu_temp: d.gpu_temp,
        gpu_usage: d.gpu_usage,
        gpu_clock: d.gpu_clock,
        gpu_fan_speed: d.gpu_fan_speed,
        gpu_voltage: d.gpu_voltage,
        gpu_power: d.gpu_power,
        gpu_vram_used: d.gpu_vram_used,
        gpu_vram_total: d.gpu_vram_total,
        gpu_memory_clock: d.gpu_memory_clock,
        memory_usage: d.memory_usage,
        ssd_temp: d.ssd_temp,
    })
}

/// 从持久化存储 (settings.json) 加载悬浮框设置到内存。
/// 仅在 CURRENT_SETTINGS 为空时尝试加载，确保快捷键触发的 toggle 使用已保存的设置而非默认值。
pub fn try_load_persisted_settings(app_handle: &tauri::AppHandle) {
    // 已经加载过就不再重复读取文件
    if SETTINGS_LOADED_FROM_STORE.load(Ordering::SeqCst) {
        return;
    }
    let mut lock = CURRENT_SETTINGS.lock().unwrap();
    if lock.is_some() {
        SETTINGS_LOADED_FROM_STORE.store(true, Ordering::SeqCst);
        return;
    }
    // 尝试从 settings.json 读取已保存的设置
    if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
        let store_path = app_data_dir.join("settings.json");
        if store_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&store_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(saved) = json.get("overlay-settings") {
                        if let Ok(settings) = serde_json::from_value::<OverlaySettings>(saved.clone()) {
                            log::info!("从持久化存储加载悬浮框设置成功");
                            *lock = Some(settings);
                        }
                    }
                }
            }
        }
    }
    // 如果加载失败，用默认值初始化（保持向后兼容）
    if lock.is_none() {
        *lock = Some(OverlaySettings::default());
    }
    SETTINGS_LOADED_FROM_STORE.store(true, Ordering::SeqCst);
}

pub fn get_or_init_settings() -> OverlaySettings {
    let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
    if settings_lock.is_none() {
        *settings_lock = Some(OverlaySettings::default());
    }
    settings_lock.as_ref().unwrap().clone()
}

/// 从 LHML 传感器列表中提取指定类型的传感器值（多模式匹配）
fn extract_sensor(
    sensors: &[crate::sensor::SensorReading],
    sensor_type: &str,
    hardware_type: &str,
    hardware: Option<&str>,
    names: &[&str],
    skip_zero: bool,
) -> Option<(f64, String)> {
    for name in names {
        if let Some(s) = sensors.iter().find(|s| {
            s.sensor_type == sensor_type
                && s.hardware_type.eq_ignore_ascii_case(hardware_type)
                && hardware.map_or(true, |h| s.hardware == h)
                && s.name.contains(name)
                && (!skip_zero || s.value > 0.0)
        }) {
            return Some((s.value, s.name.clone()));
        }
    }
    None
}

/// 从 LHML 传感器列表中提取所有匹配传感器值的平均值（用于风扇转速等）
fn extract_all_avg(
    sensors: &[crate::sensor::SensorReading],
    sensor_type: &str,
    hardware_type: &str,
    hardware: Option<&str>,
    name_prefixes: &[&str],
) -> Option<f64> {
    let values: Vec<f64> = sensors
        .iter()
        .filter(|s| {
            s.sensor_type == sensor_type
                && s.hardware_type.eq_ignore_ascii_case(hardware_type)
                && hardware.map_or(true, |h| s.hardware == h)
                && name_prefixes.iter().any(|p| s.name.starts_with(p))
        })
        .map(|s| s.value)
        .collect();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

static LAST_LHML_UPDATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static ACTIVE_GPU_INDEX: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// 从 LHML 传感器列表中收集所有 GPU 的传感器数据（支持多 GPU）
/// 按 hardware 名称分组（而非 hardware_type），避免 AMD 核显+独显都是 GpuAmd 被合并的问题
fn collect_all_gpu_sensors(
    sensors: &[crate::sensor::SensorReading],
) -> Vec<GpuSensorData> {
    // 找到所有唯一的 GPU 硬件名称
    let gpu_hardware_names: Vec<String> = {
        let mut names: Vec<String> = sensors
            .iter()
            .filter(|s| {
                let t = s.hardware_type.to_lowercase();
                t.starts_with("gpu")
            })
            .map(|s| s.hardware.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    };

    if gpu_hardware_names.is_empty() {
        return Vec::new();
    }

    // 判断是否有 NVIDIA 独显（用于跳过 Intel 核显）
    let has_nvidia = sensors
        .iter()
        .any(|s| s.hardware_type.eq_ignore_ascii_case("GpuNvidia"));

    let mut gpus = Vec::new();
    for hw_name in &gpu_hardware_names {
        // 获取该 GPU 的 hardware_type
        let hw_type = sensors
            .iter()
            .find(|s| s.hardware == *hw_name)
            .map(|s| s.hardware_type.clone())
            .unwrap_or_default();

        // NVIDIA 独显存在时跳过 Intel 核显（AMD 核显保留，老 AMD A 系列 APU 需要显示）
        if has_nvidia && hw_type.eq_ignore_ascii_case("GpuIntel") {
            log::debug!("跳过 Intel 核显: 存在 NVIDIA 独显");
            continue;
        }

        let name = hw_name.clone();

        let temperature = extract_sensor(
            sensors, "Temperature", &hw_type, Some(hw_name),
            &["GPU Core", "GPU", "Core", "GPU Temperature"],
            false,
        ).map(|(v, _)| v);

        let usage = extract_sensor(
            sensors, "Load", &hw_type, Some(hw_name),
            &["GPU Core", "D3D 3D", "GPU", "D3D Usage", "Core"],
            false,
        ).map(|(v, _)| v as u32);

        let fan_speed = extract_all_avg(sensors, "Fan", &hw_type, Some(hw_name), &["GPU Fan", "GPU", "Fans"])
            .map(|v| v as u32);

        let power = extract_sensor(
            sensors, "Power", &hw_type, Some(hw_name),
            &["GPU Power", "GPU Package", "GPU Chip Power", "Power", "Package"],
            false,
        ).map(|(v, _)| v as u32);

        let clock = extract_sensor(
            sensors, "Clock", &hw_type, Some(hw_name),
            &["GPU Core", "GPU", "Core"],
            false,
        ).map(|(v, _)| v as u32);

        let vram_used = extract_sensor(
            sensors, "SmallData", &hw_type, Some(hw_name),
            &["GPU Memory Used", "D3D Shared Memory Used", "GPU Memory"],
            false,
        ).map(|(v, _)| v as u32);

        let vram_total = extract_sensor(
            sensors, "SmallData", &hw_type, Some(hw_name),
            &["GPU Memory Total", "GPU Memory"],
            false,
        ).map(|(v, _)| v as u32);

        let memory_clock = extract_sensor(
            sensors, "Clock", &hw_type, Some(hw_name),
            &["GPU Memory", "Memory"],
            false,
        ).map(|(v, _)| v as u32);

        let voltage = extract_sensor(
            sensors, "Voltage", &hw_type, Some(hw_name),
            &["GPU Core Voltage", "GPU Core", "GPU Voltage", "GPU", "Core"],
            false,
        ).map(|(v, _)| v);

        gpus.push(GpuSensorData {
            name,
            hardware_type: hw_type,
            temperature,
            usage,
            fan_speed,
            power,
            clock,
            memory_clock,
            vram_used,
            vram_total,
            voltage,
        });
    }

    // 排序：独显（NVIDIA）优先 > 其它独显 > 核显，同优先级按名称字典序稳定
    gpus.sort_by(|a, b| {
        let ap = gpu_priority(&a.hardware_type, &a.name);
        let bp = gpu_priority(&b.hardware_type, &b.name);
        ap.cmp(&bp).then_with(|| a.name.cmp(&b.name))
    });

    gpus
}

/// 判断是否为集成显卡（核显）：
/// - NVIDIA 一律为独显
/// - Intel 默认视为核显（Intel Arc 独立显卡除外）
/// - AMD 依据名称特征判断（核显通常为 "AMD Radeon(TM) Graphics" / "Vega 3/8/11 Graphics"）
fn is_integrated_gpu(hw_type: &str, name: &str) -> bool {
    let t = hw_type.to_lowercase();
    let n = name.to_lowercase();
    if t.contains("nvidia") {
        false
    } else if t.contains("intel") {
        // Intel Arc 为独立显卡
        !(n.contains("arc") || n.contains("a3") || n.contains("a5") || n.contains("a7"))
    } else if t.contains("amd") {
        n.contains("graphics") || n.contains("vega 3") || n.contains("vega 8") || n.contains("vega 11")
    } else {
        true
    }
}

/// GPU 展示优先级，越小越靠前：NVIDIA 独显 > 其它独显 > 核显
fn gpu_priority(hw_type: &str, name: &str) -> u8 {
    if hw_type.eq_ignore_ascii_case("GpuNvidia") {
        0
    } else if is_integrated_gpu(hw_type, name) {
        2
    } else {
        1
    }
}

pub fn collect_hardware_data() -> OverlayHardwareData {
let fps = crate::game_fps::get_cached_fps();
let fps_1low = crate::game_fps::get_cached_1low_fps();
let fps_01low = crate::game_fps::get_cached_01low_fps();

    let selected_maps = {
        let settings = get_or_init_settings();
        settings.delta_password_maps.clone()
    };
    let delta_password = if selected_maps.is_empty() {
        crate::delta_force::get_cached_delta_password()
    } else {
        crate::delta_force::get_cached_delta_password_filtered(&selected_maps)
    };

    let game_ping = crate::game_ping::get_cached_ping();

    let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let last_time = LAST_LHML_UPDATE.load(std::sync::atomic::Ordering::Relaxed);
    
    let use_cached_lhml = if current_time - last_time < 1000 {
        true
    } else {
        LAST_LHML_UPDATE.store(current_time, std::sync::atomic::Ordering::Relaxed);
        false
    };

    // 从 LHML (NexBoxMonitor) 获取硬件传感器数据
    let (cpu_usage, cpu_temp, cpu_clock, cpu_voltage, cpu_power, cpu_fan_speed, ssd_temp, memory_usage, gpu_temp, gpu_usage, gpu_fan_speed, gpu_power, gpu_clock, gpu_vram_used, gpu_vram_total, gpu_memory_clock, gpu_voltage, gpu_sensors, active_gpu_index) =
        if use_cached_lhml {
            let prev = CURRENT_HARDWARE_DATA.lock().unwrap().clone().unwrap_or_default();
            (
                prev.cpu_usage, prev.cpu_temp, prev.cpu_clock, prev.cpu_voltage, prev.cpu_power, prev.cpu_fan_speed, prev.ssd_temp, prev.memory_usage, prev.gpu_temp, prev.gpu_usage, prev.gpu_fan_speed, prev.gpu_power, prev.gpu_clock, prev.gpu_vram_used, prev.gpu_vram_total, prev.gpu_memory_clock, prev.gpu_voltage, prev.gpu_sensors, prev.active_gpu_index
            )
        } else {
        match crate::sensor::read_lhm_sensors() {
            Ok(response) => {
                // CPU 占用 (Load 类型)
                let (cpu_usage, cpu_usage_name) = extract_sensor(
                    &response.sensors,
                    "Load",
                    "CPU",
                    None,
                    &["CPU Total", "Total"],
                    false,
                ).unzip();
                let mut cpu_usage = cpu_usage.map(|v| v as u16);
                // LHML 缺 CPU Load 传感器时回退 sysinfo（用户态，任何 CPU 可用）：
                // AMD FX/Bulldozer 被 LibreHardwareMonitor 0.9.6 禁用支持（PawnIO 模块死机问题），
                // 这类 CPU 没有任何 LHML CPU 传感器（温度需另装 PawnIO 后走 SuperIO 兜底）
                if cpu_usage.is_none() {
                    cpu_usage = crate::hardware::get_cpu_dynamic_info();
                }
                // CPU 温度 (AMD Ryzen: Core (Tctl/Tdie), Intel: CPU Package, 老AMD(A系列): SuperIO/Motherboard)
                let (cpu_temp, cpu_name) = extract_sensor(
                    &response.sensors,
                    "Temperature",
                    "CPU",
                    None,
                    &["Core (Tctl/Tdie)", "CPU Package", "Tctl", "Core"],
                    false,
                )
                    .or_else(|| extract_sensor(&response.sensors, "Temperature", "SuperIO", None, &["CPU", "CPU Core", "CPU Temperature", "Core"], false))
                    .or_else(|| extract_sensor(&response.sensors, "Temperature", "Motherboard", None, &["CPU", "CPU Core", "CPU Temperature", "Core"], false))
                    .unzip();
                // CPU 频率 (老AMD可能通过SuperIO报告总线频率)
                let (cpu_clock, cpu_clock_name) = extract_sensor(
                    &response.sensors,
                    "Clock",
                    "CPU",
                    None,
                    &["Cores (Average)", "Core #1", "Bus Speed"],
                    true,
                )
                    .or_else(|| extract_sensor(&response.sensors, "Clock", "SuperIO", None, &["Bus Speed", "CPU Clock"], true))
                    .unzip();
                let cpu_clock = cpu_clock.map(|v| v as u32);
                // LHML 缺 CPU 频率传感器时回退 CallNtPowerInformation（用户态，免驱动）
                let cpu_clock = cpu_clock.or_else(crate::hardware::get_cpu_clock_mhz_fallback);
                // CPU 电压 (老AMD也可能通过SuperIO报告)
                let cpu_voltage_result = extract_sensor(
                    &response.sensors,
                    "Voltage",
                    "CPU",
                    None,
                    &["CPU Core", "Vcore", "Core", "CPU VCore"],
                    false,
                )
                    .or_else(|| extract_sensor(&response.sensors, "Voltage", "SuperIO", None, &["CPU Core", "Vcore", "CPU VCore", "CPU"], false));
                let cpu_voltage = cpu_voltage_result.as_ref().map(|(v, _)| *v);
                // CPU 功耗
                let cpu_power_result = extract_sensor(
                    &response.sensors,
                    "Power",
                    "CPU",
                    None,
                    &["Package", "CPU Package", "CPU Cores", "CPU Core"],
                    false,
                );
                let cpu_power = cpu_power_result.as_ref().map(|(v, _)| *v);
                // SSD 温度 (跳过 0 值)
                let (ssd_temp, ssd_name) = extract_sensor(
                    &response.sensors,
                    "Temperature",
                    "Storage",
                    None,
                    &["Composite Temperature", "Temperature #1", "Temperature"],
                    true,
                ).unzip();

                // 内存占用 (从 LHML Memory 硬件获取)
                let memory_usage = extract_sensor(
                    &response.sensors,
                    "Load",
                    "Memory",
                    None,
                    &["Memory"],
                    false,
                ).map(|(v, _)| v);

                // 调试：打印所有 RAM 传感器
                let memory_sensors: Vec<_> = response.sensors.iter()
                    .filter(|s| s.hardware_type.eq_ignore_ascii_case("Memory"))
                    .map(|s| format!("{}|{}|{}={}", s.hardware_type, s.sensor_type, s.name, s.value))
                    .collect();
                log::debug!("LHML Memory sensors: {:?}", memory_sensors);

                // 调试：打印所有 CPU 和主板传感器
                let cpu_sensors: Vec<_> = response.sensors.iter()
                    .filter(|s| s.hardware_type.eq_ignore_ascii_case("CPU"))
                    .map(|s| format!("{}|{}|{}={}", s.hardware_type, s.sensor_type, s.name, s.value))
                    .collect();
                log::debug!("LHML CPU sensors: {:?}", cpu_sensors);
                // 打印所有非 GPU/CPU/HDD 的硬件类型（用于排查主板传感器）
                let other_types: std::collections::BTreeSet<_> = response.sensors.iter()
                    .filter(|s| !s.hardware_type.to_lowercase().starts_with("gpu")
                        && !s.hardware_type.eq_ignore_ascii_case("CPU")
                        && !s.hardware_type.eq_ignore_ascii_case("Storage")
                        && !s.hardware_type.eq_ignore_ascii_case("RAM"))
                    .map(|s| format!("{}|{}|{}={}", s.hardware_type, s.sensor_type, s.name, s.value))
                    .collect();
                log::debug!("LHML other sensors: {:?}", other_types);

                // 调试：打印 SuperIO/Motherboard 传感器（用于排查老AMD CPU温压数据）
                let superio_sensors: Vec<_> = response.sensors.iter()
                    .filter(|s| s.hardware_type.eq_ignore_ascii_case("SuperIO")
                        || s.hardware_type.eq_ignore_ascii_case("Motherboard"))
                    .map(|s| format!("{}|{}|{}={}", s.hardware_type, s.sensor_type, s.name, s.value))
                    .collect();
                if !superio_sensors.is_empty() {
                    log::debug!("LHML SuperIO/Motherboard sensors: {:?}", superio_sensors);
                }

                // 调试：打印所有 GPU 传感器
                let gpu_sensors: Vec<_> = response.sensors.iter()
                    .filter(|s| s.hardware_type.to_lowercase().starts_with("gpu"))
                    .map(|s| format!("{}|{}|{}={}", s.hardware_type, s.sensor_type, s.name, s.value))
                    .collect();
                log::debug!("LHML GPU sensors: {:?}", gpu_sensors);

                // ─── 收集所有 GPU 传感器数据（支持多 GPU 切换）───
                let gpu_sensors = collect_all_gpu_sensors(&response.sensors);

                // 确定当前活跃 GPU 索引（保留之前的选择，若仍有效）
                let prev_active = ACTIVE_GPU_INDEX.load(std::sync::atomic::Ordering::Relaxed);
                let active_gpu_index = if prev_active < gpu_sensors.len() {
                    prev_active
                } else {
                    0 // 回退到第一个 GPU（独显优先）
                };

                // 从活跃 GPU 提取单个字段（向后兼容旧的 overlay 页面）
                let active_gpu = gpu_sensors.get(active_gpu_index);
                let gpu_temp = active_gpu.and_then(|g| g.temperature);
                let gpu_usage = active_gpu.and_then(|g| g.usage);
                let gpu_fan_speed = active_gpu.and_then(|g| g.fan_speed);
                let gpu_power = active_gpu.and_then(|g| g.power);
                let gpu_clock = active_gpu.and_then(|g| g.clock);
                let gpu_vram_used = active_gpu.and_then(|g| g.vram_used);
                let gpu_vram_total = active_gpu.and_then(|g| g.vram_total);
                let gpu_memory_clock = active_gpu.and_then(|g| g.memory_clock);
                let gpu_voltage = active_gpu.and_then(|g| g.voltage);
                let gpu_temp_name = active_gpu.and_then(|g| {
                    if g.temperature.is_some() { Some(g.name.clone()) } else { None }
                });
                let gpu_usage_name = active_gpu.and_then(|g| {
                    if g.usage.is_some() { Some(g.name.clone()) } else { None }
                });
                let gpu_power_name = active_gpu.and_then(|g| {
                    if g.power.is_some() { Some(g.name.clone()) } else { None }
                });
                let gpu_clock_name = active_gpu.and_then(|g| {
                    if g.clock.is_some() { Some(g.name.clone()) } else { None }
                });

                // CPU 风扇
                // LHML 中 CPU 风扇传感器出现在 SuperIO（Nuvoton 等）或 Motherboard 硬件类型下
                // 名称通常为 "Fan #1"、"CPU Fan"、"CPU1 Fan" 等，优先取第一个非零值风扇作为 CPU 风扇
                let cpu_fan_speed = {
                    let fan = extract_sensor(&response.sensors, "Fan", "SuperIO", None, &["Fan #1", "Fan #2", "CPU Fan", "CPU1 Fan", "CPUFAN"], false)
                        .or_else(|| {
                            // 取 SuperIO 下第一个非零 RPM 的风扇
                            response.sensors.iter()
                                .filter(|s| s.sensor_type == "Fan"
                                    && s.hardware_type.eq_ignore_ascii_case("SuperIO")
                                    && s.value > 0.0)
                                .next()
                                .map(|s| (s.value, s.name.clone()))
                        })
                        .or_else(|| {
                            // 兜底：任意 SuperIO 风扇
                            response.sensors.iter()
                                .filter(|s| s.sensor_type == "Fan"
                                    && s.hardware_type.eq_ignore_ascii_case("SuperIO"))
                                .next()
                                .map(|s| (s.value, s.name.clone()))
                        })
                        .or_else(|| {
                            // 再兜底：Motherboard 下的风扇
                            response.sensors.iter()
                                .filter(|s| s.sensor_type == "Fan"
                                    && s.hardware_type.eq_ignore_ascii_case("Motherboard"))
                                .next()
                                .map(|s| (s.value, s.name.clone()))
                        });
                    fan.map(|(v, _)| v as u32)
                };

                log::debug!(
                    "LHML: CPU占用={:?}({}) CPU温度={:?}({}) CPU频率={:?}({}) CPU电压={:?}V CPU功耗={:?}W SSD温度={:?}({}) 内存占用={:?}% GPU温度={:?}({}) GPU占用={:?}({}) GPU风扇={:?}RPM GPU功耗={:?}({}) GPU频率={:?}({}) 显存={:?}/{:?}MB 显存频率={:?}MHz GPU电压={:?}V GPUs={}",
                    cpu_usage,
                    cpu_usage_name.as_deref().unwrap_or("N/A"),
                    cpu_temp,
                    cpu_name.as_deref().unwrap_or("N/A"),
                    cpu_clock,
                    cpu_clock_name.as_deref().unwrap_or("N/A"),
                    cpu_voltage,
                    cpu_power,
                    ssd_temp,
                    ssd_name.as_deref().unwrap_or("N/A"),
                    memory_usage,
                    gpu_temp,
                    gpu_temp_name.as_deref().unwrap_or("N/A"),
                    gpu_usage,
                    gpu_usage_name.as_deref().unwrap_or("N/A"),
                    gpu_fan_speed,
                    gpu_power,
                    gpu_power_name.as_deref().unwrap_or("N/A"),
                    gpu_clock,
                    gpu_clock_name.as_deref().unwrap_or("N/A"),
                    gpu_vram_used,
                    gpu_vram_total,
                    gpu_memory_clock,
                    gpu_voltage,
                    gpu_sensors.len(),
                );

                (cpu_usage, cpu_temp, cpu_clock, cpu_voltage, cpu_power, cpu_fan_speed, ssd_temp, memory_usage, gpu_temp, gpu_usage, gpu_fan_speed, gpu_power, gpu_clock, gpu_vram_used, gpu_vram_total, gpu_memory_clock, gpu_voltage, gpu_sensors, active_gpu_index)
            }
            Err(e) => {
                // 传感器启动早期未就绪是正常现象，降级为 debug 日志
                if e.contains("尚未就绪") {
                    log::debug!("LHML 传感器读取跳过: {e}");
                } else {
                    log::warn!("LHML 传感器读取失败: {e}");
                }
                // 子进程整体失败时，CPU 占用/频率仍可由用户态兜底获取，避免悬窗 CPU 长期显示 --
                let cpu_usage = crate::hardware::get_cpu_dynamic_info();
                let cpu_clock = crate::hardware::get_cpu_clock_mhz_fallback();
                (cpu_usage, None, cpu_clock, None, None, None, None, None, None, None, None, None, None, None, None, None, None, Vec::new(), 0)
            }
        }
    };

    let new_data = OverlayHardwareData {
        fps,
        fps_1low,
        fps_01low,
        cpu_usage,
        cpu_temp,
        cpu_clock,
        gpu_temp,
        gpu_usage,
        memory_usage,
        delta_password,
        game_ping,
        cpu_fan_speed,
        gpu_fan_speed,
        gpu_power,
        gpu_clock,
        gpu_vram_used,
        gpu_vram_total,
        gpu_memory_clock,
        cpu_voltage,
        gpu_voltage,
        cpu_power,
        ssd_temp,
        net_time_offset_ms: get_net_offset_ms(),
        gpu_sensors,
        active_gpu_index,
    };

    let prev_data = CURRENT_HARDWARE_DATA.lock().unwrap().clone();
    let result = if let Some(prev) = prev_data {
        let has_new_gpu_data = !new_data.gpu_sensors.is_empty();
        OverlayHardwareData {
            fps: new_data.fps.or(prev.fps),
            fps_1low: new_data.fps_1low.or(prev.fps_1low),
            fps_01low: new_data.fps_01low.or(prev.fps_01low),
            cpu_usage: new_data.cpu_usage.or(prev.cpu_usage),
            cpu_temp: new_data.cpu_temp.or(prev.cpu_temp),
            cpu_clock: new_data.cpu_clock.or(prev.cpu_clock),
            gpu_temp: new_data.gpu_temp.or(prev.gpu_temp),
            gpu_usage: new_data.gpu_usage.or(prev.gpu_usage),
            memory_usage: new_data.memory_usage.or(prev.memory_usage),
            delta_password: new_data.delta_password.or_else(|| prev.delta_password.clone()),
            game_ping: new_data.game_ping.or(prev.game_ping),
            cpu_fan_speed: new_data.cpu_fan_speed.or(prev.cpu_fan_speed),
            gpu_fan_speed: new_data.gpu_fan_speed.or(prev.gpu_fan_speed),
            gpu_power: new_data.gpu_power.or(prev.gpu_power),
            gpu_clock: new_data.gpu_clock.or(prev.gpu_clock),
            gpu_vram_used: new_data.gpu_vram_used.or(prev.gpu_vram_used),
            gpu_vram_total: new_data.gpu_vram_total.or(prev.gpu_vram_total),
            gpu_memory_clock: new_data.gpu_memory_clock.or(prev.gpu_memory_clock),
            cpu_voltage: new_data.cpu_voltage.or(prev.cpu_voltage),
            gpu_voltage: new_data.gpu_voltage.or(prev.gpu_voltage),
            cpu_power: new_data.cpu_power.or(prev.cpu_power),
            ssd_temp: new_data.ssd_temp.or(prev.ssd_temp),
            net_time_offset_ms: new_data.net_time_offset_ms.or(prev.net_time_offset_ms),
            gpu_sensors: if has_new_gpu_data { new_data.gpu_sensors } else { prev.gpu_sensors },
            active_gpu_index: if has_new_gpu_data { new_data.active_gpu_index } else { prev.active_gpu_index },
        }
    } else {
        new_data
    };

    result
}

#[cfg(target_os = "windows")]
mod win32 {
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Graphics::GdiPlus::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::UI::Accessibility::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use std::path::PathBuf;
    use std::ptr;
    use std::sync::Mutex;
    use std::result::Result::Ok;

    static GDIPLUS_TOKEN: Mutex<Option<usize>> = Mutex::new(None);
    static WIN_EVENT_HOOK: Mutex<Option<usize>> = Mutex::new(None);
    static FONT_PATH: Mutex<Option<String>> = Mutex::new(None);

    unsafe extern "system" fn win_event_proc(
        _h_win_event_hook: *mut std::ffi::c_void,
        _event: u32,
        hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _dw_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        if id_object != 0 || hwnd.is_null() {
            return;
        }
        let overlay_hwnd = super::OVERLAY_HANDLE.load(std::sync::atomic::Ordering::SeqCst);
        if overlay_hwnd.is_null() {
            return;
        }
        if hwnd != overlay_hwnd {
            SetWindowPos(
                overlay_hwnd,
                HWND_TOPMOST,
                0, 0, 0, 0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub unsafe fn install_topmost_guard() {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if !hook.is_null() {
            let mut lock = WIN_EVENT_HOOK.lock().unwrap();
            *lock = Some(hook as usize);
        }
    }

    pub unsafe fn uninstall_topmost_guard() {
        let mut lock = WIN_EVENT_HOOK.lock().unwrap();
        if let Some(hook) = lock.take() {
            UnhookWinEvent(hook as *mut std::ffi::c_void);
        }
    }

    fn find_misans_font_path() -> Option<PathBuf> {
        let font_name = "MiSansVF.ttf";
        // 1. 通过 exe 路径查找
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let candidates = [
                    parent.join("Fonts").join(font_name),
                    parent.join("fonts").join(font_name),
                    parent.join("..").join("Fonts").join(font_name),
                    parent.join("..").join("..").join("Fonts").join(font_name),
                    parent.join("..").join("..").join("..").join("Fonts").join(font_name),
                ];
                for path in &candidates {
                    if path.exists() {
                        return Some(path.canonicalize().unwrap_or_else(|_| path.clone()));
                    }
                }
            }
        }
        // 2. 编译时路径（开发环境）
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let candidates = [
            manifest_dir.join("..").join("Fonts").join(font_name),
            manifest_dir.join("..").join("public").join("fonts").join(font_name),
        ];
        for path in &candidates {
            if path.exists() {
                return Some(path.canonicalize().unwrap_or_else(|_| path.clone()));
            }
        }
        None
    }

    pub unsafe fn load_misans_font() {
        if let Some(path) = find_misans_font_path() {
            let path_str = path.to_string_lossy().to_string();
            let wide: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
            let result = AddFontResourceExW(wide.as_ptr(), FR_PRIVATE, ptr::null_mut());
            if result > 0 {
                if let Ok(mut lock) = FONT_PATH.lock() {
                    *lock = Some(path_str.clone());
                }
                log::info!("overlay: 已加载字体: {}", path_str);
            } else {
                log::warn!("overlay: 加载字体失败: {} (error: {})", path_str, GetLastError());
            }
        } else {
            log::info!("overlay: 未找到 MiSansVF.ttf 字体文件，使用系统默认字体");
        }
    }

    pub unsafe fn unload_misans_font() {
        if let Ok(mut lock) = FONT_PATH.lock() {
            if let Some(path) = lock.take() {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                RemoveFontResourceExW(wide.as_ptr(), FR_PRIVATE, ptr::null_mut());
                log::info!("overlay: 已卸载 MiSans 字体");
            }
        }
    }

    pub unsafe fn init_gdiplus() -> bool {
        let mut token = GDIPLUS_TOKEN.lock().unwrap();
        if token.is_some() {
            return true;
        }

        let mut input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };

        let mut token_value: usize = 0;
        let result = GdiplusStartup(&mut token_value, &mut input, ptr::null_mut());

        if result == 0 {
            *token = Some(token_value);
            // 加载 MiSans 字体供 GDI 渲染使用
            load_misans_font();
            true
        } else {
            log::error!("GDI+ 初始化失败: {}", result);
            false
        }
    }

    pub unsafe fn shutdown_gdiplus() {
        let mut token = GDIPLUS_TOKEN.lock().unwrap();
        if let Some(t) = token.take() {
            GdiplusShutdown(t);
            unload_misans_font();
        }
    }

    fn calculate_window_width(settings: &super::OverlaySettings) -> i32 {
        // 使用可配置的单项宽度
        let normal_item_width = settings.item_width as i32;
        // 默认密码项宽度（逻辑像素）
        let mut password_item_width = 220;
        if settings.display_items.iter().any(|item| item.id == "delta_password" && item.enabled) {
            if let Ok(lock) = super::CURRENT_HARDWARE_DATA.lock() {
                if let Some(ref data) = *lock {
                    if let Some(ref pwd) = data.delta_password {
                        unsafe {
                            // 使用屏幕 DC 和字体测量文本宽度，按 DPI 缩放
                            let screen_dc = GetDC(ptr::null_mut());
                            if !screen_dc.is_null() {
                                let dpi_x = GetDeviceCaps(screen_dc, 88);
                                let dpi_scale = dpi_x as f32 / 96.0;
                                let hfont = create_compatible_font(dpi_scale, &settings.font, settings.font_size);
                                if !hfont.is_null() {
                                    let val_w = measure_text_width(screen_dc, hfont, pwd);
                                    let est = val_w + (12.0 * dpi_scale) as i32 + 20;
                                                if est > password_item_width {
                                                    password_item_width = est;
                                                }
                                    DeleteObject(hfont as _);
                                }
                                ReleaseDC(ptr::null_mut(), screen_dc);
                            }
                        }
                    }
                }
            }
        }

        let mut width = 0i32;
        let mut enabled_count = 0i32;
        for item in &settings.display_items {
            if item.enabled {
                enabled_count += 1;
                match item.id.as_str() {
                    "delta_password" => { width += password_item_width; }
                    _ => { width += normal_item_width; }
                }
            }
        }

        // 自定义项宽度（各 150px 基础宽度）
        let custom_item_width = 150;
        let mut custom_count = 0i32;
        for custom in &settings.custom_items {
            if custom.enabled && !custom.text.is_empty() {
                width += custom_item_width;
                custom_count += 1;
            }
        }

        if width == 0 { return 200; }
        enabled_count += custom_count;
        let sep_count = if enabled_count > 1 { enabled_count - 1 } else { 0 };
        width + 32 + sep_count * 16
    }

    pub unsafe fn create_overlay_window(
        settings: &super::OverlaySettings,
    ) -> Result<HWND, String> {
        init_gdiplus();

        let h_instance = GetModuleHandleW(ptr::null());
        if h_instance.is_null() {
            return Err("无法获取模块句柄".to_string());
        }

        let class_name = windows_sys::core::w!("NexBoxOverlayPanel");

        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: LoadIconW(h_instance, IDI_APPLICATION),
            hCursor: LoadCursorW(h_instance, IDC_ARROW),
            hbrBackground: CreateSolidBrush(0),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name,
        };

        if RegisterClassW(&wnd_class) == 0 {
            let error = GetLastError();
            if error != 1410 {
                return Err(format!("注册窗口类失败: {}", error));
            }
        }

        let screen_dc = GetDC(ptr::null_mut());
        let dpi_x = if screen_dc.is_null() { 96 } else { GetDeviceCaps(screen_dc, 88) };
        if !screen_dc.is_null() {
            ReleaseDC(ptr::null_mut(), screen_dc);
        }
        let dpi_scale = dpi_x as f32 / 96.0;

        let logical_width = calculate_window_width(settings);
        let base_height = if settings.style == "dynamic_island" { 36 } else { 28 };
        let logical_height = base_height + (settings.font_size.saturating_sub(13) * 2) as i32;
        let physical_width = (logical_width as f32 * dpi_scale) as i32;
        let physical_height = (logical_height as f32 * dpi_scale) as i32;

        // 使用保存的位置，或使用默认位置
        let (x, y) = if let (Some(px), Some(py)) = (settings.position_x, settings.position_y) {
            (px, py)
        } else {
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let default_x = (screen_width - physical_width) / 2;
            let default_y = if settings.style == "dynamic_island" { 4 } else { 0 };
            (default_x, default_y)
        };

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            class_name,
            windows_sys::core::w!("NexBox Overlay Panel"),
            WS_POPUP,
            x,
            y,
            physical_width,
            physical_height,
            ptr::null_mut(),
            ptr::null_mut(),
            h_instance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            return Err("创建窗口失败".to_string());
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        Ok(hwnd)
    }

    pub unsafe fn destroy_overlay_window(hwnd: HWND) -> bool {
        if hwnd.is_null() {
            return false;
        }
        KillTimer(hwnd, 1);
        DestroyWindow(hwnd) != 0
    }

    unsafe fn create_compatible_font(dpi_scale: f32, font_name: &str, font_size: u32) -> HFONT {
        let font_height = -((font_size as f32) * dpi_scale).round() as i32;
        let wide_name: Vec<u16> = font_name.encode_utf16().chain(std::iter::once(0)).collect();
        CreateFontW(
            font_height,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            wide_name.as_ptr(),
        )
    }

    unsafe fn measure_text_width(hdc: HDC, hfont: HFONT, text: &str) -> i32 {
        let old_font = SelectObject(hdc, hfont as _);
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut size = SIZE { cx: 0, cy: 0 };
        GetTextExtentPoint32W(hdc, wide.as_ptr(), (wide.len() - 1) as i32, &mut size);
        SelectObject(hdc, old_font);
        size.cx
    }

    struct DisplayItem {
        label: String,
        value: String,
        label_width: i32,
        value_width: i32,
        total_width: i32,
        custom_color: Option<u32>,
    }

    fn parse_hex_color(hex: &str) -> u32 {
        let hex = hex.trim_start_matches('#');
        if let Ok(val) = u32::from_str_radix(hex, 16) {
            // 前端使用 #RRGGBB 格式，GDI 颜色格式为 0x00BBGGRR
            let r = (val >> 16) & 0xFF;
            let g = (val >> 8) & 0xFF;
            let b = val & 0xFF;
            (b << 16) | (g << 8) | r
        } else {
            0x00FFFFFF
        }
    }

    /// 将 GDI 颜色 0x00BBGGRR 转换为不透明直线 alpha 的 ARGB（0xFFRRGGBB），供 GDI+ 文字使用。
    fn gdi_to_argb(gdi: u32) -> u32 {
        let r = gdi & 0xFF;
        let g = (gdi >> 8) & 0xFF;
        let b = (gdi >> 16) & 0xFF;
        0xFF000000 | (r << 16) | (g << 8) | b
    }

    fn build_display_items(
        settings: &super::OverlaySettings,
        data: &super::OverlayHardwareData,
    ) -> Vec<DisplayItem> {
        let mut items = Vec::new();
        for display_item in &settings.display_items {
            if !display_item.enabled {
                continue;
            }
            match display_item.id.as_str() {
                "time" => {
                    // 优先使用网络标准时间；未同步成功时回退到北京时间（UTC+8）
                    let now = if let Some(offset_ms) = super::get_net_offset_ms() {
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0)
                            + offset_ms;
                        chrono::DateTime::from_timestamp_millis(now_ms)
                            .map(|dt| dt.with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap()))
                    } else {
                        Some(chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap()))
                    };
                    if let Some(now) = now {
                        items.push(DisplayItem {
                            label: String::new(),
                            value: now.format("%H:%M:%S").to_string(),
                            label_width: 0, value_width: 0, total_width: 0, custom_color: None,
                        });
                    }
                }
                "cpu_usage" => {
                    let val = data.cpu_usage.map(|v| format!("{}%", v)).unwrap_or_else(|| "--%".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_temp" => {
                    let val = data.gpu_temp.map(|v| format!("{:.0}°C", v)).unwrap_or_else(|| "--°C".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_usage" => {
                    let val = data.gpu_usage.map(|v| format!("{}%", v)).unwrap_or_else(|| "--%".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "memory_usage" => {
                    let val = data.memory_usage.map(|v| format!("{}%", v.round() as i32)).unwrap_or_else(|| "--%".to_string());
                    items.push(DisplayItem { label: "RAM".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "delta_password" => {
                    let val = data.delta_password.as_deref().unwrap_or("--").to_string();
                    items.push(DisplayItem { label: "".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "game_ping" => {
                    let val = data.game_ping.map(|v| format!("{}ms", v)).unwrap_or_else(|| "--ms".to_string());
                    items.push(DisplayItem { label: "PING".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "fps" => {
                    let (val, color) = match data.fps {
                        Some(v) => {
                            let c = if v < 30 {
                                0x000000FFu32
                            } else if v < 60 {
                                0x0000FFFFu32
                            } else {
                                0x0000FF00u32
                            };
                            (format!("{}", v), Some(c))
                        }
                        None => ("--".to_string(), None),
                    };
                    items.push(DisplayItem { label: "FPS".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: color });
                }
                "fps_1low" => {
                    let (val, color) = match data.fps_1low {
                        Some(v) => {
                            let c = if v < 30 {
                                0x000000FFu32
                            } else if v < 60 {
                                0x0000FFFFu32
                            } else {
                                0x0000FF00u32
                            };
                            (format!("{}", v), Some(c))
                        }
                        None => ("--".to_string(), None),
                    };
                    items.push(DisplayItem { label: "1%".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: color });
                }
                "fps_01low" => {
                    let (val, color) = match data.fps_01low {
                        Some(v) => {
                            let c = if v < 30 {
                                0x000000FFu32
                            } else if v < 60 {
                                0x0000FFFFu32
                            } else {
                                0x0000FF00u32
                            };
                            (format!("{}", v), Some(c))
                        }
                        None => ("--".to_string(), None),
                    };
                    items.push(DisplayItem { label: "0.1%".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: color });
                }
                "cpu_fan_speed" => {
                    let val = data.cpu_fan_speed.map(|v| format!("{}RPM", v)).unwrap_or_else(|| "--RPM".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_fan_speed" => {
                    let val = data.gpu_fan_speed.map(|v| format!("{}RPM", v)).unwrap_or_else(|| "--RPM".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_power" => {
                    let val = data.gpu_power.map(|v| format!("{}W", v)).unwrap_or_else(|| "--W".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_clock" => {
                    let val = data.gpu_clock.map(|v| format!("{}MHz", v)).unwrap_or_else(|| "--MHz".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_vram" => {
                    let val = match (data.gpu_vram_used, data.gpu_vram_total) {
                        (Some(used), Some(total)) => {
                            let used_gb = used as f64 / 1024.0;
                            let total_gb = total as f64 / 1024.0;
                            format!("{:.1}G/{:.1}G", used_gb, total_gb)
                        }
                        _ => "--G/--G".to_string(),
                    };
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_memory_clock" => {
                    let val = data.gpu_memory_clock.map(|v| format!("{}MHz", v)).unwrap_or_else(|| "--MHz".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "gpu_voltage" => {
                    let val = data.gpu_voltage.map(|v| format!("{:.3}V", v)).unwrap_or_else(|| "--V".to_string());
                    items.push(DisplayItem { label: "GPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "cpu_temp" => {
                    let val = data.cpu_temp.map(|v| format!("{:.0}°C", v)).unwrap_or_else(|| "--°C".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "cpu_clock" => {
                    let val = data.cpu_clock.map(|v| format!("{}MHz", v)).unwrap_or_else(|| "--MHz".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "cpu_voltage" => {
                    let val = data.cpu_voltage.map(|v| format!("{:.3}V", v)).unwrap_or_else(|| "--V".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "cpu_power" => {
                    let val = data.cpu_power.map(|v| format!("{:.1}W", v)).unwrap_or_else(|| "--W".to_string());
                    items.push(DisplayItem { label: "CPU".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                "ssd_temp" => {
                    let val = data.ssd_temp.map(|v| format!("{:.0}°C", v)).unwrap_or_else(|| "--°C".to_string());
                    items.push(DisplayItem { label: "硬盘".to_string(), value: val, label_width: 0, value_width: 0, total_width: 0, custom_color: None });
                }
                _ => {}
            }
        }
        for custom in &settings.custom_items {
            if custom.enabled && !custom.text.is_empty() {
                let color = parse_hex_color(&custom.color);
                items.push(DisplayItem {
                    label: String::new(),
                    value: custom.text.clone(),
                    label_width: 0,
                    value_width: 0,
                    total_width: 0,
                    custom_color: Some(color),
                });
            }
        }
        items
    }

    unsafe fn measure_and_layout_items(
        hdc: HDC,
        hfont: HFONT,
        items: &mut [DisplayItem],
        dpi_scale: f32,
    ) -> i32 {
        let gap = (10.0 * dpi_scale) as i32;
        let mut total = 0i32;
        for item in items.iter_mut() {
            item.label_width = measure_text_width(hdc, hfont, &item.label);
            item.value_width = measure_text_width(hdc, hfont, &item.value);
            if item.label.is_empty() {
                item.total_width = item.value_width;
            } else {
                item.total_width = item.label_width + gap + item.value_width;
            }
            total += item.total_width;
        }
        total
    }

    pub unsafe fn draw_overlay_content(
        hwnd: HWND,
        settings: &super::OverlaySettings,
        data: &super::OverlayHardwareData,
    ) {
        let dpi_scale = {
            let dc = GetDC(hwnd);
            let dpi = if dc.is_null() { 96 } else { GetDeviceCaps(dc, 88) };
            if !dc.is_null() {
                ReleaseDC(hwnd, dc);
            }
            dpi as f32 / 96.0
        };

        let hfont = create_compatible_font(dpi_scale, &settings.font, settings.font_size);
        if hfont.is_null() {
            return;
        }

        let font_color = parse_hex_color(&settings.font_color);
        let temp_dc = GetDC(ptr::null_mut());
        let mut items = build_display_items(settings, data);
        let padding = (16.0 * dpi_scale) as i32;
        let item_gap = (16.0 * dpi_scale) as i32;
        let content_width = measure_and_layout_items(temp_dc, hfont, &mut items, dpi_scale);
        ReleaseDC(ptr::null_mut(), temp_dc);
        let sep_count = if items.len() > 1 { items.len() as i32 - 1 } else { 0 };
        let total_content_width = content_width + sep_count * item_gap + padding * 2;
        let logical_height = 28 + (settings.font_size.saturating_sub(13) * 2) as i32;
        let physical_height = (logical_height as f32 * dpi_scale) as i32;

        let dib_width = total_content_width;
        let dib_height = physical_height;

        let screen_dc = GetDC(ptr::null_mut());
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = dib_width;
        bmi.bmiHeader.biHeight = -dib_height;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut std::ffi::c_void = ptr::null_mut();
        let hbitmap = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
        ReleaseDC(ptr::null_mut(), screen_dc);

        if hbitmap.is_null() {
            DeleteObject(hfont as _);
            return;
        }

        let mem_dc = CreateCompatibleDC(ptr::null_mut());
        let old_bmp = SelectObject(mem_dc, hbitmap as HGDIOBJ);

        let mut graphics: *mut GpGraphics = ptr::null_mut();
        if GdipCreateFromHDC(mem_dc, &mut graphics) != 0 {
            SelectObject(mem_dc, old_bmp);
            DeleteObject(hbitmap as HGDIOBJ);
            DeleteDC(mem_dc);
            DeleteObject(hfont as _);
            return;
        }

        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

        let mut clear_brush: *mut GpSolidFill = ptr::null_mut();
        GdipCreateSolidFill(0x00000001, &mut clear_brush);
        GdipFillRectangle(graphics, clear_brush as *mut GpBrush, 0.0, 0.0, dib_width as f32, dib_height as f32);
        GdipDeleteBrush(clear_brush as *mut GpBrush);

        // opacity>0 时才填充背景：opacity=0 时跳过，避免 GDI+ 预乘 alpha 把背景 rgb 归零，
        // 与纯黑文字 rgb=0 混淆（否则背景会被误判为文字而变不透明）。
        if settings.opacity > 0 {
            let bg_argb: u32 = ((settings.opacity as u32) << 24) | 0x00111111;
            let mut bg_brush: *mut GpSolidFill = ptr::null_mut();
            GdipCreateSolidFill(bg_argb, &mut bg_brush);
            GdipFillRectangle(graphics, bg_brush as *mut GpBrush, 0.0, 0.0, dib_width as f32, dib_height as f32);
            GdipDeleteBrush(bg_brush as *mut GpBrush);
        }
        GdipDeleteGraphics(graphics);

        let old_font = SelectObject(mem_dc, hfont as _);
        SetBkMode(mem_dc, TRANSPARENT as i32);

        let gap = (10.0 * dpi_scale) as i32;
        let mut current_x: i32 = padding;
        let win_height_i32 = dib_height;

        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                current_x += item_gap;
            }

            if !item.label.is_empty() {
                let wide_label: Vec<u16> = item.label.encode_utf16().chain(std::iter::once(0)).collect();
                let mut label_rect = RECT {
                    left: current_x,
                    top: 0,
                    right: current_x + item.label_width,
                    bottom: win_height_i32,
                };
                SetTextColor(mem_dc, font_color);
                DrawTextW(
                    mem_dc,
                    wide_label.as_ptr(),
                    (wide_label.len() - 1) as i32,
                    &mut label_rect,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
            }

            let value_x = if item.label.is_empty() {
                current_x
            } else {
                current_x + item.label_width + gap
            };
            let wide_value: Vec<u16> = item.value.encode_utf16().chain(std::iter::once(0)).collect();
            let mut value_rect = RECT {
                left: value_x,
                top: 0,
                right: value_x + item.value_width,
                bottom: win_height_i32,
            };

            let mut color: u32 = font_color;
            if let Some(custom_color) = item.custom_color {
                color = custom_color;
            } else if !item.label.is_empty() && !item.value.contains("--")
                && (item.value.contains("°C") || item.value.contains('%') || item.value.bytes().all(|b| b.is_ascii_digit()))
            {
                let mut num_str = String::new();
                for ch in item.value.chars() {
                    if ch.is_ascii_digit() || ch == '.' {
                        num_str.push(ch);
                    } else if !num_str.is_empty() {
                        break;
                    }
                }
                if !num_str.is_empty() {
                    if let Ok(nf) = num_str.parse::<f32>() {
                        let nv = nf as i32;
                        if nv < 50 {
                            color = 0x0000FF00;
                        } else if nv < 80 {
                            color = 0x0000FFFF;
                        } else {
                            color = 0x000000FF;
                        }
                    }
                }
            }

            SetTextColor(mem_dc, color);
            DrawTextW(
                mem_dc,
                wide_value.as_ptr(),
                (wide_value.len() - 1) as i32,
                &mut value_rect,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );

            current_x += item.total_width;
        }

        SelectObject(mem_dc, old_font);

        if !bits.is_null() {
            let pixels = std::slice::from_raw_parts_mut(
                bits as *mut u32,
                (dib_width * dib_height) as usize,
            );
            for pixel in pixels.iter_mut() {
                let alpha = (*pixel >> 24) & 0xFF;
                let rgb = *pixel & 0x00FFFFFF;
                // GDI 文字像素 alpha=0，清屏基准色为 0x000001 → rgb 为 0x000001 是空白、其余均为 GDI 文字
                // 旧逻辑 rgb != 0 漏掉了纯黑文字（rgb=0），这里用 rgb != 0x000001 同时覆盖黑字和彩色字
                if alpha == 0 && rgb != 0x000001 {
                    *pixel = 0xFF000000 | rgb;
                }
            }
        }

        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let default_x = (screen_width - dib_width) / 2;
        let use_x = settings.position_x.unwrap_or(default_x);
        let use_y = settings.position_y.unwrap_or(0);

        let ppt_dst = POINT { x: use_x, y: use_y };
        let psize = SIZE { cx: dib_width, cy: dib_height };
        let ppt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        UpdateLayeredWindow(
            hwnd,
            ptr::null_mut(),
            &ppt_dst,
            &psize,
            mem_dc,
            &ppt_src,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(mem_dc, old_bmp);
        DeleteObject(hbitmap as HGDIOBJ);
        DeleteDC(mem_dc);
        DeleteObject(hfont as _);
    }

    pub unsafe fn draw_overlay_content_dynamic_island(
        hwnd: HWND,
        settings: &super::OverlaySettings,
        data: &super::OverlayHardwareData,
    ) {
        let dpi_scale = {
            let dc = GetDC(hwnd);
            let dpi = if dc.is_null() { 96 } else { GetDeviceCaps(dc, 88) };
            if !dc.is_null() {
                ReleaseDC(hwnd, dc);
            }
            dpi as f32 / 96.0
        };

        let hfont = create_compatible_font(dpi_scale, &settings.font, settings.font_size);
        if hfont.is_null() {
            return;
        }

        let font_color = parse_hex_color(&settings.font_color);
        let padding = (16.0 * dpi_scale) as i32;
        let item_gap = (16.0 * dpi_scale) as i32;
        let text_gap = (10.0 * dpi_scale) as i32;

        // 用 GDI+ 测量文字宽度（与绘制 GdipDrawString 同一文字引擎），
        // 避免测宽与绘制度量不一致导致文字被裁剪/换行。字体句柄复用给绘制。
        let mut font_handle: *mut GpFont = ptr::null_mut();
        let (dib_width, dib_height, layout_items);
        {
            let temp_dc = GetDC(ptr::null_mut());
            let old_temp_font = SelectObject(temp_dc, hfont as _);
            let mut layout_items_tmp = build_display_items(settings, data);

            let mut mg: *mut GpGraphics = ptr::null_mut();
            let gdiplus_ok = !temp_dc.is_null()
                && GdipCreateFromHDC(temp_dc, &mut mg) == 0 && !mg.is_null()
                && GdipCreateFontFromDC(temp_dc, &mut font_handle) == 0 && !font_handle.is_null();

            let sep_count = if layout_items_tmp.len() > 1 { layout_items_tmp.len() as i32 - 1 } else { 0 };
            let mut content_width: i32 = -1;
            if gdiplus_ok {
                let mut fmt: *mut GpStringFormat = ptr::null_mut();
                if GdipCreateStringFormat(StringFormatFlagsNoWrap as i32, 0, &mut fmt) == 0 && !fmt.is_null() {
                    let mut c = 0i32;
                    for item in layout_items_tmp.iter_mut() {
                        let measure_w = |s: &str| -> i32 {
                            if s.is_empty() {
                                return 0;
                            }
                            let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                            let arect = RectF { X: 0.0, Y: 0.0, Width: 100000.0, Height: 100000.0 };
                            let mut brect = RectF { X: 0.0, Y: 0.0, Width: 0.0, Height: 0.0 };
                            if GdipMeasureString(
                                mg,
                                wide.as_ptr(),
                                (wide.len() - 1) as i32,
                                font_handle,
                                &arect,
                                fmt,
                                &mut brect,
                                ptr::null_mut(),
                                ptr::null_mut(),
                            ) != 0 {
                                return 0;
                            }
                            brect.Width.ceil() as i32
                        };
                        item.label_width = measure_w(&item.label);
                        item.value_width = measure_w(&item.value);
                        item.total_width = if item.label.is_empty() {
                            item.value_width
                        } else {
                            item.label_width + text_gap + item.value_width
                        };
                        c += item.total_width;
                    }
                    content_width = c;
                    // 记录布局有效，供后续 DIB 宽度计算
                    GdipDeleteStringFormat(fmt);
                }
            }

            // GDI+ 测量失败时退回 GDI 测量（保持兼容）
            let content_width = if content_width >= 0 {
                content_width
            } else {
                measure_and_layout_items(temp_dc, hfont, &mut layout_items_tmp, dpi_scale)
            };

            let logical_height = 36 + (settings.font_size.saturating_sub(13) * 2) as i32;
            let tmp_dib_width = content_width + sep_count * item_gap + padding * 2;
            let tmp_dib_height = (logical_height as f32 * dpi_scale) as i32;

            SelectObject(temp_dc, old_temp_font);
            ReleaseDC(ptr::null_mut(), temp_dc);
            if !mg.is_null() {
                GdipDeleteGraphics(mg);
            }

            dib_width = tmp_dib_width.max(2);
            dib_height = tmp_dib_height.max(2);
            layout_items = layout_items_tmp;
        }

        // --- Create 32-bit ARGB DIB section (like crosshair) ---
        let screen_dc = GetDC(ptr::null_mut());
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = dib_width;
        bmi.bmiHeader.biHeight = -dib_height;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut std::ffi::c_void = ptr::null_mut();
        let hbitmap = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut bits, ptr::null_mut(), 0);
        ReleaseDC(ptr::null_mut(), screen_dc);

        if hbitmap.is_null() {
            DeleteObject(hfont as _);
            return;
        }

        let mem_dc = CreateCompatibleDC(ptr::null_mut());
        let old_bmp = SelectObject(mem_dc, hbitmap as HGDIOBJ);

        // --- GDI+ anti-aliased rounded rect background ---
        let mut graphics: *mut GpGraphics = ptr::null_mut();
        if GdipCreateFromHDC(mem_dc, &mut graphics) != 0 {
            SelectObject(mem_dc, old_bmp);
            DeleteObject(hbitmap as HGDIOBJ);
            DeleteDC(mem_dc);
            DeleteObject(hfont as _);
            return;
        }

        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

        // Clear to fully transparent（alpha=0）。GDI+ 按预乘 alpha 写入，透明区 rgb 归零，
        // 圆角外区域保持全透明，配合末尾逆预乘不会再出现黑边/不透明圆角。
        let mut clear_brush: *mut GpSolidFill = ptr::null_mut();
        GdipCreateSolidFill(0x00000000, &mut clear_brush);
        GdipFillRectangle(graphics, clear_brush as *mut GpBrush, 0.0, 0.0, dib_width as f32, dib_height as f32);
        GdipDeleteBrush(clear_brush as *mut GpBrush);

        // Draw rounded rect with GDI+ (proper per-pixel alpha anti-aliasing)。
        // 半透明胶囊背景，alpha 由 opacity 决定；最终统一逆预乘输出直线 alpha。
        if settings.opacity > 0 {
            let bg_argb: u32 = ((settings.opacity as u32) << 24) | 0x00111111;
            let corner_r = dib_height as f32 * 0.5;
            let mut bg_brush: *mut GpSolidFill = ptr::null_mut();
            GdipCreateSolidFill(bg_argb, &mut bg_brush);

            let mut path: *mut GpPath = ptr::null_mut();
            GdipCreatePath(FillModeAlternate, &mut path);
            if !path.is_null() {
                let w = dib_width as f32;
                let h = dib_height as f32;
                let r = corner_r;
                GdipAddPathArc(path, 0.0, 0.0, r * 2.0, r * 2.0, 180.0, 90.0);
                GdipAddPathLine(path, r, 0.0, w - r, 0.0);
                GdipAddPathArc(path, w - r * 2.0, 0.0, r * 2.0, r * 2.0, 270.0, 90.0);
                GdipAddPathLine(path, w, r, w, h - r);
                GdipAddPathArc(path, w - r * 2.0, h - r * 2.0, r * 2.0, r * 2.0, 0.0, 90.0);
                GdipAddPathLine(path, w - r, h, r, h);
                GdipAddPathArc(path, 0.0, h - r * 2.0, r * 2.0, r * 2.0, 90.0, 90.0);
                GdipAddPathLine(path, 0.0, h - r, 0.0, r);
                GdipClosePathFigure(path);
                GdipFillPath(graphics, bg_brush as *mut GpBrush, path);
                GdipDeletePath(path);
            }
            GdipDeleteBrush(bg_brush as *mut GpBrush);
        }

        // --- 文字使用 GDI+ 绘制（自带正确 alpha，可支持纯黑文字，不再依赖 rgb 哨兵）---
        // 字体复用当前 DC 中已选中的兼容字体，保证字号/字体与旧实现一致。
        let old_font = SelectObject(mem_dc, hfont as _);
        // font_handle 已在测量阶段创建，复用同一句柄保证绘制度量一致
        if !font_handle.is_null() {
            let mut string_format: *mut GpStringFormat = ptr::null_mut();
            // StringFormatFlagsNoWrap：禁止自动换行，文字始终单行显示（对齐旧 GDI DT_SINGLELINE 行为）
            if GdipCreateStringFormat(StringFormatFlagsNoWrap as i32, 0, &mut string_format) == 0 && !string_format.is_null() {
                // 灰度反锯齿：避免 ClearType 彩色边缘在透明分层窗口产生色边
                GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAliasGridFit);
                GdipSetStringFormatLineAlign(string_format, StringAlignmentCenter);

                let gap = text_gap;
                let mut current_x: i32 = padding;

                for (i, item) in layout_items.iter().enumerate() {
                    if i > 0 {
                        current_x += item_gap;
                    }

                    // label：右对齐
                    if !item.label.is_empty() {
                        let mut brush_handle: *mut GpSolidFill = ptr::null_mut();
                        if GdipCreateSolidFill(gdi_to_argb(font_color), &mut brush_handle) == 0 && !brush_handle.is_null() {
                            let rect_f = RectF {
                                X: current_x as f32,
                                Y: 0.0,
                                Width: item.label_width as f32,
                                Height: dib_height as f32,
                            };
                            GdipSetStringFormatAlign(string_format, StringAlignmentFar);
                            let wide: Vec<u16> = item.label.encode_utf16().chain(std::iter::once(0)).collect();
                            GdipDrawString(
                                graphics,
                                wide.as_ptr(),
                                (wide.len() - 1) as i32,
                                font_handle,
                                &rect_f,
                                string_format,
                                brush_handle as *mut GpBrush,
                            );
                            GdipDeleteBrush(brush_handle as *mut GpBrush);
                        }
                    }

                    // value：左对齐
                    let value_x = if item.label.is_empty() {
                        current_x
                    } else {
                        current_x + item.label_width + gap
                    };

                    let mut color: u32 = font_color;
                    if let Some(custom_color) = item.custom_color {
                        color = custom_color;
                    } else if !item.label.is_empty() && !item.value.contains("--")
                        && (item.value.contains("°C") || item.value.contains('%') || item.value.bytes().all(|b| b.is_ascii_digit()))
                    {
                        let mut num_str = String::new();
                        for ch in item.value.chars() {
                            if ch.is_ascii_digit() || ch == '.' {
                                num_str.push(ch);
                            } else if !num_str.is_empty() {
                                break;
                            }
                        }
                        if !num_str.is_empty() {
                            if let Ok(nf) = num_str.parse::<f32>() {
                                let nv = nf as i32;
                                if nv < 50 {
                                    color = 0x0000FF00;
                                } else if nv < 80 {
                                    color = 0x0000FFFF;
                                } else {
                                    color = 0x000000FF;
                                }
                            }
                        }
                    }

                    let mut brush_handle: *mut GpSolidFill = ptr::null_mut();
                    if GdipCreateSolidFill(gdi_to_argb(color), &mut brush_handle) == 0 && !brush_handle.is_null() {
                        let rect_f = RectF {
                            X: value_x as f32,
                            Y: 0.0,
                            Width: item.value_width as f32,
                            Height: dib_height as f32,
                        };
                        GdipSetStringFormatAlign(string_format, StringAlignmentNear);
                        let wide: Vec<u16> = item.value.encode_utf16().chain(std::iter::once(0)).collect();
                        GdipDrawString(
                            graphics,
                            wide.as_ptr(),
                            (wide.len() - 1) as i32,
                            font_handle,
                            &rect_f,
                            string_format,
                            brush_handle as *mut GpBrush,
                        );
                        GdipDeleteBrush(brush_handle as *mut GpBrush);
                    }

                    current_x += item.total_width;
                }
                GdipDeleteStringFormat(string_format);
            }
        }
        SelectObject(mem_dc, old_font);

        GdipDeleteGraphics(graphics);
        if !font_handle.is_null() {
            GdipDeleteFont(font_handle);
        }

        // 对半透明像素做 alpha 逆预乘：GDI+ 按预乘 alpha 写入，
        // 而 UpdateLayeredWindow(AC_SRC_ALPHA) 需要直线 alpha，否则半透明边缘会偏暗、发黑。
        // alpha==0（透明/圆角外）与 alpha==255（纯不透明）保持不动。
        if !bits.is_null() {
            let pixels = std::slice::from_raw_parts_mut(
                bits as *mut u32,
                (dib_width * dib_height) as usize,
            );
            for pixel in pixels.iter_mut() {
                let v = *pixel;
                let a = (v >> 24) & 0xFF;
                if a > 0 && a < 255 {
                    let ar = a as u32;
                    let r = ((v >> 16) & 0xFF) as u32 * 255 / ar;
                    let g = ((v >> 8) & 0xFF) as u32 * 255 / ar;
                    let b = (v & 0xFF) as u32 * 255 / ar;
                    *pixel = (a as u32) << 24 | (r.clamp(0, 255) << 16) | (g.clamp(0, 255) << 8) | b.clamp(0, 255);
                }
            }
        }

        // --- Position and composite via UpdateLayeredWindow ---
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let default_x = (screen_width - dib_width) / 2;
        let use_x = settings.position_x.unwrap_or(default_x);
        let use_y = settings.position_y.unwrap_or(4);

        let ppt_dst = POINT { x: use_x, y: use_y };
        let psize = SIZE { cx: dib_width, cy: dib_height };
        let ppt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        UpdateLayeredWindow(
            hwnd,
            ptr::null_mut(),
            &ppt_dst,
            &psize,
            mem_dc,
            &ppt_src,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(mem_dc, old_bmp);
        DeleteObject(hbitmap as HGDIOBJ);
        DeleteDC(mem_dc);
        DeleteObject(hfont as _);
    }

    pub unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT {
                    hdc: ptr::null_mut(),
                    fErase: 0,
                    rcPaint: RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    fRestore: 0,
                    fIncUpdate: 0,
                    rgbReserved: [0u8; 32],
                };
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                0
            }
            WM_TIMER => {
                // 定时器 0.5s 刷新
                SetTimer(hwnd, 1, 500, None);
                let data = super::collect_hardware_data();
                *super::CURRENT_HARDWARE_DATA.lock().unwrap() = Some(data.clone());
                let settings = super::get_or_init_settings();
                if settings.style == "dynamic_island" {
                    draw_overlay_content_dynamic_island(hwnd, &settings, &data);
                } else {
                    draw_overlay_content(hwnd, &settings, &data);
                }
                0
            }
            WM_NCHITTEST => {
                // 拖动模式下返回 HTCAPTION 允许拖动
                if super::DRAG_MODE.load(std::sync::atomic::Ordering::SeqCst) {
                    HTCAPTION as LRESULT
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_EXITSIZEMOVE => {
                // 拖动结束后只保存位置，不退出拖动模式
                // 退出由前端按钮控制，避免样式切换导致位置重置
                if super::DRAG_MODE.load(std::sync::atomic::Ordering::SeqCst) {
                    // 获取当前窗口位置
                    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                    GetWindowRect(hwnd, &mut rect);

                    // 保存位置到设置
                    {
                        let mut settings_lock = super::CURRENT_SETTINGS.lock().unwrap();
                        if let Some(ref mut settings) = *settings_lock {
                            settings.position_x = Some(rect.left);
                            settings.position_y = Some(rect.top);
                        }
                    }

                    // 标记位置已变更
                    super::POSITION_CHANGED.store(true, std::sync::atomic::Ordering::SeqCst);
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

// 设置拖动模式
pub fn set_drag_mode(enabled: bool) {
    DRAG_MODE.store(enabled, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    unsafe {
        let hwnd = OVERLAY_HANDLE.load(Ordering::SeqCst);
        if !hwnd.is_null() {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;

            // 获取当前窗口样式
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

            if enabled {
                // 进入拖动模式：移除 WS_EX_TRANSPARENT
                let new_style = ex_style & !WS_EX_TRANSPARENT;
                SetWindowLongW(hwnd, GWL_EXSTYLE, new_style as i32);
            } else {
                // 退出拖动模式：恢复 WS_EX_TRANSPARENT
                let new_style = ex_style | WS_EX_TRANSPARENT;
                SetWindowLongW(hwnd, GWL_EXSTYLE, new_style as i32);
            }

            // 刷新窗口样式
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start_overlay(settings: OverlaySettings) -> Result<OverlayResult, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    if OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Ok(OverlayResult {
            success: true,
            message: "悬浮框已处于启用状态".to_string(),
        });
    }

    OVERLAY_ACTIVE.store(true, Ordering::SeqCst);

    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        *settings_lock = Some(settings.clone());
    }

    thread::spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

        crate::game_ping::start_ping_thread();
        crate::game_fps::start_fps_monitor();
        // 启动网络时间同步（悬浮框时间使用网络标准时间）
        start_net_time_sync();

        let com_initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        if !com_initialized {
            log::warn!("悬浮框线程初始化 COM 失败，网易云歌词功能可能不可用");
        }

        unsafe {
            match win32::create_overlay_window(&settings) {
                std::result::Result::Ok(hwnd) => {
                    OVERLAY_HANDLE.store(hwnd, Ordering::SeqCst);
                    crate::game_fps::set_overlay_hwnd(hwnd as u64);

                    if settings.style == "dynamic_island" {
                        win32::draw_overlay_content_dynamic_island(hwnd, &settings, &CURRENT_HARDWARE_DATA.lock().unwrap().clone().unwrap_or_default());
                    } else {
                        win32::draw_overlay_content(hwnd, &settings, &CURRENT_HARDWARE_DATA.lock().unwrap().clone().unwrap_or_default());
                    }

                    SetTimer(hwnd, 1, 500, None);
                    win32::install_topmost_guard();

                    let mut msg: MSG = std::mem::zeroed();
                    while OVERLAY_ACTIVE.load(Ordering::SeqCst) {
                        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                            if msg.message == WM_QUIT {
                                break;
                            }
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }

                        if !OVERLAY_ACTIVE.load(Ordering::SeqCst) {
                            break;
                        }

                        thread::sleep(Duration::from_millis(50));
                    }

                    win32::uninstall_topmost_guard();
                    win32::destroy_overlay_window(hwnd);
                    crate::game_fps::clear_overlay_hwnd();
                    OVERLAY_HANDLE.store(std::ptr::null_mut(), Ordering::SeqCst);
                }
                std::result::Result::Err(e) => {
                    log::error!("创建悬浮框窗口失败: {}", e);
                    OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
                }
            }
        }

        if com_initialized {
            unsafe {
                CoUninitialize();
            }
        }
    });

    Ok(OverlayResult {
        success: true,
        message: "悬浮框已启动".to_string(),
    })
}

#[cfg(target_os = "windows")]
pub fn stop_overlay() -> Result<OverlayResult, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW;
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE;

    if !OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Ok(OverlayResult {
            success: true,
            message: "悬浮框已处于关闭状态".to_string(),
        });
    }

    OVERLAY_ACTIVE.store(false, Ordering::SeqCst);

    crate::game_ping::stop_ping_thread();
    crate::game_fps::stop_fps_monitor();

    unsafe {
        let hwnd = OVERLAY_HANDLE.load(Ordering::SeqCst);
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }

    Ok(OverlayResult {
        success: true,
        message: "悬浮框已关闭".to_string(),
    })
}

#[cfg(not(target_os = "windows"))]
pub fn start_overlay(_settings: OverlaySettings) -> Result<OverlayResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn stop_overlay() -> Result<OverlayResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

/// Check if Win32 overlay is active.
pub fn is_overlay_active() -> bool {
    OVERLAY_ACTIVE.load(Ordering::SeqCst)
}

/// Toggle overlay on/off. Used by global hotkey.
pub fn toggle_overlay(app_handle: &tauri::AppHandle) -> Result<OverlayResult, String> {
    // 确保设置已从持久化存储加载，避免快捷键触发时使用默认设置
    try_load_persisted_settings(app_handle);

    // 如果当前样式是竖排面板，使用 Tauri 窗口方案
    let settings = get_or_init_settings();
    if settings.style == "vertical_panel" {
        return crate::vertical_overlay::toggle_vertical_overlay(app_handle);
    }

    // 如果竖排悬浮框正在运行，先停止它
    if crate::vertical_overlay::is_vertical_overlay_active() {
        let handle = app_handle.clone();
        let _ = tauri::async_runtime::block_on(async {
            crate::vertical_overlay::stop_vertical_overlay(handle).await
        });
    }

    let result = if OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        stop_overlay()
    } else {
        let settings = get_or_init_settings();
        start_overlay(settings)
    };

    if result.is_ok() {
        let _ = app_handle.emit("overlay-status-changed", ());
    }

    result
}

#[tauri::command]
pub async fn start_overlay_panel(
    app_handle: tauri::AppHandle,
    settings: Option<OverlaySettings>,
) -> Result<OverlayResult, String> {
    let settings = settings.unwrap_or_else(get_or_init_settings);

    if settings.style == "vertical_panel" {
        return crate::vertical_overlay::start_vertical_overlay(app_handle, Some(settings)).await;
    }

    // 如果竖排悬浮框正在运行，先停止它
    if crate::vertical_overlay::is_vertical_overlay_active() {
        let handle = app_handle.clone();
        let _ = crate::vertical_overlay::stop_vertical_overlay(handle).await;
        std::thread::sleep(Duration::from_millis(200));
    }

    start_overlay(settings)
}

#[tauri::command]
pub async fn stop_overlay_panel(app_handle: tauri::AppHandle) -> Result<OverlayResult, String> {
    // 停止 Win32 overlay
    let win32_result = if OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        stop_overlay()
    } else {
        Ok(OverlayResult {
            success: true,
            message: "Win32 悬浮框未运行".to_string(),
        })
    };

    // 停止竖排悬浮框
    let vertical_result = crate::vertical_overlay::stop_vertical_overlay(app_handle).await;

    // 任一成功即视为成功
    if win32_result.is_ok() || vertical_result.is_ok() {
        Ok(OverlayResult {
            success: true,
            message: "悬浮框已关闭".to_string(),
        })
    } else {
        Err("悬浮框关闭失败".to_string())
    }
}

#[tauri::command]
pub async fn toggle_overlay_panel(app_handle: tauri::AppHandle) -> Result<OverlayResult, String> {
    toggle_overlay(&app_handle)
}

#[tauri::command]
pub async fn get_overlay_panel_status() -> Result<bool, String> {
    Ok(OVERLAY_ACTIVE.load(Ordering::SeqCst) || crate::vertical_overlay::is_vertical_overlay_active())
}

#[tauri::command]
pub async fn set_active_gpu_index(index: usize) -> Result<(), String> {
    ACTIVE_GPU_INDEX.store(index, std::sync::atomic::Ordering::Relaxed);
    log::info!("GPU 切换至索引: {}", index);
    Ok(())
}

#[tauri::command]
pub async fn get_overlay_hardware_data() -> Result<OverlayHardwareData, String> {
    let data = CURRENT_HARDWARE_DATA
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    Ok(data)
}

#[tauri::command]
pub async fn update_overlay_settings(app_handle: tauri::AppHandle, settings: OverlaySettings) -> Result<OverlayResult, String> {
    let mut settings = settings;
    // 透明度下限钳制为 1：不允许保存为 0（0 会导致背景完全透明，影响文字可读性）
    settings.opacity = settings.opacity.max(1);
    let (old_style, old_font) = {
        let lock = CURRENT_SETTINGS.lock().unwrap();
        let s = lock.as_ref();
        (s.map(|s| s.style.clone()), s.map(|s| s.font.clone()))
    };
    let new_style = settings.style.clone();
    let new_font = settings.font.clone();

    let _old_was_vertical = old_style.as_deref() == Some("vertical_panel");
    let new_is_vertical = new_style == "vertical_panel";

    // 保存新设置；但保留当前位置字段（position_x/y、vertical_position_x/y）：
    // 设置页保存的 settings 可能来自旧缓存（不含拖动后保存的最新位置），整体替换会把刚保存的位置覆盖回旧值/空值。
    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        let preserved = settings_lock.as_ref().map(|cur| {
            (
                cur.position_x,
                cur.position_y,
                cur.vertical_position_x,
                cur.vertical_position_y,
            )
        });
        if let Some((px, py, vx, vy)) = preserved {
            if settings.position_x.is_none() {
                settings.position_x = px;
            }
            if settings.position_y.is_none() {
                settings.position_y = py;
            }
            if settings.vertical_position_x.is_none() {
                settings.vertical_position_x = vx;
            }
            if settings.vertical_position_y.is_none() {
                settings.vertical_position_y = vy;
            }
        }
        *settings_lock = Some(settings.clone());
    }

    // 检查是否有 overlay 处于活跃状态
    let win32_active = OVERLAY_ACTIVE.load(Ordering::SeqCst);
    let vertical_active = crate::vertical_overlay::is_vertical_overlay_active();
    let any_active = win32_active || vertical_active;

    // 如果有任何悬浮框处于活跃状态
    if any_active {
        let style_changed = old_style.as_deref() != Some(&new_style);
        let font_changed = old_font.as_deref() != Some(&new_font);

        if style_changed || font_changed {
            // 停止当前的 overlay
            if win32_active {
                stop_overlay()?;
                std::thread::sleep(Duration::from_millis(200));
            }
            if vertical_active {
                let _ = crate::vertical_overlay::stop_vertical_overlay(app_handle.clone()).await;
                std::thread::sleep(Duration::from_millis(200));
            }

            // 如果有活跃的 overlay，启动新样式的 overlay
            if any_active {
                if new_is_vertical {
                    let _ = crate::vertical_overlay::start_vertical_overlay(app_handle, Some(settings)).await;
                } else {
                    start_overlay(settings)?;
                    let _ = app_handle.emit("overlay-status-changed", ());
                }
            }
        } else if new_is_vertical {
            // 竖排面板且样式未变，仅推送设置更新
            let _ = app_handle.emit("vertical-overlay-settings", &settings);
        } else {
            // Win32 overlay 且样式未变，直接重绘
            #[cfg(target_os = "windows")]
            unsafe {
                let hwnd = OVERLAY_HANDLE.load(Ordering::SeqCst);
                if !hwnd.is_null() {
                    let data = CURRENT_HARDWARE_DATA.lock().unwrap().clone().unwrap_or_default();
                    let current_settings = CURRENT_SETTINGS.lock().unwrap().clone().unwrap_or_default();
                    if new_style == "dynamic_island" {
                        win32::draw_overlay_content_dynamic_island(hwnd, &current_settings, &data);
                    } else {
                        win32::draw_overlay_content(hwnd, &current_settings, &data);
                    }
                }
            }
        }
    }

    Ok(OverlayResult {
        success: true,
        message: "设置已更新".to_string(),
    })
}

#[tauri::command]
pub async fn set_overlay_drag_mode(enabled: bool) -> Result<OverlayResult, String> {
    if !OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Err("悬浮框未启用".to_string());
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        use windows_sys::Win32::Foundation::RECT;
        
        let hwnd = OVERLAY_HANDLE.load(Ordering::SeqCst);
        if hwnd.is_null() {
            return Err("悬浮框窗口不存在".to_string());
        }

        if !enabled {
            // 退出拖动模式时，先保存当前位置
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetWindowRect(hwnd, &mut rect);
            
            {
                let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
                if let Some(ref mut settings) = *settings_lock {
                    settings.position_x = Some(rect.left);
                    settings.position_y = Some(rect.top);
                }
            }
        }

        // 切换拖动模式
        set_drag_mode(enabled);

        if !enabled {
            // 退出拖动模式后，恢复窗口到保存的位置
            let (saved_x, saved_y) = {
                let settings_lock = CURRENT_SETTINGS.lock().unwrap();
                if let Some(ref settings) = *settings_lock {
                    (settings.position_x, settings.position_y)
                } else {
                    (None, None)
                }
            };
            
            // 获取当前位置
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetWindowRect(hwnd, &mut rect);
            
            // 如果位置发生变化，恢复到保存的位置
            if let (Some(sx), Some(sy)) = (saved_x, saved_y) {
                if rect.left != sx || rect.top != sy {
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        sx,
                        sy,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
            
            POSITION_CHANGED.store(false, Ordering::SeqCst);
        }
    }

    let message = if enabled { 
        "已进入拖动模式".to_string()
    } else {
        "已退出拖动模式".to_string()
    };

    Ok(OverlayResult {
        success: true,
        message,
    })
}

#[tauri::command]
pub async fn get_overlay_current_settings() -> Result<OverlaySettings, String> {
    let mut settings = CURRENT_SETTINGS.lock().unwrap().clone().unwrap_or_default();
    // 合并默认项：新增的显示项自动追加到已有设置中
    let defaults = default_display_items();
    let mut merged = false;
    for default_item in &defaults {
        if !settings.display_items.iter().any(|i| i.id == default_item.id) {
            settings.display_items.push(default_item.clone());
            merged = true;
        }
    }
    drop(defaults); // 释放借用
    if merged {
        let mut lock = CURRENT_SETTINGS.lock().unwrap();
        *lock = Some(settings.clone());
    }
    Ok(settings)
}

#[tauri::command]
pub async fn check_drag_mode_status() -> Result<bool, String> {
    // 返回当前拖动模式状态
    Ok(DRAG_MODE.load(Ordering::SeqCst))
}

#[tauri::command]
pub async fn reset_overlay_position() -> Result<OverlayResult, String> {
    if !OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Err("悬浮框未启用".to_string());
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        let hwnd = OVERLAY_HANDLE.load(Ordering::SeqCst);
        if hwnd.is_null() {
            return Err("悬浮框窗口不存在".to_string());
        }

        // 清除已保存的位置，恢复默认居中
        {
            let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
            if let Some(ref mut settings) = *settings_lock {
                settings.position_x = None;
                settings.position_y = None;
            }
        }

        // 获取当前窗口大小
        let mut rect = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        let win_w = rect.right - rect.left;
        let win_h = rect.bottom - rect.top;

        // 计算居中位置
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let new_x = (screen_width - win_w) / 2;
        let new_y = (screen_height - win_h) / 2;

        // 移动窗口到居中位置
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            new_x,
            new_y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }

    Ok(OverlayResult {
        success: true,
        message: "位置已重置为默认".to_string(),
    })
}

pub fn stop_hardware_poller() {
    BACKGROUND_POLLER_ACTIVE.store(false, Ordering::SeqCst);
}

pub fn start_hardware_poller() {
    if BACKGROUND_POLLER_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    BACKGROUND_POLLER_ACTIVE.store(true, Ordering::SeqCst);
    thread::spawn(|| {
        while BACKGROUND_POLLER_ACTIVE.load(Ordering::SeqCst) {
            let data = collect_hardware_data();
            *CURRENT_HARDWARE_DATA.lock().unwrap() = Some(data.clone());

            // 推送到硬件报告记录器（timestamp / elapsed_sec 由 push_snapshot 内部填充）
            crate::hardware_report::push_snapshot(crate::hardware_report::HardwareSnapshot {
                timestamp: String::new(),
                elapsed_sec: 0,
                cpu_usage: data.cpu_usage.map(|v| v as f64),
                cpu_temp: data.cpu_temp,
                cpu_clock: data.cpu_clock.map(|v| v as f64),
                cpu_voltage: data.cpu_voltage,
                cpu_power: data.cpu_power,
                cpu_fan_speed: data.cpu_fan_speed.map(|v| v as f64),
                gpu_usage: data.gpu_usage.map(|v| v as f64),
                gpu_temp: data.gpu_temp,
                gpu_clock: data.gpu_clock.map(|v| v as f64),
                gpu_voltage: data.gpu_voltage,
                gpu_power: data.gpu_power.map(|v| v as f64),
                gpu_fan_speed: data.gpu_fan_speed.map(|v| v as f64),
                gpu_vram_used: data.gpu_vram_used.map(|v| v as f64),
                gpu_vram_total: data.gpu_vram_total.map(|v| v as f64),
                gpu_memory_clock: data.gpu_memory_clock.map(|v| v as f64),
                memory_usage: data.memory_usage,
                ssd_temp: data.ssd_temp,
            });

            thread::sleep(Duration::from_millis(1000));
        }
    });
}

pub fn cleanup() {
    stop_hardware_poller();
    if OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        let _ = stop_overlay();
    }
    crate::game_ping::cleanup();
    crate::game_fps::cleanup();
    #[cfg(target_os = "windows")]
    unsafe {
        win32::shutdown_gdiplus();
    }
}

