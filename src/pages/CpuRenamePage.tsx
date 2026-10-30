import {
  Badge,
  Box,
  Button,
  Grid,
  HStack,
  Input,
  Tab,
  TabList,
  TabPanel,
  TabPanels,
  Tabs,
  Text,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Cpu, RotateCcw, Sparkles } from "lucide-react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { PageHeader } from "@/components/special/page-header";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";

interface CpuInfo {
  name: string;
  original_name: string | null;
  is_backed_up: boolean;
}

interface CpuRenameResult {
  success: boolean;
  message: string;
}

/** 预设型号：按厂商分 Tab，全部写成 Windows 里真实会出现的样式 */
const INTEL_PRESETS = [
  "Intel(R) Core(TM) Ultra 9 285K",
  "Intel(R) Core(TM) i9-14900K",
  "Intel(R) Core(TM) i7-14700K",
  "Intel(R) Core(TM) i5-13600K",
  "Intel(R) Xeon(R) W-3495X",
];

const AMD_PRESETS = [
  "AMD Ryzen 9 9950X3D 16-Core Processor",
  "AMD Ryzen 9 9950X 16-Core Processor",
  "AMD Ryzen 7 9800X3D 8-Core Processor",
  "AMD Ryzen 7 7800X3D 8-Core Processor",
  "AMD Ryzen 5 7600X 6-Core Processor",
  "AMD Ryzen Threadripper PRO 7995WX 96-Cores",
];

const APPLE_PRESETS = [
  "Apple M4",
  "Apple M4 Pro",
  "Apple M4 Max",
  "Apple M4 Ultra",
];

/** Tab 序号：0/1/2 为厂商预设，3 为自定义 */
const CUSTOM_TAB_INDEX = 3;

export default function CpuRenamePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("cpu");
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.600", "#555555");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const chipBg = useColorModeValue("gray.50", "rgba(255,255,255,0.04)");
  const warnBg = useColorModeValue("orange.50", "rgba(255,152,0,0.12)");
  const warnBorder = useColorModeValue("orange.200", "rgba(255,152,0,0.35)");

  const [info, setInfo] = useState<CpuInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [tabIndex, setTabIndex] = useState(0);
  const [selectedName, setSelectedName] = useState("");
  const [customName, setCustomName] = useState("");
  const [applying, setApplying] = useState(false);
  const [restoring, setRestoring] = useState(false);

  // 原型号：有备份时用备份里的原始值，否则就是当前值（说明还没改过）
  const originalModel = info?.original_name || info?.name || "";
  const isModified = !!info?.original_name && info.original_name !== info.name;
  const targetName = (tabIndex === CUSTOM_TAB_INDEX ? customName : selectedName).trim();

  const loadInfo = async () => {
    try {
      const data = await invoke<CpuInfo>("get_cpu_rename_info");
      setInfo(data);
    } catch (error) {
      toast({
        title: t("cpuRename.loadFailed"),
        description: String(error),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void loadInfo();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleApply = async () => {
    if (!targetName) return;
    setApplying(true);
    try {
      const result = await invoke<CpuRenameResult>("apply_cpu_rename", { newName: targetName });
      if (result.success) {
        toast({
          title: t("cpuRename.applied"),
          description: result.message,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        await loadInfo();
      } else {
        throw new Error(result.message);
      }
    } catch (error) {
      toast({
        title: t("cpuRename.failed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setApplying(false);
    }
  };

  const handleRestore = async () => {
    setRestoring(true);
    try {
      const result = await invoke<CpuRenameResult>("restore_cpu_name");
      if (result.success) {
        toast({
          title: t("cpuRename.restored"),
          description: result.message,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        setCustomName("");
        setSelectedName("");
        await loadInfo();
      } else {
        throw new Error(result.message);
      }
    } catch (error) {
      toast({
        title: t("cpuRename.failed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setRestoring(false);
    }
  };

  const renderPresetTab = (label: string, options: string[]) => (
    <Box w="full">
      <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
        {label}
      </Text>
      <CustomSelect
        value={selectedName}
        onChange={setSelectedName}
        options={options.map((name) => ({ value: name, label: name }))}
        placeholder={t("cpuRename.selectPlaceholder")}
        width="100%"
      />
    </Box>
  );

  return (
    <Box pt={8} pb={8}>
      <PageHeader title={t("cpuRename.title")} mb={6} />

      <VStack align="stretch" spacing={5}>
        {/* 仅供娱乐提示 */}
        <Box
          bg={warnBg}
          border="1px solid"
          borderColor={warnBorder}
          borderRadius="3xl"
          px={5}
          py={4}
        >
          <HStack spacing={2.5} align="flex-start">
            <Sparkles size={16} color={primaryColor} style={{ marginTop: 3, flexShrink: 0 }} />
            <VStack align="start" spacing={1}>
              <Text fontSize="sm" fontWeight="600" color={headingColor}>
                {t("cpuRename.funOnly")}
              </Text>
              <Text fontSize="xs" color={textColor} lineHeight="1.7">
                {t("cpuRename.funOnlyDesc")}
              </Text>
            </VStack>
          </HStack>
        </Box>

        <Grid templateColumns={{ base: "1fr", lg: "1fr 1.25fr" }} gap={5} alignItems="start">
          {/* 状态卡 */}
          <LiquidGlassCard p={6}>
            <VStack align="stretch" spacing={4}>
              <HStack justify="space-between" align="center">
                <Text fontSize="sm" fontWeight="600" color={headingColor}>
                  {t("cpuRename.originalModel")}
                </Text>
                {isModified && (
                  <Badge
                    borderRadius="full"
                    px={2}
                    py={0.5}
                    fontSize="10px"
                    color={primaryColor}
                    bg={hexToRgba(primaryColor, 0.14)}
                  >
                    {t("cpuRename.modified")}
                  </Badge>
                )}
              </HStack>

              <HStack
                spacing={2.5}
                px={3}
                py={3}
                borderRadius="2xl"
                border="1px solid"
                borderColor={cardBorder}
                bg={chipBg}
              >
                <Cpu size={18} color={primaryColor} style={{ flexShrink: 0 }} />
                <Text fontSize="sm" fontWeight="600" color={headingColor} lineHeight="1.5">
                  {loading ? t("cpuRename.loading") : originalModel || "--"}
                </Text>
              </HStack>

              <Box>
                <Text fontSize="xs" color={subTextColor} mb={1.5}>
                  {t("cpuRename.currentName")}
                </Text>
                <Text
                  fontSize="sm"
                  color={isModified ? primaryColor : headingColor}
                  fontWeight={isModified ? "600" : "400"}
                  lineHeight="1.5"
                  wordBreak="break-word"
                >
                  {loading ? t("cpuRename.loading") : info?.name || "--"}
                </Text>
              </Box>

              <Text fontSize="xs" color={subTextColor} lineHeight="1.7">
                {t("cpuRename.restartHint")}
              </Text>
              <Text fontSize="xs" color={subTextColor} lineHeight="1.7">
                {t("cpuRename.adminHint")}
              </Text>
            </VStack>
          </LiquidGlassCard>

          {/* 改名卡 */}
          <LiquidGlassCard p={6}>
            <VStack align="stretch" spacing={4}>
              <Text fontSize="sm" fontWeight="600" color={headingColor}>
                {t("cpuRename.newName")}
              </Text>

              <Tabs
                index={tabIndex}
                onChange={setTabIndex}
                variant="enclosed"
                w="full"
                colorScheme="gray"
              >
                <TabList>
                  <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>
                    Intel
                  </Tab>
                  <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>
                    AMD
                  </Tab>
                  <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>
                    Apple M4
                  </Tab>
                  <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>
                    {t("cpuRename.customTab")}
                  </Tab>
                </TabList>
                <TabPanels>
                  <TabPanel px={0}>
                    {renderPresetTab(t("cpuRename.intelPresets"), INTEL_PRESETS)}
                  </TabPanel>
                  <TabPanel px={0}>
                    {renderPresetTab(t("cpuRename.amdPresets"), AMD_PRESETS)}
                  </TabPanel>
                  <TabPanel px={0}>
                    {renderPresetTab(t("cpuRename.applePresets"), APPLE_PRESETS)}
                  </TabPanel>
                  <TabPanel px={0}>
                    <Box w="full">
                      <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                        {t("cpuRename.customTab")}
                      </Text>
                      <Input
                        value={customName}
                        onChange={(e) => setCustomName(e.target.value)}
                        placeholder={t("cpuRename.placeholder")}
                        maxLength={64}
                        color={headingColor}
                        bg={hexToRgba(primaryColor, 0.06)}
                        borderColor={hexToRgba(primaryColor, 0.38)}
                        borderRadius="2xl"
                        _hover={{ borderColor: hexToRgba(primaryColor, 0.6) }}
                        _focus={{
                          borderColor: primaryColor,
                          boxShadow: `0 0 0 1px ${hexToRgba(primaryColor, 0.55)}`,
                        }}
                        _placeholder={{ color: subTextColor }}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") void handleApply();
                        }}
                      />
                    </Box>
                  </TabPanel>
                </TabPanels>
              </Tabs>

              <VStack align="stretch" spacing={3} pt={1}>
                <Button
                  bg={primaryColor}
                  color={contrastText}
                  onClick={handleApply}
                  isLoading={applying}
                  loadingText={t("cpuRename.applying")}
                  isDisabled={!targetName}
                  _hover={{ bg: hexToRgba(primaryColor, 0.85) }}
                  _active={{ bg: hexToRgba(primaryColor, 0.7) }}
                >
                  {t("cpuRename.apply")}
                </Button>
                {info?.is_backed_up && (
                  <Button
                    variant="outline"
                    colorScheme="orange"
                    leftIcon={<RotateCcw size={15} />}
                    onClick={handleRestore}
                    isLoading={restoring}
                    loadingText={t("cpuRename.restoring")}
                  >
                    {t("cpuRename.restore")}
                  </Button>
                )}
              </VStack>
            </VStack>
          </LiquidGlassCard>
        </Grid>
      </VStack>
    </Box>
  );
}
