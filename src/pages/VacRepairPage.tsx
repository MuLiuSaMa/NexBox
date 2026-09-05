import { useCallback, useEffect, useRef, useState } from "react";
import {
  Alert,
  AlertDescription,
  AlertIcon,
  Box,
  Button,
  Flex,
  Heading,
  HStack,
  IconButton,
  SimpleGrid,
  Spinner,
  Text,
  Tooltip,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowLeft,
  RefreshCw,
  Swords,
  ShieldCheck,
  AlertTriangle,
  Wrench,
  Eraser,
  FolderOpen,
  Gamepad2,
  TerminalSquare,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface VacRepairStatus {
  steam_installed: boolean;
  steam_path: string | null;
  steam_bin_exists: boolean;
  steam_running: boolean;
  is_admin: boolean;
}

/** 输出控制台最大保留行数，防止长时间运行拖垮渲染 */
const MAX_CONSOLE_LINES = 500;

export default function VacRepairPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const toast = useDynamicIsland("shield");

  const [status, setStatus] = useState<VacRepairStatus | null>(null);
  const [isChecking, setIsChecking] = useState(true);
  const [isRepairing, setIsRepairing] = useState(false);
  const [output, setOutput] = useState<string[]>([]);
  const consoleRef = useRef<HTMLDivElement>(null);

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#e0e0e0");
  const subLabelColor = useColorModeValue("gray.500", "#969696");
  const borderColor = useColorModeValue("gray.200", "rgba(255,255,255,0.16)");

  // 订阅后端流式输出
  useEffect(() => {
    let unlistenOutput: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;
    void listen<string>("vac-repair-output", (event) => {
      setOutput((prev) => {
        const next = [...prev, event.payload];
        return next.length > MAX_CONSOLE_LINES ? next.slice(next.length - MAX_CONSOLE_LINES) : next;
      });
    }).then((handler) => { unlistenOutput = handler; });
    void listen<boolean>("vac-repair-done", () => {
      setOutput((prev) => {
        const next = [...prev, `>>> ${t("vacRepair.doneLine")}`];
        return next.length > MAX_CONSOLE_LINES ? next.slice(next.length - MAX_CONSOLE_LINES) : next;
      });
    }).then((handler) => { unlistenDone = handler; });
    return () => {
      unlistenOutput?.();
      unlistenDone?.();
    };
  }, [t]);

  // 新输出时自动滚动到底部
  useEffect(() => {
    if (consoleRef.current) {
      consoleRef.current.scrollTop = consoleRef.current.scrollHeight;
    }
  }, [output]);

  const refreshStatus = useCallback(async () => {
    setIsChecking(true);
    try {
      const next = await invoke<VacRepairStatus>("get_vac_repair_status");
      setStatus(next);
    } catch (error) {
      toast({
        title: t("vacRepair.checkFailed"),
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

  const startRepair = async () => {
    setIsRepairing(true);
    setOutput([]);
    try {
      await invoke("run_vac_repair");
      toast({
        title: t("vacRepair.repairDone"),
        description: t("vacRepair.repairDoneDesc"),
        status: "success",
        duration: 6000,
        isClosable: true,
      });
      await refreshStatus();
    } catch (error) {
      toast({
        title: t("vacRepair.repairFailed"),
        description: String(error),
        status: "error",
        duration: 8000,
        isClosable: true,
      });
    } finally {
      setIsRepairing(false);
    }
  };

  const clearOutput = () => setOutput([]);

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

  const badgeProps = (tone: "ok" | "bad") => ({
    bg: tone === "ok" ? hexToRgba("#38A169", 0.14) : hexToRgba("#E53E3E", 0.14),
    color: tone === "ok" ? "#38A169" : "#E53E3E",
    px: 2.5,
    py: 0.5,
    borderRadius: "full",
    fontSize: "xs",
    fontWeight: 600,
  });

  const canRepair =
    status !== null && status.steam_installed && status.steam_bin_exists;

  return (
    <Box pt={8} w="full">
      <VStack align="stretch" spacing={5} w="full">
        <Flex
          direction={{ base: "column", md: "row" }}
          justify="space-between"
          align={{ base: "stretch", md: "center" }}
          gap={4}
          wrap="wrap"
        >
          <HStack spacing={3} minW={0}>
            <IconButton
              aria-label={t("builtinTools.back")}
              icon={<ArrowLeft size={20} />}
              variant="ghost"
              onClick={() => navigate("/builtin-tools")}
              color={headingColor}
              flexShrink={0}
            />
            <Box minW={0}>
              <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} noOfLines={1}>
                {t("vacRepair.title")}
              </Heading>
              <Text mt={1} fontSize="sm" color={subLabelColor} noOfLines={2}>
                {t("vacRepair.subtitle")}
              </Text>
            </Box>
          </HStack>
          <Button
            size="sm"
            variant="outline"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void refreshStatus()}
            isLoading={isChecking}
            isDisabled={isRepairing}
            alignSelf={{ base: "flex-start", md: "auto" }}
            flexShrink={0}
          >
            {t("vacRepair.refresh")}
          </Button>
        </Flex>

        {!status && isChecking && (
          <LiquidGlassCard p={6} boxShadow="sm" w="full">
            <Flex align="center" justify="center" gap={3} py={4}>
              <Spinner size="sm" color={primaryColor} />
              <Text color={subLabelColor} fontSize="sm">{t("vacRepair.checking")}</Text>
            </Flex>
          </LiquidGlassCard>
        )}

        {status && (
          <>
            {!status.steam_installed && (
              <Alert status="warning" variant="subtle" borderRadius="md" fontSize="sm">
                <AlertIcon />
                <AlertDescription>{t("vacRepair.notInstalled")}</AlertDescription>
              </Alert>
            )}
            {status.steam_installed && !status.steam_bin_exists && (
              <Alert status="error" variant="subtle" borderRadius="md" fontSize="sm">
                <AlertIcon />
                <AlertDescription>{t("vacRepair.serviceMissingHint")}</AlertDescription>
              </Alert>
            )}

            <SimpleGrid columns={{ base: 1, xl: 2 }} spacing={5} w="full">
              <LiquidGlassCard p={5} boxShadow="sm" w="full" h="full">
                <Flex align="center" justify="space-between" mb={1} wrap="wrap" gap={2}>
                  <HStack spacing={2}>
                    <Gamepad2 size={17} color={primaryColor} />
                    <Text color={labelColor} fontSize="md" fontWeight="700">
                      {t("vacRepair.steamInfo")}
                    </Text>
                  </HStack>
                  {status.steam_running && (
                    <Tooltip label={t("vacRepair.steamRunningHint")} placement="top">
                      <Flex
                        align="center"
                        gap={1}
                        bg={hexToRgba("#E5A50A", 0.12)}
                        color="#B7791F"
                        px={2}
                        py={0.5}
                        borderRadius="full"
                      >
                        <AlertTriangle size={12} />
                        <Text fontSize="xs" fontWeight="600">{t("vacRepair.running")}</Text>
                      </Flex>
                    </Tooltip>
                  )}
                </Flex>

                <Box mt={2}>
                  <StatusRow
                    icon={<FolderOpen size={15} />}
                    color={labelColor}
                    label={t("vacRepair.installPath")}
                    value={
                      status.steam_path ? (
                        <Text fontSize="xs" fontWeight="500" noOfLines={1} maxW="260px">
                          {status.steam_path}
                        </Text>
                      ) : (
                        <Box as="span" {...badgeProps("bad")}>{t("vacRepair.notInstalled")}</Box>
                      )
                    }
                  />
                  <StatusRow
                    icon={<ShieldCheck size={15} />}
                    color={status.steam_bin_exists ? "#38A169" : "#E53E3E"}
                    label={t("vacRepair.steamService")}
                    value={
                      <Box as="span" {...badgeProps(status.steam_bin_exists ? "ok" : "bad")}>
                        {status.steam_bin_exists ? t("vacRepair.serviceExists") : t("vacRepair.serviceMissing")}
                      </Box>
                    }
                  />
                  <StatusRow
                    icon={<Swords size={15} />}
                    color={status.steam_running ? "#E5A50A" : "#38A169"}
                    label={t("vacRepair.steamStatus")}
                    value={
                      <Box as="span" {...badgeProps(status.steam_running ? "ok" : "ok")} color={status.steam_running ? "#B7791F" : "#38A169"} bg={status.steam_running ? hexToRgba("#E5A50A", 0.12) : hexToRgba("#38A169", 0.14)}>
                        {status.steam_running ? t("vacRepair.running") : t("vacRepair.notRunning")}
                      </Box>
                    }
                  />
                  <StatusRow
                    icon={<ShieldCheck size={15} />}
                    color={status.is_admin ? "#38A169" : "#E5A50A"}
                    label={t("vacRepair.admin")}
                    value={
                      <Box as="span" {...badgeProps(status.is_admin ? "ok" : "ok")} color={status.is_admin ? "#38A169" : "#B7791F"} bg={status.is_admin ? hexToRgba("#38A169", 0.14) : hexToRgba("#E5A50A", 0.12)}>
                        {status.is_admin ? t("vacRepair.isAdmin") : t("vacRepair.notAdmin")}
                      </Box>
                    }
                  />
                </Box>
              </LiquidGlassCard>

              <LiquidGlassCard p={5} boxShadow="sm" w="full" h="full">
                <HStack spacing={2} mb={3}>
                  <Wrench size={17} color={primaryColor} />
                  <Text color={labelColor} fontSize="md" fontWeight="700">
                    {t("vacRepair.operations")}
                  </Text>
                </HStack>

                <Flex direction={{ base: "column", sm: "row" }} gap={3} w="full" align={{ base: "stretch", sm: "center" }}>
                  <Button
                    flex={1}
                    size="md"
                    leftIcon={<Swords size={16} />}
                    bg={primaryColor}
                    color={contrastText}
                    onClick={() => void startRepair()}
                    isLoading={isRepairing}
                    isDisabled={!canRepair || isChecking}
                    loadingText={t("vacRepair.repairing")}
                    _hover={{ bg: hexToRgba(primaryColor, 0.82) }}
                    w={{ base: "full", sm: "auto" }}
                  >
                    {t("vacRepair.startRepair")}
                  </Button>
                </Flex>

                <Alert status="info" variant="subtle" borderRadius="md" mt={4} fontSize="sm">
                  <AlertIcon />
                  <AlertDescription>{t("vacRepair.firewallHint")}</AlertDescription>
                </Alert>
                <Alert status="warning" variant="subtle" borderRadius="md" mt={2} fontSize="sm">
                  <AlertIcon />
                  <AlertDescription>{t("vacRepair.closeSteamHint")}</AlertDescription>
                </Alert>
              </LiquidGlassCard>
            </SimpleGrid>
          </>
        )}

        <LiquidGlassCard p={5} boxShadow="sm" w="full">
          <Flex align="center" justify="space-between" mb={3} wrap="wrap" gap={2}>
            <HStack spacing={2}>
              <TerminalSquare size={17} color={primaryColor} />
              <Text color={labelColor} fontSize="md" fontWeight="700">
                {t("vacRepair.consoleTitle")}
              </Text>
              {isRepairing && (
                <Spinner size="xs" color={primaryColor} />
              )}
            </HStack>
            <Button
              size="xs"
              variant="ghost"
              leftIcon={<Eraser size={12} />}
              onClick={clearOutput}
              isDisabled={output.length === 0 || isRepairing}
              color={subLabelColor}
            >
              {t("vacRepair.clear")}
            </Button>
          </Flex>
          <Box
            ref={consoleRef}
            bg="rgba(10,12,16,0.82)"
            border="1px solid"
            borderColor="rgba(255,255,255,0.08)"
            borderRadius="md"
            p={3}
            h="280px"
            overflowY="auto"
            fontFamily="Consolas, 'Cascadia Mono', monospace"
            fontSize="xs"
            lineHeight="1.6"
            whiteSpace="pre-wrap"
            wordBreak="break-all"
            color="#b9f6ca"
            sx={{ "&::-webkit-scrollbar": { width: "6px" }, "&::-webkit-scrollbar-thumb": { bg: "rgba(255,255,255,0.16)", borderRadius: "full" } }}
          >
            {output.length === 0 ? (
              <Text color="rgba(255,255,255,0.35)">{t("vacRepair.consoleEmpty")}</Text>
            ) : (
              output.map((line, i) => (
                <div key={i}>{line}</div>
              ))
            )}
          </Box>
        </LiquidGlassCard>
      </VStack>
    </Box>
  );
}