/**
 * 沉浸式歌词可视化视图（WAVE 动感波光）
 *
 * 歌词双层 DOM：
 * - 前景主歌词：完整当前句，亮色居中可读（字体 NotoSerifSC-900，字号跟随窗口）
 * - 背景重影层：前几个字放大、中性灰、居于前景正后方
 * 入场动画按文本长度分支（短词旋转+模糊变清晰；长词字间距收缩），
 * 出场只做高斯模糊拉高 + 透明度淡出；旧句退场与新句入场并行播放，
 * 且距下一句开始不足 LEAD_MS 时提前切换，保证演唱开始时歌词已就位。
 *
 * 波纹由独立的全屏组件 ImmersiveRippleField 渲染，
 * 铺满整个展开播放器，不受本组件布局区域限制。
 *
 * 性能策略：
 * - 低频定时器（~250ms）检查 activeIndex，仅在行变化时 setState
 * - blur 只在入场/出场短暂运行，hold 期间无 filter
 */

import { useEffect, useRef, useState, memo } from "react";
import { Box, Text, Spinner, VStack } from "@chakra-ui/react";
import { Music as MusicIcon } from "lucide-react";
import type { KaraokeLine } from "@/types/music";

// 动画时长（ms）：旧句退场与新句入场并行播放，入场最晚 800ms（重影）
const EXIT_MS = 520;
const ENTER_MS = 860;
// 提前量（ms）：距下一句开始不足该值时提前入场，既保证演唱时歌词就位，
// 又让入场动画的后半段在演唱开始时依然可见（不会在幕后悄悄完成）
const LEAD_MS = 350;
// 歌词字体（index.css 中注册）
const LYRIC_FONT = "NotoSerifSC-900";

type Phase = "enter" | "hold" | "exit";

interface DisplayState {
  index: number;
  text: string;
  translation?: string;
  phase: Phase;
  // 入场动画类型：转动 / 字符靠拢，二者随机二选一
  anim: "rotate" | "spread";
}

/** 根据当前播放时间计算 activeIndex */
function calcActiveIndex(lines: KaraokeLine[], currentTime: number): number {
  if (lines.length === 0) return -1;
  let idx = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].time <= currentTime) idx = i;
    else break;
  }
  return idx;
}

const SENTENCE_END = /[，。！？、；,.!?;]/;

/**
 * 歌词双行排版：多句按语义拆两行；单个长句从合适断点拆两行，
 * 避免"一行排满再折行"；短句保持单行
 */
function splitIntoLines(text: string): string[] {
  const t = text.trim();
  if (!t) return [""];

  // 按标点切成句子（保留标点）
  const sentences = t
    .split(/(?<=[，。！？、；,.!?;])/)
    .map((s) => s.trim())
    .filter(Boolean);

  // 单句：长度适中保持单行，过长则在断点（空格/标点）均匀拆两行
  if (sentences.length <= 1) {
    const raw = sentences[0] || t;
    if (raw.replace(/\s/g, "").length <= 14) return [raw];
    const mid = Math.floor(raw.length / 2);
    let cut = -1;
    for (let i = 0; i < raw.length; i++) {
      if (raw[i] === " " || SENTENCE_END.test(raw[i]) || raw[i] === " ") {
        if (cut < 0 || Math.abs(i - mid) < Math.abs(cut - mid)) cut = i + 1;
      }
    }
    if (cut > 0 && cut < raw.length) {
      return [raw.slice(0, cut).trim(), raw.slice(cut).trim()];
    }
    return [raw.slice(0, mid), raw.slice(mid)].map((s) => s.trim());
  }

  // 多句：贪心分两行，尽量均衡（第二行承接剩余所有句子）
  const totalLen = t.replace(/\s/g, "").length;
  const half = totalLen / 2;
  let line1: string[] = [];
  let acc = 0;
  for (const s of sentences) {
    const sz = s.replace(/\s/g, "").length;
    if (line1.length > 0 && acc + sz > half) break;
    line1.push(s);
    acc += sz;
  }
  const rest = sentences.slice(line1.length);
  if (rest.length === 0) {
    // 全部塞进第一行了：挪最后一句到第二行保证双行
    const last = line1.pop()!;
    return [line1.join(""), last].filter((s) => s.trim()).map((s) => s.trim());
  }
  return [line1.join(""), rest.join("")].filter((s) => s.trim()).map((s) => s.trim());
}

interface ImmersiveLyricsViewProps {
  lines: KaraokeLine[];
  loading: boolean;
  audioRef?: HTMLAudioElement | null;
  isPlaying: boolean;
  coverColor: { isLight: boolean };
  baseFontSize: number;
}

function ImmersiveLyricsViewInner({
  lines,
  loading,
  audioRef,
  isPlaying,
  coverColor,
  baseFontSize,
}: ImmersiveLyricsViewProps) {
  const [activeIndex, setActiveIndex] = useState(-1);
  const [display, setDisplay] = useState<DisplayState | null>(null);
  // 退场中的上一句（与新句入场并行播放动画，结束后清除）
  const [outgoing, setOutgoing] = useState<DisplayState | null>(null);
  // 容器尺寸：歌词字号跟随窗口缩放
  const [viewH, setViewH] = useState(420);
  const rootRef = useRef<HTMLDivElement>(null);

  // 用 ref 保存最新值，供定时器/动画闭包读取，避免依赖抖动
  const linesRef = useRef(lines);
  linesRef.current = lines;
  const displayRef = useRef<DisplayState | null>(display);
  displayRef.current = display;
  const outgoingRef = useRef<DisplayState | null>(outgoing);
  outgoingRef.current = outgoing;

  const exitTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const enterTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 切歌标记：lines 变化后等待新歌 currentTime 回落再恢复更新
  const waitingForNewSongRef = useRef(false);
  const lastCurrentTimeRef = useRef(0);

  // 监听容器高度，歌词字号随窗口缩放
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0];
      if (r && r.contentRect.height > 0) setViewH(r.contentRect.height);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  /** 新句入场：先播放入场动画，ENTER_MS 后进入句中固定状态 */
  const startEnter = (idx: number) => {
    const line = linesRef.current[idx];
    const raw = (line?.text || "").trim();
    setDisplay({
      index: idx,
      text: raw || "♪ ♪ ♪",
      translation: line?.translation?.trim() || undefined,
      phase: "enter",
      // 转动 / 字符靠拢 随机二选一（与字数无关）
      anim: Math.random() < 0.5 ? "rotate" : "spread",
    });
    if (enterTimerRef.current) clearTimeout(enterTimerRef.current);
    enterTimerRef.current = setTimeout(() => {
      enterTimerRef.current = null;
      setDisplay((prev) => (prev && prev.phase === "enter" ? { ...prev, phase: "hold" } : prev));
    }, ENTER_MS);
  };

  // 切歌 / 歌词重新加载：清空显示状态与所有挂起定时器
  useEffect(() => {
    setActiveIndex(-1);
    setDisplay(null);
    setOutgoing(null);
    if (exitTimerRef.current) { clearTimeout(exitTimerRef.current); exitTimerRef.current = null; }
    if (enterTimerRef.current) { clearTimeout(enterTimerRef.current); enterTimerRef.current = null; }
    if (audioRef) lastCurrentTimeRef.current = audioRef.currentTime;
    waitingForNewSongRef.current = true;
  }, [lines]); // eslint-disable-line react-hooks/exhaustive-deps

  // 组件卸载：清理全部定时器
  useEffect(() => {
    return () => {
      if (exitTimerRef.current) clearTimeout(exitTimerRef.current);
      if (enterTimerRef.current) clearTimeout(enterTimerRef.current);
    };
  }, []);

  // 低频定时器（~250ms）检查 activeIndex，仅在行变化时 setState
  useEffect(() => {
    if (!audioRef) return;

    const checkActive = () => {
      const t = audioRef.currentTime;

      // 切歌后等待 currentTime 回落到较小值（新歌曲从 0 开始播放）
      if (waitingForNewSongRef.current) {
        if (Math.abs(t - lastCurrentTimeRef.current) < 0.5) {
          waitingForNewSongRef.current = false;
        } else if (t > 2.0 && t >= lastCurrentTimeRef.current - 1) {
          return;
        }
        waitingForNewSongRef.current = false;
      }

      // 提前切换：距下一句开始不足 LEAD_MS 时直接预切换到下一句，
      // 让入场动画在演唱前就开始，歌词到达已就位
      const linesNow = linesRef.current;
      let newIdx = calcActiveIndex(linesNow, t);
      if (newIdx >= 0 && newIdx < linesNow.length - 1 && t >= linesNow[newIdx + 1].time - LEAD_MS / 1000) {
        newIdx += 1;
      }
      setActiveIndex((prev) => (prev !== newIdx ? newIdx : prev));
    };

    checkActive();
    const interval = setInterval(checkActive, 250);
    return () => clearInterval(interval);
  }, [audioRef]);

  // 歌词状态机：activeIndex 变化 → 旧句转 outgoing 退场（与正在入场的下一句并行播放）
  useEffect(() => {
    if (activeIndex < 0) {
      if (exitTimerRef.current) { clearTimeout(exitTimerRef.current); exitTimerRef.current = null; }
      setDisplay(null);
      setOutgoing(null);
      return;
    }
    const cur = displayRef.current;
    if (cur && cur.index === activeIndex) return;
    const line = linesRef.current[activeIndex];
    if (!line) return;

    if (!cur) {
      // 首句 / 切歌后首句：直接入场
      startEnter(activeIndex);
      return;
    }

    // 新句立即入场，旧句同时退场（不等待退场完成）
    if (outgoingRef.current) {
      setOutgoing(null); // 上一次退场未结束则直接丢弃，避免堆积
    }
    setOutgoing({ ...cur, phase: "exit" });
    if (exitTimerRef.current) { clearTimeout(exitTimerRef.current); exitTimerRef.current = null; }
    exitTimerRef.current = setTimeout(() => {
      exitTimerRef.current = null;
      setOutgoing(null);
    }, EXIT_MS);

    startEnter(activeIndex);
  }, [activeIndex]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── 颜色与字号派生 ──
  const isLightText = coverColor.isLight;
  // 二层重影歌词：中性深灰/浅灰，避免随封面色过度变深
  const ghostColor = isLightText
    ? "rgba(35,35,45,0.15)"
    : "rgba(255,255,255,0.16)";
  // 前景歌词色：色板均为深色，用亮色文字
  const fgTextColor = isLightText ? "#1f2430" : "#f7f8fb";
  const fgTextShadow = isLightText
    ? "0 2px 20px rgba(255,255,255,0.4)"
    : "0 2px 28px rgba(0,0,0,0.5)";
  // 阴影透明版（与 fgTextShadow 同结构），供入场/退场动画插值：
  // 若直接带满不透明度阴影跑 blur 动画，filter 会把 text-shadow 一起放大，
  // 在歌词轮廓外闪出一圈黑线（黑阴影）或白线（白阴影）
  const fgShadowTransparent = isLightText
    ? "0 2px 20px rgba(255,255,255,0)"
    : "0 2px 28px rgba(0,0,0,0)";
  const fgSubColor = isLightText ? "rgba(31,36,48,0.55)" : "rgba(255,255,255,0.55)";

  // 前景主歌词字号：跟随窗口高度缩放（约 11% 容器高），并考虑歌词字号设置
  const fgFromView = Math.round(viewH * 0.11);
  const fgSize = Math.min(84, Math.max(30, fgFromView, Math.round(baseFontSize * 1.7)));
  // 重影再放大 2.6 倍
  const ghostSize = Math.round(fgSize * 2.6);

  // 双层歌词渲染：前景主歌词 + 背景重影（背景色深灰版），供当前句与退场句复用
  const renderPair = (st: DisplayState) => {
    // 重影字数跟随第一层歌词长度：按 35% 取，绝大多数 2~3 字，仅超长句才 4 字
    const contentLen = st.text.replace(/\s/g, "").length;
    const gCount = Math.min(4, Math.max(2, Math.round(contentLen * 0.35)));
    const gText = st.text.replace(/\s/g, "").slice(0, Math.min(gCount, contentLen));
    // 前景按语义拆双行（多句 / 长句），避免一行排满再折行
    const fgLines = splitIntoLines(st.text);
    // 两种入场动画独立，按本句随机类型二选一（与字数无关）
    const isSpread = st.anim === "spread";
    let gAnim = "none";
    if (st.phase === "exit") {
      // 出场：只做高斯模糊拉高 + 透明度淡出，无位移
      gAnim = "immersiveExit 0.52s ease-in forwards";
    } else {
      // 入场与句中：始终保留入场动画（forwards 恒久保持终态），
      // 绝不在渲染中移除动画，从根源避免"移除瞬间缩小/回退"的跳变
      gAnim = "immersiveGhostEnter 0.85s cubic-bezier(0.22,0.8,0.36,1) forwards";
    }
    return (
      <Box
        key={`pair-${st.index}`}
        position="absolute"
        inset={0}
        display="flex"
        alignItems="center"
        justifyContent="center"
      >
        {/* 背景重影层：前几个字，放大，颜色较深，z 在前景之下 */}
        <Box
          key={`ghost-${st.index}`}
          position="absolute"
          left="50%"
          top="50%"
          zIndex={1}
          sx={{
            transform: "translate(-50%, -50%)",
            fontSize: `${ghostSize}px`,
            fontWeight: 900,
            color: ghostColor,
            fontFamily: LYRIC_FONT,
            whiteSpace: "nowrap",
            lineHeight: 1,
            userSelect: "none",
            // hold 时固化入场终态（blur/transform 与动画终态完全一致），避免动画移除瞬间跳变
            ...(st.phase === "hold" ? { filter: "blur(0px)", transform: "translate(-50%, -50%) rotate(0deg)" } : {}),
            animation: gAnim,
          }}
        >
          {gText}
        </Box>

        {/* 前景主歌词：完整当前句，亮色居中可读 */}
        <Box
          key={`fg-${st.index}`}
          position="relative"
          zIndex={2}
          maxW="84%"
          textAlign="center"
          sx={{
            // 转动动画走外层（仅 transform，GPU 合成平滑）；
            // 字符靠拢 / 出场无分层需求，外层不做动画
            ...(isSpread || st.phase === "exit" || st.phase === "hold"
              ? {}
              : { animation: "immersiveRotate 0.72s cubic-bezier(0.25,0.8,0.35,1) forwards" }),
          }}
        >
          {/* 内层：模糊/透明度/字间距动画 */}
          <Box
            sx={{
              ...(st.phase === "exit"
                ? { animation: "immersiveExit 0.52s ease-in forwards" }
                : isSpread
                  ? { animation: "immersiveEnterLong 0.8s cubic-bezier(0.22,0.8,0.36,1) forwards" }
                  : { animation: "immersiveBlurIn 0.72s cubic-bezier(0.25,0.8,0.35,1) forwards" }),
              // hold 时固化终态（blur/字间距与动画终态一致），避免属性回退
              ...(st.phase === "hold" ? {
                ...(isSpread ? { letterSpacing: "0.03em", textIndent: "0.015em" } : {}),
              } : {}),
            }}
          >
            <Text
              sx={{
                fontSize: `${fgSize}px`,
                fontWeight: 900,
                fontFamily: LYRIC_FONT,
                color: fgTextColor,
                textShadow: fgTextShadow,
                // 阴影动画与内层 blur 动画同节奏：入场从透明渐显（blur 收尾后才显影）、
                // 退场提前淡出，避免 filter blur 把 text-shadow 放大成轮廓外的黑线
                animation:
                  st.phase === "exit"
                    ? "immersiveShadowOut 0.52s ease-in forwards"
                    : isSpread
                      ? "immersiveShadowIn 0.8s cubic-bezier(0.22,0.8,0.36,1) forwards"
                      : "immersiveShadowIn 0.72s cubic-bezier(0.25,0.8,0.35,1) forwards",
                lineHeight: 1.35,
                whiteSpace: "pre-wrap",
                wordBreak: "keep-all",
              }}
            >
              {fgLines.join("\n")}
            </Text>
            {st.translation && (
              <Text
                sx={{
                  mt: 2,
                  fontSize: `${Math.max(14, Math.round(fgSize * 0.38))}px`,
                  fontWeight: 500,
                  fontFamily: LYRIC_FONT,
                  color: fgSubColor,
                  textShadow: fgTextShadow,
                  // 翻译行与主歌词同步处理阴影渐显/淡出，避免 blur 动画期间黑线闪烁
                  animation:
                    st.phase === "exit"
                      ? "immersiveShadowOut 0.52s ease-in forwards"
                      : isSpread
                        ? "immersiveShadowIn 0.8s cubic-bezier(0.22,0.8,0.36,1) forwards"
                        : "immersiveShadowIn 0.72s cubic-bezier(0.25,0.8,0.35,1) forwards",
                }}
              >
                {st.translation}
              </Text>
            )}
          </Box>
        </Box>
      </Box>
    );
  };

  return (
    <Box
      ref={rootRef}
      flex={1}
      h="100%"
      minH={0}
      position="relative"
      overflow="hidden"
      zIndex={2}
      sx={{
        // ── 转动（外层专用）：仅两帧，浏览器自动平滑插值 ──
        "@keyframes immersiveRotate": {
          "0%": { transform: "rotate(-2.6deg)" },
          "100%": { transform: "rotate(0deg)" },
        },
        // ── 模糊变清晰（内层专用）：仅两帧，与转动并行 ──
        "@keyframes immersiveBlurIn": {
          "0%": { filter: "blur(14px)", opacity: 0 },
          "100%": { filter: "blur(0px)", opacity: 1 },
        },
        // ── 字符靠拢（长歌词）：字间距从大平滑合并到正常 + 模糊变清晰（无转动；text-indent 抵消末字间距保持居中） ──
        "@keyframes immersiveEnterLong": {
          "0%": { letterSpacing: "0.42em", textIndent: "0.21em", filter: "blur(12px)", opacity: 0.2 },
          "100%": { letterSpacing: "0.03em", textIndent: "0.015em", filter: "blur(0px)", opacity: 1 },
        },
        // ── 出场：模糊逐步拉高 + 淡出（不位移、不偏移） ──
        "@keyframes immersiveExit": {
          "0%": { filter: "blur(0)", opacity: 1 },
          "100%": { filter: "blur(20px)", opacity: 0 },
        },
        // ── 阴影淡入：入场时阴影保持透明，blur 收尾（约 60% 进度）后才渐显。
        // 若全程带满不透明度阴影，filter blur 会把 text-shadow 一起放大成轮廓黑线 ──
        "@keyframes immersiveShadowIn": {
          "0%": { textShadow: fgShadowTransparent },
          "60%": { textShadow: fgShadowTransparent },
          "100%": { textShadow: fgTextShadow },
        },
        // ── 阴影淡出：退场时阴影先于文字消失，避免旧句黑雾罩住正在入场的新句 ──
        "@keyframes immersiveShadowOut": {
          "0%": { textShadow: fgTextShadow },
          "100%": { textShadow: fgShadowTransparent },
        },
        // ── 重影入场：与前景同款（旋转 + 模糊变清晰），方向相反、幅度稍大 ──
        "@keyframes immersiveGhostEnter": {
          "0%": { transform: "translate(-50%, -50%) rotate(2.2deg) scale(0.9)", filter: "blur(26px)", opacity: 0 },
          "55%": { filter: "blur(5px)", opacity: 1 },
          "100%": { transform: "translate(-50%, -50%) rotate(0deg) scale(1)", filter: "blur(0)", opacity: 1 },
        },
      }}
    >
      {/* 中央内容层：加载中 / 暂无歌词 / 歌词双层 */}
      <Box
        position="absolute"
        inset={0}
        display="flex"
        alignItems="center"
        justifyContent="center"
        px={12}
        pointerEvents="none"
        zIndex={3}
      >
        {loading ? (
          <Spinner size="xl" sx={{ color: fgTextColor }} />
        ) : lines.length === 0 ? (
          <VStack spacing={3} justify="center" align="center">
            <MusicIcon size={36} color={fgSubColor} />
            <Text color={fgSubColor} fontSize="sm" fontFamily={LYRIC_FONT}>暂无歌词</Text>
          </VStack>
        ) : (
          <>
            {/* 退场中的上一句（下层，与新句入场并行） */}
            {outgoing && renderPair(outgoing)}
            {/* 当前句 */}
            {display && renderPair(display)}
          </>
        )}
      </Box>
    </Box>
  );
}

export const ImmersiveLyricsView = memo(ImmersiveLyricsViewInner);

// ═══════════════════════════════════════════════
// ImmersiveRippleField — 全屏水波层
// 铺满整个展开播放器（absolute inset-0），不受歌词布局区域限制。
// 左侧波纹 = 背景色加深版，右侧波纹 = 背景色变浅版；
// 模糊柔和、扩散缓慢消失。
// ═══════════════════════════════════════════════

interface RippleItem {
  id: number;
  side: "left" | "right";
  size: number;
  duration: number;
  delay: number;
}

/** rgb(0-255) → hsl(h 0-360, s 0-1, l 0-1) */
function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn), min = Math.min(rn, gn, bn);
  const d = max - min;
  const l = (max + min) / 2;
  if (d === 0) return [0, 0, l];
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  switch (max) {
    case rn: h = ((gn - bn) / d + (gn < bn ? 6 : 0)) * 60; break;
    case gn: h = ((bn - rn) / d + 2) * 60; break;
    default: h = ((rn - gn) / d + 4) * 60;
  }
  return [((h % 360) + 360) % 360, s, l];
}

function hsla(h: number, s: number, l: number, a: number): string {
  return `hsla(${Math.round(h)}, ${Math.round(s * 100)}%, ${Math.round(l * 100)}%, ${a})`;
}

// 波纹容量：动画最长约 5.6s、最短生成间隔约 0.5s，峰值并发约 22 个。
// 10 太小会导致新波纹把仍在扩散中的旧波纹（尤其首波最大那个）从 DOM 踢掉 → 瞬间消失，
// 提高到 24 让所有波纹都能走完渐隐动画后再被 setTimeout 移除。
const MAX_RIPPLES = 24;

export const ImmersiveRippleField = memo(function ImmersiveRippleField({
  isPlaying,
  bgRgb,
}: {
  isPlaying: boolean;
  bgRgb: [number, number, number];
}) {
  const [ripples, setRipples] = useState<RippleItem[]>([]);
  // 容器尺寸：波纹按窗口大小等比缩放
  const [viewSize, setViewSize] = useState({ w: 800, h: 500 });
  const rootRef = useRef<HTMLDivElement>(null);
  const viewSizeRef = useRef(viewSize);
  viewSizeRef.current = viewSize;
  const spawnTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const rippleIdRef = useRef(0);

  // 监听容器尺寸（铺满整个展开播放器，包含顶部/底部栏区域）
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0];
      if (r) setViewSize({ w: r.contentRect.width, h: r.contentRect.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // 波纹发射器：左右两个发射点贴窗口边缘独立运行；节拍越强 → 生成越快、半径越大。
  // 挂载即启动，不随播放状态停止——暂停后波纹依旧存在，不消失
  useEffect(() => {
    let cancelled = false;
    // 首波强制大波纹，打开即有较大的波浪
    let first = true;

    const spawn = () => {
      if (cancelled) return;

      // 定时器模拟节拍强度：正弦主波 + 随机抖动（首播直接拉满）
      const wave = 0.5 + 0.5 * Math.sin(Date.now() / 640);
      const intensity = first
        ? 0.95
        : Math.min(1, Math.max(0.15, wave * 0.72 + Math.random() * 0.34));
      first = false;

      // 波纹按窗口尺寸等比缩放（基础比例拉高，整体偏大），左右各自随机、互不相同
      const maxDim = Math.max(viewSizeRef.current.w, viewSizeRef.current.h);
      const makeRipple = (side: "left" | "right"): RippleItem => ({
        id: ++rippleIdRef.current,
        side,
        size: Math.max(200, maxDim * (0.62 + intensity * 0.3 + Math.random() * 0.15)),
        duration: 3.6 + Math.random() * 1.6,
        delay: Math.random() * 0.4,
      });

      const items: RippleItem[] = [makeRipple("left"), makeRipple("right")];
      // 动画结束后自动移除（含随机延迟，组件卸载后 setRipples 为 no-op，安全）
      items.forEach((it) => {
        setTimeout(() => {
          setRipples((p) => p.filter((r) => r.id !== it.id));
        }, (it.duration + it.delay) * 1000 + 150);
      });
      setRipples((prev) => [...prev, ...items].slice(-MAX_RIPPLES));

      // 节拍越强，波纹生成越快（~500ms ~ ~1000ms）
      spawnTimerRef.current = setTimeout(spawn, 1080 - intensity * 580);
    };

    spawn();

    return () => {
      cancelled = true;
      if (spawnTimerRef.current) { clearTimeout(spawnTimerRef.current); spawnTimerRef.current = null; }
    };
  }, []);

  // ── 派生：左右波纹用背景色自身的深/浅色（保留色相，非黑白）──
  // 波浪 = 4 道同心波环（明暗交替的涟漪），内圈环更实，外圈渐隐消失
  const [bh, bs, bl] = rgbToHsl(bgRgb[0], bgRgb[1], bgRgb[2]);
  // 左波纹：背景色的深色版波环，让背景变深
  const deepL = Math.max(0.16, bl * 0.62);
  const leftWaveGradient = `radial-gradient(circle,
    ${hsla(bh, bs, bl, 0)} 0%,
    ${hsla(bh, bs, deepL, 0.6)} 12%,
    ${hsla(bh, bs, bl, 0)} 22%,
    ${hsla(bh, bs, deepL, 0.5)} 34%,
    ${hsla(bh, bs, bl, 0)} 46%,
    ${hsla(bh, bs, deepL, 0.36)} 58%,
    ${hsla(bh, bs, bl, 0)} 70%,
    ${hsla(bh, bs, deepL, 0.2)} 84%,
    ${hsla(bh, bs, bl, 0)} 100%)`;
  // 右波纹：背景色的浅色版波环（轻微提亮，避免发白），让背景变浅
  const lightL = Math.min(0.72, bl + 0.18);
  const rightWaveGradient = `radial-gradient(circle,
    ${hsla(bh, bs, bl, 0)} 0%,
    ${hsla(bh, bs, lightL, 0.6)} 12%,
    ${hsla(bh, bs, bl, 0)} 22%,
    ${hsla(bh, bs, lightL, 0.5)} 34%,
    ${hsla(bh, bs, bl, 0)} 46%,
    ${hsla(bh, bs, lightL, 0.36)} 58%,
    ${hsla(bh, bs, bl, 0)} 70%,
    ${hsla(bh, bs, lightL, 0.2)} 84%,
    ${hsla(bh, bs, bl, 0)} 100%)`;

  return (
    <Box
      ref={rootRef}
      position="absolute"
      top={0}
      left={0}
      right={0}
      bottom={0}
      zIndex={1}
      pointerEvents="none"
      overflow="hidden"
      sx={{
        // ── 左水波扩散：圆心贴左窗口边缘，向右扩进画面。
        // 淡出从约 60% 开始分段渐隐（先缓后急再缓），末尾平滑收尾，避免最后一段瞬时消失 ──
        "@keyframes rippleLeft": {
          "0%": { transform: "translate(-50%, -50%) scale(0.3)", opacity: 0 },
          "10%": { opacity: 1 },
          "60%": { opacity: 0.92 },
          "85%": { opacity: 0.38 },
          "100%": { transform: "translate(-50%, -50%) scale(1)", opacity: 0 },
        },
        // ── 右水波扩散：圆心贴右窗口边缘，向左扩进画面（同左波纹渐变） ──
        "@keyframes rippleRight": {
          "0%": { transform: "translate(50%, -50%) scale(0.3)", opacity: 0 },
          "10%": { opacity: 1 },
          "60%": { opacity: 0.92 },
          "85%": { opacity: 0.38 },
          "100%": { transform: "translate(50%, -50%) scale(1)", opacity: 0 },
        },
      }}
    >
      {ripples.map((r) => (
        <Box
          key={r.id}
          position="absolute"
          top="50%"
          left={r.side === "left" ? 0 : undefined}
          right={r.side === "right" ? 0 : undefined}
          width={`${r.size}px`}
          height={`${r.size}px`}
          borderRadius="50%"
          sx={{
            transform: r.side === "left" ? "translate(-50%, -50%)" : "translate(50%, -50%)",
            // 左波纹加深背景，右波纹变浅背景（背景色自身深浅，非黑白）
            background: r.side === "left" ? leftWaveGradient : rightWaveGradient,
            // 不用 filter blur：radial-gradient 本身柔和；blur 会强制 Chromium 为每个波纹
            // 创建离屏渲染层（24 个常驻 + 替换时频繁分配/释放层缓冲）→ 内存持续涨
            // delay 必须并入 animation 简写：单独的 animation-delay 会被后写的
            // animation 简写重置为 0s，导致错峰失效、波纹同步涌现同步收尾
            animation: `${r.side === "left" ? "rippleLeft" : "rippleRight"} ${r.duration}s cubic-bezier(0.25, 0.6, 0.4, 1) ${r.delay}s both`,
          }}
        />
      ))}
    </Box>
  );
});