import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

interface InstallerLayoutProps {
  currentStep: number;
  children: ReactNode;
  canGoBack?: boolean;
  canGoNext?: boolean;
  nextLabel?: string;
  onBack?: () => void;
  onNext?: () => void;
  showCancel?: boolean;
}

export default function InstallerLayout({
  children,
  canGoBack = true,
  canGoNext = true,
  nextLabel,
  onBack,
  onNext,
  showCancel = false,
}: InstallerLayoutProps) {
  const { t } = useTranslation();

  const handleCancel = () => {
    if (window.confirm(t("安装未完成，确定要退出吗？"))) {
      window.close();
    }
  };

  // 没有任何页脚按钮时不渲染 footer，避免空白占位（如安装中/完成页）
  const hasFooterContent = showCancel || !!onBack || !!onNext;

  return (
    <div className="installer-app">
      <div className="installer-body">
        <div className="installer-content">
          {children}
        </div>
      </div>

      {hasFooterContent && (
        <div className="installer-footer">
        {showCancel && (
          <button className="btn-secondary" onClick={handleCancel} style={{ marginRight: "auto" }}>
            {t("btn_cancel")}
          </button>
        )}
        {onBack && (
          <button className="btn-secondary" onClick={onBack} disabled={!canGoBack}>
            {t("btn_back")}
          </button>
        )}
        {onNext && (
          <button className="btn-primary" onClick={onNext} disabled={!canGoNext}>
            {nextLabel || t("btn_next")}
          </button>
        )}
        </div>
      )}
    </div>
  );
}
