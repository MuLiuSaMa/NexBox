/**
 * 桌面歌词跨窗口通信 Hook
 *
 * 在桌面歌词窗口中运行，负责：
 * 1. 监听主窗口发来的歌词数据、时间同步、播放状态、设置
 * 2. 通过 RAF 插值实现 60fps 平滑歌词进度
 * 3. 向主窗口发送控制指令
 */

import { useEffect, useRef, useState, useCallback } from "react";
import { listen, emit, type UnlistenFn } from "@tauri-apps/api/event";
import type { Song, KaraokeLine, PlayMode } from "@/types/music";
import { Store } from "@tauri-apps/plugin-store";
import { ensureCustomFont } from "@/lib/fonts";

export interface DesktopLyricsSettings {
  fontSize: number;
  fontFamily: string;
  highlightColor: string;
  baseColor: string;
  lineCount: 1 | 2;
  isLocked: boolean;
  showTranslation: boolean;
  hideUnlockBtn: boolean;
}

const DEFAULT_SETTINGS: DesktopLyricsSettings = {
  fontSize: 36,
  fontFamily: "",
  highlightColor: "#FFD700",
  baseColor: "rgba(255,255,255,0.35)",
  lineCount: 2,
  isLocked: false,
  showTranslation: true,
  hideUnlockBtn: false,
};

export type ControlAction =
  | "play-pause"
  | "prev"
  | "next"
  | "toggle-shuffle"
  | "lock"
  | "unlock"
  | "close"
  | "volume";

export function useDesktopLyricsSync() {
  const [song, setSong] = useState<Song | null>(null);
  const [karaokeLines, setKaraokeLines] = useState<KaraokeLine[]>([]);
  const [isPlaying, setIsPlaying] = useState(false);
  const [playMode, setPlayMode] = useState<PlayMode>("list");
  const [volume, setVolumeState] = useState(0.7);
  const [settings, setSettings] = useState<DesktopLyricsSettings>(DEFAULT_SETTINGS);
  const [isLocked, setIsLocked] = useState(false);
  const [estimatedTime, setEstimatedTime] = useState(0);

  // 时间插值 refs
  const audioTimeRef = useRef(0);
  const lastSyncRef = useRef(performance.now());
  const isPlayingRef = useRef(false);
  const hasDataRef = useRef(false);

  // 初始化：从 Store 读取设置（锁状态由主窗口推送，不独立读取）
  useEffect(() => {
    (async () => {
      try {
        const store = await Store.load("music-player-settings.json");
        const fontSize = await store.get<number>("desktopLyricsFontSize");
        const fontFamily = await store.get<string>("desktopLyricsFontFamily");
        const highlightColor = await store.get<string>("desktopLyricsHighlightColor");
        const baseColor = await store.get<string>("desktopLyricsBaseColor");
        const lineCount = await store.get<1 | 2>("desktopLyricsLineCount");
        setSettings((prev) => ({
          ...prev,
          fontSize: fontSize ?? prev.fontSize,
          fontFamily: fontFamily ?? prev.fontFamily,
          highlightColor: highlightColor ?? prev.highlightColor,
          baseColor: baseColor ?? prev.baseColor,
          lineCount: lineCount ?? prev.lineCount,
          isLocked: false,
        }));
        // 不恢复锁状态：锁状态由主窗口通过 desktop-lyrics:settings 事件推送
        // 独立读取 store 会在启动时与主窗口 init() 产生竞态，导致
        // 桌面歌词未打开但解锁按钮窗口仍然出现的问题
      } catch {
        // ignore
      }
    })();
  }, []);

  // 若选中的字体是自定义字体，确保其注册到本窗口 document.fonts
  // （运行期间在主窗口导入的字体不会自动同步到歌词窗口，需按需补注册）
  useEffect(() => {
    if (!settings.fontFamily) return;
    ensureCustomFont(settings.fontFamily);
  }, [settings.fontFamily]);

  // 注册监听器 + 请求数据（合并为一次 effect，确保 listener 就绪后再 request）
  useEffect(() => {
    const unlistenFns: UnlistenFn[] = [];
    let cancelled = false;

    const setup = async () => {
      // 1. 先注册所有监听器，确保就绪
      unlistenFns.push(
        await listen<{
          song: Song | null;
          karaokeLines: KaraokeLine[];
          currentTime: number;
          isPlaying: boolean;
        }>("desktop-lyrics:data", (e) => {
          setSong(e.payload.song);
          setKaraokeLines(e.payload.karaokeLines);
          // 使用主窗口传来的当前播放时间，避免重置为 0 导致闪烁
          audioTimeRef.current = e.payload.currentTime;
          setEstimatedTime(e.payload.currentTime);
          // 注意：不更新 isPlayingRef.current，播放状态由 desktop-lyrics:state / desktop-lyrics:time 管理
          // 如果在这里设置为 false，会永久停止 RAF 插值循环（tick 检查 !isPlayingRef.current 后不再调度下一帧）
          lastSyncRef.current = performance.now();
          hasDataRef.current = true;
        })
      );

      unlistenFns.push(
        await listen<{
          currentTime: number;
          isPlaying: boolean;
        }>("desktop-lyrics:time", (e) => {
          audioTimeRef.current = e.payload.currentTime;
          lastSyncRef.current = performance.now();
          isPlayingRef.current = e.payload.isPlaying;
          if (!e.payload.isPlaying) {
            setEstimatedTime(e.payload.currentTime);
          }
        })
      );

      unlistenFns.push(
        await listen<{
          isPlaying: boolean;
          playMode: PlayMode;
          volume: number;
        }>("desktop-lyrics:state", (e) => {
          setIsPlaying(e.payload.isPlaying);
          setPlayMode(e.payload.playMode);
          setVolumeState(e.payload.volume ?? 0.7);
          isPlayingRef.current = e.payload.isPlaying;
          if (e.payload.isPlaying) {
            lastSyncRef.current = performance.now();
          }
        })
      );

      unlistenFns.push(
        await listen<DesktopLyricsSettings>("desktop-lyrics:settings", (e) => {
          setSettings(e.payload);
          setIsLocked(e.payload.isLocked);
        })
      );

      if (cancelled) {
        unlistenFns.forEach((fn) => fn());
        return;
      }

      // 2. 监听器已就绪，主动请求主窗口推送数据
      //    指数退避重试，直到收到数据为止
      const retryDelays = [0, 300, 1000, 3000];
      for (const delay of retryDelays) {
        if (cancelled || hasDataRef.current) return;
        if (delay > 0) await new Promise((r) => setTimeout(r, delay));
        if (cancelled || hasDataRef.current) return;
        emit("desktop-lyrics:request-data", {});
      }
    };

    setup();

    return () => {
      cancelled = true;
      unlistenFns.forEach((fn) => fn());
    };
  }, []);

  // RAF 时间插值 (60fps，仅播放时运行)
  useEffect(() => {
    if (!isPlaying) return;
    let rafId: number;
    let running = true;
    const tick = () => {
      if (!running || !isPlayingRef.current) return;
      const elapsed = (performance.now() - lastSyncRef.current) / 1000;
      setEstimatedTime(audioTimeRef.current + elapsed);
      rafId = requestAnimationFrame(tick);
    };
    rafId = requestAnimationFrame(tick);
    return () => {
      running = false;
      cancelAnimationFrame(rafId);
    };
  }, [isPlaying]);

  // 发送控制指令
  const sendControl = useCallback(async (action: ControlAction, value?: number) => {
    try {
      await emit("desktop-lyrics:control", { action, value });
    } catch (e) {
      console.error("[DesktopLyrics] sendControl emit failed:", action, e);
    }
  }, []);

  // 调节音量（通过主窗口控制 audioRef）
  const setVolume = useCallback(
    (v: number) => {
      setVolumeState(v);
      sendControl("volume", v);
    },
    [sendControl]
  );

  // 锁定/解锁（需要同步设置窗口的 ignoreCursorEvents）
  const lock = useCallback(() => {
    setIsLocked(true);
    sendControl("lock");
  }, [sendControl]);

  const unlock = useCallback(() => {
    setIsLocked(false);
    sendControl("unlock");
  }, [sendControl]);

  return {
    song,
    karaokeLines,
    estimatedTime,
    isPlaying,
    playMode,
    volume,
    settings,
    isLocked,
    sendControl,
    setVolume,
    lock,
    unlock,
  };
}
