import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import TitleBar from "./components/TitleBar";
import InstallerLayout from "./components/InstallerLayout";
import ModeSelectPage from "./pages/ModeSelectPage";
import SelectDirPage from "./pages/SelectDirPage";
import InstallingPage from "./pages/InstallingPage";
import FinishPage from "./pages/FinishPage";

export default function App() {
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

  // 自定义页内点击「安装」：目录合法才进入安装
  const handleInstallFromCustom = useCallback(() => {
    if (!dirValid) return;
    setStep(3);
  }, [dirValid]);

  // 自定义页「返回」：回到首页模式选择
  const handleBackToMode = useCallback(() => {
    setStep(1);
  }, []);

  const handleInstallComplete = useCallback(() => {
    setStep(4);
  }, []);

  const handleInstallError = useCallback((msg: string) => {
    setError(msg);
    alert(msg);
  }, []);

  return (
    <>
      <TitleBar />
      <InstallerLayout>
        {step === 1 && (
          <ModeSelectPage
            onQuick={handleQuick}
            onCustom={handleCustom}
            isUpgrade={isExisting}
            ready={ready}
          />
        )}
        {step === 2 && (
          <SelectDirPage
            onDirChange={setTargetDir}
            onValidChange={setDirValid}
            onShortcutChange={setCreateDesktopShortcut}
            createDesktopShortcut={createDesktopShortcut}
            onInstall={handleInstallFromCustom}
            onBack={handleBackToMode}
          />
        )}
        {step === 3 && (
          <InstallingPage
            targetDir={targetDir}
            createDesktopShortcut={createDesktopShortcut}
            onComplete={handleInstallComplete}
            onError={handleInstallError}
          />
        )}
        {step === 4 && <FinishPage targetDir={targetDir} />}
      </InstallerLayout>
    </>
  );
}