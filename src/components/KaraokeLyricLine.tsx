/**
 * 卡拉OK歌词行组件
 *
 * 渲染方案：双层叠加
 * - 底层：完整歌词，暗色（未唱）
 * - 顶层：相同歌词，高亮色（已唱），用 width% 裁剪
 * - RAF 更新顶层 width，实现逐字填充效果
 * - 顶层右侧加 mask 渐变，实现柔和过渡边缘
 *
 * 相比 background-clip:text 方案，此方案：
 * - 不依赖 CSS 变量在 calc() 中的解析
 * - 不受 Chakra/Emotion 序列化干扰
 * - 兼容性更好，性能更优
 */

import { useEffect, useRef, useState, memo } from "react";
import { Box, Text } from "@chakra-ui/react";
import type { KaraokeLine } from "@/types/music";
import { getLineProgress, calculateScrollOffset } from "@/lib/karaoke-lyrics";

interface KaraokeLyricLineProps {
  line: KaraokeLine;
  nextLine?: KaraokeLine;
  isActive: boolean;
  isPlaying: boolean;
  fontSize: number;
  activeColor: string;
  highlightColor: string;
  textColor: string;
  subTextColor: string;
  audioRef?: HTMLAudioElement | null;
  /** 文字对齐，默认居中（透明彩胶等左对齐场景传 "left"） */
  align?: "center" | "left";
}

function KaraokeLyricLineInner({
  line,
  nextLine,
  isActive,
  isPlaying,
  fontSize,
  activeColor,
  highlightColor,
  textColor,
  subTextColor,
  audioRef,
  align = "center",
}: KaraokeLyricLineProps) {
  // overlayRef 是顶层高亮文字的容器，RAF 更新它的 width
  const overlayRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLSpanElement>(null);
  const [scrollNeeded, setScrollNeeded] = useState(false);
  const scrollLimitRef = useRef(0);

  // 检测超长歌词
  useEffect(() => {
    if (!isActive || !containerRef.current || !scrollRef.current) return;
    const checkOverflow = () => {
      if (!scrollRef.current || !containerRef.current) return;
      const overflow = scrollRef.current.scrollWidth - containerRef.current.clientWidth;
      const needed = overflow > 4;
      setScrollNeeded(needed);
      scrollLimitRef.current = Math.max(0, overflow / 2);
    };
    checkOverflow();
    const timer = setTimeout(checkOverflow, 100);
    return () => clearTimeout(timer);
  }, [line.text, fontSize, isActive]);

  // 动画循环：更新顶层 width（仅当前行 + 播放中运行）
  useEffect(() => {
    if (!isActive || !isPlaying || !overlayRef.current) return;

    const el = overlayRef.current;
    el.style.width = "0%";

    let rafId: number;
    let running = true;

    const tick = () => {
      if (!running) return;
      const t = audioRef ? audioRef.currentTime : 0;
      const progress = getLineProgress(line, nextLine, t);

      // 更新顶层裁剪宽度
      el.style.width = `${(progress * 100).toFixed(2)}%`;

      // 超长歌词智能水平滚动（滚动整个文字容器）
      if (scrollNeeded && scrollRef.current) {
        const offset = calculateScrollOffset(progress, scrollLimitRef.current);
        scrollRef.current.style.transform = `translate3d(${offset.toFixed(2)}px, 0, 0)`;
      }

      rafId = requestAnimationFrame(tick);
    };

    rafId = requestAnimationFrame(tick);
    return () => {
      running = false;
      cancelAnimationFrame(rafId);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActive, isPlaying, line, nextLine, scrollNeeded, audioRef]);

  // ── 非当前行：简单显示 ──
  if (!isActive) {
    return (
      <Box py={1}>
        <Text
          fontSize={`${fontSize}px`}
          color={textColor}
          fontWeight="normal"
          textAlign={align}
          noOfLines={2}
          wordBreak="break-word"
        >
          {line.text}
        </Text>
        {line.translation && (
          <Text
            fontSize={`${fontSize - 4}px`}
            color={subTextColor}
            mt={1}
            textAlign={align}
            noOfLines={1}
          >
            {line.translation}
          </Text>
        )}
      </Box>
    );
  }

  // ── 当前行：双层叠加卡拉OK效果 ──
  const sharedTextStyle: React.CSSProperties = {
    display: "inline-block",
    whiteSpace: "nowrap",
    fontSize: `${fontSize}px`,
    fontWeight: "bold",
    lineHeight: 1.4,
    letterSpacing: "0px",
  };

  // mask 渐变宽度随字号缩放，避免小字号时遮罩过大导致底层透出
  const maskFade = Math.min(18, Math.max(6, fontSize * 0.7));
  const maskGradient = `linear-gradient(90deg, #000 0%, #000 calc(100% - ${maskFade}px), rgba(0,0,0,0.3) 100%)`;

  return (
    <Box
      ref={containerRef}
      py={1}
      overflow="hidden"
      textAlign={align}
      position="relative"
    >
      {/* scrollRef 包裹所有文字层，超长歌词时整体平移
          使用纯 span + position:relative 确保 overlay 绝对定位基准正确 */}
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
        <span
          style={{
            ...sharedTextStyle,
            color: textColor,
            opacity: 0.35,
          }}
        >
          {line.text}
        </span>
        {/* 顶层：高亮歌词，用 width 裁剪
            position:absolute 基准是 scrollRef（position:relative）
            top:0 left:0 与底层文字完全对齐 */}
        <span
          ref={overlayRef}
          style={{
            ...sharedTextStyle,
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
      {line.translation && (
        <Text
          fontSize={`${fontSize - 4}px`}
          color={subTextColor}
          opacity={0.6}
          mt={1}
          textAlign={align}
          noOfLines={1}
        >
          {line.translation}
        </Text>
      )}
    </Box>
  );
}

export const KaraokeLyricLine = memo(KaraokeLyricLineInner);
