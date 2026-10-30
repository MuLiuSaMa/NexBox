import type { ComponentType } from "react";

export interface ViewItem {
  id: string;
  path: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number; color?: string }>;
  titleKey: string;
  descriptionKey: string;
  color: string;
  /** 卡片右上角徽标文字；不传但 beta=true 时显示 BETA */
  badge?: string;
  beta?: boolean;
}
