use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{future::join_all, StreamExt};
use reqwest::Client;
use tauri::{AppHandle, Emitter};

// ===== 测速服务器配置（纯国内，可在前端切换） =====
// 说明：
// - 南航(NUAA) LibreSpeed 节点：实测可用，作为默认服务器；
// - 浙大(ZJU) 节点：近期持续返回 502 宕机，保留以便其恢复后使用；
// - 中科大(USTC) 节点：实测对非浏览器客户端返回 500 / not ustc，拒绝外部测速，仅作备选；
// - 下载测速在所选节点失败时自动切换到下方国内大厂 CDN 备用源（保障下载始终可测）；
// - 上传/延迟目前只有南航 empty.php 对外开放，故所有服务器统一走南航节点测上传与延迟。
struct ServerConfig {
    id: &'static str,
    name: &'static str,
    download_urls: &'static [&'static str],
    upload_urls: &'static [&'static str],
    ping_urls: &'static [&'static str],
}

const SERVERS: &[ServerConfig] = &[
    ServerConfig {
        id: "nuaa",
        name: "南京航空航天大学",
        download_urls: &[
            "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=50",
            "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=10",
            "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
            "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
            "http://speed.nuaa.edu.cn/backend/garbage.php?ckSize=10",
        ],
        upload_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
        ping_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
    },
    ServerConfig {
        id: "zju",
        name: "浙江大学",
        download_urls: &[
            "http://speedtest.zju.edu.cn/garbage.php",
            "http://speedtest.zju.edu.cn/1000M",
            "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
            "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
            "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
        ],
        upload_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
        ping_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
    },
    ServerConfig {
        id: "ustc",
        name: "中国科学技术大学",
        download_urls: &[
            "https://test.ustc.edu.cn/backend/garbage.php",
            "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
            "https://dldir1.qq.com/weixin/Windows/WeChatSetup.exe",
            "https://wirelesscdn-download.xuexi.cn/publish/xuexi_android/latest/xuexi_android_10002068.apk",
        ],
        upload_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
        ping_urls: &["http://speed.nuaa.edu.cn/backend/empty.php"],
    },
];

fn default_server() -> &'static ServerConfig {
    &SERVERS[0]
}

fn find_server(id: &str) -> &'static ServerConfig {
    SERVERS
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(default_server)
}

const PING_COUNT: u32 = 8;
const DEFAULT_THREADS: u32 = 16;
const DEFAULT_DURATION_SECS: u32 = 6;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(80);
/// 上传单次请求数据块：成功后按实际发出字节计数；小块可让低带宽下实时速率更平滑
const UPLOAD_CHUNK_SIZE: usize = 256 * 1024;
/// 预热期：TCP 慢启动 + 发送缓冲区填满期间速率虚高，跳过这段时间的显示
const WARMUP_SECS: f64 = 0.8;
/// 上传预热期：多线程并发时会先在本地 TCP 缓冲区堆积大量数据，需要更长时间才回落
const UPLOAD_WARMUP_SECS: f64 = 1.2;
/// 指数移动平均平滑系数
const EMA_ALPHA: f64 = 0.4;

// ===== 全局状态 =====
static TEST_RUNNING: AtomicBool = AtomicBool::new(false);
static TEST_STOP: AtomicBool = AtomicBool::new(false);
static TEST_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);

// ===== 数据结构 =====
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedTestProgress {
    /// 当前阶段: ping / download / upload / done
    pub stage: String,
    pub ping_ms: f64,
    pub jitter_ms: f64,
    pub packet_loss_pct: f64,
    /// 实时下载速度 (Mbps)
    pub download_mbps: f64,
    /// 实时上传速度 (Mbps)
    pub upload_mbps: f64,
    /// 总进度 0-100
    pub progress_pct: f64,
    pub server: String,
    pub message: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedTestServerInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedTestConfig {
    pub threads: Option<u32>,
    pub duration_secs: Option<u32>,
    /// 服务器 id（见 get_speedtest_servers），缺省使用默认服务器
    pub server: Option<String>,
}

impl Default for SpeedTestConfig {
    fn default() -> Self {
        Self {
            threads: Some(DEFAULT_THREADS),
            duration_secs: Some(DEFAULT_DURATION_SECS),
            server: None,
        }
    }
}

/// 获取可切换的测速服务器列表
#[tauri::command]
pub fn get_speedtest_servers() -> Vec<SpeedTestServerInfo> {
    SERVERS
        .iter()
        .map(|s| SpeedTestServerInfo {
            id: s.id.to_string(),
            name: s.name.to_string(),
        })
        .collect()
}

fn now_phase(phase: &str) -> f64 {
    match phase {
        "ping" => 5.0,
        "download" => 50.0,
        "upload" => 90.0,
        _ => 100.0,
    }
}

// 参数对应一次进度上报的完整上下文，拆结构体会改动所有调用点
#[allow(clippy::too_many_arguments)]
fn emit_progress(
    app: &AppHandle,
    stage: &str,
    ping_ms: f64,
    jitter_ms: f64,
    packet_loss_pct: f64,
    download_mbps: f64,
    upload_mbps: f64,
    server: &str,
    message: &str,
) {
    let progress = SpeedTestProgress {
        stage: stage.to_string(),
        ping_ms,
        jitter_ms,
        packet_loss_pct,
        download_mbps,
        upload_mbps,
        progress_pct: now_phase(stage),
        server: server.to_string(),
        message: message.to_string(),
    };
    let _ = app.emit("speedtest-progress", progress);
}

/// 延迟 + 抖动 + 丢包测试（对多个目标并发 HTTP 测 RTT）
async fn run_ping_test(
    client: &Client,
    app: &AppHandle,
    urls: &[String],
    server: &str,
) -> (f64, f64, f64) {
    let mut all_rtts: Vec<f64> = Vec::new();
    let mut failures: u32 = 0;

    for _ in 0..PING_COUNT {
        if TEST_STOP.load(Ordering::SeqCst) {
            break;
        }
        let mut round: Vec<f64> = Vec::new();
        for url in urls {
            let start = Instant::now();
            let result = client
                .get(url)
                .timeout(Duration::from_secs(3))
                .send()
                .await;
            match result {
                Ok(resp) => {
                    // 消费响应体确保连接真正建立
                    let _ = resp.bytes().await;
                    round.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                Err(_) => {
                    failures += 1;
                }
            }
            if TEST_STOP.load(Ordering::SeqCst) {
                break;
            }
        }
        if !round.is_empty() {
            // 取本轮最小 RTT
            let min = round.iter().cloned().fold(f64::INFINITY, f64::min);
            all_rtts.push(min);
        }
        // 间隔 150ms，模拟真实 ping 节奏
        tokio::time::sleep(Duration::from_millis(150)).await;
        emit_progress(app, "ping", 0.0, 0.0, 0.0, 0.0, 0.0, server, "");
    }

    let total = all_rtts.len() as u32 + failures;
    let packet_loss_pct = if total > 0 {
        (failures as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    if all_rtts.is_empty() {
        return (0.0, 0.0, packet_loss_pct);
    }

    // 平均延迟（去掉最高最低，避免毛刺）
    let mut sorted = all_rtts.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let trim = (sorted.len() / 4).max(1);
    let effective = if sorted.len() > trim * 2 {
        &sorted[trim..sorted.len() - trim]
    } else {
        &sorted[..]
    };
    let ping = effective.iter().sum::<f64>() / effective.len() as f64;

    // 抖动 = RTT 标准差
    let mean = ping;
    let variance = effective
        .iter()
        .map(|v| (v - mean).powi(2))
        .sum::<f64>()
        / effective.len() as f64;
    let jitter = variance.sqrt();

    (ping, jitter, packet_loss_pct)
}

/// 下载测速：多线程并发拉取测速文件，实时统计速率
// 参数对应一次测速任务的完整上下文，拆结构体会改动调用方
#[allow(clippy::too_many_arguments)]
async fn run_download_test(
    client: &Client,
    app: &AppHandle,
    urls: &[String],
    server: &str,
    threads: u32,
    duration: Duration,
    ping: f64,
    jitter: f64,
    loss: f64,
) -> f64 {
    let counter = Arc::new(AtomicU64::new(0));
    let url_list = urls.to_vec();

    // 为每个线程分配起始 URL（循环使用），失败时自动切换到下一个源（故障转移）
    let mut handles = Vec::new();
    for i in 0..threads {
        let client = client.clone();
        let counter = counter.clone();
        let urls = url_list.clone();
        handles.push(tokio::spawn(async move {
            let deadline = Instant::now() + duration;
            let mut idx = (i as usize) % urls.len();
            while Instant::now() < deadline {
                if TEST_STOP.load(Ordering::SeqCst) {
                    break;
                }
                let url = &urls[idx];
                let send_fut = client
                    .get(url)
                    .timeout(Duration::from_secs(8))
                    .send();
                let resp = tokio::select! {
                    res = send_fut => res,
                    _ = tokio::time::sleep_until(deadline.into()) => break,
                };
                match resp {
                    Ok(r) => {
                        let mut stream = r.bytes_stream();
                        loop {
                            let chunk = tokio::select! {
                                c = stream.next() => c,
                                _ = tokio::time::sleep_until(deadline.into()) => break,
                            };
                            match chunk {
                                Some(Ok(c)) => {
                                    counter.fetch_add(c.len() as u64, Ordering::Relaxed);
                                }
                                Some(Err(_)) | None => break,
                            }
                            if TEST_STOP.load(Ordering::SeqCst) || Instant::now() >= deadline {
                                break;
                            }
                        }
                    }
                    Err(_) => {
                        // 单个请求失败：切换到下一个 URL 源（故障转移），短暂等待后重试
                        idx = (idx + 1) % urls.len();
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                }
            }
        }));
    }

    let start = Instant::now();
    let mut last_bytes: u64 = 0;
    let mut last_elapsed = 0.0f64;
    let mut smooth_mbps = 0.0f64;
    // 稳定期统计（排除预热期数据）
    let mut stable_base: Option<u64> = None;

    loop {
        // 等待一段时间后采样
        if start.elapsed() >= duration || TEST_STOP.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(PROGRESS_INTERVAL).await;

        let current = counter.load(Ordering::Relaxed);
        let elapsed = start.elapsed().as_secs_f64();
        let delta_secs = elapsed - last_elapsed;
        let delta_bytes = current.saturating_sub(last_bytes);
        if delta_secs > 0.0 {
            let instant_mbps = delta_bytes as f64 * 8.0 / 1_000_000.0 / delta_secs;

            // 进入稳定期：记录基线字节（上次采样点）
            if elapsed >= WARMUP_SECS && stable_base.is_none() {
                stable_base = Some(last_bytes);
            }

            // EMA 平滑抑制毛刺
            smooth_mbps = if smooth_mbps <= 0.0 {
                instant_mbps
            } else {
                smooth_mbps * (1.0 - EMA_ALPHA) + instant_mbps * EMA_ALPHA
            };

            // 预热期内不发前端，避免 TCP 慢启动造成的初始虚高
            if elapsed >= WARMUP_SECS {
                emit_progress(app, "download", ping, jitter, loss, smooth_mbps, 0.0, server, "");
            }
        }
        last_bytes = current;
        last_elapsed = elapsed;
    }

    let _ = join_all(handles).await;

    let total_bytes = counter.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();

    // 最终值：稳定期平均速率（排除预热期数据，避免初始虚高）
    if let Some(base) = stable_base {
        let stable_secs = (elapsed - WARMUP_SECS).max(0.1);
        let stable_bytes = total_bytes.saturating_sub(base);
        if stable_bytes > 0 {
            return stable_bytes as f64 * 8.0 / 1_000_000.0 / stable_secs;
        }
    }
    // 兜底：全时段平均
    if elapsed <= 0.0 {
        return 0.0;
    }
    total_bytes as f64 * 8.0 / 1_000_000.0 / elapsed
}

/// 上传测速：多线程并发流式上传随机数据，边发边计数（避免速率跳变为 0）
// 参数对应一次测速任务的完整上下文，拆结构体会改动调用方
#[allow(clippy::too_many_arguments)]
async fn run_upload_test(
    client: &Client,
    app: &AppHandle,
    urls: &[String],
    server: &str,
    threads: u32,
    duration: Duration,
    ping: f64,
    jitter: f64,
    loss: f64,
    download: f64,
) -> f64 {
    use rand::rngs::StdRng;
    use rand::{RngCore, SeedableRng};
    use reqwest::Body;

    let counter = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for i in 0..threads {
        let client = client.clone();
        let counter = counter.clone();
        let url = urls[(i as usize) % urls.len()].clone();
        handles.push(tokio::spawn(async move {
            let deadline = Instant::now() + duration;
            let mut rng = StdRng::from_entropy();

            while Instant::now() < deadline {
                if TEST_STOP.load(Ordering::SeqCst) {
                    break;
                }

                // 生成一次待发送数据块；仅当请求成功（数据真正发出）后才计入速率，
                // 避免服务器不可达时把本地缓冲中从未发出的数据也算进去导致速率虚高
                let mut buf = vec![0u8; UPLOAD_CHUNK_SIZE];
                rng.fill_bytes(&mut buf);

                // 单请求硬性超时 5s（与 deadline select 竞争，确保线程及时退出）
                let req_fut = client
                    .post(&url)
                    .body(Body::from(buf))
                    .timeout(Duration::from_secs(5))
                    .send();
                let res = tokio::select! {
                    res = req_fut => res,
                    _ = tokio::time::sleep_until(deadline.into()) => {
                        // deadline 到达，丢弃挂起请求，线程立即退出
                        break;
                    }
                };
                if res.is_ok() {
                    counter.fetch_add(UPLOAD_CHUNK_SIZE as u64, Ordering::Relaxed);
                }
            }
        }));
    }

    let start = Instant::now();
    let mut last_bytes: u64 = 0;
    let mut last_elapsed = 0.0f64;
    let mut smooth_mbps = 0.0f64;
    // 稳定期统计（排除预热期的虚高堆积数据）
    let mut stable_base: Option<u64> = None;

    loop {
        if start.elapsed() >= duration || TEST_STOP.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(PROGRESS_INTERVAL).await;

        let current = counter.load(Ordering::Relaxed);
        let elapsed = start.elapsed().as_secs_f64();
        let delta_secs = elapsed - last_elapsed;
        let delta_bytes = current.saturating_sub(last_bytes);
        if delta_secs > 0.0 {
            let instant_mbps = delta_bytes as f64 * 8.0 / 1_000_000.0 / delta_secs;

            // 进入稳定期：记录基线字节（上次采样点），用于最终平均速率
            if elapsed >= UPLOAD_WARMUP_SECS && stable_base.is_none() {
                stable_base = Some(last_bytes);
            }

            // EMA 平滑抑制毛刺（预热期也平滑，但稳定期才显示）
            smooth_mbps = if smooth_mbps <= 0.0 {
                instant_mbps
            } else {
                smooth_mbps * (1.0 - EMA_ALPHA) + instant_mbps * EMA_ALPHA
            };

            // 预热期内不发前端，避免 TCP 发送缓冲区初始堆积造成的虚高
            if elapsed >= UPLOAD_WARMUP_SECS {
                emit_progress(app, "upload", ping, jitter, loss, download, smooth_mbps, server, "");
            }
        }
        last_bytes = current;
        last_elapsed = elapsed;
    }

    let _ = join_all(handles).await;

    let total_bytes = counter.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();

    // 最终值：稳定期平均速率（排除预热期堆积数据，避免虚高）
    if let Some(base) = stable_base {
        let stable_secs = (elapsed - UPLOAD_WARMUP_SECS).max(0.1);
        let stable_bytes = total_bytes.saturating_sub(base);
        if stable_bytes > 0 {
            return stable_bytes as f64 * 8.0 / 1_000_000.0 / stable_secs;
        }
    }
    // 兜底：全时段平均
    if elapsed <= 0.0 {
        return 0.0;
    }
    total_bytes as f64 * 8.0 / 1_000_000.0 / elapsed
}

#[tauri::command]
pub async fn start_speedtest(app: AppHandle, config: Option<SpeedTestConfig>) -> Result<(), String> {
    if TEST_RUNNING.load(Ordering::SeqCst) {
        return Err("测速正在进行中".to_string());
    }

    let cfg = config.unwrap_or_default();
    let threads = cfg.threads.unwrap_or(DEFAULT_THREADS).clamp(1, 16);
    let duration_secs = cfg
        .duration_secs
        .unwrap_or(DEFAULT_DURATION_SECS)
        .clamp(1, 30);

    // 解析目标服务器（缺省使用默认服务器）
    let server_cfg = match &cfg.server {
        Some(id) => find_server(id),
        None => default_server(),
    };
    let server_name = format!("{}测速服务器", server_cfg.name);
    let download_urls: Vec<String> = server_cfg
        .download_urls
        .iter()
        .map(|s| s.to_string())
        .collect();
    let upload_urls: Vec<String> = server_cfg.upload_urls.iter().map(|s| s.to_string()).collect();
    let ping_urls: Vec<String> = server_cfg.ping_urls.iter().map(|s| s.to_string()).collect();

    TEST_RUNNING.store(true, Ordering::SeqCst);
    TEST_STOP.store(false, Ordering::SeqCst);

    let handle = tauri::async_runtime::spawn(async move {
        let server = server_name;
        let client = match Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                emit_progress(
                    &app,
                    "done",
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    &server,
                    &format!("初始化失败: {}", e),
                );
                TEST_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
        };

        // 1. 延迟/抖动/丢包
        emit_progress(&app, "ping", 0.0, 0.0, 0.0, 0.0, 0.0, &server, "正在测试延迟...");
        let (ping, jitter, loss) = run_ping_test(&client, &app, &ping_urls, &server).await;
        if TEST_STOP.load(Ordering::SeqCst) {
            emit_progress(&app, "done", ping, jitter, loss, 0.0, 0.0, &server, "已停止");
            TEST_RUNNING.store(false, Ordering::SeqCst);
            return;
        }

        // 2. 下载测速
        emit_progress(
            &app,
            "download",
            ping,
            jitter,
            loss,
            0.0,
            0.0,
            &server,
            "正在测试下载...",
        );
        let download = run_download_test(
            &client,
            &app,
            &download_urls,
            &server,
            threads,
            Duration::from_secs(duration_secs as u64),
            ping,
            jitter,
            loss,
        )
        .await;
        if TEST_STOP.load(Ordering::SeqCst) {
            emit_progress(&app, "done", ping, jitter, loss, download, 0.0, &server, "已停止");
            TEST_RUNNING.store(false, Ordering::SeqCst);
            return;
        }

        // 3. 上传测速
        emit_progress(
            &app,
            "upload",
            ping,
            jitter,
            loss,
            download,
            0.0,
            &server,
            "正在测试上传...",
        );
        let upload = run_upload_test(
            &client,
            &app,
            &upload_urls,
            &server,
            threads,
            Duration::from_secs(duration_secs as u64),
            ping,
            jitter,
            loss,
            download,
        )
        .await;

        // 4. 完成
        emit_progress(
            &app,
            "done",
            ping,
            jitter,
            loss,
            download,
            upload,
            &server,
            "测试完成",
        );
        TEST_RUNNING.store(false, Ordering::SeqCst);
    });

    let mut handle_lock = TEST_HANDLE.lock().unwrap();
    *handle_lock = Some(handle);

    Ok(())
}

#[tauri::command]
pub async fn stop_speedtest() -> Result<(), String> {
    TEST_STOP.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn is_speedtest_running() -> Result<bool, String> {
    Ok(TEST_RUNNING.load(Ordering::SeqCst))
}

pub fn cleanup() {
    TEST_STOP.store(true, Ordering::SeqCst);
    let mut handle_lock = TEST_HANDLE.lock().unwrap();
    if let Some(handle) = handle_lock.take() {
        handle.abort();
    }
    TEST_RUNNING.store(false, Ordering::SeqCst);
    TEST_STOP.store(false, Ordering::SeqCst);
}
