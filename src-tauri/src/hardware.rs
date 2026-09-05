use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;
use sysinfo::System;

use crate::wmi_query;

#[derive(Error, Debug)]
pub enum HardwareError {
    #[error("JSON解析失败: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("NVML错误: {0}")]
    NvmlError(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub name: String,
    pub manufacturer: String,
    pub cores: u32,
    pub threads: u32,
    pub max_clock_speed: u32,
    pub l2_cache_size: u32,
    pub l3_cache_size: u32,
    pub load_percentage: Option<u16>,
    pub architecture: String,
    pub socket: String,
    pub l2_cache_speed: Option<u32>,
    pub l3_cache_speed: Option<u32>,
    pub current_clock_speed: Option<u32>,
    pub ext_clock: Option<u32>,
    pub processor_id: String,
    pub family: u32,
    pub stepping: String,
    pub revision: String,
    pub enabled_cores: Option<u32>,
    pub voltage_caps: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum GpuVendor {
    NVIDIA,
    AMD,
    Intel,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: GpuVendor,
    /// 显存总量（GB）；获取不到时为 None（如核显），前端将不显示显存项
    pub memory_gb: Option<f64>,
    pub driver_version: String,
    pub temperature: Option<f64>,
    pub usage: Option<u32>,
    pub video_processor: String,
    pub adapter_compatibility: String,
    pub driver_date: String,
    pub installed_drivers: String,
    pub video_mode: String,
    pub resolution_width: Option<u32>,
    pub resolution_height: Option<u32>,
    pub refresh_rate: Option<u32>,
    pub device_id: String,
    pub pnp_device_id: String,
    pub status: String,
    pub inf_filename: String,
    pub video_architecture: Option<String>,
    pub video_memory_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryInfo {
    pub manufacturer: String,
    pub part_number: String,
    pub capacity_gb: f64,
    pub speed_mhz: u32,
    pub bank_label: String,
    pub form_factor: String,
    pub memory_type: String,
    pub configured_clock_speed: Option<u32>,
    pub configured_voltage: Option<u32>,
    pub data_width: Option<u32>,
    pub total_width: Option<u32>,
    pub serial_number: String,
    pub type_detail: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoundCardInfo {
    pub name: String,
    pub manufacturer: String,
    pub status: String,
    pub device_id: String,
    pub pnp_device_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkCardInfo {
    pub name: String,
    pub manufacturer: String,
    pub adapter_type: String,
    pub mac_address: String,
    pub speed_mbps: u64,
    pub connection_name: String,
    pub service_name: String,
    pub index: u32,
    pub max_speed: Option<u64>,
    pub guid: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MotherboardInfo {
    pub product: String,
    pub manufacturer: String,
    pub serial_number: String,
    pub version: String,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_release_date: String,
    pub system_manufacturer: String,
    pub system_model: String,
    pub system_type: String,
    pub chassis_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskDetailInfo {
    pub model: String,
    pub size_gb: f64,
    pub interface_type: String,
    pub serial_number: String,
    pub firmware_revision: String,
    pub media_type: String,
    pub bytes_per_sector: Option<u32>,
    pub partitions: u32,
    pub status: String,
    pub is_ssd: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MonitorInfo {
    pub name: String,
    pub manufacturer: String,
    pub screen_width: Option<u32>,
    pub screen_height: Option<u32>,
    pub refresh_rate: Option<u32>,
    pub pnp_device_id: String,
    pub status: String,
    pub availability: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareInfo {
    pub cpu: CpuInfo,
    pub gpu: Vec<GpuInfo>,
    pub memory: Vec<MemoryInfo>,
    pub motherboard: MotherboardInfo,
    pub disk: Vec<DiskDetailInfo>,
    pub sound_card: Vec<SoundCardInfo>,
    pub network_card: Vec<NetworkCardInfo>,
    pub monitor: Vec<MonitorInfo>,
}



/// Check if a monitor name is a generic/placeholder (any language variant)
fn is_generic_monitor_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("generic")
        || lower.contains("即插即用")
        || lower.contains("通用")
        || lower.contains("pnp")
        || lower.contains("standard monitor")
        || lower.contains("digital display")
        || lower.contains("analog display")
}

/// 从 PNPDeviceID（如 "DISPLAY\DELA409\5&1f7..." 或 "MONITOR\DELA0A1A\..."）
/// 中提取 PNP ID 部分（如 "DELA409"）。用于与注册表 EDID 键精确匹配型号。
fn extract_pnpid(pnp_device_id: &str) -> Option<String> {
    let s = pnp_device_id.trim();
    // 取第一个反斜杠后的那段（DISPLAY 或 MONITOR 之后），再按反斜杠截断
    let after_prefix = s.split('\\').nth(1)?;
    let pnpid = after_prefix.split('\\').next().unwrap_or(after_prefix);
    if pnpid.is_empty() {
        None
    } else {
        Some(pnpid.to_uppercase())
    }
}

/// 通过 EnumDisplayDevicesW 获取指定显示设备名（如 "\\.\DISPLAY1"）对应的
/// PNP ID，用于在回退枚举路径中按设备 ID 匹配 EDID 型号。
#[cfg(target_os = "windows")]
fn get_pnpid_for_device(device_name: &str) -> Option<String> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};
    use std::mem;
    let device_name_wide: Vec<u16> = device_name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut disp_device: DISPLAY_DEVICEW = unsafe { mem::zeroed() };
    disp_device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
    if unsafe { EnumDisplayDevicesW(device_name_wide.as_ptr(), 0, &mut disp_device, 0) } != 0 {
        let device_id = String::from_utf16_lossy(
            &disp_device.DeviceID[..disp_device.DeviceID.iter().position(|&c| c == 0).unwrap_or(disp_device.DeviceID.len())],
        );
        return extract_pnpid(&device_id);
    }
    None
}

/// 当 WMI 查询不到显示器时，使用 EnumDisplaySettingsW 枚举显示器作为回退。
/// 注：Win32_DesktopMonitor 在现代 Windows 上已废弃，经常返回空。
fn fallback_enumerate_monitors() -> Vec<MonitorInfo> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};

    // 按 PNP ID 匹配型号，避免按下标对齐时顺序颠倒导致型号错配。
    let edid_map = crate::display_cache::get_edid_monitor_names_by_pnpid();
    let mut monitors = Vec::new();

    for i in 0..8 {
        let name = format!("\\\\.\\DISPLAY{}", i + 1);
        let tries = [name.as_str(), &format!("DISPLAY{}", i + 1)];
        let mut found = false;
        for dev_name in &tries {
            unsafe {
                let wide: Vec<u16> = dev_name.encode_utf16().chain(std::iter::once(0)).collect();
                let mut dm: DEVMODEW = std::mem::zeroed();
                dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
                if EnumDisplaySettingsW(wide.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) != 0 {
                    let w = dm.dmPelsWidth;
                    let h = dm.dmPelsHeight;
                    if w > 0 && h > 0 {
                        // 通过 EnumDisplayDevicesW 获取该显示设备的 PNP ID，再匹配 EDID 型号
                        let (mon_name, manufacturer) = match get_pnpid_for_device(&name)
                            .as_ref()
                            .and_then(|p| edid_map.get(p))
                            .filter(|n| !n.is_empty())
                        {
                            Some(edid_name) => (edid_name.clone(), "".to_string()),
                            None => (format!("DISPLAY{}", i + 1), "未知".to_string()),
                        };
                        monitors.push(MonitorInfo {
                            name: mon_name,
                            manufacturer,
                            screen_width: Some(w),
                            screen_height: Some(h),
                            refresh_rate: Some(dm.dmDisplayFrequency),
                            pnp_device_id: format!("DISPLAY{}", i + 1),
                            status: "OK".to_string(),
                            availability: Some(3),
                        });
                        found = true;
                        break;
                    }
                }
            }
        }
        if !found { break; }
    }

    log::info!("fallback_enumerate_monitors: 通过 EnumDisplaySettingsW 发现 {} 个显示器", monitors.len());
    monitors
}

// 静态硬件信息缓存（不会变化的部分）
#[derive(Debug, Clone)]
struct StaticHardwareInfo {
    cpu: CpuInfo,
    gpu_static: Vec<GpuStaticInfo>,
    motherboard: MotherboardInfo,
    memory: Vec<MemoryInfo>,
    disk: Vec<DiskDetailInfo>,
    sound_card: Vec<SoundCardInfo>,
    network_card: Vec<NetworkCardInfo>,
    monitor: Vec<MonitorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GpuStaticInfo {
    pub(crate) name: String,
    vendor: GpuVendor,
    memory_gb: Option<f64>,
    driver_version: String,
    video_processor: String,
    adapter_compatibility: String,
    driver_date: String,
    installed_drivers: String,
    video_mode: String,
    resolution_width: Option<u32>,
    resolution_height: Option<u32>,
    refresh_rate: Option<u32>,
    device_id: String,
    pub(crate) pnp_device_id: String,
    status: String,
    inf_filename: String,
    video_architecture: Option<String>,
    video_memory_type: Option<String>,
}

static STATIC_HARDWARE_CACHE: Mutex<Option<StaticHardwareInfo>> = Mutex::new(None);
static HARDWARE_INIT_LOCK: Mutex<()> = Mutex::new(());
static CPU_SYSTEM: Mutex<Option<System>> = Mutex::new(None);

fn detect_gpu_vendor(name: &str) -> GpuVendor {
    let name_lower = name.to_lowercase();
    if name_lower.contains("nvidia") || name_lower.contains("geforce") || 
       name_lower.contains("gtx") || name_lower.contains("rtx") {
        GpuVendor::NVIDIA
    } else if name_lower.contains("amd") || name_lower.contains("radeon") || 
              name_lower.contains("rx ") {
        GpuVendor::AMD
    } else if name_lower.contains("intel") {
        GpuVendor::Intel
    } else {
        GpuVendor::Unknown
    }
}

/// 虚拟显卡厂商 PCI ID 黑名单（VMware / VirtualBox / Hyper-V / QEMU 等）
const VIRTUAL_GPU_VENDORS: [&str; 6] = [
    "VEN_15AD", "VEN_80EE", "VEN_1414",
    "VEN_1234", "VEN_1AF4", "VEN_1B36",
];

/// 已知虚拟显示 / 远程串流产品名（MuMu / GameViewer / Parsec / 向日葵等）。
/// 作为结构化过滤（DeviceID 含 PCI\VEN_）之外的兜底。
const VIRTUAL_GPU_NAME_KEYWORDS: [&str; 13] = [
    "mu mu virtual display", "mumu virtual display", // MuMu 模拟器虚拟显示器
    "gameviewer virtual display", // 云游戏 / 远程串流虚拟显示器
    "parsec", "sunflower", "oray", "todesk", "rustdesk", // 远程串流 / 远控
    "virtual display", "remote display", "iddcx", "usb mobile monitor", "虚拟机",
];

/// 是否为物理 PCI 显卡：设备实例路径（DeviceID/PNPDeviceID）必须含 `PCI\VEN_`，
/// 且厂商不在虚拟显卡黑名单中。
/// 基础显示适配器、RDP 远程显示、SWD 软件设备、MuMu/GameViewer 等 IDD 虚拟显示器
/// 均为 `ROOT\...` / `SWD\...` 非 PCI 路径，由本判定直接过滤。
fn is_physical_pci_gpu(device_id: &str) -> bool {
    let upper = device_id.to_uppercase();
    upper.contains("PCI\\VEN_") && !VIRTUAL_GPU_VENDORS.iter().any(|v| upper.contains(v))
}

/// 名称黑名单兜底：命中已知虚拟显示产品名即视为虚拟显卡。
fn is_virtual_gpu_by_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    VIRTUAL_GPU_NAME_KEYWORDS.iter().any(|k| lower.contains(k))
}

fn get_nvidia_gpus_with_nvml() -> Result<Vec<GpuInfo>, HardwareError> {
    use nvml_wrapper::Nvml;

    let nvml = Nvml::init().map_err(|e| HardwareError::NvmlError(e.to_string()))?;
    let device_count = nvml
        .device_count()
        .map_err(|e| HardwareError::NvmlError(e.to_string()))?;

    let mut gpus = Vec::new();

    for i in 0..device_count {
        let device = nvml
            .device_by_index(i)
            .map_err(|e| HardwareError::NvmlError(e.to_string()))?;

        let name = device
            .name()
            .map_err(|e| HardwareError::NvmlError(e.to_string()))?;
        // 防御性过滤 NVIDIA 虚拟 GPU / 虚拟显示设备（NVML 下极少出现，无 PCI 实例路径可查）
        if is_virtual_gpu_by_name(&name) || name.to_lowercase().contains("vgpu") {
            log::debug!("跳过 NVIDIA 虚拟 GPU(NVML): {}", name);
            continue;
        }
        let memory_info = device
            .memory_info()
            .map_err(|e| HardwareError::NvmlError(e.to_string()))?;
        let memory_gb = memory_info.total as f64 / (1024.0 * 1024.0 * 1024.0);

        let temperature = device
            .temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            .ok()
            .map(|t| if t > 200 { t as f64 / 10.0 } else { t as f64 });
        let utilization = device.utilization_rates().ok();
        let usage = utilization.map(|u| u.gpu);

        let driver_version = nvml
            .sys_driver_version()
            .map_err(|e| HardwareError::NvmlError(e.to_string()))?;

        log::debug!(
            "NVIDIA GPU (NVML): {}, 显存: {:.1}GB, 温度: {:?}°C, 占用: {:?}%",
            name,
            memory_gb,
            temperature,
            usage
        );

        gpus.push(GpuInfo {
            name,
            vendor: GpuVendor::NVIDIA,
            memory_gb: Some(memory_gb),
            driver_version,
            temperature,
            usage,
            video_processor: String::new(),
            adapter_compatibility: "NVIDIA".to_string(),
            driver_date: String::new(),
            installed_drivers: String::new(),
            video_mode: String::new(),
            resolution_width: None,
            resolution_height: None,
            refresh_rate: None,
            device_id: String::new(),
            pnp_device_id: String::new(),
            status: String::new(),
            inf_filename: String::new(),
            video_architecture: None,
            video_memory_type: None,
        });
    }

    Ok(gpus)
}

/// 从 LHML (NexBoxMonitor) 管道获取 GPU 信息，支持所有厂商
fn get_gpus_from_lhml() -> Vec<GpuInfo> {
    let response = match crate::sensor::read_lhm_sensors() {
        Ok(r) => r,
        Err(e) => {
            // 传感器启动早期（LHML 初始化需数秒）未就绪是正常现象，降级为 debug 日志
            if e.contains("尚未就绪") {
                log::debug!("LHML GPU 查询跳过: {}", e);
            } else {
                log::warn!("LHML GPU 查询失败: {}", e);
            }
            // LHML（NexBoxMonitor）可能尚未就绪，等 200ms 后重试一次
            std::thread::sleep(std::time::Duration::from_millis(200));
            match crate::sensor::read_lhm_sensors() {
                Ok(r) => r,
                Err(e) => {
                    if e.contains("尚未就绪") {
                        log::debug!("LHML GPU 重试仍跳过: {}", e);
                    } else {
                        log::warn!("LHML GPU 重试仍失败: {}", e);
                    }
                    return Vec::new();
                }
            }
        }
    };

    // 按 (hardware_name, hardware_type) 分组，区分不同 GPU
    let mut gpu_groups: Vec<(String, String, Vec<&crate::sensor::SensorReading>)> = Vec::new();
    for sensor in &response.sensors {
        let hw_type = sensor.hardware_type.clone();
        if !hw_type.to_lowercase().starts_with("gpu") {
            continue;
        }
        let key = sensor.hardware.clone();
        if let Some(group) = gpu_groups.iter_mut().find(|(k, _, _)| k == &key) {
            group.2.push(sensor);
        } else {
            gpu_groups.push((key, hw_type, vec![sensor]));
        }
    }

    if gpu_groups.is_empty() {
        return Vec::new();
    }

    // 判断各 GPU 是否为核显（LHML 无法区分 AMD 核显/独显，仅通过硬件名辅助判断）
    let has_nvidia = gpu_groups.iter().any(|(_, t, _)| t.eq_ignore_ascii_case("GpuNvidia"));

    let mut gpus = Vec::new();
    for (name, hw_type, sensors) in &gpu_groups {
        // NVIDIA 独显存在时跳过 Intel 核显，但 AMD 核显保留显示
        // （LHML 统一标记为 GpuAmd，无法区分 APU 核显和独显）
        if has_nvidia && hw_type.eq_ignore_ascii_case("GpuIntel") {
            log::info!("跳过核显(LHML): 存在 NVIDIA 独显，忽略 GpuIntel");
            continue;
        }

        let vendor = if hw_type.eq_ignore_ascii_case("GpuNvidia") {
            GpuVendor::NVIDIA
        } else if hw_type.eq_ignore_ascii_case("GpuAmd") {
            GpuVendor::AMD
        } else if hw_type.eq_ignore_ascii_case("GpuIntel") {
            GpuVendor::Intel
        } else {
            detect_gpu_vendor(name)
        };

        // 显存总量 (SmallData "GPU Memory Total"，单位为 MB → GB)；LHML 无法提供的（如核显）为 None
        let memory_gb = sensors
            .iter()
            .find(|s| s.sensor_type == "SmallData" && s.name == "GPU Memory Total")
            .map(|s| s.value / 1024.0)
            .filter(|v| *v > 0.0);

        let temperature = ["GPU Core", "GPU", "Core", "GPU Temperature"]
            .iter()
            .find_map(|n| {
                sensors
                    .iter()
                    .find(|s| s.sensor_type == "Temperature" && s.name == *n)
            })
            .map(|s| s.value);

        // Intel 核显的占用传感器名为 "D3D 3D"（来自 D3D 引擎枚举），并非 "GPU Core"
        let usage = ["GPU Core", "D3D 3D", "GPU", "D3D Usage", "Core"]
            .iter()
            .find_map(|n| {
                sensors
                    .iter()
                    .find(|s| s.sensor_type == "Load" && s.name == *n)
            })
            .map(|s| s.value as u32);

        log::info!(
            "显卡(LHML): {}, 厂商: {:?}, 显存: {:?}GB, 温度: {:?}°C, 占用: {:?}%",
            name, vendor, memory_gb, temperature, usage
        );

        gpus.push(GpuInfo {
            name: name.clone(),
            vendor,
            memory_gb,
            driver_version: String::new(),
            temperature,
            usage,
            video_processor: String::new(),
            adapter_compatibility: String::new(),
            driver_date: String::new(),
            installed_drivers: String::new(),
            video_mode: String::new(),
            resolution_width: None,
            resolution_height: None,
            refresh_rate: None,
            device_id: String::new(),
            pnp_device_id: String::new(),
            status: String::new(),
            inf_filename: String::new(),
            video_architecture: None,
            video_memory_type: None,
        });
    }

    gpus
}

/// 格式化 WMI 驱动日期（"20240101000000.000000-000" → "2024-01-01"）
fn format_driver_date(d: &str) -> String {
    let digits: String = d
        .chars()
        .filter(|c| c.is_ascii_digit())
        .take(8)
        .collect();
    if digits.len() == 8 {
        format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8])
    } else {
        d.to_string()
    }
}

/// 通过 WMI Win32_VideoController 全量枚举显卡（独显 + 核显），提供完整静态信息。
///
/// 这是「列出所有显卡」的可靠来源 —— NVML 只枚举 NVIDIA，LHML 在存在 NVIDIA 独显时
/// 又会跳过 Intel 核显，因此静态列表必须用 WMI 全量枚举。
pub(crate) fn get_gpus_static_from_wmi() -> Vec<GpuStaticInfo> {
    use crate::wmi_query::{self, v_str, v_u32, v_u64};

    let rows = match wmi_query::wmi_query(
        "SELECT Name, VideoProcessor, AdapterCompatibility, AdapterRAM, DriverVersion, DriverDate, \
         InstalledDisplayDrivers, VideoModeDescription, CurrentHorizontalResolution, \
         CurrentVerticalResolution, CurrentRefreshRate, PNPDeviceID, DeviceID, Status, \
         VideoArchitecture, VideoMemoryType FROM Win32_VideoController",
    ) {
        Ok(rows) => rows,
        Err(e) => {
            log::warn!("WMI 枚举显卡失败: {}", e);
            return Vec::new();
        }
    };

    let mut gpus = Vec::new();
    for row in &rows {
        let name = row.get("Name").and_then(|v| v_str(v)).unwrap_or_default();
        // 排除 Microsoft 基础显示适配器（未安装驱动的占位设备）
        if name.is_empty() || name.to_lowercase().contains("basic display") {
            continue;
        }
        // 结构化过滤：物理显卡的 DeviceID 必须含 PCI\VEN_ 且厂商不在虚拟黑名单。
        // 排除基础显示适配器、RDP 远程显示、SWD 软件设备、虚拟化伪 PCI 显卡、
        // 以及 MuMu / GameViewer 等非 PCI 路径的 IDD 虚拟显示器。
        let device_id = row.get("DeviceID").and_then(|v| v_str(v)).unwrap_or_default();
        if !is_physical_pci_gpu(&device_id) {
            log::debug!("跳过非物理 PCI 显卡(WMI): {} ({})", name, device_id);
            continue;
        }
        // 已知虚拟显示产品名黑名单兜底（万一以 PCI 路径出现）
        if is_virtual_gpu_by_name(&name) {
            log::info!("跳过虚拟显示适配器(WMI): {}", name);
            continue;
        }
        // 排除状态异常设备（Error / Degraded）
        if let Some(status) = row.get("Status").and_then(|v| v_str(v)) {
            let s = status.to_lowercase();
            if s.contains("error") || s.contains("degraded") {
                log::info!("跳过状态异常的显卡: {} ({})", name, status);
                continue;
            }
        }

        let vendor = detect_gpu_vendor(&name);
        // AdapterRAM 单位为字节，超过 4GB 会溢出为 0，此处仅作兜底（动态源会补充真实显存）
        let memory_gb = row
            .get("AdapterRAM")
            .and_then(|v| v_u64(v))
            .map(|b| b as f64 / 1024.0 / 1024.0 / 1024.0)
            .filter(|v| *v > 0.0);
        let driver_date = row
            .get("DriverDate")
            .and_then(|v| v_str(v))
            .map(|d| format_driver_date(&d))
            .unwrap_or_default();

        gpus.push(GpuStaticInfo {
            name: name.clone(),
            vendor,
            memory_gb,
            driver_version: row
                .get("DriverVersion")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            video_processor: row
                .get("VideoProcessor")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            adapter_compatibility: row
                .get("AdapterCompatibility")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            driver_date,
            installed_drivers: row
                .get("InstalledDisplayDrivers")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            video_mode: row
                .get("VideoModeDescription")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            resolution_width: row.get("CurrentHorizontalResolution").and_then(|v| v_u32(v)),
            resolution_height: row.get("CurrentVerticalResolution").and_then(|v| v_u32(v)),
            refresh_rate: row.get("CurrentRefreshRate").and_then(|v| v_u32(v)),
            device_id: row.get("DeviceID").and_then(|v| v_str(v)).unwrap_or_default(),
            pnp_device_id: row
                .get("PNPDeviceID")
                .and_then(|v| v_str(v))
                .unwrap_or_default(),
            status: row.get("Status").and_then(|v| v_str(v)).unwrap_or_default(),
            inf_filename: row.get("InfName").and_then(|v| v_str(v)).unwrap_or_default(),
            video_architecture: None,
            video_memory_type: None,
        });
    }

    if gpus.is_empty() {
        log::warn!("WMI 未枚举到可用显卡");
    } else {
        let names: Vec<&str> = gpus.iter().map(|g| g.name.as_str()).collect();
        log::info!("WMI 枚举到 {} 张显卡: {:?}", gpus.len(), names);
    }
    gpus
}

fn get_gpu_info() -> Vec<GpuInfo> {
    // NVML（NVIDIA）+ LHML（AMD / Intel / 通用）合并，不再"NVML 命中即返回"。
    // 之前 NVML 有结果时直接 return，会导致 NVIDIA + AMD 组合（核显或双独显）下
    // AMD 显卡只在顶部状态卡出现（顶部走 LHML 全量分组，保留 AMD）、底部没有显卡卡。
    // 两者都提供真实显存，修复 AMD 显卡 >4GB 显存被 WMI AdapterRAM(Uint32) 溢出吞掉的问题。

    // 1. NVML 获取 NVIDIA 显卡（最佳方案：提供完整信息包括驱动版本；已过滤 vGPU）
    let nvml_gpus = match std::panic::catch_unwind(|| get_nvidia_gpus_with_nvml()) {
        Ok(Ok(gpus)) => gpus,
        Ok(Err(e)) => {
            log::debug!("NVML 查询失败，仅用 LHML: {}", e);
            Vec::new()
        }
        Err(_) => {
            log::warn!("NVML 检测崩溃，已跳过");
            Vec::new()
        }
    };

    // 2. LHML 获取 AMD / Intel / 其它显卡（内部已有"存在 NVIDIA 独显时跳过 Intel 核显"逻辑，并过滤虚拟显卡）
    let lhml_gpus: Vec<GpuInfo> = get_gpus_from_lhml()
        .into_iter()
        .filter(|g| !is_virtual_gpu_by_name(&g.name))
        .collect();

    // 3. 合并去重：NVIDIA 卡通常同时出现在两个来源（NVAPI 与 NVML 名称一致），
    //    按归一化名称去重，保留 NVML 条目（含驱动版本）。
    //    只对 NVML 部分去重：LHML 自身可能存在同名的多张卡（按总线号区分），不能互相去重。
    let mut merged = nvml_gpus;
    let nvml_count = merged.len();
    for g in lhml_gpus {
        let norm = normalize_gpu_name(&g.name);
        let dup = !norm.is_empty()
            && merged[..nvml_count]
                .iter()
                .any(|m| normalize_gpu_name(&m.name) == norm);
        if dup {
            log::debug!("显卡去重(LHML 与 NVML 同名): {}", g.name);
        } else {
            merged.push(g);
        }
    }
    merged
}

/// 将 WMI 的静态字段补充到动态 GPU 列表（NVML/LHML）上。
/// 动态 GPU 提供准确显存、温度、占用；WMI 提供驱动版本、核心架构、分辨率等静态字段。
/// 按名称双向匹配（与 `get_gpu_dynamic_info` 的匹配逻辑一致）。
fn merge_wmi_fields(dynamic: Vec<GpuInfo>, wmi: &[GpuStaticInfo]) -> Vec<GpuStaticInfo> {
    dynamic
        .into_iter()
        .map(|d| {
            let norm = normalize_gpu_name(&d.name);
            let wmi_match = if norm.is_empty() {
                None
            } else {
                wmi.iter()
                    .max_by_key(|w| {
                        let wnorm = normalize_gpu_name(&w.name);
                        if wnorm.is_empty() {
                            return 0usize;
                        }
                        if wnorm == norm {
                            usize::MAX
                        } else if norm.contains(&wnorm) || wnorm.contains(&norm) {
                            norm.len().min(wnorm.len())
                        } else {
                            0
                        }
                    })
                    .filter(|w| {
                        let wnorm = normalize_gpu_name(&w.name);
                        wnorm == norm || norm.contains(&wnorm) || wnorm.contains(&norm)
                    })
            };
            match wmi_match {
                Some(w) => GpuStaticInfo {
                    name: d.name,
                    vendor: d.vendor,
                    memory_gb: d.memory_gb,
                    driver_version: w.driver_version.clone(),
                    video_processor: w.video_processor.clone(),
                    adapter_compatibility: w.adapter_compatibility.clone(),
                    driver_date: w.driver_date.clone(),
                    installed_drivers: w.installed_drivers.clone(),
                    video_mode: w.video_mode.clone(),
                    resolution_width: w.resolution_width,
                    resolution_height: w.resolution_height,
                    refresh_rate: w.refresh_rate,
                    device_id: w.device_id.clone(),
                    pnp_device_id: w.pnp_device_id.clone(),
                    status: w.status.clone(),
                    inf_filename: w.inf_filename.clone(),
                    video_architecture: w.video_architecture.clone(),
                    video_memory_type: w.video_memory_type.clone(),
                },
                None => GpuStaticInfo {
                    name: d.name,
                    vendor: d.vendor,
                    memory_gb: d.memory_gb,
                    driver_version: d.driver_version,
                    video_processor: d.video_processor,
                    adapter_compatibility: d.adapter_compatibility,
                    driver_date: d.driver_date,
                    installed_drivers: d.installed_drivers,
                    video_mode: d.video_mode,
                    resolution_width: d.resolution_width,
                    resolution_height: d.resolution_height,
                    refresh_rate: d.refresh_rate,
                    device_id: d.device_id,
                    pnp_device_id: d.pnp_device_id,
                    status: d.status,
                    inf_filename: d.inf_filename,
                    video_architecture: d.video_architecture,
                    video_memory_type: d.video_memory_type,
                },
            }
        })
        .collect()
}

/// 归一化 GPU 名称，用于多数据源按名称匹配
fn normalize_gpu_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 根据显卡名称判断是否为核显（仅用于显存兜底策略，不展示给用户）
fn is_integrated_gpu_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.contains("核显") {
        return true;
    }
    // Intel 核显：UHD Graphics / HD Graphics / Iris，排除 Intel Arc 独显
    if lower.contains("uhd") || lower.contains("hd graphics") || lower.contains("iris") {
        return true;
    }
    if lower.contains("intel") && lower.contains("graphics") && !lower.contains("arc") {
        return true;
    }
    // AMD APU 核显：Radeon 系列中，含 Graphics 或 Vega 的是核显（APU），
    // 仅 RX 系列是独显。注意 Vega 在 APU 上是核显（如 Vega 8 Graphics），
    // 680M/780M 等带数字的也是 APU 核显——切勿把数字/Vega 当作独显特征。
    if lower.contains("radeon") {
        let is_rx_discrete = lower.contains("rx");
        if !is_rx_discrete && (lower.contains("graphics") || lower.contains("vega")) {
            return true;
        }
    }
    false
}

/// 获取 GPU 动态数据：(温度, 占用, 显存)。
/// 显存优先取 NVML（NVIDIA）/ LHML（AMD 等独显）的真实值，来源返回 None 则无显存。
fn get_gpu_dynamic_info(gpu_static: &[GpuStaticInfo]) -> Vec<(Option<f64>, Option<u32>, Option<f64>)> {
    // 收集各数据源的动态读数：(归一化名称, 温度, 占用, 显存)
    // NVIDIA → NVML；AMD / Intel → LHML
    let mut dynamic: Vec<(String, Option<f64>, Option<u32>, Option<f64>)> = Vec::new();

    // 1. NVIDIA：NVML
    let nvml_result = std::panic::catch_unwind(|| get_nvidia_gpus_with_nvml());
    if let Ok(Ok(gpus)) = nvml_result {
        for g in &gpus {
            dynamic.push((
                normalize_gpu_name(&g.name),
                g.temperature,
                g.usage,
                g.memory_gb,
            ));
        }
    }

    // 2. AMD / Intel / 通用：LHML（LibreHardwareMonitor）
    let lhml_result = std::panic::catch_unwind(|| get_gpus_from_lhml());
    if let Ok(gpus) = lhml_result {
        for g in &gpus {
            dynamic.push((
                normalize_gpu_name(&g.name),
                g.temperature,
                g.usage,
                g.memory_gb,
            ));
        }
    }

    // 按名称双向包含匹配，将动态读数对到静态 GPU 上
    gpu_static
        .iter()
        .map(|gs| {
            let norm = normalize_gpu_name(&gs.name);
            if norm.is_empty() {
                return (None, None, None);
            }
            let mut best: Option<(Option<f64>, Option<u32>, Option<f64>)> = None;
            let mut best_score = 0usize;
            for (dname, temp, usage, mem) in &dynamic {
                let score = if dname == &norm {
                    usize::MAX
                } else if dname.is_empty() {
                    0
                } else if norm.contains(dname.as_str()) || dname.contains(&norm) {
                    norm.len().min(dname.len())
                } else {
                    0
                };
                if score > best_score {
                    best_score = score;
                    best = Some((*temp, *usage, *mem));
                }
            }
            best.unwrap_or((None, None, None))
        })
        .collect()
}

// 获取CPU的动态数据（占用）- 使用 sysinfo 库
pub(crate) fn get_cpu_dynamic_info() -> Option<u16> {
    use sysinfo::CpuRefreshKind;
    use std::thread;
    use std::time::Duration;
    
    let mut cpu_system = CPU_SYSTEM.lock().unwrap();
    
    if cpu_system.is_none() {
        let mut sys = System::new();
        // 第一次刷新：初始化
        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        // 短暂等待，让 sysinfo 有时间采集第一个样本
        thread::sleep(Duration::from_millis(50));
        // 第二次刷新：获取准确的 CPU 使用率
        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        *cpu_system = Some(sys);
    } else {
        // 正常情况下只需要刷新一次
        let sys = cpu_system.as_mut().unwrap();
        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
    }
    
    let sys = cpu_system.as_ref().unwrap();
    let cpus = sys.cpus();
    if cpus.is_empty() {
        return None;
    }
    
    let total_usage: f32 = cpus.iter().map(|cpu| cpu.cpu_usage()).sum::<f32>() / cpus.len() as f32;
    let usage = total_usage.round() as u16;
    
    log::debug!("CPU占用 (sysinfo): {}%", usage);
    Some(usage)
}

/// 免驱动读取当前 CPU 频率（MHz），取所有逻辑处理器的最大值。
/// CallNtPowerInformation(ProcessorInformation) 用户态即可调用，任何 CPU 都支持。
/// 用于 LHML 无 CPU 频率传感器时的兜底：
/// AMD FX/Bulldozer (family 15h/16h) 被 LibreHardwareMonitor 0.9.6 禁用支持
/// （PawnIO 模块在该平台有死机问题，上游注释掉了 Amd10Cpu），这类 CPU 没有任何 LHML CPU 传感器。
#[cfg(windows)]
pub(crate) fn get_cpu_clock_mhz_fallback() -> Option<u32> {
    use windows_sys::Win32::System::Power::{
        CallNtPowerInformation, ProcessorInformation, PROCESSOR_POWER_INFORMATION,
    };
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};

    unsafe {
        let mut sys_info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut sys_info);
        let proc_count = sys_info.dwNumberOfProcessors as usize;
        if proc_count == 0 {
            return None;
        }

        let mut buffer = vec![PROCESSOR_POWER_INFORMATION {
            Number: 0,
            MaxMhz: 0,
            CurrentMhz: 0,
            MhzLimit: 0,
            MaxIdleState: 0,
            CurrentIdleState: 0,
        }; proc_count];

        let status = CallNtPowerInformation(
            ProcessorInformation,
            std::ptr::null(),
            0,
            buffer.as_mut_ptr() as *mut _,
            (proc_count * std::mem::size_of::<PROCESSOR_POWER_INFORMATION>()) as u32,
        );
        if status != 0 {
            return None;
        }

        buffer
            .iter()
            .map(|p| p.CurrentMhz)
            .max()
            .filter(|mhz| *mhz > 0)
    }
}

#[cfg(not(windows))]
pub(crate) fn get_cpu_clock_mhz_fallback() -> Option<u32> {
    None
}

fn architecture_name(code: Option<u16>) -> String {
    match code {
        Some(0) => "x86".into(),
        Some(1) => "MIPS".into(),
        Some(2) => "Alpha".into(),
        Some(3) => "PowerPC".into(),
        Some(5) => "ARM".into(),
        Some(6) => "ia64".into(),
        Some(7) => "Alpha64".into(),
        Some(9) => "x64".into(),
        Some(12) => "ARM64".into(),
        _ => "未知".into(),
    }
}

fn memory_form_factor_name(code: Option<u16>) -> String {
    match code {
        Some(0) => "未知".into(),
        Some(1) => "Other".into(),
        Some(2) => "SIP".into(),
        Some(3) => "DIP".into(),
        Some(4) => "ZIP".into(),
        Some(5) => "SOJ".into(),
        Some(6) => "Proprietary".into(),
        Some(7) => "SIMM".into(),
        Some(8) => "DIMM".into(),
        Some(9) => "TSOP".into(),
        Some(10) => "PGA".into(),
        Some(11) => "RIMM".into(),
        Some(12) => "SODIMM".into(),
        Some(13) => "SRIMM".into(),
        Some(14) => "SMD".into(),
        Some(15) => "SSMP".into(),
        Some(16) => "QFP".into(),
        Some(17) => "TQFP".into(),
        Some(18) => "SOIC".into(),
        Some(19) => "LCC".into(),
        Some(20) => "PLCC".into(),
        Some(21) => "BGA".into(),
        Some(22) => "FPBGA".into(),
        Some(23) => "LGA".into(),
        Some(24) => "FB-DIMM".into(),
        _ => "未知".into(),
    }
}

fn memory_type_name(code: Option<u16>) -> String {
    match code {
        Some(0) => "未知".into(),
        Some(1) => "Other".into(),
        Some(2) => "DRAM".into(),
        Some(3) => "Synchronous DRAM".into(),
        Some(4) => "Cache DRAM".into(),
        Some(5) => "EDO".into(),
        Some(6) => "EDRAM".into(),
        Some(7) => "VRAM".into(),
        Some(8) => "SRAM".into(),
        Some(9) => "RAM".into(),
        Some(10) => "ROM".into(),
        Some(11) => "Flash".into(),
        Some(12) => "EEPROM".into(),
        Some(13) => "FEPROM".into(),
        Some(14) => "EPROM".into(),
        Some(15) => "CDRAM".into(),
        Some(16) => "3DRAM".into(),
        Some(17) => "SDRAM".into(),
        Some(18) => "SGRAM".into(),
        Some(19) => "RDRAM".into(),
        Some(20) => "DDR".into(),
        Some(21) => "DDR2".into(),
        Some(22) => "DDR2 FB-DIMM".into(),
        Some(24) => "DDR3".into(),
        Some(25) => "FBD2".into(),
        Some(26) => "DDR4".into(),
        Some(27) => "LPDDR".into(),
        Some(28) => "LPDDR2".into(),
        Some(29) => "LPDDR3".into(),
        Some(30) => "LPDDR4".into(),
        Some(31) => "Logical non-volatile".into(),
        Some(32) => "HBM".into(),
        Some(33) => "HBM2".into(),
        Some(34) => "DDR5".into(),
        Some(35) => "LPDDR5".into(),
        Some(36) => "HBM3".into(),
        _ => "未知".into(),
    }
}

fn chassis_type_name(codes: &Option<Vec<u16>>) -> String {
    match codes.as_ref().and_then(|v| v.first()).copied() {
        Some(1) => "Other".into(),
        Some(2) => "Unknown".into(),
        Some(3) => "Desktop".into(),
        Some(4) => "Low Profile Desktop".into(),
        Some(5) => "Pizza Box".into(),
        Some(6) => "Mini Tower".into(),
        Some(7) => "Tower".into(),
        Some(8) => "Portable".into(),
        Some(9) => "Laptop".into(),
        Some(10) => "Notebook".into(),
        Some(11) => "Hand Held".into(),
        Some(12) => "Docking Station".into(),
        Some(13) => "All in One".into(),
        Some(14) => "Sub Notebook".into(),
        Some(15) => "Space-Saving".into(),
        Some(16) => "Lunch Box".into(),
        Some(17) => "Main System Chassis".into(),
        Some(18) => "Expansion Chassis".into(),
        Some(19) => "Sub Chassis".into(),
        Some(20) => "Bus Expansion Chassis".into(),
        Some(21) => "Peripheral Chassis".into(),
        Some(22) => "Storage Chassis".into(),
        Some(23) => "Rack Mount Chassis".into(),
        Some(24) => "Sealed-Case PC".into(),
        Some(25) => "Multi-System Chassis".into(),
        Some(26) => "Compact PCI".into(),
        Some(27) => "Advanced TCA".into(),
        Some(28) => "Blade".into(),
        Some(29) => "Blade Enclosure".into(),
        Some(30) => "Tablet".into(),
        Some(31) => "Convertible".into(),
        Some(32) => "Detachable".into(),
        Some(33) => "IoT Gateway".into(),
        Some(34) => "Embedded PC".into(),
        Some(35) => "Mini PC".into(),
        Some(36) => "Stick PC".into(),
        _ => "未知".into(),
    }
}

fn get_static_hardware_info() -> Result<StaticHardwareInfo, HardwareError> {
    // Fast path: 先检查缓存，不加初始化锁
    {
        let cache = STATIC_HARDWARE_CACHE.lock().unwrap();
        if let Some(ref info) = *cache {
            log::debug!("从缓存获取静态硬件信息");
            return Ok(info.clone());
        }
    }

    // 序列化首次初始化，防止并发重复获取
    let _init_guard = HARDWARE_INIT_LOCK.lock().unwrap();

    // Double-check: 获取锁后再次检查缓存
    {
        let cache = STATIC_HARDWARE_CACHE.lock().unwrap();
        if let Some(ref info) = *cache {
            log::debug!("从缓存获取静态硬件信息");
            return Ok(info.clone());
        }
    }

    log::info!("开始并行获取静态硬件信息...");

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let errors_cpu = errors.clone();
    let cpu_handle = thread::spawn(move || {
        let wql = "SELECT Name, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed, L2CacheSize, L2CacheSpeed, L3CacheSize, L3CacheSpeed, LoadPercentage, Manufacturer, Architecture, SocketDesignation, CurrentClockSpeed, ExtClock, ProcessorId, Family, Stepping, Revision, NumberOfEnabledCore, VoltageCaps FROM Win32_Processor";
        match wmi_query::wmi_query(wql) {
            Ok(results) => {
                log::info!("获取到{}个CPU信息", results.len());
                results.into_iter().next().map(|row| {
                    let name = wmi_query::v_str(row.get("Name").unwrap_or(&wmi::Variant::Null)).unwrap_or_else(|| "未知CPU".to_string());
                    log::info!("CPU型号: {}", name);
                    CpuInfo {
                        name,
                        manufacturer: wmi_query::v_str(row.get("Manufacturer").unwrap_or(&wmi::Variant::Null)).unwrap_or_else(|| "未知".to_string()),
                        cores: wmi_query::v_u32(row.get("NumberOfCores").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        threads: wmi_query::v_u32(row.get("NumberOfLogicalProcessors").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        max_clock_speed: wmi_query::v_u32(row.get("MaxClockSpeed").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        l2_cache_size: wmi_query::v_u32(row.get("L2CacheSize").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        l3_cache_size: wmi_query::v_u32(row.get("L3CacheSize").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        load_percentage: wmi_query::v_u16(row.get("LoadPercentage").unwrap_or(&wmi::Variant::Null)),
                        architecture: architecture_name(wmi_query::v_u16(row.get("Architecture").unwrap_or(&wmi::Variant::Null))),
                        socket: wmi_query::v_str(row.get("SocketDesignation").unwrap_or(&wmi::Variant::Null)).unwrap_or_else(|| "未知".to_string()),
                        l2_cache_speed: wmi_query::v_u32(row.get("L2CacheSpeed").unwrap_or(&wmi::Variant::Null)),
                        l3_cache_speed: wmi_query::v_u32(row.get("L3CacheSpeed").unwrap_or(&wmi::Variant::Null)),
                        current_clock_speed: wmi_query::v_u32(row.get("CurrentClockSpeed").unwrap_or(&wmi::Variant::Null)),
                        ext_clock: wmi_query::v_u32(row.get("ExtClock").unwrap_or(&wmi::Variant::Null)),
                        processor_id: wmi_query::v_str(row.get("ProcessorId").unwrap_or(&wmi::Variant::Null)).unwrap_or_else(|| "未知".to_string()),
                        family: wmi_query::v_u32(row.get("Family").unwrap_or(&wmi::Variant::Null)).unwrap_or(0),
                        stepping: wmi_query::v_str(row.get("Stepping").unwrap_or(&wmi::Variant::Null)).unwrap_or_else(|| "未知".to_string()),
                        revision: wmi_query::v_u16(row.get("Revision").unwrap_or(&wmi::Variant::Null)).map(|r| r.to_string()).unwrap_or_else(|| "未知".to_string()),
                        enabled_cores: wmi_query::v_u32(row.get("NumberOfEnabledCore").unwrap_or(&wmi::Variant::Null)),
                        voltage_caps: wmi_query::v_u16(row.get("VoltageCaps").unwrap_or(&wmi::Variant::Null)).map(|v| format!("{} mV", v)),
                    }
                })
            }
            Err(e) => {
                if let Ok(mut errs) = errors_cpu.lock() {
                    errs.push(format!("CPU: {}", e));
                }
                None
            }
        }
    });

    let gpu_handle = thread::spawn(move || {
        std::panic::catch_unwind(|| {
            // 新优先级：NVML → LHML 优先（提供真实显存，修复 AMD 显卡 >4GB 无法识别），
            // 再用 WMI 补充驱动版本、核心架构、分辨率等静态字段；
            // NVML / LHML 完全失败时兜底 WMI（已含虚拟显卡过滤）。
            let dynamic_gpus = get_gpu_info();
            if !dynamic_gpus.is_empty() {
                let wmi_gpus = get_gpus_static_from_wmi();
                return merge_wmi_fields(dynamic_gpus, &wmi_gpus);
            }
            get_gpus_static_from_wmi()
        })
        .unwrap_or_else(|_| {
            log::error!("GPU 检测线程崩溃，已自动降级");
            Vec::new()
        })
    });

    let errors_mobo = errors.clone();
    let mobo_handle = thread::spawn(move || {

        // 4 个独立的 WMI COM 查询（无需 PowerShell）
        let mobo_rows = wmi_query::wmi_query("SELECT Manufacturer, Product, SerialNumber, Version FROM Win32_BaseBoard").unwrap_or_default();
        let sys_rows  = wmi_query::wmi_query("SELECT Manufacturer, Model, SystemType FROM Win32_ComputerSystem").unwrap_or_default();
        let bios_rows = wmi_query::wmi_query("SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS").unwrap_or_default();
        let chassis_rows = wmi_query::wmi_query("SELECT ChassisTypes FROM Win32_SystemEnclosure").unwrap_or_default();

        let mobo_row = mobo_rows.first();
        let sys_row = sys_rows.first();
        let bios_row = bios_rows.first();
        let chassis_row = chassis_rows.first();

        // 日志
        if let Some(m) = mobo_row {
            let manu = m.get("Manufacturer").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
            let prod = m.get("Product").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
            log::info!("主板: {} {}", manu, prod);
        } else if let Some(s) = sys_row {
            let manu = s.get("Manufacturer").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
            let model = s.get("Model").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
            log::info!("主板(Win32_BaseBoard 为空，回退 Win32_ComputerSystem): {} {}", manu, model);
        }
        if let Some(b) = bios_row {
            let ver = b.get("SMBIOSBIOSVersion").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
            log::info!("BIOS: {}", ver);
        }

        // 主板信息为空时上报 warning
        if mobo_row.is_none() {
            if let Ok(mut errs) = errors_mobo.lock() {
                errs.push("主板: WMI查询失败 (Win32_BaseBoard 返回空，已用 Win32_ComputerSystem 回退)".to_string());
            }
        }

        // 兜底：BaseBoard 为空时用 ComputerSystem 的 Manufacturer / Model
        let product = mobo_row
            .and_then(|m| m.get("Product").and_then(|v| wmi_query::v_str(v)))
            .or_else(|| sys_row.and_then(|s| s.get("Model").and_then(|v| wmi_query::v_str(v))))
            .unwrap_or_else(|| "未知".to_string());
        let manufacturer = mobo_row
            .and_then(|m| m.get("Manufacturer").and_then(|v| wmi_query::v_str(v)))
            .or_else(|| sys_row.and_then(|s| s.get("Manufacturer").and_then(|v| wmi_query::v_str(v))))
            .unwrap_or_else(|| "未知".to_string());

        let chassis_types = chassis_row
            .and_then(|c| c.get("ChassisTypes"))
            .map(|v| wmi_query::v_u16_arr(v))
            .filter(|a| !a.is_empty());

        Some(MotherboardInfo {
            product,
            manufacturer,
            serial_number: mobo_row.and_then(|m| m.get("SerialNumber").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            version: mobo_row.and_then(|m| m.get("Version").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            bios_vendor: bios_row.and_then(|b| b.get("Manufacturer").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            bios_version: bios_row.and_then(|b| b.get("SMBIOSBIOSVersion").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            bios_release_date: bios_row.and_then(|b| b.get("ReleaseDate").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            system_manufacturer: sys_row.and_then(|s| s.get("Manufacturer").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            system_model: sys_row.and_then(|s| s.get("Model").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            system_type: sys_row.and_then(|s| s.get("SystemType").and_then(|v| wmi_query::v_str(v))).unwrap_or_else(|| "未知".to_string()),
            chassis_type: chassis_type_name(&chassis_types),
        })
    });

    let errors_mem = errors.clone();
    let mem_handle = thread::spawn(move || {
        match wmi_query::wmi_query("SELECT Manufacturer, PartNumber, Capacity, Speed, BankLabel, FormFactor, MemoryType, ConfiguredClockSpeed, ConfiguredVoltage, DataWidth, TotalWidth, SerialNumber, TypeDetail FROM Win32_PhysicalMemory") {
            Ok(results) => {
                log::info!("获取到{}个内存条信息", results.len());
                results.into_iter().map(|row| {
                    let capacity_bytes = row.get("Capacity").and_then(|v| wmi_query::v_u64(v)).unwrap_or(0) as f64;
                    MemoryInfo {
                        manufacturer: row.get("Manufacturer").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        part_number: row.get("PartNumber").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()).trim().to_string(),
                        capacity_gb: capacity_bytes / (1024.0 * 1024.0 * 1024.0),
                        // Speed: 优先使用 ConfiguredClockSpeed（实际运行频率），
                        // 回退到 Speed（模块额定频率），因为 Speed 在某些系统（特别是 DDR5）上可能不准确
                        speed_mhz: {
                            let configured = row.get("ConfiguredClockSpeed").and_then(|v| wmi_query::v_u32(v)).unwrap_or(0);
                            if configured > 0 { configured }
                            else { row.get("Speed").and_then(|v| wmi_query::v_u32(v)).unwrap_or(0) }
                        },
                        bank_label: row.get("BankLabel").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        form_factor: memory_form_factor_name(row.get("FormFactor").and_then(|v| wmi_query::v_u16(v))),
                        memory_type: memory_type_name(row.get("MemoryType").and_then(|v| wmi_query::v_u16(v))),
                        configured_clock_speed: row.get("ConfiguredClockSpeed").and_then(|v| wmi_query::v_u32(v)),
                        configured_voltage: row.get("ConfiguredVoltage").and_then(|v| wmi_query::v_u32(v)),
                        data_width: row.get("DataWidth").and_then(|v| wmi_query::v_u32(v)),
                        total_width: row.get("TotalWidth").and_then(|v| wmi_query::v_u32(v)),
                        serial_number: row.get("SerialNumber").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        type_detail: row.get("TypeDetail").and_then(|v| wmi_query::v_u16(v)).map(|d| d.to_string()).unwrap_or_else(|| "未知".to_string()),
                    }
                }).collect::<Vec<MemoryInfo>>()
            }
            Err(e) => {
                if let Ok(mut errs) = errors_mem.lock() {
                    errs.push(format!("内存: {}", e));
                }
                Vec::new()
            }
        }
    });

    let errors_disk = errors.clone();
    let disk_handle = thread::spawn(move || {
        match wmi_query::wmi_query("SELECT Model, Size, InterfaceType, SerialNumber, FirmwareRevision, MediaType, BytesPerSector, Partitions, Status, PNPDeviceID, Index FROM Win32_DiskDrive") {
            Ok(results) => {
                log::info!("获取到{}个硬盘信息", results.len());
                // 卷 → 物理盘号映射，用于获取每个硬盘的准确分区容量
                let volume_map = enumerate_volumes_by_disk();
                results.into_iter().map(|row| {
                    let index = row.get("Index").and_then(|v| wmi_query::v_u32(v)).unwrap_or(0);
                    let partition_total_gb =
                        volume_map.get(&index).map(|ps| ps.iter().map(|p| p.total_gb).sum());
                    let size_gb = resolve_disk_size_gb(
                        row.get("Size").and_then(|v| wmi_query::v_u64(v)).unwrap_or(0),
                        partition_total_gb,
                    );
                    let media_type = row.get("MediaType").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                    let is_ssd = media_type.to_lowercase().contains("ssd") || media_type.to_lowercase().contains("solid state");
                    DiskDetailInfo {
                        model: row.get("Model").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        size_gb,
                        interface_type: row.get("InterfaceType").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        serial_number: row.get("SerialNumber").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        firmware_revision: row.get("FirmwareRevision").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        media_type: if media_type.is_empty() { "未知".to_string() } else { media_type },
                        bytes_per_sector: row.get("BytesPerSector").and_then(|v| wmi_query::v_u32(v)),
                        partitions: row.get("Partitions").and_then(|v| wmi_query::v_u32(v)).unwrap_or(0),
                        status: row.get("Status").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        is_ssd,
                    }
                }).collect::<Vec<DiskDetailInfo>>()
            }
            Err(e) => {
                if let Ok(mut errs) = errors_disk.lock() {
                    errs.push(format!("硬盘: {}", e));
                }
                Vec::new()
            }
        }
    });

    let errors_sound = errors.clone();
    let sound_handle = thread::spawn(move || {
        match wmi_query::wmi_query("SELECT Name, Manufacturer, Status, DeviceID, PNPDeviceID FROM Win32_SoundDevice") {
            Ok(results) => {
                let sound_filter_keywords = [
                    "Virtual", "VB-Audio", "Voicemeeter", "CABLE", "Sonic Studio",
                    "NVIDIA Virtual", "Steam Streaming", "Oculus", "Wave Link",
                    "Elgato Sound Capture", "Nahimic", "DTS", "Dolby",
                    "Bluetooth", "Hands-Free", "S/PDIF",
                ];
                let filtered: Vec<_> = results.into_iter()
                    .filter(|row| {
                        let status = row.get("Status").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        let pnp = row.get("PNPDeviceID").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        let name = row.get("Name").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        if status != "OK" { return false; }
                        let pnp_lower = pnp.to_lowercase();
                        if pnp_lower.starts_with("usb\\") || pnp_lower.starts_with("hid\\") || pnp_lower.starts_with("swd\\") {
                            return false;
                        }
                        let name_lower = name.to_lowercase();
                        !sound_filter_keywords.iter().any(|kw| name_lower.contains(&kw.to_lowercase()))
                    })
                    .collect();
                log::info!("获取到{}个声卡信息", filtered.len());
                filtered.into_iter().map(|row| {
                    SoundCardInfo {
                        name: row.get("Name").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        manufacturer: row.get("Manufacturer").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        status: row.get("Status").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        device_id: row.get("DeviceID").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        pnp_device_id: row.get("PNPDeviceID").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                    }
                }).collect::<Vec<SoundCardInfo>>()
            }
            Err(e) => {
                if let Ok(mut errs) = errors_sound.lock() {
                    errs.push(format!("声卡: {}", e));
                }
                Vec::new()
            }
        }
    });

    let errors_network = errors.clone();
    let network_handle = thread::spawn(move || {
        use wmi::Variant;
        match wmi_query::wmi_query("SELECT Name, Manufacturer, AdapterType, MACAddress, Speed, NetConnectionID, ServiceName, Index, MaxSpeed, GUID, PhysicalAdapter, NetEnabled, PNPDeviceID FROM Win32_NetworkAdapter") {
            Ok(results) => {
                let net_filter_keywords = [
                    "Hyper-V", "vEthernet", "Virtual", "VirtualBox", "VMware",
                    "Bluetooth", "Tailscale", "ZeroTier", "WSL", "Docker",
                    "Npcap", "WireGuard", "OpenVPN", "TAP-Windows", "WAN Miniport",
                    "VPN", "Proton", "Nord", "Cloudflare WARP",
                ];
                let filtered: Vec<_> = results.into_iter()
                    .filter(|row| {
                        let physical = matches!(row.get("PhysicalAdapter"), Some(Variant::Bool(true)));
                        let enabled = matches!(row.get("NetEnabled"), Some(Variant::Bool(true)));
                        if !physical || !enabled { return false; }
                        let pnp = row.get("PNPDeviceID").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        if pnp.to_lowercase().starts_with("swd\\") { return false; }
                        let name = row.get("Name").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        let adapter = row.get("AdapterType").and_then(|v| wmi_query::v_str(v)).unwrap_or_default();
                        let name_lower = name.to_lowercase();
                        !net_filter_keywords.iter().any(|kw| name_lower.contains(&kw.to_lowercase()))
                            && !adapter.to_lowercase().contains("loopback")
                    })
                    .collect();
                log::info!("获取到{}个网卡信息", filtered.len());
                filtered.into_iter().map(|row| {
                    let speed_bps = row.get("Speed").and_then(|v| wmi_query::v_u64(v)).unwrap_or(0);
                    let max_bps = row.get("MaxSpeed").and_then(|v| wmi_query::v_u64(v));
                    NetworkCardInfo {
                        name: row.get("Name").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        manufacturer: row.get("Manufacturer").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        adapter_type: row.get("AdapterType").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        mac_address: row.get("MACAddress").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        speed_mbps: speed_bps / 1_000_000,
                        connection_name: row.get("NetConnectionID").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        service_name: row.get("ServiceName").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        index: row.get("Index").and_then(|v| wmi_query::v_u32(v)).unwrap_or(0),
                        max_speed: max_bps.map(|s| s / 1_000_000),
                        guid: row.get("GUID").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                    }
                }).collect::<Vec<NetworkCardInfo>>()
            }
            Err(e) => {
                if let Ok(mut errs) = errors_network.lock() {
                    errs.push(format!("网卡: {}", e));
                }
                Vec::new()
            }
        }
    });

    let monitor_handle = thread::spawn(move || {
        // Win32_DesktopMonitor 在现代 Windows 上已废弃，经常返回空，
        // 所以 WMI 失败或为空时会自动回退到 EnumDisplaySettingsW + 注册表 EDID 方案。
        let wmi_results = wmi_query::wmi_query("SELECT Name, MonitorManufacturerName, ScreenWidth, ScreenHeight, DisplayFrequency, PNPDeviceID, Status, Availability FROM Win32_DesktopMonitor");

        match wmi_results {
            Ok(results) if !results.is_empty() => {
                let filtered: Vec<_> = results.into_iter()
                    .filter(|row| {
                        let name_ok = row.get("Name").map_or(false, |v| wmi_query::v_nonempty(v));
                        let pnp_ok = row.get("PNPDeviceID").map_or(false, |v| wmi_query::v_nonempty(v));
                        name_ok && pnp_ok
                    })
                    .collect();
                log::info!("获取到{}个显示器信息", filtered.len());

                if filtered.is_empty() {
                    log::debug!("WMI 结果经筛选后为空，回退到 EnumDisplaySettingsW");
                    return fallback_enumerate_monitors();
                }

                let mut monitors: Vec<MonitorInfo> = filtered.into_iter().map(|row| {
                    MonitorInfo {
                        name: row.get("Name").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        manufacturer: row.get("MonitorManufacturerName").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        screen_width: row.get("ScreenWidth").and_then(|v| wmi_query::v_u32(v)),
                        screen_height: row.get("ScreenHeight").and_then(|v| wmi_query::v_u32(v)),
                        refresh_rate: row.get("DisplayFrequency").and_then(|v| wmi_query::v_u32(v)),
                        pnp_device_id: row.get("PNPDeviceID").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        status: row.get("Status").and_then(|v| wmi_query::v_str(v)).unwrap_or_else(|| "未知".to_string()),
                        availability: row.get("Availability").and_then(|v| wmi_query::v_u16(v)),
                    }
                }).collect::<Vec<MonitorInfo>>();

                // EDID 回退（通过注册表读取，无 PowerShell）：如果名称是通用的，替换为真实型号。
                // 关键：按 PNP 设备 ID 匹配，而不是按数组下标——WMI 的枚举顺序与注册表
                // EDID 枚举顺序并不一致，按下标对齐会把 A 显示器的型号错配到 B 显示器。
                let has_generic = monitors.iter().any(|m| is_generic_monitor_name(&m.name));
                if has_generic {
                    log::info!("检测到通用显示器名称，从注册表 EDID 获取真实型号（按 PNP ID 匹配）...");
                    let edid_map = crate::display_cache::get_edid_monitor_names_by_pnpid();
                    if !edid_map.is_empty() {
                        for m in monitors.iter_mut() {
                            if is_generic_monitor_name(&m.name) {
                                if let Some(pnpid) = extract_pnpid(&m.pnp_device_id) {
                                    if let Some(edid_name) = edid_map.get(&pnpid) {
                                        if !edid_name.is_empty() {
                                            log::info!("显示器[PNP {}]: EDID 替换 '{}' -> '{}'", pnpid, m.name, edid_name);
                                            m.name = edid_name.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                monitors
            }
            _ => {
                log::debug!("WMI Win32_DesktopMonitor 不可用或为空，回退到 EnumDisplaySettingsW 枚举显示器");
                fallback_enumerate_monitors()
            }
        }
    });

    let cpu = cpu_handle.join().unwrap_or_else(|_| None).unwrap_or_else(|| CpuInfo {
        name: "未知CPU".to_string(),
        manufacturer: "未知".to_string(),
        cores: 0,
        threads: 0,
        max_clock_speed: 0,
        l2_cache_size: 0,
        l3_cache_size: 0,
        load_percentage: None,
        architecture: "未知".to_string(),
        socket: "未知".to_string(),
        l2_cache_speed: None,
        l3_cache_speed: None,
        current_clock_speed: None,
        ext_clock: None,
        processor_id: "未知".to_string(),
        family: 0,
        stepping: "未知".to_string(),
        revision: "未知".to_string(),
        enabled_cores: None,
        voltage_caps: None,
    });

    let gpu_static = gpu_handle.join().unwrap_or_else(|_| Vec::new());
    let motherboard = mobo_handle.join().unwrap_or_else(|_| None).unwrap_or_else(|| MotherboardInfo {
        product: "未知".to_string(),
        manufacturer: "未知".to_string(),
        serial_number: "未知".to_string(),
        version: "未知".to_string(),
        bios_vendor: "未知".to_string(),
        bios_version: "未知".to_string(),
        bios_release_date: "未知".to_string(),
        system_manufacturer: "未知".to_string(),
        system_model: "未知".to_string(),
        system_type: "未知".to_string(),
        chassis_type: "未知".to_string(),
    });
    let memory = mem_handle.join().unwrap_or_else(|_| Vec::new());
    let disk = disk_handle.join().unwrap_or_else(|_| Vec::new());
    let sound_card = sound_handle.join().unwrap_or_else(|_| Vec::new());
    let network_card = network_handle.join().unwrap_or_else(|_| Vec::new());
    let mut monitor = monitor_handle.join().unwrap_or_else(|_| Vec::new());
    // Fallback: fill monitor resolution/refresh from GPU output if WMI didn't provide it
    if !gpu_static.is_empty() && !monitor.is_empty() {
        let gpu = &gpu_static[0];
        for m in monitor.iter_mut() {
            if m.screen_width.is_none() { m.screen_width = gpu.resolution_width; }
            if m.screen_height.is_none() { m.screen_height = gpu.resolution_height; }
            if m.refresh_rate.is_none() { m.refresh_rate = gpu.refresh_rate; }
        }
    }

    if let Ok(errs) = errors.lock() {
        for e in errs.iter() {
            log::warn!("硬件获取警告: {}", e);
        }
    }

    let static_info = StaticHardwareInfo {
        cpu,
        gpu_static,
        motherboard,
        memory,
        disk,
        sound_card,
        network_card,
        monitor,
    };

    // 仅在 LHML（NexBoxMonitor）至少成功读取过一次后才写入静态缓存。
    // 首次调用可能发生在启动头几秒：此时 NVML 可能只有 NVIDIA、LHML 未就绪只剩 WMI 兜底，
    // 若把这份不完整的 GPU 列表缓存住，顶部状态卡后来出现的 AMD/核显在底部会整个会话缺失。
    // 不写缓存时下次 get_hardware 会重新构建，自愈成完整列表。
    if crate::sensor::LHM_EVER_SUCCEEDED.load(std::sync::atomic::Ordering::Relaxed) {
        let mut cache = STATIC_HARDWARE_CACHE.lock().unwrap();
        *cache = Some(static_info.clone());
    } else {
        log::info!("NexBoxMonitor 尚未就绪，本次静态硬件信息不写入缓存");
    }

    log::info!("静态硬件信息并行获取完成");
    Ok(static_info)
}

pub fn get_hardware_info() -> Result<HardwareInfo, HardwareError> {
    let static_info = get_static_hardware_info()?;

    // 获取动态数据
    let cpu_load = get_cpu_dynamic_info();
    let gpu_dynamic = get_gpu_dynamic_info(&static_info.gpu_static);

    // 组合完整信息
    let mut cpu = static_info.cpu;
    // 仅在成功读取到动态 CPU 占用时覆盖静态值，避免在失败时将已有值清空
    if let Some(load) = cpu_load {
        cpu.load_percentage = Some(load);
    }

    let gpu: Vec<GpuInfo> = static_info
        .gpu_static
        .iter()
        .enumerate()
        .map(|(i, gs)| {
            let (temp, usage, dyn_mem) = gpu_dynamic
                .get(i)
                .copied()
                .unwrap_or((None, None, None));
            // 显存：优先动态来源（NVML 的 NVIDIA、LHML 的 AMD 独显等真实值）；
            // 核显 LHML 拿不到显存，不再回退 WMI AdapterRAM，直接为 None（前端不显示显存项）
            let memory_gb = if is_integrated_gpu_name(&gs.name) {
                dyn_mem
            } else {
                dyn_mem.or(gs.memory_gb)
            };
            GpuInfo {
                name: gs.name.clone(),
                vendor: gs.vendor.clone(),
                memory_gb,
                driver_version: gs.driver_version.clone(),
                temperature: temp,
                usage,
                video_processor: gs.video_processor.clone(),
                adapter_compatibility: gs.adapter_compatibility.clone(),
                driver_date: gs.driver_date.clone(),
                installed_drivers: gs.installed_drivers.clone(),
                video_mode: gs.video_mode.clone(),
                resolution_width: gs.resolution_width,
                resolution_height: gs.resolution_height,
                refresh_rate: gs.refresh_rate,
                device_id: gs.device_id.clone(),
                pnp_device_id: gs.pnp_device_id.clone(),
                status: gs.status.clone(),
                inf_filename: gs.inf_filename.clone(),
                video_architecture: gs.video_architecture.clone(),
                video_memory_type: gs.video_memory_type.clone(),
            }
        })
        .collect();

    Ok(HardwareInfo {
        cpu,
        gpu,
        motherboard: static_info.motherboard,
        memory: static_info.memory,
        disk: static_info.disk,
        sound_card: static_info.sound_card,
        network_card: static_info.network_card,
        monitor: static_info.monitor,
    })
}

#[tauri::command]
pub async fn get_hardware() -> Result<HardwareInfo, String> {
    match tauri::async_runtime::spawn_blocking(|| get_hardware_info()).await {
        Ok(Ok(info)) => Ok(info),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_cpu_load() -> Result<Option<u16>, String> {
    match tauri::async_runtime::spawn_blocking(|| get_cpu_dynamic_info()).await {
        Ok(load) => Ok(load),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuStatus {
    pub temperature: Option<f64>,
    pub usage: Option<u32>,
}

#[tauri::command]
pub async fn get_gpu_status(index: usize) -> Result<GpuStatus, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        // 1. NVML（NVIDIA 最佳方案）
        let nvml_gpus = std::panic::catch_unwind(|| get_nvidia_gpus_with_nvml());
        if let Ok(Ok(gpus)) = nvml_gpus {
            if let Some(gpu) = gpus.get(index) {
                return GpuStatus {
                    temperature: gpu.temperature,
                    usage: gpu.usage,
                };
            }
        }

        // 2. LHML 通用兜底（支持所有厂商），按硬件名分组以支持多 GPU 索引
        if let Ok(response) = crate::sensor::read_lhm_sensors() {
            let mut gpu_names: Vec<String> = Vec::new();
            for s in &response.sensors {
                let ht = s.hardware_type.to_lowercase();
                if !ht.starts_with("gpu") {
                    continue;
                }
                let key = s.hardware.clone();
                if !gpu_names.contains(&key) {
                    gpu_names.push(key);
                }
            }

            // 仅 NVIDIA 独显存在时过滤 Intel 核显，AMD 核显保留显示
            let has_nvidia = gpu_names.iter().any(|name| {
                response.sensors.iter().any(|s| {
                    s.hardware == *name
                        && s.hardware_type.eq_ignore_ascii_case("GpuNvidia")
                })
            });
            let visible: Vec<&String> = gpu_names
                .iter()
                .filter(|name| {
                    if !has_nvidia {
                        return true;
                    }
                    !response.sensors.iter().any(|s| {
                        s.hardware == **name
                            && s.hardware_type.eq_ignore_ascii_case("GpuIntel")
                    })
                })
                .collect();

            if let Some(gpu_name) = visible.get(index) {
                let temp = response
                    .sensors
                    .iter()
                    .filter(|s| {
                        s.hardware == **gpu_name
                            && s.sensor_type == "Temperature"
                            && (s.name == "GPU Core" || s.name == "GPU" || s.name == "Core"
                                || s.name == "GPU Temperature")
                    })
                    .map(|s| s.value)
                    .next();
                // Intel 核显的占用传感器名为 "D3D 3D"（来自 D3D 引擎枚举），并非 "GPU Core"
                let usage = response
                    .sensors
                    .iter()
                    .filter(|s| {
                        s.hardware == **gpu_name
                            && s.sensor_type == "Load"
                            && (s.name == "GPU Core" || s.name == "D3D 3D" || s.name == "GPU"
                                || s.name == "D3D Usage" || s.name == "Core")
                    })
                    .map(|s| s.value as u32)
                    .next();
                return GpuStatus { temperature: temp, usage };
            }
        }

        GpuStatus {
            temperature: None,
            usage: None,
        }
    })
    .await;

    match result {
        Ok(status) => Ok(status),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskInfo {
    pub name: String,
    pub total_gb: f64,
    pub available_gb: f64,
    pub used_gb: f64,
    pub usage_percent: f64,
}

/// 汇总所有磁盘的空间统计：(total_space, available_space, used_space, usage_percent)。
/// 无磁盘时 total_space=0、usage_percent=0。
fn collect_disk_stats() -> (u64, u64, u64, f64) {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();

    let mut total_space: u64 = 0;
    let mut available_space: u64 = 0;

    for disk in disks.iter() {
        let mount_point = disk.mount_point().to_string_lossy();
        if mount_point.is_empty() {
            continue;
        }
        let total = disk.total_space();
        let total_gb = total as f64 / 1_073_741_824.0;
        // 跳过容量明显异常的卷（如存储空间/虚拟盘的薄配置会报告虚高的逻辑容量，可达数百 TB），
        // 避免污染顶部状态卡的磁盘总容量统计。
        if total_gb > MAX_REASONABLE_DISK_GB {
            log::warn!(
                "跳过容量异常的卷 {}: {:.0}GB (> {}GB)",
                mount_point,
                total_gb,
                MAX_REASONABLE_DISK_GB
            );
            continue;
        }
        total_space = total_space.saturating_add(total);
        available_space = available_space.saturating_add(disk.available_space());
    }

    let used_space = total_space.saturating_sub(available_space);
    let usage_percent = if total_space > 0 {
        (used_space as f64 / total_space as f64) * 100.0
    } else {
        0.0
    };

    (total_space, available_space, used_space, usage_percent)
}

/// 所有磁盘的总占用百分比（0-100）。无磁盘时返回 None（供托盘悬停面板按需读取）。
pub fn disk_usage_percent() -> Option<f64> {
    let (total, _, _, percent) = collect_disk_stats();
    if total == 0 {
        None
    } else {
        Some(percent)
    }
}

#[tauri::command]
pub async fn get_disk_status() -> Result<DiskInfo, String> {
    let result = tauri::async_runtime::spawn_blocking(|| {
        let (total_space, available_space, used_space, usage_percent) = collect_disk_stats();

        let total_gb = total_space as f64 / (1024.0 * 1024.0 * 1024.0);
        let available_gb = available_space as f64 / (1024.0 * 1024.0 * 1024.0);
        let used_gb = used_space as f64 / (1024.0 * 1024.0 * 1024.0);

        DiskInfo {
            name: String::from("All Disks"),
            total_gb,
            available_gb,
            used_gb,
            usage_percent,
        }
    }).await;

    match result {
        Ok(info) => Ok(info),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn is_nvidia_gpu() -> bool {
    let cache = STATIC_HARDWARE_CACHE.lock().unwrap();
    cache
        .as_ref()
        .map(|c| c.gpu_static.iter().any(|g| g.vendor == GpuVendor::NVIDIA))
        .unwrap_or(false)
}

#[tauri::command]
pub fn get_os_version() -> Result<String, String> {
    long_os_version().ok_or_else(|| "无法获取操作系统版本".to_string())
}

/// 操作系统名称（如 "Windows 10 专业版" / "Windows 11 Pro"）。
///
/// 背景：旧实现直接用 `sysinfo::System::long_os_version()`，它只读取注册表
/// `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProductName`。部分机器
/// （Win7→Win10 升级、克隆/镜像恢复等）会残留旧的 ProductName（如 "Windows 7 ..."），
/// 导致真实 Win10 被误识别为 Win7。
///
/// 修复思路：以 ntdll `RtlGetVersion` 返回的**真实内核版本**判定系统家族
/// （该 API 读取内核信息，不受注册表残留影响），再用 ProductName 保留
/// “专业版/旗舰版/Home/Pro”等版本后缀；家族不一致时以内核为准。
pub(crate) fn long_os_version() -> Option<String> {
    let product_name = read_product_name();

    let Some((major, minor, build)) = kernel_version() else {
        // 拿不到内核版本（理论上不会发生）时退回旧行为
        return product_name;
    };
    let real_family = os_family_name(major, minor, build);

    let Some(pn) = product_name else {
        return Some(real_family);
    };

    // 服务器系统（Windows Server ...）在 Win10/11 内核上，直接用原名更准确
    if pn.starts_with("Windows Server") {
        return Some(pn);
    }

    // 注册表家族与真实内核一致：直接沿用原名，保留版本后缀
    if pn.contains(&real_family) {
        return Some(pn);
    }

    // Win11 的 ProductName 仍是 "Windows 10 xx"，按真实内核替换为 11
    if real_family == "Windows 11" && pn.contains("Windows 10") {
        return Some(pn.replacen("Windows 10", "Windows 11", 1));
    }

    // 家族不一致（残留旧 ProductName）：只保留原名里家族名之后的后缀，替换为真实家族
    // 例如 "Windows 7 旗舰版" -> "Windows 10 旗舰版"
    if let Some(idx) = pn.find("Windows") {
        let suffix = strip_version_token(&pn[idx + "Windows".len()..]);
        if suffix.is_empty() {
            return Some(real_family);
        }
        return Some(format!("{real_family} {suffix}"));
    }

    Some(real_family)
}

/// 读取注册表 ProductName（如 "Windows 10 Pro"）。
fn read_product_name() -> Option<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_value::<String, _>("ProductName"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 通过 ntdll RtlGetVersion 获取真实内核版本 (major, minor, build)。
/// 与 GetVersionEx 不同，RtlGetVersion 不受应用兼容性清单影响，也不依赖注册表。
fn kernel_version() -> Option<(u32, u32, u32)> {
    #[repr(C)]
    struct OsVersionInfoW {
        os_version_info_size: u32,
        major_version: u32,
        minor_version: u32,
        build_number: u32,
        platform_id: u32,
        csd_version: [u16; 128],
    }

    type PRtlGetVersion = unsafe extern "system" fn(*mut OsVersionInfoW) -> i32;

    // ntdll 常驻系统，动态加载即可（与 feature_flags.rs 保持一致，避免静态链接隐患）
    let lib = unsafe { libloading::Library::new("ntdll.dll") }.ok()?;
    let rtl_get_version = unsafe { lib.get::<PRtlGetVersion>(b"RtlGetVersion") }.ok()?;

    let mut info = OsVersionInfoW {
        os_version_info_size: std::mem::size_of::<OsVersionInfoW>() as u32,
        major_version: 0,
        minor_version: 0,
        build_number: 0,
        platform_id: 0,
        csd_version: [0; 128],
    };

    // 返回 NTSTATUS，STATUS_SUCCESS = 0
    let status = unsafe { rtl_get_version(&mut info) };
    if status != 0 {
        return None;
    }
    Some((info.major_version, info.minor_version, info.build_number))
}

/// 内核版本 -> 系统家族名。
fn os_family_name(major: u32, minor: u32, build: u32) -> String {
    if major == 10 {
        if build >= 22000 {
            "Windows 11".to_string()
        } else {
            "Windows 10".to_string()
        }
    } else if major == 6 {
        match minor {
            3 => "Windows 8.1".to_string(),
            2 => "Windows 8".to_string(),
            1 => "Windows 7".to_string(),
            0 => "Windows Vista".to_string(),
            _ => format!("Windows {}.{}", major, minor),
        }
    } else if major == 5 && minor == 1 {
        "Windows XP".to_string()
    } else {
        format!("Windows {}", major)
    }
}

/// 去除 ProductName 中紧随 "Windows" 之后的旧版本号，如 " 7 旗舰版" -> "旗舰版"。
fn strip_version_token(rest: &str) -> String {
    let rest = rest.trim_start();
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i > 0 {
        rest[i..].trim_start().to_string()
    } else {
        rest.to_string()
    }
}

// ─── Disk Health（纯 Rust + WinAPI 直读 SMART，移植自 CrystalDiskInfo）───

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartitionInfo {
    pub drive_letter: String,
    pub total_gb: f64,
    pub available_gb: f64,
    pub used_gb: f64,
    pub usage_percent: f64,
    pub filesystem: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskHealthInfo {
    pub index: u32,
    pub model: String,
    pub media_type: String,
    pub size_gb: f64,
    pub interface_type: String,
    pub health_status: String,
    pub operational_status: String,
    pub temperature_c: Option<f64>,
    pub wear_percentage: Option<f64>,
    pub power_on_hours: Option<u64>,
    /// 通电次数
    pub power_on_count: Option<u64>,
    /// 累计数据读取量（字节）
    pub data_read_bytes: Option<u64>,
    /// 累计数据写入量（字节）
    pub data_written_bytes: Option<u64>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
    pub status: String,
    pub partition_count: u32,
    pub serial_number: String,
    pub partition_style: String,
    pub is_boot_disk: bool,
    pub partitions: Vec<PartitionInfo>,
    pub total_usage_gb: f64,
    pub total_capacity_gb: f64,
    /// 健康度百分比 0-100（与 CrystalDiskInfo 一致的 Life；HDD 按状态映射）
    pub health_percent: Option<u8>,
    /// 是否为 SSD（由后端 SMART 直读判定，前端据此显示 TRIM/碎片整理文案）
    pub is_ssd: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiskHealthResponse {
    pub disks: Vec<DiskHealthInfo>,
    pub total_count: u32,
    pub healthy_count: u32,
    pub warning_count: u32,
    pub unhealthy_count: u32,
}

/// 枚举固定盘符（C:、D:…）并映射到物理盘号，收集分区容量/文件系统信息。
/// 使用 winapi（GetLogicalDrives + GetDriveTypeW + IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS），
/// 全程无需 PowerShell。
fn enumerate_volumes_by_disk() -> std::collections::HashMap<u32, Vec<PartitionInfo>> {
    use winapi::shared::minwindef::{DWORD, LPVOID};
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::fileapi::{
        CreateFileW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, OPEN_EXISTING,
    };
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::ioapiset::DeviceIoControl;
    use winapi::um::winnt::{
        FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ,
    };

    // 手动声明（winapi 的签名使用 ULARGE_INTEGER union，这里直接映射为 u64）
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
    }

    const IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS: DWORD = 0x0056_0000;
    const DRIVE_FIXED: u32 = 3;

    #[repr(C)]
    struct DiskExtent {
        disk_number: u32,
        starting_offset: i64,
        extent_length: i64,
    }
    #[repr(C)]
    struct VolumeDiskExtents {
        number_of_disk_extents: u32,
        extents: [DiskExtent; 1],
    }

    let mut map: std::collections::HashMap<u32, Vec<PartitionInfo>> = std::collections::HashMap::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{}:\\", letter);
        let root_w: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let dtype = unsafe { GetDriveTypeW(root_w.as_ptr()) };
        if dtype != DRIVE_FIXED {
            continue;
        }

        // 打开卷设备，查询所属物理盘号
        let vol = format!("\\\\.\\{}:", letter);
        let vol_w: Vec<u16> = vol.encode_utf16().chain(std::iter::once(0)).collect();
        let h = unsafe {
            CreateFileW(
                vol_w.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            continue;
        }
        let mut extents: VolumeDiskExtents = unsafe { std::mem::zeroed() };
        let mut returned: DWORD = 0;
        let ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
                std::ptr::null_mut(),
                0,
                &mut extents as *mut _ as LPVOID,
                std::mem::size_of::<VolumeDiskExtents>() as DWORD,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        unsafe { CloseHandle(h) };
        if ok == 0 || extents.number_of_disk_extents == 0 {
            continue;
        }
        let disk_number = extents.extents[0].disk_number;

        // 容量（GetDiskFreeSpaceExW）
        let (total, free) = {
            let mut free_bytes: u64 = 0;
            let mut total_bytes: u64 = 0;
            let mut total_free: u64 = 0;
            let r = unsafe {
                GetDiskFreeSpaceExW(
                    root_w.as_ptr(),
                    &mut free_bytes,
                    &mut total_bytes,
                    &mut total_free,
                )
            };
            if r == 0 {
                log::warn!("GetDiskFreeSpaceExW({}) 失败，错误码 {}", root, unsafe { GetLastError() });
                (0u64, 0u64)
            } else {
                (total_bytes, total_free)
            }
        };
        // 文件系统（GetVolumeInformationW）
        let filesystem = {
            let mut fs_buf = [0u16; 32];
            let r = unsafe {
                GetVolumeInformationW(
                    root_w.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    fs_buf.as_mut_ptr(),
                    fs_buf.len() as DWORD,
                )
            };
            if r == 0 {
                String::new()
            } else {
                let len = fs_buf.iter().position(|&c| c == 0).unwrap_or(fs_buf.len());
                String::from_utf16_lossy(&fs_buf[..len])
            }
        };

        let used = total.saturating_sub(free);
        let usage_pct = if total > 0 {
            used as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        map.entry(disk_number).or_default().push(PartitionInfo {
            drive_letter: letter.to_string(),
            total_gb: total as f64 / 1_073_741_824.0,
            available_gb: free as f64 / 1_073_741_824.0,
            used_gb: used as f64 / 1_073_741_824.0,
            usage_percent: usage_pct,
            filesystem,
        });
    }
    map
}

/// 合理硬盘容量上限（64 TB），超过视为异常（防止 WMI Size 虚高值如 800TB 展示）
const MAX_REASONABLE_DISK_GB: f64 = 65536.0;

/// 计算某物理盘的准确容量：
/// 1) 优先用已挂载分区容量之和（GetDiskFreeSpaceExW，与资源管理器一致）；
/// 2) 无分区信息时回退 WMI Size；
/// 3) 对回退的 WMI 值做 64TB 上限钳制，明显异常返回 0。
fn resolve_disk_size_gb(wmi_size_bytes: u64, partition_total_gb: Option<f64>) -> f64 {
    if let Some(part) = partition_total_gb {
        if part > 0.0 {
            return part;
        }
    }
    let wmi_gb = wmi_size_bytes as f64 / 1_073_741_824.0;
    if wmi_gb > 0.0 && wmi_gb <= MAX_REASONABLE_DISK_GB {
        wmi_gb
    } else {
        0.0
    }
}

fn get_disk_health_info_inner() -> Result<DiskHealthResponse, String> {
    use crate::wmi_query::{self, v_str, v_u32, v_u64};

    // 1. 静态磁盘信息（WMI COM 直调，非 PowerShell）
    let rows = wmi_query::wmi_query(
        "SELECT Index, Model, Size, InterfaceType, SerialNumber, FirmwareRevision, MediaType, Status, PNPDeviceID FROM Win32_DiskDrive",
    )
    .map_err(|e| format!("WMI 获取磁盘信息失败: {}", e))?;

    if rows.is_empty() {
        return Ok(DiskHealthResponse {
            disks: vec![],
            total_count: 0,
            healthy_count: 0,
            warning_count: 0,
            unhealthy_count: 0,
        });
    }

    // 2. 分区表信息（Win32_DiskPartition，用于 GPT/MBR 与引导盘判定）
    let partition_rows = wmi_query::wmi_query(
        "SELECT DiskIndex, Type, BootPartition FROM Win32_DiskPartition",
    )
    .unwrap_or_default();
    let mut part_meta: std::collections::HashMap<u32, (bool, bool)> = std::collections::HashMap::new();
    for r in &partition_rows {
        if let Some(idx) = r.get("DiskIndex").and_then(|v| v_u32(v)) {
            let ty = r.get("Type").and_then(|v| v_str(v)).unwrap_or_default();
            let is_gpt = ty.to_uppercase().contains("GPT");
            let is_boot = r
                .get("BootPartition")
                .and_then(|v| v_str(v))
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let e = part_meta.entry(idx).or_insert((false, false));
            if is_gpt {
                e.0 = true;
            }
            if is_boot {
                e.1 = true;
            }
        }
    }

    // 3. 卷 → 物理盘映射（winapi）
    let volume_map = enumerate_volumes_by_disk();

    // 4. 每块盘：SMART 直读健康判定
    let mut disk_infos = Vec::new();
    let mut healthy = 0u32;
    let mut warning = 0u32;
    let mut unhealthy = 0u32;

    for (i, row) in rows.iter().enumerate() {
        let index = row.get("Index").and_then(|v| v_u32(v)).unwrap_or(i as u32);
        let model = row.get("Model").and_then(|v| v_str(v)).unwrap_or_else(|| "未知".to_string());
        let media_type = row.get("MediaType").and_then(|v| v_str(v)).unwrap_or_default();
        let wmi_size_gb = row.get("Size").and_then(|v| v_u64(v)).unwrap_or(0) as f64 / 1_073_741_824.0;
        let interface_type = row.get("InterfaceType").and_then(|v| v_str(v)).unwrap_or_else(|| "未知".to_string());
        let serial = row.get("SerialNumber").and_then(|v| v_str(v)).unwrap_or_default();
        let status = row.get("Status").and_then(|v| v_str(v)).unwrap_or_else(|| "未知".to_string());
        let pnp = row.get("PNPDeviceID").and_then(|v| v_str(v)).unwrap_or_default();

        // 判定 NVMe / SSD（与前端 isSsdMedia 逻辑保持一致）
        let is_nvme = pnp.to_lowercase().contains("nvme")
            || interface_type.to_lowercase().contains("nvme")
            || model.to_lowercase().contains("nvme");
        let is_ssd = is_nvme
            || media_type.to_lowercase().contains("ssd")
            || media_type.to_lowercase().contains("solid state");

        // 直读 SMART（CrystalDiskInfo 方案：ATA IOCTL 失败时回退 WMI）
        let smart = crate::smart::read_disk_smart(index, is_nvme, is_ssd, &model, &pnp);

        if !smart.has_smart {
            log::warn!(
                "[DiskHealth] PhysicalDrive{} ({}) 无法读取 SMART: {}",
                index,
                model,
                smart.error.as_deref().unwrap_or("未知错误")
            );
        } else {
            log::info!(
                "[DiskHealth] PhysicalDrive{} {} | NVMe={} | 状态={:?} | 健康度={:?} | 温度={:?}°C | 通电={:?}h",
                index,
                model,
                smart.is_nvme,
                smart.status,
                smart.life_percent,
                smart.temperature_c,
                smart.power_on_hours
            );
        }

        let health_status = smart.status.as_str().to_string();
        match smart.status {
            crate::smart::DiskStatus::Good => healthy += 1,
            crate::smart::DiskStatus::Caution => warning += 1,
            crate::smart::DiskStatus::Bad => unhealthy += 1,
            crate::smart::DiskStatus::Unknown => {}
        }
        let operational_status = match smart.status {
            crate::smart::DiskStatus::Good => "OK".to_string(),
            crate::smart::DiskStatus::Caution => "Degraded".to_string(),
            crate::smart::DiskStatus::Bad => "Failure".to_string(),
            crate::smart::DiskStatus::Unknown => "Unknown".to_string(),
        };

        let partitions = volume_map.get(&index).cloned().unwrap_or_default();
        let partition_count = partitions.len() as u32;
        let total_capacity_gb: f64 = partitions.iter().map(|p| p.total_gb).sum();
        let total_usage_gb: f64 = partitions.iter().map(|p| p.used_gb).sum();
        // 容量优先取分区容量之和（准确，与资源管理器一致），回退 WMI Size 并做上限钳制
        let size_gb = if total_capacity_gb > 0.0 {
            total_capacity_gb
        } else if wmi_size_gb > 0.0 && wmi_size_gb <= MAX_REASONABLE_DISK_GB {
            wmi_size_gb
        } else {
            0.0
        };

        let (is_gpt, is_boot) = part_meta.get(&index).copied().unwrap_or((false, false));
        let partition_style = if is_gpt {
            "GPT"
        } else if !partitions.is_empty() {
            "MBR"
        } else {
            "Unknown"
        }
        .to_string();

        disk_infos.push(DiskHealthInfo {
            index,
            model,
            media_type: if media_type.is_empty() { "Unknown".to_string() } else { media_type },
            size_gb,
            interface_type,
            health_status,
            operational_status,
            temperature_c: smart.temperature_c.map(|t| t as f64),
            wear_percentage: smart.life_percent.map(|p| p as f64),
            power_on_hours: smart.power_on_hours,
            power_on_count: smart.power_on_count,
            data_read_bytes: smart.data_read_bytes,
            data_written_bytes: smart.data_written_bytes,
            read_errors: None,
            write_errors: None,
            status,
            partition_count,
            serial_number: serial,
            partition_style,
            is_boot_disk: is_boot,
            partitions,
            total_usage_gb,
            total_capacity_gb,
            health_percent: smart.life_percent,
            // SMART 直读确认 NVMe → 必定 SSD；否则沿用 WMI 介质判定
            is_ssd: smart.is_nvme || is_ssd,
        });
    }

    Ok(DiskHealthResponse {
        disks: disk_infos,
        total_count: rows.len() as u32,
        healthy_count: healthy,
        warning_count: warning,
        unhealthy_count: unhealthy,
    })
}

#[tauri::command]
pub async fn get_disk_health_info() -> Result<DiskHealthResponse, String> {
    match tauri::async_runtime::spawn_blocking(|| get_disk_health_info_inner()).await {
        Ok(Ok(info)) => Ok(info),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(format!("异步任务失败: {}", e)),
    }
}

pub fn cleanup_hardware_cache() {
    let mut cache = STATIC_HARDWARE_CACHE.lock().unwrap();
    *cache = None;

    let mut cpu_system = CPU_SYSTEM.lock().unwrap();
    *cpu_system = None;
    
    log::info!("硬件信息缓存已清理");
}
