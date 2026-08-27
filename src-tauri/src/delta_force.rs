use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::Manager;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DeltaPasswordItem {
    pub name: String,
    pub password: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DeltaPasswordResponse {
    pub status: String,
    pub data: Vec<DeltaPasswordItem>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct WeaponCode {
    pub id: String,
    pub name: String,
    pub code: String,
    pub category: String,
    pub description: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DLSSModelPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub recommended: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DLSSApplyResult {
    pub success: bool,
    pub message: String,
    pub preset: String,
    pub quality: String,
    pub texture_quality: String,
    pub antialiasing: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DLSSPresetStatus {
    pub preset: String,
    pub quality: String,
    pub texture_quality: String,
    pub antialiasing: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct MapInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DLSSSettingsStatus {
    pub dlss_indicator_enabled: bool,
    pub dlss_lock_enabled: bool,
}

static DELTA_PASSWORD_CACHE: std::sync::Mutex<Option<(Vec<DeltaPasswordItem>, std::time::Instant)>> = std::sync::Mutex::new(None);

fn fetch_delta_passwords_from_primary_api() -> Option<Vec<DeltaPasswordItem>> {
    let url = "https://i.elaina.vin/api/%E4%B8%89%E8%A7%92%E6%B4%B2/%E5%AF%86%E7%A0%81/";
    
    let response = reqwest::blocking::Client::new()
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .ok()?
        .json::<DeltaPasswordResponse>()
        .ok()?;
    
    if response.status == "success" {
        Some(response.data)
    } else {
        None
    }
}

/// 备用接口：https://api.s0o1.com/API/sjz/mm/
/// 返回纯文本，格式形如：
///   1. 潮汐监狱
///   具体点位：监狱行政区1楼大厅楼梯拐角处
///   每日密码：0557
///   地点图片：https://...
/// 即 "1." 之后是地图名，"每日密码：" 之后是密码
fn parse_backup_password_text(text: &str) -> Vec<DeltaPasswordItem> {
    let map_re = regex::Regex::new(r"^\s*\d+\.\s*(.+?)\s*$").unwrap();
    let pw_re = regex::Regex::new(r"每日密码\s*[:：]\s*(\S+)").unwrap();

    let mut items: Vec<DeltaPasswordItem> = Vec::new();
    let mut current: Option<(String, String)> = None;

    for line in text.lines() {
        if let Some(caps) = map_re.captures(line) {
            // 上一组数据完整则先保存
            if let Some((name, pw)) = current.take() {
                if !pw.is_empty() {
                    items.push(DeltaPasswordItem { name, password: pw });
                }
            }
            current = Some((caps[1].to_string(), String::new()));
            continue;
        }
        if let Some(caps) = pw_re.captures(line) {
            if let Some((_, pw)) = current.as_mut() {
                if pw.is_empty() {
                    *pw = caps[1].to_string();
                }
            }
        }
    }

    if let Some((name, pw)) = current.take() {
        if !pw.is_empty() {
            items.push(DeltaPasswordItem { name, password: pw });
        }
    }

    items
}

fn fetch_delta_passwords_from_backup_api() -> Option<Vec<DeltaPasswordItem>> {
    let url = "https://api.s0o1.com/API/sjz/mm/";

    let text = reqwest::blocking::Client::new()
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .ok()?
        .text()
        .ok()?;

    let items = parse_backup_password_text(&text);
    if items.is_empty() { None } else { Some(items) }
}

/// 主接口优先，失败或数据为空时回退到备用接口
fn fetch_delta_passwords_from_api() -> Option<Vec<DeltaPasswordItem>> {
    if let Some(items) = fetch_delta_passwords_from_primary_api() {
        if !items.is_empty() {
            return Some(items);
        }
    }
    fetch_delta_passwords_from_backup_api()
}

#[tauri::command]
pub async fn get_delta_passwords() -> Result<Vec<DeltaPasswordItem>, String> {
    {
        let cache = DELTA_PASSWORD_CACHE.lock().unwrap();
        if let Some((cached_data, cached_time)) = cache.as_ref() {
            if cached_time.elapsed().as_secs() < 60 {
                return Ok(cached_data.clone());
            }
        }
    }
    
    let passwords = tokio::task::spawn_blocking(|| {
        fetch_delta_passwords_from_api()
    })
    .await
    .map_err(|e| format!("获取密码失败: {}", e))?
    .ok_or_else(|| "无法获取三角洲密码数据".to_string())?;
    
    {
        let mut cache = DELTA_PASSWORD_CACHE.lock().unwrap();
        *cache = Some((passwords.clone(), std::time::Instant::now()));
    }
    
    Ok(passwords)
}

fn get_cached_delta_password_items() -> Option<Vec<DeltaPasswordItem>> {
    {
        let cache = DELTA_PASSWORD_CACHE.lock().unwrap();
        if let Some((data, time)) = cache.as_ref() {
            if time.elapsed().as_secs() < 300 {
                return if data.is_empty() { None } else { Some(data.clone()) };
            }
        }
    }

    match fetch_delta_passwords_from_api() {
        Some(passwords) => {
            if passwords.is_empty() {
                return None;
            }
            let mut cache = DELTA_PASSWORD_CACHE.lock().unwrap();
            *cache = Some((passwords.clone(), std::time::Instant::now()));
            Some(passwords)
        }
        None => None,
    }
}

pub fn get_cached_delta_password_filtered(selected_maps: &[String]) -> Option<String> {
    let items = get_cached_delta_password_items()?;
    let filtered: Vec<_> = if selected_maps.is_empty() {
        items
    } else {
        items.into_iter().filter(|item| selected_maps.contains(&item.name)).collect()
    };
    if filtered.is_empty() {
        return None;
    }
    let joined = filtered
        .iter()
        .map(|item| format!("{}：{}", item.name, item.password))
        .collect::<Vec<_>>()
        .join("  ");
    Some(joined)
}

pub fn get_cached_delta_password() -> Option<String> {
    get_cached_delta_password_filtered(&[])
}

#[tauri::command]
pub async fn get_weapon_codes() -> Result<Vec<WeaponCode>, String> {
    Ok(vec![
        WeaponCode {
            id: "1".to_string(),
            name: "M4A1 竞技配置".to_string(),
            code: "DELTA-M4A1-001".to_string(),
            category: "突击步枪".to_string(),
            description: "高稳定性竞技配置".to_string(),
        },
        WeaponCode {
            id: "2".to_string(),
            name: "AK47 压枪配置".to_string(),
            code: "DELTA-AK47-001".to_string(),
            category: "突击步枪".to_string(),
            description: "低后座力压枪配置".to_string(),
        },
        WeaponCode {
            id: "3".to_string(),
            name: "AWM 狙击配置".to_string(),
            code: "DELTA-AWM-001".to_string(),
            category: "狙击枪".to_string(),
            description: "精准狙击配置".to_string(),
        },
        WeaponCode {
            id: "4".to_string(),
            name: "MP5 冲锋配置".to_string(),
            code: "DELTA-MP5-001".to_string(),
            category: "冲锋枪".to_string(),
            description: "近距离快速射击".to_string(),
        },
    ])
}

#[tauri::command]
pub async fn get_dlss_model_presets() -> Result<Vec<DLSSModelPreset>, String> {
    Ok(vec![
        DLSSModelPreset { id: "A".to_string(), name: "Preset A".to_string(), description: "早期模型".to_string(), recommended: false },
        DLSSModelPreset { id: "B".to_string(), name: "Preset B".to_string(), description: "早期模型".to_string(), recommended: false },
        DLSSModelPreset { id: "C".to_string(), name: "Preset C".to_string(), description: "早期模型".to_string(), recommended: false },
        DLSSModelPreset { id: "D".to_string(), name: "Preset D".to_string(), description: "稳定模型".to_string(), recommended: false },
        DLSSModelPreset { id: "E".to_string(), name: "Preset E".to_string(), description: "实验性模型".to_string(), recommended: false },
        DLSSModelPreset { id: "F".to_string(), name: "Preset F".to_string(), description: "改进模型".to_string(), recommended: false },
        DLSSModelPreset { id: "G".to_string(), name: "Preset G".to_string(), description: "改进模型".to_string(), recommended: false },
        DLSSModelPreset { id: "J".to_string(), name: "Preset J".to_string(), description: "较新模型，画质优先".to_string(), recommended: false },
        DLSSModelPreset { id: "K".to_string(), name: "Preset K".to_string(), description: "推荐模型，大多数DLSS模式".to_string(), recommended: true },
        DLSSModelPreset { id: "L".to_string(), name: "Preset L".to_string(), description: "优化Ultra Performance模式".to_string(), recommended: true },
        DLSSModelPreset { id: "M".to_string(), name: "Preset M".to_string(), description: "优化Performance模式".to_string(), recommended: true },
    ])
}

fn get_npi_path() -> Result<PathBuf, String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("获取程序路径失败: {}", e))?;
    let parent_dir = exe_dir.parent().ok_or("无法获取父目录")?;
    
    let candidates = [
        parent_dir.join("nvidiaProfileInspector.exe"),
        parent_dir.join("_up_").join("nvidiaProfileInspector.exe"),
        parent_dir.join("resources").join("nvidiaProfileInspector.exe"),
    ];
    
    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }
    
    Err("未找到NVIDIA Profile Inspector，请确保已安装或将其放在程序目录下".to_string())
}

fn make_profile_setting(name: &str, setting_id: u32, value: u32) -> String {
    format!(
        r#"      <ProfileSetting>
        <SettingNameInfo>{name}</SettingNameInfo>
        <SettingID>{id}</SettingID>
        <SettingValue>{val}</SettingValue>
        <ValueType>Dword</ValueType>
      </ProfileSetting>"#,
        name = name,
        id = setting_id,
        val = value,
    )
}

fn generate_nip_config(preset: &str, quality_level: &str, texture_quality: &str, antialiasing: &str) -> Vec<u8> {
    // default: 由3D程序设置，不强制覆盖 DLSS-SR 预设
    let is_default = preset.eq_ignore_ascii_case("default");
    let preset_value: u32 = if is_default {
        0
    } else {
        match preset.to_uppercase().as_str() {
            "A" => 1, "B" => 2, "C" => 3, "D" => 4, "E" => 5, "F" => 6, "G" => 7,
            "J" => 10, "K" => 11, "L" => 12, "M" => 13,
            _ => 11,
        }
    };

    let mut settings = vec![
        make_profile_setting("Vertical Sync Tear Control", 5912412, 2525368439),
        make_profile_setting("DLSS Model Preset Profile", 6505105, 2),
        make_profile_setting("Enable DeepDVC Feature", 9963648, 0),
        make_profile_setting("Vertical Sync", 11041231, 1620202130),
        make_profile_setting("Saturation value for DeepDVC", 11250451, 50),
        make_profile_setting("Intensity value for DeepDVC", 11250466, 50),
        make_profile_setting("Flag to control smooth AFR behavior", 270198627, 0),
        make_profile_setting("Override DLSSG mode", 271614616, 1),
        make_profile_setting("Override DLSSG multi-frame count", 273507943, 0),
        make_profile_setting("Override maximum DLSSG dynamic multi frame count", 274083087, 0),
        make_profile_setting("VRR requested state", 278196727, 0),
    ];

    // DLSS 质量级别覆盖 (非 default 时添加)
    if quality_level != "default" {
        let (forced_quality, forced_scaling) = match quality_level {
            "ultra_performance" => (5u32, 0x21u32),
            "performance"       => (0u32, 0x32u32),
            "balanced"          => (1u32, 0x3Au32),
            "quality"           => (2u32, 0x42u32),
            "dlaa"              => (4u32, 0x64u32),
            _ => (0u32, 0u32),
        };
        settings.push(make_profile_setting("DLSS - Forced Quality Level", 279951208, forced_quality));
        settings.push(make_profile_setting("DLSS - Forced Scaling Ratio", 283385333, forced_scaling));
    }

    // 纹理过滤质量 (SettingID: 0x00CE2751 = 13510289)
    if texture_quality != "default" {
        let val = match texture_quality {
            "high_quality"    => 0xFFFFFFF6u32, // High Quality
            "quality"         => 0x00000000u32, // Quality
            "performance"     => 0x0000000Au32, // Performance
            "high_performance" => 0x00000014u32, // High Performance
            _ => 0,
        };
        if val != 0 || texture_quality == "quality" {
            settings.push(make_profile_setting("Texture filtering - Quality", 13510289, val));
        }
    }

    // 抗锯齿 - 透明度超采样 (SettingID: 0x10D48A85 = 282364549)
    if antialiasing != "default" {
        let val = match antialiasing {
            "off"     => 0x00000000u32, // Off
            "2x"      => 0x00000014u32, // 2x Supersampling
            "4x"      => 0x00000024u32, // 4x Supersampling
            "8x"      => 0x00000034u32, // 8x Supersampling
            _ => 0,
        };
        if val != 0 || antialiasing == "off" {
            settings.push(make_profile_setting("Antialiasing - Transparency Supersampling", 282364549, val));
        }
    }

    // DLSS 相关（始终包含）
    settings.push(make_profile_setting("Override DLSSG Target Frame Rate", 282018085, 0));
    settings.push(make_profile_setting("Override DLSS-FG preset", 283385329, 0));
    settings.push(make_profile_setting("Override DLSS-SR presets", 283385331, preset_value));
    settings.push(make_profile_setting("Enable DLSS-SR override", 283385345, if is_default { 0 } else { 1 }));
    settings.push(make_profile_setting("Enable DLSS-FG override", 283385347, 0));

    let settings_xml = settings.join("\n");

    let xml_content = format!(
        r#"<?xml version="1.0" encoding="utf-16"?>
<ArrayOfProfile>
  <Profile>
    <ProfileName>Delta Force</ProfileName>
    <Executeables>
      <string>deltaforceclient-win64-shipping.exe</string>
    </Executeables>
    <Settings>
{}
    </Settings>
  </Profile>
</ArrayOfProfile>"#,
        settings_xml,
    );

    // UTF-16LE BOM
    let mut bytes = vec![0xFF, 0xFE];
    for c in xml_content.encode_utf16() {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    bytes
}

#[tauri::command]
pub async fn apply_dlss_model_preset(preset: String, quality: String, texture_quality: String, antialiasing: String) -> Result<DLSSApplyResult, String> {
    let npi_path = get_npi_path()?;

    let config_content = generate_nip_config(&preset, &quality, &texture_quality, &antialiasing);

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("delta_force_dlss.nip");
    fs::write(&temp_path, &config_content)
        .map_err(|e| format!("写入配置文件失败: {}", e))?;
    
    let npi_str = npi_path.to_str().ok_or("路径编码错误")?;
    let temp_str = temp_path.to_str().ok_or("临时路径编码错误")?;
    
    let ps_command = format!(
        "Start-Process -FilePath '{}' -ArgumentList '-silentImport','\"{}\"' -Verb RunAs -Wait",
        npi_str.replace('\'', "''"),
        temp_str.replace('\'', "''")
    );
    
    let output = Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps_command])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行失败: {}", e))?;
    
    let _ = fs::remove_file(&temp_path);
    
    if output.status.success() {
        // 保存当前应用的预设状态，供前端查询
        let status = DLSSPresetStatus {
            preset: preset.clone(),
            quality: quality.clone(),
            texture_quality: texture_quality.clone(),
            antialiasing: antialiasing.clone(),
        };
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent_dir) = exe_path.parent() {
                let status_path = parent_dir.join("delta_force_dlss_status.json");
                let _ = fs::write(&status_path, serde_json::to_string(&status).unwrap_or_default());
            }
        }

        let mut parts = vec![format!(
            "DLSS预设: {}",
            if preset.eq_ignore_ascii_case("default") { "默认（由3D程序设置）" } else { &preset }
        )];
        if quality != "default" { parts.push(format!("质量: {}", quality)); }
        if texture_quality != "default" { parts.push(format!("纹理: {}", texture_quality)); }
        if antialiasing != "default" { parts.push(format!("抗锯齿: {}", antialiasing)); }

        Ok(DLSSApplyResult {
            success: true,
            message: parts.join(" / "),
            preset,
            quality,
            texture_quality,
            antialiasing,
        })
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("应用失败: {}", stderr))
    }
}

#[tauri::command]
pub async fn get_delta_maps() -> Result<Vec<MapInfo>, String> {
    Ok(vec![
        MapInfo {
            id: "1".to_string(),
            name: "长弓溪谷".to_string(),
            url: "https://df.qq.com/cp/a20241029map/index.html".to_string(),
            description: "大型开放地图".to_string(),
        },
        MapInfo {
            id: "2".to_string(),
            name: "零号大坝".to_string(),
            url: "https://df.qq.com/cp/a20241029map/index.html".to_string(),
            description: "中型战术地图".to_string(),
        },
        MapInfo {
            id: "3".to_string(),
            name: "巴克什".to_string(),
            url: "https://df.qq.com/cp/a20241029map/index.html".to_string(),
            description: "城市战斗地图".to_string(),
        },
    ])
}

fn get_dlss_indicator_registry_value() -> bool {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let path = r"SOFTWARE\NVIDIA Corporation\Global\NGXCore";
    if let Ok(key) = hklm.open_subkey(path) {
        if let Ok(value) = key.get_value::<u32, _>("ShowDlssIndicator") {
            return value != 0;
        }
    }
    false
}

#[tauri::command]
pub async fn toggle_dlss_indicator(enable: bool) -> Result<bool, String> {
    let value = if enable { 1024 } else { 0 };

    let script_content = format!(
        "$p='HKLM:\\SOFTWARE\\NVIDIA Corporation\\Global\\NGXCore'; \
         if(-not(Test-Path $p)){{New-Item -Path $p -Force|Out-Null}}; \
         Set-ItemProperty -Path $p -Name 'ShowDlssIndicator' -Value {} -Type DWord",
        value
    );

    let temp_dir = std::env::temp_dir();
    let temp_script = temp_dir.join("dlss_indicator.ps1");
    fs::write(&temp_script, &script_content)
        .map_err(|e| format!("写入临时脚本失败: {}", e))?;

    let script_path = temp_script.to_str().ok_or("路径编码错误")?;

    let ps_command = format!(
        "Start-Process -FilePath 'powershell' -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"' -Verb RunAs -Wait -WindowStyle Hidden",
        script_path.replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps_command])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行失败: {}", e))?;

    let _ = fs::remove_file(&temp_script);

    // 验证注册表值是否已更新（防止Start-Process返回成功但实际写入失败的情况）
    let actual = get_dlss_indicator_registry_value();
    if actual != enable {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.to_lowercase().contains("cancel") || stderr.to_lowercase().contains("denied") {
            return Err("管理员授权被取消，DLSS指示器设置未生效".to_string());
        }
        return Err("设置DLSS指示器失败: 注册表值未能更新，请确认已授予管理员权限".to_string());
    }

    Ok(enable)
}

#[tauri::command]
pub async fn toggle_dlss_lock(enable: bool) -> Result<bool, String> {
    let npi_path = get_npi_path()?;

    let lock_config = generate_dlss_lock_config(enable);

    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join("delta_force_dlss_lock.nip");
    fs::write(&temp_path, &lock_config)
        .map_err(|e| format!("写入锁定配置文件失败: {}", e))?;

    let npi_str = npi_path.to_str().ok_or("路径编码错误")?;
    let temp_str = temp_path.to_str().ok_or("临时路径编码错误")?;

    let ps_command = format!(
        "Start-Process -FilePath '{}' -ArgumentList '-silentImport','\"{}\"' -Verb RunAs -Wait",
        npi_str.replace('\'', "''"),
        temp_str.replace('\'', "''")
    );

    let output = Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps_command])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行失败: {}", e))?;

    let _ = fs::remove_file(&temp_path);

    if output.status.success() {
        Ok(enable)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("DLSS锁定操作失败: {}", stderr))
    }
}

fn generate_dlss_lock_config(lock_enabled: bool) -> Vec<u8> {
    let lock_value = if lock_enabled { "1" } else { "0" };

    let xml_content = format!(
        r#"<?xml version="1.0" encoding="utf-16"?>
<ArrayOfProfile>
  <Profile>
    <ProfileName>Delta Force DLSS Lock</ProfileName>
    <Executeables>
      <string>deltaforceclient-win64-shipping.exe</string>
    </Executeables>
    <Settings>
      <ProfileSetting>
        <SettingNameInfo />
        <SettingID>275602687</SettingID>
        <SettingValue>{}</SettingValue>
        <ValueType>Dword</ValueType>
      </ProfileSetting>
      <ProfileSetting>
        <SettingNameInfo>Override DLSS-SR presets</SettingNameInfo>
        <SettingID>283385331</SettingID>
        <SettingValue>11</SettingValue>
        <ValueType>Dword</ValueType>
      </ProfileSetting>
      <ProfileSetting>
        <SettingNameInfo>Enable DLSS-SR override</SettingNameInfo>
        <SettingID>283385345</SettingID>
        <SettingValue>1</SettingValue>
        <ValueType>Dword</ValueType>
      </ProfileSetting>
    </Settings>
  </Profile>
</ArrayOfProfile>"#,
        lock_value
    );

    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    bytes.extend(
        xml_content.encode_utf16().collect::<Vec<u16>>()
            .iter()
            .flat_map(|&c| c.to_le_bytes())
    );
    bytes
}

#[tauri::command]
pub async fn get_dlss_settings_status() -> Result<DLSSSettingsStatus, String> {
    let dlss_indicator_enabled = get_dlss_indicator_registry_value();

    let dlss_lock_enabled = false;

    Ok(DLSSSettingsStatus {
        dlss_indicator_enabled,
        dlss_lock_enabled,
    })
}

#[tauri::command]
pub async fn get_dlss_preset_status() -> Result<DLSSPresetStatus, String> {
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("获取程序路径失败: {}", e))?;
    let parent_dir = exe_path.parent().ok_or("无法获取父目录")?;
    let status_path = parent_dir.join("delta_force_dlss_status.json");

    if status_path.exists() {
        let content = fs::read_to_string(&status_path)
            .map_err(|e| format!("读取状态文件失败: {}", e))?;
        if let Ok(status) = serde_json::from_str::<DLSSPresetStatus>(&content) {
            return Ok(status);
        }
    }

    Ok(DLSSPresetStatus { preset: "default".to_string(), quality: "default".to_string(), texture_quality: "default".to_string(), antialiasing: "default".to_string() })
}

/// 在独立 WebView 窗口中打开外部平台链接，iframe 内嵌支持完整跳转
#[tauri::command]
pub async fn open_platform_window(
    app: tauri::AppHandle,
    url: String,
    title: String,
    label: String,
) -> Result<(), String> {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;
    use tauri::Manager;

    // 目标 URL 编码后通过 hash fragment 传入 platform-viewer.html
    let encoded = urlencoding::encode(&url);
    let app_path = format!("platform-viewer.html#{}", encoded);

    let app_clone = app.clone();
    let label_clone = label.clone();

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(app_path.into()))
        .title(&title)
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required")
        .inner_size(1200.0, 800.0)
        .resizable(true)
        .center()
        // 拦截 iframe 内 target="_blank" / window.open() 的跳转
        // 改为在 iframe 内导航，不弹出新窗口
        .on_new_window(move |new_url, _features| {
            if let Some(window) = app_clone.get_webview_window(&label_clone) {
                let safe = new_url.as_str().replace('\\', "\\\\").replace('\'', "\\'");
                let _ = window.eval(&format!(
                    "var f=document.getElementById('viewer');if(f)f.src='{safe}';"
                ));
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .build()
        .map_err(|e| format!("创建窗口失败: {}", e))?;

    Ok(())
}

/// 下载远程图片到应用缓存目录（本地缓存），返回本地文件路径（前端用 convertFileSrc 使用）。
/// 复用 QQ 群图标的缓存范式：按 URL hash 存文件，二次访问走本地缓存。
#[tauri::command]
pub async fn cache_delta_image(app: tauri::AppHandle, url: String) -> Result<String, String> {
    if url.trim().is_empty() {
        return Ok(String::new());
    }

    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("roulette_icons");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let file = dir.join(format!("{}.img", hasher.finish()));

    if !file.exists() {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 NexBox")
            .build()
            .map_err(|e| format!("client error: {e}"))?;

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("image http {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    }

    Ok(file.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_utf16_le(bytes: &[u8]) -> String {
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            // skip BOM
            let rest = &bytes[2..];
            let mut u16s = Vec::with_capacity(rest.len() / 2);
            for chunk in rest.chunks(2) {
                if chunk.len() == 2 {
                    let lo = chunk[0] as u16;
                    let hi = chunk[1] as u16;
                    u16s.push(lo | (hi << 8));
                }
            }
            String::from_utf16(&u16s).unwrap_or_default()
        } else {
            String::from_utf8(bytes.to_vec()).unwrap_or_default()
        }
    }


}
