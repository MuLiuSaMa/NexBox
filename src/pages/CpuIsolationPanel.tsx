import {
  Alert,
  AlertDescription,
  AlertIcon,
  Badge,
  Box,
  Button,
  Checkbox,
  Divider,
  HStack,
  IconButton,
  Input,
  SimpleGrid,
  Spinner,
  Switch,
  Text,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import {
  AlertTriangle,
  Cpu,
  Layers,
  Power,
  RefreshCw,
  Save,
  Search,
  ShieldCheck,
  ShieldOff,
  Trash2,
  Zap,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

// ── Types ──────────────────────────────────────────────────

interface PhysicalCore {
  core_index: number;
  core_type: "Performance" | "Efficiency" | "Unknown";
  logical_processors: number[];
  affinity_mask: number;
}

interface CpuTopology {
  cpu_name: string;
  total_physical_cores: number;
  total_logical_processors: number;
  has_hybrid_architecture: boolean;
  physical_cores: PhysicalCore[];
  system_affinity_mask: number;
}

interface ProcessInfo {
  pid: number;
  name: string;
  memory_mb: number;
  cpu_usage: number;
  exe_path: string;
}

interface IsolationApplyResult {
  total: number;
  modified: number;
  failed: number;
  failed_processes: string[];
}

interface IsolationStateRecord {
  isolated_mask: number;
  exclude_process: string;
  modified_processes: { pid: number; name: string; original_mask: number }[];
}

interface IsolationRule {
  name: string;
  isolated_mask: number;
  exclude_process: string;
  preset: string;
  description: string;
  auto_apply: boolean;
}

type PresetType = "allPCores" | "allECores" | "allCores" | "custom";

interface CpuIsolationPanelProps {
  topology: CpuTopology | null;
  processes: ProcessInfo[];
  loadProcesses: () => Promise<void>;
  icons: Record<string, string>;
}

// ── Component ──────────────────────────────────────────────

export default function CpuIsolationPanel({
  topology,
  processes,
  loadProcesses,
  icons,
}: CpuIsolationPanelProps) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("cpu");
  const { getActiveColor } = useThemeColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const listItemHoverBg = useColorModeValue("gray.50", "#222222");
  const restoreBg = useColorModeValue("#FFFFFF", "#1A202C");
  const restoreColor = useColorModeValue("#1A202C", "#FFFFFF");
  const restoreBorder = useColorModeValue("gray.300", "gray.700");

  const primaryColor = getActiveColor();
  const themeColorHex = primaryColor || "#98DDD0";
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);
  const eCoreColor = "#F6AD55";
  // 隔离操作使用琥珀/红色系，与「分配」的主题色形成语义区分
  const isoColor = "#DD6B20";
  const isoColorRgba = (opacity: number) => hexToRgba(isoColor, opacity);

  const isoCheckboxSx = useMemo(
    () => ({
      ".chakra-checkbox__control": {
        borderColor: cardBorder,
        "&[data-checked]": {
          bg: isoColor,
          borderColor: isoColor,
        },
      },
      _hover: {
        ".chakra-checkbox__control:not([data-checked])": {
          borderColor: isoColor,
        },
      },
      _focusWithin: {
        ".chakra-checkbox__control": {
          boxShadow: `0 0 0 3px ${isoColorRgba(0.3)}`,
        },
      },
    }),
    [cardBorder, isoColor, isoColorRgba],
  );

  const eCoreCheckboxSx = useMemo(
    () => ({
      ".chakra-checkbox__control": {
        borderColor: cardBorder,
        "&[data-checked]": {
          bg: eCoreColor,
          borderColor: eCoreColor,
        },
      },
      _hover: {
        ".chakra-checkbox__control:not([data-checked])": {
          borderColor: eCoreColor,
        },
      },
      _focusWithin: {
        ".chakra-checkbox__control": {
          boxShadow: `0 0 0 3px ${hexToRgba(eCoreColor, 0.3)}`,
        },
      },
    }),
    [cardBorder, eCoreColor],
  );

  // ── State ────────────────────────────────────────────────

  const [isolatedCores, setIsolatedCores] = useState<Set<number>>(new Set());
  const [isolationPreset, setIsolationPreset] = useState<PresetType>("custom");
  const [excludeProcess, setExcludeProcess] = useState<ProcessInfo | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const [isolationState, setIsolationState] = useState<IsolationStateRecord | null>(null);
  const [rules, setRules] = useState<IsolationRule[]>([]);
  const [result, setResult] = useState<IsolationApplyResult | null>(null);

  const [ruleName, setRuleName] = useState("");
  const [autoApply, setAutoApply] = useState(false);

  const [isLoadingState, setIsLoadingState] = useState(true);
  const [isApplying, setIsApplying] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const [isSavingRule, setIsSavingRule] = useState(false);

  // ── Data Loading ────────────────────────────────────────

  const loadIsolationState = useCallback(async () => {
    try {
      const state = await invoke<IsolationStateRecord | null>("get_isolation_state");
      setIsolationState(state);
    } catch {
      setIsolationState(null);
    } finally {
      setIsLoadingState(false);
    }
  }, []);

  const loadIsolationRules = useCallback(async () => {
    try {
      const data = await invoke<IsolationRule[]>("get_isolation_rules");
      setRules(data);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    loadIsolationState();
    loadIsolationRules();
  }, [loadIsolationState, loadIsolationRules]);

  // ── Core Selection ──────────────────────────────────────

  const handleToggleCore = useCallback((lp: number) => {
    setIsolatedCores((prev) => {
      const next = new Set(prev);
      if (next.has(lp)) next.delete(lp);
      else next.add(lp);
      return next;
    });
    setIsolationPreset("custom");
  }, []);

  const applyPreset = useCallback(
    (preset: PresetType) => {
      if (!topology) return;
      const next = new Set<number>();
      for (const core of topology.physical_cores) {
        const matches =
          preset === "allCores" ||
          (preset === "allPCores" && core.core_type === "Performance") ||
          (preset === "allECores" && core.core_type === "Efficiency");
        if (matches) for (const lp of core.logical_processors) next.add(lp);
      }
      setIsolatedCores(next);
      setIsolationPreset(preset);
    },
    [topology],
  );

  const handleSelectExclude = useCallback((proc: ProcessInfo) => {
    setExcludeProcess((prev) => (prev?.pid === proc.pid ? null : proc));
  }, []);

  // ── Derived ──────────────────────────────────────────────

  const systemMask = topology?.system_affinity_mask ?? 0;
  const isolatedMask = useMemo(() => {
    let mask = 0;
    for (const lp of isolatedCores) mask |= 1 << lp;
    return mask;
  }, [isolatedCores]);
  const isAllCoresSelected =
    systemMask !== 0 && (isolatedMask & systemMask) === systemMask;

  const pCores = topology?.physical_cores.filter((c) => c.core_type === "Performance") ?? [];
  const eCores = topology?.physical_cores.filter((c) => c.core_type === "Efficiency") ?? [];
  const unknownCores = topology?.physical_cores.filter((c) => c.core_type === "Unknown") ?? [];

  const filteredProcesses = (() => {
    if (!searchQuery.trim()) return processes.slice(0, 200);
    const q = searchQuery.toLowerCase();
    return processes
      .filter((p) => p.name.toLowerCase().includes(q) || String(p.pid).includes(q))
      .slice(0, 200);
  })();

  // 判断某条规则是否为当前活动隔离的来源（用于展示「生效中」与独立的「恢复」按钮）
  const isRuleActive = useCallback(
    (rule: IsolationRule) =>
      isolationState !== null &&
      isolationState.isolated_mask === rule.isolated_mask &&
      isolationState.exclude_process === rule.exclude_process,
    [isolationState],
  );

  // ── Actions ─────────────────────────────────────────────

  const handleApply = useCallback(async () => {
    if (isolatedCores.size === 0) {
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.noCoreSelected"),
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    if (isAllCoresSelected) {
      toast({
        title: t("optimization.cpuScheduler.error"),
        description: t("optimization.cpuScheduler.coreIsolation.allCoresForbidden"),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
      return;
    }
    setIsApplying(true);
    try {
      const res = await invoke<IsolationApplyResult>("apply_core_isolation", {
        isolatedMask,
        excludeProcess: excludeProcess?.name ?? "",
      });
      setResult(res);
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.applied"),
        description: t("optimization.cpuScheduler.coreIsolation.applyResult", {
          modified: res.modified,
          failed: res.failed,
        }),
        status: res.failed > 0 ? "warning" : "success",
        duration: 4000,
        isClosable: true,
      });
      await loadIsolationState();
    } catch (error) {
      toast({
        title: t("optimization.cpuScheduler.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsApplying(false);
    }
  }, [isolatedCores, isAllCoresSelected, isolatedMask, excludeProcess, t, toast, loadIsolationState]);

  const handleRestore = useCallback(async () => {
    if (!isolationState) return;
    setIsRestoring(true);
    try {
      const res = await invoke<IsolationApplyResult>("restore_core_isolation");
      setResult(res);
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.restored"),
        status: "success",
        duration: 4000,
        isClosable: true,
      });
      await loadIsolationState();
    } catch (error) {
      toast({
        title: t("optimization.cpuScheduler.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsRestoring(false);
    }
  }, [isolationState, t, toast, loadIsolationState]);

  const handleSaveRule = useCallback(async () => {
    if (!ruleName.trim()) {
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.ruleNameRequired"),
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    if (isolatedCores.size === 0) {
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.noCoreSelected"),
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    setIsSavingRule(true);
    try {
      await invoke("save_isolation_rule", {
        name: ruleName.trim(),
        isolatedMask,
        excludeProcess: excludeProcess?.name ?? "",
        preset: isolationPreset,
        description: "",
        autoApply,
      });
      toast({
        title: t("optimization.cpuScheduler.coreIsolation.ruleSaved"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
      setRuleName("");
      await loadIsolationRules();
    } catch (error) {
      toast({
        title: t("optimization.cpuScheduler.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsSavingRule(false);
    }
  }, [ruleName, isolatedCores, isolatedMask, excludeProcess, isolationPreset, autoApply, t, toast, loadIsolationRules]);

  const handleApplyRule = useCallback(
    async (rule: IsolationRule) => {
      try {
        const res = await invoke<IsolationApplyResult>("apply_isolation_rule_by_name", {
          name: rule.name,
        });
        setResult(res);
        toast({
          title: t("optimization.cpuScheduler.coreIsolation.applied"),
          description: t("optimization.cpuScheduler.coreIsolation.applyResult", {
            modified: res.modified,
            failed: res.failed,
          }),
          status: res.failed > 0 ? "warning" : "success",
          duration: 4000,
          isClosable: true,
        });
        await loadIsolationState();
      } catch (error) {
        toast({
          title: t("optimization.cpuScheduler.error"),
          description: String(error),
          status: "error",
          duration: 5000,
          isClosable: true,
        });
      }
    },
    [t, toast, loadIsolationState],
  );

  const handleDeleteRule = useCallback(
    async (rule: IsolationRule) => {
      try {
        await invoke("delete_isolation_rule", { name: rule.name });
        toast({
          title: t("optimization.cpuScheduler.coreIsolation.ruleDeleted"),
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        await loadIsolationRules();
      } catch (error) {
        toast({
          title: t("optimization.cpuScheduler.error"),
          description: String(error),
          status: "error",
          duration: 5000,
          isClosable: true,
        });
      }
    },
    [t, toast, loadIsolationRules],
  );

  // ── Render ──────────────────────────────────────────────

  const k = "optimization.cpuScheduler.coreIsolation";

  return (
    <VStack align="start" spacing={6}>
      {/* ── 状态 Alert ── */}
      {isLoadingState ? (
        <HStack justify="center" w="full" p={4}>
          <Spinner color={isoColor} />
          <Text color={subTextColor} fontSize="sm">{t("optimization.cpuScheduler.loading")}</Text>
        </HStack>
      ) : isolationState ? (
        <Alert
          status="warning"
          borderRadius="xl"
          bg={hexToRgba(isoColor, 0.12)}
          borderLeft="4px solid"
          borderColor={isoColor}
        >
          <AlertIcon as={ShieldCheck} color={isoColor} />
          <AlertDescription color={textColor} fontSize="sm">
            <Text fontWeight="700" mb={1}>
              {t(`${k}.activeState`)}
            </Text>
            <Text fontSize="xs" color={subTextColor}>
              {t(`${k}.isolatedMaskLabel`)}{" "}
              <b>0x{isolationState.isolated_mask.toString(16).toUpperCase().padStart(16, "0")}</b>
              {" · "}
              {t(`${k}.excludeProcessLabel`)}{" "}
              <b>{isolationState.exclude_process || t(`${k}.noExcludeProcess`)}</b>
              {" · "}
              {t(`${k}.modifiedCount`, { count: isolationState.modified_processes.length })}
            </Text>
          </AlertDescription>
        </Alert>
      ) : (
        <Alert status="info" borderRadius="xl" bg={themeColorRgba(0.1)} borderLeft="4px solid" borderColor={themeColorHex}>
          <AlertIcon as={ShieldOff} color={themeColorHex} />
          <AlertDescription color={textColor} fontSize="sm">
            <Text fontWeight="600" mb={1}>{t(`${k}.idleState`)}</Text>
            <Text fontSize="xs" color={subTextColor}>{t(`${k}.description`)}</Text>
          </AlertDescription>
        </Alert>
      )}

      {/* ── 第 1 步：选择游戏进程（豁免进程） ── */}
      <Box w="full">
        <HStack spacing={2} mb={1}>
          <Power size={16} color={themeColorHex} />
          <Text fontWeight="600" color={textColor} fontSize="md">
            {t(`${k}.excludeProcess`)}
          </Text>
          {excludeProcess && (
            <Badge bg={themeColorRgba(0.15)} color={themeColorHex} fontSize="xs" px={2} py={0.5} borderRadius="full">
              {excludeProcess.name}
            </Badge>
          )}
        </HStack>
        <Text color={subTextColor} fontSize="xs" mb={3}>
          {t(`${k}.excludeProcessHint`)}
        </Text>

        <VStack align="start" spacing={3} w="full">
          <HStack w="full" spacing={2}>
            <Box position="relative" flex={1}>
              <Input
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder={t("optimization.cpuScheduler.searchPlaceholder")}
                color={headingColor}
                borderColor={cardBorder}
                bg={cardBg}
                _focus={{ borderColor: isoColor }}
                pl={10}
              />
              <Box position="absolute" left={3} top="50%" transform="translateY(-50%)" color={subTextColor}>
                <Search size={16} />
              </Box>
            </Box>
            <IconButton
              aria-label="refresh processes"
              icon={<RefreshCw size={16} />}
              onClick={loadProcesses}
              color={subTextColor}
              variant="outline"
              borderColor={cardBorder}
            />
          </HStack>

          <Box
            w="full"
            maxH="220px"
            overflowY="auto"
            borderRadius="3xl"
            border="1px solid"
            borderColor={cardBorder}
            bg={cardBg}
          >
            {filteredProcesses.length === 0 ? (
              <Text color={subTextColor} fontSize="sm" p={6} textAlign="center">
                {t("optimization.cpuScheduler.noProcessFound")}
              </Text>
            ) : (
              filteredProcesses.map((proc) => (
                <HStack
                  key={proc.pid}
                  w="full"
                  px={4}
                  py={2}
                  spacing={3}
                  cursor="pointer"
                  bg={excludeProcess?.pid === proc.pid ? themeColorRgba(0.15) : "transparent"}
                  _hover={{ bg: excludeProcess?.pid === proc.pid ? themeColorRgba(0.2) : listItemHoverBg }}
                  onClick={() => handleSelectExclude(proc)}
                  borderBottom="1px solid"
                  borderBottomColor={cardBorder}
                  transition="background 0.15s"
                >
                  <Box
                    w="8px"
                    h="8px"
                    borderRadius="full"
                    bg={excludeProcess?.pid === proc.pid ? themeColorHex : "transparent"}
                    flexShrink={0}
                  />
                  <Box
                    w={6}
                    h={6}
                    borderRadius="md"
                    bg={icons[proc.exe_path] ? "transparent" : themeColorRgba(0.1)}
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    flexShrink={0}
                    overflow="hidden"
                  >
                    {icons[proc.exe_path] ? (
                      <img
                        src={icons[proc.exe_path]}
                        alt=""
                        style={{ width: "100%", height: "100%", objectFit: "contain" }}
                        loading="lazy"
                      />
                    ) : (
                      <Cpu size={13} color={subTextColor} />
                    )}
                  </Box>
                  <Text color={textColor} fontSize="sm" fontWeight="500" flex={1} isTruncated>{proc.name}</Text>
                  <Text color={subTextColor} fontSize="xs" flexShrink={0}>PID: {proc.pid}</Text>
                  <Text color={subTextColor} fontSize="xs" flexShrink={0} minW="70px" textAlign="right">{proc.memory_mb.toFixed(0)} MB</Text>
                </HStack>
              ))
            )}
          </Box>
        </VStack>
      </Box>

      {/* ── 第 2 步：选择隔离核心 ── */}
      <Box w="full">
        <HStack justify="space-between" align="center" mb={2}>
          <HStack spacing={2}>
            <Cpu size={16} color={isoColor} />
            <Text fontWeight="600" color={textColor} fontSize="md">
              {t(`${k}.selectCores`)}
            </Text>
          </HStack>
          <Text color={subTextColor} fontSize="xs">
            {isolatedCores.size} / {topology?.total_logical_processors ?? 0} {t("optimization.cpuScheduler.logicalProcessor")}
          </Text>
        </HStack>
        <Text color={subTextColor} fontSize="xs" mb={3}>
          {t(`${k}.selectCoresHint`)}
        </Text>

        {!topology ? (
          <Text color={subTextColor} fontSize="sm" p={4}>{t("optimization.cpuScheduler.loading")}</Text>
        ) : (
          <VStack align="start" spacing={4} p={4} borderRadius="3xl" border="1px solid" borderColor={cardBorder} w="full">
            <HStack spacing={2} w="full" wrap="wrap">
              <Text color={subTextColor} fontSize="sm" fontWeight="600" mr={2}>
                {t("optimization.cpuScheduler.quickPreset")}:
              </Text>
              {topology.has_hybrid_architecture && (
                <>
                  <Button
                    size="sm"
                    variant={isolationPreset === "allPCores" ? "solid" : "outline"}
                    bg={isolationPreset === "allPCores" ? isoColor : "transparent"}
                    color={isolationPreset === "allPCores" ? "#ffffff" : textColor}
                    borderColor={isoColor}
                    onClick={() => applyPreset("allPCores")}
                    borderRadius="full"
                  >
                    {t("optimization.cpuScheduler.presetAllPCores")}
                  </Button>
                  <Button
                    size="sm"
                    variant={isolationPreset === "allECores" ? "solid" : "outline"}
                    bg={isolationPreset === "allECores" ? eCoreColor : "transparent"}
                    color={isolationPreset === "allECores" ? "#1a1a1a" : textColor}
                    borderColor={eCoreColor}
                    onClick={() => applyPreset("allECores")}
                    borderRadius="full"
                  >
                    {t("optimization.cpuScheduler.presetAllECores")}
                  </Button>
                </>
              )}
              <Button
                size="sm"
                variant={isolationPreset === "allCores" ? "solid" : "outline"}
                bg={isolationPreset === "allCores" ? isoColor : "transparent"}
                color={isolationPreset === "allCores" ? "#ffffff" : textColor}
                borderColor={isoColor}
                onClick={() => applyPreset("allCores")}
                borderRadius="full"
              >
                {t("optimization.cpuScheduler.presetAllCores")}
              </Button>
            </HStack>

            {pCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Zap size={14} color={isoColor} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">{t("optimization.cpuScheduler.pCores")}</Text>
                </HStack>
                <SimpleGrid columns={{ base: 3, md: 5, lg: 8 }} spacing={2}>
                  {pCores.map((core) => (
                    <Box key={core.core_index} p={2} borderRadius="md" border="1px solid" borderColor={isoColorRgba(0.25)}>
                      <Text color={isoColor} fontSize="10px" fontWeight="700" textAlign="center" mb={1}>P{core.core_index}</Text>
                      <VStack spacing={0}>
                        {core.logical_processors.map((lp) => (
                          <Checkbox
                            key={lp}
                            isChecked={isolatedCores.has(lp)}
                            onChange={() => handleToggleCore(lp)}
                            size="md"
                            sx={isoCheckboxSx}
                          >
                            <Text color={textColor} fontSize="sm">CPU{lp}</Text>
                          </Checkbox>
                        ))}
                      </VStack>
                    </Box>
                  ))}
                </SimpleGrid>
              </Box>
            )}

            {eCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Layers size={14} color={eCoreColor} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">{t("optimization.cpuScheduler.eCores")}</Text>
                </HStack>
                <SimpleGrid columns={{ base: 3, md: 5, lg: 8 }} spacing={2}>
                  {eCores.map((core) => (
                    <Box key={core.core_index} p={2} borderRadius="md" border="1px solid" borderColor={hexToRgba(eCoreColor, 0.25)}>
                      <Text color={eCoreColor} fontSize="10px" fontWeight="700" textAlign="center" mb={1}>E{core.core_index}</Text>
                      <VStack spacing={0}>
                        {core.logical_processors.map((lp) => (
                          <Checkbox
                            key={lp}
                            isChecked={isolatedCores.has(lp)}
                            onChange={() => handleToggleCore(lp)}
                            size="md"
                            sx={eCoreCheckboxSx}
                          >
                            <Text color={textColor} fontSize="sm">CPU{lp}</Text>
                          </Checkbox>
                        ))}
                      </VStack>
                    </Box>
                  ))}
                </SimpleGrid>
              </Box>
            )}

            {unknownCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Cpu size={14} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">{t("optimization.cpuScheduler.allCores")}</Text>
                </HStack>
                <SimpleGrid columns={{ base: 3, md: 5, lg: 8 }} spacing={2}>
                  {unknownCores.map((core) => (
                    <Box key={core.core_index} p={2} borderRadius="md" border="1px solid" borderColor={cardBorder}>
                      <Text color={textColor} fontSize="10px" fontWeight="700" textAlign="center" mb={1}>C{core.core_index}</Text>
                      <VStack spacing={0}>
                        {core.logical_processors.map((lp) => (
                          <Checkbox
                            key={lp}
                            isChecked={isolatedCores.has(lp)}
                            onChange={() => handleToggleCore(lp)}
                            size="md"
                            sx={isoCheckboxSx}
                          >
                            <Text color={textColor} fontSize="sm">CPU{lp}</Text>
                          </Checkbox>
                        ))}
                      </VStack>
                    </Box>
                  ))}
                </SimpleGrid>
              </Box>
            )}

            <Divider />

            <HStack justify="space-between" w="full" wrap="wrap" spacing={4}>
              <VStack align="start" spacing={1}>
                <Text color={subTextColor} fontSize="xs">{t(`${k}.isolatedMaskLabel`)}:</Text>
                <HStack spacing={2}>
                  <Badge bg={isoColorRgba(0.15)} color={isoColor} fontSize="sm" px={3} py={1} borderRadius="full">
                    0x{isolatedMask.toString(16).toUpperCase().padStart(16, "0")}
                  </Badge>
                  {isAllCoresSelected && (
                    <Text color="red.400" fontSize="xs">{t(`${k}.allCoresForbidden`)}</Text>
                  )}
                </HStack>
              </VStack>
            </HStack>
          </VStack>
        )}
      </Box>

      {/* ── 应用 / 恢复 ── */}
      <HStack spacing={4} w="full">
        <Button
          bg={isoColor}
          color="#ffffff"
          size="lg"
          flex={1}
          onClick={handleApply}
          isLoading={isApplying}
          loadingText={t(`${k}.applying`)}
          leftIcon={<ShieldCheck size={20} />}
          borderRadius="3xl"
          fontWeight="700"
          fontSize="md"
          height="56px"
          boxShadow={`0 4px 20px -5px ${isoColorRgba(0.5)}`}
          _hover={{ bg: hexToRgba(isoColor, 0.85), boxShadow: `0 6px 25px -5px ${isoColorRgba(0.6)}` }}
          _active={{ bg: hexToRgba(isoColor, 0.75) }}
          transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
        >
          {t(`${k}.apply`)}
        </Button>
        <Button
          bg={restoreBg}
          color={restoreColor}
          border="1px solid"
          borderColor={restoreBorder}
          size="lg"
          flex={1}
          onClick={handleRestore}
          isDisabled={!isolationState}
          isLoading={isRestoring}
          loadingText={t(`${k}.restoring`)}
          leftIcon={<AlertTriangle size={20} />}
          borderRadius="3xl"
          fontWeight="700"
          fontSize="md"
          height="56px"
          _hover={{ bg: restoreBg, borderColor: isoColor, color: isoColor }}
          transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
        >
          {t(`${k}.restore`)}
        </Button>
      </HStack>

      {/* ── 结果统计 ── */}
      {result && (
        <Alert
          status={result.failed > 0 ? "warning" : "success"}
          borderRadius="xl"
          bg={result.failed > 0 ? "rgba(221, 107, 32, 0.12)" : "rgba(56, 161, 105, 0.12)"}
          borderLeft="4px solid"
          borderColor={result.failed > 0 ? "#DD6B20" : "#38A169"}
          flexDir="column"
          alignItems="flex-start"
        >
          <HStack spacing={2}>
            <AlertIcon color={result.failed > 0 ? "#DD6B20" : "#38A169"} />
            <Text color={textColor} fontSize="sm" fontWeight="600">
              {t(`${k}.applyResult`, { modified: result.modified, failed: result.failed })}
            </Text>
          </HStack>
          {result.failed > 0 && (
            <Box mt={2} pl={8} w="full">
              <Text color={subTextColor} fontSize="xs" mb={1}>
                {t(`${k}.failedList`)}: {result.failed_processes.slice(0, 30).join(", ")}
                {result.failed_processes.length > 30 ? "..." : ""}
              </Text>
              <Text color="#DD6B20" fontSize="xs">
                {t(`${k}.permissionHint`)}
              </Text>
            </Box>
          )}
        </Alert>
      )}

      {/* ── 保存为隔离规则 ── */}
      <Box w="full">
        <Text fontWeight="600" color={textColor} fontSize="md" mb={2}>
          {t(`${k}.saveRule`)}
        </Text>
        <VStack align="start" spacing={3} p={4} borderRadius="3xl" border="1px solid" borderColor={cardBorder} w="full">
          <HStack w="full" spacing={3}>
            <Input
              value={ruleName}
              onChange={(e) => setRuleName(e.target.value)}
              placeholder={t(`${k}.ruleNamePlaceholder`)}
              color={headingColor}
              borderColor={cardBorder}
              bg={cardBg}
              _focus={{ borderColor: isoColor }}
              flex={1}
            />
            <Button
              size="md"
              onClick={handleSaveRule}
              isLoading={isSavingRule}
              loadingText={t("optimization.cpuScheduler.saving")}
              leftIcon={<Save size={18} />}
              bg={isoColor}
              color="#ffffff"
              borderRadius="xl"
              fontWeight="600"
              _hover={{ bg: hexToRgba(isoColor, 0.85) }}
            >
              {t(`${k}.saveRuleBtn`)}
            </Button>
          </HStack>
          <HStack spacing={2}>
            <Switch
              isChecked={autoApply}
              onChange={(e) => setAutoApply(e.target.checked)}
              colorScheme="orange"
              size="md"
            />
            <Text color={textColor} fontSize="sm">{t(`${k}.autoApply`)}</Text>
            <Text color={subTextColor} fontSize="xs">{t(`${k}.autoApplyHint`)}</Text>
          </HStack>
        </VStack>
      </Box>

      {/* ── 隔离规则列表 ── */}
      {rules.length > 0 && (
        <Box w="full">
          <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
            {t(`${k}.rulesTitle`)}
          </Text>
          <VStack align="start" spacing={2} p={4} borderRadius="3xl" border="1px solid" borderColor={cardBorder} w="full">
            {rules.map((rule) => (
              <HStack key={rule.name} w="full" justify="space-between" spacing={3}>
                <HStack spacing={3} flex={1} minW={0}>
                  <Badge
                    bg={
                      rule.preset === "allPCores"
                        ? isoColorRgba(0.15)
                        : rule.preset === "allECores"
                          ? hexToRgba(eCoreColor, 0.15)
                          : isoColorRgba(0.1)
                    }
                    color={rule.preset === "allECores" ? eCoreColor : isoColor}
                    fontSize="xs"
                    px={2}
                    py={1}
                    borderRadius="full"
                  >
                    {rule.preset === "allPCores"
                      ? t("optimization.cpuScheduler.presetAllPCores")
                      : rule.preset === "allECores"
                        ? t("optimization.cpuScheduler.presetAllECores")
                        : rule.preset === "allCores"
                          ? t("optimization.cpuScheduler.presetAllCores")
                          : "Custom"}
                  </Badge>
                  <VStack align="start" spacing={0} minW={0}>
                    <Text color={textColor} fontSize="sm" fontWeight="600" isTruncated>{rule.name}</Text>
                    <Text color={subTextColor} fontSize="xs" isTruncated>
                      0x{rule.isolated_mask.toString(16).toUpperCase().padStart(16, "0")}
                      {rule.exclude_process && ` · ${t(`${k}.exempted`)}: ${rule.exclude_process}`}
                    </Text>
                  </VStack>
                  {isRuleActive(rule) && (
                    <Badge bg={themeColorRgba(0.15)} color={themeColorHex} fontSize="xs" px={2} py={1} borderRadius="full">
                      {t(`${k}.activeBadge`)}
                    </Badge>
                  )}
                  {rule.auto_apply && (
                    <Badge bg="green.500" color="#ffffff" fontSize="xs" px={2} py={1} borderRadius="full">
                      {t(`${k}.autoApplyBadge`)}
                    </Badge>
                  )}
                </HStack>
                <HStack spacing={1}>
                  {isRuleActive(rule) ? (
                    <Button
                      size="xs"
                      variant="ghost"
                      color={isoColor}
                      onClick={handleRestore}
                      isLoading={isRestoring}
                    >
                      {t(`${k}.restoreRule`)}
                    </Button>
                  ) : (
                    <Button size="xs" variant="ghost" color={isoColor} onClick={() => handleApplyRule(rule)}>
                      {t("optimization.cpuScheduler.applyRule")}
                    </Button>
                  )}
                  <IconButton
                    aria-label="delete"
                    size="xs"
                    variant="ghost"
                    icon={<Trash2 size={14} />}
                    color="red.400"
                    onClick={() => handleDeleteRule(rule)}
                  />
                </HStack>
              </HStack>
            ))}
          </VStack>
        </Box>
      )}

      {/* ── 隔离警告 ── */}
      <Alert
        status="warning"
        borderRadius="xl"
        bg={useColorModeValue("orange.50", "rgba(255, 165, 0, 0.1)")}
        borderLeft="4px solid"
        borderColor="orange.400"
      >
        <AlertIcon as={AlertTriangle} color="orange.500" />
        <AlertDescription color={textColor} fontSize="sm">
          <strong>{t("optimization.cpuScheduler.warning")}:</strong> {t(`${k}.warningText`)}
        </AlertDescription>
      </Alert>
    </VStack>
  );
}
