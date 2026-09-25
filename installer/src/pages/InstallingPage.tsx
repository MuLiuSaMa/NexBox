import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface InstallingPageProps {
  targetDir: string;
  createDesktopShortcut: boolean;
  onComplete: () => void;
  onError: (msg: string) => void;
}

export default function InstallingPage({
  targetDir,
  createDesktopShortcut,
  onComplete,
  onError,
}: InstallingPageProps) {
  const { t } = useTranslation();
  const [progress, setProgress] = useState(0);
  const hasStarted = useRef(false);

  useEffect(() => {
    if (hasStarted.current) return;
    hasStarted.current = true;

    const doInstall = async () => {
      try {
        for (let i = 0; i <= 70; i += 5) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 30));
        }

        await invoke("install", {
          targetDir,
          createDesktopShortcut,
        });

        for (let i = 70; i <= 85; i += 3) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 20));
        }

        for (let i = 85; i < 100; i += 2) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 15));
        }

        setProgress(100);
        await new Promise((r) => setTimeout(r, 400));
        onComplete();
      } catch (err: any) {
        onError(String(err));
      }
    };

    doInstall();
  }, []);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {/* Logo 正下方：细白进度条 + 上方小字（左状态、右百分比） */}
      <div className="install-progress">
        <div className="progress-meta">
          <span className="progress-label">{t("install_desc")}</span>
          <span className="progress-value">{progress}%</span>
        </div>
        <div className="progress-bar">
          <div className="progress-bar-fill" style={{ width: `${progress}%` }} />
        </div>
      </div>
    </div>
  );
}
