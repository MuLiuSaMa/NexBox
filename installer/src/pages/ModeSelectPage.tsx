import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LuX } from "react-icons/lu";
import LicensePage from "./LicensePage";

interface ModeSelectPageProps {
  onQuick: () => void;
  onCustom: () => void;
  isUpgrade: boolean;
  ready: boolean;
}

export default function ModeSelectPage({ onQuick, onCustom, isUpgrade, ready }: ModeSelectPageProps) {
  const { t } = useTranslation();
  const [agreed, setAgreed] = useState(true);
  const [showLicense, setShowLicense] = useState(false);

  const enabled = agreed && ready;

  return (
    <div
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
    >
      {/* Logo 由外壳固定渲染（中间略偏上），按钮位于中下部 */}
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <div className="mode-buttons">
          <button
            type="button"
            className={`btn-primary mode-btn${enabled ? "" : " disabled"}`}
            disabled={!enabled}
            onClick={onQuick}
          >
            {isUpgrade ? t("upgrade") : t("quick_install")}
          </button>

          <button
            type="button"
            className={`btn-primary mode-btn${enabled ? "" : " disabled"}`}
            disabled={!enabled}
            onClick={onCustom}
          >
            {t("custom_install")}
          </button>
        </div>
      </div>

      <div className="agree-row">
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={agreed}
            onChange={(e) => setAgreed(e.target.checked)}
          />
          <span className="checkbox-mark" />
          <span>{t("agree_prefix")}</span>
        </label>
        <span
          className="agree-link"
          role="link"
          onClick={() => setShowLicense(true)}
        >
          {t("agree_terms_name")}
        </span>
      </div>

      {showLicense && (
        <div className="modal-overlay" onClick={() => setShowLicense(false)}>
          <div className="modal-box" onClick={(e) => e.stopPropagation()}>
            <button
              className="modal-close"
              onClick={() => setShowLicense(false)}
              aria-label={t("close")}
            >
              <LuX size={18} />
            </button>
            <LicensePage onAgreed={() => {}} />
          </div>
        </div>
      )}
    </div>
  );
}
