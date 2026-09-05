import {
  Box,
  Flex,
  HStack,
  Text,
  Badge,
  VStack,
  Divider,
  useColorModeValue,
  Button,
  IconButton,
  Input,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
  Progress,
  useDisclosure,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Spinner,
  Tooltip,
  SimpleGrid,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";

import {
  LuMonitor,
  LuInfo,
  LuSettings,
  LuChevronDown,
  LuCheck,
  LuImage,
  LuUpload,
  LuX,
  LuDownload,
  LuExternalLink,
  LuRefreshCw,
  LuPalette,
  LuWifi,
  LuGlobe,
  LuHeart,
  LuKeyboard,
  LuPlus,
  LuTrash2,
  LuUsers,
  LuBug,
  LuRotateCcw,
  LuSlidersHorizontal,
} from "react-icons/lu";
import { RiBilibiliFill, RiTiktokFill } from "react-icons/ri";
import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useThemeMode } from "@/contexts/theme-mode-context";

import { useFont } from "@/contexts/font-context";
import { PRESET_COLORS, hexToRgba } from "@/lib/color-utils";
import { CustomColorPicker } from "@/components/special/custom-color-picker";
import { fetchReleaseByTag, type ReleaseInfo } from "@/lib/update-checker";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { LiquidGlassMenuItem } from "@/components/special/liquid-glass-menu-item";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { CustomSelect } from "@/components/special/custom-select";
import { useQQGroups, type QqGroup } from "@/hooks/use-qq-groups";
import { QqGroupIcon } from "@/components/ui/qq-group-icon";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useSearchParams } from "react-router-dom";
import { useUpdate } from "@/contexts/update-context";
import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { MouseHotkeyRecorder } from "@/components/mouse-hotkey-recorder";
import { useAppStartup } from "@/contexts/app-startup-context";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  DragEndEvent,
  type Modifier,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { GripVertical } from "lucide-react";
import AdvancedPage from "@/pages/AdvancedPage";
import { store } from "@/lib/store";

/** 限制导航栏拖拽只能沿竖直方向移动，禁止左右（向右）拖动 */
const restrictToVerticalAxis: Modifier = ({ transform }) => ({
  ...transform,
  x: 0,
});

const settingItems = [
  { id: "general", labelKey: "settings.general", icon: LuSettings },
  { id: "appearance", labelKey: "settings.appearance", icon: LuMonitor },
  { id: "advanced", labelKey: "settings.advanced.label", icon: LuSlidersHorizontal },
  { id: "hotkeys", labelKey: "settings.hotkeys", icon: LuKeyboard },
  { id: "network", labelKey: "settings.network", icon: LuWifi },
  { id: "contributor", labelKey: "settings.contributor", icon: LuUsers },
  { id: "sponsor", labelKey: "settings.sponsor", icon: LuHeart },
  { id: "about", labelKey: "settings.about", icon: LuInfo },
];

function SortableNavItem({
  id,
  label,
  visible,
  onToggle,
  dragHandleColor,
  labelColor,
  dragBg,
}: {
  id: string;
  label: string;
  visible: boolean;
  onToggle: () => void;
  dragHandleColor: string;
  labelColor: string;
  dragBg: string;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.8 : 1,
    zIndex: isDragging ? 10 : 1,
    background: isDragging ? dragBg : undefined,
    borderRadius: isDragging ? "md" : undefined,
  };

  return (
    <Box ref={setNodeRef} style={style}>
      <Divider />
      <HStack justify="space-between" py={2} pl={1}>
        <HStack spacing={2} flex={1}>
          <Box
            cursor="grab"
            color={dragHandleColor}
            _hover={{ color: labelColor }}
            _active={{ cursor: "grabbing" }}
            {...attributes}
            {...listeners}
            display="flex"
            alignItems="center"
          >
            <GripVertical size={16} />
          </Box>
          <Text fontSize="sm" color={labelColor} fontWeight="medium">
            {label}
          </Text>
        </HStack>
        <ThemeSwitch
          size="md"
          isChecked={visible}
          onChange={onToggle}
        />
      </HStack>
    </Box>
  );
}

function GeneralSettings() {
  const { t, i18n } = useTranslation();
  const toast = useDynamicIsland("settings");
  const { config, getContrastTextColor } = useThemeColor();
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  // 模糊立即生效：页面切换动画期间的 backdrop-filter 关闭由 .page-animating 类统一处理
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;

  const [language, setLanguage] = useState(i18n.language || "zh");
  const [todayPopularityEnabled, setTodayPopularityEnabled] = useState(true);
  const [announcementEnabled, setAnnouncementEnabled] = useState(true);
  const [randomQuoteEnabled, setRandomQuoteEnabled] = useState(true);
  const [gameLauncherEnabled, setGameLauncherEnabled] = useState(true);
  const [homeHardwareModelEnabled, setHomeHardwareModelEnabled] = useState(true);
  const [gameWinKeyCardEnabled, setGameWinKeyCardEnabled] = useState(true);
  const [gameModeEnabled, setGameModeEnabled] = useState(true);
  const [searchBarEnabled, setSearchBarEnabled] = useState(true);
  const [feedbackEnabled, setFeedbackEnabled] = useState(true);
  const [qqGroupCardEnabled, setQqGroupCardEnabled] = useState(true);
  const [docsCardEnabled, setDocsCardEnabled] = useState(true);
  const [randomImageEnabled, setRandomImageEnabled] = useState(true);
  const [moodCardEnabled, setMoodCardEnabled] = useState(true);
  const [homeUsername, setHomeUsername] = useState("");
  const [splashLogo, setSplashLogo] = useState<string | null>(null);
  const [closeBehavior, setCloseBehavior] = useState<string>(() => {
    return localStorage.getItem("nexbox_close_behavior") || "ask";
  });
  const [sidebarShowLabel, setSidebarShowLabel] = useState(false);
  const [navVisibility, setNavVisibility] = useState<Record<string, boolean>>({});
  const [navPosition, setNavPosition] = useState<"left" | "top">("left");
  const NAV_ORDER_KEY = "nexbox_nav_order";
  const defaultNavOrder = ["/hardware", "/tools", "/builtin-tools", "/optimization", "/music", "/delta-force", "/steam", "/epic-free", "/custom"];
  const [navOrder, setNavOrder] = useState<string[]>(defaultNavOrder);
  const navSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const dragHandleColor = useColorModeValue("gray.400", "gray.500");
  const sortableDragBg = useColorModeValue("blue.50", "rgba(59,130,246,0.1)");
  const [pageTransitionMode, setPageTransitionMode] = useState<"slide" | "fade" | "off">("fade");
  const [autoStart, setAutoStart] = useState(false);
  const [autoStartLoading, setAutoStartLoading] = useState(true);
  const [minimizedStart, setMinimizedStart] = useState(false);
  const { autoUpdateEnabled, setAutoUpdateEnabled } = useUpdate();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const inputBg = useColorModeValue("white", "#1a1a1a");
  const inputBorder = useColorModeValue("gray.200", "#333333");
  const splashLogoHoverBorder = useColorModeValue("blue.400", "blue.300");
  const segmentedControlBg = useColorModeValue(
    liquidGlassEnabled ? "rgba(255,255,255,0.24)" : "rgba(255,255,255,0.78)",
    liquidGlassEnabled ? "rgba(18,18,18,0.34)" : "rgba(18,18,18,0.58)"
  );
  const segmentedControlBorder = useColorModeValue(
    liquidGlassEnabled ? "rgba(255,255,255,0.34)" : "rgba(255,255,255,0.72)",
    liquidGlassEnabled ? "rgba(255,255,255,0.12)" : "rgba(255,255,255,0.08)"
  );
  const segmentedControlHoverBg = useColorModeValue(
    liquidGlassEnabled ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.96)",
    liquidGlassEnabled ? "rgba(255,255,255,0.08)" : "rgba(255,255,255,0.08)"
  );
  const segmentedControlShadow = useColorModeValue(
    liquidGlassEnabled ? "0 16px 34px rgba(15, 23, 42, 0.14)" : "0 10px 30px rgba(15, 23, 42, 0.08)",
    liquidGlassEnabled ? "0 18px 38px rgba(0, 0, 0, 0.28)" : "0 12px 30px rgba(0, 0, 0, 0.24)"
  );
  const segmentedActiveBg = useColorModeValue(
    `linear-gradient(135deg, ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.26 : 0.2)} 0%, ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.42 : 0.34)} 100%)`,
    `linear-gradient(135deg, ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.38 : 0.32)} 0%, ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.24 : 0.18)} 100%)`
  );
  const segmentedActiveBorder = useColorModeValue(
    hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.34 : 0.28),
    hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.5 : 0.42)
  );
  const segmentedActiveShadow = useColorModeValue(
    `0 10px 24px ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.22 : 0.18)}`,
    `0 12px 28px ${hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.3 : 0.24)}`
  );
  const segmentedActiveOverlayGradient = useColorModeValue(
    liquidGlassEnabled
      ? "linear(to-b, rgba(255,255,255,0.5), rgba(255,255,255,0.12))"
      : "linear(to-b, rgba(255,255,255,0.42), rgba(255,255,255,0.08))",
    liquidGlassEnabled
      ? "linear(to-b, rgba(255,255,255,0.22), rgba(255,255,255,0.03))"
      : "linear(to-b, rgba(255,255,255,0.16), rgba(255,255,255,0.02))"
  );
  const segmentedContainerGlow = useColorModeValue(
    hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.14 : 0.08),
    hexToRgba(config.primaryColor, liquidGlassEnabled ? 0.18 : 0.1)
  );
  const segmentedGlassSheen = useColorModeValue(
    "linear-gradient(135deg, rgba(255,255,255,0.58) 0%, rgba(255,255,255,0.08) 52%, rgba(255,255,255,0.02) 100%)",
    "linear-gradient(135deg, rgba(255,255,255,0.12) 0%, rgba(255,255,255,0.04) 52%, rgba(255,255,255,0.01) 100%)"
  );
  const segmentedActiveText = getContrastTextColor();

  useEffect(() => {
    const savedLang = i18n.language || "zh";
    setLanguage(savedLang);

    (async () => {
      // 今日人气
      let v = await store.get<boolean>("nexbox_today_popularity_enabled");
      if (v !== null && v !== undefined) {
        setTodayPopularityEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_today_popularity_enabled");
        if (ls !== null) setTodayPopularityEnabled(ls === "true");
      }

      // 公告
      v = await store.get<boolean>("nexbox_announcement_enabled");
      if (v !== null && v !== undefined) {
        setAnnouncementEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_announcement_enabled");
        if (ls !== null) setAnnouncementEnabled(ls === "true");
      }

      // 随机引用
      v = await store.get<boolean>("nexbox_random_quote_enabled");
      if (v !== null && v !== undefined) {
        setRandomQuoteEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_random_quote_enabled");
        if (ls !== null) setRandomQuoteEnabled(ls === "true");
      }

      // 搜索栏
      v = await store.get<boolean>("nexbox_search_bar_enabled");
      if (v !== null && v !== undefined) {
        setSearchBarEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_search_bar_enabled");
        if (ls !== null) setSearchBarEnabled(ls === "true");
      }

      // 游戏启动器
      v = await store.get<boolean>("nexbox_game_launcher_enabled");
      if (v !== null && v !== undefined) {
        setGameLauncherEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_game_launcher_enabled");
        if (ls !== null) setGameLauncherEnabled(ls === "true");
      }

      // 硬件模型
      v = await store.get<boolean>("nexbox_home_hardware_model_enabled");
      if (v !== null && v !== undefined) {
        setHomeHardwareModelEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_home_hardware_model_enabled");
        if (ls !== null) setHomeHardwareModelEnabled(ls === "true");
      }

      // 游戏时禁用 Win 键卡片显示（store 持久化，默认显示）
      v = await store.get<boolean>("nexbox_game_win_key_card_enabled");
      if (v !== null && v !== undefined) {
        setGameWinKeyCardEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_game_win_key_card_enabled");
        if (ls !== null) setGameWinKeyCardEnabled(ls === "true");
      }

      // 游戏模式（顶栏切换条）显示开关（store 持久化，默认显示）
      v = await store.get<boolean>("nexbox_game_mode_enabled");
      if (v !== null && v !== undefined) {
        setGameModeEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_game_mode_enabled");
        if (ls !== null) setGameModeEnabled(ls === "true");
      }

      // 反馈
      v = await store.get<boolean>("nexbox_feedback_enabled");
      if (v !== null && v !== undefined) {
        setFeedbackEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_feedback_enabled");
        if (ls !== null) setFeedbackEnabled(ls === "true");
      }

      // 官方QQ群卡片
      v = await store.get<boolean>("nexbox_qq_group_card_enabled");
      if (v !== null && v !== undefined) {
        setQqGroupCardEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_qq_group_card_enabled");
        if (ls !== null) setQqGroupCardEnabled(ls === "true");
      }

      // 使用文档卡片
      v = await store.get<boolean>("nexbox_docs_card_enabled");
      if (v !== null && v !== undefined) {
        setDocsCardEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_docs_card_enabled");
        if (ls !== null) setDocsCardEnabled(ls === "true");
      }

      // 主页随机图片卡片显示（store 持久化，默认开启）
      v = await store.get<boolean>("nexbox_random_image_enabled");
      if (v !== null && v !== undefined) {
        setRandomImageEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_random_image_enabled");
        if (ls !== null) setRandomImageEnabled(ls === "true");
      }

      // 主页心境卡片显示（store 持久化，默认开启）
      v = await store.get<boolean>("nexbox_mood_card_enabled");
      if (v !== null && v !== undefined) {
        setMoodCardEnabled(v);
      } else {
        const ls = localStorage.getItem("nexbox_mood_card_enabled");
        if (ls !== null) setMoodCardEnabled(ls === "true");
      }

      // 标题用户名（空 = 使用系统用户名）
      let uname = await store.get<string>("nexbox_home_username");
      if (uname !== null && uname !== undefined) {
        setHomeUsername(uname);
      } else {
        const ls = localStorage.getItem("nexbox_home_username");
        if (ls !== null) setHomeUsername(ls);
      }

      // 侧边栏标签
      v = await store.get<boolean>("nexbox_sidebar_show_label");
      if (v !== null && v !== undefined) {
        setSidebarShowLabel(v);
      } else {
        const ls = localStorage.getItem("nexbox_sidebar_show_label");
        if (ls !== null) setSidebarShowLabel(ls === "true");
      }

      // 导航位置
      let nv = await store.get<string>("nexbox_nav_position");
      if (nv === "top" || nv === "left") {
        setNavPosition(nv);
      } else {
        const ls = localStorage.getItem("nexbox_nav_position");
        if (ls === "top") setNavPosition("top");
      }

      // 过渡动画
      let pm = await store.get<string>("nexbox_page_transition");
      if (pm === "slide" || pm === "fade" || pm === "off") {
        setPageTransitionMode(pm);
      } else {
        const ls = localStorage.getItem("nexbox_page_transition") as "slide" | "fade" | "off" | null;
        if (ls) setPageTransitionMode(ls);
      }

      // 关闭行为
      let cb = await store.get<string>("nexbox_close_behavior");
      if (cb) {
        setCloseBehavior(cb);
      } else {
        const ls = localStorage.getItem("nexbox_close_behavior");
        if (ls) setCloseBehavior(ls);
      }

      // 启动 Logo
      let logo = await store.get<string>("nexbox_splash_logo");
      if (logo) {
        setSplashLogo(logo);
      } else {
        const ls = localStorage.getItem("nexbox_splash_logo");
        if (ls) setSplashLogo(ls);
      }

      // 导航可见性
      const visibilityMap: Record<string, boolean> = {};
      for (const p of ["/hardware", "/tools", "/builtin-tools", "/optimization", "/music", "/delta-force", "/steam", "/epic-free", "/custom"]) {
        const key = `nexbox_nav_visible_${p.replace(/\//g, "").replace(/-/g, "_")}`;
        let vis = await store.get<boolean>(key);
        if (vis === null || vis === undefined) {
          if (p === "/custom") {
            vis = localStorage.getItem(key) === "true";
          } else {
            vis = localStorage.getItem(key) !== "false";
          }
        }
        visibilityMap[p] = vis;
      }
      setNavVisibility(visibilityMap);

      // 导航栏排序
      let orderStr = await store.get<string>("nexbox_nav_order");
      if (orderStr) {
        try {
          const parsed = JSON.parse(orderStr) as string[];
          let changed = false;
          for (const p of defaultNavOrder) {
            if (!parsed.includes(p)) { parsed.push(p); changed = true; }
          }
          if (changed) {
            parsed.sort((a, b) => defaultNavOrder.indexOf(a) - defaultNavOrder.indexOf(b));
            await store.set("nexbox_nav_order", JSON.stringify(parsed));
            await store.save();
          }
          setNavOrder(parsed);
        } catch {}
      } else {
        const ls = localStorage.getItem(NAV_ORDER_KEY);
        if (ls) {
          try {
            const parsed = JSON.parse(ls) as string[];
            let changed = false;
            for (const p of defaultNavOrder) {
              if (!parsed.includes(p)) { parsed.push(p); changed = true; }
            }
            if (changed) {
              parsed.sort((a, b) => defaultNavOrder.indexOf(a) - defaultNavOrder.indexOf(b));
              localStorage.setItem(NAV_ORDER_KEY, JSON.stringify(parsed));
            }
            setNavOrder(parsed);
          } catch {}
        }
      }
    })();

    invoke<boolean>("check_nexbox_auto_start")
      .then((enabled) => setAutoStart(enabled))
      .catch(() => {})
      .finally(() => setAutoStartLoading(false));

    // 读取"开机最小化启动"设置
    store
      .get<boolean>("nexbox_minimized_start")
      .then((v) => {
        if (typeof v === "boolean") setMinimizedStart(v);
      })
      .catch(() => {});

    // 监听主页卡片显示变化，保持同步
    const handleWinKeyCardSync = (e: CustomEvent) => {
      setGameWinKeyCardEnabled(e.detail);
    };
    window.addEventListener("game-win-key-card-setting-changed", handleWinKeyCardSync as EventListener);

    const handleRandomImageSync = (e: CustomEvent) => {
      setRandomImageEnabled(e.detail);
    };
    window.addEventListener("random-image-setting-changed", handleRandomImageSync as EventListener);

    const handleMoodCardSync = (e: CustomEvent) => {
      setMoodCardEnabled(e.detail);
    };
    window.addEventListener("mood-card-setting-changed", handleMoodCardSync as EventListener);

    return () => {
      window.removeEventListener("game-win-key-card-setting-changed", handleWinKeyCardSync as EventListener);
      window.removeEventListener("random-image-setting-changed", handleRandomImageSync as EventListener);
      window.removeEventListener("mood-card-setting-changed", handleMoodCardSync as EventListener);
    };
  }, []);

  const handleLanguageChange = (newLang: string) => {
    setLanguage(newLang);
    i18n.changeLanguage(newLang);
    localStorage.setItem("i18nextLng", newLang);
  };

  const handleTodayPopularityToggle = () => {
    const newValue = !todayPopularityEnabled;
    setTodayPopularityEnabled(newValue);
    store.set("nexbox_today_popularity_enabled", newValue).then(() => store.save());
    window.dispatchEvent(new CustomEvent("today-popularity-setting-changed", { detail: newValue }));
  };

  const handleAnnouncementToggle = () => {
    const newValue = !announcementEnabled;
    setAnnouncementEnabled(newValue);
    store.set("nexbox_announcement_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_announcement_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("announcement-setting-changed", { detail: newValue }));
  };

  const handleRandomQuoteToggle = () => {
    const newValue = !randomQuoteEnabled;
    setRandomQuoteEnabled(newValue);
    store.set("nexbox_random_quote_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_random_quote_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("random-quote-setting-changed", { detail: newValue }));
  };

  const handleGameLauncherToggle = () => {
    const newValue = !gameLauncherEnabled;
    setGameLauncherEnabled(newValue);
    store.set("nexbox_game_launcher_enabled", newValue).then(() => store.save());
    window.dispatchEvent(new CustomEvent("game-launcher-setting-changed", { detail: newValue }));
  };

  const handleHomeHardwareModelToggle = () => {
    const newValue = !homeHardwareModelEnabled;
    setHomeHardwareModelEnabled(newValue);
    store.set("nexbox_home_hardware_model_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_home_hardware_model_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("home-hardware-model-setting-changed", { detail: newValue }));
  };

  const handleGameWinKeyCardToggle = async () => {
    const newValue = !gameWinKeyCardEnabled;
    setGameWinKeyCardEnabled(newValue);
    store.set("nexbox_game_win_key_card_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_game_win_key_card_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("game-win-key-card-setting-changed", { detail: newValue }));
    // 卡片隐藏时功能同步关闭，保持显示与功能一致
    if (!newValue) {
      try {
        await invoke("set_game_win_key_enabled", { enabled: false });
      } catch {
        // ignore
      }
    }
  };

  const handleGameModeToggle = () => {
    const newValue = !gameModeEnabled;
    setGameModeEnabled(newValue);
    store.set("nexbox_game_mode_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_game_mode_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("game-mode-setting-changed", { detail: newValue }));
    // 关闭时停用当前生效的游戏模式压制（切回默认档并释放进程），与隐藏保持一致
    if (!newValue) {
      invoke("game_mode_set_preset", { preset: "default" }).catch((e) => {
        console.error("停用游戏模式失败:", e);
      });
    }
  };

  const handleMoodCardToggle = () => {
    const newValue = !moodCardEnabled;
    setMoodCardEnabled(newValue);
    store.set("nexbox_mood_card_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_mood_card_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("mood-card-setting-changed", { detail: newValue }));
  };

  const handleSearchBarToggle = () => {
    const newValue = !searchBarEnabled;
    setSearchBarEnabled(newValue);
    store.set("nexbox_search_bar_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_search_bar_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("search-bar-setting-changed", { detail: newValue }));
  };

  const handleFeedbackToggle = () => {
    const newValue = !feedbackEnabled;
    setFeedbackEnabled(newValue);
    store.set("nexbox_feedback_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_feedback_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("feedback-setting-changed", { detail: newValue }));
  };

  const handleQqGroupCardToggle = () => {
    const newValue = !qqGroupCardEnabled;
    setQqGroupCardEnabled(newValue);
    store.set("nexbox_qq_group_card_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_qq_group_card_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("qq-group-card-setting-changed", { detail: newValue }));
  };

  const handleDocsCardToggle = () => {
    const newValue = !docsCardEnabled;
    setDocsCardEnabled(newValue);
    store.set("nexbox_docs_card_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_docs_card_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("docs-card-setting-changed", { detail: newValue }));
  };

  const handleRandomImageToggle = () => {
    const newValue = !randomImageEnabled;
    setRandomImageEnabled(newValue);
    store.set("nexbox_random_image_enabled", newValue).then(() => store.save());
    localStorage.setItem("nexbox_random_image_enabled", String(newValue));
    window.dispatchEvent(new CustomEvent("random-image-setting-changed", { detail: newValue }));
  };

  // 标题用户名：留空时使用系统用户名
  const handleHomeUsernameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const newValue = e.target.value;
    setHomeUsername(newValue);
    store.set("nexbox_home_username", newValue).then(() => store.save());
    localStorage.setItem("nexbox_home_username", newValue);
    window.dispatchEvent(new CustomEvent("home-username-setting-changed", { detail: newValue }));
  };

  const handleHomeUsernameReset = () => {
    setHomeUsername("");
    store.delete("nexbox_home_username").then(() => store.save());
    localStorage.removeItem("nexbox_home_username");
    window.dispatchEvent(new CustomEvent("home-username-setting-changed", { detail: "" }));
  };

  const handleSplashLogoUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onloadend = () => {
        const result = reader.result as string;
        setSplashLogo(result);
        localStorage.setItem("nexbox_splash_logo", result);
        store.set("nexbox_splash_logo", result).then(() => store.save());
      };
      reader.readAsDataURL(file);
    }
    e.target.value = "";
  };

  const handleSplashLogoReset = () => {
    setSplashLogo(null);
    localStorage.removeItem("nexbox_splash_logo");
    store.delete("nexbox_splash_logo").then(() => store.save());
  };

  const handleCloseBehaviorChange = (value: string) => {
    setCloseBehavior(value);
    localStorage.setItem("nexbox_close_behavior", value);
    store.set("nexbox_close_behavior", value).then(() => store.save());
    window.dispatchEvent(new CustomEvent("close-behavior-changed"));
  };

  const handleSidebarShowLabelChange = (value: string) => {
    const newValue = value === "true";
    setSidebarShowLabel(newValue);
    localStorage.setItem("nexbox_sidebar_show_label", String(newValue));
    store.set("nexbox_sidebar_show_label", newValue).then(() => store.save());
    window.dispatchEvent(new CustomEvent("sidebar-show-label-changed", { detail: newValue }));
  };

  const handleNavItemToggle = (path: string) => {
    const key = `nexbox_nav_visible_${path.replace(/\//g, "").replace(/-/g, "_")}`;
    const newValue = !navVisibility[path];
    setNavVisibility(prev => ({ ...prev, [path]: newValue }));
    localStorage.setItem(key, String(newValue));
    store.set(key, newValue).then(() => store.save());
    window.dispatchEvent(new CustomEvent("nav-visibility-changed", { detail: { path, visible: newValue } }));
  };

  const handleNavDragEnd = useCallback((event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = navOrder.indexOf(active.id as string);
    const newIndex = navOrder.indexOf(over.id as string);
    if (oldIndex === -1 || newIndex === -1) return;
    const newOrder = arrayMove(navOrder, oldIndex, newIndex);
    setNavOrder(newOrder);
    const str = JSON.stringify(newOrder);
    localStorage.setItem(NAV_ORDER_KEY, str);
    store.set("nexbox_nav_order", str).then(() => store.save());
    window.dispatchEvent(new CustomEvent("nav-order-changed"));
  }, [navOrder]);

  const handleNavPositionChange = (value: string) => {
    const newValue = value as "left" | "top";
    setNavPosition(newValue);
    localStorage.setItem("nexbox_nav_position", newValue);
    store.set("nexbox_nav_position", newValue).then(() => store.save());
    window.dispatchEvent(new CustomEvent("nav-position-changed", { detail: newValue }));
  };

  const handleAutoStartToggle = () => {
    if (autoStartLoading) return;
    const newValue = !autoStart;
    setAutoStartLoading(true);

    // 关闭开机自启时，同步关闭"开机最小化启动"（最小化启动依赖自启）
    if (!newValue && minimizedStart) {
      setMinimizedStart(false);
      store
        .set("nexbox_minimized_start", false)
        .then(() => store.save())
        .catch(() => {});
    }

    invoke("set_nexbox_auto_start", { enable: newValue, minimizedStart: newValue ? minimizedStart : false })
      .then(() => {
        setAutoStart(newValue);
        toast({
          title: newValue ? t("settings.generalSettings.autoStartEnabled", "开机自启已开启") : t("settings.generalSettings.autoStartDisabled", "开机自启已关闭"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      })
      .catch((err) => {
        // Tauri v2 错误可能是 string 或 object
        const errMsg = typeof err === "string"
          ? err
          : (err && typeof err === "object" && "message" in err)
            ? String((err as { message: unknown }).message)
            : String(err);
        toast({
          title: t("settings.generalSettings.autoStartError", "开机自启设置失败"),
          description: errMsg || t("settings.generalSettings.autoStartErrorHint", "请尝试以管理员身份运行后再试"),
          status: "error",
          duration: 6000,
          isClosable: true,
        });
      })
      .finally(() => {
        setAutoStartLoading(false);
      });
  };

  // 切换"开机最小化启动"：保存设置；若自启已开启，则重写 Run 键以应用/移除 --autostart
  const handleMinimizedStartToggle = () => {
    const newValue = !minimizedStart;
    setMinimizedStart(newValue);
    store
      .set("nexbox_minimized_start", newValue)
      .then(() => store.save())
      .catch(() => {});

    if (autoStart) {
      invoke("set_nexbox_auto_start", { enable: true, minimizedStart: newValue })
        .catch((err) => {
          const errMsg = typeof err === "string"
            ? err
            : (err && typeof err === "object" && "message" in err)
              ? String((err as { message: unknown }).message)
              : String(err);
          toast({
            title: t("settings.generalSettings.autoStartError", "开机自启设置失败"),
            description: errMsg,
            status: "error",
            duration: 4000,
            isClosable: true,
          });
        });
    }

    toast({
      title: newValue
        ? t("settings.generalSettings.minimizedStartEnabled", "开机最小化启动已开启")
        : t("settings.generalSettings.minimizedStartDisabled", "开机最小化启动已关闭"),
      status: "success",
      duration: 2000,
      isClosable: true,
    });
  };

  const handlePageTransitionChange = (newMode: "slide" | "fade" | "off") => {
    setPageTransitionMode(newMode);
    localStorage.setItem("nexbox_page_transition", newMode);
    store.set("nexbox_page_transition", newMode).then(() => store.save());
    window.dispatchEvent(new CustomEvent("page-transition-setting-changed", { detail: newMode }));
  };

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("settings.generalSettings.title")}
      </Text>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.generalSettings.startup")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between" py={2}>
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.generalSettings.autoStartLabel")}
              </Text>
            </Box>
            <ThemeSwitch
              size="md"
              isChecked={autoStart}
              onChange={handleAutoStartToggle}
              isDisabled={autoStartLoading}
            />
          </HStack>
          <HStack justify="space-between" py={2}>
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.generalSettings.minimizedStartLabel", "开机最小化启动")}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={1}>
                {t("settings.generalSettings.minimizedStartDesc", "开机时不显示主窗口，仅后台运行并保留托盘")}
              </Text>
            </Box>
            <ThemeSwitch
              size="md"
              isChecked={minimizedStart}
              onChange={handleMinimizedStartToggle}
              isDisabled={autoStartLoading || !autoStart}
            />
          </HStack>
          <HStack justify="space-between" py={2}>
            <Text fontSize="sm" color={labelColor} fontWeight="medium">
              {t("settings.generalSettings.autoUpdate")}
            </Text>
            <ThemeSwitch
              size="md"
              isChecked={autoUpdateEnabled}
              onChange={(e) => setAutoUpdateEnabled(e.target.checked)}
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.generalSettings.language")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between">
            <Text fontSize="sm" color={labelColor}>
              {t("settings.generalSettings.languageLabel")}
            </Text>
            <CustomSelect
              value={language}
              onChange={handleLanguageChange}
              options={[
                { value: "zh", label: "简体中文" },
                { value: "zh-TW", label: "繁體中文" },
                { value: "en", label: "English" },
                { value: "fr", label: "Français" },
                { value: "ja", label: "日本語" },
                { value: "de", label: "Deutsch" },
              ]}
              width="180px"
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.generalSettings.homepage")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <SimpleGrid columns={{ base: 1, md: 2 }} spacing={4}>
            {/* 左上角：问候语与顶部信息卡片 */}
            <Box>
              <Text fontSize="xs" fontWeight="semibold" color={subLabelColor} mb={2} textTransform="uppercase" letterSpacing="0.05em">
                {t("settings.generalSettings.cornerTopLeft")}
              </Text>
              <VStack spacing={0} align="stretch">
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1} pr={3}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.homeUsernameLabel")}
                    </Text>
                  </Box>
                  <HStack spacing={2}>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={handleHomeUsernameReset}
                      leftIcon={<LuRotateCcw size={13} />}
                      color={subLabelColor}
                      _hover={{ color: "red.400" }}
                      isDisabled={!homeUsername}
                      flexShrink={0}
                    >
                      {t("settings.generalSettings.homeUsernameReset")}
                    </Button>
                    <Input
                      value={homeUsername}
                      onChange={handleHomeUsernameChange}
                      placeholder={t("settings.generalSettings.homeUsernamePlaceholder")}
                      size="sm"
                      pl={3}
                      pr={3}
                      h="34px"
                      width="160px"
                      flexShrink={0}
                      borderRadius="lg"
                      bg={inputBg}
                      border="1px solid"
                      borderColor={hexToRgba(config.primaryColor, 0.4)}
                      _hover={{ borderColor: hexToRgba(config.primaryColor, 0.7) }}
                      _focus={{
                        borderColor: config.primaryColor,
                        boxShadow: `0 0 0 3px ${hexToRgba(config.primaryColor, 0.22)}`,
                      }}
                      transition="all 0.2s"
                    />
                  </HStack>
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.todayPopularityLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={todayPopularityEnabled}
                    onChange={handleTodayPopularityToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.announcementLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={announcementEnabled}
                    onChange={handleAnnouncementToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.randomQuoteLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={randomQuoteEnabled}
                    onChange={handleRandomQuoteToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.searchBarLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={searchBarEnabled}
                    onChange={handleSearchBarToggle}
                  />
                </HStack>
              </VStack>
            </Box>
            {/* 左下角：底部左侧快捷卡片 */}
            <Box>
              <Text fontSize="xs" fontWeight="semibold" color={subLabelColor} mb={2} textTransform="uppercase" letterSpacing="0.05em">
                {t("settings.generalSettings.cornerBottomLeft")}
              </Text>
              <VStack spacing={0} align="stretch">
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.randomImageLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={randomImageEnabled}
                    onChange={handleRandomImageToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.moodCardLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={moodCardEnabled}
                    onChange={handleMoodCardToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.gameWinKeyLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={gameWinKeyCardEnabled}
                    onChange={handleGameWinKeyCardToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.homeHardwareModelLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={homeHardwareModelEnabled}
                    onChange={handleHomeHardwareModelToggle}
                  />
                </HStack>
              </VStack>
            </Box>
            {/* 右上角：右侧信息卡片 */}
            <Box>
              <Text fontSize="xs" fontWeight="semibold" color={subLabelColor} mb={2} textTransform="uppercase" letterSpacing="0.05em">
                {t("settings.generalSettings.cornerTopRight")}
              </Text>
              <VStack spacing={0} align="stretch">
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.gameModeLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={gameModeEnabled}
                    onChange={handleGameModeToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.feedbackLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={feedbackEnabled}
                    onChange={handleFeedbackToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.qqGroupCardLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={qqGroupCardEnabled}
                    onChange={handleQqGroupCardToggle}
                  />
                </HStack>
                <Divider />
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.docsCardLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={docsCardEnabled}
                    onChange={handleDocsCardToggle}
                  />
                </HStack>
              </VStack>
            </Box>
            {/* 右下角：底部右侧快捷启动 */}
            <Box>
              <Text fontSize="xs" fontWeight="semibold" color={subLabelColor} mb={2} textTransform="uppercase" letterSpacing="0.05em">
                {t("settings.generalSettings.cornerBottomRight")}
              </Text>
              <VStack spacing={0} align="stretch">
                <HStack justify="space-between" py={2} minH="50px" align="center">
                  <Box flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {t("settings.generalSettings.gameLauncherLabel")}
                    </Text>
                  </Box>
                  <ThemeSwitch
                    size="md"
                    isChecked={gameLauncherEnabled}
                    onChange={handleGameLauncherToggle}
                  />
                </HStack>
              </VStack>
            </Box>
          </SimpleGrid>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.generalSettings.splash")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <VStack spacing={0} align="stretch">
            <HStack justify="space-between" align="center" py={2}>
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.generalSettings.splashLogoLabel")}
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.generalSettings.splashLogoDesc")}
                </Text>
                {splashLogo && (
                  <Button
                    size="xs"
                    variant="ghost"
                    mt={1.5}
                    onClick={handleSplashLogoReset}
                    leftIcon={<LuX size={12} />}
                    color={subLabelColor}
                    _hover={{ color: "red.400" }}
                  >
                    {t("settings.generalSettings.splashLogoReset")}
                  </Button>
                )}
              </Box>
              <Box
                w="48px"
                h="48px"
                borderRadius="md"
                overflow="hidden"
                border="1px solid"
                borderColor={cardBorder}
                cursor="pointer"
                onClick={() => {
                  const input = document.getElementById("splash-logo-upload") as HTMLInputElement;
                  input?.click();
                }}
                _hover={{ borderColor: splashLogoHoverBorder }}
                transition="all 0.2s"
                flexShrink={0}
              >
                <img
                  src={splashLogo || "/logo/Chinesew.png"}
                  alt="Splash Logo"
                  style={{ width: "100%", height: "100%", objectFit: "contain" }}
                />
              </Box>
            </HStack>
            <Divider />
            <HStack justify="space-between" py={2}>
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.generalSettings.pageTransitionLabel")}
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.generalSettings.pageTransitionDesc")}
                </Text>
              </Box>
              <HStack
                spacing={1}
                p={1}
                borderRadius="xl"
                border="1px solid"
                borderColor={segmentedControlBorder}
                bg={segmentedControlBg}
                boxShadow={segmentedControlShadow}
                backdropFilter={liquidGlassEnabled ? `blur(${effectiveBlur}px) saturate(160%)` : "blur(14px)"}
                transition="backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
                position="relative"
                overflow="hidden"
              >
                {liquidGlassEnabled && (
                  <Box
                    position="absolute"
                    inset="0"
                    pointerEvents="none"
                    bgGradient={segmentedGlassSheen}
                    opacity={0.9}
                  />
                )}
                <Box
                  position="absolute"
                  inset="0"
                  pointerEvents="none"
                  borderRadius="inherit"
                  boxShadow={`inset 0 1px 0 rgba(255,255,255,0.28), inset 0 0 0 1px ${segmentedContainerGlow}`}
                  opacity={liquidGlassEnabled ? 1 : 0.72}
                />
                {(["slide", "fade", "off"] as const).map((mode) => (
                  <Box
                    key={mode}
                    as="button"
                    type="button"
                    minW="74px"
                    px={3.5}
                    py={2}
                    borderRadius="lg"
                    border="1px solid"
                    borderColor={pageTransitionMode === mode ? segmentedActiveBorder : "transparent"}
                    bg={pageTransitionMode === mode ? segmentedActiveBg : "transparent"}
                    color={pageTransitionMode === mode ? segmentedActiveText : subLabelColor}
                    fontSize="sm"
                    fontWeight={pageTransitionMode === mode ? "semibold" : "medium"}
                    letterSpacing="0.01em"
                    boxShadow={pageTransitionMode === mode ? segmentedActiveShadow : "none"}
                    position="relative"
                    transition="color 0.16s ease, transform 0.16s ease"
                    transform={pageTransitionMode === mode ? "translateY(-1px)" : "translateY(0)"}
                    _hover={{
                      bg: pageTransitionMode === mode ? segmentedActiveBg : segmentedControlHoverBg,
                      color: pageTransitionMode === mode ? segmentedActiveText : labelColor,
                    }}
                    _active={{
                      transform: pageTransitionMode === mode ? "translateY(0)" : "scale(0.98)",
                    }}
                    _focusVisible={{
                      outline: "none",
                      boxShadow: `0 0 0 3px ${hexToRgba(config.primaryColor, 0.24)}`,
                    }}
                    aria-pressed={pageTransitionMode === mode}
                    onClick={() => handlePageTransitionChange(mode)}
                  >
                    <Box
                      position="absolute"
                      inset="1px"
                      borderRadius="inherit"
                      opacity={pageTransitionMode === mode ? 1 : 0}
                      transition="none"
                      pointerEvents="none"
                      bgGradient={segmentedActiveOverlayGradient}
                    />
                    <Text position="relative" zIndex={1}>
                      {mode === "slide" ? t("settings.generalSettings.pageTransitionSlide", "滑动") :
                       mode === "fade" ? t("settings.generalSettings.pageTransitionFade", "淡化") :
                       t("settings.generalSettings.pageTransitionOff", "关闭")}
                    </Text>
                  </Box>
                ))}
              </HStack>
            </HStack>
          </VStack>
          <input
            id="splash-logo-upload"
            type="file"
            accept="image/*"
            style={{ display: "none" }}
            onChange={handleSplashLogoUpload}
          />
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.generalSettings.closeBehavior")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between" py={2}>
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.generalSettings.closeBehaviorLabel")}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("settings.generalSettings.closeBehaviorDesc")}
              </Text>
            </Box>
            <CustomSelect
              value={closeBehavior}
              onChange={handleCloseBehaviorChange}
              options={[
                { value: "close", label: t("settings.generalSettings.closeDirectly") },
                { value: "minimize", label: t("settings.generalSettings.minimizeToTray") },
              ]}
              width="140px"
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.navigation")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <VStack spacing={0} align="stretch">
            <HStack justify="space-between" py={2}>
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.generalSettings.navPositionLabel")}
                  <Badge
                    ml={1.5}
                    fontSize="0.6rem"
                    variant="subtle"
                    px={1.5}
                    py={0.5}
                    borderRadius="full"
                    color={config.primaryColor}
                    bg={hexToRgba(config.primaryColor, 0.15)}
                  >
                    BETA
                  </Badge>
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.generalSettings.navPositionDesc")}
                </Text>
              </Box>
              <CustomSelect
                value={navPosition}
                onChange={handleNavPositionChange}
                options={[
                  { value: "left", label: t("settings.generalSettings.navPositionLeft") },
                  { value: "top", label: t("settings.generalSettings.navPositionTop") },
                ]}
                width="100px"
              />
            </HStack>
            <Divider />
            <HStack justify="space-between" py={2}>
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.generalSettings.sidebarShowLabel")}
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.generalSettings.sidebarShowLabelDesc")}
                </Text>
              </Box>
              <CustomSelect
                value={String(sidebarShowLabel)}
                onChange={handleSidebarShowLabelChange}
                options={[
                  { value: "false", label: t("settings.generalSettings.sidebarShowLabelNoText") },
                  { value: "true", label: t("settings.generalSettings.sidebarShowLabelWithText") },
                ]}
                width="100px"
              />
            </HStack>
            <Divider />
            <DndContext sensors={navSensors} collisionDetection={closestCenter} onDragEnd={handleNavDragEnd} modifiers={[restrictToVerticalAxis]}>
              <SortableContext items={navOrder} strategy={verticalListSortingStrategy}>
                {navOrder.map((path) => {
                  const labelMap: Record<string, string> = {
                    "/hardware": t("sidebar.hardware"),
                    "/tools": t("sidebar.tools"),
                    "/builtin-tools": t("sidebar.builtinTools"),
                    "/optimization": t("sidebar.optimization"),
                    "/music": t("sidebar.music"),
                    "/delta-force": t("sidebar.deltaForce"),
                    "/steam": t("sidebar.steam"),
                    "/epic-free": t("sidebar.epicFree"),
                    "/custom": t("sidebar.custom"),
                  };
                  return (
                    <SortableNavItem
                      key={path}
                      id={path}
                      label={labelMap[path] || path}
                      visible={navVisibility[path] !== false}
                      onToggle={() => handleNavItemToggle(path)}
                      dragHandleColor={dragHandleColor}
                      labelColor={labelColor}
                      dragBg={sortableDragBg}
                    />
                  );
                })}
              </SortableContext>
            </DndContext>
          </VStack>
        </LiquidGlassCard>
      </Box>
    </Box>
  );
}

function ThemeColorSettings() {
  const { t } = useTranslation();
  const {
    config,
    setPrimaryColor,
    resetToDefault,
  } = useThemeColor();
  
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const presetBorderColor = useColorModeValue("gray.200", "#444444");
  const presetActiveBorderColor = useColorModeValue("gray.400", "#666666");
  
  const handlePresetClick = (color: string) => {
    setPrimaryColor(color);
  };
  
  return (
    <Box mb={6}>
      <Text
        fontSize="xs"
        fontWeight="semibold"
        color={subLabelColor}
        mb={3}
        textTransform="uppercase"
        letterSpacing="0.05em"
      >
        {t("settings.appearanceSettings.themeColor")}
      </Text>
      <LiquidGlassCard px={4} py={3} boxShadow="sm">
        <VStack spacing={4} align="stretch">
          <Box>
            <HStack mb={2}>
              <LuPalette size={14} />
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.appearanceSettings.presets")}
              </Text>
            </HStack>
            <HStack spacing={2} flexWrap="wrap">
              {PRESET_COLORS.map((preset) => (
                <Tooltip key={preset.value} label={t(preset.labelKey)}>
                  <Box
                    w="32px"
                    h="32px"
                    borderRadius="lg"
                    bg={preset.value}
                    cursor="pointer"
                    border="2px solid"
                    borderColor={config.primaryColor === preset.value ? presetActiveBorderColor : presetBorderColor}
                    boxShadow={config.primaryColor === preset.value ? "0 0 0 2px rgba(255,255,255,0.2)" : "none"}
                    onClick={() => handlePresetClick(preset.value)}
                    transition="all 0.2s"
                    _hover={{ transform: "scale(1.1)" }}
                  />
                </Tooltip>
              ))}
            </HStack>
          </Box>
          
          <Divider borderColor={cardBorder} />
          
          <Box>
            <Text fontSize="xs" color={subLabelColor} mb={2}>
              {t("settings.appearanceSettings.customColor")}
            </Text>
            <CustomColorPicker color={config.primaryColor} onChange={setPrimaryColor} />
          </Box>
          
          <HStack justify="flex-end">
            <Button
              size="xs"
              variant="ghost"
              onClick={resetToDefault}
            >
              {t("settings.appearanceSettings.resetToDefault")}
            </Button>
          </HStack>
        </VStack>
      </LiquidGlassCard>
    </Box>
  );
}

function AppearanceSettings() {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const { font, setFont, fontOptions, importCustomFont, removeCustomFont, importing } = useFont();
  const {
    backgroundMode,
    customBgImages,
    activeBgIndex,
    dynamicBgVideo,
    setBackgroundMode,
    addCustomBgImage,
    removeCustomBgImage,
    setActiveBgIndex,
    setDynamicBgVideo,
    liquidGlassEnabled,
    setLiquidGlassEnabled,
    liquidGlassBlur,
    setLiquidGlassBlur,
    liquidGlassMode,
    setLiquidGlassMode,
    islandLiquidGlassEnabled,
    setIslandLiquidGlassEnabled,
    backgroundBlur,
    setBackgroundBlur,
    activePresetIndex,
    presetBackgrounds,
    setActivePresetIndex,
    carouselEnabled,
    setCarouselEnabled,
    jellyBounceEnabled,
    setJellyBounceEnabled,
    mrColorMode,
    mrCustomColor,
    setMrColorMode,
    setMrCustomColor,
  } = useBackground();
  const videoPreviewSrc = useMemo(() => dynamicBgVideo ? convertFileSrc(dynamicBgVideo) : null, [dynamicBgVideo]);
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const emptySlotBg = useColorModeValue("gray.100", "#1a1a1a");
  const emptySlotBorder = useColorModeValue("gray.200", "#333333");
  const activeSlotBorder = useColorModeValue("blue.400", "blue.300");
  const modeButtonActiveBg = useColorModeValue("blue.500", "blue.400");
  const modeButtonActiveColor = useColorModeValue("gray.800", "white");
  const modeButtonInactiveBg = useColorModeValue("gray.100", "#1a1a1a");
  const modeButtonInactiveBorder = useColorModeValue("gray.200", "#333333");
  const glassSectionBorder = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)");
  const toast = useDynamicIsland("settings");

  const themeOptions = [
    { value: "system", label: "跟随系统" },
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
  ];

  const { themeMode, setThemeMode } = useThemeMode();

  const handleThemeChange = (value: string) => {
    setThemeMode(value as "system" | "light" | "dark");
  };

  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onloadend = () => {
        const result = reader.result as string;
        const success = addCustomBgImage(result);
        if (!success) {
          toast({
            title: t("settings.appearanceSettings.maxImagesReached") || "最多只能添加3张背景图片",
            status: "warning",
            duration: 2000,
            isClosable: true,
          });
        }
      };
      reader.readAsDataURL(file);
    }
    e.target.value = "";
  };

  const handleVideoUpload = async () => {
    try {
      const filePath = await invoke<string | null>("pick_video_file");
      if (filePath) {
        const ext = filePath.split(".").pop()?.toLowerCase();
        const supportedFormats = ["mp4", "webm"];
        if (ext && !supportedFormats.includes(ext)) {
          toast({
            title: t("settings.appearanceSettings.unsupportedVideoFormat"),
            status: "warning",
            duration: 3000,
            isClosable: true,
          });
          return;
        }
        setDynamicBgVideo(filePath);
      }
    } catch (error) {
      console.error("选择视频文件失败:", error);
    }
  };

  const handleAddClick = () => {
    if (customBgImages.length >= 3) {
      toast({
        title: t("settings.appearanceSettings.maxImagesReached") || "最多只能添加3张背景图片",
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    const input = document.getElementById("bg-upload-new") as HTMLInputElement;
    input?.click();
  };

  const handleVideoAddClick = () => {
    handleVideoUpload();
  };

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("settings.appearanceSettings.title")}
      </Text>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.appearanceSettings.theme")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between">
            <Text fontSize="sm" color={labelColor}>
              {t("settings.appearanceSettings.themeStyle")}
            </Text>
            <CustomSelect
              value={themeMode}
              onChange={handleThemeChange}
              options={themeOptions}
              width="140px"
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.appearanceSettings.font")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <VStack spacing={0} align="stretch">
            <HStack justify="space-between">
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.appearanceSettings.fontLabel")}
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.appearanceSettings.fontDesc")}
                </Text>
              </Box>
              <CustomSelect
                value={font}
                onChange={(val) => setFont(val)}
                options={fontOptions.map((opt) => ({
                  value: opt.value,
                  label: opt.label,
                }))}
                width="180px"
              />
            </HStack>
            <Divider />
            <HStack justify="space-between" py={2}>
              <Box flex={1}>
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  {t("settings.appearanceSettings.importCustomFontLabel")}
                </Text>
                <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                  {t("settings.appearanceSettings.importCustomFontDesc")}
                </Text>
              </Box>
              <HStack spacing={2}>
                {importing && <Text fontSize="xs" color={subLabelColor}>{t("settings.appearanceSettings.importingFont")}</Text>}
                <LiquidGlassButton
                  size="xs"
                  variant="outline"
                  borderRadius="md"
                  leftIcon={<LuUpload size={12} />}
                  isLoading={importing}
                  onClick={() => {
                    const input = document.getElementById("custom-font-upload") as HTMLInputElement;
                    input?.click();
                  }}
                >
                  {t("settings.appearanceSettings.importFontButton")}
                </LiquidGlassButton>
              </HStack>
            </HStack>
            {fontOptions.filter(f => f.isCustom).length > 0 && (
              <>
                <Divider />
                {fontOptions.filter(f => f.isCustom).map((cf) => (
                  <HStack key={cf.value} justify="space-between" py={2}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {cf.label}
                    </Text>
                    <HStack spacing={2}>
                      <Tooltip label={t("settings.appearanceSettings.removeFont")}>
                        <Box
                          w="24px"
                          h="24px"
                          borderRadius="md"
                          display="flex"
                          alignItems="center"
                          justifyContent="center"
                          cursor="pointer"
                          transition="all 0.2s"
                          _hover={{ bg: "rgba(255,0,0,0.1)" }}
                          onClick={() => removeCustomFont(cf.value)}
                        >
                          <LuX size={14} color="red" />
                        </Box>
                      </Tooltip>
                    </HStack>
                  </HStack>
                ))}
              </>
            )}
          </VStack>
        </LiquidGlassCard>
        <input
          id="custom-font-upload"
          type="file"
          accept=".ttf,.otf,.woff,.woff2"
          style={{ display: "none" }}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (file) {
              try {
                await importCustomFont(file);
              } catch (err) {
                if (err instanceof Error && err.message === "DUPLICATE_FONT") {
                  toast({
                    title: t("settings.appearanceSettings.duplicateFont"),
                    status: "warning",
                    duration: 2000,
                    isClosable: true,
                  });
                } else {
                  toast({
                    title: t("settings.appearanceSettings.importFontFailed"),
                    status: "error",
                    duration: 2000,
                    isClosable: true,
                  });
                }
              }
            }
            e.target.value = "";
          }}
        />
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.appearanceSettings.liquidGlass")}
          <Badge
            ml={2}
            fontSize="0.6rem"
            colorScheme="purple"
            variant="subtle"
            px={2}
            py={0.5}
            borderRadius="full"
          >
            BETA
          </Badge>
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.appearanceSettings.liquidGlassLabel")}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("settings.appearanceSettings.liquidGlassDesc")}
              </Text>
            </Box>
            <ThemeSwitch
              size="md"
              isChecked={liquidGlassEnabled}
              onChange={() => setLiquidGlassEnabled(!liquidGlassEnabled)}
            />
          </HStack>
          {liquidGlassEnabled && (
            <Box mt={4} pt={3} borderTop="1px solid" borderColor={useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)")}>
              <HStack justify="space-between" mb={2}>
                <Text fontSize="sm" color={labelColor}>
                  {t("settings.appearanceSettings.liquidGlassModeLabel")}
                </Text>
                <CustomSelect
                  value={liquidGlassMode}
                  onChange={(val) => setLiquidGlassMode(val as "normal" | "real")}
                  width="160px"
                  options={[
                    { value: "normal", label: t("settings.appearanceSettings.liquidGlassModeNormal") },
                    { value: "real", label: t("settings.appearanceSettings.liquidGlassModeReal") },
                  ]}
                />
              </HStack>
              <Text fontSize="xs" color={subLabelColor} mt={1}>
                {liquidGlassMode === "real"
                  ? t("settings.appearanceSettings.liquidGlassModeRealDesc")
                  : t("settings.appearanceSettings.liquidGlassModeNormalDesc")}
              </Text>
            </Box>
          )}
          {liquidGlassEnabled && liquidGlassMode === "normal" && (
            <Box mt={4} pt={3} borderTop="1px solid" borderColor={useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)")}>
              <HStack justify="space-between" mb={2}>
                <Text fontSize="sm" color={labelColor}>
                  {t("settings.appearanceSettings.liquidGlassBlurLabel")}
                </Text>
                <Text fontSize="sm" color={getActiveColor()} fontWeight="bold">{liquidGlassBlur}</Text>
              </HStack>
              <Slider
                value={liquidGlassBlur}
                min={1}
                max={100}
                step={1}
                onChange={(val) => setLiquidGlassBlur(val)}
              >
                <SliderTrack bg={useColorModeValue("gray.200", "gray.700")}>
                  <SliderFilledTrack bg={getActiveColor()} />
                </SliderTrack>
                <SliderThumb />
              </Slider>
            </Box>
          )}
          {liquidGlassEnabled && (
            <Box mt={4} pt={3} borderTop="1px solid" borderColor={glassSectionBorder}>
              <HStack justify="space-between">
                <Box flex={1}>
                  <Text fontSize="sm" color={labelColor} fontWeight="medium">
                    {t("settings.appearanceSettings.islandLiquidGlassLabel")}
                  </Text>
                  <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                    {t("settings.appearanceSettings.islandLiquidGlassDesc")}
                  </Text>
                </Box>
                <ThemeSwitch
                  size="md"
                  isChecked={islandLiquidGlassEnabled}
                  onChange={() => setIslandLiquidGlassEnabled(!islandLiquidGlassEnabled)}
                />
              </HStack>
            </Box>
          )}
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.appearanceSettings.jellyBounce")}
          <Badge
            ml={2}
            fontSize="0.6rem"
            colorScheme="pink"
            variant="subtle"
            px={2}
            py={0.5}
            borderRadius="full"
          >
            BETA
          </Badge>
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.appearanceSettings.jellyBounceLabel")}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("settings.appearanceSettings.jellyBounceDesc")}
              </Text>
            </Box>
            <ThemeSwitch
              size="md"
              isChecked={jellyBounceEnabled}
              onChange={() => setJellyBounceEnabled(!jellyBounceEnabled)}
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <ThemeColorSettings />

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.appearanceSettings.customBackground")}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm">
          <VStack spacing={4} align="stretch">
            <HStack justify="space-between">
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.appearanceSettings.customBackgroundLabel")}
              </Text>
              <ThemeSwitch
                size="md"
                isChecked={backgroundMode !== "none"}
                onChange={() => {
                  if (backgroundMode === "none") {
                    setBackgroundMode("image");
                  } else {
                    setBackgroundMode("none");
                  }
                }}
              />
            </HStack>

            {backgroundMode !== "none" && (
              <>
                <Divider borderColor={cardBorder} />

                <HStack spacing={2}>
                  <LiquidGlassCard
                    flex={1}
                    px={3}
                    py={2}
                    cursor="pointer"
                    onClick={() => setBackgroundMode("preset")}
                    border="1px solid"
                    borderColor={backgroundMode === "preset" ? getActiveColor() : "transparent"}
                    bg={backgroundMode === "preset" ? `${getActiveColor()}15` : "transparent"}
                  >
                    <Text
                      fontSize="sm"
                      color={backgroundMode === "preset" ? modeButtonActiveColor : labelColor}
                      textAlign="center"
                      fontWeight="medium"
                    >
                      {t("settings.appearanceSettings.presetBackground")}
                    </Text>
                  </LiquidGlassCard>
                  <LiquidGlassCard
                    flex={1}
                    px={3}
                    py={2}
                    cursor="pointer"
                    onClick={() => setBackgroundMode("image")}
                    border="1px solid"
                    borderColor={backgroundMode === "image" ? getActiveColor() : "transparent"}
                    bg={backgroundMode === "image" ? `${getActiveColor()}15` : "transparent"}
                  >
                    <Text
                      fontSize="sm"
                      color={backgroundMode === "image" ? modeButtonActiveColor : labelColor}
                      textAlign="center"
                      fontWeight="medium"
                    >
                      {t("settings.appearanceSettings.imageBackground")}
                    </Text>
                  </LiquidGlassCard>
                  <LiquidGlassCard
                    flex={1}
                    px={3}
                    py={2}
                    cursor="pointer"
                    onClick={() => setBackgroundMode("dynamic")}
                    border="1px solid"
                    borderColor={backgroundMode === "dynamic" ? getActiveColor() : "transparent"}
                    bg={backgroundMode === "dynamic" ? `${getActiveColor()}15` : "transparent"}
                  >
                    <Text
                      fontSize="sm"
                      color={backgroundMode === "dynamic" ? modeButtonActiveColor : labelColor}
                      textAlign="center"
                      fontWeight="medium"
                    >
                      {t("settings.appearanceSettings.dynamicBackground")}
                    </Text>
                  </LiquidGlassCard>
                  <LiquidGlassCard
                    flex={1}
                    px={3}
                    py={2}
                    cursor="pointer"
                    onClick={() => setBackgroundMode("mr")}
                    border="1px solid"
                    borderColor={backgroundMode === "mr" ? getActiveColor() : "transparent"}
                    bg={backgroundMode === "mr" ? `${getActiveColor()}15` : "transparent"}
                  >
                    <Text
                      fontSize="sm"
                      color={backgroundMode === "mr" ? modeButtonActiveColor : labelColor}
                      textAlign="center"
                      fontWeight="medium"
                    >
                      {t("settings.appearanceSettings.mrBackground")}
                    </Text>
                  </LiquidGlassCard>
                </HStack>

                {backgroundMode === "preset" && (
                  <>
                    <HStack justify="space-between" w="full">
                      <Box>
                        <Text fontSize="sm" color={labelColor} fontWeight="medium">
                          {t("settings.appearanceSettings.carouselLabel")}
                        </Text>
                        <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                          {t("settings.appearanceSettings.carouselDesc")}
                        </Text>
                      </Box>
                      <ThemeSwitch
                        size="md"
                        isChecked={carouselEnabled}
                        onChange={() => setCarouselEnabled(!carouselEnabled)}
                      />
                    </HStack>
                    <HStack spacing={2} justify="flex-end">
                      {presetBackgrounds.map((preset, index) => (
                        <Box
                          key={preset.id}
                          position="relative"
                          w="160px"
                          h="90px"
                          borderRadius="lg"
                          overflow="hidden"
                          border="2px solid"
                          borderColor={index === activePresetIndex ? activeSlotBorder : emptySlotBorder}
                          cursor="pointer"
                          onClick={() => setActivePresetIndex(index)}
                          transition="all 0.2s"
                          _hover={{ borderColor: activeSlotBorder, transform: "scale(1.02)" }}
                        >
                          <img
                            src={preset.path}
                            alt={preset.name}
                            style={{
                              width: "100%",
                              height: "100%",
                              objectFit: "cover",
                            }}
                          />
                          {index === activePresetIndex && (
                            <Box
                              position="absolute"
                              bottom={1}
                              left="50%"
                              transform="translateX(-50%)"
                              bg="blue.500"
                              borderRadius="full"
                              px={1.5}
                              py={0.5}
                            >
                              <LuCheck size={10} color="white" />
                            </Box>
                          )}
                        </Box>
                      ))}
                    </HStack>
                  </>
                )}

                {backgroundMode === "image" && (
                  <HStack spacing={2} justify="flex-end">
                    {[0, 1, 2].map((index) => (
                      <Box
                        key={index}
                        position="relative"
                        w="160px"
                        h="90px"
                        borderRadius="lg"
                        overflow="hidden"
                        border="2px solid"
                        borderColor={
                          customBgImages[index]
                            ? index === activeBgIndex
                              ? activeSlotBorder
                              : cardBorder
                            : emptySlotBorder
                        }
                        cursor={customBgImages[index] ? "pointer" : "default"}
                        onClick={() => {
                          if (customBgImages[index]) {
                            setActiveBgIndex(index);
                          }
                        }}
                        transition="all 0.2s"
                        _hover={
                          customBgImages[index]
                            ? { borderColor: activeSlotBorder, transform: "scale(1.02)" }
                            : {}
                        }
                      >
                        {customBgImages[index] ? (
                          <>
                            <img
                              src={customBgImages[index]}
                              alt={`Background ${index + 1}`}
                              style={{
                                width: "100%",
                                height: "100%",
                                objectFit: "cover",
                              }}
                            />
                            <Box
                              position="absolute"
                              top={1}
                              right={1}
                              onClick={(e) => {
                                e.stopPropagation();
                                removeCustomBgImage(index);
                              }}
                              bg="blackAlpha.600"
                              borderRadius="full"
                              p={0.5}
                              cursor="pointer"
                              _hover={{ bg: "blackAlpha.800" }}
                              transition="all 0.2s"
                            >
                              <LuX size={12} color="white" />
                            </Box>
                            {index === activeBgIndex && (
                              <Box
                                position="absolute"
                                bottom={1}
                                left="50%"
                                transform="translateX(-50%)"
                                bg="blue.500"
                                borderRadius="full"
                                px={1.5}
                                py={0.5}
                              >
                                <LuCheck size={10} color="white" />
                              </Box>
                            )}
                          </>
                        ) : (
                          <Flex
                            w="100%"
                            h="100%"
                            bg={emptySlotBg}
                            align="center"
                            justify="center"
                            cursor="pointer"
                            onClick={handleAddClick}
                            _hover={{ bg: useColorModeValue("gray.200", "#222222") }}
                            transition="all 0.2s"
                          >
                            <LuUpload size={18} color={subLabelColor} />
                          </Flex>
                        )}
                      </Box>
                    ))}
                    <input
                      id="bg-upload-new"
                      type="file"
                      accept="image/*"
                      style={{ display: "none" }}
                      onChange={handleImageUpload}
                    />
                  </HStack>
                )}

                {backgroundMode === "dynamic" && (
                  <HStack spacing={2} justify="flex-end">
                    <Box
                      position="relative"
                      w="160px"
                      h="90px"
                      borderRadius="lg"
                      overflow="hidden"
                      border="2px solid"
                      borderColor={dynamicBgVideo ? activeSlotBorder : emptySlotBorder}
                    >
                      {dynamicBgVideo ? (
                        <>
                          <video
                            src={videoPreviewSrc!}
                            style={{
                              width: "100%",
                              height: "100%",
                              objectFit: "cover",
                            }}
                            muted
                            loop
                            autoPlay
                          />
                          <Box
                            position="absolute"
                            top={1}
                            right={1}
                            onClick={() => setDynamicBgVideo(null)}
                            bg="blackAlpha.600"
                            borderRadius="full"
                            p={0.5}
                            cursor="pointer"
                            _hover={{ bg: "blackAlpha.800" }}
                            transition="all 0.2s"
                          >
                            <LuX size={12} color="white" />
                          </Box>
                        </>
                      ) : (
                        <Flex
                          w="100%"
                          h="100%"
                          bg={emptySlotBg}
                          align="center"
                          justify="center"
                          cursor="pointer"
                          onClick={handleVideoAddClick}
                          _hover={{ bg: useColorModeValue("gray.200", "#222222") }}
                          transition="all 0.2s"
                          flexDirection="column"
                          gap={1}
                        >
                          <LuUpload size={20} color={subLabelColor} />
                          <Text fontSize="xs" color={subLabelColor}>
                            {t("settings.appearanceSettings.uploadVideo")}
                          </Text>
                        </Flex>
                      )}
                    </Box>
                  </HStack>
                )}

                {backgroundMode === "mr" && (
                  <VStack spacing={3} align="stretch">
                    <LiquidGlassCard px={4} py={3}>
                      <HStack spacing={3}>
                        <Box
                          w="160px"
                          h="90px"
                          borderRadius="lg"
                          overflow="hidden"
                          flexShrink={0}
                          position="relative"
                        >
                          <img
                            src="/logo/MR.png"
                            alt="MR"
                            style={{
                              width: "100%",
                              height: "100%",
                              objectFit: "cover",
                              display: "block",
                            }}
                          />
                        </Box>
                        <Box flex={1}>
                          <Text fontSize="sm" color={labelColor} fontWeight="medium">
                            {t("settings.appearanceSettings.mrBackground")}
                          </Text>
                          <Text fontSize="xs" color={subLabelColor} mt={1}>
                            {t("settings.appearanceSettings.mrBackgroundDesc")}
                          </Text>
                        </Box>
                      </HStack>
                    </LiquidGlassCard>

                    <LiquidGlassCard px={4} py={3}>
                      <VStack spacing={3} align="stretch">
                        <HStack spacing={3}>
                          <Box
                            as="button"
                            flex={1}
                            px={3}
                            py={2}
                            borderRadius="lg"
                            border="2px solid"
                            borderColor={mrColorMode === "theme" ? getActiveColor() : cardBorder}
                            bg={mrColorMode === "theme" ? `${getActiveColor()}15` : "transparent"}
                            onClick={() => setMrColorMode("theme")}
                            textAlign="center"
                            cursor="pointer"
                            transition="all 0.2s"
                            _hover={{ borderColor: getActiveColor() }}
                          >
                            <Text fontSize="sm" fontWeight="medium" color={labelColor}>
                              {t("settings.appearanceSettings.mrColorTheme")}
                            </Text>
                            <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                              {t("settings.appearanceSettings.mrColorThemeDesc")}
                            </Text>
                          </Box>
                          <Box
                            as="button"
                            flex={1}
                            px={3}
                            py={2}
                            borderRadius="lg"
                            border="2px solid"
                            borderColor={mrColorMode === "custom" ? getActiveColor() : cardBorder}
                            bg={mrColorMode === "custom" ? `${getActiveColor()}15` : "transparent"}
                            onClick={() => setMrColorMode("custom")}
                            textAlign="center"
                            cursor="pointer"
                            transition="all 0.2s"
                            _hover={{ borderColor: getActiveColor() }}
                          >
                            <Text fontSize="sm" fontWeight="medium" color={labelColor}>
                              {t("settings.appearanceSettings.mrColorCustom")}
                            </Text>
                            <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                              {t("settings.appearanceSettings.mrColorCustomDesc")}
                            </Text>
                          </Box>
                        </HStack>
                        {mrColorMode === "custom" && (
                          <HStack justify="flex-end">
                            <CustomColorPicker color={mrCustomColor} onChange={setMrCustomColor} />
                          </HStack>
                        )}
                      </VStack>
                    </LiquidGlassCard>
                  </VStack>
                )}
              </>
            )}

            <Divider borderColor={cardBorder} />
            <HStack justify="space-between">
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("settings.appearanceSettings.backgroundBlurLabel")}
              </Text>
              <Text fontSize="sm" color={getActiveColor()} fontWeight="bold">{backgroundBlur}px</Text>
            </HStack>
            <Slider
              value={backgroundBlur}
              min={0}
              max={30}
              step={1}
              onChange={(val) => setBackgroundBlur(val)}
            >
              <SliderTrack bg={useColorModeValue("gray.200", "gray.700")}>
                <SliderFilledTrack bg={getActiveColor()} />
              </SliderTrack>
              <SliderThumb />
            </Slider>
            <Text fontSize="xs" color={subLabelColor}>
              {t("settings.appearanceSettings.backgroundBlurHint")}
            </Text>
          </VStack>
        </LiquidGlassCard>
      </Box>
    </Box>
  );
}

function NetworkSettings() {
  const { t } = useTranslation();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const inputBg = useColorModeValue("white", "#1a1a1a");
  const inputBorder = useColorModeValue("gray.200", "#333333");

  const presetServers = [
    { id: "baidu", url: "https://www.baidu.com/img/flexible/logo/pc/result.png" },
    { id: "gitcode", url: "https://gitcode.com/favicon.ico" },
    { id: "github", url: "https://github.githubassets.com/favicons/favicon-dark.svg" },
    { id: "qq", url: "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='6' fill='%2312B7F5'/%3E%3Ctext x='16' y='22' text-anchor='middle' fill='white' font-size='14' font-weight='bold' font-family='Arial,sans-serif'%3EQQ%3C/text%3E%3C/svg%3E" },
    { id: "aliyun", url: "https://img.alicdn.com/tfs/TB1_ZXuNcfpK1RjSZFOXXa6nFXa-32-32.ico" },
    { id: "wangyi", url: "https://www.163.com/favicon.ico" },
    { id: "bilibili", url: "https://www.bilibili.com/favicon.ico" },
    { id: "douyin", url: "https://www.douyin.com/favicon.ico" },
    { id: "jd", url: "https://www.jd.com/favicon.ico" },
    { id: "zhihu", url: "https://www.zhihu.com/favicon.ico" },
  ];

  const CUSTOM_SERVERS_KEY = "nexbox_network_custom_servers";
  const [customServers, setCustomServers] = useState<{ id: string; hostname: string; url: string }[]>(() => {
    try {
      const saved = localStorage.getItem(CUSTOM_SERVERS_KEY);
      return saved ? JSON.parse(saved) : [];
    } catch { return []; }
  });
  const [newServerInput, setNewServerInput] = useState("");

  const allServers: { id: string; url: string; hostname?: string }[] = useMemo(
    () => [...presetServers, ...customServers],
    [customServers],
  );

  const [latencies, setLatencies] = useState<Record<string, number | null>>({});
  const [testing, setTesting] = useState<Record<string, boolean>>({});
  const [testingAll, setTestingAll] = useState(false);
  const [imgErrors, setImgErrors] = useState<Record<string, boolean>>({});

  const testLatency = async (serverId: string, serverUrl: string) => {
    setTesting((prev) => ({ ...prev, [serverId]: true }));
    const startTime = performance.now();
    try {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), 5000);

      await fetch(serverUrl, {
        mode: "no-cors",
        signal: controller.signal,
      });

      clearTimeout(timeoutId);
      const endTime = performance.now();
      const latency = Math.round(endTime - startTime);
      setLatencies((prev) => ({ ...prev, [serverId]: latency }));
    } catch {
      const endTime = performance.now();
      const elapsed = Math.round(endTime - startTime);
      if (elapsed >= 5000) {
        setLatencies((prev) => ({ ...prev, [serverId]: -1 }));
      } else {
        setLatencies((prev) => ({ ...prev, [serverId]: -2 }));
      }
    } finally {
      setTesting((prev) => ({ ...prev, [serverId]: false }));
    }
  };

  const testAll = async () => {
    setTestingAll(true);
    const promises = allServers.map((server) =>
      testLatency(server.id, server.url)
    );
    await Promise.all(promises);
    setTestingAll(false);
  };

  const getLatencyColor = (latency: number | null | undefined): string => {
    if (latency === undefined || latency === null) return subLabelColor;
    if (latency < 0) return "#e53e3e";
    if (latency < 100) return "#38a169";
    if (latency < 300) return "#d69e2e";
    return "#e53e3e";
  };

  const getLatencyText = (latency: number | null | undefined): string => {
    if (latency === undefined || latency === null) return "--";
    if (latency === -1) return t("settings.networkSettings.timeout");
    if (latency === -2) return t("settings.networkSettings.unreachable");
    return `${latency} ms`;
  };

  const addCustomServer = () => {
    const hostname = newServerInput.trim().replace(/^https?:\/\//, "").replace(/\/.*$/, "");
    if (!hostname) return;
    const newServer = {
      id: `custom-${Date.now()}`,
      hostname,
      url: `https://${hostname}/favicon.ico`,
    };
    const updated = [...customServers, newServer];
    setCustomServers(updated);
    localStorage.setItem(CUSTOM_SERVERS_KEY, JSON.stringify(updated));
    setNewServerInput("");
  };

  const removeCustomServer = (id: string) => {
    const updated = customServers.filter((s) => s.id !== id);
    setCustomServers(updated);
    localStorage.setItem(CUSTOM_SERVERS_KEY, JSON.stringify(updated));
    setLatencies((prev) => { const n = { ...prev }; delete n[id]; return n; });
  };

  const handleInputKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") addCustomServer();
  };

  return (
    <Box>
      <HStack mb={6} justify="space-between" align="center">
        <Box>
          <Text fontSize="lg" fontWeight="bold" color={titleColor}>
            {t("settings.networkSettings.title")}
          </Text>
          <Text fontSize="sm" color={subLabelColor} mt={1}>
            {t("settings.networkSettings.description")}
          </Text>
        </Box>
        <LiquidGlassButton
          size="sm"
          variant="solid"
          borderRadius="lg"
          onClick={testAll}
          isDisabled={testingAll}
          leftIcon={testingAll ? <LuRefreshCw className="animate-spin" size={14} /> : <LuGlobe size={14} />}
        >
          {testingAll ? t("settings.networkSettings.testingAll") : t("settings.networkSettings.testAll")}
        </LiquidGlassButton>
      </HStack>

      {/* 自定义服务器添加 */}
      <Box mb={5}>
        <HStack spacing={2}>
          <Input
            value={newServerInput}
            onChange={(e) => setNewServerInput(e.target.value)}
            onKeyDown={handleInputKeyDown}
            placeholder={t("settings.networkSettings.customServerPlaceholder")}
            size="sm"
            bg={inputBg}
            border="1px solid"
            borderColor={inputBorder}
            borderRadius="md"
            fontSize="sm"
            flex={1}
          />
          <LiquidGlassButton
            size="sm"
            variant="solid"
            borderRadius="md"
            onClick={addCustomServer}
            isDisabled={!newServerInput.trim()}
            leftIcon={<LuPlus size={14} />}
          >
            {t("settings.networkSettings.addServer")}
          </LiquidGlassButton>
        </HStack>
      </Box>

      <Box>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("settings.networkSettings.latency")}
        </Text>
        <VStack spacing={3} align="stretch">
          {allServers.map((server) => {
            const latency = latencies[server.id];
            const isTesting = testing[server.id];
            const latencyColor = getLatencyColor(latency);
            const isCustom = server.id.startsWith("custom-");

            return (
              <LiquidGlassCard key={server.id} px={4} py={3} boxShadow="sm">
                <HStack justify="space-between" align="center">
                  <HStack spacing={3}>
                    <Box
                      w="32px"
                      h="32px"
                      borderRadius="md"
                      overflow="hidden"
                      flexShrink={0}
                      bg={useColorModeValue("gray.50", "#1a1a1a")}
                      display="flex"
                      alignItems="center"
                      justifyContent="center"
                      position="relative"
                    >
                      {isCustom ? (
                        <Text fontSize="sm" fontWeight="bold" color={subLabelColor}>
                          {(server.hostname || "?").charAt(0).toUpperCase()}
                        </Text>
                      ) : !imgErrors[server.id] ? (
                        <img
                          src={server.url}
                          alt=""
                          style={{ width: "20px", height: "20px", objectFit: "contain" }}
                          onError={() => {
                            setImgErrors((prev) => ({ ...prev, [server.id]: true }));
                          }}
                        />
                      ) : (
                        <Text fontSize="sm" fontWeight="bold" color={subLabelColor}>
                          {t(`settings.networkSettings.servers.${server.id}`, "").charAt(0) || "?"}
                        </Text>
                      )}
                    </Box>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {isCustom ? (server.hostname || "") : t(`settings.networkSettings.servers.${server.id}`)}
                    </Text>
                    {isCustom && (
                      <IconButton
                        aria-label={t("settings.networkSettings.remove")}
                        icon={<LuTrash2 size={13} />}
                        size="xs"
                        variant="ghost"
                        color={subLabelColor}
                        _hover={{ color: "#e53e3e" }}
                        onClick={(e) => {
                          e.stopPropagation();
                          removeCustomServer(server.id);
                        }}
                      />
                    )}
                  </HStack>
                  <HStack spacing={3}>
                    <Text
                      fontSize="sm"
                      fontWeight="bold"
                      color={latencyColor}
                      minW="60px"
                      textAlign="right"
                    >
                      {getLatencyText(latency)}
                    </Text>
                    <LiquidGlassButton
                      size="xs"
                      variant="outline"
                      borderRadius="md"
                      onClick={() => testLatency(server.id, server.url)}
                      isDisabled={isTesting || testingAll}
                    >
                      {isTesting ? t("settings.networkSettings.testing") : t("settings.networkSettings.testButton")}
                    </LiquidGlassButton>
                  </HStack>
                </HStack>
              </LiquidGlassCard>
            );
          })}
        </VStack>
      </Box>
    </Box>
  );
 }

interface ContributorItem {
  name: string;
  avatar: string;
  role: string;
  bilibili: string;
  douyin: string;
}

const CONTRIBUTORS: ContributorItem[] = [
  {
    name: "刺客边风",
    avatar: "https://www.nexbox.top/gongxIan/ckbf.png",
    role: "视频推广",
    bilibili: "https://space.bilibili.com/21131684",
    douyin: "https://v.douyin.com/bJRAiesxhgk/",
  },
  {
    name: "资源汇社区",
    avatar: "https://www.nexbox.top/gongxIan/zyhsq.png",
    role: "视频推广",
    bilibili: "https://space.bilibili.com/175870152",
    douyin: "",
  },
  {
    name: "FreeDw资源库",
    avatar: "https://www.nexbox.top/gongxIan/freedw.png",
    role: "视频推广",
    bilibili: "https://space.bilibili.com/383210848",
    douyin: "",
  },
  {
    name: "风与诗的夏天",
    avatar: "https://www.nexbox.top/gongxIan/fysdxt.png",
    role: "视频推广",
    bilibili: "https://space.bilibili.com/1587687791",
    douyin: "",
  },
  {
    name: "宝藏收藏夹",
    avatar: "https://www.nexbox.top/gongxIan/bzscj.png",
    role: "视频推广",
    bilibili: "https://space.bilibili.com/3461565271509949",
    douyin: "",
  },
];

function ContributorSettings() {
  const { t } = useTranslation();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  // 优先展示远程最新名单，拉取失败时回退到内置硬编码名单
  const [contributors, setContributors] = useState<ContributorItem[]>(CONTRIBUTORS);
  const [contributorsLoading, setContributorsLoading] = useState(true);
  const [contributorsError, setContributorsError] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<ContributorItem[]>("get_contributors")
      .then((data) => {
        if (!cancelled && Array.isArray(data) && data.length > 0) {
          setContributors(data);
        }
      })
      .catch(() => {
        if (!cancelled) setContributorsError(true);
      })
      .finally(() => {
        if (!cancelled) setContributorsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const openUrl = (url: string) => {
    if (url) {
      window.open(url, "_blank");
    }
  };

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("settings.sponsorSettings.contributors.title")}
      </Text>

      {!contributorsLoading && contributorsError && (
        <Text fontSize="xs" color={subLabelColor} mb={4} textAlign="center">
          {t("settings.sponsorSettings.sponsorList.error")}
        </Text>
      )}
      {contributorsLoading ? (
        <Text fontSize="sm" color={subLabelColor} p={4} textAlign="center">
          {t("settings.sponsorSettings.sponsorList.loading")}
        </Text>
      ) : (
        <Box
          display="grid"
          gridTemplateColumns="repeat(auto-fill, 220px)"
          gap={4}
        >
        {contributors.map((contributor, index) => (
          <LiquidGlassCard
            key={index}
            p={4}
          >
            <Flex align="stretch" gap={3}>
              {/* 左侧：头像 + 名字 */}
              <VStack spacing={1} align="center" flexShrink={0} minW="60px">
                <Box
                  w="48px"
                  h="48px"
                  borderRadius="full"
                  overflow="hidden"
                  border="2px solid"
                  borderColor={cardBorder}
                >
                  <img
                    src={contributor.avatar}
                    alt={contributor.name}
                    style={{ width: "100%", height: "100%", objectFit: "cover" }}
                    onError={(e) => {
                      (e.target as HTMLImageElement).style.display = "none";
                    }}
                  />
                </Box>
                <Text
                  fontSize="xs"
                  fontWeight="bold"
                  color={labelColor}
                  textAlign="center"
                  lineHeight="1.2"
                >
                  {contributor.name}
                </Text>
              </VStack>

              {/* 右侧：说明 + 主页 */}
              <VStack align="flex-end" spacing={1.5} flex="1" justify="center" minW={0}>
                <Text fontSize="xs" color={subLabelColor} whiteSpace="nowrap">
                  {contributor.role}
                </Text>
                <Flex align="center" gap={2}>
                  {contributor.bilibili && (
                    <Tooltip label="Bilibili">
                      <Box
                        as="a"
                        href={contributor.bilibili}
                        target="_blank"
                        rel="noopener noreferrer"
                        color="#FB7299"
                        fontSize="18px"
                        display="flex"
                        alignItems="center"
                        _hover={{ opacity: 0.8 }}
                        cursor="pointer"
                        onClick={(e: React.MouseEvent) => {
                          e.preventDefault();
                          openUrl(contributor.bilibili);
                        }}
                      >
                        <RiBilibiliFill />
                      </Box>
                    </Tooltip>
                  )}
                  {contributor.douyin && (
                    <Tooltip label="抖音">
                      <Box
                        as="a"
                        href={contributor.douyin}
                        target="_blank"
                        rel="noopener noreferrer"
                        color={useColorModeValue("#111111", "#ffffff")}
                        fontSize="18px"
                        display="flex"
                        alignItems="center"
                        _hover={{ opacity: 0.8 }}
                        cursor="pointer"
                        onClick={(e: React.MouseEvent) => {
                          e.preventDefault();
                          openUrl(contributor.douyin);
                        }}
                      >
                        <RiTiktokFill />
                      </Box>
                    </Tooltip>
                  )}
                </Flex>
              </VStack>
            </Flex>
          </LiquidGlassCard>
        ))}
        </Box>
      )}
    </Box>
  );
}

interface SponsorItem {
  name: string;
  amount: string;
}

function SponsorSettings() {
  const { t } = useTranslation();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const [sponsors, setSponsors] = useState<SponsorItem[]>([]);
  const [totalAmount, setTotalAmount] = useState<string>("");
  const [sponsorsLoading, setSponsorsLoading] = useState(true);
  const [sponsorsError, setSponsorsError] = useState(false);

  // 赞助者列表可能很长（远程 sponsors.json），一次性渲染上百张卡片会阻塞主线程并导致滚动卡顿。
  // 这里先渲染前一批，再按帧分批追加剩余条目。
  const SPONSOR_CHUNK_SIZE = 24;

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    invoke<{ update_time: string; total_amount: string; list: SponsorItem[] }>(
      "get_sponsors",
    )
      .then((data) => {
        if (cancelled) return;
        setTotalAmount(data.total_amount ?? "");
        const list = data.list ?? [];
        setSponsors(list.slice(0, SPONSOR_CHUNK_SIZE));
        setSponsorsLoading(false);
        if (list.length > SPONSOR_CHUNK_SIZE) {
          let done = SPONSOR_CHUNK_SIZE;
          const step = () => {
            if (cancelled) return;
            done = Math.min(done + SPONSOR_CHUNK_SIZE, list.length);
            setSponsors(list.slice(0, done));
            if (done < list.length) {
              timer = setTimeout(step, 33);
            }
          };
          timer = setTimeout(step, 33);
        }
      })
      .catch(() => {
        if (!cancelled) setSponsorsError(true);
      })
      .finally(() => {
        if (!cancelled) setSponsorsLoading(false);
      });
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={2} color={titleColor}>
        {t("settings.sponsorSettings.title")}
      </Text>
      <Text fontSize="sm" color={subLabelColor} mb={6}>
        {t("settings.sponsorSettings.description")}
      </Text>

      <HStack spacing={6} align="stretch" justify="center">
        <LiquidGlassCard p={6} boxShadow="sm" textAlign="center" maxW="240px">
          <VStack spacing={4}>
            <Box
              w="180px"
              h="180px"
              borderRadius="xl"
              overflow="hidden"
              border="1px solid"
              borderColor={cardBorder}
              bg={useColorModeValue("white", "#1a1a1a")}
              display="flex"
              alignItems="center"
              justifyContent="center"
            >
              <img
                src="/sponsor/wechat.png"
                alt={t("settings.sponsorSettings.wechat")}
                style={{ width: "100%", height: "100%", objectFit: "contain" }}
                onError={(e) => {
                  const target = e.target as HTMLImageElement;
                  target.style.display = "none";
                  target.parentElement!.innerHTML = `<span style="color: ${subLabelColor}; font-size: 12px;">${t("settings.sponsorSettings.placeholder")}</span>`;
                }}
              />
            </Box>
            <Text fontSize="sm" fontWeight="medium" color={labelColor}>
              {t("settings.sponsorSettings.wechat")}
            </Text>
          </VStack>
        </LiquidGlassCard>

        <LiquidGlassCard p={6} boxShadow="sm" textAlign="center" maxW="240px">
          <VStack spacing={4}>
            <Box
              w="180px"
              h="180px"
              borderRadius="xl"
              overflow="hidden"
              border="1px solid"
              borderColor={cardBorder}
              bg={useColorModeValue("white", "#1a1a1a")}
              display="flex"
              alignItems="center"
              justifyContent="center"
            >
              <img
                src="/sponsor/alipay.png"
                alt={t("settings.sponsorSettings.alipay")}
                style={{ width: "100%", height: "100%", objectFit: "contain" }}
                onError={(e) => {
                  const target = e.target as HTMLImageElement;
                  target.style.display = "none";
                  target.parentElement!.innerHTML = `<span style="color: ${subLabelColor}; font-size: 12px;">${t("settings.sponsorSettings.placeholder")}</span>`;
                }}
              />
            </Box>
            <Text fontSize="sm" fontWeight="medium" color={labelColor}>
              {t("settings.sponsorSettings.alipay")}
            </Text>
          </VStack>
        </LiquidGlassCard>
      </HStack>

      <Text fontSize="sm" color={subLabelColor} mt={6} textAlign="center">
        {t("settings.sponsorSettings.thankYou")}
      </Text>

      <Box mt={8}>
        <Text fontSize="lg" fontWeight="bold" mb={4} color={titleColor}>
          {t("settings.sponsorSettings.sponsorList.title")}
        </Text>
        {totalAmount && (
          <HStack spacing={2} justify="center" mb={4}>
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.sponsorSettings.sponsorList.totalAmount")}
            </Text>
            <Box
              display="inline-block"
              px={3}
              py={1}
              borderRadius="lg"
              bg={getActiveColor()}
              color={getContrastTextColor()}
              fontSize="sm"
              fontWeight="bold"
            >
              ¥ {totalAmount}
            </Box>
          </HStack>
        )}
        {sponsorsLoading ? (
          <Text fontSize="sm" color={subLabelColor} p={4} textAlign="center">
            {t("settings.sponsorSettings.sponsorList.loading")}
          </Text>
        ) : sponsorsError ? (
          <Text fontSize="sm" color={subLabelColor} p={4} textAlign="center">
            {t("settings.sponsorSettings.sponsorList.error")}
          </Text>
        ) : sponsors.length === 0 ? (
          <Text fontSize="sm" color={subLabelColor} p={4} textAlign="center">
            {t("settings.sponsorSettings.sponsorList.empty")}
          </Text>
        ) : (
          <Flex flexWrap="wrap" gap={4} justify="center">
            {sponsors.map((sponsor, index) => (
              <LiquidGlassCard
                key={index}
                p={5}
                textAlign="center"
                minW="140px"
                flex="0 1 auto"
                // 列表可能很长：content-visibility 让浏览器跳过视口外卡片的渲染/绘制，
                // 只保留可见区的液态玻璃滤镜，滚动不卡且效果完整
                sx={{ contentVisibility: "auto", containIntrinsicSize: "96px" }}
              >
                <Text fontSize="md" fontWeight="medium" color={labelColor} mb={1}>
                  {sponsor.name}
                </Text>
                <Box
                  display="inline-block"
                  px={3}
                  py={1}
                  borderRadius="lg"
                  bg={getActiveColor()}
                  color={getContrastTextColor()}
                  fontSize="sm"
                  fontWeight="medium"
                >
                  {sponsor.amount}
                </Box>
              </LiquidGlassCard>
            ))}
          </Flex>
        )}
      </Box>
    </Box>
  );
}

function AboutSettings() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("settings");
  const { getActiveColor } = useThemeColor();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  const appNameColor = useColorModeValue("gray.400", "#ffffff");
  const graphicLogoSrc = useColorModeValue("/logo/NBB.png", "/logo/NBW.png");
  const textLogoSrc = useColorModeValue("/logo/CNBB.png", "/logo/CNBW.png");
  const changelogScrollColor = getActiveColor();

  const currentVersion = "9.5.2";
  const [currentRelease, setCurrentRelease] = useState<ReleaseInfo | null>(null);
  const [isLoadingChangelog, setIsLoadingChangelog] = useState(true);

  // 官方 QQ 群（从 gitee 配置获取，含内置兜底）
  const { groups: qqGroups, loading: loadingQQ } = useQQGroups();

  // LOGO 连续点击 5 次跳转原神官网的彩蛋
  // 用 useRef 同步计数，避免 React 异步 setState 在快速连点 5 次内计数不生效的问题
  const logoClickCountRef = useRef(0);
  const logoClickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const handleLogoClicks = () => {
    logoClickCountRef.current += 1;
    const remaining = 5 - logoClickCountRef.current;
    console.log("[AboutSettings] logo click count:", logoClickCountRef.current);
    toast({
      title: `彩蛋触发中 ${logoClickCountRef.current}/5`,
      description: `还差 ${remaining} 次`,
      status: "info",
      duration: 800,
      isClosable: false,
    });
    if (logoClickTimer.current) {
      clearTimeout(logoClickTimer.current);
    }
    logoClickTimer.current = setTimeout(() => {
      logoClickCountRef.current = 0;
    }, 2000);
    if (logoClickCountRef.current >= 5) {
      logoClickCountRef.current = 0;
      if (logoClickTimer.current) {
        clearTimeout(logoClickTimer.current);
      }
      console.log("[AboutSettings] logo clicked 5 times, opening browser...");
      handleOpenLink("https://ys.mihoyo.com/main/?from_fab=1");
    }
  };

  const {
    hasUpdate,
    isChecking,
    isDownloading,
    downloadProgress,
    isDownloadComplete,
    handleCheckUpdate,
    openModal,
  } = useUpdate();

  useEffect(() => {
    const fetchCurrentRelease = async () => {
      try {
        setIsLoadingChangelog(true);
        const release = await fetchReleaseByTag(`v${currentVersion}`);
        if (release) {
          setCurrentRelease(release);
        } else {
          // 尝试不带 v 前缀
          const releaseWithoutV = await fetchReleaseByTag(currentVersion);
          setCurrentRelease(releaseWithoutV);
        }
      } catch (error) {
        console.error("Failed to fetch current release:", error);
      } finally {
        setIsLoadingChangelog(false);
      }
    };

    fetchCurrentRelease();
  }, []);

  const handleOpenLink = async (url: string) => {
    console.log("[AboutSettings] handleOpenLink:", url);
    // 第一层：Rust 端 ShellExecuteW 系统浏览器打开（最可靠）
    try {
      await invoke("open_system_browser", { url });
      console.log("[AboutSettings] open_system_browser ok");
      return;
    } catch (error) {
      console.error("[AboutSettings] open_system_browser failed:", error);
    }
    // 第二层：plugin-opener 打开
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
      console.log("[AboutSettings] openUrl ok");
      return;
    } catch (error) {
      console.error("[AboutSettings] openUrl failed:", error);
    }
    // 第三层：window.open 兜底
    try {
      window.open(url, "_blank");
    } catch (fallbackError) {
      console.error("[AboutSettings] window.open failed:", fallbackError);
    }
  };

  // 加入/复制 QQ 群：有加群链接则打开，否则复制群号
  const onJoinGroup = (group: QqGroup) => {
    if (group.link) {
      handleOpenLink(group.link);
      return;
    }
    navigator.clipboard
      .writeText(group.number)
      .then(() =>
        toast({
          title: group.number,
          description: group.name,
          status: "success",
          duration: 1500,
          isClosable: false,
        })
      )
      .catch(() => {});
  };

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("settings.aboutSettings.title")}
      </Text>

      <LiquidGlassCard p={6} boxShadow="sm" mb={6}>
        <HStack
          spacing={4}
          justify="center"
          align="center"
          mb={4}
          cursor="pointer"
          userSelect="none"
          onClick={handleLogoClicks}
          _hover={{ opacity: 0.8 }}
        >
          <img
            src={graphicLogoSrc}
            alt="NexBox"
            style={{ height: "72px", width: "auto", objectFit: "contain", pointerEvents: "none" }}
          />
          <img
            src={textLogoSrc}
            alt="新境盒"
            style={{ height: "40px", width: "auto", objectFit: "contain", pointerEvents: "none" }}
          />
        </HStack>

        <Divider my={4} borderColor={dividerColor} />

        <Box>
          <HStack justify="space-between" mb={3}>
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.aboutSettings.version")}
            </Text>
            <HStack spacing={2}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                v{currentVersion}
              </Text>
              {isDownloading && !isDownloadComplete ? (
                <LiquidGlassButton
                  size="xs"
                  variant="solid"
                  borderRadius="lg"
                  colorScheme="teal"
                  fontVariantNumeric="tabular-nums"
                  onClick={openModal}
                  leftIcon={<LuDownload size={12} />}
                >
                  {t("settings.aboutSettings.downloadingWithProgress", {
                    progress: Math.round(downloadProgress),
                  })}
                </LiquidGlassButton>
              ) : isDownloadComplete ? (
                <LiquidGlassButton
                  size="xs"
                  variant="solid"
                  borderRadius="lg"
                  colorScheme="green"
                  onClick={openModal}
                  leftIcon={<LuRefreshCw size={12} />}
                >
                  {t("settings.aboutSettings.pendingInstall")}
                </LiquidGlassButton>
              ) : hasUpdate ? (
                <LiquidGlassButton
                  size="xs"
                  variant="solid"
                  borderRadius="lg"
                  colorScheme="orange"
                  onClick={openModal}
                  leftIcon={<LuDownload size={12} />}
                >
                  {t("settings.aboutSettings.newVersion")}
                </LiquidGlassButton>
              ) : (
                <LiquidGlassButton
                  size="xs"
                  variant="solid"
                  borderRadius="lg"
                  colorScheme="teal"
                  onClick={handleCheckUpdate}
                  isDisabled={isChecking}
                  leftIcon={isChecking ? <LuRefreshCw className="animate-spin" size={12} /> : undefined}
                >
                  {isChecking ? t("settings.aboutSettings.check") + "..." : t("settings.aboutSettings.check")}
                </LiquidGlassButton>
              )}
            </HStack>
          </HStack>
          <HStack justify="space-between">
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.aboutSettings.author")}
            </Text>
            <HStack spacing={2}>
              <Tooltip label="Bilibili">
                <Box
                  w="24px"
                  h="24px"
                  borderRadius="md"
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  color="#00A1D6"
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ bg: "rgba(0, 161, 214, 0.1)", transform: "scale(1.1)" }}
                  onClick={() => handleOpenLink("https://space.bilibili.com/1614951812")}
                >
                  <RiBilibiliFill size={18} />
                </Box>
              </Tooltip>
              <Tooltip label="抖音">
                <Box
                  w="24px"
                  h="24px"
                  borderRadius="md"
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  color="#000000"
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ bg: "rgba(0, 0, 0, 0.1)", transform: "scale(1.1)" }}
                  onClick={() => handleOpenLink("https://www.douyin.com/user/MS4wLjABAAAAytD1zP6zVeXgPQuG-PWHq4AhsZz9zNXPcJap2JVaoG88Ani9tmBj0FtH7DLrQWsH")}
                >
                  <RiTiktokFill size={16} />
                </Box>
              </Tooltip>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                木流
              </Text>
            </HStack>
          </HStack>
          <Divider my={3} borderColor={dividerColor} />
          <HStack justify="space-between">
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.aboutSettings.feedbackTitle")}
            </Text>
            <HStack spacing={2}>
              <Tooltip label={t("settings.aboutSettings.feedbackTitle")}>
                <Box
                  w="24px"
                  h="24px"
                  borderRadius="md"
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  color={getActiveColor()}
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ transform: "scale(1.1)", bg: `${getActiveColor()}1a` }}
                  onClick={() => handleOpenLink("https://nexbox.top/feedback")}
                >
                  <LuBug size={16} />
                </Box>
              </Tooltip>
            </HStack>
          </HStack>
          <Divider my={3} borderColor={dividerColor} />
          <HStack justify="space-between">
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.aboutSettings.qqGroup")}
            </Text>
            <VStack spacing={1.5} align="flex-end">
              {loadingQQ && qqGroups.length === 0 ? (
                <Text fontSize="sm" color={labelColor} fontWeight="medium">
                  ...
                </Text>
              ) : (
                qqGroups.map((g) => (
                  <HStack spacing={2} key={g.number}>
                    {g.icon ? <QqGroupIcon url={g.icon} size={16} /> : null}
                    <Text fontSize="xs" color={subLabelColor}>
                      {g.name}
                    </Text>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium" userSelect="all">
                      {g.number}
                    </Text>
                    <Tooltip label={`${t("settings.aboutSettings.joinQqGroup")} ${g.name}`}>
                      <Box
                        w="20px"
                        h="20px"
                        borderRadius="md"
                        display="flex"
                        alignItems="center"
                        justifyContent="center"
                        color={getActiveColor()}
                        cursor="pointer"
                        transition="all 0.2s"
                        _hover={{ transform: "scale(1.15)", bg: `${getActiveColor()}1a` }}
                        onClick={() => onJoinGroup(g)}
                      >
                        <LuExternalLink size={14} />
                      </Box>
                    </Tooltip>
                  </HStack>
                ))
              )}
            </VStack>
          </HStack>
          <Divider my={3} borderColor={dividerColor} />
          <HStack justify="space-between">
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.aboutSettings.joinUs")}
            </Text>
            <HStack spacing={2}>
              <Tooltip label={t("settings.aboutSettings.joinUs")}>
                <Box
                  w="24px"
                  h="24px"
                  borderRadius="md"
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ transform: "scale(1.1)", bg: "rgba(99, 102, 241, 0.1)" }}
                  onClick={() => handleOpenLink("https://team.nexbox.top")}
                >
                  <LuExternalLink size={16} />
                </Box>
              </Tooltip>
            </HStack>
          </HStack>
        </Box>
      </LiquidGlassCard>

      <LiquidGlassCard p={6} boxShadow="sm" mb={6}>
        <Text fontSize="lg" fontWeight="bold" mb={4} color={titleColor}>
          {t("settings.aboutSettings.changelogTitle")}
        </Text>
        {isLoadingChangelog ? (
          <Box p={4} textAlign="center">
            <Text color={subLabelColor}>{t("settings.aboutSettings.loadingChangelog")}</Text>
          </Box>
        ) : currentRelease && currentRelease.body ? (
          <Box
            maxH="300px"
            overflowY="auto"
            sx={{
              scrollbarGutter: "stable",
              "&::-webkit-scrollbar": { width: "4px" },
              "&::-webkit-scrollbar-track": { background: "transparent" },
              "&::-webkit-scrollbar-thumb": {
                background: `${changelogScrollColor}88`,
                borderRadius: "2px",
              },
              "&::-webkit-scrollbar-thumb:hover": { background: changelogScrollColor },
            }}
          >
            <Text color={labelColor} fontSize="sm" whiteSpace="pre-wrap">
              {currentRelease.body}
            </Text>
          </Box>
        ) : (
          <Box p={4} textAlign="center">
            <Text color={subLabelColor}>{t("settings.aboutSettings.noChangelog")}</Text>
          </Box>
        )}
      </LiquidGlassCard>
    </Box>
  );
}

function HotkeySettings() {
  const { t } = useTranslation();
  const { overlayHotkey, saveOverlayHotkey, overlayHotkeyEnabled, saveOverlayHotkeyEnabled, crosshairHotkey, saveCrosshairHotkey, crosshairHotkeyEnabled, saveCrosshairHotkeyEnabled, filterHotkey, saveFilterHotkey, filterHotkeyEnabled, saveFilterHotkeyEnabled, autoclickerHotkey, saveAutoclickerHotkey, autoclickerHotkeyEnabled, saveAutoclickerHotkeyEnabled, musicPrevHotkey, saveMusicPrevHotkey, musicPrevHotkeyEnabled, saveMusicPrevHotkeyEnabled, musicNextHotkey, saveMusicNextHotkey, musicNextHotkeyEnabled, saveMusicNextHotkeyEnabled, musicPlayPauseHotkey, saveMusicPlayPauseHotkey, musicPlayPauseHotkeyEnabled, saveMusicPlayPauseHotkeyEnabled, hotkeysEnabled, saveHotkeysEnabled } = useAppStartup();
  const toast = useDynamicIsland("settings");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const { getHoverColor, getBorderColor } = useThemeColor();
  // 悬停主题色：背景为主题色半透明，边框为主题色
  const hotkeyHoverBg = getHoverColor();
  const hotkeyHoverBorder = getBorderColor();
  // 所有热键卡片共用的悬停样式
  // 注意：LiquidGlassCard 内部已有 transition，这里只传 _hover，避免覆盖 backdrop-filter 过渡
  const hotkeyCardHover = {
    _hover: {
      bg: hotkeyHoverBg,
      borderColor: hotkeyHoverBorder,
    },
  };

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("hotkeySettings.title") || "热键设置"}
      </Text>

      {/* 全部热键总开关 */}
      <Box mb={6}>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.masterToggle") || "全部热键总开关"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.masterToggleDesc") || "关闭后所有全局热键将不生效"}
              </Text>
            </Box>
            <ThemeSwitch
              isChecked={hotkeysEnabled}
              onChange={(e) => saveHotkeysEnabled(e.target.checked)}
              size="lg"
            />
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("hotkeySettings.overlay") || "悬浮框"}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.overlayToggle") || "切换悬浮框"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.overlayToggleDesc") || "使用快捷键显示或隐藏悬浮框"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={overlayHotkeyEnabled}
                onChange={(e) => saveOverlayHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={overlayHotkey}
                onChange={async (val) => {
                  const err = await saveOverlayHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("hotkeySettings.crosshair") || "准心"}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.crosshairToggle") || "切换准心"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.crosshairToggleDesc") || "使用快捷键显示或隐藏准心"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={crosshairHotkeyEnabled}
                onChange={(e) => saveCrosshairHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={crosshairHotkey}
                onChange={async (val) => {
                  const err = await saveCrosshairHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("hotkeySettings.filter") || "滤镜"}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.filterToggle") || "切换滤镜"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.filterToggleDesc") || "使用快捷键开启或关闭滤镜"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={filterHotkeyEnabled}
                onChange={(e) => saveFilterHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={filterHotkey}
                onChange={async (val) => {
                  const err = await saveFilterHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("hotkeySettings.autoclicker") || "连点器"}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.autoclickerToggle") || "连点器热键开关"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.autoclickerToggleDesc") || "使用快捷键开始或停止连点（支持中键、侧键）"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={autoclickerHotkeyEnabled}
                onChange={(e) => saveAutoclickerHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <MouseHotkeyRecorder
                value={autoclickerHotkey}
                onChange={async (val) => {
                  const err = await saveAutoclickerHotkey(val);
                  toast({
                    title: err
                      ? err
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
        </LiquidGlassCard>
      </Box>

      <Box mb={6}>
        <Text
          fontSize="xs"
          fontWeight="semibold"
          color={subLabelColor}
          mb={3}
          textTransform="uppercase"
          letterSpacing="0.05em"
        >
          {t("hotkeySettings.music") || "音乐"}
        </Text>
        <LiquidGlassCard px={4} py={3} boxShadow="sm" {...hotkeyCardHover}>
          <HStack justify="space-between" mb={4}>
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.musicPrev") || "上一曲"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.musicPrevDesc") || "使用快捷键切换到上一曲"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={musicPrevHotkeyEnabled}
                onChange={(e) => saveMusicPrevHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={musicPrevHotkey}
                onChange={async (val) => {
                  const err = await saveMusicPrevHotkey(val);
                  toast({
                    title: err
                      ? (t("hotkeySettings.saveFailed") || "快捷键保存失败")
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
          <HStack justify="space-between" mb={4}>
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.musicNext") || "下一曲"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.musicNextDesc") || "使用快捷键切换到下一曲"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={musicNextHotkeyEnabled}
                onChange={(e) => saveMusicNextHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={musicNextHotkey}
                onChange={async (val) => {
                  const err = await saveMusicNextHotkey(val);
                  toast({
                    title: err
                      ? (t("hotkeySettings.saveFailed") || "快捷键保存失败")
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
          <HStack justify="space-between">
            <Box flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="medium">
                {t("hotkeySettings.musicPlayPause") || "播放 / 暂停"}
              </Text>
              <Text fontSize="xs" color={subLabelColor} mt={0.5}>
                {t("hotkeySettings.musicPlayPauseDesc") || "使用快捷键播放或暂停音乐"}
              </Text>
            </Box>
            <HStack spacing={3} alignItems="center">
              <ThemeSwitch
                isChecked={musicPlayPauseHotkeyEnabled}
                onChange={(e) => saveMusicPlayPauseHotkeyEnabled(e.target.checked)}
                size="md"
              />
              <HotkeyRecorder
                value={musicPlayPauseHotkey}
                onChange={async (val) => {
                  const err = await saveMusicPlayPauseHotkey(val);
                  toast({
                    title: err
                      ? (t("hotkeySettings.saveFailed") || "快捷键保存失败")
                      : (t("hotkeySettings.saved") || "快捷键已保存"),
                    status: err ? "error" : "success",
                    duration: 2000,
                    isClosable: true,
                  });
                }}
              />
            </HStack>
          </HStack>
        </LiquidGlassCard>
      </Box>

    </Box>
  );
}

export default function SettingsPage() {
  const [activeItem, setActiveItem] = useState("general");
  const [searchParams] = useSearchParams();
  const { t } = useTranslation();
  const { config } = useThemeColor();
  const { jellyBounceEnabled } = useBackground();
  const isFirstRender = useRef(true);

  // 支持通过 URL ?section=xxx 定位子菜单（如托盘"检查更新"导航到 /settings?section=about）
  useEffect(() => {
    const section = searchParams.get("section");
    if (section && settingItems.some((item) => item.id === section)) {
      setActiveItem(section);
    }
  }, [searchParams]);

  // 子菜单切换时触发果冻弹跳动画（跳过首次挂载，避免与路由切换动画重复）
  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    if (!jellyBounceEnabled) return;
    document.body.classList.remove("jelly-bounce-active");
    void document.body.offsetWidth; // 强制 reflow
    document.body.classList.add("jelly-bounce-active");
    const timer = setTimeout(() => {
      document.body.classList.remove("jelly-bounce-active");
    }, 700);
    return () => {
      clearTimeout(timer);
      document.body.classList.remove("jelly-bounce-active");
    };
  }, [activeItem, jellyBounceEnabled]);

  return (
    <Flex gap={6} pt={8}>
      <Box w="180px" flexShrink={0} position="sticky" top={8} alignSelf="flex-start">
        <VStack spacing={0.5} align="stretch">
          {settingItems.map((item) => {
            const Icon = item.icon;
            const isActive = activeItem === item.id;

            return (
              <LiquidGlassMenuItem
                key={item.id}
                isActive={isActive}
                onClick={() => setActiveItem(item.id)}
                icon={item.icon}
              >
                {t(item.labelKey)}
              </LiquidGlassMenuItem>
            );
          })}
        </VStack>
      </Box>

      <Box flex={1}>
        {/* 直接渲染子菜单，不做 motion 动画：opacity/transform 动画会让 backdrop-filter 失效，
            导致液态玻璃卡片在切换时先透明、动画结束后瞬间出现模糊 */}
        <div key={activeItem} style={{ position: 'relative', zIndex: 1 }}>
          {activeItem === "general" && <GeneralSettings />}
          {activeItem === "appearance" && <AppearanceSettings />}
          {activeItem === "advanced" && <AdvancedPage />}
          {activeItem === "hotkeys" && <HotkeySettings />}
          {activeItem === "network" && <NetworkSettings />}
          {activeItem === "contributor" && <ContributorSettings />}
          {activeItem === "sponsor" && <SponsorSettings />}
          {activeItem === "about" && <AboutSettings />}
        </div>
      </Box>
    </Flex>
  );
}
