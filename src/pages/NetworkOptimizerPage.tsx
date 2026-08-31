import { memo, useState, useEffect, useCallback, useRef } from "react";
import {
  Box,
  Heading,
  VStack,
  HStack,
  Text,
  SimpleGrid,
  Flex,
  Switch,
  Button,
  IconButton,
  Input,
  useColorModeValue,
  Spinner,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { ArrowLeft, Copy, Globe, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { store } from "@/lib/store";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import {
  dnsPresets,
  networkOptimizerItems,
  type DnsPreset,
  type NetworkOptimizerItem,
} from "@/config/network-optimizer";

const STORE_KEY = "network_optimizer_states";
const DNS_STORE_KEY = "network_optimizer_dns";

// ============ 模块级 memo 组件（避免因父组件重渲染导致卸载重挂载，保证 Switch 动画正常） ============

interface OptimizeCardProps {
  item: NetworkOptimizerItem;
  state: boolean;
  isToggling: boolean;
  onToggle: (item: NetworkOptimizerItem, enable: boolean) => void;
  headingColor: string;
  subTextColor: string;
  activeColor: string;
  cardBg: string;
  cardBorder: string;
  liquidGlassEnabled: boolean;
  t: TFunction;
}

const OptimizeCard = memo(function OptimizeCard({
  item,
  state,
  isToggling,
  onToggle,
  headingColor,
  subTextColor,
  activeColor,
  cardBg,
  cardBorder,
  liquidGlassEnabled,
  t,
}: OptimizeCardProps) {
  const IconComponent = item.icon;

  const cardContent = (
    <Flex justify="space-between" align="center" gap={3}>
      <HStack spacing={3} align="center" flex={1} minW={0}>
        <Box
          w={10}
          h={10}
          borderRadius="lg"
          bg={`${item.color}20`}
          display="flex"
          alignItems="center"
          justifyContent="center"
          color={item.color}
          flexShrink={0}
        >
          <IconComponent size={20} />
        </Box>
        <Box minW={0} flex={1}>
          <Text color={headingColor} fontSize="sm" fontWeight="bold" noOfLines={1}>
            {t(item.titleKey)}
          </Text>
          <Text color={subTextColor} fontSize="xs" noOfLines={2} mt={0.5}>
            {t(item.descKey)}
          </Text>
        </Box>
      </HStack>
      <Switch
        isChecked={state}
        isDisabled={isToggling}
        onChange={() => onToggle(item, !state)}
        sx={{
          "& .chakra-switch__track[data-checked]": {
            bg: activeColor,
          },
        }}
        size="md"
      />
    </Flex>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard w="full" p={4}>
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      w="full"
      bg={cardBg}
      borderRadius="xl"
      border="1px solid"
      borderColor={cardBorder}
      p={4}
      transition="all 0.2s"
      _hover={{
        borderColor: item.color,
        boxShadow: `0 0 12px ${item.color}20`,
      }}
    >
      {cardContent}
    </Box>
  );
});

/** DNS 延迟:毫秒(含小数)+ 实际应答来源 + 路由网卡;"timeout" 表示超时/无响应。
 *  responder 与查询目标不一致、或延迟 <1ms,说明查询被本地拦截(TUN/安全软件/路由器) */
type DnsLatency = { ms: number; responder?: string; via?: string | null } | "timeout";

interface DnsCardProps {
  preset: DnsPreset;
  currentDns: { primary: string; secondary: string };
  applyingId: string | null;
  latency?: DnsLatency;
  onApply: (preset: DnsPreset) => void;
  headingColor: string;
  subTextColor: string;
  activeColor: string;
  contrastText: string;
  hoverBg: string;
  cardBg: string;
  cardBorder: string;
  liquidGlassEnabled: boolean;
  t: TFunction;
}

const DnsCard = memo(function DnsCard({
  preset,
  currentDns,
  applyingId,
  latency,
  onApply,
  headingColor,
  subTextColor,
  activeColor,
  contrastText,
  hoverBg,
  cardBg,
  cardBorder,
  liquidGlassEnabled,
  t,
}: DnsCardProps) {
  const isApplied =
    currentDns.primary === preset.primary &&
    currentDns.secondary === preset.secondary;
  const isLoading = applyingId === preset.id;

  // 劫持判定:应答来源与查询目标不一致,或延迟 < 1ms。
  // 公网 DNS 往返物理上不可能低于 1ms;TUN 代理会伪造源 IP 应答,
  // 所以来源校验之外还要用亚毫秒阈值兜底(本地拦截往返约 0.1~0.5ms)。
  const hijacked =
    latency !== undefined &&
    (latency === "timeout"
      ? false
      : latency.responder !== undefined && latency.responder !== preset.primary
        ? true
        : latency.ms < 1);

  // 延迟颜色按可用性分档:优(绿) / 一般(橙) / 差(红),超时/劫持用弱化文字色
  const latencyColor =
    latency === undefined || latency === "timeout"
      ? subTextColor
      : hijacked
        ? "#DD6B20"
        : latency.ms < 80
          ? "#38A169"
          : latency.ms < 200
            ? "#DD6B20"
            : "#E53E3E";

  const cardContent = (
    <VStack align="start" spacing={2} w="full">
      <HStack spacing={3} align="center" w="full">
        <Box
          w={10}
          h={10}
          borderRadius="lg"
          bg={`${preset.iconColor}20`}
          display="flex"
          alignItems="center"
          justifyContent="center"
          color={preset.iconColor}
          flexShrink={0}
        >
          <Globe size={20} />
        </Box>
        <Box flex={1} minW={0}>
          <HStack spacing={2}>
            <Text color={headingColor} fontSize="sm" fontWeight="bold" noOfLines={1}>
              {preset.name}
            </Text>
            {isApplied && (
              <Text color={activeColor} fontSize="xs" fontWeight="bold" flexShrink={0}>
                {t("networkOptimize.dns.applied")}
              </Text>
            )}
          </HStack>
        </Box>
        {latency !== undefined && (
          <Text
            color={latencyColor}
            fontSize="xs"
            fontWeight="bold"
            flexShrink={0}
            marginLeft="auto"
            title={
              hijacked && latency !== "timeout"
                ? latency.responder && latency.responder !== preset.primary
                  ? `响应来自 ${latency.responder}${latency.via ? `，经网卡「${latency.via}」路由` : ""}`
                  : `延迟低于 1ms${latency.via ? `（经网卡「${latency.via}」路由）` : ""}，查询未真正到达目标服务器`
                : latency !== "timeout" && latency.via
                  ? `经网卡「${latency.via}」路由`
                  : undefined
            }
          >
            {latency === "timeout"
              ? t("networkOptimize.dns.latencyTimeout")
              : hijacked
                ? t("networkOptimize.dns.hijacked")
                : `${Math.round(latency.ms)} ms`}
          </Text>
        )}
      </HStack>
      <VStack align="start" spacing={0} w="full" px={1}>
        <Text color={subTextColor} fontSize="xs">
          {preset.primary}
        </Text>
        {preset.secondary ? (
          <Text color={subTextColor} fontSize="xs">
            {preset.secondary}
          </Text>
        ) : null}
      </VStack>
      <Button
        size="sm"
        w="full"
        onClick={() => onApply(preset)}
        isLoading={isLoading}
        loadingText={t("networkOptimize.dns.apply")}
        {...(isApplied
          ? {
              variant: "outline",
              sx: {
                borderColor: activeColor,
                color: activeColor,
                _hover: { bg: hoverBg },
              },
            }
          : {
              bg: activeColor,
              color: contrastText,
              _hover: { opacity: 0.9 },
              _active: { transform: "scale(0.97)" },
            })}
      >
        {t("networkOptimize.dns.apply")}
      </Button>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard w="full" p={4}>
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      w="full"
      bg={cardBg}
      borderRadius="xl"
      border="1px solid"
      borderColor={isApplied ? activeColor : cardBorder}
      p={4}
      transition="all 0.2s"
      _hover={{
        borderColor: preset.iconColor,
        boxShadow: `0 0 12px ${preset.iconColor}20`,
      }}
    >
      {cardContent}
    </Box>
  );
});

export default function NetworkOptimizerPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { liquidGlassEnabled } = useBackground();
  const toast = useDynamicIsland("network");
  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();
  const adaptiveTitle = useAdaptiveTextColor();

  const [scannedStates, setScannedStates] = useState<Record<string, boolean>>({});
  const [savedStates, setSavedStates] = useState<Record<string, boolean>>({});
  const [isInitialScanning, setIsInitialScanning] = useState(true);
  const [isBatchOptimizing, setIsBatchOptimizing] = useState(false);
  const [togglingItems, setTogglingItems] = useState<Set<string>>(new Set());
  const [isRescanning, setIsRescanning] = useState(false);

  const [currentDns, setCurrentDns] = useState<{ primary: string; secondary: string }>({
    primary: "",
    secondary: "",
  });
  const [applyingDnsId, setApplyingDnsId] = useState<string | null>(null);
  const [isRestoringDns, setIsRestoringDns] = useState(false);
  const [isClearingDns, setIsClearingDns] = useState(false);
  const [isResettingNetwork, setIsResettingNetwork] = useState(false);
  const [isFixingDhcp, setIsFixingDhcp] = useState(false);
  const [customPrimary, setCustomPrimary] = useState("");
  const [customSecondary, setCustomSecondary] = useState("");
  const [isApplyingCustomDns, setIsApplyingCustomDns] = useState(false);

  // 公网 IP
  const [publicIp, setPublicIp] = useState("");
  const [isLoadingIp, setIsLoadingIp] = useState(false);
  const [ipLoadFailed, setIpLoadFailed] = useState(false);

  // DNS 延迟自动测速:每 1s 对所有预设的首选 DNS 发起真实查询,按卡片展示
  const [dnsLatency, setDnsLatency] = useState<Record<string, DnsLatency>>({});
  const dnsLatencyInFlight = useRef<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    const probe = (preset: DnsPreset) => {
      // 上一次查询未返回时跳过,避免超时服务器堆积并发请求
      if (dnsLatencyInFlight.current.has(preset.id)) return;
      dnsLatencyInFlight.current.add(preset.id);
      invoke<{ latency_ms: number; responder: string; via_interface: string | null }>(
        "test_dns_latency",
        { ip: preset.primary },
      )
        .then((r) => {
          if (!cancelled) {
            setDnsLatency((prev) => ({
              ...prev,
              [preset.id]: { ms: r.latency_ms, responder: r.responder, via: r.via_interface },
            }));
          }
        })
        .catch(() => {
          if (!cancelled) {
            setDnsLatency((prev) => ({ ...prev, [preset.id]: "timeout" }));
          }
        })
        .finally(() => {
          dnsLatencyInFlight.current.delete(preset.id);
        });
    };
    const probeAll = () => dnsPresets.forEach(probe);
    probeAll();
    const timer = setInterval(probeAll, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);

  // 初始化：加载网络状态 + DNS 配置
  useEffect(() => {
    let cancelled = false;
    async function init() {
      const startTime = Date.now();
      
      // 并行加载：本地保存的状态 + 后端实时检测
      const [savedResult, dnsSavedResult, scannedResult] = await Promise.allSettled([
        store.get<Record<string, boolean>>(STORE_KEY),
        store.get<{ primary: string; secondary: string }>(DNS_STORE_KEY),
        invoke("check_network_tweak_states").catch(() => null),
      ]);
      
      if (cancelled) return;
      
      // 加载保存的优化状态
      const saved =
        savedResult.status === "fulfilled" && savedResult.value
          ? savedResult.value
          : {};
      
      // 加载保存的 DNS 状态
      const savedDns =
        dnsSavedResult.status === "fulfilled" && dnsSavedResult.value
          ? dnsSavedResult.value
          : { primary: "", secondary: "" };
      
      // 解析后端实时扫描结果
      const scanned = scannedResult.status === "fulfilled" ? scannedResult.value : null;
      const scannedMap: Record<string, boolean> = {};
      let scannedDns = { primary: "", secondary: "" };
      
      if (scanned && typeof scanned === "object") {
        const s = scanned as Record<string, unknown>;
        scannedMap["tcp_congestion_optimized"] = !!s.tcp_congestion_optimized;
        scannedMap["chimney_offload"] = !!s.chimney_offload;
        scannedMap["nagle_optimized"] = !!s.nagle_optimized;
        scannedMap["adapter_power_saving_off"] = !!s.adapter_power_saving_off;
        scannedDns = {
          primary: String(s.dns_primary ?? ""),
          secondary: String(s.dns_secondary ?? ""),
        };
      }
      
      // 确保 loading 至少显示 600ms
      const remaining = Math.max(0, 600 - (Date.now() - startTime));
      if (remaining > 0) {
        await new Promise((r) => setTimeout(r, remaining));
      }
      if (cancelled) return;
      
      // 一次性原子设置所有状态
      setSavedStates(saved);
      setScannedStates(scannedMap);
      // DNS: 优先使用保存的手动设置，否则用扫描结果
      setCurrentDns(savedDns.primary ? savedDns : scannedDns);
      setIsInitialScanning(false);
    }
    init();
    return () => {
      cancelled = true;
    };
  }, []);

  const persistStates = useCallback(async (states: Record<string, boolean>) => {
    try {
      await store.set(STORE_KEY, states);
      await store.save();
    } catch {}
  }, []);

  const persistDns = useCallback(async (dns: { primary: string; secondary: string }) => {
    try {
      await store.set(DNS_STORE_KEY, dns);
      await store.save();
    } catch {}
  }, []);

  // 获取公网 IPv4 地址
  const fetchPublicIp = useCallback(
    async (manual = false) => {
      setIsLoadingIp(true);
      setIpLoadFailed(false);
      try {
        const ip = await invoke<string>("get_public_ip");
        setPublicIp(ip);
        if (manual) {
          toast({
            title: t("networkOptimize.publicIp.updated"),
            status: "success",
            duration: 2000,
            isClosable: true,
          });
        }
      } catch {
        setIpLoadFailed(true);
        if (manual) {
          toast({
            title: t("networkOptimize.publicIp.fetchFailed"),
            status: "error",
            duration: 3000,
            isClosable: true,
          });
        }
      } finally {
        setIsLoadingIp(false);
      }
    },
    [toast, t],
  );

  // 复制公网 IP 到剪贴板
  const copyPublicIp = useCallback(async () => {
    if (!publicIp) return;
    try {
      await navigator.clipboard.writeText(publicIp);
      toast({
        title: t("networkOptimize.publicIp.copied"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch {
      toast({
        title: t("networkOptimize.publicIp.copyFailed"),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  }, [publicIp, toast, t]);

  // 页面加载时自动查询公网 IP
  useEffect(() => {
    fetchPublicIp();
  }, [fetchPublicIp]);

  // 重新扫描（不覆盖用户手动操作的状态）
  const doRescan = useCallback(async () => {
    setIsRescanning(true);
    try {
      const scanned = await invoke("check_network_tweak_states").catch(() => null);
      const scannedMap: Record<string, boolean> = {};
      let scannedDns = { primary: "", secondary: "" };
      if (scanned && typeof scanned === "object") {
        const s = scanned as Record<string, unknown>;
        scannedMap["tcp_congestion_optimized"] = !!s.tcp_congestion_optimized;
        scannedMap["chimney_offload"] = !!s.chimney_offload;
        scannedMap["nagle_optimized"] = !!s.nagle_optimized;
        scannedMap["adapter_power_saving_off"] = !!s.adapter_power_saving_off;
        scannedDns = {
          primary: String(s.dns_primary ?? ""),
          secondary: String(s.dns_secondary ?? ""),
        };
      }
      setScannedStates(scannedMap);
      // DNS: 如果用户已手动设置 DNS，不覆盖
      if (currentDns.primary) {
        // 保持用户的手动 DNS 设置
      } else {
        setCurrentDns(scannedDns);
      }
      toast({
        title: t("networkOptimize.rescanComplete"),
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.scanError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsRescanning(false);
    }
  }, [toast, t, currentDns.primary]);

  const getItemState = useCallback(
    (item: NetworkOptimizerItem): boolean => {
      if (savedStates[item.id] !== undefined) return savedStates[item.id];
      const scanned = scannedStates[item.stateKey];
      if (scanned !== undefined) return scanned;
      return false;
    },
    [scannedStates, savedStates],
  );

  // 切换单个网络优化项（乐观更新：先切 UI 再后台执行，失败回滚，保证开关流畅）
  const toggleItem = useCallback(
    async (item: NetworkOptimizerItem, enable: boolean) => {
      const cmd = enable ? item.enableCmd : item.disableCmd;
      setTogglingItems((prev) => new Set(prev).add(item.id));
      const prevVal = savedStates[item.id];
      // 乐观更新开关状态，让过渡动画立即播放
      const optimistic = { ...savedStates, [item.id]: enable };
      setSavedStates(optimistic);
      setScannedStates((prev) => ({ ...prev, [item.stateKey]: enable }));
      try {
        await invoke(cmd);
        persistStates(optimistic);
        toast({
          title: enable
            ? t("networkOptimize.optimized")
            : t("networkOptimize.reverted"),
          description: t(item.titleKey),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      } catch (err) {
        // 失败时回滚开关状态
        const rollback = { ...savedStates, [item.id]: prevVal };
        setSavedStates(rollback);
        setScannedStates((prev) => ({ ...prev, [item.stateKey]: prevVal }));
        toast({
          title: t("networkOptimize.operationError"),
          description: String(err),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      } finally {
        setTogglingItems((prev) => {
          const next = new Set(prev);
          next.delete(item.id);
          return next;
        });
      }
    },
    [savedStates, persistStates, toast, t],
  );

  // 批量优化
  const handleBatchEnable = useCallback(async () => {
    setIsBatchOptimizing(true);
    try {
      await invoke("batch_network_enable");
      const newSaved: Record<string, boolean> = {};
      const newScanned: Record<string, boolean> = {};
      for (const item of networkOptimizerItems) {
        newSaved[item.id] = true;
        newScanned[item.stateKey] = true;
      }
      setSavedStates(newSaved);
      setScannedStates(newScanned);
      persistStates(newSaved);
      toast({
        title: t("networkOptimize.batchOptimized"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.batchError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsBatchOptimizing(false);
    }
  }, [persistStates, toast, t]);

  // 批量恢复
  const handleBatchDisable = useCallback(async () => {
    setIsBatchOptimizing(true);
    try {
      await invoke("batch_network_disable");
      const newScanned: Record<string, boolean> = {};
      for (const item of networkOptimizerItems) {
        newScanned[item.stateKey] = false;
      }
      setSavedStates({});
      setScannedStates(newScanned);
      persistStates({});
      toast({
        title: t("networkOptimize.batchReverted"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.batchError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsBatchOptimizing(false);
    }
  }, [persistStates, toast, t]);

  // 应用 DNS
  const applyDns = useCallback(
    async (primary: string, secondary: string) => {
      try {
        await invoke("set_dns_servers", { dnsPrimary: primary, dnsSecondary: secondary });
        const newDns = { primary, secondary };
        setCurrentDns(newDns);
        persistDns(newDns);
        toast({
          title: t("networkOptimize.dnsApplied"),
          description: `${primary}${secondary ? " / " + secondary : ""}`,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
      } catch (err) {
        toast({
          title: t("networkOptimize.applyError"),
          description: String(err),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
    },
    [toast, t, persistDns],
  );

  // 应用预设 DNS
  const handleApplyPreset = useCallback(
    async (preset: DnsPreset) => {
      setApplyingDnsId(preset.id);
      try {
        await applyDns(preset.primary, preset.secondary);
      } finally {
        setApplyingDnsId(null);
      }
    },
    [applyDns],
  );

  // 恢复自动获取 DNS
  const handleRestoreDns = useCallback(async () => {
    setIsRestoringDns(true);
    try {
      await invoke("restore_dns_servers");
      const emptyDns = { primary: "", secondary: "" };
      setCurrentDns(emptyDns);
      persistDns(emptyDns);
      toast({
        title: t("networkOptimize.dnsRestored"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.applyError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsRestoringDns(false);
    }
  }, [toast, t, persistDns]);

  // 清理 DNS 缓存
  const handleClearDnsCache = useCallback(async () => {
    setIsClearingDns(true);
    try {
      await invoke("clear_dns_cache");
      toast({
        title: t("networkOptimize.dnsCacheCleared"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.clearDnsError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsClearingDns(false);
    }
  }, [toast, t]);

  // 重置网络（Winsock + TCP/IP）
  const handleResetNetwork = useCallback(async () => {
    setIsResettingNetwork(true);
    try {
      await invoke("reset_network");
      toast({
        title: t("networkOptimize.resetNetwork.done"),
        status: "success",
        duration: 4000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.resetNetwork.error"),
        description: String(err),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setIsResettingNetwork(false);
    }
  }, [toast, t]);

  // 修复 DHCP：将启用网卡 IP/DNS 恢复为自动获取并重新获取 IP
  const handleFixDhcp = useCallback(async () => {
    setIsFixingDhcp(true);
    try {
      await invoke("fix_dhcp");
      toast({
        title: t("networkOptimize.fixDhcp.done"),
        status: "success",
        duration: 4000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("networkOptimize.fixDhcp.error"),
        description: String(err),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setIsFixingDhcp(false);
    }
  }, [toast, t]);

  // 应用自定义 DNS
  const handleApplyCustomDns = useCallback(async () => {
    if (!customPrimary.trim()) {
      toast({
        title: t("networkOptimize.dnsRequired"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    setIsApplyingCustomDns(true);
    try {
      await applyDns(customPrimary.trim(), customSecondary.trim());
    } finally {
      setIsApplyingCustomDns(false);
    }
  }, [customPrimary, customSecondary, applyDns, toast, t]);

  // Scanning state
  if (isInitialScanning) {
    return (
      <Box pt={8}>
        {liquidGlassEnabled ? (
          <LiquidGlassCard w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
            <Flex w="full" minH="360px" align="center" justify="center" direction="column" gap={4}>
              <Spinner size="xl" color={activeColor} thickness="3px" />
              <Text color={subTextColor} fontSize="sm">
                {t("networkOptimize.scanning")}
              </Text>
            </Flex>
          </LiquidGlassCard>
        ) : (
          <Box bg={cardBg} borderRadius="xl" borderWidth="1px" borderColor={cardBorder} w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
            <Flex w="full" minH="360px" align="center" justify="center" direction="column" gap={4}>
              <Spinner size="xl" color={activeColor} thickness="3px" />
              <Text color={subTextColor} fontSize="sm">
                {t("networkOptimize.scanning")}
              </Text>
            </Flex>
          </Box>
        )}
      </Box>
    );
  }

  const content = (
    <VStack align="start" spacing={6}>
      {/* 标题 */}
      <Flex
        w="full"
        justify="space-between"
        align={{ base: "start", md: "center" }}
        direction={{ base: "column", md: "row" }}
        gap={3}
      >
        <HStack spacing={3}>
          <IconButton
            aria-label={t("builtinTools.back")}
            icon={<ArrowLeft size={20} />}
            variant="ghost"
            onClick={() => navigate("/optimize")}
            color={headingColor}
          />
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            {t("networkOptimize.pageTitle")}
          </Heading>
        </HStack>
        <HStack spacing={2}>
          <Button
            size="sm"
            leftIcon={<RefreshCw size={14} />}
            onClick={doRescan}
            isLoading={isRescanning}
            variant="ghost"
            color={subTextColor}
          >
            {t("networkOptimize.rescan")}
          </Button>
          <Button
            size="sm"
            onClick={handleBatchEnable}
            isLoading={isBatchOptimizing}
            loadingText={t("networkOptimize.optimizing")}
            bg={activeColor}
            color={contrastText}
            _hover={{ opacity: 0.9 }}
            _active={{ transform: "scale(0.97)" }}
          >
            {t("networkOptimize.batch.enable")}
          </Button>
          <Button
            size="sm"
            onClick={handleBatchDisable}
            isLoading={isBatchOptimizing}
            loadingText={t("networkOptimize.optimizing")}
            variant="outline"
            sx={{
              borderColor: activeColor,
              color: activeColor,
              _hover: { bg: hoverBg },
            }}
          >
            {t("networkOptimize.batch.disable")}
          </Button>
        </HStack>
      </Flex>

      {/* Section 0: 公网 IP */}
      <Box w="full">
        <Heading
          as="h3"
          fontSize="md"
          fontWeight="bold"
          color={headingColor}
          mb={3}
          position="relative"
          pl={3}
          sx={{
            "&::before": {
              content: '""',
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "3px",
              height: "16px",
              borderRadius: "full",
              bg: activeColor,
            },
          }}
        >
          {t("networkOptimize.publicIp.title")}
        </Heading>
        {(() => {
          const ipCardContent = (
            <HStack justify="space-between" align="center" gap={3} flexWrap="wrap">
              <VStack align="start" spacing={1} flex={1} minW={0}>
                <Text fontSize="xs" color={subTextColor}>
                  {t("networkOptimize.publicIp.label")}
                </Text>
                {isLoadingIp ? (
                  <HStack spacing={2}>
                    <Spinner size="sm" color={activeColor} thickness="2px" />
                    <Text fontSize="sm" color={subTextColor}>
                      {t("networkOptimize.publicIp.loading")}
                    </Text>
                  </HStack>
                ) : ipLoadFailed ? (
                  <Text fontSize="sm" fontWeight="bold" color="red.400">
                    {t("networkOptimize.publicIp.failed")}
                  </Text>
                ) : (
                  <Text
                    fontSize="2xl"
                    fontWeight="bold"
                    color={headingColor}
                    fontFamily="'Consolas', 'Courier New', monospace"
                    wordBreak="break-all"
                  >
                    {publicIp}
                  </Text>
                )}
              </VStack>
              <HStack spacing={2}>
                <Button
                  size="sm"
                  onClick={copyPublicIp}
                  isDisabled={!publicIp || ipLoadFailed}
                  variant="outline"
                  sx={{
                    borderColor: activeColor,
                    color: activeColor,
                    _hover: { bg: hoverBg },
                  }}
                >
                  {t("networkOptimize.publicIp.copy")}
                </Button>
                <IconButton
                  aria-label={t("networkOptimize.publicIp.refresh")}
                  icon={<RefreshCw size={14} />}
                  size="sm"
                  variant="ghost"
                  onClick={() => fetchPublicIp(true)}
                  isLoading={isLoadingIp}
                  color={subTextColor}
                />
              </HStack>
            </HStack>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4}>{ipCardContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4}>
              {ipCardContent}
            </Box>
          );
        })()}
      </Box>

      {/* Section 1: DNS 设置 */}
      <Box w="full">
        <Heading
          as="h3"
          fontSize="md"
          fontWeight="bold"
          color={headingColor}
          mb={3}
          position="relative"
          pl={3}
          sx={{
            "&::before": {
              content: '""',
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "3px",
              height: "16px",
              borderRadius: "full",
              bg: activeColor,
            },
          }}
        >
          {t("networkOptimize.dns.title")}
        </Heading>
        {/* 当前 DNS 状态 */}
        {(() => {
          const dnsContent = (
            <HStack justify="space-between" align="center">
              <VStack align="start" spacing={1}>
                <Text fontSize="xs" color={subTextColor}>
                  {t("networkOptimize.currentDns")}
                </Text>
                {currentDns.primary ? (
                  <Text fontSize="sm" fontWeight="bold" color={headingColor}>
                    {currentDns.primary}
                    {currentDns.secondary ? ` / ${currentDns.secondary}` : ""}
                  </Text>
                ) : (
                  <Text fontSize="sm" color={subTextColor}>
                    {t("networkOptimize.noDnsConfig")}
                  </Text>
                )}
              </VStack>
              <Button
                size="sm"
                onClick={handleRestoreDns}
                isLoading={isRestoringDns}
                variant="outline"
                sx={{
                  borderColor: activeColor,
                  color: activeColor,
                  _hover: { bg: hoverBg },
                }}
              >
                {t("networkOptimize.dns.restore")}
              </Button>
            </HStack>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4} mb={3}>{dnsContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4} mb={3}>
              {dnsContent}
            </Box>
          );
        })()}

        {/* DNS 预设列表 */}
        <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={3} mb={3}>
          {dnsPresets.map((preset) => (
            <DnsCard
              key={preset.id}
              preset={preset}
              currentDns={currentDns}
              applyingId={applyingDnsId}
              latency={dnsLatency[preset.id]}
              onApply={handleApplyPreset}
              headingColor={headingColor}
              subTextColor={subTextColor}
              activeColor={activeColor}
              contrastText={contrastText}
              hoverBg={hoverBg}
              cardBg={cardBg}
              cardBorder={cardBorder}
              liquidGlassEnabled={liquidGlassEnabled}
              t={t}
            />
          ))}
        </SimpleGrid>

        {/* 自定义 DNS 输入 */}
        {(() => {
          const customDnsContent = (
            <>
              <Text fontSize="xs" fontWeight="bold" color={subTextColor} mb={2}>
                {t("networkOptimize.dns.customLabel")}
              </Text>
              <HStack spacing={2} flexWrap="wrap">
                <Input
                  placeholder={t("networkOptimize.dns.primary")}
                  value={customPrimary}
                  onChange={(e) => setCustomPrimary(e.target.value)}
                  size="sm"
                  flex={1}
                  minW="140px"
                  focusBorderColor={activeColor}
                />
                <Input
                  placeholder={t("networkOptimize.dns.secondary")}
                  value={customSecondary}
                  onChange={(e) => setCustomSecondary(e.target.value)}
                  size="sm"
                  flex={1}
                  minW="140px"
                  focusBorderColor={activeColor}
                />
                <Button
                  size="sm"
                  onClick={handleApplyCustomDns}
                  isLoading={isApplyingCustomDns}
                  bg={activeColor}
                  color={contrastText}
                  _hover={{ opacity: 0.9 }}
                  _active={{ transform: "scale(0.97)" }}
                >
                  {t("networkOptimize.dns.apply")}
                </Button>
              </HStack>
            </>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4}>{customDnsContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4}>
              {customDnsContent}
            </Box>
          );
        })()}

        {/* 清理 DNS 缓存 */}
        {(() => {
          const clearDnsContent = (
            <HStack justify="space-between" align="center" gap={3} flexWrap="wrap">
              <VStack align="start" spacing={1} flex={1} minW={0}>
                <Text fontSize="sm" fontWeight="bold" color={headingColor}>
                  {t("networkOptimize.dnsCache.title")}
                </Text>
                <Text fontSize="xs" color={subTextColor}>
                  {t("networkOptimize.dnsCache.description")}
                </Text>
              </VStack>
              <Button
                size="sm"
                onClick={handleClearDnsCache}
                isLoading={isClearingDns}
                loadingText={t("networkOptimize.dnsCache.clearing")}
                bg={activeColor}
                color={contrastText}
                _hover={{ opacity: 0.9 }}
                _active={{ transform: "scale(0.97)" }}
                flexShrink={0}
              >
                {t("networkOptimize.dnsCache.clear")}
              </Button>
            </HStack>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4} mt={3}>{clearDnsContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4} mt={3}>
              {clearDnsContent}
            </Box>
          );
        })()}
      </Box>

      {/* Section 2: 网络优化项 */}
      <Box w="full">
        <Heading
          as="h3"
          fontSize="md"
          fontWeight="bold"
          color={headingColor}
          mb={3}
          position="relative"
          pl={3}
          sx={{
            "&::before": {
              content: '""',
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "3px",
              height: "16px",
              borderRadius: "full",
              bg: activeColor,
            },
          }}
        >
          {t("networkOptimize.batch.title")}
        </Heading>
        <SimpleGrid columns={{ base: 1, md: 2 }} spacing={3}>
          {networkOptimizerItems.map((item) => (
            <OptimizeCard
              key={item.id}
              item={item}
              state={getItemState(item)}
              isToggling={togglingItems.has(item.id)}
              onToggle={toggleItem}
              headingColor={headingColor}
              subTextColor={subTextColor}
              activeColor={activeColor}
              cardBg={cardBg}
              cardBorder={cardBorder}
              liquidGlassEnabled={liquidGlassEnabled}
              t={t}
            />
          ))}
        </SimpleGrid>
      </Box>

      {/* Section 3: 网络重置 */}
      <Box w="full">
        <Heading
          as="h3"
          fontSize="md"
          fontWeight="bold"
          color={headingColor}
          mb={3}
          position="relative"
          pl={3}
          sx={{
            "&::before": {
              content: '""',
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "3px",
              height: "16px",
              borderRadius: "full",
              bg: activeColor,
            },
          }}
        >
          {t("networkOptimize.resetNetwork.title")}
        </Heading>
        {(() => {
          const resetContent = (
            <HStack justify="space-between" align="center" gap={3} flexWrap="wrap">
              <VStack align="start" spacing={1} flex={1} minW={0}>
                <Text fontSize="sm" fontWeight="bold" color={headingColor}>
                  {t("networkOptimize.resetNetwork.title")}
                </Text>
                <Text fontSize="xs" color={subTextColor}>
                  {t("networkOptimize.resetNetwork.description")}
                </Text>
              </VStack>
              <Button
                size="sm"
                onClick={handleResetNetwork}
                isLoading={isResettingNetwork}
                loadingText={t("networkOptimize.resetNetwork.resetting")}
                bg={activeColor}
                color={contrastText}
                _hover={{ opacity: 0.9 }}
                _active={{ transform: "scale(0.97)" }}
                flexShrink={0}
              >
                {t("networkOptimize.resetNetwork.reset")}
              </Button>
            </HStack>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4}>{resetContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4}>
              {resetContent}
            </Box>
          );
        })()}

        {/* 修复 DHCP */}
        {(() => {
          const fixDhcpContent = (
            <HStack justify="space-between" align="center" gap={3} flexWrap="wrap">
              <VStack align="start" spacing={1} flex={1} minW={0}>
                <Text fontSize="sm" fontWeight="bold" color={headingColor}>
                  {t("networkOptimize.fixDhcp.title")}
                </Text>
                <Text fontSize="xs" color={subTextColor}>
                  {t("networkOptimize.fixDhcp.description")}
                </Text>
              </VStack>
              <Button
                size="sm"
                onClick={handleFixDhcp}
                isLoading={isFixingDhcp}
                loadingText={t("networkOptimize.fixDhcp.fixing")}
                bg={activeColor}
                color={contrastText}
                _hover={{ opacity: 0.9 }}
                _active={{ transform: "scale(0.97)" }}
                flexShrink={0}
              >
                {t("networkOptimize.fixDhcp.fix")}
              </Button>
            </HStack>
          );
          if (liquidGlassEnabled) {
            return <LiquidGlassCard w="full" p={4} mt={3}>{fixDhcpContent}</LiquidGlassCard>;
          }
          return (
            <Box w="full" bg={cardBg} borderRadius="xl" border="1px solid" borderColor={cardBorder} p={4} mt={3}>
              {fixDhcpContent}
            </Box>
          );
        })()}
      </Box>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <Box pt={8}>
        <LiquidGlassCard w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
          {content}
        </LiquidGlassCard>
      </Box>
    );
  }

  return (
    <Box pt={8}>
      <Box
        bg={cardBg}
        borderRadius="xl"
        borderWidth="1px"
        borderColor={cardBorder}
        w="full"
        boxShadow="2xl"
        overflow="hidden"
        position="relative"
        p={6}
      >
        {content}
      </Box>
    </Box>
  );
}
