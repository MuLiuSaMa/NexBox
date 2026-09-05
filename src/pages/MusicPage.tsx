import { useEffect, useRef, useState, useCallback, useMemo, memo } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { useLocation } from "react-router-dom";
import { useAppStartup } from "@/contexts/app-startup-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import {
  Box,
  VStack,
  HStack,
  Input,
  InputGroup,
  InputLeftElement,
  InputRightElement,
  Button,
  Text,
  Spinner,
  useColorModeValue,
  IconButton,
  Tooltip,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  Image as ChakraImage,
  Heading,
  Portal,
  Fade,
  Popover,
  PopoverTrigger,
  PopoverContent,
  PopoverBody,
  Switch,
  SimpleGrid,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import {
  Search,
  Volume2,
  VolumeX,
  ListMusic,
  Repeat,
  Repeat1,
  Shuffle,
  Music as MusicIcon,
  Heart,
  ArrowLeft,
  Sparkles,
  ChevronDown,
  MonitorSpeaker,
  Settings,
  User,
  MicVocal,
  Palette,
  Droplets,
  TrendingUp,
  Film,
  MessageCircle,
  Send,
  Disc3,
  Info,
  ChevronRight,
  X,
  Maximize2,
  Minimize2,
  Trash2,
  FolderOpen,
  FolderPlus,
  FileMusic,
  HeartPulse,
  Waves,
  AudioWaveform,
  Aperture,
  Image as ImageIcon,
} from "lucide-react";
import { useMusicStore, coverProxyUrl, stopTimeSync } from "@/stores/music-store";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import type { Song, Playlist, Artist, MusicComment, Album, Mv, MusicProvider } from "@/types/music";
import { MusicLoginSection } from "@/components/MusicLoginSection";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { buildKaraokeLines } from "@/lib/karaoke-lyrics";
import { KaraokeLyricsView } from "@/components/KaraokeLyricsView";
import { ImmersiveLyricsView, ImmersiveRippleField } from "@/components/ImmersiveLyricsView";
import SpectrumScene from "@/components/SpectrumScene";
import { VinylDisc } from "@/components/VinylDisc";
import { CustomColorPicker } from "@/components/special/custom-color-picker";
import { hexToHsv, hsvToHex } from "@/lib/color-utils";
import { VirtualList } from "@/components/VirtualList";
import { DesktopLyricsSettingsModal } from "@/components/DesktopLyricsSettingsModal";
import { useCoverColor, useCoverEdgeColor } from "@/hooks/use-cover-color";
import { isOverlayOpen } from "@/hooks/use-esc-back";
import { motion, AnimatePresence } from "framer-motion";

// ═══════════════════════════════════════════════
// 动画变体定义
// ═══════════════════════════════════════════════
const listContainerVariants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: { staggerChildren: 0.04, delayChildren: 0.05 },
  },
};

const listItemVariants = {
  hidden: { opacity: 0, y: 8 },
  visible: { opacity: 1, y: 0, transition: { duration: 0.25, ease: "easeOut" } },
  exit: { opacity: 0, transition: { duration: 0.15 } },
};

const dropdownVariants = {
  hidden: { opacity: 0, y: -8, scale: 0.98 },
  visible: { opacity: 1, y: 0, scale: 1, transition: { duration: 0.2, ease: "easeOut" } },
  exit: { opacity: 0, y: -4, scale: 0.98, transition: { duration: 0.12, ease: "easeIn" } },
};

const tabContentVariants = {
  hidden: { opacity: 0, x: 12 },
  visible: { opacity: 1, x: 0, transition: { duration: 0.2, ease: "easeOut" } },
  exit: { opacity: 0, x: -8, transition: { duration: 0.12, ease: "easeIn" } },
};

// 六个样式的预览图列表（用于点击切换而非单击循环；图片存放在 public/style-previews/）
const STYLE_PREVIEWS: ReadonlyArray<{
  key: "glass" | "modern" | "immersive" | "spectrum" | "vinyl" | "cover";
  src: string;
  label: string;
}> = [
  { key: "cover",     src: "/style-previews/cover.png",     label: "封面渐染" },
  { key: "vinyl",     src: "/style-previews/vinyl.png",     label: "透明彩胶" },
  { key: "immersive", src: "/style-previews/immersive.png", label: "沉浸" },
  { key: "spectrum",  src: "/style-previews/spectrum.png",  label: "音域回响" },
  { key: "modern",    src: "/style-previews/modern.png",    label: "现代" },
  { key: "glass",     src: "/style-previews/glass.png",     label: "通透" },
];

const scrollbarSx = (color: string) => ({
  scrollbarGutter: "stable",
  "&::-webkit-scrollbar": { width: "4px" },
  "&::-webkit-scrollbar-thumb": { background: color, borderRadius: "2px" },
  "&::-webkit-scrollbar-track": { background: "transparent" },
});

// 原生 range slider 样式：用 CSS 变量传递颜色，伪元素控制轨道和滑块外观
const rangeSliderSx = {
  "&": {
    appearance: "none",
    WebkitAppearance: "none",
    height: "6px",
    borderRadius: "3px",
    outline: "none",
    cursor: "pointer",
  },
  "&::-webkit-slider-runnable-track": {
    height: "6px",
    borderRadius: "3px",
  },
  "&::-webkit-slider-thumb": {
    WebkitAppearance: "none",
    width: "12px",
    height: "12px",
    borderRadius: "50%",
    marginTop: "-3px",
    border: "none",
  },
  "&::-moz-range-track": {
    height: "6px",
    borderRadius: "3px",
  },
  "&::-moz-range-thumb": {
    width: "12px",
    height: "12px",
    borderRadius: "50%",
    border: "none",
  },
};

/**
 * 生成滑块进度背景的 inline style。
 * 必须用 style 而非 sx 传递动态 background，否则 Emotion 会为
 * 每个唯一百分比值生成一条新 CSS 规则，播放越久 <style> 标签越大，
 * 浏览器样式重计算越慢 —— 这是"播放越久越卡"的根因。
 */
function sliderBgStyle(activeColor: string, pct: number, trackBg: string) {
  return {
    background: `linear-gradient(to right, ${activeColor} 0%, ${activeColor} ${pct}%, ${trackBg} ${pct}%, ${trackBg} 100%)`,
  };
}

// ── 圆角播放控制图标 ──
// 用 SVG path + Q 曲线实现圆角三角形，避免有棱有角

const PlayBtn = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <path d="M 8 5 Q 7 4 6 5 L 6 19 Q 7 20 8 19 L 19 13 Q 20 12 19 11 Z" />
  </svg>
);

const PauseIcon = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <rect x="6" y="4" width="4" height="16" rx="2" />
    <rect x="14" y="4" width="4" height="16" rx="2" />
  </svg>
);

const SkipBackBtn = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <rect x="4" y="5" width="3" height="14" rx="1.5" />
    <polygon points="16,6 8,12 16,18" stroke="currentColor" strokeWidth={2.5} strokeLinejoin="round" />
  </svg>
);

const SkipForwardBtn = ({ size = 24 }: { size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor">
    <polygon points="8,6 16,12 8,18" stroke="currentColor" strokeWidth={2.5} strokeLinejoin="round" />
    <rect x="17" y="5" width="3" height="14" rx="1.5" />
  </svg>
);

// ═══════════════════════════════════════════════
// 音质选项配置
// ═══════════════════════════════════════════════
const QUALITY_OPTIONS: { value: string; label: string; desc: string; svip: boolean }[] = [
  { value: "jymaster", label: "超清母带", desc: "SVIP 专属", svip: true },
  { value: "hires",    label: "高清臻音", desc: "1999k", svip: false },
  { value: "lossless", label: "无损",     desc: "1411k", svip: false },
  { value: "exhigh",   label: "极高",     desc: "999k", svip: false },
  { value: "standard", label: "标准",     desc: "128k", svip: false },
];

// ═══════════════════════════════════════════════
// LRC 解析工具：将 [mm:ss.xx] 格式的歌词文本解析为行数组
// ═══════════════════════════════════════════════
interface LyricLine {
  time: number; // 秒
  text: string;
  translation?: string;
}

function parseLrc(lyric: string, translation?: string): LyricLine[] {
  if (!lyric) return [];
  const lines: LyricLine[] = [];
  const transMap = new Map<number, string>();

  // 解析翻译歌词
  if (translation) {
    const transLines = translation.split("\n");
    for (const line of transLines) {
      const match = line.match(/\[(\d+):(\d+(?:\.\d+)?)\]/);
      if (match) {
        const time = parseInt(match[1]) * 60 + parseFloat(match[2]);
        const text = line.replace(/\[\d+:\d+(?:\.\d+)?\]/, "").trim();
        if (text) transMap.set(time, text);
      }
    }
  }

  // 解析主歌词
  const mainLines = lyric.split("\n");
  for (const line of mainLines) {
    const matches = [...line.matchAll(/\[(\d+):(\d+(?:\.\d+)?)\]/g)];
    if (matches.length === 0) continue;
    const text = line.replace(/\[\d+:\d+(?:\.\d+)?\]/g, "").trim();
    if (!text) continue;
    for (const m of matches) {
      const time = parseInt(m[1]) * 60 + parseFloat(m[2]);
      lines.push({ time, text, translation: transMap.get(time) });
    }
  }

  lines.sort((a, b) => a.time - b.time);
  return lines;
}

// ═══════════════════════════════════════════════
// 歌词滚动组件：超长歌词轮播显示
// ═══════════════════════════════════════════════
function LyricMarquee({ text, isActive, isTranslation, fontSize, activeColor, textColor, subTextColor }: {
  text: string; isActive: boolean; isTranslation?: boolean; fontSize: number;
  activeColor: string; textColor: string; subTextColor: string;
}) {
  const textRef = useRef<HTMLSpanElement>(null);
  const [overflows, setOverflows] = useState(false);

  useEffect(() => {
    if (textRef.current) {
      const parent = textRef.current.parentElement;
      if (parent) setOverflows(textRef.current.scrollWidth > parent.clientWidth);
    }
  }, [text]);

  return (
    <Box
      overflow="hidden"
      whiteSpace="nowrap"
      textAlign="center"
      w="100%"
    >
      {isActive && overflows ? (
        <Box
          as="span"
          display="inline-block"
          whiteSpace="nowrap"
          fontSize={`${fontSize}px`}
          fontWeight="bold"
          color={isTranslation ? subTextColor : activeColor}
          textAlign="center"
          sx={{
            animation: `lyricScroll ${Math.max(text.length * 0.08, 3)}s linear infinite`,
            "@keyframes lyricScroll": {
              "0%": { transform: "translateX(0)" },
              "100%": { transform: "translateX(-50%)" },
            },
          }}
        >
          {text}&nbsp;&nbsp;&nbsp;{text}
        </Box>
      ) : (
        <Box
          as="span"
          ref={textRef}
          display="inline-block"
          whiteSpace="nowrap"
          fontSize={`${isTranslation ? fontSize - 2 : fontSize}px`}
          fontWeight={isActive ? "bold" : "normal"}
          color={isTranslation ? subTextColor : (isActive ? activeColor : textColor)}
          sx={overflows ? { textOverflow: "ellipsis", overflow: "hidden", maxWidth: "100%" } : {}}
        >
          {text}
        </Box>
      )}
    </Box>
  );
}

// ═══════════════════════════════════════════════
// ProgressSection — 独立的进度条组件
// 自己管理 timeupdate 监听，不触发 ExpandedPlayer 重渲染
// ═══════════════════════════════════════════════
const ProgressSection = memo(function ProgressSection({
  activeColor,
  subTextColor,
  sliderTrackBg,
  audioRef,
  currentSongId,
}: {
  activeColor: string;
  subTextColor: string;
  sliderTrackBg: string;
  audioRef: HTMLAudioElement | null;
  currentSongId?: string | number;
}) {
  const [localCurrentTime, setLocalCurrentTime] = useState(0);
  const [localDuration, setLocalDuration] = useState(0);
  const isUserSeekingRef = useRef(false);
  const pendingSeekRef = useRef(0);

  // 切歌时重置
  useEffect(() => {
    setLocalCurrentTime(0);
    setLocalDuration(audioRef?.duration && isFinite(audioRef.duration) ? audioRef.duration : 0);
  }, [currentSongId]); // eslint-disable-line react-hooks/exhaustive-deps

  // 监听 audio timeupdate（含挂载时同步当前时间）
  useEffect(() => {
    if (!audioRef) return;
    if (audioRef.duration && isFinite(audioRef.duration)) {
      setLocalDuration(audioRef.duration);
    }
    // 挂载时同步当前时间（修复暂停后收起/展开进度条归零的问题）
    if (!isUserSeekingRef.current) {
      setLocalCurrentTime(audioRef.currentTime);
    }
    const onTimeUpdate = () => {
      if (isUserSeekingRef.current) return;
      setLocalCurrentTime(audioRef.currentTime);
    };
    const onLoadedMetadata = () => {
      if (isFinite(audioRef.duration)) setLocalDuration(audioRef.duration);
    };
    audioRef.addEventListener("timeupdate", onTimeUpdate);
    audioRef.addEventListener("loadedmetadata", onLoadedMetadata);
    return () => {
      audioRef.removeEventListener("timeupdate", onTimeUpdate);
      audioRef.removeEventListener("loadedmetadata", onLoadedMetadata);
    };
  }, [audioRef]);

  const handleSeekDrag = useCallback((v: number) => {
    if (!isUserSeekingRef.current) return;
    pendingSeekRef.current = v;
    setLocalCurrentTime(v);
  }, []);

  const handleSeekCommit = useCallback(() => {
    if (pendingSeekRef.current !== 0 || isUserSeekingRef.current) {
      const targetTime = pendingSeekRef.current;
      useMusicStore.getState().seekTo(targetTime);
      setTimeout(() => {
        isUserSeekingRef.current = false;
        setLocalCurrentTime(targetTime);
      }, 300);
    }
  }, []);

  return (
    <HStack spacing={3} w="100%" maxW="600px">
      <Text color={subTextColor} fontSize="xs" w="45px" textAlign="center">
        {formatTime(localCurrentTime)}
      </Text>
      <Box
        as="input"
        type="range"
        min={0}
        max={localDuration || 100}
        step={0.1}
        value={localCurrentTime}
        onMouseDown={() => { isUserSeekingRef.current = true; }}
        onTouchStart={() => { isUserSeekingRef.current = true; }}
        onChange={(e) => handleSeekDrag(parseFloat((e.target as HTMLInputElement).value))}
        onMouseUp={handleSeekCommit}
        onTouchEnd={handleSeekCommit}
        tabIndex={-1}
        aria-hidden="true"
        flex={1}
        style={sliderBgStyle(activeColor, localDuration ? (localCurrentTime / localDuration) * 100 : 0, sliderTrackBg)}
        sx={{
          ...rangeSliderSx,
          "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: activeColor },
          "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: activeColor },
          "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
          "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
        }}
      />
      <Text color={subTextColor} fontSize="xs" w="45px" textAlign="center">
        {formatTime(localDuration)}
      </Text>
    </HStack>
  );
});

// ═══════════════════════════════════════════════
// CommentPanel — 评论面板（网易云）
// ═══════════════════════════════════════════════
const formatCommentTime = (ts: number): string => {
  if (!ts) return "";
  const d = new Date(ts);
  const now = Date.now();
  const diff = (now - ts) / 1000;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

interface CommentPanelProps {
  song: Song;
  proxyPort: number;
  activeColor: string;
  subTextColor: string;
  textColor: string;
  hoverBg: string;
  loginInfo: { logged_in: boolean; nickname?: string; avatar?: string } | null;
  scrollbarSx: any;
}

const CommentPanel = memo(function CommentPanel({
  song,
  proxyPort,
  activeColor,
  subTextColor,
  textColor,
  hoverBg,
  loginInfo,
  scrollbarSx,
}: CommentPanelProps) {
  const comments = useMusicStore((s) => s.currentComments);
  const loadingComments = useMusicStore((s) => s.loadingComments);
  const sendingComment = useMusicStore((s) => s.sendingComment);
  const commentError = useMusicStore((s) => s.commentError);
  const [commentInput, setCommentInput] = useState("");
  const toast = useDynamicIsland("music");
  const [page, setPage] = useState(1);
  const [lastLoadedSong, setLastLoadedSong] = useState("");

  // 切换歌曲时重置并加载评论
  useEffect(() => {
    if (song.id !== lastLoadedSong) {
      setPage(1);
      setCommentInput("");
      useMusicStore.getState().clearComments();
      useMusicStore.getState().loadComments(song.id, 1);
      setLastLoadedSong(song.id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [song.id]);

  if (song.provider !== "netease") {
    return (
      <VStack h="100%" align="center" justify="center" spacing={2}>
        <MessageCircle size={32} color={subTextColor} opacity={0.4} />
        <Text color={subTextColor} fontSize="sm">当前平台暂不支持评论</Text>
      </VStack>
    );
  }

  const handleSend = async () => {
    const content = commentInput.trim();
    if (!content || !loginInfo?.logged_in) return;
    const ok = await useMusicStore.getState().sendComment(song.id, content);
    if (ok) {
      setCommentInput("");
      setPage(1);
      toast({ title: "评论成功", status: "success", duration: 2000, isClosable: true });
    } else {
      toast({ title: "评论失败", status: "error", duration: 3000, isClosable: true });
    }
  };

  const renderCommentRow = (c: MusicComment) => (
    <VStack key={c.comment_id} spacing={1} align="stretch" p={3} borderRadius="md" _hover={{ bg: hoverBg }} transition="background 0.15s">
      <HStack spacing={2}>
        <Box w="28px" h="28px" borderRadius="full" overflow="hidden" flexShrink={0} bg="gray.600">
          <ChakraImage
            src={coverProxyUrl(c.avatar, proxyPort)}
            alt=""
            w="28px"
            h="28px"
            objectFit="cover"
            fallback={<User size={16} color={subTextColor} />}
          />
        </Box>
        <Text color={textColor} fontSize="sm" fontWeight="bold" flexShrink={0} maxW="50%" noOfLines={1}>
          {c.nickname}
        </Text>
        <Text color={subTextColor} fontSize="xs" flexShrink={0}>{formatCommentTime(c.time)}</Text>
        <Box flex={1} />
        <HStack spacing={1} flexShrink={0}>
          <Heart size={13} color={subTextColor} />
          <Text fontSize="xs" color={subTextColor}>{c.liked_count}</Text>
        </HStack>
      </HStack>
      <Text color={textColor} fontSize="sm" whiteSpace="pre-wrap" wordBreak="break-word">
        {c.content}
      </Text>
    </VStack>
  );

  return (
    <VStack flex={1} align="stretch" minH={0} spacing={3}>
      {/* 评论输入框 */}
      <Box flexShrink={0}>
        {loginInfo?.logged_in ? (
          <HStack spacing={2}>
            <Input
              value={commentInput}
              onChange={(e) => setCommentInput(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleSend(); } }}
              placeholder={`向 ${song.name} 发个评论吧...`}
              size="sm"
              variant="filled"
              bg={hoverBg}
              border="1px solid"
              borderColor="transparent"
              borderRadius="lg"
              _placeholder={{ color: subTextColor, opacity: 0.7 }}
              color={textColor}
              flex={1}
              isDisabled={sendingComment}
              _focus={{ borderColor: activeColor, boxShadow: `0 0 0 2px ${activeColor}33, 0 0 0 1px ${activeColor}`, bg: hoverBg }}
              _hover={{ borderColor: `${activeColor}66` }}
            />
            <Button
              size="sm"
              onClick={handleSend}
              isLoading={sendingComment}
              isDisabled={!commentInput.trim()}
              leftIcon={<Send size={14} />}
              sx={{ bg: activeColor, color: "white", _hover: { opacity: 0.85 } }}
              borderRadius="lg"
            >
              发送
            </Button>
          </HStack>
        ) : (
          <Text color={subTextColor} fontSize="xs" textAlign="center" py={2}>
            登录后可参与评论
          </Text>
        )}
      </Box>

      {loadingComments && !comments ? (
        <VStack py={8}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
      ) : commentError ? (
        <VStack flex={1} align="center" justify="center" spacing={2}>
          <MessageCircle size={28} color={subTextColor} opacity={0.4} />
          <Text color="red.400" fontSize="sm">评论加载失败</Text>
          <Text color={subTextColor} fontSize="xs" textAlign="center" px={4} wordBreak="break-all">
            {commentError}
          </Text>
          <Button size="xs" variant="ghost" color={activeColor} onClick={() => useMusicStore.getState().loadComments(song.id, 1)}>
            重试
          </Button>
        </VStack>
      ) : comments && comments.total > 0 ? (
        <Box flex={1} overflowY="auto" sx={scrollbarSx}>
          {/* 热门评论 */}
          {comments.hot_comments.length > 0 && (
            <>
              <Text fontSize="xs" fontWeight="bold" color={activeColor} mb={1} mt={2}>热门评论</Text>
              <VStack spacing={1} align="stretch">
                {comments.hot_comments.map(renderCommentRow)}
              </VStack>
            </>
          )}
          <Text fontSize="xs" fontWeight="bold" color={activeColor} mb={1} mt={3}>
            最新评论 ({comments.total})
          </Text>
          <VStack spacing={1} align="stretch">
            {comments.comments.map(renderCommentRow)}
          </VStack>
          {comments.has_more && (
            <Button
              size="xs"
              variant="ghost"
              w="100%"
              mt={2}
              color={activeColor}
              isLoading={loadingComments}
              onClick={() => {
                const next = page + 1;
                setPage(next);
                useMusicStore.getState().loadComments(song.id, next);
              }}
            >
              加载更多评论
            </Button>
          )}
        </Box>
      ) : (
        <VStack flex={1} align="center" justify="center" spacing={2}>
          <MessageCircle size={28} color={subTextColor} opacity={0.4} />
          <Text color={subTextColor} fontSize="sm">暂无评论，来抢沙发吧~</Text>
        </VStack>
      )}
    </VStack>
  );
});

// ═══════════════════════════════════════════════
// ExpandedPlayer — 展开的全屏播放器
// 点击播放器封面展开，左侧封面+信息，右侧歌词
// ═══════════════════════════════════════════════
interface ExpandedPlayerProps {
  onClose: () => void;
}

/**
 * 沉浸背景固定色板（纯色，均为深色调中性色）：
 * 青 / 深蓝 / 深橙 / 紫粉 / 黄色 / 红色 / 绿色 / 灰色
 */
const VIVID_PALETTE: { name: string; hue: number; hex: string; rgb: [number, number, number] }[] = [
  { name: "青", hue: 194, hex: "#126d83", rgb: [18, 109, 131] },
  { name: "深蓝", hue: 232, hex: "#303679", rgb: [48, 54, 121] },
  { name: "深橙", hue: 18, hex: "#804127", rgb: [128, 65, 39] },
  { name: "紫粉", hue: 313, hex: "#7e356e", rgb: [126, 53, 110] },
  { name: "黄色", hue: 44, hex: "#7e681f", rgb: [126, 104, 31] },
  { name: "红色", hue: 354, hex: "#7d2c33", rgb: [125, 44, 51] },
  { name: "绿色", hue: 84, hex: "#5e8023", rgb: [94, 128, 35] },
  { name: "灰色", hue: -1, hex: "#333333", rgb: [51, 51, 51] },
];

/** HSL → hex */
function hslToHex(h: number, s: number, l: number): string {
  const hue2rgb = (p: number, q: number, t: number) => {
    let tt = t;
    if (tt < 0) tt += 1;
    if (tt > 1) tt -= 1;
    if (tt < 1 / 6) return p + (q - p) * 6 * tt;
    if (tt < 1 / 2) return q;
    if (tt < 2 / 3) return p + (q - p) * (2 / 3 - tt) * 6;
    return p;
  };
  let r: number, g: number, b: number;
  if (s === 0) {
    r = g = b = l;
  } else {
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hue2rgb(p, q, h / 360 + 1 / 3);
    g = hue2rgb(p, q, h / 360);
    b = hue2rgb(p, q, h / 360 - 1 / 3);
  }
  return "#" + [r, g, b].map((v) => Math.round(v * 255).toString(16).padStart(2, "0")).join("");
}

/** 在颜色基础上微调亮度（保持色相/饱和度），返回 hex */
function shiftLightness(rgb: [number, number, number], dL: number): string {
  const r = rgb[0] / 255, g = rgb[1] / 255, b = rgb[2] / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const d = max - min;
  const l0 = (max + min) / 2;
  const s = d === 0 ? 0 : l0 > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (d !== 0) {
    if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) * 60;
    else if (max === g) h = ((b - r) / d + 2) * 60;
    else h = ((r - g) / d + 4) * 60;
    h = ((h % 360) + 360) % 360;
  }
  const nl = Math.min(0.97, Math.max(0.03, l0 + dL));
  return hslToHex(h, s, nl);
}

/**
 * 封面主色 → 沉浸背景色：
 * 提取封面色相，在固定色板中选最近色；封面几乎无色（饱和度过低）时用灰色。
 * 额外返回 deep/light（仅微调亮度），用于"左深右浅但差别不大"的柔和渐变背景
 */
function makeVividColor(cr: number, cg: number, cb: number): {
  hex: string;
  deep: string;
  light: string;
  rgb: [number, number, number];
  isLight: boolean;
} {
  const r = cr / 255, g = cg / 255, b = cb / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const d = max - min;
  const l = (max + min) / 2;
  const s = d === 0 ? 0 : l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = 0;
  if (d !== 0) {
    switch (max) {
      case r: h = ((g - b) / d + (g < b ? 6 : 0)) * 60; break;
      case g: h = ((b - r) / d + 2) * 60; break;
      default: h = ((r - g) / d + 4) * 60;
    }
  }
  h = ((h % 360) + 360) % 360;

  // 饱和度极低 → 灰色
  if (s < 0.08) {
    return {
      hex: "#333333",
      deep: shiftLightness([51, 51, 51], -0.07),
      light: shiftLightness([51, 51, 51], 0.07),
      rgb: [51, 51, 51],
      isLight: false,
    };
  }
  // 色相环找最近固定色（灰色 hue=-1 不参与匹配）
  let best = VIVID_PALETTE[0];
  let bestDist = 361;
  for (const c of VIVID_PALETTE) {
    if (c.hue < 0) continue;
    const dist = Math.min(Math.abs(h - c.hue), 360 - Math.abs(h - c.hue));
    if (dist < bestDist) {
      bestDist = dist;
      best = c;
    }
  }
  return {
    hex: best.hex,
    deep: shiftLightness(best.rgb, -0.07),
    light: shiftLightness(best.rgb, 0.07),
    rgb: best.rgb,
    isLight: false,
  };
}

// 封面渐染样式：左侧满铺封面图层（伸到顶栏/控制栏下方），歌词条压在羽化区上。
// 右缘用 mask 羽化融入"封面最右缘主导色"背景——羽化末端的像素色与背景色天然一致，无接缝；
// 顶/底各叠一层与文字方案同向的轻渐变（浅缘配白雾、深缘配黑雾），保证悬浮图标可读
function CoverBleedLayer({ coverUrl, scrim }: { coverUrl: string; scrim: string }) {
  const [loaded, setLoaded] = useState(false);
  return (
    <Box
      position="absolute"
      top={0}
      bottom={0}
      left={0}
      w="58vw"
      zIndex={1}
      pointerEvents="none"
      sx={{
        maskImage: "linear-gradient(90deg, black 78%, transparent 98%)",
        WebkitMaskImage: "linear-gradient(90deg, black 78%, transparent 98%)",
      }}
    >
      <Box
        as="img"
        src={coverUrl}
        alt=""
        w="100%"
        h="100%"
        objectFit="cover"
        draggable={false}
        onLoad={() => setLoaded(true)}
        sx={{ opacity: loaded ? 1 : 0, transition: "opacity 0.45s ease" }}
      />
      <Box position="absolute" top={0} left={0} right={0} h="110px"
        sx={{ background: `linear-gradient(180deg, ${scrim} 0%, transparent 100%)` }} />
      <Box position="absolute" bottom={0} left={0} right={0} h="130px"
        sx={{ background: `linear-gradient(0deg, ${scrim} 0%, transparent 100%)` }} />
    </Box>
  );
}

const ExpandedPlayer = memo(function ExpandedPlayer({ onClose }: ExpandedPlayerProps) {
  const currentSong = useMusicStore((s) => s.currentSong);
  const isPlaying = useMusicStore((s) => s.isPlaying);
  const volume = useMusicStore((s) => s.volume);
  const playMode = useMusicStore((s) => s.playMode);
  const proxyPort = useMusicStore((s) => s.proxyPort);
  // 滚轮调节音量：上滚 +5%，下滚 -5%，四舍五入到 0.01，夹在 0~1 之间
  const handleWheelVolume = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    if (e.deltaY === 0) return;
    const step = 0.05;
    const cur = useMusicStore.getState().volume;
    const next = Math.round((cur + (e.deltaY < 0 ? step : -step)) * 100) / 100;
    useMusicStore.getState().setVolume(Math.min(1, Math.max(0, next)));
  }, []);
  const audioRef = useMusicStore((s) => s.audioRef);
  const currentLyrics = useMusicStore((s) => s.currentLyrics);
  const loadingLyrics = useMusicStore((s) => s.loadingLyrics);
  const likedSongIds = useMusicStore((s) => s.likedSongIds);
  const loginInfo = useMusicStore((s) => s.loginInfo);
  const playbackQuality = useMusicStore((s) => s.playbackQuality);
  const currentQuality = useMusicStore((s) => s.currentQuality);
  const currentBitrate = useMusicStore((s) => s.currentBitrate);
  const lyricsFontSize = useMusicStore((s) => s.lyricsFontSize);
  const lyricsHighlightColor = useMusicStore((s) => s.lyricsHighlightColor);
  const vinylColorMode = useMusicStore((s) => s.vinylColorMode);
  const vinylCustomColor = useMusicStore((s) => s.vinylCustomColor);
  const expandedStyle = useMusicStore((s) => s.expandedStyle);
  const dynamicEnabled = useMusicStore((s) => s.dynamicEnabled);
  const coverFilmEffect = useMusicStore((s) => s.coverFilmEffect);
  const desktopLyricsVisible = useMusicStore((s) => s.desktopLyricsVisible);
  const playQueue = useMusicStore((s) => s.playQueue);
  const currentIndex = useMusicStore((s) => s.currentIndex);

  const [isClosing, setIsClosing] = useState(false);
  // 关闭动画定时器，防止组件卸载后定时器仍触发
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    return () => {
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
    };
  }, []);
  const [rightTab, setRightTab] = useState<"lyrics" | "comments">("lyrics");
  // 页面级全屏：播放器放大铺满整个窗口，其他组件被覆盖淡出（非系统窗口全屏）
  const [pageFullscreen, setPageFullscreen] = useState(false);

  // 光标无移动自动隐藏顶部/底部控制 UI（鼠标一动立即恢复）
  const [uiHidden, setUiHidden] = useState(false);
  const uiIdleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 样式预览菜单：点击按钮从主内容区左侧弹出三个预览图，点击预览图直接切换。
  // 弹层渲染在播放器容器内（absolute 垂直居中、固定宽度），不受底部控制栏 uiHidden 淡出影响
  const [styleMenuOpen, setStyleMenuOpen] = useState(false);

  const toggleStyleMenu = useCallback(() => {
    setStyleMenuOpen((open) => !open);
  }, []);

  // 点击浮层与触发按钮之外任意位置时关闭预览菜单
  useEffect(() => {
    if (!styleMenuOpen) return;
    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("[data-style-menu], [data-style-menu-trigger]")) return;
      setStyleMenuOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [styleMenuOpen]);

  const scheduleUiHide = useCallback(() => {
    setUiHidden(false);
    if (uiIdleTimerRef.current) clearTimeout(uiIdleTimerRef.current);
    uiIdleTimerRef.current = setTimeout(() => setUiHidden(true), 3000);
  }, []);

  useEffect(() => {
    scheduleUiHide();
    const onAny = () => scheduleUiHide();
    window.addEventListener("mousemove", onAny);
    window.addEventListener("mousedown", onAny);
    window.addEventListener("touchstart", onAny);
    return () => {
      window.removeEventListener("mousemove", onAny);
      window.removeEventListener("mousedown", onAny);
      window.removeEventListener("touchstart", onAny);
      if (uiIdleTimerRef.current) clearTimeout(uiIdleTimerRef.current);
    };
  }, [scheduleUiHide]);

  // 用 ref 持有最新全屏状态，连点也能基于最新值翻转（避免闭包陈旧值）
  const pageFullscreenRef = useRef(pageFullscreen);
  pageFullscreenRef.current = pageFullscreen;

  const handleToggleFullscreen = useCallback(() => {
    const next = !pageFullscreenRef.current;
    pageFullscreenRef.current = next;
    setPageFullscreen(next);
    // 广播给标题栏：全屏时淡出搜索框/游戏模式、左上角显示"缩小"按钮
    window.dispatchEvent(new CustomEvent("immersive-page-fullscreen", { detail: next }));
  }, []);

  // 监听标题栏"缩小"按钮等外部触发，双向同步页面全屏状态
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<boolean>).detail;
      pageFullscreenRef.current = detail;
      setPageFullscreen(detail);
    };
    window.addEventListener("immersive-page-fullscreen", handler as EventListener);
    return () => {
      window.removeEventListener("immersive-page-fullscreen", handler as EventListener);
    };
  }, []);

  const handleCloseWithAnimation = useCallback(() => {
    setIsClosing(true);
    setPageFullscreen(false);
    // 同步标题栏：退出全屏恢复搜索框/游戏模式
    window.dispatchEvent(new CustomEvent("immersive-page-fullscreen", { detail: false }));
    closeTimerRef.current = setTimeout(() => onClose(), 300);
  }, [onClose]);

  // Esc 关闭展开播放器：浮层比全局 Esc 返回更上层，优先响应；
  // 有弹窗/菜单打开时让位给它们自己的 Esc 处理
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (isOverlayOpen()) return;
      e.preventDefault();
      e.stopPropagation();
      handleCloseWithAnimation();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [handleCloseWithAnimation]);

  // 音乐控制热键（用于按钮 tooltip 展示）
  const { musicPrevHotkey, musicNextHotkey, musicPlayPauseHotkey } = useAppStartup();

  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();
  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);

  // 封面主色提取
  const coverUrl = currentSong ? coverProxyUrl(currentSong.cover, proxyPort) : "";
  const coverColor = useCoverColor(coverUrl);

  // 现代模式：根据封面颜色决定文字色和背景
  const [cr, cg, cb] = coverColor.rgb;
  // 沉浸模式：封面色匹配固定色板，左略深右略浅的柔和渐变背景
  const vividBg = useMemo(() => makeVividColor(cr, cg, cb), [cr, cg, cb]);
  const immersiveBgGradient = `linear-gradient(90deg, ${vividBg.deep} 0%, ${vividBg.hex} 50%, ${vividBg.light} 100%)`;
  const modernBgSolid = coverColor.hex;
  const modernBgDark = `rgb(${Math.round(cr * 0.25)},${Math.round(cg * 0.25)},${Math.round(cb * 0.25)})`;
  const modernBgGradient = dynamicEnabled
    ? `linear-gradient(135deg, ${modernBgSolid} 0%, ${modernBgSolid} 40%, ${modernBgDark} 100%)`
    : `linear-gradient(135deg, ${modernBgSolid} 0%, ${modernBgSolid} 40%, ${modernBgDark} 100%)`;
  // 动态模式通过 CSS animation 实现渐变色流动
  const modernBgDynamic = `linear-gradient(45deg, ${modernBgSolid}, ${modernBgDark}, ${modernBgSolid})`;
  const modernBgFinal = dynamicEnabled ? modernBgDynamic : modernBgGradient;
  const modernTextColor = coverColor.isLight ? "#1a1a2e" : "#f0f0f0";
  const modernSubTextColor = coverColor.isLight ? "#4a4a5e" : "#b0b0b0";
  const modernBorderColor = coverColor.isLight ? "rgba(0,0,0,0.12)" : "rgba(255,255,255,0.15)";
  const modernHoverBg = coverColor.isLight ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.12)";

  // 本地导入歌曲没有沉浸模式：沉浸样式对无歌词的本地歌曲回退为通透；
  // 有歌词的本地歌曲与在线歌曲一致，可正常使用沉浸歌词可视化
  const isLocalSong = currentSong?.provider === "local";
  const currentLyricsSongId = useMusicStore((s) => s.currentLyricsSongId);
  // 歌词解析：优先 YRC 逐字歌词，降级为 LRC 逐行歌词（无时间轴文本解析为 0 行）
  const karaokeLines = useMemo(() => {
    return buildKaraokeLines(currentLyrics);
  }, [currentLyrics]);
  // 本地歌曲是否有歌词：仅当歌词已为该歌曲加载完成（currentLyricsSongId 匹配）且
  // 解析出带时间轴的内容时才显示歌词面板；否则维持旧的居中封面布局
  const hasLocalLyrics =
    isLocalSong && karaokeLines.length > 0 && currentLyricsSongId === currentSong?.id;
  // 是否显示右侧歌词面板：在线歌曲恒显示；本地歌曲仅在解析出歌词时显示（无歌词保持居中布局）
  const showLyricsPanel = !isLocalSong || hasLocalLyrics;
  // 歌词视图是否激活：评论 Tab 仅网易云可用，其它平台（含本地歌）恒为歌词视图
  const lyricsTabActive = rightTab === "lyrics" || currentSong?.provider !== "netease";
  const currentStyle =
    isLocalSong && !hasLocalLyrics && expandedStyle === "immersive" ? "glass" : expandedStyle;
  const isModern = currentStyle === "modern";
  const isImmersive = currentStyle === "immersive";
  const isSpectrum = currentStyle === "spectrum";
  const isVinyl = currentStyle === "vinyl";
  const isCover = currentStyle === "cover";
  // 封面渐染专用：封面最右缘主导色做右侧背景（与封面右缘相融，网易云做法）；非该样式不加载图片
  const coverEdge = useCoverEdgeColor(isCover && coverUrl ? coverUrl : "");
  const coverTextColor = coverEdge.isLight ? "#1a1a2e" : "#f0f0f0";
  const coverSubTextColor = coverEdge.isLight ? "#4a4a5e" : "#b0b0b0";
  const coverHoverBg = coverEdge.isLight ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.12)";
  // 无歌词的本地歌曲跳过沉浸预览（音域回响/透明彩胶与是否有歌词无关，不过滤）
  const availableStylePreviews = isLocalSong && !hasLocalLyrics
    ? STYLE_PREVIEWS.filter((s) => s.key !== "immersive")
    : STYLE_PREVIEWS;

  // 透明彩胶主色：auto 跟随封面提取色（钳制饱和/亮度保证白底上可读），custom 用手动固定色
  const vinylAccent = useMemo(() => {
    if (vinylColorMode === "custom") return vinylCustomColor;
    const { h, s, v } = hexToHsv(coverColor.hex);
    return hsvToHex(h, Math.min(92, Math.max(48, s)), Math.min(66, Math.max(34, v)));
  }, [vinylColorMode, vinylCustomColor, coverColor.hex]);

  const bgColor = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const glassBorderColor = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const sliderTrackBg = useColorModeValue("rgba(0,0,0,0.1)", "rgba(255,255,255,0.9)");

  // 文字颜色覆写（现代/封面渐染/音域回响；封面渐染按封面右缘色判断明暗，音域回响固定亮色，透明彩胶固定深灰）
  const effectiveTextColor = isModern ? modernTextColor : isCover ? coverTextColor : isSpectrum ? "#f0f0f0" : isVinyl ? "#333338" : textColor;
  const effectiveSubTextColor = isModern ? modernSubTextColor : isCover ? coverSubTextColor : isSpectrum ? "rgba(255,255,255,0.6)" : isVinyl ? "rgba(51,51,56,0.55)" : subTextColor;
  const effectiveHoverBg = isModern ? modernHoverBg : isCover ? coverHoverBg : isSpectrum ? "rgba(255,255,255,0.12)" : isVinyl ? "rgba(0,0,0,0.07)" : hoverBg;

  // 下拉菜单配色：白底黑字
  const menuBg = "white";
  const menuBorder = "rgba(0,0,0,0.1)";
  const menuText = "#1a1a2e";
  const menuMuted = "#666";

  const ModeIcon = playMode === "one" ? Repeat1 : playMode === "shuffle" ? Shuffle : playMode === "heartbeat" ? HeartPulse : Repeat;

  // memoize scrollbarSx，避免每次渲染创建新对象导致 KaraokeLyricsView 不必要重渲染
  const memoScrollbarSx = useMemo(() => scrollbarSx(activeColor), [activeColor]);

  // 歌词加载由 playSong 在 URL 获取前并行触发，不再需要此处 useEffect 重复加载

  if (!currentSong) return null;

  const isLiked = likedSongIds.has(currentSong.id);

  const playerEl = (
    <Box
      position={pageFullscreen ? "fixed" : "absolute"}
      top={0}
      left={0}
      right={0}
      bottom={0}
      pt={pageFullscreen ? "48px" : 0}
      zIndex={pageFullscreen ? 950 : 9999}
      bg={isModern
        ? modernBgFinal
        : isCover
          ? (coverUrl ? coverEdge.hex : modernBgFinal)
          : isSpectrum
            ? "#05070c"
            : isImmersive
              ? immersiveBgGradient
              : isVinyl
                ? "linear-gradient(160deg, #dfdfe2 0%, #d8d8dc 55%, #d1d1d6 100%)"
                : bgColor}
      backdropFilter={isModern || isCover || isImmersive || isSpectrum || isVinyl ? "none" : "blur(20px)"}
      borderRadius={pageFullscreen ? 0 : "xl"}
      overflow="hidden"
      boxShadow={pageFullscreen ? "none" : "xl"}
      sx={{
        "@keyframes expandedPlayerSlideUp": {
          from: { transform: "translateY(100%)", opacity: 0 },
          to: { transform: "translateY(0)", opacity: 1 },
        },
        "@keyframes expandedPlayerSlideDown": {
          from: { transform: "translateY(0)", opacity: 1 },
          to: { transform: "translateY(100%)", opacity: 0 },
        },
        // 页面级全屏入场：放大铺满窗口 + 淡入
        "@keyframes immersiveFullscreenIn": {
          from: { transform: "scale(0.93)", opacity: 0.75 },
          to: { transform: "scale(1)", opacity: 1 },
        },
        display: "flex",
        flexDirection: "column",
        WebkitBackdropFilter: isModern || isCover || isImmersive || isSpectrum || isVinyl ? "none" : "blur(20px)",
        animation: (() => {
          const slide = isClosing
            ? "expandedPlayerSlideDown 0.3s cubic-bezier(0.4, 0, 0.2, 1) forwards"
            : "expandedPlayerSlideUp 0.35s cubic-bezier(0.4, 0, 0.2, 1)";
          // 进入页面全屏时叠加放大淡入动画
          const full = pageFullscreen && !isClosing
            ? ", immersiveFullscreenIn 0.35s cubic-bezier(0.25, 0.8, 0.35, 1)"
            : "";
          const dynamic = (dynamicEnabled && (isModern || isImmersive)) ? ", dynamicBg 8s ease infinite" : "";
          return `${slide}${full}${dynamic}`;
        })(),
        ...(dynamicEnabled && (isModern || isImmersive) ? {
          backgroundSize: "400% 400%",
          "@keyframes dynamicBg": {
            "0%": { backgroundPosition: "0% 50%" },
            "50%": { backgroundPosition: "100% 50%" },
            "100%": { backgroundPosition: "0% 50%" },
          },
        } : {}),
      }}
    >
      {/* 沉浸模式全屏水波层：铺满整个播放器（z=1），上下栏与歌词在其上层 */}
      {isImmersive && (
        <ImmersiveRippleField isPlaying={isPlaying} bgRgb={vividBg.rgb} />
      )}
      {/* 透明彩胶：右上角伸出的半透明彩色胶片 + 光晕（z=1），歌词与控制栏在其上层 */}
      {isVinyl && (
        <VinylDisc isPlaying={isPlaying} accentColor={vinylAccent} coverUrl={coverUrl} />
      )}
      {/* 封面渐染：左侧满铺封面原图（z=1，伸到顶栏/控制栏下方），右缘羽化融入右缘主导色背景 */}
      {isCover && coverUrl && (
        <CoverBleedLayer
          coverUrl={coverUrl}
          scrim={coverEdge.isLight ? "rgba(255,255,255,0.3)" : "rgba(0,0,0,0.32)"}
        />
      )}
      {/* 音域回响全屏 3D 地形场景：铺满整个播放器（含顶部/底部栏区域，z=1），
          控制栏透明悬浮其上不遮挡。常驻渲染（active 控制显隐），避免切换样式时
          反复销毁/重建 WebGL context 导致切回无效果 */}
      <SpectrumScene audioRef={audioRef} isPlaying={isPlaying} coverColor={vividBg} active={isSpectrum} />
      {/* 顶部栏：关闭按钮 + 沉浸模式全屏按钮（光标无移动自动隐藏） */}
      <HStack
        justify="space-between"
        p={4}
        flexShrink={0}
        position="relative"
        zIndex={2}
        opacity={uiHidden ? 0 : 1}
        transform={uiHidden ? "translateY(-10px)" : "translateY(0)"}
        pointerEvents={uiHidden ? "none" : "auto"}
        visibility={uiHidden ? "hidden" : "visible"}
        sx={{ transition: "opacity 0.35s ease, transform 0.35s ease, visibility 0.35s ease" }}
      >
        <HStack spacing={3}>
          <Tooltip label="收起">
            <IconButton
              aria-label="Close"
              icon={<ChevronDown size={24} />}
              size="sm"
              variant="ghost"
              onClick={handleCloseWithAnimation}
              sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
            />
          </Tooltip>
          <Tooltip label={pageFullscreen ? "退出全屏" : "进入全屏"}>
            <IconButton
              aria-label="Fullscreen"
              icon={pageFullscreen ? <Minimize2 size={20} /> : <Maximize2 size={20} />}
              size="sm"
              variant="ghost"
              onClick={handleToggleFullscreen}
              sx={{
                color: pageFullscreen ? activeColor : effectiveTextColor,
                _hover: { bg: effectiveHoverBg },
              }}
            />
          </Tooltip>
        </HStack>
      </HStack>

      {/* 主体：音域回响为全屏 3D 地形律动场景（在播放器容器层渲染，此处留空）；
          沉浸模式为全屏歌词可视化；否则左（封面+信息）+ 右（歌词）。
          外层 wrapper 同时承载样式预览弹层（以主内容区为基准垂直居中，不含底部控制栏） */}
      <Box flex={1} minH={0} position="relative" overflow="hidden" display="flex" flexDirection="column">
      {isSpectrum ? null : isImmersive ? (
        <Box flex={1} minH={0} px={6} pb={2} overflow="hidden" display="flex" flexDirection="column">
          <ImmersiveLyricsView
            lines={karaokeLines}
            loading={loadingLyrics}
            audioRef={audioRef}
            isPlaying={isPlaying}
            coverColor={vividBg}
            baseFontSize={lyricsFontSize}
          />
        </Box>
      ) : isVinyl ? (
        // 透明彩胶：左侧歌名 + 歌词列表（当前行高亮色 = 胶片颜色），右侧留给胶片
        <Box
          flex={1}
          minH={0}
          position="relative"
          zIndex={2}
          px={{ base: 10, lg: "8vw" }}
          pb={2}
          pt={2}
          overflow="hidden"
          display="flex"
          flexDirection="column"
          w="52%"
        >
          <VStack spacing={1} align="flex-start" mb={4} flexShrink={0}>
            <Text
              fontSize={{ base: "26px", lg: "32px" }}
              fontWeight="900"
              color={effectiveTextColor}
              lineHeight={1.25}
              noOfLines={1}
            >
              {currentSong.name}
            </Text>
            <Text fontSize="md" color={effectiveSubTextColor} noOfLines={1}>
              {currentSong.artist}
            </Text>
          </VStack>
          <KaraokeLyricsView
            lines={karaokeLines}
            loading={loadingLyrics}
            fontSize={lyricsFontSize}
            activeColor={vinylAccent}
            highlightColor={vinylAccent}
            textColor="#45454c"
            subTextColor="rgba(66,66,74,0.5)"
            scrollbarSx={memoScrollbarSx}
            audioRef={audioRef}
            isPlaying={isPlaying}
            maxHeight="100%"
            align="left"
            lineScale={false}
          />
        </Box>
      ) : isCover ? (
        // 封面渐染：左侧为满铺封面图层（在容器层渲染），歌名 + 左对齐歌词压在封面羽化区上
        <Box
          flex={1}
          minH={0}
          position="relative"
          zIndex={2}
          ml="auto"
          w="50%"
          pr={{ base: 8, lg: "5vw" }}
          pl={2}
          pt={2}
          pb={2}
          overflow="hidden"
          display="flex"
          flexDirection="column"
        >
          <VStack spacing={1} align="flex-start" mb={4} flexShrink={0}>
            <Text
              fontSize={{ base: "26px", lg: "32px" }}
              fontWeight="900"
              color={effectiveTextColor}
              lineHeight={1.25}
              noOfLines={1}
            >
              {currentSong.name}
            </Text>
            <Text fontSize="md" color={effectiveSubTextColor} noOfLines={1}>
              {currentSong.artist}
            </Text>
          </VStack>
          <KaraokeLyricsView
            lines={karaokeLines}
            loading={loadingLyrics}
            fontSize={lyricsFontSize}
            activeColor={activeColor}
            highlightColor={lyricsHighlightColor}
            textColor={effectiveTextColor}
            subTextColor={effectiveSubTextColor}
            scrollbarSx={memoScrollbarSx}
            audioRef={audioRef}
            isPlaying={isPlaying}
            maxHeight="100%"
            align="left"
            lineScale={false}
          />
        </Box>
      ) : (
      <HStack
        flex={1}
        spacing={8}
        px={8}
        pb={2}
        align="stretch"
        overflow="hidden"
        minH={0}
        justify={showLyricsPanel ? undefined : "center"}
      >
        {/* 左侧：封面 + 歌曲信息 */}
        <VStack
          spacing={6}
          align="center"
          justify="center"
          flex={showLyricsPanel ? 1 : "0 0 auto"}
          flexShrink={showLyricsPanel ? undefined : 0}
          minW={0}
          w={showLyricsPanel ? undefined : "400px"}
          maxW={showLyricsPanel ? undefined : "460px"}
        >
          {/* 碟片模式 */}
          {coverFilmEffect ? (
            <Box
              position="relative"
              w={{ base: "240px", md: "320px", lg: "360px" }}
              h={{ base: "240px", md: "320px", lg: "360px" }}
            >
              {/* 唱臂（磁头）— SVG 弯臂 + 圆点支座 + 数据线接头 */}
              <Box
                position="absolute"
                top={{ base: "-15px", md: "-18px", lg: "-20px" }}
                right={{ base: "-8px", md: "-10px", lg: "-12px" }}
                w={{ base: "48px", md: "56px", lg: "62px" }}
                h={{ base: "180px", md: "220px", lg: "250px" }}
                zIndex={5}
                sx={{
                  transformOrigin: "top right",
                  transform: isPlaying
                    ? "rotate(30deg)"
                    : "rotate(-10deg)",
                  transition: "transform 0.6s cubic-bezier(0.4, 0, 0.2, 1)",
                }}
              >
                <svg
                  viewBox="0 0 48 220"
                  style={{
                    overflow: "visible",
                    width: "100%",
                    height: "100%",
                  }}
                  preserveAspectRatio="xMidYMid meet"
                >
                  {/* 圆点支座（旋转轴） */}
                  <circle cx="42" cy="8" r="7" fill="url(#pivotGrad)" stroke="rgba(255,255,255,0.2)" strokeWidth="0.5" />
                  <circle cx="40" cy="6" r="2" fill="rgba(255,255,255,0.3)" />
                  {/* 唱臂杆 — 垂直段 + 30° 弯折 */}
                  <path
                    d="M 42 14 L 42 135 Q 42 145, 30 155 L 18 175"
                    fill="none"
                    stroke="url(#armGrad)"
                    strokeWidth="4"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  {/* 唱臂内层高光 */}
                  <path
                    d="M 42 14 L 42 135 Q 42 145, 30 155 L 18 175"
                    fill="none"
                    stroke="rgba(255,255,255,0.12)"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                  {/* 数据线接头磁头 — 金属外壳 + 插槽 + 触点 */}
                  {/* 金属外壳主体 */}
                  <rect
                    x="9" y="170" width="18" height="22" rx="4"
                    fill="url(#headShellGrad)"
                    stroke="rgba(0,0,0,0.3)"
                    strokeWidth="0.5"
                  />
                  {/* 外壳顶部高光 */}
                  <rect x="10.5" y="171.5" width="15" height="4" rx="2" fill="url(#headTopShine)" />
                  {/* 左右金属侧面高光 */}
                  <rect x="9.5" y="172" width="1.5" height="18" rx="0.75" fill="rgba(255,255,255,0.25)" />
                  <rect x="25" y="172" width="1.5" height="18" rx="0.75" fill="rgba(0,0,0,0.15)" />
                  {/* 底部插槽开口（黑色凹槽） */}
                  <rect x="12" y="184" width="12" height="6" rx="1.5" fill="#000" />
                  {/* 插槽内的金属触点条 */}
                  <rect x="13" y="185.5" width="10" height="1" rx="0.5" fill="#555" />
                  <rect x="13" y="187.5" width="10" height="1" rx="0.5" fill="#444" />
                  {/* 底部连接处缩窄颈 */}
                  <rect x="14" y="192" width="8" height="4" rx="1" fill="url(#neckGrad)" />
                  {/* 唱针针尖 */}
                  <path d="M 18 196 L 18 202 L 16.5 204" fill="none" stroke="#999" strokeWidth="0.8" strokeLinecap="round" />

                  {/* 渐变定义 */}
                  <defs>
                    <radialGradient id="pivotGrad" cx="35%" cy="30%">
                      <stop offset="0%" stopColor="#aaa" />
                      <stop offset="60%" stopColor="#555" />
                      <stop offset="100%" stopColor="#222" />
                    </radialGradient>
                    <linearGradient id="armGrad" x1="0" y1="0" x2="1" y2="0">
                      <stop offset="0%" stopColor="#666" />
                      <stop offset="50%" stopColor="#3a3a3a" />
                      <stop offset="100%" stopColor="#222" />
                    </linearGradient>
                    <linearGradient id="headShellGrad" x1="0" y1="0" x2="1" y2="1">
                      <stop offset="0%" stopColor="#e8e8e8" />
                      <stop offset="35%" stopColor="#aaa" />
                      <stop offset="65%" stopColor="#888" />
                      <stop offset="100%" stopColor="#555" />
                    </linearGradient>
                    <linearGradient id="headTopShine" x1="0" y1="0" x2="1" y2="0">
                      <stop offset="0%" stopColor="rgba(255,255,255,0.1)" />
                      <stop offset="40%" stopColor="rgba(255,255,255,0.45)" />
                      <stop offset="60%" stopColor="rgba(255,255,255,0.45)" />
                      <stop offset="100%" stopColor="rgba(255,255,255,0.05)" />
                    </linearGradient>
                    <linearGradient id="neckGrad" x1="0" y1="0" x2="0" y2="1">
                      <stop offset="0%" stopColor="#666" />
                      <stop offset="100%" stopColor="#333" />
                    </linearGradient>
                  </defs>
                </svg>
              </Box>

              {/* 黑胶碟片底盘 */}
              <Box
                position="absolute"
                top={0}
                left={0}
                w="100%"
                h="100%"
                borderRadius="50%"
                sx={{
                  background:
                    "radial-gradient(circle at center, #1a1a1a 0%, #0a0a0a 100%)",
                  boxShadow:
                    "0 4px 16px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.05)",
                }}
              />
              {/* 碟片同心圆纹理 — 预渲染为伪元素避免重绘 */}
              <Box
                position="absolute"
                top={0}
                left={0}
                w="100%"
                h="100%"
                borderRadius="50%"
                pointerEvents="none"
                sx={{
                  background:
                    "repeating-radial-gradient(circle at center, transparent 0px, transparent 3px, rgba(255,255,255,0.02) 3px, rgba(255,255,255,0.02) 4px)",
                  willChange: "auto",
                }}
              />
              {/* 旋转的封面区域 — 用包裹层居中，内层只做旋转 */}
              <Box
                position="absolute"
                top="50%"
                left="50%"
                sx={{
                  transform: "translate(-50%, -50%)",
                }}
              >
                <Box
                  w={{ base: "150px", md: "200px", lg: "220px" }}
                  h={{ base: "150px", md: "200px", lg: "220px" }}
                  borderRadius="50%"
                  overflow="hidden"
                  sx={{
                    animation: "vinylSpin 12s linear infinite",
                    animationPlayState: isPlaying ? "running" : "paused",
                    boxShadow: "0 0 0 2px rgba(255,255,255,0.08)",
                    willChange: "transform",
                    backfaceVisibility: "hidden",
                  }}
                >
                  <ChakraImage
                    src={coverProxyUrl(currentSong.cover, proxyPort)}
                    alt=""
                    w="100%"
                    h="100%"
                    objectFit="cover"
                    fallback={<Box w="100%" h="100%" bg="gray.700" />}
                  />
                  {/* 封面上的高光反射 */}
                  <Box
                    position="absolute"
                    top={0}
                    left={0}
                    w="100%"
                    h="100%"
                    pointerEvents="none"
                    sx={{
                      background:
                        "linear-gradient(135deg, rgba(255,255,255,0.15) 0%, transparent 40%, transparent 60%, rgba(0,0,0,0.2) 100%)",
                    }}
                  />
                </Box>
              </Box>
            </Box>
          ) : (
            /* 普通模式 */
            <Box
              position="relative"
              borderRadius="2xl"
              overflow="hidden"
              boxShadow="2xl"
              sx={{
                transition: "transform 0.3s ease",
                _hover: { transform: "scale(1.02)" },
              }}
            >
              <ChakraImage
                src={coverProxyUrl(currentSong.cover, proxyPort)}
                alt=""
                w={{ base: "200px", md: "280px", lg: "320px" }}
                h={{ base: "200px", md: "280px", lg: "320px" }}
                objectFit="cover"
                fallback={<Box w="280px" h="280px" bg="gray.700" borderRadius="2xl" />}
              />
            </Box>
          )}

          <VStack spacing={1} align="center" maxW="400px">
            <Text color={effectiveTextColor} fontSize="xl" fontWeight="bold" noOfLines={1} textAlign="center">
              {currentSong.name}
            </Text>
            <Text color={effectiveSubTextColor} fontSize="md" noOfLines={1} textAlign="center">
              {currentSong.artist}
            </Text>
            {currentSong.album && (
              <Text color={effectiveSubTextColor} fontSize="sm" noOfLines={1} textAlign="center">
                {currentSong.album}
              </Text>
            )}
          </VStack>
        </VStack>

        {/* 右侧：歌词 / 评论（本地歌曲仅在存在歌词文件时显示） */}
        {showLyricsPanel && (
        <VStack flex={1} align="stretch" minW={0} h="100%" overflow="hidden" justify="flex-start" spacing={2}>
          {/* Tab 切换 */}
          <HStack spacing={1} flexShrink={0} justify="center">
            <Button
              size="xs"
              variant={lyricsTabActive ? "solid" : "ghost"}
              onClick={() => setRightTab("lyrics")}
              leftIcon={<MusicIcon size={13} />}
              sx={
                lyricsTabActive
                  ? { bg: activeColor, color: "white", _hover: { opacity: 0.85 } }
                  : { color: effectiveSubTextColor, _hover: { bg: effectiveHoverBg } }
              }
              borderRadius="full"
            >
              歌词
            </Button>
            {currentSong.provider === "netease" && (
              <Button
                size="xs"
                variant={rightTab === "comments" ? "solid" : "ghost"}
                onClick={() => setRightTab("comments")}
                leftIcon={<MessageCircle size={13} />}
                sx={
                  rightTab === "comments"
                    ? { bg: activeColor, color: "white", _hover: { opacity: 0.85 } }
                    : { color: effectiveSubTextColor, _hover: { bg: effectiveHoverBg } }
                }
                borderRadius="full"
              >
                评论
              </Button>
            )}
          </HStack>

          {/* 评论 Tab 仅网易云可用；非网易云歌曲（含本地歌）始终渲染歌词面板，
              避免从网易云"评论"Tab 切换歌曲时误渲染评论面板 */}
          {lyricsTabActive ? (
            <KaraokeLyricsView
              lines={karaokeLines}
              loading={loadingLyrics}
              fontSize={lyricsFontSize}
              activeColor={activeColor}
              highlightColor={lyricsHighlightColor}
              textColor={effectiveTextColor}
              subTextColor={effectiveSubTextColor}
              scrollbarSx={memoScrollbarSx}
              audioRef={audioRef}
              isPlaying={isPlaying}
            />
          ) : (
            <CommentPanel
              song={currentSong}
              proxyPort={proxyPort}
              activeColor={activeColor}
              subTextColor={effectiveSubTextColor}
              textColor={effectiveTextColor}
              hoverBg={effectiveHoverBg}
              loginInfo={loginInfo}
              scrollbarSx={memoScrollbarSx}
            />
          )}
        </VStack>
        )}
      </HStack>
      )}

      {/* 样式预览弹层：贴主内容区左侧、flex 垂直居中；宽度随窗口收缩（最多 248px）。
          外层 top:0/bottom:0 拉伸提供明确高度基准，卡片超出时内部滚动——窗口小则弹窗整体缩小，不裁剪 */}
      <Box
        position="absolute"
        top={0}
        bottom={0}
        left={12}
        zIndex={30}
        w="min(248px, calc(100% - 24px))"
        display="flex"
        alignItems="center"
        pointerEvents="none"
        data-style-menu
      >
        <AnimatePresence>
          {styleMenuOpen && (
            <motion.div
              key="style-menu"
              initial={{ opacity: 0, x: -28 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -18 }}
              transition={{ duration: 0.24, ease: "easeOut" }}
              style={{ width: "100%", maxHeight: "100%", display: "flex", flexDirection: "column" }}
            >
              <Box
                bg="rgba(18,18,26,0.92)"
                border="1px solid rgba(255,255,255,0.08)"
                color="white"
                borderRadius="xl"
                p={1.5}
                w="100%"
                maxH="100%"
                overflowY="auto"
                pointerEvents="auto"
                sx={{
                  backdropFilter: "blur(14px)",
                  WebkitBackdropFilter: "blur(14px)",
                  // 隐藏滚动条（仍可滚动），避免视觉干扰
                  scrollbarWidth: "none",
                  "&::-webkit-scrollbar": { display: "none" },
                }}
              >
                <VStack spacing={1.5} align="stretch">
                  {availableStylePreviews.map((s) => {
                    const active = currentStyle === s.key;
                    return (
                      <Box
                        key={s.key}
                        as="button"
                        type="button"
                        aria-label={s.label}
                        onClick={() => {
                          useMusicStore.getState().setExpandedStyle(s.key);
                          setStyleMenuOpen(false);
                        }}
                        cursor="pointer"
                        borderRadius="lg"
                        p="3px"
                        bg={active ? "rgba(255,255,255,0.1)" : "transparent"}
                        transition="background 0.2s ease"
                        _hover={{ bg: "rgba(255,255,255,0.14)" }}
                        sx={{
                          outline: active ? `2px solid ${activeColor}` : "2px solid transparent",
                          outlineOffset: "1px",
                        }}
                      >
                        <ChakraImage
                          src={s.src}
                          alt={s.label}
                          w="100%"
                          aspectRatio={16 / 10}
                          borderRadius="lg"
                          objectFit="cover"
                          display="block"
                          pointerEvents="none"
                        />
                        <Text
                          fontSize="xs"
                          textAlign="center"
                          mt={1}
                          color={active ? activeColor : "whiteAlpha.700"}
                          fontWeight={active ? "semibold" : "normal"}
                        >
                          {s.label}
                        </Text>
                      </Box>
                    );
                  })}
                </VStack>
              </Box>
            </motion.div>
          )}
        </AnimatePresence>
      </Box>
      </Box>

      {/* 底部：播放控制 + 进度条（全宽居中，光标无移动自动隐藏） */}
      <VStack
        spacing={4}
        w="100%"
        flexShrink={0}
        pb={4}
        px={8}
        position="relative"
        zIndex={2}
        opacity={uiHidden ? 0 : 1}
        transform={uiHidden ? "translateY(14px)" : "translateY(0)"}
        pointerEvents={uiHidden ? "none" : "auto"}
        visibility={uiHidden ? "hidden" : "visible"}
        sx={{ transition: "opacity 0.35s ease, transform 0.35s ease, visibility 0.35s ease" }}
      >
        {/* 控制按钮：主按钮居中，红心在左侧，音质在右侧 */}
        <Box position="relative" w="100%">
          {/* 左下方红心 + 样式切换 */}
          <Box position="absolute" left={0} top="50%" transform="translateY(-50%)" zIndex={1} display="flex" alignItems="center" gap={1}>
            {loginInfo?.logged_in && (
              <Tooltip label={isLiked ? "取消红心" : "红心"}>
                <IconButton
                  aria-label="Like"
                  icon={<Heart size={20} fill={isLiked ? "#e53e3e" : "none"} />}
                  size="md"
                  variant="ghost"
                  onClick={() => useMusicStore.getState().toggleLike(currentSong.id)}
                  sx={{
                    color: isLiked ? "#e53e3e" : effectiveTextColor,
                    _hover: { bg: effectiveHoverBg },
                  }}
                />
              </Tooltip>
            )}
            {/* 碟片模式按钮：沉浸/音域回响（全屏场景无封面）、透明彩胶（自带胶片）与封面渐染（左侧已是满铺封面）下隐藏 */}
            {!isImmersive && !isSpectrum && !isVinyl && !isCover && (
            <Tooltip label={coverFilmEffect ? "关闭碟片模式" : "开启碟片模式"}>
              <IconButton
                aria-label="Toggle film effect"
                icon={<Film size={18} />}
                size="sm"
                variant="ghost"
                onClick={() => useMusicStore.getState().setCoverFilmEffect(!coverFilmEffect)}
                sx={{
                  color: coverFilmEffect ? activeColor : effectiveTextColor,
                  _hover: { bg: effectiveHoverBg },
                }}
              />
            </Tooltip>
            )}
            <Box position="relative" display="inline-flex" data-style-menu-trigger>
              <Tooltip label="切换样式" placement="top">
                <Button
                  aria-label="Toggle style"
                  size="sm"
                  variant="ghost"
                  leftIcon={
                    currentStyle === "glass" ? <Droplets size={16} /> :
                    currentStyle === "modern" ? <Palette size={16} /> :
                    currentStyle === "spectrum" ? <AudioWaveform size={16} /> :
                    currentStyle === "vinyl" ? <Disc3 size={16} /> :
                    currentStyle === "cover" ? <ImageIcon size={16} /> :
                    <Waves size={16} />
                  }
                  onClick={toggleStyleMenu}
                  borderRadius="lg"
                  px={3}
                  sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
                >
                  {currentStyle === "glass" ? "通透" : currentStyle === "modern" ? "现代" : currentStyle === "spectrum" ? "音域回响" : currentStyle === "vinyl" ? "透明彩胶" : currentStyle === "cover" ? "封面渐染" : "沉浸"}
                </Button>
              </Tooltip>
              {/* 样式切换按钮右上角红色 NEW 标签 */}
              <Box
                position="absolute"
                top="-6px"
                right="-11px"
                zIndex={3}
                bg="red.500"
                color="white"
                fontSize="9px"
                fontWeight="bold"
                lineHeight="1.4"
                px={1}
                borderRadius="full"
                pointerEvents="none"
                boxShadow="0 1px 3px rgba(0,0,0,0.35)"
              >
                NEW
              </Box>
            </Box>
            {isModern && (
              <HStack spacing={1}>
                <Text fontSize="xs" color={effectiveSubTextColor} fontWeight="medium">动态</Text>
                <Switch
                  size="sm"
                  isChecked={dynamicEnabled}
                  onChange={(e) => useMusicStore.getState().setDynamicEnabled(e.target.checked)}
                  sx={{
                    "& .chakra-switch__track": { bg: "rgba(255,255,255,0.3)" },
                    "& .chakra-switch__track[data-checked]": { bg: `${activeColor} !important` },
                  }}
                />
              </HStack>
            )}
          </Box>
          <HStack spacing={4} justify="center">
          <Tooltip label={playMode === "one" ? "单曲循环" : playMode === "shuffle" ? "随机播放" : playMode === "heartbeat" ? "心动模式" : "列表循环"}>
            <IconButton
              aria-label="Play mode"
              icon={<ModeIcon size={20} />}
              size="md"
              variant="ghost"
              sx={{ color: playMode !== "list" ? activeColor : effectiveSubTextColor, _hover: { bg: effectiveHoverBg } }}
              onClick={() => useMusicStore.getState().togglePlayMode()}
            />
          </Tooltip>
          <Tooltip label={`上一曲 (${musicPrevHotkey})`}>
          <IconButton
            aria-label="Prev"
            icon={<SkipBackBtn size={24} />}
            size="md"
            variant="ghost"
            onClick={() => useMusicStore.getState().prevTrack()}
            sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
          />
          </Tooltip>
          <Tooltip label={`播放/暂停 (${musicPlayPauseHotkey})`}>
          <IconButton
            aria-label="Play/Pause"
            icon={isPlaying ? <PauseIcon size={24} /> : <PlayBtn size={24} />}
            size="md"
            variant="ghost"
            sx={{ color: effectiveTextColor, _hover: { bg: activeColor, color: contrastText } }}
            onClick={() => useMusicStore.getState().togglePlay()}
          />
          </Tooltip>
          <Tooltip label={`下一曲 (${musicNextHotkey})`}>
          <IconButton
            aria-label="Next"
            icon={<SkipForwardBtn size={24} />}
            size="md"
            variant="ghost"
            onClick={() => useMusicStore.getState().nextTrack()}
            sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
          />
          </Tooltip>
          {/* 音量控制：悬停向右展开滑块 */}
          <Box
            role="group"
            position="relative"
            onWheel={handleWheelVolume}
            sx={{
              "&:hover .volume-slider": {
                width: "80px",
                opacity: 1,
                ml: "6px",
              },
            }}
          >
            <IconButton
              aria-label="Mute"
              icon={volume === 0 ? <VolumeX size={20} /> : <Volume2 size={20} />}
              size="md"
              variant="ghost"
              onClick={() => {
                const s = useMusicStore.getState();
                s.setVolume(volume === 0 ? s.prevVolume : 0);
              }}
              sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
            />
            <Box
              className="volume-slider"
              as="input"
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={volume}
              onChange={(e) => useMusicStore.getState().setVolume(parseFloat((e.target as HTMLInputElement).value))}
              tabIndex={-1}
              style={sliderBgStyle(activeColor, volume * 100, sliderTrackBg)}
              sx={{
                ...rangeSliderSx,
                position: "absolute",
                left: "100%",
                top: "50%",
                transform: "translateY(-50%)",
                width: "0px",
                opacity: 0,
                ml: "0px",
                transition: "width 0.25s ease, opacity 0.2s ease, margin-left 0.25s ease",
                cursor: "pointer",
                "&::-webkit-slider-thumb": {
                  ...rangeSliderSx["&::-webkit-slider-thumb"],
                  background: activeColor,
                },
                "&::-moz-range-thumb": {
                  ...rangeSliderSx["&::-moz-range-thumb"],
                  background: activeColor,
                },
                "&::-webkit-slider-runnable-track": {
                  ...rangeSliderSx["&::-webkit-slider-runnable-track"],
                  background: "transparent",
                },
                "&::-moz-range-track": {
                  ...rangeSliderSx["&::-moz-range-track"],
                  background: "transparent",
                },
              }}
            />
          </Box>
        </HStack>

          {/* 歌词高亮颜色 + 歌词字体大小 + 音质选择 - 右侧（沉浸/音域回响模式隐藏歌词高亮/字号） */}
          <Box position="absolute" right={0} top="50%" transform="translateY(-50%)">
            <HStack spacing={2} align="center">
              {!isImmersive && !isSpectrum && (
              <>
              {/* 歌词高亮颜色选择器（透明彩胶下联动胶片颜色：选色即固定为手动色） */}
              <Tooltip label={isVinyl ? "胶片/歌词颜色" : "歌词高亮颜色"}>
                <Box>
                  <CustomColorPicker
                    color={isVinyl ? (vinylColorMode === "custom" ? vinylCustomColor : vinylAccent) : lyricsHighlightColor}
                    onChange={(c) => {
                      if (isVinyl) {
                        const st = useMusicStore.getState();
                        st.setVinylCustomColor(c);
                        st.setVinylColorMode("custom");
                      } else {
                        useMusicStore.getState().setLyricsHighlightColor(c);
                      }
                    }}
                    compact
                  />
                </Box>
              </Tooltip>
              {/* 透明彩胶：手动颜色模式下提供一键切回跟随封面 */}
              {isVinyl && vinylColorMode === "custom" && (
                <Tooltip label="跟随封面颜色">
                  <IconButton
                    aria-label="Follow cover color"
                    icon={<Aperture size={16} />}
                    size="sm"
                    variant="ghost"
                    onClick={() => useMusicStore.getState().setVinylColorMode("auto")}
                    sx={{ color: effectiveSubTextColor, _hover: { bg: effectiveHoverBg, color: effectiveTextColor } }}
                  />
                </Tooltip>
              )}
              <Tooltip label={`歌词字号: ${lyricsFontSize}px`}>
                <HStack spacing={1} align="center">
                  <Text fontSize="xs" color={effectiveSubTextColor} fontWeight="bold" flexShrink={0}>A</Text>
                  <Box
                    as="input"
                    type="range"
                    min={17}
                    max={48}
                    step={1}
                    value={lyricsFontSize}
                    onChange={(e) => useMusicStore.getState().setLyricsFontSize(parseInt((e.target as HTMLInputElement).value))}
                    tabIndex={-1}
                    w="60px"
                    style={sliderBgStyle(activeColor, ((lyricsFontSize - 17) / 31) * 100, sliderTrackBg)}
                    sx={{
                      ...rangeSliderSx,
                      "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: activeColor },
                      "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: activeColor },
                      "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
                      "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
                    }}
                  />
                </HStack>
              </Tooltip>
              </>
              )}
              <Popover placement="top-end" isLazy strategy="fixed">
                <Tooltip label="音质选择">
                  <PopoverTrigger>
                    <IconButton
                      aria-label="Quality"
                      icon={<Box as="span" fontSize="11px" fontWeight="bold">{currentQuality || QUALITY_OPTIONS.find((o) => o.value === playbackQuality)?.label || "高清臻音"}</Box>}
                      size="md"
                      variant="ghost"
                      sx={{ color: activeColor, minW: "auto", px: 2, _hover: { bg: effectiveHoverBg } }}
                    />
                  </PopoverTrigger>
                </Tooltip>
                <Portal>
                  <Fade in>
                    <PopoverContent w="180px" bg={menuBg} border="1px solid" borderColor={menuBorder} borderRadius="lg" boxShadow="lg">
                      <PopoverBody p={1}>
                        {QUALITY_OPTIONS.map((opt) => {
                          const isSvip = loginInfo?.is_svip ?? false;
                          const locked = opt.svip && !isSvip;
                          return (
                            <HStack
                              key={opt.value}
                              spacing={3}
                              px={3}
                              py={1.5}
                              cursor={locked ? "not-allowed" : "pointer"}
                              opacity={locked ? 0.4 : 1}
                              bg={opt.value === playbackQuality ? `${activeColor}22` : "transparent"}
                              _hover={locked ? {} : { bg: "rgba(0,0,0,0.05)" }}
                              borderRadius="md"
                              onClick={() => { if (!locked) useMusicStore.getState().setPlaybackQuality(opt.value as any); }}
                            >
                              <Text fontSize="sm" fontWeight={opt.value === playbackQuality ? "bold" : "normal"} color={menuText}>{opt.label}</Text>
                              <Text fontSize="xs" color={menuMuted}>{opt.desc}</Text>
                            </HStack>
                          );
                        })}
                      </PopoverBody>
                    </PopoverContent>
                  </Fade>
                </Portal>
              </Popover>
            <Tooltip label={desktopLyricsVisible ? "关闭桌面歌词" : "开启桌面歌词"}>
              <IconButton
                aria-label="Desktop lyrics"
                icon={<MonitorSpeaker size={20} />}
                size="md"
                variant="ghost"
                onClick={() => useMusicStore.getState().toggleDesktopLyrics()}
                sx={{
                  color: desktopLyricsVisible ? activeColor : effectiveTextColor,
                  _hover: { bg: effectiveHoverBg },
                  opacity: desktopLyricsVisible ? 1 : 0.7,
                }}
              />
            </Tooltip>
            <Popover placement="top-end" isLazy strategy="fixed">
              <Tooltip label="播放队列">
                <PopoverTrigger>
                  <IconButton
                    aria-label="Queue"
                    icon={<ListMusic size={20} />}
                    size="md"
                    variant="ghost"
                    sx={{ color: effectiveTextColor, _hover: { bg: effectiveHoverBg } }}
                  />
                </PopoverTrigger>
              </Tooltip>
              <Portal>
                <Fade in>
                  <PopoverContent
                    w="260px"
                    bg={menuBg}
                    border="1px solid"
                    borderColor={menuBorder}
                    borderRadius="lg"
                    boxShadow="lg"
                  >
                    <PopoverBody p={1}>
                      {playQueue.length === 0 ? (
                        <Text color={menuMuted} fontSize="sm" px={3} py={2}>播放列表为空</Text>
                      ) : (
                        <VirtualList
                          items={playQueue}
                          itemHeight={48}
                          height={Math.min(384, playQueue.length * 48)}
                          scrollToIndex={currentIndex}
                          getKey={(s, i) => `${s.id}-${i}`}
                          renderItem={(s, i) => (
                            <HStack
                              spacing={2}
                              px={3}
                              h="100%"
                              cursor="pointer"
                              bg={i === currentIndex ? `${activeColor}22` : "transparent"}
                              _hover={{ bg: "rgba(0,0,0,0.05)" }}
                              borderRadius="md"
                              overflow="hidden"
                              onClick={() => useMusicStore.getState().playSong(s, playQueue)}
                            >
                              <Text fontSize="xs" color={(i === currentIndex) ? activeColor : menuMuted} w="20px" flexShrink={0}>
                                {i === currentIndex ? "▶" : i + 1}
                              </Text>
                              <VStack spacing={0} flex={1} minW={0} align="start">
                                <Text
                                  fontSize="sm"
                                  fontWeight={(i === currentIndex) ? "bold" : "normal"}
                                  color={(i === currentIndex) ? activeColor : menuText}
                                  w="100%"
                                  overflow="hidden"
                                  textOverflow="ellipsis"
                                  whiteSpace="nowrap"
                                >
                                  {s.name}
                                </Text>
                                <Text
                                  fontSize="xs"
                                  color={menuMuted}
                                  w="100%"
                                  overflow="hidden"
                                  textOverflow="ellipsis"
                                  whiteSpace="nowrap"
                                >
                                  {s.artist}
                                </Text>
                              </VStack>
                            </HStack>
                          )}
                        />
                      )}
                    </PopoverBody>
                  </PopoverContent>
                </Fade>
              </Portal>
            </Popover>
            </HStack>
          </Box>
        </Box>

        {/* 进度条 — 独立组件，自己管理 timeupdate，不触发 ExpandedPlayer 重渲染 */}
        <ProgressSection
          activeColor={activeColor}
          subTextColor={effectiveSubTextColor}
          sliderTrackBg={sliderTrackBg}
          audioRef={audioRef}
          currentSongId={currentSong.id}
        />
      </VStack>
    </Box>
  );

  // 页面级全屏：将播放器挂载到 body 根部（脱离带 transform 的页面祖先），
  // 确保 fixed 相对整个视口铺满窗口
  return pageFullscreen
    ? createPortal(playerEl, document.body)
    : playerEl;
});


// ═══════════════════════════════════════════════
// 播放时间格式化工具
// ═══════════════════════════════════════════════
const formatTime = (time: number): string => {
  if (isNaN(time)) return "0:00";
  const m = Math.floor(time / 60);
  const s = Math.floor(time % 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
};

// ═══════════════════════════════════════════════
// 播放器进度条 — 独立迷你组件
// 自己管理 timeupdate，播放期间只有本组件随进度重渲染
// PlayerBar 本体不会被 timeupdate 触发重渲染
// ═══════════════════════════════════════════════
const PlayerProgress = memo(function PlayerProgress({
  activeColor,
  subTextColor,
  sliderTrackBg,
  currentSong,
  hidden,
}: {
  activeColor: string;
  subTextColor: string;
  sliderTrackBg: string;
  currentSong: Song | null;
  hidden?: boolean;
}) {
  const audioRef = useMusicStore((s) => s.audioRef);
  const storeDuration = useMusicStore((s) => s.duration);
  const [localCurrentTime, setLocalCurrentTime] = useState(0);
  const [localDuration, setLocalDuration] = useState(0);
  const isUserSeekingRef = useRef(false);
  const pendingSeekRef = useRef(0);

  useEffect(() => {
    setLocalDuration(storeDuration);
  }, [storeDuration]);

  useEffect(() => {
    if (!audioRef || hidden) return;
    if (audioRef.duration && isFinite(audioRef.duration)) {
      setLocalDuration(audioRef.duration);
    }
    if (!isUserSeekingRef.current) {
      setLocalCurrentTime(audioRef.currentTime);
    }
    const onTimeUpdate = () => {
      if (isUserSeekingRef.current) return;
      setLocalCurrentTime(audioRef.currentTime);
    };
    const onLoadedMetadata = () => {
      setLocalDuration(audioRef.duration);
    };
    audioRef.addEventListener("timeupdate", onTimeUpdate);
    audioRef.addEventListener("loadedmetadata", onLoadedMetadata);
    return () => {
      audioRef.removeEventListener("timeupdate", onTimeUpdate);
      audioRef.removeEventListener("loadedmetadata", onLoadedMetadata);
    };
  }, [audioRef, hidden]);

  useEffect(() => {
    setLocalCurrentTime(0);
  }, [currentSong]);

  const handleSeekDrag = useCallback((v: number) => {
    if (!isUserSeekingRef.current) return;
    pendingSeekRef.current = v;
    setLocalCurrentTime(v);
  }, []);

  const handleSeekCommit = useCallback(() => {
    if (pendingSeekRef.current !== 0 || isUserSeekingRef.current) {
      const targetTime = pendingSeekRef.current;
      useMusicStore.getState().seekTo(targetTime);
      setTimeout(() => {
        isUserSeekingRef.current = false;
        setLocalCurrentTime(targetTime);
      }, 300);
    }
  }, []);

  return (
    <HStack spacing={2}>
      <Text color={subTextColor} fontSize="xs" w="40px" textAlign="center">
        {formatTime(localCurrentTime)}
      </Text>
      <Box
        as="input"
        type="range"
        min={0}
        max={localDuration || 100}
        step={0.1}
        value={localCurrentTime}
        onMouseDown={() => { isUserSeekingRef.current = true; }}
        onTouchStart={() => { isUserSeekingRef.current = true; }}
        onChange={(e) => handleSeekDrag(parseFloat((e.target as HTMLInputElement).value))}
        onMouseUp={handleSeekCommit}
        onTouchEnd={handleSeekCommit}
        tabIndex={-1}
        aria-hidden="true"
        flex={1}
        style={sliderBgStyle(activeColor, localDuration ? (localCurrentTime / localDuration) * 100 : 0, sliderTrackBg)}
        sx={{
          ...rangeSliderSx,
          "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: activeColor },
          "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: activeColor },
          "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
          "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
        }}
      />
      <Text color={subTextColor} fontSize="xs" w="40px" textAlign="center">
        {formatTime(localDuration)}
      </Text>
    </HStack>
  );
});

// 关键：用 local state 监听 timeupdate，播放期间不更新 store
// 这样搜索框等组件完全不受播放影响
// ═══════════════════════════════════════════════
const PlayerBar = memo(function PlayerBar({ onExpand, hidden }: { onExpand?: () => void; hidden?: boolean }) {
  const currentSong = useMusicStore((s) => s.currentSong);
  const isPlaying = useMusicStore((s) => s.isPlaying);
  const volume = useMusicStore((s) => s.volume);
  const playMode = useMusicStore((s) => s.playMode);
  const playQueue = useMusicStore((s) => s.playQueue);
  const currentIndex = useMusicStore((s) => s.currentIndex);
  // 滚轮调节音量：上滚 +5%，下滚 -5%，四舍五入到 0.01，夹在 0~1 之间
  const handleWheelVolume = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    if (e.deltaY === 0) return;
    const step = 0.05;
    const cur = useMusicStore.getState().volume;
    const next = Math.round((cur + (e.deltaY < 0 ? step : -step)) * 100) / 100;
    useMusicStore.getState().setVolume(Math.min(1, Math.max(0, next)));
  }, []);
  const proxyPort = useMusicStore((s) => s.proxyPort);
  const playbackQuality = useMusicStore((s) => s.playbackQuality);
  const currentQuality = useMusicStore((s) => s.currentQuality);
  const currentBitrate = useMusicStore((s) => s.currentBitrate);
  const loginInfo = useMusicStore((s) => s.loginInfo);
  const desktopLyricsVisible = useMusicStore((s) => s.desktopLyricsVisible);
  const [dlSettingsOpen, setDlSettingsOpen] = useState(false);

  const { getActiveColor, getHoverColor, getContrastTextColor, getBorderColor } = useThemeColor();

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);

  const borderColor = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const dropdownBg = useColorModeValue("white", "#1a1a1a");
  const sliderTrackBg = useColorModeValue("rgba(255,255,255,0.9)", "#333333");

  const ModeIcon = playMode === "one" ? Repeat1 : playMode === "shuffle" ? Shuffle : playMode === "heartbeat" ? HeartPulse : Repeat;

  if (!currentSong) {
    return (
      <LiquidGlassCard
        p={3}
        flexShrink={0}
        sx={{ marginTop: "auto", position: "relative" }}
      >
        <HStack spacing={4} align="center" justify="center" h="56px">
          <Text color={subTextColor} fontSize="sm">未播放音乐</Text>
        </HStack>
      </LiquidGlassCard>
    );
  }

  return (
    <>
    <LiquidGlassCard
      p={3}
      flexShrink={0}
      cursor={onExpand ? "pointer" : "default"}
      onClick={(e) => {
        const target = e.target as HTMLElement;
        if (!target.closest("button, input, [role='menubutton'], [role='slider']")) {
          onExpand?.();
        }
      }}
      sx={{ marginTop: "auto", position: "relative" }}
    >
      <VStack spacing={2} align="stretch">
        <HStack spacing={4} align="center">
          {/* 左侧：封面 + 标题 */}
          <Box
            as="button"
            onClick={onExpand}
            cursor={onExpand ? "pointer" : "default"}
            borderRadius="md"
            overflow="hidden"
            flexShrink={0}
            sx={{ border: "none", bg: "transparent", p: 0 }}
            _hover={onExpand ? { transform: "scale(1.05)", transition: "transform 0.2s" } : {}}
          >
            <ChakraImage
              src={coverProxyUrl(currentSong.cover, proxyPort)}
              alt=""
              w="48px"
              h="48px"
              borderRadius="md"
              objectFit="cover"
              fallback={<Box w="48px" h="48px" borderRadius="md" bg="gray.700" />}
            />
          </Box>
          <VStack spacing={0} align="start" flexShrink={0} minW={0} maxW="200px">
            <Text color={textColor} fontWeight="medium" fontSize="sm" noOfLines={1}>
              {currentSong.name}
            </Text>
            <Text color={subTextColor} fontSize="xs" noOfLines={1}>
              {currentSong.artist}
            </Text>
          </VStack>

          {/* 中间：播放控制按钮组（绝对居中） */}
          <HStack spacing={1} position="absolute" left="50%" transform="translateX(-50%)">
            <Tooltip label="上一首">
              <IconButton aria-label="Prev" icon={<SkipBackBtn size={18} />} size="sm" variant="ghost" onClick={() => useMusicStore.getState().prevTrack()} />
            </Tooltip>
            <Tooltip label={isPlaying ? "暂停" : "播放"}>
               <IconButton
                aria-label="Play/Pause"
                icon={isPlaying ? <PauseIcon size={18} /> : <PlayBtn size={18} />}
                size="sm"
                variant="ghost"
                sx={{ color: textColor, _hover: { bg: activeColor, color: contrastText } }}
                onClick={() => useMusicStore.getState().togglePlay()}
              />
            </Tooltip>
            <Tooltip label="下一首">
              <IconButton aria-label="Next" icon={<SkipForwardBtn size={18} />} size="sm" variant="ghost" onClick={() => useMusicStore.getState().nextTrack()} />
            </Tooltip>
            <Tooltip label={playMode === "one" ? "单曲循环" : playMode === "shuffle" ? "随机播放" : playMode === "heartbeat" ? "心动模式" : "列表循环"}>
              <IconButton
                aria-label="Play mode"
                icon={<ModeIcon size={16} />}
                size="sm"
                variant="ghost"
                sx={{ color: playMode !== "list" ? activeColor : subTextColor, _hover: { bg: hoverBg } }}
                onClick={() => useMusicStore.getState().togglePlayMode()}
              />
            </Tooltip>
          </HStack>

          {/* 右侧：音质 + 音量 + 播放队列 */}
          <HStack spacing={1} w="180px" ml="auto" onWheel={handleWheelVolume}>
            <Menu>
              <Tooltip label="音质选择">
                <MenuButton
                  as={IconButton}
                  aria-label="Quality"
                  size="sm"
                  variant="ghost"
                  sx={{
                    fontSize: "10px",
                    fontWeight: "bold",
                    color: activeColor,
                    minW: "auto",
                    px: 1,
                    _hover: { bg: hoverBg },
                  }}
                >
                  {currentQuality || QUALITY_OPTIONS.find((o) => o.value === playbackQuality)?.label || "高清臻音"}
                </MenuButton>
              </Tooltip>
              <Portal>
                <MenuList minW="180px" bg={dropdownBg} borderColor={borderColor}>
                  {QUALITY_OPTIONS.map((opt) => {
                    const isSvip = loginInfo?.is_svip ?? false;
                    const locked = opt.svip && !isSvip;
                    return (
                      <MenuItem
                        key={opt.value}
                        onClick={() => {
                          if (!locked) useMusicStore.getState().setPlaybackQuality(opt.value as any);
                        }}
                        bg={opt.value === playbackQuality ? `${activeColor}33` : undefined}
                        opacity={locked ? 0.4 : 1}
                        cursor={locked ? "not-allowed" : "pointer"}
                      >
                        <HStack spacing={3} w="100%" justify="space-between">
                          <Text fontSize="sm" fontWeight={opt.value === playbackQuality ? "bold" : "normal"} color={textColor}>
                            {opt.label}
                          </Text>
                          <Text fontSize="xs" color={subTextColor}>
                            {opt.desc}
                          </Text>
                        </HStack>
                      </MenuItem>
                    );
                  })}
                </MenuList>
              </Portal>
            </Menu>
            <Tooltip label="静音">
              <IconButton
                aria-label="Mute"
                icon={volume === 0 ? <VolumeX size={16} /> : <Volume2 size={16} />}
                size="sm"
                variant="ghost"
                onClick={() => {
                  const s = useMusicStore.getState();
                  s.setVolume(volume === 0 ? s.prevVolume : 0);
                }}
              />
            </Tooltip>
            <Box
            as="input"
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={volume}
            onChange={(e) => useMusicStore.getState().setVolume(parseFloat((e.target as HTMLInputElement).value))}
            tabIndex={-1}
            w="60px"
            style={sliderBgStyle(activeColor, volume * 100, sliderTrackBg)}
            sx={{
              ...rangeSliderSx,
              "&::-webkit-slider-thumb": {
                ...rangeSliderSx["&::-webkit-slider-thumb"],
                background: activeColor,
              },
              "&::-moz-range-thumb": {
                ...rangeSliderSx["&::-moz-range-thumb"],
                background: activeColor,
              },
              "&::-webkit-slider-runnable-track": {
                ...rangeSliderSx["&::-webkit-slider-runnable-track"],
                background: "transparent",
              },
              "&::-moz-range-track": {
                ...rangeSliderSx["&::-moz-range-track"],
                background: "transparent",
              },
            }}
          />
          </HStack>

          {/* 桌面歌词开关 + 设置 */}
          <HStack spacing={1} flexShrink={0}>
            <Tooltip label={desktopLyricsVisible ? "关闭桌面歌词" : "打开桌面歌词"}>
              <IconButton
                aria-label="Desktop Lyrics"
                icon={<MonitorSpeaker size={16} />}
                size="sm"
                variant="ghost"
                sx={{
                  color: desktopLyricsVisible ? activeColor : subTextColor,
                  _hover: { bg: hoverBg },
                }}
                onClick={() => useMusicStore.getState().toggleDesktopLyrics()}
              />
            </Tooltip>
            <Tooltip label="桌面歌词设置">
              <IconButton
                aria-label="Lyrics Settings"
                icon={<Settings size={16} />}
                size="sm"
                variant="ghost"
                sx={{ color: textColor, _hover: { bg: hoverBg } }}
                onClick={() => setDlSettingsOpen(true)}
              />
            </Tooltip>
          </HStack>

          <Menu isLazy>
            {({ onClose }) => (
              <>
                <Tooltip label="播放队列">
                  <MenuButton as={IconButton} aria-label="Queue" icon={<ListMusic size={18} />} size="sm" variant="ghost" />
                </Tooltip>
                <MenuList minW="240px" py={1} bg={dropdownBg} borderColor={borderColor}>
                  {playQueue.length === 0 ? (
                    <Text px={3} py={2} fontSize="sm" color={subTextColor}>播放列表为空</Text>
                  ) : (
                    <VirtualList
                      items={playQueue}
                      itemHeight={48}
                      height={Math.min(288, playQueue.length * 48)}
                      scrollToIndex={currentIndex}
                      getKey={(s, i) => `${s.id}-${i}`}
                      renderItem={(s, i) => (
                        <VStack
                          spacing={0}
                          h="100%"
                          justify="center"
                          align="start"
                          px={3}
                          cursor="pointer"
                          bg={i === currentIndex ? `${activeColor}33` : undefined}
                          _hover={{ bg: hoverBg }}
                          onClick={() => { useMusicStore.getState().playSong(s, playQueue); onClose(); }}
                        >
                          <Text
                            fontSize="sm"
                            fontWeight={i === currentIndex ? "bold" : "normal"}
                            color={i === currentIndex ? activeColor : textColor}
                            w="100%"
                            overflow="hidden"
                            textOverflow="ellipsis"
                            whiteSpace="nowrap"
                          >
                            {i === currentIndex ? "▶" : i + 1}. {s.name}
                          </Text>
                          <Text
                            fontSize="xs"
                            color={subTextColor}
                            w="100%"
                            overflow="hidden"
                            textOverflow="ellipsis"
                            whiteSpace="nowrap"
                          >
                            {s.artist}
                          </Text>
                        </VStack>
                      )}
                    />
                  )}
                </MenuList>
              </>
            )}
          </Menu>
        </HStack>

        {/* 进度条 — 独立迷你组件，播放期间只有本组件随 timeupdate 重渲染 */}
        <PlayerProgress
          activeColor={activeColor}
          subTextColor={subTextColor}
          sliderTrackBg={sliderTrackBg}
          currentSong={currentSong}
          hidden={hidden}
        />
      </VStack>
    </LiquidGlassCard>
    <DesktopLyricsSettingsModal isOpen={dlSettingsOpen} onClose={() => setDlSettingsOpen(false)} />
    </>
    );
  });

// ═══════════════════════════════════════════════
// SongRow — memoized，避免不必要重渲染
// ═══════════════════════════════════════════════
interface SongRowProps {
  song: Song;
  index: number;
  queue: Song[];
  isCurrent: boolean;
  isPlaying: boolean;
  isLiked: boolean;
  isLoggedIn: boolean;
  proxyPort: number;
  activeColor: string;
  hoverBg: string;
  itemHoverBg: string;
  itemActiveBg: string;
  textColor: string;
  subTextColor: string;
  liquidGlassEnabled: boolean;
  onPlay: (song: Song, queue: Song[]) => void;
  onTogglePlay: () => void;
  onToggleLike: (songId: string) => void;
  onArtistClick?: (artist: Artist) => void;
}

const SongRow = memo(function SongRow({
  song,
  index,
  queue,
  isCurrent,
  isPlaying,
  isLiked,
  isLoggedIn,
  proxyPort,
  activeColor,
  hoverBg,
  itemHoverBg,
  itemActiveBg,
  textColor,
  subTextColor,
  liquidGlassEnabled,
  onPlay,
  onTogglePlay,
  onToggleLike,
  onArtistClick,
}: SongRowProps) {
  return (
    <HStack
      key={`${song.provider}-${song.id}-${index}`}
      spacing={3}
      p={2}
      borderRadius="lg"
      cursor="pointer"
      _hover={{ bg: liquidGlassEnabled ? hoverBg : itemHoverBg }}
      bg={isCurrent ? itemActiveBg : "transparent"}
      onClick={() => onPlay(song, queue)}
      transition="background 0.15s"
    >
      <ChakraImage
        src={coverProxyUrl(song.cover, proxyPort)}
        alt=""
        w="40px"
        h="40px"
        borderRadius="md"
        objectFit="cover"
        fallback={<Box w="40px" h="40px" borderRadius="md" bg="gray.700" />}
      />
      <VStack spacing={0} align="start" flex={1} minW={0}>
        <Text color={textColor} fontSize="sm" noOfLines={1} fontWeight={isCurrent ? "bold" : "normal"}>
          {song.name}
        </Text>
        <HStack spacing={1} minW={0}>
          {song.artists.length > 0 && song.artists[0].id && onArtistClick ? (
            <Text
              color={subTextColor}
              fontSize="xs"
              noOfLines={1}
              cursor="pointer"
              _hover={{ color: activeColor, textDecoration: "underline" }}
              onClick={(e) => {
                e.stopPropagation();
                onArtistClick(song.artists[0]);
              }}
            >
              {song.artist}
            </Text>
          ) : (
            <Text color={subTextColor} fontSize="xs" noOfLines={1}>
              {song.artist}
            </Text>
          )}
          {song.album && (
            <Text color={subTextColor} fontSize="xs" noOfLines={1}>
              {" "}- {song.album}
            </Text>
          )}
        </HStack>
      </VStack>
      <Text color={subTextColor} fontSize="xs" flexShrink={0}>
        {formatTime(song.duration / 1000)}
      </Text>
      {/* 语言标签 */}
      {song.language > 0 && song.language <= 4 && (
        <Box
          as="span"
          fontSize="10px"
          color={subTextColor}
          bg={useColorModeValue("gray.100", "rgba(255,255,255,0.08)")}
          px={1.5}
          py={0.5}
          borderRadius="sm"
          flexShrink={0}
          lineHeight="1.2"
        >
          {["", "华语", "日语", "韩语", "欧美"][song.language]}
        </Box>
      )}
      {isLoggedIn && (
        <Tooltip label={isLiked ? "取消红心" : "红心"}>
          <IconButton
            aria-label="Like"
            icon={<Heart size={14} fill={isLiked ? "#e53e3e" : "none"} color={isLiked ? "#e53e3e" : "currentColor"} />}
            size="xs"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              onToggleLike(song.id);
            }}
          />
        </Tooltip>
      )}
      <Tooltip label="播放">
        <IconButton
          aria-label="Play"
          icon={isCurrent && isPlaying ? <PauseIcon size={14} /> : <PlayBtn size={14} />}
          size="xs"
          variant="ghost"
          sx={{ color: activeColor, _hover: { bg: hoverBg } }}
          onClick={(e) => {
            e.stopPropagation();
            if (isCurrent) {
              onTogglePlay();
            } else {
              onPlay(song, queue);
            }
          }}
        />
      </Tooltip>
    </HStack>
  );
});

// ═══════════════════════════════════════════════
// LocalSongRow — 本地导入歌曲行
// ═══════════════════════════════════════════════
interface LocalSongRowProps {
  song: Song;
  queue: Song[];
  isCurrent: boolean;
  isPlaying: boolean;
  proxyPort: number;
  activeColor: string;
  hoverBg: string;
  itemHoverBg: string;
  itemActiveBg: string;
  textColor: string;
  subTextColor: string;
  liquidGlassEnabled: boolean;
  onPlay: (song: Song, queue: Song[]) => void;
  onTogglePlay: () => void;
  onRemove: (id: string) => void;
}

const LocalSongRow = memo(function LocalSongRow({
  song,
  queue,
  isCurrent,
  isPlaying,
  proxyPort,
  activeColor,
  hoverBg,
  itemHoverBg,
  itemActiveBg,
  textColor,
  subTextColor,
  liquidGlassEnabled,
  onPlay,
  onTogglePlay,
  onRemove,
}: LocalSongRowProps) {
  return (
    <HStack
      spacing={3}
      p={2}
      borderRadius="lg"
      cursor="pointer"
      _hover={{ bg: liquidGlassEnabled ? hoverBg : itemHoverBg }}
      bg={isCurrent ? itemActiveBg : "transparent"}
      onClick={() => onPlay(song, queue)}
      transition="background 0.15s"
    >
      <Box
        w="40px"
        h="40px"
        borderRadius="md"
        flexShrink={0}
        position="relative"
        overflow="hidden"
        display="flex"
        alignItems="center"
        justifyContent="center"
        bg={useColorModeValue("gray.200", "rgba(255,255,255,0.08)")}
      >
        {song.cover ? (
          <ChakraImage
            src={song.cover.startsWith("data:") ? song.cover : coverProxyUrl(song.cover, proxyPort)}
            alt=""
            w="100%"
            h="100%"
            objectFit="cover"
            fallback={<FileMusic size={20} color={subTextColor} />}
          />
        ) : (
          <FileMusic size={20} color={subTextColor} />
        )}
      </Box>
      <VStack spacing={0} align="start" flex={1} minW={0}>
        <Text color={textColor} fontSize="sm" noOfLines={1} fontWeight={isCurrent ? "bold" : "normal"}>
          {song.name}
        </Text>
        <HStack spacing={1} minW={0}>
          <Text color={subTextColor} fontSize="xs" noOfLines={1}>
            {song.artist !== "本地音乐" ? song.artist : "本地音乐"}
          </Text>
          {song.album && (
            <Text color={subTextColor} fontSize="xs" noOfLines={1}>
              {" "}- {song.album}
            </Text>
          )}
        </HStack>
      </VStack>
      <Text color={subTextColor} fontSize="xs" flexShrink={0}>
        {song.duration > 0 ? formatTime(song.duration / 1000) : ""}
      </Text>
      <Tooltip label="移除">
        <IconButton
          aria-label="Remove"
          icon={<Trash2 size={14} />}
          size="xs"
          variant="ghost"
          sx={{ color: subTextColor, _hover: { bg: hoverBg, color: "#e53e3e" } }}
          onClick={(e) => {
            e.stopPropagation();
            onRemove(song.id);
          }}
        />
      </Tooltip>
      <Tooltip label="播放">
        <IconButton
          aria-label="Play"
          icon={isCurrent && isPlaying ? <PauseIcon size={14} /> : <PlayBtn size={14} />}
          size="xs"
          variant="ghost"
          sx={{ color: activeColor, _hover: { bg: hoverBg } }}
          onClick={(e) => {
            e.stopPropagation();
            if (isCurrent) {
              onTogglePlay();
            } else {
              onPlay(song, queue);
            }
          }}
        />
      </Tooltip>
    </HStack>
  );
});

// ═══════════════════════════════════════════════
// SearchProviderSwitcher — 搜索结果多平台切换器
// 显示各平台（带登录状态圆点），点击切换搜索来源（歌曲/歌单/歌手）
// ═══════════════════════════════════════════════
const SEARCH_PROVIDERS: { id: MusicProvider; name: string; logo: string }[] = [
  { id: "netease", name: "网易云", logo: "/music-providers/wyy.png" },
  { id: "kugou", name: "酷狗", logo: "/music-providers/kugou.png" },
  { id: "qqmusic", name: "QQ音乐", logo: "/music-providers/qqmusic.png" },
  { id: "migu", name: "咪咕", logo: "/music-providers/migu.webp" },
];

function SearchProviderSwitcher({ onSwitch }: { onSwitch?: (provider: MusicProvider) => void }) {
  const searchProvider = useMusicStore((s) => s.searchProvider);
  const loginInfos = useMusicStore((s) => s.loginInfos);
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const { getActiveColor, getContrastTextColor } = useThemeColor();

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  // 模糊立即生效：页面切换动画期间的 backdrop-filter 关闭由 .page-animating 类统一处理
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;
  const glassTransition = "background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)";
  // 未开启液态玻璃：不透明背景（hover 用不透明色，不透明不透视）
  const plainBg = useColorModeValue("#ffffff", "#1a1a1a");
  const plainHoverBg = useColorModeValue("#f0f0f0", "#2a2a2a");
  const plainBorder = useColorModeValue("gray.200", "#333333");
  // 开启液态玻璃：半透明背景，hover 时提高不透明度（更实，不透明）
  const glassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const glassHoverBg = useColorModeValue("rgba(255,255,255,0.35)", "rgba(0,0,0,0.35)");
  const glassBorder = useColorModeValue("rgba(255,255,255,0.25)", "rgba(255,255,255,0.12)");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#a0a0a0");

  // 已登录平台排在前面，未登录排后面（组内保持原有顺序）
  const sortedProviders = useMemo(() => {
    return [...SEARCH_PROVIDERS].sort((a, b) => {
      const la = loginInfos[a.id]?.logged_in ? 0 : 1;
      const lb = loginInfos[b.id]?.logged_in ? 0 : 1;
      return la - lb;
    });
  }, [loginInfos]);

  return (
    <HStack spacing={2} flexShrink={0} flexWrap="wrap">
      {sortedProviders.map((p) => {
        const active = searchProvider === p.id;
        const loggedIn = !!loginInfos[p.id]?.logged_in;
        // 未登录平台不可搜索（当前激活来源除外，保持可点以便重新搜索）
        const disabled = !loggedIn && !active;
        return (
          <Tooltip
            key={p.id}
            label={
              loggedIn
                ? `${p.name} · 已登录`
                : active
                  ? `${p.name} · 未登录（当前搜索来源）`
                  : `${p.name} · 未登录，登录后可搜索`
            }
          >
            <Button
              size="sm"
              isDisabled={disabled}
              onClick={() => {
                if (disabled) return;
                useMusicStore.getState().setSearchProvider(p.id);
                onSwitch?.(p.id);
              }}
              leftIcon={
                <Box position="relative" display="flex" alignItems="center" flexShrink={0}>
                  <Box
                    w="18px"
                    h="18px"
                    borderRadius="4px"
                    overflow="hidden"
                    flexShrink={0}
                    bg="white"
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                  >
                    <img
                      src={p.logo}
                      alt=""
                      style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
                      draggable={false}
                    />
                  </Box>
                  {/* 登录状态圆点：绿=已登录，灰=未登录 */}
                  <Box
                    position="absolute"
                    right="-3px"
                    bottom="-3px"
                    w="9px"
                    h="9px"
                    borderRadius="full"
                    border="2px solid"
                    borderColor={active ? activeColor : (liquidGlassEnabled ? glassBg : plainBg)}
                    bg={loggedIn ? "#38a169" : "#a0a0a0"}
                  />
                </Box>
              }
              sx={{
                bg: active
                  ? activeColor
                  : (liquidGlassEnabled ? glassBg : plainBg),
                color: active ? contrastText : textColor,
                border: "1px solid",
                borderColor: active ? activeColor : (liquidGlassEnabled ? glassBorder : plainBorder),
                fontWeight: active ? "bold" : "normal",
                backdropFilter: liquidGlassEnabled ? `blur(${effectiveBlur}px) saturate(1.3)` : undefined,
                WebkitBackdropFilter: liquidGlassEnabled ? `blur(${effectiveBlur}px) saturate(1.3)` : undefined,
                transition: glassTransition,
                opacity: disabled ? 0.45 : 1,
                cursor: disabled ? "not-allowed" : "pointer",
                // hover 不透明：未开启玻璃用不透明色；开启玻璃提高不透明度
                _hover: disabled
                  ? {}
                  : active
                    ? { bg: activeColor, filter: "brightness(0.9)" }
                    : { bg: liquidGlassEnabled ? glassHoverBg : plainHoverBg, borderColor: activeColor },
              }}
              borderRadius="lg"
            >
              {p.name}
            </Button>
          </Tooltip>
        );
      })}
      {/* 当前搜索来源提示 */}
      <Text fontSize="xs" color={subTextColor} flexShrink={0}>
        搜索结果来源：{SEARCH_PROVIDERS.find((p) => p.id === searchProvider)?.name}
      </Text>
    </HStack>
  );
}

// ═══════════════════════════════════════════════
// SearchBox — 独立 memo 组件，管理搜索状态
// 不订阅 currentTime/duration，播放时不会重渲染
// ═══════════════════════════════════════════════

// ── 搜索历史（localStorage 持久化，最新在前，去重，最多 10 条）──
const SEARCH_HISTORY_KEY = "music_search_history";
const SEARCH_HISTORY_MAX = 10;
function loadSearchHistory(): string[] {
  try {
    const raw = localStorage.getItem(SEARCH_HISTORY_KEY);
    const arr = raw ? JSON.parse(raw) : [];
    return Array.isArray(arr)
      ? arr.filter((x) => typeof x === "string").slice(0, SEARCH_HISTORY_MAX)
      : [];
  } catch {
    return [];
  }
}
function saveSearchHistory(list: string[]) {
  try {
    localStorage.setItem(SEARCH_HISTORY_KEY, JSON.stringify(list.slice(0, SEARCH_HISTORY_MAX)));
  } catch {
    /* 忽略存储异常 */
  }
}
function addSearchHistory(keyword: string, list: string[]): string[] {
  const v = keyword.trim();
  if (!v) return list;
  return [v, ...list.filter((k) => k !== v)].slice(0, SEARCH_HISTORY_MAX);
}

interface SearchBoxProps {
  onUnifiedSearch: (searchInput: string) => void;
  onArtistClick?: (artist: Artist) => void;
}

const SearchBox = memo(function SearchBox({
  onUnifiedSearch,
  onArtistClick,
}: SearchBoxProps) {
  // ── 非受控 input：用 ref 跟踪值，不使用 value prop ──
  // 这样即使组件重渲染，input 也不会丢失焦点
  const inputRef = useRef<HTMLInputElement>(null);
  const searchInputRef = useRef("");
  const searchBoxRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 订阅 searching + likedSongIds，让爱心状态实时更新；不订阅 currentTime/duration
  const likedSongIds = useMusicStore((s) => s.likedSongIds);
  const searching = useMusicStore((s) => s.searching);
  const searchingArtists = useMusicStore((s) => s.searchingArtists);

  const [showSearchDropdown, setShowSearchDropdown] = useState(false);
  const [dropdownResults, setDropdownResults] = useState<Song[]>([]);
  // 搜索历史：localStorage 持久化；输入为空时聚焦显示
  const [searchHistory, setSearchHistory] = useState<string[]>(loadSearchHistory);
  const [showHistory, setShowHistory] = useState(false);

  // actions 是稳定的
  const storeActionsRef = useRef(useMusicStore.getState());
  const storeActions = storeActionsRef.current;

  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getHoverColor, getContrastTextColor, getBorderColor } = useThemeColor();

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);
  const themeBorder = getBorderColor();

  const bgColor = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const itemHoverBg = useColorModeValue("gray.50", "rgba(255,255,255,0.05)");
  const itemActiveBg = useColorModeValue(`${activeColor}22`, "rgba(255,255,255,0.08)");
  const dropdownBg = useColorModeValue("white", "#1a1a1a");
  const glassInputBg = useColorModeValue("rgba(255,255,255,0.15)", "rgba(0,0,0,0.25)");
  const dropdownBorder = useColorModeValue("gray.200", "#333333");

  // 点击外部关闭搜索下拉/历史
  useEffect(() => {
    const handleClick = (e: MouseEvent) => {
      if (searchBoxRef.current && !searchBoxRef.current.contains(e.target as Node)) {
        setShowSearchDropdown(false);
        setShowHistory(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, []);

  // 卸载时清理 debounce 定时器，防止回调操作已卸载组件
  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, []);

  // ── 搜索：边输入边出预览（debounce 300ms）──
  // 非受控：直接从 input ref 读取值，不触发 setState
  const handleInputChange = useCallback((value: string) => {
    searchInputRef.current = value;
    // 输入清空时显示历史，非空时隐藏历史
    setShowHistory(!value.trim());
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (!value.trim()) {
      setDropdownResults([]);
      setShowSearchDropdown(false);
      return;
    }
    debounceRef.current = setTimeout(async () => {
      await storeActions.search(value);
      const results = useMusicStore.getState().searchResults;
      setDropdownResults(results);
      setShowSearchDropdown(true);
    }, 300);
  }, [storeActions]);

  // ── 统一搜索（回车/搜索按钮/历史项共用）：搜歌曲+歌单+歌手，并记录搜索历史 ──
  const runUnifiedSearch = useCallback((value: string) => {
    const v = value.trim();
    if (!v) return;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    // 记录历史（去重置顶）
    setSearchHistory((prev) => {
      const next = addSearchHistory(v, prev);
      saveSearchHistory(next);
      return next;
    });
    setShowHistory(false);
    Promise.all([
      storeActions.search(v),
      storeActions.searchArtists(v),
      storeActions.searchPlaylists(v),
    ]).then(() => {
      onUnifiedSearch(v);
      setShowSearchDropdown(false);
    });
  }, [storeActions, onUnifiedSearch]);

  // ── 历史交互：点击搜索 / 删除单条 / 清除全部 ──
  const handleHistorySelect = useCallback((kw: string) => {
    searchInputRef.current = kw;
    if (inputRef.current) inputRef.current.value = kw;
    runUnifiedSearch(kw);
  }, [runUnifiedSearch]);

  const removeHistoryItem = useCallback((kw: string) => {
    setSearchHistory((prev) => {
      const next = prev.filter((k) => k !== kw);
      saveSearchHistory(next);
      return next;
    });
  }, []);

  const clearSearchHistory = useCallback(() => {
    setSearchHistory([]);
    saveSearchHistory([]);
  }, []);

  // 为了让 Input 有稳定的 onChange handler（防止失焦），将它提取为 useCallback
  const handleSearchChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    handleInputChange(e.currentTarget.value);
  }, [handleInputChange]);

  // ── 回车：统一搜索，同时搜歌曲、歌单和歌手 ──
  const handleSearchEnter = useCallback(() => {
    runUnifiedSearch(searchInputRef.current);
  }, [runUnifiedSearch]);

  // ── 搜索按钮：统一搜索 ──
  const handleSearchButtonClick = useCallback(() => {
    runUnifiedSearch(searchInputRef.current);
  }, [runUnifiedSearch]);

  // ── 回调函数（稳定引用）──
  const onPlay = useCallback((song: Song, queue: Song[]) => {
    useMusicStore.getState().playSong(song, queue);
  }, []);
  const onTogglePlay = useCallback(() => {
    useMusicStore.getState().togglePlay();
  }, []);
  const onToggleLike = useCallback((songId: string) => {
    useMusicStore.getState().toggleLike(songId);
  }, []);

  // ── 渲染歌曲行 ──
  const renderSongRow = useCallback((song: Song, index: number, queue: Song[]) => {
    const state = useMusicStore.getState();
    return (
      <SongRow
        key={`${song.provider}-${song.id}-${index}`}
        song={song}
        index={index}
        queue={queue}
        isCurrent={state.currentSong?.id === song.id}
        isPlaying={state.isPlaying}
        isLiked={likedSongIds.has(song.id)}
        isLoggedIn={!!state.loginInfo?.logged_in}
        proxyPort={state.proxyPort}
        activeColor={activeColor}
        hoverBg={hoverBg}
        itemHoverBg={itemHoverBg}
        itemActiveBg={itemActiveBg}
        textColor={textColor}
        subTextColor={subTextColor}
        liquidGlassEnabled={liquidGlassEnabled}
        onPlay={onPlay}
        onTogglePlay={onTogglePlay}
        onToggleLike={onToggleLike}
        onArtistClick={onArtistClick}
      />
    );
  }, [likedSongIds, activeColor, hoverBg, itemHoverBg, itemActiveBg, textColor, subTextColor, liquidGlassEnabled, onPlay, onTogglePlay, onToggleLike, onArtistClick]);

  return (
    <Box ref={searchBoxRef} position="relative" flexShrink={0}>
      <motion.div
        initial={{ opacity: 0, y: -6 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.3, ease: "easeOut" }}
        style={{ position: "relative", zIndex: 31 }}
      >
      <HStack spacing={2}>
        <InputGroup size="md">
          <InputLeftElement pointerEvents="none">
            <Search size={16} color={subTextColor} />
          </InputLeftElement>
          {/* 非受控 input：不使用 value prop，用 ref 跟踪值 */}
          {/* 这样即使组件因任何原因重渲染，input 焦点也不会丢失 */}
          <Input
            ref={inputRef}
            placeholder="搜索歌曲和歌手... (回车查看全部)"
            defaultValue=""
            onChange={handleSearchChange}
            onFocus={() => setShowHistory(true)}
            onKeyDown={(e) => e.key === "Enter" && handleSearchEnter()}
            bg={liquidGlassEnabled ? glassInputBg : bgColor}
            borderColor={liquidGlassEnabled ? themeBorder : borderColor}
            borderRadius="xl"
            transition="border-color 0.2s, box-shadow 0.2s"
            _focus={{ borderColor: activeColor, boxShadow: `0 0 0 2px ${activeColor}33, 0 0 0 1px ${activeColor}` }}
          />
          {/* 搜索历史下拉：输入为空且聚焦时显示；宽度=输入框（不延伸至搜索按钮） */}
          <AnimatePresence>
            {showHistory && searchHistory.length > 0 && !searchInputRef.current.trim() && (
              <motion.div
                key="search-history"
                initial={{ opacity: 0, y: -4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -4 }}
                transition={{ duration: 0.15 }}
                style={{
                  padding: "6px",
                  position: "absolute",
                  top: "44px",
                  left: 0,
                  right: 0,
                  zIndex: 30,
                  maxHeight: "280px",
                  overflowY: "auto",
                  background: dropdownBg,
                  borderRadius: "0.5rem",
                  boxShadow: "0 4px 16px rgba(0,0,0,0.2)",
                }}
              >
                <HStack justify="space-between" px={2} py={1}>
                  <Text fontSize="xs" color={subTextColor} fontWeight="semibold">搜索历史</Text>
                  <Button size="xs" variant="ghost" color="red.400" onClick={clearSearchHistory} _hover={{ color: "red.300", bg: "rgba(255,0,0,0.08)" }}>
                    全部清除
                  </Button>
                </HStack>
                <VStack spacing={0} align="stretch">
                  {searchHistory.map((kw, i) => (
                    <HStack
                      key={kw}
                      spacing={1}
                      px={2}
                      py={1.5}
                      borderRadius="md"
                      cursor="pointer"
                      onClick={() => handleHistorySelect(kw)}
                      _hover={{ bg: itemHoverBg }}
                    >
                      <Text fontSize="xs" color={subTextColor} w={5} textAlign="left" flexShrink={0}>{i + 1}.</Text>
                      <Text fontSize="sm" color={textColor} flex={1} noOfLines={1}>{kw}</Text>
                      <IconButton
                        size="xs"
                        variant="ghost"
                        aria-label="删除搜索历史"
                        icon={<X size={12} />}
                        onClick={(e) => {
                          e.stopPropagation();
                          removeHistoryItem(kw);
                        }}
                        _hover={{ bg: "transparent", color: "red.400" }}
                      />
                    </HStack>
                  ))}
                </VStack>
              </motion.div>
            )}
          </AnimatePresence>
        </InputGroup>
        <Button
          leftIcon={<Search size={16} />}
          onClick={handleSearchButtonClick}
          isLoading={searching || searchingArtists}
          size="md"
          borderRadius="xl"
          flexShrink={0}
          sx={{
            bg: activeColor,
            color: contrastText,
            _hover: { bg: activeColor, filter: "brightness(0.9)" },
            _active: { bg: activeColor, filter: "brightness(0.8)" },
          }}
        >
          搜索
        </Button>
      </HStack>
      </motion.div>

      {/* 搜索下拉预览 */}
      <AnimatePresence>
        {showSearchDropdown && dropdownResults.length > 0 && (
          <motion.div
            key="search-dropdown"
            initial="hidden"
            animate="visible"
            exit="exit"
            variants={dropdownVariants}
            style={{
              padding: "8px",
              position: "absolute",
              top: "50px",
              left: 0,
              right: 0,
              zIndex: 30,
              maxHeight: "320px",
              overflowY: "auto",
              background: dropdownBg,
              borderRadius: "0.5rem",
              boxShadow: "0 4px 16px rgba(0,0,0,0.2)",
            }}
          >
            <motion.div
              variants={listContainerVariants}
              initial="hidden"
              animate="visible"
              style={{ display: "flex", flexDirection: "column", gap: "4px" }}
            >
              {dropdownResults.slice(0, 6).map((song, i) => (
                <motion.div key={`${song.provider}-${song.id}-${i}`} variants={listItemVariants}>
                  {renderSongRow(song, i, dropdownResults)}
                </motion.div>
              ))}
              <motion.div variants={listItemVariants}>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    const val = searchInputRef.current;
                    Promise.all([
                      storeActions.search(val),
                      storeActions.searchArtists(val),
                      storeActions.searchPlaylists(val),
                    ]).then(() => {
                      onUnifiedSearch(val);
                      setShowSearchDropdown(false);
                    });
                  }}
                  sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                >
                  查看全部 ({dropdownResults.length})
                </Button>
              </motion.div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </Box>
  );
});

// ═══════════════════════════════════════════════
// Main MusicPage
// ═══════════════════════════════════════════════
export default function MusicPage() {
  // ── 使用独立选择器，每个字段用 Object.is 比较 ──
  // 比 useShallow 更可靠：播放时 timeupdate 只改 currentTime，
  // 这些选择器都不会触发重渲染
  const currentSong = useMusicStore((s) => s.currentSong);
  const isPlaying = useMusicStore((s) => s.isPlaying);
  const searchResults = useMusicStore((s) => s.searchResults);
  const userPlaylists = useMusicStore((s) => s.userPlaylists);
  const localSongs = useMusicStore((s) => s.localSongs);
  const importingLocal = useMusicStore((s) => s.importingLocal);
  const leftPlaylistTracks = useMusicStore((s) => s.leftPlaylistTracks);
  const leftPlaylistMeta = useMusicStore((s) => s.leftPlaylistMeta);
  const rightPlaylistTracks = useMusicStore((s) => s.rightPlaylistTracks);
  const rightPlaylistMeta = useMusicStore((s) => s.rightPlaylistMeta);
  const likedSongIds = useMusicStore((s) => s.likedSongIds);
  const dailyRecommendPlaylists = useMusicStore((s) => s.dailyRecommendPlaylists);
  const recommendSongs = useMusicStore((s) => s.recommendSongs);
  const loginInfo = useMusicStore((s) => s.loginInfo);
  const playbackSource = useMusicStore((s) => s.playbackSource);
  const searchProvider = useMusicStore((s) => s.searchProvider);

  const providerName = useMemo(() => {
    const map: Record<string, string> = {
      netease: "网易云音乐",
      kugou: "酷狗音乐",
      qqmusic: "QQ 音乐",
      migu: "咪咕音乐",
    };
    return map[playbackSource] ?? playbackSource;
  }, [playbackSource]);
  const searching = useMusicStore((s) => s.searching);
  const loadingPlaylists = useMusicStore((s) => s.loadingPlaylists);
  const userPlaylistsError = useMusicStore((s) => s.userPlaylistsError);
  const loadingLeftTracks = useMusicStore((s) => s.loadingLeftTracks);
  const leftPlaylistLoadingAll = useMusicStore((s) => s.leftPlaylistLoadingAll);
  const loadingRightTracks = useMusicStore((s) => s.loadingRightTracks);
  const proxyPort = useMusicStore((s) => s.proxyPort);
  const artistSearchResults = useMusicStore((s) => s.artistSearchResults);
  const artistSongs = useMusicStore((s) => s.artistSongs);
  const selectedArtist = useMusicStore((s) => s.selectedArtist);
  const searchingArtists = useMusicStore((s) => s.searchingArtists);
  const loadingArtistSongs = useMusicStore((s) => s.loadingArtistSongs);
  const artistDetail = useMusicStore((s) => s.artistDetail);
  const artistAlbums = useMusicStore((s) => s.artistAlbums);
  const artistMvs = useMusicStore((s) => s.artistMvs);
  const albumDetailSongs = useMusicStore((s) => s.albumDetailSongs);
  const albumDetailMeta = useMusicStore((s) => s.albumDetailMeta);
  const loadingArtistDetail = useMusicStore((s) => s.loadingArtistDetail);
  const loadingArtistAlbums = useMusicStore((s) => s.loadingArtistAlbums);
  const loadingArtistMvs = useMusicStore((s) => s.loadingArtistMvs);
  const loadingAlbumDetail = useMusicStore((s) => s.loadingAlbumDetail);
  const playlistSearchResults = useMusicStore((s) => s.playlistSearchResults);
  const searchingPlaylists = useMusicStore((s) => s.searchingPlaylists);
  const musicToast = useMusicStore((s) => s.musicToast);

  const toast = useDynamicIsland("music");

  // 监听 musicToast 变化，弹出提示
  useEffect(() => {
    if (musicToast) {
      toast({
        title: musicToast.message,
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      useMusicStore.setState({ musicToast: null });
    }
  }, [musicToast, toast]);

  // ── 内存诊断（临时）：区分 JS 堆 vs 渲染/媒体层，定位 WebView2 进程内存持续涨 ──
  useEffect(() => {
    const mem = (performance as unknown as { memory?: { usedJSHeapSize: number; jsHeapSizeLimit: number } }).memory;
    if (!mem) return;
    const t = setInterval(() => {
      const used = (mem.usedJSHeapSize / 1048576).toFixed(1);
      const limit = (mem.jsHeapSizeLimit / 1048576).toFixed(0);
      console.log(`[mem] JS heap ${used}MB / ${limit}MB`);
    }, 5000);
    return () => clearInterval(t);
  }, []);

  // actions 是稳定的，用 useRef 只获取一次，避免每次渲染重新创建导致 useCallback 失效
  const storeActionsRef = useRef(useMusicStore.getState());
  const storeActions = storeActionsRef.current;

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [viewMode, setViewMode] = useState<"main" | "unifiedSearch" | "fullArtistList" | "artistDetail">("main");
  const [searchInput, setSearchInput] = useState("");
  const [searchTab, setSearchTab] = useState<"songs" | "playlists" | "artists">("songs");
  const previousViewRef = useRef<typeof viewMode>("main");
  const [leftPanelView, setLeftPanelView] = useState<"playlists" | "tracks" | "local">("playlists");
  const [rightPanelView, setRightPanelView] = useState<"recommendations" | "tracks" | "daily">("recommendations");
  // 左侧「我的歌单」曲目视图的歌单内搜索关键词（空 = 不筛选）
  const [playlistKeyword, setPlaylistKeyword] = useState("");
  const [expandedPlayer, setExpandedPlayer] = useState(false);
  const [artistTab, setArtistTab] = useState<"songs" | "albums" | "mvs" | "info">("songs");
  const [expandedAlbum, setExpandedAlbum] = useState<Album | null>(null);
  const [playingMv, setPlayingMv] = useState<Mv | null>(null);
  const [mvUrl, setMvUrl] = useState("");
  const [mvLoading, setMvLoading] = useState(false);
  const mvVideoRef = useRef<HTMLVideoElement | null>(null);
  const mvPlayerRef = useRef<HTMLDivElement | null>(null);
  const mvCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [mvClosing, setMvClosing] = useState(false);
  const [mvIsPlaying, setMvIsPlaying] = useState(false);
  const [mvCurrentTime, setMvCurrentTime] = useState(0);
  const [mvDuration, setMvDuration] = useState(0);
  const [mvVolume, setMvVolume] = useState(() => {
    try {
      const saved = localStorage.getItem("nexbox-mv-volume");
      const num = saved ? parseFloat(saved) : NaN;
      return isNaN(num) ? 1 : Math.min(1, Math.max(0, num));
    } catch {
      return 1;
    }
  });
  // MV 音量条展开状态 — 仅悬停音量按钮/音量条本身时展开，移开后自动收缩
  const [mvVolumeOpen, setMvVolumeOpen] = useState(false);
  const mvVolumeCloseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const openMvVolume = useCallback(() => {
    if (mvVolumeCloseTimerRef.current) {
      clearTimeout(mvVolumeCloseTimerRef.current);
      mvVolumeCloseTimerRef.current = null;
    }
    setMvVolumeOpen(true);
  }, []);

  const scheduleCloseMvVolume = useCallback(() => {
    if (mvVolumeCloseTimerRef.current) {
      clearTimeout(mvVolumeCloseTimerRef.current);
    }
    mvVolumeCloseTimerRef.current = setTimeout(() => {
      setMvVolumeOpen(false);
      mvVolumeCloseTimerRef.current = null;
    }, 120);
  }, []);

  // 卸载时清理定时器
  useEffect(() => {
    return () => {
      if (mvVolumeCloseTimerRef.current) {
        clearTimeout(mvVolumeCloseTimerRef.current);
      }
    };
  }, []);

  // 音量变化时持久化，重进/重开播放器后保持用户设置
  useEffect(() => {
    try {
      localStorage.setItem("nexbox-mv-volume", String(mvVolume));
    } catch {}
    if (mvVideoRef.current) {
      mvVideoRef.current.volume = mvVolume;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mvVolume]);

  const closeMvPlayer = useCallback(() => {
    if (mvClosing) return;
    setMvClosing(true);
    mvCloseTimerRef.current = setTimeout(() => {
      setPlayingMv(null);
      setMvUrl("");
      setMvIsPlaying(false);
      setMvClosing(false);
    }, 300);
  }, [mvClosing]);

  // 搜索结果中展开的歌单
  const [searchExpandedPlaylist, setSearchExpandedPlaylist] = useState<Playlist | null>(null);
  const [searchExpandedTracks, setSearchExpandedTracks] = useState<Song[]>([]);
  const [searchLoadingExpanded, setSearchLoadingExpanded] = useState(false);

  const officialCharts = useMusicStore((s) => s.officialCharts);

  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getHoverColor, getContrastTextColor, getBorderColor } = useThemeColor();

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const hoverBg = getHoverColor(false);

  const borderColor = useColorModeValue("gray.200", "#333333");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const itemHoverBg = useColorModeValue("gray.50", "rgba(255,255,255,0.05)");
  const itemActiveBg = useColorModeValue(`${activeColor}22`, "rgba(255,255,255,0.08)");

  // memoize scrollbarSx，避免每次渲染创建新对象导致子组件不必要重渲染
  const memoScrollbarSx = useMemo(() => scrollbarSx(activeColor), [activeColor]);

  useEffect(() => {
    const storeState = useMusicStore.getState();
    const audio = storeState.audioRef ?? new Audio();
    const isExisting = !!storeState.audioRef;

    // CORS 模式加载音频（createMediaElementSource 频谱依赖）；preload=metadata 防预缓冲整首
    if (!audio.crossOrigin) audio.crossOrigin = "anonymous";
    if (audio.preload !== "metadata") audio.preload = "metadata";

    audioRef.current = audio;
    storeActions.setAudioRef(audio);

    if (!isExisting) {
      audio.addEventListener("ended", () => {
        useMusicStore.getState().nextTrack();
      });

      let recovering = false;
      audio.addEventListener("error", async () => {
        if (recovering) return;
        recovering = true;
        const state = useMusicStore.getState();
        if (state.currentSong && state.isPlaying) {
          const savedTime = audio.currentTime;
          try {
            await state.playSong(state.currentSong);
            audio.currentTime = savedTime;
          } catch {}
        }
        recovering = false;
      });
    }

    const initAndResume = async () => {
      await storeActions.init();
      const state = useMusicStore.getState();
      if (state.currentSong && !isExisting) {
        state.playSong(state.currentSong);
      }
    };
    initAndResume();

    return () => {
      // 离开页面不暂停 — Audio 留在 store 中继续播放
      // 停止桌面歌词时间同步定时器，避免 100ms 间隔的空转
      stopTimeSync();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 自动加载推荐歌单（登录后且无推荐数据时）
  useEffect(() => {
    if (loginInfo?.logged_in && dailyRecommendPlaylists.length === 0) {
      storeActions.loadRecommendations();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loginInfo?.logged_in]);

  const handleBack = useCallback(() => {
    if (viewMode === "artistDetail") {
      setViewMode(previousViewRef.current);
    } else {
      setViewMode("main");
      storeActions.clearArtistState();
    }
  }, [viewMode, storeActions]);

  const handleBackToMain = useCallback(() => {
    setViewMode("main");
    storeActions.clearArtistState();
  }, [storeActions]);

  // 展开播放器时加载歌词
  const handleExpandPlayer = useCallback(() => {
    const song = useMusicStore.getState().currentSong;
    if (song) {
      useMusicStore.getState().loadLyricsForSong(song);
    }
    setExpandedPlayer(true);
  }, []);

  const handleCloseExpandedPlayer = useCallback(() => {
    setExpandedPlayer(false);
  }, []);

  // 灵动岛「打开播放器」按钮：携带 expandPlayer 标记跳转到此页，自动展开全屏播放器
  const location = useLocation();
  useEffect(() => {
    const shouldExpand = (location.state as { expandPlayer?: boolean } | null)?.expandPlayer;
    if (shouldExpand) handleExpandPlayer();
  }, [location.state, handleExpandPlayer]);

  // 统一搜索：进入综合搜索结果页
  const handleUnifiedSearch = useCallback((input: string) => {
    setSearchInput(input);
    setSearchExpandedPlaylist(null);
    setSearchExpandedTracks([]);
    setViewMode("unifiedSearch");
  }, []);

  // 切换搜索平台：换源后立即用当前关键词重新搜索（歌曲/歌单/歌手）
  const handleSearchProviderChange = useCallback((_provider: MusicProvider) => {
    const kw = searchInput;
    if (!kw) return;
    setSearchExpandedPlaylist(null);
    setSearchExpandedTracks([]);
    Promise.all([
      storeActions.search(kw),
      storeActions.searchArtists(kw),
      storeActions.searchPlaylists(kw),
    ]);
  }, [searchInput, storeActions]);

  // 从统一搜索进入全部歌手列表
  const handleShowAllArtists = useCallback(() => {
    setViewMode("fullArtistList");
  }, []);

  // 从全部歌手列表返回统一搜索
  const handleBackToUnifiedSearch = useCallback(() => {
    setViewMode("unifiedSearch");
  }, []);

  // 点击歌手卡片进入歌手详情
  const handleArtistClick = useCallback((artist: Artist) => {
    previousViewRef.current = viewMode;
    const patched = { ...artist };
    useMusicStore.setState({ selectedArtist: patched });
    const artistId = patched.mid || patched.id || "";
    // 歌手来源：优先歌手自带 provider（搜索时已回填），兜底当前搜索平台/播放源
    const artistProvider = (patched.provider as MusicProvider) || searchProvider || playbackSource;
    storeActions.loadArtistSongs(artistId);
    setViewMode("artistDetail");
    // 网易云歌手扩展: 简介/专辑/MV
    if (artistProvider === "netease" && artistId) {
      storeActions.loadArtistDetail(artistId);
      storeActions.loadArtistAlbums(artistId);
      storeActions.loadArtistMvs(artistId);
    }
    // 歌手可能没有头像（从歌曲卡片进入时），按来源异步搜索补齐
    if (!patched.pic_url && patched.name) {
      const cmd =
        artistProvider === "kugou" ? "kugou_artist_search"
          : artistProvider === "qqmusic" ? "qq_artist_search"
          : "music_artist_search";
      invoke<Artist[]>(cmd, { keywords: patched.name, limit: 10 }).then((results) => {
        if (!results?.length) return;
        // 各来源候选项命中方式不同：netease/kugou 用 id，qq 搜索项不带 id 需用 mid；名称精确匹配为辅
        const match =
          results.find((a) =>
            (a.id != null && a.id === patched.id) ||
            (a.mid != null && a.mid === patched.mid) ||
            (patched.name && a.name === patched.name)) ??
          results.find((a) => a.pic_url) ?? // 兜底：取首个带头像的结果
          results[0];
        if (match?.pic_url) {
          useMusicStore.setState({ selectedArtist: { ...patched, pic_url: match.pic_url } });
        }
      }).catch(() => {});
    }
  }, [storeActions, viewMode, playbackSource, searchProvider]);

// ── 我的歌单点击：在左侧面板切换到曲目视图 ──
const handlePlaylistClick = useCallback((pl: Playlist) => {
storeActions.loadLeftPlaylistTracks(pl.id);
setLeftPanelView("tracks");
setRightPanelView("recommendations");
setPlaylistKeyword("");
}, [storeActions]);

  const handleBackToPlaylists = useCallback(() => {
    setLeftPanelView("playlists");
    setPlaylistKeyword("");
  }, []);

  // ── 本地导入歌曲 ──
  const handleImportLocal = useCallback(async () => {
    try {
      const selected = await open({
        multiple: true,
        title: "导入本地歌曲",
        filters: [
          {
            name: "音频文件",
            extensions: ["mp3", "wav", "ogg", "m4a", "flac", "aac", "opus", "wma", "aiff", "ape", "oga"],
          },
        ],
      });
      if (!selected) return;
      const paths = (Array.isArray(selected) ? selected : [selected]).filter((p): p is string => typeof p === "string");
      if (paths.length === 0) return;
      const result = await storeActions.importLocalSongs(paths);
      if (result.count > 0) {
        const noCover = result.noCoverCount > 0
          ? `，${result.noCoverCount} 首未检测到封面`
          : "";
        toast({
          title: "导入成功",
          description: `已导入 ${result.count} 首本地歌曲${noCover}`,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        setLeftPanelView("local");
      } else {
        toast({ title: "导入失败", description: "未找到支持的音频文件", status: "warning", duration: 2500, isClosable: true });
      }
    } catch (e) {
      console.error("Import local music error:", e);
      toast({ title: "导入失败", description: String(e) || "打开文件失败", status: "error", duration: 2500, isClosable: true });
    }
  }, [storeActions, toast]);

  // ── 导入本地歌曲文件夹（递归） ──
  const handleImportLocalFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "导入本地歌曲文件夹",
      });
      if (!selected) return;
      const folder = Array.isArray(selected) ? selected[0] : selected;
      if (!folder) return;
      const result = await storeActions.importLocalFolder(folder);
      if (result.count > 0) {
        const noCover = result.noCoverCount > 0
          ? `，${result.noCoverCount} 首未检测到封面`
          : "";
        toast({
          title: "导入成功",
          description: `已从文件夹导入 ${result.count} 首本地歌曲${noCover}`,
          status: "success",
          duration: 3000,
          isClosable: true,
        });
        setLeftPanelView("local");
      } else {
        toast({ title: "导入失败", description: "该文件夹下未找到支持的音频文件", status: "warning", duration: 2500, isClosable: true });
      }
    } catch (e) {
      console.error("Import local music folder error:", e);
      toast({ title: "导入失败", description: String(e) || "打开文件夹失败", status: "error", duration: 2500, isClosable: true });
    }
  }, [storeActions, toast]);

  // ── 点击本地歌曲：播放 ──
  const handleLocalPlay = useCallback((song: Song, queue: Song[]) => {
    storeActions.playSong(song, queue && queue.length > 0 ? queue : useMusicStore.getState().localSongs);
  }, [storeActions]);

  const handleRemoveLocalSong = useCallback((id: string) => {
    storeActions.removeLocalSong(id);
  }, [storeActions]);

  const handleClearLocalSongs = useCallback(() => {
    storeActions.clearLocalSongs();
  }, [storeActions]);

// ── 推荐歌单点击：在右侧面板切换到曲目视图 ──
const handleRecPlaylistClick = useCallback((pl: Playlist) => {
storeActions.loadRightPlaylistTracks(pl.id);
setRightPanelView("tracks");
}, [storeActions]);

// ── 官方榜单点击：QQ/咪咕榜单走榜单歌曲接口，其他平台走歌单歌曲接口 ──
const handleChartClick = useCallback((pl: Playlist) => {
  useMusicStore.setState({ rightPlaylistMeta: pl });
  setRightPanelView("tracks");
  if (playbackSource === "qqmusic" || playbackSource === "migu") {
    storeActions.loadRightRankTracks(pl.id);
  } else {
    storeActions.loadRightPlaylistTracks(pl.id);
  }
}, [playbackSource, storeActions]);

// 榜单平铺：酷狗/QQ 用网格铺满面板，网易云/咪咕保持横向滚动小卡片
const chartIsGrid = playbackSource === "kugou" || playbackSource === "qqmusic";

  const handleBackToRecommendations = useCallback(() => {
    setRightPanelView("recommendations");
  }, []);

  // ── 每日推荐入口：切换右侧面板到每日推荐歌曲列表 ──
  const handleDailyRecommendClick = useCallback(() => {
    setRightPanelView("daily");
  }, []);

  // ── 收藏/取消收藏歌单 ──
  const handleTogglePlaylistSubscribe = useCallback((pl: Playlist) => {
    useMusicStore.getState().togglePlaylistSubscribe(pl.id, pl.subscribed);
  }, []);

  // ── 搜索结果中点击歌单：在搜索页内加载曲目 ──
  const handleSearchPlaylistClick = useCallback(async (pl: Playlist) => {
    setSearchExpandedPlaylist(pl);
    setSearchExpandedTracks([]);
    setSearchLoadingExpanded(true);
    try {
      const cmd = pl.provider === "kugou" ? "kugou_playlist_tracks"
        : pl.provider === "qqmusic" ? "qq_playlist_tracks"
        : pl.provider === "migu" ? "migu_playlist_tracks"
        : "music_playlist_tracks";
      const result = await invoke<[Playlist, Song[]]>(cmd, { id: pl.id });
      setSearchExpandedTracks(result[1]);
    } catch {
      setSearchExpandedTracks([]);
    } finally {
      setSearchLoadingExpanded(false);
    }
  }, []);

  // 判断是否为用户自建歌单（在我的歌单里且 subscribed=false）
  const isOwnPlaylist = useCallback((pl: Playlist) => {
    return userPlaylists.some((p) => p.id === pl.id && !p.subscribed);
  }, [userPlaylists]);

  // ── 回调函数 ──
  const onPlay = useCallback((song: Song, queue: Song[]) => {
    useMusicStore.getState().playSong(song, queue);
  }, []);
  const onTogglePlay = useCallback(() => {
    useMusicStore.getState().togglePlay();
  }, []);
  const onToggleLike = useCallback((songId: string) => {
    useMusicStore.getState().toggleLike(songId);
  }, []);

  // ── 渲染歌曲行 ──
  const renderSongRow = useCallback((song: Song, index: number, queue: Song[]) => (
    <SongRow
      key={`${song.provider}-${song.id}-${index}`}
      song={song}
      index={index}
      queue={queue}
      isCurrent={currentSong?.id === song.id}
      isPlaying={isPlaying}
      isLiked={likedSongIds.has(song.id)}
      isLoggedIn={!!loginInfo?.logged_in}
      proxyPort={proxyPort}
      activeColor={activeColor}
      hoverBg={hoverBg}
      itemHoverBg={itemHoverBg}
      itemActiveBg={itemActiveBg}
      textColor={textColor}
      subTextColor={subTextColor}
      liquidGlassEnabled={liquidGlassEnabled}
      onPlay={onPlay}
      onTogglePlay={onTogglePlay}
      onToggleLike={onToggleLike}
      onArtistClick={handleArtistClick}
    />
  ), [currentSong, isPlaying, likedSongIds, loginInfo, proxyPort, activeColor, hoverBg, itemHoverBg, itemActiveBg, textColor, subTextColor, liquidGlassEnabled, onPlay, onTogglePlay, onToggleLike, handleArtistClick]);

  // VirtualList renderItem 回调（useCallback 稳定引用，避免 VirtualList memo 失效）
  const renderArtistSongItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, artistSongs),
    [renderSongRow, artistSongs]
  );
  const renderAlbumSongItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, albumDetailSongs),
    [renderSongRow, albumDetailSongs]
  );
  const renderLeftTrackItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, leftPlaylistTracks),
    [renderSongRow, leftPlaylistTracks]
  );

  // 左侧「我的歌单」曲目：歌单内搜索。输入关键词时先把歌单全部曲目拉齐，再按歌名/歌手筛选（忽略大小写）
  const displayedLeftTracks = useMemo(() => {
    const kw = playlistKeyword.trim().toLowerCase();
    if (!kw) return leftPlaylistTracks;
    return leftPlaylistTracks.filter(
      (s) => s.name.toLowerCase().includes(kw) || (s.artist || "").toLowerCase().includes(kw)
    );
  }, [playlistKeyword, leftPlaylistTracks]);

  // 歌单内搜索框输入：更新关键词；若非空且歌单未加载完，则触发一次性加载全部剩余曲目
  const handlePlaylistSearchChange = useCallback((value: string) => {
    setPlaylistKeyword(value);
    const st = useMusicStore.getState();
    const kw = value.trim();
    if (kw && st.leftPlaylistMeta
      && st.leftPlaylistTracks.length < (st.leftPlaylistMeta.track_count ?? 0)
      && !st.leftPlaylistLoadingAll) {
      // fire-and-forget：加载完成后 leftPlaylistTracks 更新会驱动 displayedLeftTracks 重新筛选
      void storeActions.loadAllLeftPlaylistTracks();
    }
  }, [storeActions]);

  const renderFilteredLeftTrackItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, displayedLeftTracks),
    [renderSongRow, displayedLeftTracks]
  );

  const searchingPlaylist = playlistKeyword.trim() !== "";
  const renderRightTrackItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, rightPlaylistTracks),
    [renderSongRow, rightPlaylistTracks]
  );
  const renderDailyTrackItem = useCallback(
    (song: Song, i: number) => renderSongRow(song, i, recommendSongs),
    [renderSongRow, recommendSongs]
  );

  // 本地导入歌曲行渲染
  const renderLocalTrackItem = useCallback(
    (song: Song, _i: number) => (
      <LocalSongRow
        song={song}
        queue={localSongs}
        isCurrent={currentSong?.id === song.id}
        isPlaying={isPlaying}
        proxyPort={proxyPort}
        activeColor={activeColor}
        hoverBg={hoverBg}
        itemHoverBg={itemHoverBg}
        itemActiveBg={itemActiveBg}
        textColor={textColor}
        subTextColor={subTextColor}
        liquidGlassEnabled={liquidGlassEnabled}
        onPlay={handleLocalPlay}
        onTogglePlay={onTogglePlay}
        onRemove={handleRemoveLocalSong}
      />
    ),
    [localSongs, currentSong, isPlaying, proxyPort, activeColor, hoverBg, itemHoverBg, itemActiveBg, textColor, subTextColor, liquidGlassEnabled, handleLocalPlay, onTogglePlay, handleRemoveLocalSong]
  );

  // ── 渲染歌单行（可自定义 onClick）──
  const renderPlaylistRow = (pl: Playlist, prefix?: string, onClick?: (pl: Playlist) => void) => (
    <HStack
      key={`${prefix || ""}${pl.id}`}
      spacing={3}
      p={2}
      borderRadius="lg"
      cursor="pointer"
      _hover={{ bg: liquidGlassEnabled ? hoverBg : itemHoverBg }}
      onClick={() => (onClick || handlePlaylistClick)(pl)}
      transition="background 0.15s"
      bg={leftPlaylistMeta?.id === pl.id || rightPlaylistMeta?.id === pl.id ? itemActiveBg : "transparent"}
    >
      <ChakraImage
        src={coverProxyUrl(pl.cover, proxyPort)}
        alt=""
        w="44px"
        h="44px"
        borderRadius="md"
        objectFit="cover"
        fallback={<Box w="44px" h="44px" borderRadius="md" bg="gray.700" />}
      />
      <VStack spacing={0} align="start" flex={1} minW={0}>
        <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
          {pl.name}
        </Text>
        <Text color={subTextColor} fontSize="xs">
          {[pl.track_count > 0 ? `${pl.track_count} 首` : "", pl.creator ? `· ${pl.creator}` : ""].filter(Boolean).join(" ")}
        </Text>
      </VStack>
      {loginInfo?.logged_in && pl.provider === "netease" && !isOwnPlaylist(pl) && (
        <Tooltip label={pl.subscribed ? "取消收藏" : "收藏歌单"}>
          <IconButton
            aria-label={pl.subscribed ? "取消收藏" : "收藏歌单"}
            icon={<Heart size={16} fill={pl.subscribed ? "#e53e3e" : "none"} />}
            size="sm"
            variant="ghost"
            onClick={(e) => {
              e.stopPropagation();
              handleTogglePlaylistSubscribe(pl);
            }}
            sx={{
              color: pl.subscribed ? "#e53e3e" : subTextColor,
              _hover: { bg: hoverBg },
              flexShrink: 0,
            }}
          />
        </Tooltip>
      )}
    </HStack>
  );

  // ═══════════════════════════════════════════════
  // 歌手详情视图
  // ═══════════════════════════════════════════════
  if (viewMode === "artistDetail") {
    const artistTabBarBg = useColorModeValue("gray.100", "rgba(255,255,255,0.04)");
    const artistTabActiveBg = useColorModeValue("white", "rgba(255,255,255,0.1)");

    const renderAlbumItem = (album: Album) => (
      <HStack
        key={album.id}
        spacing={3}
        p={3}
        borderRadius="lg"
        cursor="pointer"
        _hover={{ bg: itemHoverBg }}
        onClick={() => {
          setExpandedAlbum(album);
          storeActions.loadAlbumDetail(album.id);
        }}
        transition="background 0.15s"
      >
        <ChakraImage
          src={coverProxyUrl(album.cover, proxyPort)}
          alt=""
          w="52px"
          h="52px"
          borderRadius="md"
          objectFit="cover"
          fallback={<Box w="52px" h="52px" borderRadius="md" bg="gray.700" display="flex" alignItems="center" justifyContent="center"><MusicIcon size={20} color={subTextColor} /></Box>}
        />
        <VStack spacing={0} align="start" flex={1} minW={0}>
          <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
            {album.name}
          </Text>
          <Text color={subTextColor} fontSize="xs">
            {album.publish_time ? new Date(album.publish_time).getFullYear() : ""} · {album.song_count} 首
          </Text>
        </VStack>
        <ChevronRight size={16} color={subTextColor} />
      </HStack>
    );

    const handlePlayMv = async (mv: Mv) => {
      setPlayingMv(mv);
      setMvUrl("");
      setMvLoading(true);
      try {
        const url = await invoke<string>("music_mv_url", { mvId: mv.id, resolution: 1080 });
        setMvUrl(url);
      } catch (e) {
        console.error("[Music] load mv url failed:", e);
        toast({ title: "MV 加载失败", description: String(e), status: "error", duration: 3000, isClosable: true });
        setPlayingMv(null);
      } finally {
        setMvLoading(false);
      }
    };

    const renderMvItem = (mv: Mv) => (
      <HStack
        key={mv.id}
        spacing={3}
        p={3}
        borderRadius="lg"
        cursor="pointer"
        _hover={{ bg: itemHoverBg }}
        onClick={() => handlePlayMv(mv)}
        transition="background 0.15s"
      >
        <Box position="relative" w="80px" h="48px" borderRadius="md" overflow="hidden" flexShrink={0}>
          <ChakraImage
            src={coverProxyUrl(mv.cover, proxyPort)}
            alt=""
            w="80px"
            h="48px"
            objectFit="cover"
            fallback={<Box w="80px" h="48px" bg="gray.700" display="flex" alignItems="center" justifyContent="center"><Film size={18} color={subTextColor} /></Box>}
          />
          <Box position="absolute" inset={0} display="flex" alignItems="center" justifyContent="center" bg="rgba(0,0,0,0.3)">
            <Box color="white"><PlayBtn size={18} /></Box>
          </Box>
        </Box>
        <VStack spacing={0} align="start" flex={1} minW={0}>
          <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
            {mv.name}
          </Text>
          <Text color={subTextColor} fontSize="xs">
            {mv.duration ? formatTime(mv.duration / 1000) : ""}
            {mv.play_count > 0 ? ` · ${(mv.play_count / 10000).toFixed(1)}万播放` : ""}
          </Text>
        </VStack>
      </HStack>
    );

    const artistTabs: { key: "songs" | "albums" | "mvs" | "info"; label: string; icon: React.ReactElement; count?: number }[] = [
      { key: "songs", label: "热门歌曲", icon: <MusicIcon size={13} />, count: artistSongs.length },
      ...(playbackSource === "netease"
        ? ([
            { key: "albums", label: "专辑", icon: <Disc3 size={13} />, count: artistAlbums.length },
            { key: "mvs", label: "MV", icon: <Film size={13} />, count: artistMvs.length },
            { key: "info", label: "简介", icon: <Info size={13} /> },
          ] as { key: "albums" | "mvs" | "info"; label: string; icon: React.ReactElement; count?: number }[])
        : []),
    ];

    return (
      <VStack
        spacing={4}
        align="stretch"
        w="100%"
        h="calc(100vh - 120px)"
        overflow="hidden"
        sx={{ maxWidth: "100%", overflowX: "hidden", position: "relative" }}
      >
        <HStack spacing={3} flexShrink={0}>
          <Tooltip label="返回">
            <IconButton
              aria-label="Back"
              icon={<ArrowLeft size={18} />}
              size="sm"
              variant="ghost"
              onClick={handleBack}
              sx={{ color: activeColor, _hover: { bg: hoverBg } }}
            />
          </Tooltip>
          {selectedArtist && (
            <HStack spacing={3}>
              <ChakraImage
                src={coverProxyUrl(selectedArtist.pic_url || "", proxyPort)}
                alt=""
                w="56px"
                h="56px"
                borderRadius="full"
                objectFit="cover"
                fallback={<Box w="56px" h="56px" borderRadius="full" bg="gray.700" />}
              />
              <VStack spacing={0} align="start">
                <Text fontSize="xl" fontWeight="bold" color={textColor}>
                  {selectedArtist.name}
                </Text>
                <Text color={subTextColor} fontSize="sm">
                  {artistSongs.length} 首热门歌曲
                </Text>
              </VStack>
            </HStack>
          )}
          {!selectedArtist && (
            <Text fontSize="lg" fontWeight="bold" color={textColor}>
              歌手详情
            </Text>
          )}
        </HStack>

        <SearchBox
          onUnifiedSearch={handleUnifiedSearch}
          onArtistClick={handleArtistClick}
        />

        {/* Tab 切换 */}
        <HStack spacing={1} flexShrink={0} flexWrap="wrap">
          {artistTabs.map((tab) => (
            <Button
              key={tab.key}
              size="sm"
              onClick={() => setArtistTab(tab.key)}
              leftIcon={tab.icon}
              sx={{
                bg: artistTab === tab.key ? artistTabActiveBg : "transparent",
                color: artistTab === tab.key ? activeColor : subTextColor,
                fontWeight: artistTab === tab.key ? "bold" : "normal",
                _hover: { bg: artistTab === tab.key ? artistTabActiveBg : artistTabBarBg },
              }}
              borderRadius="full"
            >
              {tab.label}
              {tab.count != null && <Text ml={1} fontSize="xs" color={subTextColor}>({tab.count})</Text>}
            </Button>
          ))}
        </HStack>

        <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden">
          {/* 专辑展开的曲目 */}
          {expandedAlbum ? (
            <>
              <HStack spacing={2} mb={3} flexShrink={0}>
                <Tooltip label="返回专辑列表">
                  <IconButton
                    aria-label="Back to albums"
                    icon={<ArrowLeft size={16} />}
                    size="sm"
                    variant="ghost"
                    onClick={() => setExpandedAlbum(null)}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  />
                </Tooltip>
                <VStack spacing={0} align="start" minW={0}>
                  <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
                    {albumDetailMeta?.name || expandedAlbum.name}
                  </Text>
                  <Text color={subTextColor} fontSize="xs">
                    {albumDetailMeta?.song_count || expandedAlbum.song_count} 首
                  </Text>
                </VStack>
              </HStack>
              {loadingAlbumDetail ? (
                <VStack py={10}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
              ) : albumDetailSongs.length > 0 ? (
                <VirtualList
                  items={albumDetailSongs}
                  itemHeight={60}
                  renderItem={renderAlbumSongItem}
                  getKey={(song, i) => `${song.provider}-${song.id}-${i}`}
                  emptyText="暂无歌曲"
                  resetKey={expandedAlbum.id}
                  scrollbarSx={memoScrollbarSx}
                />
              ) : (
                <VStack py={10} spacing={2}>
                  <MusicIcon size={32} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm">暂无歌曲</Text>
                </VStack>
              )}
            </>
          ) : artistTab === "songs" ? (
            loadingArtistSongs ? (
              <VStack py={12}>
                <Spinner size="lg" sx={{ color: activeColor }} />
                <Text color={subTextColor} fontSize="sm">加载中...</Text>
              </VStack>
            ) : artistSongs.length > 0 ? (
              <VirtualList
                items={artistSongs}
                itemHeight={60}
                renderItem={renderArtistSongItem}
                getKey={(song, i) => `${song.provider}-${song.id}-${i}`}
                emptyText="暂无歌曲"
                resetKey={selectedArtist?.id}
                scrollbarSx={memoScrollbarSx}
              />
            ) : (
              <VStack py={12} spacing={2}>
                <MusicIcon size={32} color={subTextColor} />
                <Text color={subTextColor} fontSize="sm">暂无歌曲</Text>
              </VStack>
            )
          ) : artistTab === "albums" ? (
            loadingArtistAlbums && artistAlbums.length === 0 ? (
              <VStack py={12}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
            ) : artistAlbums.length > 0 ? (
              <Box flex={1} overflowY="auto" sx={memoScrollbarSx}>
                <VStack spacing={1} align="stretch">
                  {artistAlbums.map(renderAlbumItem)}
                </VStack>
              </Box>
            ) : (
              <VStack py={12} spacing={2}>
                <Disc3 size={32} color={subTextColor} />
                <Text color={subTextColor} fontSize="sm">暂无专辑</Text>
              </VStack>
            )
          ) : artistTab === "mvs" ? (
            loadingArtistMvs && artistMvs.length === 0 ? (
              <VStack py={12}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
            ) : artistMvs.length > 0 ? (
              <Box flex={1} overflowY="auto" sx={memoScrollbarSx}>
                <VStack spacing={1} align="stretch">
                  {artistMvs.map(renderMvItem)}
                </VStack>
              </Box>
            ) : (
              <VStack py={12} spacing={2}>
                <Film size={32} color={subTextColor} />
                <Text color={subTextColor} fontSize="sm">暂无 MV</Text>
              </VStack>
            )
          ) : (
            // 简介
            <Box flex={1} overflowY="auto" sx={memoScrollbarSx}>
              {loadingArtistDetail ? (
                <VStack py={12}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
              ) : artistDetail?.brief_desc ? (
                <Text color={textColor} fontSize="sm" whiteSpace="pre-wrap" lineHeight="tall">
                  {artistDetail.brief_desc}
                </Text>
              ) : (
                <VStack py={12} spacing={2}>
                  <Info size={32} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm">暂无歌手简介</Text>
                </VStack>
              )}
            </Box>
          )}
        </LiquidGlassCard>

        <PlayerBar onExpand={handleExpandPlayer} hidden={expandedPlayer} />

        {/* 展开的播放器 */}
        {expandedPlayer && <ExpandedPlayer onClose={handleCloseExpandedPlayer} />}

        {/* MV 播放器遮罩 — 自定义控制栏 */}
        {playingMv && (
          <Box
            position="absolute"
            top={0}
            left={0}
            right={0}
            bottom={0}
            zIndex={9998}
            bg="rgba(0,0,0,0.35)"
            display="flex"
            alignItems="center"
            justifyContent="center"
            p={8}
            onClick={closeMvPlayer}
            sx={{
              backdropFilter: "blur(14px)",
              WebkitBackdropFilter: "blur(14px)",
              "@keyframes mvPlayerFadeIn": {
                from: { opacity: 0 },
                to: { opacity: 1 },
              },
              "@keyframes mvPlayerFadeOut": {
                from: { opacity: 1 },
                to: { opacity: 0 },
              },
              animation: mvClosing
                ? "mvPlayerFadeOut 0.3s cubic-bezier(0.4, 0, 0.2, 1) forwards"
                : "mvPlayerFadeIn 0.35s cubic-bezier(0.4, 0, 0.2, 1)",
            }}
          >
            <Box
              w="100%"
              maxW="880px"
              onClick={(e) => e.stopPropagation()}
              sx={{
                "@keyframes mvPlayerSlideUp": {
                  from: { transform: "translateY(40px) scale(0.98)", opacity: 0 },
                  to: { transform: "translateY(0) scale(1)", opacity: 1 },
                },
                "@keyframes mvPlayerSlideDown": {
                  from: { transform: "translateY(0) scale(1)", opacity: 1 },
                  to: { transform: "translateY(40px) scale(0.98)", opacity: 0 },
                },
                animation: mvClosing
                  ? "mvPlayerSlideDown 0.3s cubic-bezier(0.4, 0, 0.2, 1) forwards"
                  : "mvPlayerSlideUp 0.35s cubic-bezier(0.4, 0, 0.2, 1)",
              }}
            >
              <HStack justify="space-between" mb={3}>
                <VStack spacing={0} align="start">
                  <Text color="white" fontSize="lg" fontWeight="bold" noOfLines={1}>
                    {playingMv.name}
                  </Text>
                  <Text color="rgba(255,255,255,0.6)" fontSize="sm">
                    {playingMv.artist_name || selectedArtist?.name || ""} 的 MV
                  </Text>
                </VStack>
                <IconButton
                  aria-label="Close MV"
                  icon={<X size={20} />}
                  size="sm"
                  variant="ghost"
                  color="white"
                  onClick={closeMvPlayer}
                />
              </HStack>
              <Box
                ref={mvPlayerRef}
                role="group"
                position="relative"
                w="100%"
                aspectRatio="16 / 9"
                borderRadius="lg"
                overflow="hidden"
                bg="black"
                boxShadow="2xl"
                sx={{
                  "&:fullscreen": {
                    width: "100%",
                    height: "100%",
                    aspectRatio: "auto",
                    borderRadius: 0,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    background: "#000",
                  },
                }}
              >
                {/* 视频层 */}
                {mvUrl && (
                  <video
                    key={mvUrl}
                    ref={mvVideoRef}
                    src={`http://127.0.0.1:${proxyPort}/audio?url=${encodeURIComponent(mvUrl)}`}
                    autoPlay
                    playsInline
                    style={{ width: "100%", height: "100%", objectFit: "contain", background: "#000" }}
                    onPlay={() => setMvIsPlaying(true)}
                    onPause={() => setMvIsPlaying(false)}
                    onTimeUpdate={(e) => setMvCurrentTime((e.target as HTMLVideoElement).currentTime)}
                    onLoadedMetadata={(e) => {
                      const v = e.target as HTMLVideoElement;
                      setMvDuration(v.duration);
                      // 重新挂载后强制应用用户保存的音量
                      v.volume = mvVolume;
                    }}
                    onEnded={() => setMvIsPlaying(false)}
                    onError={(e) => {
                      console.error("[Music] MV video error:", e);
                      toast({ title: "MV 播放失败", status: "error", duration: 3000, isClosable: true });
                    }}
                  />
                )}
                {mvLoading && (
                  <Box position="absolute" inset={0} display="flex" alignItems="center" justifyContent="center">
                    <Spinner size="lg" sx={{ color: "white" }} />
                  </Box>
                )}

                {/* 点击视频切换播放/暂停 */}
                {!mvLoading && mvUrl && (
                  <Box
                    position="absolute"
                    inset={0}
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    cursor="pointer"
                    onClick={() => {
                      const v = mvVideoRef.current;
                      if (!v) return;
                      if (v.paused) v.play(); else v.pause();
                    }}
                  >
                    {!mvIsPlaying && (
                      <Box
                        w="64px"
                        h="64px"
                        borderRadius="full"
                        bg="rgba(0,0,0,0.6)"
                        display="flex"
                        alignItems="center"
                        justifyContent="center"
                        color="white"
                        transition="transform 0.2s"
                        _hover={{ transform: "scale(1.1)" }}
                      >
                        <PlayBtn size={30} />
                      </Box>
                    )}
                  </Box>
                )}

                {/* 自定义控制栏 — 播放时自动隐藏，悬停视频区域显示 */}
                {!mvLoading && mvUrl && (
                  <HStack
                    position="absolute"
                    left={0}
                    right={0}
                    bottom={0}
                    px={4}
                    py={3}
                    spacing={3}
                    zIndex={1}
                    bg="linear-gradient(to top, rgba(0,0,0,0.85), transparent)"
                    sx={{
                      opacity: mvIsPlaying ? 0 : 1,
                      pointerEvents: mvIsPlaying ? "none" : "auto",
                      transition: "opacity 0.3s",
                      _groupHover: { opacity: 1, pointerEvents: "auto" },
                    }}
                  >
                    <IconButton
                      className="mv-controls"
                      aria-label="Play/Pause"
                      icon={mvIsPlaying ? <PauseIcon size={20} /> : <PlayBtn size={20} />}
                      size="sm"
                      variant="ghost"
                      color="white"
                      onClick={() => {
                        const v = mvVideoRef.current;
                        if (!v) return;
                        if (v.paused) v.play(); else v.pause();
                      }}
                    />
                    <Text color="white" fontSize="xs" flexShrink={0} w="36px" textAlign="center">
                      {formatTime(mvCurrentTime)}
                    </Text>
                    <Box
                      as="input"
                      type="range"
                      min={0}
                      max={mvDuration || 0}
                      step={0.1}
                      value={mvCurrentTime}
                      onChange={(e) => {
                        const v = mvVideoRef.current;
                        if (!v) return;
                        const t = parseFloat((e.target as HTMLInputElement).value);
                        v.currentTime = t;
                        setMvCurrentTime(t);
                      }}
                      tabIndex={-1}
                      flex={1}
                      style={sliderBgStyle("#ffffff", mvDuration ? (mvCurrentTime / mvDuration) * 100 : 0, "rgba(255,255,255,0.3)")}
                      sx={{
                        ...rangeSliderSx,
                        "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: "#ffffff" },
                        "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: "#ffffff" },
                        "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
                        "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
                      }}
                    />
                    <Text color="white" fontSize="xs" flexShrink={0} w="36px" textAlign="center">
                      {formatTime(mvDuration)}
                    </Text>
                    <Box
                      position="relative"
                      display="flex"
                      alignItems="center"
                      onMouseEnter={openMvVolume}
                      onMouseLeave={scheduleCloseMvVolume}
                    >
                      <IconButton
                        aria-label="Mute"
                        icon={mvVolume === 0 ? <VolumeX size={18} /> : <Volume2 size={18} />}
                        size="sm"
                        variant="ghost"
                        color="white"
                        onClick={() => {
                          const v = mvVideoRef.current;
                          if (!v) return;
                          const next = v.volume > 0 ? 0 : 1;
                          v.volume = next;
                          setMvVolume(next);
                        }}
                      />
                      {/* 垂直音量条 — 从按钮正上方弹出，上下调节；仅在悬停音量按钮/音量条时展开，移开自动收缩 */}
                      <Box
                        className="mv-volume-slider"
                        position="absolute"
                        left="50%"
                        bottom="100%"
                        mb={mvVolumeOpen ? "8px" : "0px"}
                        w="90px"
                        h={mvVolumeOpen ? "90px" : "0px"}
                        opacity={mvVolumeOpen ? 1 : 0}
                        pointerEvents={mvVolumeOpen ? "auto" : "none"}
                        onMouseEnter={openMvVolume}
                        onMouseLeave={scheduleCloseMvVolume}
                        sx={{
                          transform: "translateX(-50%)",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          overflow: "hidden",
                          transition: "height 0.25s ease, opacity 0.2s ease, margin-bottom 0.25s ease",
                        }}
                      >
                        <Box
                          as="input"
                          type="range"
                          min={0}
                          max={1}
                          step={0.01}
                          value={mvVolume}
                          onChange={(e) => {
                            const v = mvVideoRef.current;
                            if (!v) return;
                            const vol = parseFloat((e.target as HTMLInputElement).value);
                            v.volume = vol;
                            setMvVolume(vol);
                          }}
                          tabIndex={-1}
                          w="90px"
                          flexShrink={0}
                          style={sliderBgStyle("#ffffff", mvVolume * 100, "rgba(255,255,255,0.3)")}
                          sx={{
                            ...rangeSliderSx,
                            transform: "rotate(-90deg)",
                            cursor: "pointer",
                            "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: "#ffffff" },
                            "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: "#ffffff" },
                            "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
                            "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
                          }}
                        />
                      </Box>
                    </Box>
                    <IconButton
                      aria-label="Fullscreen"
                      icon={<Maximize2 size={18} />}
                      size="sm"
                      variant="ghost"
                      color="white"
                      onClick={() => {
                        const player = mvPlayerRef.current;
                        if (!player) return;
                        if (document.fullscreenElement) {
                          document.exitFullscreen().catch(() => {});
                        } else {
                          player.requestFullscreen?.().catch(() => {});
                        }
                      }}
                    />
                  </HStack>
                )}
              </Box>
            </Box>
          </Box>
        )}
      </VStack>
    );
  }

  // ═══════════════════════════════════════════════
  // 统一搜索结果视图：三标签切换 (单曲 / 歌单 / 歌手)
  // ═══════════════════════════════════════════════
  if (viewMode === "unifiedSearch") {
    const isLoading = searching || searchingArtists || searchingPlaylists;
    const hasAnyResults = searchResults.length > 0 || playlistSearchResults.length > 0 || artistSearchResults.length > 0;

    const tabBarBg = useColorModeValue("gray.100", "rgba(255,255,255,0.04)");
    const tabActiveBg = useColorModeValue("white", "rgba(255,255,255,0.1)");

    const tabs: { key: "songs" | "playlists" | "artists"; label: string; icon: React.ReactNode; count: number }[] = [
      { key: "songs", label: "单曲", icon: <MusicIcon size={14} />, count: searchResults.length },
      { key: "playlists", label: "歌单", icon: <ListMusic size={14} />, count: playlistSearchResults.length },
      { key: "artists", label: "歌手", icon: <User size={14} />, count: artistSearchResults.length },
    ];

    // 展开的歌单曲目视图
    if (searchExpandedPlaylist) {
      return (
        <VStack
          spacing={4}
          align="stretch"
          w="100%"
          h="calc(100vh - 120px)"
          overflow="hidden"
          sx={{ maxWidth: "100%", overflowX: "hidden", position: "relative" }}
        >
          <HStack spacing={3} flexShrink={0}>
            <Tooltip label="返回搜索结果">
              <IconButton
                aria-label="Back to search"
                icon={<ArrowLeft size={18} />}
                size="sm"
                variant="ghost"
                onClick={() => { setSearchExpandedPlaylist(null); setSearchExpandedTracks([]); }}
                sx={{ color: activeColor, _hover: { bg: hoverBg } }}
              />
            </Tooltip>
            <ChakraImage
              src={coverProxyUrl(searchExpandedPlaylist.cover, proxyPort)}
              alt=""
              w="40px"
              h="40px"
              borderRadius="md"
              objectFit="cover"
              fallback={<Box w="40px" h="40px" borderRadius="md" bg="gray.700" />}
            />
            <VStack spacing={0} align="start">
              <Text fontSize="md" fontWeight="bold" color={textColor} noOfLines={1}>
                {searchExpandedPlaylist.name}
              </Text>
              <Text color={subTextColor} fontSize="xs">
                {[searchExpandedPlaylist.track_count > 0 ? `${searchExpandedPlaylist.track_count} 首` : "", searchExpandedPlaylist.creator ? `· ${searchExpandedPlaylist.creator}` : ""].filter(Boolean).join(" ")}
              </Text>
            </VStack>
          </HStack>

          <SearchBox
            onUnifiedSearch={handleUnifiedSearch}
            onArtistClick={handleArtistClick}
          />

          <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden">
            {searchLoadingExpanded ? (
              <VStack py={12}>
                <Spinner size="lg" sx={{ color: activeColor }} />
                <Text color={subTextColor} fontSize="sm">加载曲目中...</Text>
              </VStack>
            ) : searchExpandedTracks.length > 0 ? (
              <Box flex={1} overflowY="scroll" sx={memoScrollbarSx}>
                <VStack spacing={1} align="stretch">
                  {searchExpandedTracks.map((song, i) => renderSongRow(song, i, searchExpandedTracks))}
                </VStack>
              </Box>
            ) : (
              <VStack py={12} spacing={2}>
                <MusicIcon size={32} color={subTextColor} />
                <Text color={subTextColor} fontSize="sm">暂无曲目</Text>
              </VStack>
            )}
          </LiquidGlassCard>

          <PlayerBar onExpand={handleExpandPlayer} hidden={expandedPlayer} />
          {expandedPlayer && <ExpandedPlayer onClose={handleCloseExpandedPlayer} />}
        </VStack>
      );
    }

    return (
      <VStack
        spacing={4}
        align="stretch"
        w="100%"
        h="calc(100vh - 120px)"
        overflow="hidden"
        sx={{ maxWidth: "100%", overflowX: "hidden", position: "relative" }}
      >
        <HStack spacing={3} flexShrink={0}>
          <Tooltip label="返回">
            <IconButton
              aria-label="Back"
              icon={<ArrowLeft size={18} />}
              size="sm"
              variant="ghost"
              onClick={handleBackToMain}
              sx={{ color: activeColor, _hover: { bg: hoverBg } }}
            />
          </Tooltip>
          <Text fontSize="lg" fontWeight="bold" color={textColor}>
            搜索 "{searchInput}"
          </Text>
        </HStack>

        <SearchBox
          onUnifiedSearch={handleUnifiedSearch}
          onArtistClick={handleArtistClick}
        />

        {/* 搜索平台切换：显示各平台登录状态，切换后重新搜索当前关键词 */}
        <SearchProviderSwitcher onSwitch={handleSearchProviderChange} />

        {/* 三标签导航栏 */}
        <HStack
          spacing={1}
          flexShrink={0}
          bg={liquidGlassEnabled ? "rgba(255,255,255,0.08)" : tabBarBg}
          p={1}
          borderRadius="xl"
          sx={
            liquidGlassEnabled
              ? {
                  backdropFilter: "blur(12px)",
                  WebkitBackdropFilter: "blur(12px)",
                  border: "1px solid rgba(255,255,255,0.1)",
                }
              : {}
          }
        >
          {tabs.map((tab) => (
            <Button
              key={tab.key}
              size="sm"
              variant="ghost"
              onClick={() => setSearchTab(tab.key)}
              borderRadius="lg"
              flex={1}
              sx={{
                bg: searchTab === tab.key
                  ? (liquidGlassEnabled ? "rgba(255,255,255,0.2)" : tabActiveBg)
                  : "transparent",
                color: searchTab === tab.key ? activeColor : subTextColor,
                fontWeight: searchTab === tab.key ? "bold" : "normal",
                boxShadow: searchTab === tab.key ? "sm" : "none",
                _hover: { bg: searchTab === tab.key ? undefined : hoverBg },
              }}
            >
              <HStack spacing={1.5}>
                {tab.icon}
                <Text fontSize="sm">{tab.label}</Text>
                {tab.count > 0 && (
                  <Text fontSize="xs" color={searchTab === tab.key ? activeColor : subTextColor}> ({tab.count})</Text>
                )}
              </HStack>
            </Button>
          ))}
        </HStack>

        <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden">
          {isLoading ? (
            <VStack py={12}>
              <Spinner size="lg" sx={{ color: activeColor }} />
              <Text color={subTextColor} fontSize="sm">搜索中...</Text>
            </VStack>
          ) : !hasAnyResults ? (
            <VStack py={12} spacing={2}>
              <Search size={32} color={subTextColor} />
              <Text color={subTextColor} fontSize="sm">没有找到相关内容</Text>
            </VStack>
          ) : (
            <Box flex={1} overflowY="auto" overflowX="hidden" sx={memoScrollbarSx}>
              <AnimatePresence mode="wait">
                <motion.div key={searchTab} variants={tabContentVariants} initial="hidden" animate="visible" exit="exit">
              {/* ── 单曲标签 ── */}
              {searchTab === "songs" && (
                searchResults.length > 0 ? (
                  <motion.div variants={listContainerVariants} initial="hidden" animate="visible" style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                    {searchResults.map((song, i) => (
                      <motion.div key={`${song.provider}-${song.id}-${i}`} variants={listItemVariants}>
                        {renderSongRow(song, i, searchResults)}
                      </motion.div>
                    ))}
                  </motion.div>
                ) : (
                  <VStack py={12} spacing={2}>
                    <MusicIcon size={32} color={subTextColor} />
                    <Text color={subTextColor} fontSize="sm">未找到相关单曲</Text>
                  </VStack>
                )
              )}

              {/* ── 歌单标签 ── */}
              {searchTab === "playlists" && (
                playlistSearchResults.length > 0 ? (
                  <motion.div variants={listContainerVariants} initial="hidden" animate="visible" style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                    {playlistSearchResults.map((pl) => (
                      <motion.div key={`search-pl-${pl.id}`} variants={listItemVariants}>
                        <HStack
                          spacing={3}
                          p={2}
                          borderRadius="lg"
                          cursor="pointer"
                          _hover={{ bg: liquidGlassEnabled ? hoverBg : itemHoverBg }}
                          onClick={() => handleSearchPlaylistClick(pl)}
                          transition="background 0.15s"
                        >
                          <ChakraImage
                            src={coverProxyUrl(pl.cover, proxyPort)}
                            alt=""
                            w="48px"
                            h="48px"
                            borderRadius="md"
                            objectFit="cover"
                            fallback={<Box w="48px" h="48px" borderRadius="md" bg="gray.700" />}
                          />
                          <VStack spacing={0} align="start" flex={1} minW={0}>
                            <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
                              {pl.name}
                            </Text>
                            <Text color={subTextColor} fontSize="xs">
                              {[pl.track_count > 0 ? `${pl.track_count} 首` : "", pl.creator ? `· ${pl.creator}` : ""].filter(Boolean).join(" ")}
                            </Text>
                          </VStack>
                          {loginInfo?.logged_in && pl.provider === "netease" && (
                            <Tooltip label={pl.subscribed ? "取消收藏" : "收藏歌单"}>
                              <IconButton
                                aria-label={pl.subscribed ? "取消收藏" : "收藏歌单"}
                                icon={<Heart size={14} fill={pl.subscribed ? "#e53e3e" : "none"} />}
                                size="xs"
                                variant="ghost"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  handleTogglePlaylistSubscribe(pl);
                                }}
                                sx={{
                                  color: pl.subscribed ? "#e53e3e" : subTextColor,
                                  _hover: { bg: hoverBg },
                                  flexShrink: 0,
                                }}
                              />
                            </Tooltip>
                          )}
                          <Tooltip label="查看曲目">
                            <IconButton
                              aria-label="Play"
                              icon={<PlayBtn size={14} />}
                              size="xs"
                              variant="ghost"
                              sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                              onClick={(e) => {
                                e.stopPropagation();
                                handleSearchPlaylistClick(pl);
                              }}
                            />
                          </Tooltip>
                        </HStack>
                      </motion.div>
                    ))}
                  </motion.div>
                ) : (
                  <VStack py={12} spacing={2}>
                    <ListMusic size={32} color={subTextColor} />
                    <Text color={subTextColor} fontSize="sm">未找到相关歌单</Text>
                  </VStack>
                )
              )}

              {/* ── 歌手标签 ── */}
              {searchTab === "artists" && (
                artistSearchResults.length > 0 ? (
                  <motion.div variants={listContainerVariants} initial="hidden" animate="visible" style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                    <HStack spacing={3} flexWrap="wrap">
                      {artistSearchResults.map((artist) => (
                        <motion.div key={artist.id || artist.name} variants={listItemVariants} style={{ flex: "1 1 calc(50% - 6px)", minWidth: "140px", maxWidth: "calc(50% - 6px)" }}>
                          <Box
                            as="button"
                            p={3}
                            w="100%"
                            borderRadius="lg"
                            cursor="pointer"
                            onClick={() => handleArtistClick(artist)}
                            _hover={{ transform: "scale(1.02)" }}
                            transition="transform 0.15s"
                            bg={liquidGlassEnabled ? "rgba(255,255,255,0.08)" : itemHoverBg}
                            border="1px solid"
                            borderColor="transparent"
                            sx={
                              liquidGlassEnabled
                                ? { backdropFilter: "blur(8px)", WebkitBackdropFilter: "blur(8px)" }
                                : {}
                            }
                          >
                            <VStack spacing={2} align="center">
                              <Box w="60px" h="60px" borderRadius="full" overflow="hidden" flexShrink={0}>
                                <ChakraImage
                                  src={coverProxyUrl(artist.pic_url || "", proxyPort)}
                                  alt=""
                                  w="60px"
                                  h="60px"
                                  objectFit="cover"
                                  fallback={
                                    <Box w="60px" h="60px" bg="gray.700" display="flex" alignItems="center" justifyContent="center">
                                      <User size={28} color={subTextColor} />
                                    </Box>
                                  }
                                />
                              </Box>
                              <VStack spacing={0} w="100%">
                                <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1} textAlign="center">
                                  {artist.name}
                                </Text>
                                <Text color={subTextColor} fontSize="xs">
                                  {artist.music_size != null ? `${artist.music_size} 首` : ""}
                                </Text>
                              </VStack>
                            </VStack>
                          </Box>
                        </motion.div>
                      ))}
                    </HStack>
                    {artistSearchResults.length > 4 && (
                      <HStack justify="center" pt={2}>
                        <Button
                          size="xs"
                          variant="ghost"
                          onClick={handleShowAllArtists}
                          sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                        >
                          查看全部 ({artistSearchResults.length}) →
                        </Button>
                      </HStack>
                    )}
                  </motion.div>
                ) : (
                  <VStack py={12} spacing={2}>
                    <User size={32} color={subTextColor} />
                    <Text color={subTextColor} fontSize="sm">未找到相关歌手</Text>
                  </VStack>
                )
              )}
                </motion.div>
              </AnimatePresence>
            </Box>
          )}
        </LiquidGlassCard>

        <PlayerBar onExpand={handleExpandPlayer} hidden={expandedPlayer} />

        {/* 展开的播放器 */}
        {expandedPlayer && <ExpandedPlayer onClose={handleCloseExpandedPlayer} />}
      </VStack>
    );
  }

  // ═══════════════════════════════════════════════
  // 全部歌手列表视图
  // ═══════════════════════════════════════════════
  if (viewMode === "fullArtistList") {
    return (
      <VStack
        spacing={4}
        align="stretch"
        w="100%"
        h="calc(100vh - 120px)"
        overflow="hidden"
        sx={{ maxWidth: "100%", overflowX: "hidden", position: "relative" }}
      >
        <HStack spacing={3} flexShrink={0}>
          <Tooltip label="返回">
            <IconButton
              aria-label="Back"
              icon={<ArrowLeft size={18} />}
              size="sm"
              variant="ghost"
              onClick={handleBackToUnifiedSearch}
              sx={{ color: activeColor, _hover: { bg: hoverBg } }}
            />
          </Tooltip>
          <Text fontSize="lg" fontWeight="bold" color={textColor}>
            全部歌手 - "{searchInput}"
          </Text>
          <Text color={subTextColor} fontSize="sm">
            ({artistSearchResults.length})
          </Text>
        </HStack>

        <SearchBox
          onUnifiedSearch={handleUnifiedSearch}
          onArtistClick={handleArtistClick}
        />

        <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden">
          <Box flex={1} overflowY="scroll" sx={memoScrollbarSx}>
            <VStack spacing={2} align="stretch">
              {artistSearchResults.map((artist) => (
                <HStack
                  key={artist.id || artist.name}
                  spacing={3}
                  p={3}
                  borderRadius="lg"
                  cursor="pointer"
                  _hover={{ bg: itemHoverBg }}
                  onClick={() => handleArtistClick(artist)}
                  transition="background 0.15s"
                >
                  <Box w="48px" h="48px" borderRadius="md" overflow="hidden" flexShrink={0}>
                    <ChakraImage
                      src={coverProxyUrl(artist.pic_url || "", proxyPort)}
                      alt=""
                      w="48px"
                      h="48px"
                      objectFit="cover"
                      fallback={
                        <Box
                          w="48px"
                          h="48px"
                          bg="gray.700"
                          display="flex"
                          alignItems="center"
                          justifyContent="center"
                        >
                          <User size={20} color={subTextColor} />
                        </Box>
                      }
                    />
                  </Box>
                  <VStack spacing={0} align="start" flex={1} minW={0}>
                    <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
                      {artist.name}
                    </Text>
                    <Text color={subTextColor} fontSize="xs">
                      {artist.music_size != null ? `${artist.music_size} 首歌曲` : "点击查看热门歌曲"}
                    </Text>
                  </VStack>
                  <MicVocal size={16} color={subTextColor} />
                </HStack>
              ))}
            </VStack>
          </Box>
        </LiquidGlassCard>

        <PlayerBar onExpand={handleExpandPlayer} hidden={expandedPlayer} />

        {/* 展开的播放器 */}
        {expandedPlayer && <ExpandedPlayer onClose={handleCloseExpandedPlayer} />}
      </VStack>
    );
  }

  // ═══════════════════════════════════════════════
  // 主视图
  // ═══════════════════════════════════════════════
  return (
    <VStack
      spacing={4}
      align="stretch"
      w="100%"
      h="calc(100vh - 120px)"
      overflow="hidden"
      sx={{ maxWidth: "100%", overflowX: "hidden", position: "relative" }}
    >
      {/* 标题 + 登录 */}
      <HStack justify="space-between" w="100%" flexShrink={0}>
        <HStack spacing={3}>
          <MusicIcon size={24} color={activeColor} />
          <Heading size="md" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            音乐播放器
          </Heading>
          <Box
            px={2.5}
            py={0.5}
            borderRadius="md"
            bg={useColorModeValue("gray.100", "rgba(255,255,255,0.08)")}
            border="1px solid"
            borderColor={useColorModeValue("gray.200", "rgba(255,255,255,0.12)")}
          >
            <Text fontSize="xs" color={subTextColor} fontWeight="medium" whiteSpace="nowrap">
              当前平台：{providerName}
            </Text>
          </Box>
        </HStack>
        <MusicLoginSection />
      </HStack>

      {/* 主内容区：左右 50/50 */}
      <HStack spacing={4} align="stretch" flex={1} w="100%" minH={0} overflow="hidden">
        {/* ══ 左侧：本地音乐 / 我的歌单 / 歌单曲目 ══ */}
        <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden" minW={0}>
          {leftPanelView === "tracks" ? (
            <>
              {/* 歌单曲目视图 */}
              <HStack spacing={2} mb={3} flexShrink={0}>
                <Tooltip label="返回歌单列表">
                  <IconButton
                    aria-label="Back"
                    icon={<ArrowLeft size={16} />}
                    size="sm"
                    variant="ghost"
                    onClick={handleBackToPlaylists}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  />
                </Tooltip>
                <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
                  {leftPlaylistMeta?.name || "曲目列表"}
                </Text>
                <Text color={subTextColor} fontSize="xs" flexShrink={0}>
                  ({displayedLeftTracks.length} 首)
                </Text>
              </HStack>
              {/* 歌单内搜索：固定于列表上方，不随曲目滚动 */}
              <HStack spacing={2} mb={2} flexShrink={0}>
                <InputGroup size="sm">
                  <InputLeftElement pointerEvents="none">
                    <Search size={14} style={{ color: subTextColor }} />
                  </InputLeftElement>
                  <Input
                    placeholder="搜索歌单内歌曲"
                    value={playlistKeyword}
                    onChange={(e) => handlePlaylistSearchChange(e.target.value)}
                    borderRadius="md"
                    focusBorderColor={activeColor}
                    sx={{
                      bg: liquidGlassEnabled ? itemActiveBg : "transparent",
                      borderColor: liquidGlassEnabled ? getBorderColor() : borderColor,
                      fontSize: "sm",
                    }}
                  />
                  {playlistKeyword !== "" && (
                    <InputRightElement width="2rem">
                      <IconButton
                        aria-label="清除歌单内搜索"
                        icon={<X size={14} />}
                        size="xs"
                        variant="ghost"
                        onClick={() => handlePlaylistSearchChange("")}
                        sx={{ color: subTextColor, _hover: { bg: hoverBg } }}
                      />
                    </InputRightElement>
                  )}
                </InputGroup>
                {leftPlaylistLoadingAll && (
                  <HStack spacing={1.5} color={subTextColor} flexShrink={0} pr={1}>
                    <Spinner size="xs" sx={{ color: activeColor }} />
                    <Text fontSize="xs">加载全部曲目中...</Text>
                  </HStack>
                )}
              </HStack>
              <VirtualList
                items={displayedLeftTracks}
                itemHeight={60}
                renderItem={searchingPlaylist ? renderFilteredLeftTrackItem : renderLeftTrackItem}
                getKey={(song, i) => `${song.provider}-${song.id}-${i}`}
                loading={loadingLeftTracks}
                loadingText="加载曲目中..."
                emptyText={searchingPlaylist ? "未找到匹配的歌曲" : "暂无曲目"}
                resetKey={leftPlaylistMeta?.id ? `${leftPlaylistMeta.id}|${playlistKeyword}` : playlistKeyword}
                scrollbarSx={memoScrollbarSx}
                onEndReached={searchingPlaylist ? () => {} : () => storeActions.loadMoreLeftPlaylistTracks()}
                hasMore={searchingPlaylist ? false : (leftPlaylistMeta?.track_count ?? 0) > leftPlaylistTracks.length}
              />
            </>
          ) : leftPanelView === "local" ? (
            <>
              {/* 本地歌曲视图 */}
              <HStack spacing={2} mb={3} flexShrink={0}>
                <Tooltip label="返回歌单列表">
                  <IconButton
                    aria-label="Back"
                    icon={<ArrowLeft size={16} />}
                    size="sm"
                    variant="ghost"
                    onClick={handleBackToPlaylists}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  />
                </Tooltip>
                <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
                  本地音乐
                </Text>
                <Text color={subTextColor} fontSize="xs" flexShrink={0}>
                  ({localSongs.length} 首)
                </Text>
                <Box flex={1} />
                <Tooltip label="导入本地歌曲">
                  <IconButton
                    aria-label="导入本地歌曲"
                    icon={importingLocal ? <Spinner size="xs" /> : <FolderOpen size={15} />}
                    size="sm"
                    variant="ghost"
                    isDisabled={importingLocal}
                    onClick={handleImportLocal}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  />
                </Tooltip>
                <Tooltip label="导入歌曲文件夹">
                  <IconButton
                    aria-label="导入歌曲文件夹"
                    icon={importingLocal ? <Spinner size="xs" /> : <FolderPlus size={15} />}
                    size="sm"
                    variant="ghost"
                    isDisabled={importingLocal}
                    onClick={handleImportLocalFolder}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  />
                </Tooltip>
                {localSongs.length > 0 && (
                  <Tooltip label="清空本地音乐">
                    <IconButton
                      aria-label="清空本地音乐"
                      icon={<Trash2 size={15} />}
                      size="sm"
                      variant="ghost"
                      onClick={handleClearLocalSongs}
                      sx={{ color: subTextColor, _hover: { bg: hoverBg, color: "#e53e3e" } }}
                    />
                  </Tooltip>
                )}
              </HStack>
              <VirtualList
                items={localSongs}
                itemHeight={60}
                renderItem={renderLocalTrackItem}
                getKey={(song, i) => `local-${song.id}-${i}`}
                loading={false}
                loadingText="加载中..."
                emptyText="暂无本地歌曲，点击右上角导入"
                resetKey="local"
                scrollbarSx={memoScrollbarSx}
              />
            </>
          ) : (
            <>
              {/* 顶部：本地音乐入口 + 导入按钮（始终显示） */}
              <HStack spacing={2} mb={2} flexShrink={0}>
                <ListMusic size={16} color={activeColor} />
                <Text fontSize="sm" fontWeight="bold" color={textColor} flex={1} noOfLines={1}>
                  本地音乐
                </Text>
                <Text color={subTextColor} fontSize="xs" flexShrink={0}>
                  {localSongs.length} 首
                </Text>
                <Button
                  size="xs"
                  leftIcon={importingLocal ? <Spinner size="xs" /> : <FolderOpen size={14} />}
                  variant="solid"
                  flexShrink={0}
                  bg={activeColor}
                  color={contrastText}
                  isLoading={importingLocal}
                  loadingText="导入中..."
                  isDisabled={importingLocal}
                  _hover={{ opacity: 0.9 }}
                  _active={{ transform: "scale(0.97)" }}
                  onClick={handleImportLocal}
                >
                  本地导入
                </Button>
                <Button
                  size="xs"
                  leftIcon={importingLocal ? <Spinner size="xs" /> : <FolderPlus size={14} />}
                  variant="outline"
                  flexShrink={0}
                  borderColor={activeColor}
                  color={activeColor}
                  isLoading={importingLocal}
                  loadingText="导入中..."
                  isDisabled={importingLocal}
                  _hover={{ opacity: 0.9 }}
                  _active={{ transform: "scale(0.97)" }}
                  onClick={handleImportLocalFolder}
                >
                  导入文件夹
                </Button>
              </HStack>
              <Box
                p={2}
                borderRadius="lg"
                cursor="pointer"
                mb={3}
                _hover={{ bg: liquidGlassEnabled ? hoverBg : itemHoverBg }}
                onClick={() => setLeftPanelView("local")}
                transition="background 0.15s"
                flexShrink={0}
              >
                <HStack spacing={3}>
                  <Box
                    w="44px"
                    h="44px"
                    borderRadius="md"
                    flexShrink={0}
                    display="flex"
                    alignItems="center"
                    justifyContent="center"
                    bg={useColorModeValue(`${activeColor}1a`, `${activeColor}26`)}
                  >
                    <FileMusic size={22} color={activeColor} />
                  </Box>
                  <VStack spacing={0} align="start" flex={1} minW={0}>
                    <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
                      本地导入的歌单
                    </Text>
                    <Text color={subTextColor} fontSize="xs">
                      {localSongs.length > 0 ? `${localSongs.length} 首歌曲` : "导入本地音频文件"}
                    </Text>
                  </VStack>
                  <ChevronRight size={16} color={subTextColor} />
                </HStack>
              </Box>

              {/* 登录后的歌单列表 */}
              {loginInfo?.logged_in ? (
                <>
                  <Text fontSize="sm" fontWeight="bold" color={textColor} mb={2} flexShrink={0}>
                    我的歌单
                  </Text>
                  <Box flex={1} overflowY="auto" sx={memoScrollbarSx}>
                    {loadingPlaylists ? (
                      <VStack py={6}><Spinner size="sm" sx={{ color: activeColor }} /></VStack>
                    ) : userPlaylists.length > 0 ? (
                      <motion.div variants={listContainerVariants} initial="hidden" animate="visible" style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                        {userPlaylists.map((pl) => (
                          <motion.div key={pl.id} variants={listItemVariants}>{renderPlaylistRow(pl)}</motion.div>
                        ))}
                      </motion.div>
                    ) : (
                      <VStack py={4} spacing={2}>
                        <Text color={subTextColor} fontSize="xs" textAlign="center">
                          {userPlaylistsError ? "歌单获取失败" : "暂无歌单"}
                        </Text>
                        {userPlaylistsError ? (
                          <>
                            <Text color={subTextColor} fontSize="2xs" textAlign="center" wordBreak="break-all" px={2}>
                              {userPlaylistsError}
                            </Text>
                            <Button
                              size="xs"
                              variant="ghost"
                              color={activeColor}
                              onClick={() => useMusicStore.getState().openLoginWindow(playbackSource)}
                              alignSelf="center"
                            >
                              重新登录
                            </Button>
                          </>
                        ) : null}
                      </VStack>
                    )}
                  </Box>
                </>
              ) : (
                <VStack flex={1} spacing={3} justify="center">
                  <MusicIcon size={32} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm" textAlign="center">登录后查看云端歌单</Text>
                </VStack>
              )}
            </>
          )}
        </LiquidGlassCard>

        {/* ══ 右侧：搜索 + 推荐 ══ */}
        <VStack spacing={4} align="stretch" flex={1} minW={0} overflow="hidden">
          {/* 搜索框 — 独立 memo 组件，播放时不会因重渲染而失焦 */}
          <SearchBox
            onUnifiedSearch={handleUnifiedSearch}
            onArtistClick={handleArtistClick}
          />

          {/* 推荐歌单 / 推荐歌单曲目 */}
          <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" overflow="hidden">
            {rightPanelView === "daily" ? (
              <>
                {/* 每日推荐曲目视图 */}
                <HStack spacing={2} mb={3} flexShrink={0}>
                  <Tooltip label="返回推荐歌单">
                    <IconButton
                      aria-label="Back"
                      icon={<ArrowLeft size={16} />}
                      size="sm"
                      variant="ghost"
                      onClick={handleBackToRecommendations}
                      sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                    />
                  </Tooltip>
                  <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
                    每日推荐
                  </Text>
                  <Text color={subTextColor} fontSize="xs" flexShrink={0}>
                    ({recommendSongs.length} 首)
                  </Text>
                </HStack>
                <VirtualList
                  items={recommendSongs}
                  itemHeight={60}
                  renderItem={renderDailyTrackItem}
                  getKey={(song, i) => `${song.provider}-${song.id}-${i}`}
                  loading={false}
                  loadingText="加载推荐中..."
                  emptyText="每日推荐无法获取（请确认已登录网易云）"
                  resetKey="daily"
                  scrollbarSx={memoScrollbarSx}
                  hasMore={false}
                />
              </>
            ) : rightPanelView === "tracks" ? (
              <>
                {/* 推荐歌单曲目视图 */}
                <HStack spacing={2} mb={3} flexShrink={0}>
                  <Tooltip label="返回推荐歌单">
                    <IconButton
                      aria-label="Back"
                      icon={<ArrowLeft size={16} />}
                      size="sm"
                      variant="ghost"
                      onClick={handleBackToRecommendations}
                      sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                    />
                  </Tooltip>
                  <Text fontSize="sm" fontWeight="bold" color={textColor} noOfLines={1}>
                    {rightPlaylistMeta?.name || "曲目列表"}
                  </Text>
                  <Text color={subTextColor} fontSize="xs" flexShrink={0}>
                    ({rightPlaylistTracks.length} 首)
                  </Text>
                </HStack>
                <VirtualList
                  items={rightPlaylistTracks}
                  itemHeight={60}
                  renderItem={renderRightTrackItem}
                  getKey={(song, i) => `${song.provider}-${song.id}-${i}`}
                  loading={loadingRightTracks}
                  loadingText="加载曲目中..."
                  emptyText="暂无曲目"
                  resetKey={rightPlaylistMeta?.id}
                  scrollbarSx={memoScrollbarSx}
                  onEndReached={() => storeActions.loadMoreRightPlaylistTracks()}
                  hasMore={(rightPlaylistMeta?.track_count ?? 0) > rightPlaylistTracks.length}
                />
              </>
            ) : (
              <>
                {loginInfo?.logged_in && playbackSource !== "kugou" && playbackSource !== "qqmusic" && (
                  <>
                <HStack justify="space-between" mb={3} flexShrink={0}>
                  <HStack spacing={2}>
                    <Sparkles size={16} color={activeColor} />
                    <Text fontSize="sm" fontWeight="bold" color={textColor}>
                      推荐歌单
                    </Text>
                  </HStack>
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => storeActions.loadRecommendations()}
                    sx={{ color: activeColor, _hover: { bg: hoverBg } }}
                  >
                    刷新
                  </Button>
                </HStack>
                  </>
                )}

                {/* 官方榜单 */}
                {loginInfo?.logged_in && (
                <VStack spacing={2} align="stretch" mb={2} flexShrink={chartIsGrid ? undefined : 0} flex={chartIsGrid ? 1 : undefined} overflowY={chartIsGrid ? "auto" : undefined} sx={chartIsGrid ? memoScrollbarSx : undefined}>
                  <HStack spacing={1.5}>
                    <TrendingUp size={13} color={activeColor} />
                    <Text fontSize="2xs" fontWeight="bold" color={subTextColor}>官方榜单</Text>
                  </HStack>
                  {chartIsGrid ? (
                    /* 酷狗/QQ: 网格布局铺满面板 */
                    <Box>
                      <SimpleGrid columns={3} spacing={2}>
                        {officialCharts.length > 0 ? officialCharts.map((chart) => (
                          <VStack
                            key={chart.id}
                            spacing={0.5}
                            cursor="pointer"
                            onClick={() => playbackSource === "qqmusic" ? handleChartClick(chart) : handleRecPlaylistClick(chart)}
                            _hover={{ transform: "scale(1.04)" }}
                            transition="transform 0.15s"
                          >
                            <Box
                              w="100%"
                              borderRadius="lg"
                              overflow="hidden"
                              sx={{ aspectRatio: "1 / 1" }}
                            >
                              <ChakraImage
                                src={coverProxyUrl(chart.cover, proxyPort)}
                                alt=""
                                w="100%"
                                h="100%"
                                objectFit="cover"
                                fallback={
                                  <Box w="100%" h="100%" bg="gray.700" display="flex" alignItems="center" justifyContent="center">
                                    <TrendingUp size={14} color={subTextColor} />
                                  </Box>
                                }
                              />
                            </Box>
                            <Text color={textColor} fontSize="xs" fontWeight="medium" noOfLines={1} textAlign="center" w="100%">
                              {chart.name}
                            </Text>
                          </VStack>
                        )) : (
                          <>
                            {[0, 1, 2, 3, 4, 5].map((i) => (
                              <Box key={i} borderRadius="lg" bg={itemHoverBg} sx={{ aspectRatio: "1 / 1" }} />
                            ))}
                          </>
                        )}
                      </SimpleGrid>
                    </Box>
                  ) : (
                    /* 网易云: 横向滚动 */
                    <HStack spacing={1.5} minW={0} overflowX="auto" sx={memoScrollbarSx}
                      onWheel={(e) => {
                        e.currentTarget.scrollLeft += e.deltaY;
                      }}
                    >
                    {/* 每日推荐入口（网易云） */}
                    {playbackSource === "netease" && (
                      <VStack
                        spacing={0.5}
                        cursor="pointer"
                        minW="60px"
                        maxW="70px"
                        flexShrink={0}
                        onClick={handleDailyRecommendClick}
                        _hover={{ transform: "scale(1.04)" }}
                        transition="transform 0.15s"
                      >
                        <Box
                          w="100%"
                          borderRadius="lg"
                          overflow="hidden"
                          sx={{ aspectRatio: "1 / 1" }}
                        >
                          <Box
                            w="100%"
                            h="100%"
                            bgGradient="linear(135deg, #f6b26b 0%, #e06666 100%)"
                            display="flex"
                            alignItems="center"
                            justifyContent="center"
                          >
                            <Sparkles size={16} color="#fff" />
                          </Box>
                        </Box>
                        <Text color={textColor} fontSize="xs" fontWeight="medium" noOfLines={1} textAlign="center">
                          每日推荐
                        </Text>
                      </VStack>
                    )}
                    {officialCharts.length > 0 ? officialCharts.map((chart) => (
                      <VStack
                        key={chart.id}
                        spacing={0.5}
                        cursor="pointer"
                        minW="60px"
                        maxW="70px"
                        flexShrink={0}
                        onClick={() => handleChartClick(chart)}
                        _hover={{ transform: "scale(1.04)" }}
                        transition="transform 0.15s"
                      >
                        <Box
                          w="100%"
                          borderRadius="lg"
                          overflow="hidden"
                          sx={{ aspectRatio: "1 / 1" }}
                        >
                          {playbackSource === "migu" ? (
                            /* 咪咕: 榜单无可用封面，直接把榜单名写在主题色渐变卡片上 */
                            <Box
                              w="100%"
                              h="100%"
                              display="flex"
                              alignItems="center"
                              justifyContent="center"
                              p={1}
                              bg={`linear-gradient(160deg, ${activeColor}55 0%, ${activeColor}18 100%)`}
                            >
                              <Text fontSize="2xs" fontWeight="bold" color={textColor} noOfLines={3} textAlign="center" lineHeight="1.25">
                                {chart.name}
                              </Text>
                            </Box>
                          ) : (
                            <ChakraImage
                              src={coverProxyUrl(chart.cover, proxyPort)}
                              alt=""
                              w="100%"
                              h="100%"
                              objectFit="cover"
                              fallback={
                                <Box w="100%" h="100%" bg="gray.700" display="flex" alignItems="center" justifyContent="center">
                                  <Text fontSize="lg" fontWeight="bold" color="#fff">{chart.name.slice(0, 1)}</Text>
                                </Box>
                              }
                            />
                          )}
                        </Box>
                        <Text color={textColor} fontSize="xs" fontWeight="medium" noOfLines={1} textAlign="center">
                          {chart.name}
                        </Text>
                      </VStack>
                    )) : (
                      <>
                        {[0, 1, 2, 3, 4, 5].map((i) => (
                          <Box key={i} minW="60px" maxW="70px" flexShrink={0} borderRadius="lg" bg={itemHoverBg} sx={{ aspectRatio: "1 / 1" }} />
                        ))}
                      </>
                    )}
                    </HStack>
                  )}
                </VStack>
                )}

                {playbackSource !== "kugou" && playbackSource !== "qqmusic" && (
                <Box flex={1} overflowY="auto" sx={memoScrollbarSx}>
                  {!loginInfo?.logged_in ? (
                    <VStack py={8} spacing={3}>
                      <MusicIcon size={32} color={subTextColor} />
                      <Text color={subTextColor} fontSize="sm" textAlign="center">登录后查看推荐内容</Text>
                    </VStack>
                  ) : dailyRecommendPlaylists.length === 0 ? (
                    <VStack py={8} spacing={3}>
                      <Sparkles size={32} color={subTextColor} />
                      <Text color={subTextColor} fontSize="sm" textAlign="center">点击刷新加载推荐</Text>
                    </VStack>
                  ) : (
                    <motion.div variants={listContainerVariants} initial="hidden" animate="visible" style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                      {dailyRecommendPlaylists.map((pl) => (
                        <motion.div key={`rec-${pl.id}`} variants={listItemVariants}>{renderPlaylistRow(pl, "rec-", handleRecPlaylistClick)}</motion.div>
                      ))}
                    </motion.div>
                  )}
                </Box>
                )}
              </>
            )}
          </LiquidGlassCard>
        </VStack>
      </HStack>

      {/* 底部播放器 */}
      <PlayerBar onExpand={handleExpandPlayer} hidden={expandedPlayer} />

      {/* 展开的播放器 */}
      {expandedPlayer && <ExpandedPlayer onClose={handleCloseExpandedPlayer} />}
    </VStack>
  );
}
