use std::os::windows::process::CommandExt;
use std::process::Command;
use std::{env, path::Path};
use crate::optimization::{run_simple_feature, PerfTweakResult, CREATE_NO_WINDOW};
use encoding_rs::GBK;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

fn get_powershell_path() -> String {
    if let Ok(sysroot) = env::var("SystemRoot") {
        let ps_path = format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", sysroot);
        if Path::new(&ps_path).exists() {
            return ps_path;
        }
    }
    "powershell.exe".to_string()
}

/// 原生执行 netsh 命令，返回解码后的输出；失败时检查权限错误
fn run_netsh_result(args: &[&str]) -> Result<String, String> {
    let out = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 netsh 失败: {}", e))?;
    let text = if !out.stdout.is_empty() {
        decode_console(out.stdout)
    } else {
        decode_console(out.stderr)
    };
    if out.status.success() {
        Ok(text)
    } else {
        let lower = text.to_lowercase();
        if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
            Err("需要管理员权限，请以管理员身份运行 NexBox".to_string())
        } else {
            Err(format!("命令执行失败: {}", text.trim()))
        }
    }
}

/// 网卡设备注册表类键（用于禁用/恢复网卡省电）
const NIC_CLASS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";

/// Nagle：原生注册表写入，对每个有 IP 的接口设置低延迟参数
fn set_nagle_native() -> Result<(), String> {
    let params = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters", KEY_SET_VALUE)
        .map_err(|e| format!("打开 Tcpip 参数键失败: {}", e))?;
    params
        .set_value("TcpAckFrequency", &1u32)
        .map_err(|e| format!("写入 TcpAckFrequency 失败: {}", e))?;

    let ifaces = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces", KEY_READ)
        .map_err(|e| format!("打开 Tcpip 接口键失败: {}", e))?;
    for name in ifaces.enum_keys().flatten() {
        if let Ok(key) = ifaces.open_subkey_with_flags(&name, KEY_SET_VALUE) {
            let has_ip = key.get_value::<Vec<String>, _>("IPAddress").is_ok()
                || key.get_value::<String, _>("IPAddress").is_ok();
            if has_ip {
                let _ = key.set_value("TCPNoDelay", &1u32);
                let _ = key.set_value("TcpAckFrequency", &1u32);
                let _ = key.set_value("TcpDelAckTicks", &0u32);
            }
        }
    }
    Ok(())
}

/// Nagle：原生删除低延迟参数，恢复默认
fn restore_nagle_native() -> Result<(), String> {
    let params = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters", KEY_SET_VALUE)
        .map_err(|e| format!("打开 Tcpip 参数键失败: {}", e))?;
    let _ = params.delete_value("TcpAckFrequency");

    let ifaces = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces", KEY_READ)
        .map_err(|e| format!("打开 Tcpip 接口键失败: {}", e))?;
    for name in ifaces.enum_keys().flatten() {
        if let Ok(key) = ifaces.open_subkey_with_flags(&name, KEY_SET_VALUE) {
            let has_ip = key.get_value::<Vec<String>, _>("IPAddress").is_ok()
                || key.get_value::<String, _>("IPAddress").is_ok();
            if has_ip {
                let _ = key.delete_value("TCPNoDelay");
                let _ = key.delete_value("TcpAckFrequency");
                let _ = key.delete_value("TcpDelAckTicks");
            }
        }
    }
    Ok(())
}

/// 网卡省电：off=true 设置 PnPCapabilities 0x100 位禁用省电；off=false 清除该位
fn set_power_saving_native(off: bool) -> Result<(), String> {
    let class = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(NIC_CLASS_KEY, KEY_READ)
        .map_err(|e| format!("打开网卡类键失败: {}", e))?;
    for name in class.enum_keys().flatten() {
        if let Ok(key) = class.open_subkey_with_flags(&name, KEY_READ | KEY_SET_VALUE) {
            // 仅处理网卡设备（有 DriverDesc）
            if key.get_value::<String, _>("DriverDesc").is_err() {
                continue;
            }
            let cap = key.get_value::<u32, _>("PnPCapabilities").unwrap_or(0);
            let new = if off { cap | 0x100 } else { cap & !0x100 };
            if new != cap {
                let _ = key.set_value("PnPCapabilities", &new);
            }
        }
    }
    Ok(())
}

// === 1. TCP 拥塞控制优化 ===

#[tauri::command]
pub async fn set_tcp_congestion() -> Result<PerfTweakResult, String> {
    run_netsh_result(&["int", "tcp", "set", "supplemental", "Internet", "congestionprovider=ctcp"])
        .map(|_| PerfTweakResult {
            success: true,
            message: "TCP 拥塞控制已优化".to_string(),
        })
}

#[tauri::command]
pub async fn restore_tcp_congestion() -> Result<PerfTweakResult, String> {
    run_netsh_result(&["int", "tcp", "set", "supplemental", "Internet", "congestionprovider=newreno"])
        .map(|_| PerfTweakResult {
            success: true,
            message: "TCP 拥塞控制已恢复".to_string(),
        })
}

// === 2. TCP Chimney Offload ===

#[tauri::command]
pub async fn set_tcp_chimney_off() -> Result<PerfTweakResult, String> {
    run_netsh_result(&["int", "tcp", "set", "global", "chimney=disabled"])
        .map(|_| PerfTweakResult {
            success: true,
            message: "TCP Chimney Offload 已禁用".to_string(),
        })
}

#[tauri::command]
pub async fn restore_tcp_chimney() -> Result<PerfTweakResult, String> {
    run_netsh_result(&["int", "tcp", "set", "global", "chimney=enabled"])
        .map(|_| PerfTweakResult {
            success: true,
            message: "TCP Chimney Offload 已恢复".to_string(),
        })
}

// === 3. Nagle 算法低延迟策略 ===

#[tauri::command]
pub async fn set_nagle_optimization() -> Result<PerfTweakResult, String> {
    set_nagle_native().map(|_| PerfTweakResult {
        success: true,
        message: "Nagle 低延迟优化已应用".to_string(),
    })
}

#[tauri::command]
pub async fn restore_nagle_optimization() -> Result<PerfTweakResult, String> {
    restore_nagle_native().map(|_| PerfTweakResult {
        success: true,
        message: "Nagle 低延迟优化已恢复".to_string(),
    })
}

// === 4. 禁用网卡省电模式 ===

#[tauri::command]
pub async fn set_adapter_power_saving_off() -> Result<PerfTweakResult, String> {
    set_power_saving_native(true).map(|_| PerfTweakResult {
        success: true,
        message: "网卡省电模式已禁用".to_string(),
    })
}

#[tauri::command]
pub async fn restore_adapter_power_saving() -> Result<PerfTweakResult, String> {
    set_power_saving_native(false).map(|_| PerfTweakResult {
        success: true,
        message: "网卡省电模式已恢复".to_string(),
    })
}

// === 5. DNS 延迟探测(真实 DNS 查询,UDP 直连,无进程启动开销) ===

/// DNS 探测结果
#[derive(serde::Serialize)]
pub struct DnsProbeResult {
    /// 往返延迟(毫秒,微秒精度保留小数,本地劫持时可能小于 1)
    pub latency_ms: f64,
    /// 实际应答的来源 IP(≠ 查询目标即被本地代理/安全软件劫持)
    pub responder: String,
    /// 到达目标实际经过的网卡名(TUN 虚拟网卡/安全软件驱动会在此现形)
    pub via_interface: Option<String>,
}

/// 查询到达目标 IPv4 所走网卡的友好名称。
#[cfg(windows)]
fn route_interface_name(ip: std::net::Ipv4Addr) -> Option<String> {
    use winapi::um::iphlpapi::{GetAdaptersAddresses, GetBestInterface};
    use winapi::um::iptypes::IP_ADAPTER_ADDRESSES;

    const ERROR_BUFFER_OVERFLOW: u32 = 111;

    let mut best_if: u32 = 0;
    // GetBestInterface 需要网络字节序的 IPv4 地址
    let dest = u32::from(ip).to_be();
    unsafe {
        if GetBestInterface(dest, &mut best_if) != 0 {
            return None;
        }
    }

    // 标准两次调用模式:AF_INET=2,缓冲区不足(111)时按返回大小重试
    const AF_INET: u32 = 2;
    let mut size: u32 = 16 * 1024;
    let mut buffer;
    loop {
        buffer = vec![0u8; size as usize];
        let rc = unsafe {
            GetAdaptersAddresses(
                AF_INET,
                0,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES,
                &mut size,
            )
        };
        if rc == 0 {
            break;
        }
        if rc == ERROR_BUFFER_OVERFLOW {
            continue;
        }
        return None;
    }

    let mut node = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES;
    while !node.is_null() {
        let adapter = unsafe { &*node };
        if unsafe { adapter.u.s().IfIndex } as u32 == best_if {
            let name = adapter.FriendlyName;
            let mut len = 0usize;
            unsafe {
                while *name.add(len) != 0 {
                    len += 1;
                }
            }
            return Some(unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(name, len)) });
        }
        node = adapter.Next;
    }
    None
}

#[cfg(not(windows))]
fn route_interface_name(_ip: std::net::Ipv4Addr) -> Option<String> {
    None
}

/// 测量指定 DNS 服务器的查询往返延迟。
///
/// 发送一个最小 DNS 查询(根域 "." 的 NS 记录,任何解析器都能从缓存直接应答,
/// 不依赖具体域名)到 UDP 53 端口,以收到合法应答的时间差作为延迟;
/// 超时或应答不合法视为失败。
#[tauri::command]
pub async fn test_dns_latency(ip: String) -> Result<DnsProbeResult, String> {
    use std::net::{IpAddr, SocketAddr, UdpSocket};
    use std::time::{Duration, Instant};

    let addr = ip
        .parse::<IpAddr>()
        .map_err(|_| format!("无效的 DNS 地址: {}", ip))?;
    let timeout = Duration::from_millis(1000);

    tokio::task::spawn_blocking(move || {
        let via_interface = match addr {
            IpAddr::V4(v4) => route_interface_name(v4),
            IpAddr::V6(_) => None,
        };
        let bind_addr = if addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
        let socket = UdpSocket::bind(bind_addr).map_err(|e| format!("创建 UDP 套接字失败: {}", e))?;
        socket
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("设置读取超时失败: {}", e))?;
        socket
            .set_write_timeout(Some(timeout))
            .map_err(|e| format!("设置发送超时失败: {}", e))?;

        // 报文:事务 ID(2) + 标志 RD=1(2) + QDCOUNT=1(2) + 其余计数 0(6)
        //      + 根域 "." 标签 0x00(1) + QTYPE=NS(2) + QCLASS=IN(2)
        let mut query = [0u8; 17];
        query[0] = 0x4E;
        query[1] = 0x58;
        query[2] = 0x01;
        query[5] = 0x01;
        query[13] = 0x02;
        query[16] = 0x01;

        let started = Instant::now();
        socket
            .send_to(&query, SocketAddr::new(addr, 53))
            .map_err(|e| format!("发送 DNS 查询失败: {}", e))?;

        let mut response = [0u8; 512];
        let (received, source) = socket
            .recv_from(&mut response)
            .map_err(|e| format!("DNS 无响应: {}", e))?;
        let latency_ms = started.elapsed().as_micros() as f64 / 1000.0;
        if received < 12 || response[0] != query[0] || response[1] != query[1] {
            return Err("DNS 应答无效".to_string());
        }

        Ok(DnsProbeResult {
            latency_ms,
            responder: source.ip().to_string(),
            via_interface,
        })
    })
    .await
    .map_err(|e| format!("DNS 测速任务异常: {}", e))?
}

// === 6. DNS 优化 ===

#[tauri::command]
pub async fn set_dns_servers(dns_primary: String, dns_secondary: String) -> Result<PerfTweakResult, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    let script = format!(
        r#"
$ErrorActionPreference = 'SilentlyContinue'
$adapters = Get-NetAdapter -Physical -ErrorAction SilentlyContinue | Where-Object {{ $_.Status -eq "Up" }}
foreach ($adapter in $adapters) {{
    Set-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -ServerAddresses ("{0}", "{1}") -ErrorAction SilentlyContinue | Out-Null
}}
Write-Output 'OK'
"#,
        dns_primary, dns_secondary
    );

    let ps_path = get_powershell_path();
    let result = Command::new(&ps_path)
        .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行命令失败: {}", e))?;

    if result.status.success() {
        Ok(PerfTweakResult { success: true, message: format!("DNS 已切换到 {} / {}", dns_primary, dns_secondary) })
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        let stdout = String::from_utf8_lossy(&result.stdout).to_string();
        let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else { stdout.trim().to_string() };
        let lower = err_msg.to_lowercase();
        if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
            Err("需要管理员权限，请以管理员身份运行 NexBox".to_string())
        } else {
            Err(format!("DNS 设置失败: {}", err_msg))
        }
    }
}

#[tauri::command]
pub async fn restore_dns_servers() -> Result<PerfTweakResult, String> {
    run_simple_feature(r#"
$ErrorActionPreference = 'SilentlyContinue'
$adapters = Get-NetAdapter -Physical -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq "Up" }
foreach ($adapter in $adapters) {
    Set-DnsClientServerAddress -InterfaceIndex $adapter.ifIndex -ResetServerAddresses -ErrorAction SilentlyContinue | Out-Null
}
Write-Output 'OK'
"#)
}

/// 清理 DNS 解析缓存（ipconfig /flushdns，无需 PowerShell）
#[tauri::command]
pub async fn clear_dns_cache() -> Result<PerfTweakResult, String> {
    let result = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行清理 DNS 缓存失败: {}", e))?;

    if result.status.success() {
        Ok(PerfTweakResult { success: true, message: "DNS 缓存已清理".to_string() })
    } else {
        let stderr = decode_console(result.stderr);
        let stdout = decode_console(result.stdout);
        let err_msg = if !stderr.trim().is_empty() { stderr.trim().to_string() } else { stdout.trim().to_string() };
        let lower = err_msg.to_lowercase();
        if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
            Err("需要管理员权限，请以管理员身份运行 NexBox".to_string())
        } else {
            Err(format!("清理 DNS 缓存失败: {}", err_msg))
        }
    }
}

/// 重置网络栈（netsh winsock reset + netsh int ip reset），常用于解决网络异常
/// 注意：执行后需重启电脑才能完全生效。
#[tauri::command]
pub async fn reset_network() -> Result<PerfTweakResult, String> {
    let winsock = Command::new("netsh")
        .args(["winsock", "reset"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("重置 Winsock 失败: {}", e))?;
    let ip = Command::new("netsh")
        .args(["int", "ip", "reset"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("重置 TCP/IP 协议栈失败: {}", e))?;

    let mut combined = String::new();
    for out in [winsock, ip] {
        if !out.stdout.is_empty() {
            combined.push_str(&decode_console(out.stdout));
        }
        if !out.stderr.is_empty() {
            combined.push_str(&decode_console(out.stderr));
        }
        combined.push('\n');
    }

    let lower = combined.to_lowercase();
    if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
        return Err("需要管理员权限，请以管理员身份运行 NexBox".to_string());
    }

    Ok(PerfTweakResult {
        success: true,
        message: "网络已重置，建议重启电脑后生效".to_string(),
    })
}

/// 修复 DHCP：将物理网卡恢复为 IP + DNS 自动获取，并重新获取 IP、清理 DNS 缓存
/// 纯原生实现（注册表枚举 + netsh），不启动 PowerShell，启动开销毫秒级
#[tauri::command]
pub async fn fix_dhcp() -> Result<PerfTweakResult, String> {
    // 1. 从网卡设备类键收集物理网卡的接口 GUID（NetCfgInstanceId）
    //    注意：DHCP 动态网卡在 Tcpip\Interfaces 下可能没有 IPAddress 值，
    //    因此不能用 IPAddress 过滤，改用设备类键的 NetCfgInstanceId 定位物理网卡
    let mut physical: Vec<String> = Vec::new();
    if let Ok(class) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(NIC_CLASS_KEY, KEY_READ)
    {
        for name in class.enum_keys().flatten() {
            let Ok(k) = class.open_subkey(&name) else { continue };
            if let Ok(id) = k.get_value::<String, _>("NetCfgInstanceId") {
                if !id.trim().is_empty() {
                    physical.push(id);
                }
            }
        }
    }

    if physical.is_empty() {
        return Err("未发现物理网络接口".to_string());
    }

    // 2. 逐个用 netsh 恢复 IP/DNS 为自动获取（DHCP）
    //    某些网卡可能当前未连接，netsh 会报"找不到接口"之类错误，可忽略；
    //    仅当出现权限类错误时中断提示管理员权限
    for guid in &physical {
        for r in [
            run_netsh_result(&["interface", "ipv4", "set", "address", "name", guid, "source=dhcp"]),
            run_netsh_result(&["interface", "ipv4", "set", "dnsservers", "name", guid, "source=dhcp"]),
        ] {
            if let Err(e) = r {
                let lower = e.to_lowercase();
                if lower.contains("拒绝访问") || lower.contains("denied") || lower.contains("权限不足") || lower.contains("access denied") {
                    return Err("需要管理员权限，请以管理员身份运行 NexBox".to_string());
                }
            }
        }
    }

    // 3. 刷新 DNS 解析缓存（毫秒级）
    //    说明：接口设为 DHCP 后 Windows 会在后台自动续租获取新 IP；
    //    为避免长时间阻塞等待系统 DHCP 交互（/renew 可能数秒），这里不做 /release /renew，
    //    从而让修复立即返回
    let flush = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("执行 ipconfig /flushdns 失败: {}", e))?;
    if !flush.status.success() {
        let stderr = decode_console(flush.stderr);
        let stdout = decode_console(flush.stdout);
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else {
            stdout.trim().to_string()
        };
        let lower = err_msg.to_lowercase();
        if lower.contains("access denied") || lower.contains("denied") || lower.contains("拒绝访问") || lower.contains("权限不足") {
            return Err("需要管理员权限，请以管理员身份运行 NexBox".to_string());
        }
        return Err(format!("ipconfig /flushdns: {}", err_msg));
    }

    Ok(PerfTweakResult {
        success: true,
        message: "已恢复 DHCP 自动获取，DNS 缓存已刷新".to_string(),
    })
}

// === 6. 状态检测（纯 Rust 实现，不启动 PowerShell，毫秒级） ===

#[derive(serde::Serialize)]
pub struct NetworkTweakState {
    pub tcp_congestion_optimized: bool,
    pub chimney_offload: bool,
    pub nagle_optimized: bool,
    pub adapter_power_saving_off: bool,
    pub dns_primary: String,
    pub dns_secondary: String,
}

/// 解码控制台输出（中文 Windows 的 netsh 输出为 CP936/GBK 编码）
fn decode_console(bytes: Vec<u8>) -> String {
    let (cow, _, _) = GBK.decode(&bytes);
    cow.into_owned()
}

/// 直接运行 netsh，返回解码后的输出（无 PowerShell 包装）
fn run_netsh(args: &[&str]) -> String {
    let out = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) => {
            if !o.stdout.is_empty() {
                decode_console(o.stdout)
            } else {
                decode_console(o.stderr)
            }
        }
        Err(_) => String::new(),
    }
}

fn is_chimney_disabled(output: &str) -> bool {
    let has_chimney = output.contains("Chimney Offload State") || output.contains("Chimney 卸载状态");
    has_chimney && (output.to_lowercase().contains("disabled") || output.contains("禁用"))
}

/// Nagle：读取有 IPAddress 的接口中是否存在 TCPNoDelay=1
fn check_nagle() -> bool {
    let Ok(interfaces) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces")
    else {
        return false;
    };
    for name in interfaces.enum_keys().flatten() {
        let Ok(key) = interfaces.open_subkey(&name) else { continue };
        let has_ip = key.get_value::<Vec<String>, _>("IPAddress").is_ok()
            || key.get_value::<String, _>("IPAddress").is_ok();
        if !has_ip {
            continue;
        }
        if key.get_value::<u32, _>("TCPNoDelay").ok() == Some(1) {
            return true;
        }
    }
    false
}

/// 网卡省电：网卡设备注册表 PnPCapabilities 含 0x100 位表示已禁用省电
fn check_power_saving() -> bool {
    let Ok(adapters) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}",
    ) else {
        return false;
    };
    for name in adapters.enum_keys().flatten() {
        let Ok(key) = adapters.open_subkey(&name) else { continue };
        if key
            .get_value::<u32, _>("PnPCapabilities")
            .ok()
            .is_some_and(|v| v & 0x100 != 0)
        {
            return true;
        }
    }
    false
}

/// DNS：优先读 NameServer（手动设置），否则读 DhcpNameServer（DHCP 分配）
fn read_dns() -> (String, String) {
    let Ok(interfaces) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces")
    else {
        return (String::new(), String::new());
    };
    for name in interfaces.enum_keys().flatten() {
        let Ok(key) = interfaces.open_subkey(&name) else { continue };
        let has_ip = key.get_value::<Vec<String>, _>("IPAddress").is_ok()
            || key.get_value::<String, _>("IPAddress").is_ok();
        if !has_ip {
            continue;
        }
        let servers = key
            .get_value::<String, _>("NameServer")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| key.get_value::<String, _>("DhcpNameServer").ok())
            .filter(|s| !s.trim().is_empty());
        if let Some(s) = servers {
            let parts: Vec<&str> = s
                .split(|c: char| c == ',' || c.is_whitespace())
                .filter(|x| !x.is_empty())
                .collect();
            if let Some(primary) = parts.first() {
                let secondary = parts.get(1).copied().unwrap_or_default();
                return (primary.to_string(), secondary.to_string());
            }
        }
    }
    (String::new(), String::new())
}

#[tauri::command]
pub async fn check_network_tweak_states() -> Result<NetworkTweakState, String> {
    if !cfg!(target_os = "windows") {
        return Err("此功能仅支持 Windows 系统".to_string());
    }

    // 两个 netsh 查询并行执行（每个约 0.3~1s），避免串行等待
    let supp = tokio::task::spawn_blocking(|| run_netsh(&["int", "tcp", "show", "supplemental"]));
    let global = tokio::task::spawn_blocking(|| run_netsh(&["int", "tcp", "show", "global"]));
    let supp_out = supp.await.unwrap_or_default();
    let global_out = global.await.unwrap_or_default();

    // 以下均为注册表读取，毫秒级
    let supp_lower = supp_out.to_lowercase();
    let tcp_congestion_optimized = supp_lower.contains("ctcp") || supp_lower.contains("cubic");
    let chimney_offload = is_chimney_disabled(&global_out);
    let nagle_optimized = check_nagle();
    let adapter_power_saving_off = check_power_saving();
    let (dns_primary, dns_secondary) = read_dns();

    Ok(NetworkTweakState {
        tcp_congestion_optimized,
        chimney_offload,
        nagle_optimized,
        adapter_power_saving_off,
        dns_primary,
        dns_secondary,
    })
}

// === 7. 批量优化 / 恢复（原生实现，不启动 PowerShell） ===

#[tauri::command]
pub async fn batch_network_enable() -> Result<PerfTweakResult, String> {
    let mut errors = Vec::new();
    if let Err(e) = run_netsh_result(&["int", "tcp", "set", "supplemental", "Internet", "congestionprovider=ctcp"]) {
        errors.push(format!("TCP 拥塞控制: {}", e));
    }
    if let Err(e) = run_netsh_result(&["int", "tcp", "set", "global", "chimney=disabled"]) {
        errors.push(format!("Chimney Offload: {}", e));
    }
    if let Err(e) = set_nagle_native() {
        errors.push(format!("Nagle: {}", e));
    }
    if let Err(e) = set_power_saving_native(true) {
        errors.push(format!("网卡省电: {}", e));
    }
    if errors.is_empty() {
        Ok(PerfTweakResult {
            success: true,
            message: "网络优化已全部应用".to_string(),
        })
    } else {
        Err(errors.join("; "))
    }
}

#[tauri::command]
pub async fn batch_network_disable() -> Result<PerfTweakResult, String> {
    let mut errors = Vec::new();
    if let Err(e) = run_netsh_result(&["int", "tcp", "set", "supplemental", "Internet", "congestionprovider=newreno"]) {
        errors.push(format!("TCP 拥塞控制: {}", e));
    }
    if let Err(e) = run_netsh_result(&["int", "tcp", "set", "global", "chimney=enabled"]) {
        errors.push(format!("Chimney Offload: {}", e));
    }
    if let Err(e) = restore_nagle_native() {
        errors.push(format!("Nagle: {}", e));
    }
    if let Err(e) = set_power_saving_native(false) {
        errors.push(format!("网卡省电: {}", e));
    }
    if errors.is_empty() {
        Ok(PerfTweakResult {
            success: true,
            message: "网络优化已全部恢复".to_string(),
        })
    } else {
        Err(errors.join("; "))
    }
}

// === 8. 公网 IP 查询（国内可访问的免费 API，多源 fallback，仅返回 IPv4） ===

#[derive(Clone, Copy)]
enum PublicIpProvider {
    /// 返回纯 IP 文本
    Plain,
    /// 返回 key=value 文本（cloudflare trace），需解析 ip= 字段
    Trace,
}

/// 国内可访问的免费公网 IPv4 查询 API，按顺序 fallback
const PUBLIC_IP_PROVIDERS: &[(&str, PublicIpProvider)] = &[
    ("https://4.ipw.cn", PublicIpProvider::Plain),
    ("https://ip.3322.net", PublicIpProvider::Plain),
    ("https://myip.ipip.net", PublicIpProvider::Plain),
    ("https://api.ip.sb/ip", PublicIpProvider::Plain),
    ("https://api.ipify.org", PublicIpProvider::Plain),
    ("https://cloudflare.com/cdn-cgi/trace", PublicIpProvider::Trace),
];

/// 校验是否为合法的 IPv4 地址
fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 3
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u32>().map(|n| n <= 255).unwrap_or(false)
        })
}

/// 从任意文本中提取第一个 IPv4 地址
fn find_ipv4(text: &str) -> Option<String> {
    text.split(|c: char| !c.is_ascii_digit() && c != '.')
        .find(|s| is_valid_ipv4(s))
        .map(|s| s.to_string())
}

fn extract_ipv4(text: &str, provider: PublicIpProvider) -> Option<String> {
    match provider {
        PublicIpProvider::Plain => find_ipv4(text),
        PublicIpProvider::Trace => text
            .lines()
            .find_map(|line| line.trim().strip_prefix("ip=").and_then(find_ipv4)),
    }
}

/// 获取当前网络的公网 IPv4 地址（多 API 顺序 fallback，国内可访问）
#[tauri::command]
pub async fn get_public_ip() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {}", e))?;

    for &(url, provider) in PUBLIC_IP_PROVIDERS {
        if let Ok(resp) = client.get(url).send().await {
            if let Ok(text) = resp.text().await {
                if let Some(ip) = extract_ipv4(&text, provider) {
                    return Ok(ip);
                }
            }
        }
    }

    Err("无法获取公网 IPv4 地址，请检查网络连接".to_string())
}
