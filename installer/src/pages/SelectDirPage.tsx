import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";

interface SelectDirPageProps {
  onDirChange: (dir: string) => void;
  onValidChange: (valid: boolean) => void;
  onShortcutChange?: (create: boolean) => void;
  createDesktopShortcut?: boolean;
  onInstall: () => void;
  onBack: () => void;
}

const REQUIRED_SPACE = 80_000_000;

function formatSize(bytes: number): string {
  if (bytes >= 1_000_000_000) return (bytes / 1_000_000_000).toFixed(1) + " GB";
  if (bytes >= 1_000_000) return (bytes / 1_000_000).toFixed(0) + " MB";
  if (bytes >= 1_000) return (bytes / 1_000).toFixed(0) + " KB";
  return bytes + " B";
}

export default function SelectDirPage({ onDirChange, onValidChange, onShortcutChange, createDesktopShortcut = true, onInstall, onBack }: SelectDirPageProps) {
  const { t } = useTranslation();
  const [dir, setDir] = useState("");
  const [available, setAvailable] = useState<number>(-1);
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    invoke<string>("get_default_install_path").then((path) => {
      setDir(path);
      onDirChange(path);
      checkSpace(path);
    });
  }, []);

  useEffect(() => {
    const valid = available >= REQUIRED_SPACE;
    onValidChange(valid);
  }, [available]);

  const checkSpace = async (path: string) => {
    try {
      setChecking(true);
      const space = await invoke<number>("check_disk_space", { path });
      setAvailable(space);
    } catch {
      setAvailable(0);
    } finally {
      setChecking(false);
    }
  };

  const handleBrowse = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: t("dir_title"),
      });
      if (selected) {
        const target = `${selected}\\NexBox`;
        setDir(target);
        onDirChange(target);
        setAvailable(-1);
        checkSpace(target);
      }
    } catch { }
  };

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    setDir(val);
    onDirChange(val);
    setAvailable(-1);
    checkSpace(val);
  };

  const enoughSpace = available >= REQUIRED_SPACE;
  const canInstall = enoughSpace && !checking;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        justifyContent: "center",
        gap: 16,
      }}
    >
      {/* 安装路径 */}
      <div>
        <div style={{ fontSize: 13, color: "rgba(255, 255, 255, 0.6)", marginBottom: 8 }}>
          {t("dir_title")}
        </div>
        <div className="dir-selector">
          <input
            type="text"
            value={dir}
            onChange={handleInputChange}
            placeholder="C:\\Program Files\\NexBox"
          />
          <button onClick={handleBrowse}>{t("dir_browse")}</button>
        </div>
      </div>

      {/* 空间信息 + 快捷方式 */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 24 }}>
        <div style={{ fontSize: 13, color: "rgba(255, 255, 255, 0.6)" }}>
          {t("dir_space")}：
          <span style={{ color: "#ffffff", fontWeight: 600 }}>
            {checking ? <span className="space-spinner" /> : formatSize(available)}
          </span>
        </div>
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={createDesktopShortcut}
            onChange={(e) => onShortcutChange?.(e.target.checked)}
          />
          <span className="checkbox-mark" />
          {t("创建桌面快捷方式")}
        </label>
      </div>

      {!enoughSpace && !checking && (
        <div style={{ fontSize: 13, color: "rgba(255, 255, 255, 0.75)" }}>
          {t("error_space")}
        </div>
      )}

      <div style={{ display: "flex", gap: 12, width: "100%", maxWidth: 360, alignSelf: "center" }}>
        <button
          type="button"
          className="btn-secondary mode-btn"
          onClick={onBack}
          style={{ flex: 1 }}
        >
          {t("btn_back")}
        </button>
        <button
          type="button"
          className="btn-primary mode-btn"
          disabled={!canInstall}
          onClick={onInstall}
          style={{ flex: 1 }}
        >
          {t("btn_install")}
        </button>
      </div>
    </div>
  );
}
