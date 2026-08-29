/**
 * 卡拉OK歌词列表视图
 *
 * 特性：
 * - 当前行使用 KaraokeLyricLine 渲染（带逐字进度）
 * - 非当前行简单显示，淡显效果
 * - 自动滚动到当前行
 * - 虚拟化渲染（仅渲染可见行 ± 前后若干行）提升性能
 * - 上下边缘遮罩渐变
 *
 * 性能策略（三层解耦）：
 * 1. ExpandedPlayer 不再因 timeupdate 而重渲染（进度条独立组件）
 * 2. 本组件用低频定时器（~250ms）检查 activeIndex，仅在行变化时 setState
 * 3. KaraokeLyricLine 内部 RAF 直接读 audioRef，60fps 仅操作 DOM
 * 三层完全独立，播放期间 ExpandedPlayer 零 re-render
 */

import { useEffect, useRef, useState, memo } from "react";
import { Box, VStack, Spinner, Text } from "@chakra-ui/react";
import { Music as MusicIcon } from "lucide-react";
import type { KaraokeLine } from "@/types/music";
import { KaraokeLyricLine } from "./KaraokeLyricLine";

interface KaraokeLyricsViewProps {
  lines: KaraokeLine[];
  loading: boolean;
  fontSize: number;
  activeColor: string;
  highlightColor: string;
  textColor: string;
  subTextColor: string;
  scrollbarSx: Record<string, unknown>;
  audioRef?: HTMLAudioElement | null;
  isPlaying: boolean;
  /** 滚动容器最大高度，默认 65vh（透明彩胶等铺满容器的场景传 "100%"） */
  maxHeight?: string;
  /** 歌词行文字对齐，默认居中 */
  align?: "center" | "left";
  /** 当前行是否放大强调（scale 1.02），默认开启；透明彩胶等只要卡拉OK效果的场景传 false */
  lineScale?: boolean;
}

/**
 * 根据当前播放时间计算 activeIndex
 */
function calcActiveIndex(lines: KaraokeLine[], currentTime: number): number {
  if (lines.length === 0) return -1;
  let idx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].time <= currentTime) idx = i;
    else break;
  }
  return idx;
}

function KaraokeLyricsViewInner({
  lines,
  loading,
  fontSize,
  activeColor,
  highlightColor,
  textColor,
  subTextColor,
  scrollbarSx,
  audioRef,
  isPlaying,
  maxHeight = "65vh",
  align = "center",
  lineScale = true,
}: KaraokeLyricsViewProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [activeIndex, setActiveIndex] = useState(-1);

  // 用 ref 保存最新的 lines，供定时器闭包读取
  const linesRef = useRef(lines);
  linesRef.current = lines;

  // 切歌标记：lines 变化后设为 true，等 audioRef.currentTime 明显变小（新歌曲开始）才恢复更新
  // 防止旧歌曲末尾的 currentTime 用新歌词算出高 activeIndex 导致滚到底部
  const waitingForNewSongRef = useRef(false);
  const lastCurrentTimeRef = useRef(0);

  // 新歌曲歌词加载时，重置滚动位置和 activeIndex
  useEffect(() => {
    setActiveIndex(-1);
    // 记住当前 currentTime，切歌后新歌曲的 currentTime 会从 0 附近开始
    if (audioRef) {
      lastCurrentTimeRef.current = audioRef.currentTime;
    }
    waitingForNewSongRef.current = true;
    if (scrollRef.current) {
      scrollRef.current.scrollTo({ top: 0, behavior: "auto" });
    }
  }, [lines]); // eslint-disable-line react-hooks/exhaustive-deps

  // 低频定时器（~250ms）检查 activeIndex，仅在行变化时 setState
  // 避免依赖外部 currentTime prop 导致整个组件树重渲染
  useEffect(() => {
    if (!audioRef) return;

    const checkActive = () => {
      const t = audioRef.currentTime;

      // 切歌后等待 currentTime 回落到较小值（新歌曲从 0 开始播放）
      // 用「当前时间比上次记录的小很多」来判断新歌曲已开始
      if (waitingForNewSongRef.current) {
        // 同一首歌重新挂载（收起再展开）：时间几乎没变，直接清除标记
        if (Math.abs(t - lastCurrentTimeRef.current) < 0.5) {
          waitingForNewSongRef.current = false;
        } else if (t > 2.0 && t >= lastCurrentTimeRef.current - 1) {
          // 仍在旧歌曲位置（时间没明显变小），跳过
          return;
        }
        waitingForNewSongRef.current = false; // 新歌曲开始，清除标记
      }

      const newIdx = calcActiveIndex(linesRef.current, t);
      setActiveIndex((prev) => (prev !== newIdx ? newIdx : prev));
    };

    // 立即检查一次
    checkActive();

    const interval = setInterval(checkActive, 250);
    return () => clearInterval(interval);
  }, [audioRef]);

  // 自动滚动到当前行
  useEffect(() => {
    if (activeIndex < 0 || !scrollRef.current) return;
    const container = scrollRef.current;
    const activeEl = container.querySelector(
      `[data-lyric-idx="${activeIndex}"]`
    ) as HTMLElement | null;
    if (activeEl) {
      // 当前行定位在容器约 45% 处（偏下），让用户能看到更多即将到来的歌词
      container.scrollTo({
        top:
          activeEl.offsetTop -
          container.clientHeight * 0.5,
        behavior: "smooth",
      });
    }
  }, [activeIndex]);

  if (loading) {
    return (
      <VStack py={12}>
        <Spinner size="lg" sx={{ color: activeColor }} />
      </VStack>
    );
  }

  if (lines.length === 0) {
    return (
      <VStack py={12} spacing={3}>
        <MusicIcon size={32} color={subTextColor} />
        <Text color={subTextColor} fontSize="sm" textAlign="center">
          暂无歌词
        </Text>
      </VStack>
    );
  }

  return (
    <Box
      ref={scrollRef}
      flex={1}
      maxH={maxHeight}
      overflowY="auto"
      overflowX="hidden"
      sx={{
        ...scrollbarSx,
        overflowX: "hidden",
        maskImage:
          "linear-gradient(to bottom, transparent 0%, black 12%, black 88%, transparent 100%)",
        WebkitMaskImage:
          "linear-gradient(to bottom, transparent 0%, black 12%, black 88%, transparent 100%)",
      }}
      pr={2}
    >
      <Box
        minH="100%"
        display="flex"
        flexDirection="column"
        justifyContent="center"
      >
        <VStack spacing={3} align="stretch">
          {lines.map((line, idx) => {
            const isActive = idx === activeIndex;
            return (
              <Box
                key={idx}
                data-lyric-idx={idx}
                py={1}
                flexShrink={0}
                sx={{
                  transition: "opacity 0.3s ease",
                  opacity: isActive ? 1 : 0.55,
                  ...(lineScale ? { transform: isActive ? "scale(1.02)" : "scale(1)" } : {}),
                }}
              >
                <KaraokeLyricLine
                  line={line}
                  nextLine={lines[idx + 1]}
                  isActive={isActive}
                  isPlaying={isPlaying}
                  fontSize={fontSize}
                  activeColor={activeColor}
                  highlightColor={highlightColor}
                  textColor={textColor}
                  subTextColor={subTextColor}
                  audioRef={audioRef}
                  align={align}
                />
              </Box>
            );
          })}
        </VStack>
      </Box>
    </Box>
  );
}

export const KaraokeLyricsView = memo(KaraokeLyricsViewInner);
