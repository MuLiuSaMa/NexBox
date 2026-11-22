//! 运行库补全/修复模块
//!
//! 通过注册表 + 文件系统检测 VC++ / .NET Framework / DirectX 旧版兼容组件的缺失或不完整项，
//! 从微软官方 URL 下载对应安装包，校验 Microsoft Authenticode 签名后静默安装。
//! 纯远程下载方案，不捆绑本地安装包资源。

use futures_util::StreamExt;
use reqwest::Client;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{Emitter, Window};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use winreg::enums::HKEY_LOCAL_MACHINE;
#[cfg(target_os = "windows")]
use winreg::RegKey;

const VC_X64_URL: &str = "https://aka.ms/vc14/vc_redist.x64.exe";
const VC_X86_URL: &str = "https://aka.ms/vc14/vc_redist.x86.exe";
const VC_2013_X64_URL: &str = "https://aka.ms/highdpimfc2013x64enu";
const VC_2013_X86_URL: &str = "https://aka.ms/highdpimfc2013x86enu";
const VC_2012_X64_URL: &str = "https://download.microsoft.com/download/1/6/B/16B06F60-3B20-4FF2-B699-5E9B7962F9AE/VSU_4/vcredist_x64.exe";
const VC_2012_X86_URL: &str = "https://download.microsoft.com/download/1/6/B/16B06F60-3B20-4FF2-B699-5E9B7962F9AE/VSU_4/vcredist_x86.exe";
const VC_2010_X64_URL: &str = "https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x64.exe";
const VC_2010_X86_URL: &str = "https://download.microsoft.com/download/1/6/5/165255E7-1014-4D0A-B094-B6A430A6BFFC/vcredist_x86.exe";
const VC_2008_X64_URL: &str = "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x64.exe";
const VC_2008_X86_URL: &str = "https://download.microsoft.com/download/5/D/8/5D8C65CB-C849-4025-8E95-C3966CAFD8AE/vcredist_x86.exe";
const DOTNET_481_URL: &str = "https://go.microsoft.com/fwlink/?linkid=2203305";
const DIRECTX_URL: &str = "https://download.microsoft.com/download/1/7/1/1718ccc4-6315-4d8e-9543-8e28a4e18c4c/dxwebsetup.exe";
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, serde::Serialize)]
struct RuntimeRepairProgress {
    runtime_id: String,
    phase: String,
    progress: u8,
    detail: String,
}

struct RuntimePackage {
    file_name: &'static str,
    url: &'static str,
    args: &'static [&'static str],
}

#[derive(Clone, serde::Serialize)]
pub struct RuntimeStatus {
    pub id: String,
    pub installed: bool,
    pub summary: String,
    pub missing_components: Vec<String>,
}

#[cfg(target_os = "windows")]
fn windows_dir() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

#[cfg(target_os = "windows")]
fn add_missing_file(missing: &mut Vec<String>, label: &str, path: PathBuf) {
    if !path.exists() {
        missing.push(label.to_string());
    }
}

#[cfg(target_os = "windows")]
fn has_visual_cpp_redist(architecture: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let dependency_prefix = if architecture == "x64" { "vc,redist.x64" } else { "vc,redist.x86" };
    let has_dependency = hklm
        .open_subkey(r"SOFTWARE\Classes\Installer\Dependencies")
        .ok()
        .is_some_and(|key| key.enum_keys().flatten().any(|name| name.to_ascii_lowercase().starts_with(dependency_prefix)));
    if has_dependency {
        return true;
    }
    let paths = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];
    paths.iter().any(|path| {
        hklm.open_subkey(path).ok().is_some_and(|key| {
            key.enum_keys().flatten().any(|subkey_name| {
                key.open_subkey(&subkey_name).ok().is_some_and(|subkey| {
                    let name = subkey.get_value::<String, _>("DisplayName").unwrap_or_default();
                    let normalized = name.to_ascii_lowercase();
                    normalized.contains("microsoft visual c++")
                        && (normalized.contains("v14 redistributable") || normalized.contains("2022") || normalized.contains("2015-2019") || normalized.contains("2015-2022") || normalized.contains("2015-2026"))
                        && normalized.contains(architecture)
                })
            })
        })
    })
}

#[cfg(target_os = "windows")]
fn has_legacy_visual_cpp_redist(version: &str, architecture: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut names = Vec::new();
    for path in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        if let Ok(key) = hklm.open_subkey(path) {
            for subkey_name in key.enum_keys().flatten() {
                if let Ok(subkey) = key.open_subkey(&subkey_name) {
                    if let Ok(name) = subkey.get_value::<String, _>("DisplayName") {
                        names.push(name.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    let names = names.into_iter().filter(|name| {
        name.contains("microsoft visual c++")
            && name.contains(version)
            && (name.contains(&format!("({})", architecture)) || name.contains(&format!(" {} ", architecture)))
    }).collect::<Vec<_>>();

    let has_additional = names.iter().any(|name| name.contains("additional runtime"));
    let has_minimum = names.iter().any(|name| name.contains("minimum runtime"));
    (has_additional && has_minimum) || names.iter().any(|name| name.contains("redistributable") && !name.contains("runtime"))
}

#[cfg(target_os = "windows")]
fn add_v14_missing_files(missing: &mut Vec<String>, directory: &Path, directory_label: &str) {
    for file in [
        "vcruntime140.dll",
        "msvcp140.dll",
        "msvcp140_1.dll",
        "msvcp140_2.dll",
        "concrt140.dll",
        "vcomp140.dll",
        "ucrtbase.dll",
    ] {
        add_missing_file(missing, &format!("缺少 {}\\{}", directory_label, file), directory.join(file));
    }
}

#[cfg(target_os = "windows")]
fn add_legacy_missing_files(
    missing: &mut Vec<String>,
    version: &str,
    architecture: &str,
    directory: &Path,
    directory_label: &str,
    files: &[&str],
) {
    for file in files {
        add_missing_file(
            missing,
            &format!("缺少 Visual C++ {} {} {}\\{}", version, architecture, directory_label, file),
            directory.join(file),
        );
    }
}

#[cfg(target_os = "windows")]
fn has_vc2008_winsxs_file(windows: &Path, architecture: &str, file: &str) -> bool {
    let prefix = if architecture == "x64" {
        "amd64_microsoft.vc90.crt_"
    } else {
        "x86_microsoft.vc90.crt_"
    };
    fs::read_dir(windows.join("WinSxS"))
        .ok()
        .is_some_and(|entries| entries.flatten().any(|entry| {
            entry.file_name().to_string_lossy().to_ascii_lowercase().starts_with(prefix)
                && entry.path().join(file).exists()
        }))
}

#[cfg(target_os = "windows")]
fn detect_runtime_statuses_inner() -> Vec<RuntimeStatus> {
    let windows = windows_dir();
    let system32 = windows.join("System32");
    let syswow64 = windows.join("SysWOW64");
    let is_64_bit = syswow64.exists();

    let mut vc_missing = Vec::new();
    if !has_visual_cpp_redist("x64") && is_64_bit {
        vc_missing.push("未检测到 Visual C++ v14 x64 注册项".to_string());
    }
    if !has_visual_cpp_redist("x86") {
        vc_missing.push("未检测到 Visual C++ v14 x86 注册项".to_string());
    }
    add_v14_missing_files(&mut vc_missing, &system32, if is_64_bit { "System32 (x64)" } else { "System32 (x86)" });
    if is_64_bit {
        add_v14_missing_files(&mut vc_missing, &syswow64, "SysWOW64 (x86)");
    }
    for version in ["2013", "2012", "2010", "2008"] {
        if is_64_bit && !has_legacy_visual_cpp_redist(version, "x64") {
            vc_missing.push(format!("未检测到 Visual C++ {} x64 运行库", version));
        }
        if !has_legacy_visual_cpp_redist(version, "x86") {
            vc_missing.push(format!("未检测到 Visual C++ {} x86 运行库", version));
        }
    }
    for (version, files) in [
        ("2013", &["msvcr120.dll", "msvcp120.dll"][..]),
        ("2012", &["msvcr110.dll", "msvcp110.dll"][..]),
        ("2010", &["msvcr100.dll", "msvcp100.dll"][..]),
    ] {
        if is_64_bit {
            add_legacy_missing_files(&mut vc_missing, version, "x64", &system32, "System32 (x64)", files);
            add_legacy_missing_files(&mut vc_missing, version, "x86", &syswow64, "SysWOW64 (x86)", files);
        } else {
            add_legacy_missing_files(&mut vc_missing, version, "x86", &system32, "System32 (x86)", files);
        }
    }
    for architecture in if is_64_bit { &["x64", "x86"][..] } else { &["x86"][..] } {
        for file in ["msvcr90.dll", "msvcp90.dll"] {
            if !has_vc2008_winsxs_file(&windows, architecture, file) {
                vc_missing.push(format!("缺少 Visual C++ 2008 {} WinSxS\\{}", architecture, file));
            }
        }
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let net_release = hklm
        .open_subkey(r"SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full")
        .ok()
        .and_then(|key| key.get_value::<u32, _>("Release").ok())
        .unwrap_or_default();
    let mut dotnet_missing = Vec::new();
    if net_release < 533_320 {
        dotnet_missing.push("未检测到 .NET Framework 4.8.1 Runtime".to_string());
    }
    add_missing_file(
        &mut dotnet_missing,
        "缺少 Framework\\v4.0.30319\\mscorlib.dll",
        windows.join("Microsoft.NET").join("Framework").join("v4.0.30319").join("mscorlib.dll"),
    );
    if is_64_bit {
        add_missing_file(
            &mut dotnet_missing,
            "缺少 Framework64\\v4.0.30319\\mscorlib.dll",
            windows.join("Microsoft.NET").join("Framework64").join("v4.0.30319").join("mscorlib.dll"),
        );
    }

    let directx_dlls = [
        "d3dx9_43.dll",
        "d3dx10_43.dll",
        "d3dx11_43.dll",
        "d3dcompiler_43.dll",
        "d3dcsx_43.dll",
        "xinput1_3.dll",
        "xaudio2_7.dll",
        "x3daudio1_7.dll",
        "xapofx1_5.dll",
        "xactengine3_7.dll",
    ];
    let mut directx_missing = Vec::new();
    for file in directx_dlls {
        add_missing_file(&mut directx_missing, &format!("缺少 System32\\{}", file), system32.join(file));
        if is_64_bit {
            add_missing_file(&mut directx_missing, &format!("缺少 SysWOW64\\{}", file), syswow64.join(file));
        }
    }

    vec![
        RuntimeStatus {
            id: "visual-cpp".to_string(),
            installed: vc_missing.is_empty(),
            summary: if vc_missing.is_empty() { "Visual C++ 2008-2026 x86/x64 游戏运行库完整".to_string() } else { format!("检测到 {} 项缺失", vc_missing.len()) },
            missing_components: vc_missing,
        },
        RuntimeStatus {
            id: "dotnet".to_string(),
            installed: dotnet_missing.is_empty(),
            summary: if dotnet_missing.is_empty() { format!(".NET Framework 4.8.1 Runtime 已安装 (Release {})", net_release) } else { format!("检测到 {} 项缺失", dotnet_missing.len()) },
            missing_components: dotnet_missing,
        },
        RuntimeStatus {
            id: "directx".to_string(),
            installed: directx_missing.is_empty(),
            summary: if directx_missing.is_empty() { "DirectX 旧版游戏兼容组件完整".to_string() } else { format!("检测到 {} 个 DirectX 兼容 DLL 缺失", directx_missing.len()) },
            missing_components: directx_missing,
        },
    ]
}

#[cfg(not(target_os = "windows"))]
fn detect_runtime_statuses_inner() -> Vec<RuntimeStatus> {
    Vec::new()
}

#[tauri::command]
pub fn get_runtime_statuses() -> Result<Vec<RuntimeStatus>, String> {
    #[cfg(target_os = "windows")]
    { Ok(detect_runtime_statuses_inner()) }
    #[cfg(not(target_os = "windows"))]
    { Err("运行库检测仅支持 Windows 系统".to_string()) }
}

fn runtime_packages(runtime_id: &str, missing_components: &[String]) -> Result<Vec<RuntimePackage>, String> {
    match runtime_id {
        "visual-cpp" => {
            let needs = |version: &str, architecture: &str| {
                missing_components.iter().any(|component| component.contains(&format!("Visual C++ {} {}", version, architecture)))
            };
            let v14_files = ["vcruntime140.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "concrt140.dll", "vcomp140.dll", "ucrtbase.dll"];
            let needs_v14_x64 = missing_components.iter().any(|component| {
                component.contains("Visual C++ v14 x64")
                    || v14_files.iter().any(|file| component.contains(&format!("System32 (x64)\\{}", file)))
            });
            let needs_v14_x86 = missing_components.iter().any(|component| {
                component.contains("Visual C++ v14 x86")
                    || v14_files.iter().any(|file| component.contains(&format!("SysWOW64 (x86)\\{}", file)) || component.contains(&format!("System32 (x86)\\{}", file)))
            });
            let needs_repair = |version: &str, architecture: &str| {
                !missing_components.iter().any(|component| component == &format!("未检测到 Visual C++ {} {} 运行库", version, architecture))
            };
            let mut packages = Vec::new();
            if needs("2013", "x64") { packages.push(RuntimePackage { file_name: "vc2013_x64.exe", url: VC_2013_X64_URL, args: if needs_repair("2013", "x64") { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if needs("2013", "x86") { packages.push(RuntimePackage { file_name: "vc2013_x86.exe", url: VC_2013_X86_URL, args: if needs_repair("2013", "x86") { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if needs("2012", "x64") { packages.push(RuntimePackage { file_name: "vc2012_x64.exe", url: VC_2012_X64_URL, args: if needs_repair("2012", "x64") { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if needs("2012", "x86") { packages.push(RuntimePackage { file_name: "vc2012_x86.exe", url: VC_2012_X86_URL, args: if needs_repair("2012", "x86") { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if needs("2010", "x64") { packages.push(RuntimePackage { file_name: "vc2010_x64.exe", url: VC_2010_X64_URL, args: if needs_repair("2010", "x64") { &["/repair", "/quiet", "/norestart"] } else { &["/q", "/norestart"] } }); }
            if needs("2010", "x86") { packages.push(RuntimePackage { file_name: "vc2010_x86.exe", url: VC_2010_X86_URL, args: if needs_repair("2010", "x86") { &["/repair", "/quiet", "/norestart"] } else { &["/q", "/norestart"] } }); }
            if needs("2008", "x64") { packages.push(RuntimePackage { file_name: "vc2008_x64.exe", url: VC_2008_X64_URL, args: if needs_repair("2008", "x64") { &["/repair", "/quiet", "/norestart"] } else { &["/q", "/norestart"] } }); }
            if needs("2008", "x86") { packages.push(RuntimePackage { file_name: "vc2008_x86.exe", url: VC_2008_X86_URL, args: if needs_repair("2008", "x86") { &["/repair", "/quiet", "/norestart"] } else { &["/q", "/norestart"] } }); }
            let repair_v14_x64 = missing_components.iter().any(|component| component.contains("System32 (x64)"));
            let repair_v14_x86 = missing_components.iter().any(|component| component.contains("SysWOW64 (x86)") || component.contains("System32 (x86)"));
            if needs_v14_x64 { packages.push(RuntimePackage { file_name: "vc14_x64.exe", url: VC_X64_URL, args: if repair_v14_x64 { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if needs_v14_x86 { packages.push(RuntimePackage { file_name: "vc14_x86.exe", url: VC_X86_URL, args: if repair_v14_x86 { &["/repair", "/quiet", "/norestart"] } else { &["/install", "/quiet", "/norestart"] } }); }
            if packages.is_empty() { Err("未找到可修复的 Visual C++ 缺失项".to_string()) } else { Ok(packages) }
        }
        "dotnet" => Ok(vec![
            RuntimePackage { file_name: "ndp481-x86-x64-allos-enu.exe", url: DOTNET_481_URL, args: &["/repair", "/quiet", "/norestart"] },
        ]),
        "directx" => Ok(vec![
            RuntimePackage { file_name: "dxwebsetup.exe", url: DIRECTX_URL, args: &["/Q"] },
        ]),
        _ => Err("不支持的运行库修复项目".to_string()),
    }
}

fn runtime_cache_dir() -> Result<PathBuf, String> {
    let path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("NexBox")
        .join("RuntimeRepair");
    fs::create_dir_all(&path).map_err(|error| format!("无法创建运行库缓存目录: {}", error))?;
    Ok(path)
}

fn emit_progress(window: &Window, runtime_id: &str, phase: &str, progress: u8, detail: &str) {
    let _ = window.emit("runtime-repair-progress", RuntimeRepairProgress {
        runtime_id: runtime_id.to_string(),
        phase: phase.to_string(),
        progress,
        detail: detail.to_string(),
    });
}

async fn download_package(
    client: &Client,
    package: &RuntimePackage,
    destination: &Path,
    window: &Window,
    runtime_id: &str,
    base_progress: u8,
    span: u8,
) -> Result<(), String> {
    let response = client.get(package.url)
        .send().await
        .map_err(|error| format!("下载 {} 失败: {}", package.file_name, error))?
        .error_for_status()
        .map_err(|error| format!("下载 {} 失败: {}", package.file_name, error))?;
    let total = response.content_length().unwrap_or(0);
    let mut file = fs::File::create(destination)
        .map_err(|error| format!("创建安装包缓存失败: {}", error))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("下载 {} 失败: {}", package.file_name, error))?;
        file.write_all(&chunk).map_err(|error| format!("写入安装包失败: {}", error))?;
        downloaded += chunk.len() as u64;
        let completed = (downloaded * span as u64).checked_div(total).unwrap_or(0) as u8;
        emit_progress(window, runtime_id, "downloading", base_progress.saturating_add(completed), package.file_name);
    }
    file.flush().map_err(|error| format!("写入安装包失败: {}", error))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn verify_microsoft_signature(path: &Path) -> Result<(), String> {
    // Extra arguments following PowerShell's -Command are parsed as command text
    // on Windows PowerShell 5.1. Embed the escaped path in the script instead.
    let escaped_path = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$signature = Get-AuthenticodeSignature -LiteralPath '{}'; if ($signature.Status -eq 'Valid' -and $signature.SignerCertificate.Subject -match 'Microsoft Corporation') {{ exit 0 }}; Write-Error ('签名校验失败: ' + $signature.Status); exit 1",
        escaped_path
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("无法校验安装包签名: {}", error))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn verify_microsoft_signature(_path: &Path) -> Result<(), String> {
    Err("运行库修复仅支持 Windows 系统".to_string())
}

#[cfg(target_os = "windows")]
fn run_installer(path: &Path, args: &[&str]) -> Result<bool, String> {
    let status = Command::new(path)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|error| format!("无法启动安装程序: {}", error))?;
    match status.code() {
        Some(0) => Ok(false),
        Some(3010) | Some(1641) => Ok(true),
        Some(code) => Err(format!("安装程序退出代码: {}", code)),
        None => Err("安装程序异常退出".to_string()),
    }
}

#[cfg(not(target_os = "windows"))]
fn run_installer(_path: &Path, _args: &[&str]) -> Result<bool, String> {
    Err("运行库修复仅支持 Windows 系统".to_string())
}

#[tauri::command]
pub async fn repair_runtime(runtime_id: String, window: Window) -> Result<String, String> {
    let status = get_runtime_statuses()?
        .into_iter()
        .find(|status| status.id == runtime_id)
        .ok_or("未找到运行库检测结果")?;
    if status.installed {
        return Err("检测结果显示该运行库完整，无需修复".to_string());
    }
    let packages = runtime_packages(&runtime_id, &status.missing_components)?;
    let cache_dir = runtime_cache_dir()?;
    let client = Client::builder()
        .user_agent("NexBox Runtime Repair")
        .build()
        .map_err(|error| format!("无法创建下载客户端: {}", error))?;
    let package_count = packages.len() as u8;
    let mut restart_required = false;

    for (index, package) in packages.iter().enumerate() {
        let base_progress = (index as u8 * 100) / package_count;
        let span = 100 / package_count;
        let destination = cache_dir.join(package.file_name);
        emit_progress(&window, &runtime_id, "downloading", base_progress, package.file_name);
        download_package(&client, package, &destination, &window, &runtime_id, base_progress, span.saturating_sub(8)).await?;
        emit_progress(&window, &runtime_id, "verifying", base_progress.saturating_add(span.saturating_sub(7)), package.file_name);
        verify_microsoft_signature(&destination)?;
        emit_progress(&window, &runtime_id, "installing", base_progress.saturating_add(span.saturating_sub(4)), package.file_name);
        restart_required |= run_installer(&destination, package.args)?;
    }

    emit_progress(&window, &runtime_id, "complete", 100, "完成");
    Ok(if restart_required {
        "修复完成，系统需要重启后才能完全生效".to_string()
    } else {
        "修复完成".to_string()
    })
}
