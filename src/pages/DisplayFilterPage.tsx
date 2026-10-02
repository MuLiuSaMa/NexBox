import {
  Box,
  Flex,
  Heading,
  Text,
  VStack,
  HStack,
  useColorModeValue,
  Card,
  CardBody,
  Icon,
  IconButton,
  Tooltip,
  SimpleGrid,
  Button,
  Input,
  Image,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  Progress,
  Portal,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { getBorderGlowStyle } from "@/hooks/use-glow-effect";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { 
  Sun, BookOpen, Monitor, Sparkles, RotateCcw, 
  Film, Heart, Palette, Gamepad2, Settings2, ArrowLeft,
  Upload, Trash2, FileImage, Download, Bookmark, Trophy,
  ChevronLeft, ChevronRight, Check, ChevronDown
} from "lucide-react";
import { useState, useEffect, useCallback, useMemo, useRef, memo } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useNavigate } from "react-router-dom";
import { useAppStartup } from "@/contexts/app-startup-context";
import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { store } from "@/lib/store";
import nexboxLogo from "@/assets/nexbox-b.webp";
import championLogo from "@/assets/champion.webp";

interface FilterSettings {
  temperature: number;
  brightness: number;
  contrast: number;
  saturation: number;
  r_gamma: number;
  g_gamma: number;
  b_gamma: number;
  /** 暗部增强：-100 ~ 100，0 = 不改变 */
  shadow: number;
  s_curve: number;
  r_boost: number;
  g_boost: number;
  b_boost: number;
  mode: number;
  is_active: boolean;
  icc_active: boolean;
  active_icc_id: string | null;
  preview_filter_icc: string | null;
  preview_tint_color_icc: string | null;
  preview_tint_opacity_icc: number | null;
  stacked: boolean;
  stack_preset_ids: string[];
}

interface FilterPreset {
  id: string;
  name: string;
  mode: number;
  temperature: number;
  brightness: number;
  contrast: number;
  saturation: number;
  description: string;
}

interface IccPresetInfo {
  id: string;
  name: string;
  description: string;
}

interface UserFilterPresetInfo {
  id: string;
  name: string;
  temperature: number;
  brightness: number;
  contrast: number;
  saturation: number;
  r_gamma: number;
  g_gamma: number;
  b_gamma: number;
}

interface DisplayInfo {
  index: number;
  name: string;
  device_name: string;
  is_primary: boolean;
  width: number;
  height: number;
}

interface GameEntry {
  id: string;
  name: string;
  process_names: string[];
  is_builtin: boolean;
}

interface GameFilterStatus {
  enabled: boolean;
  games: GameEntry[];
}

// ── 页面数据模块级缓存：再次进入页面首帧即可渲染缓存，后台静默刷新 ──
let cachedDisplays: DisplayInfo[] | null = null;
let cachedPresets: FilterPreset[] | null = null;
let cachedIccPresets: IccPresetInfo[] | null = null;
let cachedUserPresets: UserFilterPresetInfo[] | null = null;
let cachedGameFilterStatus: GameFilterStatus | null = null;

// 提取显示器型号（去掉名称末尾的 "(分辨率)" 后缀，仅按钮显示用）
const stripResolution = (name?: string) =>
  name ? name.replace(/\s*\(\d+\s*[xX]\s*\d+\)\s*$/, "") : "";

// Reverse mapping: builtin ICC id → filter preset id (for startup highlight)
const ICC_TO_PRESET: Record<string, string> = {
  "builtin_NexBox_DeExposurePro": "de-exposure-pro",
  "builtin_NexBox_Vivid": "vivid",
  "builtin_NexBox_Movie": "movie",
  "builtin_NexBox_Highlight": "highlight",
  "builtin_NexBox_Soft": "soft",
  "builtin_NexBox_Game": "gaming",
  "builtin_NexBox_Reading": "reading",
  "builtin_NexBox_DeExposure": "de-exposure",
  "builtin_NexBox_ShadowBoost": "shadow-boost",
  "builtin_NexBox_LowerContrast": "dam-contrast",
  "builtin_NexBox_Aerospace": "aerospace",
  "builtin_NexBox_White": "whiter",
  "builtin_NexBox_Blue": "bluish",
  "builtin_NexBox_OriginalCool": "cool-tone",
  "builtin_NexBox_DeltaSuper": "delta-super",
  "builtin_NexBox_DeltaA": "delta-a",
  "builtin_NexBox_DeltaB": "delta-b",
  "builtin_NexBox_DeltaC": "delta-c",
  "builtin_NexBox_DeltaD": "delta-d",
  "builtin_NexBox_DeltaE": "delta-e",
};

// 冠军调试系列：builtin_ChampionN ↔ champion-N
// 与后端 preset_id_to_builtin_icc 的 champion-N → ChampionN.icc 映射一一对应。
// 缺失会导致重启后冠军预设的选中高亮无法恢复。
const CHAMPION_COUNT = 15;
for (let i = 1; i <= CHAMPION_COUNT; i++) {
  ICC_TO_PRESET[`builtin_Champion${i}`] = `champion-${i}`;
}

/** 冠军调试系列预设（id 前缀 champion-），与「新境盒」系列区分开渲染。 */
const isChampionPreset = (preset: FilterPreset) =>
  preset.id.startsWith("champion-");

/** 冠军预设没有在 presetIcons/presetColors 里登记，按编号轮转取色。 */
const CHAMPION_COLORS = [
  "#F0C24B", "#FF8A5B", "#4CC9F0", "#7EE8A2", "#8E7CFF",
  "#FF6B9D", "#06D6A0", "#FFD166", "#5AA9FF", "#E8B4B8",
];
const championColor = (id: string) => {
  const n = parseInt(id.replace(/\D/g, ""), 10) || 1;
  return CHAMPION_COLORS[(n - 1) % CHAMPION_COLORS.length];
};

/** 单个预设卡片（memo 化：点击卡片时只有新旧两张卡重渲染，避免整页卡顿）。
 *  样式相关的主题值在本组件内部用 hook 取，父组件只传数据与稳定回调。 */
const PresetCard = memo(function PresetCard({
  preset,
  isActive,
  multiFilterEnabled,
  accentColor,
  onSelect,
  onExport,
}: {
  preset: FilterPreset;
  isActive: boolean;
  multiFilterEnabled: boolean;
  accentColor: string;
  onSelect: (preset: FilterPreset) => void;
  onExport: (id: string, e: React.MouseEvent) => void;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const miniGlassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const miniGlassBorder = useColorModeValue("rgba(255,255,255,0.5)", "rgba(255,255,255,0.2)");
  const miniGlassGlow = useColorModeValue("rgba(255,255,255,0.8)", "rgba(255,255,255,0.45)");
  const sliderBg = useColorModeValue("gray.100", "#222222");
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;

  const Icon =
    presetIcons[preset.id] || (isChampionPreset(preset) ? Trophy : Monitor);

  return (
    <Tooltip label={preset.description} placement="top">
      <Box
        bg={liquidGlassEnabled
          ? (isActive ? hexToRgba(accentColor, 0.2) : miniGlassBg)
          : (isActive ? `${accentColor}20` : sliderBg)}
        borderRadius="xl"
        p={4}
        cursor="pointer"
        onClick={() => onSelect(preset)}
        border={liquidGlassEnabled ? "1px solid" : "2px solid"}
        borderColor={liquidGlassEnabled
          ? (isActive ? accentColor : miniGlassBorder)
          : (isActive ? accentColor : "transparent")}
        backdropFilter={`blur(${effectiveBlur}px)`}
        sx={{
          transform: "translateZ(0)",
          WebkitTransform: "translateZ(0)",
          WebkitBackfaceVisibility: "hidden",
          backfaceVisibility: "hidden",
          willChange: "backdrop-filter, transform",
        }}
        transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
        _hover={{ borderColor: accentColor }}
        position="relative"
        overflow="hidden"
      >
        {liquidGlassEnabled && (
          <Box
            style={getBorderGlowStyle(
              isActive ? hexToRgba(accentColor, 0.5) : miniGlassGlow
            )}
          />
        )}

        {multiFilterEnabled ? (
          isActive && (
            <HStack
              position="absolute"
              top={1.5}
              right={1.5}
              spacing={1}
              bg={hexToRgba(accentColor, 0.9)}
              color="#ffffff"
              px={1.5}
              py={0.5}
              borderRadius="full"
              fontSize="9px"
              fontWeight="700"
              pointerEvents="none"
              zIndex={2}
            >
              <Box as="span">✓</Box>
              <Text as="span">{t("displayFilter.multiFilterSelected")}</Text>
            </HStack>
          )
        ) : (
          <Tooltip label={t("displayFilter.exportIcc")} placement="top">
            <IconButton
              aria-label={t("displayFilter.exportIcc")}
              icon={<Download size={14} />}
              size="xs"
              variant="ghost"
              position="absolute"
              top={1}
              right={1}
              color={subTextColor}
              opacity={0.5}
              _hover={{ opacity: 1, color: accentColor }}
              onClick={(e) => onExport(preset.id, e)}
            />
          </Tooltip>
        )}
        <VStack spacing={2}>
          <Icon size={24} color={accentColor} />
          <Text color={textColor} fontSize="sm" fontWeight="600">
            {preset.name}
          </Text>
        </VStack>
      </Box>
    </Tooltip>
  );
});

const presetIcons: Record<string, React.ElementType> = {
  "de-exposure-pro": Monitor,
  "vivid": Sparkles,
  "movie": Film,
  "highlight": Sun,
  "soft": Heart,
  "gaming": Gamepad2,
  "reading": BookOpen,
  "benq": Monitor,
  "dam-contrast": RotateCcw,
  "aerospace": Sparkles,
  "whiter": Sun,
  "bluish": Palette,
  "cool-tone": Film,
  "delta-super": Gamepad2,
  "delta-a": Sparkles,
  "delta-b": Sun,
  "delta-c": Film,
  "delta-d": Palette,
  "delta-e": Heart,
  "custom": Settings2,
};

const presetColors: Record<string, string> = {
  "de-exposure-pro": "#4ECDC4",
  "vivid": "#FF6B9D",
  "movie": "#9B59B6",
  "highlight": "#F1C40F",
  "soft": "#E8B4B8",
  "gaming": "#00D9FF",
  "reading": "#DEB887",
  "benq": "#00A862",
  "dam-contrast": "#7A8B99",
  "aerospace": "#6C8EFF",
  "whiter": "#E8E8EC",
  "bluish": "#5AA9FF",
  "cool-tone": "#7EC8E3",
  "delta-super": "#F0C24B",
  "delta-a": "#7EE8A2",
  "delta-b": "#FFD166",
  "delta-c": "#06D6A0",
  "delta-d": "#8E7CFF",
  "delta-e": "#4CC9F0",
  "custom": "#6B7280",
};

const modeParams: Record<number, { gamma: number; sCurve: number; rBoost: number; gBoost: number; bBoost: number }> = {
  0: { gamma: 1.0, sCurve: 0.0, rBoost: 1.0, gBoost: 1.0, bBoost: 1.0 },
  1: { gamma: 0.95, sCurve: 0.08, rBoost: 1.02, gBoost: 1.0, bBoost: 1.03 },
  2: { gamma: 1.05, sCurve: -0.05, rBoost: 1.0, gBoost: 0.98, bBoost: 0.96 },
  3: { gamma: 0.92, sCurve: 0.05, rBoost: 1.0, gBoost: 1.0, bBoost: 1.0 },
  4: { gamma: 1.08, sCurve: -0.08, rBoost: 0.98, gBoost: 1.0, bBoost: 1.02 },
  5: { gamma: 0.96, sCurve: 0.1, rBoost: 1.0, gBoost: 1.0, bBoost: 1.02 },
  6: { gamma: 1.0, sCurve: 0.0, rBoost: 1.0, gBoost: 0.99, bBoost: 0.97 },
  7: { gamma: 0.96, sCurve: -0.05, rBoost: 1.0, gBoost: 1.0, bBoost: 1.0 },
  8: { gamma: 1.12, sCurve: 0.03, rBoost: 1.0, gBoost: 1.0, bBoost: 1.0 },
  9: { gamma: 1.12, sCurve: 0.08, rBoost: 1.0, gBoost: 1.0, bBoost: 1.02 },
};

export default function DisplayFilterPage() {
  const navigate = useNavigate();
  const { filterHotkey, saveFilterHotkey } = useAppStartup();
  const adaptiveTitle = useAdaptiveTextColor();
  const [settings, setSettings] = useState<FilterSettings>({
    temperature: 6500,
    brightness: 100,
    contrast: 100,
    saturation: 100,
    r_gamma: 1.0,
    g_gamma: 1.0,
    b_gamma: 1.0,
    shadow: 0,
    s_curve: 0.0,
    r_boost: 1.0,
    g_boost: 1.0,
    b_boost: 1.0,
    mode: 0,
    is_active: false,
    icc_active: false,
    active_icc_id: null,
    preview_filter_icc: null,
    preview_tint_color_icc: null,
    preview_tint_opacity_icc: null,
    stacked: false,
    stack_preset_ids: [],
  });
  const [presets, setPresets] = useState<FilterPreset[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [autoApplyOnStartup, setAutoApplyOnStartup] = useState(false);
  // 滤镜工具包(icc-tools)状态:商店版不打包,缺失时引导下载
  const [iccToolsInstalled, setIccToolsInstalled] = useState(true);
  const [iccToolsBusy, setIccToolsBusy] = useState(false);
  const [iccToolsProgress, setIccToolsProgress] = useState<{ phase: string; progress: number; detail: string } | null>(null);

  useEffect(() => {
    (async () => {
      let v = await store.get<boolean>("nexbox_auto_apply");
      if (v !== null && v !== undefined) {
        setAutoApplyOnStartup(v);
      } else {
        setAutoApplyOnStartup(localStorage.getItem("nexbox_auto_apply") === "true");
      }
    })();
  }, []);

  // 检测滤镜工具包(icc-tools)是否存在,并监听下载进度
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const status: { installed: boolean; path: string | null } = await invoke("check_icc_tools");
        setIccToolsInstalled(status.installed);
      } catch {
        setIccToolsInstalled(false);
      }
      const handler = await listen<{ phase: string; progress: number; detail: string }>(
        "icc-tools-download-progress",
        (event) => setIccToolsProgress(event.payload),
      ).catch(() => undefined);
      unlisten = handler;
    })();
    return () => unlisten?.();
  }, []);

  // 恢复「使用多个滤镜」开关状态
  useEffect(() => {
    (async () => {
      let v = await store.get<boolean>("nexbox_multi_filter");
      if (v !== null && v !== undefined) {
        setMultiFilterEnabled(v);
      } else {
        setMultiFilterEnabled(localStorage.getItem("nexbox_multi_filter") === "true");
      }
    })();
  }, []);
  const [activePresetId, setActivePresetId] = useState<string>("");
  const [savedCustom, setSavedCustom] = useState<{
    temperature: number;
    brightness: number;
    contrast: number;
    saturation: number;
    r_gamma: number;
    g_gamma: number;
    b_gamma: number;
    shadow: number;
  } | null>(null);
  const [hasChanges, setHasChanges] = useState(false);
  const [inputVersion, setInputVersion] = useState(0);
  /** 拖动滑块时递增，用于触发实时预览重算（editValuesRef 是 ref，不会自己触发渲染） */
  const [liveVersion, setLiveVersion] = useState(0);
  const [manualPresetChange, setManualPresetChange] = useState(false);
  const [iccPresets, setIccPresets] = useState<IccPresetInfo[]>([]);
  const [activeIccId, setActiveIccId] = useState<string | null>(null);
  /** 滤镜预设分组 Tab：新境盒 / 冠军调试 / 自定义（数值调节） */
  const [presetTab, setPresetTab] = useState<"mine" | "nexbox" | "champion" | "custom">("nexbox");
  const [deleteIccId, setDeleteIccId] = useState<string | null>(null);
  const [deleteUserPresetId, setDeleteUserPresetId] = useState<string | null>(null);
  const [showSavePresetDialog, setShowSavePresetDialog] = useState(false);
  const [presetNameInput, setPresetNameInput] = useState("");
  const [userPresets, setUserPresets] = useState<UserFilterPresetInfo[]>([]);
  // ICC 滤镜预览参数
  const [iccPreviewFilter, setIccPreviewFilter] = useState<string | null>(null);
  const [iccTintColor, setIccTintColor] = useState<string | null>(null);
  const [iccTintOpacity, setIccTintOpacity] = useState<number>(0);
  const cancelDeleteRef = useRef<HTMLButtonElement>(null);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [activeDisplayIndex, setActiveDisplayIndex] = useState<number>(0);
  // 是否选中"主显示器"入口（与列表中的主屏型号项互斥，对应准心菜单的 -1 语义）
  const [usePrimaryDisplay, setUsePrimaryDisplay] = useState(true);
  const activeDisplayIndexRef = useRef(0);
  // 游戏启动时自动应用滤镜
  const [gameFilterEnabled, setGameFilterEnabled] = useState(false);
  const [gameFilterGames, setGameFilterGames] = useState<GameEntry[]>([]);
  const [isGameFilterBusy, setIsGameFilterBusy] = useState(false);
  const [isGameFilterListOpen, setIsGameFilterListOpen] = useState(false);
  
  // 多滤镜叠加模式：开关（持久化）+ 选中列表（点选标记，需手动应用）
  const [multiFilterEnabled, setMultiFilterEnabled] = useState(false);
  const [selectedStackIds, setSelectedStackIds] = useState<string[]>([]);
  
  // 滤镜预览相关
  const [splitPosition, setSplitPosition] = useState(50);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);
  
  // 预览图片切换
  const previewImages = ["/icc-preview.jpg", "/lhdbsn.webp", "/BKSSW.jpg", "/htjdsn.webp"];
  const [previewImageIndex, setPreviewImageIndex] = useState(0);
  
  const editValuesRef = useRef({
    temperature: 6500,
    brightness: 100,
    contrast: 100,
    saturation: 100,
    r_gamma: 1.0,
    g_gamma: 1.0,
    b_gamma: 1.0,
    shadow: 0,
  });
  
  const { t } = useTranslation();
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const toast = useDynamicIsland("filter");

  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const sliderBg = useColorModeValue("gray.100", "#222222");
  const infoBg = useColorModeValue("gray.50", "#1a1a1a");
  const inputBg = useColorModeValue("white", "#1a1a1a");
  const miniGlassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const miniGlassBorder = useColorModeValue("rgba(255,255,255,0.5)", "rgba(255,255,255,0.2)");
  const miniGlassGlow = useColorModeValue("rgba(255,255,255,0.8)", "rgba(255,255,255,0.45)");
  /** 预设分组 Tab 的选中底色 */
  const tabActiveBg = useColorModeValue("white", "#3a3a3a");
  const hoverBg = useColorModeValue("gray.100", "#252525");
  const menuListBg = useColorModeValue("white", "#1a1a1a");

  // 模糊立即生效
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;

  const loadSettings = useCallback(async () => {
    try {
      const result: FilterSettings = await invoke("get_filter_settings", { displayIndex: activeDisplayIndexRef.current });
      setSettings(result);
      // 叠加模式：回填已应用的组合徽标（不覆盖用户进行中的点选）
      if (result.stacked && result.stack_preset_ids) {
        setSelectedStackIds(result.stack_preset_ids);
      }
      // 恢复 ICC 激活状态 — 映射 active_icc_id 回预设卡片
      if (result.icc_active && result.active_icc_id) {
        setActiveIccId(result.active_icc_id);
        const mappedId = ICC_TO_PRESET[result.active_icc_id];
        setActivePresetId(mappedId || "");
        setIccPreviewFilter(result.preview_filter_icc || null);
        setIccTintColor(result.preview_tint_color_icc || null);
        setIccTintOpacity(result.preview_tint_opacity_icc ?? 0);
        setManualPresetChange(true);
        setTimeout(() => setManualPresetChange(false), 100);
      } else {
        // 切换显示器或该显示器滤镜未启用时，清除上一个显示器的选中高亮，
        // 确保开关与预设/ICC 选择都反映当前显示器的真实状态。
        setActiveIccId(null);
        setActivePresetId("");
        setIccPreviewFilter(null);
        setIccTintColor(null);
        setIccTintOpacity(0);
      }
    } catch (error) {
      console.error("Failed to load filter settings:", error);
    }
  }, []);

  const loadPresets = useCallback(async () => {
    if (cachedPresets) setPresets(cachedPresets);
    try {
      const result: FilterPreset[] = await invoke("get_filter_presets");
      cachedPresets = result;
      setPresets(result);
    } catch (error) {
      console.error("Failed to load presets:", error);
    }
  }, []);

  const loadCustomSettings = useCallback(async () => {
    try {
      const result = await invoke<{ temperature: number; brightness: number; contrast: number; saturation: number; r_gamma: number; g_gamma: number; b_gamma: number; shadow: number }>("get_custom_filter_settings", { displayIndex: activeDisplayIndexRef.current });
      editValuesRef.current = {
        temperature: result.temperature,
        brightness: result.brightness,
        contrast: result.contrast,
        saturation: result.saturation,
        r_gamma: result.r_gamma ?? 1.0,
        g_gamma: result.g_gamma ?? 1.0,
        b_gamma: result.b_gamma ?? 1.0,
        shadow: result.shadow ?? 0,
      };
      setSavedCustom(result);
    } catch (error) {
      console.error("Failed to load custom settings:", error);
      const defaults = {
        temperature: 6500,
        brightness: 100,
        contrast: 100,
        saturation: 100,
        r_gamma: 1.0,
        g_gamma: 1.0,
        b_gamma: 1.0,
        shadow: 0,
      };
      editValuesRef.current = defaults;
      setSavedCustom(defaults);
    }
  }, []);

  const loadIccPresets = useCallback(async () => {
    if (cachedIccPresets) setIccPresets(cachedIccPresets);
    try {
      const result: IccPresetInfo[] = await invoke("get_icc_presets");
      cachedIccPresets = result;
      setIccPresets(result);
    } catch (error) {
      console.error("Failed to load ICC presets:", error);
    }
  }, []);

  const loadUserFilterPresets = useCallback(async () => {
    if (cachedUserPresets) setUserPresets(cachedUserPresets);
    try {
      const result: UserFilterPresetInfo[] = await invoke("get_user_filter_presets");
      cachedUserPresets = result;
      setUserPresets(result);
    } catch (error) {
      console.error("Failed to load user filter presets:", error);
    }
  }, []);

  const loadDisplays = useCallback(async () => {
    // 先用缓存立即上屏（若有），再后台刷新
    if (cachedDisplays && cachedDisplays.length > 0) {
      setDisplays(cachedDisplays);
      const cachedIdx = cachedDisplays.find((d) => d.is_primary)?.index ?? cachedDisplays[0].index;
      setActiveDisplayIndex(cachedIdx);
      activeDisplayIndexRef.current = cachedIdx;
    }
    try {
      const result: DisplayInfo[] = await invoke("get_displays");
      if (result.length > 0) {
        cachedDisplays = result;
        setDisplays(result);
        const idx = result.find((d) => d.is_primary)?.index ?? result[0].index;
        setActiveDisplayIndex(idx);
        activeDisplayIndexRef.current = idx;
        await invoke("set_active_display", { displayIndex: idx });
      } else {
        cachedDisplays = [{ index: 0, name: "DISPLAY1", device_name: "DISPLAY1", is_primary: true, width: 0, height: 0 }];
        setDisplays(cachedDisplays);
      }
    } catch (error) {
      console.error("Failed to load displays:", error);
      cachedDisplays = [{ index: 0, name: "DISPLAY1", device_name: "DISPLAY1", is_primary: true, width: 0, height: 0 }];
      setDisplays(cachedDisplays);
    }
  }, []);

  const loadGameFilterStatus = useCallback(async () => {
    if (cachedGameFilterStatus) {
      setGameFilterEnabled(cachedGameFilterStatus.enabled);
      setGameFilterGames(cachedGameFilterStatus.games);
    }
    try {
      const result: GameFilterStatus = await invoke("get_game_filter_status");
      cachedGameFilterStatus = result;
      setGameFilterEnabled(result.enabled);
      setGameFilterGames(result.games);
    } catch (error) {
      console.error("Failed to load game filter status:", error);
    }
  }, []);

  const handleToggleGameFilter = useCallback(async (enabled: boolean) => {
    setIsGameFilterBusy(true);
    try {
      await invoke("set_game_filter_enabled", { enabled });
      setGameFilterEnabled(enabled);
    } catch (error) {
      console.error("Failed to set game filter enabled:", error);
      toast({
        title: t("displayFilter.gameFilterError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsGameFilterBusy(false);
    }
  }, [toast, t]);

  // 切换「使用多个滤镜」开关（持久化到本地 store）
  const handleToggleMultiFilter = useCallback((enabled: boolean) => {
    setMultiFilterEnabled(enabled);
    localStorage.setItem("nexbox_multi_filter", enabled ? "true" : "false");
    store.set("nexbox_multi_filter", enabled).then(() => store.save());
    if (enabled) {
      // 多选模式：把当前已生效的滤镜自动带入选中列表（否则已开启的滤镜无徽标且会丢失）
      let initial: string[] = [];
      if (settings.stacked && settings.stack_preset_ids.length > 0) {
        // 本就是叠加 → 沿用已应用的组合
        initial = settings.stack_preset_ids;
      } else if (settings.is_active) {
        if (activeIccId && activeIccId !== "") {
          // 内置预设 ICC（builtin_NexBox_*）映射回预设卡 id；其它 ICC 直接用原 id
          initial = [ICC_TO_PRESET[activeIccId] || activeIccId];
        } else if (activePresetId && activePresetId !== "" && activePresetId !== "custom") {
          initial = [activePresetId];
        } else {
          // 兜底：按当前参数匹配预设卡（自定义/用户预设应用后 mode 归 0 的场景）
          const matched = presets.find(
            (p) =>
              p.mode === settings.mode &&
              p.temperature === settings.temperature &&
              p.brightness === settings.brightness &&
              p.contrast === settings.contrast &&
              p.saturation === settings.saturation,
          );
          if (matched) initial = [matched.id];
        }
      }
      setSelectedStackIds(initial);
      // 中止单选高亮，避免与徽标混淆
      setActivePresetId("");
      setActiveIccId(null);
    } else {
      // 关闭多选：清空进行中的点选，恢复单选行为
      setSelectedStackIds([]);
      if (!settings.stacked) {
        setActivePresetId("");
      }
    }
  }, [settings, activeIccId, activePresetId, presets]);

  // 将当前选中组合同步到后端（主开关开启时即时应用；空组合 = 清除叠加）
  const syncStackToBackend = useCallback(async (ids: string[]) => {
    try {
      const result: any = await invoke("apply_filter_stack", {
        displayIndex: activeDisplayIndexRef.current,
        presetIds: ids,
      });
      if (result.success) {
        setSettings(result.settings as FilterSettings);
        setActiveIccId(null);
        setActivePresetId("");
      }
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  }, [toast, t]);

  // 多选点选/取消：仅做标记；主开关已开启时改选组合即时重新生效
  const toggleStackSelection = useCallback((id: string) => {
    const next = selectedStackIds.includes(id)
      ? selectedStackIds.filter((x) => x !== id)
      : [...selectedStackIds, id];
    setSelectedStackIds(next);
    if (settings.is_active) {
      void syncStackToBackend(next);
    }
  }, [selectedStackIds, settings.is_active, syncStackToBackend]);

  useEffect(() => {
    activeDisplayIndexRef.current = activeDisplayIndex;
  }, [activeDisplayIndex]);

  useEffect(() => {
    // 有缓存时先同步应用，首帧即可显示已加载的数据；随后后台刷新
    if (cachedDisplays && cachedDisplays.length > 0) {
      const cachedIdx = cachedDisplays.find((d) => d.is_primary)?.index ?? cachedDisplays[0].index;
      activeDisplayIndexRef.current = cachedIdx;
    }
    loadSettings();
    loadPresets();
    loadCustomSettings();
    loadIccPresets();
    loadUserFilterPresets();
    loadGameFilterStatus();
    const timer = setTimeout(() => loadDisplays(), 0);
    return () => clearTimeout(timer);
  }, [loadDisplays, loadSettings, loadPresets, loadCustomSettings, loadIccPresets, loadUserFilterPresets, loadGameFilterStatus]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<void>("filter-status-changed", () => {
      loadSettings();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [loadSettings]);

  useEffect(() => {
    if (presets.length === 0 || savedCustom === null) return;
    if (manualPresetChange) return;
    if (settings.icc_active) return; // ICC 激活时跳过预设匹配
    // 冠军调试预设同样是 ICC 驱动、参数为中性值（6500/100/100/100），
    // 参数化匹配会把它误判成「自定义」或其它中性预设 → 直接跳过，保持点击后的高亮。
    if (activePresetId.startsWith("champion-")) return;
    // 「我的预设」应用后 mode 归 0、参数来自用户保存值，不参与内置预设匹配，
    // 保持点击后 / 重进页面恢复的高亮不被内置预设抢走。
    if (userPresets.some((p) => p.id === activePresetId)) return;
    if (activeIccId && (ICC_TO_PRESET[activeIccId] || "").startsWith("champion-")) return;
    if (activePresetId === "custom") return;

    const matchesSavedCustom =
      settings.mode === 0 &&
      settings.temperature === savedCustom.temperature &&
      settings.brightness === savedCustom.brightness &&
      settings.contrast === savedCustom.contrast &&
      settings.saturation === savedCustom.saturation;

    // 空选中态（重进页面 / 切显示器后 loadSettings 清空高亮）：
    // 「我的预设」是纯参数化应用（不写 ICC 状态），原先这里直接 return → 重进后高亮丢失。
    // 现在按参数精确匹配「我的预设」恢复高亮；保存的自定义参数维持原有空选中语义。
    if (activePresetId === "") {
      if (activeIccId) return; // ICC 预设选中时跳过同步
      if (matchesSavedCustom) return;
      const userMatch = userPresets.find(
        (p) =>
          settings.mode === 0 &&
          settings.shadow === 0 &&
          settings.temperature === p.temperature &&
          settings.brightness === p.brightness &&
          settings.contrast === p.contrast &&
          settings.saturation === p.saturation &&
          settings.r_gamma === p.r_gamma &&
          settings.g_gamma === p.g_gamma &&
          settings.b_gamma === p.b_gamma
      );
      if (userMatch) {
        setActivePresetId((prev) => (prev === userMatch.id ? prev : userMatch.id));
      }
      return;
    }

    const exactPreset = presets.find(
      (p) =>
        p.mode === settings.mode &&
        p.temperature === settings.temperature &&
        p.brightness === settings.brightness &&
        p.contrast === settings.contrast &&
        p.saturation === settings.saturation
    );

    let nextId: string;
    if (matchesSavedCustom) {
      nextId = "custom";
    } else if (exactPreset) {
      nextId = exactPreset.id;
    } else {
      const modePreset = presets.find((p) => p.mode === settings.mode);
      nextId = modePreset?.id ?? "";
    }

    setActivePresetId((prev) => (prev === nextId ? prev : nextId));
  }, [presets, settings, savedCustom, activePresetId, activeIccId, manualPresetChange, userPresets]);

  const downloadIccTools = async () => {
    if (iccToolsBusy) return;
    setIccToolsBusy(true);
    setIccToolsProgress(null);
    try {
      await invoke("download_icc_tools");
      toast({
        title: t("displayFilter.iccToolsSuccess"),
        status: "success",
        duration: 3000,
      });
      const status: { installed: boolean; path: string | null } = await invoke("check_icc_tools");
      setIccToolsInstalled(status.installed);
    } catch (error) {
      toast({
        title: t("displayFilter.iccToolsFailed"),
        description: String(error),
        status: "error",
        duration: 7000,
      });
    } finally {
      setIccToolsBusy(false);
      setIccToolsProgress(null);
    }
  };

  const toggleFilter = async () => {
    setIsLoading(true);
    const willEnable = !settings.is_active;
    console.log("%c[FilterToggle] 开始操作", "color: #4CAF50; font-weight: bold", {
      willEnable,
      displayIndex: activeDisplayIndex,
      currentSettings: { ...settings },
    });

    try {
      // 启用前先检测 Gamma Ramp 支持情况
      if (willEnable) {
        console.log("%c[FilterToggle] 检测 Gamma Ramp 支持状态...", "color: #2196F3");
        try {
          const supportInfo: any = await invoke("check_gamma_support", {
            displayIndex: activeDisplayIndex,
          });
          console.log("%c[FilterToggle] Gamma Ramp 检测结果:", "color: #2196F3", {
            supported: supportInfo.supported,
            capsValue: supportInfo.caps_value,
            hdrEnabled: supportInfo.hdr_enabled,
            reason: supportInfo.reason,
          });

          if (!supportInfo.supported) {
            console.warn("%c[FilterToggle] ⚠ Gamma Ramp 可能不支持，但仍会尝试启用", "color: #FF9800; font-weight: bold", supportInfo.reason);
          }
        } catch (supportErr) {
          console.warn("%c[FilterToggle] check_gamma_support 调用失败，跳过检测", "color: #FF9800", String(supportErr));
        }
      }

      // 多选叠加模式开启：开启主开关 = 直接应用当前选中的组合
      const useStack = willEnable && multiFilterEnabled && selectedStackIds.length > 0;
      console.log("%c[FilterToggle] 调用", "color: #4CAF50", useStack ? "apply_filter_stack" : "toggle_filter", {
        displayIndex: activeDisplayIndex,
        presetIds: selectedStackIds,
      });
      const result: any = useStack
        ? await invoke("apply_filter_stack", {
            displayIndex: activeDisplayIndex,
            presetIds: selectedStackIds,
          })
        : await invoke("toggle_filter", { displayIndex: activeDisplayIndex });

      console.log("%c[FilterToggle] toggle_filter 返回:", "color: #4CAF50", {
        success: result.success,
        message: result.message,
        isActive: result.settings?.is_active,
        settings: result.settings,
      });

      if (result.success) {
        setSettings(result.settings as FilterSettings);
        const isActive = result.settings.is_active;
        // 先判断“跳过”：请求已处理，但操作被更新的操作取代、未执行。
        // 此时不得展示开启/关闭/清除成功的 toast（success=true 只代表请求正常处理）。
        if (result.skipped_stale) {
          toast({
            title: result.message || t("displayFilter.operationSuperseded"),
            status: "info",
            duration: 3000,
            isClosable: true,
          });
        } else if (!isActive && result.degraded) {
          // 降级清除：滤镜已移除，但未能恢复原有校色，必须如实提示而非“已禁用”。
          toast({
            title: t("displayFilter.filterDisabledDegraded"),
            status: "warning",
            duration: 5000,
            isClosable: true,
          });
        } else {
          toast({
            title: isActive
              ? t("displayFilter.filterEnabled")
              : t("displayFilter.filterDisabled"),
            status: "success",
            duration: 2000,
            isClosable: true,
          });
        }
      }
    } catch (error) {
      console.error("%c[FilterToggle] 操作失败!", "color: #f44336; font-weight: bold", {
        error: String(error),
        errorObj: error,
        willEnable,
        displayIndex: activeDisplayIndex,
      });
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const applyPreset = async (preset: FilterPreset) => {
    setIsLoading(true);
    // 批量更新状态，减少重渲染
    setManualPresetChange(true);
    setActivePresetId(preset.id);
    // 注意：不要在这里清空 activeIccId / iccPreview* 。
    // 否则在 invoke 返回前，滤镜侧会短暂回退到参数化 filterStyle，造成预览“闪一下”。
    // 预览状态只在拿到后端返回后一次性更新。
    setHasChanges(false);
    setInputVersion(v => v + 1);
    
    try {
      const result: any = await invoke("apply_preset", {
        displayIndex: activeDisplayIndex,
        presetId: preset.id,
        isActive: settings.is_active,
      });
      if (result.success) {
        // Use the backend-computed values (icc_active, preview_*) — do NOT hardcode.
        const s = result.settings;
        setSettings({
          temperature: s?.temperature ?? preset.temperature,
          brightness: s?.brightness ?? preset.brightness,
          contrast: s?.contrast ?? preset.contrast,
          saturation: s?.saturation ?? preset.saturation,
          r_gamma: s?.r_gamma ?? 1.0,
          g_gamma: s?.g_gamma ?? 1.0,
          b_gamma: s?.b_gamma ?? 1.0,
          shadow: s?.shadow ?? 0,
          s_curve: s?.s_curve ?? 0.0,
          r_boost: s?.r_boost ?? 1.0,
          g_boost: s?.g_boost ?? 1.0,
          b_boost: s?.b_boost ?? 1.0,
          mode: s?.mode ?? preset.mode,
          is_active: s?.is_active ?? settings.is_active,
          icc_active: s?.icc_active ?? false,
          active_icc_id: s?.active_icc_id ?? null,
          preview_filter_icc: s?.preview_filter_icc ?? null,
          preview_tint_color_icc: s?.preview_tint_color_icc ?? null,
          preview_tint_opacity_icc: s?.preview_tint_opacity_icc ?? null,
          stacked: false,
          stack_preset_ids: [],
        });
        // ICC 内置预设：使用后端返回的 ICC 预览效果（分段预览的滤镜侧）
        const hasIccPreview = !!result.preview_filter || !!result.preview_tint_color;
        if (hasIccPreview) {
          setActiveIccId(s?.active_icc_id ?? (s?.icc_active ? preset.id : null));
          setIccPreviewFilter(result.preview_filter || null);
          setIccTintColor(result.preview_tint_color || null);
          setIccTintOpacity(result.preview_tint_opacity ?? 0);
        } else {
          setActiveIccId(null);
        }
        toast({
          title: `${t("displayFilter.presetAppliedPrefix")}${preset.name}${t("displayFilter.presetAppliedSuffix")}`,
          description: !settings.is_active ? t("displayFilter.paramsUpdatedHint") : undefined,
          status: "success",
          duration: 2500,
          isClosable: true,
        });
      }
    } catch (error) {
      // 失败时清理预览，避免残留上次滤镜的预览状态
      setActiveIccId(null);
      setIccPreviewFilter(null);
      setIccTintColor(null);
      setIccTintOpacity(0);
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
      // 延迟重置标志，让 useEffect 跳过一次自动同步
      setTimeout(() => setManualPresetChange(false), 100);
    }
  };

  const openCustom = async () => {
    setManualPresetChange(true);
    setActivePresetId("custom");
    setActiveIccId(null);
    setIccPreviewFilter(null);
    setIccTintColor(null);
    setIccTintOpacity(0);
    setIsLoading(true);
    // 不要用当前已应用的 settings 覆盖：切换其它预设后 settings 会变成该预设参数，
    // 会冲掉未保存的编辑或磁盘上的自定义档案。editValuesRef 由 loadCustomSettings / 保存 / 用户输入维护。
    setInputVersion((v) => v + 1);
    if (savedCustom) {
      const r = editValuesRef.current;
      setHasChanges(
        r.temperature !== savedCustom.temperature ||
          r.brightness !== savedCustom.brightness ||
          r.contrast !== savedCustom.contrast ||
          r.saturation !== savedCustom.saturation ||
          r.r_gamma !== savedCustom.r_gamma ||
          r.g_gamma !== savedCustom.g_gamma ||
          r.b_gamma !== savedCustom.b_gamma ||
          (r.shadow ?? 0) !== (savedCustom.shadow ?? 0)
      );
      
      // 应用已保存的自定义滤镜设置
      try {
        const result: any = await invoke("set_filter_settings", {
          displayIndex: activeDisplayIndex,
          temperature: savedCustom.temperature,
          brightness: savedCustom.brightness,
          contrast: savedCustom.contrast,
          saturation: savedCustom.saturation,
          mode: 0,
          isActive: settings.is_active,
          rGamma: savedCustom.r_gamma ?? 1.0,
          gGamma: savedCustom.g_gamma ?? 1.0,
          bGamma: savedCustom.b_gamma ?? 1.0,
        });
        if (result.success) {
          setSettings(prev => ({
            temperature: savedCustom.temperature,
            brightness: savedCustom.brightness,
            contrast: savedCustom.contrast,
            saturation: savedCustom.saturation,
            r_gamma: savedCustom.r_gamma ?? 1.0,
            g_gamma: savedCustom.g_gamma ?? 1.0,
            b_gamma: savedCustom.b_gamma ?? 1.0,
            shadow: savedCustom.shadow ?? 0,
            s_curve: 0.0,
            r_boost: 1.0,
            g_boost: 1.0,
            b_boost: 1.0,
            mode: 0,
            is_active: prev.is_active,
            icc_active: false,
            active_icc_id: null,
            preview_filter_icc: null,
            preview_tint_color_icc: null,
            preview_tint_opacity_icc: null,
            stacked: false,
            stack_preset_ids: [],
          }));
        }
      } catch (error) {
        console.error("Failed to apply custom settings:", error);
      }
    } else {
      setHasChanges(false);
    }
    setIsLoading(false);
    setTimeout(() => setManualPresetChange(false), 100);
  };

  const handleExportCustom = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const result: string | null = await invoke("export_custom_filter", { displayIndex: activeDisplayIndex });
      if (result) {
        toast({
          title: t("displayFilter.exportIccSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("displayFilter.exportIccFailed"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const resetCustomValues = () => {
    editValuesRef.current = {
      temperature: 6500,
      brightness: 100,
      contrast: 100,
      saturation: 100,
      r_gamma: 1.0,
      g_gamma: 1.0,
      b_gamma: 1.0,
      shadow: 0,
    };
    setInputVersion((v) => v + 1);
    setLiveVersion((v) => v + 1);
    setHasChanges(true);
    // 屏幕一并恢复默认效果（不只是数值）：开关开启时立即用默认参数落屏
    applyCustomLive();
  };

  /** 自定义调节实时落屏（不落盘）：开关开启时，拖动滑块过程中直接写屏（同「拇指调试」）。
   *  latest-wins 串行化：同一时刻至多一个 invoke 在飞，期间产生的最新值在完成后补发；
   *  值未变化时跳过；松手时兜底调用一次，保证最终值一定落屏。 */
  const isActiveRef = useRef(settings.is_active);
  isActiveRef.current = settings.is_active;
  const liveApplyBusyRef = useRef(false);
  const liveApplyPendingRef = useRef(false);
  const liveApplyLastKeyRef = useRef("");
  const applyCustomLive = async () => {
    if (!isActiveRef.current) return; // 滤镜未开启时不写屏（仅前端预览，同参考实现）
    const e = editValuesRef.current;
    const key = `${e.temperature}|${e.brightness}|${e.contrast}|${e.saturation}|${e.r_gamma}|${e.g_gamma}|${e.b_gamma}|${e.shadow ?? 0}`;
    if (key === liveApplyLastKeyRef.current) return; // 值没变，无需重复写屏
    if (liveApplyBusyRef.current) {
      liveApplyPendingRef.current = true; // 上一次还在飞：标记待补发，取最新值
      return;
    }
    liveApplyBusyRef.current = true;
    liveApplyLastKeyRef.current = key;
    try {
      const result: any = await invoke("set_filter_settings", {
        displayIndex: activeDisplayIndex,
        temperature: Math.max(1000, Math.min(10000, e.temperature)),
        brightness: Math.max(50, Math.min(150, e.brightness)),
        contrast: Math.max(50, Math.min(150, e.contrast)),
        saturation: Math.max(50, Math.min(150, e.saturation)),
        mode: 0,
        isActive: isActiveRef.current,
        rGamma: Math.max(0.5, Math.min(2.0, e.r_gamma)),
        gGamma: Math.max(0.5, Math.min(2.0, e.g_gamma)),
        bGamma: Math.max(0.5, Math.min(2.0, e.b_gamma)),
        shadow: Math.max(-100, Math.min(100, e.shadow ?? 0)),
      });
      if (result?.settings) setSettings(result.settings as FilterSettings);
    } catch (error) {
      liveApplyLastKeyRef.current = ""; // 失败后允许重试
      console.error("Failed to apply custom live:", error);
    } finally {
      liveApplyBusyRef.current = false;
      if (liveApplyPendingRef.current) {
        liveApplyPendingRef.current = false;
        void applyCustomLive(); // 补发拖动期间产生的最新值
      }
    }
  };

  const saveCurrentAsPreset = async () => {
    const name = presetNameInput.trim();
    if (!name) return;

    const temp = Math.max(1000, Math.min(10000, editValuesRef.current.temperature));
    const brightness = Math.max(50, Math.min(150, editValuesRef.current.brightness));
    const contrast = Math.max(50, Math.min(150, editValuesRef.current.contrast));
    const saturation = Math.max(50, Math.min(150, editValuesRef.current.saturation));
    const r_gamma = Math.max(0.5, Math.min(2.0, editValuesRef.current.r_gamma));
    const g_gamma = Math.max(0.5, Math.min(2.0, editValuesRef.current.g_gamma));
    const b_gamma = Math.max(0.5, Math.min(2.0, editValuesRef.current.b_gamma));

    try {
      await invoke("save_user_filter_preset", {
        id: null,
        name,
        temperature: temp,
        brightness,
        contrast,
        saturation,
        rGamma: r_gamma,
        gGamma: g_gamma,
        bGamma: b_gamma,
      });
      setShowSavePresetDialog(false);
      setPresetNameInput("");
      loadUserFilterPresets();
      toast({
        title: t("displayFilter.savePresetSuccess"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const applyUserFilterPreset = async (preset: UserFilterPresetInfo) => {
    setIsLoading(true);
    setManualPresetChange(true);
    setActivePresetId(preset.id);
    setActiveIccId(null);
    setIccPreviewFilter(null);
    setIccTintColor(null);
    setIccTintOpacity(0);

    try {
      const result: any = await invoke("apply_user_filter_preset", {
        displayIndex: activeDisplayIndex,
        id: preset.id,
        isActive: settings.is_active,
      });
      if (result.success) {
        setSettings({
          temperature: preset.temperature,
          brightness: preset.brightness,
          contrast: preset.contrast,
          saturation: preset.saturation,
          r_gamma: preset.r_gamma,
          g_gamma: preset.g_gamma,
          b_gamma: preset.b_gamma,
          shadow: 0,
          s_curve: 0.0,
          r_boost: 1.0,
          g_boost: 1.0,
          b_boost: 1.0,
          mode: 0,
          is_active: settings.is_active,
          icc_active: false,
          active_icc_id: null,
          preview_filter_icc: null,
          preview_tint_color_icc: null,
          preview_tint_opacity_icc: null,
          stacked: false,
          stack_preset_ids: [],
        });
        editValuesRef.current = {
          temperature: preset.temperature,
          brightness: preset.brightness,
          contrast: preset.contrast,
          saturation: preset.saturation,
          r_gamma: preset.r_gamma,
          g_gamma: preset.g_gamma,
          b_gamma: preset.b_gamma,
          shadow: 0,
        };
        setInputVersion(v => v + 1);
        setHasChanges(false);
      }
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
      setTimeout(() => setManualPresetChange(false), 100);
    }
  };

  const handleDeleteUserPreset = async () => {
    if (!deleteUserPresetId) return;
    setIsLoading(true);
    try {
      await invoke("delete_user_filter_preset", { id: deleteUserPresetId });
      setDeleteUserPresetId(null);
      loadUserFilterPresets();
      toast({
        title: t("displayFilter.deletePresetSuccess"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const resetToDefault = async () => {
    setManualPresetChange(true);
    setActivePresetId("");
    setActiveIccId(null);
    try {
      // Apply the 去曝光Pro ICC as display baseline
      const result: any = await invoke("apply_icc_preset", { displayIndex: activeDisplayIndex, id: "builtin_NexBox_DeExposurePro", isActive: settings.is_active });
      if (result.success) {
        const rs = result.settings;
        setSettings(prev => ({
          temperature: rs?.temperature ?? 6500,
          brightness: rs?.brightness ?? 100,
          contrast: rs?.contrast ?? 100,
          saturation: rs?.saturation ?? 100,
          r_gamma: rs?.r_gamma ?? 1.0,
          g_gamma: rs?.g_gamma ?? 1.0,
          b_gamma: rs?.b_gamma ?? 1.0,
          shadow: rs?.shadow ?? 0,
          s_curve: rs?.s_curve ?? 0.0,
          r_boost: rs?.r_boost ?? 1.0,
          g_boost: rs?.g_boost ?? 1.0,
          b_boost: rs?.b_boost ?? 1.0,
          mode: 0,
          is_active: prev.is_active,
          icc_active: rs?.icc_active ?? false,
          active_icc_id: rs?.active_icc_id ?? null,
          preview_filter_icc: rs?.preview_filter_icc ?? null,
          preview_tint_color_icc: rs?.preview_tint_color_icc ?? null,
          preview_tint_opacity_icc: rs?.preview_tint_opacity_icc ?? null,
          stacked: false,
          stack_preset_ids: [],
        }));
        const normal = {
          temperature: 6500,
          brightness: 100,
          contrast: 100,
          saturation: 100,
          r_gamma: 1.0,
          g_gamma: 1.0,
          b_gamma: 1.0,
          shadow: 0,
        };
        editValuesRef.current = normal;
        if (savedCustom) {
          setHasChanges(
            normal.temperature !== savedCustom.temperature ||
              normal.brightness !== savedCustom.brightness ||
              normal.contrast !== savedCustom.contrast ||
              normal.saturation !== savedCustom.saturation ||
              normal.r_gamma !== savedCustom.r_gamma ||
              normal.g_gamma !== savedCustom.g_gamma ||
              normal.b_gamma !== savedCustom.b_gamma ||
              normal.shadow !== (savedCustom.shadow ?? 0)
          );
        } else {
          setHasChanges(false);
        }
        setInputVersion((v) => v + 1);
        toast({
          title: t("displayFilter.resetSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setTimeout(() => setManualPresetChange(false), 100);
    }
  };

  const handleImportIcc = async () => {
    setIsLoading(true);
    try {
      const filePath: string | null = await invoke("select_icc_file");
      if (!filePath) {
        setIsLoading(false);
        return;
      }

      const result: any = await invoke("import_icc_profile", { path: filePath });
      if (result.success && result.preset) {
        setIccPresets(prev => [...prev, result.preset]);
        toast({
          title: t("displayFilter.importIccSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("displayFilter.importIccFailed"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  };

  const handleApplyIcc = async (id: string) => {
    setIsLoading(true);
    setActiveIccId(id);
    setActivePresetId(""); // 取消内置预设选中状态
    setManualPresetChange(true);
    try {
      const result: any = await invoke("apply_icc_preset", { displayIndex: activeDisplayIndex, id, isActive: settings.is_active });
      if (result.success) {
        // 存储 ICC 预览参数
        setIccPreviewFilter(result.preview_filter || null);
        setIccTintColor(result.preview_tint_color || null);
        setIccTintOpacity(result.preview_tint_opacity ?? 0);
        if (result.settings) {
          setSettings(result.settings as FilterSettings);
        }
        toast({
          title: t("displayFilter.iccApplied"),
          description: !settings.is_active ? t("displayFilter.paramsUpdatedHint") : undefined,
          status: "success",
          duration: 2500,
          isClosable: true,
        });
      }
    } catch (error) {
      setActiveIccId(null);
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
      setTimeout(() => setManualPresetChange(false), 100);
    }
  };

  const handleExportIcc = async (presetId: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      const result: string | null = await invoke("export_preset_as_icc", { presetId });
      if (result) {
        toast({
          title: t("displayFilter.exportIccSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("displayFilter.exportIccFailed"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleDeleteIcc = async () => {
    if (!deleteIccId) return;
    setIsLoading(true);
    try {
      const result: any = await invoke("delete_icc_preset", { id: deleteIccId });
      if (result.success) {
        setIccPresets(prev => prev.filter(p => p.id !== deleteIccId));
        if (activeIccId === deleteIccId) {
          setActiveIccId(null);
        }
        toast({
          title: t("displayFilter.deleteIccSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      toast({
        title: t("displayFilter.error"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setDeleteIccId(null);
      setIsLoading(false);
    }
  };

  const getTemperatureColor = (temp: number): string => {
    if (temp >= 7000) return "#e0f0ff";
    if (temp >= 6000) return "#ffffff";
    if (temp >= 5000) return "#fff4e0";
    if (temp >= 4000) return "#ffe8c0";
    if (temp >= 3000) return "#ffd080";
    return "#ffb040";
  };

  // RGB 通道独立 Gamma 的 SVG 滤镜（自定义模式下逐通道模拟伽马效果）
  /**
   * 自定义模式下的预览参数：直接取正在拖动的待提交值，实现「拖动即时预览」。
   * editValuesRef 是 ref（不会触发渲染），因此用 liveVersion/inputVersion 作为依赖。
   */
  const previewSettings = useMemo(() => {
    if (activePresetId !== "custom") return settings;
    const e = editValuesRef.current;
    return {
      ...settings,
      temperature: e.temperature,
      brightness: e.brightness,
      contrast: e.contrast,
      saturation: e.saturation,
      r_gamma: e.r_gamma,
      g_gamma: e.g_gamma,
      b_gamma: e.b_gamma,
      shadow: e.shadow ?? 0,
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activePresetId, settings, liveVersion, inputVersion]);

  const rgbGammaSvgFilter = useMemo(() => {
    const isCustom = previewSettings.mode === 0;
    const hasPerChannel = isCustom && (
      Math.abs(previewSettings.r_gamma - 1.0) > 0.001 ||
      Math.abs(previewSettings.g_gamma - 1.0) > 0.001 ||
      Math.abs(previewSettings.b_gamma - 1.0) > 0.001
    );
    if (!hasPerChannel) return null;

    // Rust 端 apply_gamma_curve: output = input ^ (1/gamma)
    // SVG feComponentTransfer gamma: C' = amplitude * C^exponent + offset
    // 所以 exponent = 1/gamma
    const rExp = (1.0 / previewSettings.r_gamma).toFixed(4);
    const gExp = (1.0 / previewSettings.g_gamma).toFixed(4);
    const bExp = (1.0 / previewSettings.b_gamma).toFixed(4);

    return (
      <svg width="0" height="0" style={{ position: "absolute", pointerEvents: "none" }}>
        <filter id="nexbox-rgb-gamma" colorInterpolationFilters="sRGB">
          <feComponentTransfer>
            <feFuncR type="gamma" amplitude="1" exponent={rExp} offset="0" />
            <feFuncG type="gamma" amplitude="1" exponent={gExp} offset="0" />
            <feFuncB type="gamma" amplitude="1" exponent={bExp} offset="0" />
          </feComponentTransfer>
        </filter>
      </svg>
    );
  }, [previewSettings]);

  // 计算 CSS filter 近似值（缓存结果，避免每次渲染触发浏览器重绘）
  const filterStyle = useMemo((): React.CSSProperties => {
    const b = previewSettings.brightness / 100;
    const c = previewSettings.contrast / 100;
    const s = previewSettings.saturation / 100;
    const params = modeParams[previewSettings.mode] || modeParams[0];

    // 判断是否使用逐通道 gamma（自定义模式且至少一个通道 gamma ≠ 1.0）
    const isCustom = previewSettings.mode === 0;
    const hasPerChannelGamma = isCustom && (
      Math.abs(previewSettings.r_gamma - 1.0) > 0.001 ||
      Math.abs(previewSettings.g_gamma - 1.0) > 0.001 ||
      Math.abs(previewSettings.b_gamma - 1.0) > 0.001
    );

    // 非自定义模式：gamma 通过加权亮度近似
    // 自定义模式：逐通道 gamma 由 SVG feComponentTransfer 处理，
    //            不再用 brightness 近似（否则无法呈现色彩偏移）
    const gammaBrightness = hasPerChannelGamma
      ? 1.0
      : 1.0 / params.gamma;

    // S-Curve 近似：增加或减少对比度
    const sCurveContrast = 1 + params.sCurve * 0.5;

    // 暗部增强近似：正值提亮暗部（提亮 + 略降对比），负值压暗暗部
    const sh = (previewSettings.shadow ?? 0) / 100;
    const shadowBrightness = 1 + 0.18 * sh;
    const shadowContrast = 1 - 0.12 * sh;

    const filterParts: string[] = [];
    if (hasPerChannelGamma) {
      filterParts.push("url(#nexbox-rgb-gamma)");
    }
    filterParts.push(
      `brightness(${(b * gammaBrightness * shadowBrightness).toFixed(3)})`,
      `contrast(${(c * sCurveContrast * shadowContrast).toFixed(3)})`,
      `saturate(${s.toFixed(3)})`,
    );

    return { filter: filterParts.join(" ") } as React.CSSProperties;
  }, [previewSettings]);

  // 色温覆盖层颜色（缓存结果）
  const temperatureOverlay = useMemo((): React.CSSProperties => {
    const t = previewSettings.temperature;
    if (t >= 6400 && t <= 6600) return { display: "none" };
    let color: string;
    let opacity: number;
    if (t < 6500) {
      // 暖色（低色温）
      const warmth = Math.min((6500 - t) / 5500, 0.5);
      color = "#FF8C00";
      opacity = warmth;
    } else {
      // 冷色（高色温）
      const coolness = Math.min((t - 6500) / 3500, 0.4);
      color = "#4A90FF";
      opacity = coolness;
    }
    return {
      position: "absolute",
      inset: 0,
      backgroundColor: color,
      mixBlendMode: "overlay",
      opacity: opacity,
      pointerEvents: "none",
    } as React.CSSProperties;
  }, [previewSettings]);

  // 拖拽分割线
  const handleSplitMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingRef.current = true;
    document.body.style.cursor = "ew-resize";
    document.body.style.userSelect = "none";

    const handleMouseMove = (ev: MouseEvent) => {
      if (!isDraggingRef.current || !previewContainerRef.current) return;
      const rect = previewContainerRef.current.getBoundingClientRect();
      const x = ev.clientX - rect.left;
      const pct = Math.max(5, Math.min(95, (x / rect.width) * 100));
      setSplitPosition(pct);
    };

    const handleMouseUp = () => {
      isDraggingRef.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  }, []);

  // 触摸拖拽
  const handleSplitTouchStart = useCallback((e: React.TouchEvent) => {
    e.preventDefault();
    isDraggingRef.current = true;

    const handleTouchMove = (ev: TouchEvent) => {
      if (!isDraggingRef.current || !previewContainerRef.current) return;
      const rect = previewContainerRef.current.getBoundingClientRect();
      const x = ev.touches[0].clientX - rect.left;
      const pct = Math.max(5, Math.min(95, (x / rect.width) * 100));
      setSplitPosition(pct);
    };

    const handleTouchEnd = () => {
      isDraggingRef.current = false;
      document.removeEventListener("touchmove", handleTouchMove);
      document.removeEventListener("touchend", handleTouchEnd);
    };

    document.addEventListener("touchmove", handleTouchMove, { passive: false });
    document.addEventListener("touchend", handleTouchEnd);
  }, []);

  const currentModeParams = modeParams[settings.mode] || modeParams[0];

  const handleInputChange = (key: keyof typeof editValuesRef.current, value: string) => {
    const isGamma = key === "r_gamma" || key === "g_gamma" || key === "b_gamma";
    const numValue = isGamma ? (parseFloat(value) || 1.0) : (parseInt(value) || 0);
    editValuesRef.current[key] = numValue;
    setHasChanges(true);
  };

  const handleDisplayChange = async (value: string) => {
    const idx = parseInt(value);
    setActiveDisplayIndex(idx);
    activeDisplayIndexRef.current = idx;
    await invoke("set_active_display", { displayIndex: idx });
    loadSettings();
    loadCustomSettings();
  };

  // 主显示器的实际 index（"主显示器"入口与列表中的主屏型号项共用该值）
  const primaryDisplayIndex = useMemo(
    () => displays.find((d) => d.is_primary)?.index ?? displays[0]?.index ?? 0,
    [displays]
  );

  const gammaChannelColors: Record<string, string> = {
    r_gamma: "#FF4444",
    g_gamma: "#44CC44",
    b_gamma: "#4488FF",
  };

  const GammaSliderItem = ({
    channelKey,
    label,
    value,
    onChange,
    resetKey,
  }: {
    channelKey: string;
    label: string;
    value: number;
    onChange: (val: number) => void;
    resetKey: number;
  }) => {
    const dotColor = gammaChannelColors[channelKey] || "#888888";
    const [localValue, setLocalValue] = useState(value);

    // Sync with external value changes (reset)
    useEffect(() => {
      setLocalValue(value);
    }, [value, resetKey]);

    const handleChange = (v: number) => {
      setLocalValue(v);
      onChange(v);
    };

    return (
      <Box position="relative" borderRadius="lg">
        {liquidGlassEnabled && (
          <Box style={getBorderGlowStyle(miniGlassGlow)} />
        )}
        <VStack
          spacing={1}
          py={2}
          px={3}
          borderRadius="lg"
          overflow="hidden"
          bg={liquidGlassEnabled ? miniGlassBg : infoBg}
          backdropFilter={`blur(${effectiveBlur}px)`}
          sx={{
            transform: "translateZ(0)",
            WebkitTransform: "translateZ(0)",
            WebkitBackfaceVisibility: "hidden",
            backfaceVisibility: "hidden",
            willChange: "backdrop-filter, transform",
          }}
          transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
          border={liquidGlassEnabled ? "1px solid" : "none"}
          borderColor={liquidGlassEnabled ? miniGlassBorder : "transparent"}
        >
          <HStack w="full" justify="space-between">
            <HStack spacing={1.5}>
              <Box w={2.5} h={2.5} borderRadius="full" bg={dotColor} />
              <Text color={subTextColor} fontSize="xs" fontWeight="500">
                {label}
              </Text>
            </HStack>
            <Input
              type="number"
              min={0.50}
              max={2.00}
              step={0.01}
              value={localValue.toFixed(2)}
              onChange={(e) => {
                const v = parseFloat(e.target.value);
                if (!isNaN(v)) handleChange(v);
              }}
              size="xs"
              w="56px"
              h="22px"
              textAlign="right"
              fontWeight="600"
              fontSize="xs"
              color={textColor}
              bg={inputBg}
              borderColor={cardBorder}
              borderRadius="md"
              px={1.5}
              _focus={{ borderColor: dotColor, boxShadow: "none" }}
            />
          </HStack>
          <Slider
            aria-label={label}
            min={0.50}
            max={2.00}
            step={0.01}
            value={localValue}
            onChange={handleChange}
            focusThumbOnChange={false}
            size="sm"
          >
            <SliderTrack bg={sliderBg} h="3px" borderRadius="full">
              <SliderFilledTrack bg={dotColor} />
            </SliderTrack>
            <SliderThumb boxSize="12px" bg={dotColor} />
          </Slider>
        </VStack>
      </Box>
    );
  };

  const SliderInputItem = ({
    label,
    value,
    onChange,
    min,
    max,
    step,
    unit,
    colorValue,
    resetKey,
  }: {
    label: string;
    value: number;
    onChange: (val: number) => void;
    min: number;
    max: number;
    step: number;
    unit: string;
    colorValue?: number;
    resetKey: number;
  }) => {
    const [localValue, setLocalValue] = useState(value);

    useEffect(() => {
      setLocalValue(value);
    }, [value, resetKey]);

    const handleChange = (v: number) => {
      setLocalValue(v);
      onChange(v);
    };

    const dotColor = colorValue !== undefined ? getTemperatureColor(colorValue) : undefined;

    return (
      <Box position="relative" borderRadius="lg">
        {liquidGlassEnabled && (
          <Box style={getBorderGlowStyle(miniGlassGlow)} />
        )}
        <VStack
          spacing={1}
          py={2}
          px={3}
          borderRadius="lg"
          overflow="hidden"
          bg={liquidGlassEnabled ? miniGlassBg : infoBg}
          backdropFilter={`blur(${effectiveBlur}px)`}
          sx={{
            transform: "translateZ(0)",
            WebkitTransform: "translateZ(0)",
            WebkitBackfaceVisibility: "hidden",
            backfaceVisibility: "hidden",
            willChange: "backdrop-filter, transform",
          }}
          transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
          border={liquidGlassEnabled ? "1px solid" : "none"}
          borderColor={liquidGlassEnabled ? miniGlassBorder : "transparent"}
        >
          <HStack w="full" justify="space-between">
            <HStack spacing={1.5}>
              <Text color={subTextColor} fontSize="xs" fontWeight="500">
                {label}
              </Text>
              {dotColor && (
                <Box
                  w={2.5} h={2.5}
                  borderRadius="full"
                  bg={dotColor}
                  border="1px solid"
                  borderColor={cardBorder}
                />
              )}
            </HStack>
            <HStack spacing={1}>
              <Input
                type="number"
                min={min}
                max={max}
                step={step}
                value={localValue}
                onChange={(e) => {
                  const v = step < 1 ? parseFloat(e.target.value) : parseInt(e.target.value);
                  if (!isNaN(v)) handleChange(v);
                }}
                size="xs"
                w="60px"
                h="22px"
                textAlign="right"
                fontWeight="600"
                fontSize="xs"
                color={textColor}
                bg={inputBg}
                borderColor={cardBorder}
                borderRadius="md"
                px={1.5}
                _focus={{ borderColor: primaryColor, boxShadow: "none" }}
              />
              <Text color={subTextColor} fontSize="xs" minW="16px">{unit}</Text>
            </HStack>
          </HStack>
          <Slider
            aria-label={label}
            min={min}
            max={max}
            step={step}
            value={localValue}
            onChange={handleChange}
            focusThumbOnChange={false}
            size="sm"
          >
            <SliderTrack bg={sliderBg} h="3px" borderRadius="full">
              <SliderFilledTrack bg={primaryColor} />
            </SliderTrack>
            <SliderThumb boxSize="12px" bg={primaryColor} />
          </Slider>
        </VStack>
      </Box>
    );
  };

  const EditableItem = ({ 
    label, 
    value, 
    onChange, 
    unit, 
    colorValue 
  }: { 
    label: string; 
    value: number; 
    onChange: (val: string) => void;
    unit: string;
    colorValue?: number;
  }) => (
    <Box position="relative" overflow="hidden" borderRadius="lg">
      {liquidGlassEnabled && (
        <Box style={getBorderGlowStyle(miniGlassGlow)} />
      )}
      <HStack
        justify="space-between"
        py={2}
        px={3}
        borderRadius="lg"
        bg={liquidGlassEnabled ? miniGlassBg : infoBg}
        backdropFilter={`blur(${effectiveBlur}px)`}
        sx={liquidGlassEnabled ? {
          transform: "translateZ(0)",
          WebkitTransform: "translateZ(0)",
          WebkitBackfaceVisibility: "hidden",
          backfaceVisibility: "hidden",
        } : undefined}
        border={liquidGlassEnabled ? "1px solid" : "none"}
        borderColor={liquidGlassEnabled ? miniGlassBorder : "transparent"}
      >
        <HStack spacing={2}>
          <Text color={subTextColor} fontSize="sm">
            {label}
          </Text>
          {colorValue !== undefined && (
            <Box 
              w={3} 
              h={3} 
              borderRadius="full" 
              bg={getTemperatureColor(colorValue)}
              border="1px solid"
              borderColor={cardBorder}
            />
          )}
        </HStack>
        <HStack spacing={2}>
          <Input
            key={`${label}-${inputVersion}`}
            defaultValue={value}
            onChange={(e) => onChange(e.target.value)}
            size="xs"
            w="70px"
            h="24px"
            textAlign="right"
            fontWeight="600"
            color={textColor}
            bg={inputBg}
            borderColor={cardBorder}
            borderRadius="md"
            px={2}
            _focus={{ borderColor: primaryColor, boxShadow: "none" }}
          />
          <Text color={subTextColor} fontSize="sm" minW="20px">
            {unit}
          </Text>
        </HStack>
      </HStack>
    </Box>
  );

  const ReadOnlyItem = ({ label, value, unit = "", colorValue }: { 
    label: string; 
    value: string | number; 
    unit?: string;
    colorValue?: number;
  }) => (
    <Box position="relative" overflow="hidden" borderRadius="lg">
      {liquidGlassEnabled && (
        <Box style={getBorderGlowStyle(miniGlassGlow)} />
      )}
      <HStack
        justify="space-between"
        py={2}
        px={3}
        borderRadius="lg"
        bg={liquidGlassEnabled ? miniGlassBg : infoBg}
        backdropFilter={`blur(${effectiveBlur}px)`}
        sx={liquidGlassEnabled ? {
          transform: "translateZ(0)",
          WebkitTransform: "translateZ(0)",
          WebkitBackfaceVisibility: "hidden",
          backfaceVisibility: "hidden",
        } : undefined}
        border={liquidGlassEnabled ? "1px solid" : "none"}
        borderColor={liquidGlassEnabled ? miniGlassBorder : "transparent"}
      >
        <Text color={subTextColor} fontSize="sm">
          {label}
        </Text>
        <HStack>
          {colorValue !== undefined && (
            <Box 
              w={3} 
              h={3} 
              borderRadius="full" 
              bg={getTemperatureColor(colorValue)}
              border="1px solid"
              borderColor={cardBorder}
            />
          )}
          <Text color={textColor} fontSize="sm" fontWeight="600">
            {value}{unit}
          </Text>
        </HStack>
      </HStack>
    </Box>
  );

  // ── 滤镜预设分组 ──
  // 新境盒 = 参数化内置预设；冠军调试 = 纯 ICC 驱动的 champion-N 系列；自定义 = 数值调节面板。
  const nexboxPresets = useMemo(
    () => presets.filter((p) => !isChampionPreset(p)),
    [presets]
  );
  const championPresets = useMemo(
    () => presets.filter(isChampionPreset),
    [presets]
  );

  /** Tab 项：我的预设放第一位；新境盒 / 冠军调试 用图片 Logo，我的预设/自定义 用图标。默认进入新境盒。 */
  const presetTabs: {
    key: "mine" | "nexbox" | "champion" | "custom";
    label: string;
    count: number | null;
    logo: string | null;
    icon?: typeof Settings2;
  }[] = [
    { key: "mine", label: t("displayFilter.myPresets"), count: userPresets.length, logo: null, icon: Bookmark },
    { key: "nexbox", label: t("displayFilter.groupNexbox"), count: nexboxPresets.length, logo: nexboxLogo },
    { key: "champion", label: t("displayFilter.groupChampion"), count: championPresets.length, logo: championLogo },
    { key: "custom", label: t("displayFilter.custom"), count: null, logo: null, icon: Settings2 },
  ];

  /** 切换预设分组；进入「自定义」时加载并应用已保存的自定义参数。 */
  const handlePresetTabChange = (key: "mine" | "nexbox" | "champion" | "custom") => {
    setPresetTab(key);
    if (key === "custom" && activePresetId !== "custom") {
      openCustom();
    }
  };

  /** 稳定回调（ref 转发）：让 memo 化的卡片在页面其它状态变化时不因回调身份变化而重渲染。 */
  const presetCardSelectRef = useRef<(preset: FilterPreset) => void>(() => {});
  presetCardSelectRef.current = (preset) =>
    multiFilterEnabled ? toggleStackSelection(preset.id) : applyPreset(preset);
  const handlePresetCardSelect = useCallback(
    (preset: FilterPreset) => presetCardSelectRef.current(preset),
    []
  );

  const presetCardExportRef = useRef<(id: string, e: React.MouseEvent) => void>(() => {});
  presetCardExportRef.current = handleExportIcc;
  const handlePresetCardExport = useCallback(
    (id: string, e: React.MouseEvent) => presetCardExportRef.current(id, e),
    []
  );

  /** 单选模式下：当前参数与某个「我的预设」完全一致时高亮该卡片（我的预设本质是参数化预设）。 */
  const matchesUserPreset = (p: UserFilterPresetInfo) =>
    activePresetId === "custom" &&
    settings.temperature === p.temperature &&
    settings.brightness === p.brightness &&
    settings.contrast === p.contrast &&
    settings.saturation === p.saturation &&
    settings.r_gamma === p.r_gamma &&
    settings.g_gamma === p.g_gamma &&
    settings.b_gamma === p.b_gamma &&
    settings.shadow === 0;

  /** 渲染预设卡片（卡片本体已抽为 memo 组件，点击时只重渲染新旧两张卡，避免整页卡顿）。 */
  const renderPresetCard = (preset: FilterPreset) => (
    <PresetCard
      key={preset.id}
      preset={preset}
      isActive={
        multiFilterEnabled
          ? selectedStackIds.includes(preset.id)
          : activePresetId === preset.id
      }
      multiFilterEnabled={multiFilterEnabled}
      accentColor={
        presetColors[preset.id] ||
        (isChampionPreset(preset) ? championColor(preset.id) : primaryColor)
      }
      onSelect={handlePresetCardSelect}
      onExport={handlePresetCardExport}
    />
  );


  /** 拇指调试风格的简洁滑块：标题 + 实时数值一行，纯 Chakra Slider（无玻璃容器，重绘开销最小）。
   *  onChange = 拖动中（更新预览 + 开关开启时实时写屏，applyCustomLive 内部已做
   *  latest-wins 串行化，拖多快都只有一个 invoke 在飞）；onCommit = 松手兜底落屏。
   *  ⚠️ 必须用 useMemo 固定组件引用：直接在组件体内定义会让每次渲染产生新的组件类型，
   *  React 会把整个 Slider 卸载重建，正在进行的拖动手势被掐断 —— 表现为「每次只能 +1」。 */
  const PresetSlider = useMemo(
    () =>
      function PresetSlider({
        label,
        value,
        min,
        max,
        step,
        unit = "",
        decimals = 0,
        onChange,
        onCommit,
      }: {
        label: string;
        value: number;
        min: number;
        max: number;
        step: number;
        unit?: string;
        decimals?: number;
        onChange: (v: number) => void;
        onCommit?: (v: number) => void;
      }) {
        return (
          <VStack align="stretch" spacing={1}>
            <HStack justify="space-between">
              <Text color={subTextColor} fontSize="sm">
                {label}
              </Text>
              <Text color={primaryColor} fontSize="sm" fontWeight="600">
                {decimals > 0 ? value.toFixed(decimals) : Math.round(value)}
                {unit}
              </Text>
            </HStack>
            <Slider
              aria-label={label}
              min={min}
              max={max}
              step={step}
              value={value}
              focusThumbOnChange={false}
              onChange={(v) => { onChange(v); onCommit?.(v); }}
              onChangeEnd={(v) => onCommit?.(v)}
            >
              <SliderTrack bg={sliderBg} h="4px" borderRadius="full">
                <SliderFilledTrack bg={primaryColor} />
              </SliderTrack>
              <SliderThumb boxSize="14px" bg={primaryColor} />
            </Slider>
          </VStack>
        );
      },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sliderBg, primaryColor, subTextColor]
  );

  const content = (
    <VStack align="start" spacing={6}>
      <Flex justify="space-between" align="flex-start" w="full" gap={4}>
        <HStack flexShrink={0}>
          <IconButton
            aria-label={t("builtinTools.back")}
            icon={<ArrowLeft size={20} />}
            variant="ghost"
            onClick={() => navigate("/builtin-tools")}
            color={headingColor}
          />
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700" whiteSpace="nowrap">
            {t("displayFilter.title")}
          </Heading>
        </HStack>
        <VStack align="flex-end" spacing={1} flex={1} minW={0}>
          <HStack justify="flex-end">
            <Tooltip label={t("displayFilter.resetDefault")}>
              <IconButton
                aria-label="Reset"
                icon={<RotateCcw size={18} />}
                variant="ghost"
                onClick={resetToDefault}
                isDisabled={isLoading}
              />
            </Tooltip>
            <HStack spacing={4} flexWrap="wrap" justify="flex-end">
              <HotkeyRecorder
                value={filterHotkey}
                onChange={async (val) => {
                  const err = await saveFilterHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("displayFilter.hotkeySaved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
              <HStack
                bg={settings.is_active ? hexToRgba(primaryColor, 0.2) : sliderBg}
                px={4}
                py={2}
                borderRadius="xl"
                border="1px solid"
                borderColor={settings.is_active ? primaryColor : "transparent"}
              >
                <Text color={textColor} fontSize="sm" fontWeight="500">
                  {t("displayFilter.enable")}
                </Text>
                <ThemeSwitch
                  isChecked={settings.is_active}
                  onChange={toggleFilter}
                  isDisabled={isLoading}
                />
              </HStack>
            </HStack>
          </HStack>
          </VStack>
      </Flex>

      {!iccToolsInstalled && (
        <LiquidGlassCard px={5} py={4} w="full">
          <HStack spacing={4} align="center" w="full" flexWrap="wrap">
            <Flex
              flexShrink={0}
              w="40px"
              h="40px"
              borderRadius="lg"
              align="center"
              justify="center"
              bg={hexToRgba(primaryColor, 0.14)}
            >
              <Download size={20} color={primaryColor} />
            </Flex>
            <VStack align="stretch" spacing={1} flex={1} minW={0}>
              <Text fontSize="md" fontWeight="700" color={headingColor}>
                {t("displayFilter.iccToolsTitle")}
              </Text>
              <Text fontSize="sm" color={subTextColor}>
                {t("displayFilter.iccToolsDesc")}
              </Text>
              {iccToolsBusy && iccToolsProgress && (
                <Box pt={1}>
                  <HStack justify="space-between" mb={1}>
                    <Text color={subTextColor} fontSize="xs" noOfLines={1}>
                      {iccToolsProgress.detail}
                    </Text>
                    <Text color={primaryColor} fontSize="xs" fontWeight="600" flexShrink={0}>
                      {iccToolsProgress.progress}%
                    </Text>
                  </HStack>
                  <Progress
                    value={iccToolsProgress.progress}
                    h="5px"
                    borderRadius="sm"
                    bg={hexToRgba(primaryColor, 0.14)}
                    sx={{ "& > div": { background: primaryColor } }}
                  />
                </Box>
              )}
            </VStack>
            <Button
                    flexShrink={0}
                    size="sm"
                    leftIcon={<Download size={14} />}
                    onClick={() => void downloadIccTools()}
                    isLoading={iccToolsBusy}
                    loadingText={t("displayFilter.iccToolsDownloading")}
                    isDisabled={iccToolsBusy}
                    bg={primaryColor}
                    color={contrastText}
                    _hover={{ bg: getHoverColor() }}
                  >
                    {t("displayFilter.iccToolsDownloadBtn")}
                  </Button>
          </HStack>
        </LiquidGlassCard>
      )}

      <HStack spacing={2} justify="flex-start" w="full" flexWrap="nowrap">
        <Box mr="auto">
          <Menu>
          <MenuButton
            as={Box}
            bg="transparent"
            p={0}
            border="none"
            cursor="pointer"
          >
            <LiquidGlassCard px={3} py={1.5} w="220px" flexShrink={1}>
              <HStack justify="space-between" spacing={6}>
                <HStack spacing={2}>
                  <Monitor size={14} />
                  <Text fontSize="sm" color={textColor}>
                    {usePrimaryDisplay
                      ? t("crosshair.primaryMonitor")
                      : stripResolution(displays.find((d) => d.index === activeDisplayIndex)?.name) || t("crosshair.primaryMonitor")}
                  </Text>
                </HStack>
                <ChevronDown size={16} />
              </HStack>
            </LiquidGlassCard>
          </MenuButton>
          <Portal>
            <MenuList bg={menuListBg} borderColor={cardBorder} maxH="300px" overflowY="auto" zIndex={9999}>
              <MenuItem
                onClick={() => {
                  setUsePrimaryDisplay(true);
                  handleDisplayChange(String(primaryDisplayIndex));
                }}
                bg={usePrimaryDisplay && activeDisplayIndex === primaryDisplayIndex ? hoverBg : "transparent"}
                _hover={{ bg: hoverBg }}
              >
                <HStack spacing={2} w="full" justify="space-between">
                  <Text fontSize="sm">{t("crosshair.primaryMonitor")}</Text>
                  {usePrimaryDisplay && activeDisplayIndex === primaryDisplayIndex && <Check size={14} color={primaryColor} />}
                </HStack>
              </MenuItem>
              {displays.map((d) => {
                const isPrimaryItem = d.index === primaryDisplayIndex;
                return (
                  <MenuItem
                    key={d.index}
                    onClick={() => {
                      setUsePrimaryDisplay(false);
                      handleDisplayChange(String(d.index));
                    }}
                    bg={activeDisplayIndex === d.index && !(isPrimaryItem && usePrimaryDisplay) ? hoverBg : "transparent"}
                    _hover={{ bg: hoverBg }}
                  >
                    <HStack spacing={2} w="full" justify="space-between">
                      <Text fontSize="sm">{d.name}</Text>
                      {activeDisplayIndex === d.index && !(isPrimaryItem && usePrimaryDisplay) && <Check size={14} color={primaryColor} />}
                    </HStack>
                  </MenuItem>
                );
              })}
            </MenuList>
          </Portal>
        </Menu>
        </Box>
        <HStack
          bg={multiFilterEnabled ? hexToRgba(primaryColor, 0.15) : sliderBg}
          px={4}
          py={2}
          borderRadius="xl"
          border="1px solid"
          borderColor={multiFilterEnabled ? primaryColor : "transparent"}
          flexShrink={1}
          minW={0}
        >
          <Text color={textColor} fontSize="xs" fontWeight="500" noOfLines={1}>
            {t("displayFilter.multiFilterTitle")}
          </Text>
          <ThemeSwitch
            isChecked={multiFilterEnabled}
            onChange={(e) => handleToggleMultiFilter(e.target.checked)}
            isDisabled={isLoading}
          />
        </HStack>
        <HStack
          bg={gameFilterEnabled ? hexToRgba(primaryColor, 0.15) : sliderBg}
          px={4}
          py={2}
          borderRadius="xl"
          border="1px solid"
          borderColor={gameFilterEnabled ? primaryColor : "transparent"}
          flexShrink={1}
          minW={0}
        >
          <Text color={textColor} fontSize="xs" fontWeight="500" noOfLines={1}>
            {t("displayFilter.gameFilterTitle")}
          </Text>
          <Text
            color={primaryColor}
            fontSize="xs"
            fontWeight="600"
            cursor="pointer"
            onClick={() => setIsGameFilterListOpen(true)}
            _hover={{ textDecoration: "underline" }}
            noOfLines={1}
          >
            {t("displayFilter.gameFilterSupportedTitle")} ({gameFilterGames.length})
          </Text>
          <ThemeSwitch
            isChecked={gameFilterEnabled}
            onChange={(e) => handleToggleGameFilter(e.target.checked)}
            isDisabled={isGameFilterBusy}
          />
        </HStack>
        <HStack
          bg={autoApplyOnStartup ? hexToRgba(primaryColor, 0.15) : sliderBg}
          px={4}
          py={2}
          borderRadius="xl"
          border="1px solid"
          borderColor={autoApplyOnStartup ? primaryColor : "transparent"}
          flexShrink={1}
          minW={0}
        >
          <Text color={textColor} fontSize="xs" fontWeight="500" noOfLines={1}>
            启动新境盒时自动启用选中滤镜
          </Text>
          <ThemeSwitch
            isChecked={autoApplyOnStartup}
            onChange={(e) => {
              const val = e.target.checked;
              setAutoApplyOnStartup(val);
              localStorage.setItem("nexbox_auto_apply", val ? "true" : "false");
              store.set("nexbox_auto_apply", val).then(() => store.save());
            }}
            isDisabled={isLoading}
          />
        </HStack>
      </HStack>

      <VStack align="start" spacing={4} w="full">
        <HStack justify="space-between" align="center" w="full" flexWrap="wrap" gap={3}>
          <Text color={textColor} fontSize="md" fontWeight="600">
            {t("displayFilter.presets")}
          </Text>

          {/* 分组切换 Tab：我的预设 / 新境盒 / 冠军调试 / 自定义（默认新境盒） */}
          <HStack
            spacing={1}
            p={1}
            borderRadius="xl"
            bg={liquidGlassEnabled ? miniGlassBg : sliderBg}
            border="1px solid"
            borderColor={liquidGlassEnabled ? miniGlassBorder : "transparent"}
          >
            {presetTabs.map((opt) => {
              const active = presetTab === opt.key;
              return (
                <Button
                  key={opt.key}
                  size="sm"
                  variant="ghost"
                  leftIcon={
                    opt.logo ? (
                      <Image
                        src={opt.logo}
                        boxSize="18px"
                        objectFit="contain"
                        opacity={active ? 1 : 0.6}
                        transition="opacity 0.2s"
                        alt={opt.label}
                      />
                    ) : (
                      <Icon
                        as={opt.icon || Settings2}
                        boxSize="18px"
                        color={active ? textColor : subTextColor}
                        opacity={active ? 1 : 0.6}
                      />
                    )
                  }
                  onClick={() => handlePresetTabChange(opt.key)}
                  bg={active ? tabActiveBg : "transparent"}
                  color={active ? textColor : subTextColor}
                  fontWeight={active ? "700" : "500"}
                  borderRadius="lg"
                  px={3}
                  h="30px"
                  _hover={{ color: textColor }}
                >
                  {opt.label}{opt.count !== null ? ` (${opt.count})` : ""}
                </Button>
              );
            })}
          </HStack>
        </HStack>

        {presetTab === "mine" ? (
          userPresets.length === 0 ? (
            <Text color={subTextColor} fontSize="sm" py={2}>
              {t("displayFilter.noPresets")}
            </Text>
          ) : (
            <SimpleGrid
              columns={{
                base: 2,
                sm: 3,
                md: 4,
                lg: 5,
              }}
              spacing={3}
              w="full"
            >
              {userPresets.map((preset) => {
                const accentColor = "#8B5CF6";
                const isActive = multiFilterEnabled
                  ? selectedStackIds.includes(preset.id)
                  : activePresetId === preset.id || matchesUserPreset(preset);
                return (
                  <Box
                    key={preset.id}
                    bg={liquidGlassEnabled
                      ? (isActive ? hexToRgba(accentColor, 0.2) : miniGlassBg)
                      : (isActive ? `${accentColor}20` : sliderBg)}
                    borderRadius="xl"
                    p={4}
                    cursor="pointer"
                    onClick={() => (multiFilterEnabled ? toggleStackSelection(preset.id) : applyUserFilterPreset(preset))}
                    border={liquidGlassEnabled ? "1px solid" : "2px solid"}
                    borderColor={liquidGlassEnabled
                      ? (isActive ? accentColor : miniGlassBorder)
                      : (isActive ? accentColor : "transparent")}
                    backdropFilter={`blur(${effectiveBlur}px)`}
                    sx={{
                      transform: "translateZ(0)",
                      WebkitTransform: "translateZ(0)",
                      WebkitBackfaceVisibility: "hidden",
                      backfaceVisibility: "hidden",
                      willChange: "backdrop-filter, transform",
                    }}
                    transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
                    _hover={{
                      borderColor: accentColor,
                    }}
                    position="relative"
                    overflow="hidden"
                  >
                    {liquidGlassEnabled && (
                      <Box
                        style={getBorderGlowStyle(
                          isActive ? hexToRgba(accentColor, 0.5) : miniGlassGlow
                        )}
                      />
                    )}
                    {multiFilterEnabled ? (
                      isActive && (
                        <HStack
                          position="absolute"
                          top={1.5}
                          right={1.5}
                          spacing={1}
                          bg={hexToRgba(accentColor, 0.9)}
                          color="#ffffff"
                          px={1.5}
                          py={0.5}
                          borderRadius="full"
                          fontSize="9px"
                          fontWeight="700"
                          pointerEvents="none"
                          zIndex={2}
                        >
                          <Box as="span">✓</Box>
                          <Text as="span">{t("displayFilter.multiFilterSelected")}</Text>
                        </HStack>
                      )
                    ) : (
                      <IconButton
                        aria-label={t("displayFilter.delete")}
                        icon={<Trash2 size={14} />}
                        size="xs"
                        variant="ghost"
                        position="absolute"
                        top={1}
                        right={1}
                        color={subTextColor}
                        opacity={0.5}
                        _hover={{ opacity: 1, color: "red.400" }}
                        onClick={(e) => {
                          e.stopPropagation();
                          setDeleteUserPresetId(preset.id);
                        }}
                      />
                    )}
                    <VStack spacing={2}>
                      <Bookmark size={24} color={accentColor} />
                      <Text color={textColor} fontSize="sm" fontWeight="600">
                        {preset.name}
                      </Text>
                    </VStack>
                  </Box>
                );
              })}
            </SimpleGrid>
          )
        ) : presetTab === "custom" ? (
          <Box
            w="full"
            p={5}
            borderRadius="xl"
            border="1px solid"
            borderColor={cardBorder}
            bg={sliderBg}
          >
            <VStack align="stretch" spacing={5}>
              <HStack justify="space-between" align="center" flexWrap="wrap" gap={3}>
                <HStack spacing={2}>
                  <Settings2 size={18} color={presetColors["custom"]} />
                  <Text color={textColor} fontSize="md" fontWeight="600">
                    {t("displayFilter.custom")}
                  </Text>
                </HStack>
                <HStack spacing={2} flexWrap="wrap">
                  <Button
                    size="sm"
                    leftIcon={<Bookmark size={14} />}
                    variant="outline"
                    borderColor={primaryColor}
                    color={primaryColor}
                    onClick={() => {
                      setPresetNameInput("");
                      setShowSavePresetDialog(true);
                    }}
                    isDisabled={isLoading}
                    _hover={{ bg: `${primaryColor}15` }}
                  >
                    {t("displayFilter.saveAsPreset")}
                  </Button>
                  <Button
                    size="sm"
                    leftIcon={<Download size={14} />}
                    variant="outline"
                    borderColor={primaryColor}
                    color={primaryColor}
                    onClick={handleExportCustom}
                    isDisabled={isLoading}
                    _hover={{ bg: `${primaryColor}15` }}
                  >
                    {t("displayFilter.exportIcc")}
                  </Button>
                  <Button
                    size="sm"
                    leftIcon={<RotateCcw size={14} />}
                    variant="ghost"
                    color={subTextColor}
                    onClick={resetCustomValues}
                    isDisabled={isLoading}
                    _hover={{ color: textColor, bg: sliderBg }}
                  >
                    {t("displayFilter.resetDefault")}
                  </Button>
                </HStack>
              </HStack>

              <SimpleGrid columns={{ base: 1, lg: 2 }} spacing={6} w="full">
                <VStack spacing={4} align="stretch">
                  <PresetSlider
                    label={t("displayFilter.colorTemperature")}
                    value={editValuesRef.current.temperature}
                    min={1000} max={10000} step={100}
                    unit=" K"
                    onChange={(v) => { editValuesRef.current.temperature = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.brightness")}
                    value={editValuesRef.current.brightness}
                    min={50} max={150} step={1}
                    unit="%"
                    onChange={(v) => { editValuesRef.current.brightness = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.contrast")}
                    value={editValuesRef.current.contrast}
                    min={50} max={150} step={1}
                    unit="%"
                    onChange={(v) => { editValuesRef.current.contrast = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.saturation")}
                    value={editValuesRef.current.saturation}
                    min={50} max={150} step={1}
                    unit="%"
                    onChange={(v) => { editValuesRef.current.saturation = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.shadowBoost")}
                    value={editValuesRef.current.shadow ?? 0}
                    min={-100} max={100} step={1}
                    unit=""
                    onChange={(v) => { editValuesRef.current.shadow = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                </VStack>

                <VStack spacing={4} align="stretch">
                  <Text color={subTextColor} fontSize="10px" fontWeight="600">
                    {t("displayFilter.rgbGamma")}
                  </Text>
                  <PresetSlider
                    label={t("displayFilter.rGamma")}
                    value={editValuesRef.current.r_gamma}
                    min={0.5} max={2.0} step={0.01}
                    decimals={2}
                    onChange={(v) => { editValuesRef.current.r_gamma = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.gGamma")}
                    value={editValuesRef.current.g_gamma}
                    min={0.5} max={2.0} step={0.01}
                    decimals={2}
                    onChange={(v) => { editValuesRef.current.g_gamma = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                  <PresetSlider
                    label={t("displayFilter.bGamma")}
                    value={editValuesRef.current.b_gamma}
                    min={0.5} max={2.0} step={0.01}
                    decimals={2}
                    onChange={(v) => { editValuesRef.current.b_gamma = v; setHasChanges(true); setLiveVersion((x) => x + 1); }}
                    onCommit={applyCustomLive}
                  />
                </VStack>
              </SimpleGrid>
            </VStack>
          </Box>
        ) : (
          <SimpleGrid
            columns={{
              base: 2,
              sm: 3,
              md: 4,
              lg: 5,
            }}
            spacing={3}
            w="full"
          >
            {(presetTab === "nexbox" ? nexboxPresets : championPresets).map(renderPresetCard)}
          </SimpleGrid>
        )}
      </VStack>

      {/* ICC Color Profiles Section */}
      <VStack align="start" spacing={4} w="full">
        <HStack justify="space-between" w="full">
          <HStack>
            <FileImage size={20} color={textColor} />
            <Text color={textColor} fontSize="md" fontWeight="600">
              {t("displayFilter.iccProfiles")}
            </Text>
          </HStack>
          <Button
            size="sm"
            leftIcon={<Upload size={16} />}
            variant="outline"
            borderColor={primaryColor}
            color={primaryColor}
            _hover={{ bg: `${primaryColor}15` }}
            onClick={handleImportIcc}
            isLoading={isLoading}
          >
            {t("displayFilter.importIcc")}
          </Button>
        </HStack>
        {iccPresets.length === 0 ? (
          <Text color={subTextColor} fontSize="sm" py={2}>
            {t("displayFilter.noIccProfiles")}
          </Text>
        ) : (
          <SimpleGrid
            columns={{
              base: 2,
              sm: 3,
              md: 4,
              lg: 5,
            }}
            spacing={3}
            w="full"
          >
            {iccPresets.map((icc) => {
              // 多选模式：选中态由 selectedStackIds 驱动；单选模式保持原有 activeIccId 高亮
              const isActive = multiFilterEnabled
                ? selectedStackIds.includes(icc.id)
                : activeIccId === icc.id;
              const accentColor = "#38B2AC";
              return (
                <Box
                  key={icc.id}
                  bg={liquidGlassEnabled
                    ? (isActive ? hexToRgba(accentColor, 0.2) : miniGlassBg)
                    : (isActive ? `${accentColor}20` : sliderBg)}
                  borderRadius="xl"
                  p={4}
                  cursor="pointer"
                  onClick={() => (multiFilterEnabled ? toggleStackSelection(icc.id) : handleApplyIcc(icc.id))}
                  border={liquidGlassEnabled ? "1px solid" : "2px solid"}
                  borderColor={liquidGlassEnabled
                    ? (isActive ? accentColor : miniGlassBorder)
                    : (isActive ? accentColor : "transparent")}
                  backdropFilter={`blur(${effectiveBlur}px)`}
                  sx={{
                    transform: "translateZ(0)",
                    WebkitTransform: "translateZ(0)",
                    WebkitBackfaceVisibility: "hidden",
                    backfaceVisibility: "hidden",
                    willChange: "backdrop-filter, transform",
                  }}
                  transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
                  _hover={{
                    borderColor: accentColor,
                  }}
                  position="relative"
                  overflow="hidden"
                >
                  {liquidGlassEnabled && (
                    <Box
                      style={getBorderGlowStyle(
                        isActive ? hexToRgba(accentColor, 0.5) : miniGlassGlow
                      )}
                    />
                  )}
                  
                  {multiFilterEnabled ? (
                    isActive && (
                      <HStack
                        position="absolute"
                        top={1.5}
                        right={1.5}
                        spacing={1}
                        bg={hexToRgba(accentColor, 0.9)}
                        color="#ffffff"
                        px={1.5}
                        py={0.5}
                        borderRadius="full"
                        fontSize="9px"
                        fontWeight="700"
                        pointerEvents="none"
                        zIndex={2}
                      >
                        <Box as="span">✓</Box>
                        <Text as="span">{t("displayFilter.multiFilterSelected")}</Text>
                      </HStack>
                    )
                  ) : (
                    <Tooltip label={icc.description} placement="top">
                      <IconButton
                        aria-label={t("displayFilter.deleteIcc")}
                        icon={<Trash2 size={14} />}
                        size="xs"
                        variant="ghost"
                        position="absolute"
                        top={1}
                        right={1}
                        color="red.400"
                        _hover={{ bg: "red.50" }}
                        onClick={(e) => {
                          e.stopPropagation();
                          setDeleteIccId(icc.id);
                        }}
                      />
                    </Tooltip>
                  )}
                  <VStack spacing={2}>
                    <FileImage size={24} color={accentColor} />
                    <Text color={textColor} fontSize="sm" fontWeight="600" noOfLines={1}>
                      {icc.name}
                    </Text>
                  </VStack>
                </Box>
              );
            })}
          </SimpleGrid>
        )}
      </VStack>

      {/* 滤镜预览 + 当前设置 (3:1) */}
      <Flex w="full" gap={4} align="stretch" direction={{ base: "column", lg: "row" }}>
        {/* 左侧 3/4：滤镜预览对比器 */}
        <Box
          w={{ base: "full", lg: "75%" }}
          borderRadius="xl"
          overflow="hidden"
          position="relative"
          bg={sliderBg}
          border="1px solid"
          borderColor={cardBorder}
        >
          {/* RGB 通道独立 Gamma SVG 滤镜定义 */}
          {rgbGammaSvgFilter}

          <HStack justify="space-between" px={4} pt={3} pb={1}>
            <Text color={textColor} fontSize="sm" fontWeight="600">
              {t("displayFilter.previewTitle")}
            </Text>
            {!settings.is_active && (
              <Text color="#F1C40F" fontSize="xs" fontWeight="500" bg={hexToRgba("#F1C40F", 0.15)} px={2} py={0.5} borderRadius="full">
                {t("displayFilter.previewDisabledNote")}
              </Text>
            )}
            <HStack spacing={1}>
              <IconButton
                aria-label="Previous preview image"
                icon={<ChevronLeft size={16} />}
                size="sm"
                variant="outline"
                borderColor={primaryColor}
                color={primaryColor}
                bg={hexToRgba(primaryColor, 0.1)}
                isDisabled={previewImageIndex === 0}
                onClick={() => setPreviewImageIndex((i) => (i - 1 + previewImages.length) % previewImages.length)}
                _hover={{ bg: hexToRgba(primaryColor, 0.25), color: primaryColor }}
                _active={{ bg: hexToRgba(primaryColor, 0.35) }}
              />
              <Text color={textColor} fontSize="xs" fontWeight="700">
                {String(previewImageIndex + 1).padStart(2, "0")}/
                {String(previewImages.length).padStart(2, "0")}
              </Text>
              <IconButton
                aria-label="Next preview image"
                icon={<ChevronRight size={16} />}
                size="sm"
                variant="outline"
                borderColor={primaryColor}
                color={primaryColor}
                bg={hexToRgba(primaryColor, 0.1)}
                isDisabled={previewImageIndex === previewImages.length - 1}
                onClick={() => setPreviewImageIndex((i) => (i + 1) % previewImages.length)}
                _hover={{ bg: hexToRgba(primaryColor, 0.25), color: primaryColor }}
                _active={{ bg: hexToRgba(primaryColor, 0.35) }}
              />
            </HStack>
          </HStack>
          <Box
            ref={previewContainerRef}
            position="relative"
            w="full"
            h="380px"
            overflow="hidden"
            cursor="ew-resize"
            onMouseDown={handleSplitMouseDown}
            onTouchStart={handleSplitTouchStart}
          >
            {/* 原始图（全宽） */}
            <Box position="absolute" inset={0}>
              <img
                src={previewImages[previewImageIndex]}
                alt="Original"
                style={{
                  width: "100%",
                  height: "100%",
                  objectFit: "cover",
                  objectPosition: "center",
                }}
                draggable={false}
              />
            </Box>
            
            {/* 滤镜图（从分割线开始向右裁剪） */}
            <Box
              position="absolute"
              inset={0}
              style={{ clipPath: `inset(0 0 0 ${splitPosition}%)` }}
            >
              <img
                src={previewImages[previewImageIndex]}
                alt="Filtered"
                style={{
                  width: "100%",
                  height: "100%",
                  objectFit: "cover",
                  objectPosition: "center",
                  willChange: "filter",
                  ...(activeIccId
                    ? { filter: iccPreviewFilter || "none" } as React.CSSProperties
                    : settings.stacked
                    ? { filter: settings.preview_filter_icc || "none" } as React.CSSProperties
                    : filterStyle),
                }}
                draggable={false}
              />
              {/* 色温/ICC/叠加 覆盖层 */}
              {activeIccId ? (
                iccTintColor ? (
                  <Box
                    position="absolute"
                    inset={0}
                    backgroundColor={iccTintColor}
                    mixBlendMode="overlay"
                    opacity={iccTintOpacity}
                    pointerEvents="none"
                  />
                ) : null
              ) : settings.stacked && settings.preview_tint_color_icc ? (
                <Box
                  position="absolute"
                  inset={0}
                  backgroundColor={settings.preview_tint_color_icc}
                  mixBlendMode="overlay"
                  opacity={settings.preview_tint_opacity_icc ?? 0}
                  pointerEvents="none"
                />
              ) : (
                <Box style={temperatureOverlay} />
              )}
            </Box>
            
            {/* 分割线 */}
            <Box
              position="absolute"
              top={0}
              bottom={0}
              left={`${splitPosition}%`}
              width="3px"
              bg={primaryColor}
              transform="translateX(-50%)"
              boxShadow={`0 0 12px ${hexToRgba(primaryColor, 0.6)}`}
              zIndex={2}
              pointerEvents="none"
            />
            
            {/* 拖拽手柄 */}
            <Box
              position="absolute"
              top="50%"
              left={`${splitPosition}%`}
              transform="translate(-50%, -50%)"
              w="32px"
              h="32px"
              borderRadius="full"
              bg={cardBg}
              border="2px solid"
              borderColor={primaryColor}
              boxShadow={`0 0 16px ${hexToRgba(primaryColor, 0.4)}`}
              zIndex={3}
              display="flex"
              alignItems="center"
              justifyContent="center"
              pointerEvents="none"
            >
              <Box fontSize="10px" color={primaryColor} fontWeight="700" letterSpacing="-1px" userSelect="none">
                ◀▶
              </Box>
            </Box>
            
            {/* 底部标签 */}
            <HStack
              position="absolute"
              bottom={3}
              left={0}
              right={0}
              justify="space-between"
              px={4}
              zIndex={2}
              pointerEvents="none"
            >
              <Text
                color="#ffffff"
                fontSize="xs"
                fontWeight="600"
                bg="rgba(0,0,0,0.5)"
                px={2}
                py={0.5}
                borderRadius="md"
                backdropFilter="blur(4px)"
              >
                {t("displayFilter.previewOriginal")}
              </Text>
              <Text
                color="#ffffff"
                fontSize="xs"
                fontWeight="600"
                bg="rgba(0,0,0,0.5)"
                px={2}
                py={0.5}
                borderRadius="md"
                backdropFilter="blur(4px)"
              >
                {t("displayFilter.previewFiltered")}
              </Text>
            </HStack>
          </Box>
        </Box>

        {/* 右侧 1/4：当前设置 */}
        <VStack
          w={{ base: "full", lg: "25%" }}
          align="stretch"
          spacing={2}
          bg={sliderBg}
          borderRadius="xl"
          border="1px solid"
          borderColor={cardBorder}
          p={3}
        >
          <HStack justify="space-between">
            <HStack spacing={1}>
              <Text color={textColor} fontSize="sm" fontWeight="600">
                {t("displayFilter.currentSettings")}
              </Text>

              {activeIccId && (
                <Text fontSize="9px" fontWeight="700" color={primaryColor} bg={`${primaryColor}20`} px={1} py={0.5} borderRadius="full">
                  ICC
                </Text>
              )}
              {settings.stacked && !activeIccId && (
                <Text fontSize="9px" fontWeight="700" color={primaryColor} bg={hexToRgba(primaryColor, 0.2)} px={1} py={0.5} borderRadius="full">
                  {t("displayFilter.stackedMode")}
                </Text>
              )}
            </HStack>
          </HStack>

          {activeIccId && (
            <Box p={2} borderRadius="md" bg={hexToRgba(primaryColor, 0.08)}>
              <Text color={textColor} fontSize="xs">
                {iccPresets.find(p => p.id === activeIccId)?.description || t("displayFilter.iccApplied")}
              </Text>
            </Box>
          )}

          {settings.stacked && !activeIccId && (
            <Box p={2} borderRadius="md" bg={hexToRgba(primaryColor, 0.08)}>
              <Text color={textColor} fontSize="xs">
                {t("displayFilter.stackedCount")}: {settings.stack_preset_ids.length}
              </Text>
            </Box>
          )}

          <VStack spacing={1} align="stretch" flex={1} justify="center">
              {settings.stacked ? (
                <VStack spacing={2} align="stretch" justify="center" py={2}>
                  <Box p={3} borderRadius="md" bg={hexToRgba(primaryColor, 0.08)}>
                    <Text color={textColor} fontSize="sm" fontWeight="600">
                      {t("displayFilter.stackedMode")}
                    </Text>
                    <Flex justify="space-between" mt={1}>
                      <Text color={subTextColor} fontSize="xs">{t("displayFilter.stackedCount")}</Text>
                      <Text color={primaryColor} fontSize="xs" fontWeight="700">{settings.stack_preset_ids.length}</Text>
                    </Flex>
                  </Box>
                </VStack>
              ) : (
                <>
                  <ReadOnlyItem 
                    label={t("displayFilter.colorTemperature")} 
                    value={settings.temperature} 
                    unit="K"
                    colorValue={settings.temperature}
                  />
                  <ReadOnlyItem 
                    label={t("displayFilter.brightness")} 
                    value={settings.brightness} 
                    unit="%"
                  />
                  <ReadOnlyItem 
                    label={t("displayFilter.contrast")} 
                    value={settings.contrast} 
                    unit="%"
                  />
                  <ReadOnlyItem 
                    label={t("displayFilter.saturation")} 
                    value={settings.saturation} 
                    unit="%"
                  />
                  {/* 模式参数 */}
                  <Box mt={2} pt={2} borderTop="1px solid" borderColor={cardBorder}>
                    <Text color={subTextColor} fontSize="10px" fontWeight="600" mb={1}>
                      {presets.find(p => p.id === activePresetId)?.name || "Mode"}
                    </Text>
                    <Box fontSize="11px" color={subTextColor} lineHeight="1.6">
                      {activeIccId ? (
                        // ICC 预设：显示从真实 ramp 反推的 gamma / S-Curve / RGB Boost
                        <>
                          <Flex justify="space-between"><Text>Gamma</Text><Text color={textColor}>{settings.r_gamma.toFixed(2)}</Text></Flex>
                          <Flex justify="space-between"><Text>S-Curve</Text><Text color={textColor}>{settings.s_curve.toFixed(2)}</Text></Flex>
                          <Flex justify="space-between"><Text>R Boost</Text><Text color={textColor}>{(settings.r_boost * 100).toFixed(0)}%</Text></Flex>
                          <Flex justify="space-between"><Text>G Boost</Text><Text color={textColor}>{(settings.g_boost * 100).toFixed(0)}%</Text></Flex>
                          <Flex justify="space-between"><Text>B Boost</Text><Text color={textColor}>{(settings.b_boost * 100).toFixed(0)}%</Text></Flex>
                        </>
                      ) : (
                        <>
                          <Flex justify="space-between"><Text>Gamma</Text><Text color={textColor}>{currentModeParams.gamma.toFixed(2)}</Text></Flex>
                          <Flex justify="space-between"><Text>S-Curve</Text><Text color={textColor}>{currentModeParams.sCurve.toFixed(2)}</Text></Flex>
                          <Flex justify="space-between"><Text>R Boost</Text><Text color={textColor}>{(currentModeParams.rBoost * 100).toFixed(0)}%</Text></Flex>
                          <Flex justify="space-between"><Text>G Boost</Text><Text color={textColor}>{(currentModeParams.gBoost * 100).toFixed(0)}%</Text></Flex>
                          <Flex justify="space-between"><Text>B Boost</Text><Text color={textColor}>{(currentModeParams.bBoost * 100).toFixed(0)}%</Text></Flex>
                        </>
                      )}
                    </Box>
                  </Box>
                </>
              )}
            </VStack>
        </VStack>
      </Flex>

      <Box 
        w="full" 
        p={4} 
        borderRadius="xl" 
        bg={useColorModeValue(hexToRgba(primaryColor, 0.1), hexToRgba(primaryColor, 0.1))}
        border="1px solid"
        borderColor={useColorModeValue(hexToRgba(primaryColor, 0.3), hexToRgba(primaryColor, 0.2))}
      >
        <Text color={subTextColor} fontSize="xs">
          {t("displayFilter.tip")}
        </Text>
      </Box>
    </VStack>
  );

  return (
    <Box pt={8}>
      {/* 已支持的游戏名单 */}
      <Modal isOpen={isGameFilterListOpen} onClose={() => setIsGameFilterListOpen(false)} scrollBehavior="inside">
        <ModalOverlay />
        <ModalContent bg={cardBg}>
          <ModalHeader color={textColor}>{t("displayFilter.gameFilterSupportedTitle")}</ModalHeader>
          <ModalCloseButton color={subTextColor} />
          <ModalBody pb={4}>
            <SimpleGrid columns={{ base: 2, sm: 3 }} spacing={2}>
              {gameFilterGames.map((game) => (
                <Tooltip key={game.id} label={game.name}>
                  <Text
                    color={textColor}
                    fontSize="sm"
                    fontWeight="500"
                    noOfLines={1}
                  >
                    {game.name}
                  </Text>
                </Tooltip>
              ))}
            </SimpleGrid>
          </ModalBody>
        </ModalContent>
      </Modal>

      {/* Save Preset Dialog */}
      <Modal isOpen={showSavePresetDialog} onClose={() => setShowSavePresetDialog(false)}>
        <ModalOverlay />
        <ModalContent bg={cardBg}>
          <ModalHeader color={textColor}>{t("displayFilter.saveAsPreset")}</ModalHeader>
          <ModalCloseButton color={subTextColor} />
          <ModalBody>
            <Input
              placeholder={t("displayFilter.presetNamePlaceholder")}
              value={presetNameInput}
              onChange={(e) => setPresetNameInput(e.target.value)}
              bg={inputBg}
              color={textColor}
              borderColor={cardBorder}
              autoFocus
              onKeyDown={(e) => {
                if (e.key === "Enter") saveCurrentAsPreset();
              }}
            />
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={() => setShowSavePresetDialog(false)}>
              {t("displayFilter.cancel")}
            </Button>
            <Button
              bg={primaryColor}
              color={contrastText}
              onClick={saveCurrentAsPreset}
              isDisabled={!presetNameInput.trim()}
              _hover={{ bg: getHoverColor() }}
            >
              {t("displayFilter.save")}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* Delete User Preset Confirmation Dialog */}
      <AlertDialog
        isOpen={deleteUserPresetId !== null}
        leastDestructiveRef={cancelDeleteRef}
        onClose={() => setDeleteUserPresetId(null)}
      >
        <AlertDialogOverlay>
          <AlertDialogContent>
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              {t("displayFilter.deletePreset")}
            </AlertDialogHeader>
            <AlertDialogBody>
              {t("displayFilter.deletePresetConfirm")}
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelDeleteRef} onClick={() => setDeleteUserPresetId(null)}>
                {t("displayFilter.cancel")}
              </Button>
              <Button colorScheme="red" onClick={handleDeleteUserPreset} ml={3} isLoading={isLoading}>
                {t("displayFilter.delete")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>

      {/* Delete ICC Confirmation Dialog */}
      <AlertDialog
        isOpen={deleteIccId !== null}
        leastDestructiveRef={cancelDeleteRef}
        onClose={() => setDeleteIccId(null)}
      >
        <AlertDialogOverlay>
          <AlertDialogContent>
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              {t("displayFilter.deleteIcc")}
            </AlertDialogHeader>
            <AlertDialogBody>
              {t("displayFilter.deleteIccConfirm")}
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelDeleteRef} onClick={() => setDeleteIccId(null)}>
                {t("displayFilter.cancel")}
              </Button>
              <Button colorScheme="red" onClick={handleDeleteIcc} ml={3} isLoading={isLoading}>
                {t("displayFilter.delete")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
      {liquidGlassEnabled ? (
        <LiquidGlassCard
          w="full"
          boxShadow="2xl"
          overflow="hidden"
          position="relative"
          p={6}
        >
          {content}
        </LiquidGlassCard>
      ) : (
        <Card
          bg={cardBg}
          borderColor={cardBorder}
          borderWidth="1px"
          w="full"
          boxShadow="2xl"
          overflow="hidden"
          position="relative"
        >
          <CardBody p={6}>
            {content}
          </CardBody>
        </Card>
      )}
    </Box>
  );
}
