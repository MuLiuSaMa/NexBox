import { extendTheme, type ThemeConfig } from "@chakra-ui/react";

export const LS_THEME_MODE = "nexbox-theme-mode";
export type ThemeMode = "light" | "dark" | "system";

/** 判断任意值是否为合法的主题模式 */
export function isThemeMode(v: unknown): v is ThemeMode {
  return v === "light" || v === "dark" || v === "system";
}

/** 同步读取持久化的主题模式：light/dark/system，无记录时默认跟随系统 */
export function readInitialThemeMode(): ThemeMode {
  try {
    const m = localStorage.getItem(LS_THEME_MODE);
    if (isThemeMode(m)) return m;

    // 迁移：老版本只有 chakra-ui-color-mode，尊重既有显式选择。
    // 注意：新版本每次应用主题都会写 chakra-ui-color-mode（托盘菜单跨窗口同步依赖该键），
    // 因此仅凭该键无法区分"用户选了浅色"与"跟随系统时系统恰好是浅色"。
    // 若直接迁移会把 system 误判成 light/dark 并固化，故此后不再据此推断；
    // 真实模式由 ThemeModeProvider 从 settings.json 恢复。
  } catch {
    // localStorage 不可用时返回默认跟随系统
  }
  return "system";
}

/** 系统是否偏好深色 */
export function systemPrefersDark(): boolean {
  try {
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  } catch {
    return false;
  }
}

/** 把主题模式解析为具体的浅/深方案 */
export function resolveScheme(mode: ThemeMode): "light" | "dark" {
  if (mode === "light") return "light";
  if (mode === "dark") return "dark";
  return systemPrefersDark() ? "dark" : "light";
}

/** 根据持久化主题模式的解析浅/深，用作首帧初始 colorMode、避免闪烁 */
function readInitialScheme(): "light" | "dark" {
  return resolveScheme(readInitialThemeMode());
}

const config: ThemeConfig = {
  initialColorMode: readInitialScheme(),
  useSystemColorMode: false,
};

const theme = extendTheme({
  config,
  fonts: {
    heading: 'var(--app-font-family, "Sinter-Regular"), Inter, system-ui, sans-serif',
    body: 'var(--app-font-family, "Sinter-Regular"), Inter, system-ui, sans-serif',
  },
  colors: {
    brand: {
      50: "#eef2ff",
      100: "#e0e7ff",
      200: "#c7d2fe",
      300: "#a5b4fc",
      400: "#818cf8",
      500: "#6366f1",
      600: "#4f46e5",
      700: "#4338ca",
      800: "#3730a3",
      900: "#312e81",
    },
  },
  styles: {
    global: (props: any) => ({
      body: {
        bg: props.colorMode === 'dark' ? '#000000' : '#f7fafc',
        color: props.colorMode === 'dark' ? '#e0e0e0' : '#1a202c',
      },
    }),
  },
  components: {
    Button: {
      baseStyle: {
        borderRadius: "lg",
        fontWeight: "medium",
      },
    },
    Card: {
      baseStyle: {
        container: {
          borderRadius: "xl",
          bg: undefined,
          borderColor: undefined,
          borderWidth: undefined,
        },
      },
    },
    Tooltip: {
      baseStyle: {
        // 由 BackgroundProvider 通过 CSS 变量驱动：跟随液态玻璃开关与深浅主题
        bg: "var(--nexbox-tooltip-bg, #ffffff)",
        color: "var(--nexbox-tooltip-fg, #1a1a1a)",
        // 覆盖默认 css var，使箭头颜色与胶囊背景一致
        "--tooltip-bg": "var(--nexbox-tooltip-bg, #ffffff)",
        "--tooltip-fg": "var(--nexbox-tooltip-fg, #1a1a1a)",
        "--popper-arrow-bg": "var(--nexbox-tooltip-bg, #ffffff)",
        px: "8px",
        py: "2px",
        borderRadius: "full", // 胶囊全圆角
        fontWeight: "medium",
        fontSize: "xs",
        backdropFilter: "var(--nexbox-tooltip-blur, none)",
        WebkitBackdropFilter: "var(--nexbox-tooltip-blur, none)",
        border: "1px solid",
        borderColor: "var(--nexbox-tooltip-border, rgba(0,0,0,0.10))",
        maxW: "xs",
        zIndex: "tooltip",
      } as any,
      // 全局兜底：点一下提示框即关闭、关闭零延迟，避免部分情况下悬浮提示常驻不消失
      defaultProps: {
        closeOnMouseDown: true,
        closeDelay: 0,
        openDelay: 150,
      } as any,
    },
  },
});

export default theme;
