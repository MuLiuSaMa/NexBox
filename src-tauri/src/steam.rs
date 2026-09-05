use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

use reqwest::Client;

// ======================== VDF 解析器 ========================

/// VDF 值类型
#[derive(Debug, Clone)]
enum VdfValue {
    String(String),
    Object(Vec<(String, VdfValue)>),
}

impl VdfValue {
    fn get_str(&self, key: &str) -> Option<&str> {
        if let VdfValue::Object(entries) = self {
            for (k, v) in entries {
                if k == key {
                    if let VdfValue::String(s) = v {
                        return Some(s);
                    }
                }
            }
        }
        None
    }

    fn get_obj(&self, key: &str) -> Option<&VdfValue> {
        if let VdfValue::Object(entries) = self {
            for (k, v) in entries {
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }

    fn get_obj_entries(&self) -> Option<&[(String, VdfValue)]> {
        if let VdfValue::Object(entries) = self {
            Some(entries)
        } else {
            None
        }
    }
}

/// VDF 解析器
struct VdfParser {
    chars: Vec<char>,
    pos: usize,
}

impl VdfParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    /// 解析整个 VDF 文件，根级别是多个 key-value 对（无外层大括号）
    fn parse(&mut self) -> Result<VdfValue, String> {
        self.skip_whitespace_and_comments();
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                break;
            }
            // 根级别遇到 } 就结束（安全措施）
            if self.chars[self.pos] == '}' {
                break;
            }
            // 解析 key
            let key = if self.pos < self.chars.len() && self.chars[self.pos] == '"' {
                self.parse_quoted_string()?
            } else {
                self.parse_unquoted_token()?
            };
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                entries.push((key, VdfValue::String(String::new())));
                break;
            }
            // 解析 value
            let value = self.parse_value()?;
            entries.push((key, value));
        }
        Ok(VdfValue::Object(entries))
    }

    fn parse_value(&mut self) -> Result<VdfValue, String> {
        self.skip_whitespace_and_comments();
        if self.pos >= self.chars.len() {
            return Err("Unexpected end of input".into());
        }
        match self.chars[self.pos] {
            '"' => Ok(VdfValue::String(self.parse_quoted_string()?)),
            '{' => self.parse_object(),
            _ => Ok(VdfValue::String(self.parse_unquoted_token()?)),
        }
    }

    fn parse_quoted_string(&mut self) -> Result<String, String> {
        self.pos += 1; // 跳过开头的 "
        let mut result = String::new();
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            match c {
                '"' => {
                    self.pos += 1;
                    return Ok(result);
                }
                '\\' => {
                    self.pos += 1;
                    if self.pos < self.chars.len() {
                        result.push(self.chars[self.pos]);
                        self.pos += 1;
                    }
                }
                _ => {
                    result.push(c);
                    self.pos += 1;
                }
            }
        }
        Err("Unterminated string".into())
    }

    fn parse_unquoted_token(&mut self) -> Result<String, String> {
        let mut result = String::new();
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                break;
            }
            result.push(c);
            self.pos += 1;
        }
        if result.is_empty() {
            return Err(format!("Unexpected char at pos {}", self.pos));
        }
        Ok(result)
    }

    fn parse_object(&mut self) -> Result<VdfValue, String> {
        self.pos += 1; // 跳过 {
        let mut entries = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                return Err("Unexpected end of object".into());
            }
            if self.chars[self.pos] == '}' {
                self.pos += 1;
                break;
            }
            // 解析 key
            let key = if self.chars[self.pos] == '"' {
                self.parse_quoted_string()?
            } else {
                self.parse_unquoted_token()?
            };
            self.skip_whitespace_and_comments();
            // 解析 value
            let value = self.parse_value()?;
            entries.push((key, value));
        }
        Ok(VdfValue::Object(entries))
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c.is_whitespace() {
                self.pos += 1;
            } else if c == '/' && self.pos + 1 < self.chars.len() {
                if self.chars[self.pos + 1] == '/' {
                    while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                        self.pos += 1;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
}

// ======================== 数据结构 ========================

/// Steam 安装信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamInstallInfo {
    pub installed: bool,
    pub install_path: Option<String>,
    pub is_running: bool,
}

/// Steam 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamUser {
    pub steam_id64: String,
    pub account_name: String,
    pub persona_name: String,
    pub most_recent: bool,
    pub remember_password: bool,
    pub timestamp: u64,
    pub avatar_url: Option<String>,
    pub avatar_medium_url: Option<String>,
    pub avatar_full_url: Option<String>,
}

/// Steam 游戏库文件夹
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamLibrary {
    pub index: u32,
    pub path: String,
    pub label: String,
    pub total_size: u64,
    pub free_size: u64,
    pub apps: Vec<String>,
}

/// Steam 已安装游戏
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamGame {
    pub app_id: u32,
    pub name: String,
    pub install_dir: String,
    pub library_path: String,
    pub size_on_disk: u64,
    pub state_flags: u32,
    pub last_updated: u64,
    pub last_owner: String,
    pub build_id: u32,
    pub bytes_to_download: u64,
    pub bytes_downloaded: u64,
    /// 游玩时长（分钟），来自 localconfig.vdf
    pub playtime_minutes: u64,
    /// 最近游玩时间（unix 秒），来自 localconfig.vdf
    pub last_played: i64,
}

// ======================== 核心逻辑 ========================

/// 获取 Steam 安装路径
#[cfg(windows)]
fn get_steam_path() -> Option<String> {
    // 优先读 HKCU
    if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
        if let Ok(path) = hkcu.get_value::<String, _>("SteamPath") {
            return Some(path.replace('/', "\\"));
        }
    }

    // 备选：读 HKLM (64位/32位)
    for flag in &[KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
        if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags("SOFTWARE\\Valve\\Steam", KEY_READ | *flag)
        {
            if let Ok(path) = hklm.get_value::<String, _>("InstallPath") {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(not(windows))]
fn get_steam_path() -> Option<String> {
    None
}

/// 检测 Steam 是否正在运行
fn is_steam_running() -> bool {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_processes();

    sys.processes()
        .values()
        .any(|p| p.name().eq_ignore_ascii_case("steam.exe"))
}

/// 从注册表读取当前活跃的 Steam 用户 Steam3 ID，转换为 Steam64
#[cfg(windows)]
fn get_active_steam_user_from_registry() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey("Software\\Valve\\Steam\\ActiveProcess").ok()?;
    let active_user: u32 = key.get_value("ActiveUser").ok()?;
    if active_user == 0 {
        return None;
    }
    // Steam64 ID = 76561197960265728 + Steam3 account ID
    let steam64 = 76561197960265728u64 + active_user as u64;
    Some(steam64.to_string())
}

#[cfg(not(windows))]
fn get_active_steam_user_from_registry() -> Option<String> {
    None
}

/// 解析 loginusers.vdf 获取用户列表
fn parse_login_users(steam_dir: &str, is_running: bool) -> Vec<SteamUser> {
    let path = format!("{}\\config\\loginusers.vdf", steam_dir);
    log::info!("[Steam] Reading loginusers.vdf from: {}", path);
    let content = match fs::read_to_string(&path) {
        Ok(c) => {
            log::info!("[Steam] loginusers.vdf read OK, {} bytes", c.len());
            c
        }
        Err(e) => {
            log::warn!("[Steam] Failed to read loginusers.vdf: {}", e);
            return vec![];
        }
    };

    let mut parser = VdfParser::new(&content);
    let root = match parser.parse() {
        Ok(v) => v,
        Err(e) => {
            log::error!("[Steam] VDF parse error in loginusers.vdf: {}", e);
            return vec![];
        }
    };

    let mut users = vec![];
    // loginusers.vdf 格式: "users" { "steamid" { ... } }
    // root 是 Object([("users", Object([("steamid", Object(...)), ...]))])
    let users_obj = match root.get_obj("users") {
        Some(o) => o,
        None => {
            log::warn!("[Steam] 'users' key not found in loginusers.vdf root");
            // 尝试直接遍历根级别（某些格式可能没有 "users" 包装）
            if let Some(entries) = root.get_obj_entries() {
                for (steam_id, user_data) in entries {
                    if steam_id == "users" {
                        continue; // 跳过 "users" 键本身
                    }
                    if let VdfValue::Object(_) = user_data {
                        let user = parse_single_user(steam_id, user_data);
                        users.push(user);
                    }
                }
            }
            return users;
        }
    };

    if let Some(entries) = users_obj.get_obj_entries() {
        for (steam_id, user_data) in entries {
            let user = parse_single_user(steam_id, user_data);
            users.push(user);
        }
    }

    log::info!("[Steam] Parsed {} users from loginusers.vdf", users.len());

    // 按 MostRecent 排序，当前用户排第一
    users.sort_by(|a, b| b.most_recent.cmp(&a.most_recent));

    // 如果 Steam 正在运行但 loginusers.vdf 中没有标记 MostRecent，
    // 尝试从注册表 ActiveProcess\ActiveUser 读取当前活跃用户
    let has_most_recent = users.iter().any(|u| u.most_recent);
    if is_running && !has_most_recent {
        if let Some(active_steam64) = get_active_steam_user_from_registry() {
            log::info!("[Steam] Active user from registry: {}", active_steam64);
            for user in &mut users {
                if user.steam_id64 == active_steam64 {
                    user.most_recent = true;
                    log::info!("[Steam] Set most_recent by registry: {}", user.account_name);
                    break;
                }
            }
            // 重新排序
            users.sort_by(|a, b| b.most_recent.cmp(&a.most_recent));
        }
    }

    users
}

fn parse_single_user(steam_id: &str, user_data: &VdfValue) -> SteamUser {
    SteamUser {
        steam_id64: steam_id.to_string(),
        account_name: user_data.get_str("AccountName").unwrap_or("").to_string(),
        persona_name: user_data.get_str("PersonaName").unwrap_or("").to_string(),
        most_recent: user_data.get_str("MostRecent").unwrap_or("0") == "1",
        remember_password: user_data.get_str("RememberPassword").unwrap_or("0") == "1",
        timestamp: user_data
            .get_str("Timestamp")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        avatar_url: None,
        avatar_medium_url: None,
        avatar_full_url: None,
    }
}

/// 解析 libraryfolders.vdf 获取游戏库列表
fn parse_library_folders(steam_dir: &str) -> Vec<SteamLibrary> {
    // 新格式: {SteamDir}/config/libraryfolders.vdf
    let path = format!("{}\\config\\libraryfolders.vdf", steam_dir);
    log::info!("[Steam] Reading libraryfolders.vdf from: {}", path);
    let content = match fs::read_to_string(&path) {
        Ok(c) => {
            log::info!("[Steam] libraryfolders.vdf read OK, {} bytes", c.len());
            c
        }
        Err(e) => {
            log::warn!("[Steam] Failed to read config/libraryfolders.vdf: {}, trying steamapps/", e);
            // 旧格式: {SteamDir}/steamapps/libraryfolders.vdf
            let old_path = format!("{}\\steamapps\\libraryfolders.vdf", steam_dir);
            match fs::read_to_string(&old_path) {
                Ok(c) => c,
                Err(e2) => {
                    log::warn!("[Steam] Failed to read steamapps/libraryfolders.vdf: {}", e2);
                    return vec![];
                }
            }
        }
    };

    let mut parser = VdfParser::new(&content);
    let root = match parser.parse() {
        Ok(v) => v,
        Err(e) => {
            log::error!("[Steam] VDF parse error in libraryfolders.vdf: {}", e);
            return vec![];
        }
    };

    let mut libraries = vec![];
    // libraryfolders.vdf 格式: "libraryfolders" { "0" { "path" "..." "apps" { ... } } }
    // root 是 Object([("libraryfolders", Object([("0", Object(...)), ...]))])
    let lf_obj = match root.get_obj("libraryfolders") {
        Some(o) => o,
        None => {
            log::warn!("[Steam] 'libraryfolders' key not found, trying root entries directly");
            // 尝试直接遍历根级别
            if let Some(entries) = root.get_obj_entries() {
                for (index_str, folder_data) in entries {
                    if index_str == "libraryfolders" {
                        // 值就是真正的库列表
                        if let VdfValue::Object(_) = folder_data {
                            if let Some(inner_entries) = folder_data.get_obj_entries() {
                                for (idx, fd) in inner_entries {
                                    if let Some(lib) = parse_single_library(idx, fd) {
                                        libraries.push(lib);
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    // 直接是库条目
                    if let Some(lib) = parse_single_library(index_str, folder_data) {
                        libraries.push(lib);
                    }
                }
            }
            log::info!("[Steam] Parsed {} libraries (fallback path)", libraries.len());
            return libraries;
        }
    };

    if let Some(entries) = lf_obj.get_obj_entries() {
        for (index_str, folder_data) in entries {
            if let Some(lib) = parse_single_library(index_str, folder_data) {
                libraries.push(lib);
            }
        }
    }

    log::info!("[Steam] Parsed {} libraries", libraries.len());
    libraries
}

fn parse_single_library(index_str: &str, folder_data: &VdfValue) -> Option<SteamLibrary> {
    let index: u32 = index_str.parse().unwrap_or(0);
    let path = folder_data.get_str("path")?.to_string();
    if path.is_empty() {
        return None;
    }

    let mut apps = vec![];
    if let Some(app_entries) = folder_data.get_obj("apps") {
        if let Some(app_list) = app_entries.get_obj_entries() {
            for (app_id, _) in app_list {
                apps.push(app_id.clone());
            }
        }
    }

    let clean_path = path.replace('/', "\\");
    // 优先使用 VDF 中的 totalsize（用户不需要实时获取磁盘容量）
    let vdf_total: u64 = folder_data.get_str("totalsize").and_then(|s| s.parse().ok()).unwrap_or(0);
    let (disk_total, disk_free) = get_disk_space(&clean_path).unwrap_or((0, 0));
    let total_size = if vdf_total > 0 { vdf_total } else if disk_total > 0 { disk_total } else { 0 };

       Some(SteamLibrary {
        index,
        path: clean_path,
        label: folder_data.get_str("label").unwrap_or("").to_string(),
        total_size,
        free_size: disk_free,
        apps,
    })
}

/// 从 localconfig.vdf 解析各游戏的游玩时长（分钟）与最近游玩时间（unix 秒）。
/// 结构: UserLocalConfigStore -> Software -> Valve -> Steam -> apps -> {appid} -> Playtime / LastPlayed
/// 注意：userdata 目录名是 Steam3 Account ID（10 位数字），非 Steam64；字段为 Playtime（旧版为 Playtime2）。
fn parse_local_config_playtimes(steam_dir: &str) -> std::collections::HashMap<u32, (u64, i64)> {
    use std::collections::HashMap;
    let mut result: HashMap<u32, (u64, i64)> = HashMap::new();

    let userdata_dir = format!("{}\\userdata", steam_dir);
    let userdata = match fs::read_dir(&userdata_dir) {
        Ok(d) => d,
        Err(_) => return result,
    };

    // 收集用户目录：当前活跃用户（注册表 ActiveUser 即目录名）优先，再补充其他纯数字目录
    let mut user_dirs: Vec<String> = vec![];
    #[cfg(target_os = "windows")]
    {
        if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Valve\\Steam\\ActiveProcess")
        {
            if let Ok(active_user) = hkcu.get_value::<u32, _>("ActiveUser") {
                if active_user != 0 {
                    user_dirs.push(active_user.to_string());
                }
            }
        }
    }
    for entry in userdata.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_digit())
                && !user_dirs.contains(&name)
            {
                user_dirs.push(name);
            }
        }
    }

    for user_id in user_dirs {
        let local_path = format!("{}\\{}\\config\\localconfig.vdf", userdata_dir, user_id);
        let content = match fs::read_to_string(&local_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let mut parser = VdfParser::new(&content);
        let root = match parser.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let Some(store) = root.get_obj("UserLocalConfigStore") else { continue };
        let Some(software) = store.get_obj("Software") else { continue };
        let Some(valve) = software.get_obj("Valve") else { continue };
        let Some(steam) = valve.get_obj("Steam") else { continue };
        let Some(apps) = steam.get_obj("apps") else { continue };
        let Some(entries) = apps.get_obj_entries() else { continue };

        for (appid_str, app_data) in entries {
            let appid: u32 = match appid_str.parse() {
                Ok(a) => a,
                Err(_) => continue,
            };
            // 新版字段为 Playtime，旧版为 Playtime2
            let playtime = app_data
                .get_str("Playtime")
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| {
                    app_data
                        .get_str("Playtime2")
                        .and_then(|s| s.parse::<u64>().ok())
                })
                .unwrap_or(0);
            let last_played = app_data
                .get_str("LastPlayed")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);

            // 多用户时取时长/最近游玩的最大值
            let entry = result.entry(appid).or_insert((0, 0));
            if playtime > entry.0 {
                entry.0 = playtime;
            }
            if last_played > entry.1 {
                entry.1 = last_played;
            }
        }
    }

    result
}

/// 扫描已安装的游戏
fn scan_installed_games(libraries: &[SteamLibrary]) -> Vec<SteamGame> {
    let mut games = vec![];

    // 始终扫描 Steam 主安装目录的 steamapps（即使 libraryfolders.vdf 解析失败）
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };

    // 解析本地游玩时长/最近游玩（localconfig.vdf）
    let playtimes = parse_local_config_playtimes(&steam_dir);

    // 收集所有需要扫描的 steamapps 目录，并记录对应的库路径
    // (steamapps_dir, library_path) library_path 是去掉 \steamapps 后的库根路径
    let mut scan_dirs: Vec<(String, String)> = vec![];
    let main_steamapps = format!("{}\\steamapps", steam_dir);
    scan_dirs.push((main_steamapps.clone(), steam_dir.clone()));

    for lib in libraries {
        let lib_steamapps = format!("{}\\steamapps", lib.path);
        // 避免重复扫描主目录
        if !scan_dirs.iter().any(|(p, _)| p.eq_ignore_ascii_case(&lib_steamapps)) {
            scan_dirs.push((lib_steamapps, lib.path.clone()));
        }
    }

    log::info!(
        "[Steam] Scanning {} steamapps directories for game manifests",
        scan_dirs.len()
    );

    for (steamapps_dir, lib_path) in &scan_dirs {
        let entries = match fs::read_dir(steamapps_dir) {
            Ok(e) => e,
            Err(e) => {
                log::warn!(
                    "[Steam] Cannot read steamapps dir: {} ({})",
                    steamapps_dir,
                    e
                );
                continue;
            }
        };

        let mut dir_game_count = 0;
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();

            if !file_name.starts_with("appmanifest_") || !file_name.ends_with(".acf") {
                continue;
            }

            let content = match fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut parser = VdfParser::new(&content);
            let root = match parser.parse() {
                Ok(v) => v,
                Err(e) => {
                    log::warn!(
                        "[Steam] VDF parse error in {}: {}",
                        file_name,
                        e
                    );
                    continue;
                }
            };

            if let Some(app_state) = root.get_obj("AppState") {
                let app_id: u32 = app_state
                    .get_str("appid")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let (playtime_minutes, last_played) = playtimes
                    .get(&app_id)
                    .copied()
                    .unwrap_or((0, 0));
                let game = SteamGame {
                    app_id,
                    name: app_state.get_str("name").unwrap_or("").to_string(),
                    install_dir: app_state
                        .get_str("installdir")
                        .unwrap_or("")
                        .to_string(),
                    library_path: lib_path.clone(),
                    size_on_disk: app_state
                        .get_str("SizeOnDisk")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    state_flags: app_state
                        .get_str("StateFlags")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    last_updated: app_state
                        .get_str("LastUpdated")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    last_owner: app_state
                        .get_str("LastOwner")
                        .unwrap_or("")
                        .to_string(),
                    build_id: app_state
                        .get_str("buildid")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    bytes_to_download: app_state
                        .get_str("BytesToDownload")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    bytes_downloaded: app_state
                        .get_str("BytesDownloaded")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    playtime_minutes,
                    last_played,
                };
                if game.app_id > 0 {
                    games.push(game);
                    dir_game_count += 1;
                }
            } else {
                log::warn!("[Steam] 'AppState' key not found in {}", file_name);
            }
        }
        log::info!(
            "[Steam] Found {} games in {}",
            dir_game_count,
            steamapps_dir
        );
    }

    // 去重（同一游戏可能在不同库目录出现）
    let mut seen = std::collections::HashSet::new();
    games.retain(|g| seen.insert(g.app_id));

    // 按名称排序
    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    log::info!("[Steam] Total unique installed games: {}", games.len());
    games
}

/// 格式化文件大小
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    format!("{:.1} {}", size, UNITS[unit_index])
}

/// 从 Steam 社区 XML 获取头像 URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamAvatarInfo {
    pub steam_id64: String,
    pub avatar_url: Option<String>,
    pub avatar_medium_url: Option<String>,
    pub avatar_full_url: Option<String>,
}

async fn fetch_one_avatar(client: &Client, steam_id64: &str) -> Option<(String, String, String)> {
    let url = format!(
        "https://steamcommunity.com/profiles/{}/?xml=1",
        steam_id64
    );
    let resp = match client
        .get(&url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        )
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!(
                "[Steam] avatar online request failed for {}: {}",
                steam_id64,
                e
            );
            return None;
        }
    };
    if !resp.status().is_success() {
        log::warn!(
            "[Steam] avatar online HTTP {} for {}",
            resp.status(),
            steam_id64
        );
        return None;
    }
    let text = match resp.text().await {
        Ok(t) => t,
        Err(e) => {
            log::warn!(
                "[Steam] avatar online body read failed for {}: {}",
                steam_id64,
                e
            );
            return None;
        }
    };
    let avatar = extract_xml_tag(&text, "avatarIcon").unwrap_or_default();
    let avatar_medium = extract_xml_tag(&text, "avatarMedium").unwrap_or_default();
    let avatar_full = extract_xml_tag(&text, "avatarFull").unwrap_or_default();
    if avatar.is_empty() && avatar_medium.is_empty() && avatar_full.is_empty() {
        log::warn!(
            "[Steam] avatar online XML tags empty for {} (profile may be private or region blocked)",
            steam_id64
        );
    }
    Some((avatar, avatar_medium, avatar_full))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start_pattern = format!("<{}>", tag);
    let end_pattern = format!("</{}>", tag);

    let start = xml.find(&start_pattern)?;
    let content_start = start + start_pattern.len();
    let end = xml[content_start..].find(&end_pattern)?;

    let raw = &xml[content_start..content_start + end];
    if raw.is_empty() {
        return None;
    }
    // 如果有 CDATA 包裹，提取 CDATA 内的内容
    if raw.contains("<![CDATA[") {
        if let Some(cdata_start) = raw.find("<![CDATA[") {
            let inner_start = cdata_start + "<![CDATA[".len();
            if let Some(cdata_end) = raw[inner_start..].find("]]>") {
                let content = raw[inner_start..inner_start + cdata_end].to_string();
                if !content.is_empty() {
                    return Some(content);
                }
            }
        }
        return None;
    }
    Some(raw.to_string())
}

/// 从 Steam 本地缓存 avatarcache 读取用户头像，转成 base64 data URL。
/// Steam 客户端在用户登录时会自动将头像缓存到 {steam_dir}\config\avatarcache\{steam_id64}.png，
/// 无需联网即可显示，可避免国内访问 steamcommunity.com 不稳定导致头像加载失败。
fn read_local_avatar(steam_dir: &str, steam_id64: &str) -> Option<String> {
    let path = format!("{}\\config\\avatarcache\\{}.png", steam_dir, steam_id64);
    match fs::read(&path) {
        Ok(bytes) if !bytes.is_empty() => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            log::info!("[Steam] local avatar cache hit for {}", steam_id64);
            Some(format!("data:image/png;base64,{}", b64))
        }
        Ok(_) => {
            log::warn!("[Steam] local avatar cache empty file for {}", steam_id64);
            None
        }
        Err(_) => {
            log::info!("[Steam] no local avatar cache for {}", steam_id64);
            None
        }
    }
}

/// 获取指定 Steam 用户的头像（本地缓存优先，在线接口兜底）
#[tauri::command]
pub async fn get_steam_user_avatars() -> Vec<SteamAvatarInfo> {
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };
    let users = parse_login_users(&steam_dir, is_steam_running());
    if users.is_empty() {
        return vec![];
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    let mut tasks = tokio::task::JoinSet::new();
    for user in users {
        let steam_id64 = user.steam_id64.clone();
        let steam_dir = steam_dir.clone();
        let client = client.clone();
        tasks.spawn(async move {
            // 本地缓存命中就直接返回，跳过极可能超时的在线请求
            let local = read_local_avatar(&steam_dir, &steam_id64);
            if local.is_some() {
                return (steam_id64, local, None);
            }
            // 本地无缓存才尝试在线接口（国内访问 steamcommunity 多半超时）
            let online = fetch_one_avatar(&client, &steam_id64).await;
            (steam_id64, local, online)
        });
    }

    let mut results = vec![];
    while let Some(task_result) = tasks.join_next().await {
        if let Ok((steam_id64, local, online)) = task_result {
            let (o_icon, o_med, o_full) = online.unwrap_or_default();
            // 在线 URL 优先，本地 data URL 兜底
            let avatar_url = if !o_icon.is_empty() {
                Some(o_icon)
            } else {
                local.clone()
            };
            let avatar_medium_url = if !o_med.is_empty() {
                Some(o_med)
            } else {
                local.clone()
            };
            let avatar_full_url = if !o_full.is_empty() {
                Some(o_full)
            } else {
                local
            };
            results.push(SteamAvatarInfo {
                steam_id64,
                avatar_url,
                avatar_medium_url,
                avatar_full_url,
            });
        }
    }

    results
}

// ======================== Tauri 命令 ========================

/// 获取 Steam 安装信息
#[tauri::command]
pub async fn get_steam_install_info() -> SteamInstallInfo {
    let install_path = get_steam_path();
    let installed = install_path.is_some();
    let is_running = if installed {
        is_steam_running()
    } else {
        false
    };

    SteamInstallInfo {
        installed,
        install_path,
        is_running,
    }
}

/// 获取 Steam 已记住的用户列表
#[tauri::command]
pub async fn get_steam_users() -> Vec<SteamUser> {
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };
    parse_login_users(&steam_dir, is_steam_running())
}

/// 获取 Steam 游戏库文件夹列表
#[tauri::command]
pub async fn get_steam_libraries() -> Vec<SteamLibrary> {
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };
    parse_library_folders(&steam_dir)
}

/// 获取已安装的 Steam 游戏列表
#[tauri::command]
pub async fn get_steam_games() -> Vec<SteamGame> {
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };
    let libraries = parse_library_folders(&steam_dir);
    scan_installed_games(&libraries)
}

/// 获取所有 Steam 数据（一次性返回，减少前端请求次数）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamAllData {
    pub install_info: SteamInstallInfo,
    pub users: Vec<SteamUser>,
    pub libraries: Vec<SteamLibrary>,
    pub games: Vec<SteamGame>,
}

#[tauri::command]
pub async fn get_steam_all_data() -> SteamAllData {
    let install_path = get_steam_path();
    let installed = install_path.is_some();
    let is_running = if installed { is_steam_running() } else { false };

    let install_info = SteamInstallInfo {
        installed,
        install_path: install_path.clone(),
        is_running,
    };

    let steam_dir = match install_path {
        Some(p) => p,
        None => {
            return SteamAllData {
                install_info,
                users: vec![],
                libraries: vec![],
                games: vec![],
            }
        }
    };

    let users = parse_login_users(&steam_dir, is_running);
    let libraries = parse_library_folders(&steam_dir);
    let games = scan_installed_games(&libraries);

    SteamAllData {
        install_info,
        users,
        libraries,
        games,
    }
}

/// 调试命令：返回详细的诊断信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamDebugInfo {
    pub steam_path: Option<String>,
    pub steam_running: bool,
    pub loginusers_path: String,
    pub loginusers_exists: bool,
    pub loginusers_raw: Option<String>,
    pub loginusers_parse_error: Option<String>,
    pub loginusers_count: usize,
    pub libraryfolders_path: String,
    pub libraryfolders_exists: bool,
    pub libraryfolders_raw: Option<String>,
    pub libraryfolders_parse_error: Option<String>,
    pub libraryfolders_count: usize,
    pub steamapps_dirs_scanned: Vec<String>,
    pub appmanifest_files_found: Vec<String>,
    pub games_count: usize,
    pub games_parse_errors: Vec<String>,
}

#[tauri::command]
pub async fn steam_debug() -> SteamDebugInfo {
    let steam_path = get_steam_path();
    let steam_running = is_steam_running();

    let mut info = SteamDebugInfo {
        steam_path: steam_path.clone(),
        steam_running,
        loginusers_path: String::new(),
        loginusers_exists: false,
        loginusers_raw: None,
        loginusers_parse_error: None,
        loginusers_count: 0,
        libraryfolders_path: String::new(),
        libraryfolders_exists: false,
        libraryfolders_raw: None,
        libraryfolders_parse_error: None,
        libraryfolders_count: 0,
        steamapps_dirs_scanned: vec![],
        appmanifest_files_found: vec![],
        games_count: 0,
        games_parse_errors: vec![],
    };

    let steam_dir = match steam_path {
        Some(p) => p,
        None => return info,
    };

    // 检查 loginusers.vdf
    info.loginusers_path = format!("{}\\config\\loginusers.vdf", steam_dir);
    info.loginusers_exists = Path::new(&info.loginusers_path).exists();
    if info.loginusers_exists {
        let content = fs::read_to_string(&info.loginusers_path).unwrap_or_default();
        info.loginusers_raw = Some(content.clone());
        let mut parser = VdfParser::new(&content);
        match parser.parse() {
            Ok(root) => {
                if let Some(users_obj) = root.get_obj("users") {
                    if let Some(entries) = users_obj.get_obj_entries() {
                        info.loginusers_count = entries.len();
                    }
                }
            }
            Err(e) => {
                info.loginusers_parse_error = Some(e);
            }
        }
    }

    // 检查 libraryfolders.vdf
    info.libraryfolders_path = format!("{}\\config\\libraryfolders.vdf", steam_dir);
    info.libraryfolders_exists = Path::new(&info.libraryfolders_path).exists();
    if !info.libraryfolders_exists {
        info.libraryfolders_path = format!("{}\\steamapps\\libraryfolders.vdf", steam_dir);
        info.libraryfolders_exists = Path::new(&info.libraryfolders_path).exists();
    }
    if info.libraryfolders_exists {
        let content = fs::read_to_string(&info.libraryfolders_path).unwrap_or_default();
        info.libraryfolders_raw = Some(content.clone());
        let mut parser = VdfParser::new(&content);
        match parser.parse() {
            Ok(root) => {
                if let Some(lf_obj) = root.get_obj("libraryfolders") {
                    if let Some(entries) = lf_obj.get_obj_entries() {
                        info.libraryfolders_count = entries.len();
                    }
                }
            }
            Err(e) => {
                info.libraryfolders_parse_error = Some(e);
            }
        }
    }

    // 扫描 steamapps 目录
    let libraries = parse_library_folders(&steam_dir);
    let mut scan_dirs = vec![format!("{}\\steamapps", steam_dir)];
    for lib in &libraries {
        let dir = format!("{}\\steamapps", lib.path);
        if !scan_dirs.iter().any(|d| d.eq_ignore_ascii_case(&dir)) {
            scan_dirs.push(dir);
        }
    }

    for dir in &scan_dirs {
        info.steamapps_dirs_scanned.push(dir.clone());
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("appmanifest_") && name.ends_with(".acf") {
                    info.appmanifest_files_found.push(name.clone());

                    // 尝试解析
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        let mut parser = VdfParser::new(&content);
                        match parser.parse() {
                            Ok(root) => {
                                if root.get_obj("AppState").is_some() {
                                    info.games_count += 1;
                                } else {
                                    info.games_parse_errors
                                        .push(format!("{}: AppState not found", name));
                                }
                            }
                            Err(e) => {
                                info.games_parse_errors.push(format!("{}: {}", name, e));
                            }
                        }
                    }
                }
            }
        }
    }

    info
}

/// 启动 Steam 客户端
#[tauri::command]
pub async fn launch_steam_client() -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let steam_path = get_steam_path().ok_or("未找到 Steam 安装路径")?;
        let steam_exe = format!("{}\\steam.exe", steam_path);
        std::process::Command::new(&steam_exe)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("启动 Steam 失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 启动 Steam 游戏
#[tauri::command]
pub async fn launch_steam_game(app_id: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let url = format!("steam://run/{}", app_id);
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("启动游戏失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = app_id;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 在 Steam 商店中打开游戏页面
#[tauri::command]
pub async fn open_steam_store_page(app_id: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let url = format!("https://store.steampowered.com/app/{}", app_id);
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("打开商店页面失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = app_id;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 打开游戏安装目录
#[tauri::command]
pub async fn open_game_folder(library_path: String, install_dir: String) -> Result<(), String> {
    let full_path = format!("{}\\steamapps\\common\\{}", library_path, install_dir);
    if !Path::new(&full_path).exists() {
        return Err(format!("路径不存在: {}", full_path));
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        std::process::Command::new("explorer")
            .arg(&full_path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = (library_path, install_dir);
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 手动重建 loginusers.vdf，确保指定账户的 MostRecent=1、RememberPassword=1（其余账户两者为 0）
fn rebuild_loginusers_vdf(vdf_path: &str, target_account: &str, users: &[SteamUser]) -> Result<(), String> {
    let content = fs::read_to_string(vdf_path)
        .map_err(|e| format!("读取 loginusers.vdf 失败: {}", e))?;

    let mut result = String::new();
    let mut in_users_block = false;
    let mut current_steam_id = String::new();
    let mut depth = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("\"users\"") {
            in_users_block = true;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_users_block && trimmed == "{" {
            depth += 1;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_users_block && trimmed == "}" {
            depth -= 1;
            if depth == 0 {
                in_users_block = false;
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // 检查是否是 steam64 ID 行
        if in_users_block && depth == 1 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            // 提取 steam id（可能带引号）
            current_steam_id = trimmed.trim_matches('"').to_string();
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // 在用户块内，检查 MostRecent / RememberPassword 行
        let is_target_user = users.iter().any(|u|
            u.steam_id64 == current_steam_id &&
            u.account_name.eq_ignore_ascii_case(target_account)
        );
        if in_users_block && depth > 1 && trimmed.starts_with("\"MostRecent\"") {
            let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            result.push_str(&format!("{}\"MostRecent\"\t\t\"{}\"\n", indent, if is_target_user { "1" } else { "0" }));
            continue;
        }
        if in_users_block && depth > 1 && trimmed.starts_with("\"RememberPassword\"") {
            let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            result.push_str(&format!("{}\"RememberPassword\"\t\t\"{}\"\n", indent, if is_target_user { "1" } else { "0" }));
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    fs::write(vdf_path, &result)
        .map_err(|e| format!("写入 loginusers.vdf 失败: {}", e))?;

    log::info!("[Steam] Rebuilt loginusers.vdf, MostRecent set for: {}", target_account);
    Ok(())
}

/// 等待 Steam 进程完全退出（最多约 10 秒）
fn wait_steam_exit() {
    for _ in 0..50 {
        if !is_steam_running() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// 切换 Steam 账户并自动登录
#[tauri::command]
pub async fn switch_steam_account(account_name: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let steam_path = get_steam_path().ok_or("未找到 Steam 路径")?;
        let steam_exe = format!("{}\\steam.exe", steam_path);

        // 1. 先关闭 Steam，并等待其完全退出（否则 Steam 退出时会重写覆盖 loginusers.vdf 的修改）
        if is_steam_running() {
            let _ = std::process::Command::new("taskkill")
                .args(["/IM", "steam.exe", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            wait_steam_exit();
        }

        // 2. 修改 loginusers.vdf：目标账户 MostRecent=1、RememberPassword=1，其他账户为 0
        if !account_name.is_empty() {
            let vdf_path = format!("{}\\config\\loginusers.vdf", steam_path);
            let users = parse_login_users(&steam_path, false);
            // 如果目标账户还不是 most_recent / 未记住密码，重建 vdf
            let needs_rebuild = !users.iter().any(|u|
                u.account_name.eq_ignore_ascii_case(&account_name) && u.most_recent && u.remember_password
            );
            if needs_rebuild {
                let _ = rebuild_loginusers_vdf(&vdf_path, &account_name, &users);
            }
        }

        // 3. 写入注册表：AutoLoginUser 指向目标账户，RememberPassword=1（自动登录前提）
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let steam_key = hkcu
            .open_subkey_with_flags("Software\\Valve\\Steam", KEY_SET_VALUE)
            .map_err(|e| format!("打开注册表失败: {}", e))?;

        if account_name.is_empty() {
            let _ = steam_key.delete_value("AutoLoginUser");
        } else {
            steam_key
                .set_value("AutoLoginUser", &account_name)
                .map_err(|e| format!("写入 AutoLoginUser 失败: {}", e))?;
        }

        steam_key
            .set_value("RememberPassword", &1u32)
            .map_err(|e| format!("写入 RememberPassword 失败: {}", e))?;

        // 4. 启动 Steam，附加 -login <账户名> 参数实现自动登录
        let mut cmd = std::process::Command::new(&steam_exe);
        if !account_name.is_empty() {
            cmd.arg("-login").arg(&account_name);
        }
        cmd.creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("启动 Steam 失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = account_name;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 从 loginusers.vdf 中删除指定账户的用户块（按行重建，保留其他内容）
fn remove_user_from_vdf(vdf_path: &str, steam_id64: &str) -> Result<(), String> {
    let content = fs::read_to_string(vdf_path)
        .map_err(|e| format!("读取 loginusers.vdf 失败: {}", e))?;

    let mut result = String::new();
    let mut in_users_block = false;
    let mut depth = 0usize;
    let mut skip_block = false;
    let mut removed = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("\"users\"") {
            in_users_block = true;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_users_block && trimmed == "{" {
            depth += 1;
            if !skip_block {
                result.push_str(line);
                result.push('\n');
            }
            continue;
        }

        if in_users_block && trimmed == "}" {
            if skip_block {
                depth -= 1;
                if depth == 1 {
                    // 用户块结束，停止跳过
                    skip_block = false;
                    removed = true;
                }
                continue;
            }
            depth -= 1;
            if depth == 0 {
                in_users_block = false;
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // depth == 1 时是 steam64 ID 行，判断是否为要删除的账户
        if in_users_block && depth == 1 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            let id = trimmed.trim_matches('"').to_string();
            if id.eq_ignore_ascii_case(steam_id64) {
                skip_block = true;
                continue; // 跳过整个用户块
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if !skip_block {
            result.push_str(line);
            result.push('\n');
        }
    }

    if !removed {
        return Err(format!("未在 loginusers.vdf 中找到账户: {}", steam_id64));
    }

    fs::write(vdf_path, &result)
        .map_err(|e| format!("写入 loginusers.vdf 失败: {}", e))?;

    log::info!("[Steam] Removed user {} from loginusers.vdf", steam_id64);
    Ok(())
}

/// 删除本机记住的 Steam 账户（从 loginusers.vdf 移除，并清理注册表与头像缓存）
#[tauri::command]
pub async fn delete_steam_account(steam_id64: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        let steam_dir = get_steam_path().ok_or("未找到 Steam 安装路径")?;
        let vdf_path = format!("{}\\config\\loginusers.vdf", steam_dir);

        // 提前记录该账户的 AccountName，用于清理注册表 AutoLoginUser
        let target_account = parse_login_users(&steam_dir, is_steam_running())
            .into_iter()
            .find(|u| u.steam_id64 == steam_id64)
            .map(|u| u.account_name)
            .unwrap_or_default();

        // 1. 若 Steam 正在运行，先关闭（否则 Steam 退出时会重写覆盖 vdf）
        if is_steam_running() {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let _ = std::process::Command::new("taskkill")
                .args(["/IM", "steam.exe", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
            std::thread::sleep(std::time::Duration::from_secs(2));
        }

        // 2. 从 loginusers.vdf 中删除该账户
        remove_user_from_vdf(&vdf_path, &steam_id64)?;

        // 3. 若注册表 AutoLoginUser 指向该账户，一并清除
        if !target_account.is_empty() {
            if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags("Software\\Valve\\Steam", KEY_SET_VALUE)
            {
                if let Ok(current) = hkcu.get_value::<String, _>("AutoLoginUser") {
                    if current.eq_ignore_ascii_case(&target_account) {
                        let _ = hkcu.delete_value("AutoLoginUser");
                        log::info!("[Steam] Cleared AutoLoginUser registry value");
                    }
                }
            }
        }

        // 4. 清理本地头像缓存
        let avatar_path = format!("{}\\config\\avatarcache\\{}.png", steam_dir, steam_id64);
        if Path::new(&avatar_path).exists() {
            let _ = fs::remove_file(&avatar_path);
        }

        log::info!("[Steam] Deleted local account: {}", steam_id64);
    }
    #[cfg(not(windows))]
    {
        let _ = steam_id64;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 卸载 Steam 游戏（通过 Steam 协议）
#[tauri::command]
pub async fn uninstall_steam_game(app_id: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let url = format!("steam://uninstall/{}", app_id);
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("卸载请求失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = app_id;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 安装 Steam 游戏（通过 Steam 协议，未安装的库存游戏点击「安装」时调用）
#[tauri::command]
pub async fn install_steam_game(app_id: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let url = format!("steam://install/{}", app_id);
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("安装请求失败: {}", e))?;
    }
    #[cfg(not(windows))]
    {
        let _ = app_id;
        return Err("Not supported on this platform".into());
    }
    Ok(())
}

/// 格式化文件大小（供前端调用）
#[tauri::command]
pub async fn format_file_size(bytes: u64) -> String {
    format_size(bytes)
}

/// 获取游戏库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamStats {
    pub total_games: usize,
    pub total_size: u64,
    pub library_count: usize,
    pub user_count: usize,
}

#[tauri::command]
pub async fn get_steam_stats() -> SteamStats {
    let data = get_steam_all_data().await;
    let total_size = data.games.iter().map(|g| g.size_on_disk).sum();
    SteamStats {
        total_games: data.games.len(),
        total_size,
        library_count: data.libraries.len(),
        user_count: data.users.len(),
    }
}



/// 获取路径所在磁盘的实际容量和可用空间
#[cfg(windows)]
fn get_disk_space(path: &str) -> Option<(u64, u64)> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_bytes_available: *mut u64,
            total_bytes: *mut u64,
            total_free_bytes: *mut u64,
        ) -> i32;
    }

    // 确保路径以驱动器根目录结尾，如 "C:\"
    let drive_root = if path.len() >= 2 && path.as_bytes()[1] == b':' {
        format!("{}\\", &path[..2])
    } else {
        format!("{}\\", path)
    };

    let wide: Vec<u16> = OsStr::new(&drive_root)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut total_free: u64 = 0;

    let ret = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes,
            &mut total_bytes,
            &mut total_free,
        )
    };

    if ret != 0 {
        Some((total_bytes, total_free))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn get_disk_space(_path: &str) -> Option<(u64, u64)> {
    None
}

/// 获取游戏库磁盘使用情况（含实际磁盘容量）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDiskInfo {
    pub library_path: String,
    pub total_size: u64,
    pub free_size: u64,
    pub game_count: usize,
    pub game_size: u64,
}

#[tauri::command]
pub async fn get_library_disk_info() -> Vec<LibraryDiskInfo> {
    let steam_dir = match get_steam_path() {
        Some(p) => p,
        None => return vec![],
    };
    let libraries = parse_library_folders(&steam_dir);
    let games = scan_installed_games(&libraries);

    let mut result = vec![];
    for lib in &libraries {
        let lib_games: Vec<&SteamGame> = games
            .iter()
            .filter(|g| g.library_path.eq_ignore_ascii_case(&lib.path))
            .collect();
        let game_size: u64 = lib_games.iter().map(|g| g.size_on_disk).sum();

        // 获取实际磁盘容量
        let (total_size, free_size) = get_disk_space(&lib.path).unwrap_or((lib.total_size, 0));

        result.push(LibraryDiskInfo {
            library_path: lib.path.clone(),
            total_size,
            free_size,
            game_count: lib_games.len(),
            game_size,
        });
    }
    result
}

// ======================== 库存游戏（Steam GameList） ========================
//
// 参考 SteamTools 库存游戏插件的实现思路：
// - Steam 运行时：以当前账号（注册表 ActiveUser / loginusers.vdf 最近登录）为准，
//   优先在线获取该账号的完整库存（steamcommunity 游戏列表页，无需 API Key），本地缓存兜底。
// - Steam 未运行：加载 Steam 本地缓存数据（.acf 已安装清单 + localconfig 游玩记录 + appinfo 目录）。

/// 库存游戏条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamInventoryGame {
    pub app_id: u32,
    pub name: String,
    /// 是否已安装到本地
    pub installed: bool,
    /// 游玩时长（分钟）
    pub playtime_minutes: u64,
    /// 最近游玩时间（unix 秒）
    pub last_played: i64,
    pub size_on_disk: u64,
    pub install_dir: String,
    pub library_path: String,
}

/// 库存游戏统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamInventoryStats {
    pub total: usize,
    pub installed: usize,
    pub not_installed: usize,
    pub total_playtime_minutes: u64,
}

/// 当前账号摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamInventoryUser {
    pub steam_id64: String,
    pub account_name: String,
    pub persona_name: String,
}

/// 库存游戏数据（get_steam_inventory 返回值）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteamInventoryData {
    /// 数据来源："online"（在线库存）| "cache"（本地缓存）| "none"（无 Steam）
    pub source: String,
    /// Steam 客户端是否正在运行
    pub steam_running: bool,
    pub current_user: Option<SteamInventoryUser>,
    pub stats: SteamInventoryStats,
    pub games: Vec<SteamInventoryGame>,
    /// 在线获取失败时的提示（仅在线失败且回退本地缓存时非空）
    pub error: Option<String>,
}

// ---------- 二进制 appinfo.vdf 解析（v40 / v41） ----------

/// 二进制 VDF 值（仅保留库存需要的字段类型）
#[derive(Debug, Clone)]
enum BvdfValue {
    Str(String),
    Int(i64),
    U64(u64),
    Obj(Vec<(String, BvdfValue)>),
}

/// appinfo.vdf 读取器：支持 v41（key 为字符串表索引）与 v40（key 为内联字符串）
struct BvdfReader<'a> {
    data: &'a [u8],
    pos: usize,
    strtab: Option<&'a [String]>,
}

impl<'a> BvdfReader<'a> {
    fn new(data: &'a [u8], strtab: Option<&'a [String]>) -> Self {
        Self { data, pos: 0, strtab }
    }

    fn read_u32(&mut self) -> u32 {
        if self.pos + 4 > self.data.len() {
            return 0;
        }
        let v = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        v
    }

    fn read_u64(&mut self) -> u64 {
        if self.pos + 8 > self.data.len() {
            return 0;
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[self.pos..self.pos + 8]);
        self.pos += 8;
        u64::from_le_bytes(b)
    }

    /// 读取 C 风格以 \0 结尾的字符串
    fn read_cstr(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        let s = String::from_utf8_lossy(&self.data[start..self.pos]).to_string();
        if self.pos < self.data.len() {
            self.pos += 1;
        }
        s
    }

    /// 读取宽字符串（\0 结尾的 UTF-16LE）
    fn read_wstr(&mut self) -> String {
        let mut chars: Vec<u16> = Vec::new();
        while self.pos + 1 < self.data.len() {
            let c = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
            self.pos += 2;
            if c == 0 {
                break;
            }
            chars.push(c);
        }
        String::from_utf16_lossy(&chars)
    }

    /// 读取一个 key：v41 为字符串表索引（u32），v40 为内联字符串
    fn read_key(&mut self) -> String {
        if self.strtab.is_some() {
            let idx = self.read_u32() as usize;
            self.strtab
                .and_then(|st| st.get(idx))
                .cloned()
                .unwrap_or_else(|| format!("<key@{idx}>"))
        } else {
            self.read_cstr()
        }
    }

    /// 解析一段二进制 VDF（读到 type 8 结束）
    fn parse(&mut self) -> BvdfValue {
        let mut entries: Vec<(String, BvdfValue)> = Vec::new();
        let mut guard = 0usize;
        while self.pos < self.data.len() {
            guard += 1;
            if guard > 1_000_000 {
                break;
            }
            let t = self.data[self.pos];
            self.pos += 1;
            if t == 8 {
                break;
            }
            let key = self.read_key();
            match t {
                0 | 4 => {} // null / pointer：无数据
                1 => entries.push((key, BvdfValue::Str(self.read_cstr()))),
                2 => entries.push((key, BvdfValue::Int(self.read_u32() as i32 as i64))),
                3 => {
                    self.pos += 4; // float
                }
                5 => entries.push((key, BvdfValue::Str(self.read_wstr()))),
                6 => {
                    self.pos += 4; // color
                }
                7 | 9 => entries.push((key, BvdfValue::U64(self.read_u64()))),
                _ => return BvdfValue::Obj(entries), // 未知类型，中止避免错位
            }
        }
        BvdfValue::Obj(entries)
    }
}

fn bvdf_find<'a>(v: &'a BvdfValue, key: &str) -> Option<&'a BvdfValue> {
    if let BvdfValue::Obj(entries) = v {
        for (k, val) in entries {
            if k == key {
                return Some(val);
            }
        }
    }
    None
}

fn bvdf_str(v: &BvdfValue) -> Option<String> {
    match v {
        BvdfValue::Str(s) => Some(s.clone()),
        BvdfValue::Int(i) => Some(i.to_string()),
        BvdfValue::U64(u) => Some(u.to_string()),
        _ => None,
    }
}

/// 解析 appinfo.vdf，返回 所有游戏的 (名称, 类型) 映射。
/// 仅支持 v40(0x07564428) 与 v41(0x07564429)；旧版本返回空映射（容忍）。
fn parse_appinfo_file(path: &str) -> std::collections::HashMap<u32, (String, String)> {
    let mut result = std::collections::HashMap::new();
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return result,
    };
    if data.len() < 16 {
        return result;
    }
    let mut r = BvdfReader::new(&data, None);
    let magic = r.read_u32();
    let _universe = r.read_u32();

    let mut st: Vec<String> = Vec::new();
    if magic == 0x0756_4429 {
        // v41：头部还有字符串表偏移；条目 key 用字符串表索引
        let st_off = r.read_u64() as usize;
        if st_off + 4 <= data.len() {
            let mut sr = BvdfReader::new(&data[st_off..], None);
            let count = sr.read_u32() as usize;
            st.reserve(count.min(1_000_000));
            for _ in 0..count {
                if sr.pos >= sr.data.len() {
                    break;
                }
                st.push(sr.read_cstr());
            }
        }
    } else if magic != 0x0756_4428 {
        // 仅支持 v40/v41
        return result;
    }
    let strtab: Option<&[String]> = if magic == 0x0756_4429 { Some(&st) } else { None };

    loop {
        // 条目结构：appid(u32), size(u32), infostate(u32), last_updated(u32),
        // pics_token(u64), sha1(20), changenumber(u32), binary_sha1(20), binary_vdf(size-60)
        if r.pos + 68 > data.len() {
            break;
        }
        let appid = r.read_u32();
        if appid == 0 {
            break; // 文件 footer
        }
        let size = r.read_u32() as usize;
        let entry_start = r.pos;
        r.read_u32(); // infostate
        r.read_u32(); // last_updated
        r.read_u64(); // pics token
        r.pos += 20; // sha1
        r.read_u32(); // changenumber
        r.pos += 20; // binary sha1

        // 边界检查：binary vdf 长度为 size - 60（去掉后续 60 字节头字段）
        let bvdflen = size.saturating_sub(60);
        let vdf_end = r.pos + bvdflen;
        if vdf_end > data.len() {
            break;
        }
        let slice = &data[r.pos..vdf_end];
        let mut br = BvdfReader::new(slice, strtab);
        let v = br.parse();
        extract_appinfo_fields(appid, &v, &mut result);
        // 结构偏移推进到下一个条目
        r.pos = entry_start + size;
    }
    result
}

fn extract_appinfo_fields(
    appid: u32,
    v: &BvdfValue,
    result: &mut std::collections::HashMap<u32, (String, String)>,
) {
    // 名称/类型可能在根级（2026 客户端扁平布局），也可能在 common 子节点（旧布局）
    let name = bvdf_find(v, "name")
        .and_then(bvdf_str)
        .or_else(|| bvdf_find(v, "common").and_then(|c| bvdf_find(c, "name")).and_then(bvdf_str))
        .unwrap_or_default();
    let typ = bvdf_find(v, "type")
        .and_then(bvdf_str)
        .or_else(|| bvdf_find(v, "common").and_then(|c| bvdf_find(c, "type")).and_then(bvdf_str))
        .unwrap_or_default();
    if !name.is_empty() || !typ.is_empty() {
        result.insert(appid, (name, typ));
    }
}

/// appinfo 解析结果缓存（按文件修改时间失效）
static APPINFO_CACHE: std::sync::OnceLock<
    std::sync::Mutex<Option<(std::time::SystemTime, std::collections::HashMap<u32, (String, String)>)>>,
> = std::sync::OnceLock::new();

/// 获取 appinfo 应用目录（带进程内缓存）
fn load_appinfo_names(steam_dir: &str) -> std::collections::HashMap<u32, (String, String)> {
    let path = format!("{steam_dir}\\appcache\\appinfo.vdf");
    let mtime = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok();
    let mut lock = APPINFO_CACHE
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap();
    if let Some((t, map)) = &*lock {
        if Some(*t) == mtime {
            return map.clone();
        }
    }
    let map = parse_appinfo_file(&path);
    let t = mtime.unwrap_or_else(|| std::time::SystemTime::now());
    *lock = Some((t, map.clone()));
    map
}

// ---------- 库存合并与主命令 ----------

/// 获取当前账号库存（纯本地缓存：.acf 已安装清单 + localconfig 游玩记录 + appinfo 目录）
#[tauri::command]
pub async fn get_steam_inventory() -> SteamInventoryData {
    let install_path = get_steam_path();

    let none_data = SteamInventoryData {
        source: "none".to_string(),
        steam_running: false,
        current_user: None,
        stats: SteamInventoryStats {
            total: 0,
            installed: 0,
            not_installed: 0,
            total_playtime_minutes: 0,
        },
        games: vec![],
        error: None,
    };

    let steam_dir = match install_path {
        Some(p) => p,
        None => return none_data,
    };
    let steam_running = is_steam_running();

    // 当前账号：loginusers.vdf 最近登录（运行中会用注册表 ActiveUser 校正）
    let current_user = parse_login_users(&steam_dir, steam_running)
        .into_iter()
        .next()
        .map(|u| SteamInventoryUser {
            steam_id64: u.steam_id64,
            account_name: u.account_name,
            persona_name: u.persona_name,
        });

    // 本地数据：已安装清单 + 游玩记录
    let libraries = parse_library_folders(&steam_dir);
    let installed_games = scan_installed_games(&libraries);
    let local_playtimes = parse_local_config_playtimes(&steam_dir);

    // 合并容器（app_id -> 条目）
    use std::collections::HashMap;
    let mut merged: HashMap<u32, SteamInventoryGame> = HashMap::new();

    // 1) 本地已安装：.acf 清单直接作为库存基础
    for g in &installed_games {
        let entry = merged.entry(g.app_id).or_insert(SteamInventoryGame {
            app_id: g.app_id,
            name: g.name.clone(),
            installed: true,
            playtime_minutes: g.playtime_minutes,
            last_played: g.last_played,
            size_on_disk: g.size_on_disk,
            install_dir: g.install_dir.clone(),
            library_path: g.library_path.clone(),
        });
        entry.installed = true;
        if entry.name.is_empty() {
            entry.name = g.name.clone();
        }
        entry.playtime_minutes = entry.playtime_minutes.max(g.playtime_minutes);
        if g.last_played > entry.last_played {
            entry.last_played = g.last_played;
        }
        entry.size_on_disk = g.size_on_disk;
        if entry.install_dir.is_empty() {
            entry.install_dir = g.install_dir.clone();
        }
        if entry.library_path.is_empty() {
            entry.library_path = g.library_path.clone();
        }
    }

    // 3) 本地游玩记录（localconfig）：补充游玩时长/最近游玩，未出现在列表中的也加入
    for (appid, (playtime, last_played)) in &local_playtimes {
        let entry = merged.entry(*appid).or_insert(SteamInventoryGame {
            app_id: *appid,
            name: String::new(),
            installed: false,
            playtime_minutes: 0,
            last_played: 0,
            size_on_disk: 0,
            install_dir: String::new(),
            library_path: String::new(),
        });
        entry.playtime_minutes = entry.playtime_minutes.max(*playtime);
        if *last_played > entry.last_played {
            entry.last_played = *last_played;
        }
    }

    // 3) 名称为空的用 appinfo 目录补充，仍无则退化为 "App {id}"
    let appinfo = load_appinfo_names(&steam_dir);
    for (_id, g) in merged.iter_mut() {
        if g.name.is_empty() {
            if let Some((n, _)) = appinfo.get(&g.app_id) {
                g.name = n.clone();
            } else {
                g.name = format!("App {}", g.app_id);
            }
        }
    }

    // 统计与排序（最近游玩优先）
    let total = merged.len();
    let (installed, not_installed, total_playtime) = merged.values().fold(
        (0usize, 0usize, 0u64),
        |(i, ni, pt), g| {
            (
                i + if g.installed { 1 } else { 0 },
                ni + if g.installed { 0 } else { 1 },
                pt + g.playtime_minutes,
            )
        },
    );

    // 排序：已安装优先，其次最近游玩，再次游玩时长，最后按名称
    let mut games: Vec<SteamInventoryGame> = merged.into_values().collect();
    games.sort_by(|a, b| {
        b.installed
            .cmp(&a.installed)
            .then(b.last_played.cmp(&a.last_played))
            .then(b.playtime_minutes.cmp(&a.playtime_minutes))
            .then(a.name.cmp(&b.name))
    });

    SteamInventoryData {
        source: "cache".to_string(),
        steam_running,
        current_user,
        stats: SteamInventoryStats {
            total,
            installed,
            not_installed,
            total_playtime_minutes: total_playtime,
        },
        games,
        error: None,
    }
}
