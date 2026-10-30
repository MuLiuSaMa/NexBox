import {
  Box,
  VStack,
  Text,
  HStack,
  useColorModeValue,
  Button,
  Badge,
  Alert,
  AlertIcon,
  AlertDescription,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel,
  Input,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { AlertTriangle, Cpu } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { useState, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface MemoryLimitOption {
  id: string;
  label: string;
  limit_gb: number;
  min_physical_gb: number;
}

interface MemoryLimitStatus {
  physical_memory_gb: number;
  physical_memory_mb: number;
  current_limit_mb: number | null;
  available_options: MemoryLimitOption[];
}

interface MemoryLimitResult {
  success: boolean;
  message: string;
  limit_mb: number | null;
  requires_restart: boolean;
}

export default function MemoryLimitPage() {
  const [memoryStatus, setMemoryStatus] = useState<MemoryLimitStatus | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isApplying, setIsApplying] = useState(false);
  const [tabIndex, setTabIndex] = useState(0);
  const [selectedPreset, setSelectedPreset] = useState<string>("");
  const [customValue, setCustomValue] = useState<string>("");
  const [customError, setCustomError] = useState<string>("");
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const toast = useDynamicIsland("memory");

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const themeColorHex = primaryColor || "#98DDD0";
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);

  // Mode-adaptive: white in light mode, black in dark mode
  const restoreBg = useColorModeValue("#FFFFFF", "#1A202C");
  const restoreColor = useColorModeValue("#1A202C", "#FFFFFF");
  const restoreHoverBg = useColorModeValue("#F7FAFC", "#2D3748");
  const restoreBorder = useColorModeValue("gray.300", "gray.700");

  const isInitialLoad = useRef(true);

  useEffect(() => {
    loadMemoryStatus(true);
  }, []);

  const loadMemoryStatus = async (initialLoad = false) => {
    try {
      const status: MemoryLimitStatus = await invoke("get_memory_limit_status");
      setMemoryStatus(status);

      if (status.current_limit_mb) {
        const actualLimitGB = (status.physical_memory_mb - status.current_limit_mb) / 1024;
        const matchingOption = status.available_options.find(
          (opt) => Math.abs(opt.limit_gb - actualLimitGB) < 0.05
        );
        if (matchingOption) {
          setSelectedPreset(matchingOption.id);
          if (initialLoad) {
            setTabIndex(0);
          }
        } else if (initialLoad) {
          // Only auto-switch to custom tab on initial load
          setCustomValue(actualLimitGB.toFixed(1));
          setTabIndex(1);
        }
      } else if (initialLoad) {
        setSelectedPreset("");
        setCustomValue("");
      }
    } catch (error) {
      toast({
        title: t("optimization.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleCustomChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    // Allow empty, numbers, and decimal point
    if (val === "" || /^\d*\.?\d{0,2}$/.test(val)) {
      setCustomValue(val);
      setCustomError("");
    }
  };

  const validateCustomValue = (): boolean => {
    if (!customValue.trim()) {
      setCustomError(t("optimization.memoryLimit.invalidInput"));
      return false;
    }
    const num = parseFloat(customValue);
    if (isNaN(num) || num <= 0) {
      setCustomError(t("optimization.memoryLimit.invalidInput"));
      return false;
    }
    const maxGb = memoryStatus?.physical_memory_gb || 0;
    if (num < 0.1 || num > maxGb) {
      setCustomError(t("optimization.memoryLimit.outOfRange"));
      return false;
    }
    setCustomError("");
    return true;
  };

  const applyLimit = async () => {
    let limitGb: number;

    if (tabIndex === 1) {
      // Custom mode
      if (!validateCustomValue()) return;
      limitGb = parseFloat(customValue);
    } else {
      // Preset mode
      if (!selectedPreset) {
        toast({
          title: t("optimization.pleaseSelectOptions"),
          status: "warning",
          duration: 3000,
          isClosable: true,
        });
        return;
      }
      const option = memoryStatus?.available_options.find(
        (opt) => opt.id === selectedPreset
      );
      if (!option) return;
      limitGb = option.limit_gb;
    }

    setIsApplying(true);
    try {
      const result: MemoryLimitResult = await invoke("set_memory_limit", {
        limitGb,
      });

      if (result.success) {
        toast({
          title: t("optimization.memoryLimit.limitApplied"),
          description: `${result.message}\n${t("optimization.memoryLimit.requiresRestart")}`,
          status: "success",
          duration: 7000,
          isClosable: true,
        });
        await loadMemoryStatus();
      }
    } catch (error) {
      toast({
        title: t("optimization.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsApplying(false);
    }
  };

  const restoreLimit = async () => {
    setIsApplying(true);
    try {
      const result: MemoryLimitResult = await invoke("restore_memory_limit");

      if (result.success) {
        toast({
          title: t("optimization.memoryLimit.limitRestored"),
          description: `${result.message}\n${t("optimization.memoryLimit.requiresRestart")}`,
          status: "success",
          duration: 7000,
          isClosable: true,
        });
        setSelectedPreset("");
        setCustomValue("");
        setCustomError("");
        await loadMemoryStatus();
      }
    } catch (error) {
      toast({
        title: t("optimization.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsApplying(false);
    }
  };

  // current_limit_mb is actually the "removememory" value from bcdedit,
  // so we compute the actual limit as: physical - removememory
  const getActualLimitMB = (removememMb: number): number => {
    return (memoryStatus?.physical_memory_mb || 0) - removememMb;
  };
  const formatMemory = (removememMb: number | null) => {
    if (removememMb === null) return t("optimization.memoryLimit.noLimit");
    const actualMb = getActualLimitMB(removememMb);
    return `${(actualMb / 1024).toFixed(1)} GB`;
  };

  const selectOptions = (memoryStatus?.available_options || []).map((opt) => ({
    value: opt.id,
    label: t(`optimization.memoryLimit.options.${opt.id}`),
  }));

  const header = <PageHeader title={t("optimization.memoryLimit.title")} mb={6} />;

  const content = (
    <VStack align="start" spacing={6}>
      {isLoading ? (
        <Text color={subTextColor}>{t("optimization.starting")}</Text>
      ) : (
        <>
          {/* Current Status — no theme color background */}
          <Box w="full">
            <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
              {t("optimization.memoryLimit.currentStatus")}
            </Text>
            <VStack
              align="start"
              spacing={2}
              p={4}
              borderRadius="3xl"
              border="1px solid"
              borderColor={cardBorder}
              w="full"
            >
              <HStack justify="space-between" w="full">
                <Text color={subTextColor} fontSize="sm">
                  {t("optimization.memoryLimit.physicalMemory")}:
                </Text>
                <Text color={textColor} fontWeight="600" fontSize="sm">
                  {memoryStatus?.physical_memory_gb.toFixed(1)} GB
                </Text>
              </HStack>
              <HStack justify="space-between" w="full">
                <Text color={subTextColor} fontSize="sm">
                  {t("optimization.memoryLimit.currentLimit")}:
                </Text>
                <Badge
                  bg={memoryStatus?.current_limit_mb ? "#FF6B9D" : themeColorHex}
                  color="#1a1a1a"
                  fontSize="sm"
                  px={3}
                  py={1}
                  borderRadius="full"
                  fontWeight="600"
                >
                  {formatMemory(memoryStatus?.current_limit_mb || null)}
                </Badge>
              </HStack>
            </VStack>
          </Box>

          {/* Tabs: Preset / Custom */}
          <Box w="full">
            <Text fontWeight="600" color={textColor} fontSize="md" mb={3}>
              {t("optimization.memoryLimit.selectLimit")}
            </Text>
            <Tabs
              index={tabIndex}
              onChange={setTabIndex}
              variant="enclosed"
              w="full"
            >
              <TabList>
                <Tab
                  color={subTextColor}
                  _selected={{ color: headingColor, fontWeight: "600" }}
                >
                  {t("optimization.memoryLimit.presetTab")}
                </Tab>
                <Tab
                  color={subTextColor}
                  _selected={{ color: headingColor, fontWeight: "600" }}
                >
                  {t("optimization.memoryLimit.customTab")}
                </Tab>
              </TabList>
              <TabPanels>
                {/* Preset TabPanel */}
                <TabPanel px={0}>
                  <CustomSelect
                    value={selectedPreset}
                    onChange={(val) => {
                      setSelectedPreset(val);
                    }}
                    options={selectOptions}
                    placeholder={t("optimization.memoryLimit.selectPreset")}
                    width="100%"
                  />
                </TabPanel>
                {/* Custom TabPanel */}
                <TabPanel px={0}>
                  <VStack align="start" spacing={2} w="full">
                    <Input
                      value={customValue}
                      onChange={handleCustomChange}
                      placeholder={t("optimization.memoryLimit.customPlaceholder")}
                      color={headingColor}
                      borderColor={cardBorder}
                      _focus={{ borderColor: themeColorHex }}
                      type="text"
                      inputMode="decimal"
                    />
                    {customError && (
                      <Text color="red.400" fontSize="sm">
                        {customError}
                      </Text>
                    )}
                    <Text color={subTextColor} fontSize="xs">
                      {t("optimization.memoryLimit.outOfRange")}
                    </Text>
                  </VStack>
                </TabPanel>
              </TabPanels>
            </Tabs>
          </Box>

          <Alert
            status="warning"
            borderRadius="xl"
            bg={useColorModeValue("orange.50", "rgba(255, 165, 0, 0.1)")}
            borderLeft="4px solid"
            borderColor="orange.400"
          >
            <AlertIcon as={AlertTriangle} color="orange.500" />
            <AlertDescription color={textColor} fontSize="sm">
              <strong>{t("optimization.memoryLimit.warning")}:</strong>{" "}
              {t("optimization.memoryLimit.warningText")}
            </AlertDescription>
          </Alert>

          <HStack spacing={4} w="full" pt={2}>
            {/* Apply button — theme color */}
            <Button
              bg={themeColorHex}
              color="#1a1a1a"
              size="lg"
              flex={1}
              onClick={applyLimit}
              isLoading={isApplying}
              loadingText={t("optimization.optimizing")}
              leftIcon={<Cpu size={20} />}
              borderRadius="3xl"
              fontWeight="700"
              fontSize="md"
              height="56px"
              boxShadow={`0 4px 20px -5px ${themeColorRgba(0.5)}`}
              _hover={{
                bg: themeColorRgba(0.85),
                boxShadow: `0 6px 25px -5px ${themeColorRgba(0.6)}`,
              }}
              _active={{
                bg: themeColorRgba(0.75),
              }}
              transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
            >
              {t("optimization.memoryLimit.applyLimit")}
            </Button>

            {/* Restore button — white in light, black in dark */}
            <Button
              bg={restoreBg}
              color={restoreColor}
              border="1px solid"
              borderColor={restoreBorder}
              size="lg"
              flex={1}
              onClick={restoreLimit}
              isLoading={isApplying}
              loadingText={t("optimization.optimizing")}
              leftIcon={<AlertTriangle size={20} />}
              borderRadius="3xl"
              fontWeight="700"
              fontSize="md"
              height="56px"
              _hover={{
                bg: restoreHoverBg,
              }}
              _active={{}}
              transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
            >
              {t("optimization.memoryLimit.restoreLimit")}
            </Button>
          </HStack>
        </>
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
          borderColor={cardBorder}
          borderWidth="1px"
          borderRadius="3xl"
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
