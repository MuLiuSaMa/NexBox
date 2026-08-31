use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle, Manager};

pub struct SensorChild(pub Mutex<Option<Child>>);

/// LHML 传感器单条数据（与 C# SensorReading 对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub hardware: String,
    #[serde(rename = "hardwareType")]
    pub hardware_type: String,
    #[serde(rename = "subHardware")]
    pub sub_hardware: Option<String>,
    pub name: String,
    #[serde(rename = "sensorType")]
    pub sensor_type: String,
    pub value: f64,
    pub unit: Option<String>,
}

/// LHML 传感器响应（与 C# SensorsResponse 对齐）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorsResponse {
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    pub sensors: Vec<SensorReading>,
}

/// 管道桥接：管理 NexBoxMonitor 子进程的 stdin/stdout
pub struct SensorBridge {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    writer: std::process::ChildStdin,
    /// 子进程启动时间。net48 + LHML 初始化需要数秒，
    /// 启动早期读管道会卡满超时，用于判断传感器是否已就绪
    started_at: std::time::Instant,
}

impl SensorBridge {
    /// 发送读取命令，返回传感器数据。
    /// 读取带超时保护：NexBoxMonitor 子进程若卡死/不响应，会在超时后返回错误，
    /// 由调用方决定是否强制重启子进程，避免管道阻塞导致全局互斥锁被永久占用。
    pub fn read_sensors(&mut self) -> Result<SensorsResponse, String> {
        // 子进程刚启动时 LHML 仍在初始化（通常需 2~4 秒），
        // 此时发命令会卡满 8 秒管道超时。启动早期直接快速失败，
        // 让调用方（如硬件信息采集）立即返回静态数据，避免阻塞启动。
        if self.started_at.elapsed() < std::time::Duration::from_secs(5) {
            return Err("NexBoxMonitor 传感器尚未就绪".to_string());
        }

        // 发送命令
        writeln!(self.writer, r#"{{"cmd":"read"}}"#)
            .map_err(|e| format!("写入管道失败: {}", e))?;
        self.writer
            .flush()
            .map_err(|e| format!("刷新管道失败: {}", e))?;

        // 批量读取直到换行：优先消费 BufReader 内部缓冲，缓冲耗尽时才等待管道可读。
        // 每次循环最多处理一个缓冲块（默认 8KB），大幅减少系统调用次数；
        // 同时保留 WaitForSingleObject 超时保护，子进程卡死时仍能按时返回。
        let timeout = std::time::Duration::from_secs(8);
        let deadline = std::time::Instant::now() + timeout;
        let mut line_bytes: Vec<u8> = Vec::with_capacity(16384);

        loop {
            let buf = match self.reader.fill_buf() {
                Ok(buf) => buf,
                Err(e) => return Err(format!("读取管道失败: {}", e)),
            };

            if !buf.is_empty() {
                match buf.iter().position(|&b| b == b'\n') {
                    Some(pos) => {
                        line_bytes.extend_from_slice(&buf[..pos]);
                        self.reader.consume(pos + 1);
                        break;
                    }
                    None => {
                        line_bytes.extend_from_slice(buf);
                        let n = buf.len();
                        self.reader.consume(n);
                    }
                }
            } else {
                // 缓冲为空：等待底层管道有数据或关闭（带超时）
                if !wait_pipe_readable(&mut self.reader, deadline) {
                    return Err("读取管道超时".to_string());
                }
            }
        }

        // 子进程 (net48) 的 stdout 可能使用系统 ANSI 编码（如 GBK），
        // 用 lossy 转换保证 JSON 结构不因编码问题而整体解析失败
        let line = String::from_utf8_lossy(&line_bytes);
        let line = line.trim();
        if line.is_empty() {
            return Err("子进程返回空响应".to_string());
        }

        // 先尝试解析为 SensorsResponse
        match serde_json::from_str::<SensorsResponse>(line) {
            Ok(response) => Ok(response),
            Err(_) => {
                // 如果失败，检查是否是子进程报错 JSON（{"error":"..."}）
                if let Ok(err_obj) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(err_msg) = err_obj.get("error").and_then(|v| v.as_str()) {
                        return Err(format!("NexBoxMonitor 子进程报错: {}", err_msg));
                    }
                }
                Err(format!("解析传感器JSON失败: {}", line))
            }
        }
    }

    /// 检查子进程是否仍然存活
    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
        }
    }

    /// 优雅关闭子进程（最多等待 3 秒，超时则强制终止）
    pub fn shutdown(&mut self) {
        let _ = writeln!(self.writer, r#"{{"cmd":"exit"}}"#);
        let _ = self.writer.flush();
        let start = std::time::Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() > std::time::Duration::from_secs(3) {
                        log::warn!("NexBoxMonitor 子进程未在 3s 内退出，强制终止");
                        let _ = self.child.kill();
                        let _ = self.child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    log::warn!("等待 NexBoxMonitor 退出出错: {}", e);
                    let _ = self.child.kill();
                    break;
                }
            }
        }
    }
}

/// 检查子进程 stdout 管道是否可读，带截止时间（Windows 用 WaitForSingleObject 等待管道句柄）。
/// 有数据可读或管道关闭时返回 true；超过 deadline 返回 false。
#[cfg(target_os = "windows")]
fn wait_pipe_readable(
    reader: &mut std::io::BufReader<std::process::ChildStdout>,
    deadline: std::time::Instant,
) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    let handle = reader.get_ref().as_raw_handle() as HANDLE;
    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            return false;
        }
        let remaining = deadline - now;
        let wait_ms = remaining.as_millis().min(200) as u32;
        // 管道有数据或已关闭（EOF）时返回 WAIT_OBJECT_0；超时返回 WAIT_TIMEOUT
        let ret = unsafe { WaitForSingleObject(handle, wait_ms) };
        if ret == WAIT_OBJECT_0 {
            return true;
        }
        // WAIT_TIMEOUT 或其它错误：继续循环直到 deadline
    }
}

#[cfg(not(target_os = "windows"))]
fn wait_pipe_readable(
    _reader: &mut std::io::BufReader<std::process::ChildStdout>,
    _deadline: std::time::Instant,
) -> bool {
    true
}

/// 全局传感器桥接
pub static SENSOR_BRIDGE: Mutex<Option<SensorBridge>> = Mutex::new(None);

/// LHML 是否至少成功读取过一次传感器。
/// 静态硬件缓存用它判断 NexBoxMonitor 是否已就绪：
/// 未就绪时构建的 GPU 列表可能是 WMI 兜底/NVML 单来源，不写入缓存，等就绪后重建。
pub static LHM_EVER_SUCCEEDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 重启冷却（秒）：子进程崩溃后至少等待 30s 才尝试重启
const RESTART_COOLDOWN_SECS: u64 = 30;
/// 连续重启次数上限：超过此次数后不再自动重启
const MAX_RESTART_ATTEMPTS: u32 = 5;
static LAST_RESTART_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static RESTART_ATTEMPT_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub static SENSOR_CHILD_EXE_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// 启动传感器子进程
pub fn start_sensor_process(app: &App) {
    match spawn_sensor() {
        Ok(Some(bridge)) => {
            log::info!("已启动 NexBoxMonitor 子进程 (pid={})", bridge.child.id());
            *SENSOR_BRIDGE.lock().unwrap() = Some(bridge);
            app.manage(SensorChild(Mutex::new(None))); // 保持兼容
        }
        Ok(None) => {
            log::info!("NexBoxMonitor 未找到，跳过启动");
            app.manage(SensorChild(Mutex::new(None)));
        }
        Err(e) => {
            log::warn!("启动 NexBoxMonitor 失败: {e}");
            app.manage(SensorChild(Mutex::new(None)));
        }
    }
}

/// 停止传感器子进程
pub fn stop_sensor_process(app: &AppHandle) {
    // 先处理旧的 SensorChild（兼容）
    if let Some(state) = app.try_state::<SensorChild>() {
        let child = {
            let mut guard = state
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.take()
        };
        if let Some(mut child) = child {
            log::info!("正在停止传感器子进程 (pid={})", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    // 关闭新的 SensorBridge
    if let Some(mut bridge) = SENSOR_BRIDGE.lock().unwrap().take() {
        log::info!("正在关闭 NexBoxMonitor 子进程 (pid={})", bridge.child.id());
        bridge.shutdown();
    }
}

#[tauri::command]
pub async fn get_lhm_cpu_load() -> Result<Option<u16>, String> {
    match read_lhm_sensors() {
        Ok(response) => {
            for s in &response.sensors {
                if s.hardware_type.eq_ignore_ascii_case("CPU")
                    && s.sensor_type == "Load"
                    && (s.name == "CPU Total" || s.name == "Total")
                {
                    return Ok(Some(s.value as u16));
                }
            }
            Ok(None)
        }
        Err(e) => {
            if e.contains("尚未就绪") {
                log::debug!("LHML CPU load 读取跳过: {e}");
            } else {
                log::warn!("LHML CPU load 读取失败: {e}");
            }
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn get_lhm_cpu_status() -> Result<(Option<u16>, Option<f64>), String> {
    match read_lhm_sensors() {
        Ok(response) => {
            let mut load = None;
            let mut temp = None;
            for s in &response.sensors {
                if s.hardware_type.eq_ignore_ascii_case("CPU") {
                    if s.sensor_type == "Load" && (s.name == "CPU Total" || s.name == "Total") {
                        load = Some(s.value as u16);
                    }
                }
                // 温度可能来自 CPU/SuperIO/Motherboard 硬件类型（老AMD A系列通过SuperIO报告）
                if s.sensor_type == "Temperature"
                    && (s.hardware_type.eq_ignore_ascii_case("CPU")
                        || s.hardware_type.eq_ignore_ascii_case("SuperIO")
                        || s.hardware_type.eq_ignore_ascii_case("Motherboard"))
                    && (s.name == "Core (Tctl/Tdie)" || s.name == "CPU Package"
                        || s.name == "Tctl" || s.name == "Core"
                        || s.name == "CPU" || s.name == "CPU Core"
                        || s.name == "CPU Temperature")
                {
                    if temp.is_none() {
                        temp = Some(s.value);
                    }
                }
            }
            Ok((load, temp))
        }
        Err(e) => {
            if e.contains("尚未就绪") {
                log::debug!("LHML CPU status 读取跳过: {e}");
            } else {
                log::warn!("LHML CPU status 读取失败: {e}");
            }
            Ok((None, None))
        }
    }
}

#[tauri::command]
pub async fn get_lhm_gpu_status() -> Result<Vec<(Option<f64>, Option<u32>)>, String> {
    match read_lhm_sensors() {
        Ok(response) => {
            let gpu_hardware_types: Vec<_> = {
                let mut types: Vec<_> = response.sensors.iter()
                    .filter(|s| s.hardware_type.to_lowercase().starts_with("gpu"))
                    .map(|s| s.hardware_type.clone())
                    .collect();
                types.dedup();
                types
            };

            // 判断是否存在 NVIDIA 独显（仅 NVIDIA 明确是独显，AMD 可能是 APU 核显）
            let has_nvidia = gpu_hardware_types.iter().any(|t| {
                t.eq_ignore_ascii_case("GpuNvidia")
            });

            let mut results = Vec::new();
            for hw_type in &gpu_hardware_types {
                // NVIDIA 独显存在时跳过 Intel 核显，AMD 核显保留显示
                if has_nvidia && hw_type.eq_ignore_ascii_case("GpuIntel") {
                    log::info!("跳过核显(LHML): 存在 NVIDIA 独显，忽略 GpuIntel");
                    continue;
                }
                let temp = response.sensors.iter()
                    .filter(|s| s.hardware_type == *hw_type && s.sensor_type == "Temperature"
                        && (s.name == "GPU Core" || s.name == "GPU" || s.name == "Core" || s.name == "GPU Temperature"))
                    .map(|s| s.value)
                    .next();
                let usage = response.sensors.iter()
                    .filter(|s| s.hardware_type == *hw_type && s.sensor_type == "Load"
                        && (s.name == "GPU Core" || s.name == "D3D 3D" || s.name == "GPU"
                            || s.name == "D3D Usage" || s.name == "Core"))
                    .map(|s| s.value as u32)
                    .next();
                results.push((temp, usage));
            }
            Ok(results)
        }
        Err(e) => {
            if e.contains("尚未就绪") {
                log::debug!("LHML GPU status 读取跳过: {e}");
            } else {
                log::warn!("LHML GPU status 读取失败: {e}");
            }
            Ok(Vec::new())
        }
    }
}

/// 从 LHML 读取传感器数据（供 overlay_panel.rs 调用）
pub fn read_lhm_sensors() -> Result<SensorsResponse, String> {
    let mut guard = SENSOR_BRIDGE
        .lock()
        .map_err(|e| format!("锁获取失败: {}", e))?;

    match guard.as_mut() {
        Some(bridge) => {
            if !bridge.is_alive() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let last = LAST_RESTART_TIME.load(std::sync::atomic::Ordering::Relaxed);
                let attempts = RESTART_ATTEMPT_COUNT.load(std::sync::atomic::Ordering::Relaxed);

                if attempts >= MAX_RESTART_ATTEMPTS {
                    log::warn!("NexBoxMonitor 子进程已退出，但已连续重启失败 {attempts} 次，放弃自动重启");
                    *guard = None;
                    return Err("NexBoxMonitor 连续崩溃，已禁用".to_string());
                }

                if now - last < RESTART_COOLDOWN_SECS {
                    log::warn!(
                        "NexBoxMonitor 子进程已退出，冷却中（距上次重启 {}s，需等待 {}s）",
                        now - last,
                        RESTART_COOLDOWN_SECS - (now - last)
                    );
                    *guard = None;
                    return Err(format!("NexBoxMonitor 冷却中，请稍后重试"));
                }

                log::warn!("NexBoxMonitor 子进程已退出，尝试重启（第 {} 次）...", attempts + 1);
                *guard = None;
                LAST_RESTART_TIME.store(now, std::sync::atomic::Ordering::Relaxed);
                RESTART_ATTEMPT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                drop(guard);
                match spawn_sensor() {
                    Ok(Some(new_bridge)) => {
                        log::info!("NexBoxMonitor 重启成功 (pid={})", new_bridge.child.id());
                        RESTART_ATTEMPT_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
                        *SENSOR_BRIDGE.lock().unwrap() = Some(new_bridge);
                        return Err("子进程已重启，请重试".to_string());
                    }
                    _ => return Err("NexBoxMonitor 不可用".to_string()),
                }
            }
            // 重启成功或进程正常，重置计数
            RESTART_ATTEMPT_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
            let mut response = match bridge.read_sensors() {
                Ok(resp) => resp,
                Err(e) => {
                    // 子进程卡死（读取超时）/ 空响应 / EOF：强制重启，避免管道阻塞导致永久无数据
                    if e.contains("超时") || e.contains("空响应") || e.contains("EOF") {
                        log::warn!("NexBoxMonitor 读取异常，强制重启子进程: {e}");
                        let _ = bridge.child.kill();
                        let _ = bridge.child.wait();
                        // 清空 bridge，让下次调用重新拉起子进程
                        *guard = None;
                        RESTART_ATTEMPT_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
                        LAST_RESTART_TIME.store(0, std::sync::atomic::Ordering::Relaxed);
                        return Err(format!("NexBoxMonitor 读取异常，已重启子进程: {e}"));
                    }
                    return Err(e);
                }
            };
            // 过滤掉虚拟内存传感器（"全部传感器"列表中不展示）
            response.sensors.retain(|s| {
                let name = s.name.to_lowercase();
                let hw = s.hardware.to_lowercase();
                let hw_type = s.hardware_type.to_lowercase();
                !(name.contains("virtual") || hw.contains("virtual") || hw_type.contains("virtual"))
            });
            LHM_EVER_SUCCEEDED.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(response)
        }
        None => {
            // 尝试启动
            drop(guard);
            match spawn_sensor() {
                Ok(Some(bridge)) => {
                    log::info!("延迟启动 NexBoxMonitor (pid={})", bridge.child.id());
                    *SENSOR_BRIDGE.lock().unwrap() = Some(bridge);
                    Err("子进程已启动，请重试".to_string())
                }
                _ => Err("NexBoxMonitor 不可用".to_string()),
            }
        }
    }
}

/// 查找 NexBoxMonitor.exe 路径
fn find_monitor_exe() -> Option<std::path::PathBuf> {
    // 获取 exe 所在目录作为基准
    let exe_dir = std::env::current_exe()
        .ok()?
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or(std::path::PathBuf::from("."));

    let exe_name = "NexBoxMonitor.exe";

    // 定义所有要探测的候选路径生成器
    // 参数 base: 基准目录, sub: 子路径后缀
    let candidates: Vec<std::path::PathBuf> = {
        let mut list = Vec::new();
        let suffixes = [
            // 方案A: 安装版 — {app}/monitor/NexBoxMonitor.exe
            // Inno Setup 把 publish/* 复制到 {app}/monitor/
            "monitor",
            // 方案B: 用户描述的路径 — {app}/NexBox/monitor/NexBoxMonitor.exe
            "NexBox/monitor",
            // 方案C: Tauri 资源目录
            "resources/monitor",
            // 方案D: 旧版开发者构建路径
            "monitor/bin/Release/net48",
            // 方案E: publish 输出
            "monitor/bin/Release/net48",
        ];
        for suffix in &suffixes {
            let p = exe_dir.join(suffix).join(exe_name);
            list.push(p);
        }
        list
    };

    // 一次性检查所有候选路径
    for path in &candidates {
        if path.exists() {
            log::info!("找到 NexBoxMonitor: {}", path.display());
            return Some(path.clone());
        }
    }

    // 方案F: 从 exe 目录向上回溯，查找项目根目录下的 monitor 构建产物
    // 开发环境下 exe 位于 src-tauri/target/{debug,release}/nexbox.exe
    let mut probe = exe_dir.clone();
    for _ in 0..5 {
        // 每层尝试: monitor/bin/Release/net48
        let p1 = probe
            .join("monitor")
            .join("bin")
            .join("Release")
            .join("net48")
            .join(exe_name);
        if p1.exists() {
            log::info!("找到 NexBoxMonitor (probe): {}", p1.display());
            return Some(p1);
        }
        if !probe.pop() {
            break;
        }
    }

    // 方案G: 通过 CARGO_MANIFEST_DIR 编译期路径（仅 debug 构建，纯备用）
    #[cfg(debug_assertions)]
    {
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
        let dev_path = base
            .join("monitor")
            .join("bin")
            .join("Release")
            .join("net48")
            .join(exe_name);
        if dev_path.exists() {
            log::info!("找到 NexBoxMonitor (dev): {}", dev_path.display());
            return Some(dev_path);
        }
        let pub_path = base
            .join("monitor")
            .join("bin")
            .join("Release")
            .join("net48")
            .join(exe_name);
        if pub_path.exists() {
            log::info!("找到 NexBoxMonitor (publish): {}", pub_path.display());
            return Some(pub_path);
        }
    }

    log::warn!(
        "未找到 NexBoxMonitor.exe (exe_dir: {}), 已尝试路径: {:?}",
        exe_dir.display(),
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()
    );
    None
}

fn spawn_sensor() -> std::io::Result<Option<SensorBridge>> {
    let exe_path = match find_monitor_exe() {
        Some(p) => {
            // 缓存路径供重启使用
            *SENSOR_CHILD_EXE_PATH.lock().unwrap() = Some(p.clone());
            p
        }
        None => return Ok(None),
    };

    let mut cmd = Command::new(&exe_path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Windows 下隐藏控制台窗口
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn()?;

    let stdin = child.stdin.take().expect("无法获取子进程 stdin");
    let stdout = child.stdout.take().expect("无法获取子进程 stdout");

    // 捕获子进程 stderr 到日志，避免管道积压阻塞，也便于排查子进程异常
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut reader = std::io::BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => log::info!("[NexBoxMonitor] {}", line.trim()),
                    Err(_) => break,
                }
            }
        });
    }

    let bridge = SensorBridge {
        child,
        reader: BufReader::new(stdout),
        writer: stdin,
        started_at: std::time::Instant::now(),
    };

    Ok(Some(bridge))
}

/// 重启 NexBoxMonitor 子进程（内部函数，供 pawnio_driver 调用）
/// 安装 PawnIO 后调用，让新进程加载驱动
pub fn restart_sensor_process_internal() -> Result<(), String> {
    log::info!("[restart_monitor] 开始重启 NexBoxMonitor 子进程");

    // 1. 强制终止旧的子进程（用 kill 而非 shutdown，更快更可靠）
    if let Some(mut bridge) = SENSOR_BRIDGE.lock().unwrap().take() {
        log::info!("[restart_monitor] 正在终止旧进程 (pid={})", bridge.child.id());
        let _ = bridge.child.kill();
        let _ = bridge.child.wait();
        log::info!("[restart_monitor] 旧进程已终止");
    } else {
        log::info!("[restart_monitor] 没有正在运行的监控进程，直接启动新进程");
    }

    // 2. 重置重启计数器
    RESTART_ATTEMPT_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
    LAST_RESTART_TIME.store(0, std::sync::atomic::Ordering::Relaxed);

    // 3. 尝试启动新进程（最多重试 2 次）
    for attempt in 0..2 {
        match spawn_sensor() {
            Ok(Some(bridge)) => {
                log::info!(
                    "[restart_monitor] NexBoxMonitor 重启成功 (pid={}, attempt={})",
                    bridge.child.id(),
                    attempt + 1
                );
                *SENSOR_BRIDGE.lock().unwrap() = Some(bridge);
                return Ok(());
            }
            Ok(None) => {
                log::warn!("[restart_monitor] spawn_sensor 返回 None (第{}次)", attempt + 1);
                // 尝试使用缓存的路径直接启动
                let cached = SENSOR_CHILD_EXE_PATH.lock().unwrap().clone();
                if let Some(ref exe_path) = cached {
                    log::info!("[restart_monitor] 使用缓存路径重试: {}", exe_path.display());
                    let mut cmd = Command::new(exe_path);
                    cmd.stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x08000000);
                    }
                    match cmd.spawn() {
                        Ok(mut child) => {
                            let stdin = child.stdin.take().expect("无法获取子进程 stdin");
                            let stdout = child.stdout.take().expect("无法获取子进程 stdout");
                            let bridge = SensorBridge {
                                child,
                                reader: BufReader::new(stdout),
                                writer: stdin,
                                started_at: std::time::Instant::now(),
                            };
                            log::info!(
                                "[restart_monitor] 通过缓存路径重启成功 (pid={})",
                                bridge.child.id()
                            );
                            *SENSOR_BRIDGE.lock().unwrap() = Some(bridge);
                            return Ok(());
                        }
                        Err(e) => log::error!("[restart_monitor] 缓存路径启动失败: {}", e),
                    }
                } else {
                    log::warn!("[restart_monitor] 没有缓存的 exe 路径");
                }
            }
            Err(e) => {
                log::error!("[restart_monitor] spawn_sensor 出错 (第{}次): {}", attempt + 1, e);
            }
        }
        // 两次尝试之间等待 500ms
        if attempt == 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }

    let err = "NexBoxMonitor 重启失败：无法找到或启动监控进程".to_string();
    log::error!("[restart_monitor] {}", err);
    Err(err)
}

/// 重启 NexBoxMonitor 子进程（Tauri 命令，供前端调用）
#[tauri::command]
pub async fn restart_monitor_process() -> Result<String, String> {
    restart_sensor_process_internal()
        .map(|_| "监控进程已重启".to_string())
        .map_err(|e| format!("重启监控进程失败: {}", e))
}