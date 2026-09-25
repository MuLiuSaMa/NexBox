//! BugCheck 代码知识库（移植自 BSODAnalyzer-main/bsod/bugcheck.py）
//! 只收录能在客户端给出可靠解释的常见码；未收录码走通用分析路径。

/// 归因大类
pub const CAT_DRIVER: &str = "driver";
pub const CAT_MEMORY: &str = "memory_hw";
pub const CAT_GRAPHICS: &str = "graphics";
pub const CAT_STORAGE: &str = "storage";
pub const CAT_POWER: &str = "power";
pub const CAT_USB: &str = "usb";
pub const CAT_SYSTEM: &str = "system";
pub const CAT_BOOT: &str = "boot";

/// 「码来自转储记录」标志位；带该位时按低 28 位查表。
pub const DUMP_VARIANT: u32 = 0x1000_0000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BugCheckMeta {
    pub name: &'static str,
    pub cn: &'static str,
    pub params: [&'static str; 4],
    pub category: &'static str,
    pub severity: &'static str,
    pub causes: &'static [&'static str],
    pub fixes: &'static [&'static str],
}

const _DRIVER_UPDATE: &str = "把相关驱动升级到厂商官网最新版（不要用第三方驱动大师）";
const _MEM_TEST: &str = "用 Windows 内存诊断（mdsched.exe）或 MemTest86 跑一晚，确认内存无故障";

/// 表条目：(code, meta)
pub const BUGCHECKS: &[(u32, BugCheckMeta)] = &[
    (0x0A, BugCheckMeta {
        name: "IRQL_NOT_LESS_OR_EQUAL",
        cn: "驱动在过高的 IRQL 上访问了它无权访问的内存",
        params: [
            "被引用的内存地址（若小于 0x1000 通常是空指针解引用）",
            "引用发生时的 IRQL（≥2 即为 DISPATCH_LEVEL 及以上）",
            "访问类型：0=读，1=写，8=执行指令",
            "引发异常的指令地址（在该地址所在模块中定位驱动）",
        ],
        category: CAT_DRIVER, severity: "high",
        causes: &[
            "第三方内核驱动访问了已释放或分页的内存",
            "驱动内存池被其它驱动破坏（常见于杀软/外设/虚拟化驱动共存）",
            "内存条硬件故障，导致读回的数据与写入不一致",
        ],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST, "卸载最近安装的杀毒软件、VPN 客户端、虚拟光驱或外设驱动后观察"],
    }),
    (0x19, BugCheckMeta {
        name: "BAD_POOL_HEADER",
        cn: "内存池头部被破坏或同一块池内存被释放两次",
        params: [
            "0x20=池块已损坏，参数 2 为池块地址，参数 3 为池类型，参数 4 为池标签字符",
            "池块地址（或子类型相关值）", "池类型（0=NonPaged，1=Paged）", "池标签（Tag）",
        ],
        category: CAT_DRIVER, severity: "high",
        causes: &["某驱动越界写坏了自己或别人的内存池块", "驱动重复释放同一块内存（double free）", "内存硬件不稳定"],
        fixes: &[_DRIVER_UPDATE, "运行 verifier /standard 对非微软驱动开启验证后复现，可锁定破坏者", _MEM_TEST],
    }),
    (0x1A, BugCheckMeta {
        name: "MEMORY_MANAGEMENT",
        cn: "内存管理器检测到页表或物理页数据结构异常",
        params: ["子类型码（极大影响解读方向）", "子类型相关值（常为页帧号或虚拟地址）", "子类型相关值", "子类型相关值"],
        category: CAT_MEMORY, severity: "high",
        causes: &["内存条物理故障（该码最常见的根因）", "驱动破坏页表项", "页面文件/磁盘故障导致换页数据损坏"],
        fixes: &[_MEM_TEST, "检查页面文件所在磁盘的 SMART 健康度", _DRIVER_UPDATE],
    }),
    (0x1E, BugCheckMeta {
        name: "KMODE_EXCEPTION_NOT_HANDLED",
        cn: "内核态线程产生了未被处理的异常",
        params: [
            "未处理异常的错误码（0xC0000005 为访问违例，0x80000003 为断点）",
            "异常发生的内存地址", "异常记录的地址", "异常发生的指令地址（据此定位驱动）",
        ],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动在非法地址上读写（最常见）", "参数 2 若为 0xC0000005，多为空指针或已释放内存访问", "内存/CPU 超频或硬件不稳"],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST, "关闭 CPU/内存 XMP 超频后用默认频率复现"],
    }),
    (0x24, BugCheckMeta {
        name: "NTFS_FILE_SYSTEM",
        cn: "NTFS 文件系统驱动内部出错",
        params: ["源文件与行号（形如 0x00000000NNTFSxxx）", "异常记录 / 线程信息", "异常记录 / 线程信息", "异常发生的地址"],
        category: CAT_STORAGE, severity: "high",
        causes: &["磁盘物理坏道或数据线接触不良，导致元数据读写失败", "NTFS 卷元数据损坏", "磁盘控制器驱动（iaStor / storahci）异常"],
        fixes: &["用 chkdsk /f /r 修复文件系统与坏道", "更换 SATA 数据线、改插另一个主板接口",
                 "用厂商工具（CrystalDiskInfo）读 SMART，重分配扇区数增长说明盘在坏"],
    }),
    (0x2E, BugCheckMeta {
        name: "DATA_BUS_ERROR",
        cn: "内存子系统或总线检测到奇偶校验错误",
        params: ["被引用的虚拟地址", "被引用的物理地址", "处理器状态", "出错时的指令地址"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["内存条损坏或金手指氧化（首要怀疑）", "内存插槽/供电不稳", "主板或 CPU 内存控制器故障"],
        fixes: &["拔插内存条并擦净金手指，逐条单插测试定位坏条", _MEM_TEST, "关闭 XMP/DOCP，恢复内存默认频率"],
    }),
    (0x31, BugCheckMeta {
        name: "PHASE0_INITIALIZATION_FAILED",
        cn: "内核初始化早期失败，系统无法启动",
        params: ["NTSTATUS 状态码（如参数 2 非 0，可据此判断具体失败环节）", "NTSTATUS 状态码", "参数 3", "参数 4"],
        category: CAT_BOOT, severity: "critical",
        causes: &["系统文件损坏", "磁盘/文件系统故障", "引导配置或驱动在启动阶段崩溃"],
        fixes: &["用安装 U 盘启动后执行 sfc /scannow 与 DISM /Online /Cleanup-Image /RestoreHealth",
                 "检查磁盘健康", "移除最近安装的驱动或外设后再启动"],
    }),
    (0x35, BugCheckMeta {
        name: "NO_MORE_IRP_STACK_LOCATIONS",
        cn: "I/O 请求包（IRP）的栈位置被耗尽",
        params: ["IRP 地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["第三方文件系统过滤驱动（杀软、加密、备份软件）嵌套层数过多", "驱动递归下发 IRP"],
        fixes: &["卸载多余的文件过滤驱动（杀软/加密盘/同步软件保留一个即可）", "更新该过滤驱动到最新版"],
    }),
    (0x3B, BugCheckMeta {
        name: "SYSTEM_SERVICE_EXCEPTION",
        cn: "系统调用过程中发生异常",
        params: ["异常错误码（0xC0000005 最常见）", "异常发生的地址", "异常记录地址", "参数 4"],
        category: CAT_SYSTEM, severity: "high",
        causes: &["驱动或系统组件执行系统调用时访问了非法内存", "第三方安全软件挂钩系统调用出错", "内存故障引起的偶发"],
        fixes: &[_DRIVER_UPDATE, "卸载/更新杀毒、VPN、沙箱类软件", _MEM_TEST],
    }),
    (0x44, BugCheckMeta {
        name: "MULTIPLE_IRP_COMPLETE_REQUESTS",
        cn: "同一个 I/O 请求被完成两次",
        params: ["IRP 地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["驱动存在逻辑缺陷（重复完成 IRP）", "第三方驱动与系统驱动冲突"],
        fixes: &["定位并更新对应驱动；开启 verifier 的 I/O 验证可精确定位"],
    }),
    (0x4E, BugCheckMeta {
        name: "PFN_LIST_CORRUPT",
        cn: "物理页帧号（PFN）数据库被破坏",
        params: ["子类型（0x1=页表项损坏，0x2=已释放页仍在链表，0x7=驱动解锁了不属于它的页面）",
                 "子类型相关值", "子类型相关值", "子类型相关值"],
        category: CAT_MEMORY, severity: "high",
        causes: &["驱动错误地操作了物理页结构", "内存条硬件故障（该码也很常见于坏内存）"],
        fixes: &[_MEM_TEST, _DRIVER_UPDATE, "对第三方驱动开启 Driver Verifier 复现定位"],
    }),
    (0x50, BugCheckMeta {
        name: "PAGE_FAULT_IN_NONPAGED_AREA",
        cn: "访问了本应常驻内存的不存在页面",
        params: ["被引用的内存地址（无效内存的地址）", "访问类型：0=读，1=写，8=执行",
                 "引发错误的指令地址（据此定位驱动）", "参数 4（部分版本为保留）"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动引用了已释放的内存或错误指针", "内存条/显存硬件故障", "系统服务包与驱动版本不匹配"],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST, "若参数 2=8（执行），多为驱动或安全软件在不可执行内存上跳转，优先更新该驱动"],
    }),
    (0x51, BugCheckMeta {
        name: "REGISTRY_ERROR",
        cn: "注册表子系统读写失败",
        params: ["子类型", "注册表配置单元地址", "参数 3", "参数 4"],
        category: CAT_STORAGE, severity: "high",
        causes: &["注册表配置单元所在磁盘出现坏道", "磁盘写入失败导致注册表文件损坏"],
        fixes: &["检查磁盘 SMART 与数据线", "用 chkdsk 修复", "从系统还原点恢复或备份还原"],
    }),
    (0x7A, BugCheckMeta {
        name: "KERNEL_DATA_INPAGE_ERROR",
        cn: "从页面文件读入内核数据时失败",
        params: ["读操作返回的状态码（0xC000000E=设备不存在，0xC000009C=设备数据校验错误，0xC0000185=I/O 错误）",
                 "操作状态（I/O 错误码）", "发生失败的页面文件偏移（高 32 位）", "发生失败的页面文件偏移（低 32 位）"],
        category: CAT_STORAGE, severity: "critical",
        causes: &["磁盘坏道或数据线/接口接触不良（最常见）", "页面文件所在磁盘空间不足或磁盘故障",
                 "磁盘控制器驱动异常，内存故障也可能触发"],
        fixes: &["优先用 chkdsk /r 扫描并标记坏道，用 CrystalDiskInfo 检查 SMART（重映射/待映射扇区）",
                 "更换 SATA 数据线与供电线，改插主板原生接口",
                 "把页面文件迁到另一块健康的磁盘上",
                 "参数 2 为 0xC000009C/0xC000016A 时几乎可确定是磁盘坏道"],
    }),
    (0x7B, BugCheckMeta {
        name: "INACCESSIBLE_BOOT_DEVICE",
        cn: "启动设备不可访问（系统盘控制器驱动缺失或模式改变）",
        params: ["设备对象地址或 NTSTATUS", "参数 2", "参数 3", "参数 4"],
        category: CAT_BOOT, severity: "critical",
        causes: &["BIOS 里 SATA 模式被改（AHCI ↔ RAID ↔ IDE）", "磁盘控制器驱动被替换/损坏",
                 "磁盘本身故障或数据线松动", "系统迁移/克隆到新硬盘后引导信息不匹配"],
        fixes: &["进 BIOS 把 SATA Mode 改回原来能启动的模式", "开启系统自带驱动兼容模式或注入对应存储驱动",
                 "检查硬盘数据线与磁盘供电"],
    }),
    (0x7E, BugCheckMeta {
        name: "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED",
        cn: "系统线程产生未处理异常",
        params: ["异常码", "异常发生的地址", "异常记录地址", "上下文记录地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动在系统线程中访问非法地址", "驱动与系统版本不兼容", "内存故障"],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST, "若参数 1 为 0xC0000005，重点排查最近更新的驱动"],
    }),
    (0x7F, BugCheckMeta {
        name: "UNEXPECTED_KERNEL_MODE_TRAP",
        cn: "CPU 触发内核无法处理的中断/陷阱",
        params: ["陷阱类型（0x0=除零，0x8=双重错误/内核栈溢出，0xD=通用保护错误，0xA/0xB=无效 TSS）",
                 "参数 2", "参数 3", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["参数 1 为 0x8（双重错误）时通常是内核栈溢出或硬件故障", "内存条损坏、CPU 超频/供电不足", "主板或 CPU 故障"],
        fixes: &[_MEM_TEST, "恢复 BIOS 默认设置，关闭超频与 XMP", "更新主板 BIOS", "检查 CPU 散热与供电"],
    }),
    (0x80, BugCheckMeta {
        name: "NMI_HARDWARE_FAILURE",
        cn: "硬件产生不可屏蔽中断（NMI）报告故障",
        params: ["NMI 来源（内存/PCI 总线/温度过高）", "参数 2", "参数 3", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["内存或主板硬件故障（需查厂商硬件诊断日志）", "机箱温度/CPU 温度过高触发保护", "电源输出不稳"],
        fixes: &["检查 CPU/机箱温度与散热器安装", _MEM_TEST, "更换电源测试", "联系整机厂商硬件检测"],
    }),
    (0x8E, BugCheckMeta {
        name: "KERNEL_MODE_EXCEPTION_NOT_HANDLED",
        cn: "内核模式异常未处理（多见于 Windows 7/XP）",
        params: ["异常错误码", "异常地址", "异常记录地址", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动访问非法内存", "内存故障"],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST],
    }),
    (0x9C, BugCheckMeta {
        name: "MACHINE_CHECK_EXCEPTION",
        cn: "CPU 报告机器检查异常（硬件级）",
        params: ["CPU 银行号与状态（参数 1 高 16 位含 Bank 号）", "CPU 的低 32 位 MCA 状态", "CPU 的高 32 位 MCA 状态", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["CPU 过热、超频或供电不稳", "内存/缓存故障", "主板 VRM 供电问题"],
        fixes: &["清理散热器与硅脂，确认温度正常后再测", "BIOS 恢复默认频率并关闭所有超频",
                 "检查电源功率是否够用", "用 MemTest86 全量测试内存"],
    }),
    (0x9F, BugCheckMeta {
        name: "DRIVER_POWER_STATE_FAILURE",
        cn: "驱动未正确处理电源状态切换（睡眠/休眠/关机时蓝屏）",
        params: ["设备对象类型（0x3=设备对象被阻塞，0x4=设备对象电源状态无效）",
                 "设备对象结构地址（可用 !devobj 定位驱动）",
                 "功能设备对象地址（该地址指向的驱动即为问题驱动）", "参数 4"],
        category: CAT_POWER, severity: "high",
        causes: &["网卡/声卡/蓝牙/USB 控制器驱动不支持现代待机（Modern Standby）",
                 "外设驱动在睡眠时未完成 I/O 就释放", "第三方驱动（虚拟网卡、虚拟声卡）与电源管理冲突"],
        fixes: &["更新网卡、蓝牙、声卡、USB 控制器驱动到厂商最新版",
                 "关闭网卡属性中「允许计算机关闭此设备以节约电源」的省电选项",
                 "临时用 powercfg /h off 关闭休眠或改用 S3 睡眠以规避"],
    }),
    (0xA5, BugCheckMeta {
        name: "ACPI_BIOS_ERROR",
        cn: "ACPI 与 BIOS/固件交互失败",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_BOOT, severity: "high",
        causes: &["BIOS/UEFI 版本过旧或设置异常", "主板固件与系统不兼容"],
        fixes: &["更新主板 BIOS 到最新版", "BIOS 中恢复默认设置", "关闭 BIOS 中的快速启动/Fast Boot"],
    }),
    (0xB4, BugCheckMeta {
        name: "VIDEO_DRIVER_INIT_FAILURE",
        cn: "显示驱动初始化失败",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显卡驱动损坏或不匹配", "显卡硬件故障", "核显/独显切换冲突"],
        fixes: &["用 DDU 在安全模式下彻底卸载显卡驱动后重装官网版本",
                 "在 BIOS 中切换显卡输出模式（独显直连/iGPU 优先）测试"],
    }),
    (0xB8, BugCheckMeta {
        name: "ATTEMPTED_SWITCH_FROM_DPC",
        cn: "在 DPC 例程中试图切换线程",
        params: ["旧线程地址", "新线程地址", "新线程的等待对象", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["驱动在 DPC 中调用了会导致线程切换的 API（如等待、分配可分页内存）"],
        fixes: &[_DRIVER_UPDATE, "对疑似驱动开启 Driver Verifier 复现定位"],
    }),
    (0xBE, BugCheckMeta {
        name: "ATTEMPTED_WRITE_TO_READONLY_MEMORY",
        cn: "试图向只读内存写入数据",
        params: ["被写入的虚拟地址", "页表项值（PTE）", "写操作发生时的指令地址（据此定位驱动）", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动试图修改只读数据段（多为自身代码或常量区）", "内存被其它驱动破坏"],
        fixes: &[_DRIVER_UPDATE, "关注参数 3 指向的驱动，它通常就是写错内存的那一个"],
    }),
    (0xC1, BugCheckMeta {
        name: "SPECIAL_POOL_DETECTED_MEMORY_CORRUPTION",
        cn: "特殊内存池检测到越界写入",
        params: ["写入发生的地址", "写入的字节数或池地址", "写入的物理地址/池地址", "池地址或保留值"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动写越界（超出分配长度写入）"],
        fixes: &[_DRIVER_UPDATE, "对第三方驱动开启 Driver Verifier 的 Special Pool 后复现"],
    }),
    (0xC2, BugCheckMeta {
        name: "BAD_POOL_CALLER",
        cn: "驱动以错误的方式调用内存池接口",
        params: ["子类型码（0x7=试图释放已释放的池，0x9=传递了非法地址）",
                 "池块地址 / 请求大小", "内存标签（Tag）", "出错时所在的功能地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动重复释放内存、释放非法指针、用错池类型",
                 "参数 3 的池标签可与 Pooltag 对照定位驱动（!poolfind <tag>）"],
        fixes: &[_DRIVER_UPDATE, "记录参数 3 的 Tag 值，用 pooltag 查询工具反查所属驱动"],
    }),
    (0xC4, BugCheckMeta {
        name: "DRIVER_VERIFIER_DETECTED_VIOLATION",
        cn: "驱动验证器捕获到驱动违规（已开启 verifier）",
        params: ["验证规则编号（0x50=已释放内存被使用，0xE1/MISMATCHED=等待时 IRQL 不一致）",
                 "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["被验的驱动存在明确的内存或 IRQL 使用错误"],
        fixes: &["参数 4 通常指向违规驱动，直接更新或卸载该驱动",
                 "若不知是哪个驱动，关闭 verifier（verifier /reset）避免持续蓝屏"],
    }),
    (0xC5, BugCheckMeta {
        name: "DRIVER_CORRUPTED_EXPOOL",
        cn: "系统在过高 IRQL 上访问了已损坏的分页内存池",
        params: ["被引用的内存地址", "IRQL", "访问类型：0=读 1=写", "出错指令地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动在 DISPATCH_LEVEL 及以上访问了可分页内存（现已无效）", "驱动内存被破坏"],
        fixes: &[_DRIVER_UPDATE, _MEM_TEST],
    }),
    (0xC6, BugCheckMeta {
        name: "DRIVER_CAUGHT_MODIFYING_FREED_POOL",
        cn: "驱动访问了已释放的内存池",
        params: ["被访问的内存地址", "IRQL", "访问类型：0=读 1=写 8=执行", "出错指令地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["典型的 use-after-free，驱动自身缺陷或版本不匹配"],
        fixes: &[_DRIVER_UPDATE, "参数 4 指向的驱动即为嫌疑驱动"],
    }),
    (0xC7, BugCheckMeta {
        name: "TIMER_OR_DPC_INVALID",
        cn: "内核定时器或 DPC 对象处于非法状态",
        params: ["定时器地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["驱动使用了已释放或未初始化的定时器/DPC 对象"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xC9, BugCheckMeta {
        name: "DRIVER_VERIFIER_IOMANAGER_VIOLATION",
        cn: "驱动验证器 I/O 管理器捕获违规",
        params: ["违规类型编号", "参数 2", "参数 3", "参数 4（常指向违规驱动）"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动 I/O 处理逻辑违规（重复完成 IRP、未取消挂起请求等）"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xCE, BugCheckMeta {
        name: "DRIVER_UNLOADED_WITHOUT_CANCELLING_PENDING_OPERATIONS",
        cn: "驱动卸载前未取消挂起的操作",
        params: ["驱动在内存中的地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动卸载时残留挂起 I/O，卸载后被系统调用导致崩溃"],
        fixes: &["参数 1 地址落位到的驱动即为元凶，更新或卸载之", "排查与设备热插拔、驱动动态卸载相关的软件"],
    }),
    (0xD1, BugCheckMeta {
        name: "DRIVER_IRQL_NOT_LESS_OR_EQUAL",
        cn: "驱动在过高 IRQL 上访问了可分页内存（最典型的驱动故障码）",
        params: ["被引用的内存地址（可从该地址落位驱动）", "引用发生时的 IRQL",
                 "访问类型：0=读，1=写，8=执行", "引用内存的指令地址（该地址所在驱动就是元凶）"],
        category: CAT_DRIVER, severity: "high",
        causes: &["网卡/无线网卡驱动在高 IRQL 上下文访问分页内存（最常见的具体场景）",
                 "杀毒软件、VPN、虚拟网卡驱动的过滤回调出错", "驱动版本与系统补丁不匹配"],
        fixes: &["按参数 4 落位到的驱动优先更新（网卡、无线网卡、VPN 虚拟网卡、杀软过滤驱动）",
                 _DRIVER_UPDATE, "卸载最近新增的网络类软件（VPN/代理/加速器/虚拟网卡）后观察"],
    }),
    (0xD5, BugCheckMeta {
        name: "DRIVER_PAGE_FAULT_IN_FREED_SPECIAL_POOL",
        cn: "驱动访问了已释放的特殊内存池",
        params: ["被引用的内存地址", "参数 2", "参数 3", "出错指令地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动 use-after-free"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xD6, BugCheckMeta {
        name: "DRIVER_PAGE_FAULT_BEYOND_END_OF_ALLOCATION",
        cn: "驱动访问了超出分配范围的内存",
        params: ["被访问的内存地址", "参数 2", "参数 3", "出错指令地址"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动读写越界"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xD8, BugCheckMeta {
        name: "DRIVER_USED_EXCESSIVE_PTES",
        cn: "驱动占用了过多的页表项（PTE）",
        params: ["驱动映像的基地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["驱动存在内存泄漏，长期不释放映射"],
        fixes: &["参数 1 落位到的驱动需更新或替换（常见于老旧的外设/监控驱动）"],
    }),
    (0xDA, BugCheckMeta {
        name: "SYSTEM_PTE_MISUSE",
        cn: "系统页表项被错误使用",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动错误操作页表映射"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xE3, BugCheckMeta {
        name: "RESOURCE_NOT_OWNED",
        cn: "驱动释放了自己并不持有的资源",
        params: ["资源地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["驱动资源管理逻辑缺陷"],
        fixes: &[_DRIVER_UPDATE],
    }),
    (0xE6, BugCheckMeta {
        name: "DRIVER_VERIFIER_DMA_VIOLATION",
        cn: "驱动验证器捕获到 DMA 违规",
        params: ["违规子类型", "违规驱动地址/参数", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动 DMA 使用不当（常见于虚拟化、网卡、存储驱动）"],
        fixes: &[_DRIVER_UPDATE, "关闭 BIOS 中的 VT-d/IOMMU 后测试可辅助判断"],
    }),
    (0xEA, BugCheckMeta {
        name: "THREAD_STUCK_IN_DEVICE_DRIVER",
        cn: "线程卡死在设备驱动中（显卡驱动最常见）",
        params: ["阻塞线程的驱动对象地址（!analyze 可查出驱动名）", "该线程的等待状态", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显示驱动等待显卡硬件响应超时，显卡驱动或硬件故障", "外设驱动死锁"],
        fixes: &["用 DDU 彻底清理后重装显卡驱动（优先建议回退到厂商稳定版）",
                 "检查显卡供电、温度与超频设置", "更新主板芯片组与 BIOS"],
    }),
    (0xED, BugCheckMeta {
        name: "UNMOUNTABLE_BOOT_VOLUME",
        cn: "系统卷无法挂载（多表现为开机即蓝屏）",
        params: ["磁盘设备对象 / NTSTATUS 状态码", "参数 2", "参数 3", "参数 4"],
        category: CAT_STORAGE, severity: "critical",
        causes: &["数据线接触不良或硬盘坏道（经典原因）", "文件系统损坏", "SSD 固件问题或掉盘"],
        fixes: &["更换数据线并检查供电；用安装介质启动执行 chkdsk /f /r",
                 "检查硬盘 SMART；SSD 建议升级固件", "必要时重装系统或恢复镜像"],
    }),
    (0xEF, BugCheckMeta {
        name: "CRITICAL_PROCESS_DIED",
        cn: "系统关键进程（如 csrss.exe、wininit.exe）意外终止",
        params: ["终止的进程对象地址（可用 !process 查出进程名）", "参数 2", "参数 3", "参数 4"],
        category: CAT_SYSTEM, severity: "high",
        causes: &["杀毒/安全软件误杀或拦截系统进程", "系统文件损坏", "驱动在系统进程上下文中崩溃，进而拖垮进程"],
        fixes: &["执行 sfc /scannow 与 DISM 修复系统文件",
                 "卸载第三方安全/优化/汉化类软件后观察",
                 "查 Application 与 System 日志中该进程的崩溃记录"],
    }),
    (0xF4, BugCheckMeta {
        name: "CRITICAL_OBJECT_TERMINATION",
        cn: "关键系统进程或线程被异常终止",
        params: ["0x3=关键进程终止，0x6=关键线程终止", "进程/线程对象地址",
                 "进程映像名地址（可读出进程名）", "参数 4"],
        category: CAT_STORAGE, severity: "critical",
        causes: &["系统盘 I/O 失败（磁盘掉线、SSD 掉盘）导致关键进程无法读盘而终止（最常见）",
                 "文件系统或磁盘驱动故障"],
        fixes: &["重点检查系统盘 SMART 与数据线，SSD 检查固件与掉盘记录（事件 ID 129/153）",
                 "执行 chkdsk /f /r", "排查磁盘控制器驱动（iaStor/stornvme 等）"],
    }),
    (0xF7, BugCheckMeta {
        name: "DRIVER_OVERRAN_STACK_BUFFER",
        cn: "驱动检测到栈缓冲区溢出（/GS 保护被触发）",
        params: ["栈溢出检测码", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动存在栈缓冲区溢出漏洞（很可能是被恶意构造的数据触发）"],
        fixes: &[_DRIVER_UPDATE, "参数 2/3 可落位到具体驱动与函数"],
    }),
    (0xFC, BugCheckMeta {
        name: "ATTEMPTED_EXECUTE_OF_NOEXECUTE_MEMORY",
        cn: "试图执行标记为不可执行的内存",
        params: ["被执行的虚拟地址", "页表项值（0x1=页面有效但不可执行）",
                 "引发异常的指令地址（据此定位驱动）", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动跳转到错误地址（内存被破坏或函数指针被改写）", "安全软件在不可执行内存上的挂钩处理不当"],
        fixes: &[_DRIVER_UPDATE, "结合参数 3 定位的驱动优先处理"],
    }),
    (0xFE, BugCheckMeta {
        name: "BUGCODE_USB_DRIVER",
        cn: "USB 驱动栈内部错误",
        params: ["错误子类型", "参数 2", "参数 3", "参数 4"],
        category: CAT_USB, severity: "medium",
        causes: &["USB 控制器/集线器驱动缺陷，或外设本身异常"],
        fixes: &["更新主板芯片组与 USB 控制器驱动",
                 "逐个拔掉 USB 外设（尤其是 USB 网卡、扩展坞、采集卡）定位"],
    }),
    (0x101, BugCheckMeta {
        name: "CLOCK_WATCHDOG_TIMEOUT",
        cn: "多核 CPU 之间失去心跳响应（某核心被长时间挂住）",
        params: ["时钟中断超时的处理器编号（PRCB 地址）", "当前时钟中断的处理器编号",
                 "超时时间（单位 0.1 纳秒）", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["CPU 供电不稳、超频、BIOS 版本过旧或微码问题",
                 "某驱动在 DISPATCH_LEVEL 长时间霸占 CPU 导致其它核心失联",
                 "CPU 散热不良导致降频挂起"],
        fixes: &["恢复 BIOS 默认设置，关闭超频/XMP，更新主板 BIOS 与 CPU 微码",
                 "检查温度与供电（电源功率余量）", "结合时间线上其它错误判断是否为驱动长时间占用导致"],
    }),
    (0x109, BugCheckMeta {
        name: "CRITICAL_STRUCTURE_CORRUPTION",
        cn: "内核关键结构（代码或数据）被非法修改",
        params: ["被破坏的结构类型", "被破坏的地址", "参数 3", "参数 4"],
        category: CAT_SYSTEM, severity: "high",
        causes: &["驱动非法修改内核代码/数据（常见于安全软件、外挂、调试工具、旧版驱动）",
                 "内存故障导致数据位翻转"],
        fixes: &["卸载安全软件、系统优化/破解/外挂类工具后观察", _MEM_TEST,
                 "确认没有启用内核调试或第三方内核钩子"],
    }),
    (0x10E, BugCheckMeta {
        name: "VIDEO_MEMORY_MANAGEMENT_INTERNAL",
        cn: "显示驱动显存管理内部错误",
        params: ["错误子类型", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显卡驱动缺陷或显存故障", "驱动与系统版本不匹配"],
        fixes: &["用 DDU 清理后重装官网驱动", "若持续出现需考虑显卡硬件故障（尤其伴随花屏）"],
    }),
    (0x116, BugCheckMeta {
        name: "VIDEO_TDR_FAILURE",
        cn: "显示驱动在超时时间内未响应，系统尝试复位显卡失败",
        params: ["负责该显卡的驱动对象地址（可用 !analyze -v 查出驱动名）", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显卡驱动版本问题（该码最常见的根因）", "显卡过热、供电不足、超频",
                 "显卡硬件老化/虚焊，或与主板 PCIe 兼容性差"],
        fixes: &["用 DDU 在安全模式彻底卸载后安装厂商官网稳定版驱动（避免用自动更新推送的版本）",
                 "清理显卡散热器灰尘、检查供电线是否插紧",
                 "关闭显卡超频，进 BIOS 把 PCIe 速率降为 Gen3 测试",
                 "若更换驱动后仍频繁 TDR，考虑显卡硬件故障（可换卡验证）"],
    }),
    (0x117, BugCheckMeta {
        name: "VIDEO_TDR_TIMEOUT_DETECTED",
        cn: "检测到显示驱动超时未响应",
        params: ["显卡驱动对象地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["同 TDR 类问题：驱动缺陷、过热、供电或硬件故障"],
        fixes: &["DDU 重装显卡驱动", "检查散热与供电、关闭超频", "必要时替换显卡验证"],
    }),
    (0x119, BugCheckMeta {
        name: "VIDEO_SCHEDULER_INTERNAL_ERROR",
        cn: "显示调度器内部错误",
        params: ["错误子类型", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显卡驱动或图形子系统异常（多驱动叠加时更常见）"],
        fixes: &["DDU 清理后重装单一显卡驱动，避免混装多版本", "更新系统补丁"],
    }),
    (0x124, BugCheckMeta {
        name: "WHEA_UNCORRECTABLE_ERROR",
        cn: "硬件报告不可纠正错误（WHEA）—— 基本可判定为硬件问题",
        params: ["WHEA 错误源的标识（PCIe 设备的 Bus/Device/Function 等）",
                 "WHEA 错误来源的地址（可用 !errrec 查看错误记录）",
                 "MCI 状态（机器检查）", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["CPU 电压/供电异常、超频不稳（非常常见）", "内存条故障或插槽接触不良",
                 "PCIe 设备（显卡/NVMe/采集卡）接触或供电问题", "主板 VRM 老化、电源输出不稳"],
        fixes: &["BIOS 完全恢复默认，关闭超频/XMP/自动超频（如 MSI Game Boost、ASUS AI 超频）",
                 "拔插内存条和 PCIe 设备、擦净金手指，逐件替换测试",
                 "更新主板 BIOS 与芯片组驱动；若近期新增硬件，优先移除测试",
                 "查看事件日志中 WHEA-Logger 事件的详细错误来源（本工具已列出）"],
    }),
    (0x12B, BugCheckMeta {
        name: "FAULTY_HARDWARE_CORRUPTED_PAGE",
        cn: "内存错误校验发现单个页面数据被破坏（硬件级）",
        params: ["出现位翻转的物理页地址", "参数 2", "参数 3", "参数 4"],
        category: CAT_MEMORY, severity: "critical",
        causes: &["内存条硬件故障（该码几乎直接指向坏内存）"],
        fixes: &[_MEM_TEST, "逐条内存单插定位坏条并更换", "检查内存频率/电压设置是否过高"],
    }),
    (0x133, BugCheckMeta {
        name: "DPC_WATCHDOG_VIOLATION",
        cn: "DPC 或 ISR 长时间未返回，看门狗超时",
        params: ["0x0=单个 DPC/ISR 超时，0x1=系统整体在 DISPATCH_LEVEL 累计超时",
                 "超时的时间限制（时钟周期）", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["存储控制器驱动（iaStor、stornvme、storahci）在 SSD/HDD 上执行了过长的 DPC",
                 "显卡驱动、网卡驱动在中断上下文中耗时过长",
                 "SSD 固件缺陷或磁盘即将故障导致 I/O 挂起"],
        fixes: &["优先更新存储控制器驱动与 SSD 固件（AHCI/NVMe 驱动）",
                 "检查磁盘 SMART 与事件日志中的 129/153 事件", _DRIVER_UPDATE],
    }),
    (0x139, BugCheckMeta {
        name: "KERNEL_SECURITY_CHECK_FAILURE",
        cn: "内核检测到关键数据结构被破坏（安全检查失败）",
        params: ["0x0..0x5 为破坏类型（0x3=列表项损坏，最常见）", "被破坏结构的地址", "参数 3", "参数 4"],
        category: CAT_SYSTEM, severity: "high",
        causes: &["驱动越界写或 use-after-free 破坏了内核链表", "内存条故障导致数据损坏", "第三方安全软件的内核钩子异常"],
        fixes: &[_MEM_TEST, "卸载安全软件/优化工具后观察", "开启 Driver Verifier 复现以锁定破坏者",
                 "检查是否为已知的系统漏洞（保持系统更新）"],
    }),
    (0x13A, BugCheckMeta {
        name: "KERNEL_MODE_HEAP_CORRUPTION",
        cn: "内核堆被破坏",
        params: ["堆所有者地址（可落位到具体驱动）", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "high",
        causes: &["驱动写越界或释放后继续使用内存"],
        fixes: &["参数 1 通常指向堆的所有者驱动，优先更新它", _MEM_TEST],
    }),
    (0x144, BugCheckMeta {
        name: "BUGCODE_USB3_DRIVER",
        cn: "USB 3.0 驱动栈内部错误",
        params: ["错误子类型", "参数 2", "参数 3", "参数 4"],
        category: CAT_USB, severity: "medium",
        causes: &["USB 3.0 主控驱动缺陷或外设异常"],
        fixes: &["更新芯片组/USB 3.0 驱动", "拔除非必要 USB 外设（扩展坞、采集卡、外接硬盘盒）定位"],
    }),
    (0x147, BugCheckMeta {
        name: "ABNORMAL_RESET_DETECTED",
        cn: "检测到设备/系统异常复位",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_MEMORY, severity: "high",
        causes: &["供电异常或硬件复位（与 0x14C 同源）"],
        fixes: &["检查电源与供电线、主板电容状态", "更新 BIOS", "排查是否与显示驱动复位（TDR）相关"],
    }),
    (0x14C, BugCheckMeta {
        name: "FATAL_ABNORMAL_RESET_ERROR",
        cn: "致命异常复位（设备无响应后被强行复位）",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_GRAPHICS, severity: "high",
        causes: &["显卡/显示驱动在超时后复位失败", "电源供电不稳导致设备掉线"],
        fixes: &["DDU 重装显卡驱动", "检查电源功率与显卡供电线", "关闭超频"],
    }),
    (0x154, BugCheckMeta {
        name: "UNEXPECTED_STORE_EXCEPTION",
        cn: "存储子系统产生未预期异常（存储驱动/SSD 相关）",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_STORAGE, severity: "high",
        causes: &["SSD 固件缺陷、存储驱动缺陷或盘体故障导致 I/O 异常"],
        fixes: &["更新 SSD 固件与存储控制器驱动", "检查 SMART 与事件 129/153",
                 "更换 SATA/NVMe 接口或数据线测试"],
    }),
    (0x15E, BugCheckMeta {
        name: "BUGCODE_NDIS_DRIVER_LIVE_DUMP",
        cn: "NDIS 网络驱动栈内部错误",
        params: ["错误子类型", "参数 2", "参数 3", "参数 4"],
        category: CAT_DRIVER, severity: "medium",
        causes: &["网卡驱动或网络过滤驱动（VPN/加速器/杀软网络模块）缺陷"],
        fixes: &["更新网卡驱动", "卸载 VPN/代理/加速器/虚拟网卡软件后观察"],
    }),
    (0x191, BugCheckMeta {
        name: "PF_DETECTED_CORRUPTION",
        cn: "页面文件（PF）检测到数据损坏",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_STORAGE, severity: "high",
        causes: &["页面文件所在磁盘出现坏道或写入错误"],
        fixes: &["检查系统盘与页面文件所在盘的健康度", "执行 chkdsk /f /r", "把页面文件迁移到另一块磁盘测试"],
    }),
    (0x1C8, BugCheckMeta {
        name: "MANUALLY_INITIATED_POWER_BUTTON_HOLD",
        cn: "长按电源键强制关机后记录了本次事件（非系统崩溃）",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_POWER, severity: "low",
        causes: &["用户长按电源键断电，或系统已死机只能强制断电 —— 属于「结果」而非「原因」"],
        fixes: &["该记录本身不是故障原因，请结合同时间点的死机现象（画面冻结、风扇狂转）判断真实原因",
                 "若反复出现，按「系统卡死」方向排查：内存、显卡驱动、磁盘 I/O"],
    }),
    (0x1CA, BugCheckMeta {
        name: "SYNTHETIC_WATCHDOG_TIMEOUT",
        cn: "系统整体无响应，合成看门狗主动触发蓝屏",
        params: ["参数 1", "参数 2", "参数 3", "参数 4"],
        category: CAT_SYSTEM, severity: "high",
        causes: &["系统整体卡死（画面冻结、鼠标不动）后由看门狗补记蓝屏",
                 "常见根因：显卡驱动、存储驱动长时间阻塞、硬件过热"],
        fixes: &["结合崩溃前的时间线事件判断（磁盘超时、WHEA 硬件错误、显卡 TDR）",
                 "更新显卡与存储驱动", "检查散热与温度"],
    }),
];

/// 去掉「由转储反读」标志位，得到基础码。
pub fn base_code(code: u32) -> u32 {
    let c = code & 0xFFFF_FFFF;
    if c & DUMP_VARIANT != 0 { c & !DUMP_VARIANT } else { c }
}

/// 查表；命中返回 `&'static BugCheckMeta`，未命中返回 None。
pub fn lookup(code: u32) -> Option<&'static BugCheckMeta> {
    let c = base_code(code);
    BUGCHECKS.iter().find(|(k, _)| *k == c).map(|(_, v)| v)
}

/// 显示名；未收录返回 "未收录的 BugCheck 码"，code=None 返回 "未知"。
pub fn name_of(code: Option<u32>) -> String {
    match code {
        None => "未知".to_string(),
        Some(c) => lookup(c).map(|m| m.name.to_string()).unwrap_or_else(|| "未收录的 BugCheck 码".to_string()),
    }
}

/// 统一显示写法 `0x0000007E`。
pub fn format_code(code: u32) -> String {
    format!("0x{:08X}", code & 0xFFFF_FFFF)
}

/// 通用参数描述（未收录码使用）。
pub const GENERIC_PARAMS: [&str; 4] = [
    "参数 1（含义随该 BugCheck 的子类型变化）",
    "参数 2（含义随该 BugCheck 的子类型变化）",
    "参数 3（含义随该 BugCheck 的子类型变化）",
    "参数 4（含义随该 BugCheck 的子类型变化）",
];

/// 未收录码的通用 causes / fixes。
pub fn generic_causes(code: u32) -> (Vec<String>, Vec<String>) {
    if let Some(info) = lookup(code) {
        return (info.causes.iter().map(|s| s.to_string()).collect(),
                info.fixes.iter().map(|s| s.to_string()).collect());
    }
    (
        vec![
            "该 BugCheck 码未收录于本地知识库，需结合参数形态与事件日志判断".to_string(),
            "常见方向：第三方驱动缺陷、内存硬件故障、磁盘 I/O 异常".to_string(),
        ],
        vec![
            "把 BugCheck 码 + 参数 + 疑似模块一并提供给专业人员进一步分析".to_string(),
            "先做两件通用动作：Windows 内存诊断 + 驱动全部更新到厂商官网版本".to_string(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known() {
        assert_eq!(lookup(0x0A).unwrap().name, "IRQL_NOT_LESS_OR_EQUAL");
        assert_eq!(lookup(0x124).unwrap().name, "WHEA_UNCORRECTABLE_ERROR");
        assert_eq!(lookup(0x116).unwrap().name, "VIDEO_TDR_FAILURE");
        assert_eq!(lookup(0x7A).unwrap().name, "KERNEL_DATA_INPAGE_ERROR");
        assert_eq!(lookup(0x133).unwrap().name, "DPC_WATCHDOG_VIOLATION");
    }

    #[test]
    fn lookup_unknown() {
        assert!(lookup(0xDEAD_BEEF).is_none());
    }

    #[test]
    fn dump_variant_normalized() {
        // 0x1000007E 应查表为 0x7E SYSTEM_THREAD_EXCEPTION_NOT_HANDLED
        assert_eq!(lookup(0x1000_007E).unwrap().name, "SYSTEM_THREAD_EXCEPTION_NOT_HANDLED");
    }
}
