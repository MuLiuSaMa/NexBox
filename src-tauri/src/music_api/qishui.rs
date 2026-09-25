use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tokio::sync::oneshot;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow};

use super::cookie;
use super::models::{Artist, LoginInfo, Lyrics, Playlist, Song};

// ============================================================
//  汽水音乐扫码登录（Tauri 移植自 Mineradio 2.2.0 qishui-auth-v6）
//
//  核心机制（对标 Mineradio）：
//  - 隐藏 WebView2 加载官方 bdms 签名引擎（全内联进 security_host.html）
//  - passport 请求在页面内执行（bdms 自动注入 a_bogus/msToken 签名）
//  - 页面与接口同源：先导航到 https://api.qishui.com/，再 document.write
//    注入全内联页面。这样页面 origin = api.qishui.com，页面内 XHR 直连
//    api.qishui.com 无 CORS 限制，bdms 签名的也是真实 URL。
// ============================================================

const AUTH_WINDOW_LABEL: &str = "qishui-auth";

/// UTF-8 安全截断：避免在非字符边界做字节切片导致 panic
fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 判断 cookie 是否含汽水登录态 (sessionid/sessionid_ss/sid_guard/sid_tt)
pub fn qishui_cookie_has_login(cookie: &str) -> bool {
    let regexes = [
        r"(?:^|;\s*)sessionid=[^;\s]+",
        r"(?:^|;\s*)sessionid_ss=[^;\s]+",
        r"(?:^|;\s*)sid_guard=[^;\s]+",
        r"(?:^|;\s*)sid_tt=[^;\s]+",
    ];
    regexes.iter().any(|pat| {
        regex::Regex::new(pat)
            .map(|re| re.is_match(cookie))
            .unwrap_or(false)
    })
}

/// 构建汽水 cookie 优先级（参考 Mineradio 的 buildCookieString + persistSessionCookies）
const QISHUI_COOKIE_PRIORITY: &[&str] = &[
    "sessionid",
    "sessionid_ss",
    "sid_guard",
    "sid_tt",
    "sessionid_cross_app",
    "sessionid_ss_cross_app",
    "sid_guard_cross_app",
    "sid_tt_cross_app",
    "odin_tt",
    "passport_csrf_token",
    "passport_auth_status",
    "passport_auth_status_ss",
    "sid_uc",
    "uid_tt",
    "uid_tt_ss",
    "ttwid",
];

fn is_qishui_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    d == "qishui.com" || d.ends_with(".qishui.com")
}

/// 从 webview cookies 构建汽水 cookie 字符串（与 mod.rs build_cookie_from_webview 一致）
fn build_cookie_from_webview(
    cookies: &[tauri::webview::Cookie],
    priority: &[&str],
    domain_check: fn(&str) -> bool,
) -> String {
    use std::collections::HashMap;
    let mut picked: HashMap<String, String> = HashMap::new();
    for c in cookies {
        if let Some(domain) = c.domain() {
            if domain_check(domain) {
                let name = c.name().to_string();
                let value = c.value().to_string();
                if !name.is_empty() && !value.is_empty() {
                    picked.insert(name, value);
                }
            }
        }
    }
    let mut ordered: Vec<(String, String)> = Vec::new();
    for name in priority {
        if let Some(value) = picked.remove(*name) {
            ordered.push((name.to_string(), value));
        }
    }
    for (name, value) in picked {
        ordered.push((name, value));
    }
    ordered
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

// ------------------------------------------------------------
//  运行时（单例）
// ------------------------------------------------------------

struct QishuiAuthRuntime {
    app: AppHandle,
    ready: Arc<Mutex<bool>>,
    qr_token: Arc<Mutex<String>>,
}

static RUNTIME: OnceLock<Arc<QishuiAuthRuntime>> = OnceLock::new();

fn runtime(app: &AppHandle) -> Arc<QishuiAuthRuntime> {
    RUNTIME
        .get_or_init(|| {
            Arc::new(QishuiAuthRuntime {
                app: app.clone(),
                ready: Arc::new(Mutex::new(false)),
                qr_token: Arc::new(Mutex::new(String::new())),
            })
        })
        .clone()
}

impl QishuiAuthRuntime {
    /// 确保隐藏 webview 存在并注入同源安全验证页面
    async fn ensure_window(&self) -> Result<WebviewWindow, String> {
        if let Some(win) = self.app.get_webview_window(AUTH_WINDOW_LABEL) {
            return Ok(win);
        }
        // 先导航到 api.qishui.com（真实源）：页面内容加载失败也没关系，只要 origin 落在这个源
        let url = "https://api.qishui.com/".to_string();
        let win = tauri::WebviewWindowBuilder::new(
            &self.app,
            AUTH_WINDOW_LABEL,
            WebviewUrl::External(
                url.parse()
                    .map_err(|e: url::ParseError| format!("qishui-auth URL parse: {e}"))?,
            ),
        )
        .title("汽水音乐安全验证")
        .inner_size(980.0, 760.0)
        .min_inner_size(760.0, 600.0)
        .visible(false)
        // 与其它窗口保持一致的 WebView2 参数（必须与 tauri.conf.json 逐字相同）：
        // 参数不同会让 WebView2 为同一数据目录再建一个环境，创建控制器直接失败
        // （0x8007139F），窗口没有 webview，后面所有 eval 都会挂住。
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        .auto_resize()
        .build()
        .map_err(|e| format!("创建汽水验证窗口失败: {e}"))?;

        // 等待导航完成后注入全内联页面（blob 导航保持同源 origin）
        let mut on_origin = false;
        for _ in 0..60 {
            if let Ok(u) = win.url() {
                if u.as_str().contains("api.qishui.com") {
                    on_origin = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        if !on_origin {
            // 到不了该源（webview 创建失败/断网），窗口留着也没用：
            // 关掉重建，并把错误抛给前端，否则后面的 eval 会静默卡几分钟。
            let got = win.url().map(|u| u.to_string()).unwrap_or_default();
            let _ = win.close();
            return Err(format!("汽水验证页未能加载到 api.qishui.com（当前 {got}）"));
        }
        // 注入全内联页面。
        // 不能用 document.open/write：它会继承 api.qishui.com 页面（无 charset，中文
        // Windows 下为 GBK）的编码，导致外链验证组件 auth.zijieapi.com（响应无 charset）
        // 按 GBK 解码而报 "Invalid or unexpected token"，组件无法导出。
        // 改用 blob 导航：全新文档，HTML 里的 <meta charset="utf-8"> 生效 → UTF-8；
        // 且 blob origin = 创建者 origin = https://api.qishui.com，页面内 XHR 仍同源。
        let html = include_str!("../../resources/qishui-auth/security_host.html");
        let js = format!(
            "(function(){{var h={};var b=new Blob([h],{{type:'text/html;charset=utf-8'}});location.href=URL.createObjectURL(b);return true;}})()",
            serde_json::to_string(html).map_err(|e| e.to_string())?
        );
        win.eval(js).map_err(|e| format!("注入页面失败: {e}"))?;
        log::info!("[Qishui] 已导航到 UTF-8 blob 同源页面, len={}", html.len());
        Ok(win)
    }

    /// eval 页面内 JS 并等待 JSON 结果（通过 eval_with_callback + oneshot channel）
    async fn eval_sync(&self, js: &str) -> Result<String, String> {
        let win = self.ensure_window().await?;
        // 等待页面就绪（bridge 可用）
        for _ in 0..60 {
            let ready_js = "JSON.stringify({state: document.readyState, bridge: typeof window.__qishuiNewSeq !== 'undefined'})";
            match self.eval_oneshot(&win, ready_js, 5).await {
                Ok(raw) => {
                    let trimmed = raw.trim().trim_matches('"');
                    if trimmed.contains("complete") && trimmed.contains("true") {
                        break;
                    }
                }
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        self.eval_oneshot(&win, js, 20).await
    }

    /// 对指定窗口执行一次 eval_with_callback，返回 JSON 序列化结果
    async fn eval_oneshot(&self, win: &WebviewWindow, js: &str, timeout_secs: u64) -> Result<String, String> {
        let (tx, rx) = oneshot::channel::<String>();
        let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
        let js_owned = js.to_string();
        let tx_cb = tx.clone();
        win.eval_with_callback(js_owned, move |result| {
            if let Some(sender) = tx_cb.lock().unwrap().take() {
                let _ = sender.send(result);
            }
        })
        .map_err(|e| format!("eval failed: {e}"))?;
        let result = tokio::time::timeout(Duration::from_secs(timeout_secs), rx)
            .await
            .map_err(|_| "汽水登录接口超时".to_string())?
            .map_err(|_| "汽水登录接口未返回结果".to_string())?;
        Ok(result)
    }

    /// 执行一个 kick（同步返回 seq），然后轮询 __qishuiResults[seq] 直到就绪
    async fn kick_and_poll(&self, kick_js: &str, timeout_secs: u64) -> Result<serde_json::Value, String> {
        log::info!("[Qishui] kick: {}", clip(kick_js, 150));
        // 1. 执行 kick，同步拿到 seq（数字）
        let seq_raw = self.eval_sync(kick_js).await?;
        let seq_trimmed = seq_raw.trim().trim_matches('"');
        let seq: u32 = seq_trimmed
            .parse()
            .map_err(|_| format!("kick 未返回 seq: {seq_raw}"))?;
        log::info!("[Qishui] kick seq={}", seq);
        // 2. 轮询结果
        let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
        // JS 侧把结果存为对象（非字符串），ExecuteScript 会把它序列化为 JSON
        let poll_js = format!("window.__qishuiResults[{seq}]");
        loop {
            let raw = self.eval_sync(&poll_js).await?;
            let trimmed = raw.trim();
            if !trimmed.is_empty() && trimmed != "null" && trimmed != "\"null\"" {
                // ExecuteScript 对对象返回 JSON 字符串；对 null 返回 "null"
                let val = serde_json::from_str(trimmed)
                    .map_err(|e| format!("结果非 JSON: {e} | raw={}", clip(trimmed, 300)))?;
                log::info!("[Qishui] result seq={}: {}", seq, clip(trimmed, 1500));
                return Ok(val);
            }
            if std::time::Instant::now() > deadline {
                return Err("汽水登录接口超时（结果未就绪）".into());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// 初始化：等待 bdms 就绪
    async fn init(&self) -> Result<(), String> {
        if *self.ready.lock().unwrap() {
            return Ok(());
        }
        // 页面加载完成后轮询 bdms 就绪
        for _ in 0..120 {
            match self.kick_and_poll("window.__qishuiAuthReadyKick ? window.__qishuiAuthReadyKick() : (function(){return 0})()", 30).await {
                Ok(val) => {
                    let ok = val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                    if let Some(tf) = val.get("testFetch") {
                        log::info!("[Qishui] 直连测试 testFetch={}", tf);
                    }
                    if let Some(o) = val.get("origin") {
                        log::info!("[Qishui] 页面 origin={}", o);
                    }
                    log::info!("[Qishui] auth ready check: ok={} val={}", ok, val);
                    if ok {
                        *self.ready.lock().unwrap() = true;
                        log::info!("[Qishui] bdms 引擎就绪: {:?}", val);
                        return Ok(());
                    }
                }
                Err(e) => {
                    log::warn!("[Qishui] auth ready check failed: {e}");
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err("汽水安全组件初始化超时（bdms 未就绪）".into())
    }
}

// ------------------------------------------------------------
//  Tauri 命令
// ------------------------------------------------------------

/// 获取汽水音乐扫码二维码
/// 返回 { token, qrcode_url }（qrcode_url 为扫码 URL，前端用 qrcode.react 渲染）
#[tauri::command]
pub async fn qishui_qr_get(app: AppHandle) -> Result<serde_json::Value, String> {
    let rt = runtime(&app);
    rt.init().await?;
    let val = rt
        .kick_and_poll("window.__qishuiQrGetKick ? window.__qishuiQrGetKick() : (function(){return 0})()", 40)
        .await?;
    let ok = val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    if !ok {
        let err = val
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("QISHUI_QR_CREATE_FAILED")
            .to_string();
        return Err(err);
    }
    let token = val
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let scan_url = val
        .get("scan_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let expire_time = val.get("expire_time").and_then(|v| v.as_i64()).unwrap_or(0);
    if token.is_empty() || scan_url.is_empty() {
        return Err("汽水音乐二维码数据不完整".into());
    }
    // 保存当前 token 供 check 使用
    *rt.qr_token.lock().unwrap() = token.clone();
    log::info!("[Qishui] QR created, token.len={} expire={}", token.len(), expire_time);
    Ok(serde_json::json!({
        "token": token,
        "qrcode_url": scan_url,
        "expire_time": expire_time,
            "message": "请使用 汽水音乐App 扫码并确认登录",
    }))
}

/// 轮询汽水扫码登录状态
/// 返回 { status, logged_in, error_code, message }
#[tauri::command]
pub async fn qishui_qr_check(app: AppHandle, token: String) -> Result<serde_json::Value, String> {
    let rt = runtime(&app);
    rt.init().await?;
    let t = if token.is_empty() {
        rt.qr_token.lock().unwrap().clone()
    } else {
        token
    };
    if t.is_empty() {
        return Err("请先获取二维码".into());
    }
    let js = format!(
        "window.__qishuiQrCheckKick({})",
        serde_json::to_string(&t).map_err(|e| e.to_string())?
    );
    // 只有真正需要二次验证时才弹出验证窗口（并非所有账号都需要 MFA）。
    // 桥在进入 __qishuiSecondVerify 时置 window.__qishuiMfaActive=true，
    // 这里并行轮询该标志，进入 MFA 即显示窗口；check 结束后隐藏。
    let win = app.get_webview_window(AUTH_WINDOW_LABEL);
    let poll_win = win.clone();
    let rt_for_poll = rt.clone();
    let poller = tokio::spawn(async move {
        if let Some(w) = poll_win {
            for _ in 0..300 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if let Ok(raw) = rt_for_poll
                    .eval_oneshot(&w, "window.__qishuiMfaActive === true", 5)
                    .await
                {
                    if raw.contains("true") {
                        let _ = w.show();
                        let _ = w.set_focus();
                        break;
                    }
                }
            }
        }
    });
    let val = rt.kick_and_poll(&js, 60).await;
    poller.abort();
    if let Some(w) = &win {
        let _ = w.hide();
    }
    let val = val?;

    let confirmed = val.get("confirmed").and_then(|v| v.as_bool()).unwrap_or(false);
    let error_code = val.get("errorCode").and_then(|v| v.as_i64()).unwrap_or(0);
    let status = val
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("waiting")
        .to_string();
    let message = val
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if confirmed {
        // 登录成功：隐藏验证窗口，收集 qishui.com 域 cookie
        if let Some(win) = app.get_webview_window(AUTH_WINDOW_LABEL) {
            let _ = win.hide();
            match win.cookies() {
                Ok(cookies) => {
                    let cookie_str = build_cookie_from_webview(&cookies, QISHUI_COOKIE_PRIORITY, is_qishui_domain);
                    let has_login = qishui_cookie_has_login(&cookie_str);
                    log::info!(
                        "[Qishui] login confirmed, cookie len={} has_login={}",
                        cookie_str.len(),
                        has_login
                    );
                    if has_login {
                        let _ = cookie::save_cookie(&app, "qishui", &cookie_str);
                        crate::music_api::set_provider_cookie("qishui", cookie_str.clone()).await;
                        let info = qishui_fetch_profile(&cookie_str)
                            .await
                            .unwrap_or_else(|| qishui_status_info(&cookie_str));
                        let _ = app.emit("qishui-login-success", &info);
                        return Ok(serde_json::json!({
                            "status": "confirmed",
                            "logged_in": true,
                            "ok": true,
                            "error_code": 0,
                            "message": "登录成功",
                            "login_info": info,
                        }));
                    } else {
                        // cookie 未含登录态：退回 session_cookie
                        if let Some(sc) = val.get("session_cookie").and_then(|v| v.as_str()) {
                            let merged = if sc.is_empty() {
                                cookie_str.clone()
                            } else {
                                format!("{cookie_str}; {sc}")
                            };
                            if qishui_cookie_has_login(&merged) {
                                let _ = cookie::save_cookie(&app, "qishui", &merged);
                                crate::music_api::set_provider_cookie("qishui", merged.clone()).await;
                                let info = qishui_fetch_profile(&merged)
                                    .await
                                    .unwrap_or_else(|| qishui_status_info(&merged));
                                let _ = app.emit("qishui-login-success", &info);
                                return Ok(serde_json::json!({
                                    "status": "confirmed",
                                    "logged_in": true,
                                    "ok": true,
                                    "error_code": 0,
                                    "message": "登录成功",
                                    "login_info": info,
                                }));
                            }
                        }
                        log::warn!("[Qishui] confirmed but no login cookie found");
                        return Ok(serde_json::json!({
                            "status": "confirmed",
                            "logged_in": false,
                            "ok": false,
                            "error_code": 0,
                            "message": "登录成功但未能获取到完整 Cookie",
                        }));
                    }
                }
                Err(e) => {
                    log::warn!("[Qishui] read cookies failed: {e}");
                }
            }
        }
        return Ok(serde_json::json!({
            "status": "confirmed",
            "logged_in": false,
            "ok": false,
            "message": "登录确认但 cookie 读取失败",
        }));
    }

    // 未确认：返回扫码状态
    let mapped_status = match error_code {
        2 => "expired".to_string(),
        7 => "rate_limited".to_string(),
        _ if status == "2" => "scanned".to_string(),
        _ if status.is_empty() || status == "1" => "waiting".to_string(),
        _ if status == "mfa" => "mfa".to_string(),
        _ => status,
    };
    Ok(serde_json::json!({
        "status": mapped_status,
        "logged_in": false,
        "ok": false,
        "error_code": error_code,
        "message": message,
    }))
}

/// 汽水音乐登录状态（纯 cookie 判定，不请求网络）
pub fn qishui_status_info(cookie: &str) -> LoginInfo {
    let logged_in = qishui_cookie_has_login(cookie);
    LoginInfo {
        provider: "qishui".into(),
        logged_in,
        user_id: String::new(),
        nickname: String::new(),
        avatar: String::new(),
        vip_type: 0,
        vip_level: String::new(),
        is_vip: false,
        is_svip: false,
    }
}

/// 汽水音乐登录状态命令（带个人信息；网络失败时退回纯 cookie 判定）
#[tauri::command]
pub async fn qishui_status(app: AppHandle) -> Result<LoginInfo, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if let Some(info) = qishui_fetch_profile(&cookie).await {
        return Ok(info);
    }
    Ok(crate::music_api::cached_profile("qishui").unwrap_or_else(|| qishui_status_info(&cookie)))
}

/// 汽水音乐 Cookie 登录（手动粘贴 cookie）
#[tauri::command]
pub async fn qishui_login_cookie(app: AppHandle, cookie: String) -> Result<LoginInfo, String> {
    let normalized = cookie::normalize_cookie_header(&cookie);
    if !qishui_cookie_has_login(&normalized) {
        return Ok(qishui_status_info(&normalized));
    }
    cookie::save_cookie(&app, "qishui", &normalized)?;
    crate::music_api::set_provider_cookie("qishui", normalized.clone()).await;
    let info = qishui_fetch_profile(&normalized)
        .await
        .unwrap_or_else(|| qishui_status_info(&normalized));
    let _ = app.emit("qishui-login-success", &info);
    Ok(info)
}

/// 汽水音乐登出
#[tauri::command]
pub async fn qishui_logout(app: AppHandle) -> Result<serde_json::Value, String> {
    // 清空隐藏 webview 的存储
    if let Some(win) = app.get_webview_window(AUTH_WINDOW_LABEL) {
        let _ = win.clear_all_browsing_data();
        let _ = win.eval("window.__qishuiLogoutKick ? window.__qishuiLogoutKick() : '0'");
    }
    let _ = cookie::clear_cookie(&app, "qishui");
    crate::music_api::set_provider_cookie("qishui", String::new()).await;
    log::info!("[Qishui] logout");
    Ok(serde_json::json!({
        "provider": "qishui",
        "logged_in": false,
        "ok": true,
    }))
}

// ============================================================
//  汽水音乐 luna 接口（登录后：个人信息 / 歌单 / 曲目 / 搜索）
//  https://api.qishui.com/luna/pc/* ：只需登录 cookie + app 参数，
//  无需 bdms 签名（对照 Mineradio qishui-api.js）。
// ============================================================

const QISHUI_PC_UA: &str = "LunaPC/3.3.0(359450208)";

fn qishui_pc_params() -> Vec<(String, String)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let device_id = now.to_string();
    let iid = (now + 1).to_string();
    let pairs: Vec<(&str, String)> = vec![
        ("aid", "386088".into()),
        ("app_name", "luna_pc".into()),
        ("region", "cn".into()),
        ("geo_region", "cn".into()),
        ("os_region", "cn".into()),
        ("sim_region", "".into()),
        ("device_id", device_id.clone()),
        ("cdid", "".into()),
        ("iid", iid),
        ("version_name", "3.3.0".into()),
        ("version_code", "30030000".into()),
        ("channel", "official".into()),
        ("build_mode", "master".into()),
        ("network_carrier", "".into()),
        ("ac", "wifi".into()),
        ("tz_name", "Asia/Shanghai".into()),
        ("resolution", "".into()),
        ("device_platform", "windows".into()),
        ("device_type", "Windows".into()),
        ("os_version", "Windows 11".into()),
        ("fp", device_id),
    ];
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

async fn qishui_luna_get(
    cookie: &str,
    path: &str,
    extra: &[(&str, String)],
) -> Result<serde_json::Value, String> {
    let mut params = qishui_pc_params();
    for (k, v) in extra {
        params.push((k.to_string(), v.clone()));
    }
    let url = format!("https://api.qishui.com{path}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .query(&params)
        .header("Accept", "application/json,text/plain,*/*")
        .header("User-Agent", QISHUI_PC_UA)
        .header("x-luna-background-type", "foreground")
        .header("x-luna-is-background-req", "0")
        .header("x-luna-is-local-user", "1")
        .header("Cookie", cookie)
        .header("Referer", "https://www.qishui.com/")
        .timeout(Duration::from_secs(9))
        .send()
        .await
        .map_err(|e| format!("汽水接口请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("汽水接口 HTTP {}: {}", status.as_u16(), clip(&text, 200)));
    }
    serde_json::from_str(&text).map_err(|e| format!("汽水接口返回无效 JSON: {e} | {}", clip(&text, 200)))
}

fn json_str(v: &serde_json::Value, keys: &[&str]) -> String {
    for k in keys {
        match v.get(*k) {
            Some(serde_json::Value::String(s)) if !s.is_empty() => return s.clone(),
            Some(serde_json::Value::Number(n)) => return n.to_string(),
            _ => {}
        }
    }
    String::new()
}

fn json_u64(v: &serde_json::Value, keys: &[&str]) -> u64 {
    for k in keys {
        match v.get(*k) {
            Some(serde_json::Value::Number(n)) => {
                if let Some(u) = n.as_u64() {
                    return u;
                }
            }
            Some(serde_json::Value::String(s)) => {
                if let Ok(u) = s.parse::<u64>() {
                    return u;
                }
            }
            _ => {}
        }
    }
    0
}

fn qishui_first_url(value: &serde_json::Value) -> String {
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(arr) = value.as_array() {
        for item in arr {
            let s = qishui_first_url(item);
            if !s.is_empty() {
                return s;
            }
        }
    }
    String::new()
}

/// 对照 Mineradio qishuiImageUrl：对象 {uri, urls[]} → urls[0] + uri + suffix
fn qishui_image_url(value: &serde_json::Value, suffix: &str) -> String {
    if value.is_null() {
        return String::new();
    }
    if let Some(s) = value.as_str() {
        if !s.starts_with("http") {
            return String::new();
        }
        if !suffix.is_empty() && !s.contains('~') {
            return format!("{s}{suffix}");
        }
        return s.to_string();
    }
    if let Some(arr) = value.as_array() {
        for item in arr {
            let u = qishui_image_url(item, suffix);
            if !u.is_empty() {
                return u;
            }
        }
        return String::new();
    }
    if !value.is_object() {
        return String::new();
    }
    let cover = ["urls", "url_list", "url"]
        .iter()
        .find_map(|k| value.get(*k))
        .map(qishui_first_url)
        .unwrap_or_default();
    let uri = value.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    let mut out = cover;
    if !out.is_empty() && !uri.is_empty() && !out.contains(uri) {
        out.push_str(uri);
    }
    if out.is_empty() && uri.starts_with("http") {
        out = uri.to_string();
    }
    if !out.starts_with("http") {
        return String::new();
    }
    if out.contains('~') {
        return out;
    }
    // 模板后缀要用图片自带的 template_prefix：歌单封面（tos-cn-i-b829550vbb/…）配
    // ~c5_300x300.jpg 会被 CDN 判 403，而 ~<template_prefix>-image.jpeg 全部 200
    // （歌曲封面的 tos-cn-v-* 两种都行，所以不会回退坏）。
    let tpl = value.get("template_prefix").and_then(|v| v.as_str()).unwrap_or("");
    if !tpl.is_empty() {
        out.push('~');
        out.push_str(tpl);
        out.push_str("-image.jpeg");
    } else if !suffix.is_empty() {
        out.push_str(suffix);
    }
    out
}

fn qishui_pick_image(obj: &serde_json::Value, keys: &[&str], suffix: &str) -> String {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            let u = qishui_image_url(v, suffix);
            if !u.is_empty() {
                return u;
            }
        }
    }
    String::new()
}

/// 从 luna 响应中取出媒体列表（兼容多种字段名与 search result_groups）
fn qishui_extract_media(json: &serde_json::Value) -> Vec<serde_json::Value> {
    let data = json.get("data").unwrap_or(json);
    for key in [
        "media_resources", "media_list", "related_media", "medias", "media", "tracks",
        "track_list", "songs", "items", "list", "result", "song_list", "recommend_media_list",
    ] {
        if let Some(arr) = data.get(key).and_then(|v| v.as_array()) {
            if !arr.is_empty() {
                return arr.clone();
            }
        }
    }
    if let Some(groups) = data.get("result_groups").and_then(|v| v.as_array()) {
        let mut out = Vec::new();
        for g in groups {
            if let Some(arr) = g.get("data").and_then(|v| v.as_array()) {
                out.extend(arr.clone());
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// 把一条 luna 媒体项映射成统一 Song（对照 Mineradio mapQishuiMedia，取常用字段）
fn qishui_map_media(raw: &serde_json::Value) -> Option<Song> {
    let entity = raw.get("entity").or_else(|| raw.get("data")).unwrap_or(raw);
    let media = entity.get("media").or_else(|| raw.get("media")).unwrap_or(entity);
    let wrapper = entity
        .get("track_wrapper")
        .or_else(|| media.get("track_wrapper"))
        .or_else(|| raw.get("track_wrapper"));
    let track = wrapper
        .and_then(|w| w.get("track"))
        .or_else(|| media.get("track_entity"))
        .or_else(|| raw.get("track_entity"))
        .or_else(|| media.get("track"))
        .or_else(|| raw.get("track"))
        .unwrap_or(media);
    let base = track
        .get("base_info")
        .or_else(|| media.get("base_info"))
        .or_else(|| raw.get("base_info"))
        .unwrap_or(track);
    let display = track
        .get("display_info")
        .or_else(|| media.get("display_info"))
        .or_else(|| raw.get("display_info"));
    let related = track
        .get("related_info")
        .or_else(|| media.get("related_info"))
        .or_else(|| raw.get("related_info"));

    let mut id = json_str(base, &["id"]);
    if id.is_empty() {
        id = json_str(track, &["id"]);
    }
    if id.is_empty() {
        id = json_str(media, &["id"]);
    }
    if id.is_empty() {
        id = json_str(raw, &["id", "media_id", "item_id", "song_id"]);
    }
    let mut name = json_str(base, &["name", "title"]);
    if name.is_empty() {
        name = json_str(track, &["name", "title"]);
    }
    if name.is_empty() {
        name = json_str(media, &["name", "title"]);
    }
    if name.is_empty() {
        name = json_str(raw, &["name", "title"]);
    }
    if id.is_empty() || name.is_empty() {
        return None;
    }

    let mut artists: Vec<Artist> = Vec::new();
    let artist_links = related
        .and_then(|r| r.get("artist_links").or_else(|| r.get("artists")))
        .and_then(|v| v.as_array())
        .or_else(|| base.get("artists").and_then(|v| v.as_array()))
        .or_else(|| track.get("artists").and_then(|v| v.as_array()))
        .or_else(|| media.get("artists").and_then(|v| v.as_array()));
    if let Some(links) = artist_links {
        for item in links {
            let an = json_str(item, &["name", "display_name", "simple_display_name", "title", "artist_name"]);
            if an.is_empty() || artists.iter().any(|a| a.name == an) {
                continue;
            }
            let aid = json_str(item, &["id"]);
            let pic = qishui_pick_image(item, &["url_avatar", "avatar", "pic_url"], "");
            artists.push(Artist {
                id: if aid.is_empty() { None } else { Some(aid) },
                mid: None,
                name: an,
                pic_url: if pic.is_empty() { None } else { Some(pic) },
                music_size: None,
            });
        }
    }
    let artist = if artists.is_empty() {
        json_str(base, &["author"])
    } else {
        artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(" / ")
    };

    let album_link = related
        .and_then(|r| r.get("album_link").or_else(|| r.get("album")))
        .or_else(|| base.get("album"))
        .or_else(|| display.and_then(|d| d.get("album")))
        .or_else(|| track.get("album"))
        .or_else(|| media.get("album"));
    let album = album_link.map(|a| json_str(a, &["name", "title"])).unwrap_or_default();

    let mut cover = String::new();
    for src in [display, Some(base), album_link, Some(track), Some(media), Some(raw)] {
        if let Some(s) = src {
            cover = qishui_pick_image(s, &["cover_url", "url_cover", "cover"], "~c5_375x375.jpg");
            if !cover.is_empty() {
                break;
            }
        }
    }

    let mut duration_ms = json_u64(base, &["duration_ms", "duration"])
        .max(json_u64(track, &["duration_ms", "duration"]))
        .max(json_u64(media, &["duration_ms", "duration"]))
        .max(json_u64(raw, &["duration_ms", "duration"]));
    // 统一为毫秒（前端 Song.duration 用毫秒；部分字段可能是秒）
    if duration_ms > 0 && duration_ms < 10_000 {
        duration_ms *= 1000;
    }
    let duration = duration_ms;

    let vip_only = track
        .get("label_info")
        .and_then(|l| l.get("only_vip_playable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Some(Song {
        provider: "qishui".into(),
        id,
        name,
        artist,
        artists,
        album,
        cover,
        duration,
        fee: if vip_only { 1 } else { 0 },
        playable: true,
        ..Default::default()
    })
}

/// 拉取汽水个人信息（昵称/头像/会员），失败返回 None
/// 把 bool / 数字 / 字符串 统一转成布尔（汽水 VIP 字段类型不稳定）
fn json_truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or(0) > 0,
        Some(serde_json::Value::String(text)) => {
            let lower = text.trim().to_ascii_lowercase();
            !lower.is_empty()
                && lower != "0"
                && lower != "false"
                && lower != "none"
                && lower != "normal"
        }
        _ => false,
    }
}

pub async fn qishui_fetch_profile(cookie: &str) -> Option<LoginInfo> {
    if !qishui_cookie_has_login(cookie) {
        return None;
    }
    // 冷启动时网络/DNS 可能还没就绪，重试几次避免拿到空信息
    let mut json = None;
    for attempt in 0..3u64 {
        if let Ok(value) = qishui_luna_get(cookie, "/luna/pc/me", &[]).await {
            json = Some(value);
            break;
        }
        if attempt < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(700 * (attempt + 1))).await;
        }
    }
    let json = json?;
    let me = json
        .get("my_info")
        .or_else(|| json.get("data").and_then(|d| d.get("my_info")))
        .cloned()
        .unwrap_or(json);
    let user_id = json_str(&me, &["id"]);
    let mut nickname = json_str(&me, &["nickname"]);
    if nickname.is_empty() {
        nickname = json_str(&me, &["public_name"]);
    }
    let avatar = qishui_pick_image(&me, &["larger_avatar_url", "medium_avatar_url", "avatar_url", "avatar"], "");

    // VIP 字段在不同账号/版本下形态不一：可能是 bool、数字、字符串，也可能挂在
    // vip_info / vip / vip_status 子对象里，这里统一做容错判断。
    let empty = serde_json::Value::Null;
    let containers = [
        &me,
        me.get("vip_info").unwrap_or(&empty),
        me.get("vip").unwrap_or(&empty),
        me.get("vip_status").unwrap_or(&empty),
        me.get("member_info").unwrap_or(&empty),
    ];
    let mut is_vip = false;
    let mut is_svip = false;
    let mut vip_level = String::new();
    for container in containers {
        for key in ["is_svip", "is_super_vip", "is_luxury_vip", "svip", "is_svip_valid"] {
            if json_truthy(container.get(key)) {
                is_svip = true;
            }
        }
        for key in ["is_vip", "vip", "is_member", "member", "vip_valid"] {
            if json_truthy(container.get(key)) {
                is_vip = true;
            }
        }
        if vip_level.is_empty() {
            vip_level = json_str(container, &["vip_stage", "vip_level", "vip_type_name"]);
        }
    }
    let stage = vip_level.to_ascii_lowercase();
    if stage.contains("svip") || stage.contains("super") {
        is_svip = true;
    }
    if stage.contains("vip") {
        is_vip = true;
    }
    if is_svip {
        is_vip = true;
    }
    if vip_level.is_empty() {
        vip_level = if is_svip {
            "svip".to_string()
        } else if is_vip {
            "vip".to_string()
        } else {
            "none".to_string()
        };
    }
    let logged_in = !user_id.is_empty() || !nickname.is_empty();
    let info = LoginInfo {
        provider: "qishui".into(),
        logged_in,
        user_id,
        nickname,
        avatar,
        vip_type: if is_svip {
            2
        } else if is_vip {
            1
        } else {
            0
        },
        vip_level,
        is_vip,
        is_svip,
    };
    // 落盘缓存，供下次冷启动网络未就绪时兜底
    crate::music_api::remember_profile(&info);
    Some(info)
}

/// 收藏内容：与 /luna/pc/me 同级，只要 cookie，不需要 Node 签名包
async fn qishui_collection_mixed(cookie: &str) -> Result<serde_json::Value, String> {
    qishui_luna_get(
        cookie,
        "/luna/pc/me/collection/mixed",
        &[("cursor", String::new()), ("count", "50".to_string())],
    )
    .await
}

/// 收藏卡片：响应形如 `{mixed_collections: [{item_type: "playlist"|"album", playlist|album: {...}}]}`，
/// 每项只挂一个子对象。专辑卡片在这里被跳过 —— 汽水没有可用的专辑曲目接口。
fn qishui_collection_cards(body: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let items = match body.get("mixed_collections").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return out,
    };
    for item in items {
        match item.get("playlist") {
            Some(p) if p.is_object() => out.push(p.clone()),
            _ => {}
        }
    }
    out
}

/// 歌单节点 → Playlist（收藏卡片与 search/playlist 实体同构，字段名容错）
fn qishui_map_playlist_node(node: &serde_json::Value) -> Option<Playlist> {
    let id = json_str(node, &["id", "playlist_id"]);
    if id.is_empty() {
        return None;
    }
    let mut name = json_str(node, &["title", "public_title", "name"]);
    if name.is_empty() {
        name = "未命名歌单".to_string();
    }
    let cover = qishui_pick_image(node, &["url_cover", "cover_url", "cover"], "~c5_300x300.jpg");
    let mut track_count =
        json_u64(node, &["count_tracks", "count_media", "song_count", "count", "total"]) as u32;
    if track_count == 0 {
        track_count = node
            .get("stats")
            .and_then(|s| s.get("count_visible"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
    }
    let mut creator = {
        let owner = node.get("owner").unwrap_or(&serde_json::Value::Null);
        json_str(owner, &["nickname", "public_name"])
    };
    if creator.is_empty() {
        creator = json_str(node, &["creator_name", "artist_name", "author_name"]);
    }
    Some(Playlist {
        provider: "qishui".into(),
        id,
        name,
        cover,
        track_count,
        creator,
        subscribed: true,
    })
}

/// 汽水音乐用户歌单：自建 + 收藏歌单
#[tauri::command]
pub async fn qishui_user_playlists(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if !qishui_cookie_has_login(&cookie) {
        return Err("汽水音乐未登录".into());
    }
    let me = qishui_luna_get(&cookie, "/luna/pc/me", &[]).await?;
    let user_id = me
        .get("my_info")
        .and_then(|m| m.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut out: Vec<Playlist> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    if !user_id.is_empty() {
        // 自建与收藏互不依赖，一次并发拿完；收藏接口只要 cookie，不需要签名包
        let created_params = [
            ("user_id", user_id.clone()),
            ("cursor", String::new()),
            ("count", "50".to_string()),
        ];
        let (created, mixed) = tokio::join!(
            qishui_luna_get(&cookie, "/luna/pc/user/playlist", &created_params),
            qishui_collection_mixed(&cookie),
        );
        let created = created?;
        if let Some(list) = created.get("playlists").and_then(|v| v.as_array()) {
            for pl in list {
                let id = json_str(pl, &["id"]);
                if id.is_empty() {
                    continue;
                }
                let mut name = json_str(pl, &["title", "public_title", "name"]);
                if name.is_empty() {
                    name = "未命名歌单".to_string();
                }
                let cover = qishui_pick_image(pl, &["url_cover", "cover_url", "cover"], "~c5_300x300.jpg");
                let mut track_count = json_u64(pl, &["count_tracks"]) as u32;
                if track_count == 0 {
                    track_count = pl
                        .get("stats")
                        .and_then(|s| s.get("count_visible"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                }
                let creator = pl
                    .get("owner")
                    .and_then(|o| o.get("nickname"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let playlist = Playlist {
                    provider: "qishui".into(),
                    id,
                    name,
                    cover,
                    track_count,
                    creator,
                    subscribed: false,
                };
                if seen_ids.insert(playlist.id.clone()) {
                    out.push(playlist);
                }
            }
        }

        // 收藏接口挂了只影响收藏部分，退化成「只有自建歌单」，不能连累原有功能
        match mixed {
            Ok(json) => {
                let body = json.get("data").unwrap_or(&json);
                let cards = qishui_collection_cards(body);
                let collected: Vec<Playlist> = cards
                    .iter()
                    .filter_map(|node| qishui_map_playlist_node(node))
                    .collect();
                log::info!("[Qishui] collection: {} 歌单", collected.len());
                for pl in collected {
                    if seen_ids.insert(pl.id.clone()) {
                        out.push(pl);
                    }
                }
            }
            Err(e) => log::warn!("[Qishui] collection/mixed 失败，本次只返回自建歌单: {e}"),
        }
    }
    // 封面兜底：歌单自带封面常为 .png/需签名，易失效，改用第一首曲目封面。
    // 只补空封面且限量 10 次 —— 合并收藏后条目翻倍，无脑逐条请求会招来 429。
    let mut filled = 0usize;
    for i in 0..out.len() {
        if filled >= 10 {
            break;
        }
        if !out[i].cover.is_empty() {
            continue;
        }
        filled += 1;
        let pid = out[i].id.clone();
        if let Ok(detail) = qishui_luna_get(
            &cookie,
            "/luna/pc/playlist/detail",
            &[("playlist_id", pid), ("cursor", String::new()), ("count", "1".to_string())],
        )
        .await
        {
            let raw = qishui_extract_media(&detail);
            if let Some(first) = raw.first() {
                if let Some(song) = qishui_map_media(first) {
                    if !song.cover.is_empty() {
                        out[i].cover = song.cover;
                    }
                }
            }
        }
    }
    log::info!("[Qishui] user playlists: {}", out.len());
    Ok(out)
}

/// 汽水音乐歌单曲目
#[tauri::command]
pub async fn qishui_playlist_tracks(
    app: AppHandle,
    id: String,
) -> Result<(Playlist, Vec<Song>), String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if !qishui_cookie_has_login(&cookie) {
        return Err("汽水音乐未登录".into());
    }
    let json = qishui_luna_get(
        &cookie,
        "/luna/pc/playlist/detail",
        &[
            ("playlist_id", id.clone()),
            ("cursor", String::new()),
            ("count", "100".to_string()),
        ],
    )
    .await?;
    let raw = qishui_extract_media(&json);
    let songs: Vec<Song> = raw.iter().filter_map(qishui_map_media).collect();
    let data = json.get("data").unwrap_or(&json);
    let mut name = json_str(data, &["title", "name", "public_title"]);
    if name.is_empty() {
        name = "汽水歌单".to_string();
    }
    let cover = qishui_pick_image(data, &["url_cover", "cover_url", "cover"], "~c5_300x300.jpg");
    let meta = Playlist {
        provider: "qishui".into(),
        id,
        name,
        cover,
        track_count: songs.len() as u32,
        creator: String::new(),
        subscribed: false,
    };
    log::info!("[Qishui] playlist tracks: {} songs", songs.len());
    Ok((meta, songs))
}

/// 用本机 SodaMusic 的 bdms 签名发起 POST 请求（返回解析后的 JSON body）
async fn qishui_signed_post(
    app: &AppHandle,
    cookie: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (bdms, node) = qishui_find_signer(app).await.ok_or_else(qishui_deps_missing_err)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let device_id = now.to_string();
    let iid = (now + 1).to_string();
    let params: Vec<(&str, String)> = vec![
        ("aid", "386088".into()),
        ("app_name", "luna_pc".into()),
        ("region", "cn".into()),
        ("geo_region", "cn".into()),
        ("os_region", "cn".into()),
        ("device_id", device_id.clone()),
        ("iid", iid),
        ("version_name", "3.7.0".into()),
        ("version_code", "30080000".into()),
        ("channel", "official".into()),
        ("build_mode", "master".into()),
        ("ac", "wifi".into()),
        ("tz_name", "Asia/Shanghai".into()),
        ("device_platform", "windows".into()),
        ("device_type", "Windows".into()),
        ("os_version", "Windows 11".into()),
        ("fp", device_id.clone()),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("https://api.qishui.com{path}?{query}");
    let headers = serde_json::json!({
        "user-agent": "LunaPC/3.7.0(30080000)",
        "accept": "application/json, text/plain, */*",
        "content-type": "application/json; charset=utf-8",
        "x-luna-background-type": "foreground",
        "x-luna-is-background-req": "0",
        "x-luna-is-local-user": "1",
        "cookie": cookie,
    });
    let payload = serde_json::json!({
        "bdmsPath": bdms.to_string_lossy(),
        "deviceId": device_id,
        "url": url,
        "method": "POST",
        "headers": headers,
        "body": body.to_string(),
    });
    let script_path = std::env::temp_dir().join("qishui-signer.cjs");
    let payload_path = std::env::temp_dir().join(format!("qishui-sign-payload-{}.json", std::process::id()));
    std::fs::write(&script_path, QISHUI_SIGNER_JS).map_err(|e| e.to_string())?;
    std::fs::write(&payload_path, payload.to_string()).map_err(|e| e.to_string())?;

    let node_c = node.clone();
    let script_c = script_path.clone();
    let payload_c = payload_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(&node_c);
        cmd.arg(&script_c).arg(&payload_c);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.output()
    })
    .await
    .map_err(|e| format!("签名进程调度失败: {e}"))?
    .map_err(|e| format!("启动 Node 签名进程失败: {e}"))?;
    let _ = std::fs::remove_file(&payload_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("签名进程输出无效: {e} | {}", clip(&stdout, 200)))?;
    let status = parsed.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status != 200 {
        return Err(format!(
            "汽水接口 HTTP {status}: {}",
            clip(parsed.get("body").and_then(|v| v.as_str()).unwrap_or(""), 160)
        ));
    }
    let body_text = parsed.get("body").and_then(|v| v.as_str()).unwrap_or("");
    serde_json::from_str(body_text)
        .map_err(|e| format!("汽水接口返回无效 JSON: {e} | {}", clip(body_text, 200)))
}

/// 汽水「听歌模式」列表（官方 /luna/pc/feed/mode，需签名）
#[tauri::command]
pub async fn qishui_feed_modes(app: AppHandle) -> Result<Vec<serde_json::Value>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if !qishui_cookie_has_login(&cookie) {
        return Err("汽水音乐未登录".into());
    }
    let json = qishui_signed_post(&app, &cookie, "/luna/pc/feed/mode", serde_json::json!({})).await?;
    let mut out: Vec<serde_json::Value> = Vec::new();
    if let Some(blocks) = json.get("feed_mode_block").and_then(|v| v.as_array()) {
        for block in blocks {
            let block_title = json_str(block, &["title"]);
            if let Some(modes) = block.get("feed_mode").and_then(|v| v.as_array()) {
                for m in modes {
                    let entity = m
                        .get("entity")
                        .and_then(|e| e.get("feed_scene_mode"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let name = json_str(m, &["text"]);
                    if name.is_empty() {
                        continue;
                    }
                    let scene_mode_id = entity.get("scene_mode_id").and_then(|v| v.as_i64()).unwrap_or(-1);
                    let sub = json_str(&entity, &["sub_queue_type"]);
                    let cover = qishui_pick_image(m, &["url_info"], "~c5_300x300.jpg");
                    out.push(serde_json::json!({
                        "name": name,
                        "block": block_title,
                        "sceneModeId": scene_mode_id,
                        "subQueueType": sub,
                        "cover": cover,
                    }));
                }
            }
        }
    }
    log::info!("[Qishui] feed modes: {}", out.len());
    Ok(out)
}

/// 汽水「听歌模式」推荐曲目（官方 /luna/pc/feed/song-tab，需签名）
#[tauri::command]
pub async fn qishui_feed(
    app: AppHandle,
    limit: Option<u32>,
    scene_mode_id: Option<i64>,
    sub_queue_type: Option<String>,
    played_ids: Option<Vec<String>>,
) -> Result<Vec<Song>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if !qishui_cookie_has_login(&cookie) {
        return Err("汽水音乐未登录".into());
    }
    let limit = limit.unwrap_or(40).clamp(1, 100) as usize;
    let pref = match scene_mode_id {
        Some(id) if id >= 0 => serde_json::json!({ "scene_mode_id": id }),
        _ => serde_json::json!({ "preference_mode": sub_queue_type.unwrap_or_default() }),
    };
    // 已播放的曲目回传给服务端，才能拿到下一批（否则每次都返回同一批，导致循环播放）
    let played: Vec<String> = played_ids.unwrap_or_default();
    let is_first = played.is_empty();
    let body = serde_json::json!({
        "played_media": played,
        "is_first_request": is_first,
        "is_did_first_request": !is_first,
        "feed_preference": pref,
        "feed_counts": { "mix_session_count": 1 },
    });
    let json = qishui_signed_post(&app, &cookie, "/luna/pc/feed/song-tab", body).await?;
    let mut songs: Vec<Song> = json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(qishui_map_media).collect())
        .unwrap_or_default();
    songs.truncate(limit);
    log::info!("[Qishui] feed(scene) {} songs", songs.len());
    Ok(songs)
}

/// 汽水音乐搜索：先试 luna PC 搜索，空响应则退回公开目录（volcengine）
#[tauri::command]
pub async fn qishui_search(
    app: AppHandle,
    keywords: String,
    limit: Option<u32>,
) -> Result<Vec<Song>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    let limit = limit.unwrap_or(30).clamp(1, 50);

    // 1) 登录态 luna PC 搜索
    if qishui_cookie_has_login(&cookie) {
        let count = limit.to_string();
        if let Ok(json) = qishui_luna_get(
            &cookie,
            "/luna/pc/search/track",
            &[
                ("q", keywords.clone()),
                ("cursor", "0".to_string()),
                ("count", count),
                ("search_method", "input".to_string()),
            ],
        )
        .await
        {
            let raw = qishui_extract_media(&json);
            let songs: Vec<Song> = raw.iter().filter_map(qishui_map_media).collect();
            if !songs.is_empty() {
                log::info!("[Qishui] search(luna) '{}': {} songs", keywords, songs.len());
                return Ok(songs);
            }
        }
    }

    // 2) 公开目录搜索（无需登录，Mineradio 的主搜索路径）
    let songs = qishui_public_search(&keywords, limit).await?;
    log::info!("[Qishui] search(public) '{}': {} songs", keywords, songs.len());
    Ok(songs)
}

/// search/* 的条目都在 `result_groups[].data[].entity.<kind>` 下，按类型取出实体
fn qishui_search_entities(json: &serde_json::Value, kind: &str) -> Vec<serde_json::Value> {
    let groups = json
        .get("result_groups")
        .or_else(|| json.get("data").and_then(|d| d.get("result_groups")))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for group in groups {
        for item in group
            .get("data")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
        {
            if let Some(entity) = item.get("entity").and_then(|en| en.get(kind)) {
                out.push(entity.clone());
            }
        }
    }
    out
}

/// 汽水搜索歌单：/luna/pc/search/playlist（GET，只要 cookie，不需签名）
#[tauri::command]
pub async fn qishui_search_playlists(
    app: AppHandle,
    keywords: String,
    limit: Option<u32>,
) -> Result<Vec<Playlist>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    let json = qishui_luna_get(
        &cookie,
        "/luna/pc/search/playlist",
        &[
            ("q", keywords.clone()),
            ("cursor", "0".to_string()),
            ("count", limit.unwrap_or(30).clamp(1, 50).to_string()),
            ("search_method", "input".to_string()),
        ],
    )
    .await?;
    let out: Vec<Playlist> = qishui_search_entities(&json, "playlist")
        .iter()
        .filter_map(qishui_map_playlist_node)
        .collect();
    log::info!("[Qishui] search-playlists '{}': {} 个", keywords, out.len());
    Ok(out)
}

/// 汽水搜索歌手：/luna/pc/search/artist
#[tauri::command]
pub async fn qishui_search_artists(
    app: AppHandle,
    keywords: String,
    limit: Option<u32>,
) -> Result<Vec<Artist>, String> {
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    let json = qishui_luna_get(
        &cookie,
        "/luna/pc/search/artist",
        &[
            ("q", keywords.clone()),
            ("cursor", "0".to_string()),
            ("count", limit.unwrap_or(30).clamp(1, 50).to_string()),
            ("search_method", "input".to_string()),
        ],
    )
    .await?;
    let mut out: Vec<Artist> = Vec::new();
    for a in qishui_search_entities(&json, "artist") {
        let id = json_str(&a, &["id"]);
        let name = json_str(&a, &["name", "display_name"]);
        if name.is_empty() {
            continue;
        }
        let pic = qishui_pick_image(&a, &["url_avatar", "avatar"], "");
        let tracks = json_u64(&a, &["count_tracks"]) as i64;
        out.push(Artist {
            id: if id.is_empty() { None } else { Some(id) },
            mid: None,
            name,
            pic_url: if pic.is_empty() { None } else { Some(pic) },
            music_size: if tracks > 0 { Some(tracks) } else { None },
        });
    }
    log::info!("[Qishui] search-artists '{}': {} 个", keywords, out.len());
    Ok(out)
}

/// 公开目录搜索：api-vehicle.volcengine.com/v2/search/type
async fn qishui_public_search(keywords: &str, limit: u32) -> Result<Vec<Song>, String> {
    let request_limit = (limit.saturating_mul(3)).clamp(36, 100).to_string();
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api-vehicle.volcengine.com/v2/search/type")
        .query(&[
            ("keyword", keywords.to_string()),
            ("search_type", "music".to_string()),
            ("limit", request_limit),
            ("real_offset", "0".to_string()),
            ("search_source", "qishui".to_string()),
        ])
        .header("Accept", "application/json,text/plain,*/*")
        .header("User-Agent", "Mineradio/2.1.0 (Qishui public catalog bridge)")
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("汽水公开搜索请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("汽水公开搜索返回无效 JSON: {e} | {}", clip(&text, 200)))?;
    let list = json
        .get("data")
        .and_then(|d| d.get("list"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut songs: Vec<Song> = list.iter().filter_map(qishui_map_public_item).collect();
    songs.truncate(limit as usize);
    Ok(songs)
}

/// 公开目录条目 → Song（对照 Mineradio mapQishuiPublicItem）
fn qishui_map_public_item(raw: &serde_json::Value) -> Option<Song> {
    let mut id = json_str(raw, &["item_id", "id", "song_id", "music_id"]);
    let mut name = json_str(raw, &["title", "name", "song_name"]);
    if id.is_empty() {
        id = json_str(raw, &["item_id"]);
    }
    if name.is_empty() {
        name = json_str(raw, &["title"]);
    }
    if id.is_empty() || name.is_empty() {
        return None;
    }
    let author = raw
        .get("author_info")
        .or_else(|| raw.get("author"))
        .or_else(|| raw.get("artist"));
    let artist = author.map(|a| json_str(a, &["name"])).unwrap_or_default();
    let album_obj = raw.get("album_info").or_else(|| raw.get("album"));
    let album = album_obj.map(|a| json_str(a, &["name"])).unwrap_or_default();
    let cover = json_str(raw, &["cover_url", "cover", "artwork"]);
    let duration_raw = json_u64(raw, &["duration", "duration_ms"]);
    let duration = if duration_raw > 10_000 { duration_raw / 1000 } else { duration_raw };
    let vip = raw
        .get("qishui_label_info")
        .and_then(|l| l.get("only_vip_playable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut artists = Vec::new();
    if !artist.is_empty() {
        artists.push(Artist {
            id: None,
            mid: None,
            name: artist.clone(),
            pic_url: None,
            music_size: None,
        });
    }
    Some(Song {
        provider: "qishui".into(),
        id,
        name,
        artist,
        artists,
        album,
        cover,
        duration,
        fee: if vip { 1 } else { 0 },
        // 公开目录不提供播放地址（网页登录态 track_v2 被服务端拦截）
        playable: false,
        ..Default::default()
    })
}

const QISHUI_SIGNER_JS: &str = include_str!("../../resources/qishui-auth/qishui-signer.js");

// ============================================================
//  签名依赖（node.exe + bdms.node + metasecml.dll）
//  三个文件都不进开源仓库：优先本地打包进安装包（resources/qishui-auth/），
//  缺失时由前端提示「需下载 Node 包」并手动从下面的地址下载安装到缓存目录。
// ============================================================

/// 依赖包下载地址（zip 根目录含 node.exe / bdms.node / metasecml.dll），由维护者上传至 GitCode Release
const QISHUI_DEPS_URL: &str =
    "https://gitcode.com/MuLiuSaMa/NexBox-Tools/releases/download/v1.0.0/node-v22.15.1-win-x64.zip";
/// 依赖包内允许提取的文件白名单（只按文件名提取，避免 zip 路径穿越）
const QISHUI_DEPS_FILES: [&str; 3] = ["node.exe", "bdms.node", "metasecml.dll"];
/// 依赖缺失的统一错误前缀，前端据此识别并弹出下载提示框
pub const QISHUI_DEPS_MISSING: &str = "QISHUI_DEPS_MISSING";
/// 同一时刻只允许一个下载任务
static QISHUI_DEPS_DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize, Clone)]
pub struct QishuiDepsStatus {
    /// node + bdms 均可用（签名 / 解密可用）
    pub ready: bool,
    pub node_ready: bool,
    pub bdms_ready: bool,
    /// "resources" | "cache" | "system" | ""
    pub node_source: String,
    /// "resources" | "cache" | "sodamusic" | ""
    pub bdms_source: String,
    pub downloading: bool,
}

/// 依赖缺失时的统一错误文案
fn qishui_deps_missing_err() -> String {
    format!("{QISHUI_DEPS_MISSING}: 需下载 Node 包（node.exe + bdms.node + metasecml.dll）")
}

fn find_file_recursive(root: &std::path::Path, name: &str, depth: u32) -> Option<std::path::PathBuf> {
    if depth == 0 || !root.is_dir() {
        return None;
    }
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    for dir in dirs {
        if let Some(found) = find_file_recursive(&dir, name, depth - 1) {
            return Some(found);
        }
    }
    None
}

/// 定位本机 SodaMusic 客户端（SodaMusic.exe + bdms.node）——方案二所需
/// 去掉 Windows 扩展长度路径前缀 `\\?\`（Node 的 require 处理不了）
fn qishui_clean_path(p: std::path::PathBuf) -> std::path::PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(rest)
    } else {
        p
    }
}

/// 工具箱内置资源路径候选（resources/qishui-auth/<name>）
fn qishui_resource_paths(app: &AppHandle, name: &str) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = app.path().resource_dir() {
        let rd = qishui_clean_path(rd);
        let bases = [
            rd.clone(),
            rd.join("_up_"),
            rd.join("_up_").join("_up_").join("src-tauri"),
        ];
        for base in bases {
            out.push(base.join("resources").join("qishui-auth").join(name));
            out.push(base.join("qishui-auth").join(name));
        }
    }
    out
}

/// 确保汽水签名所需的 Node 运行时（resources 内置 → 本地缓存 → 系统 Node），不触发任何下载。
/// 返回 (node.exe 路径, 来源标记)
fn locate_qishui_node(app: &AppHandle) -> Option<(std::path::PathBuf, &'static str)> {
    if let Some(p) = qishui_resource_paths(app, "node.exe").into_iter().find(|p| p.is_file()) {
        return Some((qishui_clean_path(p), "resources"));
    }
    let cached = qishui_deps_cache_dir().join("node.exe");
    if cached.is_file() {
        return Some((cached, "cache"));
    }
    find_node().map(|p| (p, "system"))
}

/// 定位汽水签名模块 bdms.node（resources 内置 → 本地缓存 → 本机汽水客户端），不触发任何下载。
/// 返回 (bdms.node 路径, 来源标记)
fn locate_qishui_bdms(app: &AppHandle) -> Option<(std::path::PathBuf, &'static str)> {
    if let Some(p) = qishui_resource_paths(app, "bdms.node").into_iter().find(|p| p.is_file()) {
        return Some((qishui_clean_path(p), "resources"));
    }
    let cached = qishui_deps_cache_dir().join("bdms.node");
    if cached.is_file() {
        return Some((cached, "cache"));
    }
    find_sodamusic().map(|(_, b)| (qishui_clean_path(b), "sodamusic"))
}

/// 依赖缓存目录：%LOCALAPPDATA%\NexBox\qishui-node
fn qishui_deps_cache_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("NexBox")
        .join("qishui-node")
}

fn qishui_deps_probe(app: &AppHandle) -> QishuiDepsStatus {
    let node = locate_qishui_node(app);
    let bdms = locate_qishui_bdms(app);
    QishuiDepsStatus {
        ready: node.is_some() && bdms.is_some(),
        node_ready: node.is_some(),
        bdms_ready: bdms.is_some(),
        node_source: node.map(|(_, s)| s.to_string()).unwrap_or_default(),
        bdms_source: bdms.map(|(_, s)| s.to_string()).unwrap_or_default(),
        downloading: QISHUI_DEPS_DOWNLOADING.load(Ordering::Relaxed),
    }
}

/// 汽水签名依赖是否就绪（只读探测，不触发下载）
#[tauri::command]
pub async fn qishui_deps_status(app: AppHandle) -> Result<QishuiDepsStatus, String> {
    Ok(qishui_deps_probe(&app))
}

/// 手动下载并安装汽水签名依赖包（node.exe + bdms.node + metasecml.dll）
#[tauri::command]
pub async fn qishui_deps_download(app: AppHandle) -> Result<QishuiDepsStatus, String> {
    if QISHUI_DEPS_DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("Node 包正在下载中，请稍候".into());
    }
    let result = install_qishui_deps(&app).await;
    QISHUI_DEPS_DOWNLOADING.store(false, Ordering::SeqCst);
    result?;
    Ok(qishui_deps_probe(&app))
}

/// 下载依赖包 zip 并按文件名白名单解出文件（bdms.node 与 metasecml.dll 必须同目录）
async fn install_qishui_deps(app: &AppHandle) -> Result<(), String> {
    let cache_dir = qishui_deps_cache_dir();

    // 缓存里已有可用的 node.exe 就别再拉 35MB：重复下载才是「慢」的主要来源
    // （真正缺的往往只剩签名模块，而它随安装包 resources 走，不在这个包里）
    let cached_node = cache_dir.join("node.exe");
    if matches!(locate_qishui_node(app), Some((_, "cache")))
        && std::fs::metadata(&cached_node).map(|m| m.len() > 1024 * 1024).unwrap_or(false)
    {
        log::info!("[Qishui] 缓存已有 node.exe（{} 字节），跳过重复下载", std::fs::metadata(&cached_node).map(|m| m.len()).unwrap_or(0));
        return Ok(());
    }

    log::info!("[Qishui] 开始下载签名依赖包: {}", QISHUI_DEPS_URL);
    let mut resp = reqwest::Client::new()
        .get(QISHUI_DEPS_URL)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .timeout(Duration::from_secs(900))
        .send()
        .await
        .map_err(|e| format!("下载 Node 包失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载 Node 包失败: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("创建依赖目录失败: {e}"))?;

    // 流式落临时 zip，不在内存里堆整个包（原来 Vec::with_capacity(35MB) + extend_from_slice
    // 既占内存又要等全部到齐才开始解压）
    let tmp_zip = std::env::temp_dir().join(format!("qishui-node-deps-{}.zip", std::process::id()));
    let mut received: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    {
        let mut tmp_file = std::fs::File::create(&tmp_zip).map_err(|e| format!("写入临时包失败: {e}"))?;
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("下载中断: {e}"))? {
            std::io::Write::write_all(&mut tmp_file, &chunk).map_err(|e| format!("写入临时包失败: {e}"))?;
            received += chunk.len() as u64;
            // 每个 chunk 都 emit 会刷出几千次事件（每次还要序列化 JSON 投给 webview），
            // 直接把下载循环拖成瓶颈；按 200ms 节流，进度条够用
            if last_emit.elapsed() >= Duration::from_millis(200) {
                let _ = app.emit(
                    "qishui-deps-progress",
                    serde_json::json!({ "received": received, "total": total }),
                );
                last_emit = std::time::Instant::now();
            }
        }
    }
    let _ = app.emit(
        "qishui-deps-progress",
        serde_json::json!({ "received": received, "total": total }),
    );
    log::info!("[Qishui] 依赖包下载完成: {received} 字节");

    let mut extracted: Vec<std::path::PathBuf> = Vec::new();
    let mut failed: Option<String> = None;
    match std::fs::File::open(&tmp_zip)
        .map_err(|e| format!("打开临时包失败: {e}"))
        .and_then(|f| {
            zip::ZipArchive::new(std::io::BufReader::new(f))
                .map_err(|e| format!("Node 包不是有效的 zip: {e}"))
        }) {
        Ok(mut archive) => {
            for i in 0..archive.len() {
                let mut file = match archive.by_index(i) {
                    Ok(f) => f,
                    Err(e) => {
                        failed = Some(format!("读取 zip 条目失败: {e}"));
                        break;
                    }
                };
                // 只取文件名本身，忽略 zip 内目录结构，避免路径穿越
                let name = std::path::Path::new(file.name())
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                if !QISHUI_DEPS_FILES.contains(&name.as_str()) {
                    continue;
                }
                let target = cache_dir.join(&name);
                match std::fs::File::create(&target) {
                    Ok(mut out) => {
                        // 1MB 缓冲：node.exe 解出来 84MB，io::copy 默认 8KB 块要上万次写调用
                        let mut buf = std::io::BufWriter::with_capacity(1024 * 1024, &mut out);
                        if let Err(e) = std::io::copy(&mut file, &mut buf) {
                            failed = Some(format!("写入 {name} 失败: {e}"));
                            break;
                        }
                        if let Err(e) = std::io::Write::flush(&mut buf) {
                            failed = Some(format!("写入 {name} 失败: {e}"));
                            break;
                        }
                        extracted.push(target);
                    }
                    Err(e) => {
                        failed = Some(format!("创建 {name} 失败: {e}"));
                        break;
                    }
                }
            }
        }
        Err(e) => failed = Some(e),
    }
    let _ = std::fs::remove_file(&tmp_zip);
    // 不要求包内三件齐全：bdms.node / metasecml.dll 是字节专有二进制（.gitignore 里明确不入库），
    // 很多机器靠安装包 resources 或本机汽水客户端就已经命中，这个 zip 只负责补体积最大的 node.exe。
    // 到底能不能用交给 qishui_deps_probe 判，别在这里把「只缺 node」的场景判死。
    if failed.is_none() && extracted.is_empty() {
        failed = Some("Node 包里没有 node.exe / bdms.node / metasecml.dll".to_string());
    }
    if let Some(err) = failed {
        for p in extracted {
            let _ = std::fs::remove_file(p);
        }
        return Err(err);
    }
    log::info!("[Qishui] 签名依赖包已安装到 {}", cache_dir.display());
    Ok(())
}

/// 定位汽水签名模块与 Node 运行时（均只做本地探测，缺失时由前端引导手动下载）
async fn qishui_find_signer(app: &AppHandle) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let (bdms, _) = locate_qishui_bdms(app)?;
    let (node, _) = locate_qishui_node(app)?;
    Some((bdms, node))
}

fn find_sodamusic() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let mut roots: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("D:\\Soda Music"),
        std::path::PathBuf::from("C:\\Program Files\\Soda Music"),
        std::path::PathBuf::from("C:\\Program Files (x86)\\Soda Music"),
    ];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(std::path::Path::new(&local).join("Programs").join("SodaMusic"));
        roots.push(std::path::Path::new(&local).join("SodaMusic"));
    }
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let exe = find_file_recursive(&root, "SodaMusic.exe", 3);
        let bdms = find_file_recursive(&root, "bdms.node", 4);
        if let (Some(e), Some(b)) = (exe, bdms) {
            return Some((e, b));
        }
    }
    None
}

/// 定位可用的 Node 运行时（签名脚本需要 Node 才能加载 bdms.node；
/// SodaMusic 的 Electron-as-node 加载该原生模块会崩溃，必须用真正的 node.exe）
fn find_node() -> Option<std::path::PathBuf> {
    let mut cands: Vec<std::path::PathBuf> = vec![
        std::path::PathBuf::from("D:\\nodejs\\node.exe"),
        std::path::PathBuf::from("C:\\Program Files\\nodejs\\node.exe"),
        std::path::PathBuf::from("C:\\Program Files (x86)\\nodejs\\node.exe"),
    ];
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        cands.push(std::path::Path::new(&local).join("Programs").join("nodejs").join("node.exe"));
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        cands.push(std::path::Path::new(&profile).join("scoop").join("apps").join("nodejs").join("current").join("node.exe"));
    }
    for c in &cands {
        if c.is_file() {
            return Some(c.clone());
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("node.exe");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

const QISHUI_DECRYPT_JS: &str = include_str!("../../resources/qishui-auth/qishui-decrypt.js");

/// 构建签名 track_v2 的请求（URL / headers / body）
fn qishui_track_v2_parts(cookie: &str, track_id: &str) -> (String, serde_json::Value, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let device_id = now.to_string();
    let iid = (now + 1).to_string();
    let params: Vec<(&str, String)> = vec![
        ("aid", "386088".into()),
        ("app_name", "luna_pc".into()),
        ("region", "cn".into()),
        ("geo_region", "cn".into()),
        ("os_region", "cn".into()),
        ("device_id", device_id.clone()),
        ("iid", iid),
        ("version_name", "3.7.0".into()),
        ("version_code", "30080000".into()),
        ("channel", "official".into()),
        ("build_mode", "master".into()),
        ("ac", "wifi".into()),
        ("tz_name", "Asia/Shanghai".into()),
        ("device_platform", "windows".into()),
        ("device_type", "Windows".into()),
        ("os_version", "Windows 11".into()),
        ("fp", device_id),
    ];
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("https://api.qishui.com/luna/pc/track_v2?{query}");
    let body = serde_json::json!({
        "track_id": track_id,
        "media_type": "track",
        "queue_type": "favorite_track_playlist",
        "scene_name": "library",
    })
    .to_string();
    let headers = serde_json::json!({
        "user-agent": "LunaPC/3.7.0(30080000)",
        "accept": "application/json, text/plain, */*",
        "content-type": "application/json; charset=utf-8",
        "x-luna-background-type": "foreground",
        "x-luna-is-background-req": "0",
        "x-luna-is-local-user": "1",
        "cookie": cookie,
    });
    (url, headers, body)
}

/// 方案二（解密）：签名 track_v2 → 下载加密流 → Node(AES-128-CTR) 解密 → 临时明文文件。
/// 返回 (文件路径, 音质档, 时长ms)
async fn qishui_decrypt_track(
    app: &AppHandle,
    cookie: &str,
    track_id: &str,
    quality: &str,
) -> Result<(String, String, u64), String> {
    let (bdms, node) = qishui_find_signer(app).await.ok_or_else(qishui_deps_missing_err)?;
    let out_dir = std::env::temp_dir().join("NexBox-qishui");
    let _ = std::fs::create_dir_all(&out_dir);
    let (url, headers, body) = qishui_track_v2_parts(cookie, track_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let payload = serde_json::json!({
        "bdmsPath": bdms.to_string_lossy(),
        "deviceId": now.to_string(),
        "url": url,
        "method": "POST",
        "headers": headers,
        "body": body,
        "quality": quality,
        "outDir": out_dir.to_string_lossy(),
    });
    let script_path = std::env::temp_dir().join("qishui-decrypt.cjs");
    let payload_path = std::env::temp_dir().join(format!("qishui-decrypt-payload-{}.json", std::process::id()));
    std::fs::write(&script_path, QISHUI_DECRYPT_JS).map_err(|e| e.to_string())?;
    std::fs::write(&payload_path, payload.to_string()).map_err(|e| e.to_string())?;

    let node_c = node.clone();
    let script_c = script_path.clone();
    let payload_c = payload_path.clone();
    let output = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(&node_c);
        cmd.arg(&script_c).arg(&payload_c);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd.output()
    })
    .await
    .map_err(|e| format!("解密进程调度失败: {e}"))?
    .map_err(|e| format!("启动 Node 解密进程失败: {e}"))?;
    let _ = std::fs::remove_file(&payload_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("解密进程输出无效: {e} | {}", clip(&stdout, 200)))?;
    if parsed.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(parsed
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("解密失败")
            .to_string());
    }
    let path = parsed.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if path.is_empty() {
        return Err("解密未返回文件路径".into());
    }
    let level = parsed.get("level").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let duration = parsed.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
    Ok((path, level, duration))
}

/// 汽水播放地址。
/// 官方 track_v2 已加 X-Helios/X-Medusa 签名校验（网页登录态返回空），
/// 这里走免签名的 H5 SEO 回退接口（Issue #451 方案一）：
///   GET https://beta-luna.douyin.com/luna/h5/seo_track?track_id=<id>&device_platform=web
/// 免费歌返回全曲；VIP 歌服务端硬切为 30 秒试听。
#[tauri::command]
pub async fn qishui_song_url(
    app: AppHandle,
    id: String,
    quality: Option<String>,
) -> Result<serde_json::Value, String> {
    let _ = app;
    if id.is_empty() {
        return Ok(serde_json::json!({
            "url": null, "playable": false, "trial": false, "level": "", "quality": "", "br": 0,
            "reason": "missing_id", "message": "缺少汽水歌曲 id"
        }));
    }
    let want_quality = quality.clone().unwrap_or_default();

    // 方案二：签名 track_v2 → Node 解密 Gear 加密流 → 临时明文文件（VIP 全曲）
    let cookie = crate::music_api::load_provider_cookie(&app, "qishui").await;
    if qishui_cookie_has_login(&cookie) {
        match qishui_decrypt_track(&app, &cookie, &id, &want_quality).await {
            Ok((path, level, duration)) => {
                log::info!("[Qishui] song_url(decrypted) id={} level={}", id, level);
                return Ok(serde_json::json!({
                    "url": format!("file://{}", path.replace('\\', "/")),
                    "playable": true,
                    "trial": false,
                    "level": level,
                    "quality": level,
                    "br": 0,
                    "duration": duration,
                    "message": "",
                }));
            }
            Err(e) => log::info!("[Qishui] decrypt 不可用，回退 SEO: {}", e),
        }
    }

    // 方案一：免签名 H5 SEO（明文音频；免费歌全曲、VIP 歌 30 秒试听）
    let client = reqwest::Client::new();
    let resp = client
        .get("https://beta-luna.douyin.com/luna/h5/seo_track")
        .query(&[("track_id", id.clone()), ("device_platform", "web".to_string())])
        .header("Accept", "application/json,text/plain,*/*")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.7103.59 Safari/537.36",
        )
        .header("Referer", "https://www.douyin.com/")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("汽水 SEO 接口请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("汽水 SEO 接口返回无效 JSON: {e} | {}", clip(&text, 200)))?;

    // track_player.video_model 可能是字符串（需二次解析）
    let video_model = json
        .get("track_player")
        .and_then(|t| t.get("video_model"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let video_model = match video_model {
        serde_json::Value::String(s) => serde_json::from_str(&s).unwrap_or(serde_json::Value::Null),
        other => other,
    };
    let list = video_model
        .get("video_list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let pick = |q: &str| -> Option<&serde_json::Value> {
        list.iter().find(|v| {
            v.get("video_meta")
                .and_then(|m| m.get("quality"))
                .and_then(|x| x.as_str())
                == Some(q)
        })
    };
    let want = quality.unwrap_or_default().to_lowercase();
    let chosen = if want.contains("lossless") || want.contains("hires") || want.contains("highest") || want.contains("jymaster") {
        pick("highest")
    } else if want.contains("higher") || want.contains("exhigh") {
        pick("higher")
    } else {
        None
    }
    .or_else(|| pick("highest"))
    .or_else(|| pick("higher"))
    .or_else(|| pick("medium"))
    .or_else(|| list.first());

    let chosen = match chosen {
        Some(v) => v,
        None => {
            return Ok(serde_json::json!({
                "url": null, "playable": false, "trial": false, "level": "", "quality": "", "br": 0,
                "reason": "source_unavailable", "message": "汽水 SEO 接口未返回音频地址"
            }))
        }
    };

    let url = json_str(chosen, &["main_url"]);
    let backup = json_str(chosen, &["backup_url"]);
    let meta = chosen.get("video_meta").cloned().unwrap_or(serde_json::Value::Null);
    let level = json_str(&meta, &["quality"]);
    let br = json_u64(&meta, &["bitrate"]);

    // 试听判定：VIP 歌服务端硬切为 audition 时长
    let track = json
        .get("seo_track")
        .and_then(|s| s.get("track"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let vip_only = track
        .get("label_info")
        .and_then(|l| l.get("only_vip_playable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let full_ms = json_u64(&track, &["duration_ms", "duration"]);
    let audition_ms = track
        .get("audition_info")
        .and_then(|a| a.get("duration_ms"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let trial = vip_only && audition_ms > 0;

    if url.is_empty() {
        return Ok(serde_json::json!({
            "url": null, "playable": false, "trial": trial, "level": level, "quality": level, "br": br,
            "reason": "source_unavailable", "message": "汽水 SEO 接口未返回音频地址"
        }));
    }

    log::info!(
        "[Qishui] song_url id={} level={} br={} trial={} url.len={}",
        id,
        level,
        br,
        trial,
        url.len()
    );
    Ok(serde_json::json!({
        "url": url,
        "backup_url": if backup.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(backup) },
        "playable": true,
        "trial": trial,
        "level": level,
        "quality": level,
        "br": br,
        "duration": full_ms,
        "message": if trial { "VIP 歌曲仅 30 秒试听" } else { "" },
    }))
}

fn qishui_lrc_ts(ms: i64) -> String {
    let ms = ms.max(0);
    format!("[{:02}:{:02}.{:02}]", ms / 60000, (ms % 60000) / 1000, (ms % 1000) / 10)
}

/// KRC（字节逐字歌词）→ LRC + YRC。对照 Mineradio qishuiConvertLyric
fn qishui_krc_to_lrc(input: &str) -> (String, String) {
    let line_re = match regex::Regex::new(r"^\[(\d+),(\d+)\](.*)$") {
        Ok(re) => re,
        Err(_) => return (String::new(), String::new()),
    };
    let word_re = match regex::Regex::new(r"([<(])(\d+),(\d+),(\d+)[>)]([^<(]*)") {
        Ok(re) => re,
        Err(_) => return (String::new(), String::new()),
    };
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lrc = String::new();
    let mut yrc = String::new();
    for raw in normalized.split('\n') {
        let line = raw.trim();
        let caps = match line_re.captures(line) {
            Some(c) => c,
            None => continue,
        };
        let line_start: i64 = caps[1].parse().unwrap_or(0);
        let line_dur: i64 = caps[2].parse().unwrap_or(0);
        let body = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let mut text = String::new();
        let mut yrc_body = String::new();
        for w in word_re.captures_iter(body) {
            let bracket = w.get(1).map(|m| m.as_str()).unwrap_or("<");
            let raw_start: i64 = w.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let word_dur: i64 = w.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let word_text = w.get(5).map(|m| m.as_str()).unwrap_or("");
            if word_text.is_empty() {
                continue;
            }
            let abs = if bracket == "<" {
                line_start + raw_start
            } else if raw_start >= (line_start - 500).max(0) {
                raw_start
            } else {
                line_start + raw_start
            };
            text.push_str(word_text);
            yrc_body.push_str(&format!("({},{},0){}", abs, word_dur, word_text));
        }
        if text.is_empty() {
            text = word_re.replace_all(body, "").to_string();
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            continue;
        }
        lrc.push_str(&format!("{}{}\n", qishui_lrc_ts(line_start), text));
        yrc.push_str(&format!("[{},{}]{}\n", line_start, line_dur, yrc_body));
    }
    (lrc, yrc)
}

/// 汽水歌词：走免签名 SEO 接口，把 KRC 转成 LRC/YRC
#[tauri::command]
pub async fn qishui_lyric(app: AppHandle, id: String) -> Result<Lyrics, String> {
    let _ = app;
    if id.is_empty() {
        return Ok(Lyrics::default());
    }
    let client = reqwest::Client::new();
    let resp = client
        .get("https://beta-luna.douyin.com/luna/h5/seo_track")
        .query(&[("track_id", id.clone()), ("device_platform", "web".to_string())])
        .header("Accept", "application/json,text/plain,*/*")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.7103.59 Safari/537.36",
        )
        .header("Referer", "https://www.douyin.com/")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("汽水歌词接口请求失败: {e}"))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("汽水歌词接口返回无效 JSON: {e} | {}", clip(&text, 200)))?;

    let lyric_node = json.get("lyric").cloned().unwrap_or(serde_json::Value::Null);
    let content = json_str(&lyric_node, &["content"]);
    let translation_raw = json_str(&lyric_node, &["translation", "tlyric", "translated_lyric"]);
    let (lyric, yrc) = qishui_krc_to_lrc(&content);
    let translation = if translation_raw.is_empty() {
        None
    } else {
        let (t, _) = qishui_krc_to_lrc(&translation_raw);
        if t.is_empty() { None } else { Some(t) }
    };
    log::info!("[Qishui] lyric id={} lyric.len={}", id, lyric.len());
    Ok(Lyrics {
        lyric,
        translation,
        roma: None,
        yrc: if yrc.is_empty() { None } else { Some(yrc) },
    })
}
