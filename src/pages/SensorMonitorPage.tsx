/**
 * 传感器监控独立窗口页面
 *
 * 展示 NexBoxMonitor (LHML) 获取的所有传感器原始数据，
 * 按硬件类型分组，支持搜索/筛选，实时刷新，可折叠。
 */

import { useEffect, useRef, useState, useCallback, useMemo, memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Box,
  Text,
  VStack,
  HStack,
  Input,
  InputGroup,
  InputLeftElement,
  useColorModeValue,
  Badge,
  Spinner,
  Center,
  Flex,
  Heading,
  Tag,
  TagLabel,
  Tooltip,
  IconButton,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  Button,
} from "@chakra-ui/react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Search, X, Cpu, Monitor, MemoryStick, HardDrive, CircuitBoard,
  Thermometer, Gauge, Fan, Zap, Clock, Activity, RefreshCw, ChevronDown,
} from "lucide-react";
import type { SensorReading, SensorsResponse } from "@/lib/sensors";

// ===== 常量 =====

const POLL_INTERVAL_MS = 1000;

/** 传感器类型 → 颜色映射 */
const SENSOR_TYPE_COLORS: Record<string, string> = {
  Temperature: "#e74c3c",
  Load: "#3498db",
  Voltage: "#2ecc71",
  Fan: "#e67e22",
  Power: "#9b59b6",
  Clock: "#1abc9c",
  Frequency: "#1abc9c",
  Data: "#f39c12",
  SmallData: "#f39c12",
  Control: "#95a5a6",
  Level: "#95a5a6",
  Throughput: "#95a5a6",
  Current: "#2ecc71",
  Energy: "#9b59b6",
  Noise: "#95a5a6",
  Humidity: "#3498db",
  TimeSpan: "#95a5a6",
};

/** 传感器类型 → 中文名称 */
const SENSOR_TYPE_LABELS: Record<string, string> = {
  Temperature: "温度",
  Load: "负载",
  Voltage: "电压",
  Fan: "风扇",
  Power: "功耗",
  Clock: "频率",
  Frequency: "频率",
  Data: "数据",
  SmallData: "数据",
  Control: "控制",
  Level: "电平",
  Throughput: "吞吐量",
  Current: "电流",
  Energy: "能耗",
  Noise: "噪音",
  Humidity: "湿度",
  TimeSpan: "时间",
};

/** 传感器类型 → 显示图标 */
const SENSOR_TYPE_ICONS: Record<string, React.ReactNode> = {
  Temperature: <Thermometer size={14} />,
  Load: <Gauge size={14} />,
  Voltage: <Zap size={14} />,
  Fan: <Fan size={14} />,
  Power: <Zap size={14} />,
  Clock: <Clock size={14} />,
  Frequency: <Clock size={14} />,
  Data: <HardDrive size={14} />,
  SmallData: <HardDrive size={14} />,
};

/** 硬件类型 → 显示图标 */
const HARDWARE_TYPE_ICONS: Record<string, React.ReactNode> = {
  CPU: <Cpu size={20} />,
  GpuNvidia: <Monitor size={20} />,
  GpuAmd: <Monitor size={20} />,
  GpuIntel: <Monitor size={20} />,
  Memory: <MemoryStick size={20} />,
  Storage: <HardDrive size={20} />,
  Motherboard: <CircuitBoard size={20} />,
  SuperIO: <CircuitBoard size={20} />,
};

/** 硬件类型 → 显示名称 */
const HARDWARE_TYPE_LABELS: Record<string, string> = {
  CPU: "CPU",
  GpuNvidia: "NVIDIA 显卡",
  GpuAmd: "AMD 显卡",
  GpuIntel: "Intel 显卡",
  Memory: "内存",
  Storage: "存储",
  Motherboard: "主板",
  SuperIO: "Super I/O",
};

// ===== 辅助函数 =====

function getSensorColor(sensorType: string): string {
  return SENSOR_TYPE_COLORS[sensorType] || "#95a5a6";
}

function getSensorTypeLabel(sensorType: string): string {
  return SENSOR_TYPE_LABELS[sensorType] || sensorType;
}

function getSensorIcon(sensorType: string): React.ReactNode | null {
  return SENSOR_TYPE_ICONS[sensorType] || null;
}

function getHardwareIcon(hardwareType: string): React.ReactNode {
  const exact = HARDWARE_TYPE_ICONS[hardwareType];
  if (exact) return exact;
  for (const [key, icon] of Object.entries(HARDWARE_TYPE_ICONS)) {
    if (hardwareType.toLocaleLowerCase().startsWith(key.toLocaleLowerCase())) {
      return icon;
    }
  }
  return <Activity size={20} />;
}

function getHardwareLabel(hardwareType: string): string {
  const exact = HARDWARE_TYPE_LABELS[hardwareType];
  if (exact) return exact;
  for (const [key, label] of Object.entries(HARDWARE_TYPE_LABELS)) {
    if (hardwareType.toLocaleLowerCase().startsWith(key.toLocaleLowerCase())) {
      return label;
    }
  }
  return hardwareType;
}

function formatSensorValue(value: number, unit?: string): string {
  if (unit === "°C") return `${value.toFixed(1)}°C`;
  if (unit === "%") return `${value.toFixed(1)}%`;
  if (unit === "RPM") return `${Math.round(value)} RPM`;
  if (unit === "V") return `${value.toFixed(3)}V`;
  if (unit === "W") return `${value.toFixed(1)}W`;
  if (unit === "MHz") return `${Math.round(value)} MHz`;
  if (unit === "GB") return `${value.toFixed(2)} GB`;
  if (unit === "MB") return `${Math.round(value)} MB`;
  if (unit === "Hz") return `${value.toLocaleString()} Hz`;
  if (unit === "mWh") return `${Math.round(value)} mWh`;
  if (unit === "A") return `${value.toFixed(3)}A`;
  if (unit === "dBA") return `${value.toFixed(1)} dBA`;
  if (unit === "s") return `${value.toFixed(1)}s`;
  if (unit === "L/h") return `${value.toFixed(1)} L/h`;
  if (unit === "B/s") return `${formatBytes(value)}/s`;
  return unit ? `${value} ${unit}` : `${value}`;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}

/** 按硬件分组传感器 */
interface SensorGroup {
  hardware: string;
  hardwareType: string;
  sensors: SensorReading[];
}

function groupSensors(sensors: SensorReading[]): SensorGroup[] {
  const map = new Map<string, SensorGroup>();
  for (const s of sensors) {
    const key = `${s.hardwareType}::${s.hardware}`;
    const existing = map.get(key);
    if (existing) {
      existing.sensors.push(s);
    } else {
      map.set(key, {
        hardware: s.hardware,
        hardwareType: s.hardwareType,
        sensors: [s],
      });
    }
  }
  const order = ["CPU", "Gpu", "Memory", "Storage", "Motherboard", "SuperIO"];
  return Array.from(map.values()).sort((a, b) => {
    const ai = order.findIndex((o) => a.hardwareType.toLocaleLowerCase().startsWith(o.toLocaleLowerCase()));
    const bi = order.findIndex((o) => b.hardwareType.toLocaleLowerCase().startsWith(o.toLocaleLowerCase()));
    return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
  });
}

// ===== 自定义筛选下拉框组件 =====

interface FilterDropdownProps {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
  allLabel: string;
  minW?: string;
}

function FilterDropdown({ value, onChange, options, allLabel, minW = "120px" }: FilterDropdownProps) {
  const selectedLabel = value === "all" ? allLabel : options.find((o) => o.value === value)?.label || allLabel;

  return (
    <Menu>
      <MenuButton
        as={Button}
        size="sm"
        rightIcon={<ChevronDown size={14} />}
        bg="rgba(255,255,255,0.04)"
        border="1px solid"
        borderColor="rgba(255,255,255,0.1)"
        color="gray.200"
        borderRadius="lg"
        minW={minW}
        textAlign="left"
        fontWeight="normal"
        fontSize="sm"
        _hover={{ borderColor: "rgba(255,255,255,0.2)" }}
        _expanded={{ borderColor: "#3498db" }}
        _focus={{ bg: "rgba(255,255,255,0.04)", boxShadow: "none" }}
        _active={{ bg: "rgba(255,255,255,0.04)" }}
        _focusVisible={{ bg: "rgba(255,255,255,0.04)", boxShadow: "none" }}
        transition="border-color 0.15s"
      >
        {selectedLabel}
      </MenuButton>
      <MenuList
        bg="#1a1a1a"
        borderColor="rgba(255,255,255,0.1)"
        minW={minW}
        py={1}
        borderRadius="lg"
        boxShadow="0 4px 16px rgba(0,0,0,0.4)"
      >
        <MenuItem
          onClick={() => onChange("all")}
          bg={value === "all" ? "rgba(52,152,219,0.15)" : "transparent"}
          color={value === "all" ? "#3498db" : "gray.300"}
          _hover={{ bg: "rgba(255,255,255,0.06)" }}
          _focus={{ bg: value === "all" ? "rgba(52,152,219,0.15)" : "rgba(255,255,255,0.06)" }}
          _active={{ bg: "rgba(255,255,255,0.04)" }}
          fontSize="sm"
          borderRadius="md"
          mx={1}
        >
          {allLabel}
        </MenuItem>
        {options.map((opt) => (
          <MenuItem
            key={opt.value}
            onClick={() => onChange(opt.value)}
            bg={value === opt.value ? "rgba(52,152,219,0.15)" : "transparent"}
            color={value === opt.value ? "#3498db" : "gray.300"}
            _hover={{ bg: "rgba(255,255,255,0.06)" }}
            _focus={{ bg: value === opt.value ? "rgba(52,152,219,0.15)" : "rgba(255,255,255,0.06)" }}
            _active={{ bg: "rgba(255,255,255,0.04)" }}
            fontSize="sm"
            borderRadius="md"
            mx={1}
          >
            {opt.label}
          </MenuItem>
        ))}
      </MenuList>
    </Menu>
  );
}

// ===== 组件 =====

const SensorRow = memo(function SensorRow({ sensor }: { sensor: SensorReading }) {
  const color = getSensorColor(sensor.sensorType);
  const icon = getSensorIcon(sensor.sensorType);
  const valueColor = sensor.sensorType === "Temperature"
    ? sensor.value > 80 ? "#e74c3c" : sensor.value > 60 ? "#e67e22" : sensor.value > 40 ? "#f39c12" : "#2ecc71"
    : color;

  return (
    <HStack
      justify="space-between"
      px={4}
      py={2.5}
      _hover={{ bg: "whiteAlpha.50" }}
      borderRadius="md"
      transition="background 0.15s"
    >
      <HStack spacing={3} minW={0} flex={1}>
        {icon && (
          <Box color={color} flexShrink={0} opacity={0.7}>
            {icon}
          </Box>
        )}
        <Tooltip label={getSensorTypeLabel(sensor.sensorType)} placement="top" hasArrow>
          <Tag size="sm" variant="subtle" colorScheme="gray" flexShrink={0}>
            <TagLabel fontSize="xs">{getSensorTypeLabel(sensor.sensorType)}</TagLabel>
          </Tag>
        </Tooltip>
        <Text
          fontSize="sm"
          color="gray.300"
          noOfLines={1}
          wordBreak="break-all"
        >
          {sensor.name}
        </Text>
      </HStack>
      <Text
        fontSize="sm"
        fontWeight="semibold"
        color={valueColor}
        fontFamily="'MiSans', sans-serif"
        flexShrink={0}
        ml={4}
      >
        {formatSensorValue(sensor.value, sensor.unit)}
      </Text>
    </HStack>
  );
});

interface SensorGroupCardProps {
  group: SensorGroup;
  collapsed: boolean;
  onToggle: () => void;
}

const SensorGroupCard = memo(function SensorGroupCard({ group, collapsed, onToggle }: SensorGroupCardProps) {
  const bg = useColorModeValue("rgba(255,255,255,0.06)", "rgba(255,255,255,0.04)");
  const borderColor = useColorModeValue("rgba(255,255,255,0.12)", "rgba(255,255,255,0.08)");
  const hardwareTypeColor = getSensorColor(group.sensors[0]?.sensorType || "Load");

  const badgeColorScheme = group.hardwareType.toLocaleLowerCase().startsWith("cpu") ? "blue"
    : group.hardwareType.toLocaleLowerCase().startsWith("gpu") ? "green"
    : group.hardwareType.toLocaleLowerCase().startsWith("memory") ? "cyan"
    : group.hardwareType.toLocaleLowerCase().startsWith("storage") ? "purple"
    : "gray";

  return (
    <Box
      bg={bg}
      border="1px solid"
      borderColor={borderColor}
      borderRadius="xl"
      overflow="hidden"
      transition="border-color 0.2s"
      _hover={{ borderColor: "rgba(255,255,255,0.2)" }}
    >
      {/* 可点击的组头 */}
      <HStack
        as="button"
        type="button"
        w="full"
        px={5}
        py={3.5}
        borderBottom={collapsed ? "none" : "1px solid"}
        borderColor={borderColor}
        justify="space-between"
        cursor="pointer"
        transition="background 0.15s, border-color 0.2s"
        _hover={{ bg: "whiteAlpha.50" }}
        onClick={onToggle}
        role="button"
        aria-expanded={!collapsed}
      >
        <HStack spacing={3}>
          <Box color={hardwareTypeColor}>
            {getHardwareIcon(group.hardwareType)}
          </Box>
          <VStack align="start" spacing={0.5}>
            <Text fontSize="md" fontWeight="bold" color="gray.100" textAlign="left">
              {group.hardware}
            </Text>
            <Badge colorScheme={badgeColorScheme} fontSize="xs">
              {getHardwareLabel(group.hardwareType)}
            </Badge>
          </VStack>
        </HStack>

        <HStack spacing={3}>
          <Text fontSize="xs" color="gray.500" whiteSpace="nowrap">
            {group.sensors.length} 个传感器
          </Text>
          <Box
            color="gray.500"
            transition="transform 0.25s ease"
            transform={collapsed ? "rotate(-90deg)" : "rotate(0deg)"}
            display="flex"
            alignItems="center"
          >
            <ChevronDown size={16} />
          </Box>
        </HStack>
      </HStack>

      {/* 可折叠的传感器列表 */}
      <AnimatePresence initial={false}>
        {!collapsed && (
          <motion.div
            key="sensor-list"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.25, ease: [0.4, 0, 0.2, 1] }}
            style={{ overflow: "hidden" }}
          >
            <Box>
              {group.sensors.map((sensor, i) => (
                <SensorRow key={`${sensor.name}-${sensor.sensorType}-${i}`} sensor={sensor} />
              ))}
            </Box>
          </motion.div>
        )}
      </AnimatePresence>
    </Box>
  );
});

// ===== 主页面 =====

export default function SensorMonitorPage() {
  const [sensors, setSensors] = useState<SensorReading[]>([]);
  const [updatedAt, setUpdatedAt] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [sensorTypeFilter, setSensorTypeFilter] = useState("all");
  const [hardwareTypeFilter, setHardwareTypeFilter] = useState("all");
  // 默认所有分组折叠（记录哪些是展开的）
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  const isMounted = useRef(true);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const fetchingRef = useRef(false);
  const sensorsRef = useRef<SensorReading[]>([]);
  const visibleRef = useRef(true);

  const bg = useColorModeValue("#111111", "#0a0a0a");
  const textColor = useColorModeValue("#e0e0e0", "#ffffff");

  const toggleGroup = useCallback((groupKey: string) => {
    setExpandedGroups((prev) => {
      const next = new Set(prev);
      if (next.has(groupKey)) {
        next.delete(groupKey);
      } else {
        next.add(groupKey);
      }
      return next;
    });
  }, []);

  // 监听窗口可见性变化，隐藏时暂停轮询并释放 GPU 资源
  useEffect(() => {
    const updateVisible = (visible: boolean) => {
      visibleRef.current = visible;
      if (!visible) {
        // 隐藏时：暂停轮询 + 停止 WebView2 GPU 渲染
        if (intervalRef.current) {
          clearInterval(intervalRef.current);
          intervalRef.current = null;
        }
        // content-visibility: hidden 让 Chromium 跳过该元素渲染，
        // 从而释放 WebView2 GPU 进程的 GPU 占用
        document.body.style.contentVisibility = "hidden";
        document.body.style.overflow = "hidden";
      } else {
        // 恢复时：恢复渲染 + 拉取最新数据 + 恢复轮询
        document.body.style.contentVisibility = "";
        document.body.style.overflow = "";
        if (isMounted.current) {
          const fetchData = createFetchFn();
          fetchData();
          intervalRef.current = setInterval(fetchData, POLL_INTERVAL_MS);
        }
      }
    };

    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen<boolean>("window-visibility-changed", (e) => {
      updateVisible(e.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    const onVisibilityChange = () => updateVisible(!document.hidden);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 创建 fetch 函数（每次独立闭包，由 visibility 监听管理生命周期）
  const createFetchFn = useCallback(() => {
    return async () => {
      if (!isMounted.current || fetchingRef.current || !visibleRef.current) return;
      fetchingRef.current = true;
      try {
        const response = await invoke<SensorsResponse>("get_all_sensors");
        if (!isMounted.current) return;
        const prev = sensorsRef.current;
        const next = response.sensors;
        if (prev.length !== next.length || prev.some((s, i) => s.value !== next[i]?.value || s.name !== next[i]?.name)) {
          sensorsRef.current = next;
          setSensors(next);
          setUpdatedAt(response.updatedAt);
        }
        setError(null);
      } catch (e) {
        if (!isMounted.current) return;
        setError(String(e));
      } finally {
        if (isMounted.current) {
          fetchingRef.current = false;
          setLoading(false);
        }
      }
    };
  }, []);

  // 轮询传感器数据（仅在窗口可见时运行）
  useEffect(() => {
    isMounted.current = true;

    const fetchData = createFetchFn();
    fetchData();
    intervalRef.current = setInterval(fetchData, POLL_INTERVAL_MS);

    return () => {
      isMounted.current = false;
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [createFetchFn]);

  // 筛选逻辑（过滤掉虚拟内存传感器）
  const filtered = useMemo(() => sensors.filter((s) => {
    // 过滤掉虚拟内存传感器
    if (s.name.toLowerCase().includes("virtual memory")) return false;
    if (s.name.toLowerCase().includes("virtual")) return false;

    if (search) {
      const q = search.toLowerCase();
      if (!s.hardware.toLowerCase().includes(q) && !s.name.toLowerCase().includes(q) && !s.sensorType.toLowerCase().includes(q)) {
        return false;
      }
    }
    if (sensorTypeFilter !== "all" && s.sensorType !== sensorTypeFilter) return false;
    if (hardwareTypeFilter !== "all") {
      if (hardwareTypeFilter === "Gpu") {
        if (!s.hardwareType.toLocaleLowerCase().startsWith("gpu")) return false;
      } else if (s.hardwareType !== hardwareTypeFilter) return false;
    }
    return true;
  }), [sensors, search, sensorTypeFilter, hardwareTypeFilter]);

  const groups = useMemo(() => groupSensors(filtered), [filtered]);

  // 收集所有可用的传感器类型和硬件类型用于筛选
  const allSensorTypes = useMemo(() => Array.from(new Set(sensors.filter((s) => {
    // 过滤掉虚拟内存，避免出现在筛选选项里
    if (s.name.toLowerCase().includes("virtual memory")) return false;
    if (s.name.toLowerCase().includes("virtual")) return false;
    return true;
  }).map((s) => s.sensorType))).sort(), [sensors]);

  const allHardwareTypes = useMemo(() => Array.from(new Set(sensors.map((s) => {
    const ht = s.hardwareType;
    return ht.toLocaleLowerCase().startsWith("gpu") ? "Gpu" : ht;
  }))).sort(), [sensors]);

  // 更新状态显示
  const formatTime = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    } catch {
      return iso;
    }
  };

  return (
    <Box
      h="100vh"
      overflowY="auto"
      bg={bg}
      color={textColor}
      fontFamily="'MiSans', sans-serif"
      sx={{
        "&::-webkit-scrollbar": {
          width: "6px",
          height: "6px",
        },
        "&::-webkit-scrollbar-track": {
          background: "transparent",
          margin: "10px 0",
        },
        "&::-webkit-scrollbar-thumb": {
          background: "#3498db",
          borderRadius: "3px",
          minHeight: "40px",
        },
        "&::-webkit-scrollbar-thumb:hover": {
          background: "#2980b9",
        },
      }}
    >
      {/* 粘性顶部区域 */}
      <Box
        position="sticky"
        top={0}
        zIndex={10}
        bg={bg}
        pb={4}
        pt={6}
        px={6}
      >
        {/* 顶部标题栏 */}
        <Flex
          justify="space-between"
          align="center"
          mb={5}
          flexWrap="wrap"
          gap={3}
        >
          <HStack spacing={3}>
            <Activity size={24} color="#3498db" />
            <Heading size="lg" fontWeight="semibold" color="gray.100">
              传感器监控
            </Heading>
            {!loading && (
              <Badge colorScheme={error ? "red" : "green"} variant="subtle" fontSize="xs">
                {error ? "连接异常" : "在线"}
              </Badge>
            )}
          </HStack>
          <HStack spacing={4} fontSize="sm" color="gray.500">
            <Text>{sensors.length} 个传感器</Text>
            {updatedAt && <Text>更新于 {formatTime(updatedAt)}</Text>}
          </HStack>
        </Flex>

        {/* 筛选栏 */}
        <VStack spacing={3}>
          <InputGroup size="sm" maxW="480px" w="full">
            <InputLeftElement pointerEvents="none">
              <Search size={16} color="gray.500" />
            </InputLeftElement>
            <Input
              placeholder="搜索硬件名称、传感器名称、传感器类型..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              bg="rgba(255,255,255,0.04)"
              border="1px solid"
              borderColor="rgba(255,255,255,0.1)"
              _hover={{ borderColor: "rgba(255,255,255,0.2)" }}
              _focus={{ borderColor: "#3498db", boxShadow: "0 0 0 1px #3498db" }}
              color="gray.200"
              fontSize="sm"
              borderRadius="lg"
            />
            {search && (
              <IconButton
                aria-label="清除搜索"
                icon={<X size={16} />}
                size="xs"
                data-corner="round"
                position="absolute"
                right={2}
                top="50%"
                transform="translateY(-50%)"
                variant="ghost"
                color="gray.500"
                _hover={{ color: "gray.300" }}
                onClick={() => setSearch("")}
                zIndex={2}
              />
            )}
          </InputGroup>
          <HStack spacing={3} flexWrap="wrap" justify="center">
            <FilterDropdown
              value={hardwareTypeFilter}
              onChange={setHardwareTypeFilter}
              allLabel="全部硬件"
              options={allHardwareTypes.map((ht) => ({ value: ht, label: getHardwareLabel(ht) }))}
              minW="120px"
            />
            <FilterDropdown
              value={sensorTypeFilter}
              onChange={setSensorTypeFilter}
              allLabel="全部类型"
              options={allSensorTypes.map((st) => ({ value: st, label: getSensorTypeLabel(st) }))}
              minW="120px"
            />
          </HStack>
        </VStack>
      </Box>

      {/* 内容区 */}
      <Box px={6} pb={6}>
        {loading ? (
          <Center py={20}>
            <VStack spacing={4}>
              <Spinner size="xl" color="gray.400" thickness="3px" />
              <Text color="gray.500" fontSize="sm">正在加载传感器数据...</Text>
            </VStack>
          </Center>
        ) : error ? (
          <Center py={20}>
            <VStack spacing={3}>
              <Box color="#e74c3c" opacity={0.6}>
                <Thermometer size={48} />
              </Box>
              <Text color="#e74c3c" fontSize="sm" textAlign="center">
                传感器数据加载失败
              </Text>
              <Text color="gray.500" fontSize="xs" textAlign="center" maxW="400px">
                {error}
              </Text>
              <IconButton
                aria-label="重试"
                icon={<RefreshCw size={16} />}
                size="sm"
                variant="outline"
                colorScheme="blue"
                onClick={() => {
                  setLoading(true);
                  setError(null);
                  invoke<SensorsResponse>("get_all_sensors").then((resp) => {
                    setSensors(resp.sensors);
                    setUpdatedAt(resp.updatedAt);
                    setError(null);
                  }).catch((e) => setError(String(e))).finally(() => setLoading(false));
                }}
              />
            </VStack>
          </Center>
        ) : groups.length === 0 ? (
          <Center py={20}>
            <Text color="gray.500" fontSize="sm">
              {search ? "没有匹配的传感器" : "暂无传感器数据"}
            </Text>
          </Center>
        ) : (
          <VStack spacing={3.5} align="stretch" maxW="960px" mx="auto">
            {groups.map((group, i) => {
              const groupKey = `${group.hardwareType}-${group.hardware}-${i}`;
              return (
                <SensorGroupCard
                  key={groupKey}
                  group={group}
                  collapsed={!expandedGroups.has(groupKey)}
                  onToggle={() => toggleGroup(groupKey)}
                />
              );
            })}
          </VStack>
        )}
      </Box>
    </Box>
  );
}