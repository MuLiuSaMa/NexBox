"use client";

import { createContext, useContext, useState, ReactNode, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { store } from "@/lib/store";
import { type HardwareInfo, getHardwareInfo } from "@/lib/hardware";

/** 从 invoke 错误中提取可读的中文提示，避免显示 [object Object] */
function extractError(error: unknown, fallback = "快捷键保存失败"): string {
  if (typeof error === "string") return error.trim() || fallback;
  if (error instanceof Error && error.message) return error.message;
  const msg = String(error);
  return msg && msg.trim() && msg !== "[object Object]" ? msg : fallback;
}

const DEFAULT_OVERLAY_HOTKEY = "Shift+F10";
const DEFAULT_CROSSHAIR_HOTKEY = "Shift+F9";
const DEFAULT_FILTER_HOTKEY = "Shift+F8";
const DEFAULT_AUTOCLICKER_HOTKEY = "F8";
const DEFAULT_MUSIC_PREV_HOTKEY = "Alt+[";
const DEFAULT_MUSIC_NEXT_HOTKEY = "Alt+]";
const DEFAULT_MUSIC_PLAYPAUSE_HOTKEY = "Alt+Space";

interface DisplayItem {
  id: string;
  label: string;
  enabled: boolean;
}

type DisplayItems = DisplayItem[];

interface CustomOverlayItem {
  id: string;
  text: string;
  color: string;
  enabled: boolean;
}

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
  offset_x: number;
  offset_y: number;
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
  _version?: number;
  position_x?: number | null;
  position_y?: number | null;
  vertical_position_x?: number | null;
  vertical_position_y?: number | null;
  delta_password_maps?: string[];
}

interface ThirdPartyTool {
  id: string;
  name: string;
  description: string;
  category: string;
  tool_type: string;
  download_url: string;
  file_name: string;
  website_url: string | null;
  check_executable: string | null;
}

interface ToolWithStatus {
  tool: ThirdPartyTool;
  installed: boolean;
}

interface AppStartupContextType {
  isStartupComplete: boolean;
  startupProgress: number;
  startupMessage: string;
  hardwareInfo: HardwareInfo | null;
  tools: ThirdPartyTool[];
  initTools: () => Promise<void>;
  overlaySettings: OverlaySettings | null;
  saveOverlaySettings: (settings: OverlaySettings) => Promise<void>;
  overlayHotkey: string;
  saveOverlayHotkey: (shortcut: string) => Promise<string | null>;
  crosshairHotkey: string;
  saveCrosshairHotkey: (shortcut: string) => Promise<string | null>;
  filterHotkey: string;
  saveFilterHotkey: (shortcut: string) => Promise<string | null>;
  autoclickerHotkey: string;
  saveAutoclickerHotkey: (shortcut: string) => Promise<string | null>;
  musicPrevHotkey: string;
  saveMusicPrevHotkey: (shortcut: string) => Promise<string | null>;
  musicNextHotkey: string;
  saveMusicNextHotkey: (shortcut: string) => Promise<string | null>;
  musicPlayPauseHotkey: string;
  saveMusicPlayPauseHotkey: (shortcut: string) => Promise<string | null>;
  lyricsBtnHotkey: string;
  saveLyricsBtnHotkey: (shortcut: string) => Promise<string | null>;
  hotkeysEnabled: boolean;
  saveHotkeysEnabled: (enabled: boolean) => Promise<void>;
  overlayHotkeyEnabled: boolean;
  saveOverlayHotkeyEnabled: (enabled: boolean) => Promise<void>;
  crosshairHotkeyEnabled: boolean;
  saveCrosshairHotkeyEnabled: (enabled: boolean) => Promise<void>;
  filterHotkeyEnabled: boolean;
  saveFilterHotkeyEnabled: (enabled: boolean) => Promise<void>;
  autoclickerHotkeyEnabled: boolean;
  saveAutoclickerHotkeyEnabled: (enabled: boolean) => Promise<void>;
  musicPrevHotkeyEnabled: boolean;
  saveMusicPrevHotkeyEnabled: (enabled: boolean) => Promise<void>;
  musicNextHotkeyEnabled: boolean;
  saveMusicNextHotkeyEnabled: (enabled: boolean) => Promise<void>;
  musicPlayPauseHotkeyEnabled: boolean;
  saveMusicPlayPauseHotkeyEnabled: (enabled: boolean) => Promise<void>;
}

const DEFAULT_OVERLAY_SETTINGS: OverlaySettings = {
  display_items: [
    { id: "time", label: "时间", enabled: false },
    { id: "fps", label: "FPS", enabled: true },
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
    { id: "game_ping", label: "游戏延迟", enabled: true },
    { id: "delta_password", label: "三角洲密码", enabled: false },
  ],
  custom_items: [],
  opacity: 200,
  style: "default",
  font: "Microsoft YaHei",
  font_size: 13,
  item_width: 130,
  font_color: "#ffffff",
  position_x: null,
  position_y: null,
  vertical_position_x: null,
  vertical_position_y: null,
  delta_password_maps: [],
};

// 悬浮框字体可选列表，与 OverlayPanelPage.tsx 的 BUILTIN_CHINESE_FONTS 保持一致。
// 用于加载设置时归一化：持久化数据里的字体不在列表中（例如旧默认 "MiSans"）时回退为微软雅黑，
// 避免字体下拉框显示为"未选中任何字体"。
const KNOWN_OVERLAY_FONTS = [
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

const AppStartupContext = createContext<AppStartupContextType>({
  isStartupComplete: false,
  startupProgress: 0,
  startupMessage: "正在启动...",
  hardwareInfo: null,
  tools: [],
  initTools: async () => {},
  overlaySettings: null,
  saveOverlaySettings: async () => {},
  overlayHotkey: DEFAULT_OVERLAY_HOTKEY,
  saveOverlayHotkey: async () => null,
  crosshairHotkey: DEFAULT_CROSSHAIR_HOTKEY,
  saveCrosshairHotkey: async () => null,
  filterHotkey: DEFAULT_FILTER_HOTKEY,
  saveFilterHotkey: async () => null,
  autoclickerHotkey: DEFAULT_AUTOCLICKER_HOTKEY,
  saveAutoclickerHotkey: async () => null,
  musicPrevHotkey: DEFAULT_MUSIC_PREV_HOTKEY,
  saveMusicPrevHotkey: async () => null,
  musicNextHotkey: DEFAULT_MUSIC_NEXT_HOTKEY,
  saveMusicNextHotkey: async () => null,
  musicPlayPauseHotkey: DEFAULT_MUSIC_PLAYPAUSE_HOTKEY,
  saveMusicPlayPauseHotkey: async () => null,
  lyricsBtnHotkey: "",
  saveLyricsBtnHotkey: async () => null,
  hotkeysEnabled: true,
  saveHotkeysEnabled: async () => {},
  overlayHotkeyEnabled: true,
  saveOverlayHotkeyEnabled: async () => {},
  crosshairHotkeyEnabled: true,
  saveCrosshairHotkeyEnabled: async () => {},
  filterHotkeyEnabled: true,
  saveFilterHotkeyEnabled: async () => {},
  autoclickerHotkeyEnabled: false,
  saveAutoclickerHotkeyEnabled: async () => {},
  musicPrevHotkeyEnabled: true,
  saveMusicPrevHotkeyEnabled: async () => {},
  musicNextHotkeyEnabled: true,
  saveMusicNextHotkeyEnabled: async () => {},
  musicPlayPauseHotkeyEnabled: true,
  saveMusicPlayPauseHotkeyEnabled: async () => {},
});

export function useAppStartup() {
  return useContext(AppStartupContext);
}

export function AppStartupProvider({ children }: { children: ReactNode }) {
  const [isStartupComplete, setIsStartupComplete] = useState(false);
  const [startupProgress, setStartupProgress] = useState(0);
  const [startupMessage, setStartupMessage] = useState("正在启动...");
  const [hardwareInfo, setHardwareInfo] = useState<HardwareInfo | null>(null);
  const [tools, setTools] = useState<ThirdPartyTool[]>([]);
  const [overlaySettings, setOverlaySettings] = useState<OverlaySettings | null>(null);
  const [overlayHotkey, setOverlayHotkey] = useState(DEFAULT_OVERLAY_HOTKEY);
  const [crosshairHotkey, setCrosshairHotkey] = useState(DEFAULT_CROSSHAIR_HOTKEY);
  const [filterHotkey, setFilterHotkey] = useState(DEFAULT_FILTER_HOTKEY);
  const [autoclickerHotkey, setAutoclickerHotkey] = useState(DEFAULT_AUTOCLICKER_HOTKEY);
  const [musicPrevHotkey, setMusicPrevHotkey] = useState(DEFAULT_MUSIC_PREV_HOTKEY);
  const [musicNextHotkey, setMusicNextHotkey] = useState(DEFAULT_MUSIC_NEXT_HOTKEY);
  const [musicPlayPauseHotkey, setMusicPlayPauseHotkey] = useState(DEFAULT_MUSIC_PLAYPAUSE_HOTKEY);
  const [lyricsBtnHotkey, setLyricsBtnHotkey] = useState("");
  const [hotkeysEnabled, setHotkeysEnabled] = useState(true);
  const [overlayHotkeyEnabled, setOverlayHotkeyEnabled] = useState(true);
  const [crosshairHotkeyEnabled, setCrosshairHotkeyEnabled] = useState(true);
  const [filterHotkeyEnabled, setFilterHotkeyEnabled] = useState(true);
  const [autoclickerHotkeyEnabled, setAutoclickerHotkeyEnabled] = useState(false);
  const [musicPrevHotkeyEnabled, setMusicPrevHotkeyEnabled] = useState(true);
  const [musicNextHotkeyEnabled, setMusicNextHotkeyEnabled] = useState(true);
  const [musicPlayPauseHotkeyEnabled, setMusicPlayPauseHotkeyEnabled] = useState(true);
  const hasStarted = useRef(false);
  // 最新的 overlay 设置（供事件监听器读取，避免闭包过期）
  const overlaySettingsRef = useRef<OverlaySettings | null>(null);
  // 用户是否已手动修改过 overlay 设置（用于避免启动加载在用户修改后才返回时覆盖用户编辑）
  const userEditedOverlayRef = useRef(false);
  // 最新竖排悬浮框位置（null = 尚未加载；{ x: null, y: null } = 已重置清除）
  const verticalPosRef = useRef<{ x: number | null; y: number | null } | null>(null);

  const updateProgress = (progress: number, message: string) => {
    setStartupProgress(progress);
    setStartupMessage(message);
  };

  const loadHardwareInfo = async () => {
    try {
      const info = await getHardwareInfo();
      setHardwareInfo(info);
      return true;
    } catch (error) {
      console.error("Failed to load hardware info:", error);
      return false;
    }
  };

  const initTools = async () => {
    try {
      const toolsData = await invoke<ThirdPartyTool[]>("get_thirdparty_tools");
      setTools(toolsData);
    } catch (error) {
      console.error("Failed to load tools:", error);
    }
  };

  const loadOverlaySettings = async () => {
    try {
      let savedSettings = await store.get<OverlaySettings>("overlay-settings");
      // 若前端 store 未读到（时序/缓存问题导致读到默认），回退到 Rust 权威来源，
      // 该来源由 try_load_persisted_settings 在启动时从 settings.json 加载，保证拿到持久化的样式与位置
      if (!savedSettings) {
        try {
          const rustSettings = await invoke<OverlaySettings>("get_overlay_current_settings");
          if (rustSettings) {
            savedSettings = { ...rustSettings, _version: 4 };
          }
        } catch {
          // 忽略，继续走默认
        }
      }
      let settingsToUse: OverlaySettings;
      let needsMigration = false;
      if (savedSettings) {
        // 处理旧格式（对象）到新格式（数组）的迁移
        let displayItems: DisplayItems;
        if (Array.isArray(savedSettings.display_items)) {
          // 新格式数组：检查版本，过旧则重置顺序和标签，保留启用状态
          const currentVersion = 4;
          const savedVersion = savedSettings._version ?? 1;
          if (savedVersion < currentVersion) {
            // 版本过旧：用默认项重建，只保留启用状态
            const savedMap = new Map(savedSettings.display_items.map((i) => [i.id, i.enabled]));
            displayItems = DEFAULT_OVERLAY_SETTINGS.display_items.map((d) => ({
              ...d,
              enabled: savedMap.has(d.id) ? savedMap.get(d.id)! : d.enabled,
            }));
            needsMigration = true;
          } else {
            // 最新版本，补充可能缺失的项，移除已废弃的项
            const defaultItems = DEFAULT_OVERLAY_SETTINGS.display_items;
            const defaultIds = new Set(defaultItems.map((i) => i.id));
            displayItems = [
              ...savedSettings.display_items.filter((i) => defaultIds.has(i.id)),
              ...defaultItems.filter((i) => !savedSettings.display_items.some((s) => s.id === i.id)),
            ];
          }
        } else {
          // 旧格式：对象，需要迁移
          needsMigration = true;
          const oldItems = savedSettings.display_items as unknown as {
            fps: boolean;
            cpu_usage: boolean;
            gpu_temp: boolean;
            gpu_usage: boolean;
            memory_usage: boolean;
            delta_password: boolean;
            game_ping: boolean;
          };
          displayItems = [
            { id: "fps", label: "FPS", enabled: oldItems.fps ?? true },
            { id: "fps_1low", label: "1% Low", enabled: false },
            { id: "fps_01low", label: "0.1% Low", enabled: false },
            { id: "cpu_usage", label: "CPU占用", enabled: oldItems.cpu_usage ?? true },
            { id: "gpu_temp", label: "GPU温度", enabled: oldItems.gpu_temp ?? true },
            { id: "gpu_usage", label: "GPU占用", enabled: oldItems.gpu_usage ?? true },
            { id: "gpu_fan_speed", label: "GPU风扇转速", enabled: false },
            { id: "gpu_power", label: "GPU功耗", enabled: false },
            { id: "gpu_clock", label: "GPU频率", enabled: false },
            { id: "gpu_vram", label: "GPU显存占用", enabled: false },
            { id: "memory_usage", label: "内存占用", enabled: oldItems.memory_usage ?? true },
            { id: "game_ping", label: "游戏延迟", enabled: oldItems.game_ping ?? true },
            { id: "delta_password", label: "三角洲密码", enabled: oldItems.delta_password ?? true },
          ];
        }
        if (needsMigration) {
          settingsToUse = {
            ...DEFAULT_OVERLAY_SETTINGS,
            ...savedSettings,
            _version: 4,
            display_items: displayItems,
          };
          await store.set("overlay-settings", settingsToUse);
          await store.save();
        } else {
          settingsToUse = {
            ...DEFAULT_OVERLAY_SETTINGS,
            ...savedSettings,
            display_items: displayItems,
          };
        }
      } else {
        settingsToUse = DEFAULT_OVERLAY_SETTINGS;
      }
      // 字体归一化：持久化数据里的字体不在前端可选列表内（例如旧默认 "MiSans"）时回退为微软雅黑，
      // 避免字体下拉框显示为"未选中任何字体"，且调整颜色后不再反复回到未选中状态。
      if (settingsToUse.font && !KNOWN_OVERLAY_FONTS.includes(settingsToUse.font)) {
        settingsToUse = { ...settingsToUse, font: "Microsoft YaHei" };
      }
      // 若用户在启动加载完成前已修改过设置，则不覆盖用户编辑，避免调整颜色后被默认值重置
      if (!userEditedOverlayRef.current) {
        setOverlaySettings(settingsToUse);
        overlaySettingsRef.current = settingsToUse;
        verticalPosRef.current = {
          x: settingsToUse.vertical_position_x ?? null,
          y: settingsToUse.vertical_position_y ?? null,
        };

        // 仅在存在已保存设置时同步到后端，避免在 LazyStore 未就绪时用默认值覆盖后端已正确加载的设置
        if (savedSettings) {
          await invoke("update_overlay_settings", { settings: settingsToUse });
        }
      }

      // 启动时自动启用悬浮框（仿照辅助准心：仅当用户保存过设置时触发）
      if (savedSettings) {
        let autoOverlay = await store.get<boolean>("nexbox_auto_overlay");
        if (autoOverlay === null || autoOverlay === undefined) {
          autoOverlay = localStorage.getItem("nexbox_auto_overlay") === "true";
        }
        if (autoOverlay) {
          try {
            await invoke("start_overlay_panel", { settings: settingsToUse });
          } catch (e) {
            console.error("Failed to auto-enable overlay on startup:", e);
          }
        }
      }
    } catch (error) {
      console.error("Failed to load overlay settings:", error);
      // 加载失败时仅设置前端 UI 默认值，不覆盖后端已有的设置；用户已修改过则不覆盖
      if (!userEditedOverlayRef.current) {
        setOverlaySettings(DEFAULT_OVERLAY_SETTINGS);
        overlaySettingsRef.current = DEFAULT_OVERLAY_SETTINGS;
        verticalPosRef.current = { x: null, y: null };
      }
    }
  };

  const loadOverlayHotkey = async () => {
    try {
      // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
      const saved = await invoke<string>("get_overlay_hotkey");
      if (saved) {
        setOverlayHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load overlay hotkey:", error);
    }
  };

  const saveOverlayHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      // Rust 端负责注册并写入 settings.json；这里仅同步前端 store 内存，
      // 避免后续保存其他设置时整体写回把热键覆盖成旧值
      await invoke("set_overlay_hotkey", { shortcut });
      await store.set("overlay-hotkey", shortcut);
      setOverlayHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save overlay hotkey:", error);
      return extractError(error);
    }
  };

  const loadCrosshairHotkey = async () => {
    try {
      // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
      const saved = await invoke<string>("get_crosshair_hotkey");
      if (saved) {
        setCrosshairHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load crosshair hotkey:", error);
    }
  };

  const saveCrosshairHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      // Rust 端负责注册并写入 settings.json；这里仅同步前端 store 内存
      await invoke("set_crosshair_hotkey", { shortcut });
      await store.set("crosshair-hotkey", shortcut);
      setCrosshairHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save crosshair hotkey:", error);
      return extractError(error);
    }
  };

  const loadCrosshairSettings = async () => {
    try {
      const saved = await store.get<CrosshairSettings>("crosshair-settings");
      if (saved) {
        let autoApply = await store.get<boolean>("nexbox_auto_crosshair");
        if (autoApply === null || autoApply === undefined) {
          autoApply = localStorage.getItem("nexbox_auto_crosshair") === "true";
        }
        saved.enabled = false;
        await invoke("update_crosshair_settings", { settings: saved });
        if (autoApply) {
          await invoke("toggle_crosshair");
        }
      }
    } catch (error) {
      console.error("Failed to load crosshair settings:", error);
    }
  };

  const loadFilterHotkey = async () => {
    try {
      // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
      const saved = await invoke<string>("get_filter_hotkey");
      if (saved) {
        setFilterHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load filter hotkey:", error);
    }
  };

  const saveFilterHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      // Rust 端负责注册并写入 settings.json；这里仅同步前端 store 内存
      await invoke("set_filter_hotkey", { shortcut });
      await store.set("filter-hotkey", shortcut);
      setFilterHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save filter hotkey:", error);
      return extractError(error);
    }
  };

  const loadAutoclickerHotkey = async () => {
    try {
      // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
      const saved = await invoke<string>("get_autoclicker_hotkey");
      if (saved) {
        setAutoclickerHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load autoclicker hotkey:", error);
    }
  };

  const saveAutoclickerHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      // Rust 端负责注册并写入 settings.json；这里仅同步前端 store 内存
      await invoke("set_autoclicker_hotkey", { shortcut });
      await store.set("autoclicker-hotkey", shortcut);
      setAutoclickerHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save autoclicker hotkey:", error);
      return extractError(error);
    }
  };

  const loadMusicPrevHotkey = async () => {
    try {
      // 热键已由 Rust 端在启动时读取持久化配置注册，这里只同步 UI 显示值
      const saved = await invoke<string>("get_music_prev_hotkey");
      if (saved) {
        setMusicPrevHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load music prev hotkey:", error);
    }
  };

  const saveMusicPrevHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      await invoke("set_music_prev_hotkey", { shortcut });
      await store.set("music-prev-hotkey", shortcut);
      setMusicPrevHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save music prev hotkey:", error);
      return extractError(error);
    }
  };

  const loadMusicNextHotkey = async () => {
    try {
      const saved = await invoke<string>("get_music_next_hotkey");
      if (saved) {
        setMusicNextHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load music next hotkey:", error);
    }
  };

  const saveMusicNextHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      await invoke("set_music_next_hotkey", { shortcut });
      await store.set("music-next-hotkey", shortcut);
      setMusicNextHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save music next hotkey:", error);
      return extractError(error);
    }
  };

  const loadMusicPlayPauseHotkey = async () => {
    try {
      const saved = await invoke<string>("get_music_playpause_hotkey");
      if (saved) {
        setMusicPlayPauseHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load music play/pause hotkey:", error);
    }
  };

  const saveMusicPlayPauseHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      await invoke("set_music_playpause_hotkey", { shortcut });
      await store.set("music-playpause-hotkey", shortcut);
      setMusicPlayPauseHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save music play/pause hotkey:", error);
      return extractError(error);
    }
  };

  const loadLyricsBtnHotkey = async () => {
    try {
      const saved = await invoke<string>("get_lyrics_btn_hotkey");
      if (saved) {
        setLyricsBtnHotkey(saved);
      }
    } catch (error) {
      console.error("Failed to load lyrics btn hotkey:", error);
    }
  };

  const saveLyricsBtnHotkey = async (shortcut: string): Promise<string | null> => {
    try {
      await invoke("set_lyrics_btn_hotkey", { shortcut });
      await store.set("lyrics-btn-hotkey", shortcut);
      setLyricsBtnHotkey(shortcut);
      return null;
    } catch (error) {
      console.error("Failed to save lyrics btn hotkey:", error);
      return extractError(error);
    }
  };

  const loadHotkeysEnabled = async () => {
    try {
      // 总开关已由 Rust 端在启动时恢复，这里只同步 UI 显示值
      const saved = await invoke<boolean>("get_hotkeys_enabled_cmd");
      setHotkeysEnabled(saved);
    } catch (error) {
      console.error("Failed to load hotkeys enabled:", error);
    }
  };

  const saveHotkeysEnabled = async (enabled: boolean) => {
    setHotkeysEnabled(enabled);
    try {
      // Rust 端负责设置并写入 settings.json
      await invoke("set_hotkeys_enabled_cmd", { enabled });
      // 同步 LazyStore 内存缓存，避免后续 store.save() 整文件写回时用旧缓存覆盖该值，
      // 否则总开关会表现为重启后不持久化
      await store.set("hotkeys-enabled", enabled);
    } catch (error) {
      console.error("Failed to save hotkeys enabled:", error);
    }
  };

  // ==================== 单个热键独立开关（加载/保存） ====================

  const loadOverlayHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_overlay_hotkey_enabled");
      setOverlayHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load overlay hotkey enabled:", error);
    }
  };

  const saveOverlayHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_overlay_hotkey_enabled", { enabled });
      await store.set("overlay-hotkey-enabled", enabled);
      setOverlayHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save overlay hotkey enabled:", error);
    }
  };

  const loadCrosshairHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_crosshair_hotkey_enabled");
      setCrosshairHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load crosshair hotkey enabled:", error);
    }
  };

  const saveCrosshairHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_crosshair_hotkey_enabled", { enabled });
      await store.set("crosshair-hotkey-enabled", enabled);
      setCrosshairHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save crosshair hotkey enabled:", error);
    }
  };

  const loadFilterHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_filter_hotkey_enabled");
      setFilterHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load filter hotkey enabled:", error);
    }
  };

  const saveFilterHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_filter_hotkey_enabled", { enabled });
      await store.set("filter-hotkey-enabled", enabled);
      setFilterHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save filter hotkey enabled:", error);
    }
  };

  const loadAutoclickerHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_autoclicker_hotkey_enabled");
      setAutoclickerHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load autoclicker hotkey enabled:", error);
    }
  };

  const saveAutoclickerHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_autoclicker_hotkey_enabled", { enabled });
      await store.set("autoclicker-hotkey-enabled", enabled);
      setAutoclickerHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save autoclicker hotkey enabled:", error);
    }
  };

  const loadMusicPrevHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_music_prev_hotkey_enabled");
      setMusicPrevHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load music prev hotkey enabled:", error);
    }
  };

  const saveMusicPrevHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_music_prev_hotkey_enabled", { enabled });
      await store.set("music-prev-hotkey-enabled", enabled);
      setMusicPrevHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save music prev hotkey enabled:", error);
    }
  };

  const loadMusicNextHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_music_next_hotkey_enabled");
      setMusicNextHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load music next hotkey enabled:", error);
    }
  };

  const saveMusicNextHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_music_next_hotkey_enabled", { enabled });
      await store.set("music-next-hotkey-enabled", enabled);
      setMusicNextHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save music next hotkey enabled:", error);
    }
  };

  const loadMusicPlayPauseHotkeyEnabled = async () => {
    try {
      const saved = await invoke<boolean>("get_music_playpause_hotkey_enabled");
      setMusicPlayPauseHotkeyEnabled(saved);
    } catch (error) {
      console.error("Failed to load music play/pause hotkey enabled:", error);
    }
  };

  const saveMusicPlayPauseHotkeyEnabled = async (enabled: boolean) => {
    try {
      await invoke("set_music_playpause_hotkey_enabled", { enabled });
      await store.set("music-playpause-hotkey-enabled", enabled);
      setMusicPlayPauseHotkeyEnabled(enabled);
    } catch (error) {
      console.error("Failed to save music play/pause hotkey enabled:", error);
    }
  };

  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pendingSettingsRef = useRef<OverlaySettings | null>(null);

const saveOverlaySettings = async (settings: OverlaySettings) => {
	// 标记用户已修改过设置，启动加载若在之后返回也不会覆盖本次编辑
	userEditedOverlayRef.current = true;
	setOverlaySettings(settings);
	overlaySettingsRef.current = settings;
	pendingSettingsRef.current = settings;
	if (saveTimerRef.current) {
	clearTimeout(saveTimerRef.current);
	}
	saveTimerRef.current = setTimeout(async () => {
	saveTimerRef.current = null;
	const s = pendingSettingsRef.current;
	if (s) {
        // 合并最新竖排位置：s 可能是旧快照（不含拖动后保存的位置），直接写回会把刚保存的位置覆盖掉。
        // verticalPosRef 为 null 表示尚未加载（沿用 s 自身的值）；否则以 ref 为准（{x:null,y:null} 表示已重置）。
        const vp = verticalPosRef.current;
        const merged =
          vp === null
            ? {
                ...s,
                vertical_position_x: s.vertical_position_x ?? null,
                vertical_position_y: s.vertical_position_y ?? null,
              }
            : { ...s, vertical_position_x: vp.x, vertical_position_y: vp.y };
        try {
          await invoke("update_overlay_settings", { settings: merged });
          await store.set("overlay-settings", merged);
          await store.save();
        } catch (error) {
          console.error("Failed to save overlay settings:", error);
        }
      }
    }, 100);
  };

  // 主应用监听竖排悬浮框位置保存/重置事件，同步本地状态与共享 store，
  // 避免后续保存其他 overlay 设置时用旧缓存把位置覆盖回旧值。
  // 只订阅一次（通过 ref 读取最新状态），避免随状态变化反复重订阅时漏掉事件；
  // 且仅在主窗口订阅：独立窗口（竖排悬浮框/桌面歌词/托盘等）的 LazyStore 缓存可能过期，
  // 整体写回会覆盖主应用刚保存的其他设置。
  useEffect(() => {
    const standalonePaths = [
      "/vertical-overlay",
      "/desktop-lyrics",
      "/lyrics-unlock-btn",
      "/tray-menu",
      "/sensor-monitor",
    ];
    if (standalonePaths.includes(window.location.pathname)) return;

    let unlistenPos: UnlistenFn | undefined;
    let unlistenReset: UnlistenFn | undefined;
    (async () => {
      unlistenPos = await listen<{ x: number; y: number }>("overlay-position-saved", async (event) => {
        const { x, y } = event.payload;
        verticalPosRef.current = { x, y };
        const current = overlaySettingsRef.current;
        if (current) {
          const updated = { ...current, vertical_position_x: x, vertical_position_y: y };
          overlaySettingsRef.current = updated;
          setOverlaySettings(updated);
          await store.set("overlay-settings", updated);
          await store.save();
        }
      });
      unlistenReset = await listen("vertical-overlay-position-reset", async () => {
        verticalPosRef.current = { x: null, y: null };
        const current = overlaySettingsRef.current;
        if (current) {
          const updated = { ...current, vertical_position_x: null, vertical_position_y: null };
          overlaySettingsRef.current = updated;
          setOverlaySettings(updated);
          await store.set("overlay-settings", updated);
          await store.save();
        }
      });
    })();
    return () => {
      unlistenPos?.();
      unlistenReset?.();
    };
  }, []);

  useEffect(() => {
    if (hasStarted.current) return;
    hasStarted.current = true;

    const runStartup = async () => {
      const tasks = [
        { name: "overlay-settings", fn: loadOverlaySettings, weight: 1 },
        { name: "hardware-info", fn: loadHardwareInfo, weight: 4 },
        { name: "overlay-hotkey", fn: loadOverlayHotkey, weight: 1 },
        { name: "crosshair-hotkey", fn: loadCrosshairHotkey, weight: 1 },
        { name: "crosshair-settings", fn: loadCrosshairSettings, weight: 1 },
        { name: "filter-hotkey", fn: loadFilterHotkey, weight: 1 },
        { name: "autoclicker-hotkey", fn: loadAutoclickerHotkey, weight: 1 },
        { name: "music-prev-hotkey", fn: loadMusicPrevHotkey, weight: 1 },
        { name: "music-next-hotkey", fn: loadMusicNextHotkey, weight: 1 },
        { name: "music-playpause-hotkey", fn: loadMusicPlayPauseHotkey, weight: 1 },
        { name: "lyrics-btn-hotkey", fn: loadLyricsBtnHotkey, weight: 1 },
        { name: "hotkeys-enabled", fn: loadHotkeysEnabled, weight: 1 },
        { name: "overlay-hotkey-enabled", fn: loadOverlayHotkeyEnabled, weight: 1 },
        { name: "crosshair-hotkey-enabled", fn: loadCrosshairHotkeyEnabled, weight: 1 },
        { name: "filter-hotkey-enabled", fn: loadFilterHotkeyEnabled, weight: 1 },
        { name: "autoclicker-hotkey-enabled", fn: loadAutoclickerHotkeyEnabled, weight: 1 },
        { name: "music-prev-hotkey-enabled", fn: loadMusicPrevHotkeyEnabled, weight: 1 },
        { name: "music-next-hotkey-enabled", fn: loadMusicNextHotkeyEnabled, weight: 1 },
        { name: "music-playpause-hotkey-enabled", fn: loadMusicPlayPauseHotkeyEnabled, weight: 1 },
        {
          name: "filter-restore",
          fn: async () => {
            try {
              let autoApply = await store.get<boolean>("nexbox_auto_apply");
              if (autoApply === null || autoApply === undefined) {
                autoApply = localStorage.getItem("nexbox_auto_apply") === "true";
              }
              await invoke("restore_filter_state", { displayIndex: null, autoApply });
            } catch (e) {
              console.error("Failed to restore filter state:", e);
            }
          },
          weight: 1,
        },
      ];

      const totalWeight = tasks.reduce((sum, t) => sum + t.weight, 0);
      let completedWeight = 0;

      setStartupProgress(5);

      const updateProgress = () => {
        const baseProgress = 5;
        const maxProgress = 95;
        const progress = baseProgress + (completedWeight / totalWeight) * (maxProgress - baseProgress);
        setStartupProgress(Math.min(progress, 95));
      };

      await Promise.all(
        tasks.map(async (task) => {
          try {
            await task.fn();
          } catch (error) {
            console.error(`Failed to load ${task.name}:`, error);
          } finally {
            completedWeight += task.weight;
            updateProgress();
          }
        })
      );

      setStartupProgress(100);
      setTimeout(() => {
        setIsStartupComplete(true);
      }, 100);
    };

    runStartup();

    return () => {};
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <AppStartupContext.Provider
      value={{
        isStartupComplete,
        startupProgress,
        startupMessage,
        hardwareInfo,
        tools,
        initTools,
        overlaySettings,
        saveOverlaySettings,
        overlayHotkey,
        saveOverlayHotkey,
        crosshairHotkey,
        saveCrosshairHotkey,
        filterHotkey,
        saveFilterHotkey,
        autoclickerHotkey,
        saveAutoclickerHotkey,
        musicPrevHotkey,
        saveMusicPrevHotkey,
        musicNextHotkey,
        saveMusicNextHotkey,
        musicPlayPauseHotkey,
        saveMusicPlayPauseHotkey,
        lyricsBtnHotkey,
        saveLyricsBtnHotkey,
        hotkeysEnabled,
        saveHotkeysEnabled,
        overlayHotkeyEnabled,
        saveOverlayHotkeyEnabled,
        crosshairHotkeyEnabled,
        saveCrosshairHotkeyEnabled,
        filterHotkeyEnabled,
        saveFilterHotkeyEnabled,
        autoclickerHotkeyEnabled,
        saveAutoclickerHotkeyEnabled,
        musicPrevHotkeyEnabled,
        saveMusicPrevHotkeyEnabled,
        musicNextHotkeyEnabled,
        saveMusicNextHotkeyEnabled,
        musicPlayPauseHotkeyEnabled,
        saveMusicPlayPauseHotkeyEnabled,
      }}
    >
      {children}
    </AppStartupContext.Provider>
  );
}
