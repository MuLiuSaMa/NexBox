import {
  Box,
  Text,
  Heading,
  HStack,
  VStack,
  SimpleGrid,
  Badge,
  IconButton,
  useColorModeValue,
  NumberInput,
  NumberInputField,
  NumberInputStepper,
  NumberIncrementStepper,
  NumberDecrementStepper,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { store } from "@/lib/store";
import { useTranslation } from "react-i18next";
import { ArrowLeft, MousePointerClick, Keyboard, Zap } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useNavigate } from "react-router-dom";
import { MouseHotkeyRecorder } from "@/components/mouse-hotkey-recorder";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useAppStartup } from "@/contexts/app-startup-context";
import { ThemeSwitch } from "@/components/special/theme-switch";

interface AutoClickerStatus {
  running: boolean;
  button: string;
  interval_ms: number;
}

const AUTOCLICKER_STORE_KEY = "autoclicker-settings";
const HOTKEY_STORE_KEY = "autoclicker-hotkey";
const DEFAULT_HOTKEY = "F8";

// 预设：每秒点击次数 → 间隔毫秒
const PRESETS: { cps: number; ms: number }[] = [
  { cps: 10, ms: 100 },
  { cps: 20, ms: 50 },
  { cps: 50, ms: 20 },
  { cps: 100, ms: 10 },
];

// 快捷键预设：F8 / 侧键1 / 侧键2 / 空格
const HOTKEY_PRESETS: { labelKey: string; value: string }[] = [
  { labelKey: "autoclicker.presetF8", value: "F8" },
  { labelKey: "autoclicker.presetSide1", value: "MouseX1" },
  { labelKey: "autoclicker.presetSide2", value: "MouseX2" },
  { labelKey: "autoclicker.presetSpace", value: "Space" },
];

// 统一区块卡片容器（与其他页面一致：全宽大卡 p={6}，不做 hover 变色）
function BlockCard({ children }: { children: React.ReactNode }) {
  const { liquidGlassEnabled } = useBackground();
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const inner = <VStack align="stretch" spacing={4}>{children}</VStack>;

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard w="full" p={6}>
        {inner}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      borderRadius="xl"
      border="1px solid"
      borderColor={borderColor}
      w="full"
      p={6}
    >
      {inner}
    </Box>
  );
}

// 区块头部：主题色圆底图标 + 标题 + 描述
function SectionHeader({ icon, title, desc }: { icon: React.ReactNode; title: string; desc?: string }) {
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  return (
    <HStack spacing={3} align="center">
      <Box p={2} borderRadius="lg" bg={getActiveColor()} flexShrink={0}>
        <Box color={getContrastTextColor()} display="flex">
          {icon}
        </Box>
      </Box>
      <Box flex={1} minW={0}>
        <Text fontWeight="medium" fontSize="sm" color={textColor}>
          {title}
        </Text>
        {desc ? (
          <Text fontSize="xs" color={subTextColor} mt={0.5}>
            {desc}
          </Text>
        ) : null}
      </Box>
    </HStack>
  );
}

export default function AutoClickerPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("mouse");
  const navigate = useNavigate();
  const { getActiveColor, getHoverColor, getBorderColor } = useThemeColor();
  const { autoclickerHotkeyEnabled, saveAutoclickerHotkeyEnabled } = useAppStartup();

  const [running, setRunning] = useState(false);
  const [button, setButton] = useState<"left" | "right">("left");
  const [intervalMs, setIntervalMs] = useState(100);
  const [hotkey, setHotkey] = useState(DEFAULT_HOTKEY);

  const headingColor = useColorModeValue("black", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const themeColor = getActiveColor();
  const themeContrastText = useColorModeValue("black", "#ffffff");
  const selectedBg = getHoverColor();
  const selectedBorder = getBorderColor();

  // 加载运行状态、已保存设置与热键
  useEffect(() => {
    (async () => {
      try {
        const status = await invoke<AutoClickerStatus>("autoclicker_get_status");
        setRunning(status.running);
        setButton(status.button === "right" ? "right" : "left");
        setIntervalMs(status.interval_ms);
      } catch { /* 忽略，使用默认值 */ }

      try {
        const saved = await store.get<{ button: string; interval_ms: number }>(AUTOCLICKER_STORE_KEY);
        if (saved) {
          setButton(saved.button === "right" ? "right" : "left");
          setIntervalMs(saved.interval_ms);
        }
      } catch { /* ignore */ }

      try {
        // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
        const savedHotkey = await invoke<string>("get_autoclicker_hotkey");
        if (savedHotkey) {
          setHotkey(savedHotkey);
        }
      } catch (e) {
        console.error("Failed to load autoclicker hotkey:", e);
      }
    })();
  }, []);

  // 监听后端状态变化（热键触发开关时实时刷新）
  useEffect(() => {
    const unlisten = listen<AutoClickerStatus>("autoclicker-status-changed", (event) => {
      setRunning(event.payload.running);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const saveSettings = useCallback(async (btn: string, ms: number) => {
    try {
      await invoke("autoclicker_update", { button: btn, intervalMs: ms });
      await store.set(AUTOCLICKER_STORE_KEY, { button: btn, interval_ms: ms });
      await store.save();
    } catch (e) {
      console.error("Failed to save autoclicker settings:", e);
    }
  }, []);

  const selectButton = (btn: "left" | "right") => {
    setButton(btn);
    saveSettings(btn, intervalMs);
  };

  const selectInterval = (ms: number) => {
    const v = Math.max(1, Math.min(10000, Math.round(ms)));
    setIntervalMs(v);
    saveSettings(button, v);
  };

  const saveHotkey = async (val: string) => {
    try {
      // Rust 端负责注册并写入 settings.json；这里仅同步前端 store 内存
      await invoke("set_autoclicker_hotkey", { shortcut: val });
      await store.set(HOTKEY_STORE_KEY, val);
      setHotkey(val);
      toast({
        title: t("autoclicker.hotkeySaved") || "快捷键已保存",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (e) {
      console.error("Failed to save autoclicker hotkey:", e);
      const msg = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
      toast({
        title:
          msg && msg.trim() && msg !== "[object Object]"
            ? msg
            : t("autoclicker.hotkeySavedFailed") || "快捷键保存失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  };

  const cps = Math.round(1000 / Math.max(1, intervalMs));

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
            <HStack spacing={2}>
              <MousePointerClick size={24} color={themeColor} />
              <Text>{t("autoclicker.title")}</Text>
            </HStack>
          </Heading>
        </HStack>
        <Badge
          bg={running ? themeColor : undefined}
          color={running ? themeContrastText : subTextColor}
          px={3}
          py={1}
          borderRadius="full"
          fontSize="sm"
        >
          {running ? t("autoclicker.running") : t("autoclicker.stopped")}
        </Badge>
      </HStack>

      <VStack align="stretch" spacing={6}>
        {/* 卡片1：连点器热键开关 + 快捷键选择 */}
        <BlockCard>
          <HStack justify="space-between" align="center" spacing={4}>
            <Box flex={1} minW={0}>
              <SectionHeader
                icon={<Keyboard size={18} />}
                title={t("hotkeySettings.autoclickerToggle") || "连点器热键开关"}
                desc={t("hotkeySettings.autoclickerToggleDesc") || "使用快捷键开始或停止连点（支持中键、侧键）"}
              />
            </Box>
            <ThemeSwitch
              isChecked={autoclickerHotkeyEnabled}
              onChange={(e) => saveAutoclickerHotkeyEnabled(e.target.checked)}
              size="lg"
              flexShrink={0}
            />
          </HStack>

          <Box
            opacity={autoclickerHotkeyEnabled ? undefined : 0.5}
            pointerEvents={autoclickerHotkeyEnabled ? undefined : "none"}
            userSelect={autoclickerHotkeyEnabled ? undefined : "none"}
          >
            <HStack spacing={3} align="center">
              <Text fontSize="xs" color={subTextColor} whiteSpace="nowrap">
                {t("autoclicker.hotkeyPresets")}
              </Text>
              <SimpleGrid columns={4} spacing={2} flex={1}>
                {HOTKEY_PRESETS.map((p) => {
                  const selected = hotkey === p.value;
                  return (
                    <Box
                      key={p.value}
                      as="button"
                      py={2}
                      textAlign="center"
                      borderRadius="md"
                      border="2px solid"
                      borderColor={selected ? selectedBorder : "transparent"}
                      bg={selected ? selectedBg : "transparent"}
                      color={selected ? themeColor : textColor}
                      fontSize="sm"
                      onClick={() => saveHotkey(p.value)}
                    >
                      {t(p.labelKey)}
                    </Box>
                  );
                })}
              </SimpleGrid>
            </HStack>
            <HStack spacing={3} mt={3}>
              <MouseHotkeyRecorder value={hotkey} onChange={saveHotkey} />
              <HStack spacing={2}>
                <Box w={2} h={2} borderRadius="full" bg={running ? themeColor : "gray.400"} />
                <Text fontSize="sm" color={running ? themeColor : textColor}>
                  {running ? t("autoclicker.running") : t("autoclicker.stopped")}
                </Text>
              </HStack>
            </HStack>
            <Text fontSize="xs" color={subTextColor} mt={3}>
              {t("autoclicker.hotkeyHint")}
            </Text>
          </Box>
          {!autoclickerHotkeyEnabled ? (
            <Text fontSize="xs" color={themeColor}>
              {t("autoclicker.enableToggleHint") || "开启上方的「连点器热键开关」后快捷键才会生效"}
            </Text>
          ) : null}
        </BlockCard>

        {/* 卡片2：点击键位 + 点击间隔 */}
        <BlockCard>
          <SimpleGrid columns={{ base: 1, md: 2 }} spacing={6}>
            {/* 左右键 */}
            <Box>
              <Text fontSize="sm" fontWeight="semibold" color={textColor} mb={2}>
                {t("autoclicker.button")}
              </Text>
              <SimpleGrid columns={2} spacing={3}>
                {(["left", "right"] as const).map((b) => {
                  const selected = button === b;
                  return (
                    <Box
                      key={b}
                      as="button"
                      py={4}
                      textAlign="center"
                      borderRadius="lg"
                      border="2px solid"
                      borderColor={selected ? selectedBorder : "transparent"}
                      bg={selected ? selectedBg : "transparent"}
                      color={selected ? themeColor : textColor}
                      fontWeight="medium"
                      onClick={() => selectButton(b)}
                    >
                      {b === "left" ? t("autoclicker.left") : t("autoclicker.right")}
                    </Box>
                  );
                })}
              </SimpleGrid>
              <Text fontSize="xs" color={subTextColor} mt={2}>
                {t("autoclicker.buttonHint")}
              </Text>
            </Box>

            {/* 点击间隔 */}
            <Box>
              <Text fontSize="sm" fontWeight="semibold" color={textColor} mb={2}>
                {t("autoclicker.interval")}
              </Text>
              <SimpleGrid columns={2} spacing={3}>
                {PRESETS.map((p) => {
                  const selected = intervalMs === p.ms;
                  return (
                    <Box
                      key={p.cps}
                      as="button"
                      py={2}
                      textAlign="center"
                      borderRadius="lg"
                      border="2px solid"
                      borderColor={selected ? selectedBorder : "transparent"}
                      bg={selected ? selectedBg : "transparent"}
                      color={selected ? themeColor : textColor}
                      onClick={() => selectInterval(p.ms)}
                    >
                      <Text fontWeight="bold">{p.cps}</Text>
                      <Text fontSize="xs" color={subTextColor}>{t("autoclicker.cps")}</Text>
                    </Box>
                  );
                })}
              </SimpleGrid>
              <HStack spacing={2} mt={3}>
                <Text fontSize="sm" color={subTextColor} whiteSpace="nowrap">
                  {t("autoclicker.customInterval")}
                </Text>
                <NumberInput
                  value={intervalMs}
                  min={1}
                  max={10000}
                  step={10}
                  onChange={(_, v) => {
                    if (!isNaN(v)) selectInterval(v);
                  }}
                  size="sm"
                  flex="1"
                >
                  <NumberInputField />
                  <NumberInputStepper>
                    <NumberIncrementStepper />
                    <NumberDecrementStepper />
                  </NumberInputStepper>
                </NumberInput>
              </HStack>
              <HStack mt={2}>
                <Zap size={16} color={themeColor} />
                <Text fontSize="sm" color={textColor}>
                  {t("autoclicker.currentSpeed")}:{" "}
                  <Text as="span" fontWeight="bold" color={themeColor}>
                    {cps}
                  </Text>{" "}
                  {t("autoclicker.cps")}
                </Text>
              </HStack>
            </Box>
          </SimpleGrid>
        </BlockCard>
      </VStack>
    </Box>
  );
}