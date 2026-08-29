import {
  Box,
  Flex,
  Text,
  Heading,
  HStack,
  VStack,
  Badge,
  Button,
  IconButton,
  Spinner,
  Tooltip,
  useColorModeValue,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
  AlertDialog,
  AlertDialogOverlay,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogBody,
  AlertDialogFooter,
  Image,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import mediaPlayerImg from "@/assets/windows-media-player.png";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useBackground } from "@/contexts/background-context";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import {
  Video,
  Film,
  RefreshCw,
  FolderPlus,
  FolderOpen,
  Trash2,
  AlertTriangle,
  Info,
  ArrowLeft,
  X,
  Volume2,
  VolumeX,
  Maximize2,
  ListChecks,
  Copy,
  Check,
} from "lucide-react";

// ── MV 播放器样式辅助（与 MusicPage 一致） ──

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

/** 滑块进度背景必须走 inline style，避免 Emotion 为每个百分比生成新规则 */
function sliderBgStyle(activeColor: string, pct: number, trackBg: string) {
  return {
    background: `linear-gradient(to right, ${activeColor} 0%, ${activeColor} ${pct}%, ${trackBg} ${pct}%, ${trackBg} 100%)`,
  };
}

// 圆角播放/暂停图标（SVG 曲线三角，与音乐播放器一致）
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

/** MV 风格播放器（预览与详情共用）：点击播放/暂停 + 自动隐藏控制栏 + 悬停音量 + 全屏 */
function MvStylePlayer({
  src,
  onOpenPlayer,
  onLoadedMeta,
}: {
  src: string;
  onOpenPlayer: () => void;
  onLoadedMeta?: (duration: number, width: number, height: number) => void;
}) {
  const { t } = useTranslation();
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const playerRef = useRef<HTMLDivElement | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [volume, setVolume] = useState(1);
  const [volumeOpen, setVolumeOpen] = useState(false);
  const volTimer = useRef<number | null>(null);

  // 切换视频时重置内部状态
  useEffect(() => {
    setReady(false);
    setError(null);
    setTime(0);
    setDuration(0);
    setPlaying(false);
  }, [src]);

  const openVolume = () => {
    if (volTimer.current) {
      window.clearTimeout(volTimer.current);
      volTimer.current = null;
    }
    setVolumeOpen(true);
  };

  const scheduleCloseVolume = () => {
    if (volTimer.current) window.clearTimeout(volTimer.current);
    volTimer.current = window.setTimeout(() => setVolumeOpen(false), 200);
  };

  const togglePlay = () => {
    const v = videoRef.current;
    if (!v) return;
    if (v.paused) v.play();
    else v.pause();
  };

  return (
    <Box
      ref={playerRef}
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
      {error ? (
        <Flex direction="column" align="center" justify="center" gap={3} w="full" h="full" p={8}>
          <AlertTriangle size={36} color="rgba(255,255,255,0.7)" />
          <Text color="white" fontWeight="medium" textAlign="center">
            {error}
          </Text>
          <LiquidGlassButton size="sm" leftIcon={<MediaPlayerIcon size={15} />} onClick={onOpenPlayer}>
            {t("nvidiaRecording.openWithPlayer")}
          </LiquidGlassButton>
        </Flex>
      ) : (
        <>
          {/* 视频层 */}
          <video
            key={src}
            ref={videoRef}
            src={src}
            autoPlay
            playsInline
            style={{ width: "100%", height: "100%", objectFit: "contain", background: "#000" }}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
            onTimeUpdate={(e) => setTime((e.target as HTMLVideoElement).currentTime)}
            onLoadedMetadata={(e) => {
              const v = e.target as HTMLVideoElement;
              setDuration(v.duration);
              v.volume = volume;
              setReady(true);
              onLoadedMeta?.(v.duration, v.videoWidth, v.videoHeight);
            }}
            onWaiting={() => setReady(false)}
            onCanPlay={() => setReady(true)}
            onPlaying={() => setReady(true)}
            onError={(e: React.SyntheticEvent<HTMLVideoElement>) => {
              const el = e.currentTarget;
              const code = el.error?.code;
              const msg =
                code === MediaError.MEDIA_ERR_SRC_NOT_SUPPORTED
                  ? t("nvidiaRecording.previewUnsupported")
                  : t("nvidiaRecording.previewError") + (code ? ` (${code})` : "");
              setError(msg);
            }}
          />
          {/* 加载中 */}
          {!ready && (
            <Box position="absolute" inset={0} display="flex" alignItems="center" justifyContent="center">
              <Spinner size="lg" sx={{ color: "white" }} />
            </Box>
          )}

          {/* 点击视频切换播放/暂停 */}
          <Box
            position="absolute"
            inset={0}
            display="flex"
            alignItems="center"
            justifyContent="center"
            cursor="pointer"
            onClick={togglePlay}
          >
            {!playing && ready && (
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

          {/* 自定义控制栏 — 播放时自动隐藏，悬停视频区域显示 */}
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
              opacity: playing ? 0 : 1,
              pointerEvents: playing ? "none" : "auto",
              transition: "opacity 0.3s",
              _groupHover: { opacity: 1, pointerEvents: "auto" },
            }}
          >
            <IconButton
              aria-label="Play/Pause"
              icon={playing ? <PauseIcon size={20} /> : <PlayBtn size={20} />}
              size="sm"
              variant="ghost"
              color="white"
              _hover={{ bg: "rgba(255,255,255,0.15)" }}
              _active={{ bg: "rgba(255,255,255,0.25)" }}
              onClick={togglePlay}
            />
            <Text color="white" fontSize="xs" flexShrink={0} w="44px" textAlign="center">
              {formatDuration(time)}
            </Text>
            <Box
              as="input"
              type="range"
              min={0}
              max={duration || 0}
              step={0.1}
              value={time}
              onChange={(e) => {
                const v = videoRef.current;
                if (!v) return;
                const next = parseFloat((e.target as HTMLInputElement).value);
                v.currentTime = next;
                setTime(next);
              }}
              tabIndex={-1}
              flex={1}
              style={sliderBgStyle("#ffffff", duration ? (time / duration) * 100 : 0, "rgba(255,255,255,0.3)")}
              sx={{
                ...rangeSliderSx,
                "&::-webkit-slider-thumb": { ...rangeSliderSx["&::-webkit-slider-thumb"], background: "#ffffff" },
                "&::-moz-range-thumb": { ...rangeSliderSx["&::-moz-range-thumb"], background: "#ffffff" },
                "&::-webkit-slider-runnable-track": { ...rangeSliderSx["&::-webkit-slider-runnable-track"], background: "transparent" },
                "&::-moz-range-track": { ...rangeSliderSx["&::-moz-range-track"], background: "transparent" },
              }}
            />
            <Text color="white" fontSize="xs" flexShrink={0} w="44px" textAlign="center">
              {formatDuration(duration)}
            </Text>
            {/* 音量 — 悬停弹出垂直音量条 */}
            <Box
              position="relative"
              display="flex"
              alignItems="center"
              onMouseEnter={openVolume}
              onMouseLeave={scheduleCloseVolume}
            >
              <IconButton
                aria-label="Mute"
                icon={volume === 0 ? <VolumeX size={18} /> : <Volume2 size={18} />}
                size="sm"
                variant="ghost"
                color="white"
                _hover={{ bg: "rgba(255,255,255,0.15)" }}
                _active={{ bg: "rgba(255,255,255,0.25)" }}
                onClick={() => {
                  const v = videoRef.current;
                  if (!v) return;
                  const next = v.volume > 0 ? 0 : 1;
                  v.volume = next;
                  setVolume(next);
                }}
              />
              <Box
                position="absolute"
                left="50%"
                bottom="100%"
                mb={volumeOpen ? "8px" : "0px"}
                w="90px"
                h={volumeOpen ? "90px" : "0px"}
                opacity={volumeOpen ? 1 : 0}
                pointerEvents={volumeOpen ? "auto" : "none"}
                onMouseEnter={openVolume}
                onMouseLeave={scheduleCloseVolume}
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
                  value={volume}
                  onChange={(e) => {
                    const v = videoRef.current;
                    if (!v) return;
                    const vol = parseFloat((e.target as HTMLInputElement).value);
                    v.volume = vol;
                    setVolume(vol);
                  }}
                  tabIndex={-1}
                  w="90px"
                  flexShrink={0}
                  style={sliderBgStyle("#ffffff", volume * 100, "rgba(255,255,255,0.3)")}
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
              _hover={{ bg: "rgba(255,255,255,0.15)" }}
              _active={{ bg: "rgba(255,255,255,0.25)" }}
              onClick={() => {
                const player = playerRef.current;
                if (!player) return;
                if (document.fullscreenElement) {
                  document.exitFullscreen().catch(() => {});
                } else {
                  player.requestFullscreen?.().catch(() => {});
                }
              }}
            />
          </HStack>
        </>
      )}
    </Box>
  );
}

/** Windows 媒体播放器图标（「打开系统播放器」按钮用） */
function MediaPlayerIcon({ size = 14 }: { size?: number }) {
  return (
    <img
      src={mediaPlayerImg}
      width={size}
      height={size}
      style={{ objectFit: "contain", display: "block" }}
      alt=""
    />
  );
}

// ============================================================================
// 类型定义（与后端 nvidia_recording.rs 对应）
// ============================================================================

interface RecordingVideo {
  path: string;
  name: string;
  size: number;
  modified_ms: number;
  created_ms: number;
  ext: string;
}

interface RecordingFolder {
  path: string;
  name: string;
  custom: boolean;
  video_count: number;
  total_size: number;
}

// ============================================================================
// 工具函数
// ============================================================================

function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const v = bytes / 1024 ** i;
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

/** 格式化日期的关键部分 */
function dateKey(ms: number): string {
  const d = new Date(ms);
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
}

function dateLabel(key: string, t: (k: string) => string): string {
  const [y, m, d] = key.split("-").map(Number);
  const now = new Date();
  if (key === dateKey(now.getTime())) return t("nvidiaRecording.today");
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
  if (key === dateKey(yesterday.getTime())) return t("nvidiaRecording.yesterday");
  return `${y}年${m}月${d}日`;
}

function timeLabel(ms: number): string {
  const d = new Date(ms);
  const y = d.getFullYear();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${y}-${mm}-${dd} ${hh}:${mi}`;
}

function formatDuration(sec: number): string {
  if (!isFinite(sec) || sec < 0) return "--:--";
  const m = Math.floor(sec / 60);
  const s = Math.floor(sec % 60);
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

async function openWithPlayer(path: string) {
  // 走后端命令（Rust 侧 opener API），避免前端 opener 插件权限/作用域问题
  try {
    await invoke("open_video_with_system_player", { path });
  } catch (e) {
    console.error("open_video_with_system_player failed:", e);
  }
}

async function revealInFolder(path: string) {
  try {
    await invoke("reveal_video_in_explorer", { path });
  } catch (e) {
    console.error("reveal_video_in_explorer failed:", e);
  }
}

/** 规范化路径用于比较：统一分隔符、去尾部分隔符、忽略大小写 */
function normPath(p: string): string {
  return p.replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();
}

/** 视频是否位于指定目录内 */
function isUnderFolder(videoPath: string, folderPath: string): boolean {
  return normPath(videoPath).startsWith(normPath(folderPath) + "\\");
}

// ============================================================================
// 页面组件
// ============================================================================

/** 缩略图分批发加载的批大小 */
const THUMB_BATCH = 12;

export default function NvidiaRecordingPage() {
  const { t } = useTranslation();
  const adaptiveTitle = useAdaptiveTextColor();
  const { config, getActiveColor, getHoverColor } = useThemeColor();
  const toast = useDynamicIsland("wrench");
  const navigate = useNavigate();

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#cccccc");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  // 卡片缩略图区域的底色/边框随主题（在组件顶层取，避免在渲染回调内调用 hook）
  const prefersColorScheme = useColorModeValue("light", "dark");
  // 悬浮控制条：液态玻璃开/关两套外观
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const solidBarBg = useColorModeValue("rgba(255,255,255,0.88)", "rgba(20,20,20,0.88)");
  const glassBarBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const glassBarBorder = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");

  const [scanning, setScanning] = useState(false);
  const [videos, setVideos] = useState<RecordingVideo[]>([]);
  const [folders, setFolders] = useState<RecordingFolder[]>([]);
  /** 当前浏览的目录（null = 全部目录聚合视图） */
  const [activeFolder, setActiveFolder] = useState<string | null>(null);
  const [previewVideo, setPreviewVideo] = useState<RecordingVideo | null>(null);
  const [previewClosing, setPreviewClosing] = useState(false);
  // 详情 Modal（视频 + 信息表，播放器状态在 MvStylePlayer 内部）
  const [detailVideo, setDetailVideo] = useState<RecordingVideo | null>(null);
  const [detailInfo, setDetailInfo] = useState<{ duration?: number; width?: number; height?: number }>({});
  // 播放器延迟挂载：等弹窗展开动画结束后再初始化视频解码，避免打开时掉帧
  const [detailPlayerMounted, setDetailPlayerMounted] = useState(false);
  const detailMountTimer = useRef<number | null>(null);
  const [deleteTargets, setDeleteTargets] = useState<RecordingVideo[]>([]);
  const [deleting, setDeleting] = useState(false);
  // 批量操作：选择模式 + 已选视频路径集合
  const [batchMode, setBatchMode] = useState(false);
  const [selectedVideos, setSelectedVideos] = useState<Set<string>>(new Set());
  const [copying, setCopying] = useState(false);

  const deleteCancelRef = useRef<HTMLButtonElement>(null);

  // ---- 缩略图（后端生成 + 磁盘缓存，前端分批请求） ----
  const thumbLoaded = useRef<Set<string>>(new Set());
  const [thumbMap, setThumbMap] = useState<Record<string, string>>({});

  const loadThumbs = useCallback(async (paths: string[]) => {
    for (let i = 0; i < paths.length; i += THUMB_BATCH) {
      const batch = paths.slice(i, i + THUMB_BATCH);
      try {
        const map = await invoke<Record<string, string | null>>("get_video_thumbnails", {
          paths: batch,
        });
        setThumbMap((prev) => {
          const next = { ...prev };
          for (const [p, thumb] of Object.entries(map)) {
            if (thumb) next[p] = convertFileSrc(thumb);
          }
          return next;
        });
      } catch (e) {
        console.error("get_video_thumbnails failed:", e);
      } finally {
        for (const p of batch) thumbLoaded.current.add(p);
      }
    }
  }, []);

  const runScan = useCallback(async () => {
    setScanning(true);
    try {
      const result = await invoke<{ folders: RecordingFolder[]; videos: RecordingVideo[] }>(
        "scan_nvidia_recordings"
      );
      setFolders(result.folders);
      setVideos(result.videos);
      // 当前浏览的目录已不在扫描结果中（被移除等）时回到全部视图
      setActiveFolder((cur) =>
        cur && result.folders.some((f) => normPath(f.path) === normPath(cur)) ? cur : null
      );
    } catch (e) {
      console.error("scan_nvidia_recordings failed:", e);
      toast({
        title: t("nvidiaRecording.scanError"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setScanning(false);
    }
  }, [toast, t]);

  useEffect(() => {
    runScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 扫描完成/更换后，为尚未加载的视频分批请求缩略图
  useEffect(() => {
    const pending = videos.filter((v) => !thumbLoaded.current.has(v.path)).map((v) => v.path);
    if (pending.length) loadThumbs(pending);
  }, [videos, loadThumbs]);

  // 当前目录过滤（null = 全部）
  const visibleVideos = useMemo(
    () => (activeFolder ? videos.filter((v) => isUnderFolder(v.path, activeFolder)) : videos),
    [videos, activeFolder]
  );

  // 按天分组（保持返回的修改时间倒序）
  const groups = useMemo(() => {
    const map = new Map<string, RecordingVideo[]>();
    for (const v of visibleVideos) {
      const key = dateKey(v.modified_ms);
      const arr = map.get(key);
      if (arr) arr.push(v);
      else map.set(key, [v]);
    }
    return Array.from(map.entries());
  }, [visibleVideos]);

  const handleAddFolder = async () => {
    try {
      await invoke<string[]>("add_nvidia_recording_folder");
      await runScan();
    } catch (e) {
      toast({
        title: t("nvidiaRecording.addFolderError"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleRemoveFolder = async (path: string) => {
    try {
      await invoke<string[]>("remove_nvidia_recording_folder", { path });
      toast({
        title: t("nvidiaRecording.folderRemoved"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      await runScan();
    } catch (e) {
      toast({
        title: t("nvidiaRecording.removeFolderError"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleDelete = async () => {
    if (deleteTargets.length === 0) return;
    setDeleting(true);
    try {
      const result = await invoke<{ deleted: string[]; errors: Array<[string, string]> }>(
        "delete_nvidia_recording_video",
        { paths: deleteTargets.map((v) => v.path) }
      );
      if (result.errors.length > 0) {
        const msg = result.errors.map(([, err]) => err).join("；");
        toast({
          title: t("nvidiaRecording.deleteError"),
          description: msg,
          status: "error",
          duration: 4000,
          isClosable: true,
        });
      } else {
        toast({
          title:
            deleteTargets.length > 1
              ? t("nvidiaRecording.deleteSuccessMulti", { n: deleteTargets.length })
              : t("nvidiaRecording.deleteSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
      setDeleteTargets([]);
      // 批量删除后清空选择，避免残留失效路径
      setSelectedVideos(new Set());
      await runScan();
    } catch (e) {
      toast({
        title: t("nvidiaRecording.deleteError"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setDeleting(false);
    }
  };

  // ---- 批量操作 ----
  const enterBatchMode = () => {
    setSelectedVideos(new Set());
    setBatchMode(true);
  };

  const exitBatchMode = () => {
    setBatchMode(false);
    setSelectedVideos(new Set());
  };

  const toggleSelectVideo = (path: string) => {
    setSelectedVideos((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  // 批量复制：弹出目标文件夹选择框，由后端逐一复制
  const handleBatchCopy = async () => {
    if (selectedVideos.size === 0) return;
    setCopying(true);
    try {
      const result = await invoke<{
        copied: string[];
        errors: Array<[string, string]>;
        cancelled: boolean;
      }>("copy_nvidia_recording_videos", { paths: Array.from(selectedVideos) });
      if (result.cancelled) return;
      if (result.errors.length > 0) {
        const msg = result.errors.map(([, err]) => err).join("；");
        toast({
          title: t("nvidiaRecording.copyError"),
          description: msg,
          status: "error",
          duration: 4000,
          isClosable: true,
        });
      } else {
        toast({
          title: t("nvidiaRecording.copySuccess", { n: result.copied.length }),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (e) {
      toast({
        title: t("nvidiaRecording.copyError"),
        description: String(e),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setCopying(false);
    }
  };

  const handlePreview = (v: RecordingVideo) => {
    setPreviewVideo(v);
  };

  // 详情：视频 + 完整信息（播放器状态在 MvStylePlayer 内部）
  const handleDetails = (v: RecordingVideo) => {
    setDetailInfo({});
    setDetailVideo(v);
    setDetailPlayerMounted(false);
    if (detailMountTimer.current) window.clearTimeout(detailMountTimer.current);
    detailMountTimer.current = window.setTimeout(() => setDetailPlayerMounted(true), 250);
  };

  const closeDetails = () => {
    if (detailMountTimer.current) {
      window.clearTimeout(detailMountTimer.current);
      detailMountTimer.current = null;
    }
    setDetailPlayerMounted(false);
    setDetailVideo(null);
  };

  const closePreview = useCallback(() => {
    setPreviewClosing(true);
    window.setTimeout(() => {
      setPreviewVideo(null);
      setPreviewClosing(false);
    }, 280);
  }, []);

  const scrollbarSx = {
    "&::-webkit-scrollbar": { width: "6px" },
    "&::-webkit-scrollbar-track": { background: "transparent", margin: "10px 0" },
    "&::-webkit-scrollbar-thumb": {
      background: config.primaryColor,
      borderRadius: "3px",
      minHeight: "40px",
    },
    "&::-webkit-scrollbar-thumb:hover": {
      background: config.primaryColor,
      opacity: 0.8,
      filter: "brightness(0.9)",
    },
  } as const;

  return (
    <Box pt={8} flex={1} minH="0" display="flex" flexDirection="column">
      {/* 页头（含返回按钮） */}
      <Flex w="full" justify="space-between" align="center" mb={6} flexShrink={0}>
        <HStack>
          <IconButton
            aria-label={t("builtinTools.back")}
            icon={<ArrowLeft size={20} />}
            variant="ghost"
            onClick={() => navigate("/builtin-tools")}
            color={adaptiveTitle.text}
          />
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            {t("nvidiaRecording.title")}
          </Heading>
        </HStack>
        <HStack spacing={3}>
          <LiquidGlassButton
            size="sm"
            leftIcon={<ListChecks size={16} />}
            onClick={batchMode ? exitBatchMode : enterBatchMode}
          >
            {t("nvidiaRecording.batchAction")}
          </LiquidGlassButton>
          <LiquidGlassButton
            size="sm"
            leftIcon={<RefreshCw size={16} />}
            onClick={runScan}
            isDisabled={scanning}
          >
            {t("nvidiaRecording.scan")}
          </LiquidGlassButton>
          <LiquidGlassButton size="sm" leftIcon={<FolderPlus size={16} />} onClick={handleAddFolder}>
            {t("nvidiaRecording.addFolder")}
          </LiquidGlassButton>
        </HStack>
      </Flex>

      {/* 目录 Tab：全部目录 + 各录制目录，点击直切；自定义目录可移除 */}
      {!scanning && folders.length > 0 && videos.length > 0 && (
        <HStack mb={4} flexShrink={0} spacing={2} flexWrap="wrap" rowGap={2} align="center">
          <FolderChip
            label={t("nvidiaRecording.allFolders")}
            count={videos.length}
            active={activeFolder === null}
            onClick={() => setActiveFolder(null)}
            accentColor={getActiveColor()}
          />
          {folders.map((f) => (
            <FolderChip
              key={f.path}
              label={f.name}
              count={f.video_count}
              active={activeFolder !== null && normPath(f.path) === normPath(activeFolder)}
              onClick={() => setActiveFolder(f.path)}
              onDelete={f.custom ? () => handleRemoveFolder(f.path) : undefined}
              accentColor={getActiveColor()}
            />
          ))}
        </HStack>
      )}

      {/* 扫描中 */}
      {scanning && (
        <Flex flex={1} align="center" justify="center" direction="column" gap={3}>
          <Spinner thickness="3px" color={getActiveColor()} size="lg" />
          <Text fontSize="sm" color={subTextColor}>
            {t("nvidiaRecording.scanning")}
          </Text>
        </Flex>
      )}

      {/* 空状态 */}
      {!scanning && videos.length === 0 && (
        <Flex flex={1} align="center" justify="center">
          <LiquidGlassCard p={10} textAlign="center">
            <VStackPlaceholder
              icon={<Video size={48} color={subTextColor} />}
              text={t("nvidiaRecording.noVideosHint")}
              action={
                <LiquidGlassButton leftIcon={<FolderPlus size={16} />} onClick={handleAddFolder}>
                  {t("nvidiaRecording.addFolder")}
                </LiquidGlassButton>
              }
            />
          </LiquidGlassCard>
        </Flex>
      )}

      {/* 当前目录为空 */}
      {!scanning && videos.length > 0 && visibleVideos.length === 0 && (
        <Flex flex={1} align="center" justify="center">
          <Text fontSize="sm" color={subTextColor}>
            {t("nvidiaRecording.folderEmpty")}
          </Text>
        </Flex>
      )}

      {/* 视频列表（聚合全部录制目录，按天分组） */}
      {!scanning && videos.length > 0 && (
        <Box flex={1} minH="0" overflowY="auto" overflowX="hidden" pr={1} sx={scrollbarSx}>
          {groups.map(([key, list]) => {
            const groupSize = list.reduce((s, v) => s + v.size, 0);
            return (
              <Box key={key} mb={6}>
                <HStack mb={3} spacing={3}>
                  <Text fontSize="lg" fontWeight="bold" color={textColor}>
                    {dateLabel(key, t)}
                  </Text>
                  <Badge fontSize="xs" colorScheme="gray">
                    {list.length}
                  </Badge>
                  <Text fontSize="xs" color={subTextColor}>
                    {formatBytes(groupSize)}
                  </Text>
                </HStack>
                <Box borderBottom="1px solid" borderColor={dividerColor} mb={3} />
                <Box
                  display="grid"
                  gridTemplateColumns="repeat(auto-fill, minmax(240px, 1fr))"
                  gap={4}
                  alignItems="stretch"
                >
                  {list.map((v) => (
                    <VideoCard
                      key={v.path}
                      video={v}
                      thumbSrc={thumbMap[v.path]}
                      onPreview={handlePreview}
                      onDetails={handleDetails}
                      onDelete={(item) => setDeleteTargets([item])}
                      batchMode={batchMode}
                      selected={selectedVideos.has(v.path)}
                      onToggleSelect={toggleSelectVideo}
                      subTextColor={subTextColor}
                      hoverColor={getHoverColor()}
                      accentColor={getActiveColor()}
                      prefersColorScheme={prefersColorScheme}
                    />
                  ))}
                </Box>
              </Box>
            );
          })}
        </Box>
      )}

      {/* 批量操作悬浮控制条（适配液态玻璃） */}
      {batchMode && (
        <Flex
          position="fixed"
          bottom={8}
          left="50%"
          transform="translateX(-50%)"
          zIndex={9990}
          align="center"
          gap={3}
          px={5}
          py={2.5}
          borderRadius="full"
          border="1px solid"
          borderColor={liquidGlassEnabled ? glassBarBorder : dividerColor}
          boxShadow="xl"
          backdropFilter={liquidGlassEnabled ? `blur(${Math.max(liquidGlassBlur, 10)}px) saturate(1.3)` : undefined}
          sx={{ WebkitBackdropFilter: liquidGlassEnabled ? `blur(${Math.max(liquidGlassBlur, 10)}px) saturate(1.3)` : undefined }}
          bg={liquidGlassEnabled ? glassBarBg : solidBarBg}
        >
          <Text fontSize="sm" fontWeight="medium" color={textColor} whiteSpace="nowrap">
            {t("nvidiaRecording.selectedCount", { n: selectedVideos.size })}
          </Text>
          <LiquidGlassButton
            size="sm"
            leftIcon={<Copy size={15} />}
            onClick={handleBatchCopy}
            isDisabled={selectedVideos.size === 0 || copying}
            isLoading={copying}
          >
            {t("nvidiaRecording.copy")}
          </LiquidGlassButton>
          <LiquidGlassButton
            size="sm"
            leftIcon={<Trash2 size={15} />}
            onClick={() => setDeleteTargets(videos.filter((v) => selectedVideos.has(v.path)))}
            isDisabled={selectedVideos.size === 0 || deleting}
          >
            {t("nvidiaRecording.delete")}
          </LiquidGlassButton>
          <IconButton
            aria-label={t("nvidiaRecording.exitBatch")}
            icon={<X size={16} />}
            size="sm"
            variant="ghost"
            onClick={exitBatchMode}
          />
        </Flex>
      )}

      {/* 视频预览 — 音乐播放器 MV 风格（毛玻璃全屏遮罩 + 上滑动画 + 自定义控制栏） */}
      {previewVideo && (
        <Box
          position="fixed"
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
          onClick={closePreview}
          sx={{
            backdropFilter: "blur(14px)",
            WebkitBackdropFilter: "blur(14px)",
            "@keyframes recPreviewFadeIn": {
              from: { opacity: 0 },
              to: { opacity: 1 },
            },
            "@keyframes recPreviewFadeOut": {
              from: { opacity: 1 },
              to: { opacity: 0 },
            },
            animation: previewClosing
              ? "recPreviewFadeOut 0.3s cubic-bezier(0.4, 0, 0.2, 1) forwards"
              : "recPreviewFadeIn 0.35s cubic-bezier(0.4, 0, 0.2, 1)",
          }}
        >
          <Box
            w="100%"
            maxW="min(60vw, calc(60vh * 16 / 9))"
            onClick={(e) => e.stopPropagation()}
            sx={{
              "@keyframes recPreviewSlideUp": {
                from: { transform: "translateY(40px) scale(0.98)", opacity: 0 },
                to: { transform: "translateY(0) scale(1)", opacity: 1 },
              },
              "@keyframes recPreviewSlideDown": {
                from: { transform: "translateY(0) scale(1)", opacity: 1 },
                to: { transform: "translateY(40px) scale(0.98)", opacity: 0 },
              },
              animation: previewClosing
                ? "recPreviewSlideDown 0.3s cubic-bezier(0.4, 0, 0.2, 1) forwards"
                : "recPreviewSlideUp 0.35s cubic-bezier(0.4, 0, 0.2, 1)",
            }}
          >
            <HStack justify="space-between" mb={3}>
              <VStack spacing={0} align="start">
                <Text color="white" fontSize="lg" fontWeight="bold" noOfLines={1}>
                  {previewVideo.name}
                </Text>
                <Text color="rgba(255,255,255,0.6)" fontSize="sm">
                  {previewVideo.ext.toUpperCase()} · {formatBytes(previewVideo.size)} ·{" "}
                  {timeLabel(previewVideo.modified_ms)}
                </Text>
              </VStack>
              <HStack spacing={1}>
                <IconButton
                  aria-label={t("nvidiaRecording.openWithPlayer")}
                  icon={<MediaPlayerIcon size={18} />}
                  size="sm"
                  variant="ghost"
                  color="white"
                  _hover={{ bg: "rgba(255,255,255,0.15)" }}
                  _active={{ bg: "rgba(255,255,255,0.25)" }}
                  onClick={() => openWithPlayer(previewVideo.path)}
                />
                <IconButton
                  aria-label={t("nvidiaRecording.close")}
                  icon={<X size={20} />}
                  size="sm"
                  variant="ghost"
                  color="white"
                  _hover={{ bg: "rgba(255,255,255,0.15)" }}
                  _active={{ bg: "rgba(255,255,255,0.25)" }}
                  onClick={closePreview}
                />
              </HStack>
            </HStack>
            <MvStylePlayer
              key={previewVideo.path}
              src={convertFileSrc(previewVideo.path)}
              onOpenPlayer={() => openWithPlayer(previewVideo.path)}
            />
          </Box>
        </Box>
      )}

      {/* 详情 Modal：视频 + 完整信息（与旧版预览一致） */}
      <Modal
        isOpen={!!detailVideo}
        onClose={closeDetails}
        size="2xl"
        scrollBehavior="inside"
        closeOnOverlayClick
        returnFocusOnClose={false}
      >
        <ModalOverlay />
        {detailVideo && (
          <ModalContent bg={useColorModeValue("white", "#141414")} borderRadius="2xl">
            <ModalHeader fontSize="md" color={textColor} pr={14}>
              <HStack spacing={2}>
                <Info size={16} color={getActiveColor()} />
                <Text noOfLines={1}>{detailVideo.name}</Text>
              </HStack>
            </ModalHeader>
            <ModalCloseButton />
            <ModalBody pb={4}>
              {detailPlayerMounted ? (
                <MvStylePlayer
                  key={detailVideo.path}
                  src={convertFileSrc(detailVideo.path)}
                  onOpenPlayer={() => openWithPlayer(detailVideo.path)}
                  onLoadedMeta={(duration, width, height) => setDetailInfo({ duration, width, height })}
                />
              ) : (
                <Box borderRadius="lg" bg="black" sx={{ aspectRatio: "16 / 9" }} />
              )}
              <VStack mt={4} spacing={0} divider={<Box borderColor={dividerColor} />} align="stretch" fontSize="sm">
                <InfoRow label={t("nvidiaRecording.fileName")} value={detailVideo.name} />
                <InfoMarqueeRow label={t("nvidiaRecording.filePath")} value={detailVideo.path} />
                <InfoRow label={t("nvidiaRecording.size")} value={formatBytes(detailVideo.size)} />
                <InfoRow label={t("nvidiaRecording.modified")} value={new Date(detailVideo.modified_ms).toLocaleString()} />
                <InfoRow label={t("nvidiaRecording.created")} value={new Date(detailVideo.created_ms).toLocaleString()} />
                <InfoRow
                  label={t("nvidiaRecording.duration")}
                  value={
                    detailInfo.duration !== undefined
                      ? formatDuration(detailInfo.duration)
                      : t("nvidiaRecording.loading")
                  }
                />
                <InfoRow
                  label={t("nvidiaRecording.resolution")}
                  value={
                    detailInfo.width && detailInfo.height
                      ? `${detailInfo.width} × ${detailInfo.height}`
                      : t("nvidiaRecording.unknown")
                  }
                />
                <InfoRow label={t("nvidiaRecording.format")} value={detailVideo.ext.toUpperCase()} />
              </VStack>
            </ModalBody>
            <ModalFooter>
              <HStack spacing={3}>
                <Button variant="ghost" onClick={closeDetails}>
                  {t("nvidiaRecording.close")}
                </Button>
                <LiquidGlassButton
                  size="sm"
                  leftIcon={<MediaPlayerIcon size={17} />}
                  onClick={() => openWithPlayer(detailVideo.path)}
                >
                  {t("nvidiaRecording.openWithPlayer")}
                </LiquidGlassButton>
              </HStack>
            </ModalFooter>
          </ModalContent>
        )}
      </Modal>

      {/* 删除二次确认（单个 / 批量共用） */}
      <AlertDialog
        isOpen={deleteTargets.length > 0}
        leastDestructiveRef={deleteCancelRef}
        onClose={() => setDeleteTargets([])}
        returnFocusOnClose={false}
      >
        <AlertDialogOverlay>
          <AlertDialogContent bg={useColorModeValue("white", "#141414")} borderRadius="2xl">
            <AlertDialogHeader fontSize="lg" fontWeight="bold" color={textColor}>
              <HStack spacing={2}>
                <AlertTriangle size={18} color="red.400" />
                <Text>{t("nvidiaRecording.deleteTitle")}</Text>
              </HStack>
            </AlertDialogHeader>
            <AlertDialogBody color={subTextColor}>
              <Text mb={2}>
                {deleteTargets.length > 1
                  ? t("nvidiaRecording.deleteBodyMulti", { n: deleteTargets.length })
                  : t("nvidiaRecording.deleteBody")}
              </Text>
              <Text fontWeight="semibold" color={textColor} noOfLines={2}>
                {deleteTargets.length > 1
                  ? `${deleteTargets[0]?.name} ${t("nvidiaRecording.andMore", { n: deleteTargets.length - 1 })}`
                  : deleteTargets[0]?.name}
              </Text>
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={deleteCancelRef} onClick={() => setDeleteTargets([])}>
                {t("nvidiaRecording.cancel")}
              </Button>
              <Button colorScheme="red" ml={3} isLoading={deleting} onClick={handleDelete}>
                {t("nvidiaRecording.deleteConfirm")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </Box>
  );
}

/** 视频卡片：缩略图 + 名称/大小/时间 + 操作按钮 */
function VideoCard({
  video,
  thumbSrc,
  onPreview,
  onDetails,
  onDelete,
  batchMode,
  selected,
  onToggleSelect,
  subTextColor,
  hoverColor,
  accentColor,
  prefersColorScheme,
}: {
  video: RecordingVideo;
  thumbSrc?: string;
  onPreview: (v: RecordingVideo) => void;
  onDetails: (v: RecordingVideo) => void;
  onDelete: (v: RecordingVideo) => void;
  batchMode?: boolean;
  selected?: boolean;
  onToggleSelect: (path: string) => void;
  subTextColor: string;
  hoverColor: string;
  accentColor: string;
  prefersColorScheme: "light" | "dark";
}) {
  const { t } = useTranslation();
  const hasThumb = !!thumbSrc;

  return (
    <LiquidGlassCard
      p={0}
      overflow="hidden"
      _hover={{ borderColor: batchMode ? undefined : hoverColor }}
      cursor={batchMode ? "pointer" : undefined}
      onClick={batchMode ? () => onToggleSelect(video.path) : undefined}
      sx={selected ? { outline: "2px solid", outlineColor: accentColor, outlineOffset: "-1px" } : undefined}
    >
      {/* 缩略图（点击直接打开 MV 风格预览；名称覆盖在底部） */}
      <Box
        position="relative"
        borderBottom="1px solid"
        borderColor={useColorModeValue("gray.100", "#262626")}
        bg={prefersColorScheme === "dark" ? "#0d0d0d" : "#f3f4f6"}
        sx={{ aspectRatio: "16/9" }}
        cursor={batchMode ? undefined : "pointer"}
        onClick={batchMode ? undefined : () => onPreview(video)}
      >
        {hasThumb ? (
          <Image
            src={thumbSrc}
            alt={video.name}
            w="full"
            h="full"
            objectFit="cover"
            loading="lazy"
            fallback={
              <Flex w="full" h="full" align="center" justify="center">
                <Spinner size="sm" color={accentColor} thickness="2px" />
              </Flex>
            }
            _hover={{ transform: "scale(1.03)" }}
            transition="transform 0.3s"
          />
        ) : (
          <Flex w="full" h="full" align="center" justify="center" color={subTextColor}>
            <Film size={26} opacity={0.6} />
          </Flex>
        )}
        {/* 批量选择勾选框 */}
        {batchMode && (
          <Box
            position="absolute"
            top={2}
            left={2}
            w="22px"
            h="22px"
            borderRadius="full"
            border="2px solid"
            borderColor={selected ? accentColor : "rgba(255,255,255,0.9)"}
            bg={selected ? accentColor : "rgba(0,0,0,0.35)"}
            display="flex"
            alignItems="center"
            justifyContent="center"
            color="white"
            pointerEvents="none"
            transition="all 0.15s"
          >
            {selected && <Check size={13} strokeWidth={3} />}
          </Box>
        )}
        <Box
          position="absolute"
          left={0}
          right={0}
          bottom={0}
          px={2.5}
          pt={8}
          pb={2}
          background="linear-gradient(to top, rgba(0,0,0,0.72) 55%, rgba(0,0,0,0))"
          pointerEvents="none"
        >
          <MarqueeText text={video.name} />
        </Box>
      </Box>

      <Box px={3} py={2}>
        <Flex fontSize="xs" color={subTextColor} justify="space-between" align="center">
          <Text>{formatBytes(video.size)}</Text>
          <Text>{timeLabel(video.modified_ms)}</Text>
        </Flex>
        {!batchMode && (
          <Flex w="full" justify="flex-end" gap={0.5} mt={1}>
            <Tooltip label={t("nvidiaRecording.details")} placement="top" closeOnClick>
              <IconButton
                aria-label={t("nvidiaRecording.details")}
                icon={<Info size={14} />}
                size="xs"
                variant="ghost"
                colorScheme="telegram"
                onClick={(e) => {
                  // 立即失焦：避免 Modal 关闭后焦点归还触发 Tooltip 复现
                  (e.currentTarget as HTMLButtonElement).blur();
                  onDetails(video);
                }}
              />
            </Tooltip>
            <Tooltip label={t("nvidiaRecording.openWithPlayer")} placement="top" closeOnClick>
              <IconButton
                aria-label={t("nvidiaRecording.openWithPlayer")}
                icon={<MediaPlayerIcon size={16} />}
                size="xs"
                variant="ghost"
                onClick={(e) => {
                  (e.currentTarget as HTMLButtonElement).blur();
                  openWithPlayer(video.path);
                }}
              />
            </Tooltip>
            <Tooltip label={t("nvidiaRecording.revealInFolder")} placement="top" closeOnClick>
              <IconButton
                aria-label={t("nvidiaRecording.revealInFolder")}
                icon={<FolderOpen size={14} />}
                size="xs"
                variant="ghost"
                onClick={(e) => {
                  (e.currentTarget as HTMLButtonElement).blur();
                  revealInFolder(video.path);
                }}
              />
            </Tooltip>
            <Tooltip label={t("nvidiaRecording.delete")} placement="top" closeOnClick>
              <IconButton
                aria-label={t("nvidiaRecording.delete")}
                icon={<Trash2 size={14} />}
                size="xs"
                variant="ghost"
                colorScheme="red"
                onClick={(e) => {
                  (e.currentTarget as HTMLButtonElement).blur();
                  onDelete(video);
                }}
              />
            </Tooltip>
          </Flex>
        )}
      </Box>
    </LiquidGlassCard>
  );
}

/** 单行文本：溢出时鼠标悬停触发往返轮播滚动（缩略图名称 / 详情路径共用） */
function MarqueeText({
  text,
  color = "white",
  fontSize = "xs",
  fontWeight = "semibold",
  textShadow = "0 1px 2px rgba(0,0,0,0.6)",
  align,
}: {
  text: string;
  color?: string;
  fontSize?: string;
  fontWeight?: string;
  textShadow?: string;
  /** end：未溢出时靠右显示（溢出悬停滚动时从头开始） */
  align?: "start" | "end";
}) {
  const outerRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const [shift, setShift] = useState(0);
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    const measure = () => {
      const outer = outerRef.current;
      const inner = innerRef.current;
      if (!outer || !inner) return;
      setShift(Math.max(0, inner.scrollWidth - outer.clientWidth));
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [text]);

  const scrolling = hovering && shift > 0;
  const duration = shift > 0 ? Math.max(3, shift / 28) : 0;

  return (
    <Box
      ref={outerRef}
      overflow="hidden"
      display="flex"
      justifyContent={align === "end" && !scrolling ? "flex-end" : "flex-start"}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      sx={{
        // 共享 keyframes：位移量走 CSS 变量，多卡片互不冲突
        "@keyframes rec-name-marquee": {
          "0%": { transform: "translateX(0)" },
          "100%": { transform: "translateX(calc(-1 * var(--marquee-shift)))" },
        },
      }}
    >
      <Box
        ref={innerRef}
        as="span"
        display="inline-block"
        whiteSpace="nowrap"
        fontSize={fontSize}
        fontWeight={fontWeight}
        color={color}
        textShadow={textShadow}
        style={
          {
            "--marquee-shift": `${shift}px`,
            animation: scrolling ? `rec-name-marquee ${duration}s linear infinite alternate` : undefined,
          } as React.CSSProperties
        }
      >
        {text}
      </Box>
    </Box>
  );
}

/** 目录 Tab（自定义目录带移除按钮） */
function FolderChip({
  label,
  count,
  active,
  onClick,
  onDelete,
  accentColor,
}: {
  label: string;
  count: number;
  active?: boolean;
  onClick: () => void;
  onDelete?: () => void;
  accentColor: string;
}) {
  const chipBg = useColorModeValue("gray.100", "#252525");
  // 液态玻璃开启时用半透明+背景模糊；未开启时保持普通底色
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const glassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const glassBorder = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");
  return (
    <Box
      as="button"
      onClick={onClick}
      display="inline-flex"
      alignItems="center"
      pl={3}
      pr={onDelete ? 1.5 : 3}
      py={1.5}
      borderRadius="full"
      fontSize="xs"
      fontWeight={active ? "bold" : "medium"}
      bg={liquidGlassEnabled ? glassBg : chipBg}
      backdropFilter={liquidGlassEnabled ? `blur(${Math.max(liquidGlassBlur, 8)}px) saturate(1.3)` : undefined}
      border="1px solid"
      borderColor={active ? accentColor : liquidGlassEnabled ? glassBorder : "transparent"}
      color={active ? accentColor : undefined}
      whiteSpace="nowrap"
      cursor="pointer"
      transition="all 0.15s"
      _hover={{ borderColor: accentColor }}
    >
      <Box as="span">{label}</Box>
      <Box as="span" opacity={0.7} ml={1.5}>
        {count}
      </Box>
      {onDelete && (
        <Flex
          as="span"
          role="button"
          aria-label="remove folder"
          align="center"
          justify="center"
          ml={1.5}
          w="18px"
          h="18px"
          flexShrink={0}
          borderRadius="full"
          color="inherit"
          opacity={0.6}
          onClick={(e: React.MouseEvent) => {
            e.stopPropagation();
            onDelete();
          }}
          _hover={{ opacity: 1, bg: "rgba(255,0,0,0.15)", color: "red.400" }}
        >
          <X size={12} />
        </Flex>
      )}
    </Box>
  );
}

/** 信息面板行 */
function InfoRow({ label, value }: { label: string; value: string }) {
  const labelColor = useColorModeValue("gray.500", "#999999");
  const valueColor = useColorModeValue("gray.800", "#ffffff");
  return (
    <Flex py={2} gap={4} alignItems="flex-start">
      <Text w={88} flexShrink={0} color={labelColor}>
        {label}
      </Text>
      <Text color={valueColor} wordBreak="break-all" textAlign="right" flex={1}>
        {value}
      </Text>
    </Flex>
  );
}

/** 信息面板行 — 长文本单行显示，溢出悬停轮播滚动（用于完整路径） */
function InfoMarqueeRow({ label, value }: { label: string; value: string }) {
  const labelColor = useColorModeValue("gray.500", "#999999");
  const valueColor = useColorModeValue("gray.800", "#ffffff");
  return (
    <Flex py={2} gap={4} alignItems="center">
      <Text w={88} flexShrink={0} color={labelColor}>
        {label}
      </Text>
      <Box flex={1} minW={0}>
        <MarqueeText text={value} color={valueColor} fontSize="sm" fontWeight="normal" textShadow="none" />
      </Box>
    </Flex>
  );
}

// 空状态占位布局
function VStackPlaceholder({
  icon,
  text,
  action,
}: {
  icon: React.ReactNode;
  text: string;
  action?: React.ReactNode;
}) {
  return (
    <Flex direction="column" align="center" gap={4}>
      {icon}
      <Text>{text}</Text>
      {action}
    </Flex>
  );
}