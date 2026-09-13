//! 三角洲行动 · 战绩分析
//!
//! 通过 WeGame 助手网页（https://www.wegame.com.cn/helper/df/score/）登录后
//! 获取战绩、藏品等数据。
//!
//! 采集链路（Rust 主动拉取，规避所有回传通道问题）：
//! 1. 打开官方助手页面（WebView），用户 QQ/微信 扫码登录
//! 2. 注入采集脚本：登录后自动同源 fetch 调 Dfm API，把结果 JSON 存到 `window.__dfStatsData`
//! 3. Rust 周期性 `eval_with_callback` 读取 `window.__dfStatsData`（Tauri 原生回调，可靠）
//! 4. Rust 持久化 + 事件通知前端 → 前端展示

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const WINDOW_LABEL: &str = "df-stats-login";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

static LOGIN_STATE: OnceLock<Mutex<Option<DfLoginState>>> = OnceLock::new();
static COLLECTED: OnceLock<Mutex<std::collections::HashMap<String, serde_json::Value>>> =
    OnceLock::new();
/// 会话判定失效标记：置位后 role_info 兜底不再复活登录态（须重新扫码）
static SESSION_BROKEN: OnceLock<Mutex<bool>> = OnceLock::new();
fn session_broken_set(v: bool) {
    *SESSION_BROKEN.get_or_init(|| Mutex::new(false)).lock().unwrap() = v;
}
fn session_broken_get() -> bool {
    *SESSION_BROKEN.get_or_init(|| Mutex::new(false)).lock().unwrap()
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DfLoginState {
    pub logged_in: bool,
    pub openid: String,
    pub area: String,
    pub nickname: String,
    pub avatar: String,
    pub level: String,
    pub tgp_id: String,
    #[serde(default = "default_account_type")]
    pub account_type: i64,
    pub login_time: u64,
}

fn default_account_type() -> i64 {
    1
}

use serde::{Deserialize, Serialize};

fn login_store() -> &'static Mutex<Option<DfLoginState>> {
    LOGIN_STATE.get_or_init(|| Mutex::new(None))
}

fn collected_store() -> &'static Mutex<std::collections::HashMap<String, serde_json::Value>> {
    COLLECTED.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("NexBox").join("df_stats");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 数据源窗口独立的 WebView2 数据目录（cookie/登录态隔离于此），
/// 退出时删除整个目录即可彻底清除数据源登录态。
fn webview_data_dir() -> PathBuf {
    let dir = data_dir().join("webview");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn state_file() -> PathBuf {
    data_dir().join("login_state.json")
}

fn load_login_state() -> Option<DfLoginState> {
    std::fs::read_to_string(state_file())
        .ok()
        .and_then(|s| serde_json::from_str::<DfLoginState>(&s).ok())
        .filter(|s| s.logged_in && !s.openid.is_empty())
}

fn save_login_state(state: &DfLoginState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        std::fs::write(state_file(), json).ok();
    }
}

fn clear_login_state() {
    std::fs::remove_file(state_file()).ok();
    *login_store().lock().unwrap() = None;
}

/// 自包含采集 JS：必须在「同步」路径返回对象（WebView2 ExecuteScript 只序列化脚本的
/// 同步返回值，async IIFE 会直接变成 {}）。
/// 关键修复：不能有永久守卫 —— 若首次 fetch 挂起/被卡，守卫会让之后每轮都跳过采集
/// 永远返回 {}。这里每轮都尝试：若上一轮仍在跑用 AbortController 6s 超时兜底，
/// 超时后自动允许下一轮重试；所有错误回传 __error 供 Rust 日志定位。
fn collect_js() -> String {
    r#"
(() => {
  if (window.__dfStatsPending) return window.__dfStatsResult || {};
  window.__dfStatsPending = true;
  const API = 'https://www.wegame.com.cn/api/v1/wegame.pallas.dfm.DfmBattle';
  const CALLER = 'wegame.pallas.web.DfmBattle';
  const from_src = 'df_web';
  const AREA = 36;
  // 登录方式：tgp_user_type cookie == "1" 表示微信登录(account_type=2)，否则 QQ 登录(1)
  function getCookie(name) {
    const m = document.cookie.match(new RegExp('(?:^|; )' + name + '=([^;]*)'));
    return m ? decodeURIComponent(m[1]) : '';
  }
  const tgpUserType = getCookie('tgp_user_type');
  const tgpId = getCookie('tgp_id');
  const accountType = tgpUserType === '1' ? 2 : 1;
  window.__dfStatsCookieDiag = { tgp_id: tgpId || '', tgp_user_type: tgpUserType || '', accountType: accountType };
  const ctrl = new AbortController();
  // 45s：对局列表需按 after 游标翻页（每页 8 条，最多 2×15 次请求），8s 会中途 abort
  const timer = setTimeout(function(){ ctrl.abort(); window.__dfStatsPending = false; }, 45000);
  async function call(method, data) {
    const r = await fetch(API + '/' + method, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'trpc-caller': CALLER },
      body: JSON.stringify(Object.assign({ from_src }, data)),
      credentials: 'include',
      signal: ctrl.signal
    });
    return await r.json();
  }
  call('GetRoleInfo', { account_type: accountType, area: AREA })
    .then(function(role) {
      clearTimeout(timer);
      window.__dfStatsPending = false;
      const ok = !!(role && role.result && role.result.error_code === 0 && role.role_info);
      const out = { __ok: ok, __role_error: (role && role.result && role.result.error_message) || '' };
      if (!ok) { window.__dfStatsResult = JSON.stringify(out); return; }
      const info = role.role_info;
      const openid = info.openid || '';
      const area = info.area || String(AREA);
      out['role_info'] = info;
      out['login_state'] = {
        logged_in: true, openid, area,
        nickname: info.name || '', avatar: info.icon || '',
        level: info.level != null ? String(info.level) : '',
        tgp_id: info.tgp_id != null ? String(info.tgp_id) : '',
        login_time: Date.now()
      };
      if (!openid) { window.__dfStatsResult = JSON.stringify(out); return; }
      // 对局列表分页：官方单次上限 8 条。官方网页端从不翻页，游标语义无公开参考，
      // 这里自动尝试候选游标（最后一条 startTime → roomId），能拉到新数据即继续翻页。
      async function fetchBattleList(queue) {
        const key = queue === 'sol' ? 'sols' : 'tdms';
        let first = null; const seen = {}; let all = [];
        async function callList(after) {
          const b = await call('GetBattleList', { openid: openid, area: area, queue: queue, account_type: accountType, size: 50, after: after, filters: [] });
          if (!b || !b.result || b.result.error_code !== 0) return null;
          if (!first) first = b;
          return b[key] || [];
        }
        function absorb(items) {
          let added = 0;
          for (const it of items) {
            const k = it.roomId || it.startTime;
            if (k && !seen[k]) { seen[k] = 1; all.push(it); added++; }
          }
          return added;
        }
        const page1 = await callList(null);
        if (!page1) return first;
        absorb(page1);
        if (page1.length < 8) { first[key] = all; return first; }
        const last1 = page1[page1.length - 1];
        const candidates = [];
        if (last1.startTime) candidates.push('startTime');
        if (last1.roomId) candidates.push('roomId');
        for (const field of candidates) {
          let newInStrategy = 0; let cursor = last1[field];
          for (let p = 0; p < 14; p++) {
            const items = await callList(cursor);
            if (items === null || !items.length) break;
            const added = absorb(items);
            if (!added) break;
            newInStrategy += added;
            if (items.length < 8) break;
            const nx = items[items.length - 1][field];
            if (!nx) break;
            cursor = nx;
          }
          if (newInStrategy > 0) break;
        }
        if (first) first[key] = all;
        return first;
      }
      return Promise.all([
        call('GetBattleReport', { openid, area, account_type: accountType, queue: 'sol', sid: '' }).then(function(b){ out['battle_report_sol'] = b; }),
        call('GetBattleReport', { openid, area, account_type: accountType, queue: 'tdm', sid: '' }).then(function(b){ out['battle_report_tdm'] = b; }),
        fetchBattleList('sol').then(function(b){ out['battle_list'] = b; }),
        call('GetCollectibles', { openid, area, account_type: accountType }).then(function(b){ out['collectibles'] = b; }),
        call('GetDailyStats', { openid, area, account_type: accountType }).then(function(b){ out['daily_stats'] = b; })
      ]).then(function() {
        out['collect_done'] = { done: true };
        out['cookie_diag'] = window.__dfStatsCookieDiag || {};
        window.__dfStatsResult = JSON.stringify(out);
      });
    })
    .catch(function(e) {
      clearTimeout(timer);
      window.__dfStatsPending = false;
      window.__dfStatsResult = JSON.stringify({ __error: String(e) });
    });
  // 返回「对象」而非字符串：WebView2 ExecuteScript 会把对象转成标准 JSON，
  // 若返回字符串会被再包一层引号导致 Rust 端解析失败。
  try { return JSON.parse(window.__dfStatsResult || '{}'); } catch (e) { return {}; }
})()
"#
    .to_string()
}

/// 打开 WeGame 助手战绩页面（WebView），用户扫码登录后自动采集数据，
/// 同时启动 Rust 轮询（eval_with_callback 读取 window.__dfStatsData）。
#[tauri::command]
pub async fn open_df_stats_login(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        log::info!("[DfStats] window already exists, show");
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let url: tauri::Url = "https://www.wegame.com.cn/helper/df/score/"
        .parse()
        .map_err(|e: url::ParseError| e.to_string())?;

    log::info!("[DfStats] creating window");
    // 独立的 WebView2 数据目录：数据源的登录 cookie 仅存于此目录，
    // 退出登录时删除该目录即可彻底清除数据源登录态。
    let data_dir = webview_data_dir();
    let window = WebviewWindowBuilder::new(&app, WINDOW_LABEL, WebviewUrl::External(url))
        .title("三角洲行动 · 战绩数据源")
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36")
        .inner_size(1100.0, 750.0)
        .resizable(true)
        .center()
        .visible(true)
        .focused(true)
        .background_color(tauri::window::Color(24, 26, 32, 255))
        .data_directory(data_dir)
        .build()
        .map_err(|e| format!("创建战绩分析窗口失败: {e}"))?;

    log::info!("[DfStats] window created");
    let _ = window.show();
    let _ = window.set_focus();

    // ⭐ Rust 轮询：每隔 2s eval 自包含采集 JS，直接拿回全量结果（WebView2 ExecuteScript 自动 await async）
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        let app_for_poll = app.clone();
        let poll_win = win.clone();
        tauri::async_runtime::spawn(async move {
            log::info!("[DfStats] polling started");
            for i in 0..600 {
                // 已完成采集则停止
                if collected_store()
                    .lock()
                    .unwrap()
                    .contains_key("collect_done")
                {
                    break;
                }
                let app2 = app_for_poll.clone();
                let js = collect_js();
                let callback = Box::new(move |value: String| {
                    handle_poll_result(&app2, &value);
                });
                if let Err(e) = poll_win.eval_with_callback(js, callback) {
                    log::warn!("[DfStats] eval_with_callback failed #{i}: {e}");
                }
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
            }
            log::info!("[DfStats] polling stopped");
        });
    }

    Ok(())
}

/// 处理 eval_with_callback 取回的 JS 值（JSON 字符串：{kind: payload...}）
fn handle_poll_result(app: &AppHandle, value: &str) {
    let trimmed = value.trim();
    log::info!("[DfStats] poll callback raw len={}", trimmed.len());
    if trimmed.len() > 200 {
        log::info!("[DfStats] poll head: {}", &trimmed[..200]);
    }
    if trimmed.is_empty() || trimmed == "{}" {
        return;
    }
    let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(trimmed)
    else {
        log::warn!("[DfStats] poll parse failed: {}", &trimmed[..trimmed.len().min(200)]);
        return;
    };
    // 诊断字段
    if let Some(ok) = map.get("__ok") {
        if ok.as_bool() != Some(true) {
            let err = map.get("__role_error").and_then(|v| v.as_str()).unwrap_or("");
            log::info!("[DfStats] GetRoleInfo not ok: {}", err);
            return;
        }
    }
    if let Some(err) = map.get("__error") {
        log::warn!("[DfStats] page script error: {}", err);
        return;
    }

    // ★ 关键：只要 __ok==true 且存在 role_info，立即构造登录态落盘并关闭窗口，
    //    不依赖单独的 login_state 键（避免遍历顺序/键缺失导致漏关窗）。
    if let Some(role) = map.get("role_info") {
        if let Some(name) = role.get("name").and_then(|v| v.as_str()) {
            log::info!("[DfStats] LOGIN DETECTED via role_info: {}", name);
            let state = DfLoginState {
                logged_in: true,
                openid: role.get("openid").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                area: role.get("area").map(|v| v.to_string()).unwrap_or_else(|| "36".to_string()),
                nickname: name.to_string(),
                avatar: role.get("icon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                level: role.get("level").map(|v| v.to_string()).unwrap_or_default(),
                tgp_id: role.get("tgp_id").map(|v| v.to_string()).unwrap_or_default(),
                account_type: 2,
                login_time: now_ms(),
            };
            save_login_state(&state);
            *login_store().lock().unwrap() = Some(state.clone());
            let _ = app.emit("df-stats-login-changed", state);
            // 在数据源窗口内显示「登录成功」提示，短暂停留后自动关闭
            if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
                let js = r##"document.body.innerHTML = '<div style="position:fixed;inset:0;z-index:999999;background:#0a0c10;display:flex;flex-direction:column;align-items:center;justify-content:center;font-family:sans-serif"><div style="width:64px;height:64px;border-radius:50%;background:rgba(52,211,153,0.15);display:flex;align-items:center;justify-content:center;margin-bottom:16px"><svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="#34d399" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg></div><div style="color:#34d399;font-size:22px;font-weight:700;margin-bottom:8px">登录成功</div><div style="color:#9ca3af;font-size:14px">数据源连接成功，正在返回工具箱…</div></div>';"##;
                let _ = w.eval(js);
                let close_win = w.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                    let _ = close_win.close();
                });
            }
        }
    }

    for (kind, payload) in map {
        if kind.starts_with("__") {
            continue;
        }
        let _ = handle_collect(app, &serde_json::json!({ "kind": kind, "payload": payload }).to_string());
    }
}

/// 处理采集到的数据
fn handle_collect(app: &AppHandle, body: &str) -> Result<serde_json::Value, String> {
    #[derive(Deserialize)]
    struct CollectBody {
        kind: String,
        payload: serde_json::Value,
    }
    let parsed: CollectBody = serde_json::from_str(body).map_err(|e| e.to_string())?;

    // 所有采集数据落盘持久化（重启/切页不丢）
    if parsed.kind != "login_state" {
        let safe = parsed.kind.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        let _ = std::fs::write(data_dir().join(format!("data_{safe}.json")), serde_json::to_string(&parsed.payload).unwrap_or_default());
    }

    match parsed.kind.as_str() {
        "login_state" => {
            let state: DfLoginState =
                serde_json::from_value(parsed.payload.clone()).map_err(|e| e.to_string())?;
            log::info!("[DfStats] login_state received, openid={}", state.openid);
            save_login_state(&state);
            *login_store().lock().unwrap() = Some(state.clone());
            let emit_state = state.clone();
            let _ = app.emit("df-stats-login-changed", emit_state);
            // 登录成功：自动关闭数据源窗口
            if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
                let _ = w.close();
            }
        }
        _ => {
            if parsed.kind == "collect_done" {
                log::info!("[DfStats] all collect done");
            }
            collected_store()
                .lock()
                .unwrap()
                .insert(parsed.kind.clone(), parsed.payload.clone());
            let _ = app.emit("df-stats-data", (parsed.kind.clone(), parsed.payload.clone()));
        }
    }
    Ok(serde_json::json!({ "ok": true }))
}

/// 关闭战绩数据源窗口
#[tauri::command]
pub fn close_df_stats_login(app: AppHandle) {
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }
}

/// 检查是否已登录（内存 → 磁盘 login_state → 磁盘 role_info 兜底）
/// 会话有效性校验：QQ 登录有 uin+skey 长期凭据 → 先 try_renew 续期，
/// 失败说明令牌已被腾讯吊销（重新扫码），返回 None 引导重新登录（保留采集缓存）。
/// 微信登录无长期凭据（uin+skey 为空）→ 不做续期校验，直接放行。
#[tauri::command]
pub async fn check_df_stats_login() -> Option<DfLoginState> {
    let found = login_store().lock().unwrap().clone().or_else(load_login_state);
    if let Some(s) = found {
        if s.logged_in {
            // 恢复持久化 cookie 供续期 / 采集使用
            restore_cookies_from_disk();
            let cookies = load_cookie_snapshot();
            let has_long = !cookies.get("uin").map(|v| v.as_str()).unwrap_or("").is_empty()
                && !cookies.get("skey").map(|v| v.as_str()).unwrap_or("").is_empty();
            if !has_long && !session_broken_get() {
                // 微信登录（无 uin+skey 长期凭据）且本次未判定失效 → 直接放行
                return Some(s);
            }
            if !has_long {
                // 微信登录但本次会话已判定失效（如检查发现无法续期）→ 引导重新登录
                log::warn!("[DfStats] check login: wx session marked broken, forcing re-login");
                clear_login_state();
                return None;
            }
            let client = df_client().lock().unwrap().clone();
            match try_renew_session(&client).await {
                Ok(true) => {
                    // 续期成功：返回最新登录态（login_time 已刷新）
                    if let Some(st) = load_login_state() {
                        *login_store().lock().unwrap() = Some(st.clone());
                        return Some(st);
                    }
                    return Some(s);
                }
                _ => {
                    // 续期失败：QQ 令牌已过期/吊销 → 显式清除登录态，引导重新扫码（保留采集缓存）
                    log::warn!("[DfStats] check login: session expired (renew failed), forcing re-login");
                    session_broken_set(true);
                    clear_login_state();
                    return None;
                }
            }
        }
    }
    // 兜底：从 data_role_info.json 构造登录态（仅当会话从未判定失效时）
    // 会话已判定失效（session_broken）时禁止 role_info 兜底复活登录态，
    // 否则 1.5s 轮询会在重新扫码前反复把用户"拉回"已登录的假界面。
    if session_broken_get() {
        return None;
    }
    let path = data_dir().join("data_role_info.json");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let name = v.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string();
            let openid = v.get("openid").and_then(|s| s.as_str()).unwrap_or("").to_string();
            if !openid.is_empty() {
                let area = v.get("area").map(|s| s.to_string()).unwrap_or_else(|| "36".to_string());
                let level = v.get("level").map(|s| s.to_string()).unwrap_or_default();
                let state = DfLoginState {
                    logged_in: true,
                    openid,
                    area,
                    nickname: name,
                    avatar: v.get("icon").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                    level,
                    tgp_id: "".to_string(),
                    account_type: 2,
                    login_time: now_ms(),
                };
                save_login_state(&state);
                *login_store().lock().unwrap() = Some(state.clone());
                return Some(state);
            }
        }
    }
    None
}

/// 退出登录：清空本地保存的登录态 + 删除数据源独立 WebView2 数据目录（含 cookie），
/// 真正做到数据源彻底登出。
#[tauri::command]
pub fn logout_df_stats(app: AppHandle) {
    // 1. 清本地持久化状态（login_state + role_info 兜底 + 采集数据）
    session_broken_set(false);
    clear_login_state();
    collected_store().lock().unwrap().clear();
    let dir = data_dir();
    for name in ["login_state.json", "data_role_info.json", "data_cookie_diag.json", "cookies.json"] {
        let _ = std::fs::remove_file(dir.join(name));
    }

    // 2. 关闭数据源窗口（若存在）
    if let Some(w) = app.get_webview_window(WINDOW_LABEL) {
        let _ = w.close();
    }

    // 3. 删除数据源独立的 WebView2 数据目录（cookie/登录态物理清除）
    //    先小等片刻让 webview 释放文件句柄
    let wdir = dir.join("webview");
    if wdir.exists() {
        // 尝试立即删除，失败则安排重试（句柄可能仍被占用）
        for attempt in 0..3 {
            match std::fs::remove_dir_all(&wdir) {
                Ok(_) => break,
                Err(e) => {
                    if attempt == 2 {
                        log::warn!("[DfStats] remove webview data dir failed: {e}");
                    } else {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                }
            }
        }
    }
    log::info!("[DfStats] logout: cleared local state and webview data dir");
}

/// 数据源窗口是否打开
#[tauri::command]
pub fn is_df_stats_window_open(app: AppHandle) -> bool {
    app.get_webview_window(WINDOW_LABEL).is_some()
}

/// 获取采集到的数据（内存优先，内存为空时从磁盘兜底恢复）
#[tauri::command]
pub fn get_df_stats_cached_data(kind: String) -> Option<serde_json::Value> {
    // 内存优先
    if let Some(v) = collected_store().lock().unwrap().get(&kind).cloned() {
        return Some(v);
    }
    // 磁盘兜底：data_<kind>.json
    let safe = kind.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let path = data_dir().join(format!("data_{safe}.json"));
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            collected_store().lock().unwrap().insert(kind.clone(), v.clone());
            return Some(v);
        }
    }
    None
}

/// 重新采集一次（纯 Rust 直连 Dfm API，复用当前登录态 cookie；前端各 tab 刷新按钮调用）
#[tauri::command]
pub fn refresh_df_stats_collect(app: AppHandle) {
    let client = df_client().lock().unwrap().clone();
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        // account_type 从持久化登录态读取（QQ=1 / 微信=2），缺省 1
        let account_type = load_login_state()
            .map(|s| s.account_type)
            .unwrap_or(1);
        match df_stats_do_collect(&client, account_type).await {
            Ok(collected) => {
                // 新采集结果写入内存 store（get_df_stats_cached_data 优先读内存，
                // 不更新会导致重新登录/重启后才显示最新数据）
                {
                    let mut store = collected_store().lock().unwrap();
                    for (kind, val) in &collected {
                        store.insert(kind.clone(), val.clone());
                    }
                }
                for (kind, val) in &collected {
                    if !kind.starts_with("__") {
                        let _ = app2.emit("df-stats-data", (kind.clone(), val.clone()));
                    }
                }
                log::info!("[DfStats] refresh collect done, {} kinds", collected.len());
            }
            Err(e) => log::warn!("[DfStats] refresh collect failed: {e}"),
        }
    });
}

// ═══════════════════════════════════════════════════════════════
// 工具箱内 QQ 二维码登录（不再打开 WeGame 窗口）
// ═══════════════════════════════════════════════════════════════

/// WeGame 登录二维码参数（来自官方 wegame-login.desktop.js）
const QQ_APPID: &str = "1600001063";
const QQ_DAID: &str = "733";
const QQ_U1: &str = "https%3A%2F%2Fwww.wegame.com.cn%2Flogin%2Fcallback.html";
/// 微信 OAuth（来自官方 login/callback.html: appid=wx911818d5d92affa8, clienttype=1000005）
const WX_APPID: &str = "wx911818d5d92affa8";
const WX_REDIRECT: &str = "https://www.wegame.com.cn/login/callback.html";
const WX_CLIENTTYPE: &str = "1000005";

/// 全局共享 cookie jar：QQ/微信/wegame 全链路 cookie 都进这里，
/// 登录成功后手动注入 Dfm API 需要的 tgp_* cookie（官方 JS 写 document.cookie，reqwest 不自动种），
/// 采集时让 reqwest 自动携带全部 cookie（绝不用手动 Cookie 头覆盖 jar）。
static DF_JAR: OnceLock<std::sync::Arc<reqwest::cookie::Jar>> = OnceLock::new();
fn df_jar() -> std::sync::Arc<reqwest::cookie::Jar> {
    DF_JAR
        .get_or_init(|| std::sync::Arc::new(reqwest::cookie::Jar::default()))
        .clone()
}

static DF_CLIENT: OnceLock<Mutex<reqwest::Client>> = OnceLock::new();
fn df_client() -> &'static Mutex<reqwest::Client> {
    DF_CLIENT.get_or_init(|| {
        Mutex::new(
            reqwest::Client::builder()
                .cookie_provider(df_jar())
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        )
    })
}

// ── 微信登录独立 cookie jar/client ──
// 关键：open.weixin.qq.com 属于 *.qq.com 域，不能与 QQ 登录（ptlogin2.qq.com 的
// qrsig/skey/uin/skey 等）共用 cookie jar，否则 QQ 的 qq.com 域 cookie 会串到微信
// qrconnect 会话，导致微信二维码 404 秒失效 / 扫码换取失败。微信流程必须独立会话。
static WX_JAR: OnceLock<std::sync::Arc<reqwest::cookie::Jar>> = OnceLock::new();
fn wx_jar() -> std::sync::Arc<reqwest::cookie::Jar> {
    WX_JAR
        .get_or_init(|| std::sync::Arc::new(reqwest::cookie::Jar::default()))
        .clone()
}
static WX_CLIENT: OnceLock<Mutex<reqwest::Client>> = OnceLock::new();
fn wx_client() -> &'static Mutex<reqwest::Client> {
    WX_CLIENT.get_or_init(|| {
        Mutex::new(
            reqwest::Client::builder()
                .cookie_provider(wx_jar())
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        )
    })
}

/// 向共享 jar 注入一条 cookie（Domain 限定 .wegame.com.cn）
fn jar_add_wegame_cookie(name: &str, value: &str) {
    let jar = df_jar();
    let url = url::Url::parse("https://www.wegame.com.cn/").unwrap_or_else(|_| url::Url::parse("https://wegame.com.cn/").unwrap());
    let _ = jar.add_cookie_str(
        &format!(r#"{name}={value}; Domain=.wegame.com.cn; Path=/; Secure"#),
        &url,
    );
    log::info!("[DfStats] jar cookie: {name}={}", value.chars().take(20).collect::<String>());
}

/// 登录换取响应中的 wegame.com.cn 域 Set-Cookie 全集（显式携带兜底）
static DF_LOGIN_COOKIES: OnceLock<Mutex<String>> = OnceLock::new();
fn login_cookies_store() -> &'static Mutex<String> {
    DF_LOGIN_COOKIES.get_or_init(|| Mutex::new(String::new()))
}
fn set_login_cookies(c: String) {
    *login_cookies_store().lock().unwrap() = c;
}
fn get_login_cookies() -> String {
    login_cookies_store().lock().unwrap().clone()
}

/// ── 登录 cookie 持久化：退出工具箱后仍可恢复登录态 ──
/// 登录成功后把全部 key cookie 存到 cookies.json；应用启动 / 需要时从磁盘恢复进 DF_JAR。
fn cookies_file() -> PathBuf {
    data_dir().join("cookies.json")
}

fn save_cookie_snapshot(cookies: &std::collections::HashMap<String, String>) {
    if let Ok(json) = serde_json::to_string_pretty(cookies) {
        std::fs::write(cookies_file(), json).ok();
        log::info!("[DfStats] cookie snapshot saved: {} entries", cookies.len());
    }
}

fn load_cookie_snapshot() -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(cookies_file())
        .ok()
        .and_then(|s| serde_json::from_str::<std::collections::HashMap<String, String>>(&s).ok())
        .unwrap_or_default()
}

/// 将磁盘快照的 cookie 恢复进共享 jar（启动时/首次采集前调用）
pub fn restore_cookies_from_disk() {
    let cookies = load_cookie_snapshot();
    if cookies.is_empty() {
        return;
    }
    for (name, value) in &cookies {
        if !name.is_empty() && !value.is_empty() {
            jar_add_wegame_cookie(name, value);
        }
    }
    // 同时恢复显式 Cookie 串供 do_collect 使用
    let joined = cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    if !joined.is_empty() {
        set_login_cookies(joined);
    }
    log::info!("[DfStats] restored {} cookies from disk", cookies.len());
}

/// 生成 QQ 登录二维码：调用 ptqrshow 获取二维码图片 + qrsig cookie
#[tauri::command]
pub async fn df_stats_qr_gen() -> Result<serde_json::Value, String> {
    let client = df_client().lock().unwrap().clone().clone();
    let ts = now_ms();
    let url = format!(
        "https://ssl.ptlogin2.qq.com/ptqrshow?appid={QQ_APPID}&e=2&l=M&s=3&d=72&v=4&t={ts}&daid={QQ_DAID}&pt_3rd_aid=0&u1={QQ_U1}&pt_estags=2"
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let cookies: Vec<String> = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .collect();
    let qrsig = cookies
        .iter()
        .find(|c| c.starts_with("qrsig="))
        .and_then(|c| c.split(';').next())
        .map(|s| s.trim_start_matches("qrsig=").to_string())
        .unwrap_or_default();

    let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    use base64::Engine;
    let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    log::info!("[DfStats] qr_gen qrsig.len={} img={}B", qrsig.len(), bytes.len());
    Ok(serde_json::json!({
        "qr_base64": qr_b64,
        "qrsig": qrsig,
        "appid": QQ_APPID,
        "daid": QQ_DAID,
        "ts": ts,
    }))
}

/// 生成微信登录二维码：open.weixin.qq.com qrconnect → uuid → /connect/qrcode/<uuid>
#[tauri::command]
pub async fn df_stats_qr_wx_gen() -> Result<serde_json::Value, String> {
    let client = wx_client().lock().unwrap().clone();
    let redir = urlencoding(&WX_REDIRECT);
    let login_url = format!(
        "https://open.weixin.qq.com/connect/qrconnect?appid={WX_APPID}&redirect_uri={redir}&response_type=code&scope=snsapi_login&state=dfstats#wechat_redirect"
    );
    // 先访问登录页，让服务端种下必要 cookie（wg/uaid 等）
    let login_resp = client
        .get(&login_url)
        .header("Referer", "https://www.wegame.com.cn/")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let html = login_resp.text().await.map_err(|e| e.to_string())?;

    // 提取 uuid：页面里形如 window.wx_errcode / G="uuid" / 直接硬编码在 JS 变量 G
    let uuid = extract_wx_uuid(&html).ok_or_else(|| "无法从微信登录页提取 uuid".to_string())?;

    // 拉二维码图
    let qr_url = format!("https://open.weixin.qq.com/connect/qrcode/{uuid}");
    let qr_resp = client
        .get(&qr_url)
        .header("Referer", &login_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let img_type = qr_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = qr_resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
    use base64::Engine;
    let qr_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    log::info!("[DfStats] wx_qr_gen uuid={} img={}B", uuid, bytes.len());
    Ok(serde_json::json!({
        "qr_base64": qr_b64,
        "uuid": uuid,
        "img_type": img_type,
        "ts": now_ms(),
    }))
}

/// QQ 扫码轮询：ptqrlogin。扫码成功后自动换 tgp cookie 并采集落盘。
/// 前端每 2s 调用；响应含 ok/code 字段，ok==true 表示登录完成。
#[tauri::command]
pub async fn df_stats_qr_poll(
    app: AppHandle,
    qrsig: String,
    ptqrtoken: String,
) -> Result<serde_json::Value, String> {
    let client = df_client().lock().unwrap().clone();
    let ts = now_ms();
    let url = format!(
        "https://ssl.ptlogin2.qq.com/ptqrlogin?u1={QQ_U1}&ptqrtoken={ptqrtoken}&ptredirect=0&h=1&t=1&g=1&from_ui=1&ptlang=2052&action=0-0-{ts}&js_ver=22072012&js_type=1&login_sig=&pt_uistyle=40&aid={QQ_APPID}&daid={QQ_DAID}&o1v=3&pt_3rd_aid=0&verify_ignore=&pt_estags=2"
    );
    let resp = client
        .get(&url)
        .header("Referer", "https://xui.ptlogin2.qq.com/")
        .header("Cookie", format!("qrsig={qrsig}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    log::info!("[DfStats] ptqrlogin resp: {text}");
    // 解析 ptuiCB('code','','url','','msg','')
    let code = if text.contains("'0'") && (text.contains("登录成功") || text.contains("url")) {
        "0"
    } else if text.contains("'66'") {
        "66"
    } else if text.contains("'65'") {
        "65"
    } else {
        "-1"
    };
    // 提取跳转 URL（登录成功时含 clientuin/key）
    let mut jump_url = String::new();
    if let Some(start) = text.find("ptuiCB(") {
        let inner = &text[start + 7..];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() >= 3 {
            jump_url = parts[2].trim_matches('\'').to_string();
        }
    }
    if code == "0" && !jump_url.is_empty() {
        // 用「不自动跟随重定向」的 client 手动逐跳收集 Set-Cookie
        let chase = reqwest::Client::builder()
            .cookie_store(true)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36")
            .build()
            .map_err(|e| e.to_string())?;

        let mut uin = regex_extract(&jump_url, r"[?&](?:clientuin|uin)=(\d+)").unwrap_or_default();
        // ptqrlogin 成功 URL 里的 key 即短期 skey
        let mut sig = regex_extract(&jump_url, r"[?&](?:skey|key)=([^&\s]+)").unwrap_or_default();
        let login_sig = regex_extract(&jump_url, r"login_sig=([^&\s]+)").unwrap_or_default();

        // 主动请求 QQ 域 jump 端点，让服务端种下长期令牌 p_skey / p_uin（约 30 天）
        if !uin.is_empty() && !sig.is_empty() {
            let jump_endpoint = format!(
                "https://ssl.ptlogin2.qq.com/jump?clientuin={uin}&key={sig}&u1={QQ_U1}&pt_local_tk=&daid={QQ_DAID}&pt_3rd_aid=0&pt_estags=2&client_type=1"
            );
            match chase
                .get(&jump_endpoint)
                .header("Referer", "https://xui.ptlogin2.qq.com/")
                .send()
                .await
            {
                Ok(r) => {
                    for h in r.headers().get_all(reqwest::header::SET_COOKIE) {
                        if let Ok(hs) = h.to_str() {
                            let basic = hs.split(';').next().unwrap_or("").trim().to_string();
                            let kv: Vec<&str> = basic.splitn(2, '=').collect();
                            if kv.len() == 2 {
                                let key = kv[0].trim();
                                let val = kv[1].to_string();
                                match key {
                                    "p_skey" | "pskey" => if !val.is_empty() { sig = val.clone(); },
                                    "p_uin" | "puin" => {
                                        let v = val.trim_start_matches('o').trim_start_matches('0');
                                        if !v.is_empty() { uin = v.to_string(); }
                                    }
                                    _ => {}
                                }
                                log::info!("[DfStats] jump set-cookie {key} len={} domain_has_qq={}", val.len(), hs.to_lowercase().contains("qq.com"));
                            }
                        }
                    }
                    // 若 jump 响应是跳转（304/302），跟随最后到 wegame 回调页
                    if r.status().is_redirection() {
                        if let Some(loc) = r.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()) {
                            let mut hop = if loc.starts_with("http") { loc.to_string() } else {
                                format!("https://www.wegame.com.cn{loc}")
                            };
                            let mut step = 0;
                            while step < 6 {
                                let hop_resp = match chase.get(&hop).header("Referer", "https://xui.ptlogin2.qq.com/").send().await {
                                    Ok(x) => x,
                                    Err(_) => break,
                                };
                                if hop_resp.status().is_redirection() {
                                    if let Some(l2) = hop_resp.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()) {
                                        hop = if l2.starts_with("http") { l2.to_string() } else { format!("https://www.wegame.com.cn{l2}") };
                                        step += 1;
                                        continue;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => log::warn!("[DfStats] jump call failed: {e}"),
            }
        }

        let mut hop = jump_url.clone();
        let mut hop_idx = 0;
        for _ in 0..16 {
            hop_idx += 1;
            let r = match chase
                .get(&hop)
                .header("Referer", "https://xui.ptlogin2.qq.com/")
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => { log::warn!("[DfStats] chase hop failed: {e}"); break; }
            };
            log::info!("[DfStats] chase hop#{hop_idx} => status={} url={}", r.status().as_u16(), hop.chars().take(120).collect::<String>());
            for h in r.headers().get_all(reqwest::header::SET_COOKIE) {
                if let Ok(hs) = h.to_str() {
                    let basic = hs.split(';').next().unwrap_or("").trim().to_string();
                    let kv: Vec<&str> = basic.splitn(2, '=').collect();
                    if kv.len() == 2 {
                        let key = kv[0].trim();
                        let val = kv[1].to_string();
                        match key {
                            // p_skey/p_uin 是 QQ 域长期令牌（约 30 天），必须优先收集
                            "p_skey" | "pskey" => if !val.is_empty() { sig = val.clone(); },
                            "p_uin" | "puin" => {
                                let v = val.trim_start_matches('o').trim_start_matches('0');
                                if !v.is_empty() { uin = v.to_string(); }
                            }
                            "skey" | "key" => if sig.is_empty() && !val.is_empty() { sig = val.clone(); }
                            "uin" => {
                                let v = val.trim_start_matches('o').trim_start_matches('0');
                                if !v.is_empty() && uin.is_empty() { uin = v.to_string(); }
                            }
                            _ => {}
                        }
                        let dl = hs.to_lowercase();
                        let domain = dl.split(';').find(|p| p.contains("domain=")).unwrap_or("").trim();
                        log::info!("[DfStats] chase set-cookie {key} len={} domain={domain}", val.len());
                    }
                }
            }
            if r.status().is_redirection() {
                if let Some(loc) = r.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()) {
                    hop = if loc.starts_with("http") { loc.to_string() } else {
                        let base = "https://www.wegame.com.cn";
                        format!("{base}{loc}")
                    };
                    continue;
                }
            }
            // 到达最终页（回调页），收集完毕
            break;
        }
        log::info!("[DfStats] qq chase done uin={} sig_len={} login_sig_present={}", uin, sig.len(), !login_sig.is_empty());
        if !uin.is_empty() && !sig.is_empty() {
            return df_stats_finish_login(app.clone(), "qq".to_string(), sig, uin, String::new()).await;
        }
    }
    Ok(serde_json::json!({
        "code": if code == "0" { "-1" } else { code },
        "ok": false,
        "raw": text.chars().take(300).collect::<String>(),
        "qrsig": qrsig,
        "url": jump_url,
    }))
}

/// 微信扫码轮询：lp.open.weixin.qq.com/connect/l/qrconnect?uuid=...
/// 返回 JSONP：window.wx_errcode=408(待扫码)/405(已扫码带 code)/404(过期)
#[tauri::command]
pub async fn df_stats_qr_wx_poll(
    app: AppHandle,
    uuid: String,
) -> Result<serde_json::Value, String> {
    let client = wx_client().lock().unwrap().clone();
    let url = format!("https://lp.open.weixin.qq.com/connect/l/qrconnect?uuid={uuid}");
    // 微信 qrconnect 长轮询端点：请求可能长挂不返回（>30s）。
    // 给单次请求 12s 硬超时 + connect 3s，避免后端协程堆积；超时/网络抖动时
    // 返回 "retry" 让前端继续轮询（不产生真实 404 语义）。
    let resp = match tokio::time::timeout(
        std::time::Duration::from_secs(12),
        client
            .get(&url)
            .header("Referer", "https://open.weixin.qq.com/connect/qrconnect")
            .send(),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            log::warn!("[DfStats] wx poll send err: {e}");
            return Ok(serde_json::json!({
                "state": "retry",
                "errcode": "net_err",
                "code": "",
            }));
        }
        Err(_) => {
            log::warn!("[DfStats] wx poll timed out (12s), returning retry");
            return Ok(serde_json::json!({
                "state": "retry",
                "errcode": "timeout",
                "code": "",
            }));
        }
    };
    let text = resp.text().await.map_err(|e| e.to_string())?;
    log::info!("[DfStats] wx poll resp: {text}");
    // window.wx_errcode=405;window.wx_code='xxx';
    let errcode = regex_extract(&text, r"wx_errcode=(\d+)").unwrap_or_default();
    let code = regex_extract(&text, r"wx_code='([^']*)'").unwrap_or_default();
    let state = match errcode.as_str() {
        "405" => "scanned",   // 已扫码授权，code 就绪
        "408" => "waiting",   // 等待扫码
        "404" => "expired",   // 二维码过期
        _ => "unknown",
    };
    if errcode.is_empty() {
        // 响应不含 wx_errcode（异常页面/验证拦截）→ 按 retry 处理，避免误判过期
        log::warn!("[DfStats] wx poll no errcode, raw: {}", text.chars().take(120).collect::<String>());
        return Ok(serde_json::json!({
            "state": "retry",
            "errcode": "no_errcode",
            "code": "",
            "raw": text.chars().take(300).collect::<String>(),
        }));
    }
    if state == "scanned" && !code.is_empty() {
        return df_stats_finish_login(app, "wx".to_string(), String::new(), String::new(), code).await;
    }
    Ok(serde_json::json!({
        "state": state,
        "errcode": errcode,
        "code": code,
        "raw": text.chars().take(300).collect::<String>(),
    }))
}

/// 登录成功后的 WeGame cookie 版 Dfm API 采集（Rust 直连，无需 WebView）
/// - QQ：直接复用 df_client（qrsig→ptqrlogin 成功已写入 qq 域 cookie；再调 login_by_qq 换 tgp cookie）
/// - 微信：复用 df_client（先 wx qrconnect 换 code，再调 login_by_wechat 换 tgp cookie）
/// 说明：ptqrlogin 返回的跳转 URL 已带 clientuin/skey/sig，本函数用它们调 /api/middle/clientapi/auth/login_by_qq 换
/// WeGame 域 tgp_* cookie，之后 Dfm API 用同一 client 直连。
#[tauri::command]
pub async fn df_stats_finish_login(
    app: AppHandle,
    method: String,       // "qq" | "wx"
    sig: String,          // QQ:ptqrlogin 成功 url 里的 sig；微信空
    uin: String,          // QQ:clientuin；微信空
    code: String,         // 微信:qrconnect 返回的授权 code；QQ 空
) -> Result<serde_json::Value, String> {
    let method_l = method.to_lowercase();
    // 微信方法必须用微信独立 client（带 qrconnect 会话 cookie）；QQ 用共享 df_client
    let client = if method_l == "wx" || method_l == "wechat" {
        wx_client().lock().unwrap().clone()
    } else {
        df_client().lock().unwrap().clone()
    };
    let mut info = serde_json::Map::new();
    let (login_info, api) = if method_l == "wx" || method_l == "wechat" {
        info.insert("wx_info_type".into(), serde_json::Value::from(1));
        info.insert("appid".into(), serde_json::Value::from(WX_APPID));
        info.insert("code".into(), serde_json::Value::from(code));
        (info, "login_by_wechat")
    } else {
        info.insert("qq_info_type".into(), serde_json::Value::from(6));
        info.insert("uin".into(), serde_json::Value::from(uin.clone()));
        info.insert("sig".into(), serde_json::Value::from(sig.clone()));
        (info, "login_by_qq")
    };
    let body = serde_json::json!({
        "clienttype": WX_CLIENTTYPE,
        "mappid": "10001",
        "mcode": "",
        "config_params": { "lang_type": 0 },
        "login_info": login_info,
    });
    let url = format!("https://www.wegame.com.cn/api/middle/clientapi/auth/{api}");
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Referer", "https://www.wegame.com.cn/login/callback.html")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    // 诊断：dump 登录换取响应的全部 Set-Cookie（定位 Dfm API 鉴权 cookie）
    // 同时把 wegame.com.cn 域的可带 cookie 存到全局串（do_collect 显式携带兜底）
    let mut set_cookie_parts: Vec<String> = Vec::new();
    for h in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(hs) = h.to_str() {
            log::info!("[DfStats] {api} Set-Cookie: {hs}");
            let lower = hs.to_lowercase();
            // 只收集 wegame.com.cn 域 且非 HttpOnly（HttpOnly 无法在显式头外携带，但这里显式带没问题）
            if lower.contains("domain=wegame.com.cn") || lower.contains("domain=.wegame.com.cn") {
                let kv: Vec<&str> = hs.splitn(2, '=').collect();
                if kv.len() == 2 {
                    let val = kv[1].split(';').next().unwrap_or("").to_string();
                    set_cookie_parts.push(format!("{}={}", kv[0].trim(), val));
                }
            }
        }
    }
    if !set_cookie_parts.is_empty() {
        let joined = set_cookie_parts.join("; ");
        set_login_cookies(joined.clone());
        log::info!("[DfStats] login set-cookie string ready len={}", joined.len());
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    log::info!("[DfStats] {api} resp: {}", text.chars().take(400).collect::<String>());
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::json!({"raw": text}));
    let code_v = v.pointer("/data/result").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1);
    if code_v != 0 {
        // 该次扫码换取失败：尝试匹配微信 / 微信第三方防盗背后的真实原因
        return Ok(serde_json::json!({
            "ok": false,
            "err": v.pointer("/data/errmsg").and_then(|x| x.as_str()).unwrap_or("登录态换取失败"),
            "code": code_v,
            "raw": text.chars().take(300).collect::<String>(),
        }));
    }
    // 换取成功：组装 Dfm API 鉴权 cookie 注入共享 jar（官方 JS 写 document.cookie）
    let account_type = if method_l == "wx" || method_l == "wechat" { 2 } else { 1 };
    let user_info = v.pointer("/data/user_info").cloned().unwrap_or(serde_json::Value::Null);
    let tgpid = user_info.get("tgpid").map(|x| x.as_i64().unwrap_or(0)).unwrap_or(0);
    let wx_third = if method_l == "wx" || method_l == "wechat" { "1" } else { "0" };

    // 注入 tgp_* 到共享 jar
    if tgpid > 0 {
        jar_add_wegame_cookie("tgp_id", &tgpid.to_string());
    }
    jar_add_wegame_cookie("tgp_user_type", wx_third);
    jar_add_wegame_cookie("tgp_env", "online");
    if let Some(openid) = v.pointer("/data/openid").and_then(|x| x.as_str()) {
        if !openid.is_empty() {
            jar_add_wegame_cookie("tgp_third_openid", openid);
        }
    }
    // QQ 登录额外带 uin / pt2gguin / skey（Dfm API 鉴权核心 cookie，SDK logout 清理的就是 uin+skey）
    if method_l != "wx" && method_l != "wechat" && !uin.is_empty() {
        jar_add_wegame_cookie("uin", &uin);
        jar_add_wegame_cookie("pt2gguin", &format!("o0{uin}"));
        if !sig.is_empty() {
            jar_add_wegame_cookie("skey", &sig);
            log::info!("[DfStats] skey injected len={}", sig.len());
        } else {
            log::warn!("[DfStats] qq login ok but sig empty, skey not injected");
        }
    }

    // 持久化 cookie 快照（退出工具箱后仍可恢复登录态）
    {
        let mut snap = std::collections::HashMap::new();
        if tgpid > 0 {
            snap.insert("tgp_id".to_string(), tgpid.to_string());
        }
        snap.insert("tgp_user_type".to_string(), wx_third.to_string());
        snap.insert("tgp_env".to_string(), "online".to_string());
        if let Some(openid) = v.pointer("/data/openid").and_then(|x| x.as_str()) {
            if !openid.is_empty() {
                snap.insert("tgp_third_openid".to_string(), openid.to_string());
            }
        }
        if method_l != "wx" && method_l != "wechat" && !uin.is_empty() {
            snap.insert("uin".to_string(), uin.clone());
            snap.insert("pt2gguin".to_string(), format!("o0{uin}"));
            if !sig.is_empty() {
                snap.insert("skey".to_string(), sig.clone());
            }
        }
        // 并入 login_by_qq/wx 响应 Set-Cookie 里的会话 cookie（tgp_ticket 等）
        for part in get_login_cookies().split(';') {
            let kv: Vec<&str> = part.trim().splitn(2, '=').collect();
            if kv.len() == 2 {
                snap.insert(kv[0].trim().to_string(), kv[1].trim().to_string());
            }
        }
        save_cookie_snapshot(&snap);
    }

    // 用 user_info 构造登录态（头像/昵称/级别），openid 兜底用 data.openid
    let openid = v.pointer("/data/openid").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let nickname = user_info.get("new_name")
        .or_else(|| user_info.get("nick"))
        .and_then(|x| x.as_str())
        .unwrap_or("已登录")
        .to_string();
    let avatar = user_info.get("picurl").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let login_state = DfLoginState {
        logged_in: true,
        area: "36".to_string(),
        openid: if openid.is_empty() { format!("{tgpid}_10001") } else { openid },
        nickname,
        avatar,
        level: user_info.get("judge_level").map(|x| x.to_string()).unwrap_or_default(),
        tgp_id: if tgpid > 0 { tgpid.to_string() } else { uin.clone() },
        account_type,
        login_time: now_ms(),
    };
    save_login_state(&login_state);
    *login_store().lock().unwrap() = Some(login_state.clone());
    // 登录成功：复位会话失效标记
    session_broken_set(false);

    // 清除旧账号的采集数据（防旧微信账号数据污染新登录展示）
    if let Ok(entries) = std::fs::read_dir(data_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("data_") && name != "data_role_info.json" && name != "data_collect_done.json" {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let _ = app.emit("df-stats-login-changed", login_state.clone());

    // 采集（失败不阻断登录，采集结果逐步 emit）
    let mut collect = match df_stats_do_collect(&client, account_type).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[DfStats] collect failed (login still ok): {e}");
            std::collections::HashMap::new()
        }
    };
    // 用 role_info 回填登录态昵称（QQ 登录 user_info 无 nick，昵称在角色数据里）
    if let Some(ri) = collect.get("role_info").cloned() {
        let _ = std::fs::write(data_dir().join("data_role_info.json"), serde_json::to_string(&ri).unwrap_or_default());
        if let Some(nm) = ri.get("name").and_then(|v| v.as_str()) {
            if !nm.is_empty() && login_state.nickname.is_empty() {
                let mut st = login_state.clone();
                st.nickname = nm.to_string();
                save_login_state(&st);
                *login_store().lock().unwrap() = Some(st.clone());
                let _ = app.emit("df-stats-login-changed", st.clone());
            }
        }
    }
    for (kind, val) in &collect {
        if !kind.starts_with("__") {
            let safe = kind.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
            let _ = std::fs::write(data_dir().join(format!("data_{safe}.json")), serde_json::to_string(val).unwrap_or_default());
            let _ = app.emit("df-stats-data", (kind.clone(), val.clone()));
        }
    }
    // 确保前端能读到 role_info（若采集失败至少回显登录态）
    if !collect.contains_key("role_info") {
        collect.insert("role_info".to_string(), serde_json::json!({
            "openid": login_state.openid,
            "area": 36,
            "name": login_state.nickname,
            "icon": login_state.avatar,
            "level": login_state.level,
            "tgp_id": login_state.tgp_id,
        }));
    }
    Ok(serde_json::json!({
        "ok": true,
        "code": "0",
        "login_state": login_state,
        "user_info": user_info,
        "collect_count": collect.len(),
    }))
}

/// Rust 直连 Dfm API 全量采集（与 collect_js 等价，但无 WebView）
/// - QQ 登录：account_type=1
/// - 微信登录：account_type=2
/// - 鉴权 cookie 已注入共享 jar，reqwest 自动携带
pub async fn df_stats_do_collect(
    client: &reqwest::Client,
    account_type: i64,
) -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
    // 自动续期：短期会话(tgp_ticket/wt)过期时，用持久化的 uin+skey 重新换取，避免登录失效
    let _ = try_renew_session(client).await;
    let caller = "wegame.pallas.web.DfmBattle";
    let api = "https://www.wegame.com.cn/api/v1/wegame.pallas.dfm.DfmBattle";
    async fn call(
        client: &reqwest::Client,
        api: &str,
        caller: &str,
        extra_cookie: &str,
        method: &str,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{api}/{method}");
        let mut rb = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("trpc-caller", caller)
            .header("Referer", "https://www.wegame.com.cn/helper/df/score/")
            .header("Origin", "https://www.wegame.com.cn")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9");
        if !extra_cookie.is_empty() {
            rb = rb.header("Cookie", extra_cookie);
        }
        rb.json(&data)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())
    }

    /// 对局列表分页拉取：官方接口单次最多返回 8 条（size 参数被服务端封顶），
    /// 需要 after 游标向后翻页。官方网页端从不翻页（只取第一页），游标语义无公开参考，
    /// 因此这里自动依次尝试候选游标（最后一条的 startTime → roomId）：
    /// 某个游标能拉到新数据即锁定该策略继续翻页，全部失败则保留第一页（与旧行为一致）。
    /// 返回值保持原始响应结构（首屏响应 + 合并去重后的完整列表），前端无需改动。
    async fn fetch_battle_list_paged(
        client: &reqwest::Client,
        api: &str,
        caller: &str,
        extra_cookie: &str,
        openid: &str,
        account_type: i64,
        queue: &str,
    ) -> serde_json::Value {
        let list_key = if queue == "tdm" { "tdms" } else { "sols" };
        let mut all: Vec<serde_json::Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        // —— 第 1 页：after=null ——
        let data1 = serde_json::json!({
            "from_src": "df_web",
            "openid": openid,
            "area": 36,
            "queue": queue,
            "account_type": account_type,
            "size": 50,
            "after": serde_json::Value::Null,
            "filters": []
        });
        let resp1 = match call(client, api, caller, extra_cookie, "GetBattleList", data1).await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[DfStats] GetBattleList({queue}) page1 failed: {e}");
                let mut obj = serde_json::Map::new();
                obj.insert("result".to_string(), serde_json::json!({"error_code": -1, "error_message": format!("fetch failed: {e}")}));
                obj.insert(list_key.to_string(), serde_json::Value::Array(vec![]));
                return serde_json::Value::Object(obj);
            }
        };
        let ok1 = resp1.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
        if !ok1 {
            let em = resp1.pointer("/result/error_message").and_then(|x| x.as_str()).unwrap_or("").to_string();
            log::warn!("[DfStats] GetBattleList({queue}) page1 not ok: {em}");
            return resp1;
        }
        let mut first_resp = resp1.clone();
        let items1 = resp1.get(list_key).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for it in &items1 {
            let key = it.get("roomId").and_then(|x| x.as_str())
                .or_else(|| it.get("startTime").and_then(|x| x.as_str()))
                .unwrap_or("").to_string();
            if !key.is_empty() && seen.insert(key) {
                all.push(it.clone());
            }
        }
        if items1.len() < 8 {
            log::info!("[DfStats] GetBattleList({queue}) first page has {} (<8), no pagination needed", all.len());
            first_resp[list_key] = serde_json::Value::Array(all);
            return first_resp;
        }

        // —— 候选游标字段：startTime → roomId ——
        let last1 = &items1[items1.len() - 1];
        let mut candidates: Vec<(&str, serde_json::Value)> = Vec::new();
        if let Some(v) = last1.get("startTime") {
            if !v.is_null() { candidates.push(("startTime", v.clone())); }
        }
        if let Some(v) = last1.get("roomId") {
            if !v.is_null() { candidates.push(("roomId", v.clone())); }
        }

        for (field, cursor0) in candidates {
            let mut new_in_strategy = 0usize;
            let mut cursor = cursor0.clone();
            for page in 1..15 {
                let data = serde_json::json!({
                    "from_src": "df_web",
                    "openid": openid,
                    "area": 36,
                    "queue": queue,
                    "account_type": account_type,
                    "size": 50,
                    "after": cursor,
                    "filters": []
                });
                let resp = match call(client, api, caller, extra_cookie, "GetBattleList", data).await {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!("[DfStats] GetBattleList({queue}) page {page} ({field} cursor) failed: {e}");
                        break;
                    }
                };
                let ok = resp.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
                if !ok {
                    let em = resp.pointer("/result/error_message").and_then(|x| x.as_str()).unwrap_or("");
                    log::warn!("[DfStats] GetBattleList({queue}) page {page} ({field} cursor) not ok: {em}");
                    break;
                }
                let items = resp.get(list_key).and_then(|v| v.as_array()).cloned().unwrap_or_default();
                if items.is_empty() {
                    break; // 该策略翻到底了
                }
                let mut added = 0usize;
                for it in &items {
                    let key = it.get("roomId").and_then(|x| x.as_str())
                        .or_else(|| it.get("startTime").and_then(|x| x.as_str()))
                        .unwrap_or("").to_string();
                    if !key.is_empty() && seen.insert(key) {
                        all.push(it.clone());
                        added += 1;
                    }
                }
                if added == 0 {
                    break; // 游标未推进（返回了同一页）：策略失败
                }
                new_in_strategy += added;
                if added < 8 {
                    break; // 不足一页：已是末页
                }
                match items.last().and_then(|x| x.get(field)).cloned() {
                    Some(v) if !v.is_null() => cursor = v,
                    _ => break,
                }
            }
            if new_in_strategy > 0 {
                log::info!("[DfStats] GetBattleList({queue}) cursor strategy '{field}' worked, total={}", all.len());
                break;
            }
            log::warn!("[DfStats] GetBattleList({queue}) cursor strategy '{field}' made no progress, trying next");
        }

        log::info!("[DfStats] GetBattleList({queue}) paged total={}", all.len());
        first_resp[list_key] = serde_json::Value::Array(all);
        first_resp
    }

    let mut out = std::collections::HashMap::new();
    let cookie_str = get_login_cookies();
    // 显式带 wegame 域 cookie（响应 Set-Cookie + jar 双保险），加浏览器头避开 tqos 702
    let role = match call(client, api, caller, &cookie_str, "GetRoleInfo", serde_json::json!({"from_src":"df_web","account_type":account_type,"area":36})).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[DfStats] GetRoleInfo call failed: {e}");
            out.insert("__collect_error".to_string(), e.into());
            return Ok(out);
        }
    };
    log::info!("[DfStats] GetRoleInfo resp: {}", role.to_string().chars().take(400).collect::<String>());
    let role_ok = role.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
    out.insert("role_info_raw".to_string(), role.clone());
    if !role_ok {
        let err_msg = role.pointer("/result/error_message").and_then(|x| x.as_str()).unwrap_or("role failed").to_string();
        log::warn!("[DfStats] GetRoleInfo not ok: {err_msg}");
        out.insert("__role_error".to_string(), err_msg.into());
        return Ok(out);
    }
    let openid = role.pointer("/role_info/openid").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let info = role.pointer("/role_info").cloned().unwrap_or(serde_json::Value::Null);
    out.insert("role_info".to_string(), info.clone());

    let mut tasks = Vec::new();
    tasks.push(("battle_report_sol", "GetBattleReport", serde_json::json!({"from_src":"df_web","openid":openid,"area":36,"account_type":account_type,"queue":"sol","sid":"0"})));
    tasks.push(("battle_report_tdm", "GetBattleReport", serde_json::json!({"from_src":"df_web","openid":openid,"area":36,"account_type":account_type,"queue":"tdm","sid":"0"})));
    // 对局列表：官方单次上限 8 条，需 after 游标翻页拉满
    out.insert("battle_list".to_string(), fetch_battle_list_paged(client, api, caller, &cookie_str, &openid, account_type, "sol").await);
    out.insert("battle_list_tdm".to_string(), fetch_battle_list_paged(client, api, caller, &cookie_str, &openid, account_type, "tdm").await);
    tasks.push(("collectibles", "GetCollectibles", serde_json::json!({"from_src":"df_web","openid":openid,"area":36,"account_type":account_type})));
    tasks.push(("daily_stats", "GetDailyStats", serde_json::json!({"from_src":"df_web","openid":openid,"area":36,"account_type":account_type})));
    // 赛季列表 + 干员/地图/皮肤字典（S1-S11 筛选用）
    out.insert("seasons".to_string(), fetch_season_list(&client).await);
    for (kind, v) in fetch_rainbow_dicts(&client).await {
        out.insert(kind, v);
    }

    for (kind, method, data) in tasks {
        match call(client, api, caller, &cookie_str, method, data).await {
            Ok(v) => {
                let ok = v.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
                if !ok {
                    let em = v.pointer("/result/error_message").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    log::warn!("[DfStats] {kind} not ok: {em}");
                }
                out.insert(kind.to_string(), v);
            }
            Err(e) => { log::warn!("[DfStats] {kind} failed: {e}"); }
        }
    }
    // 落盘
    for (kind, v) in &out {
        let safe = kind.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        if !kind.starts_with("__") {
            let _ = std::fs::write(data_dir().join(format!("data_{safe}.json")), serde_json::to_string(v).unwrap_or_default());
        }
    }
    // role_info 落盘（前端直接读）
    let _ = std::fs::write(data_dir().join("data_role_info.json"), serde_json::to_string(&info).unwrap_or_default());
    Ok(out)
}

/// 拉取赛季列表（rail data_filter），返回 { ok, seasons: [...] }
async fn fetch_season_list(client: &reqwest::Client) -> serde_json::Value {
    let url = "https://www.wegame.com.cn/api/rail/web/data_filter/game_config/query";
    let body = serde_json::json!({
        "data_names": "df_helper_score_season_list",
        "command": "list_all",
        "params": { "start_page": 0, "items_per_pager": 50, "filters": [] },
        "stamp": { "agent_client_language": "zh_CN" },
        "response_format": 0
    });
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Referer", "https://www.wegame.com.cn/helper/df/score/")
        .header("Accept", "application/json, text/plain, */*")
        .json(&body)
        .send()
        .await;
    match resp {
        Ok(r) => match r.json::<serde_json::Value>().await {
            Ok(v) => {
                let ok = v.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
                let items = v.pointer("/items").cloned().unwrap_or(serde_json::Value::Null);
                log::info!("[DfStats] seasons ok={ok} count={}", items.as_array().map(|a| a.len()).unwrap_or(0));
                serde_json::json!({ "ok": ok, "seasons": items })
            }
            Err(e) => { log::warn!("[DfStats] seasons parse failed: {e}"); serde_json::json!({ "ok": false, "seasons": [] }) }
        },
        Err(e) => { log::warn!("[DfStats] seasons fetch failed: {e}"); serde_json::json!({ "ok": false, "seasons": [] }) }
    }
}

/// 按赛季拉取战报（sid：""=总览，1~11=对应赛季），返回 season.stats
#[tauri::command]
pub async fn df_stats_battle_report_season(sid: String, queue: String) -> Result<serde_json::Value, String> {
    let client = df_client().lock().unwrap().clone();
    // 先尝试用持久化凭据续期短期会话（tgp_ticket）；失败则视为登录态过期
    let renewed = try_renew_session(&client).await.unwrap_or(false);
    if !renewed {
        return Ok(serde_json::json!({
            "ok": false,
            "session_valid": false,
            "reason": "session_expired",
            "stats": serde_json::Value::Null,
            "season": serde_json::Value::Null,
            "raw": serde_json::Value::Null,
        }));
    }
    let account_type = load_login_state().map(|s| s.account_type).unwrap_or(1);
    let openid = load_login_state().map(|s| s.openid).filter(|o| !o.is_empty()).unwrap_or_default();
    let cookie_str = get_login_cookies();
    let caller = "wegame.pallas.web.DfmBattle";
    let api = "https://www.wegame.com.cn/api/v1/wegame.pallas.dfm.DfmBattle";
    let data = serde_json::json!({
        "from_src": "df_web",
        "openid": openid,
        "area": 36,
        "account_type": account_type,
        "queue": queue,
        "sid": sid,
    });
    let mut rb = client
        .post(format!("{api}/GetBattleReport"))
        .header("Content-Type", "application/json")
        .header("trpc-caller", caller)
        .header("Referer", "https://www.wegame.com.cn/helper/df/score/")
        .header("Origin", "https://www.wegame.com.cn")
        .header("Accept", "application/json, text/plain, */*");
    if !cookie_str.is_empty() {
        rb = rb.header("Cookie", cookie_str);
    }
    let resp = rb.json(&data).send().await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    // 提取 season.stats（统一给前端）
    let stats = v.pointer("/season/stats").cloned().unwrap_or(serde_json::Value::Null);
    let season = v.pointer("/season").cloned().unwrap_or(serde_json::Value::Null);
    log::info!("[DfStats] battle_report season sid={sid} queue={queue} stats={}", stats.is_object());
    let ok = stats.is_object();
    Ok(serde_json::json!({ "ok": ok, "session_valid": true, "stats": stats, "season": season, "raw": v }))
}

/// 拉取干员 + 地图 + 皮肤字典（彩虹 CDN + ams），返回可并入 out 的 map
async fn fetch_rainbow_dicts(
    client: &reqwest::Client,
) -> std::collections::HashMap<String, serde_json::Value> {
    const AGENT_URL: &str = "https://jsonschema.qpic.cn/f17859b917badfa9a083ad92c9eca90b/b965f0631cd937fde49b54fe0b709276/agentInfo";
    const MAP_URL: &str = "https://jsonschema.qpic.cn/f17859b917badfa9a083ad92c9eca90b/558596903da9c0e12df38eee20dcfc36/map";
    const SKIN_URL: &str = "https://comm.ams.game.qq.com/ide/?instanceid=661959&sIdeFlow=xXpyy2&method=dfm/object.list&param=%7B%22primary%22%3A%22assets%22%7D";
    let mut out = std::collections::HashMap::new();
    async fn get_json(client: &reqwest::Client, url: &str, post: bool) -> Result<serde_json::Value, String> {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
        let resp = if post {
            client.post(url).header("User-Agent", ua).body("{}").send().await
        } else {
            client.get(url).header("User-Agent", ua).send().await
        };
        resp.map_err(|e| e.to_string())?
            .json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())
    }
    match get_json(client, AGENT_URL, false).await {
        Ok(v) => { out.insert("agents".to_string(), v.clone()); log::info!("[DfStats] agents dict loaded"); }
        Err(e) => log::warn!("[DfStats] agents dict failed: {e}"),
    }
    match get_json(client, MAP_URL, false).await {
        Ok(v) => { out.insert("maps".to_string(), v.clone()); log::info!("[DfStats] maps dict loaded"); }
        Err(e) => log::warn!("[DfStats] maps dict failed: {e}"),
    }
    match get_json(client, SKIN_URL, true).await {
        Ok(v) => { out.insert("skins".to_string(), v.clone()); log::info!("[DfStats] skins dict loaded"); }
        Err(e) => log::warn!("[DfStats] skins dict failed: {e}"),
    }
    out
}

/// 拉取单局对局详情（GetBattleDetail）：返回同队玩家明细 + 高价值带出物品
/// 返回 { ok, soldiers: [...] }，前端点击对局行时调用并弹层展示
#[tauri::command]
pub async fn df_stats_battle_detail(
    room_id: String,
    start_time: String,
    queue: String,
) -> Result<serde_json::Value, String> {
    let client = df_client().lock().unwrap().clone();
    // 先尝试用持久化凭据续期短期会话（tgp_ticket）；失败则视为登录态过期
    let renewed = try_renew_session(&client).await.unwrap_or(false);
    if !renewed {
        return Ok(serde_json::json!({
            "ok": false,
            "session_valid": false,
            "reason": "session_expired",
            "players": [],
            "raw": serde_json::Value::Null,
        }));
    }
    let account_type = load_login_state().map(|s| s.account_type).unwrap_or(1);
    let openid = load_login_state()
        .map(|s| s.openid)
        .filter(|o| !o.is_empty())
        .unwrap_or_default();
    let cookie_str = get_login_cookies();
    let caller = "wegame.pallas.web.DfmBattle";
    let api = "https://www.wegame.com.cn/api/v1/wegame.pallas.dfm.DfmBattle";
    let data = serde_json::json!({
        "from_src": "df_web",
        "roomId": room_id,
        "openid": openid,
        "area": 36,
        "queue": queue,
        "account_type": account_type,
        "start_time": start_time,
    });
    let mut rb = client
        .post(format!("{api}/GetBattleDetail"))
        .header("Content-Type", "application/json")
        .header("trpc-caller", caller)
        .header("Referer", "https://www.wegame.com.cn/helper/df/score/")
        .header("Origin", "https://www.wegame.com.cn")
        .header("Accept", "application/json, text/plain, */*");
    if !cookie_str.is_empty() {
        rb = rb.header("Cookie", cookie_str);
    }
    let resp = rb.json(&data).send().await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let ok = v.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
    // 诊断：落盘原始对局详情（含真实玩家字段 killCnt/killPlayer 语义），前端点击对局行后读取
    if ok {
        let _ = std::fs::write(data_dir().join("data_battle_detail_diag.json"), serde_json::to_string_pretty(&v).unwrap_or_default());
        log::info!("[DfStats] battle_detail diag saved");
    }
    // 合并战场上所有玩家（sol/tdm/brick），供前端对局详情展示
    let mut all: Vec<serde_json::Value> = Vec::new();
    for key in ["sol_players", "tdm_players", "brick_players"] {
        if let Some(arr) = v.pointer(&format!("/battle_detail/{key}")).and_then(|x| x.as_array()) {
            all.extend(arr.iter().cloned());
        }
    }
    log::info!("[DfStats] battle_detail ok={ok} players={}", all.len());
    Ok(serde_json::json!({
        "ok": ok,
        "session_valid": true,
        "players": all,
        "raw": v,
    }))
}

/// 抽出微信 qrconnect 页面里的 uuid
fn extract_wx_uuid(html: &str) -> Option<String> {
    // 形如 G="0418NgZW2mFo0w3G" 或 var G = "..." 或 uuid 直接出现在 qrcode 路径
    for pat in [
        r#"G="([0-9a-zA-Z_-]{10,})"#,
        r#"G='([0-9a-zA-Z_-]{10,})'"#,
        r#"var G\s*=\s*"([0-9a-zA-Z_-]{10,})""#,
        r#"qrcode/([0-9a-zA-Z_-]{10,})"#,
        r#"uuid=([0-9a-zA-Z_-]{10,})"#,
    ] {
        if let Some(m) = regex_extract(html, pat) {
            return Some(m);
        }
    }
    None
}

/// 正则提取第一个捕获组
fn regex_extract(text: &str, pat: &str) -> Option<String> {
    let re = regex::Regex::new(pat).ok()?;
    re.captures(text)
        .and_then(|c| c.get(1).or_else(|| c.get(0)))
        .map(|m| m.as_str().to_string())
}

/// URL 编码
fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 按赛季拉取各地图收益统计（官方 GetMapStats 接口）：
/// 返回 { ok, maps: [{ mapid, total, win, total_value, total_item, kill, assist, death, score, gametime }] }
#[tauri::command]
pub async fn df_stats_map_stats(sid: String, queue: String) -> Result<serde_json::Value, String> {
    let client = df_client().lock().unwrap().clone();
    // 先尝试用持久化凭据续期短期会话（tgp_ticket）；失败则视为登录态过期
    let renewed = try_renew_session(&client).await.unwrap_or(false);
    if !renewed {
        return Ok(serde_json::json!({
            "ok": false,
            "session_valid": false,
            "reason": "session_expired",
            "maps": serde_json::Value::Null,
            "raw": serde_json::Value::Null,
        }));
    }
    let account_type = load_login_state().map(|s| s.account_type).unwrap_or(1);
    let openid = load_login_state().map(|s| s.openid).filter(|o| !o.is_empty()).unwrap_or_default();
    let cookie_str = get_login_cookies();
    let caller = "wegame.pallas.web.DfmBattle";
    let api = "https://www.wegame.com.cn/api/v1/wegame.pallas.dfm.DfmBattle";
    // 全部地图 ID（含全部模式）
    let mapids: Vec<i64> = vec![
        2201, 2202, 2211, 2212, 2231, 2232, 2233, 2242, 2251, // 零号大坝
        8102, 8103, 8151, // 巴克什
        8901, 8902, 8921, 8922, 8923, // AZ3
        3901, 3902, 3951, // 航天基地
        1901, 1902, 1911, 1912, // 长弓溪谷
        8802, 8803, // 潮汐监狱
    ];
    let data = serde_json::json!({
        "openid": openid,
        "area": 36,
        "account_type": account_type,
        "sid": sid,
        "mapids": mapids,
        "queue": queue,
        "from_src": "df_web",
    });
    let mut rb = client
        .post(format!("{api}/GetMapStats"))
        .header("Content-Type", "application/json")
        .header("trpc-caller", caller)
        .header("Referer", "https://www.wegame.com.cn/helper/df/score/")
        .header("Origin", "https://www.wegame.com.cn")
        .header("Accept", "application/json, text/plain, */*");
    if !cookie_str.is_empty() {
        rb = rb.header("Cookie", cookie_str);
    }
    let resp = rb.json(&data).send().await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let ok = v.pointer("/result/error_code").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1) == 0;
    let maps = v.pointer("/maps").cloned().unwrap_or(serde_json::Value::Null);
    log::info!("[DfStats] map_stats sid={sid} queue={queue} ok={ok} maps={}", maps.as_array().map(|a| a.len()).unwrap_or(0));
    Ok(serde_json::json!({ "ok": ok, "session_valid": true, "maps": maps, "raw": v }))
}

/// ── 自动续期：短期会话（tgp_ticket/wt，约30分钟）过期时，用持久化的长期凭据重新换取 ──
/// 目标：退出工具箱 / 隔段时间后仍保持登录态，无需重新扫码。
/// 逻辑：
/// - 从 cookies.json 读持久化凭据（QQ:uin+skey / 微信:appid+code 无法复用，仅 QQ 支持）
/// - 调用 login_by_qq 重新换取短期会话 → 更新 jar + 磁盘快照
async fn try_renew_session(client: &reqwest::Client) -> Result<bool, String> {
    let cookies = load_cookie_snapshot();
    let uin = cookies.get("uin").cloned().unwrap_or_default();
    let skey = cookies.get("skey").cloned().unwrap_or_default();
    // 无 uin+skey 长期凭据：微信登录（会话凭据在 tgp_ticket，无 30 天 uin/skey）。
    // 这种场景返回 Ok(true) 表示「无需 QQ 续期即视为会话可用」，而非 false（false 会让
    // 采集命令误判微信会话失效 → 前端弹「登录信息失效」）。真正失效由 Dfm API 返回的
    // error_code 判定（各调用方已携带 jar 内 tgp_ticket 直发请求）。
    if uin.is_empty() || skey.is_empty() {
        return Ok(true);
    }
    let body = serde_json::json!({
        "clienttype": WX_CLIENTTYPE,
        "mappid": "10001",
        "mcode": "",
        "config_params": { "lang_type": 0 },
        "login_info": { "qq_info_type": 6, "uin": uin, "sig": skey },
    });
    let resp = client
        .post("https://www.wegame.com.cn/api/middle/clientapi/auth/login_by_qq")
        .header("Content-Type", "application/json")
        .header("Referer", "https://www.wegame.com.cn/login/callback.html")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    // 收集 Set-Cookie（tgp_ticket 等短期会话）更新到 jar + 快照
    let mut new_cookies: std::collections::HashMap<String, String> = cookies.clone();
    for h in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(hs) = h.to_str() {
            let lower = hs.to_lowercase();
            if lower.contains("domain=wegame.com.cn") || lower.contains("domain=.wegame.com.cn") {
                let kv: Vec<&str> = hs.splitn(2, '=').collect();
                if kv.len() == 2 {
                    let name = kv[0].trim().to_string();
                    let val = kv[1].split(';').next().unwrap_or("").to_string();
                    jar_add_wegame_cookie(&name, &val);
                    new_cookies.insert(name, val);
                }
            }
        }
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::json!({"raw": text}));
    let code_v = v.pointer("/data/result").map(|x| x.as_i64().unwrap_or(-1)).unwrap_or(-1);
    if code_v == 0 {
        // 续期成功：更新会话 cookie 到磁盘
        let joined = new_cookies
            .iter()
            .map(|(k, val)| format!("{k}={val}"))
            .collect::<Vec<_>>()
            .join("; ");
        set_login_cookies(joined);
        save_cookie_snapshot(&new_cookies);
        log::info!("[DfStats] session renewed via uin+skey (tgp_ticket updated)");
        // 重算 login_state 覆盖旧登录态
        if let Some(mut st) = load_login_state() {
            st.login_time = now_ms();
            save_login_state(&st);
            *login_store().lock().unwrap() = Some(st);
        }
        Ok(true)
    } else {
        log::warn!("[DfStats] session renew failed code={code_v}, may need re-scan");
        Ok(false)
    }
}