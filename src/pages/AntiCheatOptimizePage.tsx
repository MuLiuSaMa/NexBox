import {
  Box,
  Text,
  VStack,
  HStack,
  Button,
  SimpleGrid,
  useColorModeValue,
  Badge,
  Switch,
  Tooltip,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Gauge, Cpu, Zap, Settings2, Shield, Info } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";

interface AcGroup {
  key: string;
  name: string;
  vendor: string;
  games: string;
  processes: string[];
}

interface OptionState {
  running: boolean;
  message: string;
  foundCount: number;
  modifiedCount: number;
}

interface AcAutoDetectStatus {
  enabled: boolean;
  is_running: boolean;
  last_check: string | null;
  total_optimized: number;
  currently_optimized: string[];
}

function formatRelativeTime(isoString: string): string {
  const date = new Date(isoString);
  const now = new Date();
  const diffSecs = Math.floor((now.getTime() - date.getTime()) / 1000);
  if (diffSecs < 60) return `${diffSecs}秒前`;
  const diffMins = Math.floor(diffSecs / 60);
  if (diffMins < 60) return `${diffMins}分钟前`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}小时前`;
  return `${Math.floor(diffHours / 24)}天前`;
}

type Field = "priority" | "affinity" | "efficiency" | "registry";

const EMPTY: OptionState = { running: false, message: "", foundCount: 0, modifiedCount: 0 };

/// 紧凑动作项：左图标+标题+说明叹号，右「应用/恢复」小按钮
function ActionItem({
  icon,
  title,
  description,
  state,
  needsAdmin,
  onApply,
  onRestore,
  badge,
}: {
  icon: React.ReactNode;
  title: string;
  description?: string;
  state?: OptionState;
  needsAdmin?: boolean;
  onApply: () => void;
  onRestore?: () => void;
  badge?: string;
}) {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const rowBg = useColorModeValue("gray.50", "#171717");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const st = state ?? EMPTY;
  const applied = st.modifiedCount > 0;
  const hasProcess = st.foundCount > 0;
  const blocked = hasProcess && !applied;

  return (
    <HStack
      justify="space-between"
      spacing={2}
      p={2}
      borderRadius="lg"
      bg={rowBg}
      border="1px solid"
      borderColor={borderColor}
    >
      <HStack spacing={2} minW="0">
        <Box color={getActiveColor()} flexShrink={0}>{icon}</Box>
        <HStack spacing={1} minW="0">
          <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
            {title}
          </Text>
          {description && (
            <Tooltip label={description} hasArrow placement="top">
              <Box as="span" color="gray.400" cursor="help" flexShrink={0} display="inline-flex">
                <Info size={13} />
              </Box>
            </Tooltip>
          )}
          {badge && (
            <Badge colorScheme="purple" variant="subtle" fontSize="2xs" px={1.5} borderRadius="full" whiteSpace="nowrap">
              {badge}
            </Badge>
          )}
        </HStack>
      </HStack>
      <HStack spacing={1} flexShrink={0}>
        {blocked && (
          <Tooltip label={t("optimization.aceOptimize.status.needsAdmin")}>
            <Box w={2} h={2} borderRadius="full" bg="orange.400" />
          </Tooltip>
        )}
        {onRestore && (
          <Button size="xs" variant="ghost" colorScheme="red" onClick={onRestore} isLoading={st.running} loadingText="" px={2}>
            {t("optimization.aceOptimize.restore")}
          </Button>
        )}
        <Button
          size="xs"
          bg={applied ? getActiveColor() : undefined}
          color={applied ? "white" : getActiveColor()}
          borderColor={!applied ? getActiveColor() : undefined}
          variant={!applied ? "outline" : undefined}
          _hover={applied ? { bg: getActiveColor(), opacity: 0.9 } : undefined}
          onClick={onApply}
          isLoading={st.running}
          loadingText=""
          px={3}
          borderRadius="md"
        >
          {applied ? t("optimization.aceOptimize.applied") : t("optimization.aceOptimize.apply")}
        </Button>
      </HStack>
    </HStack>
  );
}

export default function AntiCheatOptimizePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("shield");
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const iconBg = useColorModeValue("white", "#222222");
  const { getActiveColor } = useThemeColor();
  const { liquidGlassEnabled } = useBackground();

  const [groups, setGroups] = useState<AcGroup[]>([]);
  const [states, setStates] = useState<Record<string, Record<Field, OptionState>>>({});
  const [autoDetect, setAutoDetect] = useState<Record<string, AcAutoDetectStatus | null>>({});
  const [autoDetectLoading, setAutoDetectLoading] = useState<Record<string, boolean>>({});
  const isMountedRef = useRef(true);

  const setOpt = (key: string, field: Field, patch: Partial<OptionState>) => {
    setStates((prev) => {
      const current = prev[key] ?? { priority: EMPTY, affinity: EMPTY, efficiency: EMPTY, registry: EMPTY };
      return { ...prev, [key]: { ...current, [field]: { ...current[field], ...patch } } };
    });
  };

  useEffect(() => {
    isMountedRef.current = true;
    invoke<AcGroup[]>("anticheat_get_groups")
      .then((gs) => {
        if (!isMountedRef.current) return;
        setGroups(gs);
        const init: Record<string, Record<Field, OptionState>> = {};
        gs.forEach((g) => {
          init[g.key] = { priority: EMPTY, affinity: EMPTY, efficiency: EMPTY, registry: EMPTY };
        });
        setStates(init);
      })
      .catch((e) => console.error("Failed to load anti-cheat groups:", e));
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  // 轮询每个分组的自动检测状态（仅在状态实际变化时更新，避免无谓重渲染导致滚动卡顿）
  useEffect(() => {
    if (groups.length === 0) return;
    const poll = async () => {
      const updated: Record<string, AcAutoDetectStatus> = {};
      for (const g of groups) {
        try {
          const status = await invoke<AcAutoDetectStatus>("anticheat_get_auto_detect_status", { groupKey: g.key });
          updated[g.key] = status;
        } catch (e) {
          console.error(`Failed to load status for ${g.key}:`, e);
        }
      }
      if (!isMountedRef.current) return;
      setAutoDetect((prev) => {
        let changed = false;
        for (const [k, v] of Object.entries(updated)) {
          if (JSON.stringify(prev[k]) !== JSON.stringify(v)) {
            changed = true;
            break;
          }
        }
        return changed ? { ...prev, ...updated } : prev;
      });
    };
    poll();
    const id = window.setInterval(poll, 3000);
    return () => window.clearInterval(id);
  }, [groups]);

  const runAction = async (g: AcGroup, field: Field, cmd: string) => {
    setOpt(g.key, field, { running: true });
    try {
      const result = await invoke<{ success: boolean; message: string; count?: number; found_count?: number }>(cmd, { groupKey: g.key });
      setOpt(g.key, field, { running: false, message: result.message, foundCount: result.found_count ?? 0, modifiedCount: result.count ?? 0 });
      toast({ title: result.message, status: result.success ? "success" : "info", duration: 2200 });
    } catch (e: any) {
      setOpt(g.key, field, { running: false, message: String(e), foundCount: 0, modifiedCount: 0 });
      toast({ title: String(e), status: "error", duration: 2200 });
    }
  };

  const toggleAutoDetect = async (g: AcGroup, enabled: boolean) => {
    setAutoDetectLoading((p) => ({ ...p, [g.key]: true }));
    try {
      await invoke("anticheat_set_auto_detect", { groupKey: g.key, enabled });
      setAutoDetect((p) => ({ ...p, [g.key]: { ...(p[g.key] ?? { enabled: false, is_running: false, last_check: null, total_optimized: 0, currently_optimized: [] }), enabled } }));
      toast({ title: enabled ? "已启用自动检测" : "已禁用自动检测", status: "success", duration: 2200 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2200 });
    } finally {
      setAutoDetectLoading((p) => ({ ...p, [g.key]: false }));
    }
  };

  const cardContent = (g: AcGroup) => {
    const st = states[g.key];
    const ad = autoDetect[g.key];
    return (
      <VStack align="stretch" spacing={3}>
        <HStack justify="space-between" spacing={2}>
          <HStack spacing={3} minW="0">
            <Box
              w={10} h={10}
              borderRadius="lg"
              bg={iconBg}
              border="1px solid"
              borderColor={borderColor}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={g.key === "ace" ? "#DD6B20" : getActiveColor()}
              flexShrink={0}
            >
              <Shield size={18} />
            </Box>
            <VStack align="flex-start" spacing={0} minW="0">
              <Text fontWeight="bold" fontSize="md" color={headingColor} noOfLines={1}>
                {g.name} {g.vendor && <Text as="span" fontSize="xs" color="gray.500" fontWeight="normal">· {g.vendor}</Text>}
              </Text>
              <Text fontSize="xs" color={subTextColor} noOfLines={1}>{g.games}</Text>
            </VStack>
          </HStack>
          <Tooltip label="自动检测并优化">
            <HStack spacing={2}>
              <Text fontSize="xs" color={subTextColor} whiteSpace="nowrap">自动优化</Text>
              <Switch
                isChecked={ad?.enabled ?? false}
                onChange={(e) => toggleAutoDetect(g, e.target.checked)}
                isDisabled={autoDetectLoading[g.key]}
                size="sm"
                sx={{
                  "& > span": { bg: ad?.enabled ? getActiveColor() : useColorModeValue("gray.200", "gray.600") },
                  "& > span > span": { bg: "white" },
                }}
                aria-label="auto detect"
              />
            </HStack>
          </Tooltip>
        </HStack>

        <VStack align="stretch" spacing={2}>
          <ActionItem
            icon={<Gauge size={16} />}
            title={t("optimization.aceOptimize.aceLimit.title")}
            description={`将 ${g.name} 进程优先级降为「低」，减少 CPU 抢占，释放更多资源给游戏`}
            state={st?.priority}
            needsAdmin
            onApply={() => runAction(g, "priority", "anticheat_limit_priority")}
          />
          <ActionItem
            icon={<Cpu size={16} />}
            title={t("optimization.aceOptimize.aceLimit.affinityTitle")}
            description={`限制 ${g.name} 进程仅使用 E 核（小核）运行，避免占用游戏关键的大核 CPU 资源`}
            state={st?.affinity}
            needsAdmin
            onApply={() => runAction(g, "affinity", "anticheat_restrict_affinity")}
          />
          <ActionItem
            icon={<Zap size={16} />}
            title={t("optimization.aceOptimize.aceEfficiency.title")}
            description={`对 ${g.name} 进程启用后台节能调度：低于正常优先级 + 后台 I/O 节流 + 低内存优先级，大幅减少后台资源占用`}
            state={st?.efficiency}
            needsAdmin
            onApply={() => runAction(g, "efficiency", "anticheat_set_efficiency")}
          />
          <ActionItem
            icon={<Settings2 size={16} />}
            title={t("optimization.aceOptimize.aceRegistry.title")}
            description={`通过 IFEO 注册表在进程启动时强制限制 ${g.name} 的 CPU/IO 优先级，进程无法自行改回；需重启相关进程后生效，有一定风险`}
            state={st?.registry}
            badge={t("optimization.aceOptimize.aceRegistry.badge")}
            onApply={() => runAction(g, "registry", "anticheat_apply_registry")}
            onRestore={() => runAction(g, "registry", "anticheat_restore_registry")}
          />
        </VStack>

        {ad && (ad.currently_optimized.length > 0 || ad.last_check) && (
          <HStack spacing={2} fontSize="2xs" color={subTextColor} wrap="wrap">
            {ad.currently_optimized.length > 0 && (
              <Badge colorScheme="green" variant="subtle" fontSize="2xs">
                {t("optimization.aceOptimize.autoDetect.optimizing", { count: ad.currently_optimized.length })}
              </Badge>
            )}
            <Text>
              {t("optimization.aceOptimize.autoDetect.lastCheck")}: {ad.last_check ? formatRelativeTime(ad.last_check) : t("optimization.aceOptimize.autoDetect.never")}
            </Text>
          </HStack>
        )}
      </VStack>
    );
  };

  return (
    <Box pt={8} pb={8}>
      <PageHeader title={t("optimization.aceOptimize.title")} mb={6} />

        <Box w="100%" maxW="1500px" mx="auto" px={{ base: 2, md: 4 }}>
        {groups.length === 0 && (
          <Text color={subTextColor} fontSize="sm">加载中...</Text>
        )}

        <SimpleGrid columns={{ base: 1, md: 2, xl: 3, "2xl": 4 }} spacing={5}>
          {groups.map((g) => {
            const content = cardContent(g);
            return liquidGlassEnabled ? (
              <LiquidGlassCard key={g.key} p={5}>{content}</LiquidGlassCard>
            ) : (
              <Box key={g.key} bg={cardBg} borderRadius="3xl" p={5} border="1px solid" borderColor={borderColor}>
                {content}
              </Box>
            );
          })}
        </SimpleGrid>
      </Box>
    </Box>
  );
}