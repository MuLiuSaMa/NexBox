//! 手机远程监控面板（局域网 · 实时数值）
//!
//! 开启后在局域网内启动一个内置 HTTP 服务，手机连同一 WiFi 浏览器访问即可
//! 实时查看 FPS / CPU / GPU / 显存 / 内存 / SSD / 网络 等当前数值。
//! 数据完全复用悬浮框的硬件快照 `overlay_panel::collect_hardware_data()`，
//! 不引入任何新依赖（axum / tokio / rand / serde_json 均已存在）。

use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU16, Ordering};
use std::sync::Mutex;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use rand::Rng;
use serde::Serialize;
use serde_json::json;
use tauri::Manager;

static PORT: AtomicU16 = AtomicU16::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static CODE: Mutex<Option<String>> = Mutex::new(None);
/// 持有 HTTP 服务的任务句柄，退出时 abort 它以关闭监听并释放端口
static SERVER_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
/// 手机端展示样式：0=卡片 1=列表 2=大屏
static STYLE: AtomicU8 = AtomicU8::new(0);

/// 返回给前端的状态信息
#[derive(Serialize, Clone)]
pub struct MonitorInfo {
    pub ip: Option<String>,
    pub port: u16,
    pub code: Option<String>,
    pub url: Option<String>,
    pub enabled: bool,
    pub style: u8,
}

/// 通过 UDP connect 技巧获取本机局域网 IPv4（不实际发包，仅让内核选出出口 IP）。
fn detect_lan_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // 连接一个公网地址不会真正发包，只是让路由表决定使用哪个本地接口
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    if addr.ip().is_unspecified() {
        None
    } else {
        Some(addr.ip().to_string())
    }
}

fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

fn current_code() -> Option<String> {
    CODE.lock().unwrap().clone()
}

/// 构造当前状态快照供前端使用
fn build_info() -> MonitorInfo {
    let port = PORT.load(Ordering::Relaxed);
    let ip = detect_lan_ip();
    let code = current_code();
    let url = match (&ip, port) {
        (Some(ip), p) if p > 0 => Some(format!("http://{}:{}/", ip, p)),
        _ => None,
    };
    MonitorInfo {
        ip,
        port,
        code,
        url,
        enabled: ENABLED.load(Ordering::Relaxed),
        style: STYLE.load(Ordering::Relaxed),
    }
}

/// 启动 HTTP 服务（幂等）。返回当前状态信息。
pub async fn ensure_server() -> Result<MonitorInfo, String> {
    // 已启动则直接返回
    let existing = PORT.load(Ordering::Relaxed);
    if existing > 0 {
        // 确保有访问码
        let mut guard = CODE.lock().unwrap();
        if guard.is_none() {
            *guard = Some(generate_code());
        }
        drop(guard);
        return Ok(build_info());
    }

    // 首先生成访问码
    {
        let mut guard = CODE.lock().unwrap();
        if guard.is_none() {
            *guard = Some(generate_code());
        }
    }

    let app = Router::new()
        .route("/", get(handle_index))
        .route("/api/stats", get(handle_stats))
        .route("/api/style", post(handle_style));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("绑定远程监控端口失败: {e}"))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("获取监听地址失败: {e}"))?
        .port();

    PORT.store(port, Ordering::Relaxed);

    *SERVER_TASK.lock().unwrap() = Some(tokio::spawn(async move {
        log::info!("[RemoteMonitor] listening on 0.0.0.0:{port}");
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("[RemoteMonitor] server error: {e}");
        }
    }));

    Ok(build_info())
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// 关闭 HTTP 服务并释放端口，清空运行态。
/// 退出程序时调用；下一次必须手动开启才会重新监听。
pub fn shutdown() {
    if let Some(handle) = SERVER_TASK.lock().unwrap().take() {
        handle.abort();
    }
    ENABLED.store(false, Ordering::Relaxed);
    PORT.store(0, Ordering::Relaxed);
    *CODE.lock().unwrap() = None;
    log::info!("[RemoteMonitor] 退出时已关闭远程监控服务");
}

/// 供 setup 调用：恢复上次持久化的手机端样式。
/// 开关本身刻意不恢复 —— 远程监控只能每次由用户在弹窗里手动开启。
pub fn restore_on_startup(app: &tauri::AppHandle) {
    STYLE.store(read_persisted_style(app), Ordering::Relaxed);
}

// ───────────────────────── 持久化 ─────────────────────────

fn persisted_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    Some(dir.join("remote-monitor.json"))
}

/// 写入当前样式（开关不持久化）
fn persist_style(app: &tauri::AppHandle) {
    let path = match persisted_path(app) {
        Some(p) => p,
        None => return,
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let content = json!({ "style": STYLE.load(Ordering::Relaxed) }).to_string();
    if let Err(e) = std::fs::write(&path, content) {
        log::warn!("[RemoteMonitor] 持久化失败: {e}");
    }
}

/// 读取持久化的手机端样式（缺失时默认 0）。
fn read_persisted_style(app: &tauri::AppHandle) -> u8 {
    let path = match persisted_path(app) {
        Some(p) => p,
        None => return 0,
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("style").and_then(|s| s.as_u64()))
            .map(|s| (s as u8).min(2))
            .unwrap_or(0),
        Err(_) => 0,
    }
}

/// 尽力添加防火墙入站规则（非致命，失败忽略）。
/// Windows 首次监听 0.0.0.0 通常也会自动弹出放行提示。
fn try_add_firewall_rule() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let exe_str = exe.to_string_lossy().to_string();
    let mut cmd = std::process::Command::new("netsh");
    cmd.args([
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=NexBox Remote Monitor",
        "dir=in",
        "action=allow",
        "protocol=TCP",
    ])
    .arg(format!("program={exe_str}"));
    // CREATE_NO_WINDOW：避免闪出控制台窗口
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let _ = cmd.spawn();
}

// ───────────────────────── HTTP handlers ─────────────────────────

#[derive(serde::Deserialize)]
struct StatsQuery {
    code: Option<String>,
}

async fn handle_index() -> Response {
    Html(INDEX_HTML).into_response()
}

async fn handle_stats(Query(q): Query<StatsQuery>) -> Response {
    if !ENABLED.load(Ordering::Relaxed) {
        return (StatusCode::NOT_FOUND, "closed").into_response();
    }

    let expected = current_code();
    match (expected, q.code.as_deref()) {
        (Some(exp), Some(got)) if got == exp.as_str() => {}
        _ => return (StatusCode::FORBIDDEN, "invalid code").into_response(),
    }

    // collect_hardware_data 为同步且可能拉起监控进程，放到 blocking 线程执行避免卡住 runtime
    let data = match tokio::task::spawn_blocking(crate::overlay_panel::collect_hardware_data).await {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")).into_response(),
    };

    let mut value = match serde_json::to_value(&data) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    if let Some(obj) = value.as_object_mut() {
        let computer_name = std::env::var("COMPUTERNAME").unwrap_or_default();
        obj.insert(
            "meta".to_string(),
            json!({
                "computer_name": computer_name,
                "style": STYLE.load(Ordering::Relaxed),
                "ts": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            }),
        );
    }

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("Content-Type", axum::http::HeaderValue::from_static("application/json"));
    headers.insert("Access-Control-Allow-Origin", axum::http::HeaderValue::from_static("*"));
    headers.insert("Cache-Control", axum::http::HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, value.to_string()).into_response()
}

#[derive(serde::Deserialize)]
struct StyleReq {
    code: Option<String>,
    style: Option<u8>,
}

/// 手机端本地切换样式时回写服务端，保持与桌面端一致。
async fn handle_style(axum::Json(req): axum::Json<StyleReq>) -> Response {
    if !ENABLED.load(Ordering::Relaxed) {
        return (StatusCode::NOT_FOUND, "closed").into_response();
    }
    let expected = current_code();
    match (expected, req.code.as_deref()) {
        (Some(exp), Some(got)) if got == exp.as_str() => {}
        _ => return (StatusCode::FORBIDDEN, "invalid code").into_response(),
    }
    if let Some(s) = req.style {
        STYLE.store(s.min(2), Ordering::Relaxed);
    }
    (StatusCode::OK, "{\"ok\":true}").into_response()
}

// ───────────────────────── Tauri commands ─────────────────────────

#[tauri::command]
pub async fn cmd_get_remote_monitor() -> Result<MonitorInfo, String> {
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_enable_remote_monitor() -> Result<MonitorInfo, String> {
    ensure_server().await?;
    set_enabled(true);
    try_add_firewall_rule();
    // 必须在 set_enabled(true) 之后再构造快照，否则返回的 enabled 为旧值导致前端要点两次
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_disable_remote_monitor() -> Result<MonitorInfo, String> {
    set_enabled(false);
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_set_remote_style(app: tauri::AppHandle, style: u8) -> Result<MonitorInfo, String> {
    STYLE.store(style.min(2), Ordering::Relaxed);
    persist_style(&app);
    Ok(build_info())
}

// ───────────────────────── 手机端页面 ─────────────────────────
// 自包含黑白极简监控页：先输入 PIN，再每 1s 轮询 /api/stats。
// 样式：图表（默认，横屏多折线）/ 卡片 / 列表；数值均带实时折线图。
const INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover" />
<title>新境盒 远程监控</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body {
    font-family: system-ui,-apple-system,"Segoe UI","Microsoft YaHei",Roboto,sans-serif;
    background: #050505; color: #f5f5f5; min-height: 100vh; padding: 16px;
    -webkit-font-smoothing: antialiased;
  }
  body[data-theme="light"] { background: #f4f4f5; color: #0a0a0a; }
  .top { display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px; gap: 8px; }
  .brand { font-weight: 800; letter-spacing: 2px; font-size: 20px; }
  .host { font-size: 12px; color: #8a8a8a; margin-top: 2px; }
  .status { font-size: 12px; color: #8a8a8a; }
  .controls { display: flex; align-items: center; gap: 6px; }
  .ctlBtn {
    background: #141414; border: 1px solid #2a2a2a; color: #dcdcdc;
    border-radius: 8px; padding: 6px 10px; font-size: 12px; cursor: pointer; white-space: nowrap;
  }
  body[data-theme="light"] .ctlBtn { background: #fff; border-color: #ddd; color: #222; }

  /* 卡片网格 */
  .grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }
  @media (orientation: landscape) { .grid { grid-template-columns: repeat(4, 1fr); } }
  .card {
    background: #0e0e0e; border: 1px solid #222;
    border-radius: 14px; padding: 12px 14px;
  }
  body[data-theme="light"] .card { background: #fff; border-color: #e4e4e7; }
  .label { font-size: 12px; color: #9a9a9a; margin-bottom: 6px; }
  .value { font-size: 26px; font-weight: 800; font-variant-numeric: tabular-nums; line-height: 1.1; color: #fff; }
  body[data-theme="light"] .value { color: #0a0a0a; }
  .unit { font-size: 13px; font-weight: 500; color: #8a8a8a; margin-left: 3px; }
  .sparkWrap { height: 22px; margin-top: 6px; color: #5a5a5a; }
  body[data-theme="light"] .sparkWrap { color: #b4b4b8; }
  svg.spark { width: 100%; height: 22px; display: block; }

  /* 列表样式 */
  body[data-style="list"] .grid { display: flex; flex-direction: column; gap: 8px; }
  body[data-style="list"] .card { display: flex; align-items: center; justify-content: space-between; border-radius: 10px; padding: 12px 14px; }
  body[data-style="list"] .label { margin-bottom: 0; flex: 1; }
  body[data-style="list"] .value { font-size: 20px; }
  body[data-style="list"] .sparkWrap { display: none; }

  /* 图表样式（默认）：每个指标一条大折线 */
  #charts { display: none; grid-template-columns: repeat(1, 1fr); gap: 12px; }
  @media (orientation: landscape) { #charts { grid-template-columns: repeat(3, 1fr); } }
  body[data-style="chart"] #charts { display: grid; }
  body[data-style="chart"] #grid { display: none; }
  .chartCard { background: #0e0e0e; border: 1px solid #222; border-radius: 14px; padding: 12px 14px; }
  body[data-theme="light"] .chartCard { background: #fff; border-color: #e4e4e7; }
  .chartHead { display: flex; justify-content: space-between; align-items: baseline; }
  .chartHead .value { font-size: 22px; }
  .chartBody { height: 84px; margin: 10px 0 6px; color: #e6e6e6; }
  body[data-theme="light"] .chartBody { color: #111; }
  .chartBody svg { width: 100%; height: 100%; display: block; }
  .chartFoot { display: flex; justify-content: space-between; font-size: 11px; color: #8a8a8a; }

  .gate { max-width: 360px; margin: 60px auto; text-align: center; }
  .gate input {
    width: 100%; padding: 14px; font-size: 22px; letter-spacing: 10px; text-align: center;
    border-radius: 12px; border: 1px solid #333; background: #0e0e0e; color: #fff; margin: 16px 0;
  }
  .gate button {
    width: 100%; padding: 14px; font-size: 16px; font-weight: 700; border: none; cursor: pointer;
    border-radius: 12px; background: #fff; color: #000;
  }
  .err { color: #ff6b6b; font-size: 13px; margin-top: 10px; min-height: 18px; }
  .hidden { display: none; }
</style>
</head>
<body data-style="chart">
  <div id="gate" class="gate">
    <div class="brand">新境盒 · 远程监控</div>
    <p class="host" style="margin-top:10px">请输入电脑上显示的 6 位访问码</p>
    <input id="code" inputmode="numeric" maxlength="6" placeholder="------" autocomplete="off" />
    <button id="enter">进入</button>
    <div id="err" class="err"></div>
  </div>

  <div id="panel" class="hidden">
    <div class="top">
      <div>
        <div class="brand">新境盒</div>
        <div class="host" id="host"></div>
      </div>
      <div class="controls">
        <div class="status" id="status">连接中…</div>
        <button id="styleBtn" class="ctlBtn">图表</button>
        <button id="themeBtn" class="ctlBtn">黑</button>
      </div>
    </div>
    <div id="charts"></div>
    <div class="grid" id="grid"></div>
  </div>

<script>
  const $ = (id) => document.getElementById(id);
  let timer = null;

  const STYLES = [ { id: "chart", name: "图表" }, { id: "grid", name: "卡片" }, { id: "list", name: "列表" } ];
  let styleIdx = parseInt(localStorage.getItem("nb_style") || "0", 10);
  if (isNaN(styleIdx) || styleIdx < 0 || styleIdx >= STYLES.length) styleIdx = 0;
  let theme = localStorage.getItem("nb_theme") || "dark";

  const CARDS = [
    { k: "fps", label: "FPS", unit: "", hkey: "fps" },
    { k: "fps_1low", label: "1% Low", unit: "", hkey: "fps_1low" },
    { k: "fps_01low", label: "0.1% Low", unit: "", hkey: "fps_01low" },
    { k: "game_ping", label: "游戏延迟", unit: "ms", hkey: "game_ping" },
    { k: "cpu_usage", label: "CPU 占用", unit: "%", hkey: "cpu_usage" },
    { k: "cpu_temp", label: "CPU 温度", unit: "°C", hkey: "cpu_temp" },
    { k: "cpu_power", label: "CPU 功耗", unit: "W", hkey: "cpu_power" },
    { k: "cpu_clock", label: "CPU 频率", unit: "MHz", hkey: "cpu_clock" },
    { k: "gpu_usage", label: "GPU 占用", unit: "%", hkey: "gpu_usage" },
    { k: "gpu_temp", label: "GPU 温度", unit: "°C", hkey: "gpu_temp" },
    { k: "gpu_power", label: "GPU 功耗", unit: "W", hkey: "gpu_power" },
    { k: "gpu_clock", label: "GPU 频率", unit: "MHz", hkey: "gpu_clock" },
    { k: "vram", label: "显存", unit: "", hkey: "gpu_vram_used" },
    { k: "memory_usage", label: "内存占用", unit: "%", hkey: "memory_usage" },
    { k: "ssd_temp", label: "SSD 温度", unit: "°C", hkey: "ssd_temp" },
    { k: "net", label: "网速 下/上", unit: "", hkey: "net_down_speed" },
  ];

  // 图表样式重点展示的指标
  const CHARTS = [
    { key: "fps", label: "FPS" },
    { key: "cpu_usage", label: "CPU 占用 %" },
    { key: "gpu_usage", label: "GPU 占用 %" },
    { key: "cpu_temp", label: "CPU 温度 °C" },
    { key: "gpu_temp", label: "GPU 温度 °C" },
    { key: "memory_usage", label: "内存占用 %" },
  ];

  const MAX = 90;
  const HIST = {};
  function initHist() {
    for (const c of CARDS) if (c.hkey) HIST[c.hkey] = [];
    for (const c of CHARTS) if (!(c.key in HIST)) HIST[c.key] = [];
  }
  function pushHist(d) {
    for (const k in HIST) {
      const v = d[k];
      HIST[k].push(typeof v === "number" ? v : null);
      if (HIST[k].length > MAX) HIST[k].shift();
    }
  }

  function fmt(v) {
    if (v === null || v === undefined) return null;
    if (typeof v === "number") return Number.isInteger(v) ? String(v) : v.toFixed(1);
    return String(v);
  }

  // 生成折线 SVG；stroke 用 currentColor 以适配黑白主题
  function lineSvg(arr, h, fill) {
    const vals = arr.filter((v) => v !== null);
    if (vals.length < 2) return `<svg viewBox="0 0 100 ${h}" preserveAspectRatio="none"></svg>`;
    let mn = Math.min(...vals), mx = Math.max(...vals);
    if (mx === mn) { mx = mn + 1; }
    const n = arr.length, step = 100 / (n - 1);
    let pts = "";
    for (let i = 0; i < n; i++) {
      const v = arr[i];
      if (v === null) continue;
      const x = (i * step).toFixed(2);
      const y = (h - ((v - mn) / (mx - mn)) * (h - 4) - 2).toFixed(2);
      pts += x + "," + y + " ";
    }
    const poly = `<polyline points="${pts}" fill="none" stroke="currentColor" stroke-width="1.6" vector-effect="non-scaling-stroke"/>`;
    const area = fill ? `<polygon points="0,${h} ${pts} 100,${h}" fill="currentColor" opacity="0.08"/>` : "";
    return `<svg viewBox="0 0 100 ${h}" preserveAspectRatio="none">${area}${poly}</svg>`;
  }

  function buildGrid() {
    $("grid").innerHTML = CARDS.map((c) =>
      `<div class="card"><div class="label">${c.label}</div>` +
      `<div class="value" id="v_${c.k}">—</div>` +
      (c.hkey ? `<div class="sparkWrap" id="s_${c.k}"></div>` : "") +
      `</div>`
    ).join("");
  }

  function setCard(k, text, unit) {
    const el = $("v_" + k);
    if (el) el.innerHTML = text + (unit ? `<span class="unit">${unit}</span>` : "");
  }

  function updateCards(d) {
    for (const c of CARDS) {
      if (c.k === "vram") {
        const used = fmt(d.gpu_vram_used), total = fmt(d.gpu_vram_total);
        setCard("vram", used === null ? "—" : (used + " / " + (total ?? "—")), used === null ? "" : "MB");
      } else if (c.k === "net") {
        const dn = fmt(d.net_down_speed), up = fmt(d.net_up_speed);
        setCard("net", (dn ?? "—") + " / " + (up ?? "—"), (dn === null && up === null) ? "" : "KB/s");
      } else {
        const v = fmt(d[c.k]);
        setCard(c.k, v === null ? "—" : v, v === null ? "" : c.unit);
      }
      if (c.hkey) { const s = $("s_" + c.k); if (s) s.innerHTML = lineSvg(HIST[c.hkey] || [], 22, false); }
    }
  }

  function renderCharts() {
    $("charts").innerHTML = CHARTS.map((c) => {
      const arr = HIST[c.key] || [];
      const vals = arr.filter((v) => v !== null);
      const last = vals.length ? vals[vals.length - 1] : null;
      const mn = vals.length ? Math.min(...vals) : null;
      const mx = vals.length ? Math.max(...vals) : null;
      return `<div class="chartCard"><div class="chartHead"><span class="label">${c.label}</span>` +
        `<span class="value">${last === null ? "—" : fmt(last)}</span></div>` +
        `<div class="chartBody">${lineSvg(arr, 40, true)}</div>` +
        `<div class="chartFoot"><span>最低 ${mn === null ? "—" : fmt(mn)}</span>` +
        `<span>最高 ${mx === null ? "—" : fmt(mx)}</span></div></div>`;
    }).join("");
  }

  function applyStyle() {
    document.body.setAttribute("data-style", STYLES[styleIdx].id);
    $("styleBtn").textContent = STYLES[styleIdx].name;
  }
  function postStyle() {
    const code = sessionStorage.getItem("nb_code") || "";
    fetch("/api/style", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code, style: styleIdx }),
    }).catch(() => {});
  }
  function applyTheme() {
    document.body.setAttribute("data-theme", theme);
    $("themeBtn").textContent = theme === "dark" ? "黑" : "白";
  }

  async function poll() {
    const code = sessionStorage.getItem("nb_code") || "";
    try {
      const resp = await fetch("/api/stats?code=" + encodeURIComponent(code), { cache: "no-store" });
      if (resp.status === 403) { throw new Error("访问码错误"); }
      if (resp.status === 404) { throw new Error("功能已在电脑上关闭"); }
      if (!resp.ok) { throw new Error("HTTP " + resp.status); }
      const d = await resp.json();
      $("status").textContent = "实时";
      $("host").textContent = (d.meta && d.meta.computer_name) ? ("主机：" + d.meta.computer_name) : "";
      if (d.meta && typeof d.meta.style === "number" && d.meta.style !== styleIdx) {
        styleIdx = d.meta.style;
        localStorage.setItem("nb_style", String(styleIdx));
        applyStyle();
      }
      pushHist(d);
      updateCards(d);
      renderCharts();
    } catch (e) {
      $("status").textContent = "已断开";
      if (String(e.message).includes("访问码") || String(e.message).includes("关闭")) {
        stopAndShowGate(String(e.message));
      }
    }
  }

  function stopAndShowGate(msg) {
    if (timer) { clearInterval(timer); timer = null; }
    sessionStorage.removeItem("nb_code");
    $("panel").classList.add("hidden");
    $("gate").classList.remove("hidden");
    $("err").textContent = msg || "";
  }

  function startPolling() {
    initHist();
    buildGrid();
    poll();
    if (timer) clearInterval(timer);
    timer = setInterval(poll, 1000);
  }

  function enter() {
    const code = ($("code").value || "").trim();
    if (code.length !== 6) { $("err").textContent = "请输入 6 位访问码"; return; }
    sessionStorage.setItem("nb_code", code);
    $("err").textContent = "";
    $("gate").classList.add("hidden");
    $("panel").classList.remove("hidden");
    startPolling();
  }

  $("enter").addEventListener("click", enter);
  $("code").addEventListener("keydown", (ev) => { if (ev.key === "Enter") enter(); });
  $("styleBtn").addEventListener("click", () => {
    styleIdx = (styleIdx + 1) % STYLES.length;
    localStorage.setItem("nb_style", String(styleIdx));
    applyStyle();
    postStyle();
  });
  $("themeBtn").addEventListener("click", () => {
    theme = theme === "dark" ? "light" : "dark";
    localStorage.setItem("nb_theme", theme);
    applyTheme();
  });

  applyStyle();
  applyTheme();

  // 若本会话已验证过，直接恢复面板
  if (sessionStorage.getItem("nb_code")) {
    $("gate").classList.add("hidden");
    $("panel").classList.remove("hidden");
    startPolling();
  }
</script>
</body>
</html>"##;
