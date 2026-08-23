// ============================================================================
// SMART 硬盘健康度检测 — 移植自 CrystalDiskInfo (MIT License)
// 参考源码: CrystalDiskInfo-master/AtaSmart.cpp / AtaSmart.h / StorageQuery.h
//   - 读取层:  GetSmartAttributePd / GetSmartThresholdPd (ATA)
//              GetSmartAttributeNVMeStorageQuery (NVMe)
//   - 判定层:  CheckDiskStatus
// 使用 Windows API (winapi) 直接读取 SMART 数据，无需 PowerShell。
// ============================================================================

#![cfg(target_os = "windows")]

use winapi::shared::minwindef::{DWORD, LPVOID};
use winapi::shared::ntdef::HANDLE;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::ioapiset::DeviceIoControl;
use winapi::um::winnt::{
    FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GENERIC_READ, GENERIC_WRITE,
};

// ─── IOCTL / ATA 命令常量（来源：CDI AtaSmart.h L409-412 / ntdddisk.h）───

/// SMART READ DATA (DFP_RECEIVE_DRIVE_DATA)
const DFP_RECEIVE_DRIVE_DATA: DWORD = 0x0007C088;
/// IOCTL_STORAGE_QUERY_PROPERTY（NVMe 协议特定属性读取）
const IOCTL_STORAGE_QUERY_PROPERTY: DWORD = 0x002D1400;

/// ATA SMART 命令
const SMART_CMD: u8 = 0xB0;
/// SMART 读取属性（READ ATTRIBUTES）
const READ_ATTRIBUTES: u8 = 0xD0;
/// SMART 读取阈值（READ THRESHOLDS）
const READ_THRESHOLDS: u8 = 0xD1;
/// SMART 圆柱寄存器低位（0x4F）
const SMART_CYL_LOW: u8 = 0x4F;
/// SMART 圆柱寄存器高位（0xC2）
const SMART_CYL_HI: u8 = 0xC2;
/// 主盘 bDriveHeadReg 目标
const ATA_MASTER: u8 = 0xA0;

const READ_ATTRIBUTE_BUFFER_SIZE: DWORD = 512;
const MAX_ATTRIBUTE: usize = 30;

// NVMe StorageQuery 常量（来源：CDI StorageQuery.h）
const STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY: DWORD = 49;
const PROPERTY_STANDARD_QUERY: DWORD = 0;
const PROTOCOL_TYPE_NVME: DWORD = 3;
const NVME_DATA_TYPE_LOG_PAGE: DWORD = 2;
/// NVMe Log Page ID 2 = SMART / Health Information
const NVME_LOG_PAGE_SMART_HEALTH_INFO: DWORD = 2;

// 判定阈值默认值（与 CDI HealthDlg 默认一致）
const THRESHOLD_05: u16 = 1;
const THRESHOLD_C5: u16 = 1;
const THRESHOLD_C6: u16 = 1;
const THRESHOLD_FF: u16 = 10;

// ─── 数据结构（来源：CDI NVMeInterpreter.h L15 / AtaSmart.h L468）───

/// 单个 SMART 属性（12 字节，来源：CDI NVMeInterpreter.h L15-23）
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SmartAttribute {
    pub id: u8,
    pub status_flags: u16,
    pub current_value: u8,
    pub worst_value: u8,
    pub raw_value: [u8; 6],
    pub reserved: u8,
}

impl SmartAttribute {
    /// 小端 16 位原始值（对应 CDI B8toB16le）
    #[inline]
    fn raw16(&self) -> u16 {
        u16::from_le_bytes([self.raw_value[0], self.raw_value[1]])
    }
}

/// 单个 SMART 阈值（12 字节，来源：CDI AtaSmart.h L468-473）
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SmartThreshold {
    pub id: u8,
    pub threshold_value: u8,
    pub reserved: [u8; 10],
}

/// 健康状态枚举（对应 CDI AtaSmart.h L303-307 DISK_STATUS_*）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskStatus {
    Unknown = 0,
    Good,
    Caution,
    Bad,
}

impl DiskStatus {
    /// 映射为字符串，供前端展示（与现有 DiskHealthPage 兼容）
    pub fn as_str(&self) -> &'static str {
        match self {
            DiskStatus::Good => "healthy",
            DiskStatus::Caution => "warning",
            DiskStatus::Bad => "unhealthy",
            DiskStatus::Unknown => "unknown",
        }
    }
}

/// SMART 读取 + 判定结果
#[derive(Debug, Clone)]
pub struct SmartInfo {
    pub status: DiskStatus,
    /// 健康度百分比 0-100（NVMe: 100-PercentageUsed；SSD: 寿命属性；HDD: 按状态映射）
    pub life_percent: Option<u8>,
    /// 温度（摄氏度）
    pub temperature_c: Option<i32>,
    /// 通电小时数
    pub power_on_hours: Option<u64>,
    /// 通电次数
    pub power_on_count: Option<u64>,
    /// 累计数据读取量（字节）
    pub data_read_bytes: Option<u64>,
    /// 累计数据写入量（字节）
    pub data_written_bytes: Option<u64>,
    /// 是否为 NVMe（SSD）
    pub is_nvme: bool,
    /// SMART 属性是否有效（可判定）
    pub has_smart: bool,
    /// 读取失败原因
    pub error: Option<String>,
}

// ─── 设备打开 ───

/// 打开物理磁盘设备（\\\\.\\PhysicalDriveN）
fn open_physical_drive(index: u32) -> Result<HANDLE, String> {
    let path = format!("\\\\.\\PhysicalDrive{}", index);
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // 先尝试读写权限（与 CDI 一致），失败时降级为只读
    let mut h = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        h = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
    }
    if h == INVALID_HANDLE_VALUE {
        let err = unsafe { GetLastError() };
        return Err(format!("打开 PhysicalDrive{} 失败 (错误码 {})", index, err));
    }
    Ok(h)
}

// ─── ATA SMART 读取（对应 CDI GetSmartAttributePd / GetSmartThresholdPd）───

/// 构造 ATA SENDCMDINPARAMS 输入参数（36 字节 = 对齐后的 sizeof(SENDCMDINPARAMS)）。
/// bFeaturesReg: READ_ATTRIBUTES(0xD0) 或 READ_THRESHOLDS(0xD1)
fn build_send_cmd(feature: u8) -> [u8; 36] {
    let mut buf = [0u8; 36];
    // SENDCMDINPARAMS 布局（ntdddisk.h）:
    //   DWORD  cBufferSize;     offset 0
    //   IDEREGS irDriveRegs;    offset 4  (8 bytes)
    //   BYTE   bDriveNumber;    offset 12
    //   BYTE   bReserved[3];    offset 13
    //   DWORD  dwReserved[4];   offset 16
    //   BYTE   bBuffer[1];      offset 32
    buf[0..4].copy_from_slice(&READ_ATTRIBUTE_BUFFER_SIZE.to_le_bytes()); // cBufferSize
    buf[4] = feature; // bFeaturesReg
    buf[5] = 1; // bSectorCountReg
    buf[6] = 1; // bSectorNumberReg
    buf[7] = SMART_CYL_LOW; // bCylLowReg
    buf[8] = SMART_CYL_HI; // bCylHighReg
    buf[9] = ATA_MASTER; // bDriveHeadReg
    buf[10] = SMART_CMD; // bCommandReg
    buf
}

/// 读取 ATA SMART 数据或阈值（512 字节原始缓冲）
fn read_ata_smart_raw(index: u32, feature: u8) -> Result<[u8; 512], String> {
    let h = open_physical_drive(index)?;
    let result = (|| {
        let input = build_send_cmd(feature);
        // 输出缓冲: SENDCMDOUTPARAMS(16B) + SMART 数据(512B)，CDI 使用 SMART_READ_DATA_OUTDATA
        let mut output = [0u8; 16 + 512];
        let mut returned: DWORD = 0;
        let ok = unsafe {
            DeviceIoControl(
                h,
                DFP_RECEIVE_DRIVE_DATA,
                input.as_ptr() as LPVOID,
                input.len() as DWORD,
                output.as_mut_ptr() as LPVOID,
                output.len() as DWORD,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!("DeviceIoControl SMART 读取失败 (错误码 {})", unsafe { GetLastError() }));
        }
        // SENDCMDOUTPARAMS: cBufferSize(4) + DRIVERSTATUS(12) = 16 字节，SMART 数据从偏移 16 开始
        let mut raw = [0u8; 512];
        raw.copy_from_slice(&output[16..16 + 512]);
        Ok(raw)
    })();
    unsafe { CloseHandle(h) };
    result
}

/// 读取 ATA SMART 属性数据
fn read_ata_attributes(index: u32) -> Result<[u8; 512], String> {
    read_ata_smart_raw(index, READ_ATTRIBUTES)
}

/// 读取 ATA SMART 阈值数据
fn read_ata_thresholds(index: u32) -> Result<[u8; 512], String> {
    read_ata_smart_raw(index, READ_THRESHOLDS)
}

// ─── NVMe SMART 读取（对应 CDI GetSmartAttributeNVMeStorageQuery）───

/// 通过 IOCTL_STORAGE_QUERY_PROPERTY 读取 NVMe SMART/Health Information Log
fn read_nvme_smart_raw(index: u32) -> Result<[u8; 512], String> {
    let h = open_physical_drive(index)?;
    let result = (|| {
        // TStorageQueryWithBuffer 布局（CDI StorageQuery.h）:
        //   TStoragePropertyQuery      Query(8B)
        //   TStorageProtocolSpecificData ProtocolSpecific(40B)
        //   BYTE Buffer[4096]
        let mut buf = [0u8; 8 + 40 + 4096];
        // Query: PropertyId + QueryType
        buf[0..4].copy_from_slice(&STORAGE_ADAPTER_PROTOCOL_SPECIFIC_PROPERTY.to_le_bytes());
        buf[4..8].copy_from_slice(&PROPERTY_STANDARD_QUERY.to_le_bytes());
        // ProtocolSpecific
        buf[8..12].copy_from_slice(&PROTOCOL_TYPE_NVME.to_le_bytes());
        buf[12..16].copy_from_slice(&NVME_DATA_TYPE_LOG_PAGE.to_le_bytes());
        buf[16..20].copy_from_slice(&NVME_LOG_PAGE_SMART_HEALTH_INFO.to_le_bytes());
        buf[20..24].copy_from_slice(&0u32.to_le_bytes()); // ProtocolDataRequestSubValue
        buf[24..28].copy_from_slice(&40u32.to_le_bytes()); // ProtocolDataOffset = sizeof(TStorageProtocolSpecificData)
        buf[28..32].copy_from_slice(&4096u32.to_le_bytes()); // ProtocolDataLength
        // FixedProtocolReturnData(0) + Reserved[3](0) 已默认清零

        let mut returned: DWORD = 0;
        let mut ok = unsafe {
            DeviceIoControl(
                h,
                IOCTL_STORAGE_QUERY_PROPERTY,
                buf.as_ptr() as LPVOID,
                buf.len() as DWORD,
                buf.as_mut_ptr() as LPVOID,
                buf.len() as DWORD,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        // 与 CDI 一致：首次失败后以 ProtocolDataRequestSubValue=0xFFFFFFFF 重试
        if ok == 0 {
            buf[20..24].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
            ok = unsafe {
                DeviceIoControl(
                    h,
                    IOCTL_STORAGE_QUERY_PROPERTY,
                    buf.as_ptr() as LPVOID,
                    buf.len() as DWORD,
                    buf.as_mut_ptr() as LPVOID,
                    buf.len() as DWORD,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
        }
        if ok == 0 {
            return Err(format!("IOCTL_STORAGE_QUERY_PROPERTY 失败 (错误码 {})", unsafe { GetLastError() }));
        }
        // SMART 数据位于 Query(8) + ProtocolSpecific(40) 之后，即偏移 48
        let mut raw = [0u8; 512];
        raw.copy_from_slice(&buf[48..48 + 512]);
        Ok(raw)
    })();
    unsafe { CloseHandle(h) };
    result
}

// ─── 解析层（对应 CDI FillSmartData）───

/// 从 512 字节 SMART 原始缓冲解析属性数组（对应 CDI FillSmartData L11734）
fn parse_attributes(raw: &[u8; 512]) -> Vec<SmartAttribute> {
    let mut attrs = Vec::with_capacity(MAX_ATTRIBUTE);
    for i in 0..MAX_ATTRIBUTE {
        let off = 2 + i * 12; // 前 2 字节是 revision，属性从偏移 2 开始
        if off + 12 > raw.len() {
            break;
        }
        let attr = SmartAttribute {
            id: raw[off],
            status_flags: u16::from_le_bytes([raw[off + 1], raw[off + 2]]),
            current_value: raw[off + 3],
            worst_value: raw[off + 4],
            raw_value: raw[off + 5..off + 11].try_into().unwrap(),
            reserved: raw[off + 11],
        };
        if attr.id != 0 {
            attrs.push(attr);
        }
    }
    attrs
}

/// 从 512 字节 SMART 阈值原始缓冲解析阈值数组
fn parse_thresholds(raw: &[u8; 512]) -> Vec<SmartThreshold> {
    let mut ths = Vec::with_capacity(MAX_ATTRIBUTE);
    for i in 0..MAX_ATTRIBUTE {
        let off = 2 + i * 12;
        if off + 12 > raw.len() {
            break;
        }
        let th = SmartThreshold {
            id: raw[off],
            threshold_value: raw[off + 1],
            reserved: raw[off + 2..off + 12].try_into().unwrap(),
        };
        if th.id != 0 {
            ths.push(th);
        }
    }
    ths
}

/// 按键查找属性
fn find_attribute(attrs: &[SmartAttribute], id: u8) -> Option<&SmartAttribute> {
    attrs.iter().find(|a| a.id == id)
}

/// 按键查找阈值
fn find_threshold(ths: &[SmartThreshold], id: u8) -> Option<u8> {
    ths.iter().find(|t| t.id == id).map(|t| t.threshold_value)
}

// ─── 判定层（移植 CDI CheckDiskStatus L12522-12830）───

/// 该属性 ID 是否属于"关键属性范围"（CDI L12622-12634）
fn is_critical_id(id: u8) -> bool {
    (0x01..=0x0D).contains(&id)
        || id == 0x16
        || (0xBB..=0xBD).contains(&id)
        || (0xBF..=0xC1).contains(&id)
        || (0xC3..=0xD1).contains(&id)
        || (0xD3..=0xD4).contains(&id)
        || (0xDC..=0xE4).contains(&id)
        || (0xE6..=0xE7).contains(&id)
        || id == 0xF0
        || id == 0xFA
        || id == 0xFE
}

/// 是否为 SSD 寿命属性（CDI L12684-12699 各厂商分支的汇总）
fn is_life_attribute(id: u8) -> bool {
    matches!(
        id,
        0xA9 | 0xAD | 0xB1 | 0xBB | 0xCA | 0xD1 | 0xE6 | 0xE7 | 0xE8 | 0xE9 | 0xC9
    )
}

/// 计算 SSD 寿命（百分比）。
/// 简化移植：优先取 CurrentValue（0-100 归一化值）；对 0xE6(WDC/SanDisk) 等
/// 使用 RawValue 计算的厂商特例保留。完整厂商特例见 CDI L12704-12791。
fn compute_life(attr: &SmartAttribute) -> i32 {
    match attr.id {
        0xE6 => {
            // WDC / SanDisk: 100 - RawValue[1]（CDI L12752）
            100 - attr.raw_value[1] as i32
        }
        0xE7 => {
            // SandForce 等增量式寿命：100 - RawValue[0]（CDI L12710）
            100 - attr.raw_value[0] as i32
        }
        _ => attr.current_value as i32,
    }
}

/// 判断给定属性是否为 NVMe 的关键指标（CDI L12541-12556 使用 Attribute[0]/[2]/[3]）
fn is_nvme_available_spare(attrs: &[SmartAttribute]) -> Option<(u8, u8)> {
    // NVMe 映射后: Id=3 Available Spare, Id=4 Spare Threshold（CDI NVMeInterpreter L34-50）
    let spare = find_attribute(attrs, 3)?;
    let threshold = find_attribute(attrs, 4)?;
    Some((spare.raw_value[0], threshold.raw_value[0]))
}

/// 移植 CDI CheckDiskStatus 核心判定（普通盘：HDD/SSD）
fn check_ata_status(
    attrs: &[SmartAttribute],
    ths: &[SmartThreshold],
    is_ssd: bool,
    is_threshold_correct: bool,
    threshold_ff: u16,
) -> (DiskStatus, Option<i32>) {
    // 预检（CDI L12568-12579）：机械盘必须拥有有效阈值才能判定
    if !is_ssd && !is_threshold_correct {
        return (DiskStatus::Unknown, None);
    }

    let mut error = 0;
    let mut caution = 0;
    let mut flag_unknown = true;
    let mut life: Option<i32> = None;

    for (j, attr) in attrs.iter().enumerate() {
        // 重复 ID 检测（CDI L12590-12597）
        for k in 0..j {
            if attrs[k].id != 0 && attrs[j].id == attrs[k].id {
                return (DiskStatus::Unknown, None);
            }
        }

        let id = attr.id;
        let threshold = find_threshold(ths, id);

        // 异常排除分支（CDI L12599-12612）: 温度属性(0xC2) 与 SSD RawValues8 不参与 error
        let is_temp_or_raw8 = id == 0xC2;
        if !is_temp_or_raw8 {
            // 属性当前值低于阈值 → error（CDI L12616-12639）
            let current_below_threshold = match threshold {
                Some(t) if t != 0 => attr.current_value < t,
                _ => false,
            };
            let in_critical_range = is_critical_id(id);
            if is_ssd {
                if current_below_threshold {
                    error += 1;
                }
            } else if in_critical_range && current_below_threshold {
                error += 1;
            }
        }

        // SSD 且存在阈值 → 标记为可判定（CDI L12641-12644）
        if is_ssd && threshold.is_some() && threshold != Some(0) {
            flag_unknown = false;
        }

        // 05/C5/C6 坏扇区计数（CDI L12646-12682）
        if matches!(id, 0x05 | 0xC5 | 0xC6) {
            // 4 字节全 FF 视为不可用（CDI L12651-12656）
            let raw_all_ff = attr.raw_value[0..4].iter().all(|&b| b == 0xFF);
            if !raw_all_ff {
                let raw16 = attr.raw16();
                let th = match id {
                    0x05 => THRESHOLD_05,
                    0xC5 => THRESHOLD_C5,
                    0xC6 => THRESHOLD_C6,
                    _ => unreachable!(),
                };
                if th > 0 && raw16 >= th && !is_ssd {
                    caution = 1;
                }
            }
            if !is_ssd {
                flag_unknown = false;
            }
        } else if is_life_attribute(id) {
            // SSD 寿命属性（CDI L12683-12791）
            flag_unknown = false;
            let life_val = compute_life(attr);
            // 截断到 0-100（CDI L12721-12722）
            let life_clamped = life_val.clamp(0, 100);
            if life_val == -1 {
                // FlagLifeNoReport：不参与判定
            } else if life_val == 0 {
                error = 1;
            } else if life_clamped <= threshold_ff as i32 {
                caution = 1;
            }
            life = Some(life_clamped);
        }
    }

    // 汇总（CDI L12814-12829）
    let status = if error > 0 {
        DiskStatus::Bad
    } else if flag_unknown {
        DiskStatus::Unknown
    } else if caution > 0 {
        DiskStatus::Caution
    } else {
        DiskStatus::Good
    };
    (status, life)
}

/// 移植 CDI CheckDiskStatus 的 NVMe 分支（L12529-12566）
fn check_nvme_status(
    attrs: &[SmartAttribute],
    life: i32,
    model: &str,
    threshold_ff: u16,
) -> DiskStatus {
    // 排除虚拟机 NVMe（CDI L12533-12539）
    if model.starts_with("Parallels") || model.starts_with("VMware") || model.starts_with("QEMU") {
        return DiskStatus::Unknown;
    }

    // Critical Warning（CDI Attribute[0].RawValue[0]，NVMe Id=1）> 0 → BAD
    if let Some(critical_warning) = find_attribute(attrs, 1) {
        if critical_warning.raw_value[0] > 0 {
            return DiskStatus::Bad;
        }
    }

    // Available Spare / Spare Threshold（CDI L12546-12556）
    if let Some((spare, spare_threshold)) = is_nvme_available_spare(attrs) {
        if spare_threshold != 0 && spare_threshold <= 100 {
            if spare < spare_threshold {
                return DiskStatus::Bad;
            } else if spare == spare_threshold && spare_threshold != 100 {
                return DiskStatus::Caution;
            }
        }
    }

    // Life 与 ThresholdFF（CDI L12558-12565）
    if life > threshold_ff as i32 {
        DiskStatus::Good
    } else {
        DiskStatus::Caution
    }
}

// ─── 温度 / 通电时间解析 ───

/// 解析 ATA 温度（属性 0xC2 的 RawValue[0]）
fn parse_ata_temperature(attrs: &[SmartAttribute]) -> Option<i32> {
    let attr = find_attribute(attrs, 0xC2)?;
    // 多数盘的 0xC2 温度在 RawValue[0]，部分在 RawValue[0]+RawValue[1]（低字节在前）
    let temp = if attr.raw_value[1] == 0 {
        attr.raw_value[0] as i32
    } else {
        attr.raw16() as i32
    };
    if temp == 0 || temp > 200 {
        None
    } else {
        Some(temp)
    }
}

/// 解析 NVMe 温度（Kelvin → 摄氏度）
fn parse_nvme_temperature(raw: &[u8; 512]) -> Option<i32> {
    let kelvin = raw[1] as i32 | ((raw[2] as i32) << 8);
    let celsius = kelvin - 273;
    if celsius <= 0 || celsius > 200 {
        None
    } else {
        Some(celsius)
    }
}

/// 解析 ATA 通电小时数（属性 0x09）。
/// 与 CrystalDiskInfo 默认一致：仅取 RawValue 低 4 字节小端（高位 2 字节常为厂商冗余数据，
/// 若一并读入会导致数值被天文级放大）。
fn parse_ata_power_on_hours(attrs: &[SmartAttribute]) -> Option<u64> {
    let attr = find_attribute(attrs, 0x09)?;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&attr.raw_value[..4]);
    Some(u64::from_le_bytes(bytes))
}

/// 解析 ATA 通电次数（属性 0x0C Power Cycle Count）。
/// 与 0x09 同源同隐患，同样仅取低 4 字节小端。
fn parse_ata_power_cycles(attrs: &[SmartAttribute]) -> Option<u64> {
    let attr = find_attribute(attrs, 0x0C)?;
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&attr.raw_value[..4]);
    Some(u64::from_le_bytes(bytes))
}

/// 解析 NVMe 通电小时数（SMART/Health Log 偏移 128 起的 8 字节）
fn parse_nvme_power_on_hours(raw: &[u8; 512]) -> Option<u64> {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&raw[128..136]);
    Some(u64::from_le_bytes(bytes))
}

/// 解析 NVMe 通电次数（Power Cycles，SMART/Health Log 偏移 112 起的 8 字节）
fn parse_nvme_power_cycles(raw: &[u8; 512]) -> Option<u64> {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&raw[112..120]);
    Some(u64::from_le_bytes(bytes))
}

/// 解析 NVMe 累计数据单元数（Data Units Read 偏移 32 / Written 偏移 48，8 字节小端）。
/// 按 NVMe 规范，值单位为"千个 512 字节数据单元"（与 CDI HostReads/HostWrites 一致）。
fn parse_nvme_data_bytes(raw: &[u8; 512], offset: usize) -> Option<u64> {
    if offset + 8 > raw.len() {
        return None;
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&raw[offset..offset + 8]);
    Some(u64::from_le_bytes(bytes).saturating_mul(512_000))
}

/// 解析 ATA 累计读写量（属性 0xF1 = Total LBAs Written，0xF2 = Total LBAs Read）。
/// 取低 40 位 × 512 字节（与 CDI 各厂商分支一致），厂商高位扩展字节不计入。
fn parse_ata_data_bytes(attrs: &[SmartAttribute], id: u8) -> Option<u64> {
    let attr = find_attribute(attrs, id)?;
    let mut bytes = [0u8; 8];
    bytes[..5].copy_from_slice(&attr.raw_value[..5]);
    Some(u64::from_le_bytes(bytes).saturating_mul(512))
}

// ─── 对外统一入口 ───

/// 通过 WMI MSStorageDriver_FailurePredictData/Thresholds 读取 ATA SMART。
/// 无需管理员权限，对应 CDI GetSmartAttributeWmi (AtaSmart.cpp L10571-10584)。
/// 通过 InstanceName 前缀匹配 PNPDeviceID，读取 VendorSpecific(uint8[512])。
fn read_ata_smart_wmi(pnp_id: &str, threshold: bool) -> Result<[u8; 512], String> {
    let class = if threshold {
        "MSStorageDriver_FailurePredictThresholds"
    } else {
        "MSStorageDriver_FailurePredictData"
    };
    let rows = crate::wmi_query::wmi_query_ns(
        "ROOT\\WMI",
        &format!("SELECT * FROM {}", class),
    )
    .map_err(|e| format!("WMI {} 查询失败: {}", class, e))?;

    let pnp_upper = pnp_id.trim().to_uppercase();
    if pnp_upper.is_empty() {
        return Err("PNPDeviceID 为空，无法匹配 WMI SMART".to_string());
    }

    for row in rows {
        let inst = row
            .get("InstanceName")
            .and_then(|v| crate::wmi_query::v_str(v))
            .unwrap_or_default()
            .to_uppercase();
        if !inst.starts_with(&pnp_upper) {
            continue;
        }
        if let Some(arr) = row.get("VendorSpecific") {
            let vals = crate::wmi_query::v_u16_arr(arr);
            if vals.len() >= 512 {
                let mut raw = [0u8; 512];
                for (i, v) in vals.iter().take(512).enumerate() {
                    raw[i] = *v as u8;
                }
                return Ok(raw);
            }
        }
    }
    Err(format!("WMI 未找到匹配 {} 的 SMART {}", pnp_id, class))
}

/// 读取一块物理磁盘的 SMART 健康信息。
/// `is_nvme`: 是否为 NVMe（由上层 WMI 判定）；`is_ssd`: 是否为 SSD；
/// `pnp_id`: PNPDeviceID（WMI SMART 后备路径的匹配依据）。
/// 失败时返回 `SmartInfo { status: Unknown, has_smart: false, ... }`。
pub fn read_disk_smart(
    index: u32,
    is_nvme: bool,
    is_ssd: bool,
    model: &str,
    pnp_id: &str,
) -> SmartInfo {
    // 1. NVMe 优先：通过 IOCTL_STORAGE_QUERY_PROPERTY 读取
    if is_nvme {
        match read_nvme_smart_raw(index) {
            Ok(raw) => {
                let attrs = parse_nvme_attributes(&raw);
                // Life = 100 - PercentageUsed（CDI AtaSmart.cpp L148）
                let percentage_used = raw[5] as i32;
                let life = (100 - percentage_used).clamp(0, 100);
                let status = check_nvme_status(&attrs, life, model, THRESHOLD_FF);
                return SmartInfo {
                    status,
                    life_percent: Some(life as u8),
                    temperature_c: parse_nvme_temperature(&raw),
                    power_on_hours: parse_nvme_power_on_hours(&raw),
                    power_on_count: parse_nvme_power_cycles(&raw),
                    data_read_bytes: parse_nvme_data_bytes(&raw, 32),
                    data_written_bytes: parse_nvme_data_bytes(&raw, 48),
                    is_nvme: true,
                    has_smart: true,
                    error: None,
                };
            }
            Err(e) => {
                log::warn!("[SMART] PhysicalDrive{} NVMe 读取失败，回退 ATA: {}", index, e);
                // 继续尝试 ATA 路径（部分 NVMe 也支持 ATA 兼容命令）
            }
        }
    }

    // 2. ATA 路径：优先 DeviceIoControl（DFP_RECEIVE_DRIVE_DATA，需要管理员权限），
    //    失败时回退 WMI MSStorageDriver_FailurePredictData（无需管理员，CDI GetSmartAttributeWmi）
    let mut attrs_raw = read_ata_attributes(index);
    let mut ths_raw = read_ata_thresholds(index);
    if (attrs_raw.is_err() || ths_raw.is_err()) && !pnp_id.trim().is_empty() {
        log::warn!(
            "[SMART] PhysicalDrive{} ATA IOCTL 读取失败，回退 WMI (pnp={})",
            index,
            pnp_id
        );
        if attrs_raw.is_err() {
            attrs_raw = read_ata_smart_wmi(pnp_id, false);
        }
        if ths_raw.is_err() {
            ths_raw = read_ata_smart_wmi(pnp_id, true);
        }
    }

    match (attrs_raw, ths_raw) {
        (Ok(attrs_raw), Ok(ths_raw)) => {
            let attrs = parse_attributes(&attrs_raw);
            let ths = parse_thresholds(&ths_raw);
            let is_threshold_correct = ths.iter().any(|t| t.threshold_value != 0);
            let (status, life) = check_ata_status(&attrs, &ths, is_ssd, is_threshold_correct, THRESHOLD_FF);
            // HDD 无寿命属性，按状态映射健康度百分比（GOOD=100 / CAUTION=50 / BAD=0）
            let life_percent = life.map(|l| l.clamp(0, 100) as u8).or_else(|| {
                if is_ssd {
                    None
                } else {
                    match status {
                        DiskStatus::Good => Some(100),
                        DiskStatus::Caution => Some(50),
                        DiskStatus::Bad => Some(0),
                        DiskStatus::Unknown => None,
                    }
                }
            });
            SmartInfo {
                status,
                life_percent,
                temperature_c: parse_ata_temperature(&attrs),
                power_on_hours: parse_ata_power_on_hours(&attrs),
                power_on_count: parse_ata_power_cycles(&attrs),
                data_read_bytes: parse_ata_data_bytes(&attrs, 0xF2),
                data_written_bytes: parse_ata_data_bytes(&attrs, 0xF1),
                is_nvme: false,
                has_smart: true,
                error: None,
            }
        }
        (Ok(attrs_raw), Err(_)) => {
            // 无阈值数据：仅能解析温度/通电，健康判定为 UNKNOWN
            let attrs = parse_attributes(&attrs_raw);
            SmartInfo {
                status: DiskStatus::Unknown,
                life_percent: None,
                temperature_c: parse_ata_temperature(&attrs),
                power_on_hours: parse_ata_power_on_hours(&attrs),
                power_on_count: parse_ata_power_cycles(&attrs),
                data_read_bytes: parse_ata_data_bytes(&attrs, 0xF2),
                data_written_bytes: parse_ata_data_bytes(&attrs, 0xF1),
                is_nvme: false,
                has_smart: true,
                error: None,
            }
        }
        (Err(e), _) => {
            log::warn!("[SMART] PhysicalDrive{} 读取失败: {}", index, e);
            SmartInfo {
                status: DiskStatus::Unknown,
                life_percent: None,
                temperature_c: None,
                power_on_hours: None,
                power_on_count: None,
                data_read_bytes: None,
                data_written_bytes: None,
                is_nvme: false,
                has_smart: false,
                error: Some(e),
            }
        }
    }
}

/// 按 CDI NVMeInterpreter.cpp NVMeSmartToATASmart 的映射，将 NVMe log 解析为属性数组
fn parse_nvme_attributes(raw: &[u8; 512]) -> Vec<SmartAttribute> {
    // 仅构造 CheckDiskStatus 需要的属性（Id 对应 NVMe 原始字节）
    let mut attrs = Vec::new();
    // Id=1 Critical Warning（raw[0]）
    attrs.push(SmartAttribute {
        id: 1,
        raw_value: [raw[0], 0, 0, 0, 0, 0],
        ..Default::default()
    });
    // Id=3 Available Spare（raw[3]）
    attrs.push(SmartAttribute {
        id: 3,
        raw_value: [raw[3], 0, 0, 0, 0, 0],
        ..Default::default()
    });
    // Id=4 Spare Threshold（raw[4]）
    attrs.push(SmartAttribute {
        id: 4,
        raw_value: [raw[4], 0, 0, 0, 0, 0],
        ..Default::default()
    });
    attrs
}
