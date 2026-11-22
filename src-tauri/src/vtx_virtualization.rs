// VT-X 虚拟化修复工具（零 PowerShell）
// 参考 doras-little-toolbox-main 的后端逻辑实现：
//  - 修复：关闭内存完整性(HVCI)、VBS 总开关、Hyper-V 自动启动
//  - 恢复：回开以上三项
//  - 检测：读取注册表实际值与 bcdedit hypervisorlaunchtype
// 实现完全原生：winreg 写注册表 / bcdedit.exe 直接调用 / ShellExecuteExW 提权

use std::env;
use std::os::windows::process::CommandExt;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 内存完整性 (HVCI) 开关路径
const HVCI_KEY: &str = r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity";
/// VBS 虚拟化安全总开关路径
const VBS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\DeviceGuard";

/// 当前虚拟化相关状态
#[derive(serde::Serialize)]
pub struct VtxVirtualizationStatus {
    /// 内存完整性(HVCI)是否开启
    pub hvci_enabled: bool,
    /// 虚拟化安全(VBS)总开关是否开启
    pub vbs_enabled: bool,
    /// hypervisorlaunchtype 当前值（Auto/Off，读取失败为 None）
    pub hypervisor_launch: Option<String>,
    /// 当前进程是否以管理员运行
    pub is_admin: bool,
    /// 注册表键是否存在（区分"未配置"与"已关闭"）
    pub hvci_key_exists: bool,
    pub vbs_key_exists: bool,
}

/// 读取注册表 DWORD 值（不存在返回 None）
fn read_dword(path: &str, name: &str) -> Option<u32> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(path).ok()?;
    key.get_value::<u32, _>(name).ok()
}

/// 判断注册表键是否存在
fn key_exists(path: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm.open_subkey(path).is_ok()
}

/// 原生 winreg 写 DWORD（需管理员）
fn write_dword_direct(path: &str, name: &str, value: u32) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey(path)
        .map_err(|e| format!("打开注册表键失败: {}", e))?;
    key.set_value(name, &value)
        .map_err(|e| format!("写入注册表值 {} 失败: {}", name, e))?;
    Ok(())
}

/// 通过 ShellExecuteEx 提权运行原生命令（非管理员时弹 UAC，等待提权进程结束）
fn run_elevated(file: &str, args: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb_w = to_w("runas");
    let file_w = to_w(file);
    let args_w = to_w(args);

    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.lpVerb = PCWSTR(verb_w.as_ptr());
    sei.lpFile = PCWSTR(file_w.as_ptr());
    sei.lpParameters = PCWSTR(args_w.as_ptr());
    sei.nShow = SW_HIDE.0;

    if unsafe { ShellExecuteExW(&mut sei) }.is_err() {
        return Err("需要管理员权限：提权失败，可能是用户取消了授权".to_string());
    }

    // 等待提权进程执行完毕
    unsafe { WaitForSingleObject(sei.hProcess, u32::MAX) };

    let mut exit_code: u32 = 0;
    let got_code = unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) }.is_ok();
    let _ = unsafe { CloseHandle(sei.hProcess) };

    if !got_code || exit_code != 0 {
        return Err(format!("提权命令执行失败（退出码 {}）", exit_code));
    }
    Ok(())
}

/// 非管理员时通过 reg.exe 提权写注册表
fn write_dword_elevated(path: &str, name: &str, value: u32) -> Result<(), String> {
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let reg_path = format!(r"{}\System32\reg.exe", system_root);
    let args = format!("add HKLM\\{} /v {} /t REG_DWORD /d {} /f", path, name, value);
    run_elevated(&reg_path, &args)
}

/// 写注册表 DWORD（自动选择直写或提权）
fn write_dword(path: &str, name: &str, value: u32) -> Result<(), String> {
    if crate::optimization::is_admin() {
        write_dword_direct(path, name, value)
    } else {
        write_dword_elevated(path, name, value)
    }
}

/// 直接调用 bcdedit.exe（需管理员）
fn run_bcdedit_direct(args: &str) -> Result<(), String> {
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let bcdedit_path = format!(r"{}\System32\bcdedit.exe", system_root);
    let output = Command::new(&bcdedit_path)
        .args(args.split_whitespace())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 bcdedit 失败: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(format!(
                "bcdedit 执行失败（退出码 {:?}）",
                output.status.code()
            ))
        } else {
            Err(stderr)
        }
    }
}

/// 非管理员时提权运行 bcdedit.exe
fn run_bcdedit_elevated(args: &str) -> Result<(), String> {
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let bcdedit_path = format!(r"{}\System32\bcdedit.exe", system_root);
    run_elevated(&bcdedit_path, args)
}

/// 设置 hypervisorlaunchtype（自动选择直写或提权）
fn set_hypervisor_launch(launch_type: &str) -> Result<(), String> {
    let args = format!("/set hypervisorlaunchtype {}", launch_type);
    if crate::optimization::is_admin() {
        run_bcdedit_direct(&args)
    } else {
        run_bcdedit_elevated(&args)
    }
}

/// 查询 hypervisorlaunchtype 当前值（bcdedit 输出为 UTF-16LE，需特殊解析）
fn query_hypervisor_launch() -> Option<String> {
    let system_root = env::var("SystemRoot").ok()?;
    let bcdedit_path = format!(r"{}\System32\bcdedit.exe", system_root);
    let output = Command::new(&bcdedit_path)
        .args(["/enum", "{current}"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = &output.stdout;
    let text = if stdout.starts_with(&[0xFF, 0xFE]) {
        // UTF-16LE with BOM
        let bytes = &stdout[2..];
        let units: Vec<u16> = bytes
            .as_chunks::<2>().0.iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(stdout).to_string()
    };

    for line in text.lines() {
        let t = line.trim();
        if t.to_ascii_lowercase().starts_with("hypervisorlaunchtype") {
            return t.split_whitespace().nth(1).map(|s| s.to_string());
        }
    }
    None
}

/// 检测当前 VT-X / 虚拟化相关状态
#[tauri::command]
pub async fn check_vtx_virtualization_status() -> Result<VtxVirtualizationStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    let hvci_key_exists = key_exists(HVCI_KEY);
    let vbs_key_exists = key_exists(VBS_KEY);
    let hvci_enabled = read_dword(HVCI_KEY, "Enabled").unwrap_or(0) == 1;
    let vbs_enabled = read_dword(VBS_KEY, "EnableVirtualizationBasedSecurity").unwrap_or(0) == 1;
    Ok(VtxVirtualizationStatus {
        hvci_enabled,
        vbs_enabled,
        hypervisor_launch: query_hypervisor_launch(),
        is_admin: crate::optimization::is_admin(),
        hvci_key_exists,
        vbs_key_exists,
    })
}

/// 一键修复 VT-X 虚拟化弹窗：关闭内存完整性(HVCI)、VBS 总开关与 Hyper-V 自动启动
#[tauri::command]
pub async fn fix_vtx_virtualization_popup() -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let mut errors: Vec<String> = Vec::new();

    // 1. 关闭内存完整性 (HVCI)
    if let Err(e) = write_dword(HVCI_KEY, "Enabled", 0) {
        errors.push(format!("关闭内存完整性失败: {}", e));
    }

    // 2. 关闭 VBS 虚拟化安全总开关
    if let Err(e) = write_dword(VBS_KEY, "EnableVirtualizationBasedSecurity", 0) {
        errors.push(format!("关闭虚拟化安全(VBS)失败: {}", e));
    }

    // 3. 关闭 Hyper-V 自动启动
    if let Err(e) = set_hypervisor_launch("off") {
        errors.push(format!("关闭 Hyper-V 自动启动失败: {}", e));
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    // 回读校验
    let hvci_ok = read_dword(HVCI_KEY, "Enabled").is_none_or(|v| v == 0);
    let vbs_ok = read_dword(VBS_KEY, "EnableVirtualizationBasedSecurity").is_none_or(|v| v == 0);

    if hvci_ok && vbs_ok {
        Ok("修复已执行成功：已关闭内存完整性、VBS 虚拟化安全与 Hyper-V 自动启动，重启电脑后生效。".to_string())
    } else {
        Ok("修复命令已执行，但部分注册表项回读未生效，请重启电脑后复查。".to_string())
    }
}

/// 一键恢复 VT-X 虚拟化设置：回开内存完整性(HVCI)、VBS 总开关与 Hyper-V 自动启动
#[tauri::command]
pub async fn restore_vtx_virtualization() -> Result<String, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let mut errors: Vec<String> = Vec::new();

    // 1. 恢复内存完整性 (HVCI)
    if let Err(e) = write_dword(HVCI_KEY, "Enabled", 1) {
        errors.push(format!("恢复内存完整性失败: {}", e));
    }

    // 2. 恢复 VBS 虚拟化安全总开关
    if let Err(e) = write_dword(VBS_KEY, "EnableVirtualizationBasedSecurity", 1) {
        errors.push(format!("恢复虚拟化安全(VBS)失败: {}", e));
    }

    // 3. 恢复 Hyper-V 自动启动
    if let Err(e) = set_hypervisor_launch("auto") {
        errors.push(format!("恢复 Hyper-V 自动启动失败: {}", e));
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    // 回读校验
    let hvci_ok = read_dword(HVCI_KEY, "Enabled") == Some(1);
    let vbs_ok = read_dword(VBS_KEY, "EnableVirtualizationBasedSecurity") == Some(1);

    if hvci_ok && vbs_ok {
        Ok("恢复已执行成功：已恢复内存完整性、VBS 虚拟化安全与 Hyper-V 自动启动，重启电脑后生效。".to_string())
    } else {
        Ok("恢复命令已执行，但部分注册表项回读未生效，请重启电脑后复查。".to_string())
    }
}
