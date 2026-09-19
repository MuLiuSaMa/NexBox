use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// ===== 时间校准（NTP v4 客户端，纯 UDP 实现，不新增依赖） =====
/// 提供公共 NTP 服务器列表、延迟/偏差检测与系统时间校准（需管理员权限）。

struct ServerConfig {
    id: &'static str,
    name: &'static str,
    host: &'static str,
}

const SERVERS: &[ServerConfig] = &[
    ServerConfig { id: "aliyun", name: "阿里云公共 NTP", host: "ntp.aliyun.com" },
    ServerConfig { id: "neu", name: "东北大学", host: "ntp.neu.edu.cn" },
    ServerConfig { id: "tencent", name: "腾讯公共 NTP", host: "ntp.tencent.com" },
    ServerConfig { id: "pool", name: "全球 NTP 池", host: "pool.ntp.org" },
    ServerConfig { id: "apple", name: "苹果", host: "time.apple.com" },
    ServerConfig { id: "microsoft", name: "微软 (Windows 默认)", host: "time.windows.com" },
];

fn find_server(id: &str) -> Option<&'static ServerConfig> {
    SERVERS.iter().find(|s| s.id == id)
}

/// NTP 请求/响应超时（1s 级实时轮询场景下尽量短，慢服务器留给下一轮重试）
const NTP_TIMEOUT: Duration = Duration::from_secs(2);
/// 单台服务器最大尝试次数（每 1s 轮询一次，单轮内不重试，失败自动进入下一轮）
const NTP_RETRIES: u32 = 1;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtpServerInfo {
    pub id: String,
    pub name: String,
    pub host: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtpQueryResult {
    /// 本机时间（Unix 毫秒，发送后收到响应时刻）
    pub local_time_ms: i64,
    /// 服务器时间（Unix 毫秒，取响应中的 transmit 时间戳）
    pub server_time_ms: i64,
    /// 时间偏差（毫秒）= 服务器时间 - 本机时间，正数表示本机时间偏慢
    pub offset_ms: i64,
    /// 网络往返延迟（毫秒）
    pub latency_ms: f64,
    /// 是否查询成功
    pub ok: bool,
}

/// 当前 Unix 时间（秒，浮点保留小数）
fn now_unix_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// NTP 时间戳（1900 年起秒 + 分数）转 Unix 秒
fn ntp_to_unix(seconds: u32, fraction: u32) -> f64 {
    (seconds as f64 - 2_208_988_800.0) + fraction as f64 / 4_294_967_296.0
}

fn be_u32(buf: &[u8]) -> u32 {
    u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
}

/// 构造 NTP v4 客户端请求包：写入 transmit 时间戳作为 originate，
/// 部分服务器（如 ntp.tencent.com）会拒绝 transmit 全零的请求。
fn make_request_packet() -> [u8; 48] {
    let mut req = [0u8; 48];
    req[0] = 0x23; // LI=0, VN=4, Mode=3(Client)
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = (now.as_secs() + 2_208_988_800) as u32; // Unix → NTP 纪元（1900 年）
    let frac = now.subsec_nanos() as u32;
    req[40..44].copy_from_slice(&secs.to_be_bytes());
    req[44..48].copy_from_slice(&frac.to_be_bytes());
    req
}

/// 校验 NTP 响应：服务器模式（mode=4）、非 Kiss-o'-Death（stratum 非 0）、transmit 时间戳非零
fn is_valid_response(buf: &[u8]) -> bool {
    if buf.len() < 48 {
        return false;
    }
    if buf[0] & 0x07 != 4 {
        return false; // 非服务器模式响应
    }
    if buf[1] == 0 {
        return false; // stratum 0 = Kiss-o'-Death（拒绝/限速）
    }
    be_u32(&buf[40..44]) != 0
}

/// 查询指定 NTP 服务器，返回 (offset_sec, delay_sec, server_time_sec)
async fn query_ntp(host: &str) -> Result<(f64, f64, f64), String> {
    // 解析目标地址：优先 IPv4（多数 NTP 服务器仅 A 记录，且 UDP 套接字按 IPv4 绑定）
    let mut addrs: Vec<std::net::SocketAddr> = Vec::new();
    let lookup = tokio::net::lookup_host((host, 123))
        .await
        .map_err(|_| "域名解析失败".to_string())?;
    for addr in lookup {
        addrs.push(addr);
    }
    let mut ipv4: Option<std::net::SocketAddr> = None;
    let mut ipv6: Option<std::net::SocketAddr> = None;
    for a in addrs {
        if ipv4.is_none() && a.is_ipv4() {
            ipv4 = Some(a);
        }
        if ipv6.is_none() && a.is_ipv6() {
            ipv6 = Some(a);
        }
        if ipv4.is_some() && ipv6.is_some() {
            break;
        }
    }
    let target = ipv4.or(ipv6).ok_or_else(|| "无有效地址".to_string())?;

    let mut last_err = "请求失败".to_string();
    for _ in 0..NTP_RETRIES {
        let sock = match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(_) => {
                last_err = "套接字错误".to_string();
                continue;
            }
        };
        let req = make_request_packet();
        let t0 = now_unix_f64();
        if let Err(_e) = sock.send_to(&req, target).await {
            last_err = "连接失败".to_string();
            continue;
        }
        let mut buf = [0u8; 48];
        let recv = tokio::time::timeout(NTP_TIMEOUT, sock.recv_from(&mut buf)).await;
        match recv {
            Ok(Ok((_n, _from))) => {
                let t3 = now_unix_f64();
                if !is_valid_response(&buf) {
                    last_err = "服务器响应无效".to_string();
                    continue;
                }
                // 解析 receive（T1，字节 32-39）与 transmit（T2，字节 40-47）时间戳
                let t1 = ntp_to_unix(be_u32(&buf[32..36]), be_u32(&buf[36..40]));
                let t2 = ntp_to_unix(be_u32(&buf[40..44]), be_u32(&buf[44..48]));

                // 偏差 = ((T1 - T0) + (T2 - T3)) / 2；往返延迟 = (T3 - T0) - (T2 - T1)
                let offset = ((t1 - t0) + (t2 - t3)) / 2.0;
                let delay = ((t3 - t0) - (t2 - t1)).max(0.0);

                return Ok((offset, delay, t2));
            }
            Ok(Err(_)) => {
                last_err = "接收响应失败".to_string();
            }
            Err(_) => {
                last_err = "请求超时".to_string();
            }
        }
    }
    Err(last_err)
}

/// 获取可选择的 NTP 时间服务器列表
#[tauri::command]
pub fn get_ntp_servers() -> Vec<NtpServerInfo> {
    SERVERS
        .iter()
        .map(|s| NtpServerInfo {
            id: s.id.to_string(),
            name: s.name.to_string(),
            host: s.host.to_string(),
        })
        .collect()
}

/// 检测指定时间服务器的延迟与时间偏差
#[tauri::command]
pub async fn ntp_query(server: String) -> Result<NtpQueryResult, String> {
    let cfg = find_server(&server)
        .ok_or_else(|| format!("未知的时间服务器: {}", server))?;

    let (offset, delay, server_time) = query_ntp(cfg.host).await?;
    let local_time = now_unix_f64();

    Ok(NtpQueryResult {
        local_time_ms: (local_time * 1000.0) as i64,
        server_time_ms: (server_time * 1000.0) as i64,
        offset_ms: (offset * 1000.0) as i64,
        latency_ms: delay * 1000.0,
        ok: true,
    })
}

/// 按已检测到的时间偏差校准系统时间（不重新联网，秒级完成）。
/// 偏差 = 服务器时间 - 本机时间，因此目标时间 = 当前本机时间 + 偏差。
/// 需要管理员权限。
#[tauri::command]
pub async fn apply_time_offset(offset_ms: i64) -> Result<(), String> {
    if !crate::optimization::is_admin() {
        return Err("需要管理员权限才能修改系统时间".to_string());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let target = now + offset_ms as f64 / 1000.0;
    set_system_time_utc(target).map_err(|e| format!("设置系统时间失败: {}", e))?;
    Ok(())
}

#[cfg(windows)]
fn set_system_time_utc(unix_secs: f64) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, SYSTEMTIME};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::System::SystemInformation::SetSystemTime;
    use chrono::{Datelike, Timelike};

    // 启用 SE_SYSTEMTIME_NAME 特权（管理员令牌下默认可能为禁用状态）
    fn enable_systemtime_privilege() -> bool {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            ) == 0
            {
                return false;
            }
            let name: Vec<u16> = "SeSystemtimePrivilege"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut luid = windows_sys::Win32::Foundation::LUID {
                LowPart: 0,
                HighPart: 0,
            };
            if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
                let _ = CloseHandle(token);
                return false;
            }
            let mut tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            let ok = AdjustTokenPrivileges(
                token,
                0,
                &mut tp,
                std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ) != 0;
            let _ = CloseHandle(token);
            ok
        }
    }

    // 构造 UTC SYSTEMTIME
    let secs = unix_secs as i64;
    let nanos = ((unix_secs.fract().abs()) * 1e9) as u32;
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nanos)
        .ok_or_else(|| "服务器时间无效".to_string())?;
    let st = SYSTEMTIME {
        wYear: dt.year() as u16,
        wMonth: dt.month() as u16,
        wDayOfWeek: 0, // SetSystemTime 忽略该字段
        wDay: dt.day() as u16,
        wHour: dt.hour() as u16,
        wMinute: dt.minute() as u16,
        wSecond: dt.second() as u16,
        wMilliseconds: (dt.nanosecond() / 1_000_000) as u16,
    };

    let _ = enable_systemtime_privilege();
    if unsafe { SetSystemTime(&st) } == 0 {
        return Err(format!("SetSystemTime 返回错误码 {}", unsafe { GetLastError() }));
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_system_time_utc(_unix_secs: f64) -> Result<(), String> {
    Err("当前平台不支持修改系统时间".to_string())
}
