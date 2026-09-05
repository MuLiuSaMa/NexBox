use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

// Chart.js 4.x UMD minified — 编译期内联，实现离线可用
const CHART_JS: &str = include_str!("../resources/chart.umd.min.js");

/// 环形缓冲区最大容量（1 条/秒 × 3600 = 1 小时）
const MAX_SAMPLES: usize = 3600;

/// 每 N 条快照刷一次磁盘
const DISK_FLUSH_INTERVAL: usize = 10;

/// 嵌入 HTML 的最大数据点数（30d × 86400s = 2.5M 太多，采样的上限）
const MAX_EMBED_POINTS: usize = 60_000;

/// 单条硬件快照（时序数据点）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareSnapshot {
    pub timestamp: String,
    pub elapsed_sec: u64,
    // CPU
    pub cpu_usage: Option<f64>,
    pub cpu_temp: Option<f64>,
    pub cpu_clock: Option<f64>,
    pub cpu_voltage: Option<f64>,
    pub cpu_power: Option<f64>,
    pub cpu_fan_speed: Option<f64>,
    // GPU
    pub gpu_usage: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub gpu_clock: Option<f64>,
    pub gpu_voltage: Option<f64>,
    pub gpu_power: Option<f64>,
    pub gpu_fan_speed: Option<f64>,
    pub gpu_vram_used: Option<f64>,
    pub gpu_vram_total: Option<f64>,
    pub gpu_memory_clock: Option<f64>,
    // 其他
    pub memory_usage: Option<f64>,
    pub ssd_temp: Option<f64>,
}

/// 录制状态（供前端查询）
#[derive(Debug, Serialize)]
pub struct RecordingStatus {
    pub is_recording: bool,
    pub sample_count: u32,
    pub start_time: String,
    pub elapsed_sec: u64,
}

// ─── 持久化路径 ───────────────────────────────────

fn data_dir() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("NexBox")
}

fn data_file_path() -> PathBuf {
    data_dir().join("hardware_data.jsonl")
}

// ─── 内部录制器 ───────────────────────────────────

struct Recorder {
    samples: VecDeque<HardwareSnapshot>,
    start_time: std::time::Instant,
    start_timestamp: String,
    disk_buffer: Vec<HardwareSnapshot>, // 攒一批再写磁盘
    disk_file: Option<File>,
}

static RECORDER: Mutex<Option<Recorder>> = Mutex::new(None);

/// 启动记录器（在 lib.rs setup 中调用）
pub fn start_recording() {
    let mut guard = RECORDER.lock().unwrap();
    if guard.is_none() {
        // 确保数据目录存在
        let _ = std::fs::create_dir_all(data_dir());

        // 打开/创建数据文件（追加模式）
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(data_file_path())
            .ok();

        *guard = Some(Recorder {
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            start_time: std::time::Instant::now(),
            start_timestamp: chrono::Local::now().to_rfc3339(),
            disk_buffer: Vec::with_capacity(DISK_FLUSH_INTERVAL),
            disk_file: file,
        });
        log::info!("[HardwareReport] 记录器已启动 (持久化模式)");
    }
}

/// 停止记录器（在 RunEvent::Exit 中调用）
pub fn stop_recording() {
    let mut guard = RECORDER.lock().unwrap();
    if let Some(recorder) = guard.as_mut() {
        flush_disk_buffer(recorder);
    }
    *guard = None;
    log::info!("[HardwareReport] 记录器已停止");
}

/// 将内存中的磁盘缓冲区刷入文件
fn flush_disk_buffer(recorder: &mut Recorder) {
    if recorder.disk_buffer.is_empty() {
        return;
    }
    if let Some(ref mut file) = recorder.disk_file {
        for snap in &recorder.disk_buffer {
            if let Ok(line) = serde_json::to_string(snap) {
                let _ = writeln!(file, "{}", line);
            }
        }
        let _ = file.flush();
    }
    recorder.disk_buffer.clear();
}

/// 推送一条快照（从 overlay_panel poller 调用）
pub fn push_snapshot(mut snapshot: HardwareSnapshot) {
    let mut guard = RECORDER.lock().unwrap();
    if let Some(recorder) = guard.as_mut() {
        snapshot.timestamp = chrono::Local::now().to_rfc3339();
        snapshot.elapsed_sec = recorder.start_time.elapsed().as_secs();

        // 内存环形缓冲区
        if recorder.samples.len() >= MAX_SAMPLES {
            recorder.samples.pop_front();
        }
        recorder.samples.push_back(snapshot.clone());

        // 磁盘缓冲区（攒批写入）
        recorder.disk_buffer.push(snapshot);
        if recorder.disk_buffer.len() >= DISK_FLUSH_INTERVAL {
            flush_disk_buffer(recorder);
        }
    }
}

/// 从磁盘文件读取历史数据（最多 30 天）
fn load_disk_data() -> Vec<HardwareSnapshot> {
    let path = data_file_path();
    if !path.exists() {
        return Vec::new();
    }

    let thirty_days_ago = chrono::Utc::now() - chrono::Duration::days(30);
    let mut all_data = Vec::new();

    if let Ok(file) = File::open(&path) {
        let reader = BufReader::new(file);
        for line in reader.lines() {
            if let Ok(line) = line {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(snap) = serde_json::from_str::<HardwareSnapshot>(&line) {
                    // 只保留 30 天内的数据
                    if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&snap.timestamp) {
                        if ts < thirty_days_ago {
                            continue;
                        }
                    }
                    all_data.push(snap);
                }
            }
        }
    }

    // 按时间排序
    all_data.sort_by(|a, b| a.elapsed_sec.cmp(&b.elapsed_sec));
    all_data
}

/// 将磁盘数据与当前会话数据合并、去重、采样
fn merge_data(mut disk: Vec<HardwareSnapshot>, session: Vec<HardwareSnapshot>) -> Vec<HardwareSnapshot> {
    // 合并
    disk.extend(session);

    // 按 timestamp 去重（跨 session 不会冲突）
    let mut seen = std::collections::HashSet::new();
    disk.retain(|s| seen.insert(s.timestamp.clone()));

    // 按 timestamp 排序
    disk.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // 基于第一个数据点的 timestamp 重新计算 elapsed_sec，
    // 这样即使数据来自多个 session，时间轴也是一致的
    if let Some(first) = disk.first() {
        if let Ok(base) = chrono::DateTime::parse_from_rfc3339(&first.timestamp) {
            for s in &mut disk {
                if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&s.timestamp) {
                    let diff = (ts.with_timezone(&chrono::Utc) - base.with_timezone(&chrono::Utc))
                        .num_seconds();
                    s.elapsed_sec = diff.max(0) as u64;
                }
            }
        }
    }

    // 如果超过上限则均匀采样
    if disk.len() > MAX_EMBED_POINTS {
        let step = disk.len() as f64 / MAX_EMBED_POINTS as f64;
        let mut sampled = Vec::with_capacity(MAX_EMBED_POINTS);
        for i in 0..MAX_EMBED_POINTS {
            let idx = (i as f64 * step) as usize;
            if idx < disk.len() {
                sampled.push(disk[idx].clone());
            }
        }
        sampled
    } else {
        disk
    }
}

// ─── Tauri 命令 ──────────────────────────────────

/// 导出 HTML 报告到用户指定路径
#[tauri::command]
pub async fn export_hardware_report(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        // 1. 先刷磁盘缓冲区
        {
            let mut guard = RECORDER.lock().unwrap();
            if let Some(recorder) = guard.as_mut() {
                flush_disk_buffer(recorder);
            }
        }

        // 2. 加载磁盘历史数据
        let disk_data = load_disk_data();

        // 3. 获取当前会话数据
        let session_data = {
            let guard = RECORDER.lock().unwrap();
            match guard.as_ref() {
                Some(recorder) => {
                    recorder.samples.iter().cloned().collect::<Vec<_>>()
                }
                None => return Err("记录器未启动".to_string()),
            }
        };

        // 4. 合并 & 采样
        let all_data = merge_data(disk_data, session_data);

        // 用数据中第一条的时间戳作为报告起始时间
        let start_ts = all_data.first()
            .map(|s| s.timestamp.clone())
            .unwrap_or_else(|| chrono::Local::now().to_rfc3339());

        // 5. 获取静态硬件信息
        let hardware = crate::hardware::get_hardware_info()
            .map_err(|e| format!("获取硬件信息失败: {}", e))?;

        // 6. 获取操作系统版本
        let os_version = crate::hardware::long_os_version()
            .unwrap_or_else(|| "Unknown".to_string());

        // 7. 生成 HTML
        let html = generate_html_report(&all_data, &start_ts, &hardware, &os_version);

        // 8. 写入文件
        std::fs::write(&path, html)
            .map_err(|e| format!("写入文件失败: {}", e))?;

        log::info!(
            "[HardwareReport] 报告已导出: {} ({} 条数据, {:.1} KB)",
            path,
            all_data.len(),
            std::fs::metadata(&path).map(|m| m.len() as f64 / 1024.0).unwrap_or(0.0)
        );

        Ok(format!(
            "报告已导出 ({} 条数据)",
            all_data.len()
        ))
    })
    .await
    .map_err(|e| format!("导出任务失败: {}", e))?
}

/// 获取当前录制状态
#[tauri::command]
pub fn get_hardware_recording_status() -> RecordingStatus {
    let guard = RECORDER.lock().unwrap();
    match guard.as_ref() {
        Some(recorder) => RecordingStatus {
            is_recording: true,
            sample_count: recorder.samples.len() as u32,
            start_time: recorder.start_timestamp.clone(),
            elapsed_sec: recorder.start_time.elapsed().as_secs(),
        },
        None => RecordingStatus {
            is_recording: false,
            sample_count: 0,
            start_time: String::new(),
            elapsed_sec: 0,
        },
    }
}

/// 清除本地持久化的硬件数据文件
#[tauri::command]
pub fn clear_hardware_data() -> Result<String, String> {
    let path = data_file_path();
    let deleted = if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("删除数据文件失败: {}", e))?;
        true
    } else {
        false
    };

    // 重新打开数据文件，让后续数据能继续写入
    let mut guard = RECORDER.lock().unwrap();
    if let Some(recorder) = guard.as_mut() {
        // 清空内存 + 磁盘缓冲 + 重置文件句柄
        recorder.samples.clear();
        recorder.disk_buffer.clear();
        recorder.start_time = std::time::Instant::now();
        recorder.start_timestamp = chrono::Utc::now().to_rfc3339();
        recorder.disk_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok();
        log::info!("[HardwareReport] 记录器已重置，新数据文件已创建");
    }

    if deleted {
        log::info!("[HardwareReport] 持久化数据文件已清除并重建: {:?}", path);
        Ok("硬件数据已清除，新数据将继续记录".to_string())
    } else {
        Ok("数据文件已重建，将继续记录".to_string())
    }
}

// ─── HTML 报告生成 ───────────────────────────────

fn generate_html_report(
    samples: &[HardwareSnapshot],
    start_ts: &str,
    hardware: &crate::hardware::HardwareInfo,
    os_version: &str,
) -> String {
    // 序列化数据为 JSON
    let data_json = serde_json::to_string(samples).unwrap_or_else(|_| "[]".to_string());
    let hardware_json = serde_json::to_string(hardware).unwrap_or_else(|_| "{}".to_string());

    // 防止 </script> 注入
    let data_json_safe = data_json.replace("</", "<\\/");
    let hardware_json_safe = hardware_json.replace("</", "<\\/");
    let os_version_safe = html_escape(os_version);
    let _start_ts_safe = html_escape(start_ts);

    // 防止 Chart.js 代码中的 </script> 问题
    let chart_js_safe = CHART_JS.replace("</script>", "<\\/script>");

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _version = env!("CARGO_PKG_VERSION");
    let _sample_count = samples.len();
    let _duration_sec = samples.last().map(|s| s.elapsed_sec).unwrap_or(0);

    let mut html = String::with_capacity(384 * 1024 + chart_js_safe.len() + data_json_safe.len());

    // ── HTML head + CSS ──
    html.push_str(r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>新境盒硬件监控报告</title>
<style>
:root {
    --bg-primary: #0a0a0a;
    --bg-secondary: #0f0f0f;
    --bg-card: rgba(255, 255, 255, 0.04);
    --bg-card-hover: rgba(255, 255, 255, 0.07);
    --bg-stat: rgba(255, 255, 255, 0.02);
    --text-primary: #f0f0f0;
    --text-secondary: #999999;
    --text-muted: #555555;
    --border: rgba(255, 255, 255, 0.08);
    --border-hover: rgba(255, 255, 255, 0.15);
    --shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    --radius: 16px;
    --radius-sm: 10px;
    --accent: #cccccc;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
    background: #0a0a0a;
    background-attachment: fixed;
    color: var(--text-primary);
    font-family: 'Segoe UI', 'Microsoft YaHei', -apple-system, system-ui, sans-serif;
    line-height: 1.6;
    min-height: 100vh;
    -webkit-font-smoothing: antialiased;
}
.container { max-width: 1400px; margin: 0 auto; padding: 0 24px 80px; }

/* Hero */
.hero {
    text-align: center;
    padding: 48px 24px 32px;
    position: relative;
    overflow: hidden;
}
.hero h1 {
    font-size: 2.2rem;
    font-weight: 800;
    letter-spacing: -0.02em;
    background: linear-gradient(135deg, #ffffff, #cccccc, #aaaaaa);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    position: relative;
    z-index: 1;
}
.hero .subtitle {
    color: var(--text-secondary);
    font-size: 1rem;
    margin-top: 6px;
    position: relative; z-index: 1;
}

/* Time navigation */
.time-nav {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
    margin: 20px auto 0;
    position: relative; z-index: 1;
}
.time-btn {
    background: transparent;
    border: 1px solid rgba(255,255,255,0.1);
    color: var(--text-secondary);
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 500;
    transition: all 0.2s;
}
.time-btn:hover {
    border-color: rgba(255,255,255,0.25);
    color: var(--text-primary);
}
.time-btn.active {
    background: rgba(255,107,53,0.15);
    border-color: #ff6b35;
    color: #ff6b35;
}

/* Meta bar */
.meta-bar {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 10px;
    margin-top: 16px;
    position: relative; z-index: 1;
}
.meta-item {
    background: var(--bg-card);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px 16px;
    font-size: 0.82rem;
    color: var(--text-secondary);
}
.meta-item strong { color: var(--text-primary); font-weight: 600; }

/* Section */
.section { margin-top: 40px; }
.section-title {
    font-size: 1.25rem;
    font-weight: 700;
    margin-bottom: 16px;
    padding-left: 14px;
    border-left: 4px solid #ff6b35;
    color: var(--text-primary);
}

/* Hardware overview cards */
.hw-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 14px;
}
.hw-card {
    background: var(--bg-card);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 18px;
    position: relative;
    overflow: hidden;
    transition: border-color 0.3s, box-shadow 0.3s;
}
.hw-card:hover { border-color: var(--border-hover); box-shadow: var(--shadow); }
.hw-card::before {
    content: '';
    position: absolute;
    top: 0; left: 0; right: 0;
    height: 3px;
    background: var(--accent);
    border-radius: var(--radius) var(--radius) 0 0;
}
.hw-card h3 {
    font-size: 0.92rem;
    font-weight: 700;
    color: var(--accent);
    margin-bottom: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
}
.hw-card table { width: 100%; border-collapse: collapse; }
.hw-card td {
    padding: 4px 0;
    font-size: 0.84rem;
    border-bottom: 1px solid rgba(180,120,80,0.08);
}
.hw-card td:first-child { color: var(--text-muted); width: 42%; }
.hw-card td:last-child {
    color: var(--text-primary);
    text-align: right;
    font-weight: 500;
    word-break: break-all;
}

/* Stats grid */
.stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 10px;
}
.stat-item {
    background: var(--bg-stat);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 14px;
    text-align: center;
    transition: transform 0.2s, border-color 0.2s;
}
.stat-item:hover { transform: translateY(-2px); border-color: var(--accent, var(--border-hover)); }
.stat-label {
    color: var(--text-muted);
    font-size: 0.72rem;
    margin-bottom: 4px;
}
.stat-value {
    font-size: 1.4rem;
    font-weight: 700;
    color: var(--accent, var(--text-primary));
    line-height: 1.2;
}
.stat-range {
    color: var(--text-muted);
    font-size: 0.7rem;
    margin-top: 3px;
}

/* Charts */
.charts-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 14px;
}
.chart-card {
    background: var(--bg-card);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 18px;
    position: relative;
    overflow: hidden;
    transition: border-color 0.3s;
}
.chart-card:hover { border-color: var(--border-hover); }
.chart-card::before {
    content: '';
    position: absolute;
    top: 0; left: 0; right: 0;
    height: 2px;
    background: var(--accent);
}
.chart-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
}
.chart-header h3 {
    font-size: 0.88rem;
    font-weight: 600;
    color: var(--text-secondary);
}
.chart-value {
    font-size: 1rem;
    font-weight: 700;
    color: var(--accent, var(--text-primary));
}
.chart-container {
    position: relative;
    height: 160px;
}

/* Footer */
.footer {
    text-align: center;
    padding: 32px 0 16px;
    color: var(--text-muted);
    font-size: 0.78rem;
}

/* No data */
.no-data {
    text-align: center;
    padding: 40px;
    color: var(--text-muted);
    font-size: 0.9rem;
}

/* Print button */
.print-btn {
    position: fixed;
    top: 16px; right: 16px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
    color: #cccccc;
    padding: 7px 16px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 500;
    backdrop-filter: blur(10px);
    transition: all 0.2s;
    z-index: 100;
}
.print-btn:hover { background: rgba(255, 255, 255, 0.12); }

/* Responsive */
@media (max-width: 900px) {
    .charts-grid { grid-template-columns: 1fr; }
    .hero h1 { font-size: 1.5rem; }
}
@media print {
    .print-btn, .time-nav { display: none; }
    body { background: white !important; color: black !important; }
    .hero h1 { -webkit-text-fill-color: #1a1a1a; }
    .hw-card, .chart-card, .stat-item {
        background: white !important;
        border: 1px solid #ccc !important;
        box-shadow: none !important;
        backdrop-filter: none !important;
    }
    .chart-card { page-break-inside: avoid; }
}
</style>
</head>
<body>
<button class="print-btn" onclick="window.print()">打印 / 保存 PDF</button>
<div class="hero">
    <h1>NexBox 硬件监控报告</h1>
    <p class="subtitle">Hardware Monitoring Report</p>
    <div class="time-nav" id="time-nav"></div>
    <div class="meta-bar" id="meta-bar"></div>
</div>
<div class="container">
    <div class="section">
        <div class="section-title"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:middle;margin-right:6px;margin-top:-2px"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>硬件信息概览</div>
        <div class="hw-grid" id="hw-overview"></div>
    </div>
    <div class="section">
        <div class="section-title"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:middle;margin-right:6px;margin-top:-2px"><path d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/><polyline points="12 6 12 12 16 14"/></svg>统计摘要</div>
        <div class="stats-grid" id="stats-grid"></div>
    </div>
    <div class="section">
        <div class="section-title"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="vertical-align:middle;margin-right:6px;margin-top:-2px"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>实时监控图表</div>
        <div class="charts-grid" id="charts-grid"></div>
    </div>
</div>
<div class="footer">
    报告生成时间: "#);
    html.push_str(&now);
    html.push_str(r#" · 数据持久记录于本地
</div>
<script>"#);
    // 内联 Chart.js
    html.push_str(&chart_js_safe);
    html.push_str(r#"</script>
<script>
// ─── 原始全量数据 ────────────────────────────
const RAW_DATA = "#);
    html.push_str(&data_json_safe);
    html.push_str(r#";
const HARDWARE_INFO = "#);
    html.push_str(&hardware_json_safe);
    html.push_str(r#";
const REPORT_META = {
    osVersion: ""#);
    html.push_str(&os_version_safe);
    html.push_str(r#""};
const NOW_TS = Date.now();

// ─── 时间范围配置 ────────────────────────────
const TIME_RANGES = [
    { key:'1h',  label:'1 小时内',  seconds: 3600,        unit:'分', bucketSec: 1     },
    { key:'6h',  label:'6 小时内',  seconds: 21600,       unit:'分', bucketSec: 60    },
    { key:'12h', label:'12 小时内', seconds: 43200,       unit:'分', bucketSec: 120   },
    { key:'24h', label:'24 小时内', seconds: 86400,       unit:'分', bucketSec: 300   },
    { key:'7d',  label:'7 天内',    seconds: 604800,      unit:'时', bucketSec: 1800  },
    { key:'30d', label:'30 天内',   seconds: 2592000,     unit:'天', bucketSec: 7200  },
];
let currentRange = '1h';
let chartInstances = [];

// ─── 图表配置 ────────────────────────────────
const CHARTS = [
    { id:'cpuUsage',     label:'CPU 占用率',   field:'cpu_usage',        color:'#ff6b35', unit:'%' },
    { id:'cpuTemp',      label:'CPU 温度',      field:'cpu_temp',         color:'#e74c3c', unit:'°C' },
    { id:'cpuPower',     label:'CPU 功耗',      field:'cpu_power',        color:'#f39c12', unit:'W' },
    { id:'cpuClock',     label:'CPU 频率',      field:'cpu_clock',        color:'#ffc312', unit:'MHz' },
    { id:'cpuVoltage',   label:'CPU 电压',      field:'cpu_voltage',      color:'#fd79a8', unit:'V' },
    { id:'cpuFan',       label:'CPU 风扇转速',  field:'cpu_fan_speed',    color:'#fdcb6e', unit:'RPM' },
    { id:'gpuUsage',     label:'GPU 占用率',   field:'gpu_usage',        color:'#ff7675', unit:'%' },
    { id:'gpuTemp',      label:'GPU 温度',      field:'gpu_temp',         color:'#e74c3c', unit:'°C' },
    { id:'gpuPower',     label:'GPU 功耗',      field:'gpu_power',        color:'#ff6b35', unit:'W' },
    { id:'gpuClock',     label:'GPU 频率',      field:'gpu_clock',        color:'#f39c12', unit:'MHz' },
    { id:'gpuVoltage',   label:'GPU 电压',      field:'gpu_voltage',      color:'#fd79a8', unit:'V' },
    { id:'gpuVram',      label:'GPU 显存占用',  field:'gpu_vram_used',    color:'#e84393', unit:'MB' },
    { id:'gpuFan',       label:'GPU 风扇转速',  field:'gpu_fan_speed',    color:'#fdcb6e', unit:'RPM' },
    { id:'gpuMemClock',  label:'GPU 显存频率',  field:'gpu_memory_clock', color:'#ffc312', unit:'MHz' },
    { id:'memUsage',     label:'内存占用率',    field:'memory_usage',     color:'#ff6b35', unit:'%' },
    { id:'ssdTemp',      label:'硬盘温度',      field:'ssd_temp',         color:'#ff7675', unit:'°C' },
];

// ─── 工具函数 ────────────────────────────────
function formatDuration(sec) {
    if (sec >= 86400) return (sec/86400).toFixed(1) + ' 天';
    const h = Math.floor(sec / 3600);
    const m = Math.floor((sec % 3600) / 60);
    if (h > 0) return h + '时 ' + m + '分';
    return m + '分 ' + (sec % 60) + '秒';
}

function formatTimeLabel(sec, unit) {
    if (unit === '天') return (sec / 86400).toFixed(1) + '天';
    if (unit === '时') return Math.floor(sec / 3600) + '时';
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return m + ':' + String(s).padStart(2, '0');
}

function escapeHtml(str) {
    if (str == null) return '--';
    return String(str).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

function fmtVal(v, unit) {
    if (v == null || v === undefined) return '--';
    if (typeof v === 'number') {
        if (Math.abs(v) >= 100) return v.toFixed(0) + unit;
        if (Math.abs(v) >= 10) return v.toFixed(1) + unit;
        return v.toFixed(2) + unit;
    }
    return escapeHtml(v);
}

// ─── 数据过滤 & 聚合 ────────────────────────
function filterAndAggregate(rangeKey) {
    if (RAW_DATA.length === 0) return { data: [], labelUnit: '分' };

    const range = TIME_RANGES.find(r => r.key === rangeKey);
    if (!range) return { data: RAW_DATA, labelUnit: '分' };

    // 计算截断时间
    const nowElapsed = RAW_DATA.length > 0 ? RAW_DATA[RAW_DATA.length - 1].elapsed_sec : 0;
    const cutoff = Math.max(0, nowElapsed - range.seconds);

    // 筛选时间范围内的数据
    let filtered = RAW_DATA.filter(function(s) { return s.elapsed_sec >= cutoff; });
    if (filtered.length === 0) return { data: [], labelUnit: range.unit };

    // 按时间桶聚合
    const bucketSize = range.bucketSec;
    if (bucketSize <= 1 || filtered.length < 200) {
        return { data: filtered, labelUnit: range.unit };
    }

    // 桶聚合：每个桶内取平均值
    var buckets = {};
    filtered.forEach(function(s) {
        var bucketKey = Math.floor(s.elapsed_sec / bucketSize) * bucketSize;
        if (!buckets[bucketKey]) buckets[bucketKey] = { count: 0, sum: {}, first: null };
        var b = buckets[bucketKey];
        b.count++;
        b.first = s;
        // 对所有数值字段累加
        Object.keys(s).forEach(function(k) {
            if (typeof s[k] === 'number' && k !== 'elapsed_sec') {
                if (!b.sum[k]) b.sum[k] = 0;
                b.sum[k] += s[k];
            }
        });
    });

    // 将桶转换为数据点
    var result = [];
    Object.keys(buckets).sort(function(a,b) { return a - b; }).forEach(function(key) {
        var b = buckets[key];
        var point = { timestamp: b.first.timestamp, elapsed_sec: parseInt(key) };
        Object.keys(b.sum).forEach(function(k) {
            point[k] = b.sum[k] / b.count;
        });
        result.push(point);
    });
    return { data: result, labelUnit: range.unit };
}

// ─── 渲染导航栏 ─────────────────────────────
function populateTimeNav() {
    const nav = document.getElementById('time-nav');
    nav.innerHTML = TIME_RANGES.map(function(r) {
        return '<button class="time-btn' + (r.key === currentRange ? ' active' : '') + '" data-range="' + r.key + '" onclick="switchTimeRange(\'' + r.key + '\')">' + r.label + '</button>';
    }).join('');
}

// ─── 切换时间范围 ───────────────────────────
function switchTimeRange(rangeKey) {
    currentRange = rangeKey;

    // 更新导航按钮状态
    document.querySelectorAll('.time-btn').forEach(function(btn) {
        btn.classList.toggle('active', btn.dataset.range === rangeKey);
    });

    // 重新渲染
    renderAll();
}

// ─── 渲染所有（统计 + 图表） ────────────────
function renderAll() {
    var agg = filterAndAggregate(currentRange);
    var data = agg.data;
    var labelUnit = agg.labelUnit;

    if (data.length === 0) {
        document.getElementById('stats-grid').innerHTML = '<div class="no-data">该时间范围内暂无采样数据</div>';
        document.getElementById('charts-grid').innerHTML = '<div class="no-data">该时间范围内暂无采样数据</div>';
        return;
    }

    renderStats(data);
    renderCharts(data, labelUnit);
}

// ─── 填充元数据 ─────────────────────────────
function populateMeta() {
    const bar = document.getElementById('meta-bar');
    const len = RAW_DATA.length;
    const last = RAW_DATA.length > 0 ? RAW_DATA[RAW_DATA.length - 1] : null;
    var duration = last ? last.elapsed_sec : 0;
    const items = [
        { label: '生成时间', value: new Date().toLocaleString('zh-CN') },
        { label: '总记录时长', value: formatDuration(duration) },
        { label: '总采样数', value: len + ' 条' },
        { label: '操作系统', value: escapeHtml(REPORT_META.osVersion) },
    ];
    bar.innerHTML = items.map(function(i) {
        return '<div class="meta-item"><strong>' + i.label + ':</strong> ' + i.value + '</div>';
    }).join('');
}

// ─── 填充硬件概览 ───────────────────────────
function populateHardwareOverview() {
    const c = document.getElementById('hw-overview');
    const hw = HARDWARE_INFO;
    let html = '';

    var cpuSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2"/><rect x="9" y="9" width="6" height="6"/><line x1="9" y1="1" x2="9" y2="4"/><line x1="15" y1="1" x2="15" y2="4"/><line x1="9" y1="20" x2="9" y2="23"/><line x1="15" y1="20" x2="15" y2="23"/><line x1="20" y1="9" x2="23" y2="9"/><line x1="20" y1="14" x2="23" y2="14"/><line x1="1" y1="9" x2="4" y2="9"/><line x1="1" y1="14" x2="4" y2="14"/></svg>';
    var gpuSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/><circle cx="8" cy="12" r="3"/><circle cx="16" cy="12" r="4"/><line x1="16" y1="9" x2="16" y2="15"/><line x1="13" y1="12" x2="19" y2="12"/></svg>';
    var memSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="4" width="16" height="16" rx="2"/><line x1="9" y1="4" x2="9" y2="20"/><line x1="15" y1="4" x2="15" y2="20"/><line x1="4" y1="9" x2="9" y2="9"/><line x1="4" y1="15" x2="9" y2="15"/><line x1="15" y1="9" x2="20" y2="9"/><line x1="15" y1="15" x2="20" y2="15"/></svg>';
    var mbSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8" cy="8" r="1.5"/><circle cx="16" cy="8" r="1.5"/><circle cx="8" cy="16" r="1.5"/><circle cx="12" cy="12" r="1.5"/><line x1="12" y1="3" x2="12" y2="12"/><line x1="3" y1="12" x2="12" y2="12"/></svg>';
    var diskSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><ellipse cx="12" cy="12" rx="10" ry="10"/><circle cx="12" cy="12" r="3"/><line x1="12" y1="9" x2="12" y2="12"/><line x1="12" y1="12" x2="14" y2="14"/></svg>';
    var netSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/><path d="M8 12h1"/><path d="M15 12h1"/><rect x="7" y="10" width="2" height="4" rx="0.5"/><rect x="15" y="10" width="2" height="4" rx="0.5"/></svg>';
    var audioSvg = '<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M15.54 8.46a5 5 0 010 7.07"/></svg>';

    // CPU
    html += hwCard('CPU', '#ff6b35', [
        ['型号', hw.cpu.name],
        ['核心/线程', hw.cpu.cores + '核 / ' + hw.cpu.threads + '线程'],
        ['基础频率', (hw.cpu.max_clock_speed / 1000).toFixed(2) + ' GHz'],
        ['L3 缓存', (hw.cpu.l3_cache_size / 1024).toFixed(0) + ' MB'],
    ], cpuSvg);
    // GPU
    hw.gpu.forEach(function(gpu, i) {
        var gpuItems = [
            ['型号', gpu.name], ['厂商', gpu.vendor],
        ];
        if (gpu.memory_gb != null && gpu.memory_gb > 0) {
            gpuItems.push(['显存', gpu.memory_gb.toFixed(1) + ' GB']);
        }
        gpuItems.push(['驱动版本', gpu.driver_version]);
        html += hwCard('GPU ' + (i + 1), '#e74c3c', gpuItems, gpuSvg);
    });
    // 内存
    var totalMem = hw.memory.reduce(function(s, m) { return s + m.capacity_gb; }, 0);
    var memItems = [
        ['总容量', totalMem.toFixed(0) + ' GB'], ['内存条数', hw.memory.length + ' 条'],
    ];
    if (hw.memory.length > 0) memItems.push(['频率', hw.memory[0].speed_mhz + ' MHz']);
    hw.memory.forEach(function(m, i) {
        memItems.push(['插槽 ' + (i + 1), m.manufacturer + ' ' + m.capacity_gb.toFixed(0) + 'GB ' + m.speed_mhz + 'MHz']);
    });
    html += hwCard('内存', '#f39c12', memItems, memSvg);
    html += hwCard('主板', '#ffc312', [['型号', hw.motherboard.product], ['制造商', hw.motherboard.manufacturer], ['BIOS', hw.motherboard.bios_version]], mbSvg);
    hw.disk.forEach(function(d, i) { html += hwCard('存储 ' + (i + 1), '#fd79a8', [['型号', d.model], ['容量', d.size_gb.toFixed(1) + ' GB'], ['接口', d.interface_type]], diskSvg); });
    hw.network_card.forEach(function(n, i) {
        html += hwCard('网卡 ' + (i + 1), '#ff7675', [
            ['型号', n.name], ['厂商', n.manufacturer],
            ['类型', n.adapter_type], ['MAC 地址', n.mac_address],
            ['链接速度', n.speed_mbps > 0 ? n.speed_mbps + ' Mbps' : '--'],
        ], netSvg);
    });
    hw.sound_card.forEach(function(s, i) {
        html += hwCard('声卡 ' + (i + 1), '#e84393', [['型号', s.name], ['厂商', s.manufacturer]], audioSvg);
    });
    c.innerHTML = html;
}

function hwCard(title, color, items, svgHtml) {
    var rows = items.map(function(item) {
        return '<tr><td>' + escapeHtml(item[0]) + '</td><td>' + escapeHtml(item[1]) + '</td></tr>';
    }).join('');
    return '<div class="hw-card" style="--accent:' + color + '"><h3>' + svgHtml + escapeHtml(title) + '</h3><table>' + rows + '</table></div>';
}

// ─── 渲染统计摘要 ───────────────────────────
function renderStats(data) {
    const c = document.getElementById('stats-grid');
    let html = '';
    CHARTS.forEach(function(cfg) {
        var values = data.map(function(s) { return s[cfg.field]; })
            .filter(function(v) { return v != null && v !== undefined; });
        if (values.length === 0) return;
        var sum = values.reduce(function(a, b) { return a + b; }, 0);
        var avg = sum / values.length;
        var max = Math.max.apply(null, values);
        var min = Math.min.apply(null, values);
        html += '<div class="stat-item" style="--accent:' + cfg.color + '">' +
            '<div class="stat-label">' + cfg.label + ' 平均值</div>' +
            '<div class="stat-value">' + fmtVal(avg, cfg.unit) + '</div>' +
            '<div class="stat-range">最小值 ' + fmtVal(min, '') + ' ~ 最大值 ' + fmtVal(max, '') + ' ' + cfg.unit + '</div>' +
            '</div>';
    });
    c.innerHTML = html;
}

// ─── 渲染图表 ───────────────────────────────
function renderCharts(data, labelUnit) {
    const c = document.getElementById('charts-grid');
    c.innerHTML = '';

    // 销毁之前的图表
    chartInstances.forEach(function(ch) { ch.destroy(); });
    chartInstances = [];

    if (data.length === 0) {
        c.innerHTML = '<div class="no-data">暂无采样数据</div>';
        return;
    }

    var labels = data.map(function(s) { return formatTimeLabel(s.elapsed_sec, labelUnit); });

    CHARTS.forEach(function(cfg) {
        var card = document.createElement('div');
        card.className = 'chart-card';
        card.style.setProperty('--accent', cfg.color);

        var header = document.createElement('div');
        header.className = 'chart-header';
        var chartSvg = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>';
        header.innerHTML = '<h3>' + chartSvg + ' ' + cfg.label + '</h3><span class="chart-value" id="' + cfg.id + '-val">--</span>';

        var wrap = document.createElement('div');
        wrap.className = 'chart-container';
        var canvas = document.createElement('canvas');
        canvas.id = cfg.id;
        wrap.appendChild(canvas);

        card.appendChild(header);
        card.appendChild(wrap);
        c.appendChild(card);

        var rawData = data.map(function(s) { return s[cfg.field]; });

        // 平均值
        var validData = rawData.filter(function(v) { return v != null; });
        if (validData.length > 0) {
            var sum = validData.reduce(function(a, b) { return a + b; }, 0);
            var avg = sum / validData.length;
            document.getElementById(cfg.id + '-val').textContent = '平均 ' + fmtVal(avg, cfg.unit);
        }

        // 找到最大值用于 Y 轴范围
        var yMax = null;
        validData.forEach(function(v) { if (yMax === null || v > yMax) yMax = v; });

        var ctx = canvas.getContext('2d');
        var gradient = ctx.createLinearGradient(0, 0, 0, 160);
        gradient.addColorStop(0, cfg.color + '40');
        gradient.addColorStop(1, cfg.color + '00');

        var ch = new Chart(ctx, {
            type: 'line',
            data: {
                labels: labels,
                datasets: [{
                    label: cfg.label,
                    data: rawData,
                    borderColor: cfg.color,
                    backgroundColor: gradient,
                    fill: true,
                    tension: 0.3,
                    pointRadius: 0,
                    pointHoverRadius: 4,
                    pointHoverBackgroundColor: cfg.color,
                    pointHoverBorderColor: '#fff',
                    pointHoverBorderWidth: 2,
                    borderWidth: 1.5,
                    spanGaps: true,
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                animation: { duration: 400, easing: 'easeOutQuart' },
                plugins: {
                    legend: { display: false },
                    tooltip: {
                        backgroundColor: 'rgba(10, 10, 10, 0.95)',
                        titleColor: '#f0f0f0',
                        bodyColor: '#f0f0f0',
                        borderColor: cfg.color,
                        borderWidth: 1,
                        padding: 10,
                        cornerRadius: 6,
                        titleFont: { size: 11, weight: '600' },
                        bodyFont: { size: 12 },
                        callbacks: {
                            label: function(ctx) {
                                if (ctx.parsed.y == null) return cfg.label + ': N/A';
                                return cfg.label + ': ' + ctx.parsed.y.toFixed(1) + cfg.unit;
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        grid: { color: 'rgba(255,255,255,0.03)', drawTicks: false },
                        ticks: { color: '#555555', maxTicksLimit: 10, font: { size: 9 } },
                        border: { display: false }
                    },
                    y: {
                        grid: { color: 'rgba(255,255,255,0.03)' },
                        ticks: { color: '#555555', font: { size: 9 } },
                        border: { display: false },
                        suggestedMax: yMax !== null ? yMax * 1.1 : undefined,
                    }
                },
                interaction: { intersect: false, mode: 'index' }
            }
        });
        chartInstances.push(ch);
    });
}

// ─── 初始化 ─────────────────────────────────
document.addEventListener('DOMContentLoaded', function() {
    populateTimeNav();
    populateMeta();
    populateHardwareOverview();
    renderAll();
});
</script>
</body>
</html>"#);

    html
}

/// HTML 转义
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
