use std::os::windows::process::CommandExt;
use std::panic;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::{env, fs, path::Path, path::PathBuf};
use sysinfo::System;
use tauri::Manager;
use winreg::enums::*;
use winreg::RegKey;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows_sys::Win32::System::ProcessStatus::{
    EmptyWorkingSet, K32EnumProcesses, K32GetProcessImageFileNameW, K32GetProcessMemoryInfo,
    PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::Memory::SetSystemFileCacheSize;
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, SetPriorityClass, SetProcessAffinityMask,
    SetProcessWorkingSetSize, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA, PROCESS_VM_READ,
};

pub(crate) const CREATE_NO_WINDOW: u32 = 0x08000000;

fn get_powershell_path() -> String {
    if let Ok(sysroot) = env::var("SystemRoot") {
        let ps_path = format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", sysroot);
        if Path::new(&ps_path).exists() {
            return ps_path;
        }
    }
    "powershell.exe".to_string()
}

const PROCESS_SET_INFORMATION: u32 = 0x0200;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const IDLE_PRIORITY_CLASS: u32 = 0x00000040;
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
const REALTIME_PRIORITY_CLASS: u32 = 0x00000100;
const PROCESS_MODE_BACKGROUND_BEGIN: u32 = 0x00100000;
const IO_PRIORITY_VERY_LOW: u32 = 0;
const PROCESS_MEMORY_PRIORITY_NEW: u32 = 0;
const PROCESS_MEMORY_PRIORITY_OLD: u32 = 11;
const MEMORY_PRIORITY_VERY_LOW: u32 = 1;

// ===== 内存列表清理（NtSetSystemInformation / NtQuerySystemInformation）=====
// SystemMemoryListInformation 的各类命令值，与 Mem Reduct 等主流工具一致
const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80;
const SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION: u32 = 130;
const SYSTEM_REGISTRY_RECONCILIATION_INFORMATION: u32 = 149;
const MEMORY_FLUSH_MODIFIED_LIST: u32 = 3;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;
const MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST: u32 = 5;

/// 受保护的系统关键进程（工作集整理时跳过，避免 UI 闪烁 / 服务卡顿）
const PROTECTED_SYSTEM_PROCESSES: &[&str] = &[
    "system",
    "registry",
    "memory compression",
    "smss",
    "csrss",
    "wininit",
    "winlogon",
    "services",
    "lsass",
    "fontdrvhost",
    "dwm",
    "explorer",
    "svchost",
    "dllhost",
    "conhost",
    "sihost",
    "taskhostw",
    "ctfmon",
    "SearchIndexer",
    "MsMpEng",
    "NisSrv",
    "WmiPrvSE",
    "SecurityHealthService",
    "RuntimeBroker",
    "ShellExperienceHost",
    "StartMenuExperienceHost",
    "TextInputHost",
    "backgroundTaskHost",
];

#[link(name = "ntdll")]
extern "system" {
    fn NtSetInformationProcess(
        ProcessHandle: HANDLE,
        ProcessInformationClass: u32,
        ProcessInformation: *const std::ffi::c_void,
        ProcessInformationSize: u32,
    ) -> i32;
    fn NtQueryInformationProcess(
        ProcessHandle: HANDLE,
        ProcessInformationClass: u32,
        ProcessInformation: *mut std::ffi::c_void,
        ProcessInformationSize: u32,
        ReturnLength: *mut u32,
    ) -> i32;
    fn NtSetSystemInformation(
        InformationClass: u32,
        Information: *const std::ffi::c_void,
        Length: u32,
    ) -> i32;
    fn NtQuerySystemInformation(
        SystemInformationClass: u32,
        SystemInformation: *mut std::ffi::c_void,
        SystemInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn D3DKMTSetProcessSchedulingPriorityClass(hProcess: HANDLE, priority: i32) -> i32;
    fn D3DKMTGetProcessSchedulingPriorityClass(hProcess: HANDLE, priority: *mut i32) -> i32;
}

extern "system" {
    fn SetProcessInformation(
        hProcess: HANDLE,
        ProcessInformationClass: u32,
        ProcessInformation: *const std::ffi::c_void,
        ProcessInformationSize: u32,
    ) -> i32;
    fn GetProcessInformation(
        hProcess: HANDLE,
        ProcessInformationClass: u32,
        ProcessInformation: *mut std::ffi::c_void,
        ProcessInformationSize: u32,
    ) -> i32;
}

pub(crate) fn enable_process_efficiency_mode(pid: u32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }

        let mut applied = false;

        // 1) CPU: BELOW_NORMAL 优先级
        if SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS) != 0 {
            applied = true;
        }

        // 2) 后台 I/O + 内存：先尝试 PROCESS_MODE_BACKGROUND_BEGIN（旧版 Win11）
        if SetPriorityClass(handle, PROCESS_MODE_BACKGROUND_BEGIN) != 0 {
            SetPriorityClass(handle, BELOW_NORMAL_PRIORITY_CLASS);
        } else {
            // Build 26200+：独立 API 逐项设置
            // I/O 优先级 → VeryLow
            let io: u32 = IO_PRIORITY_VERY_LOW;
            let nt = NtSetInformationProcess(
                handle,
                33,
                &io as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            if nt != 0 {
                log::warn!("I/O priority failed (pid={}, nt={})", pid, nt);
            }

            // 内存优先级 → VeryLow：新 SDK class=0，回退旧 SDK class=11
            let mem: u32 = MEMORY_PRIORITY_VERY_LOW;
            let mut mem_ok = false;
            for cls in &[PROCESS_MEMORY_PRIORITY_NEW, PROCESS_MEMORY_PRIORITY_OLD] {
                if SetProcessInformation(
                    handle,
                    *cls,
                    &mem as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                ) != 0
                {
                    mem_ok = true;
                    break;
                }
            }
            if !mem_ok {
                let err = GetLastError();
                log::warn!("Memory priority failed (pid={}, err={})", pid, err);
            }
        }

        CloseHandle(handle);
        applied
    }
}

/// 启用当前进程的 SeDebugPrivilege（管理员令牌默认禁用），
/// 提升打开受保护进程句柄的能力（与任务管理器一致）
pub(crate) fn enable_se_debug_privilege() {
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, LUID};
        use windows_sys::Win32::Security::{
            AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
            TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) == 0
        {
            return;
        }
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        let name: Vec<u16> = "SeDebugPrivilege\0".encode_utf16().collect();
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) != 0 {
            let mut tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            AdjustTokenPrivileges(
                token,
                0,
                &mut tp,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        CloseHandle(token);
    }
}

/// 尽力打开目标进程句柄：先用最小权限（最易被放行），
/// 失败后启用 SeDebugPrivilege 并逐步扩大权限，最后尝试完整权限。
pub(crate) fn open_process_any(pid: u32) -> HANDLE {
    unsafe {
        let desired = PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION;
        let handle = OpenProcess(desired, 0, pid);
        if !handle.is_null() {
            return handle;
        }
        enable_se_debug_privilege();
        let wider = PROCESS_SET_INFORMATION
            | PROCESS_QUERY_LIMITED_INFORMATION
            | 0x0008 // PROCESS_QUERY_INFORMATION
            | 0x0400; // PROCESS_SYNCHRONIZE
        let h2 = OpenProcess(wider, 0, pid);
        if !h2.is_null() {
            return h2;
        }
        // 最后尝试完整权限（PROCESS_ALL_ACCESS）
        OpenProcess(0x001FFFFF, 0, pid)
    }
}

/// 当前进程是否以管理员（提升令牌）身份运行
pub(crate) fn is_admin() -> bool {
    unsafe {
        use std::mem::size_of;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut _,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        ) != 0;
        CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

pub(crate) fn set_process_low_priority(pid: u32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = SetPriorityClass(handle, IDLE_PRIORITY_CLASS) != 0;
        CloseHandle(handle);
        ok
    }
}

/// 设置进程优先级为实时（对应 .NET ProcessPriorityClass::RealTime）
pub(crate) fn set_process_realtime_priority(pid: u32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = SetPriorityClass(handle, REALTIME_PRIORITY_CLASS) != 0;
        CloseHandle(handle);
        ok
    }
}

/// 设置进程 CPU 亲和性掩码（直接 Win32 API，无需 PowerShell）
/// 失败时用 ntdll 层 NtSetInformationProcess 兜底（任务管理器同款调用路径）
pub(crate) fn set_process_affinity(pid: u32, mask: u64) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        // SetProcessAffinityMask 第二参数为 usize（ULONG_PTR），64 位系统为 u64
        let mut ok = SetProcessAffinityMask(handle, mask as usize) != 0;
        if !ok {
            let m = mask as usize;
            ok = NtSetInformationProcess(
                handle,
                5, // ProcessAffinityMask
                &m as *const usize as *const std::ffi::c_void,
                std::mem::size_of::<usize>() as u32,
            ) == 0;
        }
        CloseHandle(handle);
        ok
    }
}

// ─── 游戏模式压制原语（快照/恢复用） ───
// 常量与类别编号对齐 Windows 内核与 Pavise：I/O 优先级=33、内存页优先级=39、
// ProcessPowerThrottling=4 / Nt 类=77、GPU 调度优先级类（Idle=0..High=4）。

fn nt_query_u32(handle: HANDLE, class: u32) -> Option<i32> {
    let mut v: i32 = 0;
    unsafe {
        let r = NtQueryInformationProcess(
            handle,
            class,
            &mut v as *mut i32 as *mut std::ffi::c_void,
            4,
            std::ptr::null_mut(),
        );
        if r == 0 {
            Some(v)
        } else {
            None
        }
    }
}

fn nt_set_u32(handle: HANDLE, class: u32, v: i32) -> bool {
    unsafe {
        NtSetInformationProcess(
            handle,
            class,
            &v as *const i32 as *const std::ffi::c_void,
            4,
        ) == 0
    }
}

/// 设置进程优先级类别（NORMAL/BELOW_NORMAL/IDLE 等）
pub(crate) fn set_process_priority_class(pid: u32, class: u32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = SetPriorityClass(handle, class) != 0;
        CloseHandle(handle);
        ok
    }
}

/// 查询进程当前优先级类别
pub(crate) fn query_process_priority(pid: u32) -> Option<u32> {
    use windows_sys::Win32::System::Threading::GetPriorityClass;
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let v = GetPriorityClass(handle);
        CloseHandle(handle);
        if v == 0 {
            None
        } else {
            Some(v)
        }
    }
}

/// 查询进程当前 CPU 亲和性掩码
pub(crate) fn query_process_affinity(pid: u32) -> Option<u64> {
    use windows_sys::Win32::System::Threading::GetProcessAffinityMask;
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        let ok = GetProcessAffinityMask(handle, &mut process_mask, &mut system_mask) != 0;
        CloseHandle(handle);
        if ok {
            Some(process_mask as u64)
        } else {
            None
        }
    }
}

/// 设置进程 I/O 优先级（0=VeryLow, 1=Low, 2=Normal, 3=High）
pub(crate) fn set_process_io_priority(pid: u32, priority: i32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = nt_set_u32(handle, 33, priority);
        CloseHandle(handle);
        ok
    }
}

/// 查询进程 I/O 优先级（-1 表示失败）
pub(crate) fn query_process_io_priority(pid: u32) -> Option<i32> {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let v = nt_query_u32(handle, 33);
        CloseHandle(handle);
        v
    }
}

/// 设置进程内存页优先级（1=VeryLow, 2=Low, 3=Medium, 4=High, 5=RealTime）
pub(crate) fn set_process_memory_priority(pid: u32, priority: i32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = nt_set_u32(handle, 39, priority);
        CloseHandle(handle);
        ok
    }
}

/// 查询进程内存页优先级（-1 表示失败）
pub(crate) fn query_process_memory_priority(pid: u32) -> Option<i32> {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let v = nt_query_u32(handle, 39);
        CloseHandle(handle);
        v
    }
}

/// 设置进程 EcoQoS 效率模式（on=true 开启，on=false 关闭）
pub(crate) fn set_process_eco_qos(pid: u32, on: bool) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        // POWER_THROTTLING_PROCESS_STATE: Version=1(u32), ControlMask(u32), StateMask(u32)
        let state: [u32; 3] = if on { [1, 1, 1] } else { [1, 0, 0] };
        let ok = SetProcessInformation(
            handle,
            4, // ProcessPowerThrottling
            state.as_ptr() as *const std::ffi::c_void,
            std::mem::size_of::<[u32; 3]>() as u32,
        ) != 0;
        CloseHandle(handle);
        ok
    }
}

/// 查询进程 EcoQoS 状态，返回 Some((control_mask, state_mask))；失败返回 None
pub(crate) fn query_process_eco_state(pid: u32) -> Option<(u32, u32)> {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let mut state: [u32; 3] = [1, 0, 0];
        let ok = GetProcessInformation(
            handle,
            4,
            state.as_mut_ptr() as *mut std::ffi::c_void,
            std::mem::size_of::<[u32; 3]>() as u32,
        ) != 0;
        CloseHandle(handle);
        if ok {
            Some((state[1], state[2]))
        } else {
            None
        }
    }
}

/// 设置进程 GPU 调度优先级类（0=Idle, 1=BelowNormal, 2=Normal, 4=High）
pub(crate) fn set_process_gpu_priority(pid: u32, priority: i32) -> bool {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return false;
        }
        let ok = D3DKMTSetProcessSchedulingPriorityClass(handle, priority) == 0;
        CloseHandle(handle);
        ok
    }
}

/// 查询进程 GPU 调度优先级类（-1 表示失败）
pub(crate) fn query_process_gpu_priority(pid: u32) -> Option<i32> {
    unsafe {
        let handle = open_process_any(pid);
        if handle.is_null() {
            return None;
        }
        let mut v: i32 = -1;
        let ok = D3DKMTGetProcessSchedulingPriorityClass(handle, &mut v) == 0;
        CloseHandle(handle);
        if ok {
            Some(v)
        } else {
            None
        }
    }
}

fn run_bcdedit_admin(args: &str) -> Result<String, String> {
    let ps_script = format!(
        "Start-Process bcdedit -ArgumentList '{}' -Verb RunAs -Wait -WindowStyle Hidden",
        args
    );
    
    let result = Command::new("powershell")
        .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                Ok("命令执行成功".to_string())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Err(format!("执行失败: {}", stderr))
            }
        }
        Err(e) => Err(format!("执行命令失败: {}", e)),
    }
}

#[derive(serde::Serialize)]
pub struct MemoryInfo {
    total: u64,
    available: u64,
    used: u64,
    usage_percent: f32,
}

#[derive(serde::Serialize)]
pub struct OptimizationResult {
    success: bool,
    message: String,
    before: MemoryInfo,
    after: MemoryInfo,
    freed_mb: u64,
}

fn get_memory_info() -> MemoryInfo {
    let mut sys = System::new();
    sys.refresh_memory();

    let total = sys.total_memory() / 1024 / 1024;
    let available = sys.available_memory() / 1024 / 1024;
    let used = total - available;
    let usage_percent = if total > 0 {
        (used as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    MemoryInfo {
        total,
        available,
        used,
        usage_percent,
    }
}

#[tauri::command]
pub async fn optimize_memory() -> Result<OptimizationResult, String> {
    let before = get_memory_info();

    if cfg!(target_os = "windows") {
        // 原生并行清理：待机缓存 + 全进程工作集收紧，无需 PowerShell
        thread::scope(|s| {
            s.spawn(|| {
                clean_standby_memory_inner();
            });
            s.spawn(|| {
                trim_working_set_inner();
            });
        });

        let after = get_memory_info();
        let freed = if after.available > before.available {
            after.available - before.available
        } else {
            0
        };

        Ok(OptimizationResult {
            success: true,
            message: format!("内存优化完成，释放约 {} MB", freed),
            before,
            after,
            freed_mb: freed,
        })
    } else {
        Err("内存优化仅支持 Windows 系统".to_string())
    }
}

#[tauri::command]
pub async fn get_memory_status() -> Result<MemoryInfo, String> {
    Ok(get_memory_info())
}

#[derive(serde::Serialize)]
pub struct ProcessKillResult {
    success: bool,
    message: String,
    process_name: String,
    was_running: bool,
}

#[tauri::command]
pub async fn kill_wallpaper_engine() -> Result<ProcessKillResult, String> {
    let process_names = ["wallpaper64", "wallpaper32", "wallpaper_engine"];

    if cfg!(target_os = "windows") {
        let mut killed_any = false;
        let mut killed_name = String::new();

        for name in process_names {
            let result = Command::new("powershell")
                .args(&[
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    &format!(
                        r#"
                        $process = Get-Process -Name "{}" -ErrorAction SilentlyContinue
                        if ($process) {{
                            Stop-Process -Name "{}" -Force -ErrorAction SilentlyContinue
                            Write-Host "Killed: {}"
                            exit 0
                        }} else {{
                            Write-Host "Not running: {}"
                            exit 1
                        }}
                        "#,
                        name, name, name, name
                    ),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            match result {
                Ok(output) => {
                    if output.status.success() {
                        killed_any = true;
                        killed_name = name.to_string();
                        break;
                    }
                }
                Err(_) => continue,
            }
        }

        if killed_any {
            Ok(ProcessKillResult {
                success: true,
                message: "Wallpaper Engine 进程已关闭".to_string(),
                process_name: killed_name,
                was_running: true,
            })
        } else {
            Ok(ProcessKillResult {
                success: true,
                message: "Wallpaper Engine 未在运行".to_string(),
                process_name: String::new(),
                was_running: false,
            })
        }
    } else {
        Err("此功能仅支持 Windows 系统".to_string())
    }
}

#[derive(serde::Serialize)]
pub struct PowerPlanResult {
    success: bool,
    message: String,
    previous_plan: Option<String>,
    current_plan: String,
}

/// 已知的高性能/Normal GUID（系统内置计划）。
const KNOWN_HIGH_PERF_GUID: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

/// 通过名称关键词匹配，在系统电源计划中查找高性能方案。
fn find_high_performance_guid(plans: &[(String, String, bool)]) -> Option<String> {
    // 优先级：卓越性能 > 高性能/Ultimate
    let candidates: &[&[&str]] = &[
        &["卓越性能", "Ultimate Performance"],
        &["高性能", "High performance", "Ultimate"],
    ];
    for keywords in candidates {
        for (guid, name, _) in plans {
            let lower = name.to_lowercase();
            if keywords.iter().any(|kw| lower.contains(&kw.to_lowercase())) {
                return Some(guid.clone());
            }
        }
    }
    None
}

#[tauri::command]
pub async fn set_high_performance_power_plan() -> Result<PowerPlanResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 1. 记录当前计划（直接调 powercfg，不经过 PowerShell）
    let previous_plan = get_active_plan_internal()
        .map(|(guid, name)| format!("{} ({})", name, guid));

    // 2. 枚举所有系统计划，尝试按名称匹配高性能方案
    let system_plans = get_system_plans_internal();
    let target_guid = find_high_performance_guid(&system_plans)
        .unwrap_or_else(|| KNOWN_HIGH_PERF_GUID.to_string());

    // 3. 直接调用 powercfg /setactive
    let result = run_powercfg(&["/setactive", &target_guid]);

    match result {
        Ok(output) if output.status.success() => {
            // 4. 验证切换结果（直接调 powercfg）
            let current_plan = match get_active_plan_internal() {
                Some((guid, name)) => format!("{} ({})", name, guid),
                None => "高性能".to_string(),
            };

            Ok(PowerPlanResult {
                success: true,
                message: "已切换到高性能电源计划".to_string(),
                previous_plan,
                current_plan,
            })
        }
        Ok(output) => {
            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("切换电源计划失败: {}", error_msg))
        }
        Err(e) => Err(format!("执行电源计划切换命令失败: {}", e)),
    }
}

#[derive(serde::Serialize)]
pub struct AceOptimizeResult {
    success: bool,
    message: String,
    optimized_processes: Vec<String>,
}

#[tauri::command]
pub async fn optimize_ace_processes() -> Result<AceOptimizeResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if !is_admin() {
        return Err("修改 ACE 进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    let mut optimized_processes = Vec::new();
    let mut system = System::new();
    system.refresh_processes();

    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();
        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            let pid = process.pid().as_u32();
            let priority_ok = set_process_low_priority(pid);
            let affinity_ok = set_process_affinity(pid, 1);
            if priority_ok || affinity_ok {
                optimized_processes.push(name);
            }
        }
    }

    if !optimized_processes.is_empty() {
        Ok(AceOptimizeResult {
            success: true,
            message: format!("已优化 {} 个ACE进程", optimized_processes.len()),
            optimized_processes,
        })
    } else {
        Ok(AceOptimizeResult {
            success: true,
            message: "未找到运行中的ACE进程".to_string(),
            optimized_processes: vec![],
        })
    }
}

#[derive(serde::Serialize)]
pub struct AceEfficiencyResult {
    pub success: bool,
    pub message: String,
    pub count: u32,
    pub found_count: u32,
}

#[tauri::command]
pub async fn set_ace_efficiency_mode() -> Result<AceEfficiencyResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if !is_admin() {
        return Err("修改 ACE 进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    let mut found = 0u32;
    let mut count = 0u32;

    let mut system = System::new();
    system.refresh_processes();

    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();
        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            found += 1;
            if enable_process_efficiency_mode(process.pid().as_u32()) {
                count += 1;
            }
        }
    }

    Ok(AceEfficiencyResult {
        success: count > 0,
        message: ace_message(found, count, "已为 {} 个 ACE 进程开启效能模式"),
        count,
        found_count: found,
    })
}

#[derive(serde::Serialize)]
pub struct DnsFlushResult {
    success: bool,
    message: String,
}

#[derive(serde::Serialize)]
pub struct TempCleanupResult {
    success: bool,
    message: String,
    scanned_files: u64,
    deleted_files: u64,
    deleted_dirs: u64,
    failed_items: u64,
}

#[derive(serde::Serialize)]
pub struct PrivacyServiceOptimizeResult {
    success: bool,
    message: String,
    stopped_services: Vec<String>,
}

fn clean_temp_dir(path: &Path) -> (u64, u64, u64, u64) {
    let mut scanned_files = 0;
    let mut deleted_files = 0;
    let mut deleted_dirs = 0;
    let mut failed_items = 0;

    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0, 0, 1);
    };

    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let (s, df, dd, f) = clean_temp_dir(&entry_path);
            scanned_files += s;
            deleted_files += df;
            deleted_dirs += dd;
            failed_items += f;

            match fs::remove_dir(&entry_path) {
                Ok(_) => deleted_dirs += 1,
                Err(_) => failed_items += 1,
            }
        } else {
            scanned_files += 1;
            match fs::remove_file(&entry_path) {
                Ok(_) => deleted_files += 1,
                Err(_) => failed_items += 1,
            }
        }
    }

    (scanned_files, deleted_files, deleted_dirs, failed_items)
}

#[tauri::command]
pub async fn clean_temp_files() -> Result<TempCleanupResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let mut temp_paths = Vec::new();
    if let Ok(user_temp) = env::var("TEMP") {
        temp_paths.push(user_temp);
    }
    if let Ok(system_root) = env::var("SystemRoot") {
        temp_paths.push(format!("{system_root}\\Temp"));
    }
    temp_paths.sort();
    temp_paths.dedup();

    if temp_paths.is_empty() {
        return Err("未找到可清理的临时目录".to_string());
    }

    let mut scanned_files = 0;
    let mut deleted_files = 0;
    let mut deleted_dirs = 0;
    let mut failed_items = 0;

    for path in temp_paths {
        let dir = Path::new(&path);
        if !dir.exists() {
            continue;
        }
        let (s, df, dd, f) = clean_temp_dir(dir);
        scanned_files += s;
        deleted_files += df;
        deleted_dirs += dd;
        failed_items += f;
    }

    Ok(TempCleanupResult {
        success: true,
        message: format!("临时文件清理完成：删除 {} 个文件，{} 个目录", deleted_files, deleted_dirs),
        scanned_files,
        deleted_files,
        deleted_dirs,
        failed_items,
    })
}

#[tauri::command]
pub async fn optimize_privacy_services() -> Result<PrivacyServiceOptimizeResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let target_services = ["DiagTrack", "dmwappushservice", "diagnosticshub.standardcollector.service"];
    let mut stopped_services = Vec::new();

    for service in target_services {
        let result = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!(
                    r#"
                    $svc = Get-Service -Name "{}" -ErrorAction SilentlyContinue
                    if ($svc) {{
                        if ($svc.Status -ne 'Stopped') {{
                            Stop-Service -Name "{}" -Force -ErrorAction SilentlyContinue
                            Write-Host "Stopped: {}"
                        }} else {{
                            Write-Host "AlreadyStopped: {}"
                        }}
                    }} else {{
                        Write-Host "NotFound: {}"
                    }}
                    "#,
                    service, service, service, service, service
                ),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(output) = result {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.contains(&format!("Stopped: {}", service))
                || stdout.contains(&format!("AlreadyStopped: {}", service))
            {
                stopped_services.push(service.to_string());
            }
        }
    }

    Ok(PrivacyServiceOptimizeResult {
        success: true,
        message: format!("服务优化完成：已处理 {} 个服务", stopped_services.len()),
        stopped_services,
    })
}

#[tauri::command]
pub async fn flush_dns() -> Result<DnsFlushResult, String> {
    if cfg!(target_os = "windows") {
        let result = Command::new("ipconfig")
            .args(&["/flushdns"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    if stdout.contains("successfully") || stdout.contains("成功") {
                        Ok(DnsFlushResult {
                            success: true,
                            message: "DNS 缓存已成功清理".to_string(),
                        })
                    } else {
                        Ok(DnsFlushResult {
                            success: true,
                            message: "DNS 缓存清理完成".to_string(),
                        })
                    }
                } else {
                    let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
                    Err(format!("DNS 清理失败: {}", error_msg))
                }
            }
            Err(e) => Err(format!("执行 DNS 清理命令失败: {}", e)),
        }
    } else {
        Err("此功能仅支持 Windows 系统".to_string())
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct MemoryLimitOption {
    pub id: String,
    pub label: String,
    pub limit_gb: f64,
    pub min_physical_gb: f64,
}

#[derive(serde::Serialize)]
pub struct MemoryLimitStatus {
    pub physical_memory_gb: f64,
    pub physical_memory_mb: u64,
    pub current_limit_mb: Option<u64>,
    pub available_options: Vec<MemoryLimitOption>,
}

#[derive(serde::Serialize)]
pub struct MemoryLimitResult {
    pub success: bool,
    pub message: String,
    pub limit_mb: Option<u64>,
    pub requires_restart: bool,
}

fn get_physical_memory_mb() -> u64 {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys.total_memory() / 1024 / 1024
}

fn get_memory_limit_options_internal() -> Vec<MemoryLimitOption> {
    vec![
        MemoryLimitOption {
            id: "7.9gb".to_string(),
            label: "7.9 GB".to_string(),
            limit_gb: 7.9,
            min_physical_gb: 0.0,
        },
        MemoryLimitOption {
            id: "11.9gb".to_string(),
            label: "11.9 GB".to_string(),
            limit_gb: 11.9,
            min_physical_gb: 0.0,
        },
        MemoryLimitOption {
            id: "13.9gb".to_string(),
            label: "13.9 GB".to_string(),
            limit_gb: 13.9,
            min_physical_gb: 0.0,
        },
        MemoryLimitOption {
            id: "15.9gb".to_string(),
            label: "15.9 GB".to_string(),
            limit_gb: 15.9,
            min_physical_gb: 0.0,
        },
    ]
}

fn get_current_memory_limit() -> Option<u64> {
    if !cfg!(target_os = "windows") {
        return None;
    }

    let result = Command::new("bcdedit")
        .args(&["/enum", "{current}"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            for line in stdout.lines() {
                let lower_line = line.to_lowercase();
                if lower_line.contains("removememory") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for part in parts.iter().rev() {
                        if let Ok(value) = part.parse::<u64>() {
                            return Some(value);
                        }
                    }
                }
            }
            None
        }
        Err(_) => None,
    }
}

#[tauri::command]
pub async fn get_memory_limit_options() -> Vec<MemoryLimitOption> {
    get_memory_limit_options_internal()
}

#[tauri::command]
pub async fn get_memory_limit_status() -> Result<MemoryLimitStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let physical_memory_mb = get_physical_memory_mb();
    let physical_memory_gb = physical_memory_mb as f64 / 1024.0;
    let current_limit_mb = get_current_memory_limit();
    let all_options = get_memory_limit_options_internal();

    let available_options: Vec<MemoryLimitOption> = all_options
        .into_iter()
        .filter(|opt| opt.min_physical_gb <= physical_memory_gb)
        .collect();

    Ok(MemoryLimitStatus {
        physical_memory_gb,
        physical_memory_mb,
        current_limit_mb,
        available_options,
    })
}

#[tauri::command]
pub async fn set_memory_limit(limit_gb: f64) -> Result<MemoryLimitResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let physical_memory_mb = get_physical_memory_mb();
    let physical_memory_gb = physical_memory_mb as f64 / 1024.0;
    let limit_mb = (limit_gb * 1024.0) as u64;

    if limit_gb >= physical_memory_gb {
        return Err(format!(
            "限制值 ({:.1} GB) 不能大于或等于物理内存 ({:.1} GB)",
            limit_gb, physical_memory_gb
        ));
    }

    let remove_mb = physical_memory_mb.saturating_sub(limit_mb);
    let args = format!("/set \"{{current}}\" removememory {}", remove_mb);

    match run_bcdedit_admin(&args) {
        Ok(_) => Ok(MemoryLimitResult {
            success: true,
            message: format!("内存限制已设置为 {:.1} GB，需要重启生效", limit_gb),
            limit_mb: Some(limit_mb),
            requires_restart: true,
        }),
        Err(e) => Err(format!("设置内存限制失败: {}。请以管理员身份运行应用。", e)),
    }
}

#[tauri::command]
pub async fn restore_memory_limit() -> Result<MemoryLimitResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let args = "/deletevalue \"{current}\" removememory";

    match run_bcdedit_admin(args) {
        Ok(_) => Ok(MemoryLimitResult {
            success: true,
            message: "内存限制已恢复默认，需要重启生效".to_string(),
            limit_mb: None,
            requires_restart: true,
        }),
        Err(e) => Err(format!("恢复内存限制失败: {}。请以管理员身份运行应用。", e)),
    }
}

#[derive(serde::Serialize)]
pub struct DetailedMemoryInfo {
    pub physical_total: u64,
    pub physical_used: u64,
    pub physical_available: u64,
    pub virtual_total: u64,
    pub virtual_used: u64,
    pub virtual_available: u64,
    pub working_set_total: u64,
    pub working_set_used: u64,
    pub working_set_available: u64,
}

#[derive(serde::Serialize)]
pub struct MemoryCleanupResult {
    pub success: bool,
    pub message: String,
    pub freed_mb: u64,
}

/// 各内存列表的实时大小（页数换算为 MB），用于清理项勾选前的容量展示
/// available=false 表示当前进程权限不足（需管理员运行），容量不可见
#[derive(serde::Serialize)]
pub struct MemoryListSizes {
    pub available: bool,
    pub zeroed_mb: u64,
    pub free_mb: u64,
    pub standby_mb: u64,
    pub modified_mb: u64,
    pub combined_mb: u64,
}

/// NtQuerySystemInformation(SystemMemoryListInformation=80) 返回的结构（winternl.h 公开定义）
#[repr(C)]
struct SystemMemoryListInformation {
    zero_page_count: usize,
    free_page_count: usize,
    modified_page_count: usize,
    modified_no_write_page_count: usize,
    standby_page_count: usize,
    standby_cache_normal_priority: usize,
    standby_cache_system_priority: usize,
    standby_cache_reserve_priority: usize,
    standby_cache_code_priority: usize,
    modified_no_write_cache_normal_priority: usize,
    modified_no_write_cache_system_priority: usize,
    modified_no_write_cache_reserve_priority: usize,
    modified_no_write_cache_code_priority: usize,
    standby_repurposed_count: usize,
    combines_page_count: usize,
}

const PAGE_SIZE: usize = 4096;

fn pages_to_mb(pages: usize) -> u64 {
    (pages as u64) * (PAGE_SIZE as u64) / 1024 / 1024
}

/// 启用 SeProfileSingleProcessPrivilege（查询/下发内存列表命令所需，管理员进程持有但默认禁用）
fn enable_profile_single_process_privilege() -> bool {
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::OpenProcessToken;
    unsafe {
        let mut token: HANDLE = std::mem::zeroed();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let mut luid: windows_sys::Win32::Foundation::LUID = std::mem::zeroed();
        let name: Vec<u16> = "SeProfileSingleProcessPrivilege".encode_utf16().collect();
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let ok = AdjustTokenPrivileges(
            token,
            0,
            &mut tp,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        CloseHandle(token);
        ok != 0 && GetLastError() == 0
    }
}

/// 查询各内存列表大小（待机 / 修改 / 组合 / 空闲），失败返回 available=false
#[tauri::command]
pub async fn get_memory_list_sizes() -> Result<MemoryListSizes, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    unsafe {
        // 查询系统内存列表需要 SeProfileSingleProcessPrivilege，先尝试启用
        enable_profile_single_process_privilege();

        let mut info: SystemMemoryListInformation = std::mem::zeroed();
        let mut ret_len: u32 = 0;
        let status = NtQuerySystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<SystemMemoryListInformation>() as u32,
            &mut ret_len,
        );
        if status < 0 {
            // 权限不足（非管理员运行）时返回 available=false，前端显示 "--" 而非误导性的 0
            return Ok(MemoryListSizes {
                available: false,
                zeroed_mb: 0,
                free_mb: 0,
                standby_mb: 0,
                modified_mb: 0,
                combined_mb: 0,
            });
        }
        Ok(MemoryListSizes {
            available: true,
            zeroed_mb: pages_to_mb(info.zero_page_count),
            free_mb: pages_to_mb(info.free_page_count),
            standby_mb: pages_to_mb(info.standby_page_count),
            modified_mb: pages_to_mb(info.modified_page_count + info.modified_no_write_page_count),
            combined_mb: pages_to_mb(info.combines_page_count),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AutoCleanConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub threshold_mb: u64,
    pub clean_type: String,
}

use tauri_plugin_store::StoreExt;

static AUTO_CLEAN_CONFIG: Mutex<Option<AutoCleanConfig>> = Mutex::new(None);
static AUTO_CLEAN_GENERATION: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
pub async fn get_detailed_memory_status() -> Result<DetailedMemoryInfo, String> {
    let mut sys = System::new();
    sys.refresh_memory();

    let physical_total = sys.total_memory() / 1024 / 1024;
    let physical_available = sys.available_memory() / 1024 / 1024;
    let physical_used = physical_total - physical_available;

    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 原生获取虚拟内存 + 全进程工作集总和（GlobalMemoryStatusEx + EnumProcesses），无需 PowerShell
    let mut virtual_total: u64 = 0;
    let mut virtual_available: u64 = 0;
    let mut working_set_used: u64 = 0;

    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            virtual_total = status.ullTotalPageFile / 1024 / 1024;
            virtual_available = status.ullAvailPageFile / 1024 / 1024;
        }

        let mut pids: [u32; 8192] = [0; 8192];
        let mut needed: u32 = 0;
        if K32EnumProcesses(
            pids.as_mut_ptr(),
            std::mem::size_of_val(&pids) as u32,
            &mut needed,
        ) != 0
        {
            let count = ((needed as usize) / std::mem::size_of::<u32>()).min(pids.len());
            let access = PROCESS_QUERY_INFORMATION | PROCESS_VM_READ;
            let pmc_size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            for &pid in &pids[..count] {
                if pid == 0 {
                    continue;
                }
                let handle = OpenProcess(access, 0, pid);
                if handle.is_null() {
                    continue;
                }
                let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                if K32GetProcessMemoryInfo(handle, &mut pmc, pmc_size) != 0 {
                    working_set_used += (pmc.WorkingSetSize / 1024 / 1024) as u64;
                }
                CloseHandle(handle);
            }
        }
    }

    let virtual_used = virtual_total.saturating_sub(virtual_available);
    let working_set_total = physical_total;
    let working_set_available = working_set_total.saturating_sub(working_set_used);

    Ok(DetailedMemoryInfo {
        physical_total,
        physical_used,
        physical_available,
        virtual_total,
        virtual_used,
        virtual_available,
        working_set_total,
        working_set_used,
        working_set_available,
    })
}

#[repr(C)]
struct MemoryPurgeStandbyListCommand {
    next: *mut std::ffi::c_void,
    command: u32,
}

/// 执行一条内存列表清理命令（SystemMemoryListInformation 下发给内核），需要管理员权限
fn purge_memory_list_command(command: u32) -> bool {
    unsafe {
        let mut cmd = MemoryPurgeStandbyListCommand {
            next: std::ptr::null_mut(),
            command,
        };
        NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &mut cmd as *mut _ as *const std::ffi::c_void,
            std::mem::size_of::<MemoryPurgeStandbyListCommand>() as u32,
        ) == 0
    }
}

/// 原生清空待机列表（standby list），需要管理员权限，失败返回 false
fn purge_standby_list_native() -> bool {
    purge_memory_list_command(MEMORY_PURGE_STANDBY_LIST)
}

/// 合并内存列表（Win10+，物理内存去重）
fn combine_memory_lists_native() -> bool {
    unsafe {
        #[repr(C)]
        struct MemoryCombineInformationEx {
            pages_combined: usize,
            pages_combined_failures: usize,
        }
        let mut info: MemoryCombineInformationEx = std::mem::zeroed();
        NtSetSystemInformation(
            SYSTEM_COMBINE_PHYSICAL_MEMORY_INFORMATION,
            &mut info as *mut _ as *const std::ffi::c_void,
            std::mem::size_of::<MemoryCombineInformationEx>() as u32,
        ) == 0
    }
}

/// 刷新注册表缓存（Win8.1+）
fn flush_registry_cache_native() -> bool {
    unsafe {
        NtSetSystemInformation(
            SYSTEM_REGISTRY_RECONCILIATION_INFORMATION,
            std::ptr::null(),
            0,
        ) == 0
    }
}

/// 回收系统文件缓存（压低上限再恢复）
fn reclaim_file_cache_native() {
    unsafe {
        SetSystemFileCacheSize(usize::MAX, usize::MAX, 0);
        thread::sleep(Duration::from_millis(400));
        SetSystemFileCacheSize(usize::MAX, usize::MAX, 1);
    }
}

/// 原生清理待机内存（standby 文件缓存 + 待机列表），无需 PowerShell
fn clean_standby_memory_inner() -> u64 {
    let before = get_memory_info();

    // 1) 清空待机列表（管理员权限下生效）
    let purged = purge_standby_list_native();

    // 2) 回收系统文件缓存
    reclaim_file_cache_native();

    if !purged {
        unsafe {
            // 权限不足时回退：收紧当前进程工作集（原逻辑兜底，保证至少执行一次清理动作）
            let handle = GetCurrentProcess();
            SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
        }
    }

    let after = get_memory_info();
    if after.available > before.available {
        after.available - before.available
    } else {
        0
    }
}

/// 按勾选项执行内存清理，返回释放的 MB。items 支持：
/// standby / low_pri_standby / modified / registry / combined / file_cache / working_set
fn clean_items_inner(items: &[String]) -> u64 {
    let before = get_memory_info();

    let mut purge_cmds: Vec<u32> = Vec::new();
    let mut has_file_cache = false;
    let mut has_working_set = false;

    for item in items {
        match item.as_str() {
            "standby" => purge_cmds.push(MEMORY_PURGE_STANDBY_LIST),
            "low_pri_standby" => purge_cmds.push(MEMORY_PURGE_LOW_PRIORITY_STANDBY_LIST),
            "modified" => purge_cmds.push(MEMORY_FLUSH_MODIFIED_LIST),
            "registry" => {
                flush_registry_cache_native();
            }
            "combined" => {
                combine_memory_lists_native();
            }
            "file_cache" => has_file_cache = true,
            "working_set" => has_working_set = true,
            _ => {}
        }
    }

    for cmd in purge_cmds {
        purge_memory_list_command(cmd);
    }
    if has_file_cache {
        reclaim_file_cache_native();
    }
    if has_working_set {
        trim_working_set_inner();
    }

    let after = get_memory_info();
    if after.available > before.available {
        after.available - before.available
    } else {
        0
    }
}

/// 进程名是否在受保护名单内（小写比较，不含 .exe）
fn is_protected_process_name(name: &str) -> bool {
    let name = name.trim_end_matches(".exe").trim().to_lowercase();
    PROTECTED_SYSTEM_PROCESSES
        .iter()
        .any(|p| name == *p)
}

/// 原生遍历所有进程并 EmptyWorkingSet（收紧工作集），无需 PowerShell
/// 跳过：自身、游戏进程（滤镜名单）、受保护的系统关键进程
fn trim_working_set_inner() -> u64 {
    // 收集正在运行的滤镜名单游戏进程，避免游戏刚启动就被砍工作集
    let mut sys = System::new();
    sys.refresh_processes();
    let game_pids = crate::game_filter::running_game_pids(&sys);

    unsafe {
        let mut pids: [u32; 4096] = [0; 4096];
        let mut needed: u32 = 0;
        if K32EnumProcesses(
            pids.as_mut_ptr(),
            std::mem::size_of_val(&pids) as u32,
            &mut needed,
        ) == 0
        {
            return 0;
        }
        let count = ((needed as usize) / std::mem::size_of::<u32>()).min(pids.len());
        let access = PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA | PROCESS_VM_READ;
        let pmc_size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let self_pid = std::process::id();
        let mut freed_mb: u64 = 0;

        for &pid in &pids[..count] {
            if pid == 0 || pid == self_pid || game_pids.contains(&pid) {
                continue;
            }
            let handle = OpenProcess(access, 0, pid);
            if handle.is_null() {
                continue;
            }
            // 受保护的系统关键进程跳过（explorer / dwm / svchost / lsass 等）
            let mut name_buf: [u16; 260] = [0; 260];
            let name_len =
                K32GetProcessImageFileNameW(handle, name_buf.as_mut_ptr(), name_buf.len() as u32);
            if name_len > 0 && name_len < name_buf.len() as u32 {
                let path = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let stem = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if is_protected_process_name(&stem) {
                    CloseHandle(handle);
                    continue;
                }
            }
            let mut pmc_before: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            if K32GetProcessMemoryInfo(handle, &mut pmc_before, pmc_size) == 0 {
                CloseHandle(handle);
                continue;
            }
            EmptyWorkingSet(handle);
            let mut pmc_after: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            K32GetProcessMemoryInfo(handle, &mut pmc_after, pmc_size);
            CloseHandle(handle);
            if pmc_before.WorkingSetSize > pmc_after.WorkingSetSize {
                freed_mb += ((pmc_before.WorkingSetSize - pmc_after.WorkingSetSize) / 1024 / 1024) as u64;
            }
        }
        freed_mb
    }
}

#[tauri::command]
pub async fn clean_standby_memory() -> Result<MemoryCleanupResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let freed = clean_standby_memory_inner();

    Ok(MemoryCleanupResult {
        success: true,
        message: if freed > 0 {
            format!("待机内存清理完成，释放 {} MB", freed)
        } else {
            "待机内存已清理".to_string()
        },
        freed_mb: freed,
    })
}

#[tauri::command]
pub async fn trim_system_working_set() -> Result<MemoryCleanupResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let freed = trim_working_set_inner();

    Ok(MemoryCleanupResult {
        success: true,
        message: if freed > 0 {
            format!("系统工作集已收紧，释放 {} MB", freed)
        } else {
            "系统工作集已收紧".to_string()
        },
        freed_mb: freed,
    })
}

/// 按勾选项执行内存清理。items 支持：
/// standby / low_pri_standby / modified / registry / combined / file_cache / working_set
#[tauri::command]
pub async fn clean_memory_selected(items: Vec<String>) -> Result<MemoryCleanupResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if items.is_empty() {
        return Err("未选择任何清理项".to_string());
    }

    let freed = clean_items_inner(&items);

    Ok(MemoryCleanupResult {
        success: true,
        message: if freed > 0 {
            format!("内存清理完成，释放 {} MB", freed)
        } else {
            "内存已清理".to_string()
        },
        freed_mb: freed,
    })
}

fn auto_clean_loop(config: AutoCleanConfig, generation: u64) {
    use std::time::Instant;

    const CHECK_INTERVAL_SECS: u64 = 5;
    let mut last_clean_time = Instant::now();

    loop {
        if AUTO_CLEAN_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }

        thread::sleep(Duration::from_secs(CHECK_INTERVAL_SECS));

        if AUTO_CLEAN_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }

        let mem_info = get_memory_info();
        let elapsed = last_clean_time.elapsed().as_secs();
        let interval_reached = elapsed >= config.interval_seconds;
        let threshold_reached = mem_info.used >= config.threshold_mb;

        if interval_reached || threshold_reached {
            match config.clean_type.as_str() {
                "all" => {
                    // 原生并行：待机缓存 + 工作集收紧
                    thread::scope(|s| {
                        s.spawn(|| {
                            clean_standby_memory_inner();
                        });
                        s.spawn(|| {
                            trim_working_set_inner();
                        });
                    });
                }
                "standby" => {
                    clean_standby_memory_inner();
                }
                "working_set" => {
                    trim_working_set_inner();
                }
                "items" => {
                    // 跟随用户勾选的清理项
                    let items = {
                        let cfg = MEMORY_CLEAN_CONFIG
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        cfg.as_ref()
                            .map(|c| c.items.clone())
                            .unwrap_or_else(default_memory_clean_items)
                    };
                    clean_items_inner(&items);
                }
                _ => {}
            }
            last_clean_time = Instant::now();
        }
    }
}

#[tauri::command]
pub async fn start_auto_clean(config: AutoCleanConfig) -> Result<(), String> {
    let gen = AUTO_CLEAN_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;

    let mut cfg = AUTO_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
    *cfg = Some(config.clone());
    drop(cfg);

    thread::spawn(move || {
        auto_clean_loop(config, gen);
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_auto_clean() -> Result<(), String> {
    AUTO_CLEAN_GENERATION.fetch_add(1, Ordering::Relaxed);
    let mut cfg = AUTO_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
    *cfg = None;
    Ok(())
}

#[tauri::command]
pub async fn get_auto_clean_config() -> Result<Option<AutoCleanConfig>, String> {
    let cfg = AUTO_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
    Ok(cfg.clone())
}

// ===== 内存清理勾选项配置（持久化到 memory_clean_config.json）=====

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct MemoryCleanConfig {
    #[serde(default)]
    pub items: Vec<String>,
}

static MEMORY_CLEAN_CONFIG: Mutex<Option<MemoryCleanConfig>> = Mutex::new(None);

/// 默认勾选项：待机列表 + 系统文件缓存 + 低优先级待机（安全、不砍进程工作集）
fn default_memory_clean_items() -> Vec<String> {
    vec![
        "standby".to_string(),
        "file_cache".to_string(),
        "low_pri_standby".to_string(),
    ]
}

fn load_memory_clean_config(app: &tauri::AppHandle) -> MemoryCleanConfig {
    match app.store("memory_clean_config.json") {
        Ok(store) => {
            if let Some(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value::<MemoryCleanConfig>(value) {
                    return config;
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open memory_clean_config store: {}", e);
        }
    }
    MemoryCleanConfig {
        items: default_memory_clean_items(),
    }
}

fn save_memory_clean_config(app: &tauri::AppHandle, config: &MemoryCleanConfig) {
    match app.store("memory_clean_config.json") {
        Ok(store) => {
            store.set("config", serde_json::to_value(config).unwrap());
            if let Err(e) = store.save() {
                log::error!("Failed to save memory_clean_config: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to open memory_clean_config store for saving: {}", e);
        }
    }
}

#[tauri::command]
pub async fn get_memory_clean_config(
    _app: tauri::AppHandle,
) -> Result<MemoryCleanConfig, String> {
    let cfg = MEMORY_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
    Ok(cfg.clone().unwrap_or_else(|| MemoryCleanConfig {
        items: default_memory_clean_items(),
    }))
}

#[tauri::command]
pub async fn set_memory_clean_config(
    app: tauri::AppHandle,
    items: Vec<String>,
) -> Result<MemoryCleanConfig, String> {
    // 空数组视为恢复默认
    let items = if items.is_empty() {
        default_memory_clean_items()
    } else {
        items
    };
    let config = MemoryCleanConfig { items };
    {
        let mut lock = MEMORY_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
        *lock = Some(config.clone());
    }
    save_memory_clean_config(&app, &config);
    Ok(config)
}

/// 应用启动时调用：恢复持久化的勾选项配置
pub async fn init_memory_clean_config(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_memory_clean_config(&app);
    {
        let mut lock = MEMORY_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
        *lock = Some(config.clone());
    }
    log::info!("[内存清理] 已恢复勾选项配置: {:?}", config.items);
    Ok(())
}

// ===== 游戏启动时自动清理内存 =====

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct GameStartCleanConfig {
    pub enabled: bool,
}

static GAME_START_CLEAN_CONFIG: Mutex<Option<GameStartCleanConfig>> = Mutex::new(None);
static GAME_START_CLEAN_GENERATION: AtomicU64 = AtomicU64::new(0);

fn load_game_start_clean_config(app: &tauri::AppHandle) -> GameStartCleanConfig {
    match app.store("game_start_clean.json") {
        Ok(store) => {
            if let Some(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value::<GameStartCleanConfig>(value) {
                    return config;
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open game_start_clean store: {}", e);
        }
    }
    GameStartCleanConfig::default()
}

fn save_game_start_clean_config(app: &tauri::AppHandle, config: &GameStartCleanConfig) {
    match app.store("game_start_clean.json") {
        Ok(store) => {
            store.set("config", serde_json::to_value(config).unwrap());
            if let Err(e) = store.save() {
                log::error!("Failed to save game_start_clean config: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to open game_start_clean store for saving: {}", e);
        }
    }
}

/// 后台轮询：检测到滤镜名单内游戏启动时自动清理一次内存（完整清理）
fn game_start_clean_loop(generation: u64) {
    let mut system = System::new();
    let mut was_running = false;
    loop {
        if GAME_START_CLEAN_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }
        thread::sleep(Duration::from_secs(2));
        if GAME_START_CLEAN_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }

        let enabled = {
            let cfg = GAME_START_CLEAN_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
            cfg.as_ref().map(|c| c.enabled).unwrap_or(false)
        };
        if !enabled {
            was_running = false;
            continue;
        }

        system.refresh_processes();
        let running = crate::game_filter::any_game_running(&system);
        if running && !was_running {
            // 按用户勾选的清理项执行（工作集整理已带白名单，游戏进程不会被砍）
            let items = {
                let cfg = MEMORY_CLEAN_CONFIG.lock().unwrap_or_else(|e| e.into_inner());
                cfg.as_ref()
                    .map(|c| c.items.clone())
                    .unwrap_or_else(default_memory_clean_items)
            };
            clean_items_inner(&items);
            log::info!("[游戏启动清理] 检测到滤镜名单游戏启动，已按勾选项自动清理一次内存");
        }
        was_running = running;
    }
}

#[tauri::command]
pub async fn get_game_start_clean_config(
    _app: tauri::AppHandle,
) -> Result<GameStartCleanConfig, String> {
    let cfg = GAME_START_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
    Ok(cfg.clone().unwrap_or_default())
}

#[tauri::command]
pub async fn set_game_start_clean_config(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<GameStartCleanConfig, String> {
    let config = GameStartCleanConfig { enabled };
    {
        let mut lock = GAME_START_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
        *lock = Some(config.clone());
    }
    save_game_start_clean_config(&app, &config);

    let gen = GAME_START_CLEAN_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    if enabled {
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| game_start_clean_loop(gen));
        });
        log::info!("[游戏启动清理] 已开启");
    } else {
        log::info!("[游戏启动清理] 已关闭");
    }
    Ok(config)
}

/// 应用启动时调用：恢复持久化配置，开启时启动后台轮询
pub async fn init_game_start_clean(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_game_start_clean_config(&app);
    {
        let mut lock = GAME_START_CLEAN_CONFIG.lock().map_err(|e| e.to_string())?;
        *lock = Some(config.clone());
    }
    if config.enabled {
        let gen = GAME_START_CLEAN_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| game_start_clean_loop(gen));
        });
        log::info!("[游戏启动清理] 已根据持久化配置启动后台轮询");
    }
    Ok(())
}

// ===== ACE 自动检测与优化 =====

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct AceAutoDetectConfig {
    pub enabled: bool,
}

#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct AceAutoDetectStats {
    pub is_running: bool,
    pub last_check: Option<String>,
    pub total_optimized: u32,
    pub currently_optimized: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct AceAutoDetectStatus {
    pub enabled: bool,
    pub is_running: bool,
    pub last_check: Option<String>,
    pub total_optimized: u32,
    pub currently_optimized: Vec<String>,
}

static AUTO_DETECT_CONFIG: Mutex<Option<AceAutoDetectConfig>> = Mutex::new(None);
static AUTO_DETECT_GENERATION: AtomicU64 = AtomicU64::new(0);
static AUTO_DETECT_STATS: Mutex<Option<AceAutoDetectStats>> = Mutex::new(None);

// 内存中的 enabled 状态，避免 store 读取竞争
static AUTO_DETECT_ENABLED: AtomicBool = AtomicBool::new(false);

const ACE_PROCESS_NAMES: &[&str] = &[
    "ACE-Tray.exe",
    "ACE-BASE.exe",
    "ACE-GAME.exe",
    "ACE-Client.exe",
    "SGuard64.exe",
    "SGuardSvc64.exe",
    "SGuardLite64.exe",
    "SGuardLite.exe",
    "SGuardSvc.exe",
];
const ACE_DETECT_INTERVAL_SECS: u64 = 5;

fn update_auto_detect_stats(optimized: Vec<String>) {
    let mut stats = AUTO_DETECT_STATS.lock().unwrap();
    if stats.is_none() {
        *stats = Some(AceAutoDetectStats::default());
    }
    if let Some(ref mut s) = *stats {
        s.is_running = true;
        s.last_check = Some(
            chrono::Local::now()
                .to_rfc3339()
        );
        s.total_optimized = s.total_optimized.saturating_add(optimized.len() as u32);
        s.currently_optimized = optimized;
    }
}

fn set_auto_detect_running(running: bool) {
    let mut stats = AUTO_DETECT_STATS.lock().unwrap();
    if let Some(ref mut s) = *stats {
        s.is_running = running;
    }
}

fn detect_and_optimize_ace_processes() -> Vec<String> {
    let mut optimized = Vec::new();
    let mut found_unauthorized = 0u32;

    let mut system = System::new();
    system.refresh_processes();

    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();

        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            let pid = process.pid().as_u32();
            let mut this_optimized = false;

            // 1. 启用调度优化（BELOW_NORMAL 优先级 + I/O VeryLow + 低内存优先级）
            if enable_process_efficiency_mode(pid) || set_process_low_priority(pid) {
                this_optimized = true;
            }

            // 2. 限制亲和性为 CPU0 (affinity = 1)，直接 Win32 API
            if set_process_affinity(pid, 1) {
                this_optimized = true;
            }

            if this_optimized {
                optimized.push(name);
            } else {
                found_unauthorized += 1;
            }
        }
    }

    if found_unauthorized > 0 {
        log::warn!(
            "[ACE auto-detect] 发现 {} 个 ACE 进程无法修改（需管理员权限），已跳过",
            found_unauthorized
        );
    }

    optimized
}

fn ace_auto_detect_loop(config: AceAutoDetectConfig, generation: u64) {
    // 启动时立即标记为运行中
    set_auto_detect_running(true);
    
    loop {
        if AUTO_DETECT_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }
        
        thread::sleep(Duration::from_secs(ACE_DETECT_INTERVAL_SECS));
        
        if AUTO_DETECT_GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }
        
        if !config.enabled {
            continue;
        }
        
        let optimized = detect_and_optimize_ace_processes();
        update_auto_detect_stats(optimized);
    }
    set_auto_detect_running(false);
}

async fn load_persisted_config(app: &tauri::AppHandle) -> AceAutoDetectConfig {
    match app.store("ace_auto_detect.json") {
        Ok(store) => {
            if let Some(value) = store.get("config") {
                if let Ok(config) = serde_json::from_value::<AceAutoDetectConfig>(value) {
                    return config;
                }
            }
        }
        Err(e) => {
            log::warn!("Failed to open ace_auto_detect store: {}", e);
        }
    }
    AceAutoDetectConfig::default()
}

async fn save_persisted_config(app: &tauri::AppHandle, config: &AceAutoDetectConfig) {
    match app.store("ace_auto_detect.json") {
        Ok(store) => {
            store.set("config", serde_json::to_value(config).unwrap());
            if let Err(e) = store.save() {
                log::error!("Failed to save ace_auto_detect config: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to open ace_auto_detect store for saving: {}", e);
        }
    }
}

#[tauri::command]
pub async fn set_ace_auto_detect(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    // 读取当前内存状态，避免重复操作
    let current = AUTO_DETECT_ENABLED.load(Ordering::Relaxed);
    if current == enabled {
        return Ok(()); // 状态未变，无需处理
    }
    
    let gen = AUTO_DETECT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    
    let config = AceAutoDetectConfig { enabled };
    
    {
        let mut cfg = AUTO_DETECT_CONFIG.lock().map_err(|e| e.to_string())?;
        *cfg = Some(config.clone());
    }
    
    // 更新内存状态（立即生效，供 status 读取）
    AUTO_DETECT_ENABLED.store(enabled, Ordering::Relaxed);
    
    // 持久化保存（异步，不阻塞）
    save_persisted_config(&app, &config).await;
    
    if enabled {
        thread::spawn(move || {
            // 捕获 panic，防止线程意外退出
            let _ = panic::catch_unwind(|| {
                ace_auto_detect_loop(config, gen);
            });
            set_auto_detect_running(false);
        });
    } else {
        set_auto_detect_running(false);
    }
    
    Ok(())
}

#[tauri::command]
pub async fn get_ace_auto_detect_status(_app: tauri::AppHandle) -> Result<AceAutoDetectStatus, String> {
    // 直接读取内存状态，避免 store 读取竞争
    let enabled = AUTO_DETECT_ENABLED.load(Ordering::Relaxed);
    
    let stats = AUTO_DETECT_STATS.lock().map_err(|e| e.to_string())?;
    let stats = stats.clone().unwrap_or_default();
    
    Ok(AceAutoDetectStatus {
        enabled,
        is_running: stats.is_running && enabled,
        last_check: stats.last_check,
        total_optimized: stats.total_optimized,
        currently_optimized: stats.currently_optimized,
    })
}

#[tauri::command]
pub async fn init_ace_auto_detect(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_persisted_config(&app).await;
    
    // 初始化内存状态
    AUTO_DETECT_ENABLED.store(config.enabled, Ordering::Relaxed);
    
    if config.enabled {
        let gen = AUTO_DETECT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        
        {
            let mut cfg = AUTO_DETECT_CONFIG.lock().map_err(|e| e.to_string())?;
            *cfg = Some(config.clone());
        }
        
        thread::spawn(move || {
            let _ = panic::catch_unwind(|| {
                ace_auto_detect_loop(config, gen);
            });
            set_auto_detect_running(false);
        });
    }
    
    Ok(())
}

#[derive(serde::Serialize)]
pub struct ProcessOptimizeResult {
    pub success: bool,
    pub message: String,
    pub process_name: String,
    pub was_running: bool,
}

#[tauri::command]
pub async fn boost_delta_force_priority() -> Result<ProcessOptimizeResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let target = "DeltaForceClient-Win64-Shipping.exe";
    let mut system = System::new();
    system.refresh_processes();

    let mut boosted = false;
    for (_, process) in system.processes() {
        if process.name().eq_ignore_ascii_case(target) {
            if set_process_realtime_priority(process.pid().as_u32()) {
                boosted = true;
            }
        }
    }

    if boosted {
        Ok(ProcessOptimizeResult {
            success: true,
            message: "三角洲进程优先级已提升为「超高」（实时）".to_string(),
            process_name: target.to_string(),
            was_running: true,
        })
    } else {
        // 检查是否找到了进程但改不动
        let found = system
            .processes()
            .values()
            .any(|p| p.name().eq_ignore_ascii_case(target));
        Ok(ProcessOptimizeResult {
            success: false,
            message: if found {
                "三角洲进程已运行，但优先级修改失败（进程受保护或权限不足）".to_string()
            } else {
                "三角洲游戏未运行，请先启动游戏".to_string()
            },
            process_name: target.to_string(),
            was_running: found,
        })
    }
}

#[derive(serde::Serialize)]
pub struct PriorityResult {
    pub success: bool,
    pub message: String,
    pub process_name: String,
    pub was_running: bool,
}

#[tauri::command]
pub async fn boost_delta_force_affinity() -> Result<PriorityResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let target = "DeltaForceClient-Win64-Shipping.exe";
    // 默认掩码：使用除 CPU0 外的所有核心（与原 PowerShell 版一致）
    let num_cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let all_cores_mask: u64 = if num_cores >= 64 { u64::MAX } else { (1u64 << num_cores) - 1 };
    let mask = all_cores_mask ^ 1;

    let mut system = System::new();
    system.refresh_processes();

    let mut found = false;
    let mut applied = false;
    for (_, process) in system.processes() {
        if process.name().eq_ignore_ascii_case(target) {
            found = true;
            if set_process_affinity(process.pid().as_u32(), mask) {
                applied = true;
            }
        }
    }

    Ok(PriorityResult {
        success: applied,
        message: if applied {
            "三角洲进程已设置为使用所有处理器核心".to_string()
        } else if found {
            "三角洲进程已运行，但核心分配修改失败（进程受保护或权限不足）".to_string()
        } else {
            "三角洲游戏未运行，请先启动游戏".to_string()
        },
        process_name: target.to_string(),
        was_running: found,
    })
}

#[derive(serde::Serialize)]
pub struct AcePartialResult {
    pub success: bool,
    pub message: String,
    pub count: u32,
    pub found_count: u32,
}

/// 根据 found/count 生成统一文案，区分"未找到"、"受保护无法修改"、"部分成功"、"全部成功"
fn ace_message(found: u32, count: u32, ok_template: &str) -> String {
    if found == 0 {
        return "未找到运行中的 ACE 进程".to_string();
    }
    if count == 0 {
        return format!(
            "发现 {} 个 ACE 进程，但无法修改（ACE 反作弊保护了这些进程）",
            found
        );
    }
    if count < found {
        return format!(
            "{}（另有 {} 个受 ACE 反作弊保护无法修改）",
            ok_template.replace("{}", &count.to_string()),
            found - count
        );
    }
    ok_template.replace("{}", &count.to_string())
}

#[tauri::command]
pub async fn limit_ace_priority() -> Result<AcePartialResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if !is_admin() {
        return Err("修改 ACE 进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    let mut found = 0u32;
    let mut count = 0u32;

    let mut system = System::new();
    system.refresh_processes();

    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();
        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            found += 1;
            if set_process_low_priority(process.pid().as_u32()) {
                count += 1;
            }
        }
    }

    Ok(AcePartialResult {
        success: count > 0,
        message: ace_message(found, count, "已限制 {} 个 ACE 进程优先级"),
        count,
        found_count: found,
    })
}

#[tauri::command]
pub async fn restrict_ace_affinity() -> Result<AcePartialResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 默认掩码 = 1，只使用 CPU0
    restrict_ace_affinity_impl(1, "已限制 {} 个 ACE 进程使用单核心")
}

#[tauri::command]
pub async fn restrict_ace_affinity_with_mask(mask: u64) -> Result<AcePartialResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    restrict_ace_affinity_impl(mask, "已限制 {} 个 ACE 进程使用指定核心")
}

/// ACE 亲和性限制的统一实现：直接 Win32 API，不走 PowerShell
fn restrict_ace_affinity_impl(mask: u64, ok_template: &str) -> Result<AcePartialResult, String> {
    if !is_admin() {
        return Err("修改 ACE 进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    let mut found = 0u32;
    let mut count = 0u32;

    let mut system = System::new();
    system.refresh_processes();

    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();
        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            found += 1;
            if set_process_affinity(process.pid().as_u32(), mask) {
                count += 1;
            }
        }
    }

    Ok(AcePartialResult {
        success: count > 0,
        message: ace_message(found, count, ok_template),
        count,
        found_count: found,
    })
}

// ===== 注册表强制限制（IFEO PerfOptions）=====
// 原理：Windows Image File Execution Options 的 PerfOptions 会在进程启动时
// 由系统直接应用 CPU 优先级 / I/O 优先级，无需进程运行时注入，且对
// 受保护的 ACE 内核进程也生效（进程无法自行改回，除非删除注册表项）。
// 注意：这是注册表级持久设置，会一直生效直到被恢复。

const ACE_IFEO_PERF: &[(&str, &[(&str, u32)])] = &[
    (
        "DeltaForceClient-Win64-Shipping.exe",
        &[("CpuPriorityClass", 3u32), ("IoPriority", 3u32)],
    ),
    (
        "SGuard64.exe",
        &[("CpuPriorityClass", 1u32), ("IoPriority", 1u32)],
    ),
    (
        "SGuardSvc64.exe",
        &[("CpuPriorityClass", 1u32), ("IoPriority", 1u32)],
    ),
];

fn ifeo_perf_options_path(process_name: &str) -> String {
    format!(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\{}\PerfOptions",
        process_name
    )
}

/// 应用注册表强制限制（IFEO PerfOptions），需要管理员权限
#[tauri::command]
pub async fn apply_ace_registry_limits() -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if !is_admin() {
        return Err("写入注册表需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    for &(proc_name, values) in ACE_IFEO_PERF {
        let path = ifeo_perf_options_path(proc_name);
        let (key, _) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .create_subkey(&path)
            .map_err(|e| format!("创建注册表键失败 ({}): {}", proc_name, e))?;
        for &(value_name, value) in values {
            key.set_value(value_name, &value)
                .map_err(|e| format!("写入注册表失败 ({} \\ {}): {}", proc_name, value_name, e))?;
        }
    }

    Ok(PerfTweakResult {
        success: true,
        message: "注册表强制限制已应用（进程启动时自动生效，需重启游戏/相关进程后生效）".to_string(),
    })
}

/// 恢复注册表强制限制：删除对应 PerfOptions 值（不影响其他自定义项）
#[tauri::command]
pub async fn restore_ace_registry_limits() -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if !is_admin() {
        return Err("写入注册表需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }

    for &(proc_name, values) in ACE_IFEO_PERF {
        let path = ifeo_perf_options_path(proc_name);
        let key = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(&path)
            .map_err(|e| format!("打开注册表键失败 ({}): {}", proc_name, e))?;
        for &(value_name, _) in values {
            let _ = key.delete_value(value_name);
        }
    }

    Ok(PerfTweakResult {
        success: true,
        message: "注册表强制限制已恢复（相关进程重启后生效）".to_string(),
    })
}

#[tauri::command]
pub async fn boost_delta_force_affinity_with_mask(mask: u64) -> Result<PriorityResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let target = "DeltaForceClient-Win64-Shipping.exe";
    let mut system = System::new();
    system.refresh_processes();

    let mut found = false;
    let mut applied = false;
    for (_, process) in system.processes() {
        if process.name().eq_ignore_ascii_case(target) {
            found = true;
            if set_process_affinity(process.pid().as_u32(), mask) {
                applied = true;
            }
        }
    }

    Ok(PriorityResult {
        success: applied,
        message: if applied {
            "三角洲进程已设置为使用指定处理器核心".to_string()
        } else if found {
            "三角洲进程已运行，但核心分配修改失败（进程受保护或权限不足）".to_string()
        } else {
            "三角洲游戏未运行，请先启动游戏".to_string()
        },
        process_name: target.to_string(),
        was_running: found,
    })
}

#[derive(serde::Serialize)]
pub struct AllGameOptimizeResult {
    pub success: bool,
    pub message: String,
    pub delta_boosted: bool,
    pub ace_limited: bool,
    pub ace_count: u32,
}

#[tauri::command]
pub async fn optimize_all_game_processes() -> Result<AllGameOptimizeResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let delta_target = "DeltaForceClient-Win64-Shipping.exe";

    let mut system = System::new();
    system.refresh_processes();

    // 1) DeltaForce: 提升优先级为 RealTime
    let mut delta_boosted = false;
    for (_, process) in system.processes() {
        if process.name().eq_ignore_ascii_case(delta_target) {
            if set_process_realtime_priority(process.pid().as_u32()) {
                delta_boosted = true;
            }
        }
    }

    // 2) ACE: 优先级降为 Idle + 限制亲和性为 CPU0
    let mut ace_found: u32 = 0;
    let mut ace_count: u32 = 0;
    for (_, process) in system.processes() {
        let name = process.name().to_string();
        let name_lower = name.to_lowercase();
        if ACE_PROCESS_NAMES.iter().any(|n| n.to_lowercase() == name_lower) {
            ace_found += 1;
            let pid = process.pid().as_u32();
            let priority_ok = set_process_low_priority(pid);
            let affinity_ok = set_process_affinity(pid, 1);
            if priority_ok || affinity_ok {
                ace_count += 1;
            }
        }
    }

    let ace_limited = ace_count > 0;

    let mut msgs: Vec<String> = Vec::new();
    if delta_boosted {
        msgs.push("三角洲: 已优化".to_string());
    } else {
        msgs.push("三角洲: 未运行".to_string());
    }
    if ace_found == 0 {
        msgs.push("ACE: 未运行".to_string());
    } else if ace_count == 0 {
        msgs.push(format!("ACE: 发现 {} 个进程，需管理员权限", ace_found));
    } else if ace_count < ace_found {
        msgs.push(format!(
            "ACE: 已限制 {} 个进程（另有 {} 个需管理员权限）",
            ace_count,
            ace_found - ace_count
        ));
    } else {
        msgs.push(format!("ACE: 已限制 {} 个进程", ace_count));
    }

    Ok(AllGameOptimizeResult {
        success: delta_boosted || ace_limited,
        message: msgs.join(" | "),
        delta_boosted,
        ace_limited,
        ace_count,
    })
}

#[derive(serde::Serialize, Clone)]
pub struct BuiltinPowerPlan {
    pub id: String,
    pub filename: String,
    pub name: String,
    pub description: String,
    pub is_imported: bool,
    pub guid: Option<String>,
    pub is_active: bool,
}

#[derive(serde::Serialize)]
pub struct SystemPowerPlan {
    pub guid: String,
    pub name: String,
    pub is_active: bool,
}

#[derive(serde::Serialize)]
pub struct ActivePowerPlan {
    pub guid: String,
    pub name: String,
}

#[derive(serde::Serialize)]
pub struct PowerPlanOperationResult {
    pub success: bool,
    pub message: String,
    pub guid: Option<String>,
}

#[derive(serde::Serialize)]
pub struct LaptopPowerLockStatus {
    /// 是否已解锁（PlatformAoAcOverride == 0）
    pub unlocked: bool,
    /// 当前注册表值（None 表示未设置）
    pub value: Option<u32>,
}

/// 读取 PlatformAoAcOverride 注册表值。
/// 该值用于覆盖平台 AoAc（Always On Always Connected）能力：
/// 现代待机（Modern Standby）笔记本厂商通过它锁定电源计划，
/// 设为 0 可解锁，使系统可自由导入/激活电源计划（需重启生效）。
fn read_platform_aoac_override() -> Option<u32> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let power = hklm
        .open_subkey(r"System\CurrentControlSet\Control\Power")
        .ok()?;
    power.get_value("PlatformAoAcOverride").ok()
}

/// 直接写入 PlatformAoAcOverride=0（需要管理员权限）
fn write_platform_aoac_override() -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (power, _) = hklm
        .create_subkey(r"System\CurrentControlSet\Control\Power")
        .map_err(|e| format!("打开注册表键失败: {}", e))?;
    power
        .set_value("PlatformAoAcOverride", &0u32)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    Ok(())
}

/// 通过 ShellExecuteEx 提权运行 reg.exe 写入（无 PowerShell，启动开销小）。
/// 应用非管理员时弹出 UAC，等待提权进程结束。
fn run_reg_add_elevated() -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let reg_path = format!(r"{}\System32\reg.exe", system_root);

    let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb_w = to_w("runas");
    let file_w = to_w(&reg_path);
    let args_w = to_w(
        "add HKLM\\System\\CurrentControlSet\\Control\\Power /v PlatformAoAcOverride /t REG_DWORD /d 0 /f",
    );

    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.lpVerb = PCWSTR(verb_w.as_ptr());
    sei.lpFile = PCWSTR(file_w.as_ptr());
    sei.lpParameters = PCWSTR(args_w.as_ptr());
    sei.nShow = SW_HIDE.0;

    if unsafe { ShellExecuteExW(&mut sei) }.is_err() {
        let code = unsafe { GetLastError() };
        return Err(format!(
            "需要管理员权限：提权失败（错误码 {}），可能是用户取消了授权",
            code
        ));
    }

    // 等待提权的 reg.exe 执行完毕
    unsafe { WaitForSingleObject(sei.hProcess, u32::MAX) };

    let mut exit_code: u32 = 0;
    if unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) }.is_err() {
        let _ = unsafe { CloseHandle(sei.hProcess) };
        return Err("无法获取 reg.exe 执行结果".to_string());
    }
    let _ = unsafe { CloseHandle(sei.hProcess) };

    if exit_code != 0 {
        return Err(format!("reg.exe 写入失败（退出码 {}）", exit_code));
    }
    Ok(())
}

/// 获取笔记本电源计划锁定状态
#[tauri::command]
pub async fn get_laptop_power_lock_status() -> Result<LaptopPowerLockStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    let value = read_platform_aoac_override();
    Ok(LaptopPowerLockStatus {
        unlocked: value == Some(0),
        value,
    })
}

/// 解锁笔记本电源计划（写入 PlatformAoAcOverride=0，需管理员权限，重启后生效）
#[tauri::command]
pub async fn unlock_laptop_power_plan() -> Result<LaptopPowerLockStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 已解锁则直接返回
    if read_platform_aoac_override() == Some(0) {
        return Ok(LaptopPowerLockStatus {
            unlocked: true,
            value: Some(0),
        });
    }

    // 1) 应用以管理员运行时直接写入（纯 winreg，零子进程，速度快）
    if write_platform_aoac_override().is_err() {
        // 2) 非管理员：ShellExecuteEx 提权运行 reg.exe（弹 UAC）
        run_reg_add_elevated()?;
    }

    // 3) 回读验证，以注册表实际值为准
    match read_platform_aoac_override() {
        Some(0) => Ok(LaptopPowerLockStatus {
            unlocked: true,
            value: Some(0),
        }),
        other => Err(format!(
            "解锁未生效（当前注册表值: {:?}）。请以管理员身份运行 NexBox 后重试",
            other
        )),
    }
}

fn get_builtin_plan_filename(id: &str) -> String {
    match id {
        "ggOSDesktopGaming" => "ggOS Desktop Gaming.pow".to_string(),
        _ => format!("{}.pow", id),
    }
}

fn get_builtin_plan_metadata(id: &str) -> (String, String) {
    match id {
        "ACMEPCAMD" => ("ACMEPCAMD".to_string(), "AMD平台极致性能优化，最大化CPU/GPU频率与响应".to_string()),
        "AMD电源计划" => ("AMD电源计划".to_string(), "AMD官方推荐高性能电源方案，适合Ryzen平台".to_string()),
        "ggOSDesktopGaming" => ("ggOS Desktop Gaming".to_string(), "桌面游戏场景深度优化，降低延迟提升帧率".to_string()),
        "Intel大核心电源计划" => ("Intel大核心电源计划".to_string(), "Intel大小核调度优化，优先使用大核心运行游戏".to_string()),
        "PowerX-v2" => ("PowerX v2".to_string(), "极致性能电源方案，最大化系统响应与游戏帧率".to_string()),
        "卓越性能" => ("卓越性能".to_string(), "Windows 卓越性能电源计划，解锁最高性能模式".to_string()),
        _ => (id.to_string(), String::new()),
    }
}

fn extract_guid_from_line(line: &str) -> Option<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    for part in parts {
        let segs: Vec<&str> = part.split('-').collect();
        if segs.len() == 5
            && segs[0].len() == 8
            && segs[1].len() == 4
            && segs[2].len() == 4
            && segs[3].len() == 4
            && segs[4].len() == 12
            && segs.iter().all(|s| s.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Some(part.to_string());
        }
    }
    None
}

fn parse_powercfg_list(output: &str) -> Vec<(String, String, bool)> {
    let mut plans = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(guid) = extract_guid_from_line(trimmed) {
            let is_active = trimmed.contains('*');
            let after_guid = trimmed.find(&guid).map(|pos| &trimmed[pos + guid.len()..]).unwrap_or("");
            let name = after_guid
                .trim()
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim()
                .trim_end_matches('*')
                .trim()
                .to_string();
            plans.push((guid, name, is_active));
        }
    }
    plans
}

/// 直接调用 powercfg.exe（通过 cmd 设置 UTF-8 代码页），避免 PowerShell 启动开销。
/// `chcp 65001` 确保中文输出不乱码。
fn run_powercfg(args: &[&str]) -> std::io::Result<std::process::Output> {
    let powercfg_args = args.join(" ");
    let full_cmd = format!("chcp 65001 >nul && powercfg {}", powercfg_args);
    Command::new("cmd")
        .args(&["/C", &full_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

fn get_system_plans_internal() -> Vec<(String, String, bool)> {
    let result = run_powercfg(&["/list"]);

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            parse_powercfg_list(&stdout)
        }
        Err(_) => Vec::new(),
    }
}

fn get_active_plan_internal() -> Option<(String, String)> {
    let result = run_powercfg(&["/getactivescheme"]);

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let trimmed = stdout.trim();
            if let Some(guid) = extract_guid_from_line(trimmed) {
                let after_guid = trimmed.find(&guid).map(|pos| &trimmed[pos + guid.len()..]).unwrap_or("");
                let name = after_guid
                    .trim()
                    .trim_start_matches('(')
                    .trim_end_matches(')')
                    .trim()
                    .to_string();
                Some((guid, name))
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

fn find_plan_guid_by_name(system_plans: &[(String, String, bool)], plan_name: &str) -> Option<String> {
    for (guid, name, _) in system_plans {
        if name.contains(plan_name) {
            return Some(guid.clone());
        }
        // 去掉末尾括号内的作者/后缀信息再匹配
        // 例如 "英特尔-KF系列提升平均帧计划(毒药制作" -> "英特尔-KF系列提升平均帧计划"
        if let Some(pos) = name.rfind('(') {
            let base_name = name[..pos].trim();
            if base_name.contains(plan_name) || plan_name.contains(base_name) {
                return Some(guid.clone());
            }
        }
    }
    None
}

fn resolve_power_plans_dir(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidates = [
            resource_dir.join("power-plans"),
            resource_dir.join("_up_").join("power-plans"),
        ];
        for path in &candidates {
            if path.exists() {
                return Some(path.clone());
            }
        }
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join("power-plans"),
                parent.join("_up_").join("power-plans"),
            ];
            for path in &candidates {
                if path.exists() {
                    return Some(path.clone());
                }
            }
        }
    }

    None
}

#[tauri::command]
pub async fn get_builtin_power_plans(app: tauri::AppHandle) -> Result<Vec<BuiltinPowerPlan>, String> {
    let power_plans_dir = resolve_power_plans_dir(&app)
        .ok_or("未找到电源计划文件目录，请确保 power-plans 文件夹存在")?;

    let system_plans = get_system_plans_internal();
    let active_plan = get_active_plan_internal();
    let active_guid = active_plan.as_ref().map(|(g, _)| g.as_str()).unwrap_or("");

    let builtin_ids = ["ACMEPCAMD", "AMD电源计划", "ggOSDesktopGaming", "Intel大核心电源计划", "PowerX-v2", "卓越性能"];

    let mut plans = Vec::new();

    for id in builtin_ids {
        let (display_name, description) = get_builtin_plan_metadata(id);
        let filename = get_builtin_plan_filename(id);
        let file_path = power_plans_dir.join(&filename);
        let file_exists = file_path.exists();

        let (is_imported, guid, is_active) = if file_exists {
            let matched_guid = find_plan_guid_by_name(&system_plans, &display_name);
            let active = matched_guid.as_ref().map(|g| g == active_guid).unwrap_or(false);
            (matched_guid.is_some(), matched_guid, active)
        } else {
            (false, None, false)
        };

        plans.push(BuiltinPowerPlan {
            id: id.to_string(),
            filename,
            name: display_name.to_string(),
            description: description.to_string(),
            is_imported,
            guid,
            is_active,
        });
    }

    Ok(plans)
}

#[tauri::command]
pub async fn get_system_power_plans() -> Result<Vec<SystemPowerPlan>, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let plans = get_system_plans_internal();
    Ok(plans.into_iter().map(|(guid, name, is_active)| SystemPowerPlan { guid, name, is_active }).collect())
}

#[tauri::command]
pub async fn get_active_power_plan() -> Result<ActivePowerPlan, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    match get_active_plan_internal() {
        Some((guid, name)) => Ok(ActivePowerPlan { guid, name }),
        None => Err("获取当前电源计划失败".to_string()),
    }
}

#[tauri::command]
pub async fn import_power_plan(app: tauri::AppHandle, plan_id: String) -> Result<PowerPlanOperationResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let (display_name, _) = get_builtin_plan_metadata(&plan_id);

    let system_plans_before = get_system_plans_internal();
    let guids_before: Vec<String> = system_plans_before.iter().map(|(g, _, _)| g.clone()).collect();

    if let Some(existing_guid) = find_plan_guid_by_name(&system_plans_before, &display_name) {
        return Ok(PowerPlanOperationResult {
            success: true,
            message: format!("电源计划 '{}' 已存在于系统中", display_name),
            guid: Some(existing_guid),
        });
    }

    let power_plans_dir = resolve_power_plans_dir(&app)
        .ok_or("未找到电源计划文件目录")?;
    let file_path = power_plans_dir.join(get_builtin_plan_filename(&plan_id));

    if !file_path.exists() {
        return Err(format!("电源计划文件不存在: {}", plan_id));
    }

    let file_path_str = file_path.to_string_lossy().to_string();
    let result = Command::new("powercfg")
        .args(["/import", &file_path_str])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else if !stdout.trim().is_empty() { stdout.trim().to_string() } else { "未知错误".to_string() };
                return Err(format!("导入电源计划失败: {}\n如果您是笔记本，请先点击「笔记本电源计划解锁」，然后重启电脑重试", err_msg));
            }

            std::thread::sleep(std::time::Duration::from_millis(800));

            let system_plans_after = get_system_plans_internal();
            
            let mut new_guid: Option<String> = None;
            for (guid, _, _) in &system_plans_after {
                if !guids_before.contains(guid) {
                    new_guid = Some(guid.clone());
                    break;
                }
            }

            if let Some(guid) = new_guid {
                Ok(PowerPlanOperationResult {
                    success: true,
                    message: format!("电源计划 '{}' 导入成功", display_name),
                    guid: Some(guid),
                })
            } else if let Some(guid) = find_plan_guid_by_name(&system_plans_after, &display_name) {
                Ok(PowerPlanOperationResult {
                    success: true,
                    message: format!("电源计划 '{}' 导入成功", display_name),
                    guid: Some(guid),
                })
            } else {
                Err(format!("电源计划 '{}' 导入后未在系统中找到，可能导入失败。\n如果您是笔记本，请先点击「笔记本电源计划解锁」，然后重启电脑重试", display_name))
            }
        }
        Err(e) => Err(format!("执行导入命令失败: {}", e)),
    }
}

#[tauri::command]
pub async fn activate_power_plan(guid: String) -> Result<PowerPlanOperationResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let result = Command::new("powercfg")
        .args(["/setactive", &guid])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let verify = get_active_plan_internal();
                match verify {
                    Some((active_guid, active_name)) => {
                        if active_guid == guid {
                            Ok(PowerPlanOperationResult {
                                success: true,
                                message: format!("电源计划 '{}' 已激活", active_name),
                                guid: Some(guid),
                            })
                        } else {
                            Ok(PowerPlanOperationResult {
                                success: true,
                                message: "激活命令已执行，请确认是否生效".to_string(),
                                guid: Some(guid),
                            })
                        }
                    }
                    None => Ok(PowerPlanOperationResult {
                        success: true,
                        message: "激活命令已执行".to_string(),
                        guid: Some(guid),
                    }),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else if !stdout.trim().is_empty() { stdout.trim().to_string() } else { "未知错误".to_string() };
                Err(format!("电源计划激活失败: {}\n如果您是笔记本，请先点击「笔记本电源计划解锁」，然后重启电脑重试", err_msg))
            }
        }
        Err(e) => Err(format!("执行激活命令失败: {}", e)),
    }
}

#[tauri::command]
pub async fn import_and_activate_power_plan(app: tauri::AppHandle, plan_id: String) -> Result<PowerPlanOperationResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let (display_name, _) = get_builtin_plan_metadata(&plan_id);

    let system_plans_before = get_system_plans_internal();
    let guids_before: Vec<String> = system_plans_before.iter().map(|(g, _, _)| g.clone()).collect();
    let existing_guid = find_plan_guid_by_name(&system_plans_before, &display_name);

    let (guid, was_existing) = match existing_guid {
        Some(g) => (g, true),
        None => {
            let power_plans_dir = resolve_power_plans_dir(&app)
                .ok_or("未找到电源计划文件目录")?;
            let file_path = power_plans_dir.join(get_builtin_plan_filename(&plan_id));

            if !file_path.exists() {
                return Err(format!("电源计划文件不存在: {}", plan_id));
            }

            let file_path_str = file_path.to_string_lossy().to_string();
            let import_result = Command::new("powercfg")
                .args(["/import", &file_path_str])
                .creation_flags(CREATE_NO_WINDOW)
                .output();

            let g = match import_result {
                Ok(output) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                        let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else if !stdout.trim().is_empty() { stdout.trim().to_string() } else { "未知错误".to_string() };
                        return Err(format!("导入失败: {}\n如果您是笔记本，请先点击「笔记本电源计划解锁」，然后重启电脑重试", err_msg));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let system_plans_after = get_system_plans_internal();
                    
                    let mut new_guid: Option<String> = None;
                    for (guid, _, _) in &system_plans_after {
                        if !guids_before.contains(guid) {
                            new_guid = Some(guid.clone());
                            break;
                        }
                    }

                    if let Some(g) = new_guid {
                        g
                    } else if let Some(g) = find_plan_guid_by_name(&system_plans_after, &display_name) {
                        g
                    } else {
                        return Err(format!("电源计划 '{}' 导入后未在系统中找到，可能导入失败。\n如果您是笔记本，请先点击「笔记本电源计划解锁」，然后重启电脑重试", display_name));
                    }
                }
                Err(e) => return Err(format!("导入失败: {}", e)),
            };
            (g, false)
        }
    };

    let activate_result = activate_power_plan(guid.clone()).await?;
    Ok(PowerPlanOperationResult {
        success: true,
        message: if was_existing {
            format!("电源计划 '{}' 已存在，{}", display_name, activate_result.message)
        } else {
            format!("电源计划 '{}' 导入并激活成功", display_name)
        },
        guid: Some(guid),
    })
}

#[derive(serde::Serialize)]
pub struct PerfTweakResult {
    pub success: bool,
    pub message: String,
}

/// 通用 PowerShell 脚本执行工具：将脚本写入临时 .ps1 文件并通过 -File 参数执行，
/// 避免 Windows 命令行长度限制（错误 206）。
fn run_ps_script(script: &str) -> Result<std::process::Output, String> {
    let ps_path = get_powershell_path();
    let tmp_dir = std::env::temp_dir();
    let script_path = tmp_dir.join(format!("nexbox_{}.ps1", std::process::id()));
    fs::write(&script_path, script).map_err(|e| format!("写入临时脚本失败: {}", e))?;
    let result = Command::new(&ps_path)
        .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &script_path.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行命令失败: {}", e));
    let _ = fs::remove_file(&script_path);
    result
}

/// 执行 PowerShell 脚本并返回统一结果，自动处理权限错误
pub(crate) fn run_simple_feature(script: &str) -> Result<PerfTweakResult, String> {
    let result = run_ps_script(script)?;
    if result.status.success() {
        Ok(PerfTweakResult { success: true, message: "操作成功".to_string() })
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else { stdout.trim().to_string() };
        let lower = err_msg.to_lowercase();
        if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
            Err("需要管理员权限，请以管理员身份运行 NexBox".to_string())
        } else {
            Err(format!("操作失败: {}", err_msg))
        }
    }
}

// === Windows Update Disable/Enable (Pure Rust, no PowerShell) ===

/// Convert a Rust string to a null-terminated wide string for Windows API calls.
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Helper: clear failure actions for a service (prevents auto-restart/reboot).
/// Uses sc.exe to reset failure actions.
fn clear_service_failure_actions(service_name: &str) -> Result<(), String> {
    // sc.exe failure <svc> reset=0 actions=""
    let result = std::process::Command::new("sc.exe")
        .args(&["failure", service_name, "reset=", "0", "actions=", ""])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 sc.exe 失败: {}", e))?;

    if !result.status.success() {
        let err = String::from_utf8_lossy(&result.stderr);
        if !err.trim().is_empty() && !err.contains("FAIL") {
            // Non-fatal for some services
            log::info!("sc.exe failure {} 输出: {}", service_name, err.trim());
        }
    }
    Ok(())
}

/// Helper: set a service start type.
/// Preferred method: `sc.exe config` — goes through the SCM (Service Control Manager)
/// official API, which updates the service configuration reliably even for ACL-protected
/// services (wuauserv/UsoSvc/WaaSMedicSvc are owned by TrustedInstaller). Direct reg add
/// on those keys is silently ignored in some environments (reg add returns 0 but the
/// value is unchanged), which is exactly the bug we observed.
/// start_type: 2=auto, 3=demand, 4=disabled
/// Returns Err if the start type could not be set (so callers don't claim success).
fn set_service_start_reg(service_name: &str, start_type: u32) -> Result<(), String> {
    let type_str = match start_type {
        2 => "auto",
        3 => "demand",
        4 => "disabled",
        _ => return Err(format!("未知的启动类型: {}", start_type)),
    };

    // Method 1: sc.exe config (SCM API) — the official & reliable way.
    // NOTE: "start=" and the value MUST be separate args. sc.exe parses them as
    // two tokens ("start= disabled"); a single "start= disabled" arg would be
    // quoted by Rust and rejected with "无效 start= 参数".
    let sc_output = std::process::Command::new("sc.exe")
        .args(["config", service_name, "start=", type_str])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 sc.exe config 失败: {}", e))?;

    if sc_output.status.success() {
        let out = String::from_utf8_lossy(&sc_output.stdout);
        let err = String::from_utf8_lossy(&sc_output.stderr);
        log::info!(
            "sc.exe config {} start={} 输出: {} {}",
            service_name, type_str,
            out.trim(), err.trim()
        );
        // Verify through the registry (SCM writes Start there as well).
        if get_service_start(service_name) == Some(start_type) {
            log::info!("服务 {} Start={} 设置成功 (sc config)", service_name, start_type);
            return Ok(());
        }
        log::warn!(
            "sc.exe config 后注册表 Start 仍为 {:?}，尝试 reg add",
            get_service_start(service_name)
        );
    } else {
        let err = String::from_utf8_lossy(&sc_output.stderr);
        let out = String::from_utf8_lossy(&sc_output.stdout);
        log::warn!(
            "sc.exe config {} start= {} 失败: {} {}",
            service_name, type_str,
            out.trim(), err.trim()
        );
    }

    // Method 2 (fallback): direct reg add, retried up to 3 times with verification.
    // Note: for TrustedInstaller-protected keys this may silently no-op; WaaSMedicSvc
    // is fully covered by the DLL rename step in disable_windows_update instead.
    let cmd = format!(
        "reg add \"HKLM\\SYSTEM\\CurrentControlSet\\Services\\{}\" /v Start /t REG_DWORD /d {} /f",
        service_name, start_type
    );
    let mut last_msg = String::new();
    for attempt in 1..=3 {
        let output = std::process::Command::new("cmd.exe")
            .args(&["/c", &cmd])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("执行 cmd/reg 失败: {}", e))?;

        if output.status.success() {
            let after = get_service_start(service_name);
            log::info!("reg add 尝试 {} 后 {} Start = {:?}", attempt, service_name, after);
            if after == Some(start_type) {
                log::info!("服务 {} Start={} 设置成功 (reg add)", service_name, start_type);
                return Ok(());
            }
            last_msg = format!("写入后 Start = {:?}", after);
        } else {
            last_msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        thread::sleep(Duration::from_millis(200 * attempt as u64));
    }
    Err(format!("设置服务 {} Start={} 失败: {}", service_name, start_type, last_msg))
}

/// Helper: control a service (stop/start).
/// For STOP, it sends the stop control and waits (with retries) until the service
/// actually reaches the STOPPED state, so the running check reflects reality.
unsafe fn control_service(service_name: &str, control: u32) -> Result<(), String> {
    use windows_sys::Win32::System::Services::{
        OpenSCManagerW, OpenServiceW, ControlService, CloseServiceHandle, StartServiceW,
        QueryServiceStatus,
        SC_MANAGER_CONNECT, SERVICE_STOP, SERVICE_START,
        SERVICE_QUERY_STATUS, SERVICE_STOPPED, SERVICE_RUNNING, SERVICE_START_PENDING,
        SERVICE_STOP_PENDING, SERVICE_CONTINUE_PENDING, SERVICE_PAUSE_PENDING, SERVICE_PAUSED,
        SERVICE_CONTROL_STOP,
    };

    let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
    if scm.is_null() {
        return Err(format!("无法打开 SCM (服务控制)"));
    }

    let svc_name = to_wide(service_name);
    let access = if control == SERVICE_CONTROL_STOP { SERVICE_STOP | SERVICE_QUERY_STATUS }
                 else { SERVICE_START | SERVICE_QUERY_STATUS };
    let svc = OpenServiceW(scm, svc_name.as_ptr(), access);
    if svc.is_null() {
        CloseServiceHandle(scm);
        // Service not found or not accessible – not fatal for disable
        return Ok(());
    }

    let mut status: windows_sys::Win32::System::Services::SERVICE_STATUS = std::mem::zeroed();

    if control == SERVICE_CONTROL_STOP {
        let _ = QueryServiceStatus(svc, &mut status);
        // Skip if already stopped
        if status.dwCurrentState == SERVICE_STOPPED {
            CloseServiceHandle(svc);
            CloseServiceHandle(scm);
            return Ok(());
        }

        // Send stop control, retrying while it is still stopping
        for _ in 0..3 {
            if status.dwCurrentState == SERVICE_STOPPED {
                break;
            }
            if status.dwCurrentState != SERVICE_STOP_PENDING {
                let mut s = std::mem::zeroed();
                ControlService(svc, SERVICE_CONTROL_STOP, &mut s);
            }
            // Wait for the stop to progress (wait up to ~30s total)
            for _ in 0..20 {
                thread::sleep(Duration::from_millis(250));
                let _ = QueryServiceStatus(svc, &mut status);
                if status.dwCurrentState == SERVICE_STOPPED {
                    break;
                }
                // Stop pending may need multiple wait hints; keep polling
                if status.dwCurrentState == SERVICE_RUNNING
                    || status.dwCurrentState == SERVICE_START_PENDING
                    || status.dwCurrentState == SERVICE_CONTINUE_PENDING
                    || status.dwCurrentState == SERVICE_PAUSE_PENDING
                    || status.dwCurrentState == SERVICE_PAUSED
                {
                    break; // stop attempt apparently failed / not progressing; try again
                }
            }
            if status.dwCurrentState == SERVICE_STOPPED {
                break;
            }
        }
    } else {
        // Start
        StartServiceW(svc, 0, std::ptr::null_mut());
        // Wait for start to complete
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(250));
            let _ = QueryServiceStatus(svc, &mut status);
            if status.dwCurrentState == SERVICE_RUNNING || status.dwCurrentState == SERVICE_STOPPED {
                break;
            }
        }
    }

    CloseServiceHandle(svc);
    CloseServiceHandle(scm);
    Ok(())
}

/// Helper: kill a process by name.
fn kill_process(name: &str) {
    let _ = std::process::Command::new("taskkill")
        .args(&["/f", "/im", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Helper: disable or enable Windows Update scheduled tasks via schtasks.exe (no PowerShell).
/// Tasks that Windows refuses to toggle (e.g. "Refresh Group Policy Cache" is
/// TrustedInstaller-protected and can NEVER be disabled by any tool) are skipped with
/// a warning instead of failing the whole operation — same behavior as Winhance.
fn schtasks_wu_tasks(enable: bool) -> Result<(), String> {
    let sch_action = if enable { "/enable" } else { "/disable" };

    // One full query to get task name + current status.
    // CSV format: "TaskName","Next Run Time","Status"
    let query = std::process::Command::new("schtasks")
        .args(["/query", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let out = match query {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            log::warn!("schtasks /query 失败，无法处理计划任务");
            return Ok(());
        }
    };

    // Collect tasks belonging to WU-related folders, with their current status.
    let mut tasks: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        if !(lower.contains("windowsupdate")
            || lower.contains("updateorchestrator")
            || lower.contains("waasmedic")
            || lower.contains("updateassistant")
            || lower.contains("installservice"))
        {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 3 {
            continue;
        }
        let name = fields[0].trim().trim_matches('"').to_string();
        let status = fields[2].trim().trim_matches('"').to_string();
        if name.is_empty() {
            continue;
        }
        if !tasks.iter().any(|(n, _)| n == &name) {
            tasks.push((name, status));
        }
    }

    log::info!("找到 {} 个 Windows Update 相关计划任务", tasks.len());

    if tasks.is_empty() {
        log::warn!("未找到任何 Windows Update 计划任务，跳过");
        return Ok(());
    }

    let mut skipped: Vec<String> = Vec::new();
    let mut changed: usize = 0;
    let mut already: usize = 0;

    for (task_path, status) in &tasks {
        // schtasks output language depends on system locale (Disabled / 已禁用).
        let is_disabled = status.eq_ignore_ascii_case("Disabled") || status == "已禁用";
        let is_ready = status.eq_ignore_ascii_case("Ready") || status == "就绪";
        // Skip tasks already in the desired state (fast path, no schtasks call).
        if enable && is_disabled {
            log::info!("计划任务 {} 已禁用，需启用", task_path);
        } else if enable && is_ready {
            already += 1;
            continue;
        } else if !enable && is_disabled {
            already += 1;
            continue;
        }

        let result = std::process::Command::new("schtasks")
            .args(&["/change", "/tn", task_path, sch_action])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    log::info!(
                        "schtasks {} {} -> 成功",
                        sch_action,
                        task_path
                    );
                    changed += 1;
                } else {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    let err_str = String::from_utf8_lossy(&output.stderr);
                    log::warn!(
                        "schtasks {} {} -> {} {}",
                        sch_action,
                        task_path,
                        out_str.trim(),
                        err_str.trim()
                    );
                    // Windows-protected task (e.g. Refresh Group Policy Cache).
                    // It cannot be toggled even by SYSTEM; skip, not fatal.
                    skipped.push(task_path.clone());
                }
            }
            Err(e) => {
                log::error!("schtasks 调用失败: {}", e);
                skipped.push(task_path.clone());
            }
        }
    }

    log::info!(
        "计划任务处理完成: 成功 {} 个，跳过 {} 个，已处于目标状态 {} 个",
        changed,
        skipped.len(),
        already
    );

    Ok(())
}

/// Check if any WU scheduled task is disabled — via `schtasks.exe /query` (no PowerShell).
/// A task is considered "disabled" if its CSV Status is "Disabled".
/// Lenient: any disabled WindowsUpdate task (e.g. "Scheduled Start") counts as success.
/// Note: "Refresh Group Policy Cache" is Windows-protected and can never be disabled
/// by any tool, so it is excluded from the judgment.
fn check_schtasks_wu_disabled() -> bool {
    let output = std::process::Command::new("schtasks")
        .args(["/query", "/fo", "csv", "/nh"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let out = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => {
            log::warn!("schtasks /query 失败，回退到注册表 TaskCache");
            return check_schtasks_wu_disabled_registry();
        }
    };

    let mut disabled_any = false;
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        // CSV: "TaskName","Next Run Time","Status"
        let lower = line.to_lowercase();
        if !(lower.contains("windowsupdate")
            || lower.contains("updateorchestrator")
            || lower.contains("waasmedic"))
        {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() >= 3 {
            let status = fields[2].trim().trim_matches('"');
            // schtasks output language depends on system locale (Disabled / 已禁用).
            if status.eq_ignore_ascii_case("Disabled") || status == "已禁用" {
                let name = fields[0].trim().trim_matches('"');
                log::info!("已禁用的计划任务: {}", name);
                disabled_any = true;
            }
        }
    }

    disabled_any
}

/// Fallback: check the TaskCache registry for a disabled State (used if PowerShell fails).
fn check_schtasks_wu_disabled_registry() -> bool {
    const TREE_PATH: &str =
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\TaskCache\Tree\Microsoft\Windows\WindowsUpdate";
    const TASKS_PATH: &str =
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\TaskCache\Tasks";
    const STATE_DISABLED: u32 = 1;

    fn task_state_disabled(hklm: &RegKey, tasks_path: &str, key: &RegKey) -> bool {
        // 1) 任务键自身的 State
        if let Ok(v) = key.get_value::<u32, _>("State") {
            if v == STATE_DISABLED {
                return true;
            }
        }
        // 2) 通过 Id 定位到 TaskCache\Tasks\{guid} 读取 State
        if let Ok(id) = key.get_value::<String, _>("Id") {
            if let Ok(task_key) = hklm.open_subkey(format!(r"{}\{}", tasks_path, id)) {
                if let Ok(v) = task_key.get_value::<u32, _>("State") {
                    if v == STATE_DISABLED {
                        return true;
                    }
                }
            }
        }
        // 3) 递归子键
        for child in key.enum_keys().flatten() {
            if let Ok(sub) = key.open_subkey(child) {
                if task_state_disabled(hklm, tasks_path, &sub) {
                    return true;
                }
            }
        }
        false
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.open_subkey(TREE_PATH) {
        Ok(root) => task_state_disabled(&hklm, TASKS_PATH, &root),
        Err(_) => false,
    }
}

/// Rename the critical Windows Update DLLs (WaaSMedicSvc.dll, wuaueng.dll) to a
/// ._BAK.dll backup. This is the key hardening step (based on Chris Titus / Winhance):
/// even if Windows re-enables WaaSMedicSvc, the service cannot run without its DLL,
/// so Windows Update stays fully disabled.
/// Returns (renamed, skipped).
fn rename_critical_update_dlls() -> (Vec<String>, Vec<String>) {
    const DLLS: [&str; 2] = ["WaaSMedicSvc.dll", "wuaueng.dll"];
    let sys32 = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let sys32 = format!(r"{}\System32", sys32);

    let mut renamed = Vec::new();
    let mut skipped = Vec::new();

    for dll in DLLS {
        let dll_path = format!(r"{}\{}", sys32, dll);
        let backup_path = format!(r"{}\{}", sys32, dll.replace(".dll", "_BAK.dll"));

        if !Path::new(&dll_path).exists() {
            if Path::new(&backup_path).exists() {
                renamed.push(dll.to_string()); // already renamed previously
            } else {
                skipped.push(format!("{} (不存在)", dll));
            }
            continue;
        }

        if Path::new(&backup_path).exists() {
            // Original exists AND backup exists: stale backup, remove it first
            let _ = std::process::Command::new("takeown")
                .args(["/f", &backup_path])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = std::process::Command::new("icacls")
                .args([&backup_path, "/grant", "*S-1-1-0:F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = fs::remove_file(&backup_path);
        }

        // take ownership + grant full control, then rename
        let _ = std::process::Command::new("takeown")
            .args(["/f", &dll_path])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = std::process::Command::new("icacls")
            .args([&dll_path, "/grant", "*S-1-1-0:F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Err(e) = fs::rename(&dll_path, &backup_path) {
            log::warn!("重命名 {} 失败: {}", dll, e);
            skipped.push(format!("{} ({})", dll, e));
        } else {
            log::info!("已重命名 {} -> _BAK.dll", dll);
            renamed.push(dll.to_string());
        }
    }

    (renamed, skipped)
}

/// Restore the critical Windows Update DLLs from their _BAK.dll backups.
fn restore_critical_update_dlls() {
    const DLLS: [&str; 2] = ["WaaSMedicSvc.dll", "wuaueng.dll"];
    let sys32 = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let sys32 = format!(r"{}\System32", sys32);

    for dll in DLLS {
        let dll_path = format!(r"{}\{}", sys32, dll);
        let backup_path = format!(r"{}\{}", sys32, dll.replace(".dll", "_BAK.dll"));

        if !Path::new(&backup_path).exists() {
            continue;
        }

        let _ = std::process::Command::new("takeown")
            .args(["/f", &backup_path])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = std::process::Command::new("icacls")
            .args([&backup_path, "/grant", "*S-1-1-0:F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if Path::new(&dll_path).exists() {
            // System already restored it; remove stale backup
            let _ = fs::remove_file(&backup_path);
            log::info!("{} 已恢复，删除多余备份", dll);
        } else if let Err(e) = fs::rename(&backup_path, &dll_path) {
            log::warn!("恢复 {} 失败: {}", dll, e);
        } else {
            log::info!("已恢复 {}", dll);
        }
    }
}

/// Clean up the Windows Update download cache folder.
fn cleanup_software_distribution() {
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let path = format!(r"{}\SoftwareDistribution", sysroot);
    // No PowerShell: clear the folder via cmd rd /s /q (ignore failures — files may be in use).
    let cmd = format!("rd /s /q \"{}\"", path);
    let _ = std::process::Command::new("cmd.exe")
        .args(&["/c", &cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    log::info!("已清理 SoftwareDistribution 文件夹");
}

#[tauri::command]
pub async fn disable_windows_update() -> Result<String, String> {
    log::info!("开始关闭 Windows Update...");
    let mut warnings: Vec<String> = Vec::new();

    // 1. Stop services
    let services = ["wuauserv", "UsoSvc", "WaaSMedicSvc"];
    for svc in &services {
        log::info!("停止服务: {}", svc);
        unsafe {
            control_service(svc, windows_sys::Win32::System::Services::SERVICE_CONTROL_STOP)
                .unwrap_or_else(|e| log::error!("停止服务 {} 失败: {}", svc, e));
        }
    }

    // 2. Kill UsoClient.exe
    log::info!("终止 UsoClient.exe 进程");
    kill_process("UsoClient.exe");

    // 3. Set service start types to disabled (4) + clear failure actions
    for svc in &services {
        log::info!("禁用服务启动: {}", svc);
        if let Err(e) = set_service_start_reg(svc, 4) {
            log::error!("禁用 {} 服务启动失败: {}", svc, e);
            warnings.push(format!("{}: {}", svc, e));
        }
        let after = get_service_start(svc);
        log::info!("服务 {} 禁用后 Start = {:?}", svc, after);
        if after != Some(4) {
            warnings.push(format!("{} 的 Start 值未生效(当前={:?})", svc, after));
        }
        clear_service_failure_actions(svc)
            .unwrap_or_else(|e| log::error!("清空失败恢复 {} 失败: {}", svc, e));
    }

    // 4. Disable scheduled tasks (protected ones are skipped, not fatal)
    log::info!("禁用 Windows Update 计划任务");
    schtasks_wu_tasks(false)?;

    // 5. Rename critical DLLs — this is what truly prevents WaaSMedicSvc/wuaueng
    //    from being revived even if Windows tries to repair them.
    log::info!("重命名关键 Windows Update DLL");
    let (renamed, skipped) = rename_critical_update_dlls();
    if renamed.is_empty() {
        warnings.push(format!("未能重命名任何关键 DLL: {:?}", skipped));
    }

    // 6. Registry policies
    log::info!("设置注册表策略: NoAutoUpdate, AUOptions");
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let (wu_key, _) = hklm.create_subkey(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU"
    ).map_err(|e| format!("创建/打开注册表键失败: {}", e))?;
    wu_key.set_value("NoAutoUpdate", &1u32).map_err(|e| format!("设置 NoAutoUpdate 失败: {}", e))?;
    wu_key.set_value("AUOptions", &1u32).map_err(|e| format!("设置 AUOptions 失败: {}", e))?;

    // Optional: DisableWindowsUpdateAccess for Pro/Enterprise
    let (wu_policy_key, _) = hklm.create_subkey(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate"
    ).map_err(|e| format!("创建 WindowsUpdate 策略键失败: {}", e))?;
    wu_policy_key.set_value("DisableWindowsUpdateAccess", &1u32)
        .unwrap_or_else(|e| log::info!("DisableWindowsUpdateAccess 设置跳过（非 Pro/Enterprise 系统）: {}", e));

    // 7. Clean up downloaded update files
    log::info!("清理 Windows Update 下载缓存");
    cleanup_software_distribution();

    if warnings.is_empty() {
        log::info!("Windows Update 已彻底关闭");
        Ok("Windows Update 已彻底关闭".to_string())
    } else {
        log::warn!("Windows Update 关闭完成，但有警告: {:?}", warnings);
        Ok(format!("Windows Update 已关闭（部分项需注意: {}）", warnings.join("; ")))
    }
}

#[tauri::command]
pub async fn enable_windows_update() -> Result<String, String> {
    log::info!("开始恢复 Windows Update...");
    let mut warnings: Vec<String> = Vec::new();

    // 1. Restore critical DLLs first (they block the update services)
    log::info!("恢复关键 Windows Update DLL");
    restore_critical_update_dlls();

    // 2. Restore service start types
    let services_config = [
        ("wuauserv", 3u32),   // demand
        ("UsoSvc", 2u32),     // auto
        ("WaaSMedicSvc", 3u32), // demand
    ];

    for (svc, start_type) in &services_config {
        log::info!("恢复服务启动类型: {} -> {}", svc, start_type);
        if let Err(e) = set_service_start_reg(svc, *start_type) {
            warnings.push(format!("{}: {}", svc, e));
            log::error!("恢复 {} 服务启动类型失败: {}", svc, e);
        }
    }

    // 3. Registry: Delete policy keys
    log::info!("删除注册表策略键值");
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);

    if let Ok(wu_key) = hklm.open_subkey_with_flags(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU",
        winreg::enums::KEY_SET_VALUE
    ) {
        let _ = wu_key.delete_value("NoAutoUpdate");
        let _ = wu_key.delete_value("AUOptions");
    }

    if let Ok(wu_policy_key) = hklm.open_subkey_with_flags(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate",
        winreg::enums::KEY_SET_VALUE
    ) {
        let _ = wu_policy_key.delete_value("DisableWindowsUpdateAccess");
    }

    // 4. Enable scheduled tasks
    log::info!("启用 Windows Update 计划任务");
    schtasks_wu_tasks(true)?;

    log::info!("Windows Update 恢复完成");
    if warnings.is_empty() {
        Ok("Windows Update 已恢复".to_string())
    } else {
        Ok(format!("Windows Update 已恢复（部分项需注意: {}）", warnings.join("; ")))
    }
}

/// Check service Start value via registry (simpler and more reliable than SCM query).
/// 优先直接读注册表（毫秒级），仅当 ACL 阻止读取时回退到 reg query 进程。
fn get_service_start(service_name: &str) -> Option<u32> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = format!(r"SYSTEM\CurrentControlSet\Services\{}", service_name);
    if let Ok(key) = hklm.open_subkey(path) {
        if let Ok(v) = key.get_value::<u32, _>("Start") {
            return Some(v);
        }
    }

    // Fallback: use reg query via cmd.exe for reliable reading of ACL-protected keys
    let output = std::process::Command::new("cmd.exe")
        .args(&[
            "/c",
            &format!(
                "reg query \"HKLM\\SYSTEM\\CurrentControlSet\\Services\\{}\" /v Start",
                service_name
            ),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    let out = String::from_utf8_lossy(&output.stdout);
    // Parse: "    Start    REG_DWORD    0x4"
    for line in out.lines() {
        let line = line.trim();
        if line.contains("Start") && line.contains("REG_DWORD") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                if let Ok(v) = u32::from_str_radix(last.trim_start_matches("0x"), 16) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Check whether the critical Windows Update DLLs have been renamed to backups
/// (the strongest indicator that WU is disabled — services can't run without them).
fn are_update_dlls_renamed() -> bool {
    const DLLS: [&str; 2] = ["WaaSMedicSvc.dll", "wuaueng.dll"];
    let sys32 = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let sys32 = format!(r"{}\System32", sys32);

    DLLS.iter().any(|dll| {
        let dll_path = format!(r"{}\{}", sys32, dll);
        let backup_path = format!(r"{}\{}", sys32, dll.replace(".dll", "_BAK.dll"));
        Path::new(&backup_path).exists() && !Path::new(&dll_path).exists()
    })
}

#[tauri::command]
pub async fn check_windows_update_state() -> Result<serde_json::Value, String> {
    let services_to_check = ["wuauserv", "UsoSvc", "WaaSMedicSvc"];
    let services_disabled = services_to_check.iter().all(|svc| {
            get_service_start(svc).map_or(false, |st| st == 4)
        });
    log::info!(
        "check_windows_update_state: services_disabled={} (wuauserv={:?} UsoSvc={:?} WaaSMedicSvc={:?})",
        services_disabled,
        get_service_start("wuauserv"),
        get_service_start("UsoSvc"),
        get_service_start("WaaSMedicSvc"),
    );

    // Check registry: NoAutoUpdate == 1?
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let policy_set = hklm.open_subkey(
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate\AU"
    ).and_then(|key| {
        key.get_value::<u32, _>("NoAutoUpdate")
    }).map_or(false, |v| v == 1);

    let scheduler_disabled = check_schtasks_wu_disabled();
    let dlls_renamed = are_update_dlls_renamed();
    log::info!(
        "check_windows_update_state: policy_set={} scheduler_disabled={} dlls_renamed={}",
        policy_set,
        scheduler_disabled,
        dlls_renamed,
    );
    // "Disabled" is determined by the strongest signals: services + DLL rename.
    // scheduler/policy are treated as contributing factors (some tasks like
    // "Refresh Group Policy Cache" are Windows-protected and can never be disabled).
    let all_disabled = services_disabled && dlls_renamed;

    let result = serde_json::json!({
        "services_disabled": services_disabled,
        "policy_set": policy_set,
        "scheduler_disabled": scheduler_disabled,
        "dlls_renamed": dlls_renamed,
        "all_disabled": all_disabled,
    });

    Ok(result)
}

#[tauri::command]
pub async fn delete_power_plan(guid: String) -> Result<PowerPlanOperationResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let active_plan = get_active_plan_internal();
    if let Some((active_guid, _)) = active_plan {
        if active_guid == guid {
            return Err("无法删除当前激活的电源计划，请先切换到其他计划".to_string());
        }
    }

    let result = Command::new("powercfg")
        .args(["/delete", &guid])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else if !stdout.trim().is_empty() { stdout.trim().to_string() } else { "未知错误".to_string() };
                return Err(format!("删除电源计划失败: {}", err_msg));
            }

            std::thread::sleep(std::time::Duration::from_millis(500));

            let system_plans = get_system_plans_internal();
            let still_exists = system_plans.iter().any(|(g, _, _)| g == &guid);

            if still_exists {
                Err("电源计划删除可能未生效，请确认是否具有管理员权限".to_string())
            } else {
                Ok(PowerPlanOperationResult {
                    success: true,
                    message: "电源计划已删除".to_string(),
                    guid: None,
                })
            }
        }
        Err(e) => Err(format!("执行删除命令失败: {}", e)),
    }
}

#[derive(serde::Serialize)]
pub struct PeripheralStatus {
    pub mouse_value: Option<i32>,
    pub keyboard_value: Option<i32>,
    pub mouse_queue_value: Option<i32>,
}

#[tauri::command]
pub async fn get_peripheral_status() -> Result<PeripheralStatus, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let read_dword = |path: &str, name: &str| -> Option<i32> {
        hklm.open_subkey_with_flags(path, KEY_READ)
            .ok()
            .and_then(|k| k.get_value::<u32, _>(name).ok())
            .map(|v| v as i32)
    };
    Ok(PeripheralStatus {
        mouse_value: read_dword(
            r"SYSTEM\CurrentControlSet\Control\PriorityControl",
            "Win32PrioritySeparation",
        ),
        keyboard_value: read_dword(
            r"SYSTEM\CurrentControlSet\Services\Kbdclass\Parameters",
            "KeyboardDataQueueSize",
        ),
        mouse_queue_value: read_dword(
            r"SYSTEM\CurrentControlSet\Services\mouclass\Parameters",
            "MouseDataQueueSize",
        ),
    })
}

#[tauri::command]
pub async fn set_peripheral_settings(
    mouse_value: u32,
    keyboard_value: u32,
    mouse_queue_value: u32,
) -> Result<PerfTweakResult, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let write_dword = |path: &str, name: &str, value: u32| -> Result<(), String> {
        let (key, _) = hklm
            .create_subkey(path)
            .map_err(|e| format!("打开注册表键失败: {e}"))?;
        key.set_value(name, &value)
            .map_err(|e| format!("写入 {name} 失败: {e}"))
    };
    write_dword(
        r"SYSTEM\CurrentControlSet\Control\PriorityControl",
        "Win32PrioritySeparation",
        mouse_value,
    )?;
    write_dword(
        r"SYSTEM\CurrentControlSet\Services\Kbdclass\Parameters",
        "KeyboardDataQueueSize",
        keyboard_value,
    )?;
    write_dword(
        r"SYSTEM\CurrentControlSet\Services\mouclass\Parameters",
        "MouseDataQueueSize",
        mouse_queue_value,
    )?;
    Ok(PerfTweakResult {
        success: true,
        message: "操作成功".to_string(),
    })
}

#[tauri::command]
pub async fn reset_peripheral_settings() -> Result<PerfTweakResult, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let delete_if_exists = |path: &str, name: &str| {
        if let Ok(key) = hklm.open_subkey_with_flags(path, KEY_SET_VALUE) {
            let _ = key.delete_value(name);
        }
    };
    delete_if_exists(
        r"SYSTEM\CurrentControlSet\Control\PriorityControl",
        "Win32PrioritySeparation",
    );
    delete_if_exists(
        r"SYSTEM\CurrentControlSet\Services\Kbdclass\Parameters",
        "KeyboardDataQueueSize",
    );
    delete_if_exists(
        r"SYSTEM\CurrentControlSet\Services\mouclass\Parameters",
        "MouseDataQueueSize",
    );
    Ok(PerfTweakResult {
        success: true,
        message: "操作成功".to_string(),
    })
}

// ========== AQ_REGISTRY 模块 - 纯 Rust 注册表操作（零外部进程） ==========
// 感谢 1U 工具箱提供系统优化支持

/// 读取 .reg 文件内容（自动处理 UTF-8 和 UTF-16LE 编码）
fn read_reg_file(name: &str, is_restore: bool) -> Result<String, String> {
    let path = resolve_reg_path(name, is_restore)?;
    let bytes = fs::read(&path)
        .map_err(|e| format!("读取注册表文件失败: {}", e))?;

    // 检测编码：UTF-16LE BOM (FF FE) 或 UTF-8 BOM (EF BB BF)
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        // UTF-16LE 编码（部分 .reg 文件使用此编码）
        let u16s: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        Ok(String::from_utf16_lossy(&u16s))
    } else if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        // UTF-8 BOM
        Ok(String::from_utf8_lossy(&bytes[3..]).to_string())
    } else {
        // 纯 UTF-8
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }
}

/// 解析 .reg 文件路径（适配开发模式和打包模式）
fn resolve_reg_path(name: &str, is_restore: bool) -> Result<PathBuf, String> {
    let (dir, suffix) = if is_restore {
        ("aq_registry_restore", ".restore.reg")
    } else {
        ("aq_registry", ".reg")
    };

    // 开发模式下从项目根目录查找
    if let Ok(cwd) = std::env::current_dir() {
        let dev_candidates = [
            cwd.join(dir).join(format!("{}{}", name, suffix)),
            cwd.join("..").join(dir).join(format!("{}{}", name, suffix)),
            cwd.join("..").join("..").join(dir).join(format!("{}{}", name, suffix)),
        ];
        for path in &dev_candidates {
            if path.exists() {
                return Ok(path.clone());
            }
        }
    }

    // 开发模式后备：通过编译时 CARGO_MANIFEST_DIR 定位项目根目录
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_candidates = [
        Path::new(manifest_dir).join(dir).join(format!("{}{}", name, suffix)),
        Path::new(manifest_dir).join("..").join(dir).join(format!("{}{}", name, suffix)),
    ];
    for path in &manifest_candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // 打包模式下从 exe 同级目录查找
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join(dir).join(format!("{}{}", name, suffix)),
                parent.join("_up_").join(dir).join(format!("{}{}", name, suffix)),
                parent.join("resources").join(dir).join(format!("{}{}", name, suffix)),
            ];
            for path in &candidates {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }
    }

    Err(format!("未找到注册表文件: {}{}", name, suffix))
}

/// 解析 .reg 文件内容并直接通过 winreg 写入注册表
/// 支持：[HKEY_...] 键路径 / "Name"=dword:XXX / "Name"="string" / "Name"=- 删除值
fn apply_reg_content(content: &str) -> Result<(), String> {
    let mut current_key: Option<RegKey> = None;

    for line in content.lines() {
        let line = line.trim();

        // 跳过空行和注释
        if line.is_empty() || line.starts_with(';') || line.starts_with("Windows Registry Editor") {
            continue;
        }

        // [HKEY_LOCAL_MACHINE\SYSTEM\...] — 注册表键路径
        // [-HKEY_LOCAL_MACHINE\...] — 删除整个注册表键
        if line.starts_with('[') && line.ends_with(']') {
            let path = &line[1..line.len() - 1];
            if let Some(delete_path) = path.strip_prefix('-') {
                delete_reg_key(delete_path)?;
                current_key = None;
            } else {
                current_key = Some(open_or_create_reg_key(path)?);
            }
            continue;
        }

        // "ValueName"=dword:00000001 — DWORD 值
        // "ValueName"="string"        — 字符串值
        // "ValueName"=-                — 删除值
        // @="string"                  — 默认值（键的空名值）
        if let Some(ref key) = current_key {
            if let Some(value) = line.strip_prefix("@=") {
                if let Some(inner) = value.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                    let val = inner.replace("\\\"", "\"");
                    key.set_value("", &val)
                        .map_err(|e| format!("写入注册表默认值失败: {}", e))?;
                }
            } else if let Some(rest) = line.strip_prefix('"') {
                if let Some(eq_pos) = rest.find("\"=") {
                    let name = &rest[..eq_pos];
                    // 反转义 .reg 中的双引号 \"
                    let name = name.replace("\\\"", "\"");
                    let value = &rest[eq_pos + 2..];

                    if value.starts_with("dword:") {
                        // DWORD 值
                        let hex_str = &value[6..];
                        let val = u32::from_str_radix(hex_str, 16)
                            .map_err(|e| format!("解析 dword 值失败: {}", e))?;
                        key.set_value(&name, &val)
                            .map_err(|e| format!("写入注册表值失败: {}", e))?;
                    } else if value.starts_with('"') {
                        // 字符串值（去掉首尾引号，反转义）
                        let val = &value[1..value.len() - 1];
                        let val = val.replace("\\\"", "\"");
                        key.set_value(&name, &val)
                            .map_err(|e| format!("写入注册表值失败: {}", e))?;
                    } else if value == "-" {
                        // 删除值
                        let _ = key.delete_value(&name);
                    }
                    // hex: 格式（二进制值）暂不支持，aq_registry 中未使用
                }
            }
        }
    }

    Ok(())
}

/// 解析 .reg 键路径为（根键，子路径）
fn parse_reg_hive(path: &str) -> Result<(RegKey, String), String> {
    if let Some(sub) = path.strip_prefix("HKEY_LOCAL_MACHINE\\") {
        Ok((RegKey::predef(HKEY_LOCAL_MACHINE), sub.to_string()))
    } else if let Some(sub) = path.strip_prefix("HKEY_CURRENT_USER\\") {
        Ok((RegKey::predef(HKEY_CURRENT_USER), sub.to_string()))
    } else if let Some(sub) = path.strip_prefix("HKEY_CLASSES_ROOT\\") {
        Ok((RegKey::predef(HKEY_CLASSES_ROOT), sub.to_string()))
    } else if let Some(sub) = path.strip_prefix("HKEY_USERS\\") {
        Ok((RegKey::predef(HKEY_USERS), sub.to_string()))
    } else {
        Err(format!("不支持的注册表根键: {}", path))
    }
}

/// 根据 .reg 文件中的路径打开或创建注册表键
fn open_or_create_reg_key(path: &str) -> Result<RegKey, String> {
    let (root, subpath) = parse_reg_hive(path)?;
    let (key, _) = root
        .create_subkey(&subpath)
        .map_err(|e| format!("创建注册表键失败: {} - {}", path, e))?;

    Ok(key)
}

/// 只读打开注册表键（不存在时返回 Err），用于扫描优化项状态
fn open_reg_key_readonly(path: &str) -> Result<RegKey, String> {
    let (root, subpath) = parse_reg_hive(path)?;
    root.open_subkey_with_flags(&subpath, KEY_READ)
        .map_err(|e| format!("打开注册表键失败: {} - {}", path, e))
}

/// 删除整个注册表键（含子键），支持 .reg 中的 [-HKEY_...] 语法
fn delete_reg_key(path: &str) -> Result<(), String> {
    let (root, subpath) = parse_reg_hive(path)?;

    // 拆分父路径与叶子键名
    let (parent_path, leaf) = match subpath.rfind('\\') {
        Some(pos) => (subpath[..pos].to_string(), subpath[pos + 1..].to_string()),
        None => (String::new(), subpath),
    };

    let parent = if parent_path.is_empty() {
        root
    } else {
        root.open_subkey_with_flags(&parent_path, KEY_ALL_ACCESS)
            .map_err(|e| format!("打开注册表父键失败: {} - {}", parent_path, e))?
    };

    match parent.delete_subkey_all(&leaf) {
        Ok(_) => Ok(()),
        // 键不存在（ERROR_FILE_NOT_FOUND）视为已达成删除目标
        Err(e) if e.raw_os_error() == Some(2) => Ok(()),
        Err(e) => Err(format!("删除注册表键失败: {} - {}", path, e)),
    }
}

/// 检查单个注册表值是否与 .reg 目标一致（dword / 字符串 / 删除值）
fn check_reg_entry_matches(key: &RegKey, name: &str, value: &str) -> bool {
    if let Some(hex_str) = value.strip_prefix("dword:") {
        // 目标为 DWORD 值
        match u32::from_str_radix(hex_str, 16) {
            Ok(expected) => match key.get_value::<u32, _>(name) {
                Ok(v) => v == expected,
                Err(_) => false,
            },
            Err(_) => false,
        }
    } else if value.starts_with('"') && value.ends_with('"') {
        // 目标为字符串值
        let expected = value[1..value.len() - 1].replace("\\\"", "\"");
        match key.get_value::<String, _>(name) {
            Ok(v) => v == expected,
            Err(_) => false,
        }
    } else if value == "-" {
        // 目标为删除值：当前值不存在才视为匹配
        key.get_raw_value(name).is_err()
    } else {
        false
    }
}

/// 扫描 .reg 应用文件：所有条目均与注册表当前状态一致才算已优化
fn scan_reg_content(content: &str) -> Result<bool, String> {
    let mut current_key: Option<RegKey> = None;
    let mut all_match = true;

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with(';') || line.starts_with("Windows Registry Editor") {
            continue;
        }

        // [HKEY_LOCAL_MACHINE\SYSTEM\...] — 注册表键路径
        // [-HKEY_LOCAL_MACHINE\...] — 删除键目标：键不存在才视为已达成
        if line.starts_with('[') && line.ends_with(']') {
            let path = &line[1..line.len() - 1];
            if let Some(delete_path) = path.strip_prefix('-') {
                current_key = open_reg_key_readonly(delete_path).ok();
                // 键仍存在则说明删除目标未达成
                if current_key.is_some() {
                    all_match = false;
                }
            } else {
                current_key = open_reg_key_readonly(path).ok();
            }
            continue;
        }

        // @="string" — 默认值（键的空名值）
        if let Some(value) = line.strip_prefix("@=") {
            let matched = match &current_key {
                Some(key) => check_reg_entry_matches(key, "", value),
                None => false,
            };
            if !matched {
                all_match = false;
            }
        } else if let Some(rest) = line.strip_prefix('"') {
            if let Some(eq_pos) = rest.find("\"=") {
                let name = rest[..eq_pos].replace("\\\"", "\"");
                let value = &rest[eq_pos + 2..];

                let matched = match &current_key {
                    Some(key) => check_reg_entry_matches(key, &name, value),
                    // 键不存在：dword/字符串目标值必然不匹配；删除值目标视为已匹配
                    None => value == "-",
                };
                if !matched {
                    all_match = false;
                }
            }
        }
    }

    Ok(all_match)
}

/// 单个优化项的扫描结果
#[derive(serde::Serialize)]
pub struct TweakScanResult {
    pub name: String,
    pub applied: bool,
}

/// 批量扫描优化项当前状态（读取 aq_registry/<name>.reg 与注册表当前值比对）
#[tauri::command]
pub async fn scan_registry_tweaks(names: Vec<String>) -> Result<Vec<TweakScanResult>, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 扫描同样为阻塞的注册表读取，放入后台线程避免卡顿 UI
    tokio::task::spawn_blocking(move || {
        let mut results = Vec::with_capacity(names.len());
        for name in &names {
            let applied = match read_reg_file(name, false) {
                Ok(content) => scan_reg_content(&content).unwrap_or(false),
                Err(_) => false,
            };
            results.push(TweakScanResult {
                name: name.clone(),
                applied,
            });
        }
        Ok(results)
    })
    .await
    .map_err(|e| format!("扫描线程异常: {}", e))?
}

/// 应用单个注册表优化（读取 aq_registry/<name>.reg 并通过 winreg 写入）
#[tauri::command]
pub async fn apply_registry_tweak(name: String) -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 注册表读写为阻塞操作，放入后台线程执行，避免卡顿主线程 UI
    tokio::task::spawn_blocking(move || {
        let content = read_reg_file(&name, false)?;
        apply_reg_content(&content)?;
        Ok(PerfTweakResult {
            success: true,
            message: "优化已应用".to_string(),
        })
    })
    .await
    .map_err(|e| format!("优化执行线程异常: {}", e))?
}

/// 恢复单个注册表优化（读取 aq_registry_restore/<name>.restore.reg 并通过 winreg 写入）
#[tauri::command]
pub async fn restore_registry_tweak(name: String) -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let content = read_reg_file(&name, true)?;
        apply_reg_content(&content)?;
        Ok(PerfTweakResult {
            success: true,
            message: "优化已恢复".to_string(),
        })
    })
    .await
    .map_err(|e| format!("恢复执行线程异常: {}", e))?
}

/// 并行执行一批注册表 .reg 应用/恢复任务（注册表写入互不冲突，按 CPU 数分块并发）
/// 返回 (成功数, 失败列表)。restore=true 时读取 <name>.restore.reg 执行恢复。
fn batch_apply_reg_files(names: &[String], restore: bool) -> (usize, Vec<(String, String)>) {
    let results: Mutex<Vec<(String, Result<(), String>)>> = Mutex::new(Vec::new());
    let worker_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk_size = (names.len() + worker_count - 1) / worker_count;

    std::thread::scope(|s| {
        for chunk in names.chunks(chunk_size.max(1)) {
            let res_ref = &results;
            s.spawn(move || {
                let mut local: Vec<(String, Result<(), String>)> = Vec::new();
                for name in chunk {
                    let r = read_reg_file(name, restore)
                        .and_then(|content| apply_reg_content(&content));
                    local.push((name.clone(), r));
                }
                res_ref.lock().unwrap().extend(local);
            });
        }
    });

    let all = results.into_inner().unwrap();
    let success_count = all.iter().filter(|(_, r)| r.is_ok()).count();
    let failed: Vec<(String, String)> = all
        .into_iter()
        .filter_map(|(name, r)| r.err().map(|e| (name, e)))
        .collect();
    (success_count, failed)
}

/// 批量应用注册表优化
#[tauri::command]
pub async fn batch_apply_registry_tweaks(names: Vec<String>) -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let (success_count, failed) = batch_apply_reg_files(&names, false);

        if failed.is_empty() {
            Ok(PerfTweakResult {
                success: true,
                message: format!("成功应用 {} 项优化", success_count),
            })
        } else {
            let failed_names: Vec<String> = failed.iter().map(|(n, _)| n.clone()).collect();
            Ok(PerfTweakResult {
                success: failed.len() < names.len(),
                message: format!(
                    "成功 {} 项，失败 {} 项: {}",
                    success_count,
                    failed.len(),
                    failed_names.join(", ")
                ),
            })
        }
    })
    .await
    .map_err(|e| format!("批量优化线程异常: {}", e))?
}

/// 批量恢复注册表优化
#[tauri::command]
pub async fn batch_restore_registry_tweaks(names: Vec<String>) -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    tokio::task::spawn_blocking(move || {
        let (success_count, failed) = batch_apply_reg_files(&names, true);

        if failed.is_empty() {
            Ok(PerfTweakResult {
                success: true,
                message: format!("成功恢复 {} 项优化", success_count),
            })
        } else {
            let failed_names: Vec<String> = failed.iter().map(|(n, _)| n.clone()).collect();
            Ok(PerfTweakResult {
                success: failed.len() < names.len(),
                message: format!(
                    "成功 {} 项，失败 {} 项: {}",
                    success_count,
                    failed.len(),
                    failed_names.join(", ")
                ),
            })
        }
    })
    .await
    .map_err(|e| format!("批量恢复线程异常: {}", e))?
}

/// 重启显卡驱动（禁用全部显示适配器后再自动启用）
/// 通过 SetupAPI（C API，不依赖 PowerShell）将显卡驱动完整卸载并重新加载，
/// 比模拟 Win+Ctrl+Shift+B 更彻底，能让显卡改名等注册表修改完整生效。
/// 注意：操作期间当前输出屏幕会短暂黑屏数秒，且需要管理员权限。
#[tauri::command]
pub fn restart_graphics_driver() -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    use std::mem::size_of;
    use std::time::Duration;
    use windows::core::{GUID, HRESULT};
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        DICS_DISABLE, DICS_ENABLE, DICS_FLAG_GLOBAL, DIF_PROPERTYCHANGE, DIGCF_PRESENT,
        SetupDiCallClassInstaller, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiGetClassDevsW, SetupDiSetClassInstallParamsW, HDEVINFO, SP_CLASSINSTALL_HEADER,
        SP_DEVINFO_DATA, SP_PROPCHANGE_PARAMS,
    };
    use windows::Win32::Foundation::HWND;

    // 显示适配器设备类 GUID：{4d36e968-e325-11ce-bfc1-08002be10318}
    const DISPLAY_CLASS_GUID: GUID = GUID::from_u128(0x4d36e968e32511cebfc108002be10318);
    // 枚举结束错误码 ERROR_NO_MORE_ITEMS (259) 的 HRESULT 形式（0x80070103）
    const HRESULT_NO_MORE_ITEMS: HRESULT = HRESULT::from_win32(259);
    // 权限不足错误码 ERROR_ACCESS_DENIED (5) 的 HRESULT 形式（0x80070005）
    const HRESULT_ACCESS_DENIED: HRESULT = HRESULT::from_win32(5);

    /// 对单个设备执行 禁用/启用 属性更改
    fn set_device_state(
        devs: HDEVINFO,
        dev_info: &SP_DEVINFO_DATA,
        enable: bool,
    ) -> Result<(), String> {
        let mut params = SP_PROPCHANGE_PARAMS::default();
        params.ClassInstallHeader.cbSize = size_of::<SP_CLASSINSTALL_HEADER>() as u32;
        params.ClassInstallHeader.InstallFunction = DIF_PROPERTYCHANGE;
        params.StateChange = if enable { DICS_ENABLE } else { DICS_DISABLE };
        params.Scope = DICS_FLAG_GLOBAL;
        params.HwProfile = 0;

        unsafe {
            SetupDiSetClassInstallParamsW(
                devs,
                Some(dev_info),
                Some(&params.ClassInstallHeader),
                size_of::<SP_PROPCHANGE_PARAMS>() as u32,
            )
            .map_err(|e| {
                format!(
                    "设置{}参数失败: {}",
                    if enable { "启用" } else { "禁用" },
                    e
                )
            })?;

            SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, devs, Some(dev_info)).map_err(|e| {
                let action = if enable { "启用" } else { "禁用" };
                if e.code() == HRESULT_ACCESS_DENIED {
                    format!(
                        "{}显示适配器失败：权限不足，请以管理员身份运行 NexBox 后重试（{}）",
                        action, e
                    )
                } else {
                    format!("{}显示适配器失败: {}", action, e)
                }
            })
        }
    }

    // 1. 一次性枚举并保存全部显示适配器。
    //    禁用后重新枚举（DIGCF_PRESENT）会枚举不到已禁用的设备，导致无法自动启用，
    //    因此禁用/启用都必须基于这同一份快照，而不是重新枚举。
    let devs = unsafe {
        SetupDiGetClassDevsW(
            Some(&DISPLAY_CLASS_GUID),
            None,
            HWND::default(),
            DIGCF_PRESENT,
        )
        .map_err(|e| format!("枚举显示适配器失败: {}", e))?
    };

    let mut devices: Vec<SP_DEVINFO_DATA> = Vec::new();
    let mut index = 0u32;
    loop {
        let mut dev_info = SP_DEVINFO_DATA::default();
        dev_info.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        match unsafe { SetupDiEnumDeviceInfo(devs, index, &mut dev_info) } {
            Ok(()) => {
                devices.push(dev_info);
                index += 1;
            }
            Err(e) if e.code() == HRESULT_NO_MORE_ITEMS => break,
            Err(e) => {
                unsafe { SetupDiDestroyDeviceInfoList(devs).ok(); }
                return Err(format!("枚举显示适配器失败: {}", e));
            }
        }
    }
    if devices.is_empty() {
        unsafe { SetupDiDestroyDeviceInfoList(devs).ok(); }
        return Err("未找到可操作的显示适配器".to_string());
    }

    // 2. 禁用全部显示适配器（当前输出屏幕会短暂黑屏）-> 等待驱动卸载 -> 自动重新启用
    let mut disabled = 0usize;
    let mut enabled = 0usize;
    let step_result = (|| -> Result<(), String> {
        for dev in &devices {
            set_device_state(devs, dev, false)?;
            disabled += 1;
        }
        std::thread::sleep(Duration::from_secs(3));
        for dev in &devices {
            set_device_state(devs, dev, true)?;
            enabled += 1;
        }
        Ok(())
    })();
    unsafe { SetupDiDestroyDeviceInfoList(devs).ok(); }
    step_result?;

    Ok(PerfTweakResult {
        success: true,
        message: format!(
            "显卡驱动已通过禁用/启用方式重置（禁用 {} 台，启用 {} 台），屏幕可能短暂黑屏",
            disabled, enabled
        ),
    })
}

/// 检查 Windows 更新暂停状态
#[tauri::command]
pub fn check_pause_update_state() -> Result<bool, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(r"SOFTWARE\Microsoft\WindowsUpdate\UX\Settings")
        .map_err(|e| format!("无法打开注册表键: {}", e))?;

    // 检查 PauseUpdatesExpiryTime 值是否存在
    let paused: bool = key
        .get_value::<String, _>("PauseUpdatesExpiryTime")
        .map(|_| true)
        .unwrap_or(false);

    Ok(paused)
}

/// 检查 Windows Defender 是否已被关闭（组策略键 DisableAntiSpyware == 1）
#[tauri::command]
pub fn check_defender_state() -> Result<bool, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let disabled: bool = hklm
        .open_subkey(r"SOFTWARE\Policies\Microsoft\Windows Defender")
        .ok()
        .and_then(|key| key.get_value::<u32, _>("DisableAntiSpyware").ok())
        .map(|v| v == 1)
        .unwrap_or(false);

    Ok(disabled)
}

// ===================== 虚拟内存（页面文件）管理 =====================
// 完全基于 winreg + ShellExecuteExW(runas)，不使用 PowerShell。

/// 虚拟内存合理性诊断结果
#[derive(serde::Serialize)]
pub struct PagefileRecommendation {
    pub verdict: String, // "low" | "ok" | "high"（偏少/合理/偏多）
    pub suggested_initial_mb: u64,
    pub suggested_max_mb: u64,
    pub message: String, // 中文建议文案
}

/// 页面文件当前状态
#[derive(serde::Serialize)]
pub struct PagefileStatus {
    pub physical_memory_mb: u64,
    pub total_virtual_memory_mb: u64, // GlobalMemoryStatusEx.ullTotalPageFile（提交限制）
    pub pagefile_size_mb: u64,        // total_virtual - physical（当前页面文件容量）
    pub drives: Vec<PagefileDrive>,   // 每个磁盘盘符各自的模式与大小
    pub recommendation: PagefileRecommendation,
}

/// 单个磁盘盘符的页面文件设置（用于状态展示与写入）
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PagefileDrive {
    pub path: String,    // 如 "C:\\pagefile.sys"
    pub drive: String,   // 如 "C:"
    pub mode: String,    // "none" | "system" | "custom"
    pub initial_mb: u64,
    pub max_mb: u64,
}

/// 设置页面文件的结果
#[derive(serde::Serialize)]
pub struct PagefileResult {
    pub success: bool,
    pub message: String,
    pub requires_restart: bool,
}

const PAGE_MANAGEMENT_REG_KEY: &str = r"System\CurrentControlSet\Control\Session Manager\Memory Management";
const PAGE_MANAGEMENT_VALUE: &str = "PagingFiles";
const DRIVE_FIXED: u32 = 3;

/// 读取注册表中的页面文件条目（REG_MULTI_SZ），返回原始行（如 "C:\\pagefile.sys 0 0"）
fn read_pagefile_lines_from_registry() -> Vec<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let Ok(key) = hklm.open_subkey(PAGE_MANAGEMENT_REG_KEY) else {
        return Vec::new();
    };
    key.get_value::<Vec<String>, _>(PAGE_MANAGEMENT_VALUE).unwrap_or_default()
}

/// 从一行注册表条目解析出（路径, 初始, 最大）
fn parse_pagefile_line(line: &str) -> Option<(String, u64, u64)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let initial_mb = parts[1].parse::<u64>().ok()?;
    let max_mb = parts[2].parse::<u64>().ok()?;
    Some((parts[0].to_string(), initial_mb, max_mb))
}

/// 枚举所有固定磁盘盘符（如 C:、D:、E:），不含软驱/光驱/网络盘
fn enumerate_fixed_drives() -> Vec<String> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};

    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();
    for i in 0..26 {
        if mask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            let root = format!("{}:\\", letter);
            let root_w: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
            let dtype = unsafe { GetDriveTypeW(root_w.as_ptr()) };
            if dtype == DRIVE_FIXED {
                drives.push(format!("{}:", letter));
            }
        }
    }
    drives
}

/// 读取当前所有磁盘盘符及其页面文件模式（none/system/custom）
fn read_pagefile_drives() -> Vec<PagefileDrive> {
    let lines = read_pagefile_lines_from_registry();
    let drives = enumerate_fixed_drives();

    drives
        .into_iter()
        .map(|drive| {
            // 在注册表条目中按盘符前缀匹配（如 C: -> c:\pagefile.sys）
            let drive_upper = drive.to_uppercase();
            let entry = lines.iter().find_map(|line| {
                parse_pagefile_line(line).filter(|(path, _, _)| {
                    path.to_uppercase().starts_with(&drive_upper)
                })
            });

            match entry {
                Some((path, initial_mb, max_mb)) if initial_mb == 0 && max_mb == 0 => PagefileDrive {
                    path,
                    drive,
                    mode: "system".to_string(),
                    initial_mb: 0,
                    max_mb: 0,
                },
                Some((path, initial_mb, max_mb)) => PagefileDrive {
                    path,
                    drive,
                    mode: "custom".to_string(),
                    initial_mb,
                    max_mb,
                },
                None => PagefileDrive {
                    path: format!(r"{}\pagefile.sys", drive),
                    drive,
                    mode: "none".to_string(),
                    initial_mb: 0,
                    max_mb: 0,
                },
            }
        })
        .collect()
}

/// 通过 ShellExecuteExW 提权运行 reg.exe 写入 PagingFiles（REG_MULTI_SZ）。
/// 应用非管理员时弹出 UAC，等待提权进程结束。零 PowerShell。
fn run_reg_set_pagefiles_elevated(entries: &[String]) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    // reg.exe 的 REG_MULTI_SZ 使用 \0 分隔多个条目；单个条目直接传入即可
    let data = entries.join("\\0");
    let system_root = env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let reg_path = format!(r"{}\System32\reg.exe", system_root);
    let args = format!(
        "add HKLM\\{} /v {} /t REG_MULTI_SZ /d \"{}\" /f",
        PAGE_MANAGEMENT_REG_KEY, PAGE_MANAGEMENT_VALUE, data
    );

    let to_w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb_w = to_w("runas");
    let file_w = to_w(&reg_path);
    let args_w = to_w(&args);

    let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    sei.fMask = SEE_MASK_NOCLOSEPROCESS;
    sei.lpVerb = PCWSTR(verb_w.as_ptr());
    sei.lpFile = PCWSTR(file_w.as_ptr());
    sei.lpParameters = PCWSTR(args_w.as_ptr());
    sei.nShow = SW_HIDE.0;

    if unsafe { ShellExecuteExW(&mut sei) }.is_err() {
        let code = unsafe { GetLastError() };
        return Err(format!(
            "需要管理员权限：提权失败（错误码 {}），可能是用户取消了授权",
            code
        ));
    }

    unsafe { WaitForSingleObject(sei.hProcess, u32::MAX) };

    let mut exit_code: u32 = 0;
    if unsafe { GetExitCodeProcess(sei.hProcess, &mut exit_code) }.is_err() {
        let _ = unsafe { CloseHandle(sei.hProcess) };
        return Err("无法获取 reg.exe 执行结果".to_string());
    }
    let _ = unsafe { CloseHandle(sei.hProcess) };

    if exit_code != 0 {
        return Err(format!("reg.exe 写入失败（退出码 {}）", exit_code));
    }
    Ok(())
}

/// 写入页面文件条目。先尝试直接 winreg 写（管理员），失败则提权 reg.exe。
fn write_pagefiles(entries: Vec<String>) -> Result<(), String> {
    // 1) 应用以管理员运行时直接写入（纯 winreg，速度快）
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let direct = hklm
        .open_subkey_with_flags(
            PAGE_MANAGEMENT_REG_KEY,
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )
        .and_then(|key| key.set_value(PAGE_MANAGEMENT_VALUE, &entries));

    if direct.is_ok() {
        return Ok(());
    }

    // 2) 非管理员：ShellExecuteEx 提权运行 reg.exe（弹 UAC）
    run_reg_set_pagefiles_elevated(&entries)
}

/// 根据物理内存给出页面文件建议大小与合理性判定
fn recommend_pagefile(physical_mb: u64, current_pagefile_mb: u64) -> PagefileRecommendation {
    let (mut init, mut max) = match physical_mb {
        p if p < 8 * 1024 => (p * 3 / 2, p * 3),
        p if p < 16 * 1024 => (p, p * 2),
        p => ((p / 2).max(4096), p),
    };
    // 四舍五入到整 MB（保留原始以 1024 为基准的值即可）
    if init < 1024 {
        init = 1024;
    }
    if max < init {
        max = init;
    }

    let verdict = if current_pagefile_mb < (init as u64 / 2) {
        "low"
    } else if current_pagefile_mb > (max as u64 * 3 / 2) {
        "high"
    } else {
        "ok"
    };

    let message = match verdict {
        "low" => format!(
            "当前页面文件偏少，建议初始 {} MB、最大 {} MB",
            init, max
        ),
        "high" => format!(
            "当前页面文件偏多，建议初始 {} MB、最大 {} MB",
            init, max
        ),
        _ => format!("当前页面文件大小合理（建议区间 初始 {} MB ~ 最大 {} MB）", init, max),
    };

    PagefileRecommendation {
        verdict: verdict.to_string(),
        suggested_initial_mb: init,
        suggested_max_mb: max,
        message,
    }
}

/// 获取页面文件（虚拟内存）当前状态
#[tauri::command]
pub async fn get_pagefile_status() -> Result<PagefileStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let physical_memory_mb = get_physical_memory_mb();

    // 总虚拟内存（提交限制）来自 GlobalMemoryStatusEx.ullTotalPageFile
    let mut total_virtual_memory_mb: u64 = 0;
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            total_virtual_memory_mb = status.ullTotalPageFile / 1024 / 1024;
        }
    }

    // 每个磁盘盘符各自的模式与大小
    let drives = read_pagefile_drives();
    let pagefile_size_mb = total_virtual_memory_mb.saturating_sub(physical_memory_mb);

    let recommendation = recommend_pagefile(physical_memory_mb, pagefile_size_mb);

    Ok(PagefileStatus {
        physical_memory_mb,
        total_virtual_memory_mb,
        pagefile_size_mb,
        drives,
        recommendation,
    })
}

/// 设置页面文件：按磁盘独立设置模式（none 无分页 / system 系统管理 / custom 自定义）。
/// 只更新传入的盘符，其余盘的现有设置保持不变（合并写入）。
#[tauri::command]
pub async fn set_pagefile(
    drives: Vec<PagefileDrive>,
) -> Result<PagefileResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    if drives.is_empty() {
        return Err("缺少磁盘条目".to_string());
    }

    let physical_memory_mb = get_physical_memory_mb();

    // 校验传入条目
    for d in &drives {
        if d.mode == "custom" {
            if d.initial_mb == 0 {
                return Err(format!("{} 的初始大小必须大于 0", d.path));
            }
            if d.max_mb < d.initial_mb {
                return Err(format!("{} 的最大大小不能小于初始大小", d.path));
            }
            if d.max_mb > physical_memory_mb.saturating_mul(4) {
                return Err(format!(
                    "{} 的最大大小过大（上限为物理内存的 4 倍 = {} MB）",
                    d.path,
                    physical_memory_mb.saturating_mul(4)
                ));
            }
        } else if d.mode != "none" && d.mode != "system" {
            return Err(format!("未知模式: {}", d.mode));
        }
    }

    // 读取现有所有盘的条目（盘符 -> 原始行），用于保留未修改的盘
    let existing_lines = read_pagefile_lines_from_registry();
    let mut merged: Vec<(String, String)> = Vec::new(); // (drive, raw_line)
    for line in &existing_lines {
        if let Some((path, _, _)) = parse_pagefile_line(line) {
            let drive = path
                .get(..2)
                .map(|s| s.to_uppercase())
                .unwrap_or_else(|| path.to_uppercase());
            merged.push((drive, line.clone()));
        }
    }

    // 应用传入的盘
    for d in &drives {
        let drive_key = d.drive.to_uppercase();
        // 移除该盘原有条目
        merged.retain(|(k, _)| *k != drive_key);
        match d.mode.as_str() {
            // 无分页：不写入该盘条目
            "none" => {}
            // 系统管理：写 "路径 0 0"
            "system" => merged.push((drive_key, format!(r"{} 0 0", d.path))),
            // 自定义：写 "路径 初始 最大"
            "custom" => {
                merged.push((drive_key, format!(r"{} {} {}", d.path, d.initial_mb, d.max_mb)));
            }
            _ => {}
        }
    }

    let raw_entries: Vec<String> = if merged.is_empty() {
        // 所有盘都设为无分页时，写空 REG_MULTI_SZ
        vec![String::new()]
    } else {
        merged.into_iter().map(|(_, line)| line).collect()
    };

    match write_pagefiles(raw_entries) {
        Ok(_) => Ok(PagefileResult {
            success: true,
            message: "虚拟内存设置已应用，需要重启生效".to_string(),
            requires_restart: true,
        }),
        Err(e) => Err(format!("设置虚拟内存失败: {}。请以管理员身份运行应用。", e)),
    }
}
