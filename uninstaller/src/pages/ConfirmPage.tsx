import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";

interface ConfirmPageProps {
  onStart: () => void;
}

export default function ConfirmPage({ onStart }: ConfirmPageProps) {
  const { t } = useTranslation();
  const [installDir, setInstallDir] = useState("");

  useEffect(() => {
    invoke<{ install_dir: string }>("get_install_info")
      .then((info) => setInstallDir(info.install_dir))
      .catch(() => setInstallDir("未知"));
  }, []);

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
      <div className="glass-card">
        <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none" style={{ flexShrink: 0, marginTop: 1 }}>
            <circle cx="10" cy="10" r="9" stroke="#ffffff" strokeWidth="1.5" />
            <path d="M10 6v5" stroke="#ffffff" strokeWidth="1.5" strokeLinecap="round" />
            <circle cx="10" cy="14" r="1" fill="#ffffff" />
          </svg>
          <div>
            <p style={{ fontSize: 13, color: "rgba(255, 255, 255, 0.75)", lineHeight: 1.6, marginBottom: 8 }}>
              {t("confirm_warning")}
            </p>
            <p style={{ fontSize: 12, color: "rgba(255, 255, 255, 0.55)" }}>
              {t("confirm_location")}{" "}
              <span style={{ color: "#ffffff", fontWeight: 500 }}>{installDir}</span>
            </p>
          </div>
        </div>
      </div>

      <button
        type="button"
        className="btn-primary mode-btn"
        onClick={onStart}
        style={{ maxWidth: 360, alignSelf: "center", width: "100%", marginTop: 4 }}
      >
        {t("confirm_btn")}
      </button>
    </div>
  );
}
