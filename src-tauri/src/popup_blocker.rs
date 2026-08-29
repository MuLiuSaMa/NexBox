//! NexBoxPopNull 弹窗拦截（黑白名单规则引擎）
//!
//! 参考 PopKiller 的黑白名单拦截机制（仅黑白名单档，无启发式/ML）：
//! 后台线程挂 WinEvent 钩子（EVENT_OBJECT_SHOW + EVENT_SYSTEM_FOREGROUND），
//! 对每个新出现/前置的顶层窗口依次做：
//!   1. 过滤：仅 OBJID_WINDOW / CHILDID_SELF、可见、顶层窗口（GA_ROOT）
//!   2. 保护名单：自身进程 + 系统进程（explorer/dwm/svchost 等）→ 直接放行
//!   3. 规则匹配：白名单命中 → 放行（最高优先）；黑名单命中 → 拦截
//! 拦截动作：PostMessage(WM_CLOSE) + ShowWindow(SW_HIDE)，
//! 并在 400ms 后对非系统路径（非 C:\Windows\ / C:\Program Files\ 前缀）的进程 TerminateProcess 强杀。
//!
//! 规则持久化由前端 store（settings.json）统一负责：
//!   - nexbox_popnull_enabled：启用开关
//!   - nexbox_popnull_rules：规则数组（list/field/mode/pattern）

use serde::{Deserialize, Serialize};

/// 规则匹配字段（镜像 PopKiller 的 RuleField）
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RuleField {
    Exe,
    Path,
    Title,
    Class,
}

/// 规则匹配模式（镜像 PopKiller 的 MatchMode）
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    Contains,
    Exact,
    Wildcard,
}

/// 一条拦截规则
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Rule {
    /// "B"=黑名单 / "W"=白名单
    pub list: String,
    pub field: RuleField,
    pub mode: MatchMode,
    /// 匹配内容（统一小写存储）
    pub pattern: String,
}

/// 引擎状态（前端首次挂载时获取）
#[derive(Serialize)]
pub struct PopNullState {
    pub enabled: bool,
    pub rules: Vec<Rule>,
}

/// 「选取窗口」返回的窗口信息
#[derive(Serialize)]
pub struct WindowInfo {
    pub hwnd: i64,
    pub title: String,
    pub exe: String,
    pub path: String,
    pub class: String,
}

/// 内置预设黑名单（移植自 PopKiller community_rules.json 常用条目 + 社区常见推广弹窗，离线生效）
fn default_rules() -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    // 进程名包含匹配（黑名单）
    for exe in [
        // PopKiller 社区规则经典条目
        "flashcenter.exe",
        "minipage.exe",
        "flashhelperservice.exe",
        "kwallpaper.exe",
        // 常见推广/广告弹窗进程
        "wpscenter.exe",     // WPS 办公弹窗中心
        "sgtool.exe",        // 搜狗输入法推广/皮肤升级
        "qqpctray.exe",      // QQ 电脑管家广告弹窗
        "radiocloud.exe",    // 2345 电台/热点弹窗
        "lxrs.exe",          // 雷蛇/影音类推广弹窗
        "rdwebservice.exe",  // 迅雷系推广服务
        "kbasesrv.exe",      // 快压/输入法类推广常驻
    ] {
        rules.push(Rule {
            list: "B".into(),
            field: RuleField::Exe,
            mode: MatchMode::Contains,
            pattern: exe.to_string(),
        });
    }
    // 窗口标题包含匹配（黑名单）
    for title in [
        "迷你首页",
        // 用更精确的「热点资讯/今日热点」替代宽泛的「热点」，减少误杀
        "热点资讯",
        "今日热点",
        "每日推荐",
        "推荐资讯",
        "升级提醒",
        "立即升级",
        "福利中心",
        "领红包",
        "送会员",
        "抢票",
        "加速器",
        "下载器",
        "高速下载",
    ] {
        rules.push(Rule {
            list: "B".into(),
            field: RuleField::Title,
            mode: MatchMode::Contains,
            pattern: title.to_string(),
        });
    }
    rules
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;

    pub fn init(enabled: bool, rules: Vec<Rule>) {
        let _ = (enabled, rules);
    }
    pub fn apply_enabled(_enabled: bool) {}
    pub fn set_rules(_rules: Vec<Rule>) {}
    pub fn in_memory_rules() -> Vec<Rule> {
        Vec::new()
    }
    pub fn list_windows() -> Vec<WindowInfo> {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        GetThreadId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EnumWindows, GetAncestor, GetClassNameW, GetWindowLongW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindow, IsWindowVisible, PeekMessageW, PostMessageW,
        PostThreadMessageW, ShowWindow, TranslateMessage, CHILDID_SELF, EVENT_OBJECT_SHOW,
        EVENT_SYSTEM_FOREGROUND, GA_ROOT, GWL_STYLE, MSG, OBJID_WINDOW, PM_REMOVE, SW_HIDE,
        WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_CLOSE, WM_QUIT, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_THICKFRAME,
    };

    // ─── 全局状态（参照 media_keys.rs） ───

    static RULES: Mutex<Vec<Rule>> = Mutex::new(Vec::new());
    static RUNNING: AtomicBool = AtomicBool::new(false);
    static WORKER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
    static SELF_EXE: OnceLock<String> = OnceLock::new();

    /// 通配符匹配（* / ?），大小写不敏感（调用方保证输入均已小写）
    fn wildcard_match(s: &[u8], pat: &[u8]) -> bool {
        let (mut si, mut pi) = (0usize, 0usize);
        let (mut star_s, mut star_p): (Option<usize>, Option<usize>) = (None, None);
        while si < s.len() {
            if pi < pat.len() && (pat[pi] == b'?' || pat[pi] == s[si]) {
                si += 1;
                pi += 1;
            } else if pi < pat.len() && pat[pi] == b'*' {
                star_p = Some(pi);
                star_s = Some(si);
                pi += 1;
            } else if let (Some(sp), Some(ps)) = (star_s, star_p) {
                pi = ps + 1;
                si = sp + 1;
                star_s = Some(si);
            } else {
                return false;
            }
        }
        while pi < pat.len() && pat[pi] == b'*' {
            pi += 1;
        }
        pi == pat.len()
    }

    // ─── 窗口/进程信息提取 ───

    fn get_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        if len <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize]).to_lowercase()
    }

    fn get_class(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        if len <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize]).to_lowercase()
    }

    fn pid_of(hwnd: HWND) -> Option<u32> {
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }

    fn process_path_ex(pid: u32) -> Option<String> {
        let handle = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)
        };
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle,
                0,
                buf.as_mut_ptr(),
                &mut size,
            )
        };
        unsafe { CloseHandle(handle) };
        if ok == 0 || size == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..size as usize]).to_lowercase())
    }

    fn process_name_of(hwnd: HWND) -> String {
        let Some(pid) = pid_of(hwnd) else { return String::new() };
        process_path_ex(pid)
            .and_then(|p| {
                p.rsplit('\\')
                    .next()
                    .map(|n| n.to_string())
            })
            .unwrap_or_default()
    }

    fn process_path_of(hwnd: HWND) -> String {
        pid_of(hwnd).and_then(process_path_ex).unwrap_or_default()
    }

    fn process_path_by_pid(pid: u32) -> String {
        process_path_ex(pid).unwrap_or_default()
    }

    // ─── 窗口外观判断（黑名单命中无需二次判断，仅日志/取窗用） ───

    fn looks_like_popup(hwnd: HWND) -> bool {
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        let resizable = (style & WS_THICKFRAME) != 0;
        let has_minmax = (style & (WS_MINIMIZEBOX | WS_MAXIMIZEBOX)) != 0;
        !resizable && !has_minmax
    }

    // ─── 保护名单 ───

    fn is_protected_name(exe: &str) -> bool {
        if let Some(self_exe) = SELF_EXE.get() {
            if exe == self_exe {
                return true;
            }
        }
        const PROTECTED: &[&str] = &[
            // 外壳与桌面
            "explorer.exe", "dwm.exe", "sihost.exe", "shellexperiencehost.exe",
            "startmenuexperiencehost.exe", "searchui.exe", "searchhost.exe", "searchapp.exe",
            "lockapp.exe", "applicationframehost.exe", "backgroundtaskhost.exe",
            "runtimebroker.exe", "credentialuibroker.exe", "consent.exe",
            "peopleexperiencehost.exe",
            // 登录与安全
            "winlogon.exe", "logonui.exe", "smartscreen.exe", "securityhealthsystray.exe",
            // 输入法与辅助功能
            "ctfmon.exe", "textinputhost.exe", "tabtip.exe", "osk.exe", "narrator.exe",
            "magnify.exe", "sethc.exe", "utilman.exe",
            // 系统工具与对话框
            "taskmgr.exe", "systemsettings.exe", "systemsettingsbroker.exe", "control.exe",
            "mmc.exe", "openwith.exe", "msiexec.exe", "sndvol.exe", "snippingtool.exe",
            "screensketch.exe", "mstsc.exe", "conhost.exe", "shellhost.exe", "mspaint.exe",
            "calc.exe", "svchost.exe", "services.exe", "lsass.exe", "csrss.exe",
            // Win11 小组件
            "widgets.exe", "widgetservice.exe",
            // 常见浏览器/终端
            "msedge.exe", "windowsterminal.exe", "chrome.exe", "firefox.exe",
        ];
        PROTECTED.contains(&exe)
    }

    fn is_protected(hwnd: HWND) -> bool {
        is_protected_name(&process_name_of(hwnd))
    }

    // ─── 规则匹配 ───

    /// 匹配结果
    enum Verdict {
        Allow,
        Block,
        None,
    }

    fn match_rule(target: &str, rule: &Rule) -> bool {
        if rule.pattern.is_empty() {
            return false;
        }
        match rule.mode {
            MatchMode::Exact => target == rule.pattern,
            MatchMode::Contains => target.contains(&rule.pattern),
            MatchMode::Wildcard => wildcard_match(target.as_bytes(), rule.pattern.as_bytes()),
        }
    }

    fn match_rules(hwnd: HWND) -> Verdict {
        let rules = RULES.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if rules.is_empty() {
            return Verdict::None;
        }
        // 惰性提取：多条规则共享同一窗口信息
        let mut exe: Option<String> = None;
        let mut path: Option<String> = None;
        let mut title: Option<String> = None;
        let mut cls: Option<String> = None;
        let mut matched_w = false;
        let mut matched_b = false;
        for rule in &rules {
            let target = match rule.field {
                RuleField::Exe => {
                    exe.get_or_insert_with(|| process_name_of(hwnd)).clone()
                }
                RuleField::Path => {
                    path.get_or_insert_with(|| process_path_of(hwnd)).clone()
                }
                RuleField::Title => {
                    title.get_or_insert_with(|| get_title(hwnd)).clone()
                }
                RuleField::Class => {
                    cls.get_or_insert_with(|| get_class(hwnd)).clone()
                }
            };
            if match_rule(&target, rule) {
                if rule.list == "W" {
                    matched_w = true;
                } else {
                    matched_b = true;
                }
            }
        }
        if matched_w {
            Verdict::Allow
        } else if matched_b {
            Verdict::Block
        } else {
            Verdict::None
        }
    }

    // ─── 拦截 ───

    fn block_window(hwnd: HWND) {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
            ShowWindow(hwnd, SW_HIDE);
        }
        // 黑名单强杀：400ms 后若窗口仍存在，对非系统路径进程 TerminateProcess
        // （HWND 指针非 Send，转 isize 捕获进线程）
        let hwnd_ptr = hwnd as isize;
        std::thread::Builder::new()
            .name("popnull-kill".into())
            .spawn(move || {
                let hwnd = hwnd_ptr as HWND;
                std::thread::sleep(std::time::Duration::from_millis(400));
                if unsafe { IsWindow(hwnd) } == 0 {
                    return;
                }
                let Some(pid) = pid_of(hwnd) else { return };
                let path = process_path_by_pid(pid);
                let is_system_path = path.starts_with("c:\\windows\\")
                    || path.starts_with("c:\\program files\\")
                    || path.starts_with("c:\\program files (x86)\\");
                if is_system_path || path.is_empty() {
                    return;
                }
                let handle = unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                        0,
                        pid,
                    )
                };
                if handle.is_null() {
                    return;
                }
                unsafe {
                    TerminateProcess(handle, 0);
                    CloseHandle(handle);
                }
                log::info!("[PopNull] 已强杀黑名单进程 pid={pid} path={path}");
            })
            .ok();
    }

    // ─── WinEvent 钩子 ───

    unsafe extern "system" fn win_event_proc(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        id_object: i32,
        id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if id_object != OBJID_WINDOW || id_child != CHILDID_SELF as i32 {
            return;
        }
        if hwnd.is_null() {
            return;
        }
        if IsWindowVisible(hwnd) == 0 {
            return;
        }
        // 仅处理顶层窗口
        if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return;
        }
        // 保护进程（自身 + 系统进程）直接放行
        if is_protected(hwnd) {
            return;
        }

        match match_rules(hwnd) {
            Verdict::Allow => {
                log::info!(
                    "[PopNull] 白名单放行 exe={} title={}",
                    process_name_of(hwnd),
                    get_title(hwnd)
                );
            }
            Verdict::Block => {
                let exe = process_name_of(hwnd);
                let title = get_title(hwnd);
                let is_pop = looks_like_popup(hwnd);
                log::info!(
                    "[PopNull] 拦截黑名单窗口 exe={exe} title={title} looks_like_popup={is_pop}"
                );
                block_window(hwnd);
            }
            Verdict::None => {}
        }
    }

    // ─── 钩子线程 ───

    fn worker_main() {
        unsafe {
            let hook_show = SetWinEventHook(
                EVENT_OBJECT_SHOW,
                EVENT_OBJECT_SHOW,
                std::ptr::null_mut(),
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            let hook_fg = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            log::info!(
                "[PopNull] WinEvent 钩子已挂载 show={} fg={}",
                !hook_show.is_null(),
                !hook_fg.is_null()
            );

            let mut msg: MSG = std::mem::zeroed();
            'outer: loop {
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        break 'outer;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                if !RUNNING.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            if !hook_show.is_null() {
                UnhookWinEvent(hook_show);
            }
            if !hook_fg.is_null() {
                UnhookWinEvent(hook_fg);
            }
            log::info!("[PopNull] 钩子线程退出");
        }
    }

    // ─── 引擎启停 ───

    pub fn start() {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        let handle = std::thread::Builder::new()
            .name("popnull-win-events".into())
            .spawn(worker_main);
        match handle {
            Ok(h) => {
                *WORKER.lock().unwrap_or_else(|p| p.into_inner()) = Some(h);
            }
            Err(e) => {
                RUNNING.store(false, Ordering::SeqCst);
                log::error!("[PopNull] 启动钩子线程失败: {e}");
            }
        }
    }

    pub fn stop() {
        if !RUNNING.swap(false, Ordering::SeqCst) {
            return;
        }
        let tid = {
            let guard = WORKER.lock().unwrap_or_else(|p| p.into_inner());
            guard.as_ref().map(|h| {
                let raw = h.as_raw_handle();
                unsafe { GetThreadId(raw as *mut _) }
            })
        };
        if let Some(tid) = tid {
            unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) };
        }
        if let Some(h) = WORKER.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = h.join();
        }
        log::info!("[PopNull] 引擎已停止");
    }

    pub fn apply_enabled(enabled: bool) {
        if enabled {
            start();
        } else {
            stop();
        }
    }

    // ─── 规则管理 ───

    pub fn set_rules(rules: Vec<Rule>) {
        let mut guard = RULES.lock().unwrap_or_else(|p| p.into_inner());
        *guard = rules;
    }

    pub fn in_memory_rules() -> Vec<Rule> {
        RULES.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn init(enabled: bool, rules: Vec<Rule>) {
        let _ = SELF_EXE.get_or_init(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                })
                .unwrap_or_default()
        });
        set_rules(rules);
        if enabled {
            start();
        }
    }

    // ─── 选取窗口 ───

    struct WindowCollector {
        items: Vec<WindowInfo>,
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let collector = unsafe { &mut *(lparam as *mut WindowCollector) };
        if collector.items.len() >= 200 {
            return 0; // 停止枚举
        }
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return 1;
        }
        let exe = process_name_of(hwnd);
        if exe.is_empty() || is_protected_name(&exe) {
            return 1;
        }
        let title = get_title(hwnd);
        if title.is_empty() {
            return 1;
        }
        collector.items.push(WindowInfo {
            hwnd: hwnd as i64,
            title,
            exe,
            path: process_path_of(hwnd),
            class: get_class(hwnd),
        });
        1
    }

    pub fn list_windows() -> Vec<WindowInfo> {
        let mut collector = WindowCollector { items: Vec::new() };
        unsafe {
            EnumWindows(Some(enum_windows_proc), &mut collector as *mut WindowCollector as LPARAM);
        }
        collector.items
    }
}

// ─── 对外接口 ───

/// 应用启动时初始化（setup 调用）：读取设置 + 装载规则 + enabled 时启动引擎
pub fn init(app: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    {
        let enabled = crate::hotkey::read_settings_value(&app, "nexbox_popnull_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let rules = crate::hotkey::read_settings_value(&app, "nexbox_popnull_rules")
            .and_then(|v| serde_json::from_value::<Vec<Rule>>(v).ok())
            .filter(|r| !r.is_empty())
            .unwrap_or_else(default_rules);
        log::info!(
            "[PopNull] 启动初始化 enabled={enabled} rules={}",
            rules.len()
        );
        imp::init(enabled, rules);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
    }
}

/// 查询引擎状态（enabled + 当前规则）
#[tauri::command]
pub fn popnull_get_state(app: tauri::AppHandle) -> PopNullState {
    let enabled = crate::hotkey::read_settings_value(&app, "nexbox_popnull_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    PopNullState {
        enabled,
        rules: imp::in_memory_rules(),
    }
}

/// 启用/停用弹窗拦截（持久化由前端 store 统一负责）
#[tauri::command]
pub fn popnull_set_enabled(enabled: bool) {
    log::info!("[PopNull] 开关命令 popnull_set_enabled({enabled})");
    imp::apply_enabled(enabled);
}

/// 替换内存规则（持久化由前端 store 统一负责）
#[tauri::command]
pub fn popnull_set_rules(rules: Vec<Rule>) {
    log::info!("[PopNull] 更新规则条数 rules={}", rules.len());
    imp::set_rules(rules);
}

/// 枚举可见顶层窗口（供「选取窗口」）
#[tauri::command]
pub fn popnull_list_windows() -> Result<Vec<WindowInfo>, String> {
    Ok(imp::list_windows())
}