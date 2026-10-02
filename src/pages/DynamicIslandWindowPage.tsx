/**
 * 桌面灵动岛窗口页面（Tauri 窗口 label=dynamic-island，路由 /dynamic-island）
 *
 * 这个窗口只是「纯渲染器」：灵动岛的状态真源（提示队列、自动关闭计时、持久基线）
 * 全部留在主窗口的 dynamic-island store 里，本页面只做三件事：
 *   1. 接收主窗口推来的快照 island:snapshot → 交给同一套 DynamicIslandHost 渲染动画；
 *   2. 把悬停 / 点击 / 播控以 island:cmd、island:music-control 回传主窗口执行；
 *   3. 常驻置顶 + 空闲鼠标穿透：光标不在岛体上时整窗穿透，避免在屏幕顶部中央
 *      挡出一条永远点不到的区域。
 *
 * 已知视觉差异：透明桌面窗口里 CSS backdrop-filter 只能采样本 WebView 表面（背后是
 * 桌面），拿不到真实折射/模糊，因此岛体使用不透明胶囊底色，见 dynamic-island.tsx 的
 * islandGlass 判定。
 */
import { useEffect, useRef } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { availableMonitors, getCurrentWindow, PhysicalPosition, primaryMonitor } from "@tauri-apps/api/window";
import { useMusicStore } from "@/stores/music-store";
import type { Song } from "@/types/music";
import type { IslandSongLite } from "@/components/ui/island-remote-bridge";
import {
  applyRemoteSnapshot,
  clearRemoteSnapshot,
  notifyIslandIdle,
  ISLAND_HOST_DOM_ID,
  type RemoteIslandSnapshot,
} from "@/components/ui/dynamic-island";
import { DynamicIslandHost } from "@/components/ui/dynamic-island";

/** 必须与 Rust 侧 island_window.rs 的 inner_size 保持一致（逻辑像素） */
const WINDOW_LOGICAL_W = 348;
const WINDOW_LOGICAL_H = 176;
/** 光标进入岛体的判定边距（CSS px），离开时用它 + EXIT_EXTRA 形成回差，避免抖动 */
const HIT_MARGIN = 14;
const HIT_EXIT_EXTRA = 18;

interface IslandMusicTrack {
  song: IslandSongLite | null;
  isPlaying: boolean;
  durationSec: number;
  positionSec: number;
}

interface IslandMusicProgress {
  positionSec: number;
  isPlaying: boolean;
}

/**
 * 音乐岛的进度来源占位：主窗口把 currentTime 推过来后写入这里并派发 timeupdate，
 * MusicIslandContent 沿用「监听 audio 元素 timeupdate」的原逻辑即可，无需为跨窗口改造。
 */
class IslandFakeAudio extends EventTarget {
  currentTime = 0;
  duration = 0;
}
const fakeAudio = new IslandFakeAudio();

/**
 * 进度插值基准：主窗口 500ms 推一次进度，岛窗口每 200ms 自行往前推，
 * 否则进度条与时间会明显卡顿甚至不动。
 */
const progressBase = { pos: 0, at: 0, playing: false };

function advanceProgress(pos: number, playing: boolean) {
  progressBase.pos = pos;
  progressBase.at = performance.now();
  progressBase.playing = playing;
  fakeAudio.currentTime = pos;
  fakeAudio.dispatchEvent(new Event("timeupdate"));
}

/**
 * 本窗口的播控全部替换为跨窗口指令：岛窗口没有 <audio>、没有播放队列，
 * 真正的播放动作必须由主窗口的 music-store 执行（见 IslandRemoteBridge）。
 * 首次渲染前同步安装，确保 MusicIslandContent 拿到的是替换后的动作。
 */
function installIslandStoreShim() {
  useMusicStore.setState({
    audioRef: fakeAudio as unknown as HTMLAudioElement,
    togglePlay: () => {
      void emit("island:music-control", { action: "play-pause" });
    },
    prevTrack: () => {
      void emit("island:music-control", { action: "prev" });
    },
    nextTrack: () => {
      void emit("island:music-control", { action: "next" });
    },
    seekTo: (time: number) => {
      // 本地立即钉到目标位置，等主窗口真的跳过去，避免松手瞬间回弹
      advanceProgress(time, progressBase.playing);
      void emit("island:music-control", { action: "seek", value: time });
    },
    externalControl: (action: "play-pause" | "prev" | "next" | "seek", valueMs?: number) => {
      void emit("island:music-control", { action: `external-${action}`, value: valueMs });
    },
  });
  // 封面走主窗口同一套本地代理（同 origin、同 WebView2 数据目录，可直接加载）
  void invoke<number>("cmd_get_proxy_port")
    .then((port) => useMusicStore.setState({ proxyPort: port }))
    .catch(() => {});
}

export default function DynamicIslandWindowPage() {
  const shimInstalled = useRef(false);
  if (!shimInstalled.current) {
    shimInstalled.current = true;
    installIslandStoreShim();
  }

  // 是否有内容正在显示（无内容时永远穿透，不必询问光标）
  const hasContentRef = useRef(false);
  // 当前穿透状态，避免重复调用 set_ignore_cursor_events
  const throughRef = useRef(true);
  // 左键按住中的时间戳：拖动进度条时绝不能中途开启穿透，否则 mouseup 会丢到
  // 下方窗口、拖动状态永远不重置（带 5s 上限，即使漏事件也不会永久挡下点击）
  const mouseDownAtRef = useRef(0);

  useEffect(() => {
    const down = () => {
      mouseDownAtRef.current = Date.now();
    };
    const up = () => {
      mouseDownAtRef.current = 0;
    };
    window.addEventListener("mousedown", down);
    window.addEventListener("mouseup", up);
    window.addEventListener("blur", up);
    return () => {
      window.removeEventListener("mousedown", down);
      window.removeEventListener("mouseup", up);
      window.removeEventListener("blur", up);
    };
  }, []);

  // 强制整窗背景透明：index.css 的 :root{background-color} 会填充 WebView2 导致穿透失效
  useEffect(() => {
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("root");
    const prev = [html.style.background, body.style.background, root?.style.background ?? ""];
    html.style.background = "transparent";
    body.style.background = "transparent";
    if (root) root.style.background = "transparent";
    return () => {
      html.style.background = prev[0];
      body.style.background = prev[1];
      if (root) root.style.background = prev[2];
    };
  }, []);

  // 定位主屏顶部中央；每 2s 兜底校正一次（改分辨率 / 插拔显示器后自动归位）
  useEffect(() => {
    const win = getCurrentWindow();
    let disposed = false;

    const place = async () => {
      try {
        // 主屏优先；取不到（极端驱动/虚拟屏）时回退到枚举出的第一个显示器
        const all = await availableMonitors();
        const primary = (await primaryMonitor().catch(() => null)) ?? all[0] ?? null;
        if (disposed || !primary) return;
        const dpr = window.devicePixelRatio || 1;
        const wPhys = Math.round(WINDOW_LOGICAL_W * dpr);
        const x = primary.position.x + Math.max(0, Math.round((primary.size.width - wPhys) / 2));
        const y = primary.position.y;
        const cur = await win.outerPosition().catch(() => null);
        if (cur && Math.abs(cur.x - x) <= 1 && Math.abs(cur.y - y) <= 1) return;
        await win.setPosition(new PhysicalPosition(x, y));
      } catch (e) {
        console.error("[IslandWindow] 定位失败:", e);
      }
    };

    void place();
    const timer = setInterval(() => void place(), 2000);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, []);

  // 接收快照与音乐数据；监听注册完成后才发 island:ready（带退避重发，解决主窗口先发我还没听的问题）
  useEffect(() => {
    const unlisten: UnlistenFn[] = [];
    const cleanups: Array<() => void> = [];
    let cancelled = false;

    (async () => {
      const onSnapshot = await listen<RemoteIslandSnapshot>("island:snapshot", (e) => {
        hasContentRef.current = Boolean(e.payload?.item);
        applyRemoteSnapshot(e.payload);
      });
      const onTrack = await listen<IslandMusicTrack>("island:music-track", (e) => {
        const { song, isPlaying, durationSec, positionSec } = e.payload;
        const full: Song | null = song
          ? {
              provider: "island",
              id: song.id,
              name: song.name,
              artist: song.artist,
              artists: song.artists.map((n) => ({ name: n })),
              album: "",
              cover: song.cover,
              duration: song.duration || durationSec || 0,
              fee: 0,
              playable: true,
              language: 0,
            }
          : null;
        fakeAudio.duration = full?.duration ?? 0;
        useMusicStore.setState({
          currentSong: full,
          isPlaying,
          duration: durationSec || full?.duration || 0,
        });
        // 换曲快照自带当前进度，作为本地插值的基准（positionSec 缺失时不覆盖基准）
        if (typeof positionSec === "number") advanceProgress(positionSec, isPlaying);
      });
      const onProgress = await listen<IslandMusicProgress>("island:music-progress", (e) => {
        advanceProgress(e.payload.positionSec, e.payload.isPlaying);
        useMusicStore.setState({ isPlaying: e.payload.isPlaying });
      });
      // 开关关掉时自己立刻退场：不依赖 Rust 的 destroy 时序（销毁可能被拖慢或阻止），
      // 否则桌面上会留一个旧快照的岛、而主窗口内又已经不再推新内容
      const retire = () => {
        hasContentRef.current = false;
        clearRemoteSnapshot();
        void getCurrentWindow().hide().catch(() => {});
      };
      const onSetting = await listen<{ enabled?: boolean }>("island:setting-changed", (e) => {
        if (e.payload?.enabled === false) retire();
      });
      const onClosed = await listen("island:closed", retire);
      if (cancelled) {
        onSnapshot();
        onTrack();
        onProgress();
        onSetting();
        onClosed();
        return;
      }
      unlisten.push(onSnapshot, onTrack, onProgress, onSetting, onClosed);

      // 就绪：请求主窗口把当前快照与音乐数据全量推一份（重建窗口时靠它拿到歌曲数据）
      void emit("island:ready", {});
      // 保活心跳：主窗口的 remoteMode 靠它维持（主窗口重载 / StrictMode 双挂载会丢掉
      // 一次性 ready，那样提示又会退回主窗口内显示）。只证活、不拉数据。
      const heartbeat = setInterval(() => {
        if (cancelled) {
          clearInterval(heartbeat);
          return;
        }
        void emit("island:alive", {});
      }, 1000);
      cleanups.push(() => clearInterval(heartbeat));
    })();

    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
      cleanups.forEach((fn) => fn());
      void emit("island:closed", {});
    };
  }, []);

  // 进度本地插值：两次推送之间持续往前推，否则进度条/时间会停在上一帧
  useEffect(() => {
    const timer = setInterval(() => {
      if (!progressBase.playing) return;
      const pos = progressBase.pos + (performance.now() - progressBase.at) / 1000;
      fakeAudio.currentTime = pos;
      fakeAudio.dispatchEvent(new Event("timeupdate"));
    }, 200);
    return () => clearInterval(timer);
  }, []);

  // 空闲鼠标穿透：只有光标落在岛体（含边距）上时才收事件，其余一律穿透到下方窗口
  useEffect(() => {
    const win = getCurrentWindow();
    let disposed = false;
    let busy = false;

    const setThrough = async (ignore: boolean) => {
      if (throughRef.current === ignore) return;
      // 开启穿透前补一次「光标已离开」：否则悬停中直接穿透，岛窗口的 mouseleave
      // 永远不会触发 → 主窗口只 hold 不 extend → 提示永不自动收起
      if (ignore) notifyIslandIdle();
      throughRef.current = ignore;
      try {
        await invoke("dynamic_island_set_click_through", { ignore });
      } catch {
        // 回退：直接走 Tauri 窗口 API（命令要求窗口已存在，销毁瞬间可能取不到）
        try {
          await win.setIgnoreCursorEvents(ignore);
        } catch {
          /* ignore */
        }
      }
    };

    const tick = async () => {
      if (disposed || busy) return;
      busy = true;
      try {
        if (mouseDownAtRef.current && Date.now() - mouseDownAtRef.current < 5000) {
          // 拖动中：锁定为可交互，不做光标判定
          await setThrough(false);
          return;
        }
        if (!hasContentRef.current) {
          await setThrough(true);
          return;
        }
        const el = document.getElementById(ISLAND_HOST_DOM_ID);
        if (!el) {
          await setThrough(true);
          return;
        }
        const [cursor, pos] = await Promise.all([
          invoke<{ x: number; y: number }>("get_cursor_position"),
          win.outerPosition(),
        ]);
        const dpr = window.devicePixelRatio || 1;
        const r = el.getBoundingClientRect();
        const margin = (throughRef.current ? HIT_MARGIN : HIT_MARGIN + HIT_EXIT_EXTRA) * dpr;
        const inside =
          cursor.x >= pos.x + r.left * dpr - margin &&
          cursor.x <= pos.x + r.right * dpr + margin &&
          cursor.y >= pos.y + r.top * dpr - margin &&
          cursor.y <= pos.y + r.bottom * dpr + margin;
        await setThrough(!inside);
      } catch {
        /* 窗口正在销毁 / 光标查询失败：下一轮再试 */
      } finally {
        busy = false;
      }
    };

    void setThrough(true);
    const timer = setInterval(() => void tick(), 60);
    return () => {
      disposed = true;
      clearInterval(timer);
    };
  }, []);

  return (
    <div
      style={{
        width: "100vw",
        height: WINDOW_LOGICAL_H,
        background: "transparent",
        overflow: "hidden",
      }}
    >
      <DynamicIslandHost />
    </div>
  );
}
