use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessRefreshKind, System};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use log;

// LOGICAL_PROCESSOR_RELATIONSHIP 常量
const RELATION_PROCESSOR_CORE: i32 = 0;

// ── Win32 常量 ──────────────────────────────────────────────
const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
const PROCESS_SET_INFORMATION: u32 = 0x0200;

// ── 数据结构 ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CoreType {
    Performance,
    Efficiency,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhysicalCore {
    pub core_index: u32,
    pub core_type: CoreType,
    pub logical_processors: Vec<u32>,
    /// 该核心在亲和性掩码中的位组合 (1 << lp for each lp)
    pub affinity_mask: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuTopology {
    pub cpu_name: String,
    pub total_physical_cores: u32,
    pub total_logical_processors: u32,
    pub has_hybrid_architecture: bool,
    pub physical_cores: Vec<PhysicalCore>,
    /// 系统全部可用逻辑处理器的掩码
    pub system_affinity_mask: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory_mb: f64,
    pub cpu_usage: f32,
    pub exe_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessAffinityInfo {
    pub pid: u32,
    pub process_name: String,
    pub affinity_mask: u64,
    pub system_mask: u64,
    pub assigned_logical_processors: Vec<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchedulerRule {
    pub process_name: String,
    pub mask: u64,
    pub preset: String,
    pub description: String,
}

// ── 核心隔离数据结构 ────────────────────────────────────────

/// 核心隔离规则（持久化于 cpu-isolation-rules.json，区别于「进程→掩码」调度规则）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IsolationRule {
    pub name: String,
    /// 被隔离的核心掩码（其他进程被剔除的位）
    pub isolated_mask: u64,
    /// 豁免进程名（游戏进程，保持全核心）
    pub exclude_process: String,
    pub preset: String,
    pub description: String,
    /// 开机自动应用开关
    pub auto_apply: bool,
}

/// 核心隔离应用/恢复结果（返回前端展示统计）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IsolationApplyResult {
    /// 尝试处理的进程数
    pub total: u32,
    /// 成功修改的进程数
    pub modified: u32,
    /// 失败进程数
    pub failed: u32,
    /// 失败进程名列表
    pub failed_processes: Vec<String>,
}

/// 单个被隔离进程的原始状态（供恢复时还原）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IsolationModifiedProcess {
    pub pid: u32,
    pub name: String,
    pub original_mask: u64,
}

/// 活动隔离状态（持久化于 cpu-isolation-state.json，pid+name 供恢复时校验防 PID 复用）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IsolationStateRecord {
    pub isolated_mask: u64,
    pub exclude_process: String,
    pub modified_processes: Vec<IsolationModifiedProcess>,
}

// ── CPU 拓扑 ────────────────────────────────────────────────

/// 读取 LE u32
fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}
/// 读取 LE u16
fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}
/// 读取 LE u64
fn read_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        buf[off], buf[off + 1], buf[off + 2], buf[off + 3],
        buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7],
    ])
}

#[cfg(target_os = "windows")]
fn get_cpu_topology_win32() -> Result<CpuTopology, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformationEx, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
    };

    let cpu_name = get_cpu_name();

    // 调用 GetLogicalProcessorInformationEx 获取核心拓扑
    let relationship: i32 = RELATION_PROCESSOR_CORE;
    let mut buffer_size: u32 = 0;
    unsafe {
        GetLogicalProcessorInformationEx(relationship, std::ptr::null_mut(), &mut buffer_size);
    }
    if buffer_size == 0 {
        return Err("无法获取CPU处理器信息".to_string());
    }

    // 分配 8 字节对齐的 buffer
    let alloc_units = ((buffer_size as usize) + 7) / 8;
    let mut buffer_aligned: Vec<u64> = vec![0u64; alloc_units];
    let success = unsafe {
        GetLogicalProcessorInformationEx(
            relationship,
            buffer_aligned.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut buffer_size,
        )
    };
    if success == 0 {
        let err = unsafe { GetLastError() };
        return Err(format!("GetLogicalProcessorInformationEx 失败, 错误码: {}", err));
    }

    // 转为字节切片
    let buf: &[u8] = unsafe {
        std::slice::from_raw_parts(buffer_aligned.as_ptr() as *const u8, buffer_size as usize)
    };

    // ── 按 Windows SDK 精确字节偏移解析 ─────────────────────
    //
    // SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX:
    //   +0  Relationship  DWORD  (4)
    //   +4  Size           DWORD  (4)
    //   +8  union data
    //
    // PROCESSOR_RELATIONSHIP (从 entry+8 开始):
    //   +0  Flags           BYTE   (1)
    //   +1  EfficiencyClass BYTE   (1)  (用于区分 P/E 核：值越高 = P 核)
    //   +2  Reserved        BYTE[20] (20)
    //   +22 GroupCount      WORD   (2)
    //   +24 GroupMask[]     GROUP_AFFINITY[] (每个 16 bytes)
    //
    // GROUP_AFFINITY:
    //   +0  Mask    ULONG_PTR (8 on 64-bit)
    //   +8  Group   WORD      (2)
    //   +10 Reserved WORD[3]  (6)
    //   Total: 16 bytes

    let mut physical_cores: Vec<PhysicalCore> = Vec::new();
    let mut efficiency_classes: Vec<u8> = Vec::new(); // 与 physical_cores 下标对齐
    let mut total_logical: u32 = 0;
    let mut system_mask: u64 = 0;
    let mut max_class: u8 = 0;

    let mut offset = 0usize;
    while offset + 8 <= buf.len() {
        let rel = read_u32(buf, offset) as i32;
        let entry_size = read_u32(buf, offset + 4) as usize;

        if entry_size == 0 || offset + entry_size > buf.len() {
            break;
        }

        // 只处理 RelationProcessorCore (0)
        if rel == 0 {
            let p = offset + 8; // union 起点

            // EfficiencyClass（BYTE @ +1）：值越高 = 性能越强（P 核），越低 = 越能效（E 核）。
            // 这是区分 P/E 核的可靠信号（见 PROCESSOR_RELATIONSHIP 官方文档）。
            // 不能再用「线程数」判定——Arrow Lake 及以后的 Core Ultra 取消超线程，
            // P 核与 E 核都只有 1 个线程，按线程数会把所有核心误判为 E 核。
            let efficiency_class = buf[p + 1];
            if efficiency_class > max_class {
                max_class = efficiency_class;
            }

            let group_count = read_u16(buf, p + 22) as usize;

            // 读取该核心的逻辑处理器掩码
            let mut core_mask: u64 = 0;
            for g in 0..group_count {
                let gm = p + 24 + g * 16;
                if gm + 10 > buf.len() {
                    break;
                }
                let mask = read_u64(buf, gm);
                let group = read_u16(buf, gm + 8);
                if group == 0 {
                    core_mask |= mask;
                }
            }

            // 从掩码中提取逻辑处理器编号
            let mut logical_processors: Vec<u32> = Vec::new();
            for bit in 0..64u32 {
                if (core_mask >> bit) & 1 == 1 {
                    logical_processors.push(bit);
                }
            }

            total_logical += logical_processors.len() as u32;
            system_mask |= core_mask;

            efficiency_classes.push(efficiency_class);
            // 核心类型在循环结束后按效率等级统一赋值
            physical_cores.push(PhysicalCore {
                core_index: physical_cores.len() as u32,
                core_type: CoreType::Unknown,
                logical_processors,
                affinity_mask: core_mask,
            });
        }

        offset += entry_size;
    }

    if physical_cores.is_empty() {
        return Err("未能解析到任何物理核心信息".to_string());
    }

    // ── 核心类型判定（基于 EfficiencyClass）──────────────────
    // 官方语义：EfficiencyClass 值越高 = 性能越强（P 核），越低 = 越能效（E 核）。
    // 混合 CPU（Intel Alder Lake 及以后）存在多个效率等级：最高等级 = P 核，其余 = E 核
    // （Arrow Lake 的低功耗 E 核也归为 E 核）。
    // 非混合 CPU（AMD / 旧 Intel）所有核心效率等级一致 → 全部视为性能核。
    let distinct_classes: std::collections::BTreeSet<u8> =
        efficiency_classes.iter().copied().collect();

    // 次级兜底：老 Windows 10 在混合 CPU 上可能未正确上报 EfficiencyClass（全为 0），
    // 此时若线程数出现混合（部分 2 线程、部分 1 线程），退回「线程数」启发式识别 P/E。
    let has_smt_mix = physical_cores.iter().any(|c| c.logical_processors.len() == 2)
        && physical_cores.iter().any(|c| c.logical_processors.len() == 1);

    let has_hybrid = distinct_classes.len() > 1 || has_smt_mix;

    for (core, &class) in physical_cores.iter_mut().zip(&efficiency_classes) {
        if distinct_classes.len() > 1 {
            // EfficiencyClass 是权威信号：最高效率等级 = P 核，其余 = E 核
            core.core_type = if class == max_class {
                CoreType::Performance
            } else {
                CoreType::Efficiency
            };
        } else if has_smt_mix {
            // 按线程数启发式：2 线程（超线程）= P 核，1 线程 = E 核
            core.core_type = if core.logical_processors.len() == 2 {
                CoreType::Performance
            } else {
                CoreType::Efficiency
            };
        } else {
            core.core_type = CoreType::Performance;
        }
    }

    let p_count = physical_cores
        .iter()
        .filter(|c| c.core_type == CoreType::Performance)
        .count();
    let e_count = physical_cores
        .iter()
        .filter(|c| c.core_type == CoreType::Efficiency)
        .count();
    log::info!(
        "[CPU拓扑] {} | 物理核 {} | P核 {} | E核 {} | 混合架构: {} | 效率等级: {:?}",
        cpu_name,
        physical_cores.len(),
        p_count,
        e_count,
        has_hybrid,
        distinct_classes
    );

    Ok(CpuTopology {
        cpu_name,
        total_physical_cores: physical_cores.len() as u32,
        total_logical_processors: total_logical,
        has_hybrid_architecture: has_hybrid,
        physical_cores,
        system_affinity_mask: system_mask,
    })
}

#[cfg(not(target_os = "windows"))]
fn get_cpu_topology_win32() -> Result<CpuTopology, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

fn get_cpu_name() -> String {
    let sys = System::new_all();
    let cpus = sys.cpus();
    if !cpus.is_empty() {
        return cpus[0].brand().to_string();
    }
    "未知 CPU".to_string()
}

#[tauri::command]
pub async fn get_cpu_topology() -> Result<CpuTopology, String> {
    get_cpu_topology_win32()
}

// ── 进程列表 ────────────────────────────────────────────────

/// 系统关键进程白名单 — 禁止用户操作
const PROTECTED_PROCESSES: &[&str] = &[
    "System",
    "Registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "services.exe",
    "lsass.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "winlogon.exe",
    "MemCompression",
    "kthreadd",
    "Idle",
];

fn is_protected_process(name: &str) -> bool {
    let lower = name.to_lowercase();
    PROTECTED_PROCESSES.iter().any(|p| p.to_lowercase() == lower)
}

#[tauri::command]
pub async fn get_process_list() -> Result<Vec<ProcessInfo>, String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessRefreshKind::everything().with_cpu().with_memory(),
    );

    let mut processes: Vec<ProcessInfo> = Vec::new();

    for (_, proc_) in sys.processes() {
        let name = proc_.name().to_string();
        if name.is_empty() || is_protected_process(&name) {
            continue;
        }
        let memory_mb = proc_.memory() as f64 / 1024.0 / 1024.0;
        processes.push(ProcessInfo {
            pid: proc_.pid().as_u32(),
            name,
            memory_mb,
            cpu_usage: proc_.cpu_usage(),
            exe_path: proc_.exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
        });
    }

    // 按内存占用降序
    processes.sort_by(|a, b| {
        b.memory_mb
            .partial_cmp(&a.memory_mb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(processes)
}

// ── 进程亲和性 ──────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn open_process_for_affinity(pid: u32) -> Result<windows_sys::Win32::Foundation::HANDLE, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::OpenProcess;

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION,
            0,
            pid,
        )
    };

    if handle.is_null() {
        let err = unsafe { GetLastError() };
        // 尝试以有限权限打开
        let handle_limited = unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION,
                0,
                pid,
            )
        };
        if handle_limited.is_null() {
            return Err(format!("无法打开进程 (PID: {}), 错误码: {}", pid, err));
        }
        return Ok(handle_limited);
    }

    Ok(handle)
}

#[tauri::command]
pub async fn get_process_affinity(pid: u32) -> Result<ProcessAffinityInfo, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::GetProcessAffinityMask;

        let handle = open_process_for_affinity(pid)?;
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        let success = unsafe {
            GetProcessAffinityMask(handle, &mut process_mask, &mut system_mask)
        };

        let proc_name = get_process_name_by_pid(pid);
        let pm = process_mask as u64;
        let sm = system_mask as u64;

        unsafe { CloseHandle(handle); }

        if success == 0 {
            return Err(format!("获取进程亲和性失败 (PID: {})", pid));
        }

        let mut assigned: Vec<u32> = Vec::new();
        for bit in 0..64u32 {
            if (pm >> bit) & 1 == 1 {
                assigned.push(bit);
            }
        }

        Ok(ProcessAffinityInfo {
            pid,
            process_name: proc_name,
            affinity_mask: pm,
            system_mask: sm,
            assigned_logical_processors: assigned,
        })
    }

    #[cfg(not(target_os = "windows"))]
    Err("此功能仅支持 Windows 系统".to_string())
}

#[tauri::command]
pub async fn set_process_affinity(pid: u32, mask: u64) -> Result<bool, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
        use windows_sys::Win32::System::Threading::SetProcessAffinityMask;

        let handle = open_process_for_affinity(pid)?;
        let success = unsafe { SetProcessAffinityMask(handle, mask as usize) };
        let err = unsafe { GetLastError() };
        unsafe { CloseHandle(handle); }

        if success == 0 {
            return Err(format!("设置进程亲和性失败, 错误码: {} (可能需要管理员权限)", err));
        }

        Ok(true)
    }

    #[cfg(not(target_os = "windows"))]
    Err("此功能仅支持 Windows 系统".to_string())
}

#[tauri::command]
pub async fn restore_process_affinity(pid: u32) -> Result<bool, String> {
    // 恢复 = 设置为系统全部可用核心
    let topology = get_cpu_topology().await?;
    set_process_affinity(pid, topology.system_affinity_mask).await
}

fn get_process_name_by_pid(pid: u32) -> String {
    let mut sys = System::new();
    sys.refresh_process_specifics(Pid::from_u32(pid), ProcessRefreshKind::everything());
    sys.process(Pid::from_u32(pid))
        .map(|p| p.name().to_string())
        .unwrap_or_else(|| "未知".to_string())
}

// ── 规则持久化 ──────────────────────────────────────────────

const STORE_FILE: &str = "cpu-scheduler-rules.json";

#[tauri::command]
pub async fn get_saved_rules(app: AppHandle) -> Result<Vec<SchedulerRule>, String> {
    let store = app.store(STORE_FILE).map_err(|e| format!("无法打开规则存储: {}", e))?;
    let mut rules: Vec<SchedulerRule> = Vec::new();

    for (key, value) in store.entries() {
        if let Some(obj) = value.as_object() {
            let mask = obj
                .get("mask")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let preset = obj
                .get("preset")
                .and_then(|v| v.as_str())
                .unwrap_or("custom")
                .to_string();
            let description = obj
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            rules.push(SchedulerRule {
                process_name: key,
                mask,
                preset,
                description,
            });
        }
    }

    rules.sort_by(|a, b| a.process_name.cmp(&b.process_name));
    Ok(rules)
}

#[tauri::command]
pub async fn save_rule(
    app: AppHandle,
    process_name: String,
    mask: u64,
    preset: String,
    description: String,
) -> Result<bool, String> {
    let store = app.store(STORE_FILE).map_err(|e| format!("无法打开规则存储: {}", e))?;
    let rule = serde_json::json!({
        "mask": mask,
        "preset": preset,
        "description": description,
    });
    store.set(&process_name, rule);
    store
        .save()
        .map_err(|e| format!("保存规则失败: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn delete_rule(app: AppHandle, process_name: String) -> Result<bool, String> {
    let store = app.store(STORE_FILE).map_err(|e| format!("无法打开规则存储: {}", e))?;
    store.delete(&process_name);
    store
        .save()
        .map_err(|e| format!("删除规则失败: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn apply_rule_by_name(app: AppHandle, process_name: String) -> Result<(bool, u32), String> {
    let store = app.store(STORE_FILE).map_err(|e| format!("无法打开规则存储: {}", e))?;
    let value = store
        .get(&process_name)
        .ok_or_else(|| format!("未找到进程 {} 的规则", process_name))?;

    let mask = value
        .as_object()
        .and_then(|o| o.get("mask"))
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("未找到进程 {} 的规则", process_name))?;

    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessRefreshKind::new());

    let mut count = 0u32;
    for (_, proc_) in sys.processes() {
        if proc_.name() == process_name {
            match set_process_affinity(proc_.pid().as_u32(), mask).await {
                Ok(_) => count += 1,
                Err(_) => {}
            }
        }
    }

    if count == 0 {
        return Err(format!("未找到运行中的进程: {}", process_name));
    }

    Ok((true, count))
}

// ── 启动时自动应用所有已保存规则 ──────────────────────────────

pub async fn apply_all_saved_rules(app: &AppHandle) {
    let store = match app.store(STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[CPU调度] 无法打开规则存储: {}", e);
            return;
        }
    };

    let entries = store.entries();
    if entries.is_empty() {
        return;
    }

    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessRefreshKind::new());

    let mut applied = 0u32;
    let mut skipped = 0u32;

    for (process_name, value) in entries {
        let mask = match value
            .as_object()
            .and_then(|o| o.get("mask"))
            .and_then(|v| v.as_u64())
        {
            Some(m) => m,
            None => continue,
        };

        let mut found = false;
        for (_, proc_) in sys.processes() {
            if proc_.name() == process_name.as_str() {
                found = true;
                match set_process_affinity(proc_.pid().as_u32(), mask).await {
                    Ok(_) => applied += 1,
                    Err(e) => log::warn!("[CPU调度] 应用规则 {} 失败: {}", process_name, e),
                }
            }
        }
        if !found {
            skipped += 1;
        }
    }

    if applied > 0 || skipped > 0 {
        log::info!(
            "[CPU调度] 启动时自动应用规则完成: {} 条已应用, {} 条进程未运行",
            applied,
            skipped
        );
    }

    // 应用标记为「开机自动应用」的核心隔离规则
    apply_auto_isolation_rules(app).await;
}

// ── 核心隔离 ────────────────────────────────────────────────

const ISOLATION_STATE_FILE: &str = "cpu-isolation-state.json";
const ISOLATION_RULES_FILE: &str = "cpu-isolation-rules.json";

// ── 隔离状态持久化 ──────────────────────────────────────────

fn read_isolation_state(app: &AppHandle) -> Option<IsolationStateRecord> {
    let store = app.store(ISOLATION_STATE_FILE).ok()?;
    let value = store.get("state")?;
    serde_json::from_value::<IsolationStateRecord>(value).ok()
}

fn save_isolation_state(app: &AppHandle, state: &IsolationStateRecord) -> Result<(), String> {
    let store = app
        .store(ISOLATION_STATE_FILE)
        .map_err(|e| format!("无法打开隔离状态存储: {}", e))?;
    store.set(
        "state",
        serde_json::to_value(state).map_err(|e| format!("隔离状态序列化失败: {}", e))?,
    );
    store
        .save()
        .map_err(|e| format!("保存隔离状态失败: {}", e))
}

fn clear_isolation_state(app: &AppHandle) -> Result<(), String> {
    let store = app
        .store(ISOLATION_STATE_FILE)
        .map_err(|e| format!("无法打开隔离状态存储: {}", e))?;
    store.delete("state");
    store
        .save()
        .map_err(|e| format!("清除隔离状态失败: {}", e))
}

// ── Win32 同步辅助（仅 Windows）──────────────────────────────

/// 读取指定进程当前亲和掩码（失败返回错误）
#[cfg(target_os = "windows")]
fn get_process_mask_sync(pid: u32) -> Result<u64, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::GetProcessAffinityMask;

    let handle = open_process_for_affinity(pid)?;
    let mut process_mask: usize = 0;
    let mut system_mask: usize = 0;
    let success = unsafe { GetProcessAffinityMask(handle, &mut process_mask, &mut system_mask) };
    unsafe { CloseHandle(handle); }

    if success == 0 {
        return Err(format!("读取进程亲和掩码失败 (PID: {})", pid));
    }
    Ok(process_mask as u64)
}

/// 设置指定进程亲和掩码（失败返回错误）
#[cfg(target_os = "windows")]
fn set_process_mask_sync(pid: u32, mask: u64) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::SetProcessAffinityMask;

    let handle = open_process_for_affinity(pid)?;
    let success = unsafe { SetProcessAffinityMask(handle, mask as usize) };
    let err = unsafe { GetLastError() };
    unsafe { CloseHandle(handle); }

    if success == 0 {
        return Err(format!(
            "设置进程亲和掩码失败, 错误码: {} (可能需要管理员权限)",
            err
        ));
    }
    Ok(())
}

// ── 应用 / 恢复核心隔离 ─────────────────────────────────────

#[cfg(target_os = "windows")]
fn apply_core_isolation_sync(
    app: &AppHandle,
    isolated_mask: u64,
    exclude_process: &str,
) -> Result<IsolationApplyResult, String> {
    // 边界校验：隔离核心必须有效且不能隔离全部核心
    if isolated_mask == 0 {
        return Err("请至少选择一个要隔离的核心".to_string());
    }
    let topology = get_cpu_topology_win32()?;
    let system_mask = topology.system_affinity_mask;
    if isolated_mask & system_mask == 0 {
        return Err("所选隔离核心不在系统可用核心范围内".to_string());
    }
    if isolated_mask == system_mask {
        return Err("不允许隔离全部核心，否则将导致系统无法调度".to_string());
    }
    let restricted = system_mask & !isolated_mask;
    if restricted == 0 {
        return Err("隔离后的受限掩码为空，操作已取消".to_string());
    }

    // 若已存在活动隔离状态，先自动恢复旧状态，避免原始掩码记录错乱
    if let Some(old_state) = read_isolation_state(app) {
        log::info!(
            "[核心隔离] 检测到活动隔离状态，先自动恢复旧状态 (隔离掩码 0x{:X})",
            old_state.isolated_mask
        );
        let _ = restore_core_isolation_state_sync(app, &old_state);
    }

    // 遍历全部进程（尽力而为，受保护/系统进程与豁免进程跳过）
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessRefreshKind::new());

    let mut result = IsolationApplyResult {
        total: 0,
        modified: 0,
        failed: 0,
        failed_processes: Vec::new(),
    };
    let mut modified_processes: Vec<IsolationModifiedProcess> = Vec::new();

    for (_, proc_) in sys.processes() {
        let name = proc_.name().to_string();
        if name.is_empty() || is_protected_process(&name) {
            continue;
        }
        // 跳过豁免进程（游戏进程，保持全核心）
        if !exclude_process.is_empty() && name.eq_ignore_ascii_case(exclude_process) {
            continue;
        }
        // 跳过 NexBox 自身，避免锁死 UI
        if proc_.pid().as_u32() == std::process::id() as u32 {
            continue;
        }

        let pid = proc_.pid().as_u32();
        result.total += 1;

        // 读取原始掩码（失败说明权限不足或进程已退出 → 计入失败）
        let original_mask = match get_process_mask_sync(pid) {
            Ok(m) => m,
            Err(_) => {
                result.failed += 1;
                result.failed_processes.push(name);
                continue;
            }
        };
        // 原始掩码已等于受限掩码 → 无需修改也无需记录
        if original_mask == restricted {
            continue;
        }

        match set_process_mask_sync(pid, restricted) {
            Ok(_) => {
                result.modified += 1;
                modified_processes.push(IsolationModifiedProcess {
                    pid,
                    name: name.clone(),
                    original_mask,
                });
            }
            Err(_) => {
                result.failed += 1;
                result.failed_processes.push(name);
            }
        }
    }

    // 保存活动状态，供恢复时逐条还原
    let state = IsolationStateRecord {
        isolated_mask,
        exclude_process: exclude_process.to_string(),
        modified_processes,
    };
    save_isolation_state(app, &state)?;

    log::info!(
        "[核心隔离] 应用完成: 隔离掩码 0x{:X}, 总进程 {}, 修改 {} 个, 失败 {} 个",
        isolated_mask,
        result.total,
        result.modified,
        result.failed
    );
    Ok(result)
}

#[cfg(not(target_os = "windows"))]
fn apply_core_isolation_sync(
    _app: &AppHandle,
    _isolated_mask: u64,
    _exclude_process: &str,
) -> Result<IsolationApplyResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

/// 按记录还原各进程原始亲和掩码（pid+name 双重校验，防止 PID 复用误改新进程）
#[cfg(target_os = "windows")]
fn restore_core_isolation_state_sync(
    _app: &AppHandle,
    state: &IsolationStateRecord,
) -> Result<IsolationApplyResult, String> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessRefreshKind::new());

    let mut result = IsolationApplyResult {
        total: 0,
        modified: 0,
        failed: 0,
        failed_processes: Vec::new(),
    };

    for rec in &state.modified_processes {
        result.total += 1;

        // 校验当前 pid 对应的进程名是否一致；进程已退出或 PID 被复用 → 跳过（新进程默认全核心）
        let name_matches = sys
            .process(Pid::from_u32(rec.pid))
            .map(|p| p.name().to_string().eq_ignore_ascii_case(&rec.name))
            .unwrap_or(false);
        if !name_matches {
            continue;
        }

        match set_process_mask_sync(rec.pid, rec.original_mask) {
            Ok(_) => result.modified += 1,
            Err(_) => {
                result.failed += 1;
                result.failed_processes.push(rec.name.clone());
            }
        }
    }

    log::info!(
        "[核心隔离] 恢复完成: 隔离掩码 0x{:X}, 总记录 {}, 还原 {} 个, 失败 {} 个",
        state.isolated_mask,
        result.total,
        result.modified,
        result.failed
    );
    Ok(result)
}

#[cfg(not(target_os = "windows"))]
fn restore_core_isolation_state_sync(
    _app: &AppHandle,
    _state: &IsolationStateRecord,
) -> Result<IsolationApplyResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

// ── 核心隔离 Tauri 命令 ─────────────────────────────────────

#[tauri::command]
pub async fn apply_core_isolation(
    app: AppHandle,
    isolated_mask: u64,
    exclude_process: String,
) -> Result<IsolationApplyResult, String> {
    apply_core_isolation_sync(&app, isolated_mask, &exclude_process)
}

#[tauri::command]
pub async fn restore_core_isolation(app: AppHandle) -> Result<IsolationApplyResult, String> {
    let state = read_isolation_state(&app)
        .ok_or_else(|| "当前没有活动中的核心隔离，无需恢复".to_string())?;
    let result = restore_core_isolation_state_sync(&app, &state)?;
    // 无论还原结果如何均清除活动状态（失败的进程可能已退出；仍在运行的由失败列表提示）
    clear_isolation_state(&app)?;
    Ok(result)
}

#[tauri::command]
pub async fn get_isolation_state(
    app: AppHandle,
) -> Result<Option<IsolationStateRecord>, String> {
    Ok(read_isolation_state(&app))
}

// ── 核心隔离规则 CRUD ───────────────────────────────────────

#[tauri::command]
pub async fn get_isolation_rules(app: AppHandle) -> Result<Vec<IsolationRule>, String> {
    let store = app
        .store(ISOLATION_RULES_FILE)
        .map_err(|e| format!("无法打开隔离规则存储: {}", e))?;
    let mut rules: Vec<IsolationRule> = Vec::new();

    for (name, value) in store.entries() {
        if let Some(obj) = value.as_object() {
            rules.push(IsolationRule {
                name,
                isolated_mask: obj
                    .get("isolated_mask")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0),
                exclude_process: obj
                    .get("exclude_process")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                preset: obj
                    .get("preset")
                    .and_then(|v| v.as_str())
                    .unwrap_or("custom")
                    .to_string(),
                description: obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                auto_apply: obj
                    .get("auto_apply")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            });
        }
    }

    rules.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(rules)
}

#[tauri::command]
pub async fn save_isolation_rule(
    app: AppHandle,
    name: String,
    isolated_mask: u64,
    exclude_process: String,
    preset: String,
    description: String,
    auto_apply: bool,
) -> Result<bool, String> {
    if name.trim().is_empty() {
        return Err("规则名称不能为空".to_string());
    }
    if isolated_mask == 0 {
        return Err("请至少选择一个要隔离的核心".to_string());
    }
    let store = app
        .store(ISOLATION_RULES_FILE)
        .map_err(|e| format!("无法打开隔离规则存储: {}", e))?;
    let rule = serde_json::json!({
        "isolated_mask": isolated_mask,
        "exclude_process": exclude_process,
        "preset": preset,
        "description": description,
        "auto_apply": auto_apply,
    });
    store.set(&name, rule);
    store
        .save()
        .map_err(|e| format!("保存隔离规则失败: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn delete_isolation_rule(app: AppHandle, name: String) -> Result<bool, String> {
    let store = app
        .store(ISOLATION_RULES_FILE)
        .map_err(|e| format!("无法打开隔离规则存储: {}", e))?;
    store.delete(&name);
    store
        .save()
        .map_err(|e| format!("删除隔离规则失败: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn apply_isolation_rule_by_name(
    app: AppHandle,
    name: String,
) -> Result<IsolationApplyResult, String> {
    let store = app
        .store(ISOLATION_RULES_FILE)
        .map_err(|e| format!("无法打开隔离规则存储: {}", e))?;
    let value = store
        .get(&name)
        .ok_or_else(|| format!("未找到隔离规则: {}", name))?;
    let obj = value
        .as_object()
        .ok_or_else(|| format!("隔离规则数据异常: {}", name))?;
    let isolated_mask = obj
        .get("isolated_mask")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("隔离规则缺少掩码: {}", name))?;
    let exclude_process = obj
        .get("exclude_process")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    apply_core_isolation_sync(&app, isolated_mask, &exclude_process)
}

// ── 启动时自动应用标记 auto_apply 的隔离规则 ─────────────────

async fn apply_auto_isolation_rules(app: &AppHandle) {
    let store = match app.store(ISOLATION_RULES_FILE) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[核心隔离] 无法打开隔离规则存储: {}", e);
            return;
        }
    };

    let mut applied = 0u32;
    for (name, value) in store.entries() {
        let obj = match value.as_object() {
            Some(o) => o,
            None => continue,
        };
        if !obj
            .get("auto_apply")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let isolated_mask = match obj.get("isolated_mask").and_then(|v| v.as_u64()) {
            Some(m) => m,
            None => continue,
        };
        let exclude_process = obj
            .get("exclude_process")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        match apply_core_isolation_sync(app, isolated_mask, &exclude_process) {
            Ok(result) => {
                applied += 1;
                log::info!(
                    "[核心隔离] 启动时自动应用规则 {}: 修改 {} 个进程",
                    name,
                    result.modified
                );
            }
            Err(e) => log::warn!("[核心隔离] 启动时应用规则 {} 失败: {}", name, e),
        }
    }

    if applied > 0 {
        log::info!("[核心隔离] 启动时自动应用完成: {} 条隔离规则", applied);
    }
}
