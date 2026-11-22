use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

use crate::audio_engine::{AudioEngine, BandParam, SoundEffectParams, get_device_name, get_device_id};
use windows::core::{GUID, HRESULT, PCWSTR, HSTRING, IUnknown, Interface};
use windows::Win32::Media::Audio as wa;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize,
    CLSCTX_ALL, COINIT_MULTITHREADED,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const FXVAD_SERVICE: &str = "FXVAD";

/// 驱动状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverStatus {
    pub installed: bool,
    pub service_exists: bool,
    pub service_running: bool,
    pub device_name: String,
    pub needs_reboot: bool,
}

/// 音频输出设备信息（用于 EQ 设备选择）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// EQ 频段信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    pub freq: f64,
    pub gain: f64,
}

/// FxSound 音效预设参数（0-1 范围）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxPresetParams {
    pub clarity: f64,
    pub ambience: f64,
    pub width: f64,
    pub dynamics: f64,
    pub bass: f64,
}

/// EQ 预设
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqPreset {
    pub id: String,
    pub name: String,
    pub bands: Vec<EqBand>,
    pub enabled: bool,
    #[serde(default)]
    pub effects: Option<FxPresetParams>,
}

/// EQ 引擎状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub running: bool,
    pub pid: Option<u32>,
}

/// 全局 AudioEngine 实例
static EQ_ENGINE: OnceLock<Mutex<Option<AudioEngine>>> = OnceLock::new();

fn eq_engine() -> &'static Mutex<Option<AudioEngine>> {
    EQ_ENGINE.get_or_init(|| Mutex::new(None))
}

/// 全局音效参数
static FX_PARAMS: OnceLock<Mutex<SoundEffectParams>> = OnceLock::new();

pub fn fx_params() -> &'static Mutex<SoundEffectParams> {
    FX_PARAMS.get_or_init(|| Mutex::new(SoundEffectParams::default()))
}

/// 更新音效参数
#[tauri::command]
pub fn update_eq_effects(clarity: f64, ambience: f64, width: f64, dynamics: f64, bass: f64) -> Result<(), String> {
    let mut params = fx_params().lock().map_err(|e| format!("锁错误: {}", e))?;
    params.clarity = clarity.clamp(0.0, 1.0);
    params.ambience = ambience.clamp(0.0, 1.0);
    params.width = width.clamp(0.0, 1.0);
    params.dynamics = dynamics.clamp(0.0, 1.0);
    params.bass = bass.clamp(0.0, 1.0);
    params.version = params.version.wrapping_add(1);
    log::info!("[eq] Effects updated: clarity={:.2} ambience={:.2} width={:.2} dynamics={:.2} bass={:.2}", params.clarity, params.ambience, params.width, params.dynamics, params.bass);
    Ok(())
}

/// 获取当前音效参数
#[tauri::command]
pub fn get_eq_effects() -> Result<SoundEffectParams, String> {
    let params = fx_params().lock().map_err(|e| format!("锁错误: {}", e))?;
    Ok(params.clone())
}

/// 获取 fxvad 资源目录
fn get_fxvad_resource_dir(app: &AppHandle) -> Option<PathBuf> {
    // 1. 通过 Tauri resource_dir 查找（生产环境 + 部分开发现境）
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidates = [
            resource_dir.join("binaries").join("fxvad"),
            resource_dir.join("resources").join("binaries").join("fxvad"),
            resource_dir.join("_up_").join("resources").join("binaries").join("fxvad"),
            resource_dir.join("_up_").join("_up_").join("src-tauri").join("resources").join("binaries").join("fxvad"),
        ];
        for path in &candidates {
            if path.exists() {
                return Some(path.clone());
            }
        }
    }

    // 2. 通过 exe 路径查找（开发环境：exe 在 src-tauri/target/debug/ 下）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join("binaries").join("fxvad"),
                parent.join("resources").join("binaries").join("fxvad"),
                // dev: target/debug -> src-tauri/resources/binaries/fxvad
                parent.join("..").join("..").join("resources").join("binaries").join("fxvad"),
                // dev: target/debug -> NexBox/src-tauri/resources/binaries/fxvad
                parent.join("..").join("..").join("..").join("src-tauri").join("resources").join("binaries").join("fxvad"),
            ];
            for path in &candidates {
                if path.exists() {
                    if let Ok(canon) = path.canonicalize() {
                        return Some(canon);
                    }
                    return Some(path.clone());
                }
            }
        }
    }

    // 3. 编译时路径（开发环境最可靠：CARGO_MANIFEST_DIR = src-tauri 目录）
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_path = manifest_dir.join("resources").join("binaries").join("fxvad");
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}

/// FXVAD 实例 ID 存档路径
fn fxvad_instance_id_file() -> PathBuf {
    let mut p = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push("NexBox");
    p.push("fxvad_instance_id.txt");
    p
}

/// 获取导入预设存储目录（%LOCALAPPDATA%/NexBox/EQEngine/presets）
fn get_user_presets_dir() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("NexBox");
    path.push("EQEngine");
    path.push("presets");
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

/// 解析 .fac 预设文件（含 FxSound 音效参数）
fn parse_fac_file(content: &str, file_id: &str) -> Option<EqPreset> {
    let mut name = String::new();
    let mut bands: Vec<EqBand> = Vec::new();
    let mut enabled = true;
    let mut in_bands = false;
    let mut current_freq: Option<f64> = None;

    // FxSound 音效参数（Main 0-5，MIDI 0-127）
    let mut main_vals: [f64; 6] = [0.0; 6];
    // 音效开关（Integer[0-4]，0=off, 1=on）
    let mut effect_on: [bool; 5] = [false; 5];
    // 行索引计数器（用于追踪 Integer[] 的索引）
    let mut int_count = 0u32;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // 第3行是预设名称（索引2）
        if line_idx == 2 {
            name = trimmed.to_string();
            continue;
        }

        // ── FxSound Main 参数（行6-11，索引5-10）──
        if (5..=10).contains(&line_idx) {
            // 格式 "   50: Main 0" → 提取 50
            if let Some(val_str) = trimmed.split(':').next() {
                if let Ok(val) = val_str.trim().parse::<f64>() {
                    let idx = line_idx - 5;
                    if idx < 6 {
                        main_vals[idx] = val;
                    }
                }
            }
            continue;
        }

        // ── FxSound Integer[] 开关（行23-29，索引22-28）──
        if (22..=28).contains(&line_idx) {
            // 格式 "   1: Integer[0]" → 提取 1
            if let Some(val_str) = trimmed.split(':').next() {
                let val = val_str.trim().parse::<i32>().unwrap_or(0);
                if int_count < 5 {
                    effect_on[int_count as usize] = val != 0;
                }
                int_count += 1;
            }
            continue;
        }

        // 检测 On/Off Flag
        if trimmed.contains("On/Off Flag") {
            if let Some(val_str) = trimmed.split(':').next() {
                if let Ok(val) = val_str.trim().parse::<i32>() {
                    enabled = val != 0;
                }
            }
            in_bands = true;
            continue;
        }

        if in_bands {
            // 匹配 "   62.5: CF" 格式
            if trimmed.ends_with(": CF") || trimmed.ends_with(":CF") {
                let freq_str = trimmed.split(':').next().unwrap_or("").trim();
                if let Ok(freq) = freq_str.parse::<f64>() {
                    current_freq = Some(freq);
                }
            }
            // 匹配 "   0: Boost/Cut" 格式
            else if trimmed.contains("Boost/Cut") {
                if let Some(freq) = current_freq {
                    let gain_str = trimmed.split(':').next().unwrap_or("").trim();
                    if let Ok(gain) = gain_str.parse::<f64>() {
                        bands.push(EqBand { freq, gain });
                    }
                    current_freq = None;
                }
            }
        }
    }

    if name.is_empty() {
        name = format!("Preset {}", file_id);
    }

    // 构建音效参数：MIDI 0-127 → 0.0-1.0
    let has_effects = main_vals.iter().any(|&v| v > 0.0);
    let effects = if has_effects {
        Some(FxPresetParams {
            clarity: main_vals[0] / 127.0,
            width:   main_vals[1] / 127.0,
            // main_vals[2] 未使用（Main 2 = 0）
            ambience: main_vals[3] / 127.0,
            dynamics: main_vals[4] / 127.0,
            bass:     main_vals[5] / 127.0,
        })
    } else {
        None
    };

    let _ = effect_on; // 开关状态保留以备后续使用

    Some(EqPreset {
        id: file_id.to_string(),
        name,
        bands,
        enabled,
        effects,
    })
}

/// 检查虚拟声卡驱动状态
#[tauri::command]
pub fn check_virtual_audio_driver() -> Result<DriverStatus, String> {
    let mut service_exists = false;
    let mut service_running = false;

    // 使用 sc query 检查服务
    if let Ok(output) = Command::new("sc")
        .args(["query", FXVAD_SERVICE])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        // sc query 成功 (exit code 0) 才说明服务存在
        // 服务不存在时 exit code 非零，输出包含错误码 1060
        let cmd_success = output.status.success();
        let is_error_1060 = combined.contains("1060")
            && (combined.contains("does not exist")
                || combined.contains("不存在")
                || combined.contains("未安装")
                || combined.contains("FAILED")
                || combined.contains("失败"));

        if cmd_success || (combined.to_lowercase().contains("fxvad") && !is_error_1060) {
            service_exists = true;
            if combined.contains("RUNNING") {
                service_running = true;
            }
        }
    }

    // 检测是否需要重启：服务存在但启动类型已设为"禁用"（说明之前卸载过）
    let mut needs_reboot = false;
    if service_exists {
        if let Ok(output) = Command::new("sc")
            .args(["qc", FXVAD_SERVICE])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("DISABLED") || stdout.contains("禁用") {
                needs_reboot = true;
                service_exists = false; // 已禁用视为不存在
            }
        }
    }

    // 驱动是否已安装：仅看服务记录是否存在（服务不在 = 已卸载）
    // DriverStore 残留不影响判定，不需要额外检查 pnputil

    Ok(DriverStatus {
        installed: service_exists,
        service_exists,
        service_running,
        device_name: "FxSound Audio Enhancer".to_string(),
        needs_reboot,
    })
}

/// 安装虚拟声卡驱动（零 PowerShell，原生 SetupAPI）
#[tauri::command]
pub async fn install_virtual_audio_driver(app: AppHandle) -> Result<String, String> {
    if !is_admin() {
        return Err("需要管理员权限才能安装驱动。请以管理员身份运行 NexBox".to_string());
    }

    let fxvad_dir = get_fxvad_resource_dir(&app)
        .ok_or_else(|| "无法找到驱动资源目录".to_string())?;

    let inf_path = fxvad_dir.join("fxvad.inf");
    let sys_path = fxvad_dir.join("fxvad.sys");
    let devcon_path = fxvad_dir.join("fxdevcon64.exe");

    if !inf_path.exists() {
        return Err("驱动 INF 文件不存在".to_string());
    }
    if !sys_path.exists() {
        return Err("驱动 SYS 文件不存在".to_string());
    }
    if !devcon_path.exists() {
        return Err("fxdevcon64.exe 不存在，请重新安装 NexBox".to_string());
    }

    // 安装前记录当前默认音频设备
    let prev_device = run_com_thread(|| {
        unsafe {
            let enumerator: wa::IMMDeviceEnumerator = CoCreateInstance(
                &wa::MMDeviceEnumerator,
                None,
                CLSCTX_ALL,
            ).map_err(|e| format!("CoCreateInstance MMDeviceEnumerator failed: {}", e))?;
            native_get_default_name(&enumerator)
                .ok_or_else(|| "无法获取当前默认设备".to_string())
        }
    }).unwrap_or_default();

    // Step 1: 复制驱动文件到 system32\drivers
    let sys_dst = "C:\\Windows\\System32\\drivers\\fxvad.sys";
    if std::path::Path::new(sys_dst).exists() {
        log::info!("[install] fxvad.sys already exists in system32\\drivers");
    } else {
        fs::copy(&sys_path, sys_dst)
            .map_err(|e| format!("复制 fxvad.sys 失败: {}", e))?;
        log::info!("[install] Copied fxvad.sys to system32\\drivers");
    }

    // Step 2: pnputil 注册到 Driver Store
    log::info!("[install] Running pnputil /add-driver ...");
    let pnputil_output = Command::new("pnputil")
        .args(["/add-driver", inf_path.to_string_lossy().as_ref(), "/install"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("启动 pnputil 失败: {}", e))?;

    if !pnputil_output.status.success() {
        let msg = String::from_utf8_lossy(&pnputil_output.stdout);
        log::warn!("[install] pnputil 输出: {}", msg);
        // pnputil 可能因为驱动已存在而失败，不中断流程
    }

    // Step 3: fxdevcon64.exe 创建设备并安装驱动
    log::info!("[install] Running fxdevcon64.exe install ...");
    let devcon_output = Command::new(devcon_path.to_string_lossy().as_ref())
        .args(["install", inf_path.to_string_lossy().as_ref(), "Root\\FXVAD"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("启动 fxdevcon64.exe 失败: {}", e))?;

    if !devcon_output.status.success() {
        let stdout = String::from_utf8_lossy(&devcon_output.stdout).trim().to_string();
        log::warn!("[install] fxdevcon64.exe 退出码非零: code={}, output={}",
            devcon_output.status.code().unwrap_or(-1), stdout);
        // fxdevcon64 可能因需要重启而返回 -1，但驱动可能已装好
        // 以实际驱动状态为准，不中断流程
    }

    // 等待驱动注册完成
    std::thread::sleep(std::time::Duration::from_secs(3));

    // 如果系统自动切换到了 FxSound 虚拟声卡，恢复为之前的物理设备
    if !prev_device.is_empty() && !prev_device.to_lowercase().contains("fxsound") {
        log::info!("[install] Restoring default audio device to: {}", prev_device);
        restore_default_audio_device(&prev_device);
    }

    // 验证
    let status = check_virtual_audio_driver()?;
    if status.installed {
        Ok("虚拟声卡驱动安装成功".to_string())
    } else {
        Err("驱动已安装但服务未启动，可能需要重启电脑".to_string())
    }
}

/// 原生移除 FXVAD 设备节点（被 fxdevcon64.exe 替代，保留为备用）
fn native_remove_fxvad_devnode() -> Result<bool, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Locate_DevNodeW, CM_Uninstall_DevNode, CM_LOCATE_DEVNODE_NORMAL, CONFIGRET,
    };

    let cr_success = CONFIGRET(0);

    // 优先用安装时保存的精确实例 ID
    if let Ok(saved) = fs::read_to_string(fxvad_instance_id_file()) {
        let id = saved.trim().to_string();
        if !id.is_empty() {
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                let mut devinst = 0u32;
                let cr = CM_Locate_DevNodeW(
                    &mut devinst,
                    PCWSTR(wide.as_ptr()),
                    CM_LOCATE_DEVNODE_NORMAL,
                );
                if cr == cr_success {
                    let cr2 = CM_Uninstall_DevNode(devinst, 0);
                    log::info!("[uninstall] CM_Uninstall_DevNode({}) -> {:?}", id, cr2);
                    if cr2 == cr_success {
                        let _ = fs::remove_file(fxvad_instance_id_file());
                        return Ok(true);
                    }
                }
                log::warn!("[uninstall] Saved instance id '{}' not found (CR={:?}), falling back to enum", id, cr);
            }
        }
    }

    // 回退：枚举 ROOT 下所有设备，按硬件 ID 匹配 FXVAD 后移除
    native_find_and_remove_by_hwid("FXVAD")
}

/// 回退方案：枚举 ROOT 枚举器下所有设备，按硬件 ID 关键词匹配后 DIF_REMOVE
fn native_find_and_remove_by_hwid(hwid_keyword: &str) -> Result<bool, String> {
    use windows::core::w;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiGetClassDevsW, SetupDiEnumDeviceInfo, SetupDiGetDeviceRegistryPropertyW,
        SetupDiCallClassInstaller, SetupDiDestroyDeviceInfoList,
        SP_DEVINFO_DATA, DIGCF_PRESENT, DIGCF_ALLCLASSES, SPDRP_HARDWAREID, DIF_REMOVE,
    };

    unsafe {
        let dev_info_set = SetupDiGetClassDevsW(None, w!("ROOT"), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
            .map_err(|e| format!("SetupDiGetClassDevsW failed: {}", e))?;

        let mut index = 0u32;
        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(dev_info_set, index, &mut data).is_err() {
                break;
            }

            let mut buf = [0u16; 512];
            let mut required = 0u32;
            let bytes = std::slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut u8,
                buf.len() * 2,
            );
            let got = SetupDiGetDeviceRegistryPropertyW(
                dev_info_set,
                &data,
                SPDRP_HARDWAREID,
                None,
                Some(bytes),
                Some(&mut required),
            ).is_ok();

            if got && String::from_utf16_lossy(&buf).to_lowercase().contains(&hwid_keyword.to_lowercase()) {
                let ok = SetupDiCallClassInstaller(DIF_REMOVE, dev_info_set, Some(&data)).is_ok();
                log::info!("[uninstall] DIF_REMOVE by HWID match -> {}", ok);
                let _ = SetupDiDestroyDeviceInfoList(dev_info_set);
                let _ = fs::remove_file(fxvad_instance_id_file());
                return Ok(ok);
            }
            index += 1;
        }
        let _ = SetupDiDestroyDeviceInfoList(dev_info_set);
    }
    log::info!("[uninstall] No FXVAD devnode found via HWID enum, may already be gone");
    Ok(false)
}

/// 检查当前进程是否具有管理员权限（通过 UAC 令牌提升状态判断）
fn is_admin() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            log::warn!("[is_admin] OpenProcessToken failed");
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;

        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut std::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        );

        let _ = CloseHandle(token);

        if result.is_err() {
            log::warn!("[is_admin] GetTokenInformation failed: {:?}", result);
            return false;
        }

        log::info!("[is_admin] TokenElevation: {}", elevation.TokenIsElevated != 0);
        elevation.TokenIsElevated != 0
    }
}

/// 卸载虚拟声卡驱动（原生移除设备节点 → 清理驱动包，无 PowerShell/devcon）
#[tauri::command]
pub async fn uninstall_virtual_audio_driver(_app: AppHandle) -> Result<String, String> {
    if !is_admin() {
        return Err("需要管理员权限才能卸载驱动。请以管理员身份运行 NexBox".to_string());
    }

    let _ = stop_eq_engine();

    // 1. 【关键】先原生移除设备节点——原子操作，内部正确停止驱动实例
    //    不需要也不能提前 sc config disabled，否则制造 Code 32 孤儿节点
    match native_remove_fxvad_devnode() {
        Ok(true) => log::info!("[uninstall] Devnode removed via native API"),
        Ok(false) => log::info!("[uninstall] Devnode not found, may already be gone"),
        Err(e) => log::error!("[uninstall] Devnode removal error: {}", e),
    }

    // 2. 设备节点已摘除，现在安全删除驱动包
    let pnp_output = run_hidden_output("pnputil", &["/enum-drivers"]);
    let oem_list = find_all_oem_infs(&pnp_output, "fxvad");
    log::info!("[uninstall] Found {} FXVAD OEM INF(s): {:?}", oem_list.len(), oem_list);
    for oem in &oem_list {
        let del_out = run_hidden_output("pnputil", &["/delete-driver", oem, "/uninstall", "/force"]);
        log::info!("[uninstall] pnputil /delete-driver {}: {}", oem, del_out.trim());
    }

    // 3. 保险清理：删除服务项、.sys 文件
    run_hidden_command("sc", &["delete", "FXVAD"]);
    let sys_path = std::path::Path::new(r"C:\Windows\System32\drivers\fxvad.sys");
    if sys_path.exists()
        && fs::remove_file(sys_path).is_err() {
            schedule_reboot_delete(sys_path);
        }

    // 4. 清理实例 ID 存档
    let _ = fs::remove_file(fxvad_instance_id_file());

    // 5. 验证
    let sc_check = run_hidden_output("sc", &["query", "FXVAD"]);
    let check_pnp = run_hidden_output("pnputil", &["/enum-drivers"]);
    log::info!(
        "[uninstall] Verify: service_1060={}, pnp_fxvad={}",
        sc_check.contains("1060"),
        check_pnp.to_lowercase().contains("fxvad")
    );
    Ok("虚拟声卡驱动已卸载".to_string())
}

/// 运行隐藏窗口命令，忽略结果
fn run_hidden_command(cmd: &str, args: &[&str]) {
    let _ = Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// 运行隐藏窗口命令，返回 stdout 字符串
fn run_hidden_output(cmd: &str, args: &[&str]) -> String {
    match Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(e) => {
            log::warn!("[uninstall] Command failed: {} {}: {}", cmd, args.join(" "), e);
            String::new()
        }
    }
}

/// 从 pnputil /enum-drivers 输出中找到所有 fxvad/fxsound 对应的 OEM 文件名
fn find_all_oem_infs(pnp_output: &str, keyword: &str) -> Vec<String> {
    let mut results = Vec::new();
    // 方法1: 搜索包含关键词的 OEM INF
    let re = match regex::bytes::Regex::new(r"(?m)^(?:Published Name|发布名称):\s+(oem\d+\.inf)\s*$") {
        Ok(r) => r,
        Err(_) => return results,
    };
    for caps in re.captures_iter(pnp_output.as_bytes()) {
        let oem = String::from_utf8_lossy(&caps[1]).to_string();
        // 在找到的 oem.inf 行后面搜索关键词
        let start = caps.get(0).unwrap().end();
        let end = (start + 500).min(pnp_output.len());
        let tail = &pnp_output[start..end];
        if tail.to_lowercase().contains(&keyword.to_lowercase())
            && !results.contains(&oem) {
                results.push(oem);
            }
    }
    // 方法2（兜底）: 直接搜索整个输出中的 oemNN.inf 及其所在区块
    if results.is_empty() {
        let re2 = match regex::bytes::Regex::new(r"(?m)^(?:Published Name|发布名称):\s+(oem\d+\.inf)\s*$") {
            Ok(r) => r,
            Err(_) => return results,
        };
        let bytes = pnp_output.as_bytes();
        let mut all_oems: Vec<(usize, String)> = Vec::new();
        for caps in re2.captures_iter(bytes) {
            let oem = String::from_utf8_lossy(&caps[1]).to_string();
            let pos = caps.get(0).unwrap().end();
            all_oems.push((pos, oem));
        }
        // 为每个 oem 检查其 500 字节范围内是否含有关键词
        for i in 0..all_oems.len() {
            let (start, ref oem) = all_oems[i];
            let end_block = if i + 1 < all_oems.len() {
                all_oems[i + 1].0 - 50
            } else {
                (start + 500).min(pnp_output.len())
            };
            let chunk = &pnp_output[start..end_block.min(pnp_output.len())];
            if chunk.to_lowercase().contains(&keyword.to_lowercase()) && !results.contains(oem) {
                results.push(oem.clone());
            }
        }
    }
    results
}

/// 安排文件在重启后删除 (MoveFileEx with MOVEFILE_DELAY_UNTIL_REBOOT)
fn schedule_reboot_delete(path: &std::path::Path) {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::MoveFileExW;
    use windows::Win32::Storage::FileSystem::MOVEFILE_DELAY_UNTIL_REBOOT;

    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let _ = MoveFileExW(
            PCWSTR::from_raw(wide.as_ptr()),
            None,
            MOVEFILE_DELAY_UNTIL_REBOOT,
        );
    }
}

/// 启动 EQ 引擎（原生 WASAPI），并自动切换默认音频设备到 FxSound
/// `device_name` 可选：指定要应用 EQ 的物理输出设备名称；
/// 未指定时使用启动前的系统默认设备。
#[tauri::command]
pub async fn start_eq_engine(_app: AppHandle, device_name: Option<String>) -> Result<String, String> {
    // 检查是否已在运行
    {
        let guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
        if let Some(ref engine) = *guard {
            if engine.is_running() {
                return Err("EQ 引擎已在运行".to_string());
            }
        }
    }

    // 检查驱动是否已安装
    let driver_status = check_virtual_audio_driver()?;
    if !driver_status.installed {
        return Err("请先安装虚拟声卡驱动".to_string());
    }

    // 保存当前默认设备，切换到 FxSound 虚拟声卡（原生 COM，无 PowerShell）
    let prev_device = switch_default_audio_to_fxsound()?;
    // 用户指定了物理设备则优先使用，否则使用启动前的默认设备
    let physical_device_name = if let Some(name) = device_name {
        let name = name.trim().to_string();
        if name.is_empty() {
            prev_device.strip_prefix("PREV:").unwrap_or("").to_string()
        } else {
            log::info!("[eq] Using user-selected physical device: '{}'", name);
            // 将用户选择的设备写入临时文件，供停止时恢复默认设备
            let prev_file = std::env::temp_dir().join("nexbox_eq_prev_device.txt");
            let _ = fs::write(&prev_file, &name);
            name
        }
    } else if prev_device.starts_with("PREV:") {
        let name = prev_device.strip_prefix("PREV:").unwrap_or("").to_string();
        log::info!("[eq] Previous device name: '{}'", name);
        name
    } else {
        String::new()
    };

    // Wait for default device switch to fully propagate before starting audio engine
    std::thread::sleep(std::time::Duration::from_millis(200));

    // 启动原生音频引擎
    let engine = match AudioEngine::start(physical_device_name.clone()) {
        Ok(e) => e,
        Err(e) => {
            // 引擎启动失败，恢复原始默认设备
            log::error!("[eq] Audio engine failed to start: {}", e);
            if !physical_device_name.is_empty() {
                log::info!("[eq] Restoring audio device to: {}", physical_device_name);
                restore_default_audio_device(&physical_device_name);
            }
            let _ = fs::remove_file(std::env::temp_dir().join("nexbox_eq_prev_device.txt"));
            return Err(e);
        }
    };

    // Store the engine
    {
        let mut guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
        *guard = Some(engine);
    }

    // 等待引擎初始化
    std::thread::sleep(std::time::Duration::from_millis(300));

    Ok(prev_device)
}

// ===== 音频设备切换功能（原生 COM，无 PowerShell） =====
// 直接使用 Windows IPolicyConfig COM 接口切换默认音频设备
// 这是 SoundVolumeView、EarTrumpet、SoundSwitch 等工具背后的同一接口

/// IPolicyConfig COM 接口（Windows 非公开但稳定的 API）
/// CLSID: {870AF99C-171D-4F9E-AF0D-E63DF40C2BC9}
/// IID:   {F8679F50-850A-41CF-9C72-430F290290C8}
const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
const IID_IPOLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

/// IUnknown vtable（COM 接口的头 3 个方法）
#[repr(C)]
struct IUnknownVtbl {
    query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
}

/// IPolicyConfig vtable（IUnknown + 12 个方法）
#[repr(C)]
struct IPolicyConfigVtbl {
    // IUnknown
    query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
    // IPolicyConfig
    get_mix_format: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *mut *mut std::ffi::c_void) -> HRESULT,
    get_device_format: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, i32, *mut *mut std::ffi::c_void) -> HRESULT,
    reset_device_format: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR) -> HRESULT,
    set_device_format: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *mut std::ffi::c_void, *mut std::ffi::c_void) -> HRESULT,
    get_processing_period: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, i32, *mut i64, *mut i64) -> HRESULT,
    set_processing_period: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *mut i64) -> HRESULT,
    get_share_mode: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *mut std::ffi::c_void) -> HRESULT,
    set_share_mode: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *mut std::ffi::c_void) -> HRESULT,
    get_property_value: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *const std::ffi::c_void, *mut std::ffi::c_void) -> HRESULT,
    set_property_value: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, *const std::ffi::c_void, *mut std::ffi::c_void) -> HRESULT,
    set_default_endpoint: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, i32) -> HRESULT,
    set_endpoint_visibility: unsafe extern "system" fn(*mut std::ffi::c_void, PCWSTR, i32) -> HRESULT,
}

#[repr(C)]
struct IPolicyConfigCom {
    vtbl: *const IPolicyConfigVtbl,
}

/// 设置默认音频设备（三个 role 全部设置：eConsole / eMultimedia / eCommunications）
fn native_set_default_device(device_id: &str) -> Result<(), String> {
    unsafe {
        // 1. 创建 IPolicyConfig COM 实例（通过 IUnknown）
        let unknown: IUnknown = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)
            .map_err(|e| format!("CoCreateInstance IPolicyConfig failed: {}", e))?;

        // 2. QueryInterface 获取 IPolicyConfig 接口指针
        let raw = unknown.as_raw();
        let unk_vtbl = *(raw as *const *const IUnknownVtbl);
        let mut policy_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let hr = ((*unk_vtbl).query_interface)(raw, &IID_IPOLICY_CONFIG, &mut policy_ptr);
        if hr.is_err() {
            return Err(format!("QueryInterface IPolicyConfig failed: {:?}", hr));
        }

        let policy = &*(policy_ptr as *const IPolicyConfigCom);
        let id = HSTRING::from(device_id);
        let pcwstr = PCWSTR(id.as_ptr());

        // 3. 三个 role 都要设置
        for role in [0i32, 1, 2] {
            let hr = ((*policy.vtbl).set_default_endpoint)(policy_ptr, pcwstr, role);
            if hr.is_err() {
                ((*policy.vtbl).release)(policy_ptr);
                return Err(format!("SetDefaultEndpoint failed (role {}): {:?}", role, hr));
            }
        }

        // 4. Release
        ((*policy.vtbl).release)(policy_ptr);
    }
    Ok(())
}

/// 查找渲染设备（名称包含匹配，大小写不敏感）
/// 返回 (device_id, device_name)
fn native_find_device(enumerator: &wa::IMMDeviceEnumerator, name_contains: &str) -> Option<(String, String)> {
    unsafe {
        let collection = enumerator.EnumAudioEndpoints(wa::eRender, wa::DEVICE_STATE_ACTIVE).ok()?;
        let count = collection.GetCount().ok()?;
        let search = name_contains.to_lowercase();
        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                let id = get_device_id(&device).unwrap_or_default();
                let name = get_device_name(&device);
                if name.to_lowercase().contains(&search) {
                    return Some((id, name));
                }
            }
        }
    }
    None
}

/// 获取当前默认渲染设备名称
fn native_get_default_name(enumerator: &wa::IMMDeviceEnumerator) -> Option<String> {
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(wa::eRender, wa::eConsole).ok()?;
        Some(get_device_name(&device))
    }
}

/// 在独立线程上运行 COM 操作（确保 COM 初始化正确）
fn run_com_thread<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let handle = std::thread::spawn(move || -> Result<T, String> {
        unsafe {
            let co_hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if co_hr.is_err() {
                return Err(format!("CoInitializeEx failed: {:?}", co_hr));
            }
            let result = f();
            CoUninitialize();
            result
        }
    });
    handle.join().map_err(|_| "COM thread panicked".to_string())?
}

/// 切换 Windows 默认音频播放设备到 FxSound Audio Enhancer
/// 返回格式 "PREV:设备名"，包含切换前的默认设备名
/// 同时将前一个设备名写入临时文件（供 stop_eq_engine 恢复用）
fn switch_default_audio_to_fxsound() -> Result<String, String> {
    run_com_thread(|| {
        let enumerator: wa::IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&wa::MMDeviceEnumerator, None, CLSCTX_ALL)
        }.map_err(|e| format!("CoCreateInstance MMDeviceEnumerator failed: {}", e))?;

        // 获取当前默认设备名（保存供恢复用）
        let prev_name = native_get_default_name(&enumerator).unwrap_or_default();
        log::info!("[audio_switch] Previous default device: '{}'", prev_name);

        // 查找 FxSound 设备
        let (fxsound_id, fxsound_name) = native_find_device(&enumerator, "fxsound")
            .ok_or_else(|| "找不到 FxSound Audio Enhancer 设备，请确认驱动已安装并在声音设置中可见".to_string())?;
        log::info!("[audio_switch] Found FxSound: '{}' (id={})", fxsound_name, fxsound_id);

        // 切换到 FxSound
        native_set_default_device(&fxsound_id)?;
        log::info!("[audio_switch] Switched default device to FxSound");

        // 保存前一个设备名到文件（Rust 原生写入，UTF-8 无 BOM）
        let prev_file = std::env::temp_dir().join("nexbox_eq_prev_device.txt");
        let _ = fs::write(&prev_file, &prev_name);

        Ok(format!("PREV:{}", prev_name))
    })
}

/// 恢复 Windows 默认音频设备
fn restore_default_audio_device(device_name: &str) {
    let name = device_name.to_string();
    let result = run_com_thread(move || {
        let enumerator: wa::IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&wa::MMDeviceEnumerator, None, CLSCTX_ALL)
        }.map_err(|e| format!("CoCreateInstance failed: {}", e))?;

        // 按名称查找设备
        if let Some((device_id, found_name)) = native_find_device(&enumerator, &name) {
            log::info!("[audio_restore] Found device '{}' (id={}), switching...", found_name, device_id);
            native_set_default_device(&device_id)?;
            log::info!("[audio_restore] Restored to: {}", found_name);
        } else {
            log::warn!("[audio_restore] Device '{}' not found", name);
        }
        Ok(())
    });

    if let Err(e) = result {
        log::warn!("[audio_restore] Failed: {}", e);
    }
}

/// 获取当前默认音频输出设备名称
#[tauri::command]
pub fn get_default_audio_device() -> Result<String, String> {
    run_com_thread(|| {
        let enumerator: wa::IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&wa::MMDeviceEnumerator, None, CLSCTX_ALL)
        }.map_err(|e| format!("CoCreateInstance failed: {}", e))?;

        native_get_default_name(&enumerator)
            .ok_or_else(|| "No default audio device found".to_string())
    })
}

/// 枚举所有活动的音频输出设备（排除 FxSound 虚拟声卡）
/// 返回设备列表，其中 is_default 标记当前默认设备
#[tauri::command]
pub fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    run_com_thread(|| {
        let enumerator: wa::IMMDeviceEnumerator = unsafe {
            CoCreateInstance(&wa::MMDeviceEnumerator, None, CLSCTX_ALL)
        }.map_err(|e| format!("CoCreateInstance failed: {}", e))?;

        unsafe {
            let collection = enumerator
                .EnumAudioEndpoints(wa::eRender, wa::DEVICE_STATE_ACTIVE)
                .map_err(|e| format!("EnumAudioEndpoints failed: {}", e))?;
            let count = collection
                .GetCount()
                .map_err(|e| format!("GetCount failed: {}", e))?;

            let default_id = enumerator
                .GetDefaultAudioEndpoint(wa::eRender, wa::eConsole)
                .ok()
                .and_then(|d| get_device_id(&d).ok())
                .unwrap_or_default();

            let mut devices = Vec::new();
            for i in 0..count {
                let device = match collection.Item(i) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let id = get_device_id(&device).unwrap_or_default();
                let name = get_device_name(&device);
                // 排除 FxSound 虚拟声卡
                if name.to_lowercase().contains("fxsound") {
                    continue;
                }
                let is_default = id == default_id;
                devices.push(AudioDevice {
                    id,
                    name,
                    is_default,
                });
            }
            log::info!("[eq] Listed {} audio devices", devices.len());
            Ok(devices)
        }
    })
}

/// 停止 EQ 引擎
#[tauri::command]
pub fn stop_eq_engine() -> Result<(), String> {
    // 停止音频引擎
    {
        let mut guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
        if let Some(mut engine) = guard.take() {
            engine.stop();
            log::info!("[eq] Audio engine stopped");
        }
    }

    // 恢复原始默认音频设备
    let prev_file = std::env::temp_dir().join("nexbox_eq_prev_device.txt");
    match fs::read_to_string(&prev_file) {
        Ok(raw) => {
            let prev_name = raw.trim_start_matches('\u{FEFF}').trim().to_string();
            if !prev_name.is_empty() && !prev_name.to_lowercase().contains("fxsound") {
                log::info!("[eq] Restoring audio device to: {}", prev_name);
                restore_default_audio_device(&prev_name);
            } else if prev_name.to_lowercase().contains("fxsound") {
                log::info!("[eq] Previous device was FxSound, skipping restore");
            }
            let _ = fs::remove_file(&prev_file);
        }
        Err(_) => {
            log::info!("[eq] No previous device file found, skipping restore");
        }
    }

    Ok(())
}

/// 获取 EQ 引擎运行状态
#[tauri::command]
pub fn get_eq_engine_status() -> Result<EngineStatus, String> {
    let guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
    if let Some(ref engine) = *guard {
        if engine.is_running() {
            return Ok(EngineStatus {
                running: true,
                pid: None,
            });
        }
    }

    Ok(EngineStatus {
        running: false,
        pid: None,
    })
}

/// 实时更新 EQ 频段增益（无需重启引擎）
#[tauri::command]
pub fn update_eq_bands(bands: Vec<(f64, f64)>) -> Result<(), String> {
    let guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
    if let Some(ref engine) = *guard {
        let band_params: Vec<BandParam> = bands
            .iter()
            .map(|(freq, gain)| BandParam { freq: *freq, gain: *gain })
            .collect();
        engine.update_bands(band_params);
        log::info!("[eq] Updated {} bands", bands.len());
    }
    Ok(())
}

/// 设置总增益 (-12..+12 dB)
#[tauri::command]
pub fn update_eq_preamp(gain: f64) -> Result<(), String> {
    let guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
    if let Some(ref engine) = *guard {
        engine.set_preamp(gain);
        log::info!("[eq] Preamp set to {} dB", gain);
    }
    Ok(())
}

/// 获取所有 EQ 预设（内置 + 用户导入）
#[tauri::command]
pub fn get_eq_presets(app: AppHandle) -> Result<Vec<EqPreset>, String> {
    let mut presets: Vec<EqPreset> = Vec::new();

    // 1. 加载内置预设
    if let Some(fxvad_dir) = get_fxvad_resource_dir(&app) {
        let builtin_dir = fxvad_dir.join("presets");
        load_presets_from_dir(&builtin_dir, &mut presets);
    }

    // 2. 加载用户导入的预设
    let user_dir = get_user_presets_dir();
    load_presets_from_dir(&user_dir, &mut presets);


    // 按 ID 排序
    presets.sort_by_key(|p| p.id.parse::<i32>().unwrap_or(999));

    Ok(presets)
}

fn load_presets_from_dir(dir: &PathBuf, presets: &mut Vec<EqPreset>) {
    if !dir.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    files.sort_by_key(|e| {
        e.file_name()
            .to_string_lossy()
            .trim_end_matches(".fac")
            .parse::<i32>()
            .unwrap_or(999)
    });
    for entry in files {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("fac") {
            continue;
        }
        let file_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        // 跳过已存在的同名 ID（内置优先）
        if presets.iter().any(|p| p.id == file_id) {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&path) {
            if let Some(preset) = parse_fac_file(&content, &file_id) {
                presets.push(preset);
            }
        }
    }
}

/// 应用 EQ 预设（更新引擎中的频段参数），搜索内置和用户两个目录
#[tauri::command]
pub fn apply_eq_preset(app: AppHandle, preset_id: String) -> Result<(), String> {
    let mut content: Option<String> = None;

    // 先查内置目录
    if let Some(fxvad_dir) = get_fxvad_resource_dir(&app) {
        let f = fxvad_dir.join("presets").join(format!("{}.fac", preset_id));
        if f.exists() {
            content = Some(fs::read_to_string(&f).map_err(|e| format!("读取预设失败: {}", e))?);
        }
    }

    // 再查用户目录
    if content.is_none() {
        let f = get_user_presets_dir().join(format!("{}.fac", preset_id));
        if f.exists() {
            content = Some(fs::read_to_string(&f).map_err(|e| format!("读取预设失败: {}", e))?);
        }
    }

    let content = content.ok_or_else(|| format!("预设 {} 不存在", preset_id))?;
    let preset = parse_fac_file(&content, &preset_id);

    if let Some(p) = &preset {
        let guard = eq_engine().lock().map_err(|e| format!("锁错误: {}", e))?;
        if let Some(ref engine) = *guard {
            let band_params: Vec<BandParam> = p.bands
                .iter()
                .map(|b| BandParam { freq: b.freq, gain: b.gain })
                .collect();
            engine.update_bands(band_params);
            log::info!("[eq] Applied preset '{}' with {} bands", p.name, p.bands.len());
        }
    }

    Ok(())
}

/// 导入 FAC 预设文件，返回解析后的预设（保存到 %LOCALAPPDATA%/NexBox/EQEngine/presets/）
#[tauri::command]
pub fn import_eq_preset(app: AppHandle, content: String) -> Result<EqPreset, String> {
    let presets_dir = get_user_presets_dir();

    // 找到最大 ID + 1，同时扫描内置和用户两个目录，确保 ID 不冲突
    let mut max_id = 0;
    let mut scan_ids = |dir: &PathBuf| {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(stem) = entry.path().file_stem() {
                    if let Some(s) = stem.to_str() {
                        if let Ok(id) = s.parse::<u32>() {
                            max_id = max_id.max(id);
                        }
                    }
                }
            }
        }
    };
    scan_ids(&presets_dir);
    if let Some(fxvad_dir) = get_fxvad_resource_dir(&app) {
        scan_ids(&fxvad_dir.join("presets"));
    }
    let new_id = (max_id + 1).to_string();

    // 写入 .fac 文件
    let new_path = presets_dir.join(format!("{}.fac", new_id));
    fs::write(&new_path, &content)
        .map_err(|e| format!("写入预设文件失败: {}", e))?;

    // 解析并返回
    parse_fac_file(&content, &new_id)
        .ok_or_else(|| "解析预设文件失败".to_string())
}

/// 删除导入的 FAC 预设文件（从用户目录）
#[tauri::command]
pub fn delete_eq_preset(preset_id: String) -> Result<(), String> {
    let preset_file = get_user_presets_dir().join(format!("{}.fac", preset_id));
    if preset_file.exists() {
        fs::remove_file(&preset_file)
            .map_err(|e| format!("删除预设失败: {}", e))?;
        log::info!("[eq] Deleted user preset file: {:?}", preset_file);
    }
    Ok(())
}

/// 获取 10 段频谱数据（FxSound IIR 滤波器组，每段 0.0-1.0）
#[tauri::command]
pub fn get_spectrum() -> Result<[f64; 10], String> {
    let guard = crate::audio_engine::spectrum_bands()
        .read().map_err(|e| format!("{}", e))?;
    Ok(*guard)
}

/// 获取当前音频输出电平（L/R 峰值，0.0-1.0）
#[tauri::command]
pub fn get_audio_levels() -> Result<(f32, f32), String> {
    let packed = crate::audio_engine::output_levels().load(std::sync::atomic::Ordering::Relaxed);
    Ok(crate::audio_engine::unpack_levels(packed))
}

/// 保存预设（覆盖写入 .fac 文件，包含 EQ 频段和音效参数）
#[tauri::command]
pub fn save_eq_preset(preset_id: String, content: String) -> Result<(), String> {
    let preset_file = get_user_presets_dir().join(format!("{}.fac", preset_id));
    fs::write(&preset_file, &content)
        .map_err(|e| format!("保存预设失败: {}", e))?;
    log::info!("[eq] Saved preset: {:?}", preset_file);
    Ok(())
}

/// 导出 .fac 文件到指定路径
#[tauri::command]
pub fn export_fac_file(path: String, content: String) -> Result<(), String> {
    fs::write(&path, &content)
        .map_err(|e| format!("写入失败: {}", e))?;
    log::info!("[eq] Exported .fac to: {}", path);
    Ok(())
}

/// 清理 EQ 相关资源（应用退出时调用）
pub fn cleanup() {
    let _ = stop_eq_engine();
}
