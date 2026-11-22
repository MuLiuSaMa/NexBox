use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use tauri::Emitter;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThirdPartyTool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tool_type: String,
    pub download_url: String,
    pub file_name: String,
    pub website_url: Option<String>,
    pub check_executable: Option<String>,
}

fn get_tools_directory() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("NexBox");
    path.push("Tools");
    if !path.exists() {
        let _ = fs::create_dir_all(&path);
    }
    path
}

fn get_tools_list() -> Vec<ThirdPartyTool> {
    vec![
        ThirdPartyTool {
            id: "memreduct".to_string(),
            name: "Memreduct".to_string(),
            description: "内存清理工具".to_string(),
            category: "optimization".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://memreduct.org".to_string()),
            check_executable: Some("memreduct.exe".to_string()),
        },
        ThirdPartyTool {
            id: "optimizer".to_string(),
            name: "Optimizer".to_string(),
            description: "Windows系统优化工具".to_string(),
            category: "optimization".to_string(),
            tool_type: "portable".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://github.com/hellzerg/optimizer/".to_string()),
            check_executable: None,
        },
        ThirdPartyTool {
            id: "cpu-z".to_string(),
            name: "CPU-Z".to_string(),
            description: "CPU信息检测工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.cpuid.com/softwares/cpu-z.html".to_string()),
            check_executable: Some("cpuz.exe".to_string()),
        },
        ThirdPartyTool {
            id: "gpu-z".to_string(),
            name: "GPU-Z".to_string(),
            description: "显卡信息检测工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "portable".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.techpowerup.com/gpuz/".to_string()),
            check_executable: None,
        },

        ThirdPartyTool {
            id: "process-lasso".to_string(),
            name: "Process Lasso".to_string(),
            description: "系统进程优化工具".to_string(),
            category: "optimization".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://bitsum.com/download-process-lasso/".to_string()),
            check_executable: Some("ProcessLassoLauncher.exe".to_string()),
        },
        ThirdPartyTool {
            id: "fxsound".to_string(),
            name: "FxSound".to_string(),
            description: "音频增强工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.fxsound.com/download".to_string()),
            check_executable: Some("FxSound.exe".to_string()),
        },
        ThirdPartyTool {
            id: "msi-afterburner".to_string(),
            name: "MSI Afterburner".to_string(),
            description: "显卡超频监控工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.msi.cn/Landing/afterburner/graphics-cards".to_string()),
            check_executable: Some("MSIAfterburner.exe".to_string()),
        },
        ThirdPartyTool {
            id: "geek".to_string(),
            name: "Geek".to_string(),
            description: "软件卸载清理工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "portable".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://geekuninstaller.com/".to_string()),
            check_executable: None,
        },
        ThirdPartyTool {
            id: "hwinfo".to_string(),
            name: "HWiNFO".to_string(),
            description: "硬件信息检测工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.hwinfo.com/".to_string()),
            check_executable: Some("HWiNFO64.exe".to_string()),
        },
        ThirdPartyTool {
            id: "crystaldiskinfo".to_string(),
            name: "CrystalDiskInfo".to_string(),
            description: "硬盘健康检测工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://crystalmark.info/en/software/crystaldiskinfo/".to_string()),
            check_executable: Some("DiskInfo64.exe".to_string()),
        },
        ThirdPartyTool {
            id: "core-temp".to_string(),
            name: "Core Temp".to_string(),
            description: "CPU温度监控工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.alcpu.com/CoreTemp/".to_string()),
            check_executable: Some("Core Temp.exe".to_string()),
        },
        ThirdPartyTool {
            id: "aida64".to_string(),
            name: "AIDA64".to_string(),
            description: "系统诊断基准测试工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.aida64.com/".to_string()),
            check_executable: Some("aida64.exe".to_string()),
        },
        ThirdPartyTool {
            id: "ddu".to_string(),
            name: "DDU".to_string(),
            description: "显卡驱动卸载工具".to_string(),
            category: "optimization".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.wagnardsoft.com/display-driver-uninstaller-DDU-".to_string()),
            check_executable: Some("Display Driver Uninstaller.exe".to_string()),
        },
        ThirdPartyTool {
            id: "lossless-scaling".to_string(),
            name: "Lossless Scaling".to_string(),
            description: "游戏帧生成缩放工具".to_string(),
            category: "optimization".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://store.steampowered.com/app/993090/".to_string()),
            check_executable: Some("LosslessScaling.exe".to_string()),
        },
        ThirdPartyTool {
            id: "everything".to_string(),
            name: "Everything".to_string(),
            description: "文件快速搜索工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.voidtools.com/".to_string()),
            check_executable: Some("Everything.exe".to_string()),
        },
        ThirdPartyTool {
            id: "dismpp".to_string(),
            name: "Dism++".to_string(),
            description: "系统维护优化工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "portable".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://github.com/Chuyu-Team/Dism-Multi-language".to_string()),
            check_executable: None,
        },
        ThirdPartyTool {
            id: "snipaste".to_string(),
            name: "Snipaste".to_string(),
            description: "截图贴图工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.snipaste.com/".to_string()),
            check_executable: Some("Snipaste.exe".to_string()),
        },
        ThirdPartyTool {
            id: "powertoys".to_string(),
            name: "PowerToys".to_string(),
            description: "微软官方增强工具集".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://github.com/microsoft/PowerToys".to_string()),
            check_executable: Some("PowerToys.exe".to_string()),
        },
        ThirdPartyTool {
            id: "wiztree".to_string(),
            name: "WizTree".to_string(),
            description: "磁盘空间分析工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://diskanalyzer.com/".to_string()),
            check_executable: Some("WizTree.exe".to_string()),
        },
        ThirdPartyTool {
            id: "7zip".to_string(),
            name: "7-Zip".to_string(),
            description: "开源压缩解压工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.7-zip.org/".to_string()),
            check_executable: Some("7zFM.exe".to_string()),
        },
        ThirdPartyTool {
            id: "watt-toolkit".to_string(),
            name: "Watt Toolkit".to_string(),
            description: "开源网络加速工具".to_string(),
            category: "network".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://steampp.net/".to_string()),
            check_executable: Some("Steam++.exe".to_string()),
        },
        ThirdPartyTool {
            id: "trafficmonitor".to_string(),
            name: "TrafficMonitor".to_string(),
            description: "任务栏网速监控工具".to_string(),
            category: "network".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://github.com/zhongyang219/TrafficMonitor".to_string()),
            check_executable: Some("TrafficMonitor.exe".to_string()),
        },
        ThirdPartyTool {
            id: "rtss".to_string(),
            name: "RTSS".to_string(),
            description: "帧率统计显示工具".to_string(),
            category: "hardware".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://www.guru3d.com/download/rtss-rivatuner-statistics-server-download/".to_string()),
            check_executable: Some("RTSS.exe".to_string()),
        },
        ThirdPartyTool {
            id: "playnite".to_string(),
            name: "Playnite".to_string(),
            description: "全平台游戏库管理工具".to_string(),
            category: "assistant".to_string(),
            tool_type: "install".to_string(),
            download_url: String::new(),
            file_name: String::new(),
            website_url: Some("https://playnite.link/".to_string()),
            check_executable: Some("Playnite.DesktopApp.exe".to_string()),
        },
    ]
}

#[tauri::command]
pub fn get_thirdparty_tools() -> Vec<ThirdPartyTool> {
    get_tools_list()
}

#[tauri::command]
pub fn get_tool_install_path(tool_id: String) -> Option<String> {
    let tools = get_tools_list();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        if tool.tool_type == "install" {
            if let Some(check_exe) = &tool.check_executable {
                return find_via_desktop_shortcut(check_exe)
                    .map(|p| p.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn find_via_desktop_shortcut(exe_name: &str) -> Option<PathBuf> {
    let desktop_paths = get_desktop_paths();
    let mut lnk_files: Vec<PathBuf> = Vec::new();

    for desktop_path in desktop_paths {
        if !desktop_path.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&desktop_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext.eq_ignore_ascii_case("lnk") {
                        lnk_files.push(path);
                    }
                }
            }
        }
    }

    if lnk_files.is_empty() {
        return None;
    }

    let lnk_paths: Vec<String> = lnk_files
        .iter()
        .map(|p| format!("'{}'", p.to_string_lossy().escape_default()))
        .collect();

    let ps_script = format!(
        r#"$shell = New-Object -ComObject WScript.Shell; @({}) | ForEach-Object {{ $shell.CreateShortcut($_).TargetPath }}"#,
        lnk_paths.join(",")
    );

    if let Ok(output) = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                let target = line.trim();
                if !target.is_empty() {
                    let target_path = PathBuf::from(target);
                    if let Some(file_name) = target_path.file_name() {
                        if file_name.to_string_lossy().eq_ignore_ascii_case(exe_name) {
                            return Some(target_path);
                        }
                    }
                }
            }
        }
    }

    None
}

fn get_desktop_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(user_profile) = dirs::home_dir() {
        paths.push(user_profile.join("Desktop"));
    }

    paths.push(PathBuf::from("C:\\Users\\Public\\Desktop"));

    paths
}

#[tauri::command]
pub fn get_tool_download_path(tool_id: String) -> String {
    let tools = get_tools_list();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        let tools_dir = get_tools_directory();
        tools_dir
            .join(&tool.file_name)
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

#[tauri::command]
pub fn run_tool(tool_id: String) -> Result<(), String> {
    let tools = get_tools_list();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        match tool.tool_type.as_str() {
            "install" => {
                if let Some(check_exe) = &tool.check_executable {
                    if let Some(exe_path) = find_via_desktop_shortcut(check_exe) {
                        Command::new("cmd")
                            .args(["/c", "start", "", exe_path.to_str().unwrap()])
                            .creation_flags(CREATE_NO_WINDOW)
                            .spawn()
                            .map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                    return Err("Executable not found".to_string());
                }
            }
            "portable" => {
                let tools_dir = get_tools_directory();
                let tool_path = tools_dir.join(&tool.file_name);
                if tool_path.exists() {
                    Command::new("cmd")
                        .args(["/c", "start", "", tool_path.to_str().unwrap()])
                        .creation_flags(CREATE_NO_WINDOW)
                        .spawn()
                        .map_err(|e| e.to_string())?;
                } else {
                    return Err("Tool not found".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    } else {
        Err("Tool not found".to_string())
    }
}

#[tauri::command]
pub async fn download_tool(tool_id: String, window: tauri::Window) -> Result<String, String> {
    let tools = get_tools_list();
    if let Some(tool) = tools.iter().find(|t| t.id == tool_id) {
        let tools_dir = get_tools_directory();
        let file_path = tools_dir.join(&tool.file_name);

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client
            .get(&tool.download_url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let total_size = response.content_length().unwrap_or(0);

        let mut file = std::fs::File::create(&file_path).map_err(|e| e.to_string())?;

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut file, &chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            let progress = (downloaded * 100).checked_div(total_size).unwrap_or(0) as u32;

            let _ = window.emit(
                "tool-download-progress",
                serde_json::json!({
                    "tool_id": tool_id,
                    "progress": progress,
                }),
            );
        }

        Ok(file_path.to_string_lossy().into_owned())
    } else {
        Err("Tool not found".to_string())
    }
}

#[tauri::command]
pub async fn open_tool_installer(file_path: String) -> Result<(), String> {
    Command::new("cmd")
        .args(["/c", "start", "", &file_path])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
