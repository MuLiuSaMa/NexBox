import { useCallback, useEffect, useState } from "react";
import {
  Alert,
  AlertDescription,
  AlertIcon,
  Box,
  Button,
  Flex,
  HStack,
  SimpleGrid,
  Spinner,
  Text,
  Tooltip,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, ShieldCheck, ShieldOff, Cpu, AlertTriangle, RotateCcw, Wrench } from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { useTranslation } from "react-i18next";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";

interface VtxStatus {
  hvci_enabled: boolean;
  vbs_enabled: boolean;
  hypervisor_launch: string | null;
  is_admin: boolean;
  hvci_key_exists: boolean;
  vbs_key_exists: boolean;
}

type ActionKind = "fix" | "restore";

export default function VtxVirtualizationPage() {
  const { t } = useTranslation();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const toast = useDynamicIsland("shield");

  const [status, setStatus] = useState<VtxStatus | null>(null);
  const [isChecking, setIsChecking] = useState(true);
  const [actionKind, setActionKind] = useState<ActionKind | null>(null);

  const labelColor = useColorModeValue("gray.700", "#e0e0e0");
  const subLabelColor = useColorModeValue("gray.500", "#969696");
  const borderColor = useColorModeValue("gray.200", "rgba(255,255,255,0.16)");

  const refreshStatus = useCallback(async () => {
    setIsChecking(true);
    try {
      const next = await invoke<VtxStatus>("check_vtx_virtualization_status");
      setStatus(next);
    } catch (error) {
      toast({
        title: t("vtxVirtualization.checkFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsChecking(false);
    }
  }, [t, toast]);

  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus]);

  const runAction = async (kind: ActionKind) => {
    setActionKind(kind);
    try {
      const command = kind === "fix" ? "fix_vtx_virtualization_popup" : "restore_vtx_virtualization";
      const message = await invoke<string>(command);
      toast({
        title: kind === "fix" ? t("vtxVirtualization.fixDone") : t("vtxVirtualization.restoreDone"),
        description: message,
        status: "success",
        duration: 7000,
        isClosable: true,
      });
      await refreshStatus();
    } catch (error) {
      toast({
        title: kind === "fix" ? t("vtxVirtualization.fixFailed") : t("vtxVirtualization.restoreFailed"),
        description: String(error),
        status: "error",
        duration: 8000,
        isClosable: true,
      });
    } finally {
      setActionKind(null);
    }
  };

  const launchLabel = status?.hypervisor_launch
    ? status.hypervisor_launch
    : t("vtxVirtualization.unknown");

  const launchIsOff = status?.hypervisor_launch
    ? status.hypervisor_launch.toLocaleLowerCase() === "off"
    : null;

  const isFullyFixed =
    status !== null &&
    status.hvci_key_exists &&
    !status.hvci_enabled &&
    status.vbs_key_exists &&
    !status.vbs_enabled &&
    launchIsOff === true;

  const isFullyRestored =
    status !== null &&
    status.hvci_key_exists &&
    status.hvci_enabled &&
    status.vbs_key_exists &&
    status.vbs_enabled &&
    launchIsOff === false;

  const StatusRow = ({
    icon,
    color,
    label,
    value,
  }: {
    icon: React.ReactNode;
    color: string;
    label: string;
    value: React.ReactNode;
  }) => (
    <Flex
      align="center"
      justify="space-between"
      gap={3}
      py={2.5}
      borderBottom="1px solid"
      borderColor={borderColor}
      _last={{ borderBottom: "none" }}
      wrap="wrap"
    >
      <HStack spacing={2.5} minW={0}>
        <Box color={color} flexShrink={0}>{icon}</Box>
        <Text color={labelColor} fontSize="sm" noOfLines={1}>{label}</Text>
      </HStack>
      <Text color={color} fontSize="sm" fontWeight="600" flexShrink={0}>{value}</Text>
    </Flex>
  );

  const badgeProps = (on: boolean) => ({
    bg: on ? hexToRgba("#E53E3E", 0.14) : hexToRgba("#38A169", 0.14),
    color: on ? "#E53E3E" : "#38A169",
    px: 2.5,
    py: 0.5,
    borderRadius: "full",
    fontSize: "xs",
    fontWeight: 600,
  });

  return (
    <Box pt={8} w="full">
      <VStack align="stretch" spacing={5} w="full">
        <PageHeader
          title={t("vtxVirtualization.title")}
          mb={0}
        >
          <Button
            size="sm"
            variant="outline"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void refreshStatus()}
            isLoading={isChecking}
            isDisabled={actionKind !== null}
            alignSelf={{ base: "flex-start", md: "auto" }}
            flexShrink={0}
          >
            {t("vtxVirtualization.refresh")}
          </Button>
        </PageHeader>

        {!status && isChecking && (
          <LiquidGlassCard p={6} boxShadow="sm" w="full">
            <Flex align="center" justify="center" gap={3} py={4}>
              <Spinner size="sm" color={primaryColor} />
              <Text color={subLabelColor} fontSize="sm">{t("vtxVirtualization.checking")}</Text>
            </Flex>
          </LiquidGlassCard>
        )}

        {status && (
          <>
            <SimpleGrid columns={{ base: 1, xl: 2 }} spacing={5} w="full">
              <LiquidGlassCard p={5} boxShadow="sm" w="full" h="full">
              <Flex align="center" justify="space-between" mb={1} wrap="wrap" gap={2}>
                <HStack spacing={2}>
                  <Cpu size={17} color={primaryColor} />
                  <Text color={labelColor} fontSize="md" fontWeight="700">
                    {t("vtxVirtualization.currentStatus")}
                  </Text>
                </HStack>
                {!status.is_admin && (
                  <Tooltip label={t("vtxVirtualization.adminHint")} placement="top">
                    <Flex
                      align="center"
                      gap={1}
                      bg={hexToRgba("#E53E3E", 0.1)}
                      color="#E53E3E"
                      px={2}
                      py={0.5}
                      borderRadius="full"
                    >
                      <AlertTriangle size={12} />
                      <Text fontSize="xs" fontWeight="600">{t("vtxVirtualization.notAdmin")}</Text>
                    </Flex>
                  </Tooltip>
                )}
              </Flex>

              <Box mt={2}>
                <StatusRow
                  icon={<ShieldCheck size={15} />}
                  color={status.hvci_enabled ? "#E53E3E" : "#38A169"}
                  label={t("vtxVirtualization.memoryIntegrity")}
                  value={
                    status.hvci_key_exists ? (
                      <Box as="span" {...badgeProps(status.hvci_enabled)}>
                        {status.hvci_enabled ? t("vtxVirtualization.on") : t("vtxVirtualization.off")}
                      </Box>
                    ) : (
                      <Box as="span" bg={hexToRgba("#969696", 0.14)} color={subLabelColor} px={2.5} py={0.5} borderRadius="full" fontSize="xs" fontWeight={600}>
                        {t("vtxVirtualization.notConfigured")}
                      </Box>
                    )
                  }
                />
                <StatusRow
                  icon={<ShieldOff size={15} />}
                  color={status.vbs_enabled ? "#E53E3E" : "#38A169"}
                  label={t("vtxVirtualization.vbs")}
                  value={
                    status.vbs_key_exists ? (
                      <Box as="span" {...badgeProps(status.vbs_enabled)}>
                        {status.vbs_enabled ? t("vtxVirtualization.on") : t("vtxVirtualization.off")}
                      </Box>
                    ) : (
                      <Box as="span" bg={hexToRgba("#969696", 0.14)} color={subLabelColor} px={2.5} py={0.5} borderRadius="full" fontSize="xs" fontWeight={600}>
                        {t("vtxVirtualization.notConfigured")}
                      </Box>
                    )
                  }
                />
                <StatusRow
                  icon={<Cpu size={15} />}
                  color={launchIsOff === null ? subLabelColor : launchIsOff ? "#38A169" : "#E53E3E"}
                  label={t("vtxVirtualization.hypervisorLaunch")}
                  value={launchLabel}
                />
              </Box>
            </LiquidGlassCard>

            <LiquidGlassCard p={5} boxShadow="sm" w="full" h="full">
              <HStack spacing={2} mb={3}>
                <Wrench size={17} color={primaryColor} />
                <Text color={labelColor} fontSize="md" fontWeight="700">
                  {t("vtxVirtualization.operations")}
                </Text>
              </HStack>

              <Flex
                direction={{ base: "column", sm: "row" }}
                gap={3}
                w="full"
              >
                <Button
                  flex={1}
                  size="md"
                  leftIcon={<Wrench size={16} />}
                  bg={primaryColor}
                  color={contrastText}
                  onClick={() => void runAction("fix")}
                  isLoading={actionKind === "fix"}
                  isDisabled={actionKind !== null || isChecking}
                  loadingText={t("vtxVirtualization.fixing")}
                  _hover={{ bg: hexToRgba(primaryColor, 0.82) }}
                  w={{ base: "full", sm: "auto" }}
                >
                  {t("vtxVirtualization.fix")}
                </Button>
                <Button
                  flex={1}
                  size="md"
                  leftIcon={<RotateCcw size={16} />}
                  variant="outline"
                  borderColor={hexToRgba("#E53E3E", 0.5)}
                  color="#E53E3E"
                  onClick={() => void runAction("restore")}
                  isLoading={actionKind === "restore"}
                  isDisabled={actionKind !== null || isChecking}
                  loadingText={t("vtxVirtualization.restoring")}
                  _hover={{ bg: hexToRgba("#E53E3E", 0.1) }}
                  w={{ base: "full", sm: "auto" }}
                >
                  {t("vtxVirtualization.restore")}
                </Button>
              </Flex>

              <Alert status="info" variant="subtle" borderRadius="md" mt={4} fontSize="sm">
                <AlertIcon />
                <AlertDescription>{t("vtxVirtualization.rebootHint")}</AlertDescription>
              </Alert>
            </LiquidGlassCard>
            </SimpleGrid>

            <LiquidGlassCard p={5} boxShadow="sm" w="full">
              <HStack spacing={2} mb={2}>
                <AlertTriangle size={16} color="#E5A50A" />
                <Text color={labelColor} fontSize="sm" fontWeight="700">
                  {t("vtxVirtualization.limitationTitle")}
                </Text>
              </HStack>
              <Text color={subLabelColor} fontSize="xs" lineHeight="1.7">
                {t("vtxVirtualization.limitationBody")}
              </Text>
            </LiquidGlassCard>
          </>
        )}

        {isFullyFixed && (
          <Alert status="success" variant="subtle" borderRadius="md" fontSize="sm">
            <AlertIcon />
            <AlertDescription>{t("vtxVirtualization.fullyFixed")}</AlertDescription>
          </Alert>
        )}
        {isFullyRestored && (
          <Alert status="info" variant="subtle" borderRadius="md" fontSize="sm">
            <AlertIcon />
            <AlertDescription>{t("vtxVirtualization.fullyRestored")}</AlertDescription>
          </Alert>
        )}
      </VStack>
    </Box>
  );
}
