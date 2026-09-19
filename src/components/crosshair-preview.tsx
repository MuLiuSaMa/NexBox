import { convertFileSrc } from "@tauri-apps/api/core";
import type { ReactNode } from "react";
import type { CrosshairSettings } from "@/pages/CrosshairPage";

/** 与 Rust 端 Preset_* 样式对应的预设图片文件名（用于预览路径） */
const PRESET_IMAGE_FILES: Record<string, string> = {
  Preset_cat: "cat.png",
  Preset_donk: "donk.png",
  Preset_s1mple: "s1mple.png",
  Preset_ropz: "ropz.png",
  "Preset_MMW2.0": "MMW2.0.png",
  Preset_SSCJ: "SSCJ.png",
  "Preset_T字准星": "TShape.png",
};

interface CrosshairPreviewProps {
  settings: CrosshairSettings;
  /** 透明背景（不加深色底与参考线），用于小尺寸缩略图 */
  transparent?: boolean;
  /** viewBox 留白（越大则准心在画面中越小），默认 6 */
  padding?: number;
}

/**
 * 按 Rust 叠加层（crosshair.rs）的绘制逻辑在 SVG 中还原准心效果，
 * 用于页面内实时预览与预设缩略图。SVG 填满父容器，内容自动居中。
 */
export default function CrosshairPreview({
  settings,
  transparent = false,
  padding = 6,
}: CrosshairPreviewProps) {
  const {
    style,
    size,
    thickness,
    color,
    gap,
    dot_size,
    opacity,
    use_custom_image,
    custom_image_path,
    outline_enabled,
    outline_color,
    outline_thickness,
  } = settings;

  const svgBg = transparent ? "transparent" : "#0d0d0d";

  const backdropGuides = (extent: number) => (
    <g opacity={0.14} stroke="#ffffff" strokeWidth={0.75}>
      <line x1={-extent} y1="0" x2={extent} y2="0" />
      <line x1="0" y1={-extent} x2="0" y2={extent} />
    </g>
  );

  // 自定义图片 / 预设图片：叠加层按 size×size 拉伸绘制，且不应用透明度
  if (use_custom_image) {
    const src = style.startsWith("Preset_")
      ? `/crosshair-presets/${PRESET_IMAGE_FILES[style] ?? ""}`
      : custom_image_path
        ? convertFileSrc(custom_image_path)
        : null;
    const half = Math.max(size, 8) / 2;
    const extent = half + padding;
    return (
      <svg
        width="100%"
        height="100%"
        viewBox={`${-extent} ${-extent} ${extent * 2} ${extent * 2}`}
        preserveAspectRatio="xMidYMid meet"
        style={{ display: "block", background: svgBg }}
      >
        {!transparent && backdropGuides(extent)}
        {src ? (
          <image href={src} x={-half} y={-half} width={size} height={size} />
        ) : null}
      </svg>
    );
  }

  const outlineVisible = outline_enabled && outline_thickness > 0;
  const outlineW = thickness + outline_thickness * 2;

  // 轮廓（描边）与本体共用同一条线段集合，只是笔宽/颜色不同
  const crossSegments = (
    <>
      <line x1="0" y1={-gap - size} x2="0" y2={-gap} />
      <line x1="0" y1={gap} x2="0" y2={gap + size} />
      <line x1={-gap - size} y1="0" x2={-gap} y2="0" />
      <line x1={gap} y1="0" x2={gap + size} y2="0" />
    </>
  );
  const outlineCross = (
    <g
      stroke={outline_color}
      strokeWidth={outlineW}
      strokeLinecap="round"
      strokeLinejoin="miter"
    >
      {crossSegments}
    </g>
  );
  const mainCross = (
    <g
      stroke={color}
      strokeWidth={thickness}
      strokeLinecap="round"
      strokeLinejoin="miter"
    >
      {crossSegments}
    </g>
  );

  const dotOutline = outlineVisible ? (
    <circle r={(dot_size + outline_thickness * 2) / 2} fill={outline_color} />
  ) : null;
  const dotFill = <circle r={dot_size / 2} fill={color} />;

  const boxRect = (
    <rect x={-size} y={-size} width={size * 2} height={size * 2} />
  );
  const outlineBox = (
    <g stroke={outline_color} strokeWidth={outlineW} strokeLinejoin="miter">
      {boxRect}
    </g>
  );
  const mainBox = (
    <g stroke={color} strokeWidth={thickness} strokeLinejoin="miter">
      {boxRect}
    </g>
  );

  let content: ReactNode;
  switch (style) {
    case "Cross":
      content = (
        <>
          {outlineVisible && outlineCross}
          {mainCross}
        </>
      );
      break;
    case "Dot":
      content = (
        <>
          {dotOutline}
          {dotFill}
        </>
      );
      break;
    case "Circle":
      content = (
        <>
          {outlineVisible && (
            <circle r={size} stroke={outline_color} strokeWidth={outlineW} />
          )}
          <circle r={size} stroke={color} strokeWidth={thickness} />
        </>
      );
      break;
    case "CrossDot":
      content = (
        <>
          {outlineVisible && outlineCross}
          {mainCross}
          {dotOutline}
          {dotFill}
        </>
      );
      break;
    case "CircleCross":
      content = (
        <>
          {outlineVisible && (
            <circle r={size} stroke={outline_color} strokeWidth={outlineW} />
          )}
          <circle r={size} stroke={color} strokeWidth={thickness} />
          {outlineVisible && outlineCross}
          {mainCross}
        </>
      );
      break;
    case "DotBox":
      content = (
        <>
          {outlineVisible && outlineBox}
          {mainBox}
          {dotOutline}
          {dotFill}
        </>
      );
      break;
    default:
      // 未知样式兜底为「+」
      content = <g stroke={color} strokeWidth={thickness} strokeLinecap="round"><line x1={-size} y1="0" x2={size} y2="0" /><line x1="0" y1={-size} x2="0" y2={size} /></g>;
  }

  // 计算绘制范围：线条/圆环最远像素 + 描边半宽 + 留白
  const halfOutline = outlineVisible ? outlineW / 2 : thickness / 2;
  const crossExt = gap + size + halfOutline;
  const circleExt = size + halfOutline;
  const dotExt = dot_size / 2 + (outlineVisible ? outline_thickness : 0);
  const extent = Math.max(crossExt, circleExt, dotExt, 1) + padding;

  return (
    <svg
      width="100%"
      height="100%"
      viewBox={`${-extent} ${-extent} ${extent * 2} ${extent * 2}`}
      preserveAspectRatio="xMidYMid meet"
      style={{ display: "block", background: svgBg }}
    >
      {!transparent && backdropGuides(extent)}
      <g opacity={opacity / 255}>{content}</g>
    </svg>
  );
}