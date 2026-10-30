import {
  Box,
  Heading,
  VStack,
  Text,
  HStack,
  SimpleGrid,
  useColorModeValue,
  Button,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { MousePointer2, Keyboard, Mouse } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface PeripheralStatus {
  mouse_value: number | null;
  keyboard_value: number | null;
  mouse_queue_value: number | null;
}

const MOUSE_OPTIONS = [26, 36, 38, 40];
const KEYBOARD_OPTIONS = [16, 18, 20, 22];
const MOUSE_QUEUE_OPTIONS = [16, 18, 20, 22];
// 推荐档位：鼠标调度 38(Windows 官方「程序」方案)、队列 18(延迟/稳定性平衡)
const RECOMMENDED = {
  mouse: 38,
  keyboard: 18,
  mouseQueue: 18,
} as const;

export default function PeripheralOptimizePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("mouse");
  const { liquidGlassEnabled } = useBackground();

  const [selectedMouse, setSelectedMouse] = useState<number | null>(null);
  const [selectedKeyboard, setSelectedKeyboard] = useState<number | null>(null);
  const [selectedMouseQueue, setSelectedMouseQueue] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isApplying, setIsApplying] = useState(false);
  const [isReverting, setIsReverting] = useState(false);

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
  const optionBg = useColorModeValue(themeColorRgba(0.1), themeColorRgba(0.15));

  useEffect(() => {
    loadCurrentValues();
  }, []);

  const loadCurrentValues = async () => {
    try {
      const status: PeripheralStatus = await invoke("get_peripheral_status");
      if (status.mouse_value !== null && MOUSE_OPTIONS.includes(status.mouse_value)) {
        setSelectedMouse(status.mouse_value);
      }
      if (status.keyboard_value !== null && KEYBOARD_OPTIONS.includes(status.keyboard_value)) {
        setSelectedKeyboard(status.keyboard_value);
      }
      if (
        status.mouse_queue_value !== null &&
        MOUSE_QUEUE_OPTIONS.includes(status.mouse_queue_value)
      ) {
        setSelectedMouseQueue(status.mouse_queue_value);
      }
    } catch (error) {
      console.error("Failed to load peripheral status:", error);
    } finally {
      setIsLoading(false);
    }
  };

  const handleApply = async () => {
    if (selectedMouse === null && selectedKeyboard === null) {
      toast({
        title: t("peripheralOptimize.error"),
        description: "请至少选择一个鼠标或键盘值",
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    setIsApplying(true);
    try {
      await invoke("set_peripheral_settings", {
        mouseValue: selectedMouse ?? 26,
        keyboardValue: selectedKeyboard ?? 16,
        mouseQueueValue: selectedMouseQueue ?? 16,
      });
      toast({
        title: t("peripheralOptimize.success"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("peripheralOptimize.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsApplying(false);
    }
  };

  const handleRevert = async () => {
    setIsReverting(true);
    try {
      await invoke("reset_peripheral_settings");
      setSelectedMouse(null);
      setSelectedKeyboard(null);
      setSelectedMouseQueue(null);
      toast({
        title: t("peripheralOptimize.revertSuccess"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("peripheralOptimize.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsReverting(false);
    }
  };

  function OptionCard({
    value,
    isSelected,
    onClick,
    label,
    desc,
    isRecommended,
  }: {
    value: number;
    isSelected: boolean;
    onClick: () => void;
    label: string;
    desc?: string;
    isRecommended?: boolean;
  }) {
    const content = (
      <Box position="relative" w="full">
        {isRecommended && (
          <Box
            position="absolute"
            top={1.5}
            right={1.5}
            fontSize="10px"
            fontWeight="600"
            px={1.5}
            py={0.5}
            borderRadius="full"
            bg={themeColorHex}
            color={contrastText}
          >
            {t("peripheralOptimize.recommended")}
          </Box>
        )}
        <VStack justify="center" align="center" spacing={2} py={6} px={4}>
          <Box
            w="16px"
            h="16px"
            borderRadius="full"
            border="2px solid"
            borderColor={isSelected ? themeColorHex : subTextColor}
            bg={isSelected ? themeColorHex : "transparent"}
            display="flex"
            alignItems="center"
            justifyContent="center"
          >
            {isSelected && (
              <Box w="5px" h="5px" borderRadius="full" bg="#1a1a1a" />
            )}
          </Box>
          <Text color={headingColor} fontSize="xl" fontWeight="700">
            {value}
          </Text>
          <Text color={subTextColor} fontSize="xs">
            {label}
          </Text>
          {desc && (
            <Text color={subTextColor} fontSize="xs" textAlign="center">
              {desc}
            </Text>
          )}
        </VStack>
      </Box>
    );

    if (liquidGlassEnabled) {
      return (
        <LiquidGlassCard
          p={0}
          cursor="pointer"
          border={isSelected ? `2px solid ${themeColorHex}` : "2px solid transparent"}
          onClick={onClick}
        >
          {content}
        </LiquidGlassCard>
      );
    }

    return (
      <Box
        w="full"
        borderRadius="3xl"
        bg={isSelected ? optionBg : cardBg}
        border="2px solid"
        borderColor={isSelected ? themeColorHex : cardBorder}
        cursor="pointer"
        transition="all 0.2s cubic-bezier(0.4, 0, 0.2, 1)"
        _hover={{ borderColor: themeColorHex }}
        onClick={onClick}
      >
        {content}
      </Box>
    );
  }

  const header = <PageHeader title={t("peripheralOptimize.pageTitle")} mb={6} />;

  const content = (
    <VStack align="start" spacing={8}>
      <Box w="full">
        <HStack spacing={2} mb={1}>
          <MousePointer2 size={18} color={themeColorHex} />
          <Heading as="h3" fontSize="md" fontWeight="bold" color={headingColor}>
            {t("peripheralOptimize.mouseSection")}
          </Heading>
        </HStack>
        <Text color={subTextColor} fontSize="sm" mb={4}>
          {t("peripheralOptimize.mouseDesc")}
        </Text>
        <SimpleGrid columns={{ base: 2, md: 4 }} spacing={3}>
          {MOUSE_OPTIONS.map((val) => (
            <OptionCard
              key={val}
              value={val}
              isSelected={selectedMouse === val}
              onClick={() => setSelectedMouse(val)}
              label="Win32PrioritySeparation"
              desc={t(`peripheralOptimize.mouseValue${val}`)}
              isRecommended={val === RECOMMENDED.mouse}
            />
          ))}
        </SimpleGrid>
      </Box>

      <Box w="full">
        <HStack spacing={2} mb={1}>
          <Keyboard size={18} color={themeColorHex} />
          <Heading as="h3" fontSize="md" fontWeight="bold" color={headingColor}>
            {t("peripheralOptimize.keyboardSection")}
          </Heading>
        </HStack>
        <Text color={subTextColor} fontSize="sm" mb={4}>
          {t("peripheralOptimize.keyboardDesc")}
        </Text>
        <SimpleGrid columns={{ base: 2, md: 4 }} spacing={3}>
          {KEYBOARD_OPTIONS.map((val) => (
            <OptionCard
              key={val}
              value={val}
              isSelected={selectedKeyboard === val}
              onClick={() => setSelectedKeyboard(val)}
              label="KeyboardDataQueueSize"
              desc={t(`peripheralOptimize.keyboardValue${val}`)}
              isRecommended={val === RECOMMENDED.keyboard}
            />
          ))}
        </SimpleGrid>
      </Box>

      <Box w="full">
        <HStack spacing={2} mb={1}>
          <Mouse size={18} color={themeColorHex} />
          <Heading as="h3" fontSize="md" fontWeight="bold" color={headingColor}>
            {t("peripheralOptimize.mouseQueueSection")}
          </Heading>
        </HStack>
        <Text color={subTextColor} fontSize="sm" mb={4}>
          {t("peripheralOptimize.mouseQueueDesc")}
        </Text>
        <SimpleGrid columns={{ base: 2, md: 4 }} spacing={3}>
          {MOUSE_QUEUE_OPTIONS.map((val) => (
            <OptionCard
              key={val}
              value={val}
              isSelected={selectedMouseQueue === val}
              onClick={() => setSelectedMouseQueue(val)}
              label="MouseDataQueueSize"
              desc={t(`peripheralOptimize.mouseQueueValue${val}`)}
              isRecommended={val === RECOMMENDED.mouseQueue}
            />
          ))}
        </SimpleGrid>
      </Box>

      <HStack spacing={3} w="full" justify="center" pt={2}>
        <Button
          size="lg"
          onClick={handleApply}
          isLoading={isApplying}
          loadingText={t("peripheralOptimize.applying")}
          bg={themeColorHex}
          color={contrastText}
          _hover={{ opacity: 0.9 }}
          _active={{ transform: "scale(0.97)" }}
          px={8}
        >
          {t("peripheralOptimize.apply")}
        </Button>
        <Button
          size="lg"
          onClick={handleRevert}
          isLoading={isReverting}
          loadingText={t("peripheralOptimize.reverting")}
          variant="outline"
          borderColor={themeColorHex}
          color={themeColorHex}
          _hover={{ bg: themeColorRgba(0.1) }}
          px={8}
        >
          {t("peripheralOptimize.revert")}
        </Button>
      </HStack>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <Box pt={8}>
        {header}
        <LiquidGlassCard w="full" boxShadow="2xl" overflow="hidden" position="relative" p={6}>
          {content}
        </LiquidGlassCard>
      </Box>
    );
  }

  return (
    <Box pt={8}>
      {header}
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
    </Box>
  );
}
