import { Box } from "@chakra-ui/react";
import amdLogo from "@/assets/brands/amd.svg";
import intelLogo from "@/assets/brands/intel.svg";
import nvidiaLogo from "@/assets/brands/nvidia.svg";
import asusLogo from "@/assets/brands/asus.svg";
import msiLogo from "@/assets/brands/msi.svg";
import gigabyteLogo from "@/assets/brands/gigabyte.svg";
import asrockLogo from "@/assets/brands/asrock.svg";
import zotacLogo from "@/assets/brands/zotac.svg";
import sapphireLogo from "@/assets/brands/sapphire.svg";
import xfxLogo from "@/assets/brands/xfx.svg";
import evgaLogo from "@/assets/brands/evga.svg";
import powercolorLogo from "@/assets/brands/powercolor.png";
import colorfulLogo from "@/assets/brands/colorful.png";
import galaxLogo from "@/assets/brands/galax.png";
import maxsunLogo from "@/assets/brands/maxsun.png";
import biostarLogo from "@/assets/brands/biostar.svg";

export type BrandKey =
  | "amd"
  | "intel"
  | "nvidia"
  | "asus"
  | "msi"
  | "gigabyte"
  | "asrock"
  | "zotac"
  | "sapphire"
  | "xfx"
  | "evga"
  | "powercolor"
  | "colorful"
  | "galax"
  | "maxsun"
  | "biostar";

interface BrandRule {
  key: BrandKey;
  /** 匹配规则按数组顺序优先，先板卡/主板品牌，后芯片品牌 */
  matchers: RegExp[];
  logo: string;
  /** 展示高度（px）：方形图标类略高，宽扁文字类略矮，使视觉大小接近 */
  displayH: number;
  /** 展示最大宽度（px） */
  displayMaxW?: number;
}

const BRAND_RULES: BrandRule[] = [
  { key: "asus", matchers: [/asus|asustek|华硕|rog|prime|tuf/], logo: asusLogo, displayH: 26 },
  { key: "msi", matchers: [/micro-star|\bmsi\b|微星/], logo: msiLogo, displayH: 26 },
  { key: "gigabyte", matchers: [/gigabyte|技嘉|aorus/], logo: gigabyteLogo, displayH: 15, displayMaxW: 70 },
  { key: "asrock", matchers: [/asrock|华擎/], logo: asrockLogo, displayH: 15, displayMaxW: 72 },
  { key: "zotac", matchers: [/zotac|索泰/], logo: zotacLogo, displayH: 16, displayMaxW: 72 },
  { key: "sapphire", matchers: [/sapphire|蓝宝石/], logo: sapphireLogo, displayH: 22, displayMaxW: 34 },
  { key: "xfx", matchers: [/xfx|讯景/], logo: xfxLogo, displayH: 14, displayMaxW: 70 },
  { key: "evga", matchers: [/evga/], logo: evgaLogo, displayH: 16, displayMaxW: 70 },
  { key: "powercolor", matchers: [/power ?color|撼讯/], logo: powercolorLogo, displayH: 18, displayMaxW: 70 },
  { key: "colorful", matchers: [/colorful|七彩虹|igame/], logo: colorfulLogo, displayH: 15, displayMaxW: 70 },
  { key: "galax", matchers: [/gal(?:a|l)xy|\bgalax\b|影驰/], logo: galaxLogo, displayH: 14, displayMaxW: 70 },
  { key: "maxsun", matchers: [/maxsun|铭瑄/], logo: maxsunLogo, displayH: 18, displayMaxW: 68 },
  { key: "biostar", matchers: [/biostar|映泰/], logo: biostarLogo, displayH: 15, displayMaxW: 70 },
  { key: "amd", matchers: [/amd|advanced micro|ryzen|radeon|athlon/], logo: amdLogo, displayH: 26 },
  { key: "intel", matchers: [/intel|genuineintel/], logo: intelLogo, displayH: 26 },
  { key: "nvidia", matchers: [/nvidia|geforce|rtx|gtx|quadro/], logo: nvidiaLogo, displayH: 26 },
];

export function detectBrand(text?: string | null): BrandKey | null {
  if (!text) return null;
  const lower = text.toLowerCase();
  for (const rule of BRAND_RULES) {
    for (const re of rule.matchers) {
      if (re.test(lower)) return rule.key;
    }
  }
  return null;
}

const BRAND_LOGOS: Record<BrandKey, string> = Object.fromEntries(
  BRAND_RULES.map((r) => [r.key, r.logo])
) as Record<BrandKey, string>;

const BRAND_SIZE: Record<BrandKey, { h: number; maxW?: number }> = Object.fromEntries(
  BRAND_RULES.map((r) => [r.key, { h: r.displayH, maxW: r.displayMaxW }])
) as Record<BrandKey, { h: number; maxW?: number }>;

/**
 * 品牌 Logo：根据厂商文本自动识别品牌并展示卡片右上角 Logo。
 * 图片均为透明背景，直接叠放；未识别到品牌时不渲染任何内容。
 */
export function BrandLogo({ text }: { text?: string | null }) {
  const brand = detectBrand(text);
  const logo = brand ? BRAND_LOGOS[brand] : null;
  if (!logo) return null;
  const size = BRAND_SIZE[brand];

  return (
    <Box
      as="img"
      src={logo}
      alt={brand}
      h={`${size.h}px`}
      maxW={`${size.maxW ?? 64}px`}
      objectFit="contain"
      draggable={false}
      userSelect="none"
      flexShrink={0}
    />
  );
}