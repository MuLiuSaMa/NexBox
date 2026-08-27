import { useState, useEffect, useCallback, useRef, useMemo, memo } from "react";
import type { TFunction } from "i18next";
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
  useColorModeValue,
  Spinner,
  Tooltip,
  Badge,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { ArrowLeft, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { store } from "@/lib/store";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import {
  optimizerItems,
  categoryLabels,
  categoryOrder,
  type OptimizerItem,
  type OptimizerCategory,
} from "@/config/system-optimizer";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

const STORE_KEY = "system_optimizer_states";
const NOTICE_KEY = "system_optimizer_notice_agreed";
const NOTICE_COUNTDOWN_SECONDS = 5;

// ============ 模块级 memo 组件（避免因父组件重渲染导致卸载重挂载，保证 Switch 动画正常） ============

interface OptimizeCardProps {
  item: OptimizerItem;
  state: boolean | string;
  isToggling: boolean;
  onToggle: (item: OptimizerItem, enable: boolean) => void;
  onSelect: (item: OptimizerItem, value: string) => void;
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
  onSelect,
  headingColor,
  subTextColor,
  activeColor,
  cardBg,
  cardBorder,
  liquidGlassEnabled,
  t,
}: OptimizeCardProps) {
  const isSelect = item.type === "select";
  const isOptimized = state === true;
  const selectValue = isSelect && typeof state === "string" ? state : (item.defaultSelectValue ?? "");
  const IconComponent = item.icon;

  const content = (
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
          <HStack spacing={2} align="center" flexWrap="wrap">
            <Text
              color={headingColor}
              fontSize="sm"
              fontWeight="bold"
              noOfLines={1}
            >
              {t(item.titleKey)}
            </Text>
            {item.requiresReboot && (
              <Tooltip label={t("systemOptimizer.requiresReboot")}>
                <Badge
                  fontSize="9px"
                  colorScheme="orange"
                  variant="subtle"
                  borderRadius="full"
                  px={1.5}
                  lineHeight="1.2"
                >
                  REBOOT
                </Badge>
              </Tooltip>
            )}
          </HStack>
          <Text color={subTextColor} fontSize="xs" noOfLines={2} mt={0.5}>
            {t(item.descKey)}
          </Text>
        </Box>
      </HStack>
      {isSelect ? (
        <Box
          pointerEvents={isToggling ? "none" : "auto"}
          opacity={isToggling ? 0.5 : 1}
          flexShrink={0}
        >
          <CustomSelect
            value={selectValue}
            onChange={(v) => onSelect(item, v)}
            options={(item.options ?? []).map((opt) => ({
              value: opt.value,
              label: t(opt.labelKey),
            }))}
            width="140px"
          />
        </Box>
      ) : (
        <Switch
          isChecked={isOptimized}
          isDisabled={isToggling}
          onChange={() => onToggle(item, !isOptimized)}
          sx={{
            "& .chakra-switch__track[data-checked]": {
              bg: activeColor,
            },
          }}
          size="md"
        />
      )}
    </Flex>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard w="full" p={4}>
        {content}
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
    >
      {content}
    </Box>
  );
});

interface CategorySectionProps {
  category: OptimizerCategory;
  items: OptimizerItem[];
  savedStates: Record<string, boolean | string>;
  togglingItems: Set<string>;
  onToggle: (item: OptimizerItem, enable: boolean) => void;
  onSelect: (item: OptimizerItem, value: string) => void;
  headingColor: string;
  subTextColor: string;
  activeColor: string;
  cardBg: string;
  cardBorder: string;
  liquidGlassEnabled: boolean;
  t: TFunction;
}

const CategorySection = memo(function CategorySection({
  category,
  items,
  savedStates,
  togglingItems,
  onToggle,
  onSelect,
  headingColor,
  subTextColor,
  activeColor,
  cardBg,
  cardBorder,
  liquidGlassEnabled,
  t,
}: CategorySectionProps) {
  if (items.length === 0) return null;

  return (
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
        {t(categoryLabels[category])}
      </Heading>
      <SimpleGrid columns={{ base: 1, md: 2 }} spacing={3}>
        {items.map((item) => {
          const v = savedStates[item.id];
          const state = v !== undefined
            ? v
            : (item.type === "select" ? (item.defaultSelectValue ?? "") : false);
          return (
            <OptimizeCard
              key={item.id}
              item={item}
              state={state}
              isToggling={togglingItems.has(item.id)}
              onToggle={onToggle}
              onSelect={onSelect}
              headingColor={headingColor}
              subTextColor={subTextColor}
              activeColor={activeColor}
              cardBg={cardBg}
              cardBorder={cardBorder}
              liquidGlassEnabled={liquidGlassEnabled}
              t={t}
            />
          );
        })}
      </SimpleGrid>
    </Box>
  );
});

// 收集所有需要扫描的 reg 文件名（select 类型扫描其全部选项）
function collectScanNames(): string[] {
  const names: string[] = [];
  for (const item of optimizerItems) {
    if (item.type === "select") {
      for (const opt of item.options ?? []) names.push(opt.regName);
    } else {
      names.push(item.regName);
    }
  }
  return names;
}

// 由注册表扫描结果构建各优化项状态
function buildScanStates(results: { name: string; applied: boolean }[]) {
  const map = new Map(results.map((r) => [r.name, r.applied]));
  const states: Record<string, boolean | string> = {};
  let optimizedCount = 0;
  for (const item of optimizerItems) {
    if (item.type === "select") {
      const matched = (item.options ?? []).find((opt) => map.get(opt.regName) === true);
      const val = matched?.value ?? item.defaultSelectValue ?? "";
      states[item.id] = val;
      if (val !== item.defaultSelectValue) optimizedCount++;
    } else {
      const applied = map.get(item.regName) === true;
      states[item.id] = applied;
      if (applied) optimizedCount++;
    }
  }
  return { states, optimizedCount };
}

// ============ 页面主组件 ============

export default function SystemOptimizerPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { liquidGlassEnabled } = useBackground();
  const toast = useDynamicIsland("sparkles");
  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();

  const [savedStates, setSavedStatesState] = useState<Record<string, boolean | string>>({});
  const [isLoading, setIsLoading] = useState(true);
  const [isBatchOptimizing, setIsBatchOptimizing] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [togglingItems, setTogglingItems] = useState<Set<string>>(new Set());
  const [showNotice, setShowNotice] = useState(false);
  const [noticeCountdown, setNoticeCountdown] = useState(NOTICE_COUNTDOWN_SECONDS);

  // 用 ref 持有最新状态，确保 toggleItem/selectItem 引用稳定（配合 memo 生效）
  const savedStatesRef = useRef<Record<string, boolean | string>>({});
  const setSavedStates = useCallback((next: Record<string, boolean | string>) => {
    savedStatesRef.current = next;
    setSavedStatesState(next);
  }, []);

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);

  // 按分类缓存优化项（稳定引用，配合 memo 生效）
  const itemsByCategory = useMemo(() => {
    const map = {} as Record<OptimizerCategory, OptimizerItem[]>;
    for (const cat of categoryOrder) map[cat] = [];
    for (const item of optimizerItems) map[item.category].push(item);
    return map;
  }, []);

  // 初始化：并行加载保存状态 + 扫描注册表真实状态（每次进入页面都会自动扫描）
  useEffect(() => {
    let cancelled = false;
    async function init() {
      const startTime = Date.now();
      const [savedResult, noticeResult, scanResult] = await Promise.allSettled([
        store.get<Record<string, boolean | string>>(STORE_KEY),
        store.get<boolean>(NOTICE_KEY),
        invoke<{ name: string; applied: boolean }[]>("scan_registry_tweaks", {
          names: collectScanNames(),
        }),
      ]);
      if (cancelled) return;
      // 未确认过提示则弹出（保存到 appdata，确认一次后不再弹出）
      const noticeAgreed = noticeResult.status === "fulfilled" && noticeResult.value;
      if (!noticeAgreed) {
        setShowNotice(true);
        setNoticeCountdown(NOTICE_COUNTDOWN_SECONDS);
      }
      // 已保存的状态（扫描失败时的兜底）
      const saved = savedResult.status === "fulfilled" && savedResult.value
        ? savedResult.value
        : {};
      // 扫描结果优先；扫描失败时回退到保存的状态，保证开关仍能显示
      const scanned = scanResult.status === "fulfilled" && scanResult.value
        ? buildScanStates(scanResult.value).states
        : null;
      const finalStates = scanned ?? saved;
      // 确保 loading 至少显示 400ms
      const remaining = Math.max(0, 400 - (Date.now() - startTime));
      if (remaining > 0) {
        await new Promise((r) => setTimeout(r, remaining));
      }
      if (cancelled) return;
      setSavedStates(finalStates);
      // 扫描成功时把真实状态同步持久化，避免下次进入读到陈旧数据
      if (scanned) {
        try {
          await store.set(STORE_KEY, scanned);
          await store.save();
        } catch {}
      }
      setIsLoading(false);
    }
    init();
    return () => {
      cancelled = true;
    };
  }, [setSavedStates]);

  // 5 秒倒计时
  useEffect(() => {
    if (!showNotice || noticeCountdown <= 0) return;
    const timer = setInterval(() => {
      setNoticeCountdown((c) => Math.max(0, c - 1));
    }, 1000);
    return () => clearInterval(timer);
  }, [showNotice, noticeCountdown]);

  const handleNoticeConfirm = async () => {
    try {
      await store.set(NOTICE_KEY, true);
      await store.save();
    } catch {}
    setShowNotice(false);
  };

  // 保存状态到持久化存储
  const persistStates = useCallback(async (states: Record<string, boolean | string>) => {
    try {
      await store.set(STORE_KEY, states);
      await store.save();
    } catch {}
  }, []);

  // 扫描当前优化项真实状态（读取注册表与应用 .reg 比对）
  const handleScan = useCallback(
    async (showToast = true) => {
      setIsScanning(true);
      try {
        const results = await invoke<{ name: string; applied: boolean }[]>(
          "scan_registry_tweaks",
          { names: collectScanNames() },
        );
        const { states: newSaved, optimizedCount } = buildScanStates(results);
        setSavedStates(newSaved);
        persistStates(newSaved);
        if (showToast) {
          toast({
            title: t("systemOptimizer.scanComplete"),
            description: t("systemOptimizer.scanResult", {
              count: optimizedCount,
              total: optimizerItems.length,
            }),
            status: "success",
            duration: 2500,
            isClosable: true,
          });
        }
      } catch (err) {
        if (showToast) {
          toast({
            title: t("systemOptimizer.scanError"),
            description: String(err),
            status: "error",
            duration: 3000,
            isClosable: true,
          });
        }
      } finally {
        setIsScanning(false);
      }
    },
    [persistStates, setSavedStates, toast, t],
  );

  // 执行单个优化项（乐观更新：立即翻转开关状态以获得流畅动画，后台执行注册表写入）
  const toggleItem = useCallback(
    async (item: OptimizerItem, enable: boolean) => {
      const cmd = enable ? "apply_registry_tweak" : "restore_registry_tweak";
      setTogglingItems((prev) => new Set(prev).add(item.id));
      const prevVal = savedStatesRef.current[item.id];
      // 乐观更新开关状态，让过渡动画立即播放
      const optimistic = { ...savedStatesRef.current, [item.id]: enable };
      setSavedStates(optimistic);
      try {
        await invoke(cmd, { name: item.regName });
        persistStates(optimistic);
        toast({
          title: enable
            ? t("systemOptimizer.optimized")
            : t("systemOptimizer.reverted"),
          description: t(item.titleKey),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      } catch (err) {
        // 失败时回滚开关状态
        const rollback = { ...savedStatesRef.current, [item.id]: prevVal };
        setSavedStates(rollback);
        toast({
          title: t("systemOptimizer.operationError"),
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
    [persistStates, setSavedStates, toast, t],
  );

  // 执行 select 类型优化项（下拉切换时应用对应选项的 reg 文件）
  const selectItem = useCallback(
    async (item: OptimizerItem, value: string) => {
      const option = item.options?.find((opt) => opt.value === value);
      if (!option) return;
      setTogglingItems((prev) => new Set(prev).add(item.id));
      const prevVal = savedStatesRef.current[item.id];
      const optimistic = { ...savedStatesRef.current, [item.id]: value };
      setSavedStates(optimistic);
      try {
        await invoke("apply_registry_tweak", { name: option.regName });
        persistStates(optimistic);
        toast({
          title: t("systemOptimizer.optimized"),
          description: t(item.titleKey),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      } catch (err) {
        const rollback = { ...savedStatesRef.current, [item.id]: prevVal };
        setSavedStates(rollback);
        toast({
          title: t("systemOptimizer.operationError"),
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
    [persistStates, setSavedStates, toast, t],
  );

  // 全部优化
  const handleBatchEnable = useCallback(async () => {
    setIsBatchOptimizing(true);
    try {
      const names = optimizerItems.map((item) => {
        // select 类型应用默认选中值对应的 reg 文件
        if (item.type === "select") {
          const def = item.options?.find((opt) => opt.value === item.defaultSelectValue);
          return def?.regName ?? item.regName;
        }
        return item.regName;
      });
      await invoke("batch_apply_registry_tweaks", { names });
      const newSaved: Record<string, boolean | string> = {};
      for (const item of optimizerItems) {
        newSaved[item.id] = item.type === "select"
          ? (item.defaultSelectValue ?? "")
          : true;
      }
      setSavedStates(newSaved);
      persistStates(newSaved);
      toast({
        title: t("systemOptimizer.batchOptimized"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("systemOptimizer.batchError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsBatchOptimizing(false);
    }
  }, [persistStates, setSavedStates, toast, t]);

  // 全部恢复
  const handleBatchDisable = useCallback(async () => {
    setIsBatchOptimizing(true);
    try {
      const names = optimizerItems.map((item) => item.regName);
      await invoke("batch_restore_registry_tweaks", { names });
      const newSaved: Record<string, boolean | string> = {};
      for (const item of optimizerItems) {
        newSaved[item.id] = item.type === "select"
          ? (item.defaultSelectValue ?? "")
          : false;
      }
      setSavedStates(newSaved);
      persistStates(newSaved);
      toast({
        title: t("systemOptimizer.batchReverted"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: t("systemOptimizer.batchError"),
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsBatchOptimizing(false);
    }
  }, [persistStates, setSavedStates, toast, t]);

  // 加载中
  if (isLoading) {
    return (
      <Box pt={8}>
        {liquidGlassEnabled ? (
          <LiquidGlassCard w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
            <Flex w="full" minH="360px" align="center" justify="center" direction="column" gap={4}>
              <Spinner size="xl" color={activeColor} thickness="3px" />
              <Text color={subTextColor} fontSize="sm">
                {t("systemOptimizer.loading")}
              </Text>
            </Flex>
          </LiquidGlassCard>
        ) : (
          <Box bg={cardBg} borderRadius="xl" borderWidth="1px" borderColor={cardBorder} w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
            <Flex w="full" minH="360px" align="center" justify="center" direction="column" gap={4}>
              <Spinner size="xl" color={activeColor} thickness="3px" />
              <Text color={subTextColor} fontSize="sm">
                {t("systemOptimizer.loading")}
              </Text>
            </Flex>
          </Box>
        )}
      </Box>
    );
  }

  const content = (
    <VStack align="start" spacing={6}>
      {/* 标题和操作按钮 */}
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
            {t("systemOptimizer.pageTitle")}
          </Heading>
        </HStack>
        <HStack spacing={2}>
          <Button
            size="sm"
            onClick={() => handleScan(true)}
            isLoading={isScanning}
            loadingText={t("systemOptimizer.scanning")}
            leftIcon={<RefreshCw size={16} />}
            variant="outline"
            sx={{
              borderColor: activeColor,
              color: activeColor,
              _hover: { bg: hoverBg },
            }}
          >
            {t("systemOptimizer.scanStatus")}
          </Button>
          <Button
            size="sm"
            onClick={handleBatchEnable}
            isLoading={isBatchOptimizing}
            loadingText={t("systemOptimizer.optimizing")}
            bg={activeColor}
            color={contrastText}
            _hover={{ opacity: 0.9 }}
            _active={{ transform: "scale(0.97)" }}
          >
            {t("systemOptimizer.batchEnable")}
          </Button>
          <Button
            size="sm"
            onClick={handleBatchDisable}
            isLoading={isBatchOptimizing}
            loadingText={t("systemOptimizer.optimizing")}
            variant="outline"
            sx={{
              borderColor: activeColor,
              color: activeColor,
              _hover: { bg: hoverBg },
            }}
          >
            {t("systemOptimizer.batchDisable")}
          </Button>
        </HStack>
      </Flex>

      {/* 优化项分类列表 */}
      {categoryOrder.map((cat) => (
        <CategorySection
          key={cat}
          category={cat}
          items={itemsByCategory[cat]}
          savedStates={savedStates}
          togglingItems={togglingItems}
          onToggle={toggleItem}
          onSelect={selectItem}
          headingColor={headingColor}
          subTextColor={subTextColor}
          activeColor={activeColor}
          cardBg={cardBg}
          cardBorder={cardBorder}
          liquidGlassEnabled={liquidGlassEnabled}
          t={t}
        />
      ))}

      {/* 温馨提示弹窗 */}
      <Modal
        isOpen={showNotice}
        onClose={() => {}}
        isCentered
        closeOnOverlayClick={false}
        closeOnEsc={false}
      >
        <ModalOverlay />
        <ModalContent bg={cardBg} borderRadius="xl" borderWidth="1px" borderColor={cardBorder}>
          <ModalHeader color={headingColor} fontSize="lg">
            {t("systemOptimizer.noticeTitle")}
          </ModalHeader>
          <ModalBody>
            <Text
              color={subTextColor}
              fontSize="sm"
              lineHeight="taller"
              whiteSpace="pre-line"
            >
              {t("systemOptimizer.noticeContent")}
            </Text>
          </ModalBody>
          <ModalFooter>
            <Button
              bg={activeColor}
              color={contrastText}
              isDisabled={noticeCountdown > 0}
              onClick={handleNoticeConfirm}
              _hover={{ opacity: 0.9 }}
            >
              {t("systemOptimizer.noticeAgree")}
              {noticeCountdown > 0 ? ` (${noticeCountdown}s)` : ""}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
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
