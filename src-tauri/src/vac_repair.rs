//! CS:GO / CS2 VAC 修复工具
//!
//! 参考 Steam++(Watt Toolkit) 游戏工具箱的「CS:GO VAC 修复」逻辑实现：
//!  - 检测 Steam 安装路径、steamservice.exe、Steam 运行状态与管理员权限
//!  - 一键修复：恢复系统服务(Netman/RasMan/TapiSrv/MpsSvc) → 开启防火墙 →
//!    结束 Steam → 执行 steamservice /install + /repair → 恢复 DEP 默认启动设置 →
//!    重启 Steam → 自启动「Steam Client Service」
//!  - 通过事件把批处理输出流式推送到前端（vac-repair-output / vac-repair-done）
//!
//! 执行方案（吸取了 Rust std::process::Command 直接拼接 cmd 命令时
//! 引号解析会被破坏的教训）：把「调用修复脚本 + 传参 + 输出重定向」全部烘焙进
//! 一个 RUNNER 批处理文件，再用 PowerShell Start-Process 提权执行它
//! （该模式与 pawnio_driver.rs 一致，是项目内已验证的提权执行方案），
//! 输出被重定向到日志文件后由本模块轮询读取并转发给前端，实现「边执行边显示」。

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::{Emitter, Manager};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 前端订阅的流式输出事件名（每行一条）
const EVENT_OUTPUT: &str = "vac-repair-output";
/// 修复流程结束事件（payload 为 bool，true 表示脚本顺利执行完）
const EVENT_DONE: &str = "vac-repair-done";

// ============================== 状态检测 ==============================

#[derive(serde::Serialize)]
pub struct VacRepairStatus {
    /// 是否检测到 Steam 安装
    pub steam_installed: bool,
    /// Steam 安装路径（如 C:\Program Files (x86)\Steam）
    pub steam_path: Option<String>,
    /// bin\steamservice.exe 是否存在（VAC 启动器服务，修复的核心对象）
    pub steam_bin_exists: bool,
    /// Steam 当前是否正在运行
    pub steam_running: bool,
    /// 当前进程是否以管理员身份运行
    pub is_admin: bool,
}

fn get_steam_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
            if let Ok(path) = hkcu.get_value::<String, _>("SteamPath") {
                return Some(PathBuf::from(path.replace('/', "\\")));
            }
        }
        for flag in &[KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags("SOFTWARE\\Valve\\Steam", KEY_READ | *flag)
            {
                if let Ok(path) = hklm.get_value::<String, _>("InstallPath") {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }
    None
}

fn is_steam_running() -> bool {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_processes();
    sys.processes()
        .values()
        .any(|p| p.name().eq_ignore_ascii_case("steam.exe"))
}

/// 检测当前 VAC 修复所需状态
#[tauri::command]
pub async fn get_vac_repair_status() -> Result<VacRepairStatus, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }
    let steam_path = get_steam_path();
    let steam_bin_exists = steam_path
        .as_ref()
        .map(|p| p.join("bin").join("steamservice.exe").exists())
        .unwrap_or(false);
    Ok(VacRepairStatus {
        steam_installed: steam_path.is_some(),
        steam_path: steam_path.map(|p| p.to_string_lossy().into_owned()),
        steam_bin_exists,
        steam_running: is_steam_running(),
        is_admin: crate::optimization::is_admin(),
    })
}

// ============================== 脚本生成 ==============================

/// 批处理内引号包裹路径：`%` 在批处理里即使包在引号内也会做变量展开，需转义成 %%。
/// （路径由 AppData / 注册表而来，正常不含 ^&<>| 等特殊字符，引号内它们也不生效）
fn batch_quote(p: &Path) -> String {
    format!("\"{}\"", p.display().to_string().replace('%', "%%"))
}

/// 生成 CSGOVAC_REPAIR.bat（修复主脚本，内容对齐 Steam++ 的 CsgoVacRepairPageViewModel）。
/// 参数 %1 为 Steam 的 bin 目录路径，由 RUNNER 脚本传入。
fn build_repair_script() -> String {
    let script = r#"@echo off
chcp 65001
goto enableservice
:steam
echo Info - Checking if Steam is launched.
tasklist | find /I "Steam.exe"
if errorlevel 1 goto closedstatus
if not errorlevel 1 goto killsteam

:killsteam
taskkill /F /IM Steam.exe
goto steamrepair

:closedstatus
echo Info - Not Started
goto steamrepair

:enableservice
sc config Netman start= AUTO
sc start Netman
sc config RasMan start= AUTO
sc start RasMan
sc config TapiSrv start= AUTO
sc start TapiSrv
sc config MpsSvc start= AUTO
sc start MpsSvc
netsh advfirewall set allprofiles state on
goto steam

:steamrepair
echo Info - ※^>^>^> 执行修复启动器服务项
cd /d %1
steamservice /install
ping -n 2 127.0.0.1>nul
echo.
steamservice /repair
ping -n 2 127.0.0.1>nul
echo Info - ※ 恢复DEP默认启动设置
bcdedit /deletevalue nointegritychecks
bcdedit /deletevalue loadoptions
bcdedit /debug off
bcdedit /deletevalue nx
echo Info - ※^>^>^> 重启 Steam
cd /d ..
start /high steam
ping -n 2 127.0.0.1>nul
sc config "Steam Client Service" start= AUTO
sc start "Steam Client Service"
echo Info - ※ 执行完毕
exit
"#;
    script.replace('\n', "\r\n")
}

/// 生成 RUNNER 批处理：一次性烘焙「调用修复脚本 + 传参 + 输出重定向」。
/// 批处理文件内容由 cmd 内部解析，引号处理可靠，不经过命令行参数传递。
fn build_runner_script(repair_bat: &Path, steam_bin: &Path, log_path: &Path) -> String {
    let content = format!(
        "@echo off\r\ncall {} {} > {} 2>&1\r\n",
        batch_quote(repair_bat),
        batch_quote(steam_bin),
        batch_quote(log_path)
    );
    content
}

// ============================== 执行 ==============================

/// 通过 PowerShell Start-Process 执行 RUNNER 脚本（非管理员时附 -Verb RunAs 弹 UAC）。
/// 与 pawnio_driver.rs 的提权方案一致：PS 脚本内只有单引号、无双引号嵌套，
/// Rust Command 引号处理后给出的整条命令可被 PowerShell 正确解析。
fn spawn_repair_runner(runner_bat: &Path, need_elevation: bool) -> Result<std::process::Child, String> {
    let runner = runner_bat.to_string_lossy().replace('\'', "''");
    let verb = if need_elevation { " -Verb RunAs" } else { "" };
    let ps = format!(
        "Start-Process -FilePath 'cmd.exe' -ArgumentList '/d','/c','{}'{} -Wait -WindowStyle Hidden",
        runner, verb
    );
    log::info!("[VacRepair] spawn powershell: {ps}");
    Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动修复进程失败: {e}"))
}

// ============================== 日志轮询 / 事件推送 ==============================

/// 读取日志文件新增字节，按行拆分后推送事件；跨次读取的残行暂存在 pending 中。
fn tail_log(path: &Path, offset: &mut u64, pending: &mut String, app: &tauri::AppHandle) {
    let Ok(mut file) = fs::File::open(path) else { return };
    if file.seek(SeekFrom::Start(*offset)).is_err() {
        return;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return;
    }
    *offset += buf.len() as u64;

    pending.push_str(&String::from_utf8_lossy(&buf));
    let mut start = 0usize;
    while let Some(pos) = pending[start..].find('\n') {
        let line = pending[start..start + pos].trim_end_matches('\r').to_string();
        start += pos + 1;
        if line.is_empty() {
            continue;
        }
        let _ = app.emit(EVENT_OUTPUT, line.clone());
        // 防火墙例外添加失败：追加 Steam++ 同款提示
        if line.contains("Add firewall exception failed for steamservice.exe") {
            let _ = app.emit(EVENT_OUTPUT, "info - ※ 修复 Steam Services 失败");
            let _ = app.emit(EVENT_OUTPUT, "info - ※ 请检查您的防火墙设置(关闭 \"不允许例外\" 选项)再次尝试");
        }
    }
    pending.drain(..start);
}

// ============================== 修复入口 ==============================

/// 执行 CS:GO VAC 修复：生成脚本并以管理员权限运行，输出流式推送到前端。
#[tauri::command]
pub async fn run_vac_repair(app: tauri::AppHandle) -> Result<(), String> {
    let steam_path = get_steam_path().ok_or("未检测到 Steam 安装，请先安装并运行一次 Steam 客户端")?;
    let steam_bin = steam_path.join("bin");
    if !steam_bin.join("steamservice.exe").exists() {
        return Err("未找到 steamservice.exe，请确认 Steam 安装完整（可先在 Steam 设置中修复库文件夹）。".to_string());
    }

    // 1) 生成修复脚本 + RUNNER + 日志文件（放在应用数据目录，避免被系统清理）
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法定位应用数据目录: {e}"))?
        .join("BAT");
    fs::create_dir_all(&dir).map_err(|e| format!("创建脚本目录失败: {e}"))?;
    let repair_bat = dir.join("CSGOVAC_REPAIR.bat");
    let runner_bat = dir.join("CSGOVAC_REPAIR_RUN.bat");
    let log_path = dir.join("vac_repair.log");
    fs::write(&repair_bat, build_repair_script()).map_err(|e| format!("写入修复脚本失败: {e}"))?;
    fs::write(&runner_bat, build_runner_script(&repair_bat, &steam_bin, &log_path))
        .map_err(|e| format!("写入 Runner 脚本失败: {e}"))?;
    let _ = fs::write(&log_path, b""); // 预创建日志文件，保证轮询可打开

    let is_admin = crate::optimization::is_admin();
    let _ = app.emit(EVENT_OUTPUT, ">>> 正在执行 CS:GO VAC 修复...");

    let app_for_task = app.clone();
    let join_result: Result<(), String> =
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let mut child = spawn_repair_runner(&runner_bat, !is_admin)?;

            // 2) 轮询日志文件并转发输出，直到进程结束
            let mut offset: u64 = 0;
            let mut pending = String::new();
            let mut had_output = false;
            loop {
                let before = pending.len();
                tail_log(&log_path, &mut offset, &mut pending, &app_for_task);
                if pending.len() > before {
                    had_output = true;
                }
                match child.try_wait() {
                    Ok(Some(_)) => {
                        // 等待输出刷盘后再取一次尾巴
                        std::thread::sleep(Duration::from_millis(300));
                        let before = pending.len();
                        tail_log(&log_path, &mut offset, &mut pending, &app_for_task);
                        if pending.len() > before {
                            had_output = true;
                        }
                        if !had_output {
                            let _ = app_for_task.emit(
                                EVENT_OUTPUT,
                                "Tips - 未捕获到脚本输出（可能取消了 UAC 授权，或 Steam 目录不可写）",
                            );
                        }
                        let _ = app_for_task.emit(EVENT_OUTPUT, "Info - ※ 执行完毕");
                        let _ = app_for_task.emit(EVENT_DONE, true);
                        break;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let _ = app_for_task.emit(EVENT_OUTPUT, &format!("Err - 修复进程异常: {e}"));
                        let _ = app_for_task.emit(EVENT_DONE, false);
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Ok(())
        })
        .await
        .map_err(|e| format!("修复任务异常: {e}"))?;
    join_result
}