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
  Divider,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { AnimatePresence, motion } from "framer-motion";
import { useTransitionMode, getVariants, getTransitionConfig } from "@/components/ui/animated-page";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ChevronDown,
  ChevronUp,
  Trash2,
  RefreshCw,
  ArrowLeft,
} from "lucide-react";
import { useBackground } from "@/contexts/background-context";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface ShaderCacheDir {
  name: string;
  path: string;
  exists: boolean;
  size_bytes: number;
}

interface VendorScanResult {
  vendor: string;
  dirs: ShaderCacheDir[];
  total_dirs: number;
  total_size: number;
}

interface ScanResult {
  nvidia: VendorScanResult;
  amd: VendorScanResult;
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function VendorCard({
  vendorKey,
  result,
  isSelected,
  isExpanded,
  onToggleSelect,
  onToggleExpand,
}: {
  vendorKey: "nvidia" | "amd";
  result: VendorScanResult | null;
  isSelected: boolean;
  isExpanded: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
}) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const dirPathColor = useColorModeValue("gray.400", "#666666");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const name =
    vendorKey === "nvidia"
      ? t("shaderCache.nvidia.name")
      : t("shaderCache.amd.name");
  const description =
    vendorKey === "nvidia"
      ? t("shaderCache.nvidia.description")
      : t("shaderCache.amd.description");
  const hasDetected = result && result.total_dirs > 0;

  return (
    <LiquidGlassCard
      w="full"
      cursor="pointer"
      onClick={onToggleExpand}
      position="relative"
      overflow="hidden"
    >
      <VStack align="stretch" spacing={4} p={5}>
        <Flex justify="space-between" align="start">
          <Box>
            <Text fontSize="lg" fontWeight="bold" color={headingColor}>
              {name}
            </Text>
            <Text fontSize="sm" color={descColor} mt={1} maxW="280px">
              {description}
            </Text>
          </Box>
          <HStack spacing={2}>
            <Badge
              variant="outline"
              colorScheme={isSelected ? "green" : "gray"}
              borderRadius="full"
              px={3}
              py={1}
              fontSize="xs"
              fontWeight="medium"
              onClick={(e) => {
                e.stopPropagation();
                onToggleSelect();
              }}
              cursor="pointer"
              _hover={{ transform: "scale(1.05)" }}
              transition="all 0.15s"
            >
              {isSelected
                ? t(`shaderCache.${vendorKey}.selected`)
                : t(`shaderCache.${vendorKey}.selectable`)}
            </Badge>
          </HStack>
        </Flex>

        <AnimatePresence initial={false}>
          {isExpanded && result && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.2, ease: "easeInOut" }}
              style={{ overflow: "hidden" }}
            >
              <VStack align="stretch" spacing={2} mt={2}>
                {result.dirs.map((dir, idx) => (
                  <Box
                    key={idx}
                    p={3}
                    borderRadius="lg"
                    bg={useColorModeValue(
                      "rgba(0,0,0,0.02)",
                      "rgba(255,255,255,0.03)"
                    )}
                    border="1px solid"
                    borderColor={
                      dir.exists
                        ? useColorModeValue(
                            "rgba(0,0,0,0.06)",
                            "rgba(255,255,255,0.08)"
                          )
                        : useColorModeValue(
                            "rgba(0,0,0,0.03)",
                            "rgba(255,255,255,0.05)"
                          )
                    }
                  >
                    <Text fontSize="sm" fontWeight="semibold" color={headingColor}>
                      {dir.name}
                    </Text>
                    <Text fontSize="xs" color={dirPathColor} mt={0.5}>
                      {dir.path}
                    </Text>
                  </Box>
                ))}
              </VStack>
            </motion.div>
          )}
        </AnimatePresence>

        <Divider />

        <HStack spacing={3}>
          <Badge
            borderRadius="full"
            px={3}
            py={1.5}
            fontSize="xs"
            fontWeight="medium"
            colorScheme={hasDetected ? "green" : "gray"}
            bg={useColorModeValue(
              hasDetected ? "green.50" : "gray.50",
              hasDetected ? "rgba(72,187,120,0.1)" : "rgba(128,128,128,0.15)"
            )}
          >
            {hasDetected
              ? t("shaderCache.detected")
              : t("shaderCache.notDetected")}
          </Badge>
          <Badge
            borderRadius="full"
            px={3}
            py={1.5}
            fontSize="xs"
            colorScheme="blue"
            bg={useColorModeValue(
              "blue.50",
              "rgba(66,153,225,0.1)"
            )}
          >
            {result
              ? t("shaderCache.dirs", { count: result.total_dirs })
              : t("shaderCache.dirs", { count: 0 })}
          </Badge>
          <Badge
            borderRadius="full"
            px={3}
            py={1.5}
            fontSize="xs"
            colorScheme="orange"
            bg={useColorModeValue(
              "orange.50",
              "rgba(237,137,54,0.1)"
            )}
          >
            {result ? formatSize(result.total_size) : "0 B"}
          </Badge>
          <Box flex={1} />
          <Flex
            alignItems="center"
            color={useColorModeValue("gray.400", "gray.600")}
          >
            {isExpanded ? (
              <ChevronUp size={16} />
            ) : (
              <ChevronDown size={16} />
            )}
          </Flex>
        </HStack>
      </VStack>
    </LiquidGlassCard>
  );
}

export default function ShaderCachePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("layers");
  const { liquidGlassEnabled } = useBackground();
  const navigate = useNavigate();

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const tipBg = useColorModeValue(
    "rgba(59,130,246,0.05)",
    "rgba(59,130,246,0.1)"
  );
  const tipBorder = useColorModeValue(
    "rgba(59,130,246,0.2)",
    "rgba(59,130,246,0.25)"
  );
  const tipTitleColor = useColorModeValue("blue.700", "blue.300");
  const tipTextColor = useColorModeValue(
    "gray.600",
    "rgba(200,200,200,0.85)"
  );

  const [scanResult, setScanResult] = useState<ScanResult | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [isCleaning, setIsCleaning] = useState(false);
  const [selectedVendors, setSelectedVendors] = useState<Set<string>>(
    new Set(["nvidia"])
  );
  const [expandedVendor, setExpandedVendor] = useState<string | null>(null);

  const doScan = useCallback(async () => {
    setIsScanning(true);
    try {
      const result = await invoke<ScanResult>("scan_shader_caches");
      setScanResult(result);
    } catch (error) {
      console.error("Failed to scan shader caches:", error);
      toast({
        title: t("shaderCache.scanError") || "扫描失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsScanning(false);
  }, [t, toast]);

  useEffect(() => {
    doScan();
  }, [doScan]);

  const handleToggleVendor = (vendor: string) => {
    setSelectedVendors((prev) => {
      const next = new Set(prev);
      if (next.has(vendor)) {
        if (next.size > 1) {
          next.delete(vendor);
        }
      } else {
        next.add(vendor);
      }
      return next;
    });
  };

  const handleClean = async () => {
    if (selectedVendors.size === 0) {
      toast({
        title: t("shaderCache.noVendorSelected"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    setIsCleaning(true);
    let totalFreed = 0;
    let totalRebootPending = 0;
    let successCount = 0;

    for (const vendor of selectedVendors) {
      try {
        const result = await invoke<{
          success: boolean;
          message: string;
          freed_bytes: number;
          reboot_pending_count: number;
        }>("clean_shader_cache", { vendor });
        if (result.success) {
          totalFreed += result.freed_bytes;
          totalRebootPending += result.reboot_pending_count;
          successCount++;
        }
      } catch (error) {
        console.error(`Failed to clean ${vendor}:`, error);
      }
    }

    setIsCleaning(false);

    if (successCount > 0) {
      toast({
        title: t("shaderCache.cleanSuccess", { size: formatSize(totalFreed) }),
        description: totalRebootPending > 0
          ? t("shaderCache.rebootPending", { count: totalRebootPending })
          : undefined,
        status: "success",
        duration: 4000,
        isClosable: true,
      });
    } else {
      toast({
        title: t("shaderCache.cleanError"),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }

    await doScan();
  };

  const transitionMode = useTransitionMode();

  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
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
          {t("shaderCache.title")}
        </Heading>
        <Box w="100px" />
      </HStack>

      <Grid templateColumns={{ base: "1fr", lg: "1fr 1fr" }} gap={5}>
        <VendorCard
          vendorKey="nvidia"
          result={scanResult?.nvidia ?? null}
          isSelected={selectedVendors.has("nvidia")}
          isExpanded={expandedVendor === "nvidia"}
          onToggleSelect={() => handleToggleVendor("nvidia")}
          onToggleExpand={() =>
            setExpandedVendor((prev) =>
              prev === "nvidia" ? null : "nvidia"
            )
          }
        />
        <VendorCard
          vendorKey="amd"
          result={scanResult?.amd ?? null}
          isSelected={selectedVendors.has("amd")}
          isExpanded={expandedVendor === "amd"}
          onToggleSelect={() => handleToggleVendor("amd")}
          onToggleExpand={() =>
            setExpandedVendor((prev) =>
              prev === "amd" ? null : "amd"
            )
          }
        />
      </Grid>

      <HStack spacing={3} justify="start">
        <LiquidGlassButton
          leftIcon={isCleaning ? <Spinner size="sm" /> : <Trash2 size={16} />}
          onClick={handleClean}
          isLoading={isCleaning}
          loadingText={t("shaderCache.cleaning")}
          disabled={isScanning || selectedVendors.size === 0}
          colorScheme="red"
        >
          {t("shaderCache.cleanButton")}
        </LiquidGlassButton>
        <LiquidGlassButton
          leftIcon={<RefreshCw size={16} />}
          onClick={doScan}
          isLoading={isScanning}
          variant="outline"
          colorScheme="gray"
        >
          {t("shaderCache.scanButton")}
        </LiquidGlassButton>
      </HStack>

      <Box
        p={5}
        borderRadius="xl"
        border="1px solid"
        borderColor={tipBorder}
        bg={tipBg}
      >
        <HStack mb={3}>
          <Text fontSize="sm" fontWeight="bold" color={tipTitleColor}>
            {t("shaderCache.officialTip.title")}
          </Text>
        </HStack>
        <VStack align="start" spacing={2} pl={1}>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("shaderCache.officialTip.description")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("shaderCache.officialTip.step1")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("shaderCache.officialTip.step2")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("shaderCache.officialTip.step3")}
          </Text>
        </VStack>
      </Box>
    </VStack>
  );

  return transitionMode !== "off" ? (
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
  );
}

function Grid({ children, ...props }: React.ComponentProps<typeof Box>) {
  return <Box display="grid" {...props}>{children}</Box>;
}
