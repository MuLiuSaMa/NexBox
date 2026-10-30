import { Routes, Route, useLocation, Navigate } from "react-router-dom";
import { Box } from "@chakra-ui/react";
import { MainLayout } from "./components/ui/main-layout";
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
import DeltaStatsPage from "./pages/DeltaStatsPage";
import StreamerDirectoryPage from "./pages/StreamerDirectoryPage";
import StreamerProfilePage from "./pages/StreamerProfilePage";
import MoodPage from "./pages/MoodPage";
import BuiltinToolsPage from "./pages/BuiltinToolsPage";
import GpuRenamePage from "./pages/GpuRenamePage";
import CpuRenamePage from "./pages/CpuRenamePage";
import ResolutionConverterPage from "./pages/ResolutionConverterPage";
import ShaderCachePage from "./pages/ShaderCachePage";
import PsoCachePage from "./pages/PsoCachePage";
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
import DesktopLyricsPage from "./pages/DesktopLyricsPage";
import DynamicIslandWindowPage from "./pages/DynamicIslandWindowPage";
import VerticalOverlayPage from "./pages/VerticalOverlayPage";
import SensorMonitorPage from "./pages/SensorMonitorPage";
import RuntimeRepairPage from "./pages/RuntimeRepairPage";
import AppManagerPage from "./pages/AppManagerPage";
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
import TimeSyncPage from "./pages/TimeSyncPage";
import ScreenTestPage from "./pages/ScreenTestPage";
import BsodLogPage from "./pages/BsodLogPage";
import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { IS_STORE_BUILD } from "@/lib/build-flags";
import { store } from "@/lib/store";
import { STARTUP_PAGE_STORE_KEY, STARTUP_PAGE_VALUES } from "@/config/startup-pages";

import { UpdateModal } from "./components/UpdateModal";
import { SplashScreen } from "./components/SplashScreen";
import { StartupAdModal } from "./components/ads/startup-ad-popup";
import { useAds } from "./hooks/use-ads";
import { useAppStartup } from "./contexts/app-startup-context";
import { MusicProvider } from "./contexts/music-context";
import MusicPage from "./pages/MusicPage";
import { ImportantAnnouncementModal } from "./components/ImportantAnnouncementModal";
import { StarPromptModal } from "./components/StarPromptModal";
import { DynamicIslandHost } from "./components/ui/dynamic-island";
import { AccelIslandBridge } from "./components/ui/accel-island-bridge";
import { IslandRemoteBridge } from "./components/ui/island-remote-bridge";

/** 启动完成后展示一次开屏广告弹窗（ads 为空时不显示任何内容） */
function StartupAdHost() {
  const { splash } = useAds();
  return <StartupAdModal ads={splash} />;
}

/**
 * 启动页跳转：应用启动时如果用户设置了「默认启动页面」，直接落到该页面。
 * - 只在主窗口首次进入 "/" 时执行一次，不会影响独立窗口（灵动岛/歌词/悬浮窗等）
 * - 先读 localStorage 首帧缓存避免闪一下主页，缓存缺失时再读 settings.json 的权威值
 */
function StartupPageRedirect() {
  const navigate = useNavigate();
  const location = useLocation();
  const done = useRef(false);

  useEffect(() => {
    if (done.current) return;
    done.current = true;
    if (location.pathname !== "/") return;

    const apply = (target: string | null | undefined) => {
      if (!target || target === "/") return;
      if (!STARTUP_PAGE_VALUES.includes(target)) return;
      navigate(target, { replace: true });
    };

    let cached: string | null = null;
    try {
      cached = localStorage.getItem(STARTUP_PAGE_STORE_KEY);
    } catch {
      cached = null;
    }
    if (cached) {
      apply(cached);
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const saved = await store.get<string>(STARTUP_PAGE_STORE_KEY);
        if (!cancelled) apply(saved);
      } catch {
        /* 读取失败就用默认首页 */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [navigate, location.pathname]);

  return null;
}

function App() {
  const { isStartupComplete } = useAppStartup();
  const location = useLocation();

  // 桌面灵动岛独立窗口：只负责渲染，状态真源在主窗口（见 IslandRemoteBridge）
  if (location.pathname === "/dynamic-island") {
    return <DynamicIslandWindowPage />;
  }

  // Desktop lyrics window: render standalone, no main layout
  if (location.pathname === "/desktop-lyrics") {
    return <DesktopLyricsPage />;
  }

  // 解锁按钮已内嵌到桌面歌词窗口,不再使用独立窗口

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

  return (
    <MusicProvider>
      <>
        <StartupPageRedirect />
        {!isStartupComplete && <SplashScreen />}
        {isStartupComplete && <StartupAdHost />}
        {/* <MiniMusicPlayer /> */}
        <MainLayout>
          {/* 主页常驻挂载：路由切换只隐藏、不卸载，避免每次回到主页重新加载硬件信息与快捷启动 */}
          <Box display={location.pathname === "/" ? "block" : "none"}>
            <HomePage />
          </Box>
          {/* 工具页常驻挂载：路由切换只隐藏、不卸载，避免每次进入工具页时各栏目（官方/社区/第三方）重新加载 */}
          <Box display={location.pathname === "/tools" ? "block" : "none"}>
            <ToolsPage />
          </Box>
          {location.pathname !== "/" && location.pathname !== "/tools" && (
              <Routes location={location}>
                  <Route
                    path="/hardware"
                    element={
                      <HardwarePage />
                    }
                  />
                  <Route
                    path="/builtin-tools"
                    element={
                      <BuiltinToolsPage />
                    }
                  />
                  <Route
                    path="/optimization"
                    element={
                      <OptimizePage />
                    }
                  />
                  <Route
                    path="/optimize"
                    element={
                      <OptimizePage />
                    }
                  />
                  <Route
                    path="/optimize/memory-cleanup"
                    element={
                      <MemoryCleanupPage />
                    }
                  />
                  <Route
                    path="/optimize/ace-optimize"
                    element={
                      <AceOptimizePage />
                    }
                  />
                  <Route
                    path="/optimize/game-process-optimize"
                    element={
                      <GameProcessOptimizePage />
                    }
                  />
                  <Route
                    path="/optimize/memory-limit"
                    element={
                      <MemoryLimitPage />
                    }
                  />
                  <Route
                    path="/display-filter"
                    element={
                      <DisplayFilterPage />
                    }
                  />
                  <Route
                    path="/settings"
                    element={
                      <SettingsPage />
                    }
                  />
                  <Route
                    path="/crosshair"
                    element={
                      <CrosshairPage />
                    }
                  />
                  <Route
                    path="/autoclicker"
                    element={
                      <AutoClickerPage />
                    }
                  />
                  <Route
                    path="/disk-health"
                    element={
                      <DiskHealthPage />
                    }
                  />
                  <Route
                    path="/overlay-panel"
                    element={
                      <OverlayPanelPage />
                    }
                  />
                  <Route
                    path="/delta-force"
                    element={
                      <DeltaForcePage />
                    }
                  />
                  <Route
                    path="/delta-force/other-platforms"
                    element={
                      <OtherGunCodePlatformsPage />
                    }
                  />
                  <Route
                    path="/delta-force/random-equipment"
                    element={
                      <DeltaForceRoulettePage />
                    }
                  />
                  <Route
                    path="/delta-force/stats"
                    element={
                      <DeltaStatsPage />
                    }
                  />
                  <Route
                    path="/delta-force/streamers"
                    element={
                      <StreamerDirectoryPage />
                    }
                  />
                  <Route
                    path="/delta-force/streamers/:id"
                    element={
                      <StreamerProfilePage />
                    }
                  />
                  <Route
                    path="/gpu-rename"
                    element={
                      <GpuRenamePage />
                    }
                  />
                  <Route
                    path="/cpu-rename"
                    element={
                      <CpuRenamePage />
                    }
                  />
                  <Route
                    path="/resolution-converter"
                    element={
                      <ResolutionConverterPage />
                    }
                  />
                  <Route
                    path="/optimize/shader-cache"
                    element={
                      <ShaderCachePage />
                    }
                  />
                  <Route
                    path="/optimize/pso-cache"
                    element={
                      <PsoCachePage />
                    }
                  />
                  <Route
                    path="/optimize/power-management"
                    element={
                      <PowerManagementPage />
                    }
                  />
                  <Route
                    path="/optimize/storage-clean"
                    element={
                      <StorageCleanPage />
                    }
                  />
                  <Route
                    path="/optimize/startup-manager"
                    element={
                      <StartupManagerPage />
                    }
                  />
                  <Route
                    path="/system-optimizer"
                    element={
                      <SystemOptimizerPage />
                    }
                  />
                  {/* 旧路径（原优化页子页）重定向到独立导航页 */}
                  <Route
                    path="/optimize/system-optimizer"
                    element={
                      <Navigate to="/system-optimizer" replace />
                    }
                  />
                  <Route
                    path="/optimize/network-optimizer"
                    element={
                      <NetworkOptimizerPage />
                    }
                  />
                  <Route
                    path="/optimize/peripheral-optimize"
                    element={
                      <PeripheralOptimizePage />
                    }
                  />
                  <Route
                    path="/optimize/windows-update"
                    element={
                      <WindowsUpdatePage />
                    }
                  />
                  <Route
                    path="/optimize/cpu-scheduler"
                    element={
                      <CpuSchedulerPage />
                    }
                  />
                  <Route
                    path="/dlss-preset"
                    element={
                      <DLSSPresetPage />
                    }
                  />
                  <Route
                    path="/audio-eq"
                    element={
                      <AudioEqPage />
                    }
                  />
                  <Route
                    path="/nvidia-driver"
                    element={
                      <NvidiaDriverPage />
                    }
                  />
                  {/* 商店版隐藏路由：整页为 NVIDIA 官网驱动下载入口（Store 10.1.5） */}
                  {!IS_STORE_BUILD && (
                    <Route
                      path="/nvidia-driver-download"
                      element={
                        <NvidiaDriverDownloadPage />
                      }
                    />
                  )}
                  <Route
                    path="/steam"
                    element={
                      <SteamPage />
                    }
                  />
                  <Route
                    path="/epic-free"
                    element={
                      <EpicFreePage />
                    }
                  />
                  <Route
                    path="/music"
                    element={
                      <MusicPage />
                    }
                  />
                  <Route
                    path="/custom"
                    element={
                      <CustomPage />
                    }
                  />
                  <Route
                    path="/speedtest"
                    element={
                      <SpeedTestPage />
                    }
                  />
                  <Route
                    path="/runtime-repair"
                    element={
                      <RuntimeRepairPage />
                    }
                  />
                  <Route
                    path="/app-manager"
                    element={
                      <AppManagerPage />
                    }
                  />
                  <Route
                    path="/vtx-virtualization"
                    element={
                      <VtxVirtualizationPage />
                    }
                  />
                  <Route
                    path="/hidden-features"
                    element={
                      <HiddenFeaturesPage />
                    }
                  />
                  <Route
                    path="/context-menu"
                    element={
                      <ContextMenuManagerPage />
                    }
                  />
                  <Route
                    path="/download-accelerator"
                    element={
                      <DownloadAcceleratorPage />
                    }
                  />
                  <Route
                    path="/nvidia-recording"
                    element={
                      <NvidiaRecordingPage />
                    }
                  />
                  <Route
                    path="/vac-repair"
                    element={
                      <VacRepairPage />
                    }
                  />
                  <Route
                    path="/time-sync"
                    element={
                      <TimeSyncPage />
                    }
                  />
                  <Route
                    path="/screen-test"
                    element={
                      <ScreenTestPage />
                    }
                  />
                  <Route
                    path="/bsod-log"
                    element={
                      <BsodLogPage />
                    }
                  />
                </Routes>
          )}
        </MainLayout>

        <UpdateModal />
        <ImportantAnnouncementModal />
        <StarPromptModal />
        <DynamicIslandHost />
        <AccelIslandBridge />
        <IslandRemoteBridge />
      </>
    </MusicProvider>
  );
}

export default App;
