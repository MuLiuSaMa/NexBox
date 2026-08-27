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
  useDisclosure,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  Divider,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { AnimatePresence, motion } from "framer-motion";
import { useTransitionMode, getVariants, getTransitionConfig } from "@/components/ui/animated-page";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import PowerAdvancedSettingsPanel from "@/components/power/power-advanced-settings-panel";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Zap,
  ArrowLeft,
  RefreshCw,
  Battery,
  CheckCircle,
  Download,
  Play,
  Trash2,
  Cpu,
  Lock,
} from "lucide-react";

import { useNavigate } from "react-router-dom";

interface BuiltinPowerPlan {
  id: string;
  filename: string;
  name: string;
  description: string;
  is_imported: boolean;
  guid: string | null;
  is_active: boolean;
}

interface SystemPowerPlan {
  guid: string;
  name: string;
  is_active: boolean;
}

interface ActivePowerPlan {
  guid: string;
  name: string;
}

interface PowerPlanOperationResult {
  success: boolean;
  message: string;
  guid: string | null;
}

interface LaptopPowerLockStatus {
  unlocked: boolean;
  value: number | null;
}

function SystemPlanCard({
  plan,
  onActivate,
  onDelete,
  activateLoading,
  deleteLoading,
}: {
  plan: SystemPowerPlan;
  onActivate: () => void;
  onDelete: () => void;
  activateLoading: boolean;
  deleteLoading: boolean;
}) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const accentColor = "#805ad5";

  return (
    <LiquidGlassCard
      w="full"
      cursor="default"
      position="relative"
      overflow="hidden"
    >
      <VStack align="stretch" spacing={3} p={4}>
        <Flex justify="space-between" align="center">
          <HStack spacing={3} flex={1}>
            <Box
              w={8}
              h={8}
              borderRadius="lg"
              bg={`${accentColor}20`}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={accentColor}
              flexShrink={0}
            >
              <Cpu size={16} />
            </Box>
            <Box flex={1} minW={0}>
              <Text fontSize="sm" fontWeight="bold" color={headingColor} noOfLines={1}>
                {plan.name}
              </Text>
              <Text fontSize="xs" color={descColor} mt={0.5} fontFamily="mono" noOfLines={1}>
                {plan.guid}
              </Text>
            </Box>
          </HStack>

          <HStack spacing={2} flexShrink={0}>
            {plan.is_active ? (
              <Badge
                borderRadius="full"
                px={2.5}
                py={0.5}
                fontSize="xs"
                fontWeight="bold"
                colorScheme="green"
                bg={useColorModeValue(
                  "green.50",
                  "rgba(72,187,120,0.1)"
                )}
              >
                <HStack spacing={1}>
                  <CheckCircle size={10} />
                  <Text>{t("optimization.powerManagement.active")}</Text>
                </HStack>
              </Badge>
            ) : (
              <>
                <LiquidGlassButton
                  size="xs"
                  leftIcon={<CheckCircle size={12} />}
                  onClick={onActivate}
                  isLoading={activateLoading}
                  loadingText={t("optimization.powerManagement.activating")}
                  colorScheme="green"
                >
                  {t("optimization.powerManagement.activate")}
                </LiquidGlassButton>
                <LiquidGlassButton
                  size="xs"
                  leftIcon={<Trash2 size={12} />}
                  onClick={onDelete}
                  isLoading={deleteLoading}
                  loadingText={t("optimization.powerManagement.deleting")}
                  colorScheme="red"
                  variant="outline"
                >
                  {t("optimization.powerManagement.delete")}
                </LiquidGlassButton>
              </>
            )}
          </HStack>
        </Flex>
      </VStack>
    </LiquidGlassCard>
  );
}

function BuiltinPlanCard({
  plan,
  onImport,
  onImportAndActivate,
  importLoading,
  importActivateLoading,
}: {
  plan: BuiltinPowerPlan;
  onImport: () => void;
  onImportAndActivate: () => void;
  importLoading: boolean;
  importActivateLoading: boolean;
}) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const accentColor = "#F6AD55";

  return (
    <LiquidGlassCard
      w="full"
      cursor="default"
      position="relative"
      overflow="hidden"
    >
      <VStack align="stretch" spacing={3} p={4}>
        <Flex justify="space-between" align="center">
          <HStack spacing={3} flex={1}>
            <Box
              w={8}
              h={8}
              borderRadius="lg"
              bg={`${accentColor}20`}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={accentColor}
              flexShrink={0}
            >
              <Zap size={16} />
            </Box>
            <Box flex={1} minW={0}>
              <Text fontSize="sm" fontWeight="bold" color={headingColor} noOfLines={1}>
                {plan.name}
              </Text>
              <Text fontSize="xs" color={descColor} mt={0.5} noOfLines={2}>
                {plan.description}
              </Text>
            </Box>
          </HStack>

          <HStack spacing={2} flexShrink={0}>
            {plan.is_imported ? (
              <Badge
                borderRadius="full"
                px={2.5}
                py={0.5}
                fontSize="xs"
                fontWeight="medium"
                colorScheme="blue"
                bg={useColorModeValue(
                  "blue.50",
                  "rgba(66,153,225,0.1)"
                )}
              >
                {t("optimization.powerManagement.alreadyImported")}
              </Badge>
            ) : (
              <>
                <Badge
                  borderRadius="full"
                  px={2.5}
                  py={0.5}
                  fontSize="xs"
                  fontWeight="medium"
                  colorScheme="gray"
                  bg={useColorModeValue(
                    "gray.50",
                    "rgba(128,128,128,0.15)"
                  )}
                >
                  {t("optimization.powerManagement.notImported")}
                </Badge>
                <LiquidGlassButton
                  size="xs"
                  leftIcon={<Download size={12} />}
                  onClick={onImport}
                  isLoading={importLoading}
                  loadingText={t("optimization.powerManagement.importing")}
                  colorScheme="orange"
                  variant="outline"
                >
                  {t("optimization.powerManagement.import")}
                </LiquidGlassButton>
                <LiquidGlassButton
                  size="xs"
                  leftIcon={<Play size={12} />}
                  onClick={onImportAndActivate}
                  isLoading={importActivateLoading}
                  loadingText={t("optimization.powerManagement.importing")}
                  colorScheme="orange"
                >
                  {t("optimization.powerManagement.importAndActivate")}
                </LiquidGlassButton>
              </>
            )}
          </HStack>
        </Flex>
      </VStack>
    </LiquidGlassCard>
  );
}

export default function PowerManagementPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("power");

  const navigate = useNavigate();
  const cancelRef = useRef<HTMLButtonElement>(null);
  const adaptiveTitle = useAdaptiveTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  const [builtinPlans, setBuiltinPlans] = useState<BuiltinPowerPlan[]>([]);
  const [systemPlans, setSystemPlans] = useState<SystemPowerPlan[]>([]);
  const [activePlan, setActivePlan] = useState<ActivePowerPlan | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [operating, setOperating] = useState<{ planId: string; action: string } | null>(null);
  const [isImportingAll, setIsImportingAll] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ guid: string; name: string } | null>(null);
  const [laptopLock, setLaptopLock] = useState<LaptopPowerLockStatus | null>(null);
  const [unlockLoading, setUnlockLoading] = useState(false);
  const { isOpen: isDeleteOpen, onOpen: onDeleteOpen, onClose: onDeleteClose } = useDisclosure();

  const loadData = useCallback(async () => {
    setIsLoading(true);
    try {
      const [builtin, system, active] = await Promise.all([
        invoke<BuiltinPowerPlan[]>("get_builtin_power_plans"),
        invoke<SystemPowerPlan[]>("get_system_power_plans"),
        invoke<ActivePowerPlan>("get_active_power_plan").catch(() => null),
      ]);
      setBuiltinPlans(builtin);
      setSystemPlans(system);
      setActivePlan(active);
      invoke<LaptopPowerLockStatus>("get_laptop_power_lock_status")
        .then(setLaptopLock)
        .catch(() => {});
    } catch (error) {
      console.error("Failed to load power plans:", error);
      toast({
        title: t("optimization.powerManagement.loadFailed") || "加载失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  }, [t, toast]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleImport = async (planId: string) => {
    setOperating({ planId, action: "import" });
    try {
      const result: PowerPlanOperationResult = await invoke(
        "import_power_plan",
        { planId }
      );
      if (result.success) {
        toast({
          title: t("optimization.powerManagement.importSuccess") || "导入成功",
          description: result.message,
          status: "success",
          duration: 4000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("optimization.powerManagement.importFailed") || "导入失败",
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setOperating(null);
    await loadData();
  };

  const handleImportAndActivate = async (planId: string) => {
    setOperating({ planId, action: "importAndActivate" });
    try {
      const result: PowerPlanOperationResult = await invoke(
        "import_and_activate_power_plan",
        { planId }
      );
      if (result.success) {
        toast({
          title: t("optimization.powerManagement.activateSuccess"),
          description: result.message,
          status: "success",
          duration: 4000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("optimization.powerManagement.activateFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setOperating(null);
    await loadData();
  };

  const handleActivate = async (guid: string) => {
    setOperating({ planId: guid, action: "activate" });
    try {
      const result: PowerPlanOperationResult = await invoke(
        "activate_power_plan",
        { guid }
      );
      if (result.success) {
        toast({
          title: t("optimization.powerManagement.activateSuccess"),
          description: result.message,
          status: "success",
          duration: 4000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("optimization.powerManagement.activateFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setOperating(null);
    await loadData();
  };

  const handleDeleteClick = (guid: string, name: string) => {
    setDeleteTarget({ guid, name });
    onDeleteOpen();
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    onDeleteClose();
    setOperating({ planId: deleteTarget.guid, action: "delete" });
    try {
      const result: PowerPlanOperationResult = await invoke(
        "delete_power_plan",
        { guid: deleteTarget.guid }
      );
      if (result.success) {
        toast({
          title: t("optimization.powerManagement.deleteSuccess"),
          description: result.message,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("optimization.powerManagement.deleteFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setOperating(null);
    setDeleteTarget(null);
    await loadData();
  };

  const handleUnlockLaptop = async () => {
    setUnlockLoading(true);
    try {
      const result: LaptopPowerLockStatus = await invoke(
        "unlock_laptop_power_plan"
      );
      setLaptopLock(result);
      if (result.unlocked) {
        toast({
          title: t("optimization.powerManagement.laptopUnlockSuccess"),
          description: t("optimization.powerManagement.laptopUnlockRestartHint"),
          status: "success",
          duration: 8000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("optimization.powerManagement.laptopUnlockFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setUnlockLoading(false);
  };

  const handleImportAll = async () => {
    setIsImportingAll(true);
    const unimported = builtinPlans.filter((p) => !p.is_imported);
    if (unimported.length === 0) {
      toast({
        title: t("optimization.powerManagement.allImported") || "所有电源计划已导入",
        status: "info",
        duration: 2000,
        isClosable: true,
      });
      setIsImportingAll(false);
      return;
    }

    let successCount = 0;
    let failCount = 0;
    for (const plan of unimported) {
      try {
        const result: PowerPlanOperationResult = await invoke(
          "import_power_plan",
          { planId: plan.id }
        );
        if (result.success) {
          successCount++;
        } else {
          failCount++;
        }
      } catch {
        failCount++;
      }
    }

    if (successCount > 0) {
      toast({
        title: t("optimization.powerManagement.importAllSuccess", {
          count: successCount,
        }) || `成功导入 ${successCount} 个电源计划`,
        status: "success",
        duration: 4000,
        isClosable: true,
      });
    }
    if (failCount > 0) {
      toast({
        title: t("optimization.powerManagement.importAllFailed", {
          count: failCount,
        }) || `${failCount} 个电源计划导入失败`,
        status: "warning",
        duration: 4000,
        isClosable: true,
      });
    }

    setIsImportingAll(false);
    await loadData();
  };

  const hasUnimported = builtinPlans.some((p) => !p.is_imported);

  const transitionMode = useTransitionMode();

  const content = (
    <VStack align="stretch" spacing={6} pt={8} w="full">
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/optimization")}
          color={headingColor}
        >
                        返回
        </Button>
        <Heading size="lg" color={adaptiveTitle.text} fontWeight="700" textShadow={adaptiveTitle.shadow}>
          {t("optimization.powerManagement.title")}
        </Heading>
        <Box w="100px" />
      </HStack>

      {isLoading ? (
        <Flex justify="center" py={10}>
          <Spinner size="lg" color="#F6AD55" />
        </Flex>
      ) : (
        <>
          {activePlan && (
            <LiquidGlassCard w="full" p={4}>
              <HStack spacing={4}>
                <Box
                  w={10}
                  h={10}
                  borderRadius="xl"
                  bg="rgba(246,173,85,0.15)"
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  color="#F6AD55"
                  flexShrink={0}
                >
                  <Battery size={22} />
                </Box>
                <VStack align="start" spacing={0} flex={1} minW={0}>
                  <Text fontSize="xs" color={subTextColor}>
                    {t("optimization.powerManagement.currentPlan")}
                  </Text>
                  <Text fontSize="md" fontWeight="bold" color={headingColor} noOfLines={1}>
                    {activePlan.name}
                  </Text>
                  <Text fontSize="xs" color={subTextColor} fontFamily="mono" noOfLines={1}>
                    {activePlan.guid}
                  </Text>
                </VStack>
                <LiquidGlassButton
                  size="sm"
                  leftIcon={<RefreshCw size={14} />}
                  onClick={loadData}
                  isLoading={isLoading}
                  variant="outline"
                  colorScheme="gray"
                >
                  {t("optimization.powerManagement.refresh") ||
                    t("shaderCache.scanButton")}
                </LiquidGlassButton>
                <Badge
                  borderRadius="full"
                  px={3}
                  py={1.5}
                  fontSize="xs"
                  fontWeight="bold"
                  colorScheme="green"
                  bg={useColorModeValue(
                    "green.50",
                    "rgba(72,187,120,0.1)"
                  )}
                >
                  <HStack spacing={1}>
                    <CheckCircle size={12} />
                    <Text>{t("optimization.powerManagement.active")}</Text>
                  </HStack>
                </Badge>
              </HStack>
            </LiquidGlassCard>
          )}

          <HStack align="stretch" spacing={6} w="full">
            {/* 左侧：系统电源计划 */}
            <Box flex={1} minW={0}>
              <Box mb={3}>
                <Text fontWeight="600" color={headingColor} fontSize="md">
                  {t("optimization.powerManagement.systemPlans") || "系统电源计划"}
                </Text>
              </Box>
              <VStack align="stretch" spacing={2} maxH="60vh" overflowY="auto">
                <AnimatePresence>
                  {systemPlans.map((plan) => (
                    <motion.div
                      key={plan.guid}
                      initial={{ opacity: 0, x: -10 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ duration: 0.2 }}
                    >
                      <SystemPlanCard
                        plan={plan}
                        onActivate={() => handleActivate(plan.guid)}
                        onDelete={() => handleDeleteClick(plan.guid, plan.name)}
                        activateLoading={operating?.planId === plan.guid && operating?.action === "activate"}
                        deleteLoading={operating?.planId === plan.guid && operating?.action === "delete"}
                      />
                    </motion.div>
                  ))}
                </AnimatePresence>
              </VStack>
            </Box>

            {/* 中间分隔线 */}
            <Divider orientation="vertical" h="auto" />

            {/* 右侧：内置电源计划 */}
            <Box flex={1} minW={0}>
              <HStack justify="space-between" align="center" mb={3}>
                <Text fontWeight="600" color={headingColor} fontSize="md" minW={0} noOfLines={1}>
                  {t("optimization.powerManagement.builtinPlans")}
                </Text>
                <HStack spacing={2} flexShrink={0}>
                  <LiquidGlassButton
                    size="sm"
                    leftIcon={
                      laptopLock?.unlocked ? <CheckCircle size={14} /> : <Lock size={14} />
                    }
                    onClick={handleUnlockLaptop}
                    isLoading={unlockLoading}
                    loadingText={t("optimization.powerManagement.unlocking") || "解锁中"}
                    isDisabled={laptopLock?.unlocked}
                    colorScheme={laptopLock?.unlocked ? "green" : "purple"}
                  >
                    {laptopLock?.unlocked
                      ? t("optimization.powerManagement.laptopUnlockDone")
                      : t("optimization.powerManagement.laptopUnlockButton")}
                  </LiquidGlassButton>
                  {hasUnimported && (
                    <LiquidGlassButton
                      size="sm"
                      leftIcon={<Download size={14} />}
                      onClick={handleImportAll}
                      isLoading={isImportingAll}
                      loadingText={t("optimization.powerManagement.importingAll")}
                      colorScheme="orange"
                    >
                      {t("optimization.powerManagement.importAll")}
                    </LiquidGlassButton>
                  )}
                </HStack>
              </HStack>
              <VStack align="stretch" spacing={2} maxH="60vh" overflowY="auto">
                <AnimatePresence>
                  {builtinPlans.map((plan) => (
                    <motion.div
                      key={plan.id}
                      initial={{ opacity: 0, x: 10 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ duration: 0.2 }}
                    >
                      <BuiltinPlanCard
                        plan={plan}
                        onImport={() => handleImport(plan.id)}
                        onImportAndActivate={() => handleImportAndActivate(plan.id)}
                        importLoading={operating?.planId === plan.id && operating?.action === "import"}
                        importActivateLoading={operating?.planId === plan.id && operating?.action === "importAndActivate"}
                      />
                    </motion.div>
                  ))}
                </AnimatePresence>
              </VStack>
            </Box>
          </HStack>

        </>
      )}

      <PowerAdvancedSettingsPanel />
    </VStack>
  );

  return (
    <>
      {transitionMode !== "off" ? (
        <motion.div
          initial="initial"
          animate="enter"
          exit="exit"
          variants={getVariants(transitionMode)}
          transition={getTransitionConfig(transitionMode)}
        >
          {content}
        </motion.div>
      ) : (
        <div>
          {content}
        </div>
      )}

      <AlertDialog
        isOpen={isDeleteOpen}
        leastDestructiveRef={cancelRef}
        onClose={onDeleteClose}
      >
        <AlertDialogOverlay>
          <AlertDialogContent>
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              {t("optimization.powerManagement.deleteConfirmTitle")}
            </AlertDialogHeader>

            <AlertDialogBody>
              {t("optimization.powerManagement.deleteConfirmText", {
                name: deleteTarget?.name || "",
              })}
            </AlertDialogBody>

            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={onDeleteClose}>
                {t("optimization.powerManagement.cancel")}
              </Button>
              <Button colorScheme="red" onClick={handleDeleteConfirm} ml={3}>
                {t("optimization.powerManagement.delete")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </>
  );
}
