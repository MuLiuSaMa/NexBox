import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { ChakraProvider, ColorModeScript, Spinner, Center } from "@chakra-ui/react";
import { CacheProvider } from "@emotion/react";
import createCache from "@emotion/cache";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import "./index.css";
import { I18nextProvider } from "react-i18next";
import i18n from "./lib/i18n";
import { BackgroundProvider } from "./contexts/background-context";
import { ThemeColorProvider } from "./contexts/theme-color-context";
import { AppStartupProvider } from "./contexts/app-startup-context";
import { UpdateProvider } from "./contexts/update-context";
import { FontProvider } from "./contexts/font-context";
import { ThemeModeProvider } from "./contexts/theme-mode-context";
import { LiquidGlassSvgFilter } from "./components/special/liquid-glass-svg-filter";
import theme from "./lib/theme";
import { mockIPC, mockWindows, mockConvertFileSrc } from "@tauri-apps/api/mocks";

// Mock Tauri APIs when running in browser dev mode (no top-level await needed)
if (typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window)) {
  const npmMockStore = new Map<string, unknown>();
  mockWindows("main");
  mockConvertFileSrc("windows");
  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case "plugin:store|load":
          return 1;
        case "plugin:store|get_store":
          return 1;
        case "plugin:store|get": {
          const a = args as Record<string, unknown>;
          const v = npmMockStore.get(a.key as string);
          return [v !== undefined ? v : null, v !== undefined];
        }
        case "plugin:store|set": {
          const a = args as Record<string, unknown>;
          npmMockStore.set(a.key as string, a.value);
          return;
        }
        case "plugin:store|has": {
          const a = args as Record<string, unknown>;
          return npmMockStore.has(a.key as string);
        }
        case "plugin:store|delete": {
          const a = args as Record<string, unknown>;
          npmMockStore.delete(a.key as string);
          return;
        }
        case "plugin:store|keys":
          return Array.from(npmMockStore.keys());
        case "plugin:store|clear":
          npmMockStore.clear();
          return;
        case "plugin:store|save":
        case "plugin:store|reload":
          return;
        case "get_system_locale":
          return "zh";
        case "get_hardware":
          return null;
        case "get_thirdparty_tools":
          return [];
        case "update_overlay_settings":
        case "set_overlay_hotkey":
        case "set_crosshair_hotkey":
        case "set_filter_hotkey":
        case "update_crosshair_settings":
        case "minimize_to_tray":
        case "exit_app":
        case "install_update":
        case "delete_download_file":
        case "mark_pending_install":
        case "clear_pending_install":
        case "cancel_download":
        case "reset_download_cancel":
          return;
        case "download_update":
          return "downloaded.exe";
        default:
          if (cmd.startsWith("plugin:")) return;
          console.warn("[Tauri Mock] Unhandled command:", cmd, args);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}

const emotionCache = createCache({
  key: "css",
  prepend: true,
});

function Root() {
  const [isI18nReady, setIsI18nReady] = useState(false);

  useEffect(() => {
    if (i18n.isInitialized) {
      setIsI18nReady(true);
    } else {
      i18n.on("initialized", () => setIsI18nReady(true));
    }
  }, []);

  if (!isI18nReady) {
    return (
      <Center h="100vh">
        <Spinner size="xl" />
      </Center>
    );
  }

  return (
    <React.StrictMode>
      <CacheProvider value={emotionCache}>
        <ColorModeScript initialColorMode={theme.config.initialColorMode} />
        <I18nextProvider i18n={i18n}>
          <ChakraProvider theme={theme}>
            <BrowserRouter>
              <AppStartupProvider>
                <UpdateProvider>
                  <BackgroundProvider>
                    <ThemeColorProvider>
                      <FontProvider>
                        <LiquidGlassSvgFilter />
                        <ThemeModeProvider>
                          <App />
                        </ThemeModeProvider>
                      </FontProvider>
                    </ThemeColorProvider>
                  </BackgroundProvider>
                </UpdateProvider>
              </AppStartupProvider>
            </BrowserRouter>
          </ChakraProvider>
        </I18nextProvider>
      </CacheProvider>
    </React.StrictMode>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<Root />);
