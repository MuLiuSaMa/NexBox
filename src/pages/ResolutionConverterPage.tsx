import {
  Box,
  Flex,
  Text,
  VStack,
  HStack,
  useColorModeValue,
  SimpleGrid,
  useBreakpointValue,
  Button,
  Spinner,
  Badge,
  Input,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { Monitor } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { useState, useMemo, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { CustomSelect } from "@/components/special/custom-select";

type ResolutionType = "1K" | "1.5K" | "2K" | "2.5K" | "3K" | "4K" | "custom";

interface ResolutionInfo {
  width: number;
  height: number;
  ratio: string;
  ratioLabel: string;
}

interface NvidiaDisplay {
  display_id: number;
  device_name: string;
  monitor_name: string;
  is_primary: boolean;
  current_width: number;
  current_height: number;
}

interface DisplayMode {
  width: number;
  height: number;
  refresh_rate: number;
  is_current: boolean;
}

interface SetResolutionResult {
  applied: boolean;
  injected: boolean;
}

interface InjectedResolution {
  width: number;
  height: number;
}

const RESOLUTION_PRESETS: Record<Exclude<ResolutionType, "custom">, { width: number; height: number }> = {
  "1K": { width: 1920, height: 1080 },
  "1.5K": { width: 1920, height: 1200 },
  "2K": { width: 2560, height: 1440 },
  "2.5K": { width: 2560, height: 1600 },
  "3K": { width: 3200, height: 1800 },
  "4K": { width: 3840, height: 2160 },
};

const ASPECT_RATIOS = [
  { ratio: "16:9", widthRatio: 16, heightRatio: 9, color: "#4A90E2" },
  { ratio: "4:3", widthRatio: 4, heightRatio: 3, color: "#FF6B9D" },
  { ratio: "16:10", widthRatio: 16, heightRatio: 10, color: "#98DDD0" },
];

function calculateResolution(
  baseHeight: number,
  widthRatio: number,
  heightRatio: number
): number {
  return Math.round((baseHeight * widthRatio) / heightRatio);
}

function ResolutionCard({
  resolution,
  color,
  isActive,
  onApply,
  isApplying,
}: {
  resolution: ResolutionInfo;
  color: string;
  isActive: boolean;
  onApply?: () => void;
  isApplying?: boolean;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  const aspectRatioBox = useMemo(() => {
    const maxWidth = 120;
    const maxHeight = 80;
    const ratio = resolution.width / resolution.height;
    let boxWidth: number;
    let boxHeight: number;

    if (ratio > maxWidth / maxHeight) {
      boxWidth = maxWidth;
      boxHeight = maxWidth / ratio;
    } else {
      boxHeight = maxHeight;
      boxWidth = maxHeight * ratio;
    }

    return { width: boxWidth, height: boxHeight };
  }, [resolution.width, resolution.height]);

  const cardContent = (
    <VStack spacing={4} align="stretch">
      <HStack justify="space-between">
        <Text
          fontSize="sm"
          fontWeight="600"
          color={color}
          bg={`${color}20`}
          px={3}
          py={1}
          borderRadius="full"
        >
          {resolution.ratioLabel}
        </Text>
        {isActive && (
          <Text fontSize="xs" color={color} fontWeight="500">
            {t("resolutionConverter.standard")}
          </Text>
        )}
      </HStack>

      <VStack spacing={2}>
        <Text
          fontSize="2xl"
          fontWeight="bold"
          color={textColor}
          letterSpacing="tight"
        >
          {resolution.width} × {resolution.height}
        </Text>
        <HStack spacing={4} fontSize="sm" color={subTextColor}>
          <HStack spacing={1}>
            <Text>{t("resolutionConverter.width")}:</Text>
            <Text fontWeight="600" color={textColor}>
              {resolution.width}
            </Text>
          </HStack>
          <HStack spacing={1}>
            <Text>{t("resolutionConverter.height")}:</Text>
            <Text fontWeight="600" color={textColor}>
              {resolution.height}
            </Text>
          </HStack>
        </HStack>
      </VStack>

      <Flex justify="center" pt={2}>
        <Box
          width={`${aspectRatioBox.width}px`}
          height={`${aspectRatioBox.height}px`}
          border="2px solid"
          borderColor={color}
          borderRadius="md"
          position="relative"
          bg={`${color}10`}
        >
          <Box
            position="absolute"
            bottom={-6}
            left="50%"
            transform="translateX(-50%)"
            fontSize="xs"
            color={subTextColor}
            whiteSpace="nowrap"
          >
            {resolution.ratio}
          </Box>
        </Box>
      </Flex>

      {onApply && (
        <Button
          mt={2}
          size="sm"
          width="full"
          borderRadius="lg"
          bg={`${color}15`}
          color={color}
          border="1px solid"
          borderColor={`${color}40`}
          isLoading={isApplying}
          loadingText={t("resolutionConverter.applying")}
          onClick={onApply}
          _hover={{ bg: `${color}25` }}
          fontWeight="600"
          fontSize="sm"
        >
          {t("resolutionConverter.applyCustom")}
        </Button>
      )}
    </VStack>
  );

  return (
    <LiquidGlassCard
      p={6}
      minH="220px"
      position="relative"
      transition="all 0.2s"
      _hover={{
        transform: "translateY(-2px)",
      }}
    >
      {isActive && (
        <Box
          position="absolute"
          top={0}
          left={0}
          right={0}
          h="3px"
          bg={color}
        />
      )}
      {cardContent}
    </LiquidGlassCard>
  );
}

function ResolutionSelector({
  selected,
  onSelect,
}: {
  selected: ResolutionType;
  onSelect: (type: ResolutionType) => void;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const buttonBg = useColorModeValue("gray.100", "#222222");
  const activeBg = primaryColor;

  const options: { type: ResolutionType; label: string; subLabel: string }[] = [
    { type: "1K", label: t("resolutionConverter.resolution1K"), subLabel: "1920×1080" },
    { type: "1.5K", label: t("resolutionConverter.resolution1_5K"), subLabel: "1920×1200" },
    { type: "2K", label: t("resolutionConverter.resolution2K"), subLabel: "2560×1440" },
    { type: "2.5K", label: t("resolutionConverter.resolution2_5K"), subLabel: "2560×1600" },
    { type: "3K", label: t("resolutionConverter.resolution3K"), subLabel: "3200×1800" },
    { type: "4K", label: t("resolutionConverter.resolution4K"), subLabel: "3840×2160" },
    { type: "custom", label: t("resolutionConverter.custom"), subLabel: t("resolutionConverter.customPlaceholder") },
  ];

  return (
    <SimpleGrid columns={{ base: 2, md: 3, lg: 7 }} spacing={3} w="full">
      {options.map((option) => {
        const isActive = selected === option.type;
        return (
          <LiquidGlassCard
            key={option.type}
            p={4}
            cursor="pointer"
            onClick={() => onSelect(option.type)}
            _hover={{
              transform: "translateY(-2px)",
            }}
            position="relative"
          >
            {isActive && (
              <Box
                position="absolute"
                top={0}
                left={0}
                right={0}
                h="3px"
                bg={contrastText}
                opacity={0.3}
              />
            )}
            <VStack spacing={1}>
              <Monitor size={24} color={isActive ? activeBg : undefined} />
              <Text fontSize="md" fontWeight="600" color={isActive ? activeBg : textColor}>
                {option.label}
              </Text>
              <Text fontSize="xs" color={isActive ? activeBg : subTextColor}>
                {option.subLabel}
              </Text>
            </VStack>
          </LiquidGlassCard>
        );
      })}
    </SimpleGrid>
  );
}

export default function ResolutionConverterPage() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const toast = useDynamicIsland("monitor");
  const [selectedResolution, setSelectedResolution] = useState<ResolutionType>("1K");

  // 显示器分辨率管理状态
  const [displays, setDisplays] = useState<NvidiaDisplay[]>([]);
  const [selectedDisplayIdx, setSelectedDisplayIdx] = useState(0);
  const [displayModes, setDisplayModes] = useState<DisplayMode[]>([]);
  const [selectedResKey, setSelectedResKey] = useState<string>("");
  const [selectedRateKey, setSelectedRateKey] = useState<string>("");
  const [isApplyingResolution, setIsApplyingResolution] = useState(false);
  const [isLoadingModes, setIsLoadingModes] = useState(false);
  const [applyingResKey, setApplyingResKey] = useState<string>("");
  const [injectedResolutions, setInjectedResolutions] = useState<InjectedResolution[]>([]);

  // 自定义分辨率输入
  const [customWidth, setCustomWidth] = useState("");
  const [customHeight, setCustomHeight] = useState("");
  const [customRefreshRate, setCustomRefreshRate] = useState("");
  const [isApplyingCustom, setIsApplyingCustom] = useState(false);

  // 去重后的分辨率列表（按面积降序），用于两级联动下拉
  const availableResolutions = useMemo(() => {
    const seen = new Set<string>();
    const list: { key: string; width: number; height: number; label: string }[] = [];
    for (const m of displayModes) {
      const key = `${m.width}x${m.height}`;
      if (!seen.has(key)) {
        seen.add(key);
        list.push({ key, width: m.width, height: m.height, label: `${m.width} × ${m.height}` });
      }
    }
    list.sort((a, b) => b.width * b.height - a.width * a.height);
    return list;
  }, [displayModes]);

  // 当前所选分辨率下的刷新率列表
  const availableRates = useMemo(() => {
    if (!selectedResKey) return [];
    const [w, h] = selectedResKey.split("x").map(Number);
    const seen = new Set<string>();
    const list: { key: string; rate: number; label: string; isCurrent: boolean }[] = [];
    for (const m of displayModes) {
      if (m.width === w && m.height === h) {
        const key = (Math.round(m.refresh_rate * 10) / 10).toFixed(1);
        if (!seen.has(key)) {
          seen.add(key);
          list.push({
            key,
            rate: Math.round(m.refresh_rate * 10) / 10,
            label: `${key} Hz`,
            isCurrent: m.is_current,
          });
        }
      }
    }
    list.sort((a, b) => b.rate - a.rate);
    return list;
  }, [displayModes, selectedResKey]);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const resolutions = useMemo(() => {
    if (selectedResolution === "custom") return [];
    const base = RESOLUTION_PRESETS[selectedResolution];
    return ASPECT_RATIOS.map((aspect) => {
      const width = calculateResolution(
        base.height,
        aspect.widthRatio,
        aspect.heightRatio
      );
      return {
        width,
        height: base.height,
        ratio: aspect.ratio,
        ratioLabel: t(`resolutionConverter.ratio${aspect.ratio.replace(":", "")}`),
        color: aspect.color,
        isActive: aspect.ratio === "16:9",
      };
    });
  }, [selectedResolution, t]);

  // === 显示器分辨率管理 ===
  async function loadDisplayModes(deviceName: string) {
    setIsLoadingModes(true);
    try {
      const modes = await invoke<DisplayMode[]>("get_nvidia_display_modes", { deviceName });
      setDisplayModes(modes);
      // 默认定位到当前正在使用的模式，而不是列表第一个，避免"对不上"
      const currentMode = modes.find((m) => m.is_current);
      const target = currentMode ?? modes[0];
      if (target) {
        setSelectedResKey(`${target.width}x${target.height}`);
        setSelectedRateKey((Math.round(target.refresh_rate * 10) / 10).toFixed(1));
      }
    } catch (e) {
      console.error("加载分辨率列表失败:", e);
      toast({
        title: t("resolutionConverter.loadModesFailed"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
      setDisplayModes([]);
    } finally {
      setIsLoadingModes(false);
    }
  }

  async function loadInjectedResolutions() {
    try {
      const list = await invoke<InjectedResolution[]>("get_injected_resolutions");
      setInjectedResolutions(list);
    } catch (e) {
      console.error("加载注入分辨率列表失败:", e);
    }
  }

  function handleDisplayChange(value: string) {
    const idx = Number(value);
    setSelectedDisplayIdx(idx);
    const disp = displays[idx];
    if (disp) {
      loadDisplayModes(disp.device_name);
    }
  }

  const applyCustomResolution = useCallback(async (width: number, height: number, refreshRate?: number) => {
    const disp = displays[selectedDisplayIdx];
    if (!disp) {
      toast({
        title: t("resolutionConverter.applyFailed"),
        description: t("resolutionConverter.selectDisplayFirst"),
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }

    const resKey = refreshRate
      ? `${width}x${height} @ ${refreshRate.toFixed(1)}Hz`
      : `${width}x${height}`;
    setApplyingResKey(resKey);
    try {
      const result = await invoke<SetResolutionResult>("set_nvidia_display_resolution", {
        displayId: disp.display_id,
        width,
        height,
        deviceName: disp.device_name,
        refreshRate: refreshRate ?? null,
      });
      if (result.injected) {
        toast({
          title: "注入完成",
          description: "需要重启电脑，稍后在游戏里更改分辨率",
          status: "info",
          duration: 5000,
          isClosable: true,
        });
        loadInjectedResolutions();
      } else {
        toast({
          title: t("resolutionConverter.resolutionApplied"),
          description: resKey,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (e) {
      toast({
        title: t("resolutionConverter.applyFailed"),
        description: String(e),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setApplyingResKey("");
    }
  }, [displays, selectedDisplayIdx, t, toast]);

  function handleApplyResolution() {
    if (!selectedResKey || !selectedRateKey) return;
    const [wStr, hStr] = selectedResKey.split("x");
    const w = parseInt(wStr);
    const h = parseInt(hStr);
    const rr = parseFloat(selectedRateKey);
    setIsApplyingResolution(true);
    applyCustomResolution(w, h, rr).finally(() =>
      setIsApplyingResolution(false)
    );
  }

  // 加载 NVIDIA 显示器列表和已注入分辨率
  useEffect(() => {
    // 延迟加载显示器列表，避免进入页面时阻塞渲染导致卡顿
    const timer = setTimeout(() => {
      (async () => {
        try {
          const dispList = await invoke<NvidiaDisplay[]>("list_nvidia_displays");
          setDisplays(dispList);
          if (dispList.length > 0) {
            await loadDisplayModes(dispList[0].device_name);
          }
        } catch (e) {
          console.error("加载 NVIDIA 显示器列表失败:", e);
        }
        await loadInjectedResolutions();
      })();
    }, 200);
    return () => clearTimeout(timer);
  }, []);

  const header = (
    <PageHeader
      title={t("resolutionConverter.title")}
      mb={6}
    />
  );

  const content = (
    <VStack align="start" spacing={6}>
      <VStack align="start" spacing={4} w="full">
        <Text color={textColor} fontSize="md" fontWeight="600">
          {t("resolutionConverter.selectResolution")}
        </Text>
        <ResolutionSelector
          selected={selectedResolution}
          onSelect={setSelectedResolution}
        />
      </VStack>

      {selectedResolution === "custom" ? (
        <VStack align="start" spacing={4} w="full">
          <Text color={textColor} fontSize="md" fontWeight="600">
            {t("resolutionConverter.custom")}
          </Text>
          <LiquidGlassCard w="full" p={5} boxShadow="2xl">
            <VStack spacing={3} align="stretch">
              <SimpleGrid columns={{ base: 1, md: 3 }} spacing={3} w="full">
                <Box>
                  <Text fontWeight="medium" fontSize="xs" color={subTextColor} mb={1}>
                    {t("resolutionConverter.width")}
                  </Text>
                  <Input
                    type="number"
                    value={customWidth}
                    onChange={(e) => setCustomWidth(e.target.value)}
                    placeholder={t("resolutionConverter.widthInputPlaceholder")}
                    bg={useColorModeValue("white", "#1a1a1a")}
                  />
                </Box>
                <Box>
                  <Text fontWeight="medium" fontSize="xs" color={subTextColor} mb={1}>
                    {t("resolutionConverter.height")}
                  </Text>
                  <Input
                    type="number"
                    value={customHeight}
                    onChange={(e) => setCustomHeight(e.target.value)}
                    placeholder={t("resolutionConverter.heightInputPlaceholder")}
                    bg={useColorModeValue("white", "#1a1a1a")}
                  />
                </Box>
                <Box>
                  <Text fontWeight="medium" fontSize="xs" color={subTextColor} mb={1}>
                    {t("resolutionConverter.refreshRate")}
                  </Text>
                  <Input
                    type="number"
                    value={customRefreshRate}
                    onChange={(e) => setCustomRefreshRate(e.target.value)}
                    placeholder={t("resolutionConverter.refreshRateInputPlaceholder")}
                    bg={useColorModeValue("white", "#1a1a1a")}
                  />
                </Box>
              </SimpleGrid>
              <Button
                bg={getActiveColor()}
                color={getContrastTextColor()}
                size="md"
                borderRadius="lg"
                isLoading={isApplyingCustom}
                loadingText={t("resolutionConverter.applying")}
                isDisabled={!customWidth || !customHeight || Number(customWidth) <= 0 || Number(customHeight) <= 0}
                onClick={async () => {
                  const w = parseInt(customWidth);
                  const h = parseInt(customHeight);
                  if (!w || !h) return;
                  const rr = customRefreshRate ? parseFloat(customRefreshRate) : undefined;
                  if (rr && rr <= 0) {
                    toast({
                      title: t("resolutionConverter.applyFailed"),
                      description: t("resolutionConverter.customResolutionHint"),
                      status: "error",
                      duration: 3000,
                      isClosable: true,
                    });
                    return;
                  }
                  setIsApplyingCustom(true);
                  try {
                    await applyCustomResolution(w, h, rr);
                  } finally {
                    setIsApplyingCustom(false);
                  }
                }}
                _hover={{ filter: 'brightness(0.85)' }}
                w="full"
              >
                {t("resolutionConverter.applyCustomResolution")}
              </Button>
              <Text fontSize="xs" color={subTextColor}>
                {t("resolutionConverter.customResolutionHint")}
              </Text>
            </VStack>
          </LiquidGlassCard>
        </VStack>
      ) : (
        <VStack align="start" spacing={4} w="full">
          <Text color={textColor} fontSize="md" fontWeight="600">
            {t("resolutionConverter.aspectRatios")}
          </Text>
          <SimpleGrid columns={{ base: 1, md: 3 }} spacing={4} w="full">
            {resolutions.map((res) => (
              <ResolutionCard
                key={res.ratio}
                resolution={res}
                color={res.color}
                isActive={res.isActive}
                onApply={() => applyCustomResolution(res.width, res.height)}
                isApplying={applyingResKey === `${res.width}x${res.height}`}
              />
            ))}
          </SimpleGrid>
        </VStack>
      )}

      <Box
        w="full"
        p={4}
        borderRadius="xl"
        bg={hexToRgba(primaryColor, 0.1)}
        border="1px solid"
        borderColor={hexToRgba(primaryColor, 0.3)}
      >
        <Text color={subTextColor} fontSize="xs">
          {t("resolutionConverter.tip")}
        </Text>
      </Box>

      {/* 显示器分辨率管理 */}
      <LiquidGlassCard w="full" p={5} boxShadow="2xl">
        <HStack mb={4} spacing={2}>
          <Monitor size={18} color={getActiveColor()} />
          <Text fontWeight="semibold" color={headingColor} fontSize="md">
            {t("builtinTools.resolutionManagement")}
          </Text>
        </HStack>

        <VStack spacing={4} align="stretch">
          {/* 显示器选择 */}
          <Box>
            <Text fontWeight="medium" fontSize="sm" color={textColor} mb={2}>
              {t("builtinTools.selectDisplay")}
            </Text>
            <CustomSelect
              value={selectedDisplayIdx.toString()}
              onChange={handleDisplayChange}
              direction="up"
              options={displays.map((d, i) => ({
                value: i.toString(),
                label: d.monitor_name
                  ? `${d.monitor_name}${d.current_width > 0 && d.current_height > 0 ? ` (${d.current_width}x${d.current_height})` : ""}`
                  : `${d.device_name}${d.current_width > 0 && d.current_height > 0 ? ` (${d.current_width}x${d.current_height})` : ""}`,
              }))}
              width="100%"
            />
          </Box>

          {/* 分辨率选择（两级联动） */}
          <Box>
            <Text fontWeight="medium" fontSize="sm" color={textColor} mb={2}>
              {t("builtinTools.selectResolution")}
            </Text>
            {isLoadingModes ? (
              <Flex justify="center" py={4}>
                <Spinner size="sm" color={getActiveColor()} />
              </Flex>
            ) : availableResolutions.length > 0 ? (
              <CustomSelect
                value={selectedResKey}
                onChange={(v) => {
                  setSelectedResKey(v);
                  // 切换分辨率后，自动选中该分辨率下的第一个刷新率（最高）
                  const [w, h] = v.split("x").map(Number);
                  const rates = displayModes
                    .filter((m) => m.width === w && m.height === h)
                    .map((m) => Math.round(m.refresh_rate * 10) / 10)
                    .sort((a, b) => b - a);
                  if (rates.length > 0) {
                    setSelectedRateKey(rates[0].toFixed(1));
                  }
                }}
                direction="up"
                options={availableResolutions.map((r) => ({
                  value: r.key,
                  label: `${r.label}${
                    displayModes.some(
                      (m) =>
                        m.width === r.width && m.height === r.height && m.is_current
                    )
                      ? " (当前)"
                      : ""
                  }`,
                }))}
                width="100%"
              />
            ) : (
              <Text fontSize="sm" color={subTextColor}>
                {t("resolutionConverter.noModes")}
              </Text>
            )}
          </Box>

          {/* 刷新率选择 */}
          {selectedResKey && availableRates.length > 0 && (
            <Box>
              <Text fontWeight="medium" fontSize="sm" color={textColor} mb={2}>
                刷新率
              </Text>
              <CustomSelect
                value={selectedRateKey}
                onChange={setSelectedRateKey}
                direction="up"
                options={availableRates.map((r) => ({
                  value: r.key,
                  label: `${r.label}${r.isCurrent ? " (当前)" : ""}`,
                }))}
                width="100%"
              />
            </Box>
          )}

          {/* 应用按钮 */}
          <Button
            bg={getActiveColor()}
            color={getContrastTextColor()}
            isDisabled={!selectedResKey || !selectedRateKey || isLoadingModes}
            isLoading={isApplyingResolution}
            loadingText={t("resolutionConverter.applying")}
            onClick={handleApplyResolution}
            w="full"
            size="md"
            borderRadius="lg"
            _hover={{ filter: 'brightness(0.85)' }}
          >
            {t("builtinTools.applyResolution")}
          </Button>
        </VStack>
      </LiquidGlassCard>

      {/* 已注入的分辨率 */}
      {injectedResolutions.length > 0 && (
        <LiquidGlassCard w="full" p={5} boxShadow="2xl">
          <HStack mb={4} spacing={2}>
            <Monitor size={18} color={getActiveColor()} />
            <Text fontWeight="semibold" color={headingColor} fontSize="md">
              已注入的分辨率
            </Text>
          </HStack>
          <VStack spacing={2} align="stretch">
            {injectedResolutions.map((r) => (
              <HStack
                key={`${r.width}x${r.height}`}
                justify="space-between"
                p={3}
                borderRadius="lg"
                bg={useColorModeValue("gray.50", "#1a1a1a")}
              >
                <Text color={textColor} fontWeight="500" fontSize="sm">
                  {r.width} × {r.height}
                </Text>
                <Button
                  size="xs"
                  variant="ghost"
                  colorScheme="red"
                  onClick={async () => {
                    try {
                      await invoke("remove_injected_resolution", {
                        width: r.width,
                        height: r.height,
                      });
                      loadInjectedResolutions();
                    } catch (e) {
                      console.error("删除注入记录失败:", e);
                    }
                  }}
                >
                  删除
                </Button>
              </HStack>
            ))}
            <Text fontSize="xs" color={subTextColor} mt={2}>
              注：已注入的分辨率需重启电脑后方可使用，重启后可在游戏内设置
            </Text>
          </VStack>
        </LiquidGlassCard>
      )}
    </VStack>
  );

  return (
    <Box pt={8}>
      {header}
      {liquidGlassEnabled ? (
        <LiquidGlassCard
          w="full"
          boxShadow="2xl"
          overflow="hidden"
          position="relative"
          p={6}
        >
          {content}
        </LiquidGlassCard>
      ) : (
        <Box
          bg={cardBg}
          borderRadius="3xl"
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
      )}
    </Box>
  );
}
