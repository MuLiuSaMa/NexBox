"use client";

import React, { useCallback, useEffect, useId, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { motion, useAnimationControls } from "framer-motion";
import { useColorModeValue, Tooltip } from "@chakra-ui/react";
import { useNavigate } from "react-router-dom";
import { useThemeColor } from "@/contexts/theme-color-context";
import {
  Zap,
  MemoryStick,
  Globe,
  Cpu,
  Layers,
  HardDrive,
  AudioWaveform,
  Gamepad2,
  Rocket,
  Sparkles,
  ShieldCheck,
  Crosshair,
  Mouse,
  SlidersHorizontal,
  Wrench,
  Monitor,
  Music,
  Download,
  Bluetooth,
  Image,
  Search,
  FileText,
  LayoutGrid,
  PanelsTopLeft,
  Gauge,
  Settings,
} from "lucide-react";
import { useMusicStore, coverProxyUrl } from "@/stores/music-store";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { ExternalPlayback, Song } from "@/types/music";

export type IslandStatus = "success" | "error" | "info" | "warning" | "loading" | "blue";

/** 预设语义图标键，按页面/操作类型区分（如电源计划→闪电） */
export type IconKey =
  | "power"
  | "memory"
  | "network"
  | "cpu"
  | "layers"
  | "disk"
  | "audio"
  | "gamepad"
  | "rocket"
  | "sparkles"
  | "shield"
  | "target"
  | "mouse"
  | "gpu"
  | "filter"
  | "wrench"
  | "monitor"
  | "music"
  | "download"
  | "bluetooth"
  | "image"
  | "search"
  | "file"
  | "layout"
  | "panels"
  | "gauge"
  | "settings";

export interface IslandOptions {
  id?: string;
  title?: string;
  description?: string;
  status?: IslandStatus;
  duration?: number | null;
  isClosable?: boolean;
  /** 兼容 Chakra toast 传入的 variant（如 "left-accent"），灵动岛样式固定，忽略该值 */
  variant?: string;
  icon?: React.ReactNode;
  /** 预设语义图标键，优先于 status 默认图标 */
  iconKey?: IconKey;
  onCloseComplete?: () => void;
  /** 持久基线（如更新下载岛）：无自动关闭，普通提示消失后自动恢复显示 */
  persistent?: boolean;
  /** 有值时展开态渲染进度条（0-100） */
  progress?: number;
  /** 覆盖默认点击行为（如完成态点击重启安装） */
  onClick?: () => void;
  /** 音乐播放灵动岛：渲染专辑封面 + 音量波动，悬停展开更高的播放控制布局 */
  kind?: "music";
}

interface IslandItem {
  id: string;
  title?: string;
  description?: string;
  status: IslandStatus;
  duration: number | null;
  isClosable?: boolean;
  icon?: React.ReactNode;
  iconKey?: IconKey;
  onCloseComplete?: () => void;
  persistent?: boolean;
  progress?: number;
  onClick?: () => void;
  kind?: "music";
  timer?: ReturnType<typeof setTimeout>;
}

/* ------------------------------------------------------------------ */
/* 提示文案精简：灵动岛只显示核心动作短语，去掉冗长解释，保持简洁。    */
/* 例如「未找到运行中的 ACE 进程」→「未找到」，「已限制 N 个进程」→「已限制」 */
/* ------------------------------------------------------------------ */
const SHORT_VERBS = [
  "未找到",
  "未检测到",
  "未发现",
  "已限制",
  "已优化",
  "已恢复",
  "已启用",
  "已禁用",
  "已清除",
  "已清理",
  "已设置",
  "已应用",
  "已暂停",
  "已更新",
  "已重置",
  "已关闭",
  "已开启",
  "已锁定",
  "已解锁",
  "已添加",
  "已删除",
  "已保存",
  "已安装",
  "已复制",
  "已上传",
  "已下载",
  "已提升",
  "已分配",
  "导入失败",
  "应用失败",
];

function shortenMessage(text?: string): string | undefined {
  if (!text) return text;
  const t0 = text.trim();
  if (!t0) return t0;
  let t = t0;

  // 0) 盘符前缀：如 "C: 优化(TRIM)完成" → 保留盘符，精简剩余部分 → "C 已优化"
  //    （避免盘符冒号被当成句号截断，导致只显示 "C"）
  const driveMatch = t.match(/^([A-Za-z]):\s*(.*)$/);
  if (driveMatch && driveMatch[2].trim()) {
    const rest = driveMatch[2].trim();
    const shortened = shortenVerb(rest);
    if (shortened !== rest) {
      return `${driveMatch[1]} ${shortened}`;
    }
    return t;
  }

  return shortenVerb(t);
}

function shortenVerb(t: string): string {
  // 1) 已知动作动词开头 → 只保留动词本身（未找到 / 已限制 / 已优化…）
  for (const v of SHORT_VERBS) {
    if (t.startsWith(v)) return v;
  }

  // 2) 「需要管理员权限」类长句 → 统一为短提示
  if (t.includes("需要管理员权限")) return "需要管理员权限";

  // 3) 特定长句映射为短词
  if (t.includes("应用注册表强制限制")) return "已限制";
  if (t.includes("IFEO 强制优先级")) return t.includes("清除") ? "已清除" : "已应用";
  if (t.includes("分配指定核心")) return "已分配";

  // 4) 完成/成功态 → 提取动作词，如 "优化(TRIM)完成" → "已优化"、"Optimization complete" → "Optimized"
  if (/完成|成功/.test(t)) {
    if (/优化|整理/.test(t)) return "已优化";
    if (/恢复/.test(t)) return "已恢复";
    if (/清理/.test(t)) return "已清理";
    if (/清除/.test(t)) return "已清除";
    if (/更新/.test(t)) return "已更新";
    if (/安装/.test(t)) return "已安装";
    if (/下载/.test(t)) return "已下载";
    if (/上传/.test(t)) return "已上传";
    if (/应用/.test(t)) return "已应用";
    if (/保存/.test(t)) return "已保存";
    if (/复制/.test(t)) return "已复制";
    if (/重置/.test(t)) return "已重置";
  } else if (/complete|done|finished|successful/i.test(t)) {
    const lower = t.toLowerCase();
    if (/optimiz|trim|defrag/.test(lower)) return "Optimized";
    if (/restor/.test(lower)) return "Restored";
    if (/clean/.test(lower)) return "Cleaned";
    if (/clear/.test(lower)) return "Cleared";
    if (/updat/.test(lower)) return "Updated";
    if (/install/.test(lower)) return "Installed";
    if (/download/.test(lower)) return "Downloaded";
    if (/upload/.test(lower)) return "Uploaded";
    if (/appl/.test(lower)) return "Applied";
    if (/sav/.test(lower)) return "Saved";
    if (/cop/.test(lower)) return "Copied";
    if (/reset/.test(lower)) return "Reset";
  }

  // 5) 截断到第一句主干：去掉「。」/「，」后的解释、冒号后的细节、尾部括号说明
  for (const sep of ["。", "，", "：", ":", "（", "("]) {
    const idx = t.indexOf(sep);
    if (idx > 0) {
      t = t.slice(0, idx).trim();
      break;
    }
  }

  return t;
}

/* ------------------------------------------------------------------ */
/* 模块级全局 store：只保留单个灵动岛，新提示替换旧提示                */
/* ------------------------------------------------------------------ */
let current: IslandItem | null = null;
// 持久基线（如更新下载岛）：普通提示显示时被覆盖，提示关闭后自动恢复
let pending: IslandItem | null = null;
let revision = 0;
const listeners = new Set<() => void>();
let idSeed = 0;
// getSnapshot 必须返回缓存引用（仅 store 变化时替换），否则 useSyncExternalStore 会无限重渲染
let cachedSnapshot: { item: IslandItem | null; revision: number } = { item: null, revision: 0 };

function emit() {
  cachedSnapshot = { item: current, revision };
  for (const l of listeners) l();
}

function buildItem(options: IslandOptions): IslandItem {
  const id = options.id ?? `island-${++idSeed}`;
  return {
    id,
    // 保留原始完整文案：折叠时显示精简版，悬停展开时显示详情
    title: options.title,
    description: options.description,
    status: options.status ?? "info",
    // 显式 null = 不自动关闭（如持久更新岛）；undefined 才用默认 2000ms
    duration: options.duration === null ? null : options.duration ?? 2000,
    isClosable: options.isClosable,
    icon: options.icon,
    iconKey: options.iconKey,
    onCloseComplete: options.onCloseComplete,
    persistent: options.persistent,
    progress: options.progress,
    onClick: options.onClick,
    kind: options.kind,
  };
}

function show(options: IslandOptions) {
  const next = buildItem(options);
  if (current) revision++; // 已有提示，触发「缩小→换内容→扩散」
  if (current?.timer) clearTimeout(current.timer);
  current = next;
  if (typeof next.duration === "number" && next.duration > 0) {
    next.timer = setTimeout(() => close(next.id), next.duration);
  }
  emit();
}

function update(id: string, options: IslandOptions) {
  show({ ...options, id });
}

/** 显示持久基线（无自动关闭）。若当前正显示普通提示则不覆盖，等待其关闭后由 close() 恢复 */
function showPersistent(options: IslandOptions) {
  const next = buildItem({ ...options, duration: null });
  pending = next;
  if (!current) {
    revision++;
    current = next;
    emit();
  } else if (current.id === next.id) {
    // 当前已是同 id 基线：直接换内容（触发 replace 动画）
    revision++;
    if (current.timer) clearTimeout(current.timer);
    current = next;
    emit();
  }
}

/**
 * 更新持久基线。animate=true 触发 replace 动画（状态切换：下载中→完成）；
 * animate=false 静默更新（进度实时刷新，避免每 200ms 缩→扩动画）。
 * 当前正显示普通提示时仅暂存 pending，待其关闭后恢复。
 */
function updatePersistent(id: string, options: IslandOptions, animate = true) {
  if (!pending || pending.id !== id) return;
  const next = { ...pending, ...options, id };
  pending = next;
  if (current?.id === id) {
    if (animate) revision++;
    if (current.timer) clearTimeout(current.timer);
    current = next;
    emit();
  }
}

/** 关闭持久基线：清除 pending；若正显示基线则淡出，否则不影响当前普通提示 */
function closePersistent(id: string) {
  if (!pending || pending.id !== id) return;
  pending = null;
  if (current?.id === id) {
    current = null;
    revision++;
    emit();
  }
}

function close(id: string) {
  if (!current || current.id !== id) return;
  if (current.timer) clearTimeout(current.timer);
  const cb = current.onCloseComplete;
  // 若存在持久基线且被关闭的不是它 → 恢复基线显示（replace 动画）
  if (pending && pending.id !== id) {
    revision++;
    current = pending;
  } else {
    current = null;
  }
  emit();
  cb?.();
}

function closeAll() {
  if (current?.timer) clearTimeout(current.timer);
  // 清空普通提示后恢复持久基线；当前已是基线或无变化时不重复触发动画
  if (pending && current?.id !== pending.id) {
    revision++;
    current = pending;
    emit();
  } else if (!pending && current) {
    current = null;
    revision++;
    emit();
  }
}

function isActive(id: string) {
  return current?.id === id;
}

/** 悬停展开时暂停自动关闭 */
function hold(id: string) {
  if (!current || current.id !== id) return;
  if (current.timer) clearTimeout(current.timer);
  current.timer = undefined;
}

/** 移开后恢复自动关闭计时 */
function extend(id: string) {
  if (!current || current.id !== id) return;
  if (current.timer) clearTimeout(current.timer);
  if (typeof current.duration === "number" && current.duration > 0) {
    current.timer = setTimeout(() => close(current.id), current.duration);
  }
}

function subscribe(cb: () => void) {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

function getSnapshot() {
  return cachedSnapshot;
}

export type DynamicIslandToast = ((options: IslandOptions) => void) & {
  update: typeof update;
  isActive: typeof isActive;
  close: typeof close;
  closeAll: typeof closeAll;
  showPersistent: typeof showPersistent;
  updatePersistent: typeof updatePersistent;
  closePersistent: typeof closePersistent;
};

/** 返回与 Chakra useToast 兼容的调用签名，旧 toast({...}) 调用体无需改动。
 *  传入 defaultIcon 后，该页面的成功/信息提示会自动带上对应语义图标。 */
export function useDynamicIsland(defaultIcon?: IconKey): DynamicIslandToast {
  return useMemo(() => makeToast(defaultIcon), [defaultIcon]);
}

function makeToast(defaultIcon?: IconKey): DynamicIslandToast {
  const resolveIcon = (options: IslandOptions): IslandOptions => {
    if (options.icon || options.iconKey) return options;
    if (defaultIcon && (options.status === "success" || options.status === "info")) {
      return { ...options, iconKey: defaultIcon };
    }
    return options;
  };
  const fn = ((options: IslandOptions) => show(resolveIcon(options))) as DynamicIslandToast;
  fn.update = (id, options) => update(id, resolveIcon(options));
  fn.isActive = isActive;
  fn.close = close;
  fn.closeAll = closeAll;
  fn.showPersistent = (options) => showPersistent(resolveIcon(options));
  fn.updatePersistent = (id, options, animate) => updatePersistent(id, resolveIcon(options), animate);
  fn.closePersistent = closePersistent;
  return fn;
}

/* ------------------------------------------------------------------ */
/* 状态图标：自绘 SVG（渐变 + 发光），info/loading 复用品牌主题色        */
/* ------------------------------------------------------------------ */
function StatusIcon({
  status,
  iconKey,
  primaryColor,
  size = 22,
}: {
  status: IslandStatus;
  iconKey?: IconKey;
  primaryColor: string;
  size?: number;
}) {
  const gid = useId().replace(/:/g, "");
  const palette: Record<IslandStatus, { from: string; to: string }> = {
    success: { from: "#34d399", to: "#10b981" },
    error: { from: "#f87171", to: "#ef4444" },
    warning: { from: "#fbbf24", to: "#f59e0b" },
    info: { from: primaryColor, to: primaryColor },
    loading: { from: primaryColor, to: primaryColor },
    blue: { from: "#3b82f6", to: "#22d3ee" },
  };
  const p = palette[status];
  const glowColor = status === "info" || status === "loading" ? `${primaryColor}55` : `${p.to}55`;

  return (
    <div
      style={{
        position: "relative",
        width: size,
        height: size,
        borderRadius: 999,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        boxShadow: `0 0 14px 0 ${glowColor}, inset 0 1px 0 rgba(255,255,255,0.35)`,
      }}
    >
      <svg width={size} height={size} viewBox="0 0 36 36">
        <defs>
          <linearGradient id={`ig-${gid}`} x1="0" y1="0" x2="1" y2="1">
            <stop offset="0%" stopColor={p.from} stopOpacity="0.8" />
            <stop offset="100%" stopColor={p.to} stopOpacity="1" />
          </linearGradient>
        </defs>
        <circle cx="18" cy="18" r="18" fill={`url(#ig-${gid})`} />
        {!iconKey && <StatusSymbol status={status} />}
      </svg>
      {iconKey && (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            pointerEvents: "none",
          }}
        >
          <SemanticIcon iconKey={iconKey} />
        </div>
      )}
      {status === "loading" && (
        <motion.svg
          width={size * 0.55}
          height={size * 0.55}
          viewBox="0 0 24 24"
          style={{ position: "absolute", inset: 0, margin: "auto" }}
          animate={{ rotate: 360 }}
          transition={{ repeat: Infinity, duration: 0.9, ease: "linear" }}
        >
          <circle cx="12" cy="12" r="9" fill="none" stroke="rgba(255,255,255,0.35)" strokeWidth="2.6" />
          <path
            d="M12 3a9 9 0 0 1 8.4 5.2"
            fill="none"
            stroke="#fff"
            strokeWidth="2.6"
            strokeLinecap="round"
          />
        </motion.svg>
      )}
    </div>
  );
}

function StatusSymbol({ status }: { status: IslandStatus }) {
  if (status === "success")
    return <path d="M12.2 18.2 16.3 22.4 23.6 14.6" fill="none" stroke="#fff" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />;
  if (status === "error")
    return <g><path d="M18 12.2v7.6" stroke="#fff" strokeWidth="2.8" strokeLinecap="round" /><circle cx="18" cy="22.6" r="1.4" fill="#fff" /></g>;
  if (status === "warning")
    return <g><path d="M18 10.4 25.6 22.6H10.4Z" fill="none" stroke="#fff" strokeWidth="2.2" strokeLinejoin="round" /><path d="M18 15.2v4.2" stroke="#fff" strokeWidth="2.4" strokeLinecap="round" /><circle cx="18" cy="21.8" r="1.3" fill="#fff" /></g>;
  if (status === "info")
    return <g><circle cx="18" cy="13.2" r="1.4" fill="#fff" /><path d="M17.1 17.4h1.8v5.4h-1.8z" fill="#fff" /></g>;
  if (status === "blue")
    return <g><circle cx="18" cy="13.2" r="1.4" fill="#fff" /><path d="M17.1 17.4h1.8v5.4h-1.8z" fill="#fff" /></g>;
  return null;
}

function SemanticIcon({ iconKey }: { iconKey: IconKey }) {
  const p = { size: 15, strokeWidth: 2.4, color: "#fff" as const, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (iconKey) {
    case "power":     return <Zap {...p} />;
    case "memory":    return <MemoryStick {...p} />;
    case "network":   return <Globe {...p} />;
    case "cpu":       return <Cpu {...p} />;
    case "layers":    return <Layers {...p} />;
    case "disk":      return <HardDrive {...p} />;
    case "audio":     return <AudioWaveform {...p} />;
    case "gamepad":   return <Gamepad2 {...p} />;
    case "rocket":    return <Rocket {...p} />;
    case "sparkles":  return <Sparkles {...p} />;
    case "shield":    return <ShieldCheck {...p} />;
    case "target":    return <Crosshair {...p} />;
    case "mouse":     return <Mouse {...p} />;
    case "gpu":       return <Cpu {...p} />;
    case "filter":    return <SlidersHorizontal {...p} />;
    case "wrench":    return <Wrench {...p} />;
    case "monitor":   return <Monitor {...p} />;
    case "music":     return <Music {...p} />;
    case "download":  return <Download {...p} />;
    case "bluetooth": return <Bluetooth {...p} />;
    case "image":     return <Image {...p} />;
    case "search":    return <Search {...p} />;
    case "file":      return <FileText {...p} />;
    case "layout":    return <LayoutGrid {...p} />;
    case "panels":    return <PanelsTopLeft {...p} />;
    case "gauge":     return <Gauge {...p} />;
    case "settings":  return <Settings {...p} />;
    default: return null;
  }
}

/* ------------------------------------------------------------------ */
/* 音乐灵动岛音量波动：【不触碰正在播放的 <audio>】                    */
/* createMediaElementSource 会接管音频元素输出，且 WebView 的          */
/* AudioContext 未激活时会导致静音，严重破坏播放。因此改为纯前端       */
/* 驱动的平滑波动动画：播放期间跳动，暂停时归零（仅作视觉「音量波动」）。 */
/* ------------------------------------------------------------------ */
function useSongLevels(playing: boolean): number[] {
  const BAR_COUNT = 5;
  const rafRef = useRef<number>(0);
  const [levels, setLevels] = useState<number[]>(() => Array(BAR_COUNT).fill(0));

  useEffect(() => {
    if (!playing) {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
      setLevels(Array(BAR_COUNT).fill(0));
      return;
    }
    let t = 0;
    const tick = () => {
      // 多正弦叠加+轻微噪声，得到连续、起伏不乱的「音量波动」
      t += 0.06;
      const next: number[] = [];
      for (let i = 0; i < BAR_COUNT; i++) {
        const envelope = 0.5 + 0.5 * Math.sin(t * 1.7 + i * 1.1) * Math.sin(t * 0.9 + i * 0.7);
        const ripple = 0.4 * Math.sin(t * 3.1 + i * 2.3);
        next.push(Math.max(0.06, Math.min(1, envelope * 0.7 + ripple * 0.5)));
      }
      setLevels(next);
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      rafRef.current = 0;
    };
  }, [playing]);

  return levels;
}

function formatMusicTime(sec: number): string {
  if (!isFinite(sec) || sec < 0) return "0:00";
  const s = Math.floor(sec % 60);
  const m = Math.floor(sec / 60);
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/** 音乐播放灵动岛内容：折叠为「封面 + 音量波动」，展开为更高的播放控制布局。
 *  支持两种来源：NexBox 内部播放器（currentSong/audioRef）或外部客户端（SMTC 接管，
 *  优先显示外部客户端）。 */
function MusicIslandContent({ expanded, expandedVisible, foldVisible }: { expanded: boolean; expandedVisible: boolean; foldVisible: boolean }) {
  const navigate = useNavigate();
  const currentSong = useMusicStore((s) => s.currentSong);
  const isPlaying = useMusicStore((s) => s.isPlaying);
  // 外部客户端播放（SMTC 接管），非空时优先级高于内部播放器
  const externalTrack = useMusicStore((s) => s.externalTrack);
  const externalPlaying = useMusicStore((s) => s.externalPlaying);
  const externalPositionMs = useMusicStore((s) => s.externalPositionMs);
  const externalDurationMs = useMusicStore((s) => s.externalDurationMs);
  const isExternal = Boolean(externalTrack?.title) && !(currentSong && (isPlaying || !externalPlaying));
  const proxyPort = useMusicStore((s) => s.proxyPort);
  const audioRef = useMusicStore((s) => s.audioRef);
  const actionsRef = useRef(useMusicStore.getState());
  const playing = isExternal ? externalPlaying : isPlaying;
  const levels = useSongLevels(playing);

  const [time, setTime] = useState(0);
  useEffect(() => {
    const a = audioRef;
    if (!a) return;
    const upd = () => {
      const cur = a.currentTime || 0;
      setTime(cur);
      // 拖动 seek 后：音频实际到达目标附近才放开「钉住」的位置
      const target = pendingSeekTargetRef.current;
      if (target != null && Math.abs(cur - target) < 1.5) {
        pendingSeekTargetRef.current = null;
        if (pendingTimerRef.current) clearTimeout(pendingTimerRef.current);
        pendingTimerRef.current = null;
        setDragFrac(null);
      }
    };
    upd();
    a.addEventListener("timeupdate", upd);
    a.addEventListener("play", upd);
    return () => {
      a.removeEventListener("timeupdate", upd);
      a.removeEventListener("play", upd);
      if (pendingTimerRef.current) clearTimeout(pendingTimerRef.current);
    };
  }, [audioRef]);

  const [coverFailed, setCoverFailed] = useState(false);
  useEffect(() => setCoverFailed(false), [currentSong?.cover, externalTrack?.cover]);
  const cover = isExternal
    ? (externalTrack?.cover ?? "")
    : currentSong ? (currentSong.cover.startsWith("data:") ? currentSong.cover : coverProxyUrl(currentSong.cover, proxyPort)) : "";

  // 时长兜底：部分客户端（如网易云）经系统媒体会话不给 EndTime（duration_ms=0），
  // 导致无法渲染进度条。此时用网易云搜索按「歌名+歌手」查一次补总时长（每曲只查一次）。
  const [extDurationLookup, setExtDurationLookup] = useState(0);
  const queriedKeyRef = useRef("");
  useEffect(() => {
    const trackKey = `${externalTrack?.title ?? ""}|${externalTrack?.artist ?? ""}`;
    if (trackKey !== queriedKeyRef.current) {
      queriedKeyRef.current = trackKey;
      setExtDurationLookup(0);
      if (isExternal && externalTrack?.title && externalDurationMs <= 0 && trackKey !== "|") {
        let cancelled = false;
        (async () => {
          try {
            const res = await invoke<Song[]>("music_search", {
              keywords: `${externalTrack.title} ${externalTrack.artist}`.trim(),
              limit: 1,
            });
            const hit = (res || [])[0];
            if (!cancelled && hit?.duration) setExtDurationLookup(hit.duration);
          } catch {
            // 忽略：查不到时长时进度条暂以不显示处理
          }
        })();
        return () => {
          cancelled = true;
        };
      }
    }
  }, [isExternal, externalTrack?.title, externalTrack?.artist, externalDurationMs]);

  const titleColor = useColorModeValue("#1a1a1a", "#ffffff");
  const descColor = useColorModeValue("rgba(0,0,0,0.62)", "rgba(255,255,255,0.66)");
  const { config } = useThemeColor();
  const primaryColor = config.primaryColor;
  const barTrack = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.14)");

  // 进度条拖动跟随：用 window 级 mousemove，拖出元素也按鼠标真实位置计算
  const [dragFrac, setDragFrac] = useState<number | null>(null);
  const dragRef = useRef(false);
  const dragFracRef = useRef<number | null>(null);
  const dragElRef = useRef<HTMLElement | null>(null);
  const barRef = useRef<HTMLDivElement | null>(null);
  const durationRef = useRef(0);
  const applyDrag = useCallback((ev: MouseEvent) => {
    const el = dragElRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const f = Math.max(0, Math.min(1, (ev.clientX - rect.left) / rect.width));
    dragFracRef.current = f;
    setDragFrac(f);
  }, []);
  const startDrag = useCallback((el: HTMLElement, ev: React.MouseEvent) => {
    dragRef.current = true;
    dragElRef.current = el;
    applyDrag(ev.nativeEvent);
  }, [applyDrag]);
  // 内部拖动 seek 后的「钉住」逻辑：松手先停在拖到的位置，音频跳到目标附近再放开，避免回弹闪烁
  const pendingSeekTargetRef = useRef<number | null>(null);
  const pendingTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const clearPendingSeek = useCallback(() => {
    pendingSeekTargetRef.current = null;
    if (pendingTimerRef.current) {
      clearTimeout(pendingTimerRef.current);
      pendingTimerRef.current = null;
    }
    setDragFrac(null);
  }, []);
  const endDrag = useCallback(() => {
    if (!dragRef.current) return;
    dragRef.current = false;
    const f = dragFracRef.current;
    dragFracRef.current = null;
    dragElRef.current = null;
    if (f != null) {
      if (isExternal) {
        // 外部进度条目前只读，正常不会走到这里；兜底放开并照常发送 seek
        setDragFrac(null);
        actionsRef.current.externalControl("seek", Math.round(f * externalDurationMs));
      } else {
        // 内部：松手后先钉在拖到的位置，等音频 timeupdate 真正跳到目标附近再放开，
        // 避免松手瞬间进度条回弹一下再跳到目标。
        const target = f * (durationRef.current || 0);
        actionsRef.current.seekTo(target);
        pendingSeekTargetRef.current = target;
        if (pendingTimerRef.current) clearTimeout(pendingTimerRef.current);
        pendingTimerRef.current = setTimeout(clearPendingSeek, 2000);
      }
    } else {
      setDragFrac(null);
    }
  }, [isExternal, externalDurationMs, clearPendingSeek]);
  useEffect(() => {
    const move = (ev: MouseEvent) => { if (dragRef.current) applyDrag(ev); };
    const up = () => endDrag();
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    window.addEventListener("mouseleave", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      window.removeEventListener("mouseleave", up);
    };
  }, [applyDrag, endDrag]);

  if (!currentSong && !externalTrack?.title) return null;

  // 进度/时长：内部走 audioRef、外部走 SMTC 轮询的毫秒值
  const displayTimeSec = isExternal ? externalPositionMs / 1000 : time;
  const durationSec = isExternal
    ? (externalDurationMs || extDurationLookup) / 1000
    : (useMusicStore.getState().duration || audioRef?.duration || currentSong.duration || 0);
  durationRef.current = durationSec;
  const progress = durationSec > 0 ? displayTimeSec / durationSec : 0;
  const fillPct = Math.max(0, Math.min(100, (dragFrac != null ? dragFrac : progress) * 100));
  // 单一共享封面元素 + 单一共享歌名元素：opacity 恒为 1，位置/大小都由 framer motion
  // 数值插值(x/y/scale)直接放大缩小到展开/折叠位置，不再用淡入淡出切换位置。
  const coverPos = expanded
    ? { x: 16, y: 12, scale: 1 }  // 展开：左上、与歌名齐顶，放大到 48
    : { x: 10, y: 3, scale: 0.5 }; // 折叠：左侧垂直居中缩到 24
  const namePos = expanded
    ? { x: 74, y: 12, scale: 1 }   // 展开：封面右侧、与封面下移对齐，放大到 15px
    : { x: 44, y: 8, scale: 0.73 }; // 折叠：封面右侧、垂直居中，缩小到 ≈11px

  return (
    <div style={{ flex: 1, minWidth: 0, position: "relative", alignSelf: "stretch" }}>
      {/* 共享封面（唯一，不淡化） */}
      <motion.div
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          width: 48,
          height: 48,
          zIndex: 1,
          opacity: 1,
          pointerEvents: "none",
          transformOrigin: "left top",
        }}
        animate={{ x: coverPos.x, y: coverPos.y, scale: coverPos.scale }}
        transition={{ duration: 0.3, ease: [0.22, 0.61, 0.36, 1] }}
      >
        <CoverThumb cover={cover} failed={coverFailed} onError={() => setCoverFailed(true)} size={48} primaryColor={primaryColor} />
      </motion.div>

      {/* 共享歌名（唯一，不淡化）：直接放大缩小到展开/折叠位置 */}
      <motion.div
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          zIndex: 1,
          pointerEvents: "none",
          transformOrigin: "left top",
          fontSize: 15,
          lineHeight: 1.2,
          fontWeight: 700,
          color: titleColor,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
          maxWidth: expanded ? 224 : 118,
        }}
        animate={{ x: namePos.x, y: namePos.y, scale: namePos.scale }}
        transition={{ duration: 0.3, ease: [0.22, 0.61, 0.36, 1] }}
      >
        {isExternal ? externalTrack?.title : currentSong?.name}
      </motion.div>

      {/* 波形：仅折叠态显示，透明淡入淡出 */}
      <div
        style={{
          position: "absolute",
          right: 10,
          top: 7,
          zIndex: 1,
          display: "flex",
          alignItems: "center",
          gap: 2.5,
          height: 16,
          opacity: foldVisible ? 1 : 0,
          transition: "opacity 0.18s",
          pointerEvents: "none",
        }}
      >
        {levels.map((v, i) => (
          <div
            key={i}
            style={{
              width: 2.5,
              height: Math.max(3, Math.round(v * 13)),
              borderRadius: 999,
              background: titleColor,
              opacity: 0.85,
              transition: "height 90ms linear",
            }}
          />
        ))}
      </div>

      {/* 展开内容：歌手 + 进度条 + 控制按钮（歌名由共享元素提供） */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          flexDirection: "column",
          gap: 12,
          padding: "32px 16px 10px",
          opacity: expanded && expandedVisible ? 1 : 0,
          transition: "opacity 0.18s",
          pointerEvents: expanded && expandedVisible ? "auto" : "none",
        }}
      >
        {/* 行1：歌手（在共享歌名下方，封面右侧） */}
        <div style={{ marginLeft: 60, fontSize: 12, fontWeight: 500, color: descColor, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
          {isExternal ? (externalTrack?.artist || "未知歌手") : (currentSong?.artist || currentSong?.artists?.map((a) => a.name).join(" / ") || "未知歌手")}
        </div>
        {/* 行2：有可用的总时长 → 进度条（左时间+进度+右总长，支持拖拽）；
          外部播放拿不到时长（如网易云不给 EndTime）→ 用音量波动条代替 */}
        {isExternal && durationSec <= 0 ? (
          <div
            onClick={(e) => e.stopPropagation()}
            style={{ position: "relative", height: 22, display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 6, marginLeft: 12, marginRight: 12, minWidth: 0 }}
          >
            {Array.from({ length: 24 }).map((_, i) => {
              const v = levels[i % levels.length];
              return (
                <div
                  key={i}
                  style={{
                    width: 3,
                    height: Math.max(4, Math.round(v * 20)),
                    borderRadius: 999,
                    background: primaryColor,
                    opacity: 0.9,
                    transition: "height 90ms linear",
                  }}
                />
              );
            })}
          </div>
        ) : isExternal ? (
          <div
            onClick={(e) => e.stopPropagation()}
            style={{ position: "relative", height: 14, display: "flex", alignItems: "center", minWidth: 0, gap: 8, marginTop: 14 }}
          >
            <div style={{ fontSize: 10, fontWeight: 600, color: descColor, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", minWidth: 26 }}>
              {formatMusicTime(displayTimeSec)}
            </div>
            <div style={{ flex: 1, height: 4, borderRadius: 999, background: barTrack, overflow: "hidden" }}>
              <div style={{ height: "100%", borderRadius: 999, background: primaryColor, width: `${fillPct}%` }} />
            </div>
            <div style={{ fontSize: 10, fontWeight: 600, color: descColor, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", minWidth: 26, textAlign: "right" }}>
              {formatMusicTime(durationSec)}
            </div>
          </div>
        ) : (
          <div
            onMouseDown={(e) => {
              e.preventDefault();
              e.stopPropagation();
              startDrag(barRef.current as HTMLElement, e); // 用进度条本身的 rect，避免左右空白
            }}
            onClick={(e) => e.stopPropagation()} // 阻断冒泡，避免点击/拖动触发灵动岛收起
            style={{ position: "relative", height: 14, display: "flex", alignItems: "center", cursor: "pointer", minWidth: 0, gap: 8, marginTop: 14 }}
          >
            <div style={{ fontSize: 10, fontWeight: 600, color: descColor, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", minWidth: 26 }}>
              {formatMusicTime(displayTimeSec)}
            </div>
            <div ref={barRef} style={{ flex: 1, height: 4, borderRadius: 999, background: barTrack, overflow: "hidden" }}>
              <div style={{ height: "100%", borderRadius: 999, background: primaryColor, width: `${fillPct}%` }} />
            </div>
            <div style={{ fontSize: 10, fontWeight: 600, color: descColor, fontVariantNumeric: "tabular-nums", whiteSpace: "nowrap", minWidth: 26, textAlign: "right" }}>
              {formatMusicTime(durationSec)}
            </div>
          </div>
        )}
        {/* 行3：控制按钮 —— 整宽居中、纯图标（音乐页同套）；右下角另加「打开播放器」跳转按钮 */}
        <div style={{ display: "flex", justifyContent: "center", alignItems: "center", gap: 26, position: "relative" }}>
          <ControlButton label="上一曲" onClick={(e) => { e.stopPropagation(); isExternal ? actionsRef.current.externalControl("prev") : actionsRef.current.prevTrack(); }}>
            <MSkipBackIcon size={20} color={titleColor} />
          </ControlButton>
          <ControlButton label={playing ? "暂停" : "播放"} onClick={(e) => { e.stopPropagation(); isExternal ? actionsRef.current.externalControl("play-pause") : actionsRef.current.togglePlay(); }}>
            {playing ? <MPauseIcon size={22} color={titleColor} /> : <MPlayIcon size={22} color={titleColor} />}
          </ControlButton>
          <ControlButton label="下一曲" onClick={(e) => { e.stopPropagation(); isExternal ? actionsRef.current.externalControl("next") : actionsRef.current.nextTrack(); }}>
            <MSkipForwardIcon size={20} color={titleColor} />
          </ControlButton>
          {!isExternal && (
            <div style={{ position: "absolute", right: 12 }}>
              <ControlButton label="打开播放器" onClick={(e) => { e.stopPropagation(); navigate("/music", { state: { expandPlayer: true } }); }}>
                <MOpenPlayerIcon size={16} color={titleColor} />
              </ControlButton>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/* 与音乐页底部播放器一致的圆角控制图标 */
const MPlayIcon = ({ size = 22, color }: { size?: number; color: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ color }}>
    <path d="M 8 5 Q 7 4 6 5 L 6 19 Q 7 20 8 19 L 19 13 Q 20 12 19 11 Z" />
  </svg>
);
const MPauseIcon = ({ size = 22, color }: { size?: number; color: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ color }}>
    <rect x="6" y="4" width="4" height="16" rx="2" />
    <rect x="14" y="4" width="4" height="16" rx="2" />
  </svg>
);
const MSkipBackIcon = ({ size = 22, color }: { size?: number; color: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ color }}>
    <rect x="4" y="5" width="3" height="14" rx="1.5" />
    <polygon points="16,6 8,12 16,18" stroke="currentColor" strokeWidth={2.5} strokeLinejoin="round" />
  </svg>
);
const MSkipForwardIcon = ({ size = 22, color }: { size?: number; color: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" style={{ color }}>
    <polygon points="8,6 16,12 8,18" stroke="currentColor" strokeWidth={2.5} strokeLinejoin="round" />
    <rect x="17" y="5" width="3" height="14" rx="1.5" />
  </svg>
);
/** 打开全屏播放器：细线四角外扩箭头（与 MSkip 系列同风格），呼应「展开到完整播放器页」 */
const MOpenPlayerIcon = ({ size = 16, color }: { size?: number; color: string }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" style={{ color }}>
    <path d="M9 4H4v5" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
    <path d="M15 4h5v5" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
    <path d="M9 20H4v-5" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
    <path d="M15 20h5v-5" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

/** 播放控制按钮：纯图标、无圆形底、悬停无变化（与音乐页播放器一致） */
function ControlButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: (e: React.MouseEvent) => void;
  children: React.ReactNode;
}) {
  return (
    <Tooltip label={label} placement="top" hasArrow aria-label={label}>
      <button
        aria-label={label}
        onClick={onClick}
        style={{
          width: 30,
          height: 30,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          background: "transparent",
          border: "none",
          cursor: "pointer",
          padding: 0,
        }}
      >
        {children}
      </button>
    </Tooltip>
  );
}

/** 专辑封面缩略图：失败时回退为主题色音乐占位块 */
function CoverThumb({
  cover,
  failed,
  onError,
  size,
  primaryColor,
}: {
  cover: string;
  failed: boolean;
  onError: () => void;
  size: number;
  primaryColor: string;
}) {
  const { getContrastTextColor } = useThemeColor();
  if (!cover || failed) {
    return (
      <div
        style={{
          width: size,
          height: size,
          borderRadius: Math.max(4, size * 0.18),
          background: `linear-gradient(135deg, ${primaryColor}, ${primaryColor}88)`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        <Music size={size * 0.5} color={getContrastTextColor()} strokeWidth={2.4} />
      </div>
    );
  }
  return (
    <img
      src={cover}
      alt=""
      onError={onError}
      style={{ width: size, height: size, borderRadius: Math.max(4, size * 0.18), objectFit: "cover", flexShrink: 0, background: "rgba(0,0,0,0.12)" }}
    />
  );
}

/* ------------------------------------------------------------------ */
/* Host：单个灵动岛，置于顶部标题栏拖拽区内。                           */
/* 动画：淡化出圆形 → 向两侧扩散成胶囊 → 缩回圆形 → 淡出；             */
/*       多条触发时先缩成圆形、换内容、再扩散成胶囊。                    */
/* 交互：悬停时横向扩散成更大的圆角长矩形，显示完整详细文案，          */
/*       移开后直接收起为小胶囊（不经过圆形），悬停期间暂停自动关闭。    */
/*       展开态点击收起为小胶囊，折叠态点击关闭。                        */
/* ------------------------------------------------------------------ */
const EXPANDED_WIDTH = 320;
const COLLAPSED_HEIGHT = 30;
const MUSIC_COLLAPSED_HEIGHT = 30;
const MUSIC_COLLAPSED_WIDTH = 200;
const MUSIC_EXPANDED_HEIGHT = 150;

export function DynamicIslandHost() {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot);
  const { item, revision } = snapshot;
  const { config } = useThemeColor();
  const primaryColor = config.primaryColor;
  const currentSong = useMusicStore((s) => s.currentSong);
  const externalTrack = useMusicStore((s) => s.externalTrack);
  const isPlaying = useMusicStore((s) => s.isPlaying);
  const externalPlaying = useMusicStore((s) => s.externalPlaying);

  // 全局注册「外部客户端播放状态」监听（灵动岛常驻，不依赖音乐页是否打开）。
  // 后端 SMTC 每 1s 推送一次；有外部音乐播放则写入 store，供播放动岛接管显示。
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    (async () => {
      unlisten = await listen<ExternalPlayback | null>("external-player:state", (event) => {
        const p = event.payload;
        if (disposed) return;
        if (p && p.track) {
          useMusicStore.setState({
            externalTrack: p.track,
            externalPlaying: p.isPlaying,
            externalPositionMs: p.positionMs ?? 0,
            externalDurationMs: p.durationMs ?? 0,
          });
        } else {
          useMusicStore.setState({
            externalTrack: null,
            externalPlaying: false,
            externalPositionMs: 0,
            externalDurationMs: 0,
          });
        }
      });
    })();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const controls = useAnimationControls();
  const [displayed, setDisplayed] = useState<IslandItem | null>(null);
  const [textVisible, setTextVisible] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [expandedTextVisible, setExpandedTextVisible] = useState(false);
  const [foldVisible, setFoldVisible] = useState(true); // 音乐岛折叠文字是否可见（初始=true）
  const textRef = useRef<HTMLDivElement>(null);
  const [textOverflowing, setTextOverflowing] = useState(false);

  const itemRef = useRef<IslandItem | null>(item);
  const visibleRef = useRef(false);
  const expandedRef = useRef(false);
  const lastRevisionRef = useRef(revision);
  const chainRef = useRef<Promise<void>>(Promise.resolve());
  const islandRef = useRef<HTMLDivElement | null>(null);
  const lastMusicKeyRef = useRef<string>("");

  useEffect(() => {
    itemRef.current = item;
  }, [item]);

  // 音乐播放灵动岛：有歌时设为持久基线（无自动关闭），无歌时才关闭。
  // 优先级：内部正在播放 > 外部正在播放 > 内部(暂停) > 外部(暂停)；即内部在播时优先显示内部，
  // 否则「正在播放」优先。切歌/切换来源触发 replace 动画。
  // 注意：playSong 会先置 currentSong(暂停) 再异步置 isPlaying=true，若以「播放态」为展示触发，
  // 会导致同一首歌先淡入展开、再缩回扩散，看起来「加载两次才播放」。
  // 因此用「展示键(来源+曲目)」去重：仅当实际展示的曲目/来源变化时才触发 showPersistent 动画；
  // 仅 play/pause 翻转只实时驱动波形（useSongLevels），不再重复缩→扩动画。
  useEffect(() => {
    const internalActive = Boolean(currentSong) && (isPlaying || !externalPlaying);
    const useExternal = Boolean(externalTrack?.title) && !internalActive;
    const key = useExternal
      ? `ext:${externalTrack!.title}`
      : currentSong
        ? `int:${currentSong.id}:${currentSong.name}`
        : "none";
    if (key === lastMusicKeyRef.current) return;
    lastMusicKeyRef.current = key;
    if (key === "none") {
      closePersistent("music");
    } else {
      showPersistent({
        id: "music",
        kind: "music",
        duration: null,
        iconKey: "music",
        title: useExternal ? externalTrack!.title : currentSong!.name,
      });
    }
  }, [currentSong, isPlaying, externalPlaying, externalTrack?.title]);

  // 静默实时更新（如下载进度）：同 id 时同步 displayed 内容，不触发动画
  useEffect(() => {
    if (item && displayed?.id === item.id) {
      setDisplayed(item);
    }
  }, [item, displayed?.id]);

  // 文字显示不全时用右端淡出代替省略号：实时测量是否溢出
  useEffect(() => {
    const el = textRef.current;
    if (!el) return;
    const check = () => setTextOverflowing(el.scrollWidth > el.clientWidth + 1);
    check();
    const ro = new ResizeObserver(check);
    ro.observe(el);
    return () => ro.disconnect();
  }, [displayed, textVisible, expanded]);

  const run = useCallback((fn: () => Promise<void>) => {
    chainRef.current = chainRef.current.then(fn).catch(() => {});
  }, []);

  const playAppear = useCallback(async () => {
    if (visibleRef.current) return;
    const next = itemRef.current;
    if (!next) return;
    visibleRef.current = true;
    expandedRef.current = false;
    setExpanded(false);
    setExpandedTextVisible(false);
    setDisplayed(next);
    setTextVisible(false);
    if (next.kind === "music") setFoldVisible(false); // 圆形阶段隐藏波形
    // 1. 淡化出圆形（图标已直接显示在圆形上）
    const baseHeight = itemRef.current?.kind === "music" ? MUSIC_COLLAPSED_HEIGHT : COLLAPSED_HEIGHT;
    await controls.start({ opacity: 1, width: 30, height: baseHeight });
    if (!visibleRef.current || !itemRef.current) return;
    // 2. 文字浮现，向两侧扩散成胶囊
    setTextVisible(true);
    const targetWidth = itemRef.current?.kind === "music" ? MUSIC_COLLAPSED_WIDTH : "auto";
    await controls.start({ width: targetWidth });
    if (targetWidth === MUSIC_COLLAPSED_WIDTH) setFoldVisible(true); // 扩散到折叠宽后显示波形
  }, [controls]);

  const playReplace = useCallback(async () => {
    const next = itemRef.current;
    if (!next) return;
    // 展开中收到新提示：直接换内容，保持展开状态
    if (expandedRef.current) {
      setDisplayed(next);
      return;
    }
    // 1. 缩成圆形（隐藏文字，图标保留在圆形上）
    setTextVisible(false);
    setFoldVisible(false); // 收窄为圆圈前隐藏波形（无论是否被普通提示替换）
    await controls.start({ width: 30 });
    if (!visibleRef.current || !itemRef.current) return;
    // 2. 换上新内容，再扩散成胶囊
    setDisplayed(next);
    setTextVisible(true);
    const targetWidth = next.kind === "music" ? MUSIC_COLLAPSED_WIDTH : "auto";
    await controls.start({ width: targetWidth });
    if (targetWidth === MUSIC_COLLAPSED_WIDTH) setFoldVisible(true); // 扩散到折叠宽后显示波形
  }, [controls]);

  const playDismiss = useCallback(async () => {
    // 1. 缩成圆形（图标保留在圆形上）
    expandedRef.current = false;
    setExpanded(false);
    setExpandedTextVisible(false);
    setTextVisible(false);
    setFoldVisible(false); // 圆形阶段隐藏波形
    await controls.start({ width: 30 });
    // 2. 淡出
    await controls.start({ opacity: 0 });
    visibleRef.current = false;
    setDisplayed(null);
  }, [controls]);

  const playExpand = useCallback(async () => {
    const next = itemRef.current;
    if (!next || expandedRef.current) return;

    // 音乐岛：折叠文字固定在顶部、不随胶囊高度移动，可即时同步展开/淡出/放大
    if (next.kind === "music") {
      expandedRef.current = true;
      setFoldVisible(false); // 折叠文字原位透明淡出（位置固定，无位移）
      setExpanded(true); // 触发封面放大（与胶囊同帧）
      await controls.start({ width: EXPANDED_WIDTH, height: MUSIC_EXPANDED_HEIGHT }); // 胶囊同步放大
      if (!expandedRef.current) return;
      setExpandedTextVisible(true); // 展开文字在最终位淡入
      setTextVisible(false);
      return;
    }

    expandedRef.current = true;
    setExpanded(true);
    // 横向扩散成圆角长矩形（折叠短文案保持可见，稍后交叉淡出）
    // 持久更新岛为单行长条（30px 高）；普通提示按内容行数决定高度
    const expandHeight =
      next.persistent ? COLLAPSED_HEIGHT : next.description ? 62 : 34;
    await controls.start({
      width: EXPANDED_WIDTH,
      height: expandHeight,
    });
    if (!expandedRef.current) return;
    // 详细内容淡入，短文案淡出
    setExpandedTextVisible(true);
    setTextVisible(false);
  }, [controls]);

  const playCollapse = useCallback(async () => {
    if (!expandedRef.current) return;

    // 音乐岛：折叠文字固定在顶部、位置不变，可即时同步淡入/收缩
    if (itemRef.current?.kind === "music") {
      expandedRef.current = false;
      setExpandedTextVisible(false); // 展开文字淡出
      setFoldVisible(true); // 折叠文字原位透明淡入（位置固定，无位移）
      setExpanded(false); // 触发：封面 framer 回缩
      await new Promise<void>((r) => requestAnimationFrame(() => r())); // 与封面动画同帧
      if (expandedRef.current) return; // 期间被重新展开则中止
      await controls.start({ width: MUSIC_COLLAPSED_WIDTH, height: MUSIC_COLLAPSED_HEIGHT }); // 胶囊收缩
      setTextVisible(true);
      return;
    }

    expandedRef.current = false;
    // 1. 详情淡出（折叠短文案暂不显示，避免缩小过程中文字移动）
    setExpandedTextVisible(false);
    // 2. 切换回折叠内容（外层宽度仍为展开宽，overflow:hidden 使内层切换不产生可见跳变）
    setExpanded(false);
    // 3. 等一帧让折叠布局提交，避免「auto」测量到展开内容宽度的歧义
    await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
    if (expandedRef.current) return; // 若期间被重新展开则中止
    // 4. 直接收缩回小灵动岛（胶囊），不再经过圆形
    const baseHeight = itemRef.current?.kind === "music" ? MUSIC_COLLAPSED_HEIGHT : COLLAPSED_HEIGHT;
    const targetWidth = itemRef.current?.kind === "music" ? MUSIC_COLLAPSED_WIDTH : "auto";
    await controls.start({ width: targetWidth, height: baseHeight });
    // 5. 收缩完成后，短文案淡化浮现（纯透明度变化，无位置移动）
    setTextVisible(true);
  }, [controls]);

  const handleMouseEnter = useCallback(() => {
    const id = itemRef.current?.id;
    if (!visibleRef.current || !id || expandedRef.current) return;
    run(playExpand);
    hold(id); // 悬停期间不自动关闭
  }, [run, playExpand]);

  const handleMouseLeave = useCallback(() => {
    const id = itemRef.current?.id;
    if (!visibleRef.current || !expandedRef.current || !id) return;
    run(playCollapse);
    extend(id); // 移开后恢复自动关闭
  }, [run, playCollapse]);

  // 兜底：展开期间监听全局鼠标移动，光标确实离开灵动岛区域（含一定边距）则自动收起。
  // 解决个别情况下 onMouseLeave/onPointerLeave 未触发导致「移走光标仍保持展开」。
  const pendingLeaveRef = useRef(false);
  useEffect(() => {
    if (!expanded) return;
    const onMove = (e: MouseEvent) => {
      if (!expandedRef.current) return;
      const el = islandRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const m = 24; // 容错边距，避免在边缘来回时误收
      const out =
        e.clientX < r.left - m ||
        e.clientX > r.right + m ||
        e.clientY < r.top - m ||
        e.clientY > r.bottom + m;
      if (out && !pendingLeaveRef.current) {
        pendingLeaveRef.current = true;
        requestAnimationFrame(() => {
          pendingLeaveRef.current = false;
          handleMouseLeave();
        });
      }
    };
    window.addEventListener("mousemove", onMove);
    return () => window.removeEventListener("mousemove", onMove);
  }, [expanded, handleMouseLeave]);

  // 点击：有自定义 onClick 直接触发（如完成态重启安装）；
  // 展开态收起为小灵动岛；持久岛折叠态点击展开（查看版本+进度）；普通提示折叠态点击关闭
  const handleClick = useCallback(() => {
    const it = itemRef.current;
    const id = it?.id;
    if (!it || !id) return;
    if (it.onClick) {
      it.onClick();
      return;
    }
    if (expandedRef.current) {
      run(playCollapse);
      extend(id); // 收起后恢复自动关闭计时
    } else if (it.persistent) {
      run(playExpand);
      hold(id); // 持久岛无自动关闭，hold 无副作用
    } else {
      close(id);
    }
  }, [run, playCollapse, playExpand]);

  useEffect(() => {
    if (item) {
      if (!visibleRef.current) {
        run(playAppear);
      } else if (revision !== lastRevisionRef.current) {
        run(playReplace);
      }
      lastRevisionRef.current = revision;
    } else if (visibleRef.current) {
      run(playDismiss);
    }
  }, [item, revision, run, playAppear, playReplace, playDismiss]);

  const pillBg = useColorModeValue("rgba(255,255,255,0.9)", "rgba(20,20,20,0.88)");
  const pillBorder = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.12)");
  const titleColor = useColorModeValue("#1a1a1a", "#ffffff");
  const descColor = useColorModeValue("rgba(0,0,0,0.62)", "rgba(255,255,255,0.66)");
  const highlight = useColorModeValue("rgba(255,255,255,0.9)", "rgba(255,255,255,0.14)");
  const barTrack = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.14)");

  // 折叠：只显示精简短文案；展开：显示完整标题 + 详细描述
  const collapsedText = displayed
    ? shortenMessage(displayed.title || displayed.description || "")
    : "";
  const detailTitle = displayed?.title || displayed?.description || "";
  const detailDesc =
    displayed?.title && displayed?.description ? displayed.description : undefined;

  return (
    <div
      style={{
        position: "fixed",
        top: 4,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 1001,
        display: "flex",
        alignItems: "center",
        pointerEvents: "none",
      }}
    >
      <motion.div
        ref={islandRef}
        animate={controls}
        initial={{ opacity: 0, width: 30, height: COLLAPSED_HEIGHT }}
        role="status"
        aria-live="polite"
        onClick={handleClick}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onPointerLeave={handleMouseLeave}
        style={{
          borderRadius: displayed?.kind === "music" ? 22 : (expanded ? 20 : 999),
          background: pillBg,
          border: `1px solid ${pillBorder}`,
          backdropFilter: "blur(20px)",
          WebkitBackdropFilter: "blur(20px)",
          overflow: "hidden",
          cursor: "pointer",
          pointerEvents: displayed ? "auto" : "none",
          display: "flex",
          alignItems: "center",
          position: "relative",
        }}
      >
        {/* 顶部高光细线，玻璃质感 */}
        <div
          style={{
            position: "absolute",
            top: 0,
            left: "6%",
            right: "6%",
            height: 1,
            background: highlight,
            opacity: 0.5,
          }}
        />
        {displayed?.kind === "music" ? (
          <MusicIslandContent expanded={expanded} expandedVisible={expandedTextVisible} foldVisible={foldVisible} />
        ) : (
        <>
        {/* 图标始终显示（圆形阶段即图标），扩散/收缩时文字淡入淡出 */}
        <div style={{ padding: "0 0 0 4px", flexShrink: 0, display: "flex" }}>
          {displayed?.icon ??
            (displayed && (
              <StatusIcon
                status={displayed.status}
                iconKey={displayed.iconKey}
                primaryColor={primaryColor}
                size={22}
              />
            ))}
        </div>
        {/* 折叠状态：精简短文案（单行） */}
        <motion.div
          animate={{ opacity: textVisible && !expanded ? 1 : 0 }}
          transition={{ duration: 0.16 }}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            minWidth: 0,
            whiteSpace: "nowrap",
            paddingRight: 10,
            width: expanded ? 0 : "auto",
            overflow: "hidden",
          }}
        >
          {/* 中间留空 */}
          <div style={{ flex: 1, minWidth: 12 }} />
          <div
            ref={textRef}
            style={{
              fontSize: 11.5,
              fontWeight: 600,
              color: titleColor,
              whiteSpace: "nowrap",
              overflow: "hidden",
              maxWidth: 120,
              minWidth: 0,
              // 溢出时右端淡出，而不是显示省略号
              maskImage: textOverflowing
                ? "linear-gradient(to right, #000 88%, transparent 100%)"
                : undefined,
              WebkitMaskImage: textOverflowing
                ? "linear-gradient(to right, #000 88%, transparent 100%)"
                : undefined,
            }}
          >
            {collapsedText}
          </div>
        </motion.div>
          {/* 展开状态：持久更新岛为单行长条（中进度条/标题 + 右版本号）；普通提示为两行标题+描述 */}
          <div
            style={{
              display: "flex",
              minWidth: 0,
              width: expanded ? "auto" : 0,
              overflow: "hidden",
              ...(displayed?.persistent
                ? { flex: expanded ? 1 : 0, alignItems: "center", padding: "0 12px 0 4px" }
                : {
                    flexDirection: "column",
                    justifyContent: "center",
                    // 无描述（如「已切换为默认」单行提示）时文字右对齐，图标在左、文字靠右
                    marginLeft: detailDesc ? undefined : "auto",
                    padding: "5px 12px 5px 4px",
                    gap: 4,
                  }),
            }}
          >
            <motion.div
              animate={{ opacity: expandedTextVisible ? 1 : 0 }}
              transition={{ duration: 0.18 }}
              style={{
                display: "flex",
                minWidth: 0,
                ...(displayed?.persistent
                  ? { flex: 1, alignItems: "center", gap: 10 }
                  : { flexDirection: "column", gap: 4 }),
              }}
            >
              {displayed?.persistent ? (
                <>
                  {/* 单行长条：中间进度条(或标题) + 百分比 + 右侧版本号 */}
                  {displayed.progress !== undefined ? (
                    <div
                      style={{
                        flex: 1,
                        height: 7,
                        borderRadius: 999,
                        background: barTrack,
                        overflow: "hidden",
                      }}
                    >
                      <div
                        style={{
                          height: "100%",
                          borderRadius: 999,
                          background: primaryColor,
                          width: `${Math.max(0, Math.min(100, displayed.progress))}%`,
                          transition: "width 0.2s",
                        }}
                      />
                    </div>
                  ) : (
                    <div
                      style={{
                        flex: 1,
                        fontSize: 12,
                        fontWeight: 700,
                        color: titleColor,
                        whiteSpace: "nowrap",
                      }}
                    >
                      {detailTitle}
                    </div>
                  )}
                  {displayed.progress !== undefined && (
                    <div
                      style={{
                        fontSize: 10.5,
                        fontWeight: 700,
                        color: titleColor,
                        fontVariantNumeric: "tabular-nums",
                      }}
                    >
                      {Math.round(displayed.progress)}%
                    </div>
                  )}
                  {detailDesc && (
                    <div
                      style={{
                        fontSize: 11,
                        fontWeight: 600,
                        color: descColor,
                        whiteSpace: "nowrap",
                        flexShrink: 0,
                      }}
                    >
                      {detailDesc}
                    </div>
                  )}
                </>
              ) : displayed?.progress !== undefined ? (
                <>
                  {/* 标题行：左侧标题 + 右侧版本号 */}
                  <div style={{ display: "flex", alignItems: "baseline", gap: 10, minWidth: 0 }}>
                    <div
                      style={{
                        fontSize: 12,
                        fontWeight: 700,
                        color: titleColor,
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                      }}
                    >
                      {detailTitle}
                    </div>
                    {detailDesc && (
                      <div
                        style={{
                          fontSize: 10.5,
                          color: descColor,
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                        }}
                      >
                        {detailDesc}
                      </div>
                    )}
                  </div>
                  {/* 进度条行：主题色填充 + 百分比 */}
                  <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
                    <div
                      style={{
                        flex: 1,
                        height: 6,
                        borderRadius: 999,
                        background: barTrack,
                        overflow: "hidden",
                      }}
                    >
                      <div
                        style={{
                          height: "100%",
                          borderRadius: 999,
                          background: primaryColor,
                          width: `${Math.max(0, Math.min(100, displayed.progress))}%`,
                          transition: "width 0.2s",
                        }}
                      />
                    </div>
                    <div
                      style={{
                        fontSize: 10.5,
                        fontWeight: 700,
                        color: titleColor,
                        fontVariantNumeric: "tabular-nums",
                      }}
                    >
                      {Math.round(displayed.progress)}%
                    </div>
                  </div>
                </>
              ) : (
                <>
                  <div
                    style={{
                      fontSize: 12,
                      fontWeight: 700,
                      color: titleColor,
                      whiteSpace: "nowrap",
                      overflow: "hidden",
                    }}
                  >
                    {detailTitle}
                  </div>
                  {detailDesc && (
                    <div
                      style={{
                        fontSize: 10.5,
                        lineHeight: 1.35,
                        color: descColor,
                        maxHeight: 30,
                        overflow: "hidden",
                      }}
                    >
                      {detailDesc}
                    </div>
                  )}
                </>
              )}
            </motion.div>
          </div>
        </>
        )}
      </motion.div>
    </div>
  );
}
