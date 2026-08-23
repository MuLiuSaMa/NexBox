import { Box, Flex, HStack, Text, useColorModeValue } from "@chakra-ui/react";
import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LuSettings } from "react-icons/lu";
import { useBackground } from "@/contexts/background-context";
import { useDynamicIsland, type IslandStatus } from "@/components/ui/dynamic-island";
import { ThemeSwitch } from "@/components/special/theme-switch";

type Preset = "default" | "regular" | "competitive";

// 顶部开关提示：开=常规 / 关=默认
const MODE_TOAST: Record<"on" | "off", { title: string; status: IslandStatus; description?: string }> = {
  on: { title: "已开启游戏模式", status: "blue", description: "压制后台进程（常规）" },
  off: { title: "已关闭游戏模式", status: "info" },
};

// 启动时自动打开提示
const AUTO_TOAST: Record<"on" | "off", { title: string; status: IslandStatus; description?: string }> = {
  on: { title: "已开启启动时自动打开", status: "blue" },
  off: { title: "已关闭启动时自动打开", status: "info" },
};

interface Status {
  preset: Preset;
  effective_preset: Preset;
  manual_enabled: boolean;
  auto_enabled: boolean;
}

/** 顶栏右上角游戏模式开关（游戏模式 + 开/关，弹窗含启动时自动打开开关） */
export function GameModeSwitch() {
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const toast = useDynamicIsland("gamepad");
  const textColor = useColorModeValue("gray.600", "gray.300");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  // 玻璃开启时使用半透明玻璃底色 + 柔和边框；关闭时用普通底色
  const glassTrackBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const plainTrackBg = useColorModeValue("whiteAlpha.700", "blackAlpha.500");
  const glassBorderColor = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");
  const plainBorderColor = useColorModeValue("gray.200", "#333333");
  const trackBg = liquidGlassEnabled ? glassTrackBg : plainTrackBg;
  const borderColor = liquidGlassEnabled ? glassBorderColor : plainBorderColor;
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;
  const backdropFilter = liquidGlassEnabled
    ? `blur(${effectiveBlur}px) saturate(1.3)`
    : "blur(10px)";

  // 弹窗玻璃：开启时半透明 + 模糊（与软件内一致），关闭时纯色不透明
  const glassPopupBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const plainPopupBg = useColorModeValue("white", "#111111");
  const popupBg = liquidGlassEnabled ? glassPopupBg : plainPopupBg;
  const popupBackdropFilter = liquidGlassEnabled
    ? `blur(${effectiveBlur}px) saturate(1.3)`
    : "none";
  const popupBorderColor = liquidGlassEnabled ? glassBorderColor : plainBorderColor;

  const [preset, setPreset] = useState<Preset>("default");
  // 自动打开设置：内联浮层 + 开关状态
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [autoOn, setAutoOn] = useState(false);
  const [saving, setSaving] = useState(false);
  const popupRef = useRef<HTMLDivElement | null>(null);

  // 打开浮层时读取当前自动档
  const toggleSettings = useCallback(async () => {
    const next = !settingsOpen;
    setSettingsOpen(next);
    if (next) {
      try {
        const c = await invoke<{ auto_preset?: Preset }>("game_mode_get_config");
        setAutoOn((c.auto_preset || "default") !== "default");
      } catch (e) {
        console.error("读取自动打开设置失败:", e);
      }
    }
  }, [settingsOpen]);

  // 点空白处关闭浮层
  useEffect(() => {
    if (!settingsOpen) return;
    const onMouseDown = (e: MouseEvent) => {
      if (popupRef.current && !popupRef.current.contains(e.target as Node)) {
        setSettingsOpen(false);
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    return () => document.removeEventListener("mousedown", onMouseDown);
  }, [settingsOpen]);

  // 手动开关：开=常规 / 关=默认
  const handleManualToggle = useCallback(
    async (on: boolean) => {
      if (saving) return;
      setSaving(true);
      const value: Preset = on ? "regular" : "default";
      setPreset(value);
      try {
        await invoke("game_mode_set_preset", { preset: value });
        window.dispatchEvent(
          new CustomEvent("game-mode-preset-changed", { detail: { preset: value } })
        );
        toast({
          title: MODE_TOAST[on ? "on" : "off"].title,
          status: MODE_TOAST[on ? "on" : "off"].status,
          description: MODE_TOAST[on ? "on" : "off"].description,
          iconKey: "gamepad",
        });
      } catch (e) {
        console.error("切换游戏模式失败:", e);
        setPreset((p) => p);
      } finally {
        setSaving(false);
      }
    },
    [saving, toast]
  );

  // 启动时自动打开：开=常规 / 关=默认
  const handleAutoToggle = useCallback(
    async (on: boolean) => {
      if (saving) return;
      setSaving(true);
      const value: Preset = on ? "regular" : "default";
      setAutoOn(on);
      try {
        await invoke("game_mode_set_auto_preset", { preset: value });
        toast({
          title: AUTO_TOAST[on ? "on" : "off"].title,
          status: AUTO_TOAST[on ? "on" : "off"].status,
          description: "游戏启动时自动打开已更新",
          iconKey: "gamepad",
        });
      } catch (e) {
        console.error("设置自动打开失败:", e);
        setAutoOn((a) => a);
      } finally {
        setSaving(false);
      }
    },
    [saving, toast]
  );

  useEffect(() => {
    // 初始加载生效档位（游戏运行时可能已被后端强制为常规/竞技）
    invoke<Status>("game_mode_get_status")
      .then((s) => setPreset(s.effective_preset || "default"))
      .catch((e) => console.error("加载游戏模式状态失败:", e));
    // 后端生效档位变化（游戏启动/退出）→ 实时同步顶栏
    let unlisten: UnlistenFn | undefined;
    listen<Preset>("game-mode-effective-changed", (event) => {
      setPreset(event.payload || "default");
    }).then((fn) => {
      unlisten = fn;
    });
    // 页面内切换时同步
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail;
      if (detail && detail.preset) setPreset(detail.preset as Preset);
    };
    window.addEventListener("game-mode-preset-changed", handler as EventListener);
    return () => {
      if (unlisten) unlisten();
      window.removeEventListener("game-mode-preset-changed", handler as EventListener);
    };
  }, []);

  // 非默认档（常规/竞技）均视为开启
  const isOn = preset !== "default";

  return (
    <Box position="relative" onMouseDown={(e) => e.stopPropagation()}>
      <Flex
        align="center"
        gap={2}
        px={3}
        py="5px"
        borderRadius="full"
        bg={trackBg}
        border="1px solid"
        borderColor={borderColor}
        backdropFilter={backdropFilter}
      >
        <Text fontSize="xs" fontWeight="medium" color={labelColor} whiteSpace="nowrap">
          游戏模式
        </Text>
        <ThemeSwitch
          size="sm"
          isChecked={isOn}
          onChange={(e) => handleManualToggle(e.target.checked)}
        />
        <Box
          as="button"
          aria-label="游戏模式设置"
          display="flex"
          alignItems="center"
          justifyContent="center"
          w="26px"
          h="26px"
          borderRadius="full"
          ml={0.5}
          color={textColor}
          cursor="pointer"
          transition="color 0.15s, background 0.15s"
          zIndex={1}
          _hover={{ color: useColorModeValue("gray.800", "gray.100"), bg: useColorModeValue("gray.100", "gray.700") }}
          onClick={toggleSettings}
        >
          <LuSettings size={14} />
        </Box>
      </Flex>

      <AnimatePresence>
        {settingsOpen && (
          <motion.div
            ref={popupRef}
            initial={{ opacity: 0, y: -6, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -6, scale: 0.98 }}
            transition={{ duration: 0.15 }}
            style={{
              position: "absolute",
              top: "calc(100% + 8px)",
              right: 0,
              zIndex: 30,
              pointerEvents: "auto",
            }}
          >
            <Box
              data-backdrop-filter
              bg={popupBg}
              border="1px solid"
              borderColor={popupBorderColor}
              borderRadius="lg"
              boxShadow="lg"
              minW="260px"
              style={{ backdropFilter: popupBackdropFilter, WebkitBackdropFilter: popupBackdropFilter }}
            >
              <HStack justify="space-between" px={5} py={4} spacing={4}>
                <Text fontSize="sm" color={labelColor} whiteSpace="nowrap">
                  游戏启动时自动打开
                </Text>
                <ThemeSwitch
                  size="md"
                  isChecked={autoOn}
                  onChange={(e) => handleAutoToggle(e.target.checked)}
                />
              </HStack>
            </Box>
          </motion.div>
        )}
      </AnimatePresence>
    </Box>
  );
}
