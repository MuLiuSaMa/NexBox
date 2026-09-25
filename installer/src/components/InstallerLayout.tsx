import type { ReactNode } from "react";

interface InstallerLayoutProps {
  children: ReactNode;
}

/**
 * 持久化外壳：左上角小号品牌标（图标 + 中文标准字，绝对定位不占布局），
 * 中间略偏上为居中标语图，下方内容区按安装步骤切换。
 */
export default function InstallerLayout({ children }: InstallerLayoutProps) {
  return (
    <div className="installer-app">
      <div className="brand-corner">
        <img
          className="corner-icon"
          src="/logo/NexBoxW.webp"
          alt="NexBox"
          draggable={false}
        />
        <img
          className="corner-word"
          src="/logo/Chinesew.webp"
          alt=""
          draggable={false}
        />
      </div>

      <div className="tagline">
        <img src="/logo/tagline.png" alt="" draggable={false} />
      </div>

      <div className="installer-content">{children}</div>
    </div>
  );
}
