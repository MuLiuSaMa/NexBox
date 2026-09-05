import { useState } from "react";
import { motion } from "framer-motion";
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
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -20 }}
      transition={{ duration: 0.3 }}
      style={{ display: "flex", flexDirection: "column", flex: 1 }}
    >
      <div style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 28,
      }}>
        <motion.div
          initial={{ scale: 0.85, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          transition={{ type: "spring", stiffness: 180, delay: 0.1 }}
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 16,
          }}
        >
          <img
            src="/logo/NexBoxW.png"
            alt="NexBox"
            style={{ width: 72, height: 72, objectFit: "contain" }}
          />
          <img
            src="/logo/Chinesew.png"
            alt="NexBox"
            style={{ height: 46, objectFit: "contain" }}
          />
        </motion.div>

        <div className="mode-buttons">
          <motion.button
            type="button"
            className={`btn-primary mode-btn${enabled ? "" : " disabled"}`}
            disabled={!enabled}
            whileTap={enabled ? { scale: 0.98 } : undefined}
            onClick={onQuick}
          >
            {isUpgrade ? t("upgrade") : t("quick_install")}
          </motion.button>

          <motion.button
            type="button"
            className={`btn-primary mode-btn${enabled ? "" : " disabled"}`}
            disabled={!enabled}
            whileTap={enabled ? { scale: 0.98 } : undefined}
            onClick={onCustom}
          >
            {t("custom_install")}
          </motion.button>
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
    </motion.div>
  );
}