import { useState, useEffect } from "react";
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
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, Cpu, Settings2, AlertTriangle, RefreshCw, Monitor, Eye } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { CustomSelect } from "@/components/special/custom-select";
import nvidiaLogo from "@/assets/nvidia.png";
import { useBackground } from "@/contexts/background-context";
import { IS_STORE_BUILD } from "@/lib/build-flags";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";

interface SettingOption {
  value: number;
  label: string;
}

interface NvApiStatus {
  available: boolean;
  error: string | null;
}

interface CandidateDllInfo {
  path: string;
  exists: boolean;
  size: number;
  loaded: boolean;
  has_initialize: boolean;
  export_count: number;
  company: string | null;
  version: string | null;
}

interface NvApiDiagnostic {
  nvapi64_exists: boolean;
  nvapi64_size: number;
  nvapi32_exists: boolean;
  nvapi32_size: number;
  dll_loaded: boolean;
  loaded_module_path: string | null;
  has_initialize: boolean;
  has_unload: boolean;
  has_drs_create: boolean;
  last_error: number;
  conclusion: string;
  suggestion: string;
  file_company: string | null;
  file_product: string | null;
  file_version: string | null;
  exports: string[];
  export_count: number;
  candidate_dlls: CandidateDllInfo[];
}

interface NvidiaSetting {
  id: number;
  name: string;
  description: string;
  current_value: number;
  default_value: number;
  options: SettingOption[];
}

const SETTING_IDS: Record<string, number> = {
  // 同步与显示
  VSYNCMODE: 0x00a879cf,
  FRL_FPS: 0x10835002,
  VRR_APP_OVERRIDE: 0x10a879cf,
  VRR_MODE: 0x1194f158,
  REFRESH_RATE_OVERRIDE: 0x0064b541,
  VSYNCTEARCONTROL: 0x005a375c,
  OGL_TRIPLE_BUFFER: 0x20fdd1f9,
  // 画质与纹理
  QUALITY_ENHANCEMENTS: 0x00ce2691,
  ANISO_MODE_LEVEL: 0x101e61a9,
  AA_MODE_METHOD: 0x10d773d2,
  FXAA_ENABLE: 0x1074c972,
  AO_MODE: 0x00667329,
  MFAA: 0x0098c1ac,
  SHADERDISKCACHE: 0x00198fff,
  AA_MODE_SELECTOR: 0x107efc5b,
  AA_MODE_ALPHATOCOVERAGE: 0x10fc2d9c,
  TEXFILTER_ANISO_OPTS2: 0x00e73211,
  TEXFILTER_NO_NEG_LODBIAS: 0x0019bb68,
  AUTO_LODBIASADJUST: 0x00638e8f,
  TEXFILTER_BILINEAR_IN_ANISO: 0x0084cd70,
  TEXFILTER_DISABLE_TRILIN_SLOPE: 0x002ecaf2,
  // 电源与性能
  PREFERRED_PSTATE: 0x1057eb71,
  PRERENDERLIMIT: 0x007ba09e,
  OGL_THREAD_CONTROL: 0x20c1221e,
  VRPRERENDERLIMIT: 0x10111133,
  BATTERY_BOOST_APP_FPS: 0x10115c8c,
  // 同步与显示 (追加)
  VSYNC_BEHAVIOR_FLAGS: 0x10fdec23,
  VSYNCVRRCONTROL: 0x10a879ce,
  // 画质与纹理 (追加)
  AA_BEHAVIOR_FLAGS: 0x10ecdb82,
  AA_MODE_REPLAY: 0x10d48a85,
  ANISO_MODE_SELECTOR: 0x10d2bb16,
  PREVENT_UI_AF_OVERRIDE: 0x103bccb5,
  // 指示器与覆盖
  VRRFEATUREINDICATOR: 0x1094f157,
  VRROVERLAYINDICATOR: 0x1095f16f,
  VRRREQUESTSTATE: 0x1094f1f7,
  FXAA_INDICATOR_ENABLE: 0x1068fb9c,
  PHYSXINDICATOR: 0x1094f16f,
  EXPORT_PERF_COUNTERS: 0x108f0841,
  // 电源与性能 (追加)
  EXTERNAL_QUIET_MODE: 0x10115c8d,
  // 新增常用设置
  FXAA_ALLOW: 0x1034cb89,
  QUALITY_ENHANCEMENT_SUBSTITUTION: 0x00ce2692,
  AO_MODE_ACTIVE: 0x00664339,
  VSYNCSMOOTHAFR: 0x101ae763,
  LATENCY_INDICATOR_AUTOALIGN: 0x1095f170,
  SHADERDISKCACHE_MAX_SIZE: 0x00ac8497,
};

export default function NvidiaDriverPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("gpu");
  const { liquidGlassEnabled } = useBackground();
  const { config, getActiveColor, getContrastTextColor } = useThemeColor();
  const isDark = useColorModeValue(false, true);
  const adaptiveTitle = useAdaptiveTextColor();

  const [isLoading, setIsLoading] = useState(true);
  const [isNvidiaAvailable, setIsNvidiaAvailable] = useState<boolean | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [diagnostic, setDiagnostic] = useState<NvApiDiagnostic | null>(null);
  const [driverVersion, setDriverVersion] = useState<string>("");
  const [driverBranch, setDriverBranch] = useState<string>("");
  const [gpuName, setGpuName] = useState<string>("NVIDIA GPU");
  const [settings, setSettings] = useState<NvidiaSetting[]>([]);
  const [modifiedSettings, setModifiedSettings] = useState<Record<number, number>>({});
  const [isSaving, setIsSaving] = useState(false);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const sectionBg = useColorModeValue("gray.50", "#161616");

  useEffect(() => {
    loadData();
  }, []);

  async function loadData() {
    setIsLoading(true);
    setLoadError(null);
    try {
      // 1. 确保硬件缓存已填充，然后检测是否有 NVIDIA 显卡
      try { await invoke("get_hardware"); } catch (_) {}
      const available = await invoke<boolean>("is_nvidia_gpu");
      setIsNvidiaAvailable(available);
      if (!available) {
        setIsLoading(false);
        return;
      }

      // 2. 检查 NVAPI 是否真正可用（硬件检测到 NVIDIA ≠ NVAPI 库可用）
      const nvapiStatus = await invoke<NvApiStatus>("get_nvapi_status");
      if (!nvapiStatus.available) {
        const reason = nvapiStatus.error || "未知原因";
        // 运行诊断以获取详细信息
        try {
          const diag = await invoke<NvApiDiagnostic>("diagnose_nvapi");
          setDiagnostic(diag);
          setLoadError(diag.conclusion || reason);
        } catch (_) {
          setLoadError(reason);
        }
        setIsLoading(false);
        return;
      }

      // 3. 获取驱动版本和 GPU 名称
      try {
        const info = await invoke<{ version: number; branch: string; gpu_name: string }>("get_nvidia_driver_version");
        // NVIDIA driver version: e.g., 56590 → "565.90"
        const ver = info.version || 0;
        setDriverVersion(ver > 99 ? `${Math.floor(ver / 100)}.${(ver % 100).toString().padStart(2, '0')}` : ver.toString());
        setDriverBranch(info.branch || "");
        if (info.gpu_name && info.gpu_name !== "NVIDIA GPU") {
          setGpuName(info.gpu_name);
        }
      } catch (_) {
        // 老版本 NVAPI 可能不支持
      }

      // 4. 读取所有设置
      const allSettings = await invoke<NvidiaSetting[]>("list_nvidia_settings");
      setSettings(allSettings);

    } catch (e) {
      console.error("加载 NVIDIA 设置失败:", e);
      setLoadError(`加载 NVIDIA 设置时出错: ${String(e)}`);
    } finally {
      setIsLoading(false);
    }
  }

  function getModifiedValue(settingId: number): number {
    if (settingId in modifiedSettings) {
      return modifiedSettings[settingId];
    }
    const setting = settings.find((s) => s.id === settingId);
    return setting?.current_value ?? 0;
  }

  function handleChange(settingId: number, value: number) {
    setModifiedSettings((prev) => ({ ...prev, [settingId]: value }));
  }

  function hasChanges(): boolean {
    if (Object.keys(modifiedSettings).length === 0) return false;
    for (const [id, value] of Object.entries(modifiedSettings)) {
      const original = settings.find((s) => s.id === Number(id));
      if (!original || original.current_value !== value) return true;
    }
    return false;
  }

  async function handleApply() {
    setIsSaving(true);
    try {
      let changed = 0;
      for (const [idStr, value] of Object.entries(modifiedSettings)) {
        const id = Number(idStr);
        const original = settings.find((s) => s.id === id);
        if (original && original.current_value !== value) {
          await invoke("set_nvidia_setting", { settingId: id, value });
          changed++;
        }
      }

      if (changed > 0) {
        toast({
          title: "设置已应用",
          description: `已修改 ${changed} 个 NVIDIA 驱动设置`,
          status: "success",
          duration: 2000,
          isClosable: true,
        });
        // 重新加载
        setModifiedSettings({});
        await loadData();
      } else {
        toast({
          title: "未检测到更改",
          status: "info",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (e) {
      toast({
        title: "应用失败",
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsSaving(false);
    }
  }

  async function handleReset() {
    setIsSaving(true);
    try {
      await invoke("reset_nvidia_settings");
      toast({
        title: "已恢复默认",
        description: "所有 NVIDIA 图形设置已恢复为默认值",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      setModifiedSettings({});
      await loadData();
    } catch (e) {
      toast({
        title: "恢复失败",
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsSaving(false);
    }
  }

  // === 渲染单个设置项（统一使用选择框） ===
  function renderSettingControl(setting: NvidiaSetting) {
    const currentVal = getModifiedValue(setting.id);

    const options = setting.options.map(opt => ({
      value: opt.value.toString(),
      label: opt.label,
    }));
    return (
      <Box w="full">
        <CustomSelect
          value={currentVal.toString()}
          onChange={(val) => handleChange(setting.id, Number(val))}
          options={options}
          width="100%"
        />
      </Box>
    );
  }

  // === 渲染设置分组 ===
  function renderSettingGroup(title: string, icon: React.ReactNode, settingIds: number[]) {
    const groupSettings = settings.filter((s) => settingIds.includes(s.id));
    if (groupSettings.length === 0) return null;

    return (
      <LiquidGlassCard
        p={5}
        borderRadius="xl"
        w="full"
      >
        <HStack mb={4} spacing={2}>
          {icon}
          <Text fontWeight="semibold" color={headingColor} fontSize="md">
            {title}
          </Text>
        </HStack>
        <VStack spacing={4} align="stretch">
          {groupSettings.map((setting) => (
            <Box key={setting.id}>
              <HStack justify="space-between" mb={1}>
                <Text fontWeight="medium" fontSize="sm" color={textColor}>
                  {setting.name}
                </Text>
                <Text fontSize="xs" color={subTextColor}>
                  {setting.description}
                </Text>
              </HStack>
              {renderSettingControl(setting)}
            </Box>
          ))}
        </VStack>
      </LiquidGlassCard>
    );
  }

  // === 加载中 ===
  if (isLoading) {
    return (
      <Box pt={8}>
        <Flex justify="center" align="center" minH="200px">
          <Spinner size="xl" color={getActiveColor()} />
        </Flex>
      </Box>
    );
  }

  // === 无 NVIDIA 显卡 ===
  if (isNvidiaAvailable === false) {
    return (
      <Box pt={8}>
        <VStack spacing={6} align="start">
          <HStack>
            <Box as="button" onClick={() => navigate(-1)} p={1} borderRadius="md"
              _hover={{ bg: useColorModeValue("gray.100", "gray.700") }}>
              <ArrowLeft size={20} />
            </Box>
            <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
              显卡设置（仅 NVIDIA）
            </Heading>
          </HStack>

          <Alert status="warning" borderRadius="xl" bg={useColorModeValue("orange.50", "rgba(255, 152, 0, 0.1)")}>
            <AlertIcon />
            <Box>
              <AlertTitle>未检测到 NVIDIA 显卡</AlertTitle>
              <AlertDescription>
                此功能仅适用于搭载 NVIDIA 独立显卡的设备。
                请确认已正确安装 NVIDIA 显卡驱动，且当前使用的是 NVIDIA GPU。
              </AlertDescription>
            </Box>
          </Alert>
        </VStack>
      </Box>
    );
  }

  // === NVAPI 不可用（检测到 NVIDIA 显卡但 NVAPI 库无法加载） ===
  if (loadError) {
    return (
      <Box pt={8}>
        <VStack spacing={6} align="start">
          <HStack>
            <Box as="button" onClick={() => navigate(-1)} p={1} borderRadius="md"
              _hover={{ bg: useColorModeValue("gray.100", "gray.700") }}>
              <ArrowLeft size={20} />
            </Box>
            <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
              显卡设置（仅 NVIDIA）
            </Heading>
          </HStack>

          <LiquidGlassCard
            p={6}
            borderRadius="xl"
            w="full"
          >
            <VStack spacing={4} align="start">
              <HStack spacing={3}>
                <Box color={useColorModeValue("orange.500", "orange.300")}>
                  <AlertTriangle size={32} />
                </Box>
                <VStack align="start" spacing={0}>
                  <Text fontWeight="bold" color={headingColor} fontSize="md">
                    NVAPI 加载失败
                  </Text>
                  <Text fontSize="xs" color={subTextColor}>
                    已检测到 NVIDIA 显卡，但无法加载 NVAPI 接口
                  </Text>
                </VStack>
              </HStack>

              {/* 诊断结论 */}
              <Alert status="error" borderRadius="lg" bg={useColorModeValue("red.50", "rgba(239, 68, 68, 0.1)")}>
                <AlertIcon />
                <Box>
                  <AlertTitle fontSize="sm">诊断结果</AlertTitle>
                  <AlertDescription fontSize="xs" whiteSpace="pre-wrap">
                    {loadError}
                  </AlertDescription>
                </Box>
              </Alert>

              {/* 诊断详细信息 */}
              {diagnostic && (
                <Box
                  w="full"
                  p={4}
                  borderRadius="lg"
                  bg={useColorModeValue("gray.50", "#111111")}
                  borderWidth="1px"
                  borderColor={borderColor}
                >
                  <Text fontWeight="semibold" fontSize="xs" color={subTextColor} mb={2}>
                    系统诊断详情
                  </Text>
                  <VStack spacing={1} align="start" w="full">
                    <HStack justify="space-between" w="full">
                      <Text fontSize="xs" color={subTextColor}>nvapi64.dll 存在</Text>
                      <Badge colorScheme={diagnostic.nvapi64_exists ? "green" : "red"} fontSize="2xs">
                        {diagnostic.nvapi64_exists ? "是" : "否"}
                      </Badge>
                    </HStack>
                    {diagnostic.nvapi64_exists && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>nvapi64.dll 大小</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono">
                          {(diagnostic.nvapi64_size / 1024).toFixed(1)} KB
                        </Text>
                      </HStack>
                    )}
                    {diagnostic.file_company && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>文件公司</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono">
                          {diagnostic.file_company}
                        </Text>
                      </HStack>
                    )}
                    {diagnostic.file_product && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>产品名称</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono">
                          {diagnostic.file_product}
                        </Text>
                      </HStack>
                    )}
                    {diagnostic.file_version && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>文件版本</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono">
                          {diagnostic.file_version}
                        </Text>
                      </HStack>
                    )}
                    <HStack justify="space-between" w="full">
                      <Text fontSize="xs" color={subTextColor}>DLL 加载成功</Text>
                      <Badge colorScheme={diagnostic.dll_loaded ? "green" : "red"} fontSize="2xs">
                        {diagnostic.dll_loaded ? "是" : "否"}
                      </Badge>
                    </HStack>
                    {diagnostic.loaded_module_path && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>加载路径</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono" wordBreak="break-all" maxW="60%">
                          {diagnostic.loaded_module_path}
                        </Text>
                      </HStack>
                    )}
                    <HStack justify="space-between" w="full">
                      <Text fontSize="xs" color={subTextColor}>NvAPI_Initialize 导出</Text>
                      <Badge colorScheme={diagnostic.has_initialize ? "green" : "red"} fontSize="2xs">
                        {diagnostic.has_initialize ? "存在" : "缺失"}
                      </Badge>
                    </HStack>
                    <HStack justify="space-between" w="full">
                      <Text fontSize="xs" color={subTextColor}>导出函数总数</Text>
                      <Text fontSize="xs" color={textColor} fontFamily="mono">
                        {diagnostic.export_count}
                      </Text>
                    </HStack>
                    {diagnostic.last_error > 0 && (
                      <HStack justify="space-between" w="full">
                        <Text fontSize="xs" color={subTextColor}>Windows 错误码</Text>
                        <Text fontSize="xs" color={textColor} fontFamily="mono">
                          {diagnostic.last_error}
                        </Text>
                      </HStack>
                    )}
                  </VStack>

                  {/* 导出函数列表 */}
                  {diagnostic.exports.length > 0 && (
                    <Box mt={3} w="full">
                      <Text fontWeight="semibold" fontSize="xs" color={subTextColor} mb={1}>
                        DLL 导出函数（前 {diagnostic.exports.length} 个{diagnostic.export_count > 50 ? `，共 ${diagnostic.export_count} 个` : ""}）
                      </Text>
                      <Box
                        maxH="120px"
                        overflowY="auto"
                        p={2}
                        borderRadius="md"
                        bg={useColorModeValue("white", "#0a0a0a")}
                        borderWidth="1px"
                        borderColor={borderColor}
                      >
                        <VStack spacing={0} align="start" fontSize="2xs" fontFamily="mono">
                          {diagnostic.exports.map((exp, i) => (
                            <Text
                              key={i}
                              color={exp.includes("NvAPI_Initialize") ? "green.500" : subTextColor}
                              fontWeight={exp.includes("NvAPI_Initialize") ? "bold" : "normal"}
                            >
                              {exp}
                            </Text>
                          ))}
                        </VStack>
                      </Box>
                    </Box>
                  )}
                  {diagnostic.export_count === 0 && diagnostic.nvapi64_exists && (
                    <Text fontSize="xs" color="red.500" mt={2}>
                      ⚠ 该 DLL 没有导出任何函数，可能不是有效的动态链接库
                    </Text>
                  )}

                  {/* 候选 DLL 列表 */}
                  {diagnostic.candidate_dlls && diagnostic.candidate_dlls.length > 0 && (
                    <Box mt={3} w="full">
                      <Text fontWeight="semibold" fontSize="xs" color={subTextColor} mb={2}>
                        系统中找到的 nvapi64.dll 候选文件（{diagnostic.candidate_dlls.length} 个）
                      </Text>
                      <VStack spacing={2} align="start" w="full">
                        {diagnostic.candidate_dlls.map((dll, i) => (
                          <Box
                            key={i}
                            w="full"
                            p={2}
                            borderRadius="md"
                            bg={useColorModeValue("white", "#0a0a0a")}
                            borderWidth="1px"
                            borderColor={dll.has_initialize ? "green.300" : borderColor}
                          >
                            <HStack justify="space-between" mb={1}>
                              <Text fontSize="2xs" color={textColor} fontFamily="mono" wordBreak="break-all" maxW="70%">
                                {dll.path}
                              </Text>
                              {dll.has_initialize ? (
                                <Badge colorScheme="green" fontSize="2xs">✓ 可用</Badge>
                              ) : (
                                <Badge colorScheme="red" fontSize="2xs">✗ 不可用</Badge>
                              )}
                            </HStack>
                            <HStack spacing={3} fontSize="2xs" color={subTextColor}>
                              <Text>{(dll.size / 1024).toFixed(1)} KB</Text>
                              <Text>导出: {dll.export_count}</Text>
                              {dll.company && <Text>来源: {dll.company}</Text>}
                              {dll.version && <Text>版本: {dll.version}</Text>}
                            </HStack>
                          </Box>
                        ))}
                      </VStack>
                    </Box>
                  )}
                </Box>
              )}

              {/* 建议操作：商店版隐藏，文案（含后端 diagnostic.suggestion）引导前往 NVIDIA 官网下载驱动，触发 Store 10.1.5 */}
              {!IS_STORE_BUILD && (
                <VStack spacing={2} align="start" w="full" mt={2}>
                  <Text fontWeight="semibold" fontSize="sm" color={textColor}>
                    建议操作：
                  </Text>
                  <Text fontSize="xs" color={subTextColor}>
                    {diagnostic?.suggestion || "请从 NVIDIA 官网下载并安装最新的显卡驱动程序。"}
                  </Text>
                  <VStack spacing={1} align="start" pl={4} mt={1}>
                    <Text fontSize="xs" color={subTextColor}>
                      1. 前往 NVIDIA 官网（nvidia.com/drivers）下载对应显卡型号的最新驱动
                    </Text>
                    <Text fontSize="xs" color={subTextColor}>
                      2. 安装时选择「自定义安装」并勾选「执行清洁安装」
                    </Text>
                    <Text fontSize="xs" color={subTextColor}>
                      3. 如果是笔记本，请确认是否使用了 OEM 定制驱动（如联想/戴尔/华硕版），建议改用 NVIDIA 官方驱动
                    </Text>
                    <Text fontSize="xs" color={subTextColor}>
                      4. 安装完成后重启电脑，再返回此页面重试
                    </Text>
                  </VStack>
                </VStack>
              )}

              <HStack spacing={3} w="full" pt={2}>
                <Button
                  leftIcon={<RefreshCw size={16} />}
                  bg={getActiveColor()}
                  color={getContrastTextColor()}
                  variant="solid"
                  size="md"
                  borderRadius="lg"
                  onClick={() => loadData()}
                  _hover={{ filter: 'brightness(0.85)' }}
                >
                  重新检测
                </Button>
                <Button
                  variant="outline"
                  borderColor={borderColor}
                  size="md"
                  borderRadius="lg"
                  color={textColor}
                  onClick={() => navigate(-1)}
                >
                  返回
                </Button>
              </HStack>
            </VStack>
          </LiquidGlassCard>
        </VStack>
      </Box>
    );
  }

  // === 主页面 ===
  const hasAnyChanges = hasChanges();

  return (
    <Box pt={8}>
      <VStack spacing={6} align="start">
        {/* 标题栏 */}
        <HStack w="full" justify="space-between">
          <HStack>
            <Box as="button" onClick={() => navigate(-1)} p={1} borderRadius="md"
              _hover={{ bg: useColorModeValue("gray.100", "gray.700") }}>
              <ArrowLeft size={20} />
            </Box>
            <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
              显卡设置（仅 NVIDIA）
            </Heading>
          </HStack>
        </HStack>

        {/* 驱动信息卡片 */}
        <LiquidGlassCard
          p={4}
          borderRadius="xl"
          w="full"
          borderLeft="4px solid"
          borderLeftColor={getActiveColor()}
        >
          <HStack spacing={4}>
            <Box color={getActiveColor()}>
              <img src={nvidiaLogo} width={40} height={40} style={{ objectFit: 'contain' }} alt="NVIDIA" />
            </Box>
            <Box>
              <Text fontWeight="bold" color={textColor} fontSize="sm">
                {gpuName}
              </Text>
              <Text fontSize="xs" color={subTextColor}>
                驱动版本: {driverVersion || "未知"} {driverBranch ? `(${driverBranch})` : ""}
              </Text>
            </Box>
            <Box ml="auto">
              <Badge color={getActiveColor()} bg={hexToRgba(config.primaryColor, 0.12)} px={2} borderRadius="full">
                NVAPI 已就绪
              </Badge>
            </Box>
          </HStack>
        </LiquidGlassCard>


        {/* 同步与显示 */}
        {renderSettingGroup(
          "同步与显示",
          <Monitor size={18} color={getActiveColor()} />,
          [
            SETTING_IDS.VSYNCMODE,
            SETTING_IDS.VRR_APP_OVERRIDE,
            SETTING_IDS.VRR_MODE,
            SETTING_IDS.FRL_FPS,
            SETTING_IDS.REFRESH_RATE_OVERRIDE,
            SETTING_IDS.VSYNCTEARCONTROL,
            SETTING_IDS.OGL_TRIPLE_BUFFER,
            SETTING_IDS.VSYNC_BEHAVIOR_FLAGS,
            SETTING_IDS.VSYNCVRRCONTROL,
            SETTING_IDS.VRRFEATUREINDICATOR,
            SETTING_IDS.VRROVERLAYINDICATOR,
            SETTING_IDS.VRRREQUESTSTATE,
            SETTING_IDS.VSYNCSMOOTHAFR,
          ]
        )}

        {/* 画质与纹理 */}
        {renderSettingGroup(
          "画质与纹理",
          <Settings2 size={18} color={getActiveColor()} />,
          [
            SETTING_IDS.QUALITY_ENHANCEMENTS,
            SETTING_IDS.ANISO_MODE_LEVEL,
            SETTING_IDS.ANISO_MODE_SELECTOR,
            SETTING_IDS.AA_MODE_METHOD,
            SETTING_IDS.AA_MODE_SELECTOR,
            SETTING_IDS.AA_MODE_ALPHATOCOVERAGE,
            SETTING_IDS.AA_BEHAVIOR_FLAGS,
            SETTING_IDS.AA_MODE_REPLAY,
            SETTING_IDS.FXAA_ENABLE,
            SETTING_IDS.FXAA_INDICATOR_ENABLE,
            SETTING_IDS.AO_MODE,
            SETTING_IDS.MFAA,
            SETTING_IDS.SHADERDISKCACHE,
            SETTING_IDS.TEXFILTER_ANISO_OPTS2,
            SETTING_IDS.TEXFILTER_NO_NEG_LODBIAS,
            SETTING_IDS.AUTO_LODBIASADJUST,
            SETTING_IDS.TEXFILTER_BILINEAR_IN_ANISO,
            SETTING_IDS.TEXFILTER_DISABLE_TRILIN_SLOPE,
            SETTING_IDS.PREVENT_UI_AF_OVERRIDE,
            SETTING_IDS.FXAA_ALLOW,
            SETTING_IDS.QUALITY_ENHANCEMENT_SUBSTITUTION,
            SETTING_IDS.AO_MODE_ACTIVE,
            SETTING_IDS.SHADERDISKCACHE_MAX_SIZE,
          ]
        )}

        {/* 指示器与覆盖 */}
        {renderSettingGroup(
          "指示器与覆盖",
          <Eye size={18} color={getActiveColor()} />,
          [
            SETTING_IDS.PHYSXINDICATOR,
            SETTING_IDS.EXPORT_PERF_COUNTERS,
            SETTING_IDS.LATENCY_INDICATOR_AUTOALIGN,
          ]
        )}

        {/* 电源与性能 */}
        {renderSettingGroup(
          "电源与性能",
          <Cpu size={18} color={getActiveColor()} />,
          [
            SETTING_IDS.PREFERRED_PSTATE,
            SETTING_IDS.PRERENDERLIMIT,
            SETTING_IDS.OGL_THREAD_CONTROL,
            SETTING_IDS.VRPRERENDERLIMIT,
            SETTING_IDS.BATTERY_BOOST_APP_FPS,
            SETTING_IDS.EXTERNAL_QUIET_MODE,
          ]
        )}

        {/* 操作按钮 */}
        <HStack w="full" spacing={4} pt={2} pb={8}>
          <Button
            bg={getActiveColor()}
            color={getContrastTextColor()}
            isDisabled={!hasAnyChanges}
            isLoading={isSaving}
            loadingText="应用设置..."
            onClick={handleApply}
            flex={1}
            size="md"
            borderRadius="lg"
            _hover={{ filter: 'brightness(0.85)' }}
          >
            应用更改
          </Button>
          <Button
            variant="outline"
            borderColor={borderColor}
            isDisabled={isSaving}
            onClick={handleReset}
            flex={1}
            size="md"
            borderRadius="lg"
            color={textColor}
          >
            恢复默认
          </Button>
        </HStack>

        {/* 提示 */}
        <Box w="full" pb={4}>
          <Text fontSize="xs" color={subTextColor} textAlign="center">
            修改将在下次启动游戏或应用程序时生效。一些设置可能需要重启才能完全应用。
          </Text>
        </Box>
      </VStack>
    </Box>
  );
}
