//! 蓝屏日志（BSOD Log）后端
//!
//! 参考 BSODAnalyzer-main：从事件日志 + Minidump + BugCheck 知识库聚合证据，
//! 输出崩溃记录、原因分析与修复建议。不做 windbg 集成、不做符号服务。
//!
//! 三个 Tauri 命令：
//! - `bsod_collect_evidence()` 一次拉全量证据并本地归因
//! - `bsod_open_dump_dir()` 打开 C:\Windows\Minidump
//! - `bsod_reveal_dump(path)` 在资源管理器中定位单个 dmp

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Globalization::GetACP;
use windows_sys::Win32::System::ProcessStatus::{EnumPageFilesW, ENUM_PAGE_FILE_INFORMATION};
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformation, GetSystemInfo, GetTickCount64, GlobalMemoryStatusEx,
    RelationProcessorCore, MEMORYSTATUSEX, SYSTEM_INFO, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use crate::bsod_bugcheck as bugcheck;

/// 避免控制台窗口闪现（与仓库其他模块一致，手写常量）
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ============================================================================
// 数据结构
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemInfo {
    pub caption: String,
    pub version: String,
    pub build: String,
    pub arch: String,
    pub install_date: String,
    pub last_boot: String,
    pub mem_total: u64,
    pub mem_free: u64,
    pub cpu_name: String,
    pub cpu_cores: u32,
    pub cpu_threads: u32,
    pub machine_vendor: String,
    pub machine_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DumpConfig {
    pub crash_dump_enabled: i64,
    pub dump_file: String,
    pub pagefile_alloc_mb: u64,
    pub pagefile_current_mb: u64,
    pub pagefile_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub t: String,
    pub id: u32,
    pub prov: String,
    pub level: String,
    pub task: String,
    pub msg: String,
    pub props: Vec<String>,
    /// "crash" | "whea" | "disk" | "power" | "other"
    pub group: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpInfo {
    pub path: String,
    pub file_size: u64,
    pub mtime: String,
    pub format: String,
    pub bugcheck: Option<u32>,
    pub bugcheck_hex: String,
    pub args: Vec<u64>,
    pub exception_address: Option<u64>,
    pub arch: String,
    pub os_build: String,
    pub processor_count: u32,
    pub ok: bool,
    pub error: String,
    pub is_live: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgExplain {
    pub value: u64,
    pub hex: String,
    pub meaning: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashRecord {
    pub time: String,
    pub bugcheck: Option<u32>,
    pub code_hex: String,
    pub name: String,
    pub bugcheck_cn: String,
    pub category: String,
    pub severity: String,
    pub args: Vec<u64>,
    pub explains: Vec<ArgExplain>,
    pub causes: Vec<String>,
    pub fixes: Vec<String>,
    pub dump_path: String,
    pub dump_format: String,
    pub dump_size: u64,
    pub dump_ok: bool,
    pub sources: Vec<String>,
    pub related: Vec<EventRecord>,
    pub is_live: bool,
    pub note: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub evidence: Vec<String>,
    pub actions: Vec<String>,
    pub confidence: f64,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub status: String,
    pub headline: String,
    pub sub: String,
    pub count_crashes: usize,
    pub count_whea_events: usize,
    pub count_suspects: usize,
    pub count_dumps_ok: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BsodEvidence {
    pub system: SystemInfo,
    pub config: DumpConfig,
    pub dumps: Vec<DumpInfo>,
    pub events: Vec<EventRecord>,
    pub crashes: Vec<CrashRecord>,
    pub findings: Vec<Finding>,
    pub summary: Summary,
    pub errors: Vec<String>,
    pub generated_at: String,
}

// ============================================================================
// 常量：分类 / 时间窗
// ============================================================================

const T_WINDOW_SEC: i64 = 600;

const HARDWARE_CODES: [u32; 9] = [0x124, 0x101, 0x12B, 0x9C, 0x2E, 0x80, 0x147, 0x1A, 0x4E];
#[allow(dead_code)]
const STORAGE_CODES: [u32; 9] = [0x7A, 0x7B, 0x24, 0xED, 0xF4, 0x51, 0x154, 0x133, 0x191];
#[allow(dead_code)]
const GRAPHICS_CODES: [u32; 7] = [0x116, 0x117, 0x119, 0x10E, 0xB4, 0xEA, 0x14C];

const WHEA_TEXT: &str = "WHEA-Logger 报告了硬件级错误。WHEA 是 CPU/芯片组对「不可纠正错误」的上报通道，出现该类事件时故障几乎都落在硬件或固件层面（CPU 供电/超频、内存、PCIe 设备、主板），而不是 Windows 本身。";

// ============================================================================
// 原生采集：注册表 / Win32 API / wevtutil（无 PowerShell、无 WMI）
// ============================================================================

// ============================================================================
// 主命令
// ============================================================================

#[tauri::command]
pub async fn bsod_collect_evidence() -> Result<BsodEvidence, String> {
    tokio::task::spawn_blocking(collect_evidence_sync)
        .await
        .map_err(|e| format!("任务调度失败: {}", e))?
}

fn collect_evidence_sync() -> Result<BsodEvidence, String> {
    let mut errors: Vec<String> = Vec::new();
    let queries = [
        EventQuery {
            log: "System",
            xpath: "*[System[(EventID=1001 or EventID=41 or EventID=6008)]]",
            max: 200,
            group: "crash",
        },
        EventQuery {
            log: "Application",
            xpath: "*[System[Provider[@Name='Microsoft-Windows-WHEA-Logger'] and (EventID=17 or EventID=18 or EventID=19)]]",
            max: 200,
            group: "whea",
        },
        EventQuery {
            log: "System",
            xpath: "*[System[(EventID=7 or EventID=11 or EventID=51 or EventID=55 or EventID=98 or EventID=129 or EventID=153 or EventID=157)]]",
            max: 300,
            group: "disk",
        },
    ];

    // 三组事件日志查询最耗时（约 80–110ms/组），与注册表、转储枚举并行。
    let (system, config, dumps, event_results) = std::thread::scope(|scope| {
        let handles: Vec<_> = queries
            .iter()
            .map(|q| scope.spawn(move || query_events(q)))
            .collect();

        let system = collect_system_info(&mut errors);
        let config = collect_dump_config(&mut errors);
        let dumps = collect_dumps();

        let mut results = Vec::with_capacity(handles.len());
        for handle in handles {
            results.push(handle.join());
        }
        (system, config, dumps, results)
    });

    let mut events = Vec::new();
    for (i, joined) in event_results.into_iter().enumerate() {
        match joined {
            Ok(Ok(mut evs)) => events.append(&mut evs),
            Ok(Err(e)) => errors.push(format!("事件日志 {}: {}", queries[i].log, e)),
            Err(_) => errors.push(format!("事件日志 {}: 查询线程异常退出", queries[i].log)),
        }
    }

    let (crashes, findings, summary) = analyze(&system, &config, &dumps, &events, &errors);

    Ok(BsodEvidence {
        system, config, dumps, events, crashes, findings, summary, errors,
        generated_at: now_local_string(),
    })
}

struct EventQuery {
    log: &'static str,
    xpath: &'static str,
    max: u32,
    group: &'static str,
}

fn query_events(q: &EventQuery) -> Result<Vec<EventRecord>, String> {
    let exe = system32_exe("wevtutil.exe");
    let filter = format!("/q:{}", q.xpath);
    let count = format!("/c:{}", q.max);
    let output = Command::new(&exe)
        .args(["qe", q.log, filter.as_str(), "/f:xml", count.as_str(), "/rd:true"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("启动 wevtutil 失败: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "退出码 {:?}: {}",
            output.status.code(),
            decode_console_text(&output.stderr).trim()
        ));
    }
    Ok(parse_events_xml(&decode_console_text(&output.stdout), q.group))
}

fn system32_exe(name: &str) -> PathBuf {
    std::env::var("SystemRoot")
        .map(|root| Path::new(&root).join("System32").join(name))
        .unwrap_or_else(|_| PathBuf::from(name))
}

/// wevtutil 的输出编码跟随系统 ANSI 代码页（中文系统是 GBK）：
/// 先按 UTF-8 严格解码，失败再用 GetACP 对应的编码。
fn decode_console_text(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let acp = unsafe { GetACP() };
    let label = match acp {
        936 => "gbk",
        950 => "big5",
        932 => "shift_jis",
        949 => "euc-kr",
        65001 => "utf-8",
        _ => "windows-1252",
    };
    let enc = encoding_rs::Encoding::for_label(label.as_bytes())
        .unwrap_or(encoding_rs::WINDOWS_1252);
    let (text, _, _) = enc.decode(bytes);
    text.into_owned()
}

fn parse_events_xml(xml: &str, group: &str) -> Vec<EventRecord> {
    use regex::Regex;
    static PROVIDER_RE: OnceLock<Regex> = OnceLock::new();
    static EVENT_ID_RE: OnceLock<Regex> = OnceLock::new();
    static TIME_RE: OnceLock<Regex> = OnceLock::new();
    static LEVEL_RE: OnceLock<Regex> = OnceLock::new();
    static DATA_RE: OnceLock<Regex> = OnceLock::new();

    let provider_re =
        PROVIDER_RE.get_or_init(|| Regex::new(r"Provider Name='([^']*)'").unwrap());
    let event_id_re =
        EVENT_ID_RE.get_or_init(|| Regex::new(r"<EventID[^>]*>(\d+)</EventID>").unwrap());
    let time_re = TIME_RE.get_or_init(|| Regex::new(r"SystemTime='([^']+)'").unwrap());
    let level_re = LEVEL_RE.get_or_init(|| Regex::new(r"<Level>(\d+)</Level>").unwrap());
    let data_re = DATA_RE
        .get_or_init(|| Regex::new(r"(?s)<Data(?: Name='([^']*)')?>([^<]*)</Data>").unwrap());

    let mut events = Vec::new();
    for raw in xml.split("<Event ").skip(1) {
        let block = raw.split("</Event>").next().unwrap_or(raw);
        let Some(id) = event_id_re
            .captures(block)
            .and_then(|c| c[1].parse::<u32>().ok())
        else {
            continue;
        };
        let prov = provider_re
            .captures(block)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let time_raw = time_re
            .captures(block)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let t = parse_event_time(&time_raw);
        let level = level_key(
            level_re
                .captures(block)
                .and_then(|c| c[1].parse::<u32>().ok())
                .unwrap_or(4),
        )
        .to_string();

        let mut named: Vec<(String, String)> = Vec::new();
        for cap in data_re.captures_iter(block) {
            let name = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            named.push((name, xml_unescape(&cap[2])));
        }

        if group == "crash" {
            match id {
                1001 => {
                    // 只认带 BugcheckCode 的 1001，避免把其它来源的 1001 当成蓝屏。
                    if !named
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("BugcheckCode"))
                    {
                        continue;
                    }
                }
                41 => {
                    if !prov.to_ascii_lowercase().contains("kernel-power") {
                        continue;
                    }
                }
                6008 => {
                    if !prov.eq_ignore_ascii_case("EventLog") {
                        continue;
                    }
                }
                _ => continue,
            }
        } else if group == "whea" && !prov.to_ascii_lowercase().contains("whea") {
            continue;
        }

        let props = event_props(id, &named);
        let msg = synth_event_message(id, group, &named, &props);
        if group == "disk" {
            // 与旧 PS 采集脚本一致：存储堆栈白名单外的同 ID 事件（Kernel-Processor-Power、
            // Filter Manager、SCM 等）不进入事件列表；NTFS 98 的健康巡检也不计入。
            if !is_disk_provider(&prov) {
                continue;
            }
            if id == 98 && is_healthy_volume_msg(&msg) {
                continue;
            }
        }
        events.push(EventRecord {
            t,
            id,
            prov,
            level,
            task: String::new(),
            msg,
            props,
            group: group.to_string(),
        });
    }
    events
}

fn parse_event_time(raw: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| raw.chars().take(19).collect::<String>().replace('T', " "))
}

fn level_key(level: u32) -> &'static str {
    match level {
        1 => "critical",
        2 => "error",
        3 => "warning",
        5 => "verbose",
        _ => "information",
    }
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#13;", " ")
        .replace("&#10;", " ")
        .replace("&amp;", "&")
}

fn event_props(id: u32, named: &[(String, String)]) -> Vec<String> {
    if id == 1001 || id == 41 {
        let get = |name: &str| {
            named
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.clone())
        };
        if let Some(code) = get("BugcheckCode") {
            let mut props = Vec::with_capacity(5);
            props.push(code);
            for i in 1..=4 {
                if let Some(v) = get(&format!("BugcheckParameter{}", i)) {
                    props.push(v);
                }
            }
            return props;
        }
    }
    named
        .iter()
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
        .collect()
}

fn synth_event_message(
    id: u32,
    group: &str,
    named: &[(String, String)],
    props: &[String],
) -> String {
    match (group, id) {
        ("crash", 1001) => {
            let code = props
                .first()
                .and_then(|s| parse_evt_u32(s))
                .filter(|c| *c != 0);
            match code {
                Some(c) => format!(
                    "检测错误: {} ({})",
                    bugcheck::format_code(c),
                    format_bugcheck_args(props)
                ),
                None => "系统记录了蓝屏事件（未解析到错误码）".to_string(),
            }
        }
        ("crash", 41) => {
            let code = props
                .first()
                .and_then(|s| parse_evt_u32(s))
                .filter(|c| *c != 0);
            match code {
                Some(c) => format!(
                    "系统非正常关机：BugCheck {} ({})",
                    bugcheck::format_code(c),
                    format_bugcheck_args(props)
                ),
                None => "系统非正常关机（未记录 BugCheck 码）".to_string(),
            }
        }
        ("crash", 6008) => "上一次系统关机是意外的。".to_string(),
        ("disk", 98) => {
            let drive = named_value(named, "DriveName").unwrap_or_else(|| "卷".to_string());
            match named_value(named, "CorruptionActionState") {
                Some(state) if state == "0" => {
                    format!("{} 运行状况良好，无需执行任何操作。", drive)
                }
                Some(state) => format!("{} 需要修复（CorruptionActionState={}）。", drive, state),
                None => format!("{} 状态未知（事件 98）。", drive),
            }
        }
        _ => join_event_data(named, 700),
    }
}

fn format_bugcheck_args(props: &[String]) -> String {
    props
        .iter()
        .skip(1)
        .take(4)
        .map(|s| parse_evt_u64_raw(s).unwrap_or(0))
        .map(|v| format!("0x{:016X}", v))
        .collect::<Vec<_>>()
        .join(", ")
}

fn named_value(named: &[(String, String)], name: &str) -> Option<String> {
    named
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn join_event_data(named: &[(String, String)], max: usize) -> String {
    let text = named
        .iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(n, v)| {
            if n.is_empty() {
                v.trim().to_string()
            } else {
                format!("{}={}", n, v.trim())
            }
        })
        .collect::<Vec<_>>()
        .join(" ｜ ");
    truncate(&text, max)
}

/// BugcheckCode 在事件 XML 里是十进制，BugcheckParameterN 是带 0x 的十六进制；
/// 统一按「有 0x 走十六进制，否则十进制」解析。
fn parse_evt_u64_raw(s: &str) -> Option<u64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else if let Ok(v) = t.parse::<u64>() {
        Some(v)
    } else {
        u64::from_str_radix(t, 16).ok()
    }
}

fn parse_evt_u32(s: &str) -> Option<u32> {
    parse_evt_u64_raw(s).map(|v| v as u32)
}

fn collect_system_info(errors: &mut Vec<String>) -> SystemInfo {
    use chrono::TimeZone;

    let mut system = SystemInfo::default();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    match hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
        Ok(key) => {
            let product: String = key.get_value("ProductName").unwrap_or_default();
            system.caption = crate::hardware::long_os_version()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(product);
            system.version = key.get_value("CurrentVersion").unwrap_or_default();
            let build: String = key
                .get_value::<String, _>("CurrentBuildNumber")
                .or_else(|_| key.get_value::<String, _>("CurrentBuild"))
                .unwrap_or_default();
            let ubr: u32 = key.get_value("UBR").unwrap_or(0);
            system.build = if build.trim().is_empty() {
                String::new()
            } else if ubr > 0 {
                format!("{}.{}", build.trim(), ubr)
            } else {
                build.trim().to_string()
            };
            if let Ok(install) = key.get_value::<u32, _>("InstallDate") {
                system.install_date = chrono::Local
                    .timestamp_opt(install as i64, 0)
                    .single()
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
            }
        }
        Err(e) => errors.push(format!("OS: {}", e)),
    }

    system.arch = if std::env::var_os("PROCESSOR_ARCHITEW6432").is_some() {
        "64-bit".to_string()
    } else if std::env::var("PROCESSOR_ARCHITECTURE")
        .map(|v| v.eq_ignore_ascii_case("x86"))
        .unwrap_or(false)
    {
        "32-bit".to_string()
    } else {
        "64-bit".to_string()
    };

    let mut mem: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut mem) } != 0 {
        system.mem_total = mem.ullTotalPhys;
        system.mem_free = mem.ullAvailPhys;
    }

    let uptime_ms = unsafe { GetTickCount64() };
    system.last_boot = SystemTime::now()
        .checked_sub(std::time::Duration::from_millis(uptime_ms))
        .map(|boot| {
            let dt: chrono::DateTime<chrono::Local> = boot.into();
            dt.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_default();

    match hklm.open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") {
        Ok(key) => {
            system.cpu_name = key
                .get_value::<String, _>("ProcessorNameString")
                .unwrap_or_default()
                .trim()
                .to_string();
        }
        Err(e) => errors.push(format!("CPU: {}", e)),
    }
    system.cpu_cores = count_physical_cores();
    system.cpu_threads = std::env::var("NUMBER_OF_PROCESSORS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(|n| n.get() as u32)
        })
        .unwrap_or(0);

    if let Ok(key) = hklm.open_subkey(r"HARDWARE\DESCRIPTION\System\BIOS") {
        system.machine_vendor = key
            .get_value::<String, _>("SystemManufacturer")
            .unwrap_or_default()
            .trim()
            .to_string();
        system.machine_model = key
            .get_value::<String, _>("SystemProductName")
            .unwrap_or_default()
            .trim()
            .to_string();
    }

    system
}

fn count_physical_cores() -> u32 {
    unsafe {
        let mut len: u32 = 0;
        GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len);
        if len == 0 {
            return 0;
        }
        let mut buf = vec![0u8; len as usize];
        if GetLogicalProcessorInformation(
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
            &mut len,
        ) == 0
        {
            return 0;
        }
        let count = len as usize / std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        let entries = std::slice::from_raw_parts(
            buf.as_ptr() as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
            count,
        );
        entries
            .iter()
            .filter(|info| info.Relationship == RelationProcessorCore)
            .count() as u32
    }
}

fn collect_dump_config(errors: &mut Vec<String>) -> DumpConfig {
    let mut config = DumpConfig::default();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    match hklm.open_subkey(r"SYSTEM\CurrentControlSet\Control\CrashControl") {
        Ok(key) => {
            config.crash_dump_enabled = key
                .get_value::<u32, _>("CrashDumpEnabled")
                .map(|v| v as i64)
                .unwrap_or(-1);
            config.dump_file =
                expand_env(&key.get_value::<String, _>("DumpFile").unwrap_or_default());
        }
        Err(e) => errors.push(format!("CrashControl: {}", e)),
    }

    if let Ok(key) = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management")
    {
        if let Ok(lines) = key.get_value::<Vec<String>, _>("PagingFiles") {
            if let Some(first) = lines.first() {
                config.pagefile_name = first
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }

    // 页面文件：EnumPageFilesW 直接给出每个分页文件的 TotalSize/TotalInUse（页数），
    // 与原来的 Win32_PageFileUsage.AllocatedBaseSize/CurrentUsage 对应，且不需要 WMI。
    let page_size = system_page_size();
    let mut acc = PagefileAccum::default();
    if unsafe {
        EnumPageFilesW(
            Some(pagefile_enum_cb),
            &mut acc as *mut PagefileAccum as *mut core::ffi::c_void,
        )
    } != 0
    {
        if !acc.name.is_empty() {
            config.pagefile_name = acc.name;
        }
        let mb = 1024 * 1024;
        config.pagefile_alloc_mb = acc.alloc_pages.saturating_mul(page_size) / mb;
        config.pagefile_current_mb = acc.current_pages.saturating_mul(page_size) / mb;
    } else {
        // 枚举失败时用提交限制 - 物理内存近似页面文件总量。
        let mut mem: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        mem.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut mem) } != 0 {
            let mb = 1024 * 1024;
            config.pagefile_alloc_mb = mem.ullTotalPageFile.saturating_sub(mem.ullTotalPhys) / mb;
            let committed = mem.ullTotalPageFile.saturating_sub(mem.ullAvailPageFile);
            let physical_in_use = mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys);
            config.pagefile_current_mb = committed.saturating_sub(physical_in_use) / mb;
        }
    }

    config
}

#[derive(Default)]
struct PagefileAccum {
    alloc_pages: u64,
    current_pages: u64,
    name: String,
}

unsafe extern "system" fn pagefile_enum_cb(
    context: *mut core::ffi::c_void,
    info: *mut ENUM_PAGE_FILE_INFORMATION,
    filename: windows_sys::core::PCWSTR,
) -> windows_sys::Win32::Foundation::BOOL {
    let acc = &mut *(context as *mut PagefileAccum);
    if !info.is_null() {
        acc.alloc_pages += (*info).TotalSize as u64;
        acc.current_pages += (*info).TotalInUse as u64;
    }
    if acc.name.is_empty() && !filename.is_null() {
        acc.name = pcwstr_to_string(filename);
    }
    1
}

fn pcwstr_to_string(ptr: *const u16) -> String {
    let mut len = 0usize;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

fn system_page_size() -> u64 {
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    unsafe { GetSystemInfo(&mut info) };
    if info.dwPageSize == 0 {
        4096
    } else {
        info.dwPageSize as u64
    }
}

fn expand_env(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if let Ok(v) = std::env::var(name) {
                out.push_str(&v);
            } else {
                out.push('%');
                out.push_str(name);
                out.push('%');
            }
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

type DumpFileEntry = (String, u64, String);

fn collect_dumps() -> Vec<DumpInfo> {
    let mut files = list_dump_files();
    files.sort_by(|a, b| b.2.cmp(&a.2));
    files
        .into_iter()
        .map(|(path, size, mtime)| parse_dump_file(&path, size, &mtime))
        .collect()
}

fn list_dump_files() -> Vec<DumpFileEntry> {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let mut out: Vec<DumpFileEntry> = Vec::new();

    let minidump = Path::new(&root).join("Minidump");
    if let Ok(entries) = fs::read_dir(&minidump) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !has_dmp_ext(&path) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                out.push((
                    path.to_string_lossy().to_string(),
                    meta.len(),
                    fmt_mtime(meta.modified().ok()),
                ));
            }
        }
    }

    let memory = Path::new(&root).join("MEMORY.DMP");
    if let Ok(meta) = fs::metadata(&memory) {
        out.push((
            memory.to_string_lossy().to_string(),
            meta.len(),
            fmt_mtime(meta.modified().ok()),
        ));
    }

    let live = Path::new(&root).join("LiveKernelReports");
    if live.is_dir() {
        for entry in walkdir::WalkDir::new(&live)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() || !has_dmp_ext(entry.path()) {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                out.push((
                    entry.path().to_string_lossy().to_string(),
                    meta.len(),
                    fmt_mtime(meta.modified().ok()),
                ));
            }
        }
    }

    out
}

fn has_dmp_ext(path: &Path) -> bool {
    path.extension()
        .map(|e| e.eq_ignore_ascii_case("dmp"))
        .unwrap_or(false)
}

fn fmt_mtime(t: Option<SystemTime>) -> String {
    t.map(|t| {
        let dt: chrono::DateTime<chrono::Local> = t.into();
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    })
    .unwrap_or_default()
}

// ============================================================================
// 转储文件解析
// ============================================================================

const MDMP_SIG: u32 = 0x504D_444D;    // 'MDMP'
const PAGE_SIG_LE: u32 = 0x4547_4150; // 'PAGE'
const PAGE_SIG_BE: u32 = 0x5041_4745; // 'EGAP'
const DU64_TAG: u32 = 0x3436_5544;    // 'DU64'
const DUMP_TAG: u32 = 0x504D_5544;    // 'DUMP'

const ST_EXCEPTION: u32 = 6;
const ST_SYSTEM_INFO: u32 = 7;

const KERNEL_ADDR_MIN: u64 = 0xFFFF_0000_0000_0000;

fn parse_dump_file(path: &str, size: u64, mtime: &str) -> DumpInfo {
    let mut info = DumpInfo {
        path: path.to_string(),
        file_size: size,
        mtime: mtime.to_string(),
        format: String::new(),
        bugcheck: None,
        bugcheck_hex: String::new(),
        args: vec![],
        exception_address: None,
        arch: String::new(),
        os_build: String::new(),
        processor_count: 0,
        ok: false,
        error: String::new(),
        is_live: path.contains("LiveKernel"),
    };
    if size < 32 { info.error = "文件过小".to_string(); return info; }

    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => { info.error = format!("打开失败: {}", e); return info; }
    };
    let mut head8 = [0u8; 8];
    if file.read_exact(&mut head8).is_err() {
        info.error = "读取头部失败".to_string(); return info;
    }
    let sig = u32::from_le_bytes([head8[0], head8[1], head8[2], head8[3]]);
    let tag = u32::from_le_bytes([head8[4], head8[5], head8[6], head8[7]]);

    let result = if sig == MDMP_SIG {
        info.format = "MDMP".to_string();
        parse_mdmp(&mut file, &mut info)
    } else if sig == PAGE_SIG_LE || sig == PAGE_SIG_BE {
        if tag == DU64_TAG {
            info.format = "PAGEDU64".to_string();
        } else if tag == DUMP_TAG {
            info.format = "PAGEDUMP".to_string();
        } else {
            info.format = "PAGE".to_string();
        }
        parse_pagedump(&mut file, &mut info)
    } else {
        info.format = "UNKNOWN".to_string();
        info.error = format!("无法识别的转储签名 0x{:08X}", sig);
        Ok(())
    };
    if let Err(e) = result { if info.error.is_empty() { info.error = e; } }
    info.ok = info.bugcheck.is_some();
    if let Some(c) = info.bugcheck { info.bugcheck_hex = bugcheck::format_code(c); }
    info
}

fn read_u32_at<R: Read + Seek>(r: &mut R, off: u64) -> Option<u32> {
    r.seek(SeekFrom::Start(off)).ok()?;
    let mut b = [0u8; 4];
    r.read_exact(&mut b).ok()?;
    Some(u32::from_le_bytes(b))
}

fn read_u8_at<R: Read + Seek>(r: &mut R, off: u64) -> Option<u8> {
    r.seek(SeekFrom::Start(off)).ok()?;
    let mut b = [0u8; 1];
    r.read_exact(&mut b).ok()?;
    Some(b[0])
}

fn read_u64_at<R: Read + Seek>(r: &mut R, off: u64) -> Option<u64> {
    r.seek(SeekFrom::Start(off)).ok()?;
    let mut b = [0u8; 8];
    r.read_exact(&mut b).ok()?;
    Some(u64::from_le_bytes(b))
}

fn parse_mdmp<R: Read + Seek>(r: &mut R, info: &mut DumpInfo) -> Result<(), String> {
    let n_streams = read_u32_at(r, 8).unwrap_or(0);
    let dir_rva = read_u32_at(r, 12).unwrap_or(0);
    if n_streams == 0 || n_streams > 200 { return Err(format!("MDMP 流数量异常: {}", n_streams)); }
    if dir_rva == 0 { return Err("MDMP 目录 RVA 为 0".to_string()); }

    let mut exc_rva: Option<u32> = None;
    let mut sys_rva: Option<u32> = None;
    for i in 0..n_streams {
        let base = (dir_rva as u64) + (i as u64) * 12;
        let stype = match read_u32_at(r, base) { Some(v) => v, None => continue };
        let rva = match read_u32_at(r, base + 8) { Some(v) => v, None => continue };
        match stype {
            ST_EXCEPTION if exc_rva.is_none() => exc_rva = Some(rva),
            ST_SYSTEM_INFO if sys_rva.is_none() => sys_rva = Some(rva),
            _ => {}
        }
    }

    if let Some(er) = exc_rva {
        let code = read_u32_at(r, (er as u64) + 8).unwrap_or(0);
        let exc_addr = read_u64_at(r, (er as u64) + 24).unwrap_or(0);
        let n_params = read_u32_at(r, (er as u64) + 32).unwrap_or(0).min(15);
        if code != 0 { info.bugcheck = Some(code); }
        if exc_addr != 0 { info.exception_address = Some(exc_addr); }
        let mut args = Vec::with_capacity(4);
        for k in 0..n_params.min(4) {
            let v = read_u64_at(r, (er as u64) + 40 + (k as u64) * 8).unwrap_or(0);
            args.push(v);
        }
        info.args = args;
    }

    if let Some(sr) = sys_rva {
        let arch = read_u32_at(r, sr as u64).unwrap_or(0) & 0xFFFF;
        info.arch = match arch { 0 => "x86".into(), 5 => "ARM".into(), 6 => "IA64".into(),
            9 => "AMD64".into(), 12 => "ARM64".into(), v => format!("未知({})", v) };
        // MINIDUMP_SYSTEM_INFO.NumberOfProcessors 是 +6 处的 UCHAR；
        // 旧实现按 +4 读 u32 & 0xFF，实际拿到的是 ProcessorRevision 低字节。
        info.processor_count = read_u8_at(r, (sr as u64) + 6).unwrap_or(0) as u32;
        let major = read_u32_at(r, (sr as u64) + 8).unwrap_or(0);
        let minor = read_u32_at(r, (sr as u64) + 12).unwrap_or(0);
        let build = read_u32_at(r, (sr as u64) + 16).unwrap_or(0);
        if build != 0 { info.os_build = format!("{}.{}.{}", major, minor, build); }
        else if major != 0 { info.os_build = format!("{}.{}", major, minor); }
    }
    Ok(())
}

fn parse_pagedump<R: Read + Seek>(r: &mut R, info: &mut DumpInfo) -> Result<(), String> {
    let code = read_u32_at(r, 0x38).unwrap_or(0);
    if code != 0 { info.bugcheck = Some(code); }
    let mut args = Vec::with_capacity(4);
    for off in [0x40u64, 0x48, 0x50, 0x58] { args.push(read_u64_at(r, off).unwrap_or(0)); }
    info.args = args;
    let machine = read_u32_at(r, 0x30).unwrap_or(0);
    info.arch = match machine { 0x014C => "x86".into(), 0x8664 => "AMD64".into(),
        0xAA64 => "ARM64".into(), 0x0200 => "IA64".into(), 0x01C0 | 0x01C4 => "ARM".into(),
        _ => String::new() };
    let major = read_u32_at(r, 0x08).unwrap_or(0);
    let minor = read_u32_at(r, 0x0C).unwrap_or(0);
    if major != 0 { info.os_build = format!("{}.{}", major, minor); }
    info.processor_count = read_u32_at(r, 0x34).unwrap_or(0);
    if info.bugcheck.is_none() {
        info.error = "转储头中 BugCheck 码为 0（可能是 LiveKernel 事件）".to_string();
    }
    Ok(())
}

// ============================================================================
// 归因
// ============================================================================

fn parse_ts(s: &str) -> Option<i64> {
    if s.len() < 19 { return None; }
    chrono::NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%d %H:%M:%S")
        .ok()
        .and_then(|dt| dt.and_local_timezone(chrono::Local).single())
        .map(|z| z.timestamp())
}

fn fmt_ts(secs: i64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_opt(secs, 0).single()
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

fn now_local_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_bugcheck_msg(msg: &str) -> (Option<u32>, Vec<u64>, String) {
    use regex::Regex;
    use std::sync::OnceLock;
    static CODE_RE: OnceLock<Regex> = OnceLock::new();
    static ARG_RE: OnceLock<Regex> = OnceLock::new();
    static DUMP_RE: OnceLock<Regex> = OnceLock::new();
    static SHORT: OnceLock<Regex> = OnceLock::new();
    let code_re = CODE_RE.get_or_init(|| Regex::new("0x([0-9a-fA-F]{8})").unwrap());
    let arg_re = ARG_RE.get_or_init(|| Regex::new("0x([0-9a-fA-F]{16})").unwrap());
    // PS 脚本里已经把消息内的换行替换成空格；此处 `.` 默认匹配任意非换行符即可。
    let dump_re = DUMP_RE.get_or_init(|| Regex::new("(?i)([A-Za-z]:.{0,220}\\.dmp)").unwrap());
    let short_re = SHORT.get_or_init(|| Regex::new("0x([0-9a-fA-F]{1,16})").unwrap());

    let code = code_re.captures(msg).and_then(|c| u32::from_str_radix(&c[1], 16).ok());
    let mut args: Vec<u64> = arg_re.captures_iter(msg).take(4)
        .filter_map(|cap| u64::from_str_radix(&cap[1], 16).ok())
        .collect();
    if args.len() < 4 {
        let vals: Vec<u64> = short_re.captures_iter(msg).skip(1).take(4)
            .filter_map(|cap| u64::from_str_radix(&cap[1], 16).ok()).collect();
        if vals.len() == 4 { args = vals; }
    }
    let dump = dump_re.captures(msg).map(|c| c[1].to_string()).unwrap_or_default();
    (code, args, dump)
}

/// 从事件 XML 的结构化 props 里取 BugCheck 码与参数：
/// props 顺序为 [BugcheckCode, BugcheckParameter1..4]。
fn parse_bugcheck_fields(props: &[String]) -> Option<(u32, Vec<u64>)> {
    let code = props
        .first()
        .and_then(|s| parse_evt_u32(s))
        .filter(|c| *c != 0)?;
    let args = props
        .iter()
        .skip(1)
        .take(4)
        .filter_map(|s| parse_evt_u64_raw(s))
        .collect();
    Some((code, args))
}

#[derive(Debug, Default, Clone)]
struct CrashAgg {
    time: i64,
    bugcheck: Option<u32>,
    args: Vec<u64>,
    dump_path: String,
    dump_idx: Option<usize>,
    sources: Vec<String>,
    event_msgs: Vec<EventRecord>,
    is_live: bool,
    note: String,
}

fn analyze(system: &SystemInfo, config: &DumpConfig, dumps: &[DumpInfo], events: &[EventRecord],
           errors: &[String]) -> (Vec<CrashRecord>, Vec<Finding>, Summary) {
    // 1. 合并证据
    let mut crashes: Vec<CrashAgg> = Vec::new();
    for (i, d) in dumps.iter().enumerate() {
        let t = parse_ts(&d.mtime).unwrap_or(0);
        crashes.push(CrashAgg {
            time: t, bugcheck: d.bugcheck, args: d.args.clone(),
            dump_path: d.path.clone(), dump_idx: Some(i),
            sources: vec![format!("转储文件 {}", d.format)],
            is_live: d.is_live, ..Default::default()
        });
    }
    for e in events.iter().filter(|e| e.group == "crash") {
        let t = parse_ts(&e.t).unwrap_or(0);
        match e.id {
            1001 => {
                let (code, args, dump_path) = match parse_bugcheck_fields(&e.props) {
                    Some((c, a)) => (Some(c), a, String::new()),
                    None => parse_bugcheck_msg(&e.msg),
                };
                // 非蓝屏来源的 1001（或码为 0）不生成崩溃记录；XML 解析路径已在
                // parse_events_xml 里按 BugcheckCode 字段过滤，这里兜住异常输入。
                let Some(code) = code.filter(|c| *c != 0) else { continue; };
                if let Some(idx) = find_or_new(&mut crashes, t, Some(code), &dump_path, true) {
                    let c = &mut crashes[idx];
                    c.sources.push("事件 1001（BugCheck）".to_string());
                    if c.bugcheck.is_none() { c.bugcheck = Some(code); }
                    if c.args.is_empty() && !args.is_empty() { c.args = args.clone(); }
                    if c.dump_path.is_empty() && !dump_path.is_empty() { c.dump_path = dump_path.clone(); }
                    if c.time == 0 { c.time = t; }
                    c.event_msgs.push(e.clone());
                }
            }
            41 => {
                // BugcheckCode 在事件 XML 里是十进制；旧实现一律按十六进制解析会得到错误码。
                let code: Option<u32> = e.props.first()
                    .and_then(|s| parse_evt_u32(s))
                    .filter(|c| *c != 0);
                let args: Vec<u64> = e.props.iter().skip(1).take(4)
                    .filter_map(|s| parse_evt_u64_raw(s))
                    .collect();
                let allow_new = code.is_some();
                match find_or_new(&mut crashes, t, code, "", allow_new) {
                    Some(idx) => {
                        let c = &mut crashes[idx];
                        c.sources.push("事件 41（Kernel-Power）".to_string());
                        if c.bugcheck.is_none() && code.is_some() { c.bugcheck = code; c.args = args.clone(); }
                        c.event_msgs.push(e.clone());
                    }
                    None => {
                        let mut c = CrashAgg::default();
                        c.time = t;
                        c.sources.push("事件 41（Kernel-Power 非正常关机）".to_string());
                        c.note = "Kernel-Power 41 且 BugCheck 码为 0：系统是被强制断电/长按电源键/直接掉电结束的，没有留下蓝屏信息".to_string();
                        c.event_msgs.push(e.clone());
                        crashes.push(c);
                    }
                }
            }
            6008 => {
                let idx = match find_or_new(&mut crashes, t, None, "", false) {
                    Some(i) => i,
                    None => { crashes.push(CrashAgg { time: t, ..Default::default() }); crashes.len() - 1 }
                };
                crashes[idx].sources.push("事件 6008（意外关机）".to_string());
                crashes[idx].event_msgs.push(e.clone());
            }
            _ => {}
        }
    }
    crashes.retain(|c| c.bugcheck.is_some() || !c.sources.is_empty());
    crashes.sort_by_key(|c| -c.time);

    // 2. 逐条 CrashRecord
    let mut crash_records: Vec<CrashRecord> = Vec::with_capacity(crashes.len());
    for c in &crashes {
        let info = c.bugcheck.and_then(bugcheck::lookup);
        let (causes, fixes) = match c.bugcheck {
            Some(_bc) if info.is_some() => (
                info.unwrap().causes.iter().map(|s| s.to_string()).collect(),
                info.unwrap().fixes.iter().map(|s| s.to_string()).collect(),
            ),
            Some(b) => bugcheck::generic_causes(b),
            None => (vec![], vec![]),
        };
        let explains = build_explains(c.bugcheck, &c.args);
        let mut related: Vec<EventRecord> = Vec::new();
        if c.time > 0 {
            for ev in events.iter() {
                if ev.group == "crash" && ev.id == 1001 { continue; }
                if let Some(et) = parse_ts(&ev.t) {
                    if (et - c.time).abs() <= T_WINDOW_SEC { related.push(ev.clone()); }
                }
            }
        }
        if related.len() > 12 { related.truncate(12); }
        let dump_ok = c.dump_idx.map(|i| dumps[i].ok).unwrap_or(false);
        let mut conf: f64 = 0.0;
        if dump_ok { conf += 0.4; }
        if c.bugcheck.is_some() && info.is_some() { conf += 0.2; }
        if !c.dump_path.is_empty() && !dump_ok { conf += 0.1; }
        if !related.is_empty() { conf += 0.05; }
        if !c.event_msgs.is_empty() { conf += 0.1; }
        conf = conf.min(1.0_f64);

        crash_records.push(CrashRecord {
            time: if c.time > 0 { fmt_ts(c.time) } else { String::new() },
            bugcheck: c.bugcheck,
            code_hex: c.bugcheck.map(bugcheck::format_code).unwrap_or_default(),
            name: bugcheck::name_of(c.bugcheck),
            bugcheck_cn: info.map(|i| i.cn.to_string()).unwrap_or_default(),
            // 详情表未收录时按官方名称关键词推断大类，避免分类列空白
            category: c.bugcheck.map(bugcheck::category_of).unwrap_or("").to_string(),
            severity: info.map(|i| i.severity.to_string()).unwrap_or_else(|| {
                // 官方名称表命中但无详情：按归因大类给一个保守的等级
                match c.bugcheck.map(bugcheck::category_of) {
                    Some(bugcheck::CAT_MEMORY) | Some(bugcheck::CAT_BOOT) => "high".to_string(),
                    Some(bugcheck::CAT_GRAPHICS) | Some(bugcheck::CAT_STORAGE) => "medium".to_string(),
                    Some(_) => "medium".to_string(),
                    None => "medium".to_string(),
                }
            }),
            args: c.args.clone(),
            explains, causes, fixes,
            dump_path: c.dump_path.clone(),
            dump_format: c.dump_idx.map(|i| dumps[i].format.clone()).unwrap_or_default(),
            dump_size: c.dump_idx.map(|i| dumps[i].file_size).unwrap_or(0),
            dump_ok,
            sources: c.sources.clone(),
            related,
            is_live: c.is_live,
            note: c.note.clone(),
            confidence: (conf * 100.0).round() / 100.0,
        });
    }

    // 3. findings
    let mut findings: Vec<Finding> = Vec::new();
    let ev_whea: Vec<&EventRecord> = events.iter().filter(|e| e.group == "whea").collect();
    let ev_disk: Vec<&EventRecord> = events.iter().filter(|e| e.group == "disk").collect();
    let has_crash = !crash_records.is_empty();
    // 「有蓝屏」必须是真正解析到 BugCheck 码的崩溃；只有 41/6008、LiveKernel
    // 或 WHEA/磁盘事件时，不能把环境事件当成蓝屏原因，否则无蓝屏的机器也会被
    // 提示成硬件故障。
    let has_bsod = crash_records
        .iter()
        .any(|c| c.bugcheck.is_some() && !c.is_live);

    for c in crash_records.iter() {
        if c.bugcheck.is_none() {
            findings.push(Finding {
                severity: "high".into(),
                title: "发生了一次非正常关机（无蓝屏记录）".into(),
                detail: if !c.note.is_empty() { c.note.clone() } else {
                    "系统在未正常关机的情况下终止，且没有生成可分析的转储文件。".into()
                },
                evidence: if !c.sources.is_empty() {
                    vec!["事件 41（Kernel-Power）: 非正常关机".into(),
                         "事件 6008（EventLog）: 上次关机是意外的".into()]
                } else { vec![] },
                actions: vec![
                    "检查电源（特别是电源功率与显卡供电线）、内存与散热".into(),
                    "如果是直接断电（跳闸/停电/插排问题），先排除供电链路".into(),
                    "开启小内存转储后复现，才能进一步定位".into(),
                ],
                confidence: 0.75, tag: "非正常关机".into(),
            });
        } else if c.is_live {
            findings.push(Finding {
                severity: "medium".into(),
                title: "检测到 LiveKernel 硬件/驱动异常报告".into(),
                detail: "LiveKernelReports 下存在转储文件，说明系统或某个设备出现过「未导致重启的异常」（如显卡复位、设备挂起）。这类记录常见于硬件不稳定或驱动缺陷的早期阶段。".into(),
                evidence: {
                    let mut v = vec![format!("转储文件：{}", if c.dump_path.is_empty() { "-" } else { &c.dump_path })];
                    if c.bugcheck.is_some() { v.push(format!("BugCheck 码：{}（{}）", c.code_hex, c.name)); }
                    v
                },
                actions: vec!["按显卡/存储驱动的方向更新驱动".into(), "检查散热与供电".into()],
                confidence: 0.6, tag: "LiveKernel".into(),
            });
        }
    }

    let hw_crashes: Vec<&CrashRecord> = crash_records.iter()
        .filter(|c| c.bugcheck.map_or(false, |b| HARDWARE_CODES.contains(&bugcheck::base_code(b))))
        .collect();
    if !hw_crashes.is_empty() {
        let mut codes: Vec<String> = hw_crashes.iter().map(|c| c.code_hex.clone()).collect();
        codes.sort(); codes.dedup();
        findings.push(Finding {
            severity: "critical".into(),
            title: format!("存在硬件级错误迹象（{}）", codes.join("、")),
            detail: "这些 BugCheck 码（0x124 WHEA、0x101 CLOCK_WATCHDOG、0x12B 内存位翻转、0x9C 机器检查、0x2E 数据总线错误）由 CPU/芯片组直接上报硬件异常，指向供电、超频、内存或主板，而不是软件问题。".into(),
            evidence: vec![format!("涉及 {} 次崩溃：{}", hw_crashes.len(),
                hw_crashes.iter().take(5).map(|c| c.time.clone()).collect::<Vec<_>>().join("、"))],
            actions: vec![
                "BIOS 恢复默认设置：关闭 XMP/EXPO/DOCP 与一切自动超频".into(),
                "用 MemTest86 逐条内存单插测试（至少两个完整循环）".into(),
                "清理散热器并确认 CPU 温度与供电正常，必要时更换电源测试".into(),
                "更新主板 BIOS 与芯片组驱动".into(),
            ],
            confidence: 0.85, tag: "硬件".into(),
        });
    }
    if has_bsod && !ev_whea.is_empty() {
        let ev: Vec<String> = ev_whea.iter().take(6).map(|e| format!("{} ｜ {}", e.t, truncate(&e.msg, 150))).collect();
        findings.push(Finding {
            severity: "critical".into(),
            title: format!("事件日志中存在 WHEA 硬件错误（{} 条）", ev_whea.len()),
            detail: WHEA_TEXT.into(),
            evidence: ev,
            actions: vec![
                "按事件详情里的错误来源定位部件（如 PCIe 设备、内存通道、CPU 核心）".into(),
                "关闭超频并恢复 BIOS 默认值后复现验证".into(),
                "拔插内存条与 PCIe 设备、更换供电线，逐件替换测试".into(),
            ],
            confidence: 0.9, tag: "硬件".into(),
        });
    }

    let disk_hits: Vec<(&EventRecord, &'static str)> = {
        const DISK_NAMES: &[(u32, &str)] = &[
            (7, "磁盘设备出现坏块"), (9, "磁盘设备 I/O 重试"), (11, "磁盘控制器错误"),
            (51, "磁盘分页操作出错"), (52, "磁盘扇区 SMART 预警"), (55, "NTFS 文件系统损坏"),
            (98, "卷需要修复"), (129, "存储设备复位"), (153, "磁盘 I/O 重试"),
            (157, "磁盘意外移除"), (219, "驱动加载失败"),
        ];
        let mut hits = Vec::new();
        for e in ev_disk.iter() {
            // 仅接受真存储堆栈的 provider；否则 FltMgr/ACE-CORE 这类反作弊过滤驱动
            // 上报的同 ID 事件会被误判为磁盘错误。
            if !is_disk_provider(&e.prov) { continue; }
            // NTFS 98 有两种语义：一种是真的「卷需要修复」，另一种是每次开机都会写的
            // 「卷运行状况良好」。后者是健康巡检，计入磁盘错误会刷出几十条误报。
            if e.id == 98 && is_healthy_volume_msg(&e.msg) { continue; }
            if let Some((_, name)) = DISK_NAMES.iter().find(|(id, _)| *id == e.id) {
                hits.push((*e, *name));
            }
        }
        hits
    };
    if has_bsod && disk_hits.len() >= 3 {
        let ev: Vec<String> = disk_hits.iter().take(8)
            .map(|(e, name)| format!("{} ｜ {}（事件 {}）｜ {}", e.t, name, e.id, truncate(&e.msg, 130)))
            .collect();
        findings.push(Finding {
            severity: "critical".into(),
            title: format!("检测到磁盘/存储层错误（{} 条）", disk_hits.len()),
            detail: "磁盘类错误（坏块、重试、复位、文件系统损坏）是蓝屏的高频真实原因：当系统需要从磁盘读入内核数据或换页内容而失败时，就会以 0x7A / 0x24 / 0xF4 / 0x133 等蓝屏表现。".into(),
            evidence: ev,
            actions: vec![
                "立即备份重要数据（磁盘故障往往先报错后彻底失效）".into(),
                "用厂商工具读 SMART，重点关注「重映射扇区」「待映射扇区」「通电时间」".into(),
                "更换 SATA/NVMe 数据线与供电，改插主板原生接口；SSD 升级固件".into(),
                "执行 chkdsk C: /f /r 修复文件系统与坏道".into(),
            ],
            confidence: 0.8, tag: "存储".into(),
        });
    }

    // 本机没有任何蓝屏记录时，转储配置/页面文件这类「预防建议」不是归因结论，
    // 放进「原因分析」只会制造噪声（用户会以为系统真的出了故障）。
    let enabled = config.crash_dump_enabled;
    if has_crash && enabled == 0 {
        findings.push(Finding {
            severity: "medium".into(),
            title: "系统当前禁用了崩溃转储".into(),
            detail: "「写入调试信息」被设为「无」，因此蓝屏时不会生成任何 dump 文件，下次蓝屏将无法做深度分析。".into(),
            evidence: vec!["CrashDumpEnabled = 0".into()],
            actions: vec![
                "开启「小内存转储」：系统属性 → 高级 → 启动和故障恢复 → 设置".into(),
                "设置立即生效，下次蓝屏时会写入 C:\\Windows\\Minidump".into(),
            ],
            confidence: 0.95, tag: "配置".into(),
        });
    }
    if has_crash && enabled == 3 && !dumps.iter().any(|d| d.path.to_lowercase().contains("minidump")) {
        findings.push(Finding {
            severity: "low".into(),
            title: "已配置小内存转储，但尚未找到 dump 文件".into(),
            detail: "配置没有问题；可能是最近没有发生蓝屏，或蓝屏时未能成功写出转储。".into(),
            evidence: vec!["CrashDumpEnabled = 3（小内存转储）".into()],
            actions: vec!["确认 C:\\Windows\\Minidump 目录存在且当前用户可读".into()],
            confidence: 0.6, tag: "配置".into(),
        });
    }

    let mem_gb = system.mem_total as f64 / 1024.0 / 1024.0 / 1024.0;
    if has_crash && (enabled == 1 || enabled == 7) && config.pagefile_alloc_mb > 0 && mem_gb > 0.0 {
        let alloc_mb = config.pagefile_alloc_mb as f64;
        let need = mem_gb * 1024.0 * if enabled == 1 { 0.5 } else { 1.0 };
        if alloc_mb < need * 0.6 {
            findings.push(Finding {
                severity: "medium".into(),
                title: "页面文件偏小，可能导致转储写入失败".into(),
                detail: format!("内核转储需要把内存内容写入页面文件。当前页面文件约 {:.1} GB，而物理内存为 {:.1} GB，蓝屏时可能因空间不足而写不出完整转储。",
                    alloc_mb / 1024.0, mem_gb),
                evidence: vec![
                    format!("页面文件已分配 {} MB，物理内存 {:.1} GB", config.pagefile_alloc_mb, mem_gb),
                    format!("当前转储类型：{}", if enabled == 1 { "完整内存转储" } else { "自动内存转储" }),
                ],
                actions: vec![
                    "把页面文件改为「系统管理的大小」，或至少设置为物理内存的 1 倍".into(),
                    "也可以先把转储类型改为「小内存转储」以保证每次都能留下记录".into(),
                ],
                confidence: 0.7, tag: "配置".into(),
            });
        }
    }

    let mut by_code: std::collections::HashMap<u32, Vec<&CrashRecord>> = Default::default();
    for c in &crash_records { if let Some(b) = c.bugcheck { by_code.entry(b).or_default().push(c); } }
    for (code, cs) in by_code.iter() {
        if cs.len() >= 3 {
            let info = bugcheck::lookup(*code);
            let mut ev = vec![
                format!("{}（{}）", bugcheck::format_code(*code), info.map(|i| i.name).unwrap_or("未收录")),
                format!("出现次数：{} 次", cs.len()),
            ];
            for c in cs.iter().take(6) { ev.push(format!("时间：{}", c.time)); }
            findings.push(Finding {
                severity: "high".into(),
                title: format!("同一错误码反复出现：{}（{} 次）", bugcheck::format_code(*code), cs.len()),
                detail: "同一 BugCheck 码重复出现，说明问题具有稳定的可复现路径（固定的驱动缺陷、固定的硬件故障或固定的配置问题），而不是偶发的内存位翻转。".into(),
                evidence: ev,
                actions: vec![
                    "按该错误码对应的排查路径逐项执行（见崩溃记录页）".into(),
                    "重点测试单一变量：一次只改一个驱动/设置，再观察是否复现".into(),
                ],
                confidence: 0.8, tag: "模式".into(),
            });
        }
    }

    // 没有解析到任何蓝屏记录时，「原因分析」保持为空：页头/概览已经明确
    // 说明“未发现蓝屏”，再塞一条结论卡片只会让用户误以为系统有问题。

    findings.sort_by(|a, b| sev_order(&a.severity).cmp(&sev_order(&b.severity))
        .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)));

    // 4. Summary
    let bugs: Vec<&CrashRecord> = crash_records.iter().filter(|c| c.bugcheck.is_some() && !c.is_live).collect();
    let live: Vec<&CrashRecord> = crash_records.iter().filter(|c| c.is_live).collect();
    let (status, headline, sub) = if !bugs.is_empty() {
        let last = &bugs[0];
        ("alert".to_string(),
         format!("发现 {} 次蓝屏记录", bugs.len()),
         format!("最近一次：{} ｜ {}（{}）", last.time, last.code_hex, last.name))
    } else if !live.is_empty() || !crash_records.is_empty() {
        ("warn".to_string(),
         "发现异常关机/硬件异常记录".to_string(),
         "未解析到蓝屏错误码，但存在非正常关机或 LiveKernel 记录".to_string())
    } else {
        ("ok".to_string(), "未发现蓝屏记录".to_string(), "系统日志与转储目录中都没有蓝屏证据".to_string())
    };
    if has_crash && !errors.is_empty() {
        findings.push(Finding {
            severity: "info".into(),
            title: "部分证据未采集成功".into(),
            detail: "以下子任务失败（通常不影响主要结论，如需完整记录请以管理员身份运行）：".into(),
            evidence: errors.iter().take(10).cloned().collect(),
            actions: vec!["以管理员身份重新启动 NexBox 可看到更完整的事件日志".into()],
            confidence: 0.4, tag: "采集".into(),
        });
    }

    let summary = Summary {
        status, headline, sub,
        count_crashes: bugs.len(),
        count_whea_events: ev_whea.len(),
        count_suspects: crash_records.iter().filter(|c| c.dump_ok).count(),
        count_dumps_ok: dumps.iter().filter(|d| d.ok).count(),
    };

    (crash_records, findings, summary)
}

fn find_or_new(crashes: &mut Vec<CrashAgg>, t: i64, code: Option<u32>, dump_path: &str, allow_new: bool) -> Option<usize> {
    if t > 0 {
        for (i, c) in crashes.iter().enumerate() {
            if c.time > 0 && (c.time - t).abs() <= T_WINDOW_SEC { return Some(i); }
        }
        if !dump_path.is_empty() {
            for (i, c) in crashes.iter().enumerate() {
                if !c.dump_path.is_empty() && c.dump_path.eq_ignore_ascii_case(dump_path) { return Some(i); }
            }
        }
    }
    if !allow_new { return None; }
    crashes.push(CrashAgg { time: t, bugcheck: code, ..Default::default() });
    Some(crashes.len() - 1)
}

fn build_explains(code: Option<u32>, args: &[u64]) -> Vec<ArgExplain> {
    let meta = code.and_then(bugcheck::lookup);
    let names: [&str; 4] = if let Some(m) = meta { m.params } else { bugcheck::GENERIC_PARAMS };
    let mut out = Vec::with_capacity(4);
    for i in 0..4 {
        let val = args.get(i).copied().unwrap_or(0);
        let mut note: Vec<String> = Vec::new();
        if val >= KERNEL_ADDR_MIN { note.push("内核地址".to_string()); }
        else if val < 0x10000 { note.push("很小的数值（多为状态码 / IRQL / 索引，不是内存地址）".to_string()); }
        else if val > 0xFFFF_FFFF { note.push("高于用户态地址范围的异常值".to_string()); }
        else if val > 0xFFFF { note.push("用户态地址或数值".to_string()); }
        out.push(ArgExplain {
            value: val,
            hex: format!("0x{:016X}", val),
            meaning: names[i].to_string(),
            note: note.join("；"),
        });
    }
    out
}

fn sev_order(s: &str) -> u8 {
    match s { "critical" => 0, "high" => 1, "medium" => 2, "low" => 3, _ => 4 }
}

/// 判断事件的 provider 是否属于真正的存储堆栈（白名单），避免 FltMgr/FilterManager
/// 之类的过滤驱动以同名 Event ID 污染磁盘归因。匹配时忽略大小写与常见前缀。
fn is_disk_provider(prov: &str) -> bool {
    let p = prov.to_ascii_lowercase();
    const ALLOW: &[&str] = &[
        "disk", "ntfs", "microsoft-windows-ntfs", "volmgr", "partmgr",
        "storahci", "stornvme", "ia stor", "iastor", "iafst", "amdahcix", "rvsdx",
        "microsoft-windows-disk", "microsoft-windows-diskdiagnostic",
        "microsoft-windows-diskstress", "microsoft-windows-storport",
        "microsoft-windows-storage-spaces", "microsoft-windows-storage-tiers",
        "microsoft-windows-hidusb", "nvme", "nvmexpressdriver",
        "storufs", "ucx01000", "usbstor", "reliabilityanalysis",
        "volsnap", "vhdstore", "clfs", "clusdisk", "spaceport",
    ];
    for a in ALLOW {
        if p == *a || p.ends_with(&format!("-{}", a)) || p.contains(&format!(" {}", a)) {
            return true;
        }
    }
    // 兵分一道：任何包含 "disk"/"ntfs"/"volume"/"storage" 的 provider 也归入白名单
    // （注意用 "storage" 而不是 "stor"，避免误匹到 Microsoft-Windows-Store）
    p.contains("disk") || p.contains("ntfs") || p.contains("volume") || p.contains("storage")
}

/// 判断一条 NTFS 98 事件是不是「卷运行状况良好」这种健康巡检消息。
/// 同一事件 ID 也会用于「卷需要修复」的真实故障，所以只能看消息文本。
fn is_healthy_volume_msg(msg: &str) -> bool {
    const HEALTHY: &[&str] = &[
        "运行状况良好", "无需执行任何操作", "无需采取任何操作",
        "is healthy", "no action is required", "no further action",
    ];
    let m = msg.to_ascii_lowercase();
    HEALTHY.iter().any(|k| {
        let k = k.to_ascii_lowercase();
        m.contains(&k)
    })
}

fn truncate(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n { format!("{}…", t) } else { t }
}

// ============================================================================
// 打开转储目录 / 定位单文件
// ============================================================================

#[tauri::command]
pub fn bsod_open_dump_dir() -> Result<String, String> {
    let dir = dump_dir();
    if !dir.exists() { return Err("转储目录不存在（C:\\Windows\\Minidump）".to_string()); }
    Command::new("explorer")
        .arg(&dir)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("打开资源管理器失败: {}", e))?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn bsod_reveal_dump(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() { return Err("文件不存在".to_string()); }
    Command::new("explorer")
        .arg(format!("/select,{}", path))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("打开资源管理器失败: {}", e))?;
    Ok(())
}

fn dump_dir() -> std::path::PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    std::path::Path::new(&root).join("Minidump")
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_code_preserves_upper_bits() {
        assert_eq!(bugcheck::format_code(0x7E), "0x0000007E");
        assert_eq!(bugcheck::format_code(0x1000_007E), "0x1000007E");
    }

    #[test]
    fn lookup_handles_dump_variant() {
        // 0x1000007E 归一化后应命中 SYSTEM_THREAD_EXCEPTION_NOT_HANDLED
        assert_eq!(bugcheck::lookup(0x1000_007E).unwrap().name, "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED");
    }

    #[test]
    fn parse_bugcheck_msg_basic() {
        let msg = "计算机已经从检测错误后重新启动。检测错误: 0x0000001a (0x0000000000041790, 0x000000141bb77b60, 0xfffff806792ac240, 0x000076bb00000000)。已将转储的数据保存在: C:\\Windows\\Minidump\\091826-15031-01.dmp。";
        let (code, args, dump) = parse_bugcheck_msg(msg);
        assert_eq!(code, Some(0x1A));
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], 0x41790);
        assert!(dump.ends_with(".dmp"));
    }

    #[test]
    fn sev_order_correct() {
        assert!(sev_order("critical") < sev_order("high"));
        assert!(sev_order("high") < sev_order("medium"));
    }

    #[test]
    fn generic_causes_for_unknown() {
        let (c, f) = bugcheck::generic_causes(0xDEAD_BEEF);
        assert!(!c.is_empty());
        assert!(!f.is_empty());
    }

    #[test]
    fn truncate_utf8_safe() {
        let s = "中文测试字符串hello";
        assert_eq!(truncate(s, 2), "中文…");
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn healthy_volume_msg_detected() {
        // 每次开机都会写的健康巡检消息，不能算磁盘故障
        assert!(is_healthy_volume_msg(
            "卷 C: (\\Device\\HarddiskVolume3) 运行状况良好。无需执行任何操作。"));
        assert!(is_healthy_volume_msg(
            "Volume C: (\\Device\\HarddiskVolume3) is healthy. No action is required."));
        // 真正需要修复的消息仍然命中
        assert!(!is_healthy_volume_msg(
            "卷 C: (\\Device\\HarddiskVolume3) 需要修复。请运行 chkdsk /f。"));
        assert!(!is_healthy_volume_msg(""));
    }

    #[test]
    fn is_disk_provider_filters_fltmgr() {
        // 真磁盘/存储 provider 命中
        assert!(is_disk_provider("disk"));
        assert!(is_disk_provider("Ntfs"));
        assert!(is_disk_provider("Microsoft-Windows-Ntfs"));
        assert!(is_disk_provider("volmgr"));
        assert!(is_disk_provider("storahci"));
        assert!(is_disk_provider("stornvme"));
        assert!(is_disk_provider("Microsoft-Windows-DiskDiagnostic"));
        // 反作弊过滤驱动 FltMgr 不命中（修复前会误报 300 条）
        assert!(!is_disk_provider("Microsoft-Windows-Filter Manager"));
        assert!(!is_disk_provider("FltMgr"));
        assert!(!is_disk_provider("ACE-CORE202706"));
        assert!(!is_disk_provider("Microsoft-Windows-Kernel-Power"));
        assert!(!is_disk_provider("Service Control Manager"));
        assert!(!is_disk_provider("Microsoft-Windows-Store"));
    }

    fn evt_xml(provider: &str, id: u32, level: u32, data: &[(&str, &str)]) -> String {
        let data_xml = data
            .iter()
            .map(|(n, v)| format!("<Data Name='{}'>{}</Data>", n, v))
            .collect::<String>();
        format!(
            "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System>\
             <Provider Name='{}'/><EventID>{}</EventID><Level>{}</Level>\
             <TimeCreated SystemTime='2026-11-14T02:25:34.8990917Z'/><Channel>System</Channel>\
             </System><EventData>{}</EventData></Event>",
            provider, id, level, data_xml
        )
    }

    #[test]
    fn evt_numbers_decimal_and_hex() {
        // BugcheckCode 是十进制
        assert_eq!(parse_evt_u32("159"), Some(0x9F));
        assert_eq!(parse_evt_u32("26"), Some(0x1A));
        // BugcheckParameterN 是带 0x 的十六进制
        assert_eq!(parse_evt_u64_raw("0x1000007E"), Some(0x1000_007E));
        assert_eq!(parse_evt_u64_raw("0x41790"), Some(0x41790));
        assert_eq!(parse_evt_u32(""), None);
    }

    #[test]
    fn xml_bugcheck_1001_uses_structured_fields() {
        let xml = evt_xml(
            "Microsoft-Windows-WER-SystemErrorReporting",
            1001,
            2,
            &[
                ("BugcheckCode", "26"),
                ("BugcheckParameter1", "0x41790"),
                ("BugcheckParameter2", "0x141bb77b60"),
                ("BugcheckParameter3", "0xfffff806792ac240"),
                ("BugcheckParameter4", "0x76bb00000000"),
            ],
        );
        let events = parse_events_xml(&xml, "crash");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, 1001);
        assert_eq!(events[0].level, "error");
        let (code, args) = parse_bugcheck_fields(&events[0].props).unwrap();
        assert_eq!(code, 0x1A);
        assert_eq!(args[0], 0x41790);
        assert_eq!(args.len(), 4);
        assert!(events[0].msg.contains("0x0000001A"));
    }

    #[test]
    fn xml_kernel_power_41_decimal_code() {
        let xml = evt_xml(
            "Microsoft-Windows-Kernel-Power",
            41,
            1,
            &[
                ("BugcheckCode", "159"),
                ("BugcheckParameter1", "0x0"),
                ("BugcheckParameter2", "0x0"),
                ("BugcheckParameter3", "0x0"),
                ("BugcheckParameter4", "0x0"),
            ],
        );
        let events = parse_events_xml(&xml, "crash");
        assert_eq!(events.len(), 1);
        let (code, _) = parse_bugcheck_fields(&events[0].props).unwrap();
        assert_eq!(code, 0x9F);
        assert!(events[0].msg.contains("0x0000009F"));
    }

    #[test]
    fn xml_ignores_non_bugcheck_1001() {
        // 没有 BugcheckCode 的 1001（其它 provider 复用事件 ID）不能再生成崩溃记录
        let xml = evt_xml("Some-Provider", 1001, 4, &[("foo", "bar")]);
        assert!(parse_events_xml(&xml, "crash").is_empty());
    }

    #[test]
    fn ntfs98_health_from_xml() {
        // 健康巡检在采集阶段就被丢弃（与旧 PS 脚本一致），不进入事件列表
        let healthy = evt_xml(
            "Microsoft-Windows-Ntfs",
            98,
            4,
            &[
                ("DriveName", "C:"),
                ("DeviceName", "\\Device\\HarddiskVolume3"),
                ("CorruptionActionState", "0"),
            ],
        );
        assert!(parse_events_xml(&healthy, "disk").is_empty());

        let broken = evt_xml(
            "Microsoft-Windows-Ntfs",
            98,
            2,
            &[
                ("DriveName", "C:"),
                ("DeviceName", "\\Device\\HarddiskVolume3"),
                ("CorruptionActionState", "1"),
            ],
        );
        let events = parse_events_xml(&broken, "disk");
        assert_eq!(events.len(), 1);
        assert!(!is_healthy_volume_msg(&events[0].msg));
    }

    #[test]
    fn disk_group_drops_noise_providers() {
        // Kernel-Processor-Power 复用事件 ID 55，不能进入磁盘事件列表
        let noise = evt_xml(
            "Microsoft-Windows-Kernel-Processor-Power",
            55,
            4,
            &[("Group", "0"), ("Number", "15")],
        );
        assert!(parse_events_xml(&noise, "disk").is_empty());

        let real = evt_xml(
            "Microsoft-Windows-Ntfs",
            55,
            2,
            &[("DriveName", "C:"), ("DeviceName", "\\Device\\HarddiskVolume3")],
        );
        assert_eq!(parse_events_xml(&real, "disk").len(), 1);
    }

    #[test]
    fn no_bsod_suppresses_hardware_and_disk_findings() {
        let event = |id: u32, prov: &str, group: &str| EventRecord {
            t: "2026-11-14 10:00:00".to_string(),
            id,
            prov: prov.to_string(),
            level: "error".to_string(),
            task: String::new(),
            msg: format!("event {}", id),
            props: Vec::new(),
            group: group.to_string(),
        };
        let events = vec![
            event(17, "Microsoft-Windows-WHEA-Logger", "whea"),
            event(7, "disk", "disk"),
            event(129, "Microsoft-Windows-StorPort", "disk"),
            event(153, "disk", "disk"),
        ];
        let (crashes, findings, summary) =
            analyze(&SystemInfo::default(), &DumpConfig::default(), &[], &events, &[]);

        assert!(crashes.is_empty());
        assert_eq!(summary.status, "ok");
        // 没有解析到 BugCheck 时，WHEA/磁盘事件只能留在「事件日志」页，
        // 不能作为「原因分析」里的硬件/存储故障结论；干净机器的结论应为空。
        assert!(findings.is_empty());
    }

    #[test]
    fn mdmp_processor_count_reads_byte_at_plus6() {
        // 构造最小 MDMP：目录 1 项（SystemInfo），SystemInfo 在 0x60，
        // NumberOfProcessors 是 +6 处的 UCHAR；旧实现读 +4 会拿到 ProcessorRevision。
        let mut buf = vec![0u8; 0x100];
        buf[8..12].copy_from_slice(&1u32.to_le_bytes());
        buf[12..16].copy_from_slice(&0x20u32.to_le_bytes());
        buf[0x20..0x24].copy_from_slice(&7u32.to_le_bytes());
        buf[0x24..0x28].copy_from_slice(&0x38u32.to_le_bytes());
        buf[0x28..0x2C].copy_from_slice(&0x60u32.to_le_bytes());
        buf[0x60..0x64].copy_from_slice(&9u32.to_le_bytes()); // AMD64
        buf[0x64..0x66].copy_from_slice(&0x1234u16.to_le_bytes()); // ProcessorRevision
        buf[0x66] = 12; // NumberOfProcessors
        buf[0x68..0x6C].copy_from_slice(&10u32.to_le_bytes());
        buf[0x6C..0x70].copy_from_slice(&0u32.to_le_bytes());
        buf[0x70..0x74].copy_from_slice(&22631u32.to_le_bytes());

        let mut info = DumpInfo {
            path: String::new(),
            file_size: buf.len() as u64,
            mtime: String::new(),
            format: "MDMP".to_string(),
            bugcheck: None,
            bugcheck_hex: String::new(),
            args: Vec::new(),
            exception_address: None,
            arch: String::new(),
            os_build: String::new(),
            processor_count: 0,
            ok: false,
            error: String::new(),
            is_live: false,
        };
        parse_mdmp(&mut std::io::Cursor::new(buf), &mut info).unwrap();
        assert_eq!(info.processor_count, 12);
        assert_eq!(info.arch, "AMD64");
        assert_eq!(info.os_build, "10.0.22631");
    }
}
