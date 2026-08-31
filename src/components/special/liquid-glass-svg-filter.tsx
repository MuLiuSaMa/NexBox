"use client";

import { useEffect, useRef, useState } from "react";

const FILTER_ID = "nexbox-liquid-glass-filter";

// 灵动岛专用折射滤镜：全局滤镜 scale=1 几乎不可见，灵动岛需要肉眼可见的边缘折射；
// 且胶囊只有 30px 高，位移贴图需要更宽的垂直边缘带才能在矮胶囊上呈现折射
export const ISLAND_FILTER_ID = "nexbox-island-glass-filter";
const ISLAND_MAP_W = 400, ISLAND_MAP_H = 100, ISLAND_MAP_E = 20; // 水平折射带 5% 宽（匹配胶囊圆头），垂直折射带 20% 高
const ISLAND_SCALE = 30; // 折射强度（px）：边缘最大位移 = ISLAND_SCALE / 2，观感偏强/偏弱只调这里

export function supportsSvgBackdropFilter(): boolean {
  try {
    const ua = navigator.userAgent || "";
    if ((/Safari/.test(ua) && !/Chrome/.test(ua)) || /Firefox/.test(ua)) return false;
    const div = document.createElement("div");
    div.style.backdropFilter = `url(#${FILTER_ID})`;
    return div.style.backdropFilter !== "";
  } catch {
    return false;
  }
}

/**
 * 位移贴图：中心全覆盖 rgb(128,128,128) = 零位移
 * 仅最外层窄边有 R/B 梯度 → 边缘折射背景内容
 * feDisplacementMap scale 控制折射强度
 */
function generateDisplacementMap(W = 400, H = 200, E = 10): string {

  const svg =
    `<svg viewBox="0 0 ${W} ${H}" xmlns="http://www.w3.org/2000/svg">` +
    `<defs>` +
    // 上边：B 0→128（背景向上折射 → 向外）
    `<linearGradient id="et" x1="0" y1="0" x2="0" y2="1">` +
    `<stop offset="0%" stop-color="rgb(128,128,0)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    // 下边：B 255→128（背景向下折射）
    `<linearGradient id="eb" x1="0" y1="1" x2="0" y2="0">` +
    `<stop offset="0%" stop-color="rgb(128,128,255)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    // 左边：R 0→128（背景向左折射）
    `<linearGradient id="el" x1="0" y1="0" x2="1" y2="0">` +
    `<stop offset="0%" stop-color="rgb(0,128,128)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    // 右边：R 255→128（背景向右折射）
    `<linearGradient id="er" x1="1" y1="0" x2="0" y2="0">` +
    `<stop offset="0%" stop-color="rgb(255,128,128)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    // 四角同时有 R+B 折射
    `<linearGradient id="ec-tl" x1="0" y1="0" x2="1" y2="1">` +
    `<stop offset="0%" stop-color="rgb(0,128,0)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    `<linearGradient id="ec-tr" x1="1" y1="0" x2="0" y2="1">` +
    `<stop offset="0%" stop-color="rgb(255,128,0)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    `<linearGradient id="ec-bl" x1="0" y1="1" x2="1" y2="0">` +
    `<stop offset="0%" stop-color="rgb(0,128,255)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    `<linearGradient id="ec-br" x1="1" y1="1" x2="0" y2="0">` +
    `<stop offset="0%" stop-color="rgb(255,128,255)"/><stop offset="100%" stop-color="rgb(128,128,128)"/>` +
    `</linearGradient>` +
    `</defs>` +
    // 基底：全覆盖零位移
    `<rect width="${W}" height="${H}" fill="rgb(128,128,128)"/>` +
    // 四边折射带
    `<rect x="0" y="0"     width="${W}" height="${E}" fill="url(#et)"/>` +
    `<rect x="0" y="${H - E}" width="${W}" height="${E}" fill="url(#eb)"/>` +
    `<rect x="0" y="0"     width="${E}" height="${H}" fill="url(#el)"/>` +
    `<rect x="${W - E}" y="0" width="${E}" height="${H}" fill="url(#er)"/>` +
    // 四角补丁
    `<rect x="0"             y="0"             width="${E}" height="${E}" fill="url(#ec-tl)"/>` +
    `<rect x="${W - E}"      y="0"             width="${E}" height="${E}" fill="url(#ec-tr)"/>` +
    `<rect x="0"             y="${H - E}"      width="${E}" height="${E}" fill="url(#ec-bl)"/>` +
    `<rect x="${W - E}"      y="${H - E}"      width="${E}" height="${E}" fill="url(#ec-br)"/>` +
    `</svg>`;

  return "data:image/svg+xml," + encodeURIComponent(svg);
}

export function LiquidGlassSvgFilter() {
  const mapRef = useRef<string>("");

  useEffect(() => {
    mapRef.current = generateDisplacementMap();
    const islandMap = generateDisplacementMap(ISLAND_MAP_W, ISLAND_MAP_H, ISLAND_MAP_E);
    const maps: Array<[string, string]> = [
      ["nexbox-glass-displacement-map", mapRef.current],
      ["nexbox-island-displacement-map", islandMap],
    ];
    for (const [id, href] of maps) {
      const img = document.getElementById(id);
      if (img) {
        img.setAttribute("href", href);
        try { img.setAttributeNS("http://www.w3.org/1999/xlink", "href", href); } catch { /* */ }
      }
    }
  }, []);

  return (
    <svg className="real-liquid-glass-svg-defs" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">
      <defs>
        <filter id={FILTER_ID} colorInterpolationFilters="sRGB" x="-50%" y="-50%" width="200%" height="200%">
          <feImage
            id="nexbox-glass-displacement-map"
            x="0" y="0" width="100%" height="100%"
            preserveAspectRatio="none"
            result="map"
          />
          {/* 中心=128 零位移，边缘=非128 折射 */}
          <feDisplacementMap
            in="SourceGraphic"
            in2="map"
            scale="1"
            xChannelSelector="R"
            yChannelSelector="B"
          />
        </filter>
        {/* 灵动岛专用：强折射 + 为矮胶囊加宽的垂直边缘带 */}
        <filter id={ISLAND_FILTER_ID} colorInterpolationFilters="sRGB" x="-50%" y="-50%" width="200%" height="200%">
          <feImage
            id="nexbox-island-displacement-map"
            x="0" y="0" width="100%" height="100%"
            preserveAspectRatio="none"
            result="map"
          />
          <feDisplacementMap
            in="SourceGraphic"
            in2="map"
            scale={ISLAND_SCALE}
            xChannelSelector="R"
            yChannelSelector="B"
          />
        </filter>
      </defs>
    </svg>
  );
}

export function useLiquidGlassRefraction(_enabled?: boolean) {
  const [svgSupported] = useState(() => supportsSvgBackdropFilter());
  return { svgSupported };
}
