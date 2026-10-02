// 三角洲行动 PSO 缓存清理：定位并校验游戏 Saved\PSOCache 目录，默认移入回收站（可选永久删除）。
//
// 安全边界（对齐参考实现）：
// - 只接受目录名 PSOCache、直接父目录 Saved，且完整路径包含 DeltaForce / 2001918 / 三角洲 的目标；
// - 检测到游戏或启动器进程运行时拒绝清理；
// - 始终保留 PSOCache 根目录本身与顶层 .ini（含 UserSystemSettingHD.ini），只逐项处理其余内容；
// - 回收站用 shell32!SHFileOperationW（FO_DELETE + FOF_ALLOWUNDO），永久删除走 fs + 重启调度兜底；
// - 路径记录保存在 %LOCALAPPDATA%\NexBox\pso_cache_path.txt。
// 目标位于用户可访问目录，无需管理员权限。
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsoDetectResult {
    pub path: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsoScanResult {
    pub exists: bool,
    pub path: String,
    pub item_total: u64,
    pub item_clean: u64,
    pub ini_kept: u64,
    pub size_bytes: u64,
    pub game_running: bool,
    pub running_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsoCleanResult {
    pub success: bool,
    pub message: String,
    pub freed_bytes: u64,
    pub removed_items: u64,
    pub kept_inis: u64,
    pub reboot_pending: u64,
}

/// 保留扩展名（顶层 .ini 配置一律不动）。
const KEEP_EXT: &str = ".ini";
/// 游戏本体进程候选（小写、去 .exe）。
const GAME_PROCS: &[&str] = &[
    "deltaforceclient-win64-shipping",
    "deltaforce",
    "dfclient",
    "dfclient-win64-shipping",
];
/// 启动器进程候选。
const LAUNCHER_PROCS: &[&str] = &["wegame", "wemezone", "tensafe", "tensafewatchdog"];

// ─── 路径记录 ───

fn storage_file() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("NexBox").join("pso_cache_path.txt"))
}

fn load_saved_path() -> Option<String> {
    let f = storage_file()?;
    fs::read_to_string(&f)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn save_path(p: &str) {
    let Some(f) = storage_file() else { return };
    if let Some(parent) = f.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(e) = fs::write(&f, p) {
        log::warn!("保存 PSO 路径记录失败: {}", e);
    }
}

// ─── 校验与定位 ───

/// 三重安全校验：目录名 PSOCache + 父目录 Saved + 路径含 DeltaForce/2001918/三角洲。
///
/// 关键词比对前会先去掉空白与 `_` `-` 分隔符：Steam 上的安装目录常写作 `Delta Force`
/// （带空格），直接 `contains("deltaforce")` 会漏判。
pub fn validate_pso_cache(dir: &str) -> bool {
    if dir.trim().is_empty() {
        return false;
    }
    let p = Path::new(dir);
    let base = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let parent = p
        .parent()
        .and_then(|s| s.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if base != "psocache" || parent != "saved" {
        return false;
    }
    let full = p.to_string_lossy().to_ascii_lowercase();
    let compact: String = full
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '-')
        .collect();
    compact.contains("deltaforce") || full.contains("三角洲") || compact.contains("2001918")
}

fn list_top(dir: &Path) -> Vec<fs::DirEntry> {
    match fs::read_dir(dir) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// 在给定根目录下限定深度/规模地查找合法 PSOCache。
fn find_pso_cache_under(root: &Path, max_depth: usize, max_nodes: usize) -> Option<PathBuf> {
    if !root.is_dir() {
        return None;
    }
    if validate_pso_cache(&root.to_string_lossy()) {
        return Some(root.to_path_buf());
    }
    let skip: [&str; 5] = [
        "node_modules",
        ".git",
        "windowsapps",
        "$recycle.bin",
        "system volume information",
    ];
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    queue.push_back((root.to_path_buf(), 0));
    let mut visited = 0usize;
    while let Some((cur, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let Ok(rd) = fs::read_dir(&cur) else { continue };
        for e in rd.flatten() {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if !is_dir {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if skip.contains(&name.as_str()) {
                continue;
            }
            let full = e.path();
            visited += 1;
            if visited > max_nodes {
                return None;
            }
            if validate_pso_cache(&full.to_string_lossy()) {
                return Some(full);
            }
            queue.push_back((full, depth + 1));
        }
    }
    None
}

fn logical_drives() -> Vec<char> {
    let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1u32 << i) != 0 {
            out.push((b'A' + i as u8) as char);
        }
    }
    if out.is_empty() {
        out.extend(['C', 'D', 'E', 'F', 'G']);
    }
    out
}

/// 一个候选搜索根 + 其搜索预算（深度 / 节点数）。
struct RootCand {
    path: PathBuf,
    depth: usize,
    nodes: usize,
}

/// 解析 Steam 的 libraryfolders.vdf，取出所有库根目录（含自定义库盘）。
fn steam_library_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for d in logical_drives() {
        let base = PathBuf::from(format!("{}:\\", d));
        for rel in [
            r"Steam\steamapps\libraryfolders.vdf",
            r"SteamLibrary\steamapps\libraryfolders.vdf",
            r"Program Files (x86)\Steam\steamapps\libraryfolders.vdf",
            r"Program Files\Steam\steamapps\libraryfolders.vdf",
        ] {
            let Ok(text) = fs::read_to_string(base.join(rel)) else {
                continue;
            };
            for line in text.lines() {
                let l = line.trim();
                if !l.starts_with("\"path\"") {
                    continue;
                }
                // 形如：  "path"		"D:\\SteamLibrary"
                if let Some(v) = l.split('"').nth(3) {
                    let p = PathBuf::from(v.replace("\\\\", "\\"));
                    let key = p.to_string_lossy().to_ascii_lowercase();
                    if p.is_dir() && seen.insert(key) {
                        out.push(p);
                    }
                }
            }
        }
    }
    out
}

fn push_cand(
    out: &mut Vec<RootCand>,
    seen: &mut HashSet<String>,
    p: PathBuf,
    depth: usize,
    nodes: usize,
) {
    let key = p.to_string_lossy().to_ascii_lowercase();
    if p.is_dir() && seen.insert(key) {
        out.push(RootCand { path: p, depth, nodes });
    }
}

/// 常见安装根（存在目录才纳入），带各自搜索预算。
///
/// 注意：容器型根（`steamapps\common`、`WeGameApps`、`Games` 等）下面往往堆了几十款游戏，
/// 预算给小了会在到达目标前就耗尽节点数 → 表现为「自动检测不到」。这里统一放宽到 8 层 / 30000 节点。
fn candidate_roots() -> Vec<RootCand> {
    let mut out: Vec<RootCand> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1) Steam：常见库 + vdf 里声明的自定义库
    let mut steam_commons: Vec<PathBuf> = Vec::new();
    for d in logical_drives() {
        let base = PathBuf::from(format!("{}:\\", d));
        for rel in [
            r"SteamLibrary\steamapps\common",
            r"Steam\steamapps\common",
            r"Program Files (x86)\Steam\steamapps\common",
            r"Program Files\Steam\steamapps\common",
        ] {
            steam_commons.push(base.join(rel));
        }
    }
    for lib in steam_library_roots() {
        steam_commons.push(lib.join("steamapps").join("common"));
    }
    for c in steam_commons {
        push_cand(&mut out, &mut seen, c, 6, 8000);
    }

    // 2) WeGame / 其它常见安装位置
    for d in logical_drives() {
        let base = PathBuf::from(format!("{}:\\", d));
        for rel in [
            "WeGameApps",
            r"WeGameApps\rail_apps",
            "DeltaForce",
            "Delta Force",
            "三角洲行动",
            "Games",
            "Game",
            "游戏",
            r"Program Files\WeGameApps",
        ] {
            push_cand(&mut out, &mut seen, base.join(rel), 8, 30000);
        }
    }

    out
}

/// 自动检测：先读 path.txt 记录，再按候选根逐个扫描。
fn auto_detect() -> PsoDetectResult {
    if let Some(saved) = load_saved_path() {
        if validate_pso_cache(&saved) && Path::new(&saved).is_dir() {
            return PsoDetectResult {
                path: Some(saved),
                source: Some("saved".to_string()),
            };
        }
    }
    for cand in candidate_roots() {
        if let Some(found) = find_pso_cache_under(&cand.path, cand.depth, cand.nodes) {
            let s = found.to_string_lossy().into_owned();
            save_path(&s);
            return PsoDetectResult {
                path: Some(s),
                source: Some("scan".to_string()),
            };
        }
    }
    PsoDetectResult {
        path: None,
        source: None,
    }
}

/// 用户手动选择目录后，将其解析为合法 PSOCache（自身或子树内查找）；命中则记录路径。
fn resolve_chosen(picked: &str) -> Option<String> {
    if picked.trim().is_empty() {
        return None;
    }
    if validate_pso_cache(picked) {
        save_path(picked);
        return Some(picked.to_string());
    }
    if let Some(found) = find_pso_cache_under(Path::new(picked), 8, 30000) {
        let s = found.to_string_lossy().into_owned();
        save_path(&s);
        return Some(s);
    }
    None
}

// ─── 进程检测 ───

/// 命中的游戏/启动器显示名（空数组代表未运行）。
fn detect_running() -> Vec<String> {
    let mut sys = System::new();
    sys.refresh_processes();
    let mut set: HashSet<String> = HashSet::new();
    for (_, p) in sys.processes() {
        let name = p.name().to_string().to_ascii_lowercase();
        set.insert(name.trim_end_matches(".exe").to_string());
    }
    let mut out = Vec::new();
    if GAME_PROCS.iter().any(|g| set.contains(*g)) {
        out.push("三角洲行动".to_string());
    }
    if LAUNCHER_PROCS.iter().any(|g| set.contains(*g)) {
        out.push("WeGame 启动器".to_string());
    }
    out
}

// ─── 统计 ───

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = fs::metadata(&p) {
                total += m.len();
            }
        }
    }
    total
}

fn measure(paths: &[PathBuf]) -> u64 {
    let mut total = 0u64;
    for p in paths {
        if let Ok(m) = fs::metadata(p) {
            total += if m.is_dir() { dir_size(p) } else { m.len() };
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

// ─── 删除 ───

fn to_wide(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// 将文件/目录标记为重启后删除（MoveFileEx + MOVEFILE_DELAY_UNTIL_REBOOT）。
fn schedule_reboot_delete(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
    let wide = to_wide(path);
    unsafe { MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) != 0 }
}

/// 自底向上删除目录内容，被占用项调度重启后删除。
fn delete_tree(dir: &Path, reboot: &mut u64) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                delete_tree(&p, reboot);
                if fs::remove_dir(&p).is_err() && schedule_reboot_delete(&p) {
                    *reboot += 1;
                }
            } else if fs::remove_file(&p).is_err() && schedule_reboot_delete(&p) {
                *reboot += 1;
            }
        }
    }
}

/// 永久删除单个条目（文件或目录），返回被占用而调度重启删除的项数。
fn remove_permanent(p: &Path) -> u64 {
    if !p.is_dir() {
        return if fs::remove_file(p).is_ok() {
            0
        } else if schedule_reboot_delete(p) {
            1
        } else {
            0
        };
    }
    if fs::remove_dir_all(p).is_ok() && !p.exists() {
        return 0;
    }
    let mut reboot = 0u64;
    delete_tree(p, &mut reboot);
    if fs::remove_dir(p).is_err() && schedule_reboot_delete(p) {
        reboot += 1;
    }
    reboot
}

/// 把若干路径整体移入回收站；成功返回 true。
fn to_recycle_bin(paths: &[PathBuf]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, SHFILEOPSTRUCTW, FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
        FOF_NOERRORUI, FOF_SILENT,
    };

    if paths.is_empty() {
        return true;
    }

    // 各路径以 \0 分隔、结尾再补一个 \0 → 构成 API 要求的双 null 结尾列表。
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        from.extend(p.as_os_str().encode_wide());
        from.push(0);
    }
    from.push(0);

    let flags: u16 = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI) as u16;

    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: flags,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };

    let ret = unsafe { SHFileOperationW(&mut op) };
    ret == 0 && op.fAnyOperationsAborted == 0
}

// ─── Tauri 命令 ───

/// 自动检测 PSOCache 目录（先读记录，再扫描常见安装根）。
#[tauri::command]
pub async fn pso_detect() -> Result<PsoDetectResult, String> {
    Ok(auto_detect())
}

/// 手动选择目录：弹系统文件夹选择框，并解析为合法 PSOCache（返回 None 表示取消）。
#[tauri::command]
pub fn pso_choose() -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .set_title("选择《三角洲行动》安装目录或 Saved\\PSOCache 目录")
        .pick_folder()
        .map(|p| p.to_string_lossy().into_owned());
    let Some(picked) = picked else {
        return Ok(None);
    };
    Ok(resolve_chosen(&picked))
}

/// 扫描 PSO 缓存信息（不做任何改动）。
#[tauri::command]
pub async fn pso_scan(dir: String) -> Result<PsoScanResult, String> {
    let running = detect_running();
    let valid = validate_pso_cache(&dir) && Path::new(&dir).is_dir();
    if !valid {
        return Ok(PsoScanResult {
            exists: false,
            path: dir,
            item_total: 0,
            item_clean: 0,
            ini_kept: 0,
            size_bytes: 0,
            game_running: !running.is_empty(),
            running_names: running,
        });
    }

    let entries = list_top(Path::new(&dir));
    let mut ini_kept = 0u64;
    let mut clean_paths: Vec<PathBuf> = Vec::new();
    for e in &entries {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(KEEP_EXT) {
            ini_kept += 1;
        } else {
            clean_paths.push(e.path());
        }
    }
    let size_bytes = measure(&clean_paths);

    Ok(PsoScanResult {
        exists: true,
        path: dir,
        item_total: entries.len() as u64,
        item_clean: clean_paths.len() as u64,
        ini_kept,
        size_bytes,
        game_running: !running.is_empty(),
        running_names: running,
    })
}

/// 清理 PSO 缓存：默认移入回收站，permanent=true 时永久删除。
#[tauri::command]
pub async fn pso_clean(dir: String, permanent: bool) -> Result<PsoCleanResult, String> {
    if !validate_pso_cache(&dir) || !Path::new(&dir).is_dir() {
        return Err(
            "所选目录不是合法的三角洲 PSOCache（需 …\\DeltaForce\\…\\Saved\\PSOCache）".to_string(),
        );
    }

    let running = detect_running();
    if !running.is_empty() {
        return Err(format!(
            "检测到 {} 正在运行，请先完全退出游戏与启动器",
            running.join(" / ")
        ));
    }

    let entries = list_top(Path::new(&dir));
    let mut kept_inis = 0u64;
    let mut targets: Vec<PathBuf> = Vec::new();
    for e in &entries {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(KEEP_EXT) {
            kept_inis += 1;
        } else {
            targets.push(e.path());
        }
    }

    if targets.is_empty() {
        save_path(&dir);
        return Ok(PsoCleanResult {
            success: true,
            message: "PSOCache 内没有可清理的缓存项（仅保留 .ini 配置）".to_string(),
            freed_bytes: 0,
            removed_items: 0,
            kept_inis,
            reboot_pending: 0,
        });
    }

    let freed_bytes = measure(&targets);

    if permanent {
        let mut removed = 0u64;
        let mut reboot = 0u64;
        for t in &targets {
            removed += 1;
            reboot += remove_permanent(t);
        }
        save_path(&dir);
        let mut message = format!(
            "已永久删除 {} 项缓存，释放 {}",
            removed,
            format_size(freed_bytes)
        );
        if reboot > 0 {
            message.push_str(&format!("，{} 项被占用将在重启后删除", reboot));
        }
        log::info!("PSO 缓存清理: {}", message);
        return Ok(PsoCleanResult {
            success: true,
            message,
            freed_bytes,
            removed_items: removed,
            kept_inis,
            reboot_pending: reboot,
        });
    }

    if !to_recycle_bin(&targets) {
        return Err("移入回收站失败，请检查系统回收站设置或改用「永久删除」".to_string());
    }
    save_path(&dir);
    let message = format!(
        "已将 {} 项缓存移入回收站，释放 {}（可从回收站恢复）",
        targets.len(),
        format_size(freed_bytes)
    );
    log::info!("PSO 缓存清理: {}", message);
    Ok(PsoCleanResult {
        success: true,
        message,
        freed_bytes,
        removed_items: targets.len() as u64,
        kept_inis,
        reboot_pending: 0,
    })
}
