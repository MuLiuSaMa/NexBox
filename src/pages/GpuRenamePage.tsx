import {
  Box,
  Button,
  Heading,
  Text,
  VStack,
  useColorModeValue,
  HStack,
  IconButton,
  Card,
  CardBody,
  Input,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { CustomSelect } from "@/components/special/custom-select";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { AnimatedPage } from "@/components/ui/animated-page";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, Monitor } from "lucide-react";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";

interface GpuInfo {
  name: string;
  is_integrated: boolean;
  original_name: string | null;
  is_backed_up: boolean;
  key_path: string;
}

interface GpuOption {
  id: string;
  name: string;
  category: string;
}

interface GpuRenameResult {
  success: boolean;
  message: string;
}

export default function GpuRenamePage() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const toast = useDynamicIsland("gpu");
  const navigate = useNavigate();

  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const adaptiveTitle = useAdaptiveTextColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.600", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const [gpuList, setGpuList] = useState<GpuInfo[]>([]);
  const [gpuOptions, setGpuOptions] = useState<GpuOption[]>([]);
  // targetGpuKey: 选择要改写哪张显卡（按 key_path 精确指定）
  const [targetGpuKey, setTargetGpuKey] = useState<string>("");
  // selectedOption: 选择改写后的目标名字（低端/高端预设）
  const [selectedOption, setSelectedOption] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [applying, setApplying] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [tabIndex, setTabIndex] = useState(0);
  const [customName, setCustomName] = useState("");
  const [restarting, setRestarting] = useState(false);

  // 当前选中要改写的显卡对象
  const targetGpu = gpuList.find((g) => g.key_path === targetGpuKey);
  const anyBackedUp = gpuList.some((g) => g.is_backed_up);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    try {
      setLoading(true);
      const [list, options] = await Promise.all([
        invoke<GpuInfo[]>("get_gpu_list"),
        invoke<GpuOption[]>("get_gpu_options"),
      ]);
      setGpuList(list);

      // 改写后的目标名字只从预设中选择（不再把当前显卡名作为改写目标）
      setGpuOptions(options);

      // 保留用户已选择的显卡；若之前的选择已失效（不在新列表中），则回退到默认独显
      setTargetGpuKey((prev) => {
        if (prev && list.some((g) => g.key_path === prev)) {
          return prev;
        }
        const defaultGpu = list.find((g) => !g.is_integrated) ?? list[0];
        return defaultGpu?.key_path ?? "";
      });
      // 注意：不再重置 selectedOption，保留用户之前选择的改写后名字
    } catch (error) {
      toast({
        title: t("gpuRename.loadError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setLoading(false);
    }
  };

  const handleApply = async () => {
    let targetName = "";

    if (tabIndex === 1) {
      if (!customName.trim()) {
        toast({
          title: t("gpuRename.selectGpu"),
          status: "warning",
          duration: 2000,
          isClosable: true,
        });
        return;
      }
      targetName = customName.trim();
    } else {
      if (!selectedOption) {
        toast({
          title: t("gpuRename.selectGpu"),
          status: "warning",
          duration: 2000,
          isClosable: true,
        });
        return;
      }
      const selectedGpu = gpuOptions.find((opt) => opt.id === selectedOption);
      if (!selectedGpu) return;
      targetName = selectedGpu.name;
    }

    if (!targetGpuKey) {
      toast({
        title: t("gpuRename.selectGpu"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    try {
      setApplying(true);
      const result = await invoke<GpuRenameResult>("apply_gpu_rename", {
        newName: targetName,
        targetKeyPath: targetGpuKey,
      });

      if (result.success) {
        toast({
          title: t("gpuRename.success"),
          description: result.message,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        await loadData();
      } else {
        toast({
          title: t("gpuRename.error"),
          description: result.message,
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("gpuRename.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setApplying(false);
    }
  };

  const handleRestore = async () => {
    try {
      setRestoring(true);
      const result = await invoke<GpuRenameResult>("restore_gpu_name");

      if (result.success) {
        toast({
          title: t("gpuRename.restored"),
          description: result.message,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        await loadData();
      } else {
        toast({
          title: t("gpuRename.error"),
          description: result.message,
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("gpuRename.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setRestoring(false);
    }
  };

  const handleRestartDriver = async () => {
    try {
      setRestarting(true);
      await invoke("restart_graphics_driver");
      toast({
        title: t("gpuDriverRestart.success"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("gpuDriverRestart.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setRestarting(false);
    }
  };

  // 当前显卡下拉框的选项：来自实际检测到的显卡列表
  const targetGpuOptions = gpuList.map((g) => ({
    value: g.key_path,
    label: g.is_integrated
      ? `${g.name} (${t("gpuRename.integrated")})`
      : `${g.name} (${t("gpuRename.discrete")})`,
  }));

  const content = (
    <VStack align="start" spacing={6}>
      <HStack>
        <IconButton
          aria-label={t("builtinTools.back")}
          icon={<ArrowLeft size={20} />}
          variant="ghost"
          onClick={() => navigate("/builtin-tools")}
          color={headingColor}
        />
        <Monitor size={28} color={headingColor} />
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("gpuRename.title")}
        </Heading>
      </HStack>

      {loading ? (
        <Text color={textColor}>{t("gpuRename.loading")}</Text>
      ) : (
        <>
          <VStack align="start" spacing={3} w="full">
            <Box w="full">
              <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                {t("gpuRename.currentGpu")}
              </Text>
              <CustomSelect
                value={targetGpuKey}
                onChange={setTargetGpuKey}
                options={targetGpuOptions}
                placeholder={t("gpuRename.selectPlaceholder")}
                width="100%"
              />
            </Box>

            <HStack spacing={6} w="full" align="start">
              <Box flex={1}>
                <Text color={textColor} fontSize="sm" mb={1}>
                  {t("gpuRename.backupStatus")}
                </Text>
                <Text
                  color={anyBackedUp ? "green.500" : "orange.500"}
                  fontWeight="medium"
                >
                  {anyBackedUp
                    ? t("gpuRename.backedUp")
                    : t("gpuRename.notBackedUp")}
                </Text>
              </Box>

              {targetGpu?.original_name && (
                <Box flex={1}>
                  <Text color={textColor} fontSize="sm" mb={1}>
                    {t("gpuRename.originalGpu")}
                  </Text>
                  <Text color={headingColor} fontWeight="medium" wordBreak="break-all">
                    {targetGpu.original_name}
                  </Text>
                </Box>
              )}
            </HStack>
          </VStack>

          <Tabs index={tabIndex} onChange={setTabIndex} variant="enclosed" w="full" mt={4}>
            <TabList>
              <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>{t("gpuRename.presetTab")}</Tab>
              <Tab color={textColor} _selected={{ color: headingColor, fontWeight: "600" }}>{t("gpuRename.customTab")}</Tab>
            </TabList>
            <TabPanels>
              <TabPanel px={0}>
                <Box w="full">
                  <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                    {t("gpuRename.lowEnd")}
                  </Text>
                  <CustomSelect
                    value={selectedOption}
                    onChange={setSelectedOption}
                    options={gpuOptions
                      .filter(option => option.category === "low-end")
                      .map(option => ({ value: option.id, label: option.name }))}
                    placeholder={t("gpuRename.selectPlaceholder")}
                    width="100%"
                  />
                </Box>
                <Box w="full" mt={4}>
                  <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                    {t("gpuRename.highEnd")}
                  </Text>
                  <CustomSelect
                    value={selectedOption}
                    onChange={setSelectedOption}
                    options={gpuOptions
                      .filter(option => option.category === "high-end")
                      .map(option => ({ value: option.id, label: option.name }))}
                    placeholder={t("gpuRename.selectPlaceholder")}
                    width="100%"
                  />
                </Box>
              </TabPanel>
              <TabPanel px={0}>
                <Box w="full">
                  <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                    {t("gpuRename.customTab")}
                  </Text>
                  <Input
                    value={customName}
                    onChange={(e) => setCustomName(e.target.value)}
                    placeholder={t("gpuRename.customPlaceholder")}
                    color={headingColor}
                    borderColor={cardBorder}
                    _focus={{ borderColor: primaryColor }}
                  />
                </Box>
              </TabPanel>
            </TabPanels>
          </Tabs>

          <VStack align="start" spacing={3} w="full" mt={4}>
            <Button
              bg={primaryColor}
              color={contrastText}
              onClick={handleApply}
              isLoading={applying}
              loadingText={t("gpuRename.applying")}
              w="full"
              _hover={{
                bg: hexToRgba(primaryColor, 0.8),
                transform: "translateY(-1px)",
                boxShadow: `0 4px 12px ${hexToRgba(primaryColor, 0.3)}`,
              }}
              _active={{
                bg: hexToRgba(primaryColor, 0.6),
                transform: "translateY(0)",
              }}
              transition="all 0.2s"
            >
              {t("gpuRename.apply")}
            </Button>

            {anyBackedUp && (
              <Button
                colorScheme="orange"
                onClick={handleRestore}
                isLoading={restoring}
                loadingText={t("gpuRename.restoring")}
                w="full"
              >
                {t("gpuRename.restore")}
              </Button>
            )}

            {/* 重启显卡驱动 */}
            <Box w="full" pt={2}>
              <Text color={textColor} fontSize="sm" mb={2} fontWeight="600">
                {t("gpuDriverRestart.pageTitle")}
              </Text>
              <Text color={textColor} fontSize="xs" mb={3}>
                {t("gpuDriverRestart.pageDesc")}
              </Text>
              <Button
                colorScheme="red"
                variant="outline"
                onClick={handleRestartDriver}
                isLoading={restarting}
                loadingText={t("gpuDriverRestart.waiting")}
                w="full"
              >
                {t("gpuDriverRestart.restartBtn")}
              </Button>
            </Box>
          </VStack>
        </>
      )}
    </VStack>
  );

  return (
    <AnimatedPage>
      <Box pt={8}>
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
          <Card
            bg={cardBg}
            borderColor={cardBorder}
            borderWidth="1px"
            w="full"
            boxShadow="2xl"
            overflow="hidden"
            position="relative"
          >
            <CardBody p={6}>
              {content}
            </CardBody>
          </Card>
        )}
      </Box>
    </AnimatedPage>
  );
}
