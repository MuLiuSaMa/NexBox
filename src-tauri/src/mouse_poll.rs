//! 鼠标轮询率测试
//!
//! 浏览器 mousemove 受屏幕刷新率限制（60~165Hz 封顶），测不出真实轮询率，
//! 因此用 WH_MOUSE_LL 全局低级鼠标钩子在 Rust 侧统计系统收到的鼠标移动上报流：
//! - 钩子回调只做一件事：把事件时间戳(µs)写进无锁环形缓冲后立即放行，
//!   回调开销极小，不影响系统输入延迟；
//! - 汇聚线程每 200ms 消费新样本，计算实时轮询率（最近 1s 窗口）、
//!   上报间隔均值/中位/最小/最大、抖动（标准差）、丢帧尖峰数，推送给前端。
//!
//! 间隔 > 100ms 视为「移动停止/恢复」的间隙（gap），不计入统计，
//! 以免暂停移动污染最大间隔与抖动。

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::Emitter;

/// 环形缓冲大小（8kHz×2s 的余量，2 的幂便于取模）
const RING_SIZE: usize = 16384;
/// 超过该间隔视为移动间隙（暂停后恢复），不参与统计
const GAP_US: u64 = 100_000;
/// 间隔 > 1.5 × 最近中位数 记为一次尖峰（丢帧/卡顿）
const SPIKE_FACTOR: f64 = 1.5;
/// 中位数采样窗口：最近 8000 个间隔（8kHz 下约 1s）
const MEDIAN_WINDOW: usize = 8000;

static POLLING: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
/// 钩子线程 id（线程内登记，用于投递 WM_QUIT 唤醒阻塞的消息循环）
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// 进程级计时锚点（回调内只做原子读，避免在热路径上加锁）
static ANCHOR: OnceLock<std::time::Instant> = OnceLock::new();
/// 本次会话起点相对锚点的 µs 偏移
static START_OFFSET_US: AtomicU64 = AtomicU64::new(0);

/// 事件时间戳环形缓冲（SPSC：钩子回调单写，汇聚线程单读）
struct Ring {
    ts: Box<[AtomicU64]>,
    write_idx: AtomicU64,
}

static RING: OnceLock<Ring> = OnceLock::new();

fn ring() -> &'static Ring {
    RING.get_or_init(|| Ring {
        ts: (0..RING_SIZE)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        write_idx: AtomicU64::new(0),
    })
}

static HOOK_HANDLE: OnceLock<Mutex<Option<usize>>> = OnceLock::new();
static HOOK_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
static COLLECTOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

fn hook_slot() -> &'static Mutex<Option<usize>> {
    HOOK_HANDLE.get_or_init(|| Mutex::new(None))
}

/// 推送给前端的统计快照（None = 样本不足，前端显示等待状态）
#[derive(serde::Serialize, Clone)]
pub struct PollStats {
    pub moving: bool,
    pub hz: Option<f64>,
    pub total: u64,
    pub avg_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub spikes: u64,
}

/// 开始一次新的测试（幂等：先停掉旧会话再开新的，保证统计干净）
pub fn start(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        imp::start(app)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        Err("鼠标轮询率测试仅支持 Windows".to_string())
    }
}

/// 停止测试并卸载钩子（幂等）
pub fn stop() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        imp::stop()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

/// 退出清理
pub fn cleanup() {
    let _ = stop();
}

#[tauri::command]
pub async fn mouse_poll_start(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || start(app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn mouse_poll_stop() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(stop)
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::collections::VecDeque;
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, HHOOK, MSG, WH_MOUSE_LL, WM_MOUSEMOVE, WM_QUIT,
    };

    /// 低层鼠标钩子回调：只记录时间戳，其余全部放行。
    /// 系统派发输入事件会同步等待钩子返回，回调必须尽快结束；
    /// LL 钩子回调在本线程上被串行调用，写索引用「存时间戳 → 存索引」发布。
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && wparam as u32 == WM_MOUSEMOVE && POLLING.load(Ordering::Relaxed) {
            if let Some(anchor) = ANCHOR.get() {
                let r = ring();
                let i = r.write_idx.load(Ordering::Relaxed);
                let idx = (i as usize) & (RING_SIZE - 1);
                let ts = anchor.elapsed().as_micros() as u64 - START_OFFSET_US.load(Ordering::Relaxed);
                r.ts[idx].store(ts, Ordering::Relaxed);
                r.write_idx.store(i.wrapping_add(1), Ordering::Release);
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    /// 钩子线程体：安装钩子后进入阻塞式消息循环。
    /// 低层钩子回调只在线程泵消息时被调用，必须用 GetMessageW 阻塞等待，
    /// 不能 Peek+sleep 轮询（sleep 期间全系统输入会被积压，表现为光标卡顿）。
    fn hook_thread(tx: std::sync::mpsc::Sender<(u32, bool)>) {
        unsafe {
            let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), std::ptr::null_mut(), 0);
            if hook.is_null() {
                let _ = tx.send((0, false));
                log::error!("[MousePoll] 安装鼠标低层钩子失败");
                return;
            }
            *hook_slot().lock().unwrap() = Some(hook as usize);
            HOOK_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
            let _ = tx.send((GetCurrentThreadId(), true));
            log::info!("[MousePoll] 低层鼠标钩子已安装");

            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // 收到 WM_QUIT；若 stop() 已主动卸过钩子，这里重复卸载只会安全地返回失败
            UnhookWindowsHookEx(hook);
            *hook_slot().lock().unwrap() = None;
            HOOK_THREAD_ID.store(0, Ordering::SeqCst);
            log::info!("[MousePoll] 低层鼠标钩子已卸载");
        }
    }

    /// 汇聚线程：每 200ms 消费环形缓冲新样本，计算统计并推送前端。
    /// 会话统计由本线程独占，无需加锁。
    fn collector_loop(app: tauri::AppHandle) {
        let r = ring();
        let mut next_read = r.write_idx.load(Ordering::Acquire);
        // 会话累计（间隔单位 µs）
        let mut total: u64 = 0;
        let mut sum: f64 = 0.0;
        let mut min_us = u64::MAX;
        let mut max_us: u64 = 0;
        // Welford 在线标准差
        let mut mean: f64 = 0.0;
        let mut m2: f64 = 0.0;
        let mut spikes: u64 = 0;
        // 最近间隔样本（中位数窗口）
        let mut recent: VecDeque<f64> = VecDeque::with_capacity(MEDIAN_WINDOW + 1);
        // 最近 1s 事件时间戳（窗口实时 Hz）
        let mut window: VecDeque<u64> = VecDeque::new();
        let mut last_ts: Option<u64> = None;
        let mut idle_cycles: u32 = u32::MAX; // u32::MAX = 从未移动
        let sleeper = spin_sleep::SpinSleeper::default();

        loop {
            if STOP.load(Ordering::SeqCst) {
                break;
            }
            sleeper.sleep(Duration::from_millis(200));
            if STOP.load(Ordering::SeqCst) {
                break;
            }

            let write = r.write_idx.load(Ordering::Acquire);
            // 溢出保护：写入领先超过一圈时跳到最旧的有效槽（8kHz 下 200ms 至多 1600 条，远小于缓冲）
            if write.wrapping_sub(next_read) >= RING_SIZE as u64 {
                next_read = write - (RING_SIZE as u64 - 1);
            }
            let mut cycle_intervals: Vec<f64> = Vec::new();
            let mut new_events: u64 = 0;
            while next_read < write {
                let ts = r.ts[(next_read as usize) & (RING_SIZE - 1)].load(Ordering::Relaxed);
                next_read += 1;
                new_events += 1;
                if let Some(prev) = last_ts {
                    if ts.saturating_sub(prev) > GAP_US {
                        // 移动间隙：窗口清零，间隔不计入统计
                        window.clear();
                    } else {
                        let x = (ts - prev) as f64;
                        total += 1;
                        sum += x;
                        min_us = min_us.min(ts - prev);
                        max_us = max_us.max(ts - prev);
                        let d = x - mean;
                        mean += d / total as f64;
                        m2 += d * (x - mean);
                        cycle_intervals.push(x);
                    }
                }
                last_ts = Some(ts);
                window.push_back(ts);
                while let Some(front) = window.front() {
                    if ts.saturating_sub(*front) > 1_000_000 {
                        window.pop_front();
                    } else {
                        break;
                    }
                }
            }

            // 先并入中位数窗口再计算，保证尖峰判定用上最新样本
            for &x in &cycle_intervals {
                recent.push_back(x);
                if recent.len() > MEDIAN_WINDOW {
                    recent.pop_front();
                }
            }
            let median = if recent.is_empty() {
                None
            } else {
                let mut v: Vec<f64> = recent.iter().copied().collect();
                v.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
                let n = v.len();
                Some(if n % 2 == 1 {
                    v[n / 2]
                } else {
                    (v[n / 2 - 1] + v[n / 2]) / 2.0
                })
            };
            if let Some(m) = median {
                for &x in &cycle_intervals {
                    if x > m * SPIKE_FACTOR {
                        spikes += 1;
                    }
                }
            }

            idle_cycles = if new_events > 0 {
                0
            } else {
                idle_cycles.saturating_add(1)
            };
            let moving = idle_cycles <= 2;

            let hz = if moving && window.len() >= 2 {
                let front = *window.front().unwrap() as f64;
                let back = *window.back().unwrap() as f64;
                if back > front {
                    Some((window.len() - 1) as f64 * 1e6 / (back - front))
                } else {
                    None
                }
            } else {
                None
            };

            let payload = PollStats {
                moving,
                hz,
                total,
                avg_ms: if total > 0 {
                    Some(sum / total as f64 / 1000.0)
                } else {
                    None
                },
                median_ms: median.map(|m| m / 1000.0),
                min_ms: if total > 0 {
                    Some(min_us as f64 / 1000.0)
                } else {
                    None
                },
                max_ms: if total > 0 {
                    Some(max_us as f64 / 1000.0)
                } else {
                    None
                },
                jitter_ms: if total > 1 {
                    Some((m2 / (total - 1) as f64).sqrt() / 1000.0)
                } else {
                    None
                },
                spikes,
            };
            let _ = app.emit("mouse-poll-stats", payload);
        }
    }

    pub fn start(app: tauri::AppHandle) -> Result<(), String> {
        let _ = stop();

        // 重置会话状态
        let anchor = *ANCHOR.get_or_init(std::time::Instant::now);
        START_OFFSET_US.store(anchor.elapsed().as_micros() as u64, Ordering::SeqCst);
        ring().write_idx.store(0, Ordering::SeqCst);
        STOP.store(false, Ordering::SeqCst);
        POLLING.store(true, Ordering::SeqCst);

        // 钩子线程（同步等待安装结果）
        let (tx, rx) = std::sync::mpsc::channel::<(u32, bool)>();
        let hook_handle = std::thread::spawn(move || hook_thread(tx));
        let (_tid, installed) = rx
            .recv_timeout(Duration::from_millis(2000))
            .map_err(|_| "mouse poll hook thread failed to start".to_string())?;
        if !installed {
            POLLING.store(false, Ordering::SeqCst);
            if let Some(h) = HOOK_THREAD.lock().unwrap().take() {
                let _ = h.join();
            }
            return Err("安装鼠标低层钩子失败".to_string());
        }
        *HOOK_THREAD.lock().unwrap() = Some(hook_handle);

        // 汇聚线程
        let collector = std::thread::spawn(move || collector_loop(app));
        *COLLECTOR_THREAD.lock().unwrap() = Some(collector);
        log::info!("[MousePoll] 测试已开始");
        Ok(())
    }

    pub fn stop() -> Result<(), String> {
        POLLING.store(false, Ordering::SeqCst);
        STOP.store(true, Ordering::SeqCst);

        // 主动卸钩（钩子线程退出时会再次卸载，幂等）
        if let Ok(mut slot) = hook_slot().lock() {
            if let Some(hook) = slot.take() {
                unsafe {
                    UnhookWindowsHookEx(hook as HHOOK);
                }
            }
        }

        // 投递 WM_QUIT 唤醒阻塞在 GetMessageW 的钩子线程
        let tid = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if tid != 0 {
            for _ in 0..250 {
                if unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) } != 0 {
                    break;
                }
                if HOOK_THREAD_ID.load(Ordering::SeqCst) == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        if let Some(h) = COLLECTOR_THREAD.lock().unwrap().take() {
            let _ = h.join();
        }
        if let Some(h) = HOOK_THREAD.lock().unwrap().take() {
            let _ = h.join();
        }
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;

    pub fn start(_app: tauri::AppHandle) -> Result<(), String> {
        Err("鼠标轮询率测试仅支持 Windows".to_string())
    }

    pub fn stop() -> Result<(), String> {
        Ok(())
    }
}
