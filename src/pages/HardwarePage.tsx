import {
  Box,
  Heading,
  Text,
  VStack,
  HStack,
  useColorModeValue,
  Grid,
  Button,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  useDisclosure,
  Tooltip,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useAppStartup } from "@/contexts/app-startup-context";
import { useBackground } from "@/contexts/background-context";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import HardwareDetailModal, { type SpecItem } from "@/components/HardwareDetailModal";
import type { CpuInfo, GpuInfo, MemoryInfo, MotherboardInfo, DiskDetailInfo, SoundCardInfo, NetworkCardInfo, MonitorInfo } from "@/lib/hardware";
import {
  Cpu,
  Monitor,
  MemoryStick as Ram,
  Database,
  CircuitBoard,
  HardDrive,
  Volume2,
  Wifi,
  Download,
  Trash2,
  ChevronDown,
  Activity,
  Copy,
} from "lucide-react";
import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useHardwareReportExport } from "@/lib/use-hardware-report-export";
import { PawnioInstallModal } from "@/components/PawnioInstallModal";
import { BrandLogo } from "@/components/hardware-brand-logo";
import { RemoteMonitorDialog } from "@/components/remote-monitor-dialog";

interface DisplayInfo {
  name: string;
  value: string;
}

interface MemoryStatus {
  total: number;
  available: number;
  used: number;
  usage_percent: number;
}

interface DiskInfo {
  name: string;
  total_gb: number;
  available_gb: number;
  used_gb: number;
  usage_percent: number;
}

/** 文字过长时自动轮播：没超出不滚；超出时从右侧进入、向左滚出、再从右侧滚回 */
function MarqueeText({ text, color }: { text: string; color?: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLSpanElement>(null);
  const [overflow, setOverflow] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    const measure = () => {
      if (!container || !measureRef.current) return;
      const cw = container.clientWidth;
      const tw = measureRef.current.offsetWidth;
      // 布局未完成（宽度为 0）时跳过，避免误判
      if (cw === 0) return;
      setOverflow(tw > cw + 1);
    };
    measure();
    // 下一帧再测一次，确保字体/布局就绪，避免初次宽度为 0 导致卡在「不滚」
    const raf = requestAnimationFrame(measure);
    const ro = new ResizeObserver(measure);
    ro.observe(container);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [text]);

  return (
    <Tooltip label={text} isDisabled={!overflow}>
      <Box
        ref={containerRef}
        flex={1}
        overflow="hidden"
        position="relative"
        display="flex"
        justifyContent={overflow ? "flex-start" : "flex-end"}
        whiteSpace="nowrap"
      >
      <style>{`
        @keyframes hw-marquee {
          0%   { transform: translateX(0); }
          100% { transform: translateX(-50%); }
        }
      `}</style>
      {/* 隐藏测量副本：精确取得单份文本宽度，不受动画/截断影响 */}
      <Text
        as="span"
        ref={measureRef}
        position="absolute"
        visibility="hidden"
        whiteSpace="nowrap"
        fontSize="sm"
        color={color}
      >
        {text}
      </Text>
      {overflow ? (
        <Box
          as="span"
          display="inline-flex"
          whiteSpace="nowrap"
          style={{
            animation: "hw-marquee 12s linear infinite",
            willChange: "transform",
          }}
        >
          <Text as="span" fontSize="sm" color={color} pr="24px">{text}</Text>
          <Text as="span" fontSize="sm" color={color} pr="24px">{text}</Text>
        </Box>
      ) : (
        <Text as="span" fontSize="sm" color={color} noOfLines={1} flexShrink={0}>
          {text}
        </Text>
      )}
      </Box>
    </Tooltip>
  );
}

interface GpuSensorData {
  name: string;
  hardware_type: string;
  temperature: number | null;
  usage: number | null;
  fan_speed: number | null;
  power: number | null;
  clock: number | null;
  memory_clock: number | null;
  vram_used: number | null;
  vram_total: number | null;
  voltage: number | null;
}

function Sparkline({ data, color }: { data: number[]; color: string }) {
  if (data.length < 2) return null;

  const width = 120;
  const height = 40;
  const maxVal = Math.max(...data, 100);
  const minVal = Math.min(...data, 0);
  const range = maxVal - minVal || 1;

  const points = data.map((val, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - ((val - minVal) / range) * height;
    return `${x},${y}`;
  }).join(" ");

  return (
    <svg width={width} height={height} style={{ overflow: "visible" }}>
      <polyline
        points={points}
        fill="none"
        stroke={color}
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity={0.8}
      />
      <defs>
        <linearGradient id={`grad-${color.replace("#", "")}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.3" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon
        points={`0,${height} ${points.split(" ").join(" ")} ${width},${height}`}
        fill={`url(#grad-${color.replace("#", "")})`}
      />
    </svg>
  );
}

function StatCard({
  title,
  titleContent,
  value,
  subValue,
  color,
  sparklineData,
  icon: IconComponent,
  cardBg,
  textColor,
  subTextColor,
  liquidGlassEnabled,
}: {
  title?: string;
  titleContent?: React.ReactNode;
  value: string;
  subValue?: string;
  color: string;
  sparklineData: number[];
  icon: React.ElementType;
  cardBg: string;
  textColor: string;
  subTextColor: string;
  liquidGlassEnabled: boolean;
}) {
  const cardContent = (
    <Box position="relative" overflow="hidden" p={5} height="140px">
      <VStack align="start" spacing={2} position="relative" zIndex={2}>
        <HStack spacing={2}>
          <IconComponent size={16} color={color} />
          {titleContent || (
            <Text fontSize="sm" color={subTextColor} fontWeight="medium">
              {title}
            </Text>
          )}
        </HStack>
        <HStack spacing={2} align="baseline">
          <Text fontSize="3xl" fontWeight="bold" color={textColor}>
            {value}
          </Text>
          {subValue && (
            <Text fontSize="xs" color={subTextColor}>
              {subValue}
            </Text>
          )}
        </HStack>
      </VStack>
      <Box position="absolute" bottom={2} left={2} zIndex={1}>
        <Sparkline data={sparklineData} color={color} />
      </Box>
    </Box>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard
        borderRadius="xl"
        overflow="hidden"
        position="relative"
        borderColor="white"
      >
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      borderRadius="xl"
      border="1px solid"
      borderColor="white"
      overflow="hidden"
      position="relative"
    >
      {cardContent}
    </Box>
  );
}

function DetailCard({
  title,
  icon: IconComponent,
  info,
  type,
  cardBg,
  textColor,
  subTextColor,
  liquidGlassEnabled,
  onClick,
  brandText,
}: {
  title: string;
  icon: React.ElementType;
  info: DisplayInfo[];
  type: string;
  cardBg: string;
  textColor: string;
  subTextColor: string;
  liquidGlassEnabled: boolean;
  onClick?: () => void;
  brandText?: string;
}) {
  const iconColor =
    type === "cpu"
      ? "#3b82f6"
      : type === "gpu"
        ? "#22c55e"
        : type === "memory"
          ? "#06b6d4"
          : type === "storage"
            ? "#a855f7"
            : type === "sound"
              ? "#f97316"
              : type === "network"
                ? "#14b8a6"
                : "#f59e0b";

  const cardContent = (
    <Box position="relative" overflow="hidden" p={5} minH="140px">
      <VStack align="start" spacing={3} position="relative" zIndex={2}>
        <HStack spacing={2} width="full">
          <IconComponent size={18} color={iconColor} />
          <Text fontSize="md" fontWeight="bold" color={textColor} flex={1}>
            {title}
          </Text>
          <BrandLogo text={brandText} />
        </HStack>

        <VStack align="start" spacing={1.5} width="full">
          {info.map((item, index) => (
            <HStack key={index} justify="space-between" width="full" spacing={3}>
              <Text fontSize="sm" color={subTextColor} noOfLines={1} flexShrink={0} maxW="35%">
                {item.name}
              </Text>
              <MarqueeText text={item.value} color={textColor} />
            </HStack>
          ))}
        </VStack>
      </VStack>
    </Box>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard
        borderRadius="xl"
        overflow="hidden"
        position="relative"
        borderColor="white"
        cursor={onClick ? "pointer" : undefined}
        onClick={onClick}
      >
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      borderRadius="xl"
      border="1px solid"
      borderColor="white"
      overflow="hidden"
      position="relative"
      cursor={onClick ? "pointer" : undefined}
      onClick={onClick}
    >
      {cardContent}
    </Box>
  );
}

export default function HardwarePage() {
  const toast = useDynamicIsland("cpu");
  const { hardwareInfo } = useAppStartup();
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const adaptiveTitle = useAdaptiveTextColor();
  const { exportReport, isExporting } = useHardwareReportExport();
  const { isOpen: isPawnioModalOpen, onOpen: onPawnioModalOpen, onClose: onPawnioModalClose } = useDisclosure();
  
  const cardBg = useColorModeValue("white", "#111111");
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const btnBorderColor = useColorModeValue("gray.300", "#333333");

  const [cpuLoad, setCpuLoad] = useState<number | null>(null);
  const [cpuTemp, setCpuTemp] = useState<number | null>(null);
  const [gpuSensors, setGpuSensors] = useState<GpuSensorData[]>([]);
  const [activeGpuIndex, setActiveGpuIndex] = useState(0);
  const [memoryStatus, setMemoryStatus] = useState<MemoryStatus | null>(null);
  const [diskStatus, setDiskStatus] = useState<DiskInfo | null>(null);

  const [cpuSparkline, setCpuSparkline] = useState<number[]>(Array(20).fill(0));
  const [gpuSparkline, setGpuSparkline] = useState<number[]>(Array(20).fill(0));
  const [memSparkline, setMemSparkline] = useState<number[]>(Array(20).fill(0));
  const [storageSparkline, setStorageSparkline] = useState<number[]>(Array(20).fill(0));

  // Modal state for detail expansion
  const { isOpen: isDetailOpen, onOpen: onDetailOpen, onClose: onDetailClose } = useDisclosure();
  const [detailCard, setDetailCard] = useState<{
    title: string;
    icon: React.ElementType;
    type: string;
    specs: SpecItem[];
  } | null>(null);

  const handleOpenDetail = (title: string, icon: React.ElementType, type: string, specs: SpecItem[]) => {
    setDetailCard({ title, icon, type, specs });
    onDetailOpen();
  };

  // GPU 切换处理
  const handleGpuSwitch = async (index: number) => {
    setActiveGpuIndex(index);
    setGpuSparkline(Array(20).fill(0)); // 切换 GPU 时重置 sparkline
    try {
      await invoke("set_active_gpu_index", { index });
    } catch (e) {
      console.error("Failed to switch GPU:", e);
    }
  };

  const isMounted = useRef(true);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    isMounted.current = true;

    const fetchSensorData = async () => {
      if (!isMounted.current) return;

      try {
        // 从悬窗缓存读取实时数据（后台线程每1秒更新，无需等待 PowerShell）
        const overlay = await invoke<{
          cpu_usage: number | null;
          cpu_temp: number | null;
          gpu_temp: number | null;
          gpu_usage: number | null;
          memory_usage: number | null;
          gpu_sensors: GpuSensorData[];
          active_gpu_index: number;
        }>("get_overlay_hardware_data");
        if (!isMounted.current) return;

        const cpuLoadVal = overlay.cpu_usage ?? null;
        const cpuTempVal = overlay.cpu_temp ?? null;
        const memPercent = overlay.memory_usage ?? null;

        if (cpuLoadVal !== null) {
          setCpuLoad(cpuLoadVal);
          setCpuSparkline((prev) => [...prev.slice(1), cpuLoadVal]);
        }
        if (cpuTempVal !== null) {
          setCpuTemp(Math.round(cpuTempVal));
        }
        if (memPercent !== null) {
          setMemSparkline((prev) => [...prev.slice(1), Math.round(memPercent)]);
        }

        // 多 GPU 数据处理：每轮都更新传感器数值（温度/占用率等实时变化）
        if (overlay.gpu_sensors && overlay.gpu_sensors.length > 0) {
          setGpuSensors(overlay.gpu_sensors);
          // 同步后端活跃 GPU 索引
          setActiveGpuIndex((prev) => {
            const idx = overlay.active_gpu_index;
            if (idx < overlay.gpu_sensors.length) return idx;
            return prev;
          });
          // 更新当前活跃 GPU 的 sparkline
          const activeGpu = overlay.gpu_sensors[overlay.active_gpu_index] ?? overlay.gpu_sensors[0];
          if (activeGpu?.usage !== null && activeGpu.usage !== undefined) {
            setGpuSparkline((prev) => [...prev.slice(1), activeGpu.usage as number]);
          }
        }

        // 内存和磁盘的详情（非百分比信息）仍需单独查询
        const memResult = await invoke<MemoryStatus>("get_memory_status");
        if (!isMounted.current) return;
        if (memResult) {
          setMemoryStatus(memResult);
          if (memPercent === null) {
            setMemSparkline((prev) => [...prev.slice(1), Math.round(memResult.usage_percent)]);
          }
        } else if (memPercent !== null) {
          setMemoryStatus({ usage_percent: memPercent, used: 0, total: 0 } as MemoryStatus);
        }

        const diskResult = await invoke<DiskInfo>("get_disk_status");
        if (!isMounted.current) return;
        if (diskResult) {
          setDiskStatus(diskResult);
          setStorageSparkline((prev) => [...prev.slice(1), Math.round(diskResult.usage_percent)]);
        }
      } catch (error) {
        console.error("Failed to fetch sensor data:", error);
      }
    };

    fetchSensorData();
    intervalRef.current = setInterval(fetchSensorData, 2000);

    return () => {
      isMounted.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, []); // 不依赖 hardwareInfo，立即开始轮询

  // 从多 GPU 列表中取当前活跃 GPU 的数据
  const activeGpuData = gpuSensors.length > 0 ? (gpuSensors[activeGpuIndex] ?? gpuSensors[0]) : null;
  const gpuTemp = activeGpuData?.temperature ?? null;
  const gpuUsage = activeGpuData?.usage ?? null;
  const memUsage = memoryStatus ? Math.round(memoryStatus.usage_percent) : null;
  const memUsed = memoryStatus ? (memoryStatus.used / 1024).toFixed(1) : "--";
  const memTotal = memoryStatus ? (memoryStatus.total / 1024).toFixed(1) : "--";
  const diskUsage = diskStatus ? Math.round(diskStatus.usage_percent) : null;
  const diskUsed = diskStatus ? diskStatus.used_gb.toFixed(1) : "--";
  const diskTotal = diskStatus ? diskStatus.total_gb.toFixed(1) : "--";

  // ─── Card display info (summary) ───
  const cpuDisplayInfo: DisplayInfo[] = hardwareInfo ? [
    { name: t("hardware.model"), value: hardwareInfo.cpu.name },
    {
      name: t("hardware.coresThreads"),
      value: `${hardwareInfo.cpu.cores} ${t("hardware.cores")} ${hardwareInfo.cpu.threads} ${t("hardware.threads")}`,
    },
    {
      name: t("hardware.baseClock"),
      value: `${(hardwareInfo.cpu.max_clock_speed / 1000).toFixed(1)} GHz`,
    },
    {
      name: t("hardware.l3Cache"),
      value: `${(hardwareInfo.cpu.l3_cache_size / 1024).toFixed(0)} MB`,
    },
  ] : [];

  const gpuDisplayInfos: DisplayInfo[][] = hardwareInfo ? hardwareInfo.gpu.map((gpu) => {
    const items: DisplayInfo[] = [
      { name: t("hardware.model"), value: gpu.name },
      { name: t("hardware.vendor"), value: gpu.vendor },
    ];
    if (gpu.memory_gb != null && gpu.memory_gb > 0) {
      items.push({ name: t("hardware.memory"), value: `${gpu.memory_gb.toFixed(1)} GB` });
    }
    items.push({ name: t("hardware.driverVersion"), value: gpu.driver_version });
    return items;
  }) : [];

  const totalCapacity = hardwareInfo ? hardwareInfo.memory.reduce((sum, mem) => sum + mem.capacity_gb, 0) : 0;
  const memoryDisplayInfo: DisplayInfo[] = hardwareInfo ? (() => {
    const mems = hardwareInfo.memory;
    // 取首个有效品牌/型号（过滤“未知”）用于汇总卡展示
    const brand = mems.find((m) => m.manufacturer && m.manufacturer !== "未知")?.manufacturer;
    const model = mems.find((m) => m.part_number && m.part_number !== "未知")?.part_number;
    const items: DisplayInfo[] = [];
    if (brand) items.push({ name: t("hardware.manufacturer"), value: brand });
    if (model) items.push({ name: t("hardware.model"), value: model });
    items.push({ name: t("hardware.totalCapacity"), value: `${totalCapacity.toFixed(0)} GB` });
    items.push({ name: t("hardware.speed"), value: mems.length > 0 ? `${mems[0].speed_mhz} MHz` : "--" });
    items.push({ name: t("hardware.count"), value: `${mems.length}` });
    return items;
  })() : [];

  const storageDisplayInfo: DisplayInfo[] = hardwareInfo ? hardwareInfo.disk.map((disk, i) => ({
    name: `${t("hardware.storage")} ${i + 1}`,
    value: `${disk.model} (${disk.size_gb.toFixed(0)}GB)`,
  })) : [];

  const motherboardDisplayInfo: DisplayInfo[] = hardwareInfo ? [
    { name: t("hardware.model"), value: hardwareInfo.motherboard.product },
    { name: t("hardware.manufacturer"), value: hardwareInfo.motherboard.manufacturer },
  ] : [];

  const soundCardDisplayInfos: DisplayInfo[][] = hardwareInfo ? (hardwareInfo.sound_card || []).map((card) => [
    { name: t("hardware.model"), value: card.name },
    { name: t("hardware.manufacturer"), value: card.manufacturer },
  ]) : [];

  const networkCardDisplayInfos: DisplayInfo[][] = hardwareInfo ? (hardwareInfo.network_card || []).map((card) => [
    { name: t("hardware.model"), value: card.name },
    { name: t("hardware.connectionName") || "连接", value: card.connection_name },
    { name: t("hardware.adapterType"), value: card.adapter_type },
    { name: t("hardware.linkSpeed"), value: card.speed_mbps > 0 ? `${card.speed_mbps} Mbps` : "--" },
  ]) : [];

  const monitorDisplayInfos: DisplayInfo[][] = hardwareInfo ? (hardwareInfo.monitor || []).map((m) => [
    { name: t("hardware.model"), value: m.is_primary ? `${m.name}（${t("hardware.primaryDisplay") || "主屏"}）` : m.name },
    { name: t("hardware.manufacturer"), value: m.manufacturer || "--" },
    { name: t("hardware.resolution") || "分辨率", value: m.screen_width && m.screen_height ? `${m.screen_width} x ${m.screen_height}` : "--" },
    { name: t("hardware.refreshRate") || "刷新率", value: m.refresh_rate ? `${m.refresh_rate} Hz` : "--" },
    { name: t("hardware.screenSize") || "尺寸", value: m.diagonal_inches ? `${m.diagonal_inches.toFixed(1)}″` : "--" },
  ]) : [];

  // ─── Detail specs builders (for modal) ───
  const buildCpuSpecs = (cpu: CpuInfo): SpecItem[] => [
    { label: t("hardware.model"), value: cpu.name },
    { label: t("hardware.manufacturer"), value: cpu.manufacturer },
    { label: t("hardware.architecture") || "架构", value: cpu.architecture },
    { label: t("hardware.socket") || "插槽", value: cpu.socket },
    { label: t("hardware.cores"), value: `${cpu.cores}` },
    { label: t("hardware.threads"), value: `${cpu.threads}` },
    { label: t("hardware.enabledCores") || "已启用核心", value: cpu.enabled_cores ? `${cpu.enabled_cores}` : "--" },
    { label: t("hardware.baseClock"), value: `${(cpu.max_clock_speed / 1000).toFixed(1)} GHz` },
    { label: t("hardware.currentClock") || "当前频率", value: cpu.current_clock_speed ? `${(cpu.current_clock_speed / 1000).toFixed(1)} GHz` : "--" },
    { label: t("hardware.extClock") || "外频(Bus)", value: cpu.ext_clock ? `${cpu.ext_clock} MHz` : "--" },
    { label: t("hardware.l2Cache"), value: cpu.l2_cache_size > 0 ? `${(cpu.l2_cache_size / 1024).toFixed(1)} MB` : "--" },
    { label: t("hardware.l3Cache"), value: cpu.l3_cache_size > 0 ? `${(cpu.l3_cache_size / 1024).toFixed(1)} MB` : "--" },
    { label: t("hardware.family") || "系列", value: cpu.family > 0 ? `${cpu.family}` : "--" },
    { label: t("hardware.stepping") || "步进", value: cpu.stepping || "--" },
    { label: t("hardware.revision") || "修订", value: cpu.revision || "--" },
    { label: t("hardware.processorId") || "处理器ID", value: cpu.processor_id || "--" },
    { label: t("hardware.voltageCaps") || "电压能力", value: cpu.voltage_caps || "--" },
  ];

  const buildGpuSpecs = (gpu: GpuInfo, idx: number): SpecItem[] => {
    const specs: SpecItem[] = [
      { label: t("hardware.model"), value: gpu.name },
      { label: t("hardware.vendor"), value: gpu.vendor },
      { label: t("hardware.videoProcessor") || "核心架构", value: gpu.video_processor || "--" },
    ];
    if (gpu.memory_gb != null && gpu.memory_gb > 0) {
      specs.push({ label: t("hardware.memory"), value: `${gpu.memory_gb.toFixed(1)} GB` });
    }
    specs.push(
      { label: t("hardware.videoMemoryType") || "显存类型", value: gpu.video_memory_type || "--" },
      { label: t("hardware.driverVersion"), value: gpu.driver_version },
      { label: t("hardware.driverDate") || "驱动日期", value: gpu.driver_date || "--" },
      { label: t("hardware.infFilename") || "INF文件", value: gpu.inf_filename || "--" },
      { label: t("hardware.deviceId") || "设备ID", value: gpu.device_id || "--" },
      { label: t("hardware.pnpDeviceId") || "PNP ID", value: gpu.pnp_device_id || "--" },
      { label: t("hardware.resolution"), value: gpu.resolution_width && gpu.resolution_height ? `${gpu.resolution_width} x ${gpu.resolution_height}` : "--" },
      { label: t("hardware.refreshRate") || "刷新率", value: gpu.refresh_rate ? `${gpu.refresh_rate} Hz` : "--" },
      { label: t("hardware.status"), value: gpu.status || "--" },
    );
    return specs;
  };

  const buildMemorySpecs = (mems: MemoryInfo[]): SpecItem[] => {
    const specs: SpecItem[] = [];
    const totalGb = mems.reduce((s, m) => s + m.capacity_gb, 0);
    specs.push(
      { label: t("hardware.totalCapacity"), value: `${totalGb.toFixed(0)} GB` },
      { label: t("hardware.count"), value: `${mems.length}` },
    );
    mems.forEach((mem, i) => {
      const prefix = mems.length > 1 ? `[${i + 1}] ` : "";
      specs.push(
        { label: `${prefix}${t("hardware.bankLabel")}`, value: mem.bank_label },
        { label: `${prefix}${t("hardware.partNumber")}`, value: mem.part_number },
        { label: `${prefix}${t("hardware.manufacturer")}`, value: mem.manufacturer },
        { label: `${prefix}${t("hardware.capacity")}`, value: `${mem.capacity_gb.toFixed(0)} GB` },
        { label: `${prefix}${t("hardware.speed")}`, value: `${mem.speed_mhz} MHz` },
        { label: `${prefix}${t("hardware.memoryType") || "类型"}`, value: mem.memory_type || "--" },
        { label: `${prefix}${t("hardware.formFactor") || "外形"}`, value: mem.form_factor || "--" },
        { label: `${prefix}${t("hardware.serialNumber") || "序列号"}`, value: mem.serial_number || "--" },
      );
    });
    return specs;
  };

  const buildMotherboardSpecs = (mobo: MotherboardInfo): SpecItem[] => [
    { label: t("hardware.model"), value: mobo.product },
    { label: t("hardware.manufacturer"), value: mobo.manufacturer },
    { label: t("hardware.serialNumber") || "序列号", value: mobo.serial_number || "--" },
    { label: t("hardware.version"), value: mobo.version || "--" },
    { label: t("hardware.biosVendor") || "BIOS 厂商", value: mobo.bios_vendor || "--" },
    { label: t("hardware.biosVersion") || "BIOS 版本", value: mobo.bios_version || "--" },
    { label: t("hardware.biosReleaseDate") || "BIOS 日期", value: mobo.bios_release_date || "--" },
    { label: t("hardware.systemManufacturer") || "系统制造商", value: mobo.system_manufacturer || "--" },
    { label: t("hardware.systemModel") || "系统型号", value: mobo.system_model || "--" },
    { label: t("hardware.systemType") || "系统类型", value: mobo.system_type || "--" },
    { label: t("hardware.chassisType") || "机箱类型", value: mobo.chassis_type || "--" },
  ];

  const buildStorageSpecs = (disks: DiskDetailInfo[]): SpecItem[] => {
    const specs: SpecItem[] = [];
    disks.forEach((disk, i) => {
      const prefix = disks.length > 1 ? `[${i + 1}] ` : "";
      specs.push(
        { label: `${prefix}${t("hardware.model")}`, value: disk.model },
        { label: `${prefix}${t("hardware.capacity")}`, value: `${disk.size_gb.toFixed(1)} GB` },
        { label: `${prefix}${t("hardware.diskType") || "类型"}`, value: disk.is_ssd ? "SSD" : disk.media_type || "HDD" },
        { label: `${prefix}${t("hardware.interfaceType") || "接口"}`, value: disk.interface_type || "--" },
        { label: `${prefix}${t("hardware.serialNumber") || "序列号"}`, value: disk.serial_number || "--" },
        { label: `${prefix}${t("hardware.firmware") || "固件版本"}`, value: disk.firmware_revision || "--" },
        { label: `${prefix}${t("hardware.status")}`, value: disk.status || "--" },
      );
    });
    return specs;
  };

  const buildSoundCardSpecs = (cards: SoundCardInfo[]): SpecItem[] => {
    const specs: SpecItem[] = [];
    cards.forEach((card, i) => {
      const prefix = cards.length > 1 ? `[${i + 1}] ` : "";
      specs.push(
        { label: `${prefix}${t("hardware.name") || "名称"}`, value: card.name },
        { label: `${prefix}${t("hardware.manufacturer")}`, value: card.manufacturer },
        { label: `${prefix}${t("hardware.status")}`, value: card.status || "--" },
      );
    });
    return specs;
  };

  const buildNetworkCardSpecs = (cards: NetworkCardInfo[]): SpecItem[] => {
    const specs: SpecItem[] = [];
    cards.forEach((card, i) => {
      const prefix = cards.length > 1 ? `[${i + 1}] ` : "";
      specs.push(
        { label: `${prefix}${t("hardware.name") || "名称"}`, value: card.name },
        { label: `${prefix}${t("hardware.connectionName") || "连接名称"}`, value: card.connection_name || "--" },
        { label: `${prefix}${t("hardware.manufacturer")}`, value: card.manufacturer },
        { label: `${prefix}${t("hardware.adapterType")}`, value: card.adapter_type },
        { label: `${prefix}${t("hardware.macAddress")}`, value: card.mac_address },
        { label: `${prefix}${t("hardware.linkSpeed")}`, value: card.speed_mbps > 0 ? `${card.speed_mbps} Mbps` : "--" },
        { label: `${prefix}${t("hardware.maxSpeed") || "最大速度"}`, value: card.max_speed ? `${card.max_speed} Mbps` : "--" },
        { label: `${prefix}${t("hardware.guid") || "GUID"}`, value: card.guid || "--" },
      );
    });
    return specs;
  };

  const buildMonitorSpecs = (monitors: MonitorInfo[]): SpecItem[] => {
    const specs: SpecItem[] = [];
    monitors.forEach((mon, i) => {
      const prefix = monitors.length > 1 ? `[${i + 1}] ` : "";
      specs.push(
        { label: `${prefix}${t("hardware.name") || "名称"}`, value: mon.is_primary ? `${mon.name}（${t("hardware.primaryDisplay") || "主屏"}）` : mon.name },
        { label: `${prefix}${t("hardware.manufacturer")}`, value: mon.manufacturer || "--" },
        { label: `${prefix}${t("hardware.resolution") || "分辨率"}`, value: mon.screen_width && mon.screen_height ? `${mon.screen_width} x ${mon.screen_height}` : "--" },
        { label: `${prefix}${t("hardware.refreshRate") || "刷新率"}`, value: mon.refresh_rate ? `${mon.refresh_rate} Hz` : "--" },
        { label: `${prefix}${t("hardware.screenSize") || "尺寸"}`, value: mon.diagonal_inches ? `${mon.diagonal_inches.toFixed(1)}″` : "--" },
        { label: `${prefix}${t("hardware.pnpDeviceId") || "PNP ID"}`, value: mon.pnp_device_id || "--" },
        { label: `${prefix}${t("hardware.status")}`, value: mon.status || "--" },
      );
    });
    return specs;
  };

  // ─── 复制所有硬件信息 ───
  // 将分类的 SpecItem 列表转成对齐的分段文本
  const formatSpecSection = (title: string, specs: SpecItem[]): string => {
    if (specs.length === 0) return "";
    const header = `========== ${title} ==========`;
    // 计算 label 最大宽度，实现整齐对齐
    const maxLabelLen = Math.max(...specs.map((s) => s.label.length));
    const body = specs.map((s) => `  ${s.label.padEnd(maxLabelLen)} : ${s.value}`).join("\n");
    return `${header}\n${body}`;
  };

  const handleCopyHardwareInfo = async () => {
    if (!hardwareInfo) {
      toast({
        title: t("hardware.copyFailed") || "复制失败",
        description: t("hardware.noData") || "暂无硬件信息",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
      return;
    }

    const sections: string[] = [];
    sections.push(formatSpecSection(t("hardware.processor"), buildCpuSpecs(hardwareInfo.cpu)));
    hardwareInfo.gpu.forEach((gpu, i) => {
      sections.push(formatSpecSection(t("hardware.gpu"), buildGpuSpecs(gpu, i)));
    });
    sections.push(formatSpecSection(t("hardware.ram"), buildMemorySpecs(hardwareInfo.memory)));
    sections.push(formatSpecSection(t("hardware.motherboard"), buildMotherboardSpecs(hardwareInfo.motherboard)));
    if (hardwareInfo.disk?.length) {
      sections.push(formatSpecSection(t("hardware.storage"), buildStorageSpecs(hardwareInfo.disk)));
    }
    if (hardwareInfo.sound_card?.length) {
      sections.push(formatSpecSection(t("hardware.soundCard"), buildSoundCardSpecs(hardwareInfo.sound_card)));
    }
    if (hardwareInfo.network_card?.length) {
      sections.push(formatSpecSection(t("hardware.networkCard"), buildNetworkCardSpecs(hardwareInfo.network_card)));
    }
    if (hardwareInfo.monitor?.length) {
      sections.push(formatSpecSection(t("hardware.monitor") || "显示器", buildMonitorSpecs(hardwareInfo.monitor)));
    }

    const text = sections.filter(Boolean).join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      toast({
        title: t("hardware.copySuccess") || "已复制到剪贴板",
        description: t("hardware.copyDesc") || "硬件信息已按分类复制",
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (e) {
      console.error("Failed to copy hardware info:", e);
      toast({
        title: t("hardware.copyFailed") || "复制失败",
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  return (
    <Box pt={8}>
      <HStack justify="space-between" mb={6}>
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("hardware.title")}
        </Heading>
        <HStack gap={2}>
          <RemoteMonitorDialog />
          <Button
            size="sm"
            variant="outline"
            colorScheme="blue"
            leftIcon={<Activity size={15} />}
            onClick={() => invoke("open_sensor_monitor")}
          >
            全部传感器状态
          </Button>
          <Button
            size="sm"
            variant="outline"
            colorScheme="blue"
            leftIcon={<Download size={15} />}
            onClick={onPawnioModalOpen}
          >
            安装驱动（获取CPU温度）
          </Button>
          <Button
            size="sm"
            variant="outline"
            colorScheme="teal"
            leftIcon={<Copy size={15} />}
            onClick={handleCopyHardwareInfo}
          >
            {t("hardware.copyAll") || "复制所有硬件信息"}
          </Button>
          <Button
            leftIcon={<Trash2 size={16} />}
            size="sm"
            variant="outline"
            colorScheme="red"
            color="#e74c3c"
            borderColor="rgba(231,76,60,0.3)"
            _hover={{ bg: "rgba(231,76,60,0.1)" }}
            onClick={async () => {
              try {
                await invoke("clear_hardware_data");
                toast({
                  title: "硬件数据已清除",
                  status: "success",
                  duration: 3000,
                  isClosable: true,
                });
              } catch (e) {
                toast({
                  title: "清除失败",
                  description: String(e),
                  status: "error",
                  duration: 3000,
                  isClosable: true,
                });
              }
            }}
          >
            清除数据
          </Button>
          <Button
            leftIcon={<Download size={16} />}
            size="sm"
            variant="outline"
            color={headingColor}
            borderColor={btnBorderColor}
            onClick={exportReport}
            isLoading={isExporting}
          >
            {t("hardwareReport.export") || "导出报告"}
          </Button>
        </HStack>
      </HStack>

      <VStack spacing={6} align="stretch">
        <Grid
          templateColumns={{
            base: "repeat(2, 1fr)",
            md: "repeat(4, 1fr)",
          }}
          gap={4}
        >
          <StatCard
            title="CPU"
            value={`${cpuLoad ?? "--"}%`}
            subValue={cpuTemp !== null ? `${t("hardware.temperature")} ${Math.round(cpuTemp)}${t("hardware.temperatureUnit")}` : undefined}
            color="#3b82f6"
            sparklineData={cpuSparkline}
            icon={Cpu}
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
          />
          <StatCard
            titleContent={gpuSensors.length >= 1 ? (
              <Menu>
                <MenuButton
                  cursor="pointer"
                  color={subTextColor}
                  _hover={{ color: textColor }}
                  transition="color 0.15s"
                  bg="transparent"
                  border="none"
                  p={0}
                  minW="0"
                  flex={1}
                >
                  <HStack spacing={1}>
                    <Box flex={1} overflow="hidden">
                      <MarqueeText text={activeGpuData?.name || activeGpuData?.hardware_type || "GPU"} />
                    </Box>
                    <ChevronDown size={12} />
                  </HStack>
                </MenuButton>
                <MenuList bg={cardBg} borderColor={btnBorderColor} minW="180px" zIndex={9999}>
                  {gpuSensors.map((gpu, i) => (
                    <MenuItem
                      key={i}
                      onClick={() => handleGpuSwitch(i)}
                      bg={i === activeGpuIndex ? "whiteAlpha.200" : "transparent"}
                      color={i === activeGpuIndex ? textColor : subTextColor}
                      fontSize="sm"
                      _hover={{ bg: "whiteAlpha.100" }}
                    >
                      {gpu.name || gpu.hardware_type}
                    </MenuItem>
                  ))}
                </MenuList>
              </Menu>
            ) : (
              <Text fontSize="sm" color={subTextColor} fontWeight="medium">GPU</Text>
            )}
            value={`${gpuUsage ?? "--"}%`}
            subValue={gpuTemp !== null ? `${t("hardware.temperature")} ${Math.round(gpuTemp)}${t("hardware.temperatureUnit")}` : undefined}
            color="#22c55e"
            sparklineData={gpuSparkline}
            icon={Monitor}
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
          />
          <StatCard
            title={t("hardware.ram")}
            value={`${memUsage ?? "--"}%`}
            subValue={`${memUsed} / ${memTotal} GB`}
            color="#06b6d4"
            sparklineData={memSparkline}
            icon={Ram}
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
          />
          <StatCard
            title={t("hardware.storage")}
            value={`${diskUsage ?? "--"}%`}
            subValue={`${diskUsed} / ${diskTotal} GB`}
            color="#a855f7"
            sparklineData={storageSparkline}
            icon={Database}
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
          />
        </Grid>

        <Grid
          templateColumns={{
            base: "1fr",
            md: "repeat(2, 1fr)",
            lg: "repeat(3, 1fr)",
          }}
          gap={4}
        >
          <DetailCard
            title={t("hardware.processor")}
            icon={Cpu}
            info={cpuDisplayInfo}
            type="cpu"
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
            brandText={hardwareInfo ? `${hardwareInfo.cpu.name} ${hardwareInfo.cpu.manufacturer}` : undefined}
            onClick={hardwareInfo ? () => handleOpenDetail(t("hardware.processor"), Cpu, "cpu", buildCpuSpecs(hardwareInfo.cpu)) : undefined}
          />
          {gpuDisplayInfos.map((gpuInfo, i) => (
            <DetailCard
              key={i}
              title={t("hardware.gpu")}
              icon={Monitor}
              info={gpuInfo}
              type="gpu"
              cardBg={cardBg}
              textColor={textColor}
              subTextColor={subTextColor}
              liquidGlassEnabled={liquidGlassEnabled}
              brandText={hardwareInfo && hardwareInfo.gpu[i] ? `${hardwareInfo.gpu[i].name} ${hardwareInfo.gpu[i].vendor}` : undefined}
              onClick={hardwareInfo && hardwareInfo.gpu[i] ? () => handleOpenDetail(t("hardware.gpu"), Monitor, "gpu", buildGpuSpecs(hardwareInfo.gpu[i], i)) : undefined}
            />
          ))}
          <DetailCard
            title={t("hardware.ram")}
            icon={Ram}
            info={memoryDisplayInfo}
            type="memory"
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
            onClick={hardwareInfo ? () => handleOpenDetail(t("hardware.ram"), Ram, "memory", buildMemorySpecs(hardwareInfo.memory)) : undefined}
          />
          <DetailCard
            title={t("hardware.motherboard")}
            icon={CircuitBoard}
            info={motherboardDisplayInfo}
            type="motherboard"
            cardBg={cardBg}
            textColor={textColor}
            subTextColor={subTextColor}
            liquidGlassEnabled={liquidGlassEnabled}
            brandText={hardwareInfo ? `${hardwareInfo.motherboard.manufacturer} ${hardwareInfo.motherboard.product}` : undefined}
            onClick={hardwareInfo ? () => handleOpenDetail(t("hardware.motherboard"), CircuitBoard, "motherboard", buildMotherboardSpecs(hardwareInfo.motherboard)) : undefined}
          />
          {storageDisplayInfo.length > 0 && (
            <DetailCard
              title={t("hardware.storage")}
              icon={HardDrive}
              info={storageDisplayInfo}
              type="storage"
              cardBg={cardBg}
              textColor={textColor}
              subTextColor={subTextColor}
              liquidGlassEnabled={liquidGlassEnabled}
              onClick={hardwareInfo ? () => handleOpenDetail(t("hardware.storage"), HardDrive, "storage", buildStorageSpecs(hardwareInfo.disk)) : undefined}
            />
          )}
          {soundCardDisplayInfos.map((info, i) => (
            <DetailCard
              key={`sound-${i}`}
              title={t("hardware.soundCard")}
              icon={Volume2}
              info={info}
              type="sound"
              cardBg={cardBg}
              textColor={textColor}
              subTextColor={subTextColor}
              liquidGlassEnabled={liquidGlassEnabled}
              onClick={hardwareInfo && hardwareInfo.sound_card ? () => handleOpenDetail(t("hardware.soundCard"), Volume2, "sound", buildSoundCardSpecs(hardwareInfo.sound_card)) : undefined}
            />
          ))}
          {networkCardDisplayInfos.map((info, i) => (
            <DetailCard
              key={`network-${i}`}
              title={t("hardware.networkCard")}
              icon={Wifi}
              info={info}
              type="network"
              cardBg={cardBg}
              textColor={textColor}
              subTextColor={subTextColor}
              liquidGlassEnabled={liquidGlassEnabled}
              onClick={hardwareInfo && hardwareInfo.network_card ? () => handleOpenDetail(t("hardware.networkCard"), Wifi, "network", buildNetworkCardSpecs(hardwareInfo.network_card)) : undefined}
            />
          ))}
          {monitorDisplayInfos.map((info, i) => (
            <DetailCard
              key={`monitor-${i}`}
              title={t("hardware.monitor") || "显示器"}
              icon={Monitor}
              info={info}
              type="monitor"
              cardBg={cardBg}
              textColor={textColor}
              subTextColor={subTextColor}
              liquidGlassEnabled={liquidGlassEnabled}
              onClick={hardwareInfo && hardwareInfo.monitor ? () => handleOpenDetail(t("hardware.monitor") || "显示器", Monitor, "monitor", buildMonitorSpecs(hardwareInfo.monitor)) : undefined}
            />
          ))}
        </Grid>
      </VStack>

      {/* Detail Modal */}
      <HardwareDetailModal
        isOpen={isDetailOpen}
        onClose={onDetailClose}
        title={detailCard?.title || ""}
        icon={detailCard?.icon || Cpu}
        type={detailCard?.type || "cpu"}
        specs={detailCard?.specs || []}
      />

      {/* PawnIO 安装对话框 */}
      <PawnioInstallModal
        isOpen={isPawnioModalOpen}
        onClose={onPawnioModalClose}
      />
    </Box>
  );
}
