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
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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
// PowerShell 采集脚本
// ============================================================================

const PS_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
$out = $env:BSOD_OUT

function Trim-Msg($m, $n) {
  if ($null -eq $m) { return '' }
  $s = [string]$m
  $s = $s -replace "`r`n", ' ' -replace "`n", ' '
  if ($s.Length -gt $n) { return $s.Substring(0, $n) }
  return $s
}

function Props($e) {
  $vals = @()
  foreach ($p in $e.Properties) {
    try { $v = $p.Value } catch { $v = $null }
    if ($null -eq $v) { $vals += ''; continue }
    if ($v -is [byte[]]) { $vals += ('0x' + (($v | ForEach-Object { $_.ToString('X2') }) -join '')) }
    else { $vals += [string]$v }
  }
  return ,$vals
}

function Ev($filter, $max, $group) {
  $res = @()
  try { $evs = Get-WinEvent -FilterHashtable $filter -MaxEvents $max -ErrorAction Stop } catch { return @() }
  foreach ($e in $evs) {
    $res += @{
      t = $e.TimeCreated.ToString('yyyy-MM-dd HH:mm:ss')
      id = $e.Id
      prov = $e.ProviderName
      level = $e.LevelDisplayName
      task = $e.TaskDisplayName
      msg = (Trim-Msg $e.Message 700)
      props = (Props $e)
      group = $group
    }
  }
  return ,$res
}

$R = @{}
$R['errors'] = @()

try {
  $os = Get-CimInstance Win32_OperatingSystem
  $R['os'] = @{
    caption = $os.Caption; version = $os.Version; build = $os.BuildNumber
    arch = $os.OSArchitecture
    install_date = $(if ($os.InstallDate) { $os.InstallDate.ToString('yyyy-MM-dd') } else { '' })
    last_boot = $(if ($os.LastBootUpTime) { $os.LastBootUpTime.ToString('yyyy-MM-dd HH:mm:ss') } else { '' })
    mem_total = [int64]$os.TotalVisibleMemorySize * 1024
    mem_free  = [int64]$os.FreePhysicalMemory * 1024
  }
} catch { $R['errors'] += "OS: $($_.Exception.Message)" }

try {
  $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
  $R['cpu'] = @{ name = $cpu.Name; cores = $cpu.NumberOfCores; threads = $cpu.NumberOfLogicalProcessors }
} catch { $R['errors'] += "CPU: $($_.Exception.Message)" }

try {
  $cs = Get-CimInstance Win32_ComputerSystem
  $R['machine'] = @{ vendor = $cs.Manufacturer; model = $cs.Model }
} catch { $R['errors'] += "CS: $($_.Exception.Message)" }

try {
  $cc = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\CrashControl'
  $R['crashcontrol'] = @{ enabled = [int]$cc.CrashDumpEnabled; dumpfile = [string]$cc.DumpFile }
} catch { $R['errors'] += "CrashControl: $($_.Exception.Message)" }

try {
  $pf = @(Get-CimInstance Win32_PageFileUsage | Select-Object -First 1)
  if ($pf -and $pf[0]) {
    $R['pagefile'] = @{ name = [string]$pf[0].Name; alloc = [int64]$pf[0].AllocatedBaseSize; current = [int64]$pf[0].CurrentUsage }
  } else { $R['pagefile'] = @{ name=''; alloc=0; current=0 } }
} catch { $R['errors'] += "PageFile: $($_.Exception.Message)"; $R['pagefile'] = @{ name=''; alloc=0; current=0 } }

$R['events'] = @()
$R['events'] += Ev @{ LogName = 'System'; Id = 1001,41,6008 } 200 'crash'
$R['events'] += Ev @{ LogName = 'Application'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; Id = 17,18,19 } 200 'whea'
$R['events'] += Ev @{ LogName = 'System'; Id = 7,11,51,55,98,129,153,157 } 300 'disk'

$dumps = @()
foreach ($dir in @("$env:SystemRoot\Minidump")) {
  if (Test-Path $dir) {
    Get-ChildItem -Path $dir -Filter '*.dmp' -ErrorAction SilentlyContinue | ForEach-Object {
      $dumps += @{ path = $_.FullName; size = $_.Length; mtime = $_.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') }
    }
  }
}
$fullDump = Join-Path $env:SystemRoot 'MEMORY.DMP'
if (Test-Path $fullDump) {
  $fi = Get-Item $fullDump
  $dumps += @{ path = $fi.FullName; size = $fi.Length; mtime = $fi.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') }
}
$liveDir = Join-Path $env:SystemRoot 'LiveKernelReports'
if (Test-Path $liveDir) {
  Get-ChildItem -Path $liveDir -Filter '*.dmp' -Recurse -ErrorAction SilentlyContinue | ForEach-Object {
    $dumps += @{ path = $_.FullName; size = $_.Length; mtime = $_.LastWriteTime.ToString('yyyy-MM-dd HH:mm:ss') }
  }
}
$R['dump_files'] = $dumps

$R | ConvertTo-Json -Depth 8 -Compress | Out-File -Encoding UTF8 -FilePath $out
exit 0
"#;

// ============================================================================
// PS 输出的中间结构
// ============================================================================

#[derive(Debug, Default, Deserialize)]
struct RawPs {
    #[serde(default)] os: Option<RawOs>,
    #[serde(default)] cpu: Option<RawCpu>,
    #[serde(default)] machine: Option<RawMachine>,
    #[serde(default)] crashcontrol: Option<RawCrashCtl>,
    #[serde(default)] pagefile: Option<RawPageFile>,
    #[serde(default)] events: Option<Vec<RawEvent>>,
    #[serde(default)] dump_files: Option<Vec<RawDumpFile>>,
    #[serde(default)] errors: Option<Vec<String>>,
}
#[derive(Debug, Default, Deserialize)]
struct RawOs { caption: Option<String>, version: Option<String>, build: Option<String>,
    arch: Option<String>, install_date: Option<String>, last_boot: Option<String>,
    mem_total: Option<u64>, mem_free: Option<u64> }
#[derive(Debug, Default, Deserialize)]
struct RawCpu { name: Option<String>, cores: Option<u32>, threads: Option<u32> }
#[derive(Debug, Default, Deserialize)]
struct RawMachine { vendor: Option<String>, model: Option<String> }
#[derive(Debug, Default, Deserialize)]
struct RawCrashCtl { enabled: Option<i64>, dumpfile: Option<String> }
#[derive(Debug, Default, Deserialize)]
struct RawPageFile { name: Option<String>, alloc: Option<u64>, current: Option<u64> }
#[derive(Debug, Default, Deserialize)]
struct RawEvent { t: Option<String>, id: Option<u32>, prov: Option<String>,
    level: Option<String>, task: Option<String>, msg: Option<String>,
    props: Option<Vec<String>>, group: Option<String> }
#[derive(Debug, Default, Deserialize)]
struct RawDumpFile { path: Option<String>, size: Option<u64>, mtime: Option<String> }

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

    let raw: RawPs = match run_ps_collect() {
        Ok(v) => v,
        Err(e) => { errors.push(format!("PowerShell 采集失败: {}", e)); RawPs::default() }
    };
    if let Some(es) = raw.errors.as_ref() {
        for e in es { errors.push(format!("PS 子任务: {}", e)); }
    }

    let mut system = SystemInfo::default();
    if let Some(os) = raw.os.as_ref() {
        system.caption = os.caption.clone().unwrap_or_default();
        system.version = os.version.clone().unwrap_or_default();
        system.build = os.build.clone().unwrap_or_default();
        system.arch = os.arch.clone().unwrap_or_default();
        system.install_date = os.install_date.clone().unwrap_or_default();
        system.last_boot = os.last_boot.clone().unwrap_or_default();
        system.mem_total = os.mem_total.unwrap_or(0);
        system.mem_free = os.mem_free.unwrap_or(0);
    }
    if let Some(cpu) = raw.cpu.as_ref() {
        system.cpu_name = cpu.name.clone().unwrap_or_default().trim().to_string();
        system.cpu_cores = cpu.cores.unwrap_or(0);
        system.cpu_threads = cpu.threads.unwrap_or(0);
    }
    if let Some(mc) = raw.machine.as_ref() {
        system.machine_vendor = mc.vendor.clone().unwrap_or_default().trim().to_string();
        system.machine_model = mc.model.clone().unwrap_or_default().trim().to_string();
    }

    let mut config = DumpConfig::default();
    if let Some(cc) = raw.crashcontrol.as_ref() {
        config.crash_dump_enabled = cc.enabled.unwrap_or(-1);
        config.dump_file = cc.dumpfile.clone().unwrap_or_default();
    }
    if let Some(pf) = raw.pagefile.as_ref() {
        config.pagefile_name = pf.name.clone().unwrap_or_default();
        config.pagefile_alloc_mb = pf.alloc.unwrap_or(0);
        config.pagefile_current_mb = pf.current.unwrap_or(0);
    }

    let events: Vec<EventRecord> = raw.events.unwrap_or_default().into_iter().map(|e| {
        EventRecord {
            t: e.t.unwrap_or_default(),
            id: e.id.unwrap_or(0),
            prov: e.prov.unwrap_or_default(),
            level: e.level.unwrap_or_default(),
            task: e.task.unwrap_or_default(),
            msg: e.msg.unwrap_or_default(),
            props: e.props.unwrap_or_default(),
            group: e.group.unwrap_or_else(|| "other".to_string()),
        }
    }).collect();

    let dumps: Vec<DumpInfo> = raw.dump_files.unwrap_or_default().into_iter().map(|d| {
        let path = d.path.unwrap_or_default();
        let size = d.size.unwrap_or(0);
        let mtime = d.mtime.unwrap_or_default();
        parse_dump_file(&path, size, &mtime)
    }).collect();

    let (crashes, findings, summary) = analyze(&system, &config, &dumps, &events, &errors);

    Ok(BsodEvidence {
        system, config, dumps, events, crashes, findings, summary, errors,
        generated_at: now_local_string(),
    })
}

fn run_ps_collect() -> Result<RawPs, String> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let pid = std::process::id();
    let tmp = std::env::temp_dir().join(format!("nexbox_bsod_{}_{}.json", pid, stamp));
    let tmp_str = tmp.to_string_lossy().to_string();

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", PS_SCRIPT])
        .env("BSOD_OUT", &tmp_str)
        .current_dir("C:\\")
        .creation_flags(CREATE_NO_WINDOW);

    let output = cmd.output().map_err(|e| format!("启动 powershell 失败: {}", e))?;
    if !output.status.success() && !tmp.exists() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("powershell 退出码 {:?}：{}", output.status.code(), stderr.trim()));
    }

    let content = read_with_retry(&tmp, Duration::from_secs(3))?;
    let _ = fs::remove_file(&tmp);
    let s = content.trim_start_matches('\u{feff}');
    serde_json::from_str::<RawPs>(s)
        .map_err(|e| format!("解析 PS JSON 失败: {} | head={}", e, s.chars().take(120).collect::<String>()))
}

fn read_with_retry(path: &std::path::Path, timeout: Duration) -> Result<String, String> {
    let start = std::time::Instant::now();
    loop {
        match fs::read_to_string(path) {
            Ok(s) if !s.trim().is_empty() => return Ok(s),
            _ => {}
        }
        if start.elapsed() > timeout { return Err("PS 未生成输出文件（超时）".to_string()); }
        std::thread::sleep(Duration::from_millis(150));
    }
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
        info.processor_count = (read_u32_at(r, (sr as u64) + 4).unwrap_or(0) & 0xFF) as u32;
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
                let (code, args, dump_path) = parse_bugcheck_msg(&e.msg);
                if let Some(idx) = find_or_new(&mut crashes, t, code, &dump_path, true) {
                    let c = &mut crashes[idx];
                    c.sources.push("事件 1001（BugCheck）".to_string());
                    if c.bugcheck.is_none() && code.is_some() { c.bugcheck = code; }
                    if c.args.is_empty() && !args.is_empty() { c.args = args.clone(); }
                    if c.dump_path.is_empty() && !dump_path.is_empty() { c.dump_path = dump_path.clone(); }
                    if c.time == 0 { c.time = t; }
                    c.event_msgs.push(e.clone());
                }
            }
            41 => {
                let props = &e.props;
                let code: Option<u32> = props.first().and_then(|s| {
                    let s = s.trim();
                    if s.is_empty() { None } else {
                        u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
                    }
                });
                let args: Vec<u64> = props.iter().skip(1).take(4).filter_map(|s| {
                    let s = s.trim();
                    if s.is_empty() { None } else {
                        u64::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
                    }
                }).collect();
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
            category: info.map(|i| i.category.to_string()).unwrap_or_default(),
            severity: info.map(|i| i.severity.to_string()).unwrap_or_else(|| "medium".to_string()),
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
    if !ev_whea.is_empty() {
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
            if let Some((_, name)) = DISK_NAMES.iter().find(|(id, _)| *id == e.id) {
                hits.push((*e, *name));
            }
        }
        hits
    };
    if disk_hits.len() >= 3 {
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

    let enabled = config.crash_dump_enabled;
    if enabled == 0 {
        findings.push(Finding {
            severity: "medium".into(),
            title: "系统当前禁用了崩溃转储".into(),
            detail: "「写入调试信息」被设为「无」，因此蓝屏时不会生成任何 dump 文件，下次蓝屏将无法做深度分析。".into(),
            evidence: vec!["CrashDumpEnabled = 0".into()],
            actions: vec![
                "开启「小内存转储」：系统属性 → 高级 → 启动和故障恢复 → 设置".into(),
                "或在管理员 PowerShell 执行：wmic recoveros set DebugInfoType = 3".into(),
            ],
            confidence: 0.95, tag: "配置".into(),
        });
    }
    if enabled == 3 && !dumps.iter().any(|d| d.path.to_lowercase().contains("minidump")) {
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
    if (enabled == 1 || enabled == 7) && config.pagefile_alloc_mb > 0 && mem_gb > 0.0 {
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

    if crash_records.is_empty() {
        findings.push(Finding {
            severity: "info".into(),
            title: "本机未发现蓝屏记录".into(),
            detail: "在系统事件日志与转储目录中都没有找到蓝屏（BugCheck）或非正常关机的记录。如果确实遇到过蓝屏，可能是转储被关闭、或已有记录被清理；下面给出的配置与变更信息仍可用于预防性检查。".into(),
            evidence: vec![
                "事件日志 1001（BugCheck）：无".into(),
                format!("C:\\Windows\\Minidump：{}", if dumps.is_empty() { "未发现 .dmp 文件".into() } else { format!("存在 {} 个文件", dumps.len()) }),
            ],
            actions: vec![
                "确认「小内存转储」已开启，便于下次蓝屏自动留下可分析的记录".into(),
                "若蓝屏时没有时间看代码，可开启蓝屏界面截图或留意屏幕上的错误码".into(),
            ],
            confidence: 0.9, tag: "状态".into(),
        });
    }

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
    if !errors.is_empty() {
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
}
