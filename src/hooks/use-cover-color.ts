import { useEffect, useState } from "react";

interface CoverColor {
  hex: string;       // 主色 hex 值
  isLight: boolean;  // 是否为浅色
  rgb: [number, number, number];
}

/**
 * 从图片提取主导颜色
 * 使用 Canvas 采样像素，计算平均颜色
 */
function extractDominantColor(img: HTMLImageElement): CoverColor {
  const canvas = document.createElement("canvas");
  const size = 100; // 缩小到 100x100 加速采样
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) return { hex: "#1a1a2e", isLight: false, rgb: [26, 26, 46] };

  ctx.drawImage(img, 0, 0, size, size);
  const data = ctx.getImageData(0, 0, size, size).data;

  let r = 0, g = 0, b = 0;
  let count = 0;

  // 采样每个像素，加权中心区域（忽略边缘）
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const idx = (y * size + x) * 4;
      const alpha = data[idx + 3];
      if (alpha < 128) continue; // 跳过透明像素

      // 中心区域权重更高
      const cx = Math.abs(x - size / 2) / (size / 2);
      const cy = Math.abs(y - size / 2) / (size / 2);
      const weight = 1 - Math.min(cx + cy, 1) * 0.5;

      r += data[idx] * weight;
      g += data[idx + 1] * weight;
      b += data[idx + 2] * weight;
      count += weight;
    }
  }

  if (count === 0) return { hex: "#1a1a2e", isLight: false, rgb: [26, 26, 46] };

  r = Math.round(r / count);
  g = Math.round(g / count);
  b = Math.round(b / count);

  // 增强饱和度
  const avg = (r + g + b) / 3;
  r = Math.min(255, Math.max(0, Math.round(r + (r - avg) * 0.35)));
  g = Math.min(255, Math.max(0, Math.round(g + (g - avg) * 0.35)));
  b = Math.min(255, Math.max(0, Math.round(b + (b - avg) * 0.35)));

  const hex = "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");

  // 计算相对亮度 (WCAG)
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  const isLight = luminance > 0.55;

  return { hex, isLight, rgb: [r, g, b] };
}

const DEFAULT_COLOR: CoverColor = { hex: "#1a1a2e", isLight: false, rgb: [26, 26, 46] };

/**
 * 提取封面最右侧竖条的主导色（像素最多的量化色桶的实际均值）。
 * 封面渐染样式用：右侧背景与封面右缘主色一致，mask 羽化后无缝相融
 */
function extractRightEdgeColor(img: HTMLImageElement): CoverColor {
  const canvas = document.createElement("canvas");
  const size = 100;
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) return DEFAULT_COLOR;

  ctx.drawImage(img, 0, 0, size, size);
  const data = ctx.getImageData(0, 0, size, size).data;

  // 只统计最右 10% 竖条；每通道 4bit 量化分桶（4096 桶），取像素最多的桶
  const stripX = Math.floor(size * 0.9);
  const buckets = new Map<number, { n: number; r: number; g: number; b: number }>();
  for (let y = 0; y < size; y++) {
    for (let x = stripX; x < size; x++) {
      const idx = (y * size + x) * 4;
      if (data[idx + 3] < 128) continue;
      const key = ((data[idx] >> 4) << 8) | ((data[idx + 1] >> 4) << 4) | (data[idx + 2] >> 4);
      const b = buckets.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
      b.n++; b.r += data[idx]; b.g += data[idx + 1]; b.b += data[idx + 2];
      buckets.set(key, b);
    }
  }
  if (buckets.size === 0) return DEFAULT_COLOR;

  let best = { n: 0, r: 0, g: 0, b: 0 };
  for (const b of buckets.values()) if (b.n > best.n) best = b;

  const r = Math.round(best.r / best.n);
  const g = Math.round(best.g / best.n);
  const b = Math.round(best.b / best.n);
  const hex = "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
  const luminance = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return { hex, isLight: luminance > 0.55, rgb: [r, g, b] };
}

// 缓存：同一 URL 不重复提取，避免 Strict Mode 双调用和重复展开时重复计算
const coverColorCache = new Map<string, CoverColor>();

/**
 * 从专辑封面提取主色
 *
 * 清理策略：
 * - cancelled 标志防止卸载后 setState
 * - 断开 onload/onerror 闭包引用，允许 GC
 * - 不设置 img.src = ""（会取消图片加载）
 * - 不使用 lastUrlRef 跳过（与 Strict Mode 双调用冲突，导致图片永不加载）
 * - 用模块级缓存替代 lastUrlRef 去重
 */
export function useCoverColor(coverUrl: string): CoverColor {
  const [color, setColor] = useState<CoverColor>(() => {
    // 初始化时尝试从缓存读取，避免首次渲染闪烁
    return coverColorCache.get(coverUrl) ?? DEFAULT_COLOR;
  });

  useEffect(() => {
    if (!coverUrl) return;

    // 缓存命中：直接使用，无需创建 Image
    const cached = coverColorCache.get(coverUrl);
    if (cached) {
      setColor(cached);
      return;
    }

    let cancelled = false;
    const img = new Image();
    img.crossOrigin = "anonymous";

    img.onload = () => {
      if (cancelled) return;
      const c = extractDominantColor(img);
      if (cancelled) return;
      coverColorCache.set(coverUrl, c);
      setColor(c);
    };

    img.onerror = () => {
      if (cancelled) return;
      setColor(DEFAULT_COLOR);
    };

    img.src = coverUrl;

    return () => {
      cancelled = true;
      img.onload = null;
      img.onerror = null;
    };
  }, [coverUrl]);

  return color;
}

// 右缘主导色独立缓存（与平均主色分开，互不覆盖）
const coverEdgeColorCache = new Map<string, CoverColor>();

/**
 * 从专辑封面最右缘提取主导色（封面渐染样式专用）
 * 缓存/清理策略与 useCoverColor 相同
 */
export function useCoverEdgeColor(coverUrl: string): CoverColor {
  const [color, setColor] = useState<CoverColor>(() => {
    return coverEdgeColorCache.get(coverUrl) ?? DEFAULT_COLOR;
  });

  useEffect(() => {
    if (!coverUrl) return;

    const cached = coverEdgeColorCache.get(coverUrl);
    if (cached) {
      setColor(cached);
      return;
    }

    let cancelled = false;
    const img = new Image();
    img.crossOrigin = "anonymous";

    img.onload = () => {
      if (cancelled) return;
      const c = extractRightEdgeColor(img);
      if (cancelled) return;
      coverEdgeColorCache.set(coverUrl, c);
      setColor(c);
    };

    img.onerror = () => {
      if (cancelled) return;
      setColor(DEFAULT_COLOR);
    };

    img.src = coverUrl;

    return () => {
      cancelled = true;
      img.onload = null;
      img.onerror = null;
    };
  }, [coverUrl]);

  return color;
}
