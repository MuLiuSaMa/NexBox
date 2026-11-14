/**
 * 桌面歌词渲染组件
 *
 * 特性：
 * - 当前行使用双层叠加实现卡拉OK逐字高亮效果
 * - 支持单行(仅当前句)和双行(当前句+下一句)模式
 * - 文字描边保证在任意背景下可读
 * - RAF 更新进度，60fps 流畅
 */

import { useEffect, useRef, memo, useMemo, useCallback } from "react";
import type { KaraokeLine } from "@/types/music";
import { getLineProgress, calculateScrollOffset, measureScrollRange } from "@/lib/karaoke-lyrics";

interface LyricsCanvasProps {
  lines: KaraokeLine[];
  currentTime: number;
  fontSize: number;
  fontFamily: string;
  highlightColor: string;
  baseColor: string;
  lineCount: 1 | 2;
  isPlaying: boolean;
  showTranslation: boolean;
}

/** 计算当前活跃行索引 */
function calcActiveIndex(lines: KaraokeLine[], currentTime: number): number {
  if (lines.length === 0) return -1;
  let idx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].time <= currentTime) idx = i;
    else break;
  }
  return idx;
}

/** 当前行的卡拉OK效果渲染 */
function ActiveKaraokeLine({
  line,
  nextLine,
  currentTime,
  fontSize,
  fontFamily,
  highlightColor,
  baseColor,
  isPlaying,
  showTranslation,
}: {
  line: KaraokeLine;
  nextLine?: KaraokeLine;
  currentTime: number;
  fontSize: number;
  fontFamily: string;
  highlightColor: string;
  baseColor: string;
  isPlaying: boolean;
  showTranslation: boolean;
}) {
  const overlayRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLSpanElement>(null);
  /** 可滚动区间：from = 行首贴齐左边缘，to = 行尾贴齐右边缘 */
  const scrollRangeRef = useRef({ from: 0, to: 0 });
  const currentTimeRef = useRef(currentTime);
  currentTimeRef.current = currentTime;

  // 文字描边样式
  const textShadow = `
    -1px -1px 0 rgba(0,0,0,0.8),
    1px -1px 0 rgba(0,0,0,0.8),
    -1px 1px 0 rgba(0,0,0,0.8),
    1px 1px 0 rgba(0,0,0,0.8),
    0 0 8px rgba(0,0,0,0.5)
  `;

  // 按当前进度刷新高亮宽度 + 超长行的横向滚动位置
  const applyProgress = useCallback(() => {
    const overlay = overlayRef.current;
    if (!overlay) return;

    const progress = getLineProgress(line, nextLine, currentTimeRef.current);
    overlay.style.width = `${(progress * 100).toFixed(2)}%`;

    // 短行 from/to 都是 0，偏移归零，避免残留上一行的平移量
    const scroll = scrollRef.current;
    if (scroll) {
      const { from, to } = scrollRangeRef.current;
      const offset = calculateScrollOffset(progress, from, to);
      scroll.style.transform = `translate3d(${offset.toFixed(2)}px, 0, 0)`;
    }
  }, [line, nextLine]);

  // 测量可滚动区间：文案、字号、字体加载、容器宽度都会改变文字实际宽度
  useEffect(() => {
    const scroll = scrollRef.current;
    const container = containerRef.current;
    if (!scroll || !container) return;

    const measure = () => {
      scrollRangeRef.current = measureScrollRange(scroll, container);
      applyProgress();
    };
    measure();
    const timer = window.setTimeout(measure, 120);
    // 自定义字体（MiSans 等）加载完成后文字宽度会变，需要重测，
    // 否则按回退字体量出的区间偏小，长句仍是露不全
    let cancelled = false;
    document.fonts?.ready
      .then(() => {
        if (!cancelled) measure();
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [line.text, fontSize, applyProgress]);

  // 播放时逐帧跟随；暂停时也定位一次，避免停在中部被裁切的状态
  useEffect(() => {
    applyProgress();
    if (!isPlaying) return;

    let running = true;
    let rafId = 0;
    const tick = () => {
      if (!running) return;
      applyProgress();
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);

    return () => {
      running = false;
      cancelAnimationFrame(rafId);
    };
  }, [applyProgress, isPlaying]);

  const sharedStyle: React.CSSProperties = {
    display: "inline-block",
    whiteSpace: "nowrap",
    fontSize: `${fontSize}px`,
    fontFamily: fontFamily || undefined,
    fontWeight: "bold",
    lineHeight: 1.4,
    letterSpacing: "0px",
    textShadow,
  };

  const maskFade = Math.min(18, Math.max(6, fontSize * 0.3));
  const maskGradient = `linear-gradient(90deg, #000 0%, #000 calc(100% - ${maskFade}px), rgba(0,0,0,0.3) 100%)`;

  return (
    <div
      ref={containerRef}
      style={{
        textAlign: "center",
        overflow: "hidden",
        position: "relative",
        padding: "2px 0",
      }}
    >
      <span
        ref={scrollRef}
        style={{
          display: "inline-block",
          position: "relative",
          whiteSpace: "nowrap",
          transform: "translate3d(0, 0, 0)",
          willChange: "transform",
        }}
      >
        {/* 底层：完整歌词，暗色 */}
        <span style={{ ...sharedStyle, color: baseColor }}>
          {line.text}
        </span>
        {/* 顶层：高亮歌词，用 width 裁剪 */}
        <span
          ref={overlayRef}
          style={{
            ...sharedStyle,
            position: "absolute",
            top: 0,
            left: 0,
            width: "0%",
            overflow: "hidden",
            color: highlightColor,
            maskImage: maskGradient,
            WebkitMaskImage: maskGradient,
          }}
        >
          {line.text}
        </span>
      </span>
      {/* 当前行翻译：清晰硬描边 + 更大字号 */}
      {showTranslation && line.translation && (
        <div
          style={{
            marginTop: "2px",
            fontSize: `${Math.max(18, Math.round(fontSize * 0.65))}px`,
            fontFamily: fontFamily || undefined,
            color: baseColor,
            fontWeight: "bold",
            lineHeight: 1.3,
            textShadow: `
              -1px -1px 0 rgba(0,0,0,0.9),
              1px -1px 0 rgba(0,0,0,0.9),
              -1px 1px 0 rgba(0,0,0,0.9),
              1px 1px 0 rgba(0,0,0,0.9)
            `,
            WebkitFontSmoothing: "antialiased",
            textAlign: "center",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            maxWidth: "100%",
          }}
        >
          {line.translation}
        </div>
      )}
    </div>
  );
}

function LyricsCanvasInner({
  lines,
  currentTime,
  fontSize,
  fontFamily,
  highlightColor,
  baseColor,
  lineCount,
  isPlaying,
  showTranslation,
}: LyricsCanvasProps) {
  const activeIndex = useMemo(
    () => calcActiveIndex(lines, currentTime),
    [lines, currentTime]
  );

  // 空歌词占位
  if (lines.length === 0 || activeIndex < 0) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          width: "100%",
        }}
      >
        <span
          style={{
            fontSize: `${fontSize * 0.7}px`,
            fontFamily: fontFamily || undefined,
            color: baseColor,
            fontWeight: "bold",
            textShadow: `
              -1px -1px 0 rgba(0,0,0,0.8),
              1px -1px 0 rgba(0,0,0,0.8),
              -1px 1px 0 rgba(0,0,0,0.8),
              1px 1px 0 rgba(0,0,0,0.8)
            `,
          }}
        >
          ♪ 暂无歌词 ♪
        </span>
      </div>
    );
  }

  const currentLine = lines[activeIndex];
  const nextLine = lines[activeIndex + 1];

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        width: "100%",
        gap: "4px",
      }}
    >
      <ActiveKaraokeLine
        line={currentLine}
        nextLine={nextLine}
        currentTime={currentTime}
        fontSize={fontSize}
        fontFamily={fontFamily}
        highlightColor={highlightColor}
        baseColor={baseColor}
        isPlaying={isPlaying}
        showTranslation={showTranslation}
      />

      {lineCount === 2 && nextLine && (
        <div
          style={{
            overflow: "hidden",
            textAlign: "center",
            padding: "2px 0",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
          }}
        >
          <span
            style={{
              display: "inline-block",
              whiteSpace: "nowrap",
              fontSize: `${fontSize * 0.7}px`,
              fontFamily: fontFamily || undefined,
              fontWeight: "bold",
              color: baseColor,
              opacity: 0.6,
              textShadow: `
                -1px -1px 0 rgba(0,0,0,0.6),
                1px -1px 0 rgba(0,0,0,0.6),
                -1px 1px 0 rgba(0,0,0,0.6),
                1px 1px 0 rgba(0,0,0,0.6)
              `,
            }}
          >
            {nextLine.text}
          </span>
          {showTranslation && nextLine.translation && (
            <span
              style={{
                display: "block",
                maxWidth: "100%",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                fontSize: `${Math.max(14, Math.round(fontSize * 0.48))}px`,
                fontFamily: fontFamily || undefined,
                fontWeight: "bold",
                color: baseColor,
                opacity: 0.6,
                lineHeight: 1.3,
                textShadow: `
                  -1px -1px 0 rgba(0,0,0,0.8),
                  1px -1px 0 rgba(0,0,0,0.8),
                  -1px 1px 0 rgba(0,0,0,0.8),
                  1px 1px 0 rgba(0,0,0,0.8)
                `,
                WebkitFontSmoothing: "antialiased",
              }}
            >
              {nextLine.translation}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

export const LyricsCanvas = memo(LyricsCanvasInner);
