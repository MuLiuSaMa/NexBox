// WMI COM 直调模块 — 替代 PowerShell，通过 IWbemServices 查询 WMI
// 基于 wmi crate（底层即 windows crate 的 COM 调用）

use std::collections::HashMap;
use wmi::{COMLibrary, WMIConnection};
pub use wmi::Variant;

fn create_connection() -> Result<WMIConnection, String> {
    let com_con =
        COMLibrary::new().map_err(|e| format!("COM初始化失败: {}", e))?;
    WMIConnection::new(com_con.into()).map_err(|e| format!("WMI连接失败: {}", e))
}

/// 执行 WQL 查询，返回行列表（每行是 属性名 → Variant）
pub fn wmi_query(wql: &str) -> Result<Vec<HashMap<String, Variant>>, String> {
    let con = create_connection()?;
    con.raw_query(wql)
        .map_err(|e| format!("WMI查询失败: {}", e))
}

/// 在指定命名空间执行 WQL 查询（如 ROOT\WMI 读取 SMART），返回行列表
pub fn wmi_query_ns(namespace: &str, wql: &str) -> Result<Vec<HashMap<String, Variant>>, String> {
    let com_con = COMLibrary::new().map_err(|e| format!("COM初始化失败: {}", e))?;
    let con = WMIConnection::with_namespace_path(namespace, com_con.into())
        .map_err(|e| format!("WMI连接失败({}): {}", namespace, e))?;
    con.raw_query(wql)
        .map_err(|e| format!("WMI查询失败({}): {}", namespace, e))
}

// ─── Variant 提取辅助函数 ───

/// 提取为字符串（null/empty 返回 None）
pub fn v_str(v: &Variant) -> Option<String> {
    match v {
        Variant::String(s) => {
            let s = s.trim();
            if s.is_empty() { None } else { Some(s.to_string()) }
        }
        Variant::UI1(n) => Some(n.to_string()),
        Variant::UI2(n) => Some(n.to_string()),
        Variant::UI4(n) => Some(n.to_string()),
        Variant::UI8(n) => Some(n.to_string()),
        Variant::I1(n) => Some(n.to_string()),
        Variant::I2(n) => Some(n.to_string()),
        Variant::I4(n) => Some(n.to_string()),
        Variant::I8(n) => Some(n.to_string()),
        Variant::R4(n) => Some(format!("{:.1}", n)),
        Variant::R8(n) => Some(format!("{}", n)),
        Variant::Bool(b) => Some(b.to_string()),
        Variant::Empty | Variant::Null => None,
        _ => None,
    }
}

pub fn v_u16(v: &Variant) -> Option<u16> {
    match v {
        Variant::UI1(n) => Some(*n as u16),
        Variant::UI2(n) => Some(*n),
        Variant::UI4(n) => Some(*n as u16),
        Variant::UI8(n) => Some(*n as u16),
        Variant::I1(n) => Some(*n as u16),
        Variant::I2(n) => Some(*n as u16),
        Variant::I4(n) => Some(*n as u16),
        Variant::String(s) => s.trim().parse().ok(),
        Variant::Empty | Variant::Null => None,
        _ => None,
    }
}

pub fn v_u32(v: &Variant) -> Option<u32> {
    match v {
        Variant::UI2(n) => Some(*n as u32),
        Variant::UI4(n) => Some(*n),
        Variant::UI8(n) => Some(*n as u32),
        Variant::I4(n) => Some(*n as u32),
        Variant::String(s) => s.trim().parse().ok(),
        Variant::Empty | Variant::Null => None,
        _ => None,
    }
}

pub fn v_u64(v: &Variant) -> Option<u64> {
    match v {
        Variant::UI4(n) => Some(*n as u64),
        Variant::UI8(n) => Some(*n),
        Variant::I4(n) => Some(*n as u64),
        Variant::I8(n) => Some(*n as u64),
        Variant::String(s) => s.trim().parse().ok(),
        Variant::Empty | Variant::Null => None,
        _ => None,
    }
}

/// 判断 Variant 是否非空且非空字符串
pub fn v_u16_arr(v: &Variant) -> Vec<u16> {
    match v {
        Variant::String(s) => s
            .split(',')
            .filter_map(|p| p.trim().parse::<u16>().ok())
            .collect(),
        Variant::UI1(n) => vec![*n as u16],
        Variant::UI2(n) => vec![*n],
        Variant::UI4(n) => vec![*n as u16],
        Variant::Array(arr) => arr.iter().filter_map(|e| v_u16(e)).collect(),
        _ => vec![],
    }
}
