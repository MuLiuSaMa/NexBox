import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Badge,
  Button,
  Checkbox,
  Spinner,
  Divider,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  Trash2,
  RefreshCw,
  Search,
  FolderOpen,
  CheckCircle2,
  ShieldAlert,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface PsoScanResult {
  exists: boolean;
  path: string;
  item_total: number;
  item_clean: number;
  ini_kept: number;
  size_bytes: number;
  game_running: boolean;
  running_names: string[];
}

interface PsoCleanResult {
  success: boolean;
  message: string;
  freed_bytes: number;
  removed_items: number;
  kept_inis: number;
  reboot_pending: number;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export default function PsoCachePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("layers");
  const navigate = useNavigate();
  const adaptiveTitle = useAdaptiveTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const mutedColor = useColorModeValue("gray.500", "#8a8a8a");
  const statLabelColor = useColorModeValue("gray.500", "#a0a0a0");
  const pathColor = useColorModeValue("gray.700", "#e6e6e6");
  const tipBg = useColorModeValue("rgba(59,130,246,0.05)", "rgba(59,130,246,0.1)");
  const tipBorder = useColorModeValue(
    "rgba(59,130,246,0.2)",
    "rgba(59,130,246,0.25)"
  );
  const tipTitleColor = useColorModeValue("blue.700", "blue.300");
  const tipTextColor = useColorModeValue(
    "gray.600",
    "rgba(200,200,200,0.85)"
  );
  const warnBg = useColorModeValue("rgba(237,137,54,0.08)", "rgba(237,137,54,0.12)");
  const warnBorder = useColorModeValue(
    "rgba(237,137,54,0.3)",
    "rgba(237,137,54,0.35)"
  );
  const warnTextColor = useColorModeValue("orange.700", "orange.300");
  const badgeBg = useColorModeValue("blue.50", "rgba(66,153,225,0.15)");

  const [dir, setDir] = useState<string | null>(null);
  const [scan, setScan] = useState<PsoScanResult | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [cleaning, setCleaning] = useState(false);
  const [permanent, setPermanent] = useState(false);
  const [done, setDone] = useState<string | null>(null);

  const refreshScan = useCallback(
    async (target: string | null) => {
      if (!target) {
        setScan(null);
        return;
      }
      setScanning(true);
      try {
        const r = await invoke<PsoScanResult>("pso_scan", { dir: target });
        setScan(r);
      } catch (error) {
        setScan(null);
        console.error("Failed to scan PSO cache:", error);
      }
      setScanning(false);
    },
    []
  );

  const doDetect = useCallback(
    async (silent = false) => {
      setDetecting(true);
      try {
        const r = await invoke<{ path: string | null; source: string | null }>(
          "pso_detect"
        );
        if (r?.path) {
          setDir(r.path);
          await refreshScan(r.path);
        } else {
          setDir(null);
          setScan(null);
          if (!silent) {
            toast({
              title: t("psoCache.notDetected"),
              description: t("psoCache.notDetectedDesc"),
              status: "warning",
              duration: 3500,
              isClosable: true,
            });
          }
        }
      } catch (error) {
        console.error("Failed to detect PSO cache:", error);
        if (!silent) {
          toast({
            title: t("psoCache.detectError"),
            description: String(error),
            status: "error",
            duration: 3000,
            isClosable: true,
          });
        }
      }
      setDetecting(false);
    },
    [refreshScan, t, toast]
  );

  useEffect(() => {
    // 进入页面静默自动检测一次
    doDetect(true);
  }, [doDetect]);

  const doChoose = async () => {
    setDone(null);
    try {
      const picked = await invoke<string | null>("pso_choose");
      if (picked) {
        setDir(picked);
        await refreshScan(picked);
      }
    } catch (error) {
      console.error("Failed to choose PSO dir:", error);
      toast({
        title: t("psoCache.chooseError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleClean = async () => {
    if (!dir) return;
    setCleaning(true);
    try {
      const r = await invoke<PsoCleanResult>("pso_clean", {
        dir,
        permanent,
      });
      if (r.success) {
        setDone(r.message);
        toast({
          title: t("psoCache.cleanSuccess", { size: formatSize(r.freed_bytes) }),
          description:
            r.reboot_pending > 0
              ? t("psoCache.rebootPending", { count: r.reboot_pending })
              : r.message,
          status: "success",
          duration: 4500,
          isClosable: true,
        });
        await refreshScan(dir);
      } else {
        setDone(null);
        toast({
          title: t("psoCache.cleanError"),
          description: r.message,
          status: "error",
          duration: 4000,
          isClosable: true,
        });
      }
    } catch (error) {
      setDone(null);
      toast({
        title: t("psoCache.cleanError"),
        description: String(error),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    }
    setCleaning(false);
  };

  const gameRunning = !!scan?.game_running;
  const canClean =
    !!scan?.exists && scan.item_clean > 0 && !gameRunning && !scanning && !cleaning;

  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/optimization")}
          color={headingColor}
        >
          {t("psoCache.back")}
        </Button>
        <Heading
          size="lg"
          color={adaptiveTitle.text}
          textShadow={adaptiveTitle.shadow}
          fontWeight="700"
        >
          {t("psoCache.title")}
        </Heading>
        <Box w="100px" />
      </HStack>

      <Text fontSize="sm" color={subTextColor} maxW="720px">
        {t("psoCache.subtitle")}
      </Text>

      {/* 定位与目录 */}
      <LiquidGlassCard w="full">
        <VStack align="stretch" spacing={4} p={5}>
          <HStack spacing={3} flexWrap="wrap">
            <LiquidGlassButton
              size="sm"
              leftIcon={detecting ? <Spinner size="sm" /> : <Search size={16} />}
              onClick={() => doDetect(false)}
              isLoading={detecting}
              loadingText={t("psoCache.detecting")}
              variant="outline"
            >
              {t("psoCache.autoDetect")}
            </LiquidGlassButton>
            <LiquidGlassButton
              size="sm"
              leftIcon={<FolderOpen size={16} />}
              onClick={doChoose}
              variant="outline"
            >
              {t("psoCache.manualSelect")}
            </LiquidGlassButton>
            <LiquidGlassButton
              size="sm"
              leftIcon={<RefreshCw size={16} />}
              onClick={() => refreshScan(dir)}
              isLoading={scanning}
              isDisabled={!dir}
              variant="ghost"
            >
              {t("psoCache.rescan")}
            </LiquidGlassButton>
          </HStack>

          <Box>
            <Text fontSize="xs" color={mutedColor}>
              {t("psoCache.currentPath")}
            </Text>
            <Text
              fontSize="sm"
              fontWeight="600"
              color={pathColor}
              wordBreak="break-all"
              mt={1}
            >
              {dir ?? t("psoCache.noPath")}
            </Text>
          </Box>
        </VStack>
      </LiquidGlassCard>

      {/* 运行中拦截告警 */}
      {scan && gameRunning && (
        <Box p={4} borderRadius="xl" border="1px solid" borderColor={warnBorder} bg={warnBg}>
          <HStack spacing={2} mb={1}>
            <Box color={warnTextColor}>
              <ShieldAlert size={18} />
            </Box>
            <Text fontSize="sm" fontWeight="bold" color={warnTextColor}>
              {t("psoCache.gameRunningTitle", {
                names: scan.running_names.join(" / "),
              })}
            </Text>
          </HStack>
          <Text fontSize="xs" color={tipTextColor}>
            {t("psoCache.gameRunningDesc")}
          </Text>
        </Box>
      )}

      {/* 缓存信息 / 非法目录提示 */}
      {scan && scan.exists ? (
        <LiquidGlassCard w="full">
          <HStack spacing={8} flexWrap="wrap" p={5}>
            <VStack align="start" spacing={0}>
              <Text fontSize="xs" color={statLabelColor}>
                {t("psoCache.statSize")}
              </Text>
              <Text fontSize="xl" fontWeight="800" color={headingColor}>
                {formatSize(scan.size_bytes)}
              </Text>
            </VStack>
            <VStack align="start" spacing={0}>
              <Text fontSize="xs" color={statLabelColor}>
                {t("psoCache.statItems")}
              </Text>
              <Text fontSize="xl" fontWeight="800" color={headingColor}>
                {scan.item_clean}
              </Text>
            </VStack>
            <VStack align="start" spacing={0}>
              <Text fontSize="xs" color={statLabelColor}>
                {t("psoCache.statIni")}
              </Text>
              <Text fontSize="xl" fontWeight="800" color={headingColor}>
                {scan.ini_kept}
              </Text>
            </VStack>
          </HStack>
        </LiquidGlassCard>
      ) : scan && !scanning ? (
        <Box p={4} borderRadius="xl" border="1px solid" borderColor={tipBorder} bg={tipBg}>
          <Text fontSize="sm" color={tipTextColor}>
            {t("psoCache.invalidDir")}
          </Text>
        </Box>
      ) : null}

      {/* 操作 */}
      <LiquidGlassCard w="full">
        <VStack align="stretch" spacing={4} p={5}>
          <Checkbox
            isChecked={permanent}
            colorScheme="red"
            onChange={(e) => setPermanent(e.target.checked)}
            isDisabled={!canClean}
          >
            <Text fontSize="sm" color={headingColor}>
              {t("psoCache.permanent")}
              <Text as="span" color={mutedColor} fontSize="xs" ml={2}>
                {t("psoCache.permanentDesc")}
              </Text>
            </Text>
          </Checkbox>

          <Divider />

          <HStack spacing={3}>
            <LiquidGlassButton
              leftIcon={cleaning ? <Spinner size="sm" /> : <Trash2 size={16} />}
              onClick={handleClean}
              isLoading={cleaning}
              loadingText={t("psoCache.cleaning")}
              isDisabled={!canClean}
              colorScheme="red"
            >
              {t("psoCache.cleanButton")}
            </LiquidGlassButton>
          </HStack>

          {done && (
            <HStack
              key={done}
              spacing={2}
              p={3}
              borderRadius="lg"
              border="1px solid"
              borderColor={tipBorder}
              bg={tipBg}
            >
              <Box color="green.400">
                <CheckCircle2 size={18} />
              </Box>
              <Text fontSize="sm" fontWeight="600" color={headingColor}>
                {done}
              </Text>
            </HStack>
          )}
        </VStack>
      </LiquidGlassCard>

      {/* 说明 */}
      <Box p={5} borderRadius="xl" border="1px solid" borderColor={tipBorder} bg={tipBg}>
        <HStack mb={3}>
          <Badge
            borderRadius="full"
            px={3}
            py={1}
            fontSize="xs"
            colorScheme="blue"
            bg={badgeBg}
          >
            {t("psoCache.tipTitle")}
          </Badge>
        </HStack>
        <VStack align="start" spacing={2} pl={1}>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("psoCache.tip1")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("psoCache.tip2")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("psoCache.tip3")}
          </Text>
        </VStack>
      </Box>
    </VStack>
  );

  return content;
}
