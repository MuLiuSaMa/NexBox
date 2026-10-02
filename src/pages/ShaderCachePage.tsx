import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Badge,
  Button,
  IconButton,
  useColorModeValue,
  Spinner,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  Circle,
  ChevronDown,
  ChevronUp,
  Trash2,
  RefreshCw,
  ArrowLeft,
} from "lucide-react";
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
  groups: VendorScanResult[];
  total_size: number;
}

/** 扫描完成前的占位分组（保持卡片立即可见，顺序与后端 VENDORS 一致） */
const FALLBACK_VENDORS: VendorScanResult[] = [
  "nvidia",
  "amd",
  "intel",
  "directx",
].map((vendor) => ({ vendor, dirs: [], total_dirs: 0, total_size: 0 }));

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
  vendorKey: string;
  result: VendorScanResult | null;
  isSelected: boolean;
  isExpanded: boolean;
  onToggleSelect: () => void;
  onToggleExpand: () => void;
}) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const mutedColor = useColorModeValue("gray.400", "#8a8a8a");
  const idleBorder = useColorModeValue("gray.200", "#333333");
  const rowBg = useColorModeValue("rgba(0,0,0,0.02)", "rgba(255,255,255,0.03)");
  const selectedBg = useColorModeValue(
    "rgba(72,187,120,0.07)",
    "rgba(72,187,120,0.12)"
  );

  const name = t(`shaderCache.${vendorKey}.name`);
  const description = t(`shaderCache.${vendorKey}.description`);
  const hasDetected = !!result && result.total_dirs > 0;
  const accent = "#38A169";
  const dirs = result?.dirs ?? [];
  // 折叠时把目录路径挂到悬浮提示；展开后列表里已能看到，就不再提示
  const pathsTip =
    !isExpanded && dirs.length ? dirs.map((d) => d.path).join("\n") : undefined;

  return (
    <LiquidGlassCard
      w="full"
      cursor="pointer"
      onClick={onToggleSelect}
      title={pathsTip}
      border="1.5px solid"
      borderColor={isSelected ? accent : idleBorder}
      bg={isSelected ? selectedBg : undefined}
      transition="border-color 0.15s ease, background 0.15s ease"
      _hover={{ borderColor: isSelected ? accent : mutedColor }}
    >
      <HStack spacing={3} px={4} py={3} align="center">
        <Box color={isSelected ? accent : mutedColor} flexShrink={0}>
          {isSelected ? <CheckCircle2 size={20} /> : <Circle size={20} />}
        </Box>

        <VStack align="start" spacing={0} flex={1} minW={0}>
          <HStack spacing={2} maxW="full">
            <Text fontSize="md" fontWeight="bold" color={headingColor}>
              {name}
            </Text>
            <Badge
              borderRadius="full"
              px={2}
              py={0.5}
              fontSize="10px"
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
          </HStack>
          <Text fontSize="xs" color={descColor} noOfLines={1} maxW="full">
            {description}
          </Text>
        </VStack>

        <VStack align="end" spacing={0} flexShrink={0}>
          <Text fontSize="md" fontWeight="bold" color={headingColor}>
            {result ? formatSize(result.total_size) : "0 B"}
          </Text>
          <Text fontSize="xs" color={mutedColor}>
            {t("shaderCache.dirs", { count: result?.total_dirs ?? 0 })}
          </Text>
        </VStack>

        <IconButton
          aria-label={isExpanded ? "collapse" : "expand"}
          icon={isExpanded ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
          size="xs"
          variant="ghost"
          color={mutedColor}
          flexShrink={0}
          onClick={(e) => {
            e.stopPropagation();
            onToggleExpand();
          }}
        />
      </HStack>

      {isExpanded && (
        <VStack align="stretch" spacing={1} px={4} pb={3} pt={0}>
          {dirs.length === 0 ? (
            <Text fontSize="xs" color={mutedColor}>
              {t("shaderCache.notDetected")}
            </Text>
          ) : (
            dirs.map((dir, idx) => (
              <HStack
                key={idx}
                spacing={3}
                px={2.5}
                py={1.5}
                borderRadius="md"
                bg={rowBg}
              >
                <VStack align="start" spacing={0} flex={1} minW={0}>
                  <Text
                    fontSize="xs"
                    fontWeight="semibold"
                    color={headingColor}
                    noOfLines={1}
                  >
                    {dir.name}
                  </Text>
                  <Text fontSize="10px" color={mutedColor} noOfLines={1}>
                    {dir.path}
                  </Text>
                </VStack>
                <Text
                  fontSize="xs"
                  color={dir.exists ? descColor : mutedColor}
                  flexShrink={0}
                >
                  {dir.exists ? formatSize(dir.size_bytes) : "—"}
                </Text>
              </HStack>
            ))
          )}
        </VStack>
      )}
    </LiquidGlassCard>
  );
}

export default function ShaderCachePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("layers");
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
        next.delete(vendor);
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

      <Grid templateColumns={{ base: "1fr", lg: "1fr 1fr" }} gap={3} alignItems="start">
        {(scanResult?.groups ?? FALLBACK_VENDORS).map((group) => (
          <VendorCard
            key={group.vendor}
            vendorKey={group.vendor}
            result={group}
            isSelected={selectedVendors.has(group.vendor)}
            isExpanded={expandedVendor === group.vendor}
            onToggleSelect={() => handleToggleVendor(group.vendor)}
            onToggleExpand={() =>
              setExpandedVendor((prev) =>
                prev === group.vendor ? null : group.vendor
              )
            }
          />
        ))}
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

  return content;
}

function Grid({ children, ...props }: React.ComponentProps<typeof Box>) {
  return <Box display="grid" {...props}>{children}</Box>;
}
