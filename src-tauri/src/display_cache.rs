//! 显示器信息公共缓存
//!
//! 多个页面（滤镜、准心、分辨率管理）都需要获取显示器型号。
//! 通过直接读取注册表中的 EDID 数据并解析显示器名称，
//! 速度极快（纯内存/注册表操作，不启动任何进程）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 缓存有效期。
///
/// 显示器配置在一次使用会话中基本不会变化，60 秒足够安全；
/// 同时保证热插拔后一段时间能自动刷新。
const CACHE_TTL: Duration = Duration::from_secs(60);

struct EdidCache {
    fetched_at: Option<Instant>,
    /// (PNP ID, 显示器型号名称) 列表。PNP ID 用于按设备 ID 精确匹配，
    /// 避免不同 API 的枚举顺序不一致导致型号张冠李戴。
    entries: Vec<(String, String)>,
}

static EDID_CACHE: Mutex<EdidCache> = Mutex::new(EdidCache {
    fetched_at: None,
    entries: Vec::new(),
});

/// 从原始 (PNP ID, 名称) 条目构建型号名称列表（按名称去重，保留首次出现），
/// 与历史 `get_edid_monitor_names` 行为一致。
fn build_names(entries: &[(String, String)]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for (_, name) in entries {
        if seen.insert(name.clone()) {
            names.push(name.clone());
        }
    }
    names
}

/// 获取 EDID 显示器型号名称（带 TTL 缓存）。
///
/// 通过读注册表 HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\...\Device Parameters\EDID
/// 直接解析 EDID 中的 Monitor Name (Descriptor Tag 0xFC)，不启动任何外部进程。
pub fn get_edid_monitor_names() -> Vec<String> {
    // 先尝试读缓存
    {
        let lock = EDID_CACHE.lock().unwrap();
        if let Some(t) = lock.fetched_at {
            if t.elapsed() < CACHE_TTL {
                return build_names(&lock.entries);
            }
        }
    }

    // 缓存过期或不存在，重新查询
    log::info!("display_cache: EDID 缓存未命中，从注册表读取 EDID…");
    let entries = query_edid_via_registry();
    log::info!("display_cache: 注册表 EDID 查询完成，获取到 {} 个名称", entries.len());

    let mut lock = EDID_CACHE.lock().unwrap();
    lock.fetched_at = Some(Instant::now());
    lock.entries = entries;
    build_names(&lock.entries)
}

/// 获取 PNP ID -> 显示器型号名称 的映射（带 TTL 缓存）。
///
/// 用于按显示器的 PNP 设备 ID 精确匹配型号，避免 WMI 顺序与注册表 EDID
/// 顺序不一致时把 A 显示器的型号错配到 B 显示器（型号颠倒）的问题。
pub fn get_edid_monitor_names_by_pnpid() -> HashMap<String, String> {
    {
        let lock = EDID_CACHE.lock().unwrap();
        if let Some(t) = lock.fetched_at {
            if t.elapsed() < CACHE_TTL {
                return lock.entries.iter().cloned().collect();
            }
        }
    }
    // 触发一次查询（同时填充缓存）
    get_edid_monitor_names();
    let lock = EDID_CACHE.lock().unwrap();
    lock.entries.iter().cloned().collect()
}

/// 通过 EnumDisplayDevicesW 获取指定显示设备（如 "\\.\DISPLAY1"）的 PNP ID
/// （如 "DELA409"），用于按设备 ID 精确匹配 EDID 型号，
/// 避免不同 API 枚举顺序不一致导致型号张冠李戴。
#[cfg(target_os = "windows")]
pub fn get_pnp_id_for_device(device_name: &str) -> Option<String> {
    use std::mem;
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};

    unsafe {
        let device_name_wide: Vec<u16> = device_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut disp_device: DISPLAY_DEVICEW = mem::zeroed();
        disp_device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;

        if EnumDisplayDevicesW(device_name_wide.as_ptr(), 0, &mut disp_device, 0) != 0 {
            let len = disp_device
                .DeviceID
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(disp_device.DeviceID.len());
            if len > 0 {
                let device_id = String::from_utf16_lossy(&disp_device.DeviceID[..len]);
                // DeviceID 格式: "MONITOR\PNPID\..."，提取 PNPID
                let prefix = "MONITOR\\";
                if let Some(pnp_start) = device_id.find(prefix) {
                    let after_prefix = &device_id[pnp_start + prefix.len()..];
                    if let Some(backslash_pos) = after_prefix.find('\\') {
                        return Some(after_prefix[..backslash_pos].to_string());
                    }
                    // 没有反斜杠时取到末尾
                    if !after_prefix.is_empty() {
                        return Some(after_prefix.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub fn get_pnp_id_for_device(_device_name: &str) -> Option<String> {
    None
}

/// 强制刷新缓存（例如显示器配置变化时调用）。
#[allow(dead_code)]
pub fn invalidate() {
    let mut lock = EDID_CACHE.lock().unwrap();
    lock.fetched_at = None;
    lock.entries.clear();
}

/// 通过注册表枚举所有显示器的 EDID，解析出 Monitor Name。
///
/// 返回 (PNP ID, 显示器型号名称) 列表。
/// 路径: HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\<PNPID>\<InstanceID>\Device Parameters\EDID
#[cfg(target_os = "windows")]
fn query_edid_via_registry() -> Vec<(String, String)> {
    use winreg::enums::*;
    use winreg::RegKey;

    let enum_key = match RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Enum\DISPLAY",
            KEY_READ,
        ) {
        Ok(k) => k,
        Err(e) => {
            log::warn!("display_cache: 无法打开 DISPLAY 注册表项: {}", e);
            return Vec::new();
        }
    };

    let mut entries = Vec::new();

    // 遍历每个 PNP ID 子项（如 "DELA409", "SAM0F9E" 等）
    for pnp_result in enum_key.enum_keys() {
        let pnp_id = match pnp_result {
            Ok(id) => id,
            Err(_) => continue,
        };

        let instance_key = match enum_key.open_subkey_with_flags(&pnp_id, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };

        // 遍历每个实例子项
        for inst_result in instance_key.enum_keys() {
            let instance_id = match inst_result {
                Ok(id) => id,
                Err(_) => continue,
            };

            // 读取 EDID 数据
            let edid_path = format!(r"{}\Device Parameters", instance_id);
            let dev_params = match instance_key.open_subkey_with_flags(&edid_path, KEY_READ) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let edid_bytes = match dev_params.get_raw_value("EDID") {
                Ok(raw) => raw.bytes,
                Err(_) => continue,
            };

            if let Some(name) = parse_edid_monitor_name(&edid_bytes) {
                entries.push((pnp_id.clone(), name));
            }
        }
    }

    log::info!("display_cache: 注册表 EDID 扫描完成，找到 {} 个显示器", entries.len());
    entries
}

#[cfg(not(target_os = "windows"))]
fn query_edid_via_registry() -> Vec<(String, String)> {
    Vec::new()
}

/// 从 EDID 原始二进制数据中解析 Monitor Name。
///
/// EDID 128 字节块中包含 4 个描述符块（每个 18 字节），
/// 起始偏移分别为 0x36, 0x48, 0x5A, 0x6C。
/// 描述符 Tag 0xFC 表示 Monitor Name。
/// 名称最多 13 个 ASCII 字符，以换行符 (0x0A) 或空格填充结尾。
fn parse_edid_monitor_name(edid: &[u8]) -> Option<String> {
    if edid.len() < 128 {
        return None;
    }

    // 检查 EDID 头标识
    if edid[0] != 0x00 || edid[1] != 0xFF || edid[2] != 0xFF || edid[3] != 0xFF
        || edid[4] != 0xFF || edid[5] != 0xFF || edid[6] != 0xFF || edid[7] != 0x00
    {
        log::warn!("display_cache: EDID 头校验失败，跳过");
        return None;
    }

    // 4 个描述符块，每个 18 字节
    for block_idx in 0..4 {
        let offset = 0x36 + block_idx * 18;
        if offset + 18 > edid.len() {
            break;
        }

        // 描述符前 2 个字节通常为 0x00, 0x00
        // 第 3 字节是 Tag
        let tag = edid[offset + 3];

        // Tag 0xFC = Monitor Name
        if tag != 0xFC {
            continue;
        }

        // 第 5 字节开始是名称数据（最多 13 字节）
        let name_start = offset + 5;
        let name_end = (name_start + 13).min(edid.len());

        let name_bytes = &edid[name_start..name_end];

        // 提取名称：遇到 0x0A（换行符）或 0x20（空格填充）截断
        let name: String = name_bytes
            .iter()
            .take_while(|&&b| b != 0x0A && b != 0x00)
            .map(|&b| b as char)
            .collect();

        let trimmed = name.trim().to_string();
        if !trimmed.is_empty() {
            log::info!(
                "display_cache: EDID 解析到 Monitor Name: '{}' (block={})",
                trimmed, block_idx
            );
            return Some(trimmed);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edid_parse_real_data() {
        // 构造一个包含 Monitor Name 描述符的最小 EDID
        let mut edid = vec![0u8; 128];
        // EDID 头
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // 第一个描述符块 (offset 0x36): Monitor Name (tag 0xFC)
        // 18 字节描述符布局: [0..2]=0x0000 标志, [2]=保留, [3]=tag, [4]=保留,
        // [5..18]=13 字节名称。名称必须从 offset+5 开始，写进 [3] 会覆盖 tag。
        edid[0x36 + 3] = 0xFC;
        let name_bytes = b"DELL S2721QS";
        let name_len = name_bytes.len().min(13);
        edid[0x36 + 5..0x36 + 5 + name_len].copy_from_slice(&name_bytes[..name_len]);

        let result = parse_edid_monitor_name(&edid);
        assert_eq!(result, Some("DELL S2721QS".to_string()));
    }

    #[test]
    fn test_edid_parse_no_name() {
        let mut edid = vec![0u8; 128];
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // 所有描述符都是其他类型 (tag != 0xFC)
        edid[0x36 + 3] = 0xFF;
        edid[0x48 + 3] = 0xFD;
        edid[0x5A + 3] = 0xFC; // Monitor name but empty
        edid[0x6C + 3] = 0x10;

        let result = parse_edid_monitor_name(&edid);
        assert_eq!(result, None);
    }
}
