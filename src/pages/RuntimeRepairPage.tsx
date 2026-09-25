import { useEffect, useState } from "react";
import {
  Box,
  Button,
  Flex,
  Heading,
  HStack,
  IconButton,
  Image,
  Progress,
  Text,
  VStack,
  useColorModeValue,
  Tooltip,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowLeft, RefreshCw, CheckCircle2, XCircle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import dotnetLogo from "@/assets/dotnet-framework.webp";
import directxLogo from "@/assets/directx.webp";
import visualStudioLogo from "@/assets/visual-studio.webp";

type RuntimeId = "visual-cpp" | "dotnet" | "directx";

interface RepairProgress {
  runtime_id: RuntimeId;
  phase: "downloading" | "verifying" | "installing" | "complete";
  progress: number;
  detail: string;
}

interface RuntimeStatus {
  id: RuntimeId;
  installed: boolean;
  summary: string;
  missing_components: string[];
}

const runtimes: Array<{
  id: RuntimeId;
  title: string;
  description: string;
  detail: string;
  logo: string;
  /** logo 尺寸（px），默认 36 */
  logoSize?: number;
}> = [
  {
    id: "visual-cpp",
    title: "Microsoft Visual C++ 2008-2026",
    description: "检测并修复游戏常用 VC++ x86 和 x64 运行库",
    detail: "覆盖 2008、2010、2012、2013 及当前 v14 (2015-2026)；v14 同时校验完整运行时 DLL。",
    logo: visualStudioLogo,
  },
  {
    id: "dotnet",
    title: ".NET Framework",
    description: "修复 Microsoft .NET Framework 4.8.1 Runtime",
    detail: "使用微软离线 Runtime 安装包，不安装开发者工具包。",
    logo: dotnetLogo,
    logoSize: 52,
  },
  {
    id: "directx",
    title: "DirectX 9-12 游戏组件",
    description: "补充旧版 DirectX SDK 兼容组件",
    detail: "修复 D3DX、XAudio、XInput 等旧游戏依赖；系统 DirectX 12 由 Windows 更新维护。",
    logo: directxLogo,
  },
];

const phaseLabel: Record<RepairProgress["phase"], string> = {
  downloading: "正在下载",
  verifying: "正在校验 Microsoft 签名",
  installing: "正在安装",
  complete: "已完成",
};

export default function RuntimeRepairPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const toast = useDynamicIsland("wrench");
  const adaptiveTitle = useAdaptiveTextColor();
  const [activeId, setActiveId] = useState<RuntimeId | null>(null);
  const [progress, setProgress] = useState<RepairProgress | null>(null);
  const [statuses, setStatuses] = useState<Record<string, RuntimeStatus>>({});
  const [isChecking, setIsChecking] = useState(true);
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#e0e0e0");
  const subLabelColor = useColorModeValue("gray.500", "#969696");
  const borderColor = useColorModeValue("gray.200", "rgba(255,255,255,0.16)");

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<RepairProgress>("runtime-repair-progress", (event) => {
      setProgress(event.payload);
    }).then((handler) => { unlisten = handler; });
    return () => unlisten?.();
  }, []);

  const refreshStatuses = async () => {
    setIsChecking(true);
    try {
      const next = await invoke<RuntimeStatus[]>("get_runtime_statuses");
      setStatuses(Object.fromEntries(next.map((status) => [status.id, status])));
    } catch (error) {
      toast({ title: "运行库检测失败", description: String(error), status: "error", duration: 5000, isClosable: true });
    } finally {
      setIsChecking(false);
    }
  };

  useEffect(() => {
    void refreshStatuses();
  }, []);

  const repair = async (runtimeId: RuntimeId) => {
    setActiveId(runtimeId);
    setProgress({ runtime_id: runtimeId, phase: "downloading", progress: 0, detail: "准备下载" });
    try {
      const message = await invoke<string>("repair_runtime", { runtimeId });
      toast({ title: "运行库修复完成", description: message, status: "success", duration: 5000, isClosable: true });
      await refreshStatuses();
    } catch (error) {
      toast({ title: "运行库修复失败", description: String(error), status: "error", duration: 7000, isClosable: true });
    } finally {
      setActiveId(null);
    }
  };

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
              <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>运行库修复</Heading>
              <Text mt={1} fontSize="sm" color={subLabelColor}>先检测系统运行库与游戏常用 DLL，仅修复缺失或不完整的项目。</Text>
            </Box>
          </HStack>
          <Button
            size="sm"
            variant="outline"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void refreshStatuses()}
            isLoading={isChecking}
            isDisabled={activeId !== null}
            alignSelf={{ base: "flex-start", md: "auto" }}
            flexShrink={0}
          >
            重新检测
          </Button>
        </Flex>

        <VStack align="stretch" spacing={3} w="full">
          {runtimes.map((runtime) => {
            const isActive = activeId === runtime.id;
            const runtimeProgress = progress?.runtime_id === runtime.id ? progress : null;
            const status = statuses[runtime.id];
            const isComplete = status?.installed === true;
            return (
              <LiquidGlassCard key={runtime.id} p={5} boxShadow="sm" w="full">
                <Flex
                  direction={{ base: "column", md: "row" }}
                  gap={{ base: 3, md: 5 }}
                  align={{ base: "stretch", md: "center" }}
                  w="full"
                >
                  <Box
                    flexShrink={0}
                    w="52px"
                    h="52px"
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    alignSelf={{ base: "flex-start", md: "center" }}
                  >
                    <Image
                      src={runtime.logo}
                      alt={runtime.title}
                      w={`${runtime.logoSize ?? 36}px`}
                      h={`${runtime.logoSize ?? 36}px`}
                      objectFit="contain"
                      draggable={false}
                    />
                  </Box>

                  <VStack align="stretch" spacing={1.5} flex={1} minW={0}>
                    <Tooltip label={runtime.title}>
                      <Text color={labelColor} fontSize="md" fontWeight="700" noOfLines={1}>
                        {runtime.title}
                      </Text>
                    </Tooltip>
                    <Text color={labelColor} fontSize="sm">{runtime.description}</Text>
                    <Text color={subLabelColor} fontSize="xs">{runtime.detail}</Text>
                    {status && !isComplete && (
                      <Tooltip label={status.missing_components.join("\n")}>
                        <Text
                          color={subLabelColor}
                          fontSize="xs"
                          noOfLines={2}
                        >
                          {status.missing_components.join("；")}
                        </Text>
                      </Tooltip>
                    )}
                    {runtimeProgress && (
                      <Box pt={1}>
                        <HStack justify="space-between" mb={1}>
                          <Text color={subLabelColor} fontSize="xs" noOfLines={1}>
                            {phaseLabel[runtimeProgress.phase]}：{runtimeProgress.detail}
                          </Text>
                          <Text color={primaryColor} fontSize="xs" fontWeight="600" flexShrink={0}>
                            {runtimeProgress.progress}%
                          </Text>
                        </HStack>
                        <Progress
                          value={runtimeProgress.progress}
                          h="5px"
                          borderRadius="sm"
                          bg={hexToRgba(primaryColor, 0.14)}
                          sx={{ "& > div": { background: primaryColor } }}
                        />
                      </Box>
                    )}
                  </VStack>

                  <HStack spacing={3} flexShrink={0} align="center">
                    {status && (
                      <HStack spacing={1.5} color={isComplete ? "green.500" : "orange.400"}>
                        {isComplete ? <CheckCircle2 size={15} /> : <XCircle size={15} />}
                        <Text fontSize="xs" fontWeight="600">{status.summary}</Text>
                      </HStack>
                    )}
                    <Button
                      flexShrink={0}
                      size="sm"
                      minW="110px"
                      bg={primaryColor}
                      color={contrastText}
                      onClick={() => void repair(runtime.id)}
                      isLoading={isActive}
                      isDisabled={isChecking || isComplete || (activeId !== null && !isActive)}
                      loadingText="修复中"
                      _hover={{ bg: hexToRgba(primaryColor, 0.82) }}
                    >
                      {isComplete ? "已完整" : "修复缺失项"}
                    </Button>
                  </HStack>
                </Flex>
              </LiquidGlassCard>
            );
          })}
        </VStack>

        <Box borderTop="1px solid" borderColor={borderColor} pt={4}>
          <Text color={subLabelColor} fontSize="xs">安装过程可能需要网络连接，完成后按系统提示重启。</Text>
        </Box>
      </VStack>
    </Box>
  );
}