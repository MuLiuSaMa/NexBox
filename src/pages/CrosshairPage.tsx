import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Switch,
  SimpleGrid,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Input,
  useColorModeValue,
  useColorMode,
  Badge,
  Icon,
  IconButton,
  Button,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  Portal,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { store } from "@/lib/store";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff, ArrowLeft, RotateCcw, Monitor, ChevronDown, Check, Image, Plus, Minus } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useAppStartup } from "@/contexts/app-startup-context";
import { useNavigate } from "react-router-dom";
import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { HoldKeyRecorder } from "@/components/hold-key-recorder";
import { CustomColorPicker } from "@/components/special/custom-color-picker";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { hexToRgba } from "@/lib/color-utils";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface CrosshairSettings {
  enabled: boolean;
  style: string;
  size: number;
  thickness: number;
  color: string;
  gap: number;
  dot_size: number;
  opacity: number;
  monitor_index: number;
  monitor_device_name: string | null;
  use_custom_image: boolean;
  custom_image_path: string | null;
  offset_x: number;
  offset_y: number;
  screen_width?: number;
  screen_height?: number;
  outline_enabled: boolean;
  outline_color: string;
  outline_thickness: number;
}

interface DisplayInfo {
  index: number;
  name: string;
  device_name: string;
  is_primary: boolean;
  width: number;
  height: number;
}

const PRESET_IMAGE_STYLES = [
  { id: "Preset_cat", file: "cat.png", labelKey: "crosshair.presetImagesNames.cat" },
  { id: "Preset_donk", file: "donk.png", labelKey: "crosshair.presetImagesNames.donk" },
  { id: "Preset_s1mple", file: "s1mple.png", labelKey: "crosshair.presetImagesNames.s1mple" },
  { id: "Preset_ropz", file: "ropz.png", labelKey: "crosshair.presetImagesNames.ropz" },
  { id: "Preset_MMW2.0", file: "MMW2.0.png", labelKey: "crosshair.presetImagesNames.mmw20" },
  { id: "Preset_SSCJ", file: "SSCJ.png", labelKey: "crosshair.presetImagesNames.sscj" },

  { id: "Preset_T字准星", file: "T字准星.png", labelKey: "crosshair.presetImagesNames.tShape" },
];

const CROSSHAIR_STORE_KEY = "crosshair-settings";

const DEFAULT_SETTINGS: CrosshairSettings = {
  enabled: false,
  style: "Cross",
  size: 20,
  thickness: 2,
  color: "#ff0000",
  gap: 0,
  dot_size: 2,
  opacity: 255,
  monitor_index: -1,
  monitor_device_name: null,
  use_custom_image: false,
  custom_image_path: null,
  offset_x: 0,
  offset_y: 0,
  screen_width: 0,
  screen_height: 0,
  outline_enabled: false,
  outline_color: "#000000",
  outline_thickness: 1,
};

const STYLE_OPTIONS = [
  { id: "Cross", labelKey: "crosshair.styles.cross", icon: "+" },
  { id: "Dot", labelKey: "crosshair.styles.dot", icon: "\u25CF" },
  { id: "Circle", labelKey: "crosshair.styles.circle", icon: "\u25CB" },
  { id: "CrossDot", labelKey: "crosshair.styles.crossDot", icon: "\u271A" },
  { id: "CircleCross", labelKey: "crosshair.styles.circleCross", icon: "\u2295" },
  { id: "DotBox", labelKey: "crosshair.styles.dotBox", icon: "\u25A3" },
];

const COLOR_PRESETS = [
  { value: "#ff0000" },
  { value: "#00ff00" },
  { value: "#0000ff" },
  { value: "#00ffff" },
  { value: "#ff00ff" },
  { value: "#ffff00" },
  { value: "#ffffff" },
  { value: "#ff8800" },
  { value: "#ff0088" },
];

function SettingCard({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  const { liquidGlassEnabled } = useBackground();
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { colorMode } = useColorMode();
  const headerColor = colorMode === 'light' ? '#000000' : '#ffffff';

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={5}>
        <VStack align="stretch" spacing={4}>
          <Text fontWeight="medium" color={headerColor}>{title}</Text>
          {children}
        </VStack>
      </LiquidGlassCard>
    );
  }

  return (
    <Box bg={cardBg} borderRadius="xl" p={5} border="1px solid" borderColor={borderColor}>
      <VStack align="stretch" spacing={4}>
        <Text fontWeight="medium" color={headerColor}>{title}</Text>
        {children}
      </VStack>
    </Box>
  );
}

export default function CrosshairPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("target");
  const navigate = useNavigate();
  const { crosshairHotkey, saveCrosshairHotkey } = useAppStartup();
  const { getActiveColor, getHoverColor, getBorderColor, getContrastTextColor } = useThemeColor();

  const [settings, setSettings] = useState<CrosshairSettings>(DEFAULT_SETTINGS);
  const [isLoading, setIsLoading] = useState(false);
  const [autoApplyOnStartup, setAutoApplyOnStartup] = useState(false);
  const [holdEnabled, setHoldEnabled] = useState(false);
  const [holdKey, setHoldKey] = useState("MouseRight");
  const [holdDelay, setHoldDelay] = useState(0);
  const [holdDelayInput, setHoldDelayInput] = useState("0");

  useEffect(() => {
    (async () => {
      let v = await store.get<boolean>("nexbox_auto_crosshair");
      if (v !== null && v !== undefined) {
        setAutoApplyOnStartup(v);
      } else {
        setAutoApplyOnStartup(localStorage.getItem("nexbox_auto_crosshair") === "true");
      }
    })();
  }, []);
  const [displays, setDisplays] = useState<DisplayInfo[]>([]);
  const [editingAxis, setEditingAxis] = useState<'x' | 'y' | null>(null);
  const [editValue, setEditValue] = useState('');
  const editRef = useRef<HTMLInputElement>(null);
  const lastProceduralStyle = useRef<string>("Cross");

  const headingColor = useColorModeValue("black", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const sliderBg = useColorModeValue("gray.200", "gray.600");
  const hoverBg = useColorModeValue("gray.100", "#252525");
  const menuListBg = useColorModeValue("white", "#1a1a1a");
  const inputBg = useColorModeValue("white", "#1a1a1a");

  // 根据显示器分辨率自动适配偏移范围，最小 ±500 保证小屏幕也有足够空间
  const maxOffsetX = Math.max(500, Math.ceil((settings.screen_width || 1920) / 2));
  const maxOffsetY = Math.max(500, Math.ceil((settings.screen_height || 1080) / 2));

  useEffect(() => {
    loadSettings();
    loadHoldSettings();
    // 延迟加载显示器列表，避免进入页面时阻塞渲染导致卡顿
    const timer = setTimeout(() => loadDisplays(), 200);
    return () => clearTimeout(timer);
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<void>("crosshair-status-changed", () => {
      loadSettings();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const loadSettings = async () => {
    try {
      const status = await invoke<CrosshairSettings>("get_crosshair_status");
      const isDefault = JSON.stringify(status) === JSON.stringify({ ...DEFAULT_SETTINGS, enabled: status.enabled });
      if (isDefault) {
        const saved = await store.get<CrosshairSettings>(CROSSHAIR_STORE_KEY);
        if (saved) {
          const merged = { ...DEFAULT_SETTINGS, ...saved };
          merged.enabled = status.enabled;
          setSettings(merged);
          await invoke("update_crosshair_settings", { settings: merged });
          return;
        }
      }
      setSettings(status);
    } catch (error) {
      console.error("Failed to load crosshair settings:", error);
    }
  };

  const loadDisplays = async () => {
    try {
      const list = await invoke<DisplayInfo[]>("get_crosshair_displays");
      setDisplays(list);
    } catch (error) {
      console.error("Failed to load displays:", error);
    }
  };

  const loadHoldSettings = async () => {
    try {
      const enabled = await invoke<boolean>("get_crosshair_hold_enabled");
      setHoldEnabled(enabled);
      const key = await invoke<string>("get_crosshair_hold_key");
      if (key) setHoldKey(key);
      const delay = await invoke<number>("get_crosshair_hold_delay");
      setHoldDelay(delay || 0);
      setHoldDelayInput(String((delay || 0) / 1000));
    } catch (error) {
      console.error("Failed to load crosshair hold settings:", error);
    }
  };

  const toggleHoldMode = async (enabled: boolean) => {
    setHoldEnabled(enabled);
    try {
      await invoke("set_crosshair_hold_enabled", { enabled });
      toast({
        title: enabled
          ? t("crosshair.holdModeOn")
          : t("crosshair.holdModeOff") || "按住模式已关闭",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to set crosshair hold mode:", error);
      setHoldEnabled(!enabled);
      toast({
        title: t("crosshair.updateFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  // 保存按住显示延迟（输入单位为秒，内部以毫秒落盘），失焦/回车时提交
  const commitHoldDelay = async () => {
    const seconds = Number(holdDelayInput) || 0;
    const parsed = Math.max(0, Math.min(10, seconds));
    const delayMs = Math.round(parsed * 1000);
    setHoldDelayInput(String(delayMs / 1000));
    if (delayMs === holdDelay) return;
    setHoldDelay(delayMs);
    try {
      await invoke("set_crosshair_hold_delay", { delayMs });
      toast({
        title: t("crosshair.holdDelaySaved") || "延迟已保存",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to save hold delay:", error);
      toast({
        title: t("crosshair.updateFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  const saveHoldKey = async (val: string) => {
    setHoldKey(val);
    try {
      await invoke("set_crosshair_hold_key", { key: val });
      toast({
        title: t("crosshair.hotkeySaved") || "快捷键已保存",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to save hold key:", error);
      toast({
        title: t("crosshair.updateFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  const selectImage = async () => {
    try {
      const path = await invoke<string | null>("pick_crosshair_image");
      if (path) {
        updateSetting("custom_image_path", path);
      }
    } catch (error) {
      console.error("Failed to pick image:", error);
    }
  };

  const getPresetCrosshairPath = async (filename: string): Promise<string> => {
    try {
      return await invoke<string>("get_preset_crosshair_path", { filename });
    } catch (error) {
      console.error("Failed to get preset path:", error);
      return "";
    }
  };

  const selectPresetImage = async (preset: { id: string; file: string; labelKey: string }) => {
    const path = await getPresetCrosshairPath(preset.file);
    if (path) {
      setSettings(prev => {
        const newSettings = {
          ...prev,
          style: preset.id,
          use_custom_image: true,
          custom_image_path: path,
        };
        updateSettings(newSettings);
        return newSettings;
      });
    }
  };

  const resetToDefault = () => {
    const defaults: CrosshairSettings = {
      ...DEFAULT_SETTINGS,
      enabled: settings.enabled,
    };
    updateSettings(defaults);
  };

  const updateSettings = async (newSettings: CrosshairSettings) => {
    setSettings(newSettings);
    setIsLoading(true);
    try {
      await invoke("update_crosshair_settings", { settings: newSettings });
      await store.set(CROSSHAIR_STORE_KEY, newSettings);
      await store.save();
    } catch (error) {
      console.error("Failed to update settings:", error);
      toast({
        title: t("crosshair.updateFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  };

  const toggleCrosshair = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("toggle_crosshair");
      if (result.success) {
        setSettings(prev => ({ ...prev, enabled: !prev.enabled }));
        toast({
          title: result.message,
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to toggle crosshair:", error);
      toast({
        title: t("crosshair.toggleFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  };

  const updateSetting = <K extends keyof CrosshairSettings>(
    key: K,
    value: CrosshairSettings[K]
  ) => {
    const newSettings = { ...settings, [key]: value };
    updateSettings(newSettings);
  };

  const startEdit = (axis: 'x' | 'y') => {
    setEditValue(String(axis === 'x' ? settings.offset_x : settings.offset_y));
    setEditingAxis(axis);
    requestAnimationFrame(() => editRef.current?.focus());
  };

  const commitEdit = useCallback(() => {
    if (editingAxis === null) return;
    const raw = parseInt(editValue, 10);
    const key = editingAxis === 'x' ? 'offset_x' : 'offset_y';
    const max = editingAxis === 'x' ? maxOffsetX : maxOffsetY;
    if (!isNaN(raw)) {
      updateSetting(key as 'offset_x' | 'offset_y', Math.max(-max, Math.min(max, raw)));
    }
    setEditingAxis(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editingAxis, editValue, maxOffsetX, maxOffsetY]);

  return (
    <Box pt={8} pb={8}>
      <HStack justify="space-between" mb={6}>
        <HStack>
          <IconButton
            aria-label={t("builtinTools.back")}
            icon={<ArrowLeft size={20} />}
            variant="ghost"
            onClick={() => navigate("/builtin-tools")}
            color={headingColor}
          />
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            {t("crosshair.title")}
          </Heading>
        </HStack>
      </HStack>

      <SimpleGrid columns={2} spacing={5}>
        <VStack align="stretch" spacing={5}>
          <SettingCard title={t("crosshair.enableCrosshair")}>
            <VStack align="stretch" spacing={3}>
              <HStack justify="space-between" wrap="wrap" spacing={4}>
                <HStack>
                  <Icon as={settings.enabled ? Eye : EyeOff} boxSize={5} color={settings.enabled ? "green.400" : "gray.400"} />
                  <Badge colorScheme={settings.enabled ? "green" : "gray"}>
                    {settings.enabled ? t("crosshair.statusEnabled") : t("crosshair.statusDisabled")}
                  </Badge>
                </HStack>
                <HStack spacing={4}>
                  <HotkeyRecorder
                    value={crosshairHotkey}
                    onChange={async (val) => {
                      const err = await saveCrosshairHotkey(val);
                      toast({
                        title: err
                          ? err
                          : (t("crosshair.hotkeySaved") || "快捷键已保存"),
                        status: err ? "error" : "success",
                        duration: 2000,
                        isClosable: true,
                      });
                    }}
                  />
                  <Switch
                    isChecked={settings.enabled}
                    onChange={toggleCrosshair}
                    isDisabled={isLoading}
                    size="lg"
                    sx={{
                      '& .chakra-switch__track[data-checked]': {
                        bg: getActiveColor(),
                      },
                    }}
                  />
                </HStack>
              </HStack>
              <HStack
                bg={autoApplyOnStartup ? hexToRgba(getActiveColor(), 0.15) : sliderBg}
                px={4}
                py={2}
                borderRadius="xl"
                border="1px solid"
                borderColor={autoApplyOnStartup ? getActiveColor() : "transparent"}
                w="fit-content"
                alignSelf="flex-end"
              >
                <Text color={textColor} fontSize="xs" fontWeight="500">
                  启动新境盒时自动启用选中准星
                </Text>
                <ThemeSwitch
                  isChecked={autoApplyOnStartup}
                  onChange={(e) => {
                    const val = e.target.checked;
                    setAutoApplyOnStartup(val);
                    localStorage.setItem("nexbox_auto_crosshair", val ? "true" : "false");
                    store.set("nexbox_auto_crosshair", val).then(() => store.save());
                  }}
                  isDisabled={isLoading}
                />
              </HStack>
              <HStack
                px={4}
                py={2}
                borderRadius="xl"
                justify="space-between"
                flexWrap="wrap"
                spacing={3}
              >
                <HStack spacing={2}>
                  <Text color={textColor} fontSize="sm" fontWeight="500" whiteSpace="nowrap">
                    {t("crosshair.holdMode")}
                  </Text>
                  <ThemeSwitch
                    isChecked={holdEnabled}
                    onChange={(e) => toggleHoldMode(e.target.checked)}
                    isDisabled={isLoading}
                  />
                </HStack>
                <HStack
                  spacing={2}
                  opacity={holdEnabled ? undefined : 0.5}
                  pointerEvents={holdEnabled ? undefined : "none"}
                  userSelect={holdEnabled ? undefined : "none"}
                >
                  <Text color={subTextColor} fontSize="xs" whiteSpace="nowrap">
                    {t("crosshair.holdKey")}
                  </Text>
                  <HoldKeyRecorder value={holdKey} onChange={saveHoldKey} />
                  <Text color={subTextColor} fontSize="xs" whiteSpace="nowrap" ml={2}>
                    {t("crosshair.holdDelay")}
                  </Text>
                  <Input
                    value={holdDelayInput}
                    onChange={(e) => {
                      // 允许输入小数秒，去掉非法字符并只保留一个小数点
                      const cleaned = e.target.value.replace(/[^\d.]/g, "");
                      const firstDot = cleaned.indexOf(".");
                      const normalized =
                        firstDot === -1
                          ? cleaned
                          : cleaned.slice(0, firstDot + 1) + cleaned.slice(firstDot + 1).replace(/\./g, "");
                      setHoldDelayInput(normalized);
                    }}
                    onBlur={commitHoldDelay}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") (e.target as HTMLInputElement).blur();
                    }}
                    w="72px"
                    h="30px"
                    size="sm"
                    textAlign="center"
                    bg={inputBg}
                    color={textColor}
                    borderColor={getActiveColor()}
                    _focus={{ borderColor: getActiveColor(), boxShadow: `0 0 0 1px ${getActiveColor()}` }}
                    placeholder="0"
                    isDisabled={!holdEnabled || isLoading}
                    title={t("crosshair.holdDelayDesc") || undefined}
                  />
                  <Text color={subTextColor} fontSize="xs" whiteSpace="nowrap">
                    s
                  </Text>
                </HStack>
              </HStack>
            </VStack>
          </SettingCard>

          <SettingCard title={t("crosshair.renderMode")}>
            <HStack spacing={2}>
              <LiquidGlassCard
                py={2.5}
                px={3}
                textAlign="center"
                cursor="pointer"
                flex={1}
                onClick={() => {
                  if (!settings.use_custom_image) return;
                  updateSettings({ ...settings, use_custom_image: false, style: lastProceduralStyle.current, custom_image_path: null });
                }}
                opacity={!settings.use_custom_image ? 1 : 0.5}
                border={!settings.use_custom_image ? `1px solid ${getActiveColor()}` : "1px solid transparent"}
              >
                <Text fontSize="xs" fontWeight="medium" color={!settings.use_custom_image ? getActiveColor() : textColor}>
                  {t("crosshair.procedural")}
                </Text>
              </LiquidGlassCard>
              <LiquidGlassCard
                py={2.5}
                px={3}
                textAlign="center"
                cursor="pointer"
                flex={1}
                onClick={() => {
                  if (settings.use_custom_image && settings.style.startsWith("Preset_")) return;
                  if (!settings.use_custom_image) {
                    lastProceduralStyle.current = settings.style;
                  }
                  selectPresetImage(PRESET_IMAGE_STYLES[0]);
                }}
                opacity={settings.use_custom_image && settings.style.startsWith("Preset_") ? 1 : 0.5}
                border={settings.use_custom_image && settings.style.startsWith("Preset_") ? `1px solid ${getActiveColor()}` : "1px solid transparent"}
              >
                <Text fontSize="xs" fontWeight="medium" color={settings.use_custom_image && settings.style.startsWith("Preset_") ? getActiveColor() : textColor}>
                  {t("crosshair.presetImages")}
                </Text>
              </LiquidGlassCard>
              <LiquidGlassCard
                py={2.5}
                px={3}
                textAlign="center"
                cursor="pointer"
                flex={1}
                onClick={() => {
                  if (settings.use_custom_image && !settings.style.startsWith("Preset_")) return;
                  if (!settings.use_custom_image) {
                    lastProceduralStyle.current = settings.style;
                  }
                  updateSettings({ ...settings, use_custom_image: true, style: "Custom", custom_image_path: null });
                }}
                opacity={settings.use_custom_image && !settings.style.startsWith("Preset_") ? 1 : 0.5}
                border={settings.use_custom_image && !settings.style.startsWith("Preset_") ? `1px solid ${getActiveColor()}` : "1px solid transparent"}
              >
                <Text fontSize="xs" fontWeight="medium" color={settings.use_custom_image && !settings.style.startsWith("Preset_") ? getActiveColor() : textColor}>
                  {t("crosshair.customImage")}
                </Text>
              </LiquidGlassCard>
            </HStack>
          </SettingCard>

          <SettingCard title={t("crosshair.monitor")}>
            <Menu matchWidth>
              <MenuButton
                as={Box}
                bg="transparent"
                p={0}
                border="none"
                w="full"
                cursor="pointer"
              >
                <LiquidGlassCard px={3} py={1.5}>
                  <HStack justify="space-between">
                    <HStack spacing={2}>
                      <Monitor size={14} />
                      <Text fontSize="sm" color={textColor}>
                        {settings.monitor_index === -1
                          ? t("crosshair.primaryMonitor")
                          : displays.find(d => d.index === settings.monitor_index)?.name || t("crosshair.primaryMonitor")}
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
                      const newSettings = { ...settings, monitor_index: -1, monitor_device_name: null };
                      updateSettings(newSettings);
                    }}
                    bg={settings.monitor_index === -1 ? hoverBg : "transparent"}
                    _hover={{ bg: hoverBg }}
                  >
                    <HStack spacing={2} w="full" justify="space-between">
                      <Text fontSize="sm">{t("crosshair.primaryMonitor")}</Text>
                      {settings.monitor_index === -1 && <Check size={14} color={getActiveColor()} />}
                    </HStack>
                  </MenuItem>
                  {displays.map((d) => (
                    <MenuItem
                      key={d.index}
                      onClick={() => {
                        const newSettings = { ...settings, monitor_index: d.index, monitor_device_name: d.device_name };
                        updateSettings(newSettings);
                      }}
                      bg={settings.monitor_index === d.index ? hoverBg : "transparent"}
                      _hover={{ bg: hoverBg }}
                    >
                      <HStack spacing={2} w="full" justify="space-between">
                        <Text fontSize="sm">{d.name}</Text>
                        {settings.monitor_index === d.index && <Check size={14} color={getActiveColor()} />}
                      </HStack>
                    </MenuItem>
                  ))}
                </MenuList>
              </Portal>
            </Menu>
          </SettingCard>

          {!settings.use_custom_image && (
            <SettingCard title={t("crosshair.style")}>
              <SimpleGrid columns={5} spacing={2}>
                {STYLE_OPTIONS.map((option) => {
                  const isActive = settings.style === option.id;
                  return (
                    <LiquidGlassCard
                      key={option.id}
                      py={3}
                      textAlign="center"
                      cursor="pointer"
                      onClick={() => updateSetting("style", option.id)}
                    >
                      <Text fontSize="xl" mb={0.5} color={isActive ? getActiveColor() : textColor}>{option.icon}</Text>
                      <Text fontSize="xs" fontWeight="medium" color={isActive ? getActiveColor() : textColor}>
                        {t(option.labelKey)}
                      </Text>
                    </LiquidGlassCard>
                  );
                })}
              </SimpleGrid>
            </SettingCard>
          )}

          {!settings.use_custom_image && (
            <SettingCard title={t("crosshair.color")}>
            <VStack align="stretch" spacing={3}>
              <HStack flexWrap="wrap" gap={2}>
                {COLOR_PRESETS.map((color) => (
                  <Box
                    key={color.value}
                    w={8}
                    h={8}
                    bg={color.value}
                    borderRadius="md"
                    cursor="pointer"
                    border="2px solid"
                    borderColor={settings.color === color.value ? getActiveColor() : "transparent"}
                    onClick={() => updateSetting("color", color.value)}
                    _hover={{ transform: "scale(1.15)" }}
                    transition="all 0.15s"
                    boxShadow={settings.color === color.value ? `0 0 8px ${color.value}` : "none"}
                  />
                ))}
                <CustomColorPicker color={settings.color} onChange={(c) => updateSetting("color", c)} />
              </HStack>

              {/* 描边设置 */}
              <Box>
                <HStack
                  justify="space-between"
                  align="center"
                  bg={settings.outline_enabled ? hexToRgba(getActiveColor(), 0.08) : "transparent"}
                  px={3}
                  py={2}
                  borderRadius="lg"
                  border="1px solid"
                  borderColor={settings.outline_enabled ? getActiveColor() : cardBorder}
                >
                  <Text color={textColor} fontSize="sm" fontWeight="medium">{t("crosshair.outline")}</Text>
                  <ThemeSwitch
                    isChecked={settings.outline_enabled}
                    onChange={(e) => updateSetting("outline_enabled", e.target.checked)}
                    isDisabled={isLoading}
                  />
                </HStack>
                {settings.outline_enabled && (
                  <VStack align="stretch" spacing={3} mt={3}>
                    <Box>
                      <Text color={subTextColor} fontSize="xs" mb={2}>{t("crosshair.outlineColor")}</Text>
                      <HStack flexWrap="wrap" gap={2}>
                        {COLOR_PRESETS.map((color) => (
                          <Box
                            key={color.value}
                            w={7}
                            h={7}
                            bg={color.value}
                            borderRadius="md"
                            cursor="pointer"
                            border="2px solid"
                            borderColor={settings.outline_color === color.value ? getActiveColor() : "transparent"}
                            onClick={() => updateSetting("outline_color", color.value)}
                            _hover={{ transform: "scale(1.15)" }}
                            transition="all 0.15s"
                            boxShadow={settings.outline_color === color.value ? `0 0 6px ${color.value}` : "none"}
                          />
                        ))}
                        <CustomColorPicker color={settings.outline_color} onChange={(c) => updateSetting("outline_color", c)} compact />
                      </HStack>
                    </Box>
                    <Box>
                      <HStack justify="space-between" mb={1}>
                        <Text color={textColor} fontSize="sm">{t("crosshair.outlineThickness")}</Text>
                        <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.outline_thickness}</Text>
                      </HStack>
                      <Slider value={settings.outline_thickness} min={1} max={5} step={1} onChange={(val) => updateSetting("outline_thickness", val)}>
                        <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                        <SliderThumb />
                      </Slider>
                    </Box>
                  </VStack>
                )}
              </Box>
            </VStack>
          </SettingCard>
          )}

          {settings.use_custom_image && settings.style.startsWith("Preset_") && (
            <SettingCard title={t("crosshair.presetImagesTitle")}>
              <SimpleGrid columns={5} spacing={2}>
                {PRESET_IMAGE_STYLES.map((preset) => {
                  const isActive = settings.style === preset.id;
                  return (
                    <LiquidGlassCard
                      key={preset.id}
                      py={2}
                      textAlign="center"
                      cursor="pointer"
                      onClick={() => selectPresetImage(preset)}
                    >
                      <Box
                        w="full"
                        h="36px"
                        mb={1}
                        borderRadius="md"
                        overflow="hidden"
                        bg="black"
                      >
                        <img
                          src={`/crosshair-presets/${preset.file}`}
                          alt={t(preset.labelKey)}
                          style={{ width: '100%', height: '100%', objectFit: 'contain' }}
                        />
                      </Box>
                      <Text fontSize="xs" fontWeight="medium" color={isActive ? getActiveColor() : textColor}>
                        {t(preset.labelKey)}
                      </Text>
                    </LiquidGlassCard>
                  );
                })}
              </SimpleGrid>
            </SettingCard>
          )}
        </VStack>

        <VStack align="stretch" spacing={5}>
          <SettingCard title={t("crosshair.parameters")}>
            <VStack align="stretch" spacing={4}>
              {settings.use_custom_image ? (
                <>
                  {settings.style.startsWith("Preset_") ? (
                    <Box>
                      <HStack spacing={2} mb={1}>
                        <Box
                          w={10} h={10}
                          borderRadius="md"
                          overflow="hidden"
                          bg="black"
                          flexShrink={0}
                        >
                          <img
                            src={`/crosshair-presets/${PRESET_IMAGE_STYLES.find(p => p.id === settings.style)?.file || ""}`}
                            alt=""
                            style={{ width: '100%', height: '100%', objectFit: 'contain' }}
                          />
                        </Box>
                        <VStack align="flex-start" spacing={0}>
                          <Text fontSize="sm" fontWeight="medium" color={textColor}>
                            {(() => {
                              const found = PRESET_IMAGE_STYLES.find(p => p.id === settings.style);
                              return found ? t(found.labelKey) : t("crosshair.presetImages");
                            })()}
                          </Text>
                          <Text fontSize="2xs" color={subTextColor}>{t("crosshair.presetHint")}</Text>
                        </VStack>
                      </HStack>
                    </Box>
                  ) : (
                    <Box>
                      <Button
                        leftIcon={<Image size={16} />}
                        w="full"
                        variant="outline"
                        colorScheme="gray"
                        size="sm"
                        onClick={selectImage}
                        justifyContent="flex-start"
                        h="auto"
                        py={2.5}
                        whiteSpace="normal"
                        textAlign="left"
                      >
                        <VStack align="stretch" spacing={0.5}>
                          <Text fontSize="sm" color={textColor}>
                            {settings.custom_image_path
                              ? settings.custom_image_path.split(/[\\/]/).pop()
                              : t("crosshair.selectImage")}
                          </Text>
                          {settings.custom_image_path && (
                            <Text fontSize="2xs" color={subTextColor} noOfLines={1}>
                              {settings.custom_image_path}
                            </Text>
                          )}
                        </VStack>
                      </Button>
                    </Box>
                  )}

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.size")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.size}</Text>
                    </HStack>
                    <Slider value={settings.size} min={1} max={200} step={1} onChange={(val) => updateSetting("size", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.opacity")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">
                        {Math.round(settings.opacity / 255 * 100)}%
                      </Text>
                    </HStack>
                    <Slider value={settings.opacity} min={0} max={255} step={5} onChange={(val) => updateSetting("opacity", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">X {t("crosshair.offset")}</Text>
                      <HStack spacing={1}>
                        <IconButton
                          aria-label="X-"
                          icon={<Minus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_x", settings.offset_x - 1)}
                          isDisabled={settings.offset_x <= -maxOffsetX}
                        />
                        {editingAxis === 'x' ? (
                          <Input
                            ref={editRef}
                            value={editValue}
                            size="xs"
                            w="48px"
                            textAlign="center"
                            fontSize="xs"
                            bg={inputBg}
                            color={textColor}
                            borderColor={getActiveColor()}
                            _focus={{ borderColor: getActiveColor(), boxShadow: `0 0 0 1px ${getActiveColor()}` }}
                            px={2.5}
                            py={1}
                            borderRadius="md"
                            onChange={(e) => setEditValue(e.target.value)}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') commitEdit();
                              if (e.key === 'Escape') setEditingAxis(null);
                            }}
                          />
                        ) : (
                          <Box
                            onClick={() => startEdit('x')}
                            px={2.5}
                            py={1}
                            minW="48px"
                            textAlign="center"
                            bg={inputBg}
                            borderRadius="md"
                            border="1px solid"
                            borderColor={cardBorder}
                            fontSize="xs"
                            color={textColor}
                            fontWeight="medium"
                            fontFamily="mono"
                            cursor="text"
                            _hover={{ borderColor: getActiveColor() }}
                            transition="border-color 0.15s"
                          >
                            {settings.offset_x}
                          </Box>
                        )}
                        <IconButton
                          aria-label="X+"
                          icon={<Plus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_x", settings.offset_x + 1)}
                          isDisabled={settings.offset_x >= maxOffsetX}
                        />
                      </HStack>
                    </HStack>
                    <Slider value={settings.offset_x} min={-maxOffsetX} max={maxOffsetX} step={1} onChange={(val) => updateSetting("offset_x", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">Y {t("crosshair.offset")}</Text>
                      <HStack spacing={1}>
                        <IconButton
                          aria-label="Y-"
                          icon={<Minus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_y", settings.offset_y - 1)}
                          isDisabled={settings.offset_y <= -maxOffsetY}
                        />
                        {editingAxis === 'y' ? (
                          <Input
                            ref={editRef}
                            value={editValue}
                            size="xs"
                            w="48px"
                            textAlign="center"
                            fontSize="xs"
                            bg={inputBg}
                            color={textColor}
                            borderColor={getActiveColor()}
                            _focus={{ borderColor: getActiveColor(), boxShadow: `0 0 0 1px ${getActiveColor()}` }}
                            px={2.5}
                            py={1}
                            borderRadius="md"
                            onChange={(e) => setEditValue(e.target.value)}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') commitEdit();
                              if (e.key === 'Escape') setEditingAxis(null);
                            }}
                          />
                        ) : (
                          <Box
                            onClick={() => startEdit('y')}
                            px={2.5}
                            py={1}
                            minW="48px"
                            textAlign="center"
                            bg={inputBg}
                            borderRadius="md"
                            border="1px solid"
                            borderColor={cardBorder}
                            fontSize="xs"
                            color={textColor}
                            fontWeight="medium"
                            fontFamily="mono"
                            cursor="text"
                            _hover={{ borderColor: getActiveColor() }}
                            transition="border-color 0.15s"
                          >
                            {settings.offset_y}
                          </Box>
                        )}
                        <IconButton
                          aria-label="Y+"
                          icon={<Plus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_y", settings.offset_y + 1)}
                          isDisabled={settings.offset_y >= maxOffsetY}
                        />
                      </HStack>
                    </HStack>
                    <Slider value={settings.offset_y} min={-maxOffsetY} max={maxOffsetY} step={1} onChange={(val) => updateSetting("offset_y", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <HStack justify="space-between" pt={1}>
                    <HStack spacing={2}>
                      <Box
                        w={10} h={10}
                        borderRadius="md"
                        bg="black"
                        display="flex"
                        alignItems="center"
                        justifyContent="center"
                        opacity={settings.opacity / 255}
                        overflow="hidden"
                      >
                        {settings.style.startsWith("Preset_") ? (
                          <img
                            src={`/crosshair-presets/${PRESET_IMAGE_STYLES.find(p => p.id === settings.style)?.file || ""}`}
                            alt=""
                            style={{ maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' }}
                          />
                        ) : settings.custom_image_path ? (
                          <img
                            src={convertFileSrc(settings.custom_image_path)}
                            alt="preview"
                            style={{ maxWidth: '100%', maxHeight: '100%', objectFit: 'contain' }}
                          />
                        ) : (
                          <Image size={20} color="white" />
                        )}
                      </Box>
                      <VStack align="flex-start" spacing={0}>
                        <Text fontSize="xs" color={subTextColor} fontWeight="medium">{t("crosshair.preview")}</Text>
                        <Text fontSize="2xs" color={subTextColor}>
                          {settings.style.startsWith("Preset_")
                            ? (() => {
                                const found = PRESET_IMAGE_STYLES.find(p => p.id === settings.style);
                                return found ? t(found.labelKey) : t("crosshair.presetImages");
                              })()
                            : t("crosshair.customImage")}
                        </Text>
                      </VStack>
                    </HStack>
                    <Button
                      leftIcon={<RotateCcw size={13} />}
                      colorScheme="gray"
                      variant="outline"
                      size="sm"
                      onClick={resetToDefault}
                    >
                      {t("crosshair.resetDefault") || "恢复默认"}
                    </Button>
                  </HStack>
                </>
              ) : (
                <>
                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.size")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.size}</Text>
                    </HStack>
                    <Slider value={settings.size} min={1} max={100} step={1} onChange={(val) => updateSetting("size", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.thickness")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.thickness}</Text>
                    </HStack>
                    <Slider value={settings.thickness} min={1} max={10} step={1} onChange={(val) => updateSetting("thickness", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.gap")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.gap}</Text>
                    </HStack>
                    <Slider value={settings.gap} min={0} max={50} step={1} onChange={(val) => updateSetting("gap", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.dotSize")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{settings.dot_size}</Text>
                    </HStack>
                    <Slider value={settings.dot_size} min={1} max={8} step={1} onChange={(val) => updateSetting("dot_size", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">{t("crosshair.opacity")}</Text>
                      <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">
                        {Math.round(settings.opacity / 255 * 100)}%
                      </Text>
                    </HStack>
                    <Slider value={settings.opacity} min={50} max={255} step={5} onChange={(val) => updateSetting("opacity", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">X {t("crosshair.offset")}</Text>
                      <HStack spacing={1}>
                        <IconButton
                          aria-label="X-"
                          icon={<Minus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_x", settings.offset_x - 1)}
                          isDisabled={settings.offset_x <= -maxOffsetX}
                        />
                        {editingAxis === 'x' ? (
                          <Input
                            ref={editRef}
                            value={editValue}
                            size="xs"
                            w="48px"
                            textAlign="center"
                            fontSize="xs"
                            bg={inputBg}
                            color={textColor}
                            borderColor={getActiveColor()}
                            _focus={{ borderColor: getActiveColor(), boxShadow: `0 0 0 1px ${getActiveColor()}` }}
                            px={2.5}
                            py={1}
                            borderRadius="md"
                            onChange={(e) => setEditValue(e.target.value)}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') commitEdit();
                              if (e.key === 'Escape') setEditingAxis(null);
                            }}
                          />
                        ) : (
                          <Box
                            onClick={() => startEdit('x')}
                            px={2.5}
                            py={1}
                            minW="48px"
                            textAlign="center"
                            bg={inputBg}
                            borderRadius="md"
                            border="1px solid"
                            borderColor={cardBorder}
                            fontSize="xs"
                            color={textColor}
                            fontWeight="medium"
                            fontFamily="mono"
                            cursor="text"
                            _hover={{ borderColor: getActiveColor() }}
                            transition="border-color 0.15s"
                          >
                            {settings.offset_x}
                          </Box>
                        )}
                        <IconButton
                          aria-label="X+"
                          icon={<Plus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_x", settings.offset_x + 1)}
                          isDisabled={settings.offset_x >= maxOffsetX}
                        />
                      </HStack>
                    </HStack>
                    <Slider value={settings.offset_x} min={-maxOffsetX} max={maxOffsetX} step={1} onChange={(val) => updateSetting("offset_x", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <Box>
                    <HStack justify="space-between" mb={1}>
                      <Text color={textColor} fontSize="sm">Y {t("crosshair.offset")}</Text>
                      <HStack spacing={1}>
                        <IconButton
                          aria-label="Y-"
                          icon={<Minus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_y", settings.offset_y - 1)}
                          isDisabled={settings.offset_y <= -maxOffsetY}
                        />
                        {editingAxis === 'y' ? (
                          <Input
                            ref={editRef}
                            value={editValue}
                            size="xs"
                            w="48px"
                            textAlign="center"
                            fontSize="xs"
                            bg={inputBg}
                            color={textColor}
                            borderColor={getActiveColor()}
                            _focus={{ borderColor: getActiveColor(), boxShadow: `0 0 0 1px ${getActiveColor()}` }}
                            px={2.5}
                            py={1}
                            borderRadius="md"
                            onChange={(e) => setEditValue(e.target.value)}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') commitEdit();
                              if (e.key === 'Escape') setEditingAxis(null);
                            }}
                          />
                        ) : (
                          <Box
                            onClick={() => startEdit('y')}
                            px={2.5}
                            py={1}
                            minW="48px"
                            textAlign="center"
                            bg={inputBg}
                            borderRadius="md"
                            border="1px solid"
                            borderColor={cardBorder}
                            fontSize="xs"
                            color={textColor}
                            fontWeight="medium"
                            fontFamily="mono"
                            cursor="text"
                            _hover={{ borderColor: getActiveColor() }}
                            transition="border-color 0.15s"
                          >
                            {settings.offset_y}
                          </Box>
                        )}
                        <IconButton
                          aria-label="Y+"
                          icon={<Plus size={12} />}
                          size="xs"
                          variant="outline"
                          borderColor={cardBorder}
                          _hover={{ borderColor: getActiveColor(), bg: hoverBg }}
                          onClick={() => updateSetting("offset_y", settings.offset_y + 1)}
                          isDisabled={settings.offset_y >= maxOffsetY}
                        />
                      </HStack>
                    </HStack>
                    <Slider value={settings.offset_y} min={-maxOffsetY} max={maxOffsetY} step={1} onChange={(val) => updateSetting("offset_y", val)}>
                      <SliderTrack bg={sliderBg}><SliderFilledTrack bg={getActiveColor()} /></SliderTrack>
                      <SliderThumb />
                    </Slider>
                  </Box>

                  <HStack justify="space-between" pt={1}>
                    <HStack spacing={2}>
                      <Box
                        w={10} h={10}
                        borderRadius="md"
                        bg="black"
                        display="flex"
                        alignItems="center"
                        justifyContent="center"
                        opacity={settings.opacity / 255}
                      >
                        <Text fontSize="lg" color={settings.color} fontWeight="bold" lineHeight={1}>
                          {STYLE_OPTIONS.find(s => s.id === settings.style)?.icon || "+"}
                        </Text>
                      </Box>
                      <VStack align="flex-start" spacing={0}>
                        <Text fontSize="xs" color={subTextColor} fontWeight="medium">{t("crosshair.preview")}</Text>
                        <Text fontSize="2xs" color={subTextColor}>{t(STYLE_OPTIONS.find(s => s.id === settings.style)?.labelKey ?? "crosshair.styles.cross")}</Text>
                      </VStack>
                    </HStack>
                    <Button
                      leftIcon={<RotateCcw size={13} />}
                      colorScheme="gray"
                      variant="outline"
                      size="sm"
                      onClick={resetToDefault}
                    >
                      {t("crosshair.resetDefault") || "恢复默认"}
                    </Button>
                  </HStack>
                </>
              )}
            </VStack>
          </SettingCard>
        </VStack>
      </SimpleGrid>
    </Box>
  );
}
