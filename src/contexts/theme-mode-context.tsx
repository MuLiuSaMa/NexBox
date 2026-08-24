"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  ReactNode,
  useCallback,
  useMemo,
} from "react";
import { useColorMode } from "@chakra-ui/react";
import { store } from "@/lib/store";
import {
  LS_THEME_MODE,
  type ThemeMode,
  readInitialThemeMode,
  systemPrefersDark,
} from "@/lib/theme";

interface ThemeModeContextType {
  /** 用户选定的主题模式：跟随系统 / 浅色 / 深色 */
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
}

const ThemeModeContext = createContext<ThemeModeContextType>({
  themeMode: "system",
  setThemeMode: () => {},
});

/**
 * 主题模式三态协调 Provider：
 * - 统一管理 themeMode（system/light/dark）并持久化（localStorage + settings.json）。
 * - 将 themeMode 解析为 Chakra colorMode（light/dark），全项目 useColorModeValue 实时响应。
 * - "system" 模式下监听系统深浅偏好，实时跟随。
 */
export function ThemeModeProvider({ children }: { children: ReactNode }) {
  const { setColorMode } = useColorMode();
  const [themeMode, setThemeModeState] = useState<ThemeMode>(readInitialThemeMode);

  // 应用 themeMode 到 Chakra colorMode
  useEffect(() => {
    const apply = () => {
      if (themeMode === "system") {
        setColorMode(systemPrefersDark() ? "dark" : "light");
      } else {
        setColorMode(themeMode);
      }
    };
    apply();

    if (themeMode !== "system") return;

    // 跟随系统：监听系统深浅偏好变化
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => apply();
    mql.addEventListener("change", onChange);
    return () => mql.removeEventListener("change", onChange);
  }, [themeMode, setColorMode]);

  // 持久化 themeMode
  useEffect(() => {
    try {
      localStorage.setItem(LS_THEME_MODE, themeMode);
    } catch {
      // 忽略 localStorage 不可用
    }
    store
      .set("theme-mode", themeMode)
      .then(() => store.save())
      .catch((err) => console.error("Failed to save theme mode:", err));
  }, [themeMode]);

  const setThemeMode = useCallback((mode: ThemeMode) => {
    setThemeModeState(mode);
  }, []);

  const value = useMemo(() => ({ themeMode, setThemeMode }), [themeMode, setThemeMode]);

  return <ThemeModeContext.Provider value={value}>{children}</ThemeModeContext.Provider>;
}

export function useThemeMode() {
  return useContext(ThemeModeContext);
}