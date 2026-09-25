import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export default function CompletePage() {
  const { t } = useTranslation();

  const handleClose = async () => {
    // Spawn detached cleanup batch first, then close immediately
    invoke("self_delete").catch(() => {});
    getCurrentWindow().close().catch(() => {});
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
      <button
        type="button"
        className="btn-primary mode-btn"
        onClick={handleClose}
        style={{ maxWidth: 360, alignSelf: "center", width: "100%" }}
      >
        {t("complete_btn")}
      </button>
    </div>
  );
}
