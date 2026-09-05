import { Routes, Route, useLocation } from "react-router-dom";
import { AnimatePresence } from "framer-motion";
import { Box } from "@chakra-ui/react";
import { MainLayout } from "./components/ui/main-layout";
import {
  AnimatedPage,
  type TransitionMode,
  readTransitionMode,
} from "./components/ui/animated-page";
import HomePage from "./pages/HomePage";
import HardwarePage from "./pages/HardwarePage";
import ToolsPage from "./pages/ToolsPage";
import OptimizePage from "./pages/OptimizePage";
import MemoryLimitPage from "./pages/MemoryLimitPage";
import MemoryCleanupPage from "./pages/MemoryCleanupPage";
import AceOptimizePage from "./pages/AntiCheatOptimizePage";
import DisplayFilterPage from "./pages/DisplayFilterPage";
import SettingsPage from "./pages/SettingsPage";
import CrosshairPage from "./pages/CrosshairPage";
import DiskHealthPage from "./pages/DiskHealthPage";
import OverlayPanelPage from "./pages/OverlayPanelPage";
import DeltaForcePage from "./pages/DeltaForcePage";
import OtherGunCodePlatformsPage from "./pages/OtherGunCodePlatformsPage";
import DeltaForceRoulettePage from "./pages/DeltaForceRoulettePage";
import MoodPage from "./pages/MoodPage";
import BuiltinToolsPage from "./pages/BuiltinToolsPage";
import GpuRenamePage from "./pages/GpuRenamePage";
import ResolutionConverterPage from "./pages/ResolutionConverterPage";
import ShaderCachePage from "./pages/ShaderCachePage";
import PowerManagementPage from "./pages/PowerManagementPage";
import StorageCleanPage from "./pages/StorageCleanPage";
import StartupManagerPage from "./pages/StartupManagerPage";
import ContextMenuManagerPage from "./pages/ContextMenuManagerPage";
import DownloadAcceleratorPage from "./pages/DownloadAcceleratorPage";
import SystemOptimizerPage from "./pages/SystemOptimizerPage";
import NetworkOptimizerPage from "./pages/NetworkOptimizerPage";
import PeripheralOptimizePage from "./pages/PeripheralOptimizePage";
import WindowsUpdatePage from "./pages/WindowsUpdatePage";
import DLSSPresetPage from "./pages/DLSSPresetPage";
import NvidiaDriverPage from "./pages/NvidiaDriverPage";
import NvidiaDriverDownloadPage from "./pages/NvidiaDriverDownloadPage";
import EpicFreePage from "./pages/EpicFreePage";
import SteamPage from "./pages/SteamPage";
import TrayMenuPage from "./pages/TrayMenuPage";
import DesktopLyricsPage from "./pages/DesktopLyricsPage";
import LyricsUnlockBtnPage from "./pages/LyricsUnlockBtnPage";
import VerticalOverlayPage from "./pages/VerticalOverlayPage";
import SensorMonitorPage from "./pages/SensorMonitorPage";
import RuntimeRepairPage from "./pages/RuntimeRepairPage";
import VtxVirtualizationPage from "./pages/VtxVirtualizationPage";
import HiddenFeaturesPage from "./pages/HiddenFeaturesPage";
import AudioEqPage from "./pages/AudioEqPage";
import AutoClickerPage from "./pages/AutoClickerPage";
import GameProcessOptimizePage from "./pages/GameProcessOptimizePage";
import CpuSchedulerPage from "./pages/CpuSchedulerPage";
import SpeedTestPage from "./pages/SpeedTestPage";
import CustomPage from "./pages/CustomPage";
import NvidiaRecordingPage from "./pages/NvidiaRecordingPage";
import VacRepairPage from "./pages/VacRepairPage";
import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

import { UpdateModal } from "./components/UpdateModal";
import { SplashScreen } from "./components/SplashScreen";
import { StartupAdModal } from "./components/ads/startup-ad-popup";
import { useAds } from "./hooks/use-ads";
import { useAppStartup } from "./contexts/app-startup-context";
import { MusicProvider } from "./contexts/music-context";
import MusicPage from "./pages/MusicPage";
import { ImportantAnnouncementModal } from "./components/ImportantAnnouncementModal";
import { DynamicIslandHost } from "./components/ui/dynamic-island";
import { AccelIslandBridge } from "./components/ui/accel-island-bridge";

/** 启动完成后展示一次开屏广告弹窗（ads 为空时不显示任何内容） */
function StartupAdHost() {
  const { splash } = useAds();
  return <StartupAdModal ads={splash} />;
}

function App() {
  const { isStartupComplete } = useAppStartup();
  const location = useLocation();

  // Tray menu: render standalone, no main layout
  if (location.pathname === "/tray-menu") {
    return <TrayMenuPage />;
  }

  // Desktop lyrics window: render standalone, no main layout
  if (location.pathname === "/desktop-lyrics") {
    return <DesktopLyricsPage />;
  }

  // Lyrics unlock button window: tiny standalone overlay
  if (location.pathname === "/lyrics-unlock-btn") {
    return <LyricsUnlockBtnPage />;
  }

  // Vertical overlay window: standalone, no main layout
  if (location.pathname === "/vertical-overlay") {
    return <VerticalOverlayPage />;
  }

  // Sensor monitor window: standalone, no main layout
  if (location.pathname === "/sensor-monitor") {
    return <SensorMonitorPage />;
  }

  // Mood window: standalone, no main layout（主页「心境」卡片点击打开独立窗口）
  if (location.pathname === "/mood") {
    return <MoodPage />;
  }

  // 开机自启(--autostart)模式：后端已离屏预热加载本窗口，前端初始化完成后隐藏到托盘，
  // 复用 minimize_to_tray 正确更新后端可见性并触发 EcoQoS。
  useEffect(() => {
    (async () => {
      try {
        const autostart = await invoke<boolean>("is_autostart_mode");
        if (autostart) await invoke("minimize_to_tray");
      } catch (e) {
        console.error("autostart hide check failed:", e);
      }
    })();
  }, []);

  const [pageTransitionMode, setPageTransitionMode] = useState<TransitionMode>("fade");

  useEffect(() => {
    setPageTransitionMode(readTransitionMode());

    const handler = () => setPageTransitionMode(readTransitionMode());

    window.addEventListener("page-transition-setting-changed", handler);
    return () => window.removeEventListener("page-transition-setting-changed", handler);
  }, []);

  return (
    <MusicProvider>
      <>
        {!isStartupComplete && <SplashScreen />}
        {isStartupComplete && <StartupAdHost />}
        {/* <MiniMusicPlayer /> */}
        <MainLayout>
          {/* 主页常驻挂载：路由切换只隐藏、不卸载，避免每次回到主页重新加载硬件信息与快捷启动 */}
          <Box display={location.pathname === "/" ? "block" : "none"}>
            <AnimatedPage>
              <HomePage />
            </AnimatedPage>
          </Box>
          {location.pathname !== "/" &&
            (pageTransitionMode !== "off" ? (
              <AnimatePresence mode="wait" initial={false}>
                <Routes location={location} key={location.pathname}>
                  <Route
                    path="/hardware"
                    element={
                      <AnimatedPage>
                        <HardwarePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/tools"
                    element={
                      <AnimatedPage>
                        <ToolsPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/builtin-tools"
                    element={
                      <AnimatedPage>
                        <BuiltinToolsPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimization"
                    element={
                      <AnimatedPage>
                        <OptimizePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize"
                    element={
                      <AnimatedPage>
                        <OptimizePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/memory-cleanup"
                    element={
                      <AnimatedPage>
                        <MemoryCleanupPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/ace-optimize"
                    element={
                      <AnimatedPage>
                        <AceOptimizePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/game-process-optimize"
                    element={
                      <AnimatedPage>
                        <GameProcessOptimizePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/memory-limit"
                    element={
                      <AnimatedPage>
                        <MemoryLimitPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/display-filter"
                    element={
                      <AnimatedPage>
                        <DisplayFilterPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/settings"
                    element={
                      <AnimatedPage>
                        <SettingsPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/crosshair"
                    element={
                      <AnimatedPage>
                        <CrosshairPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/autoclicker"
                    element={
                      <AnimatedPage>
                        <AutoClickerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/disk-health"
                    element={
                      <AnimatedPage>
                        <DiskHealthPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/overlay-panel"
                    element={
                      <AnimatedPage>
                        <OverlayPanelPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/delta-force"
                    element={
                      <AnimatedPage>
                        <DeltaForcePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/delta-force/other-platforms"
                    element={
                      <AnimatedPage>
                        <OtherGunCodePlatformsPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/delta-force/random-equipment"
                    element={
                      <AnimatedPage>
                        <DeltaForceRoulettePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/gpu-rename"
                    element={
                      <AnimatedPage>
                        <GpuRenamePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/resolution-converter"
                    element={
                      <AnimatedPage>
                        <ResolutionConverterPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/shader-cache"
                    element={
                      <AnimatedPage>
                        <ShaderCachePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/power-management"
                    element={
                      <AnimatedPage>
                        <PowerManagementPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/storage-clean"
                    element={
                      <AnimatedPage>
                        <StorageCleanPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/startup-manager"
                    element={
                      <AnimatedPage>
                        <StartupManagerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/system-optimizer"
                    element={
                      <AnimatedPage>
                        <SystemOptimizerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/network-optimizer"
                    element={
                      <AnimatedPage>
                        <NetworkOptimizerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/peripheral-optimize"
                    element={
                      <AnimatedPage>
                        <PeripheralOptimizePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/windows-update"
                    element={
                      <AnimatedPage>
                        <WindowsUpdatePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/optimize/cpu-scheduler"
                    element={
                      <AnimatedPage>
                        <CpuSchedulerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/dlss-preset"
                    element={
                      <AnimatedPage>
                        <DLSSPresetPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/audio-eq"
                    element={
                      <AnimatedPage>
                        <AudioEqPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/nvidia-driver"
                    element={
                      <AnimatedPage>
                        <NvidiaDriverPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/nvidia-driver-download"
                    element={
                      <AnimatedPage>
                        <NvidiaDriverDownloadPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/steam"
                    element={
                      <AnimatedPage>
                        <SteamPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/epic-free"
                    element={
                      <AnimatedPage>
                        <EpicFreePage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/music"
                    element={
                      <AnimatedPage>
                        <MusicPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/custom"
                    element={
                      <AnimatedPage>
                        <CustomPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/speedtest"
                    element={
                      <AnimatedPage>
                        <SpeedTestPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/runtime-repair"
                    element={
                      <AnimatedPage>
                        <RuntimeRepairPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/vtx-virtualization"
                    element={
                      <AnimatedPage>
                        <VtxVirtualizationPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/hidden-features"
                    element={
                      <AnimatedPage>
                        <HiddenFeaturesPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/context-menu"
                    element={
                      <AnimatedPage>
                        <ContextMenuManagerPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/download-accelerator"
                    element={
                      <AnimatedPage>
                        <DownloadAcceleratorPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/nvidia-recording"
                    element={
                      <AnimatedPage>
                        <NvidiaRecordingPage />
                      </AnimatedPage>
                    }
                  />
                  <Route
                    path="/vac-repair"
                    element={
                      <AnimatedPage>
                        <VacRepairPage />
                      </AnimatedPage>
                    }
                  />
                </Routes>
              </AnimatePresence>
            ) : (
              <Routes location={location}>
                <Route
                  path="/hardware"
                  element={
                    <AnimatedPage>
                      <HardwarePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/tools"
                  element={
                    <AnimatedPage>
                      <ToolsPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/builtin-tools"
                  element={
                    <AnimatedPage>
                      <BuiltinToolsPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimization"
                  element={
                    <AnimatedPage>
                      <OptimizePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize"
                  element={
                    <AnimatedPage>
                      <OptimizePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/memory-cleanup"
                  element={
                    <AnimatedPage>
                      <MemoryCleanupPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/ace-optimize"
                  element={
                    <AnimatedPage>
                      <AceOptimizePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/game-process-optimize"
                  element={
                    <AnimatedPage>
                      <GameProcessOptimizePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/memory-limit"
                  element={
                    <AnimatedPage>
                      <MemoryLimitPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/display-filter"
                  element={
                    <AnimatedPage>
                      <DisplayFilterPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/settings"
                  element={
                    <AnimatedPage>
                      <SettingsPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/crosshair"
                  element={
                    <AnimatedPage>
                      <CrosshairPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/autoclicker"
                  element={
                    <AnimatedPage>
                      <AutoClickerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/overlay-panel"
                  element={
                    <AnimatedPage>
                      <OverlayPanelPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/delta-force"
                  element={
                    <AnimatedPage>
                      <DeltaForcePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/delta-force/other-platforms"
                  element={
                    <AnimatedPage>
                      <OtherGunCodePlatformsPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/delta-force/random-equipment"
                  element={
                    <AnimatedPage>
                      <DeltaForceRoulettePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/gpu-rename"
                  element={
                    <AnimatedPage>
                      <GpuRenamePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/resolution-converter"
                  element={
                    <AnimatedPage>
                      <ResolutionConverterPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/shader-cache"
                  element={
                    <AnimatedPage>
                      <ShaderCachePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/power-management"
                  element={
                    <AnimatedPage>
                      <PowerManagementPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/storage-clean"
                  element={
                    <AnimatedPage>
                      <StorageCleanPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/startup-manager"
                  element={
                    <AnimatedPage>
                      <StartupManagerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/system-optimizer"
                  element={
                    <AnimatedPage>
                      <SystemOptimizerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/network-optimizer"
                  element={
                    <AnimatedPage>
                      <NetworkOptimizerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/peripheral-optimize"
                  element={
                    <AnimatedPage>
                      <PeripheralOptimizePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/windows-update"
                  element={
                    <AnimatedPage>
                      <WindowsUpdatePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/optimize/cpu-scheduler"
                  element={
                    <AnimatedPage>
                      <CpuSchedulerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/dlss-preset"
                  element={
                    <AnimatedPage>
                      <DLSSPresetPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/audio-eq"
                  element={
                    <AnimatedPage>
                      <AudioEqPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/nvidia-driver"
                  element={
                    <AnimatedPage>
                      <NvidiaDriverPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/nvidia-driver-download"
                  element={
                    <AnimatedPage>
                      <NvidiaDriverDownloadPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/disk-health"
                  element={
                    <AnimatedPage>
                      <DiskHealthPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/epic-free"
                  element={
                    <AnimatedPage>
                      <EpicFreePage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/steam"
                  element={
                    <AnimatedPage>
                      <SteamPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/music"
                  element={
                    <AnimatedPage>
                      <MusicPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/custom"
                  element={
                    <AnimatedPage>
                      <CustomPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/speedtest"
                  element={
                    <AnimatedPage>
                      <SpeedTestPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/runtime-repair"
                  element={
                    <AnimatedPage>
                      <RuntimeRepairPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/vtx-virtualization"
                  element={
                    <AnimatedPage>
                      <VtxVirtualizationPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/hidden-features"
                  element={
                    <AnimatedPage>
                      <HiddenFeaturesPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/context-menu"
                  element={
                    <AnimatedPage>
                      <ContextMenuManagerPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/download-accelerator"
                  element={
                    <AnimatedPage>
                      <DownloadAcceleratorPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/nvidia-recording"
                  element={
                    <AnimatedPage>
                      <NvidiaRecordingPage />
                    </AnimatedPage>
                  }
                />
                <Route
                  path="/vac-repair"
                  element={
                    <AnimatedPage>
                      <VacRepairPage />
                    </AnimatedPage>
                  }
                />
              </Routes>
            ))}
        </MainLayout>

        <UpdateModal />
        <ImportantAnnouncementModal />
        <DynamicIslandHost />
        <AccelIslandBridge />
      </>
    </MusicProvider>
  );
}

export default App;
