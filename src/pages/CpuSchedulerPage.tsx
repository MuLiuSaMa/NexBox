import {
  Box,
  Heading,
  VStack,
  HStack,
  Text,
  useColorModeValue,
  Button,
  Badge,
  Alert,
  AlertIcon,
  AlertDescription,
  Input,
  Checkbox,
  SimpleGrid,
  Spinner,
  Divider,
  IconButton,
  Tooltip,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useProcessIcons } from "@/hooks/use-process-icons";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import CpuIsolationPanel from "@/pages/CpuIsolationPanel";
import {
  ArrowLeft,
  AlertTriangle,
  Cpu,
  Search,
  RefreshCw,
  Layers,
  Zap,
  Save,
  Trash2,
} from "lucide-react";
import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
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

interface ProcessAffinityInfo {
  pid: number;
  process_name: string;
  affinity_mask: number;
  system_mask: number;
  assigned_logical_processors: number[];
}

interface SchedulerRule {
  process_name: string;
  mask: number;
  preset: string;
  description: string;
}

type PresetType = "allPCores" | "allECores" | "allCores" | "custom";

// ── Component ──────────────────────────────────────────────

export default function CpuSchedulerPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("cpu");
  const { icons, ensureIcons } = useProcessIcons();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const adaptiveTitle = useAdaptiveTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");
  const listItemHoverBg = useColorModeValue("gray.50", "#222222");
  const restoreBg = useColorModeValue("#FFFFFF", "#1A202C");
  const restoreColor = useColorModeValue("#1A202C", "#FFFFFF");
  const restoreHoverBg = useColorModeValue("#F7FAFC", "#2D3748");
  const restoreBorder = useColorModeValue("gray.300", "gray.700");

  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const themeColorHex = primaryColor || "#98DDD0";
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);
  const eCoreColor = "#F6AD55";

  const pCoreCheckboxSx = useMemo(() => ({
    ".chakra-checkbox__control": {
      borderColor: cardBorder,
      "&[data-checked]": {
        bg: themeColorHex,
        borderColor: themeColorHex,
      },
    },
    _hover: {
      ".chakra-checkbox__control:not([data-checked])": {
        borderColor: themeColorHex,
      },
    },
    _focusWithin: {
      ".chakra-checkbox__control": {
        boxShadow: `0 0 0 3px ${themeColorRgba(0.3)}`,
      },
    },
  }), [cardBorder, themeColorHex, themeColorRgba]);

  const eCoreCheckboxSx = useMemo(() => ({
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
  }), [cardBorder, eCoreColor]);

  // State
  const [topology, setTopology] = useState<CpuTopology | null>(null);
  const [processes, setProcesses] = useState<ProcessInfo[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedProcess, setSelectedProcess] = useState<ProcessInfo | null>(null);
  const [affinityInfo, setAffinityInfo] = useState<ProcessAffinityInfo | null>(null);
  const [selectedCores, setSelectedCores] = useState<Set<number>>(new Set());
  const [currentPreset, setCurrentPreset] = useState<PresetType>("custom");
  const [rules, setRules] = useState<SchedulerRule[]>([]);

  const [isLoadingTopology, setIsLoadingTopology] = useState(true);
  const [isLoadingProcesses, setIsLoadingProcesses] = useState(false);
  const [isLoadingAffinity, setIsLoadingAffinity] = useState(false);
  const [isApplying, setIsApplying] = useState(false);
  const [isSavingRule, setIsSavingRule] = useState(false);

  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // ── Data Loading ────────────────────────────────────────

  const loadTopology = useCallback(async () => {
    try {
      const data = await invoke<CpuTopology>("get_cpu_topology");
      setTopology(data);
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsLoadingTopology(false);
    }
  }, [t, toast]);

  const loadProcesses = useCallback(async () => {
    setIsLoadingProcesses(true);
    try {
      const data = await invoke<ProcessInfo[]>("get_process_list");
      setProcesses(data);
      ensureIcons(data.map(p => p.exe_path));
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsLoadingProcesses(false);
    }
  }, [t, toast, ensureIcons]);

  const loadRules = useCallback(async () => {
    try {
      const data = await invoke<SchedulerRule[]>("get_saved_rules");
      setRules(data);
    } catch { /* ignore */ }
  }, []);

  useEffect(() => {
    loadTopology();
    loadProcesses();
    loadRules();
  }, [loadTopology, loadProcesses, loadRules]);

  // Auto-refresh process list
  useEffect(() => {
    if (searchQuery) return;
    refreshTimerRef.current = setInterval(() => {
      invoke<ProcessInfo[]>("get_process_list")
        .then(data => {
          setProcesses(data);
          ensureIcons(data.map(p => p.exe_path));
        })
        .catch(() => {});
    }, 3000);
    return () => { if (refreshTimerRef.current) clearInterval(refreshTimerRef.current); };
  }, [searchQuery, ensureIcons]);

  // ── Process Selection ───────────────────────────────────

  const handleSelectProcess = useCallback(async (proc: ProcessInfo) => {
    setSelectedProcess(proc);
    setSelectedCores(new Set());
    setCurrentPreset("custom");
    setIsLoadingAffinity(true);
    try {
      const info = await invoke<ProcessAffinityInfo>("get_process_affinity", { pid: proc.pid });
      setAffinityInfo(info);
      setSelectedCores(new Set(info.assigned_logical_processors));
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 3000, isClosable: true });
      setAffinityInfo(null);
    } finally {
      setIsLoadingAffinity(false);
    }
  }, [t, toast]);

  // ── Core Toggle ─────────────────────────────────────────

  const handleToggleCore = useCallback((lp: number) => {
    setSelectedCores((prev) => {
      const next = new Set(prev);
      if (next.has(lp)) next.delete(lp); else next.add(lp);
      return next;
    });
    setCurrentPreset("custom");
  }, []);

  // ── Presets ─────────────────────────────────────────────

  const applyPreset = useCallback((preset: PresetType) => {
    if (!topology) return;
    const next = new Set<number>();
    for (const core of topology.physical_cores) {
      const matches = preset === "allCores"
        || (preset === "allPCores" && core.core_type === "Performance")
        || (preset === "allECores" && core.core_type === "Efficiency");
      if (matches) for (const lp of core.logical_processors) next.add(lp);
    }
    setSelectedCores(next);
    setCurrentPreset(preset);
  }, [topology]);

  // ── Apply / Restore ─────────────────────────────────────

  const handleApply = useCallback(async () => {
    if (!selectedProcess) return;
    if (selectedCores.size === 0) {
      toast({ title: t("optimization.cpuScheduler.noCoreSelected"), status: "warning", duration: 3000, isClosable: true });
      return;
    }
    let mask = 0;
    for (const lp of selectedCores) mask |= (1 << lp);
    setIsApplying(true);
    try {
      await invoke("set_process_affinity", { pid: selectedProcess.pid, mask });
      toast({ title: t("optimization.cpuScheduler.assignmentApplied"), description: `${selectedProcess.name} → ${selectedCores.size} ${t("optimization.cpuScheduler.logicalProcessor")}`, status: "success", duration: 4000, isClosable: true });
      const info = await invoke<ProcessAffinityInfo>("get_process_affinity", { pid: selectedProcess.pid });
      setAffinityInfo(info);
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsApplying(false);
    }
  }, [selectedProcess, selectedCores, t, toast]);

  const handleRestore = useCallback(async () => {
    if (!selectedProcess) return;
    setIsApplying(true);
    try {
      await invoke("restore_process_affinity", { pid: selectedProcess.pid });
      toast({ title: t("optimization.cpuScheduler.assignmentRestored"), status: "success", duration: 3000, isClosable: true });
      const info = await invoke<ProcessAffinityInfo>("get_process_affinity", { pid: selectedProcess.pid });
      setAffinityInfo(info);
      setSelectedCores(new Set(info.assigned_logical_processors));
      setCurrentPreset("custom");
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsApplying(false);
    }
  }, [selectedProcess, t, toast]);

  // ── Rules ───────────────────────────────────────────────

  const handleSaveRule = useCallback(async () => {
    if (!selectedProcess || selectedCores.size === 0) return;
    let mask = 0;
    for (const lp of selectedCores) mask |= (1 << lp);
    setIsSavingRule(true);
    try {
      await invoke("save_rule", { processName: selectedProcess.name, mask, preset: currentPreset, description: "" });
      toast({ title: t("optimization.cpuScheduler.ruleSaved"), status: "success", duration: 3000, isClosable: true });
      await loadRules();
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsSavingRule(false);
    }
  }, [selectedProcess, selectedCores, currentPreset, t, toast, loadRules]);

  const handleDeleteRule = useCallback(async (ruleName: string) => {
    try {
      await invoke("delete_rule", { processName: ruleName });
      toast({ title: t("optimization.cpuScheduler.ruleDeleted"), status: "success", duration: 3000, isClosable: true });
      await loadRules();
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    }
  }, [t, toast, loadRules]);

  const handleApplyRule = useCallback(async (rule: SchedulerRule) => {
    try {
      const [success, count] = await invoke<[boolean, number]>("apply_rule_by_name", { processName: rule.process_name });
      if (success) {
        toast({ title: t("optimization.cpuScheduler.assignmentApplied"), description: `${rule.process_name} → ${count} ${t("optimization.cpuScheduler.processes")}`, status: "success", duration: 4000, isClosable: true });
      }
    } catch (error) {
      toast({ title: t("optimization.cpuScheduler.error"), description: String(error), status: "error", duration: 5000, isClosable: true });
    }
  }, [t, toast]);

  // ── Filtered Processes ──────────────────────────────────

  const filteredProcesses = (() => {
    if (!searchQuery.trim()) return processes.slice(0, 200);
    const q = searchQuery.toLowerCase();
    return processes
      .filter((p) => p.name.toLowerCase().includes(q) || String(p.pid).includes(q))
      .slice(0, 200);
  })();

  // ── Group Cores by Type ─────────────────────────────────

  const pCores = topology?.physical_cores.filter((c) => c.core_type === "Performance") ?? [];
  const eCores = topology?.physical_cores.filter((c) => c.core_type === "Efficiency") ?? [];
  const unknownCores = topology?.physical_cores.filter((c) => c.core_type === "Unknown") ?? [];

  // Current mask for display
  const currentMask = (() => {
    let mask = 0;
    for (const lp of selectedCores) mask |= (1 << lp);
    return mask;
  })();

  // ── Render ──────────────────────────────────────────────

  const content = (
    <VStack align="start" spacing={6}>
      {/* Header */}
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button variant="ghost" leftIcon={<ArrowLeft size={18} />} onClick={() => navigate("/optimize")} color={headingColor}>
          {t("optimization.cpuScheduler.back")}
        </Button>
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("optimization.cpuScheduler.title")}
        </Heading>
        <Box w="100px" />
      </HStack>

      {/* ── 模式切换 Tab ── */}
      <Tabs variant="soft-rounded" defaultIndex={0} isLazy w="full">
        <TabList gap={2} mb={4}>
          <Tab
            _selected={{
              bg: themeColorHex,
              color: contrastText,
              boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
              // 悬停已选中的胶囊时保持实心主题色，避免被下面的半透明 hover 底色冲淡成「透明」
              _hover: { bg: themeColorHex },
            }}
            _hover={{ bg: themeColorRgba(0.15) }}
            borderRadius="full"
            fontWeight="600"
            fontSize="sm"
            px={6}
            py={2}
          >
            <Cpu size={15} style={{ marginRight: 6 }} />
            {t("optimization.cpuScheduler.coreIsolation.assignmentTab")}
          </Tab>
          <Tab
            _selected={{
              bg: themeColorHex,
              color: contrastText,
              boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
              // 悬停已选中的胶囊时保持实心主题色，避免被下面的半透明 hover 底色冲淡成「透明」
              _hover: { bg: themeColorHex },
            }}
            _hover={{ bg: themeColorRgba(0.15) }}
            borderRadius="full"
            fontWeight="600"
            fontSize="sm"
            px={6}
            py={2}
          >
            <Layers size={15} style={{ marginRight: 6 }} />
            {t("optimization.cpuScheduler.coreIsolation.tab")}
          </Tab>
        </TabList>
        <TabPanels>
          <TabPanel px={0} pt={0}>
      {/* ── CPU Topology ── */}
      <Box w="full">
        <HStack justify="space-between" align="center" mb={3}>
          <Text fontWeight="600" color={textColor} fontSize="md">
            {t("optimization.cpuScheduler.topologyTitle")}
          </Text>
          <IconButton
            aria-label="refresh"
            icon={<RefreshCw size={16} />}
            size="sm"
            variant="ghost"
            onClick={() => { loadTopology(); loadProcesses(); }}
            color={subTextColor}
          />
        </HStack>

        {isLoadingTopology ? (
          <HStack justify="center" p={8}><Spinner color={themeColorHex} /><Text color={subTextColor}>{t("optimization.cpuScheduler.loading")}</Text></HStack>
        ) : topology ? (
          <VStack align="start" spacing={3} p={4} borderRadius="xl" border="1px solid" borderColor={cardBorder} w="full">
            <HStack justify="space-between" w="full">
              <HStack spacing={2}>
                <Cpu size={18} color={themeColorHex} />
                <Text color={textColor} fontWeight="600" fontSize="sm">{topology.cpu_name}</Text>
              </HStack>
              <HStack spacing={2}>
                <Badge bg={themeColorRgba(0.15)} color={themeColorHex} fontSize="xs" px={2} py={1} borderRadius="full">
                  {topology.total_physical_cores} {t("optimization.cpuScheduler.cores")}
                </Badge>
                <Badge bg={themeColorRgba(0.15)} color={themeColorHex} fontSize="xs" px={2} py={1} borderRadius="full">
                  {topology.total_logical_processors} {t("optimization.cpuScheduler.threads")}
                </Badge>
              </HStack>
            </HStack>

            {!topology.has_hybrid_architecture && unknownCores.length === 0 && (
              <Text color={subTextColor} fontSize="xs">{t("optimization.cpuScheduler.hybridNotSupported")}</Text>
            )}

            {/* P-Cores */}
            {pCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Zap size={14} color={themeColorHex} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">
                    {t("optimization.cpuScheduler.pCores")} ({pCores.length})
                  </Text>
                </HStack>
                <SimpleGrid columns={{ base: 2, md: 4, lg: 6 }} spacing={2}>
                  {pCores.map((core) => (
                    <Tooltip key={core.core_index} label={`P${core.core_index}: CPU ${core.logical_processors.join(", ")}`}>
                      <Box p={2} borderRadius="lg" border="1px solid" borderColor={themeColorRgba(0.3)} bg={themeColorRgba(0.08)}>
                        <Text color={themeColorHex} fontSize="xs" fontWeight="700" textAlign="center">P{core.core_index}</Text>
                        <Text color={subTextColor} fontSize="10px" textAlign="center">{core.logical_processors.join(",")}</Text>
                      </Box>
                    </Tooltip>
                  ))}
                </SimpleGrid>
              </Box>
            )}

            {/* E-Cores */}
            {eCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Layers size={14} color={eCoreColor} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">
                    {t("optimization.cpuScheduler.eCores")} ({eCores.length})
                  </Text>
                </HStack>
                <SimpleGrid columns={{ base: 2, md: 4, lg: 6 }} spacing={2}>
                  {eCores.map((core) => (
                    <Tooltip key={core.core_index} label={`E${core.core_index}: CPU ${core.logical_processors.join(", ")}`}>
                      <Box p={2} borderRadius="lg" border="1px solid" borderColor={hexToRgba(eCoreColor, 0.3)} bg={hexToRgba(eCoreColor, 0.08)}>
                        <Text color={eCoreColor} fontSize="xs" fontWeight="700" textAlign="center">E{core.core_index}</Text>
                        <Text color={subTextColor} fontSize="10px" textAlign="center">{core.logical_processors.join(",")}</Text>
                      </Box>
                    </Tooltip>
                  ))}
                </SimpleGrid>
              </Box>
            )}

            {/* Unknown cores (non-hybrid AMD etc) */}
            {unknownCores.length > 0 && (
              <Box w="full">
                <HStack spacing={2} mb={2}>
                  <Cpu size={14} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm" fontWeight="600">
                    {t("optimization.cpuScheduler.allCores")} ({unknownCores.length})
                  </Text>
                </HStack>
                <SimpleGrid columns={{ base: 2, md: 4, lg: 6 }} spacing={2}>
                  {unknownCores.map((core) => (
                    <Tooltip key={core.core_index} label={`Core ${core.core_index}: CPU ${core.logical_processors.join(", ")}`}>
                      <Box p={2} borderRadius="lg" border="1px solid" borderColor={cardBorder} bg={hoverBg}>
                        <Text color={textColor} fontSize="xs" fontWeight="700" textAlign="center">C{core.core_index}</Text>
                        <Text color={subTextColor} fontSize="10px" textAlign="center">{core.logical_processors.join(",")}</Text>
                      </Box>
                    </Tooltip>
                  ))}
                </SimpleGrid>
              </Box>
            )}
          </VStack>
        ) : (
          <Text color="red.400" fontSize="sm">{t("optimization.cpuScheduler.error")}</Text>
        )}
      </Box>

      {/* ── Process Search ── */}
      <Box w="full">
        <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
          {t("optimization.cpuScheduler.processSelection")}
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
                _focus={{ borderColor: themeColorHex }}
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
            maxH="280px"
            overflowY="auto"
            borderRadius="xl"
            border="1px solid"
            borderColor={cardBorder}
            bg={cardBg}
          >
            {isLoadingProcesses ? (
              <HStack justify="center" p={6}><Spinner size="sm" color={themeColorHex} /><Text color={subTextColor} fontSize="sm">{t("optimization.cpuScheduler.loadingProcesses")}</Text></HStack>
            ) : filteredProcesses.length === 0 ? (
              <Text color={subTextColor} fontSize="sm" p={6} textAlign="center">{t("optimization.cpuScheduler.noProcessFound")}</Text>
            ) : (
              filteredProcesses.map((proc) => (
                <HStack
                  key={proc.pid}
                  w="full"
                  px={4}
                  py={2}
                  spacing={3}
                  cursor="pointer"
                  bg={selectedProcess?.pid === proc.pid ? themeColorRgba(0.12) : "transparent"}
                  _hover={{ bg: selectedProcess?.pid === proc.pid ? themeColorRgba(0.15) : listItemHoverBg }}
                  onClick={() => handleSelectProcess(proc)}
                  borderBottom="1px solid"
                  borderBottomColor={cardBorder}
                  transition="background 0.15s"
                >
                  <Box
                    w="8px" h="8px" borderRadius="full"
                    bg={selectedProcess?.pid === proc.pid ? themeColorHex : "transparent"}
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
                  <Text color={subTextColor} fontSize="xs" flexShrink={0} minW="50px" textAlign="right">{proc.cpu_usage.toFixed(1)}%</Text>
                </HStack>
              ))
            )}
          </Box>
        </VStack>
      </Box>

      {/* ── Core Assignment ── */}
      <Box w="full">
        <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
          {t("optimization.cpuScheduler.coreAssignment")}
          {selectedProcess && (
            <Text as="span" color={subTextColor} fontSize="sm" ml={2}>
              ({t("optimization.cpuScheduler.selectedProcess")}: {selectedProcess.name})
            </Text>
          )}
        </Text>

        {!selectedProcess ? (
          <Box p={6} borderRadius="xl" border="1px solid" borderColor={cardBorder} bg={cardBg} textAlign="center">
            <Text color={subTextColor} fontSize="sm">{t("optimization.cpuScheduler.noProcessSelected")}</Text>
          </Box>
        ) : (
          <VStack align="start" spacing={4} p={4} borderRadius="xl" border="1px solid" borderColor={cardBorder} w="full">
            {isLoadingAffinity ? (
              <HStack justify="center" w="full" p={4}><Spinner color={themeColorHex} /></HStack>
            ) : (
              <>
                {/* Quick Presets */}
                <HStack spacing={2} w="full" wrap="wrap">
                  <Text color={subTextColor} fontSize="sm" fontWeight="600" mr={2}>{t("optimization.cpuScheduler.quickPreset")}:</Text>
                  {topology?.has_hybrid_architecture && (
                    <>
                      <Button
                        size="sm" variant={currentPreset === "allPCores" ? "solid" : "outline"}
                        bg={currentPreset === "allPCores" ? themeColorHex : "transparent"}
                        color={currentPreset === "allPCores" ? contrastText : textColor}
                        borderColor={themeColorHex}
                        onClick={() => applyPreset("allPCores")}
                        borderRadius="full"
                      >
                        {t("optimization.cpuScheduler.presetAllPCores")}
                      </Button>
                      <Button
                        size="sm" variant={currentPreset === "allECores" ? "solid" : "outline"}
                        bg={currentPreset === "allECores" ? eCoreColor : "transparent"}
                        color={currentPreset === "allECores" ? "#1a1a1a" : textColor}
                        borderColor={eCoreColor}
                        onClick={() => applyPreset("allECores")}
                        borderRadius="full"
                      >
                        {t("optimization.cpuScheduler.presetAllECores")}
                      </Button>
                    </>
                  )}
                  <Button
                    size="sm" variant={currentPreset === "allCores" ? "solid" : "outline"}
                    bg={currentPreset === "allCores" ? themeColorHex : "transparent"}
                    color={currentPreset === "allCores" ? contrastText : textColor}
                    borderColor={themeColorHex}
                    onClick={() => applyPreset("allCores")}
                    borderRadius="full"
                  >
                    {t("optimization.cpuScheduler.presetAllCores")}
                  </Button>
                </HStack>

                <Divider />

                {/* Core Checkboxes — grouped */}
                {pCores.length > 0 && (
                  <Box w="full">
                    <HStack spacing={2} mb={2}>
                      <Zap size={14} color={themeColorHex} />
                      <Text color={subTextColor} fontSize="sm" fontWeight="600">{t("optimization.cpuScheduler.pCores")}</Text>
                    </HStack>
                    <SimpleGrid columns={{ base: 3, md: 5, lg: 8 }} spacing={2}>
                      {topology?.physical_cores.filter((c) => c.core_type === "Performance").map((core) => (
                        <Box key={core.core_index} p={2} borderRadius="md" border="1px solid" borderColor={themeColorRgba(0.2)}>
                          <Text color={themeColorHex} fontSize="10px" fontWeight="700" textAlign="center" mb={1}>P{core.core_index}</Text>
                          <VStack spacing={0}>
                            {core.logical_processors.map((lp) => (
                              <Checkbox
                                key={lp}
                                isChecked={selectedCores.has(lp)}
                                onChange={() => handleToggleCore(lp)}
                                size="md"
                                sx={pCoreCheckboxSx}
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
                      {topology?.physical_cores.filter((c) => c.core_type === "Efficiency").map((core) => (
                        <Box key={core.core_index} p={2} borderRadius="md" border="1px solid" borderColor={hexToRgba(eCoreColor, 0.2)}>
                          <Text color={eCoreColor} fontSize="10px" fontWeight="700" textAlign="center" mb={1}>E{core.core_index}</Text>
                          <VStack spacing={0}>
                            {core.logical_processors.map((lp) => (
                              <Checkbox
                                key={lp}
                                isChecked={selectedCores.has(lp)}
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
                                isChecked={selectedCores.has(lp)}
                                onChange={() => handleToggleCore(lp)}
                                size="md"
                                sx={pCoreCheckboxSx}
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

                {/* Current affinity info */}
                <HStack justify="space-between" w="full" wrap="wrap" spacing={4}>
                  <VStack align="start" spacing={1}>
                    <Text color={subTextColor} fontSize="xs">{t("optimization.cpuScheduler.currentAffinity")}:</Text>
                    <HStack spacing={2}>
                      <Badge bg={themeColorRgba(0.15)} color={themeColorHex} fontSize="sm" px={3} py={1} borderRadius="full">
                        0x{currentMask.toString(16).toUpperCase().padStart(16, "0")}
                      </Badge>
                      <Text color={textColor} fontSize="sm" fontWeight="600">
                        {selectedCores.size} / {topology?.total_logical_processors ?? 0} {t("optimization.cpuScheduler.logicalProcessor")}
                      </Text>
                    </HStack>
                  </VStack>
                  {affinityInfo && affinityInfo.affinity_mask !== currentMask && (
                    <Text color={subTextColor} fontSize="xs">
                      {t("optimization.cpuScheduler.previousAffinity")}: 0x{affinityInfo.affinity_mask.toString(16).toUpperCase().padStart(16, "0")}
                    </Text>
                  )}
                </HStack>

                {/* Action buttons */}
                <HStack spacing={4} w="full" pt={2}>
                  <Button
                    bg={themeColorHex}
                    color={contrastText}
                    size="lg"
                    flex={1}
                    onClick={handleApply}
                    isLoading={isApplying}
                    loadingText={t("optimization.cpuScheduler.applying")}
                    leftIcon={<Cpu size={20} />}
                    borderRadius="2xl"
                    fontWeight="700"
                    fontSize="md"
                    height="56px"
                    boxShadow={`0 4px 20px -5px ${themeColorRgba(0.5)}`}
                    _hover={{ bg: themeColorRgba(0.85), boxShadow: `0 6px 25px -5px ${themeColorRgba(0.6)}` }}
                    _active={{ bg: themeColorRgba(0.75) }}
                    transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
                  >
                    {t("optimization.cpuScheduler.applyAssignment")}
                  </Button>
                  <Button
                    bg={themeColorHex}
                    color={contrastText}
                    size="lg"
                    flex={1}
                    onClick={handleSaveRule}
                    isLoading={isSavingRule}
                    loadingText={t("optimization.cpuScheduler.saving")}
                    leftIcon={<Save size={20} />}
                    borderRadius="2xl"
                    fontWeight="700"
                    fontSize="md"
                    height="56px"
                    boxShadow={`0 4px 20px -5px ${themeColorRgba(0.5)}`}
                    _hover={{ bg: themeColorRgba(0.85), boxShadow: `0 6px 25px -5px ${themeColorRgba(0.6)}` }}
                    _active={{ bg: themeColorRgba(0.75) }}
                    transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
                  >
                    {t("optimization.cpuScheduler.saveRule")}
                  </Button>
                  <Button
                    bg={restoreBg}
                    color={restoreColor}
                    border="1px solid"
                    borderColor={restoreBorder}
                    size="lg"
                    flex={1}
                    onClick={handleRestore}
                    isLoading={isApplying}
                    leftIcon={<AlertTriangle size={20} />}
                    borderRadius="2xl"
                    fontWeight="700"
                    fontSize="md"
                    height="56px"
                    _hover={{ bg: restoreHoverBg }}
                    transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
                  >
                    {t("optimization.cpuScheduler.restoreDefault")}
                  </Button>
                </HStack>
              </>
            )}
          </VStack>
        )}
      </Box>

      {/* ── Saved Rules ── */}
      {rules.length > 0 && (
        <Box w="full">
          <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
            {t("optimization.cpuScheduler.savedRules")}
          </Text>
          <VStack align="start" spacing={2} p={4} borderRadius="xl" border="1px solid" borderColor={cardBorder} w="full">
            {rules.map((rule) => (
              <HStack key={rule.process_name} w="full" justify="space-between" spacing={3}>
                <HStack spacing={3} flex={1} minW={0}>
                  <Badge
                    bg={rule.preset === "allPCores" ? themeColorRgba(0.15) : rule.preset === "allECores" ? hexToRgba(eCoreColor, 0.15) : themeColorRgba(0.1)}
                    color={rule.preset === "allPCores" ? themeColorHex : rule.preset === "allECores" ? eCoreColor : textColor}
                    fontSize="xs"
                    px={2}
                    py={1}
                    borderRadius="full"
                  >
                    {rule.preset === "allPCores" ? t("optimization.cpuScheduler.presetAllPCores") : rule.preset === "allECores" ? t("optimization.cpuScheduler.presetAllECores") : rule.preset === "allCores" ? t("optimization.cpuScheduler.presetAllCores") : "Custom"}
                  </Badge>
                  <Text color={textColor} fontSize="sm" isTruncated>{rule.process_name}</Text>
                  <Text color={subTextColor} fontSize="xs">0x{rule.mask.toString(16).toUpperCase().padStart(16, "0")}</Text>
                </HStack>
                <HStack spacing={1}>
                  <Button size="xs" variant="ghost" color={themeColorHex} onClick={() => handleApplyRule(rule)}>{t("optimization.cpuScheduler.applyRule")}</Button>
                  <IconButton aria-label="delete" size="xs" variant="ghost" icon={<Trash2 size={14} />} color="red.400" onClick={() => handleDeleteRule(rule.process_name)} />
                </HStack>
              </HStack>
            ))}
          </VStack>
        </Box>
      )}

      {/* ── Warning ── */}
      <Alert
        status="warning"
        borderRadius="xl"
        bg={useColorModeValue("orange.50", "rgba(255, 165, 0, 0.1)")}
        borderLeft="4px solid"
        borderColor="orange.400"
      >
        <AlertIcon as={AlertTriangle} color="orange.500" />
        <AlertDescription color={textColor} fontSize="sm">
          <strong>{t("optimization.cpuScheduler.warning")}:</strong> {t("optimization.cpuScheduler.warningText")}
        </AlertDescription>
      </Alert>
          </TabPanel>

          <TabPanel px={0} pt={0}>
            <CpuIsolationPanel
              topology={topology}
              processes={processes}
              loadProcesses={loadProcesses}
              icons={icons}
            />
          </TabPanel>
        </TabPanels>
      </Tabs>
    </VStack>
  );

  return (
    <Box pt={8}>
      {liquidGlassEnabled ? (
        <LiquidGlassCard w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
          {content}
        </LiquidGlassCard>
      ) : (
        <Box bg={cardBg} borderColor={cardBorder} borderWidth="1px" borderRadius="2xl" w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
          {content}
        </Box>
      )}
    </Box>
  );
}
