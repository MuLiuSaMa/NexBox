import {
  Box, VStack, Text, HStack, useColorModeValue,
  Button, Progress, SimpleGrid, Switch, Checkbox,
  Slider, SliderTrack, SliderFilledTrack, SliderThumb,
  Input, Alert, AlertIcon, AlertDescription, Badge,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { ArrowLeft, MemoryStick, Cpu, HardDrive, Settings } from "lucide-react";
import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { LazyStore } from "@tauri-apps/plugin-store";
import { CustomSelect } from "@/components/special/custom-select";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";

interface MemoryData {
  physical_total: number; physical_used: number; physical_available: number;
  virtual_total: number; virtual_used: number; virtual_available: number;
  working_set_total: number; working_set_used: number; working_set_available: number;
}
interface CleanupResult { success: boolean; message: string; freed_mb: number; }
interface GameStartCleanConfig { enabled: boolean; }
interface MemoryListSizes {
  available: boolean; zeroed_mb: number; free_mb: number; standby_mb: number; modified_mb: number; combined_mb: number;
}
interface MemoryCleanConfig { items: string[]; }
interface AutoCleanConfig {
  enabled: boolean; interval_seconds: number; threshold_mb: number; clean_type: string;
}

interface PagefileRecommendation {
  verdict: string; suggested_initial_mb: number; suggested_max_mb: number; message: string;
}
// 单个磁盘盘符的页面文件设置（模式 + 初始/最大）
interface PagefileDrive {
  path: string; drive: string; mode: string; initial_mb: number; max_mb: number;
}
interface PagefileStatus {
  physical_memory_mb: number; total_virtual_memory_mb: number; pagefile_size_mb: number;
  drives: PagefileDrive[]; recommendation: PagefileRecommendation;
}
interface PagefileResult { success: boolean; message: string; requires_restart: boolean; }

const PAGE_FILE_MODES = ["none", "system", "custom"] as const;

function formatMemory(mb: number): string {
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1)} GB`;
  }
  return `${mb} MB`;
}

// 从页面文件路径中提取盘符（如 "C:\pagefile.sys" -> "C:"），失败则返回原路径
function getDriveLabel(path: string): string {
  const m = path.match(/^([A-Za-z]:)/);
  return m ? m[1].toUpperCase() : path;
}

const store = new LazyStore("auto-clean.json");

export default function MemoryCleanupPage() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const navigate = useNavigate();
  const toast = useDynamicIsland("memory");

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const labelColor = useColorModeValue("gray.600", "#ffffff");
  const trackBg = useColorModeValue("gray.200", "#333333");

  const [memoryData, setMemoryData] = useState<MemoryData | null>(null);
  const [loading, setLoading] = useState(true);
  const [cleaning, setCleaning] = useState(false);
  const [selectedItems, setSelectedItems] = useState<string[]>([]);
  const [listSizes, setListSizes] = useState<MemoryListSizes | null>(null);

  const [autoClean, setAutoClean] = useState(false);
  const [autoInterval, setAutoInterval] = useState("300");
  const [autoThreshold, setAutoThreshold] = useState(12288);
  const [gameStartClean, setGameStartClean] = useState(false);

  const [pagefile, setPagefile] = useState<PagefileStatus | null>(null);
  const [pagefileLoading, setPagefileLoading] = useState(true);
  // 按盘编辑：每个盘独立模式与初始/最大
  const [drives, setDrives] = useState<PagefileDrive[]>([]);
  const [applyingDriveIdx, setApplyingDriveIdx] = useState<number | null>(null);

  const intervalOptions = [
    { value: "300", label: t("optimization.memoryCleanup.interval5min", "5分钟") },
    { value: "600", label: t("optimization.memoryCleanup.interval10min", "10分钟") },
    { value: "1800", label: t("optimization.memoryCleanup.interval30min", "30分钟") },
    { value: "3600", label: t("optimization.memoryCleanup.interval1hour", "1小时") },
  ];

  // 可勾选的清理项定义（sizeKey 对应后端 get_memory_list_sizes 返回的字段，null 表示不展示容量）
  const cleanItemDefs: {
    key: string; label: string; desc: string;
    sizeKey: keyof Omit<MemoryListSizes, "available"> | null;
  }[] = [
    {
      key: "standby",
      label: t("optimization.memoryCleanup.itemStandby", "待机列表"),
      desc: t("optimization.memoryCleanup.itemStandbyDesc", "SuperFetch 预读缓存，回收最安全"),
      sizeKey: "standby_mb",
    },
    {
      key: "file_cache",
      label: t("optimization.memoryCleanup.itemFileCache", "系统文件缓存"),
      desc: t("optimization.memoryCleanup.itemFileCacheDesc", "压低缓存上限强制回收文件页"),
      sizeKey: null,
    },
    {
      key: "low_pri_standby",
      label: t("optimization.memoryCleanup.itemLowPriStandby", "低优先级待机"),
      desc: t("optimization.memoryCleanup.itemLowPriStandbyDesc", "仅清理 0 优先级待机页"),
      sizeKey: null,
    },
    {
      key: "modified",
      label: t("optimization.memoryCleanup.itemModified", "修改页面列表"),
      desc: t("optimization.memoryCleanup.itemModifiedDesc", "脏页写盘后回收，触发磁盘 I/O"),
      sizeKey: "modified_mb",
    },
    {
      key: "combined",
      label: t("optimization.memoryCleanup.itemCombined", "组合内存"),
      desc: t("optimization.memoryCleanup.itemCombinedDesc", "物理内存页去重，Win10+ 可用"),
      sizeKey: "combined_mb",
    },
    {
      key: "registry",
      label: t("optimization.memoryCleanup.itemRegistry", "注册表缓存"),
      desc: t("optimization.memoryCleanup.itemRegistryDesc", "注册表预读缓存，Win8.1+ 可用"),
      sizeKey: null,
    },
    {
      key: "working_set",
      label: t("optimization.memoryCleanup.itemWorkingSet", "进程工作集"),
      desc: t("optimization.memoryCleanup.itemWorkingSetDesc", "逐进程收紧，系统进程与游戏自动跳过"),
      sizeKey: null,
    },
  ];

  const defaultItems = ["standby", "file_cache", "low_pri_standby"];

  const fetchMemoryData = useCallback(async () => {
    try {
      const data = await invoke<MemoryData>("get_detailed_memory_status");
      setMemoryData(data);
    } catch (error) {
      console.error("Failed to fetch memory data:", error);
    } finally {
      setLoading(false);
    }
    try {
      const sizes = await invoke<MemoryListSizes>("get_memory_list_sizes");
      setListSizes(sizes);
    } catch (error) {
      console.error("Failed to fetch memory list sizes:", error);
    }
  }, []);

  // Load auto-clean config from store
  useEffect(() => {
    (async () => {
      try {
        const enabled = await store.get<boolean>("auto-clean-enabled");
        const interval = await store.get<string>("auto-clean-interval");
        const threshold = await store.get<number>("auto-clean-threshold");

        if (enabled !== null && enabled !== undefined) setAutoClean(enabled);
        if (interval) setAutoInterval(interval);
        if (threshold !== null && threshold !== undefined) setAutoThreshold(threshold);
      } catch (error) {
        console.error("Failed to load auto-clean config:", error);
      }
    })();

    // 加载「游戏启动时自动清理内存」开关（后端持久化）
    (async () => {
      try {
        const cfg = await invoke<GameStartCleanConfig>("get_game_start_clean_config");
        setGameStartClean(cfg.enabled);
      } catch (error) {
        console.error("Failed to load game-start-clean config:", error);
      }
    })();

    // 加载内存清理勾选项（后端持久化）
    (async () => {
      try {
        const cfg = await invoke<MemoryCleanConfig>("get_memory_clean_config");
        const items = cfg.items && cfg.items.length > 0 ? cfg.items : defaultItems;
        setSelectedItems(items);
      } catch (error) {
        console.error("Failed to load memory clean config:", error);
        setSelectedItems(defaultItems);
      }
    })();
  }, []);

  useEffect(() => {
    fetchMemoryData();
    const interval = setInterval(fetchMemoryData, 2000);
    return () => clearInterval(interval);
  }, [fetchMemoryData]);

  // Start/stop auto-clean when switch changes
  useEffect(() => {
    (async () => {
      try {
        if (autoClean) {
          await invoke("start_auto_clean", {
            config: {
              enabled: autoClean,
              interval_seconds: parseInt(autoInterval),
              threshold_mb: autoThreshold,
              clean_type: "items",
            },
          });
        } else {
          await invoke("stop_auto_clean");
        }
      } catch (error) {
        console.error("Auto-clean toggle error:", error);
      }
    })();
  }, [autoClean]);

  const restartAutoClean = useCallback(async () => {
    try {
      await invoke("stop_auto_clean");
      if (autoClean) {
        await invoke("start_auto_clean", {
          config: {
            enabled: autoClean,
            interval_seconds: parseInt(autoInterval),
            threshold_mb: autoThreshold,
            clean_type: "items",
          },
        });
      }
    } catch (error) {
      console.error("Failed to restart auto-clean:", error);
    }
  }, [autoClean, autoInterval, autoThreshold]);

  const handleAutoCleanChange = async (enabled: boolean) => {
    setAutoClean(enabled);
    await store.set("auto-clean-enabled", enabled);
    await store.save();
  };

  const handleGameStartCleanChange = async (enabled: boolean) => {
    setGameStartClean(enabled);
    try {
      const cfg = await invoke<GameStartCleanConfig>("set_game_start_clean_config", { enabled });
      setGameStartClean(cfg.enabled);
    } catch (error) {
      console.error("game-start-clean toggle error:", error);
    }
  };

  const handleIntervalChange = async (value: string) => {
    setAutoInterval(value);
    await store.set("auto-clean-interval", value);
    await store.save();
    await restartAutoClean();
  };

  const handleThresholdChange = async (value: number) => {
    setAutoThreshold(value);
    await store.set("auto-clean-threshold", value);
    await store.save();
  };

  const handleThresholdChangeEnd = async (value: number) => {
    await restartAutoClean();
  };

  // ---- 虚拟内存（页面文件）----
  const loadPagefile = useCallback(async () => {
    try {
      const data = await invoke<PagefileStatus>("get_pagefile_status");
      setPagefile(data);
      // 每个盘独立模式与大小
      setDrives(data.drives.map((d) => ({ ...d })));
    } catch (error) {
      console.error("Failed to load pagefile status:", error);
      toast({
        title: t("optimization.error"),
        description: t("optimization.memoryCleanup.pagefile.loadError"),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setPagefileLoading(false);
    }
  }, [t, toast]);

  useEffect(() => {
    loadPagefile();
  }, [loadPagefile]);

  const updateDrive = (index: number, patch: Partial<PagefileDrive>) => {
    setDrives((prev) => prev.map((d, i) => (i === index ? { ...d, ...patch } : d)));
  };

  // 仅应用指定盘符的设置（其它盘保持不变）
  const applyDrive = async (idx: number) => {
    const d = drives[idx];
    if (!d || applyingDriveIdx !== null) return;
    if (d.mode === "custom") {
      if (isNaN(d.initial_mb) || d.initial_mb <= 0 || isNaN(d.max_mb) || d.max_mb < d.initial_mb) {
        toast({
          title: t("optimization.error"),
          description: t("optimization.memoryLimit.invalidInput"),
          status: "warning",
          duration: 3000,
          isClosable: true,
        });
        return;
      }
    }
    setApplyingDriveIdx(idx);
    try {
      const result = await invoke<PagefileResult>("set_pagefile", {
        drives: [{
          path: d.path,
          drive: d.drive,
          mode: d.mode,
          initial_mb: d.mode === "custom" ? d.initial_mb : 0,
          max_mb: d.mode === "custom" ? d.max_mb : 0,
        }],
      });
      if (result.success) {
        toast({
          title: t("optimization.memoryCleanup.pagefile.apply"),
          description: `${result.message}\n${t("optimization.memoryCleanup.pagefile.needsRestart")}`,
          status: "success",
          duration: 7000,
          isClosable: true,
        });
        await loadPagefile();
      }
    } catch (error) {
      toast({
        title: t("optimization.error"),
        description: t("optimization.memoryCleanup.pagefile.applyError") + "：" + String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setApplyingDriveIdx(null);
    }
  };

  // 仅将指定盘符设为推荐大小（自定义模式），其它盘保持不变
  const applyDriveRecommended = async (idx: number) => {
    const d = drives[idx];
    if (!d || !pagefile || applyingDriveIdx !== null) return;
    setApplyingDriveIdx(idx);
    try {
      const recommended = pagefile.recommendation;
      const result = await invoke<PagefileResult>("set_pagefile", {
        drives: [{
          path: d.path,
          drive: d.drive,
          mode: "custom",
          initial_mb: recommended.suggested_initial_mb,
          max_mb: recommended.suggested_max_mb,
        }],
      });
      if (result.success) {
        toast({
          title: t("optimization.memoryCleanup.pagefile.setRecommended"),
          description: `${result.message}\n${t("optimization.memoryCleanup.pagefile.needsRestart")}`,
          status: "success",
          duration: 7000,
          isClosable: true,
        });
        await loadPagefile();
      }
    } catch (error) {
      toast({
        title: t("optimization.error"),
        description: t("optimization.memoryCleanup.pagefile.applyError") + "：" + String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setApplyingDriveIdx(null);
    }
  };

  // Stop auto-clean on unmount
  useEffect(() => {
    return () => {
      invoke("stop_auto_clean").catch(() => {});
    };
  }, []);

  // 勾选/取消勾选清理项，变更持久化到后端
  const handleItemToggle = async (key: string, checked: boolean) => {
    const next = checked
      ? [...selectedItems, key]
      : selectedItems.filter((i) => i !== key);
    setSelectedItems(next);
    try {
      await invoke("set_memory_clean_config", { items: next });
    } catch (error) {
      console.error("Failed to save memory clean config:", error);
    }
  };

  // 全选 / 清空
  const toggleSelectAll = async () => {
    const next =
      selectedItems.length === cleanItemDefs.length
        ? []
        : cleanItemDefs.map((d) => d.key);
    setSelectedItems(next);
    try {
      await invoke("set_memory_clean_config", { items: next });
    } catch (error) {
      console.error("Failed to save memory clean config:", error);
    }
  };

  // 按勾选项执行清理
  const handleCleanSelected = async () => {
    if (selectedItems.length === 0) {
      toast({
        title: t("optimization.memoryCleanup.cleanAll"),
        description: t("optimization.memoryCleanup.noItemSelected", "请先勾选要清理的项目"),
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    setCleaning(true);
    try {
      const result = await invoke<CleanupResult>("clean_memory_selected", {
        items: selectedItems,
      });
      await fetchMemoryData();
      toast({
        title: t("optimization.memoryCleanup.cleanAll"),
        description:
          result.freed_mb > 0
            ? t("optimization.memoryCleanup.freedMemory", { size: result.freed_mb })
            : t("optimization.memoryCleanup.noMemoryFreed"),
        status: result.freed_mb > 0 ? "success" : "info",
        duration: 4000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("optimization.memoryCleanup.error"),
        description: String(error),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setCleaning(false);
    }
  };

  const getUsagePercent = (used: number, total: number): number => {
    if (total <= 0) return 0;
    return Math.round((used / total) * 100);
  };

  const getProgressColor = (percent: number): string => {
    if (percent < 60) return "green";
    if (percent < 85) return "yellow";
    return "red";
  };

  const renderMemoryCard = (
    icon: React.ReactNode,
    title: string,
    used: number,
    available: number,
    total: number
  ) => {
    const percent = getUsagePercent(used, total);
    const progressColor = getProgressColor(percent);

    return (
      <LiquidGlassCard w="full" p={5}>
        <HStack mb={4} spacing={3}>
          <Box color={themeConfig.primaryColor}>{icon}</Box>
          <Text fontWeight="bold" color={headingColor} fontSize="md">
            {title}
          </Text>
        </HStack>
        <Progress
          value={percent}
          size="sm"
          colorScheme={progressColor}
          borderRadius="full"
          mb={3}
          bg={trackBg}
        />
        <SimpleGrid columns={3} spacing={2}>
          <VStack align="center" spacing={0}>
            <Text fontSize="xs" color={subTextColor}>
              {t("optimization.memoryCleanup.used")}
            </Text>
            <Text fontSize="sm" fontWeight="bold" color={textColor}>
              {formatMemory(used)}
            </Text>
          </VStack>
          <VStack align="center" spacing={0}>
            <Text fontSize="xs" color={subTextColor}>
              {t("optimization.memoryCleanup.available")}
            </Text>
            <Text fontSize="sm" fontWeight="bold" color="green.400">
              {formatMemory(available)}
            </Text>
          </VStack>
          <VStack align="center" spacing={0}>
            <Text fontSize="xs" color={subTextColor}>
              {t("optimization.memoryCleanup.total")}
            </Text>
            <Text fontSize="sm" fontWeight="bold" color={labelColor}>
              {formatMemory(total)}
            </Text>
          </VStack>
        </SimpleGrid>
      </LiquidGlassCard>
    );
  };

  return (
    <Box pt={8}>
      <VStack align="start" spacing={6}>
        {/* Top bar: back button + cleanup buttons */}
          <HStack justifyContent="space-between" alignItems="center" w="full">
            <Button
              variant="ghost"
              leftIcon={<ArrowLeft size={18} />}
              onClick={() => navigate("/optimize")}
              color={headingColor}
            >
                            返回
            </Button>
            <HStack spacing={3}>
              <Button
                variant="ghost"
                size="sm"
                onClick={toggleSelectAll}
                color={labelColor}
                borderRadius="xl"
              >
                {selectedItems.length === cleanItemDefs.length
                  ? t("optimization.memoryCleanup.clearAll", "清空")
                  : t("optimization.memoryCleanup.selectAll", "全选")}
              </Button>
              <Button
                bg={themeConfig.primaryColor}
                color={getContrastTextColor()}
                size="sm"
                onClick={handleCleanSelected}
                isLoading={cleaning}
                loadingText={t("optimization.memoryCleanup.cleaning")}
                borderRadius="xl"
                fontWeight="600"
                _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
                _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
              >
                {t("optimization.memoryCleanup.cleanSelected", "清理所选")}
              </Button>
            </HStack>
          </HStack>

          {loading ? (
            <Text color={subTextColor} textAlign="center" w="full" py={8}>
              {t("optimization.memoryCleanup.loading")}
            </Text>
          ) : (
            memoryData && (
              <>
                {/* Memory usage cards */}
                <SimpleGrid columns={{ base: 1, md: 3 }} spacing={4} w="full">
                  {renderMemoryCard(
                    <MemoryStick size={22} />,
                    t("optimization.memoryCleanup.physicalMemory"),
                    memoryData.physical_used,
                    memoryData.physical_available,
                    memoryData.physical_total
                  )}
                  {renderMemoryCard(
                    <HardDrive size={22} />,
                    t("optimization.memoryCleanup.virtualMemory"),
                    memoryData.virtual_used,
                    memoryData.virtual_available,
                    memoryData.virtual_total
                  )}
                  {renderMemoryCard(
                    <Cpu size={22} />,
                    t("optimization.memoryCleanup.workingSet"),
                    memoryData.working_set_used,
                    memoryData.working_set_available,
                    memoryData.working_set_total
                  )}
                </SimpleGrid>

                {/* Clean items selection card */}
                <LiquidGlassCard w="full" p={5}>
                  <VStack align="start" spacing={4} w="full">
                    <HStack justify="space-between" w="full">
                      <Box>
                        <Text fontWeight="bold" color={headingColor} fontSize="md">
                          {t("optimization.memoryCleanup.cleanItems", "清理项")}
                        </Text>
                        <Text fontSize="xs" color={subTextColor}>
                          {t("optimization.memoryCleanup.cleanItemsHint", "勾选要清理的内存区域，定时与游戏启动清理将跟随勾选项")}
                        </Text>
                      </Box>
                    </HStack>
                    {listSizes && !listSizes.available && (
                      <HStack
                        w="full"
                        p={3}
                        bg="rgba(255, 165, 0, 0.08)"
                        borderLeft="4px solid"
                        borderColor="orange.400"
                        borderRadius="lg"
                        spacing={2}
                      >
                        <Text fontSize="xs" color="orange.300">
                          {t("optimization.memoryCleanup.adminHint", "以管理员权限运行可显示各区域实时容量，且清理才完全生效")}
                        </Text>
                      </HStack>
                    )}
                    <SimpleGrid columns={{ base: 1, md: 2 }} spacing={3} w="full">
                      {cleanItemDefs.map((item) => {
                        const checked = selectedItems.includes(item.key);
                        const sizeMb =
                          item.sizeKey && listSizes ? listSizes[item.sizeKey] : 0;
                        return (
                          <HStack
                            key={item.key}
                            p={3}
                            borderWidth="1px"
                            borderColor={checked ? themeConfig.primaryColor : cardBorder}
                            borderRadius="xl"
                            justify="space-between"
                            spacing={3}
                            cursor="pointer"
                            onClick={() => handleItemToggle(item.key, !checked)}
                            _hover={{ borderColor: themeConfig.primaryColor }}
                          >
                            <HStack spacing={3} flex={1} minW={0}>
                              <Checkbox
                                isChecked={checked}
                                onChange={(e) => handleItemToggle(item.key, e.target.checked)}
                                onClick={(e) => e.stopPropagation()}
                                sx={{
                                  ".chakra-checkbox__control": {
                                    borderColor: cardBorder,
                                    _checked: {
                                      bg: themeConfig.primaryColor,
                                      borderColor: themeConfig.primaryColor,
                                    },
                                  },
                                }}
                              />
                              <VStack align="start" spacing={0} minW={0}>
                                <Text
                                  fontSize="sm"
                                  fontWeight="600"
                                  color={textColor}
                                  noOfLines={1}
                                >
                                  {item.label}
                                </Text>
                                <Text fontSize="xs" color={subTextColor} noOfLines={1}>
                                  {item.desc}
                                </Text>
                              </VStack>
                            </HStack>
                            {item.sizeKey && listSizes && (
                              <Text
                                fontSize="sm"
                                fontWeight="600"
                                color={sizeMb > 0 ? "green.400" : subTextColor}
                                whiteSpace="nowrap"
                              >
                                {listSizes.available ? formatMemory(sizeMb) : "--"}
                              </Text>
                            )}
                          </HStack>
                        );
                      })}
                    </SimpleGrid>
                  </VStack>
                </LiquidGlassCard>

                {/* Scheduled cleanup card */}
                <LiquidGlassCard w="full" p={5}>
                  <VStack align="start" spacing={4} w="full">
                    <HStack justify="space-between" w="full">
                      <Text fontWeight="bold" color={headingColor} fontSize="md">
                        {t("optimization.memoryCleanup.scheduledCleanup", "定时清理")}
                      </Text>
                      <Switch
                        isChecked={autoClean}
                        onChange={(e) => handleAutoCleanChange(e.target.checked)}
                        sx={{
                          "span.chakra-switch__track": {
                            bg: autoClean ? themeConfig.primaryColor : undefined,
                          },
                        }}
                      />
                    </HStack>

                    <HStack justify="space-between" w="full">
                      <Text fontWeight="bold" color={headingColor} fontSize="md">
                        {t("optimization.memoryCleanup.cleanOnGameStart", "游戏启动时自动清理内存")}
                      </Text>
                      <Switch
                        isChecked={gameStartClean}
                        onChange={(e) => handleGameStartCleanChange(e.target.checked)}
                        sx={{
                          "span.chakra-switch__track": {
                            bg: gameStartClean ? themeConfig.primaryColor : undefined,
                          },
                        }}
                      />
                    </HStack>

                    <SimpleGrid columns={{ base: 1, md: 3 }} spacing={4} w="full">
                      {/* Interval selector */}
                      <VStack align="start" spacing={1}>
                        <Text fontSize="sm" color={labelColor}>
                          {t("optimization.memoryCleanup.cleanInterval", "清理间隔")}
                        </Text>
                        <CustomSelect
                          value={autoInterval}
                          onChange={handleIntervalChange}
                          options={intervalOptions}
                          width="full"
                        />
                      </VStack>

                      {/* Memory threshold slider */}
                      <VStack align="start" spacing={1}>
                        <Text fontSize="sm" color={labelColor}>
                          {t("optimization.memoryCleanup.memoryThreshold", "内存阈值")}
                        </Text>
                        <HStack w="full" spacing={3}>
                          <Slider
                            flex={1}
                            value={autoThreshold}
                            min={4096}
                            max={32768}
                            step={1024}
                            onChange={handleThresholdChange}
                            onChangeEnd={handleThresholdChangeEnd}
                          >
                            <SliderTrack bg={trackBg}>
                              <SliderFilledTrack bg={themeConfig.primaryColor} />
                            </SliderTrack>
                            <SliderThumb boxSize={4} />
                          </Slider>
                          <Text fontSize="sm" fontWeight="bold" color={textColor} minW="60px" textAlign="right">
                            {Math.round(autoThreshold / 1024)} GB
                          </Text>
                        </HStack>
                      </VStack>

                      {/* Clean type: follows selection */}
                      <VStack align="start" spacing={1}>
                        <Text fontSize="sm" color={labelColor}>
                          {t("optimization.memoryCleanup.cleanType", "清理类型")}
                        </Text>
                        <HStack
                          w="full"
                          p={3}
                          bg={trackBg}
                          borderRadius="xl"
                          minH="40px"
                          align="center"
                        >
                          <Text fontSize="sm" color={textColor}>
                            {t("optimization.memoryCleanup.cleanTypeFollowsSelection", "随上方勾选项")}
                          </Text>
                        </HStack>
                      </VStack>
                    </SimpleGrid>
                  </VStack>
                </LiquidGlassCard>

                {/* Virtual memory (page file) settings card */}
                <LiquidGlassCard w="full" p={5}>
                  <VStack align="start" spacing={4} w="full">
                    <HStack justify="space-between" w="full">
                      <Box>
                        <Text fontWeight="bold" color={headingColor} fontSize="md">
                          {t("optimization.memoryCleanup.pagefile.title", "虚拟内存（页面文件）设置")}
                        </Text>
                        <Text fontSize="xs" color={subTextColor}>
                          {t("optimization.memoryCleanup.pagefile.description")}
                        </Text>
                      </Box>
                      <Box color={themeConfig.primaryColor}><Settings size={22} /></Box>
                    </HStack>

                    {pagefileLoading ? (
                      <Text color={subTextColor}>{t("optimization.memoryCleanup.loading")}</Text>
                    ) : (
                      pagefile && (
                        <>
                          {/* Current status */}
                          <VStack align="start" spacing={2} w="full">
                            <Text fontWeight="600" color={textColor} fontSize="sm">
                              {t("optimization.memoryCleanup.pagefile.currentStatus")}
                            </Text>
                            <SimpleGrid columns={2} spacing={2} w="full">
                              <HStack justify="space-between" w="full">
                                <Text color={subTextColor} fontSize="sm">
                                  {t("optimization.memoryCleanup.pagefile.physicalMemory")}
                                </Text>
                                <Text color={textColor} fontWeight="600" fontSize="sm">
                                  {formatMemory(pagefile.physical_memory_mb)}
                                </Text>
                              </HStack>
                              <HStack justify="space-between" w="full">
                                <Text color={subTextColor} fontSize="sm">
                                  {t("optimization.memoryCleanup.pagefile.totalVirtualMemory")}
                                </Text>
                                <Text color={textColor} fontWeight="600" fontSize="sm">
                                  {formatMemory(pagefile.total_virtual_memory_mb)}
                                </Text>
                              </HStack>
                              <HStack justify="space-between" w="full">
                                <Text color={subTextColor} fontSize="sm">
                                  {t("optimization.memoryCleanup.pagefile.currentPagefile")}
                                </Text>
                                <Text color={textColor} fontWeight="600" fontSize="sm">
                                  {formatMemory(pagefile.pagefile_size_mb)}
                                </Text>
                              </HStack>
                              <HStack justify="space-between" w="full">
                                <Text color={subTextColor} fontSize="sm">
                                  {t("optimization.memoryCleanup.pagefile.currentPagefile")}
                                </Text>
                                <Text color={textColor} fontWeight="600" fontSize="sm">
                                  {formatMemory(pagefile.pagefile_size_mb)}
                                </Text>
                              </HStack>
                            </SimpleGrid>
                          </VStack>

                          {/* 按磁盘逐盘设置：每个盘一个当前模式 */}
                          <VStack align="start" spacing={3} w="full">
                            <Text fontWeight="600" color={textColor} fontSize="sm">
                              {t("optimization.memoryCleanup.pagefile.perDrive", "按磁盘设置")}
                            </Text>
                            {drives.map((d, idx) => (
                              <VStack key={d.drive} align="start" spacing={2} w="full">
                                <HStack justify="space-between" w="full">
                                  <HStack spacing={2} align="center">
                                    <Text fontWeight="700" color={headingColor} fontSize="md">
                                      {getDriveLabel(d.path)}
                                    </Text>
                                    <Badge
                                      bg={d.mode === "custom" ? themeConfig.primaryColor : d.mode === "none" ? "#FF6B9D" : trackBg}
                                      color={d.mode === "none" ? "#1a1a1a" : getContrastTextColor()}
                                      fontSize="xs"
                                      px={2}
                                      py={0.5}
                                      borderRadius="full"
                                      fontWeight="600"
                                    >
                                      {t(`optimization.memoryCleanup.pagefile.mode${d.mode === "none" ? "None" : d.mode === "custom" ? "Custom" : "System"}`)}
                                    </Badge>
                                  </HStack>
                                  <HStack spacing={1}>
                                    {PAGE_FILE_MODES.map((mode) => {
                                      const active = d.mode === mode;
                                      return (
                                        <Button
                                          key={mode}
                                          size="sm"
                                          borderRadius="full"
                                          bg={active ? themeConfig.primaryColor : undefined}
                                          color={active ? getContrastTextColor() : labelColor}
                                          border={active ? "none" : "1px solid"}
                                          borderColor={cardBorder}
                                          fontSize="sm"
                                          height="32px"
                                          _hover={{ bg: active ? themeConfig.primaryColor : trackBg }}
                                          onClick={() => updateDrive(idx, { mode })}
                                        >
                                          {t(`optimization.memoryCleanup.pagefile.mode${mode === "none" ? "None" : mode === "custom" ? "Custom" : "System"}`)}
                                        </Button>
                                      );
                                    })}
                                  </HStack>
                                </HStack>

                                {d.mode === "custom" && (
                                  <SimpleGrid columns={2} spacing={3} w="full">
                                    <VStack align="start" spacing={1}>
                                      <Text fontSize="xs" color={subTextColor}>
                                        {t("optimization.memoryCleanup.pagefile.initialSize")}
                                      </Text>
                                      <Input
                                        value={d.initial_mb === 0 ? "" : String(d.initial_mb)}
                                        onChange={(e) => updateDrive(idx, { initial_mb: e.target.value === "" ? 0 : Number(e.target.value) })}
                                        placeholder="4096"
                                        color={headingColor}
                                        borderColor={cardBorder}
                                        _focus={{ borderColor: themeConfig.primaryColor }}
                                        type="text"
                                        inputMode="numeric"
                                      />
                                    </VStack>
                                    <VStack align="start" spacing={1}>
                                      <Text fontSize="xs" color={subTextColor}>
                                        {t("optimization.memoryCleanup.pagefile.maxSize")}
                                      </Text>
                                      <Input
                                        value={d.max_mb === 0 ? "" : String(d.max_mb)}
                                        onChange={(e) => updateDrive(idx, { max_mb: e.target.value === "" ? 0 : Number(e.target.value) })}
                                        placeholder="8192"
                                        color={headingColor}
                                        borderColor={cardBorder}
                                        _focus={{ borderColor: themeConfig.primaryColor }}
                                        type="text"
                                        inputMode="numeric"
                                      />
                                    </VStack>
                                  </SimpleGrid>
                                )}

                                {/* 该盘独立的 应用 / 设为推荐 按钮 */}
                                <HStack spacing={2} w="full">
                                  <Button
                                    bg={themeConfig.primaryColor}
                                    color={getContrastTextColor()}
                                    size="sm"
                                    flex={1}
                                    onClick={() => applyDrive(idx)}
                                    isLoading={applyingDriveIdx === idx}
                                    loadingText={t("optimization.optimizing")}
                                    borderRadius="xl"
                                    fontWeight="600"
                                    fontSize="sm"
                                    height="36px"
                                    _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
                                    _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
                                  >
                                    {t("optimization.memoryCleanup.pagefile.apply")}
                                  </Button>
                                  <Button
                                    colorScheme="blue"
                                    variant="outline"
                                    size="sm"
                                    flex={1}
                                    onClick={() => applyDriveRecommended(idx)}
                                    isLoading={applyingDriveIdx === idx}
                                    loadingText={t("optimization.optimizing")}
                                    borderRadius="xl"
                                    fontSize="sm"
                                    height="36px"
                                  >
                                    {t("optimization.memoryCleanup.pagefile.setRecommended")}
                                  </Button>
                                </HStack>
                              </VStack>
                            ))}
                          </VStack>

                          {/* Recommendation banner */}
                          <Alert
                            status={pagefile.recommendation.verdict === "low" ? "warning" : pagefile.recommendation.verdict === "high" ? "error" : "success"}
                            borderRadius="xl"
                            bg={useColorModeValue("orange.50", "rgba(255, 165, 0, 0.1)")}
                            borderLeft="4px solid"
                            borderColor={pagefile.recommendation.verdict === "low" ? "orange.400" : pagefile.recommendation.verdict === "high" ? "red.400" : "green.400"}
                          >
                            <AlertIcon color={pagefile.recommendation.verdict === "low" ? "orange.500" : pagefile.recommendation.verdict === "high" ? "red.500" : "green.500"} />
                            <AlertDescription color={textColor} fontSize="sm">
                              <strong>{t("optimization.memoryCleanup.pagefile.recommendation")}:</strong>{" "}
                              {t(`optimization.memoryCleanup.pagefile.verdict${pagefile.recommendation.verdict === "low" ? "Low" : pagefile.recommendation.verdict === "high" ? "High" : "Ok"}`)}
                              {" "}— {pagefile.recommendation.message}
                            </AlertDescription>
                          </Alert>
                        </>
                      )
                    )}
                  </VStack>
                </LiquidGlassCard>
              </>
            )
          )}
        </VStack>
    </Box>
  );
}
