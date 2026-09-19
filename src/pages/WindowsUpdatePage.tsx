import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Badge,
  Button,
  useColorModeValue,
  Spinner,
  SimpleGrid,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Wrench, FileText, BarChart3, ShieldBan, PauseCircle, PlayCircle, Ban, RotateCcw } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface UpdateState {
  services_disabled: boolean;
  policy_set: boolean;
  scheduler_disabled: boolean;
  dlls_renamed: boolean;
  all_disabled: boolean;
}

export default function WindowsUpdatePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("download");
  const navigate = useNavigate();

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const descColor = useColorModeValue("gray.600", "#ffffff");
  const warningBg = useColorModeValue(
    "rgba(229,62,62,0.05)",
    "rgba(229,62,62,0.1)"
  );
  const warningBorder = useColorModeValue(
    "rgba(229,62,62,0.2)",
    "rgba(229,62,62,0.25)"
  );
  const warningTitleColor = useColorModeValue("red.700", "red.300");
  const warningTextColor = useColorModeValue(
    "gray.600",
    "rgba(200,200,200,0.85)"
  );

  const [state, setState] = useState<UpdateState | null>(null);
  const [isChecking, setIsChecking] = useState(true);
  const [isOperating, setIsOperating] = useState(false);
  const [pauseEnabled, setPauseEnabled] = useState<boolean | null>(null);
  const [isPauseChecking, setIsPauseChecking] = useState(true);
  const [isPauseOperating, setIsPauseOperating] = useState(false);
  const [defenderDisabled, setDefenderDisabled] = useState<boolean | null>(null);
  const [isDefenderChecking, setIsDefenderChecking] = useState(true);
  const [isDefenderOperating, setIsDefenderOperating] = useState(false);

  const checkState = useCallback(async () => {
    setIsChecking(true);
    try {
      const result = await invoke<UpdateState>("check_windows_update_state");
      setState(result);
    } catch (error) {
      console.error("Failed to check Windows Update state:", error);
      toast({
        title: t("windowsUpdate.disableError") || "检查状态失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsChecking(false);
  }, [t, toast]);

  useEffect(() => {
    checkState();
  }, [checkState]);

  const checkPauseState = useCallback(async () => {
    setIsPauseChecking(true);
    try {
      const result = await invoke<boolean>("check_pause_update_state");
      setPauseEnabled(result);
    } catch (error) {
      console.error("Failed to check pause state:", error);
      setPauseEnabled(false);
    }
    setIsPauseChecking(false);
  }, []);

  useEffect(() => {
    checkPauseState();
  }, [checkPauseState]);

  const handlePause = async () => {
    setIsPauseOperating(true);
    try {
      await invoke("apply_registry_tweak", { name: "暂停Windows更新" });
      await checkPauseState();
      toast({
        title: t("windowsUpdate.pauseCard.applySuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to pause update:", error);
      toast({
        title: t("windowsUpdate.pauseCard.applyError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsPauseOperating(false);
  };

  const handleRestorePause = async () => {
    setIsPauseOperating(true);
    try {
      await invoke("restore_registry_tweak", { name: "暂停Windows更新" });
      await checkPauseState();
      toast({
        title: t("windowsUpdate.pauseCard.restoreSuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to restore update:", error);
      toast({
        title: t("windowsUpdate.pauseCard.restoreError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsPauseOperating(false);
  };

  const checkDefenderState = useCallback(async () => {
    setIsDefenderChecking(true);
    try {
      const result = await invoke<boolean>("check_defender_state");
      setDefenderDisabled(result);
    } catch (error) {
      console.error("Failed to check Defender state:", error);
      setDefenderDisabled(false);
    }
    setIsDefenderChecking(false);
  }, []);

  useEffect(() => {
    checkDefenderState();
  }, [checkDefenderState]);

  const handleDisableDefender = async () => {
    setIsDefenderOperating(true);
    try {
      await invoke("apply_registry_tweak", { name: "关闭Windows Defender" });
      await checkDefenderState();
      toast({
        title: t("windowsUpdate.defenderCard.applySuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to disable Defender:", error);
      toast({
        title: t("windowsUpdate.defenderCard.applyError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsDefenderOperating(false);
  };

  const handleRestoreDefender = async () => {
    setIsDefenderOperating(true);
    try {
      await invoke("restore_registry_tweak", { name: "关闭Windows Defender" });
      await checkDefenderState();
      toast({
        title: t("windowsUpdate.defenderCard.restoreSuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to restore Defender:", error);
      toast({
        title: t("windowsUpdate.defenderCard.restoreError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsDefenderOperating(false);
  };

  const handleDisable = async () => {
    setIsOperating(true);
    try {
      await invoke("disable_windows_update");
      await checkState();
      toast({
        title: t("windowsUpdate.disableSuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to disable Windows Update:", error);
      toast({
        title: t("windowsUpdate.disableError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsOperating(false);
  };

  const handleEnable = async () => {
    setIsOperating(true);
    try {
      await invoke("enable_windows_update");
      await checkState();
      toast({
        title: t("windowsUpdate.enableSuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to enable Windows Update:", error);
      toast({
        title: t("windowsUpdate.enableError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsOperating(false);
  };


  const StatusCard = ({
    icon: Icon,
    label,
    isGood,
    goodLabel,
    badLabel,
    description,
  }: {
    icon: React.ComponentType<{ size?: number }>;
    label: string;
    isGood: boolean;
    goodLabel: string;
    badLabel: string;
    description: string;
  }) => (
    <LiquidGlassCard w="full">
      <VStack align="start" spacing={3} p={4}>
        <HStack spacing={3}>
          <Icon size={20} />
          <Text fontSize="sm" fontWeight="bold" color={headingColor}>
            {label}
          </Text>
        </HStack>
        <Badge
          variant="subtle"
          colorScheme={isGood ? "green" : "red"}
          borderRadius="full"
          px={3}
          py={1}
          fontSize="xs"
          fontWeight="medium"
        >
          {isGood ? goodLabel : badLabel}
        </Badge>
        <Text fontSize="xs" color={descColor}>
          {description}
        </Text>
      </VStack>
    </LiquidGlassCard>
  );

  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
      {/* Header */}
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/optimization")}
          color={headingColor}
        >
                    返回
        </Button>
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("windowsUpdate.pageTitle")}
        </Heading>
        <Box w="100px" />
      </HStack>

      {/* Description */}
      <Text fontSize="sm" color={subTextColor} textAlign="center">
        {t("windowsUpdate.pageDesc")}
      </Text>

      {/* Status Cards */}
      {isChecking && !state ? (
        <Flex justify="center" align="center" py={10}>
          <Spinner size="md" mr={3} />
          <Text color={subTextColor}>{t("windowsUpdate.checking")}</Text>
        </Flex>
      ) : (
        <>
          <SimpleGrid columns={{ base: 1, md: 2 }} spacing={4}>
            <StatusCard
              icon={Wrench}
              label={t("windowsUpdate.servicesStatus")}
              isGood={state?.services_disabled ?? false}
              goodLabel={t("windowsUpdate.servicesDisabled")}
              badLabel={t("windowsUpdate.servicesRunning")}
              description={
                state?.services_disabled
                  ? t("windowsUpdate.servicesDisabled")
                  : t("windowsUpdate.servicesRunning")
              }
            />
            <StatusCard
              icon={FileText}
              label={t("windowsUpdate.policyStatus")}
              isGood={state?.policy_set ?? false}
              goodLabel={t("windowsUpdate.policyConfigured")}
              badLabel={t("windowsUpdate.policyNotConfigured")}
              description={
                state?.policy_set
                  ? t("windowsUpdate.policyConfigured")
                  : t("windowsUpdate.policyNotConfigured")
              }
            />
            <StatusCard
              icon={ShieldBan}
              label={t("windowsUpdate.dllStatus")}
              isGood={state?.dlls_renamed ?? false}
              goodLabel={t("windowsUpdate.dllRenamed")}
              badLabel={t("windowsUpdate.dllNotRenamed")}
              description={
                state?.dlls_renamed
                  ? t("windowsUpdate.dllRenamed")
                  : t("windowsUpdate.dllNotRenamed")
              }
            />
            <StatusCard
              icon={BarChart3}
              label={t("windowsUpdate.overallStatus")}
              isGood={state?.all_disabled ?? false}
              goodLabel={t("windowsUpdate.allDisabled")}
              badLabel={t("windowsUpdate.notAllDisabled")}
              description={
                state?.all_disabled
                  ? t("windowsUpdate.allDisabled")
                  : t("windowsUpdate.notAllDisabled")
              }
            />
          </SimpleGrid>

          {/* Action Buttons */}
          <HStack spacing={4} justify="center" wrap="wrap">
            <LiquidGlassButton
              leftIcon={
                isOperating ? <Spinner size="sm" /> : <Ban size={18} />
              }
              onClick={handleDisable}
              isLoading={isOperating}
              disabled={isOperating || isChecking}
              colorScheme="red"
              size="lg"
              px={8}
              py={6}
              fontSize="md"
              fontWeight="bold"
            >
              {t("windowsUpdate.disableBtn")}
            </LiquidGlassButton>
            <LiquidGlassButton
              leftIcon={
                isOperating ? <Spinner size="sm" /> : <RotateCcw size={18} />
              }
              onClick={handleEnable}
              isLoading={isOperating}
              disabled={isOperating || isChecking}
              colorScheme="green"
              size="lg"
              px={8}
              py={6}
              fontSize="md"
              fontWeight="bold"
            >
              {t("windowsUpdate.enableBtn")}
            </LiquidGlassButton>
           </HStack>

          {/* Pause Update Card */}
          <LiquidGlassCard w="full">
            <VStack align="start" spacing={4} p={4}>
              <HStack spacing={3}>
                <PauseCircle size={22} color="#DD6B20" />
                <Text fontSize="md" fontWeight="bold" color={headingColor}>
                  {t("windowsUpdate.pauseCard.title")}
                </Text>
              </HStack>
              <Text fontSize="sm" color={descColor}>
                {t("windowsUpdate.pauseCard.desc")}
              </Text>
              {isPauseChecking && pauseEnabled === null ? (
                <HStack spacing={2}>
                  <Spinner size="xs" />
                  <Text fontSize="xs" color={subTextColor}>
                    {t("windowsUpdate.pauseCard.checking")}
                  </Text>
                </HStack>
              ) : (
                <Badge
                  variant="subtle"
                  colorScheme={pauseEnabled ? "orange" : "gray"}
                  borderRadius="full"
                  px={3}
                  py={1}
                  fontSize="xs"
                  fontWeight="medium"
                >
                  {pauseEnabled
                    ? t("windowsUpdate.pauseCard.paused")
                    : t("windowsUpdate.pauseCard.notPaused")}
                </Badge>
              )}
              <HStack spacing={3}>
                <LiquidGlassButton
                  leftIcon={
                    isPauseOperating ? <Spinner size="sm" /> : <PauseCircle size={16} />
                  }
                  onClick={handlePause}
                  isLoading={isPauseOperating}
                  disabled={isPauseOperating || isChecking}
                  colorScheme="orange"
                  size="sm"
                  px={5}
                  py={4}
                  fontSize="sm"
                  fontWeight="bold"
                >
                  {t("windowsUpdate.pauseCard.applyBtn")}
                </LiquidGlassButton>
                <LiquidGlassButton
                  leftIcon={
                    isPauseOperating ? <Spinner size="sm" /> : <PlayCircle size={16} />
                  }
                  onClick={handleRestorePause}
                  isLoading={isPauseOperating}
                  disabled={isPauseOperating || isChecking}
                  colorScheme="green"
                  size="sm"
                  px={5}
                  py={4}
                  fontSize="sm"
                  fontWeight="bold"
                >
                  {t("windowsUpdate.pauseCard.restoreBtn")}
                </LiquidGlassButton>
              </HStack>
              <Box
                p={3}
                borderRadius="lg"
                bg={warningBg}
                border="1px solid"
                borderColor={warningBorder}
                w="full"
              >
                <Text fontSize="xs" fontWeight="bold" color={warningTitleColor} mb={1}>
                  {t("windowsUpdate.pauseCard.manualHintTitle")}
                </Text>
                <Text fontSize="xs" color={warningTextColor} lineHeight="tall">
                  {t("windowsUpdate.pauseCard.manualHint")}
                </Text>
              </Box>
            </VStack>
          </LiquidGlassCard>

          {/* Windows Defender Card */}
          <LiquidGlassCard w="full">
            <VStack align="start" spacing={4} p={4}>
              <HStack spacing={3}>
                <ShieldBan size={22} color="#DD6B20" />
                <Text fontSize="md" fontWeight="bold" color={headingColor}>
                  {t("windowsUpdate.defenderCard.title")}
                </Text>
              </HStack>
              <Text fontSize="sm" color={descColor}>
                {t("windowsUpdate.defenderCard.desc")}
              </Text>
              {isDefenderChecking && defenderDisabled === null ? (
                <HStack spacing={2}>
                  <Spinner size="xs" />
                  <Text fontSize="xs" color={subTextColor}>
                    {t("windowsUpdate.defenderCard.checking")}
                  </Text>
                </HStack>
              ) : (
                <Badge
                  variant="subtle"
                  colorScheme={defenderDisabled ? "green" : "gray"}
                  borderRadius="full"
                  px={3}
                  py={1}
                  fontSize="xs"
                  fontWeight="medium"
                >
                  {defenderDisabled
                    ? t("windowsUpdate.defenderCard.disabled")
                    : t("windowsUpdate.defenderCard.enabled")}
                </Badge>
              )}
              <HStack spacing={3}>
                <LiquidGlassButton
                  leftIcon={
                    isDefenderOperating ? <Spinner size="sm" /> : <Ban size={16} />
                  }
                  onClick={handleDisableDefender}
                  isLoading={isDefenderOperating}
                  disabled={isDefenderOperating || isChecking}
                  colorScheme="red"
                  size="sm"
                  px={5}
                  py={4}
                  fontSize="sm"
                  fontWeight="bold"
                >
                  {t("windowsUpdate.defenderCard.applyBtn")}
                </LiquidGlassButton>
                <LiquidGlassButton
                  leftIcon={
                    isDefenderOperating ? <Spinner size="sm" /> : <RotateCcw size={16} />
                  }
                  onClick={handleRestoreDefender}
                  isLoading={isDefenderOperating}
                  disabled={isDefenderOperating || isChecking}
                  colorScheme="green"
                  size="sm"
                  px={5}
                  py={4}
                  fontSize="sm"
                  fontWeight="bold"
                >
                  {t("windowsUpdate.defenderCard.restoreBtn")}
                </LiquidGlassButton>
              </HStack>
              <Box
                p={3}
                borderRadius="lg"
                bg={warningBg}
                border="1px solid"
                borderColor={warningBorder}
                w="full"
              >
                <Text fontSize="xs" fontWeight="bold" color={warningTitleColor} mb={1}>
                  {t("windowsUpdate.defenderCard.manualHintTitle")}
                </Text>
                <Text fontSize="xs" color={warningTextColor} lineHeight="tall">
                  {t("windowsUpdate.defenderCard.manualHint")}
                </Text>
              </Box>
            </VStack>
          </LiquidGlassCard>
        </>
      )}
    </VStack>
  );

  return content;
}
