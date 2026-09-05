import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Switch,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Button,
  useColorModeValue,
  Badge,
  Icon,
  IconButton,
  SimpleGrid,
  Input,
  Checkbox,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  useDisclosure,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Eye, EyeOff, ArrowLeft, Trash2, Plus, Move, RotateCcw, Download, Settings } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useBackground } from "@/contexts/background-context";
import { useAppStartup } from "@/contexts/app-startup-context";
import { useHardwareReportExport } from "@/lib/use-hardware-report-export";
import { useNavigate } from "react-router-dom";
import { CustomSelect } from "@/components/special/custom-select";
import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { DraggableDisplayItems, DisplayItem } from "@/components/DraggableDisplayItems";
import { useThemeColor } from "@/contexts/theme-color-context";
import { CustomColorPicker } from "@/components/special/custom-color-picker";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { hexToRgba } from "@/lib/color-utils";
import { store } from "@/lib/store";

interface DisplayItemConfig {
  id: string;
  label: string;
  enabled: boolean;
}

type DisplayItems = DisplayItemConfig[];

interface CustomOverlayItem {
  id: string;
  text: string;
  color: string;
  enabled: boolean;
}

interface OverlaySettings {
  display_items: DisplayItems;
  custom_items: CustomOverlayItem[];
  opacity: number;
  style: string;
  font: string;
  font_size: number;
  item_width: number;
  font_color: string;
  position_x?: number | null;
  position_y?: number | null;
  vertical_position_x?: number | null;
  vertical_position_y?: number | null;
  delta_password_maps?: string[];
}

interface HardwareData {
  fps: number | null;
  fps_1low: number | null;
  fps_01low: number | null;
  cpu_usage: number | null;
  cpu_temp: number | null;
  cpu_clock: number | null;
  cpu_voltage: number | null;
  cpu_power: number | null;
  cpu_fan_speed: number | null;
  gpu_temp: number | null;
  gpu_usage: number | null;
  gpu_fan_speed: number | null;
  gpu_power: number | null;
  gpu_clock: number | null;
  gpu_voltage: number | null;
  gpu_memory_clock: number | null;
  memory_usage: number | null;
  ssd_temp: number | null;
  delta_password: string | null;
  game_ping: number | null;
  gpu_vram_used: number | null;
  gpu_vram_total: number | null;
}

const DEFAULT_DISPLAY_ITEMS: DisplayItems = [
  { id: "time", label: "时间", enabled: false },
  { id: "fps", label: "FPS", enabled: false },
  { id: "fps_1low", label: "1% Low", enabled: false },
  { id: "fps_01low", label: "0.1% Low", enabled: false },
  { id: "cpu_temp", label: "CPU温度", enabled: false },
  { id: "cpu_usage", label: "CPU占用", enabled: true },
  { id: "cpu_fan_speed", label: "CPU风扇转速", enabled: false },
  { id: "cpu_clock", label: "CPU频率", enabled: false },
  { id: "cpu_voltage", label: "CPU电压", enabled: false },
  { id: "cpu_power", label: "CPU功耗", enabled: false },
  { id: "gpu_temp", label: "GPU温度", enabled: true },
  { id: "gpu_usage", label: "GPU占用", enabled: true },
  { id: "gpu_fan_speed", label: "GPU风扇转速", enabled: false },
  { id: "gpu_power", label: "GPU功耗", enabled: false },
  { id: "gpu_clock", label: "GPU频率", enabled: false },
  { id: "gpu_voltage", label: "GPU电压", enabled: false },
  { id: "gpu_vram", label: "GPU显存占用", enabled: false },
  { id: "gpu_memory_clock", label: "GPU显存频率", enabled: false },
  { id: "memory_usage", label: "内存占用", enabled: true },
  { id: "ssd_temp", label: "硬盘温度", enabled: false },
  { id: "game_ping", label: "游戏延迟", enabled: false },
  { id: "delta_password", label: "三角洲密码", enabled: false },
];

const DEFAULT_SETTINGS: OverlaySettings = {
  display_items: DEFAULT_DISPLAY_ITEMS,
  custom_items: [],
  opacity: 200,
  style: "default",
  font: "Microsoft YaHei",
  font_size: 13,
  item_width: 130,
  font_color: "#ffffff",
};

const BUILTIN_CHINESE_FONTS = [
  "Microsoft YaHei",
  "Microsoft YaHei UI",
  "SimSun",
  "NSimSun",
  "SimHei",
  "KaiTi",
  "FangSong",
  "DengXian",
  "Microsoft JhengHei",
  "YouYuan",
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
  const headerColor = useColorModeValue("gray.900", "#ffffff");

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

function SliderControl({
  label,
  value,
  min,
  max,
  step,
  onChange,
  suffix = "",
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (val: number) => void;
  suffix?: string;
}) {
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const sliderBg = useColorModeValue("gray.200", "gray.700");
  const { getActiveColor } = useThemeColor();

  return (
    <Box>
      <HStack justify="space-between" mb={2}>
        <Text color={textColor} fontSize="sm">{label}</Text>
        <Text color={getActiveColor()} fontSize="sm" fontWeight="bold">{value}{suffix}</Text>
      </HStack>
      <Slider value={value} min={min} max={max} step={step} onChange={onChange}>
        <SliderTrack bg={sliderBg}>
          <SliderFilledTrack bg={getActiveColor()} />
        </SliderTrack>
        <SliderThumb />
      </Slider>
    </Box>
  );
}

interface CustomItemCardProps {
  item: CustomOverlayItem;
  onUpdate: (id: string, field: keyof CustomOverlayItem, value: string | boolean) => void;
  onRemove: (id: string) => void;
}

function CustomItemCard({ item, onUpdate, onRemove }: CustomItemCardProps) {
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const inputBg = useColorModeValue("gray.50", "#1a1a1a");
  const { getActiveColor } = useThemeColor();

  return (
    <Box p={3} border="1px solid" borderColor={borderColor} borderRadius="lg">
      <VStack align="stretch" spacing={2}>
        <HStack justify="space-between">
          <Input
            size="sm"
            value={item.text}
            onChange={(e) => onUpdate(item.id, "text", e.target.value)}
            placeholder="输入自定义文字..."
            bg={inputBg}
            borderColor={borderColor}
            color={textColor}
            flex={1}
          />
          <IconButton
            aria-label="删除"
            icon={<Trash2 size={14} />}
            size="xs"
            variant="ghost"
            colorScheme="red"
            onClick={() => onRemove(item.id)}
          />
        </HStack>
        <HStack justify="space-between">
          <HStack spacing={2}>
            <CustomColorPicker color={item.color} onChange={(c) => onUpdate(item.id, "color", c)} compact />
            <Text color={textColor} fontSize="xs">{item.color}</Text>
          </HStack>
          <Switch
            isChecked={item.enabled}
            onChange={(e) => onUpdate(item.id, "enabled", e.target.checked)}
            size="sm"
            sx={{
              '& .chakra-switch__track[data-checked]': {
                bg: getActiveColor(),
              },
            }}
          />
        </HStack>
      </VStack>
    </Box>
  );
}

export default function OverlayPanelPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("panels");
  const { overlaySettings, saveOverlaySettings, overlayHotkey, saveOverlayHotkey } = useAppStartup();
  const { exportReport, isExporting } = useHardwareReportExport();
  const navigate = useNavigate();
  const adaptiveTitle = useAdaptiveTextColor();

  const [hardwareData, setHardwareData] = useState<HardwareData>({
    fps: null,
    fps_1low: null,
    fps_01low: null,
    cpu_usage: null,
    cpu_temp: null,
    cpu_clock: null,
    cpu_voltage: null,
  cpu_power: null,
  cpu_fan_speed: null,
  gpu_temp: null,
    gpu_usage: null,
    gpu_fan_speed: null,
    gpu_power: null,
    gpu_clock: null,
    gpu_voltage: null,
    gpu_memory_clock: null,
    memory_usage: null,
    ssd_temp: null,
    delta_password: null,
    game_ping: null,
    gpu_vram_used: null,
    gpu_vram_total: null,
  });
  const [isEnabled, setIsEnabled] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [isDragMode, setIsDragMode] = useState(false);
  const [autoApplyOnStartup, setAutoApplyOnStartup] = useState(false);
  const [isNvidia, setIsNvidia] = useState(true);
  const [availableMaps, setAvailableMaps] = useState<string[]>([]);
  const [selectedMaps, setSelectedMaps] = useState<string[]>([]);

  const {
    isOpen: isMapSettingsOpen,
    onOpen: onMapSettingsOpen,
    onClose: onMapSettingsClose,
  } = useDisclosure();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.600", "gray.400");
  const sliderBg = useColorModeValue("gray.200", "gray.600");
  const { getActiveColor, getHoverColor } = useThemeColor();

  const settings = overlaySettings || DEFAULT_SETTINGS;

  useEffect(() => {
    (async () => {
      let v = await store.get<boolean>("nexbox_auto_overlay");
      if (v !== null && v !== undefined) {
        setAutoApplyOnStartup(v);
      } else {
        setAutoApplyOnStartup(localStorage.getItem("nexbox_auto_overlay") === "true");
      }
    })();
  }, []);

  useEffect(() => {
    loadStatus();
    loadHardwareData(0);
    invoke<boolean>("is_nvidia_gpu").then(setIsNvidia).catch(() => setIsNvidia(false));
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    listen<void>("overlay-status-changed", () => {
      loadStatus();
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  useEffect(() => {
    let requestId = 0;
    const interval = setInterval(() => {
      const currentRequestId = ++requestId;
      loadHardwareData(currentRequestId);
    }, 1000);
    return () => clearInterval(interval);
  }, []);

  const loadStatus = async () => {
    try {
      const status = await invoke<boolean>("get_overlay_panel_status");
      setIsEnabled(status);
    } catch (error) {
      console.error("Failed to load overlay panel status:", error);
    }
  };

  const loadHardwareData = async (requestId: number) => {
    try {
      const data = await invoke<HardwareData>("get_overlay_hardware_data");
      setHardwareData(prev => {
        return {
          fps: data.fps ?? prev.fps,
          fps_1low: data.fps_1low ?? prev.fps_1low,
          fps_01low: data.fps_01low ?? prev.fps_01low,
          cpu_usage: data.cpu_usage ?? prev.cpu_usage,
          cpu_temp: data.cpu_temp ?? prev.cpu_temp,
          cpu_clock: data.cpu_clock ?? prev.cpu_clock,
          cpu_voltage: data.cpu_voltage ?? prev.cpu_voltage,
          cpu_power: data.cpu_power ?? prev.cpu_power,
          cpu_fan_speed: data.cpu_fan_speed ?? prev.cpu_fan_speed,
          gpu_temp: data.gpu_temp ?? prev.gpu_temp,
          gpu_usage: data.gpu_usage ?? prev.gpu_usage,
          gpu_fan_speed: data.gpu_fan_speed ?? prev.gpu_fan_speed,
          gpu_power: data.gpu_power ?? prev.gpu_power,
          gpu_clock: data.gpu_clock ?? prev.gpu_clock,
          gpu_voltage: data.gpu_voltage ?? prev.gpu_voltage,
          gpu_memory_clock: data.gpu_memory_clock ?? prev.gpu_memory_clock,
          memory_usage: data.memory_usage ?? prev.memory_usage,
          ssd_temp: data.ssd_temp ?? prev.ssd_temp,
          delta_password: data.delta_password ?? prev.delta_password,
          game_ping: data.game_ping ?? prev.game_ping,
          gpu_vram_used: data.gpu_vram_used ?? prev.gpu_vram_used,
          gpu_vram_total: data.gpu_vram_total ?? prev.gpu_vram_total,
        };
      });
    } catch (error) {
      console.error("Failed to load hardware data:", error);
    }
  };

  const startOverlay = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("start_overlay_panel", {
        settings: settings,
      });
      if (result.success) {
        setIsEnabled(true);
        toast({
          title: result.message,
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to start overlay panel:", error);
      toast({
        title: t("overlayPanel.startFailed") || "启动失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  };

  const stopOverlay = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("stop_overlay_panel");
      if (result.success) {
        setIsEnabled(false);
        toast({
          title: result.message,
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to stop overlay panel:", error);
      toast({
        title: t("overlayPanel.stopFailed") || "停止失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
    setIsLoading(false);
  };

  const toggleOverlay = async () => {
    if (isEnabled) {
      await stopOverlay();
    } else {
      await startOverlay();
    }
  };

  const toggleDragMode = async () => {
    if (!isEnabled) {
      toast({
        title: t("overlayPanel.overlayNotEnabled") || "请先启用悬浮框",
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    try {
      const newDragMode = !isDragMode;
      // 竖排面板模式：开关鼠标穿透来启用/退出拖动
      if (settings.style === "vertical_panel") {
        if (newDragMode) {
          await invoke("set_vertical_overlay_click_through", { enabled: false });
          setIsDragMode(true);
          toast({
            title: "可在悬浮框面板上拖动，拖完自动固定",
            status: "info",
            duration: 3000,
            isClosable: true,
          });
        } else {
          setIsDragMode(false);
          toast({
            title: t("overlayPanel.positionSaved") || "位置已保存",
            status: "success",
            duration: 2000,
            isClosable: true,
          });
        }
        return;
      }
      await invoke("set_overlay_drag_mode", { enabled: newDragMode });
      setIsDragMode(newDragMode);
      
      // 退出拖动模式时保存位置
      if (!newDragMode) {
        const currentSettings = await invoke<OverlaySettings>("get_overlay_current_settings");
        saveOverlaySettings(currentSettings);
        toast({
          title: t("overlayPanel.positionSaved") || "位置已保存",
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      } else {
        toast({
          title: t("overlayPanel.dragModeEnabled") || "已进入拖动模式，拖动后点击按钮退出",
          status: "info",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to toggle drag mode:", error);
      toast({
        title: t("overlayPanel.dragModeFailed") || "切换拖动模式失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  const resetPosition = async () => {
    if (!isEnabled) {
      toast({
        title: t("overlayPanel.overlayNotEnabled") || "请先启用悬浮框",
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    try {
      // 竖排面板模式使用独立的重置位置命令
      if (settings.style === "vertical_panel") {
        await invoke("reset_vertical_overlay_position");
        const currentSettings = await invoke<OverlaySettings>("get_overlay_current_settings");
        saveOverlaySettings(currentSettings);
      } else {
        await invoke("reset_overlay_position");
        const currentSettings = await invoke<OverlaySettings>("get_overlay_current_settings");
        saveOverlaySettings(currentSettings);
      }
      toast({
        title: t("overlayPanel.positionReset") || "位置已恢复默认",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to reset position:", error);
      toast({
        title: t("overlayPanel.positionResetFailed") || "重置位置失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  const updateSettings = (newSettings: OverlaySettings) => {
    saveOverlaySettings(newSettings);
  };

  const updateDisplayItem = (id: string, enabled: boolean) => {
    const newSettings = {
      ...settings,
      display_items: settings.display_items.map((item) =>
        item.id === id ? { ...item, enabled } : item
      ),
    };
    saveOverlaySettings(newSettings);
  };

  const reorderDisplayItems = useCallback((newOrder: DisplayItems) => {
    const newSettings = {
      ...settings,
      display_items: newOrder,
    };
    saveOverlaySettings(newSettings);
  }, [settings, saveOverlaySettings]);

  const updateSetting = <K extends keyof OverlaySettings>(
    key: K,
    value: OverlaySettings[K]
  ) => {
    const newSettings = { ...settings, [key]: value };
    saveOverlaySettings(newSettings);
  };

  const addCustomItem = () => {
    const newItem: CustomOverlayItem = {
      id: crypto.randomUUID(),
      text: "",
      color: "#00FF00",
      enabled: true,
    };
    const newSettings = {
      ...settings,
      custom_items: [...settings.custom_items, newItem],
    };
    saveOverlaySettings(newSettings);
  };

  const updateCustomItem = (id: string, field: keyof CustomOverlayItem, value: string | boolean) => {
    const newSettings = {
      ...settings,
      custom_items: settings.custom_items.map((item) =>
        item.id === id ? { ...item, [field]: value } : item
      ),
    };
    saveOverlaySettings(newSettings);
  };

  const removeCustomItem = (id: string) => {
    const newSettings = {
      ...settings,
      custom_items: settings.custom_items.filter((item) => item.id !== id),
    };
    saveOverlaySettings(newSettings);
  };

  const formatValue = (value: number | null, suffix: string): string => {
    if (value === null) return "--";
    return `${value}${suffix}`;
  };

  const openMapSettings = async () => {
    // Initialize selected maps from current settings
    const currentMaps = settings.delta_password_maps || [];
    setSelectedMaps(currentMaps);

    // Fetch available maps from the API
    try {
      const passwords = await invoke<Array<{ name: string; password: string }>>("get_delta_passwords");
      const mapNames = passwords.map(p => p.name);
      setAvailableMaps(mapNames);
      // Auto-select all maps if no selection exists
      if (currentMaps.length === 0 && mapNames.length > 0) {
        setSelectedMaps(mapNames);
      }
    } catch {
      setAvailableMaps([]);
    }
    onMapSettingsOpen();
  };

  const toggleMapSelection = (mapName: string) => {
    setSelectedMaps(prev =>
      prev.includes(mapName)
        ? prev.filter(m => m !== mapName)
        : [...prev, mapName]
    );
  };

  const saveMapSettings = () => {
    const newSettings = {
      ...settings,
      delta_password_maps: selectedMaps,
    };
    saveOverlaySettings(newSettings);
    onMapSettingsClose();
  };

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
            {t("overlayPanel.title") || "悬浮框"}
          </Heading>
        </HStack>
        <HStack gap={2}>
          <Button
            leftIcon={<Trash2 size={15} />}
            size="sm"
            variant="outline"
            color="#e74c3c"
            borderColor="rgba(231,76,60,0.3)"
            _hover={{ bg: "rgba(231,76,60,0.1)" }}
            onClick={async () => {
              try {
                await invoke("clear_hardware_data");
                toast({
                  title: "硬件数据已清除",
                  status: "success",
                  duration: 3000,
                  isClosable: true,
                });
              } catch (e) {
                toast({
                  title: "清除失败",
                  description: String(e),
                  status: "error",
                  duration: 3000,
                  isClosable: true,
                });
              }
            }}
          >
            清除数据
          </Button>
          <Button
            leftIcon={<Download size={16} />}
            size="sm"
            variant="outline"
            color={getActiveColor()}
            borderColor={getActiveColor()}
            onClick={exportReport}
            isLoading={isExporting}
          >
            {t("hardwareReport.export") || "导出报告"}
          </Button>
        </HStack>
      </HStack>

      <VStack align="stretch" spacing={5}>
        <SettingCard title={t("overlayPanel.enableOverlay") || "启用悬浮框"}>
          <HStack justify="space-between" wrap="wrap" spacing={4}>
            <HStack>
              <Icon as={isEnabled ? Eye : EyeOff} boxSize={5} color={isEnabled ? "green.400" : "gray.400"} />
              <Badge colorScheme={isEnabled ? "green" : "gray"}>
                {isEnabled ? (t("overlayPanel.statusEnabled") || "已启用") : (t("overlayPanel.statusDisabled") || "已禁用")}
              </Badge>
            </HStack>
            <HStack spacing={4}>
              <HotkeyRecorder
                value={overlayHotkey}
                onChange={async (val) => {
                  const err = await saveOverlayHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("overlayPanel.hotkeySaved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
              <Switch
                isChecked={isEnabled}
                onChange={toggleOverlay}
                isDisabled={isLoading}
                size="lg"
                sx={{
                  '& .chakra-switch__track[data-checked]': {
                    bg: getActiveColor(),
                  },
                }}
              />
              <Button
                leftIcon={<Move size={16} />}
                size="sm"
                variant={isDragMode ? "solid" : "outline"}
                colorScheme={isDragMode ? "orange" : undefined}
                color={isDragMode ? undefined : getActiveColor()}
                borderColor={isDragMode ? undefined : getActiveColor()}
                onClick={toggleDragMode}
                isDisabled={!isEnabled}
              >
                {isDragMode 
                  ? (t("overlayPanel.dragModeActive") || "退出拖动") 
                  : (t("overlayPanel.dragModeStart") || "移动")}
              </Button>
              <Button
                leftIcon={<RotateCcw size={16} />}
                size="sm"
                variant="ghost"
                colorScheme="gray"
                onClick={resetPosition}
                isDisabled={!isEnabled}
              >
                {t("overlayPanel.resetPosition") || "重置"}
              </Button>
            </HStack>
          </HStack>
          <HStack justify="space-between" align="flex-end" spacing={4}>
            <VStack align="flex-start" spacing={1} flex={1} minW={0}>
              <Text fontSize="xs" color={subTextColor} opacity={0.75}>
                游戏需开启无边框模式，全屏模式会被覆盖
              </Text>
              
            </VStack>
            <HStack
              bg={autoApplyOnStartup ? hexToRgba(getActiveColor(), 0.15) : sliderBg}
              px={4}
              py={2}
              borderRadius="xl"
              border="1px solid"
              borderColor={autoApplyOnStartup ? getActiveColor() : "transparent"}
              w="fit-content"
              alignSelf="flex-end"
              flexShrink={0}
            >
              <Text color={subTextColor} fontSize="xs" fontWeight="500">
                启动新境盒时自动启用悬浮框
              </Text>
              <ThemeSwitch
                isChecked={autoApplyOnStartup}
                onChange={(e) => {
                  const val = e.target.checked;
                  setAutoApplyOnStartup(val);
                  localStorage.setItem("nexbox_auto_overlay", val ? "true" : "false");
                  store.set("nexbox_auto_overlay", val).then(() => store.save());
                }}
                isDisabled={isLoading}
              />
            </HStack>
          </HStack>
        </SettingCard>

        <SettingCard title={t("overlayPanel.displayItems") || "显示项"}>
          <SimpleGrid columns={2} spacing={4}>
            <Box>
              <Text fontSize="sm" fontWeight="medium" mb={2} color={subTextColor}>
                {t("overlayPanel.hardwareMonitor") || "硬件监控"} (拖拽排序)
              </Text>
              <DraggableDisplayItems
                items={settings.display_items}
                onReorder={reorderDisplayItems}
                onToggle={updateDisplayItem}
                disabledItems={[]}
                onDeltaPasswordSettings={openMapSettings}
              />
            </Box>

            <Box>
              <Text fontSize="sm" fontWeight="medium" mb={2} color={subTextColor}>
                {t("overlayPanel.custom") || "自定义"}
              </Text>
              <VStack align="stretch" spacing={2}>
                {settings.custom_items.length === 0 ? (
                  <Text fontSize="sm" color="gray.500" fontStyle="italic">
                    暂无自定义项，点击下方按钮添加
                  </Text>
                ) : (
                  settings.custom_items.map((item) => (
                    <CustomItemCard
                      key={item.id}
                      item={item}
                      onUpdate={updateCustomItem}
                      onRemove={removeCustomItem}
                    />
                  ))
                )}
                <Button
                  leftIcon={<Plus size={16} />}
                  size="sm"
                  variant="outline"
                  color={getActiveColor()}
                  borderColor={getActiveColor()}
                  onClick={addCustomItem}
                  mt={1}
                >
                  {t("overlayPanel.addCustomItem") || "添加自定义项"}
                </Button>
              </VStack>
            </Box>
          </SimpleGrid>
        </SettingCard>



        <SettingCard title={t("overlayPanel.appearance") || "外观设置"}>
          <HStack align="start" spacing={6}>
            {/* 左侧：样式选择 */}
            <VStack align="stretch" spacing={2} flex={1}>
              <Text fontSize="sm" fontWeight="medium" color={subTextColor}>
                {t("overlayPanel.styles") || "悬浮框样式"}
              </Text>
              <Box
                as="button"
                onClick={() => updateSetting("style", "default")}
                bg={settings.style === "default" ? hexToRgba(getActiveColor(), 0.12) : "transparent"}
                border="2px solid"
                borderColor={settings.style === "default" ? getActiveColor() : "gray.600"}
                borderRadius="xl"
                p={3}
                cursor="pointer"
                textAlign="center"
                transition="all 0.2s"
                _hover={{
                  borderColor: getActiveColor(),
                  bg: settings.style === "default" ? hexToRgba(getActiveColor(), 0.12) : hexToRgba(getActiveColor(), 0.08),
                }}
              >
                <VStack spacing={2}>
                  <Box
                    w="80px"
                    h="16px"
                    bg="gray.500"
                    borderRadius="none"
                    opacity={0.6}
                  />
                  <Text fontSize="sm" fontWeight="medium" color={subTextColor}>
                    {t("overlayPanel.styles.default") || "默认"}
                  </Text>
                </VStack>
              </Box>
              <Box
                as="button"
                onClick={() => updateSetting("style", "dynamic_island")}
                bg={settings.style === "dynamic_island" ? hexToRgba(getActiveColor(), 0.12) : "transparent"}
                border="2px solid"
                borderColor={settings.style === "dynamic_island" ? getActiveColor() : "gray.600"}
                borderRadius="xl"
                p={3}
                cursor="pointer"
                textAlign="center"
                transition="all 0.2s"
                _hover={{
                  borderColor: getActiveColor(),
                  bg: settings.style === "dynamic_island" ? hexToRgba(getActiveColor(), 0.12) : hexToRgba(getActiveColor(), 0.08),
                }}
              >
                <VStack spacing={2}>
                  <Box
                    w="64px"
                    h="20px"
                    bg="gray.500"
                    borderRadius="full"
                    opacity={0.6}
                  />
                  <Text fontSize="sm" fontWeight="medium" color={subTextColor}>
                    {t("overlayPanel.styles.dynamicIsland") || "灵动岛"}
                  </Text>
                </VStack>
              </Box>
              <Box
                as="button"
                onClick={() => updateSetting("style", "vertical_panel")}
                bg={settings.style === "vertical_panel" ? hexToRgba(getActiveColor(), 0.12) : "transparent"}
                border="2px solid"
                borderColor={settings.style === "vertical_panel" ? getActiveColor() : "gray.600"}
                borderRadius="xl"
                p={3}
                cursor="pointer"
                textAlign="center"
                transition="all 0.2s"
                _hover={{
                  borderColor: getActiveColor(),
                  bg: settings.style === "vertical_panel" ? hexToRgba(getActiveColor(), 0.12) : hexToRgba(getActiveColor(), 0.08),
                }}
              >
                <VStack spacing={2}>
                  <Box display="flex" flexDirection="column" gap="3px" alignItems="center">
                    <Box w="48px" h="5px" bg="gray.500" borderRadius="sm" opacity={0.6} />
                    <Box w="48px" h="5px" bg="gray.500" borderRadius="sm" opacity={0.6} />
                    <Box w="48px" h="5px" bg="gray.500" borderRadius="sm" opacity={0.6} />
                  </Box>
                  <Text fontSize="sm" fontWeight="medium" color={subTextColor}>
                    {t("overlayPanel.styles.verticalPanel") || "竖排面板"}
                  </Text>
                </VStack>
              </Box>
            </VStack>

            {/* 右侧：字体选择 + 不透明度 */}
            <VStack align="stretch" spacing={4} flex={1}>
              <Box>
                <Text fontSize="sm" fontWeight="medium" mb={2} color={subTextColor}>
                  字体
                </Text>
                <CustomSelect
                  value={settings.font}
                  onChange={(val) => updateSetting("font", val)}
                  options={BUILTIN_CHINESE_FONTS.map((f) => ({ value: f, label: f }))}
                  width="100%"
                  direction="up"
                />
              </Box>
              <Box>
                <Text fontSize="sm" fontWeight="medium" mb={2} color={subTextColor}>
                  {t("overlayPanel.fontColor") || "字体颜色"}
                </Text>
                <HStack spacing={3}>
                  <CustomColorPicker
                    color={settings.font_color}
                    onChange={(c) => updateSetting("font_color", c)}
                  />
                  <Text fontSize="sm" color={subTextColor}>{settings.font_color}</Text>
                </HStack>
              </Box>
              <SliderControl
                label={t("overlayPanel.opacity") || "透明度"}
                value={Math.max(1, Math.round(settings.opacity / 255 * 100))}
                min={1}
                max={100}
                onChange={(val) => updateSetting("opacity", Math.round(val / 100 * 255))}
                suffix="%"
              />
              <SliderControl
                label="大小"
                value={settings.font_size}
                min={10}
                max={28}
                step={1}
                onChange={(val) => updateSetting("font_size", val)}
                suffix="px"
              />
              {settings.style === "vertical_panel" && (
                <SliderControl
                  label="单行宽度"
                  value={settings.item_width}
                  min={140}
                  max={400}
                  step={5}
                  onChange={(val) => updateSetting("item_width", val)}
                  suffix="px"
                />
              )}
            </VStack>
          </HStack>
        </SettingCard>
      </VStack>

      {/* 三角洲密码地图选择弹窗 */}
      <Modal isOpen={isMapSettingsOpen} onClose={onMapSettingsClose} isCentered>
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>选择显示的地图</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <Text fontSize="sm" color={subTextColor} mb={3}>
              勾选需要在悬浮框显示的地图密码，未勾选的地图将不会显示。
            </Text>
            {availableMaps.length === 0 ? (
              <Text fontSize="sm" color="gray.500" fontStyle="italic">
                正在加载地图列表...
              </Text>
            ) : (
              <VStack align="stretch" spacing={2}>
                {availableMaps.map((mapName) => (
                  <Checkbox
                    key={mapName}
                    isChecked={selectedMaps.includes(mapName)}
                    onChange={() => toggleMapSelection(mapName)}
                    sx={{
                      '& .chakra-checkbox__control[data-checked]': {
                        bg: getActiveColor(),
                        borderColor: getActiveColor(),
                      },
                    }}
                  >
                    {mapName}
                  </Checkbox>
                ))}
              </VStack>
            )}
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" mr={3} onClick={onMapSettingsClose}>
              取消
            </Button>
            <Button
              bg={getActiveColor()}
              color="white"
              _hover={{ bg: getHoverColor() }}
              onClick={saveMapSettings}
            >
              确认
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}
