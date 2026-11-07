//! 磁盘测速（硬盘健康页「测速」板块）— CrystalDiskMark 式全矩阵
//!
//! 口径：FILE_FLAG_NO_BUFFERING + WRITE_THROUGH + SEQUENTIAL_SCAN 无缓冲直 IO，
//! 测试文件落在所选物理硬盘的分区上。
//! 测试矩阵：
//! - 顺序 1MiB：写 2 遍 / 读 2 遍 → 带宽 + IOPS（1MiB 块）
//! - 随机 4K（Q1T1）：按时间窗（各 4s）随机 4096 块读写 → 带宽 + IOPS
//! - 访问时间：由随机 4K 循环推导（平均单次 IO 耗时 = 耗时 ÷ 操作数），机械盘≈寻道、SSD≈裸延迟
//! 无缓冲 IO 要求缓冲地址与读写大小按扇区（4096）对齐。
//! 任务骨架（RUNNING/STOP 守卫 + 事件进度 + stop/cleanup）与 speedtest.rs 一致。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

const PROGRESS_EVENT: &str = "disk-speedtest:progress";
/// 顺序块大小：1MiB（对齐 CDM SEQ1M 口径；4096 的倍数满足无缓冲对齐）
const SEQ_CHUNK: usize = 1024 * 1024;
/// 随机块大小：4KiB
const RND_CHUNK: usize = 4096;
/// 缓冲对齐（无缓冲 IO 要求按扇区对齐）
const BUFFER_ALIGN: usize = 4096;
/// 顺序测试每个方向遍数
const SEQ_PASSES: usize = 2;
/// 随机 4K 每个方向的时间窗
const RND_DURATION: Duration = Duration::from_secs(4);
/// 候选测试文件大小：按剩余空间从大到小取第一个放得下的档位
const SIZE_CANDIDATES: [u64; 3] = [
    1024 * 1024 * 1024,
    512 * 1024 * 1024,
    256 * 1024 * 1024,
];
/// 测试文件之外预留的空间余量
const FREE_MARGIN: u64 = 256 * 1024 * 1024;

static TEST_RUNNING: AtomicBool = AtomicBool::new(false);
static TEST_STOP: AtomicBool = AtomicBool::new(false);
/// 当前测试临时文件（取消 / 退出时兜底删除）
static CURRENT_TEMP_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

// ===== 数据结构 =====

/// 全矩阵结果：done 事件一次带齐；其余阶段这些字段为 0
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpeedResult {
    /// 顺序带宽 MB/s（十进制 1e6）
    pub seq_write_mbps: f64,
    pub seq_read_mbps: f64,
    /// 顺序 IOPS（1MiB 块）
    pub seq_write_iops: f64,
    pub seq_read_iops: f64,
    /// 随机 4K 带宽 MB/s
    pub rnd_write_mbps: f64,
    pub rnd_read_mbps: f64,
    /// 随机 4K IOPS
    pub rnd_write_iops: f64,
    pub rnd_read_iops: f64,
    /// 平均访问时间 ms（由随机 4K 循环推导）
    pub lat_write_ms: f64,
    pub lat_read_ms: f64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpeedProgress {
    /// 当前阶段：prepare / seq_write / seq_read / rnd_write / rnd_read / done / cancelled / error
    pub stage: String,
    pub drive: String,
    /// 整体进度 0-100（顺序写 0-30 / 顺序读 30-60 / 随机写 60-80 / 随机读 80-100）
    pub progress_pct: f64,
    /// 当前阶段实时带宽 MB/s
    pub current_mbps: f64,
    #[serde(flatten)]
    pub result: DiskSpeedResult,
    pub message: String,
}

fn emit(app: &AppHandle, stage: &str, drive: &str, pct: f64, current_mbps: f64, message: &str, result: &DiskSpeedResult) {
    let _ = app.emit(
        PROGRESS_EVENT,
        DiskSpeedProgress {
            stage: stage.to_string(),
            drive: drive.to_string(),
            progress_pct: pct,
            current_mbps,
            result: result.clone(),
            message: message.to_string(),
        },
    );
}

/// 盘符 normalize：接受 `C` / `c:` / `C:`，取首个 ASCII 字母并大写。
fn normalize_drive_letter(value: &str) -> Result<char, String> {
    value
        .chars()
        .find(|ch| ch.is_ascii_alphabetic())
        .map(|ch| ch.to_ascii_uppercase())
        .ok_or_else(|| "无效的盘符".to_string())
}

/// 删除测试临时文件并清空登记（幂等）。
fn cleanup_temp() {
    if let Ok(mut guard) = CURRENT_TEMP_PATH.lock() {
        if let Some(path) = guard.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// xorshift64：随机 4K 的偏移源（确定性、无锁，比 rand 轻得多）。
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

// ===== 测速核心（Windows） =====

#[cfg(windows)]
mod bench {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, FILE_FLAG_NO_BUFFERING, FILE_FLAG_SEQUENTIAL_SCAN,
        FILE_FLAG_WRITE_THROUGH,
    };

    /// 目标盘剩余可用字节数。
    fn free_disk_bytes(path: &std::path::Path) -> Option<u64> {
        let wide: Vec<u16> = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut free: u64 = 0;
        let ok = unsafe {
            GetDiskFreeSpaceExW(wide.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), &mut free)
        };
        if ok != 0 { Some(free) } else { None }
    }

    /// 选测试文件大小：按剩余空间从大到小取第一个放得下的档位。
    fn pick_test_size(free: u64) -> Option<u64> {
        SIZE_CANDIDATES
            .into_iter()
            .find(|&size| free >= size + FREE_MARGIN)
    }

    /// 打开测速临时文件：首选盘根目录（无缓冲直写最准），
    /// 失败（如系统盘根目录默认拒绝普通用户写入）回退 `Users\Public`。
    fn open_test_file(drive: char, size: u64) -> Result<(File, PathBuf), String> {
        let candidates = [
            PathBuf::from(format!("{drive}:\\nexbox-disk-speed-test.tmp")),
            PathBuf::from(format!("{drive}:\\Users\\Public\\nexbox-disk-speed-test.tmp")),
        ];
        let mut last_err = String::new();
        for path in &candidates {
            let result = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH | FILE_FLAG_SEQUENTIAL_SCAN)
                .open(path);
            match result {
                Ok(file) => {
                    // 预分配到目标大小，避免测速期间元数据扩展干扰写入速率
                    if let Err(e) = file.set_len(size) {
                        last_err = format!("无法预分配测试文件 {}: {e}", path.display());
                        continue;
                    }
                    return Ok((file, path.clone()));
                }
                Err(e) => last_err = format!("无法创建测试文件 {}: {e}", path.display()),
            }
        }
        Err(last_err)
    }

    /// 分配 align 对齐的缓冲并填充非零递增模式（避免全零内容被存储层特殊处理）。
    fn make_buffer(size: usize) -> Vec<u8> {
        let mut buf = unsafe {
            let layout = std::alloc::Layout::from_size_align(size, BUFFER_ALIGN)
                .expect("缓冲布局非法");
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Vec::from_raw_parts(ptr, size, size)
        };
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        buf
    }

    /// 释放对齐缓冲（对应 make_buffer 的 alloc_zeroed）。
    fn drop_buffer(buf: Vec<u8>) {
        let ptr = buf.as_ptr() as *mut u8;
        let len = buf.len();
        std::mem::forget(buf);
        unsafe {
            let layout = std::alloc::Layout::from_size_align(len, BUFFER_ALIGN).expect("缓冲布局非法");
            std::alloc::dealloc(ptr, layout);
        }
    }

    /// 顺序 1MiB 测速：写/读 SEQ_PASSES 遍，返回 (总字节数, 总耗时)。
    /// 每块之间检查 STOP；进度节流 200ms（与 downloader.rs 同口径）。
    fn run_sequential(
        app: &AppHandle,
        file: &mut File,
        size: u64,
        is_write: bool,
        buf: &mut [u8],
        drive: &str,
        phase_start_pct: f64,
        phase_span_pct: f64,
    ) -> Result<(u64, Duration), String> {
        let mut last_emit = Instant::now();
        let started = Instant::now();
        let stage = if is_write { "seq_write" } else { "seq_read" };
        let msg = if is_write { "顺序写入中" } else { "顺序读取中" };

        for pass in 0..SEQ_PASSES {
            file.seek(SeekFrom::Start(0))
                .map_err(|e| format!("定位失败：{e}"))?;
            let pass_started = Instant::now();
            let mut pass_done: u64 = 0;

            while pass_done < size {
                if TEST_STOP.load(Ordering::SeqCst) {
                    return Err("__cancelled__".to_string());
                }
                if is_write {
                    file.write_all(buf).map_err(|e| format!("写入失败：{e}"))?;
                } else {
                    file.read_exact(buf).map_err(|e| format!("读取失败：{e}"))?;
                }
                pass_done += buf.len() as u64;

                if last_emit.elapsed() >= Duration::from_millis(200) {
                    last_emit = Instant::now();
                    let phase_total = size * SEQ_PASSES as u64;
                    let done_all = pass as u64 * size + pass_done;
                    let pct = phase_start_pct
                        + phase_span_pct * (done_all as f64 / phase_total as f64);
                    // 当前遍的阶段内平均带宽作为实时值
                    let phase_elapsed = pass_started.elapsed().as_secs_f64();
                    let cur = if phase_elapsed > 0.0 {
                        pass_done as f64 / 1_000_000.0 / phase_elapsed
                    } else {
                        0.0
                    };
                    emit(app, stage, drive, pct, cur, msg, &DiskSpeedResult::default());
                }
            }
        }
        Ok((size * SEQ_PASSES as u64, started.elapsed()))
    }

    /// 随机 4K 测速（Q1T1）：在 duration 时间窗内以随机偏移做 4096 块 IO。
    /// 返回 (总字节, 操作数, 总耗时)；访问时间 = 耗时 ÷ 操作数。
    #[allow(clippy::too_many_arguments)]
    fn run_random(
        app: &AppHandle,
        file: &mut File,
        size: u64,
        is_write: bool,
        buf: &mut [u8],
        drive: &str,
        phase_start_pct: f64,
        phase_span_pct: f64,
        duration: Duration,
        seed: u64,
    ) -> Result<(u64, u64, Duration), String> {
        let deadline = Instant::now() + duration;
        let mut rng = Rng(seed | 1);
        let num_blocks = (size / RND_CHUNK as u64).max(1);
        let stage = if is_write { "rnd_write" } else { "rnd_read" };
        let msg = if is_write { "随机写入中" } else { "随机读取中" };

        let started = Instant::now();
        let mut ops: u64 = 0;
        let mut last_emit = Instant::now();

        while Instant::now() < deadline {
            if TEST_STOP.load(Ordering::SeqCst) {
                return Err("__cancelled__".to_string());
            }
            let offset = (rng.next() % num_blocks) * RND_CHUNK as u64;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| format!("定位失败：{e}"))?;
            if is_write {
                file.write_all(buf).map_err(|e| format!("写入失败：{e}"))?;
            } else {
                file.read_exact(buf).map_err(|e| format!("读取失败：{e}"))?;
            }
            ops += 1;

            if last_emit.elapsed() >= Duration::from_millis(200) {
                last_emit = Instant::now();
                let elapsed = started.elapsed().as_secs_f64();
                let pct = phase_start_pct
                    + phase_span_pct * (elapsed / duration.as_secs_f64()).min(1.0);
                let cur = if elapsed > 0.0 {
                    (ops * RND_CHUNK as u64) as f64 / 1_000_000.0 / elapsed
                } else {
                    0.0
                };
                emit(app, stage, drive, pct, cur, msg, &DiskSpeedResult::default());
            }
        }
        Ok((ops * RND_CHUNK as u64, ops, started.elapsed()))
    }

    /// 完整测速流程：顺序写/读 → 随机 4K 写/读 → done 事件。
    /// 所有终止态（done/cancelled/error）由此发出；返回值仅供日志。
    pub fn run_test(app: &AppHandle, drive: char) -> Result<(), String> {
        let drive_str = format!("{drive}:");
        let root = PathBuf::from(format!("{drive}:\\"));
        emit(app, "prepare", &drive_str, 0.0, 0.0, "准备中", &DiskSpeedResult::default());

        let free = free_disk_bytes(&root)
            .ok_or_else(|| format!("无法读取 {drive}: 盘剩余空间"))?;
        let size = pick_test_size(free).ok_or_else(|| {
            format!("{drive}: 盘剩余空间不足（至少需要 {} MB）", (FREE_MARGIN as f64 / 1048576.0) as u64)
        })?;

        let (mut file, temp_path) = open_test_file(drive, size)?;
        *CURRENT_TEMP_PATH.lock().unwrap() = Some(temp_path.clone());

        // 先顺序写满 2 遍再随机测：确保簇已初始化，随机写不受"簇首次写入"额外成本污染
        let mut seq_buf = make_buffer(SEQ_CHUNK);
        let mut rnd_buf = make_buffer(RND_CHUNK);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);

        let result = (|| -> Result<DiskSpeedResult, String> {
            let mut r = DiskSpeedResult::default();

            let (wbytes, wtime) =
                run_sequential(app, &mut file, size, true, &mut seq_buf, &drive_str, 0.0, 30.0)?;
            r.seq_write_mbps = wbytes as f64 / 1_000_000.0 / wtime.as_secs_f64();
            r.seq_write_iops = wbytes as f64 / 1_048_576.0 / wtime.as_secs_f64();

            let (rbytes, rtime) =
                run_sequential(app, &mut file, size, false, &mut seq_buf, &drive_str, 30.0, 30.0)?;
            r.seq_read_mbps = rbytes as f64 / 1_000_000.0 / rtime.as_secs_f64();
            r.seq_read_iops = rbytes as f64 / 1_048_576.0 / rtime.as_secs_f64();

            let (rw_bytes, rw_ops, rw_time) = run_random(
                app, &mut file, size, true, &mut rnd_buf, &drive_str, 60.0, 20.0,
                RND_DURATION, seed,
            )?;
            r.rnd_write_mbps = rw_bytes as f64 / 1_000_000.0 / rw_time.as_secs_f64();
            r.rnd_write_iops = rw_ops as f64 / rw_time.as_secs_f64();
            r.lat_write_ms = rw_time.as_secs_f64() * 1000.0 / rw_ops as f64;

            let (rr_bytes, rr_ops, rr_time) = run_random(
                app, &mut file, size, false, &mut rnd_buf, &drive_str, 80.0, 20.0,
                RND_DURATION, seed ^ 0x9E3779B97F4A7C15,
            )?;
            r.rnd_read_mbps = rr_bytes as f64 / 1_000_000.0 / rr_time.as_secs_f64();
            r.rnd_read_iops = rr_ops as f64 / rr_time.as_secs_f64();
            r.lat_read_ms = rr_time.as_secs_f64() * 1000.0 / rr_ops as f64;

            Ok(r)
        })();

        drop_buffer(seq_buf);
        drop_buffer(rnd_buf);
        drop(file);
        cleanup_temp();

        match result {
            Ok(result) => {
                emit(app, "done", &drive_str, 100.0, 0.0, "完成", &result);
                Ok(())
            }
            Err(e) if e == "__cancelled__" => {
                emit(app, "cancelled", &drive_str, 0.0, 0.0, "已取消", &DiskSpeedResult::default());
                Err(e)
            }
            Err(e) => {
                emit(app, "error", &drive_str, 0.0, 0.0, &e, &DiskSpeedResult::default());
                Err(e)
            }
        }
    }
}

#[cfg(not(windows))]
mod bench {
    use super::*;

    pub fn run_test(_app: &AppHandle, _drive: char) -> Result<(), String> {
        Err("仅支持 Windows".to_string())
    }
}

// ===== Tauri 命令 =====

/// 启动磁盘测速（同一时间仅允许一个任务；进度经
/// `disk-speedtest:progress` 事件推送，全矩阵结果在 stage=done 事件里）。
#[tauri::command]
pub async fn disk_speed_test_start(app: AppHandle, drive_letter: String) -> Result<(), String> {
    if TEST_RUNNING.load(Ordering::SeqCst) {
        return Err("磁盘测速正在进行中".to_string());
    }
    let drive = normalize_drive_letter(&drive_letter)?;

    TEST_RUNNING.store(true, Ordering::SeqCst);
    TEST_STOP.store(false, Ordering::SeqCst);

    let task_app = app.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || bench::run_test(&task_app, drive));

    // 收尾：清运行标志与临时文件；错误/取消事件由 run_test 自己发出，这里只兜 join 异常
    tauri::async_runtime::spawn(async move {
        let result = joined.await;
        TEST_RUNNING.store(false, Ordering::SeqCst);
        if let Ok(Err(e)) = result {
            if e != "__cancelled__" {
                log::warn!("[DiskSpeedTest] 测速失败: {e}");
            }
        }
    });

    Ok(())
}

/// 停止进行中的测速：置位后测速循环在下一块边界中止并清理临时文件。
#[tauri::command]
pub fn disk_speed_test_stop() -> Result<(), String> {
    TEST_STOP.store(true, Ordering::SeqCst);
    Ok(())
}

/// 当前是否有测速在跑（前端兜底轮询用）。
#[tauri::command]
pub fn disk_speed_test_running() -> bool {
    TEST_RUNNING.load(Ordering::SeqCst)
}

/// 退出时兜底：置停止标志并删除残留的测试临时文件。
pub fn cleanup() {
    TEST_STOP.store(true, Ordering::SeqCst);
    if let Ok(mut guard) = CURRENT_TEMP_PATH.lock() {
        if let Some(path) = guard.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}
