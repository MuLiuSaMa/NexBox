import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  useColorModeValue,
  Button,
  Badge,
  Grid,
  IconButton,
  Switch,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  MenuOptionGroup,
  MenuItemOption,
  Spinner,
  Divider,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Cpu, ArrowLeft, Zap, Eye, FileText, Settings, ChevronDown, Check, Monitor, Thermometer } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { useNavigate } from "react-router-dom";
import { getHardwareInfo, GpuInfo, GpuVendor } from "@/lib/hardware";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface DLSSModelPreset {
  id: string;
  name: string;
  description: string;
  recommended: boolean;
}

interface DLSSApplyResult {
  success: boolean;
  message: string;
  preset: string;
  quality: string;
  texture_quality: string;
  antialiasing: string;
}

interface DLSSPresetStatus {
  preset: string;
  quality: string;
  texture_quality: string;
  antialiasing: string;
}

interface DLSSSettingsStatus {
  dlss_indicator_enabled: boolean;
}

interface PresetReference {
  id: string;
  gpuSeries: string;
  preset: string;
  driverVersion: string;
  typeLabel: string;
  presetColor: "purple" | "green";
}

const getDLSSPresets = (t: (key: string) => string): DLSSModelPreset[] => [
  { id: "default", name: t("deltaForce.dlssModels.default.name"), description: t("deltaForce.dlssModels.default.description"), recommended: false },
  { id: "A", name: t("deltaForce.dlssModels.A.name"), description: t("deltaForce.dlssModels.A.description"), recommended: false },
  { id: "B", name: t("deltaForce.dlssModels.B.name"), description: t("deltaForce.dlssModels.B.description"), recommended: false },
  { id: "C", name: t("deltaForce.dlssModels.C.name"), description: t("deltaForce.dlssModels.C.description"), recommended: false },
  { id: "D", name: t("deltaForce.dlssModels.D.name"), description: t("deltaForce.dlssModels.D.description"), recommended: false },
  { id: "E", name: t("deltaForce.dlssModels.E.name"), description: t("deltaForce.dlssModels.E.description"), recommended: false },
  { id: "F", name: t("deltaForce.dlssModels.F.name"), description: t("deltaForce.dlssModels.F.description"), recommended: false },
  { id: "G", name: t("deltaForce.dlssModels.G.name"), description: t("deltaForce.dlssModels.G.description"), recommended: false },
  { id: "J", name: t("deltaForce.dlssModels.J.name"), description: t("deltaForce.dlssModels.J.description"), recommended: false },
  { id: "K", name: t("deltaForce.dlssModels.K.name"), description: t("deltaForce.dlssModels.K.description"), recommended: true },
  { id: "L", name: t("deltaForce.dlssModels.L.name"), description: t("deltaForce.dlssModels.L.description"), recommended: true },
  { id: "M", name: t("deltaForce.dlssModels.M.name"), description: t("deltaForce.dlssModels.M.description"), recommended: true },
];

interface DLSSQualityOption {
  id: string;
  name: string;
  description: string;
}
const getDLSSQualityOptions = (t: (key: string) => string): DLSSQualityOption[] => [
  { id: "default", name: t("dlssQuality.default.name"), description: t("dlssQuality.default.description") },
  { id: "dlaa", name: "DLAA", description: "100% 渲染分辨率" },
  { id: "quality", name: t("dlssQuality.quality.name"), description: "约 67% 渲染分辨率" },
  { id: "balanced", name: t("dlssQuality.balanced.name"), description: "约 58% 渲染分辨率" },
  { id: "performance", name: t("dlssQuality.performance.name"), description: "50% 渲染分辨率" },
  { id: "ultra_performance", name: t("dlssQuality.ultraPerformance.name"), description: "约 33% 渲染分辨率" },
];

interface TextureQualityOption {
  id: string;
  name: string;
  description: string;
}
const getTextureQualityOptions = (t: (key: string) => string): TextureQualityOption[] => [
  { id: "default", name: "默认", description: "不修改" },
  { id: "high_quality", name: "高质量", description: "最高纹理过滤质量" },
  { id: "quality", name: "质量", description: "平衡画质与性能" },
  { id: "performance", name: "性能", description: "偏性能取向" },
  { id: "high_performance", name: "高性能", description: "最高性能取向" },
];

interface AntialiasingOption {
  id: string;
  name: string;
  description: string;
}
const getAntialiasingOptions = (t: (key: string) => string): AntialiasingOption[] => [
  { id: "default", name: "默认", description: "不修改" },
  { id: "off", name: "关", description: "关闭透明度超采样" },
  { id: "2x", name: "2x 超采样", description: "2倍透明度超采样" },
  { id: "4x", name: "4x 超采样", description: "4倍透明度超采样" },
  { id: "8x", name: "8x 超采样", description: "8倍透明度超采样" },
];

const getPresetReferences = (t: (key: string) => string): PresetReference[] => [
  { id: "1", gpuSeries: t("dlssPresetTable.rtx20Series"), preset: t("dlssPresetTable.presetK"), driverVersion: "581.08", typeLabel: t("dlssPresetTable.typeGeneralEnhance"), presetColor: "purple" },
  { id: "2", gpuSeries: t("dlssPresetTable.rtx30Series"), preset: t("dlssPresetTable.presetK"), driverVersion: "581.08", typeLabel: t("dlssPresetTable.typeGeneralEnhance"), presetColor: "purple" },
  { id: "3", gpuSeries: t("dlssPresetTable.rtx4060Ti"), preset: t("dlssPresetTable.presetK"), driverVersion: "581/595", typeLabel: t("dlssPresetTable.typePerformanceFirst"), presetColor: "purple" },
  { id: "4", gpuSeries: t("dlssPresetTable.rtx4060Ti"), preset: t("dlssPresetTable.presetM"), driverVersion: "581/595", typeLabel: t("dlssPresetTable.typeQualityEnhance"), presetColor: "green" },
  { id: "5", gpuSeries: t("dlssPresetTable.rtx50HighFps"), preset: t("dlssPresetTable.presetK"), driverVersion: "581/595", typeLabel: t("dlssPresetTable.typeFpsPriority"), presetColor: "purple" },
  { id: "6", gpuSeries: t("dlssPresetTable.rtx50HighQuality"), preset: t("dlssPresetTable.presetM"), driverVersion: "581/595", typeLabel: t("dlssPresetTable.typeQualityEnhance"), presetColor: "green" },
];

function SectionCard({
  title,
  children,
  icon,
}: {
  title: string;
  children: React.ReactNode;
  icon?: React.ReactNode;
}) {
  const { liquidGlassEnabled } = useBackground();
  const { config: themeConfig } = useThemeColor();
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const headerColor = useColorModeValue("gray.900", "#ffffff");

  const headerContent = (
    <HStack spacing={2}>
      <Box color={themeConfig.primaryColor}>{icon}</Box>
      <Text fontWeight="semibold" fontSize="md" color={headerColor}>{title}</Text>
    </HStack>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={5}>
        <VStack align="stretch" spacing={4}>
          {headerContent}
          {children}
        </VStack>
      </LiquidGlassCard>
    );
  }

  return (
    <Box bg={cardBg} borderRadius="xl" p={5} border="1px solid" borderColor={borderColor}>
      <VStack align="stretch" spacing={4}>
        {headerContent}
        {children}
      </VStack>
    </Box>
  );
}

function DLSSCard() {
  const { t } = useTranslation();
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const toast = useDynamicIsland("gpu");
  const [selectedPreset, setSelectedPreset] = useState("default");
  const [selectedQuality, setSelectedQuality] = useState("default");
  const [selectedTextureQuality, setSelectedTextureQuality] = useState("default");
  const [selectedAntialiasing, setSelectedAntialiasing] = useState("default");
  const [isApplying, setIsApplying] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const dlssPresets = getDLSSPresets(t);
  const dlssQualityOptions = getDLSSQualityOptions(t);
  const textureQualityOptions = getTextureQualityOptions(t);
  const antialiasingOptions = getAntialiasingOptions(t);

  useEffect(() => {
    const style = document.createElement("style");
    style.id = "chakra-menu-z-fix";
    style.textContent = `[data-popper-placement] { z-index: 99999 !important; }`;
    document.head.appendChild(style);
    return () => document.getElementById("chakra-menu-z-fix")?.remove();
  }, []);

  useEffect(() => {
    invoke<DLSSPresetStatus>("get_dlss_preset_status")
      .then((status) => {
        setSelectedPreset(status.preset || "K");
        setSelectedQuality(status.quality || "default");
        setSelectedTextureQuality(status.texture_quality || "default");
        setSelectedAntialiasing(status.antialiasing || "default");
      })
      .catch(() => {})
      .finally(() => setIsLoading(false));
  }, []);

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const hoverBg = useColorModeValue("gray.100", "#252525");
  const menuListBg = useColorModeValue("white", "#1a1a1a");

  const handleApply = async () => {
    setIsApplying(true);
    try {
      const result = await invoke<DLSSApplyResult>("apply_dlss_model_preset", {
        preset: selectedPreset,
        quality: selectedQuality,
        textureQuality: selectedTextureQuality,
        antialiasing: selectedAntialiasing,
      });
      toast({
        title: result.message,
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsApplying(false);
  };

  const currentPreset = dlssPresets.find(p => p.id === selectedPreset);
  const currentQuality = dlssQualityOptions.find(q => q.id === selectedQuality);
  const currentTextureQuality = textureQualityOptions.find(q => q.id === selectedTextureQuality);
  const currentAntialiasing = antialiasingOptions.find(a => a.id === selectedAntialiasing);

  return (
    <SectionCard title={t("deltaForce.dlssPreset")} icon={<Settings size={18} />}>
      <VStack align="stretch" spacing={4}>
        <Box>
          <Text fontSize="sm" fontWeight="medium" color={textColor} mb={2}>
            {t("dlssPreset.presetLabel")}
          </Text>
          <Menu matchWidth>
            <MenuButton as={Box} bg="transparent" p={0} border="none" w="full" cursor="pointer">
              <LiquidGlassCard px={3} py={1.5}>
                <HStack justify="space-between">
                  <HStack spacing={2}>
                    <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{currentPreset?.name}</Badge>
                    <Text fontSize="xs" color={subTextColor} noOfLines={1}>{currentPreset?.description}</Text>
                    {currentPreset?.recommended && <Badge colorScheme="green" fontSize="8px">{t("deltaForce.recommended")}</Badge>}
                  </HStack>
                  <ChevronDown size={16} />
                </HStack>
              </LiquidGlassCard>
            </MenuButton>
            <MenuList bg={menuListBg} borderColor={borderColor} maxH="300px" overflowY="auto">
              {dlssPresets.map(preset => (
                <MenuItem key={preset.id} onClick={() => setSelectedPreset(preset.id)} bg={selectedPreset === preset.id ? hoverBg : "transparent"} _hover={{ bg: hoverBg }}>
                  <HStack spacing={3} w="full" justify="space-between">
                    <HStack spacing={2}>
                      <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{preset.name}</Badge>
                      <Text fontSize="sm" color={textColor}>{preset.description}</Text>
                    </HStack>
                    <HStack spacing={2}>
                      {preset.recommended && <Badge colorScheme="green" fontSize="8px">{t("deltaForce.recommended")}</Badge>}
                      {selectedPreset === preset.id && <Check size={14} color={themeConfig.primaryColor} />}
                    </HStack>
                  </HStack>
                </MenuItem>
              ))}
            </MenuList>
          </Menu>
        </Box>

        <Box>
          <Text fontSize="sm" fontWeight="medium" color={textColor} mb={2}>{t("dlssQuality.label")}</Text>
          <Menu matchWidth>
            <MenuButton as={Box} bg="transparent" p={0} border="none" w="full" cursor="pointer">
              <LiquidGlassCard px={3} py={1.5}>
                <HStack justify="space-between">
                  <HStack spacing={2}>
                    <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{currentQuality?.name}</Badge>
                    <Text fontSize="xs" color={subTextColor} noOfLines={1}>{currentQuality?.description}</Text>
                  </HStack>
                  <ChevronDown size={16} />
                </HStack>
              </LiquidGlassCard>
            </MenuButton>
            <MenuList bg={menuListBg} borderColor={borderColor} maxH="300px" overflowY="auto">
              {dlssQualityOptions.map(option => (
                <MenuItem key={option.id} onClick={() => setSelectedQuality(option.id)} bg={selectedQuality === option.id ? hoverBg : "transparent"} _hover={{ bg: hoverBg }}>
                  <HStack spacing={3} w="full" justify="space-between">
                    <HStack spacing={2}>
                      <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{option.name}</Badge>
                      <Text fontSize="sm" color={textColor}>{option.description}</Text>
                    </HStack>
                    {selectedQuality === option.id && <Check size={14} color={themeConfig.primaryColor} />}
                  </HStack>
                </MenuItem>
              ))}
            </MenuList>
          </Menu>
        </Box>

        <Box>
          <Text fontSize="sm" fontWeight="medium" color={textColor} mb={2}>纹理过滤质量</Text>
          <Menu matchWidth>
            <MenuButton as={Box} bg="transparent" p={0} border="none" w="full" cursor="pointer">
              <LiquidGlassCard px={3} py={1.5}>
                <HStack justify="space-between">
                  <HStack spacing={2}>
                    <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{currentTextureQuality?.name}</Badge>
                    <Text fontSize="xs" color={subTextColor} noOfLines={1}>{currentTextureQuality?.description}</Text>
                  </HStack>
                  <ChevronDown size={16} />
                </HStack>
              </LiquidGlassCard>
            </MenuButton>
            <MenuList bg={menuListBg} borderColor={borderColor} maxH="300px" overflowY="auto">
              {textureQualityOptions.map(option => (
                <MenuItem key={option.id} onClick={() => setSelectedTextureQuality(option.id)} bg={selectedTextureQuality === option.id ? hoverBg : "transparent"} _hover={{ bg: hoverBg }}>
                  <HStack spacing={3} w="full" justify="space-between">
                    <HStack spacing={2}>
                      <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{option.name}</Badge>
                      <Text fontSize="sm" color={textColor}>{option.description}</Text>
                    </HStack>
                    {selectedTextureQuality === option.id && <Check size={14} color={themeConfig.primaryColor} />}
                  </HStack>
                </MenuItem>
              ))}
            </MenuList>
          </Menu>
        </Box>

        <Box>
          <Text fontSize="sm" fontWeight="medium" color={textColor} mb={2}>抗锯齿-透明度</Text>
          <Menu matchWidth>
            <MenuButton as={Box} bg="transparent" p={0} border="none" w="full" cursor="pointer">
              <LiquidGlassCard px={3} py={1.5}>
                <HStack justify="space-between">
                  <HStack spacing={2}>
                    <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{currentAntialiasing?.name}</Badge>
                    <Text fontSize="xs" color={subTextColor} noOfLines={1}>{currentAntialiasing?.description}</Text>
                  </HStack>
                  <ChevronDown size={16} />
                </HStack>
              </LiquidGlassCard>
            </MenuButton>
            <MenuList bg={menuListBg} borderColor={borderColor} maxH="300px" overflowY="auto">
              {antialiasingOptions.map(option => (
                <MenuItem key={option.id} onClick={() => setSelectedAntialiasing(option.id)} bg={selectedAntialiasing === option.id ? hoverBg : "transparent"} _hover={{ bg: hoverBg }}>
                  <HStack spacing={3} w="full" justify="space-between">
                    <HStack spacing={2}>
                      <Badge bg={hexToRgba(themeConfig.primaryColor, 0.15)} color={themeConfig.primaryColor} borderRadius="full" px={2}>{option.name}</Badge>
                      <Text fontSize="sm" color={textColor}>{option.description}</Text>
                    </HStack>
                    {selectedAntialiasing === option.id && <Check size={14} color={themeConfig.primaryColor} />}
                  </HStack>
                </MenuItem>
              ))}
            </MenuList>
          </Menu>
        </Box>

        <Button
          onClick={handleApply}
          isLoading={isApplying}
          bg={themeConfig.primaryColor}
          color={getContrastTextColor()}
          _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
          _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
          w="full"
          leftIcon={<Cpu size={16} />}
          size="sm"
        >
          {t("deltaForce.applyPreset")}
        </Button>

        <Text fontSize="xs" color={subTextColor} textAlign="center">
          {t("deltaForce.dlssNote")}
        </Text>
      </VStack>
    </SectionCard>
  );
}

function GpuInfoCard() {
  const { config: themeConfig } = useThemeColor();
  const [gpuInfo, setGpuInfo] = useState<GpuInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  useEffect(() => {
    getHardwareInfo()
      .then((info) => {
        const nvidiaGpu = info.gpu.find((g) => g.vendor === GpuVendor.NVIDIA);
        setGpuInfo(nvidiaGpu || info.gpu[0] || null);
      })
      .catch(() => {})
      .finally(() => setIsLoading(false));
  }, []);

  if (isLoading || !gpuInfo) return null;

  return (
    <SectionCard title="显卡" icon={<Monitor size={18} />}>
      <VStack align="stretch" spacing={3}>
        <HStack justify="space-between">
          <Text fontSize="xs" color={subTextColor}>型号</Text>
          <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={2} maxW="70%" textAlign="right">
            {gpuInfo.name}
          </Text>
        </HStack>
        {gpuInfo.memory_gb != null && gpuInfo.memory_gb > 0 && (
          <HStack justify="space-between">
            <Text fontSize="xs" color={subTextColor}>显存</Text>
            <Text fontSize="sm" fontWeight="bold" color={textColor}>{gpuInfo.memory_gb.toFixed(0)} GB</Text>
          </HStack>
        )}
        <HStack justify="space-between">
          <Text fontSize="xs" color={subTextColor}>驱动</Text>
          <Text fontSize="sm" fontWeight="bold" color={textColor}>{gpuInfo.driver_version}</Text>
        </HStack>
      </VStack>
    </SectionCard>
  );
}

function DLSSIndicatorCard() {
  const { t } = useTranslation();
  const { config: themeConfig } = useThemeColor();
  const toast = useDynamicIsland("gpu");
  const [isEnabled, setIsEnabled] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  useEffect(() => {
    invoke<DLSSSettingsStatus>("get_dlss_settings_status")
      .then((status) => setIsEnabled(status.dlss_indicator_enabled))
      .catch(() => {});
  }, []);

  const handleToggle = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<boolean>("toggle_dlss_indicator", {
        enable: !isEnabled,
      });
      setIsEnabled(result);
      toast({
        title: isEnabled ? t("dlssIndicator.disabled") : t("dlssIndicator.enabled"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  };

  return (
    <SectionCard title={t("dlssIndicator.title")} icon={<Eye size={18} />}>
      <HStack justify="space-between">
        <VStack align="start" spacing={0} flex={1}>
          {isLoading ? (
            <HStack spacing={2}>
              <Spinner size="sm" color={themeConfig.primaryColor} />
              <Text fontSize="sm" color={themeConfig.primaryColor} fontWeight="medium">
                {isEnabled ? t("dlssIndicator.disabling") : t("dlssIndicator.enabling")}
              </Text>
            </HStack>
          ) : (
            <>
              <Text fontSize="sm" color={subTextColor}>
                {t("dlssIndicator.description")}
              </Text>
              <Text fontSize="xs" color={subTextColor}>
                {t("dlssIndicator.note")}
              </Text>
            </>
          )}
        </VStack>
        <Switch
          isChecked={isEnabled}
          onChange={handleToggle}
          isDisabled={isLoading}
          sx={{
            "& .chakra-switch__track[data-checked]": {
              bg: themeConfig.primaryColor,
            },
          }}
          size="md"
        />
      </HStack>
    </SectionCard>
  );
}

function PresetReferenceTable() {
  const { t } = useTranslation();
  const { config: themeConfig } = useThemeColor();
  const presetReferences = getPresetReferences(t);

  const tableBg = useColorModeValue("gray.50", "#1a1a1a");
  const headerBg = useColorModeValue("gray.100", "#252525");
  const textColor = useColorModeValue("gray.800", "#ffffff");

  return (
    <SectionCard title={t("dlssPresetTable.title")} icon={<FileText size={18} />}>
      <Text fontSize="sm" color={useColorModeValue("gray.500", "#ffffff")} mb={3}>
        {t("dlssPresetTable.description")}
      </Text>

      <Box overflowX="auto" borderRadius="lg" border="1px solid" borderColor={useColorModeValue("gray.200", "#333333")}>
        <Table variant="simple" size="sm">
          <Thead bg={headerBg}>
            <Tr>
              <Th color={textColor} textTransform="none">{t("dlssPresetTable.gpuSeries")}</Th>
              <Th color={textColor} textTransform="none">{t("dlssPresetTable.recommendedPreset")}</Th>
              <Th color={textColor} textTransform="none">{t("dlssPresetTable.driverVersion")}</Th>
              <Th color={textColor} textTransform="none">{t("dlssPresetTable.scenario")}</Th>
            </Tr>
          </Thead>
          <Tbody>
            {presetReferences.map((ref) => (
              <Tr key={ref.id} bg={tableBg}>
                <Td color={textColor}>{ref.gpuSeries}</Td>
                <Td>
                  <Badge
                    bg={hexToRgba(themeConfig.primaryColor, 0.15)}
                    color={themeConfig.primaryColor}
                    borderRadius="full"
                    px={3}
                    py={1}
                  >
                    {ref.preset}
                  </Badge>
                </Td>
                <Td color={textColor}>{ref.driverVersion}</Td>
                <Td color={textColor}>{ref.typeLabel}</Td>
              </Tr>
            ))}
          </Tbody>
        </Table>
      </Box>
    </SectionCard>
  );
}

export default function DLSSPresetPage() {
  const { t } = useTranslation();
  const { config: themeConfig } = useThemeColor();
  const navigate = useNavigate();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();

  return (
    <Box pt={8} pb={8}>
      <HStack spacing={3} mb={6}>
        <IconButton
          aria-label="返回"
          icon={<ArrowLeft size={20} />}
          variant="ghost"
          onClick={() => navigate("/builtin-tools")}
          color={headingColor}
        />
        <Zap size={28} color={themeConfig.primaryColor} />
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("dlssPreset.title")}
        </Heading>
      </HStack>

      <Grid templateColumns={{ base: "1fr", lg: "1fr 1fr" }} gap={5} mb={5}>
        <Box position="relative" zIndex={10}>
          <DLSSCard />
        </Box>
        <Flex direction="column" h="full">
          <GpuInfoCard />
          <Box flex={1} minH={4} />
          <DLSSIndicatorCard />
        </Flex>
      </Grid>

      <PresetReferenceTable />
    </Box>
  );
}
