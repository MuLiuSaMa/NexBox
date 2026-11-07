import {
  Box,
  Text,
  VStack,
  HStack,
  SimpleGrid,
  useColorModeValue,
  useColorMode,
  Button,
  Badge,
  Spinner,
  Progress,
  Tooltip,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { RefreshCw, Thermometer, AlertTriangle, Star, Sparkles, Wrench, Clock, Download, Upload, Power, Shuffle, Timer } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";

interface PartitionInfo {
  drive_letter: string;
  total_gb: number;
  available_gb: number;
  used_gb: number;
  usage_percent: number;
  filesystem: string;
}

interface DiskHealthInfo {
  index: number;
  model: string;
  media_type: string;
  size_gb: number;
  interface_type: string;
  health_status: string;
  operational_status: string;
  temperature_c: number | null;
  wear_percentage: number | null;
  power_on_hours: number | null;
  power_on_count: number | null;
  data_read_bytes: number | null;
  data_written_bytes: number | null;
  read_errors: number | null;
  write_errors: number | null;
  status: string;
  partition_count: number;
  serial_number: string;
  partition_style: string;
  is_boot_disk: boolean;
  partitions: PartitionInfo[];
  total_usage_gb: number;
  total_capacity_gb: number;
  health_percent: number | null;
  is_ssd: boolean;
}

interface DiskHealthResponse {
  disks: DiskHealthInfo[];
  total_count: number;
  healthy_count: number;
  warning_count: number;
  unhealthy_count: number;
}

/** `disk-speedtest:progress` 事件负载（与 Rust DiskSpeedProgress 对齐，result 字段平铺） */
interface DiskSpeedProgress {
  stage: "prepare" | "seq_write" | "seq_read" | "rnd_write" | "rnd_read" | "done" | "cancelled" | "error";
  drive: string;
  progressPct: number;
  currentMbps: number;
  seqWriteMbps: number;
  seqReadMbps: number;
  seqWriteIops: number;
  seqReadIops: number;
  rndWriteMbps: number;
  rndReadMbps: number;
  rndWriteIops: number;
  rndReadIops: number;
  latWriteMs: number;
  latReadMs: number;
  message: string;
}

/** 带宽格式化：统一用 MB/s（用户口径，不做 GB/s 换算） */
function formatBandwidth(mbps: number): string {
  if (!Number.isFinite(mbps) || mbps <= 0) return "--";
  return `${mbps >= 1000 ? mbps.toFixed(0) : mbps.toFixed(1)} MB/s`;
}

/** IOPS 格式化：千位 / 万位 / 百万位缩写 */
function formatIops(iops: number): string {
  if (!Number.isFinite(iops) || iops <= 0) return "--";
  if (iops >= 1_000_000) return `${(iops / 1_000_000).toFixed(2)}M`;
  if (iops >= 10_000) return `${(iops / 1000).toFixed(1)}K`;
  return Math.round(iops).toLocaleString();
}

interface DiskOptimizeResult {
  drive_letter: string;
  operation: string;
  is_ssd: boolean;
  background: boolean;
  success: boolean;
  message: string;
}

// 分区整理/优化按钮：固态盘做 TRIM/优化，机械盘做碎片整理。
// isSsd 由后端 SMART 直读判定返回（前端 MediaType 对 NVMe 盘常为空导致误判）。
function OptimizeButton({
  letter,
  index,
  interfaceType,
  model,
  isSsd,
}: {
  letter: string;
  index: number;
  interfaceType: string;
  model: string;
  isSsd: boolean;
}) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("disk");
  const { getActiveColor } = useThemeColor();
  const [loading, setLoading] = useState(false);

  // 按钮文案与图标依据后端判定：SSD 显示「优化/TRIM」，机械盘显示「整理碎片」
  const label = isSsd ? t("diskHealth.optimize") : t("diskHealth.defrag");

  const handle = async () => {
    setLoading(true);
    try {
      const res = await invoke<DiskOptimizeResult>("optimize_disk", {
        driveLetter: letter,
        index,
        interfaceType,
        model,
      });
      if (res.background) {
        // 机械盘碎片整理：已在后台低优先级启动，立即返回，不阻塞使用
        toast({
          title: `${letter}: ${t("diskHealth.defragStarted")}`,
          description: res.message,
          status: "info",
          duration: 6000,
          isClosable: true,
          variant: "left-accent",
        });
      } else {
        const done = res.operation === "retrim"
          ? t("diskHealth.optimizeDone")
          : res.operation === "defrag"
            ? t("diskHealth.defragDone")
            : t("diskHealth.optimizeDone");
        toast({
          title: `${letter}: ${done}`,
          description: res.message,
          status: "success",
          duration: 6000,
          isClosable: true,
          variant: "left-accent",
        });
      }
    } catch (e) {
      toast({
        title: t("diskHealth.optimizeFailed"),
        description: String(e),
        status: "error",
        duration: 6000,
        isClosable: true,
        variant: "left-accent",
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <Tooltip label={isSsd ? t("diskHealth.optimizeHint") : t("diskHealth.defragHint")} hasArrow>
      <Button
        leftIcon={isSsd ? <Sparkles size={13} /> : <Wrench size={13} />}
        size="xs"
        variant="outline"
        color={getActiveColor()}
        borderColor={getActiveColor()}
        _hover={{ bg: getActiveColor(), color: "#fff", borderColor: getActiveColor() }}
        fontWeight="medium"
        isLoading={loading}
        loadingText={t("diskHealth.optimizing")}
        onClick={handle}
        flex="none"
      >
        {label}
      </Button>
    </Tooltip>
  );
}

function SettingCard({ title, children }: { title: string; children: React.ReactNode }) {
  const { liquidGlassEnabled } = useBackground();
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { colorMode } = useColorMode();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={5}>
        <VStack align="stretch" spacing={4}>
          <Text fontWeight="medium" color={headerColor}>{title}</Text>
          {children}
        </VStack>
      </LiquidGlassCard>
    );
  }

  return (
    <Box bg={cardBg} borderRadius="3xl" p={5} border="1px solid" borderColor={borderColor}>
      <VStack align="stretch" spacing={4}>
        <Text fontWeight="medium" color={headerColor}>{title}</Text>
        {children}
      </VStack>
    </Box>
  );
}

function HealthBadge({ status }: { status: string }) {
  switch (status.toLowerCase()) {
    case "healthy":
      return <Badge colorScheme="green" px={2} py={0.5} borderRadius="full">Healthy</Badge>;
    case "warning":
      return <Badge colorScheme="yellow" px={2} py={0.5} borderRadius="full">Warning</Badge>;
    case "unhealthy":
      return <Badge colorScheme="red" px={2} py={0.5} borderRadius="full">Unhealthy</Badge>;
    default:
      return <Badge colorScheme="gray" px={2} py={0.5} borderRadius="full">{status}</Badge>;
  }
}

function MediaTypeBadge({ type }: { type: string }) {
  const t = type.toLowerCase();
  const label = t.includes("hdd") ? "HDD" : t.includes("nvme") ? "NVMe" : t.includes("ssd") ? "SSD" : type;
  const cs = t.includes("ssd") || t.includes("nvme") ? "blue" : "orange";
  return <Badge colorScheme={cs} px={2} py={0.5} borderRadius="md" fontSize="xs">{label}</Badge>;
}

function formatGb(gb: number): string {
  if (gb >= 1024) return `${(gb / 1024).toFixed(2)}TB`;
  if (gb >= 1) return `${gb.toFixed(2)}GB`;
  return `${(gb * 1024).toFixed(1)}MB`;
}

// 通电小时数格式化（千分位）
function formatPowerOnHours(hours: number | null): string {
  if (hours === null || hours < 0) return "--";
  return Math.round(hours).toLocaleString();
}

// 通电次数格式化（千分位）
function formatCount(count: number | null): string {
  if (count === null || count < 0) return "--";
  return Math.round(count).toLocaleString();
}

// 数据量格式化（字节 → TB/GB，1024^3 进制，与分区容量一致）
function formatBytes(bytes: number | null): string {
  if (bytes === null || bytes < 0) return "--";
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1024) return `${(gb / 1024).toFixed(2)} TB`;
  return `${gb.toFixed(1)} GB`;
}

/** 测速结果独立磁贴卡：图标 + 项目名 + 主值 + 次值（IOPS），样式与页面指标磁贴一致 */
function SpeedCard({
  icon: Icon,
  label,
  value,
  secondary,
}: {
  icon: React.ComponentType<{ size?: number; color?: string }>;
  label: string;
  value: string;
  secondary?: string;
}) {
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor } = useThemeColor();
  const subColor = useColorModeValue("gray.500", "gray.400");
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { colorMode } = useColorMode();
  const titleColor = colorMode === "light" ? "#000000" : "#ffffff";

  const body = (
    <VStack align="start" spacing={1}>
      <HStack spacing={2}>
        <Icon size={14} color={getActiveColor()} />
        <Text fontSize="xs" color={subColor} noOfLines={1}>{label}</Text>
      </HStack>
      <Text fontSize="xl" fontWeight="bold" color={titleColor} lineHeight="1.1">
        {value}
      </Text>
      {secondary ? (
        <Text fontSize="2xs" color={subColor}>{secondary}</Text>
      ) : (
        <Text fontSize="2xs" color="transparent" userSelect="none">
          &nbsp;
        </Text>
      )}
    </VStack>
  );

  if (liquidGlassEnabled) {
    return <LiquidGlassCard p={4}>{body}</LiquidGlassCard>;
  }
  return (
    <Box bg={cardBg} borderRadius="2xl" p={4} border="1px solid" borderColor={borderColor}>
      {body}
    </Box>
  );
}

/** 「测速」板块：按物理硬盘选择 + 全矩阵测试（顺序 1MiB / 随机 4K / 访问时间）。
 *  后端走无缓冲 IO（CrystalDiskMark 口径）；测试文件落在该硬盘剩余空间最大的分区。 */
function DiskSpeedTestSection({ disks }: { disks: DiskHealthInfo[] }) {
  const { t } = useTranslation();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const subColor = useColorModeValue("gray.500", "gray.400");
  const labelColor = useColorModeValue("gray.800", "#ffffff");
  const trackBg = useColorModeValue("gray.100", "#222222");

  const [diskIndex, setDiskIndex] = useState(() => {
    const boot = disks.find((d) => d.is_boot_disk);
    return String(boot?.index ?? disks[0]?.index ?? "");
  });
  const [testing, setTesting] = useState(false);
  const [progress, setProgress] = useState<DiskSpeedProgress | null>(null);
  const [result, setResult] = useState<DiskSpeedProgress | null>(null);
  const [error, setError] = useState("");
  const testingRef = useRef(false);

  // 磁盘列表刷新后，若当前选中项不在列表里则回落到系统盘 / 第一项
  useEffect(() => {
    if (disks.length > 0 && !disks.some((d) => String(d.index) === diskIndex)) {
      const boot = disks.find((d) => d.is_boot_disk);
      setDiskIndex(String(boot?.index ?? disks[0].index));
    }
  }, [disks, diskIndex]);

  // 进度事件：done 落最终数值；error/cancelled 收尾
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    (async () => {
      const fn = await listen<DiskSpeedProgress>("disk-speedtest:progress", (e) => {
        const p = e.payload;
        setProgress(p);
        if (p.stage === "done") {
          setResult(p);
          testingRef.current = false;
          setTesting(false);
        } else if (p.stage === "cancelled") {
          testingRef.current = false;
          setTesting(false);
        } else if (p.stage === "error") {
          setError(p.message);
          testingRef.current = false;
          setTesting(false);
        }
      });
      if (disposed) fn();
      else unlisten = fn;
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // 离开页面时若仍在测速，通知后端中止（MousePollingPage 同款兜底）
  useEffect(
    () => () => {
      if (testingRef.current) {
        invoke("disk_speed_test_stop").catch(() => {});
      }
    },
    []
  );

  const stageLabel = (stage: string): string => {
    const keys: Record<string, string> = {
      prepare: "stagePrepare",
      seq_write: "stageSeqWrite",
      seq_read: "stageSeqRead",
      rnd_write: "stageRndWrite",
      rnd_read: "stageRndRead",
    };
    const key = keys[stage];
    return key ? t(`diskHealth.speedTest.${key}`) : "";
  };

  const handleStart = async () => {
    if (testingRef.current) return;
    // 按物理硬盘选择：测试文件落在该硬盘剩余空间最大的分区上
    const disk = disks.find((d) => String(d.index) === diskIndex);
    const target = disk?.partitions.length
      ? disk.partitions.reduce((a, b) => (b.available_gb > a.available_gb ? b : a))
      : null;
    if (!target) {
      setError(t("diskHealth.speedTest.noPartition"));
      return;
    }
    setError("");
    setResult(null);
    setProgress(null);
    testingRef.current = true;
    setTesting(true);
    try {
      await invoke("disk_speed_test_start", { driveLetter: target.drive_letter });
    } catch (e) {
      // 后端已发出 error 事件；这里兜住启动被拒（如重复启动）的情况
      setError(String(e));
      testingRef.current = false;
      setTesting(false);
    }
  };

  const handleStop = async () => {
    await invoke("disk_speed_test_stop").catch(() => {});
  };

  return (
    <SettingCard title={t("diskHealth.speedTest.title")}>
      {disks.length === 0 ? (
        <Text fontSize="sm" color={subColor}>
          {t("diskHealth.speedTest.noDrive")}
        </Text>
      ) : (
        <VStack align="stretch" spacing={4}>
          <HStack justify="space-between" align="center" flexWrap="wrap" gap={2}>
            <HStack spacing={3} flex={1} minW="260px">
              <Text fontSize="sm" color={labelColor} flexShrink={0}>
                {t("diskHealth.speedTest.drive")}
              </Text>
              <Box maxW="320px" w="full">
                <CustomSelect
                  value={diskIndex}
                  onChange={(val) => setDiskIndex(val)}
                  width="100%"
                  options={disks.map((d) => ({
                    value: String(d.index),
                    label: `${d.model}（${formatGb(d.size_gb)}）`,
                  }))}
                />
              </Box>
            </HStack>
            {testing ? (
              <Button
                size="sm"
                variant="outline"
                colorScheme="red"
                onClick={handleStop}
                flexShrink={0}
              >
                {t("diskHealth.speedTest.stop")}
              </Button>
            ) : (
              <Button
                size="sm"
                bg={getActiveColor()}
                color={getContrastTextColor()}
                _hover={{ opacity: 0.88 }}
                onClick={handleStart}
                flexShrink={0}
              >
                {t("diskHealth.speedTest.start")}
              </Button>
            )}
          </HStack>

          {testing && progress && (
            <VStack align="stretch" spacing={2}>
              <Progress
                value={progress.progressPct}
                size="sm"
                borderRadius="full"
                hasStripe
                sx={{ "& > div": { background: getActiveColor() } }}
                bg={trackBg}
              />
              <HStack justify="space-between">
                <Text fontSize="xs" color={subColor}>
                  {progress.drive} · {stageLabel(progress.stage) || progress.message}
                </Text>
                <Text fontSize="xs" fontWeight="bold" color={getActiveColor()}>
                  {progress.currentMbps > 0 ? `${progress.currentMbps.toFixed(1)} MB/s` : ""}
                </Text>
              </HStack>
            </VStack>
          )}

          {result && (
            <SimpleGrid columns={{ base: 2, md: 3 }} spacing={3}>
              <SpeedCard
                icon={Upload}
                label={t("diskHealth.speedTest.seqWrite")}
                value={formatBandwidth(result.seqWriteMbps)}
                secondary={`${formatIops(result.seqWriteIops)} IOPS`}
              />
              <SpeedCard
                icon={Download}
                label={t("diskHealth.speedTest.seqRead")}
                value={formatBandwidth(result.seqReadMbps)}
                secondary={`${formatIops(result.seqReadIops)} IOPS`}
              />
              <SpeedCard
                icon={Shuffle}
                label={t("diskHealth.speedTest.rndWrite")}
                value={formatBandwidth(result.rndWriteMbps)}
                secondary={`${formatIops(result.rndWriteIops)} IOPS`}
              />
              <SpeedCard
                icon={Shuffle}
                label={t("diskHealth.speedTest.rndRead")}
                value={formatBandwidth(result.rndReadMbps)}
                secondary={`${formatIops(result.rndReadIops)} IOPS`}
              />
              <SpeedCard
                icon={Timer}
                label={t("diskHealth.speedTest.latWrite")}
                value={`${result.latWriteMs.toFixed(2)} ms`}
              />
              <SpeedCard
                icon={Timer}
                label={t("diskHealth.speedTest.latRead")}
                value={`${result.latReadMs.toFixed(2)} ms`}
              />
            </SimpleGrid>
          )}

          {error && (
            <Text fontSize="xs" color="red.400">
              {error}
            </Text>
          )}
        </VStack>
      )}
    </SettingCard>
  );
}

export default function DiskHealthPage() {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const { liquidGlassEnabled } = useBackground();

  const [response, setResponse] = useState<DiskHealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const headingColor = useColorModeValue("black", "#ffffff");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  const fetchHealth = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await invoke<DiskHealthResponse>("get_disk_health_info");
      setResponse(data);
    } catch (e) {
      setError(String(e));
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    fetchHealth();
  }, [fetchHealth]);

  return (
    <Box pt={8} pb={8}>
      <PageHeader title={t("diskHealth.title")} mb={6}>
        <Button
          leftIcon={<RefreshCw size={16} />}
          variant="outline"
          size="sm"
          onClick={fetchHealth}
          isLoading={loading}
          loadingText={t("diskHealth.scanning")}
      >
          {t("diskHealth.refresh")}
        </Button>
      </PageHeader>

      {loading && !response && (
        <VStack py={20} spacing={4}>
          <Spinner size="xl" color={getActiveColor()} />
          <Text color={subTextColor}>{t("diskHealth.scanning")}</Text>
        </VStack>
      )}

      {error && (
        <LiquidGlassCard p={6}>
          <VStack spacing={3}>
            <AlertTriangle size={32} color="red" />
            <Text color="red.400" fontWeight="medium">{t("diskHealth.loadError")}</Text>
            <Text color={subTextColor} fontSize="sm" textAlign="center">{error}</Text>
            <Button size="sm" onClick={fetchHealth}>{t("diskHealth.retry")}</Button>
          </VStack>
        </LiquidGlassCard>
      )}

      {response && (
        <VStack align="stretch" spacing={6}>
          <SimpleGrid columns={{ base: 2, md: 4 }} spacing={4}>
            <LiquidGlassCard p={4} textAlign="center">
              <Text fontSize="3xl" fontWeight="bold" color={headingColor}>{response.total_count}</Text>
              <Text fontSize="sm" color={subTextColor}>{t("diskHealth.total")}</Text>
            </LiquidGlassCard>
            <LiquidGlassCard p={4} textAlign="center">
              <Text fontSize="3xl" fontWeight="bold" color="green.400">{response.healthy_count}</Text>
              <Text fontSize="sm" color={subTextColor}>{t("diskHealth.healthy")}</Text>
            </LiquidGlassCard>
            <LiquidGlassCard p={4} textAlign="center">
              <Text fontSize="3xl" fontWeight="bold" color="yellow.400">{response.warning_count}</Text>
              <Text fontSize="sm" color={subTextColor}>{t("diskHealth.warning")}</Text>
            </LiquidGlassCard>
            <LiquidGlassCard p={4} textAlign="center">
              <Text fontSize="3xl" fontWeight="bold" color="red.400">{response.unhealthy_count}</Text>
              <Text fontSize="sm" color={subTextColor}>{t("diskHealth.unhealthy")}</Text>
            </LiquidGlassCard>
          </SimpleGrid>

          {response.disks.map((disk) => (
            <SettingCard key={disk.index} title={disk.model}>
              <VStack align="stretch" spacing={4}>
                <HStack justify="space-between" wrap="wrap" gap={2}>
                  <HStack gap={2}>
                    <HealthBadge status={disk.health_status} />
                    <MediaTypeBadge type={disk.media_type} />
                    {disk.partition_style && (
                      <Badge colorScheme="telegram" px={2} py={0.5} borderRadius="md" fontSize="xs">
                        {disk.partition_style}
                      </Badge>
                    )}
                    {disk.interface_type && (
                      <Badge colorScheme="gray" px={2} py={0.5} borderRadius="md" fontSize="xs">
                        {disk.interface_type}
                      </Badge>
                    )}
                    {disk.is_boot_disk && (
                      <Badge colorScheme="yellow" px={2} py={0.5} borderRadius="md" fontSize="xs" display="flex" alignItems="center" gap={1}>
                        <Star size={10} /> System
                      </Badge>
                    )}
                  </HStack>
                  <HStack align="center" gap={3}>
                    <Text fontSize="sm" color={subTextColor}>
                      {disk.size_gb >= 1024
                        ? `${(disk.size_gb / 1024).toFixed(2)} TB`
                        : `${disk.size_gb.toFixed(2)} GB`}
                    </Text>
                    {/* 健康度纯大数字（CrystalDiskInfo 方案） */}
                    <Box textAlign="center">
                      <Text
                        fontSize="3xl"
                        fontWeight="bold"
                        lineHeight="1"
                        color={disk.health_percent !== null
                          ? disk.health_percent > 50 ? "green.400" : disk.health_percent > 20 ? "yellow.400" : "red.400"
                          : subTextColor}
                        sx={{ fontVariantNumeric: "tabular-nums" }}
                      >
                        {disk.health_percent !== null ? disk.health_percent : "--"}
                      </Text>
                      <Text fontSize="10px" color={subTextColor} mt={0.5}>{t("diskHealth.health")}</Text>
                    </Box>
                  </HStack>
                </HStack>

                {disk.partitions.map((part) => (
                  <Box key={part.drive_letter}>
                    <HStack justify="space-between" mb={1}>
                      <HStack spacing={1}>
                        <Badge colorScheme="purple" px={2} py={0} borderRadius="md" fontSize="xs">
                          {part.drive_letter}:
                        </Badge>
                        {part.filesystem && (
                          <Badge colorScheme="cyan" px={1.5} py={0} borderRadius="md" fontSize="2xs" textTransform="uppercase">
                            {part.filesystem}
                          </Badge>
                        )}
                        <Text fontSize="xs" color={subTextColor}>
                          {formatGb(part.used_gb)} / {formatGb(part.total_gb)}
                        </Text>
                      </HStack>
                      <Text fontSize="xs" fontWeight="medium" color={part.usage_percent > 90 ? "red.400" : part.usage_percent > 75 ? "yellow.400" : textColor}>
                        {part.usage_percent.toFixed(0)}%
                      </Text>
                      <OptimizeButton
                        letter={part.drive_letter}
                        index={disk.index}
                        interfaceType={disk.interface_type}
                        model={disk.model}
                        isSsd={disk.is_ssd}
                      />
                    </HStack>
                    <Progress
                      value={part.usage_percent}
                      size="sm"
                      borderRadius="full"
                      colorScheme={part.usage_percent > 90 ? "red" : part.usage_percent > 75 ? "yellow" : "green"}
                      bg={useColorModeValue("gray.200", "gray.600")}
                    />
                  </Box>
                ))}

                <SimpleGrid columns={{ base: 2, md: 3 }} spacing={4}>
                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <HStack justify="center" mb={1}>
                        <Thermometer size={14} />
                        <Text fontSize="xs" color={headingColor}>{t("diskHealth.temperature")}</Text>
                      </HStack>
                      {disk.temperature_c !== null ? (
                        <Text fontSize="lg" fontWeight="bold" color={disk.temperature_c > 55 ? "orange.400" : disk.temperature_c > 45 ? "yellow.400" : "green.400"}>
                          {disk.temperature_c.toFixed(0)}°C
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={headingColor}>--</Text>
                      )}
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <HStack justify="center" mb={1}>
                        <Thermometer size={14} />
                        <Text fontSize="xs" color={subTextColor}>{t("diskHealth.temperature")}</Text>
                      </HStack>
                      {disk.temperature_c !== null ? (
                        <Text fontSize="lg" fontWeight="bold" color={disk.temperature_c > 55 ? "orange.400" : disk.temperature_c > 45 ? "yellow.400" : "green.400"}>
                          {disk.temperature_c.toFixed(0)}°C
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={subTextColor}>--</Text>
                      )}
                    </Box>
                  )}

                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <HStack justify="center" mb={1}>
                        <Clock size={14} />
                        <Text fontSize="xs" color={headingColor}>{t("diskHealth.powerOnHours")}</Text>
                      </HStack>
                      {disk.power_on_hours !== null ? (
                        <Text
                          fontSize="lg"
                          fontWeight="bold"
                          color={headingColor}
                          sx={{ fontVariantNumeric: "tabular-nums" }}
                        >
                          {t("diskHealth.powerOnHoursValue", { hours: formatPowerOnHours(disk.power_on_hours) })}
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={headingColor}>--</Text>
                      )}
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <HStack justify="center" mb={1}>
                        <Clock size={14} />
                        <Text fontSize="xs" color={subTextColor}>{t("diskHealth.powerOnHours")}</Text>
                      </HStack>
                      {disk.power_on_hours !== null ? (
                        <Text
                          fontSize="lg"
                          fontWeight="bold"
                          color={textColor}
                          sx={{ fontVariantNumeric: "tabular-nums" }}
                        >
                          {t("diskHealth.powerOnHoursValue", { hours: formatPowerOnHours(disk.power_on_hours) })}
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={subTextColor}>--</Text>
                      )}
                    </Box>
                  )}

                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <HStack justify="center" mb={1}>
                        <Power size={14} />
                        <Text fontSize="xs" color={headingColor}>{t("diskHealth.powerOnCount")}</Text>
                      </HStack>
                      {disk.power_on_count !== null ? (
                        <Text fontSize="lg" fontWeight="bold" color={headingColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                          {formatCount(disk.power_on_count)}
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={headingColor}>--</Text>
                      )}
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <HStack justify="center" mb={1}>
                        <Power size={14} />
                        <Text fontSize="xs" color={subTextColor}>{t("diskHealth.powerOnCount")}</Text>
                      </HStack>
                      {disk.power_on_count !== null ? (
                        <Text fontSize="lg" fontWeight="bold" color={textColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                          {formatCount(disk.power_on_count)}
                        </Text>
                      ) : (
                        <Text fontSize="lg" color={subTextColor}>--</Text>
                      )}
                    </Box>
                  )}

                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <HStack justify="center" mb={1}>
                        <Download size={14} />
                        <Text fontSize="xs" color={headingColor}>{t("diskHealth.dataRead")}</Text>
                      </HStack>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                        {formatBytes(disk.data_read_bytes)}
                      </Text>
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <HStack justify="center" mb={1}>
                        <Download size={14} />
                        <Text fontSize="xs" color={subTextColor}>{t("diskHealth.dataRead")}</Text>
                      </HStack>
                      <Text fontSize="lg" fontWeight="bold" color={textColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                        {formatBytes(disk.data_read_bytes)}
                      </Text>
                    </Box>
                  )}

                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <HStack justify="center" mb={1}>
                        <Upload size={14} />
                        <Text fontSize="xs" color={headingColor}>{t("diskHealth.dataWritten")}</Text>
                      </HStack>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                        {formatBytes(disk.data_written_bytes)}
                      </Text>
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <HStack justify="center" mb={1}>
                        <Upload size={14} />
                        <Text fontSize="xs" color={subTextColor}>{t("diskHealth.dataWritten")}</Text>
                      </HStack>
                      <Text fontSize="lg" fontWeight="bold" color={textColor} sx={{ fontVariantNumeric: "tabular-nums" }}>
                        {formatBytes(disk.data_written_bytes)}
                      </Text>
                    </Box>
                  )}

                  {liquidGlassEnabled ? (
                    <LiquidGlassCard p={3} textAlign="center">
                      <Text fontSize="xs" color={headingColor} mb={1}>{t("diskHealth.operationalStatus")}</Text>
                      <Text fontSize="sm" fontWeight="medium" color={headingColor} noOfLines={1}>
                        {disk.operational_status}
                      </Text>
                    </LiquidGlassCard>
                  ) : (
                    <Box textAlign="center" p={3} borderRadius="lg" bg={useColorModeValue("gray.50", "rgba(255,255,255,0.03)")}>
                      <Text fontSize="xs" color={subTextColor} mb={1}>{t("diskHealth.operationalStatus")}</Text>
                      <Text fontSize="sm" fontWeight="medium" color={textColor} noOfLines={1}>
                        {disk.operational_status}
                      </Text>
                    </Box>
                  )}
                </SimpleGrid>
              </VStack>
            </SettingCard>
          ))}

          <DiskSpeedTestSection disks={response?.disks ?? []} />
        </VStack>
      )}
    </Box>
  );
}