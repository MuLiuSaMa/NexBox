import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface FinishPageProps {
  targetDir: string;
}

export default function FinishPage({ targetDir }: FinishPageProps) {
  const { t } = useTranslation();

  // 关闭前统一调度安装程序自删除（更新场景清理）
  const closeSelf = async () => {
    try {
      await invoke("schedule_installer_cleanup");
    } catch { }
    try {
      await getCurrentWindow().close();
    } catch { }
  };

  const handleLaunch = async () => {
    try {
      await invoke("launch_installed_app", { targetDir });
    } catch { }
    await closeSelf();
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        justifyContent: "center",
      }}
    >
      <div style={{ display: "flex", gap: 12, width: "100%", maxWidth: 360, alignSelf: "center" }}>
        <button
          type="button"
          className="btn-secondary mode-btn"
          onClick={closeSelf}
          style={{ flex: 1 }}
        >
          {t("btn_cancel")}
        </button>
        <button
          type="button"
          className="btn-primary mode-btn"
          onClick={handleLaunch}
          style={{ flex: 1 }}
        >
          {t("launch_now")}
        </button>
      </div>
    </div>
  );
}
