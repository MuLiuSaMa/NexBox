import { useState, useCallback, useEffect } from "react";
import { AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import TitleBar from "./components/TitleBar";
import InstallerLayout from "./components/InstallerLayout";
import ModeSelectPage from "./pages/ModeSelectPage";
import SelectDirPage from "./pages/SelectDirPage";
import InstallingPage from "./pages/InstallingPage";
import FinishPage from "./pages/FinishPage";

export default function App() {
  const { t } = useTranslation();
  const [step, setStep] = useState(1);
  const [targetDir, setTargetDir] = useState("");
  const [dirValid, setDirValid] = useState(false);
  const [createDesktopShortcut, setCreateDesktopShortcut] = useState(true);
  const [defaultDir, setDefaultDir] = useState("");
  const [isExisting, setIsExisting] = useState(false);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState("");

  // 启动时获取默认（或已装）安装目录，并检测电脑上是否已安装
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<string>("get_default_install_path"),
      invoke<boolean>("is_existing_install"),
    ])
      .then(([dir, existing]) => {
        if (cancelled) return;
        setDefaultDir(dir);
        setIsExisting(existing);
        setReady(true);
      })
      .catch(() => {
        // 查询失败也放行：SelectDirPage 挂载时会自行查询，仅快速安装按钮暂不可用
        if (cancelled) return;
        setReady(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 快速安装 / 升级：默认目录 + 创建桌面快捷方式，直接进入安装
  const handleQuick = useCallback(() => {
    if (!defaultDir) return;
    setTargetDir(defaultDir);
    setCreateDesktopShortcut(true);
    setStep(3);
  }, [defaultDir]);

  // 自定义安装：进入选择安装位置页
  const handleCustom = useCallback(() => {
    setStep(2);
  }, []);

  const handleNext = useCallback(() => {
    if (step === 2 && !dirValid) return;
    if (step < 3) setStep((s) => s + 1);
  }, [step, dirValid]);

  const handleBack = useCallback(() => {
    if (step > 1) setStep((s) => s - 1);
  }, [step]);

  const handleInstallComplete = useCallback(() => {
    setStep(4);
  }, []);

  const handleInstallError = useCallback((msg: string) => {
    setError(msg);
    alert(msg);
  }, []);

  const nextLabel = step === 2 ? t("btn_install") : undefined;

  return (
    <>
      <TitleBar />
      <InstallerLayout
        currentStep={step}
        canGoBack={step === 2}
        canGoNext={step === 2 ? dirValid : false}
        nextLabel={nextLabel}
        onBack={step === 2 ? handleBack : undefined}
        onNext={step === 2 ? handleNext : undefined}
        showCancel={false}
      >
        {/* 页面切换动画保留 framer-motion；玻璃卡片的背景模糊通过 CSS animation
            延迟到页面动画结束后再平滑浮现，避免合成层内 backdrop-filter 采样失败
            导致的「先透明、动画结束瞬间变模糊」（WebView2/Chromium 已知行为） */}
        <AnimatePresence mode="wait">
          {step === 1 && (
            <ModeSelectPage
              key="mode"
              onQuick={handleQuick}
              onCustom={handleCustom}
              isUpgrade={isExisting}
              ready={ready}
            />
          )}
          {step === 2 && (
            <SelectDirPage
              key="dir"
              onDirChange={setTargetDir}
              onValidChange={setDirValid}
              onShortcutChange={setCreateDesktopShortcut}
              createDesktopShortcut={createDesktopShortcut}
            />
          )}
          {step === 3 && (
            <InstallingPage
              key="install"
              targetDir={targetDir}
              createDesktopShortcut={createDesktopShortcut}
              onComplete={handleInstallComplete}
              onError={handleInstallError}
            />
          )}
          {step === 4 && (
            <FinishPage key="finish" targetDir={targetDir} />
          )}
        </AnimatePresence>
      </InstallerLayout>
    </>
  );
}