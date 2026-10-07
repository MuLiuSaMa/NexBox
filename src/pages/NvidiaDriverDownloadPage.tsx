import { useState, useEffect, useMemo, useCallback } from "react";
import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  useColorModeValue,
  Button,
  Spinner,
  Badge,
  Alert,
  AlertIcon,
  AlertTitle,
  AlertDescription,
  Flex,
  Input,
  InputGroup,
  InputLeftElement,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  Search,
  Monitor,
  Laptop,
  RefreshCw,
  Cpu,
  AlertTriangle,
} from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import nvidiaLogo from "@/assets/nvidia.png";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";

/** 单个驱动版本在某个设备类型下的信息 */
interface DriverClassInfo {
  id: string;
  detail_url: string;
  download_url: string;
}

/** 单个驱动版本（已合并台式机/笔记本两个通道） */
interface DriverEntry {
  version: string;
  branch: string;
  release_date: string;
  name: string;
  is_latest_only: boolean;
  desktop: DriverClassInfo | null;
  laptop: DriverClassInfo | null;
}

interface GpuDetection {
  gpu_name: string;
  series_name: string;
  is_laptop: boolean;
  driver_version: string;
}

export default function NvidiaDriverDownloadPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("download");
  const { liquidGlassEnabled } = useBackground();
  const adaptiveTitle = useAdaptiveTextColor();
  const { config, getActiveColor, getContrastTextColor } = useThemeColor();
  const isDark = useColorModeValue(false, true);

  const [drivers, setDrivers] = useState<DriverEntry[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isDetecting, setIsDetecting] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [detection, setDetection] = useState<GpuDetection | null>(null);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const inputBg = useColorModeValue("white", "#1a1a1a");

  // 1. 初始化：自动检测当前 GPU（不阻塞页面渲染，检测完成后填充信息卡）
  useEffect(() => {
    (async () => {
      try {
        const detected = await invoke<GpuDetection | null>(
          "detect_current_nvidia_gpu"
        );
        if (detected) {
          setDetection(detected);
        }
      } catch {
        /* 检测失败静默处理 */
      } finally {
        setIsDetecting(false);
      }
    })();
  }, []);

  // 2. 拉取驱动列表（同时包含台式机/笔记本两个通道）
  const fetchDrivers = useCallback(async (force = false) => {
    setIsLoading(true);
    setError(null);
    try {
      const list = await invoke<DriverEntry[]>(
        "fetch_nvidia_drivers",
        force ? { forceRefresh: true } : undefined
      );
      setDrivers(list);
    } catch (e) {
      setError(String(e));
      // 仅在还没有任何数据时才清空，避免刷新失败把已有列表也清掉
      setDrivers((prev) => (prev.length > 0 ? prev : []));
    } finally {
      setIsLoading(false);
    }
  }, []);

  // 3. 首次进入时自动拉取
  //    - nvidia-drivers-partial：后端每拿到一个通道就推送增量结果，先到先渲染
  //    - nvidia-drivers-updated：全部通道完成后推送最终结果
  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    const applyList = (payload: DriverEntry[]) => {
      setDrivers(payload);
      setIsLoading(false);
    };
    (async () => {
      try {
        // 先注册监听，再发起拉取，确保增量事件不会漏收
        unlisteners.push(
          await listen<DriverEntry[]>("nvidia-drivers-partial", (event) =>
            applyList(event.payload)
          )
        );
        unlisteners.push(
          await listen<DriverEntry[]>("nvidia-drivers-updated", (event) =>
            applyList(event.payload)
          )
        );
      } catch {
        /* 事件监听失败不影响主流程 */
      }
      if (!cancelled) fetchDrivers();
    })();
    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [fetchDrivers]);

  // 4. 搜索过滤
  const filteredDrivers = useMemo(() => {
    if (!searchQuery.trim()) return drivers;
    const q = searchQuery.toLowerCase();
    return drivers.filter(
      (d) =>
        d.version.toLowerCase().includes(q) ||
        d.branch.toLowerCase().includes(q) ||
        d.release_date.toLowerCase().includes(q)
    );
  }, [drivers, searchQuery]);

  // 5. 打开链接
  const handleOpenDetail = async (url: string) => {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
    } catch (e) {
      console.error("打开链接失败:", e);
      toast({
        title: t("nvidiaDriverDownload.openFailed") || "打开链接失败",
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  return (
    <Box pt={8} pb={8}>
      <VStack spacing={6} align="start">
        {/* 标题栏 */}
        <HStack w="full" justify="space-between">
          <HStack>
            <Box
              as="button"
              onClick={() => navigate(-1)}
              p={1}
              borderRadius="md"
              _hover={{ bg: useColorModeValue("gray.100", "gray.700") }}
            >
              <ArrowLeft size={20} />
            </Box>
            <img
              src={nvidiaLogo}
              width={28}
              height={28}
              style={{ objectFit: "contain" }}
              alt="NVIDIA"
            />
            <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
              {t("nvidiaDriverDownload.title")}
            </Heading>
          </HStack>
        </HStack>

        {/* 检测信息区域 */}
        <LiquidGlassCard
          forceGlass
          p={5}
          borderRadius="xl"
          w="full"
          borderLeft="4px solid"
          borderLeftColor={getActiveColor()}
        >
          <VStack spacing={4} align="start">
            {isDetecting && !detection && (
              <HStack
                spacing={3}
                w="full"
                p={3}
                borderRadius="lg"
                bg={hexToRgba(config.primaryColor, isDark ? 0.08 : 0.05)}
              >
                <Spinner size="sm" color={getActiveColor()} />
                <Text fontSize="sm" color={subTextColor}>
                  {t("nvidiaDriverDownload.detecting")}
                </Text>
              </HStack>
            )}

            {detection && (
              <HStack
                spacing={3}
                w="full"
                p={3}
                borderRadius="lg"
                bg={hexToRgba(config.primaryColor, isDark ? 0.08 : 0.05)}
              >
                <Cpu size={18} color={getActiveColor()} />
                <VStack align="start" spacing={0}>
                  <Text fontSize="sm" fontWeight="medium" color={textColor}>
                    {detection.gpu_name}
                  </Text>
                  <Text fontSize="xs" color={subTextColor}>
                    {t("nvidiaDriverDownload.currentDriver")}:
                    {detection.driver_version || t("nvidiaDriverDownload.unknown")}
                  </Text>
                </VStack>
                <Badge
                  ml="auto"
                  color={getActiveColor()}
                  bg={hexToRgba(config.primaryColor, 0.12)}
                  px={2}
                  borderRadius="full"
                  fontSize="xs"
                >
                  {t("nvidiaDriverDownload.detected")}
                </Badge>
              </HStack>
            )}

            <Text fontSize="xs" color={subTextColor}>
              {t("nvidiaDriverDownload.selectHint")}
            </Text>
          </VStack>
        </LiquidGlassCard>

        {/* 错误提示 */}
        {error && (
          <Alert
            status="error"
            borderRadius="xl"
            bg={useColorModeValue("red.50", "rgba(239, 68, 68, 0.1)")}
          >
            <AlertIcon />
            <Box>
              <AlertTitle fontSize="sm">
                {t("nvidiaDriverDownload.errorTitle")}
              </AlertTitle>
              <AlertDescription fontSize="xs" whiteSpace="pre-wrap">
                {error}
              </AlertDescription>
            </Box>
            <Button
              ml="auto"
              size="sm"
              variant="outline"
              borderColor={borderColor}
              leftIcon={<RefreshCw size={14} />}
              onClick={() => fetchDrivers(true)}
            >
              {t("nvidiaDriverDownload.retry")}
            </Button>
          </Alert>
        )}

        {/* 驱动列表区域 */}
        {!error && (
          <LiquidGlassCard forceGlass p={5} borderRadius="xl" w="full">
            <VStack spacing={4} align="start" w="full">
              {/* 搜索栏 + 计数 */}
              <HStack w="full" justify="space-between">
                <HStack spacing={3}>
                  <Text fontWeight="semibold" color={headingColor} fontSize="md">
                    {t("nvidiaDriverDownload.driverList")}
                  </Text>
                  {drivers.length > 0 && (
                    <Badge
                      colorScheme="green"
                      fontSize="xs"
                      px={2}
                      borderRadius="full"
                    >
                      {t("nvidiaDriverDownload.totalDrivers", {
                        count: drivers.length,
                      })}
                    </Badge>
                  )}
                  {isLoading && drivers.length > 0 && (
                    <HStack spacing={1.5}>
                      <Spinner size="xs" color={getActiveColor()} />
                      <Text fontSize="xs" color={subTextColor}>
                        {t("nvidiaDriverDownload.loading")}
                      </Text>
                    </HStack>
                  )}
                </HStack>
                <HStack spacing={2}>
                  <Button
                    size="sm"
                    variant="outline"
                    borderColor={borderColor}
                    leftIcon={<RefreshCw size={14} />}
                    isDisabled={isLoading}
                    onClick={() => fetchDrivers(true)}
                    _hover={{
                      bg: getActiveColor(),
                      color: getContrastTextColor(),
                      borderColor: getActiveColor(),
                      opacity: 1,
                    }}
                  >
                    {t("nvidiaDriverDownload.refresh")}
                  </Button>
                  <InputGroup maxW="240px">
                    <InputLeftElement pointerEvents="none">
                      <Search size={14} color={subTextColor} />
                    </InputLeftElement>
                    <Input
                      size="sm"
                      bg={inputBg}
                      borderRadius="lg"
                      placeholder={t("nvidiaDriverDownload.searchPlaceholder")}
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                    />
                  </InputGroup>
                </HStack>
              </HStack>

              {/* 加载中（仅在还没有任何数据时才占位，避免刷新时列表消失） */}
              {isLoading && drivers.length === 0 && (
                <Flex w="full" justify="center" align="center" minH="150px">
                  <VStack spacing={3}>
                    <Spinner size="lg" color={getActiveColor()} />
                    <Text fontSize="xs" color={subTextColor}>
                      {t("nvidiaDriverDownload.loading")}
                    </Text>
                  </VStack>
                </Flex>
              )}

              {/* 驱动列表 */}
              <>
                {filteredDrivers.length > 0 && (
                  <VStack spacing={2} align="stretch" w="full" maxH="500px" overflowY="auto">
                    {filteredDrivers.map((driver, idx) => (
                      <Box
                        key={driver.version}
                        p={3}
                        borderRadius="lg"
                        borderWidth="1px"
                        borderColor={useColorModeValue(
                          "rgba(255,255,255,0.5)",
                          "rgba(255,255,255,0.14)"
                        )}
                        bg={useColorModeValue(
                          "rgba(255,255,255,0.35)",
                          "rgba(255,255,255,0.07)"
                        )}
                        sx={{
                          backdropFilter: "blur(14px) saturate(1.3)",
                          WebkitBackdropFilter: "blur(14px) saturate(1.3)",
                        }}
                        _hover={{
                          borderColor: getActiveColor(),
                          bg: useColorModeValue(
                            "rgba(255,255,255,0.5)",
                            "rgba(255,255,255,0.12)"
                          ),
                          opacity: 1,
                        }}
                        transition="all 0.2s"
                      >
                        <HStack spacing={4} w="full">
                          {/* 序号 */}
                          <Text
                            fontSize="xs"
                            color={subTextColor}
                            fontFamily="'MiSans', sans-serif"
                            minW="28px"
                            textAlign="right"
                          >
                            {idx + 1}
                          </Text>

                          {/* 版本号 */}
                          <VStack align="start" spacing={0} flex={1} minW="0">
                            <HStack spacing={2}>
                              <Text
                                fontSize="sm"
                                fontWeight="bold"
                                color={textColor}
                                fontFamily="'MiSans', sans-serif"
                                _hover={{ opacity: 1 }}
                              >
                                {driver.version}
                              </Text>
                              <Badge
                                fontSize="2xs"
                                colorScheme={
                                  driver.branch === "Studio" ? "purple" : "green"
                                }
                                variant="subtle"
                                px={1.5}
                                borderRadius="full"
                              >
                                {driver.branch}
                              </Badge>
                              {driver.is_latest_only && (
                                <Badge
                                  fontSize="2xs"
                                  colorScheme="blue"
                                  variant="subtle"
                                  px={1.5}
                                  borderRadius="full"
                                >
                                  {t("nvidiaDriverDownload.latest")}
                                </Badge>
                              )}
                            </HStack>
                            {driver.release_date && (
                              <Text fontSize="xs" color={subTextColor}>
                                {driver.release_date}
                              </Text>
                            )}
                          </VStack>

                          {/* 台式机 / 笔记本 按钮 */}
                          <HStack spacing={2}>
                            <Button
                              size="sm"
                              leftIcon={<Monitor size={14} />}
                              variant="outline"
                              borderColor={borderColor}
                              color={textColor}
                              isDisabled={!driver.desktop}
                              onClick={() =>
                                driver.desktop &&
                                handleOpenDetail(driver.desktop.detail_url)
                              }
                              _hover={{
                                bg: getActiveColor(),
                                color: getContrastTextColor(),
                                borderColor: getActiveColor(),
                                opacity: 1,
                              }}
                            >
                              {t("nvidiaDriverDownload.desktop")}
                            </Button>
                            <Button
                              size="sm"
                              leftIcon={<Laptop size={14} />}
                              variant="outline"
                              borderColor={borderColor}
                              color={textColor}
                              isDisabled={!driver.laptop}
                              onClick={() =>
                                driver.laptop &&
                                handleOpenDetail(driver.laptop.detail_url)
                              }
                              _hover={{
                                bg: getActiveColor(),
                                color: getContrastTextColor(),
                                borderColor: getActiveColor(),
                                opacity: 1,
                              }}
                            >
                              {t("nvidiaDriverDownload.laptop")}
                            </Button>
                          </HStack>
                        </HStack>
                      </Box>
                    ))}
                  </VStack>
                )}
              </>

              {/* 搜索无结果 */}
              {!isLoading && filteredDrivers.length === 0 && drivers.length > 0 && (
                <Flex w="full" justify="center" align="center" minH="100px">
                  <VStack spacing={2}>
                    <AlertTriangle size={32} color={subTextColor} />
                    <Text fontSize="sm" color={subTextColor}>
                      {t("nvidiaDriverDownload.noSearchResults")}
                    </Text>
                  </VStack>
                </Flex>
              )}

              {/* 无驱动 */}
              {!isLoading && drivers.length === 0 && (
                <Flex w="full" justify="center" align="center" minH="100px">
                  <Text fontSize="sm" color={subTextColor}>
                    {t("nvidiaDriverDownload.noDrivers")}
                  </Text>
                </Flex>
              )}
            </VStack>
          </LiquidGlassCard>
        )}

        {/* 底部提示 */}
        <Box w="full">
          <Text fontSize="xs" color={subTextColor} textAlign="center">
            {t("nvidiaDriverDownload.tip")}
          </Text>
        </Box>
      </VStack>
    </Box>
  );
}
