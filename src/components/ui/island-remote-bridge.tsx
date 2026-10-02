/**
 * 主窗口侧的「桌面灵动岛」桥接。
 *
 * 灵动岛的状态真源始终留在主窗口的 dynamic-island store（提示队列、自动关闭计时、
 * 持久基线一行都不搬）；本组件只负责三条通道：
 *   1. 快照外发：主窗口 store 每次变更 → emit("island:snapshot") 给桌面岛窗口渲染；
 *   2. 命令回收：桌面岛窗口的悬停/点击 → emit("island:cmd") → 在这里落到真源 store；
 *   3. 音乐岛镜像：内部播放器状态外发（换曲全量 + 进度高频），播控指令回收。
 *      外部客户端（SMTC）由 Rust 全局广播，桌面窗口自己监听，无需在此镜像。
 *
 * 桌面岛窗口就绪（island:ready）后才开启 remoteMode；未就绪 / 已关闭时自动退回
 * 主窗口内嵌渲染，因此启动早期（窗口还没建好）触发到的提示不会丢失。
 */
import { useEffect } from "react";
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useNavigate } from "react-router-dom";
import { useMusicStore } from "@/stores/music-store";
import { store } from "@/lib/store";
import {
  applyIslandCommand,
  isIslandRemoteMode,
  IS_ISLAND_WINDOW,
  pushIslandSnapshot,
  setIslandExternalRequested,
  setIslandSnapshotSink,
  setIslandWindowAlive,
} from "./dynamic-island";

/** 桌面岛窗口回传的交互命令 */
interface IslandCmdPayload {
  op: "hold" | "extend" | "close" | "click";
  id: string;
  shouldClose?: boolean;
}

/**
 * 岛窗口只展示歌名/歌手/封面/时长，所以下发精简字段而不是整条 Song：
 * 整条 Song 里可能带 base64 封面（几 MB）或 blob: URL，走事件通道既贵又可能整包丢，
 * blob: URL 更是只能在创建它的窗口里解引用，跨窗口必然加载失败。
 */
export interface IslandSongLite {
  id: string;
  name: string;
  artist: string;
  artists: string[];
  cover: string;
  duration: number;
}

/** 音乐岛换曲快照（内部播放器），同时带上当前进度作为岛窗口本地插值的基准 */
export interface IslandMusicTrack {
  song: IslandSongLite | null;
  isPlaying: boolean;
  durationSec: number;
  positionSec: number;
}

/** 音乐岛进度（小载荷，500ms 一次，仅作纠偏，岛窗口自己会插值前进） */
export interface IslandMusicProgress {
  positionSec: number;
  isPlaying: boolean;
}

/** base64 封面体积上限：超过就不跨窗口下发（岛窗口会回退到音符占位图） */
const MAX_INLINE_COVER_CHARS = 400_000;

/**
 * 时长单位统一：各平台/本地导入的 duration 有秒也有毫秒（一首歌不可能超过 1 小时，
 * 超过 3600 一律当毫秒折算），否则岛窗口会显示成「2625:00」。
 * 优先取 audio 元素的 duration（一定是秒）。
 */
function toSeconds(v: number | undefined): number {
  if (!v || !Number.isFinite(v) || v <= 0) return 0;
  return v > 3600 ? v / 1000 : v;
}

function safeCover(cover: string | undefined): string {
  if (!cover) return "";
  if (cover.startsWith("blob:")) return "";
  if (cover.startsWith("data:") && cover.length > MAX_INLINE_COVER_CHARS) return "";
  return cover;
}

function toLite(song: ReturnType<typeof useMusicStore.getState>["currentSong"]): IslandSongLite | null {
  if (!song) return null;
  return {
    id: song.id,
    name: song.name ?? "",
    artist: song.artist ?? "",
    artists: (song.artists ?? []).map((a) => a.name).slice(0, 4),
    cover: safeCover(song.cover),
    duration: toSeconds(song.duration),
  };
}

/** 上一次下发的换曲数据指纹：封面可能是几百 KB 的 base64，完全相同时不重发 */
let lastTrackFingerprint = "";

function pushMusicTrack(force = false) {
  const s = useMusicStore.getState();
  const song = toLite(s.currentSong);
  const durationSec =
    toSeconds(s.audioRef?.duration) || toSeconds(s.duration) || toSeconds(song?.duration ?? 0);
  const payload: IslandMusicTrack = {
    song,
    isPlaying: s.isPlaying,
    durationSec,
    positionSec: s.audioRef?.currentTime ?? 0,
  };
  // 进度不在指纹里（由 island:music-progress 单独驱动），否则每 500ms 都会重发封面
  const fingerprint = `${JSON.stringify(song)}|${s.isPlaying}|${durationSec}`;
  if (!force && fingerprint === lastTrackFingerprint) return;
  lastTrackFingerprint = fingerprint;
  void emit("island:music-track", payload).catch((e) =>
    console.warn("[Island] island:music-track 下发失败:", e)
  );
}

function pushMusicProgress() {
  const s = useMusicStore.getState();
  if (!s.currentSong) return;
  const payload: IslandMusicProgress = {
    positionSec: s.audioRef?.currentTime ?? 0,
    isPlaying: s.isPlaying,
  };
  void emit("island:music-progress", payload).catch((e) =>
    console.warn("[Island] island:music-progress 下发失败:", e)
  );
}

export function IslandRemoteBridge() {
  const navigate = useNavigate();

  useEffect(() => {
    // 仅主窗口做真源桥接：桌面岛窗口/托盘菜单等独立窗口挂载了同一批 Provider，必须跳过
    if (IS_ISLAND_WINDOW) return;
    if (getCurrentWindow().label !== "main") return;

    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    // 推送出口常驻注册：remoteMode 为真时立即能推快照，不必等 ready 与 store 变更抢跑
    setIslandSnapshotSink((snap) => {
      void emit("island:snapshot", snap).catch((e) => console.warn("[Island] 快照下发失败:", e));
    });

    /*
     * 外部渲染开关收敛在 dynamic-island 模块里：用户意图（设置页/启动恢复直接写入，
     * 与 store 同一 JS 上下文、零延迟）|| 桌面窗口心跳存活。
     * 本组件只负责维护「心跳存活」与数据补推。
     */
    let lastAliveAt = 0;
    let wasOn = isIslandRemoteMode();
    const syncAlive = () => {
      setIslandWindowAlive(Date.now() - lastAliveAt < 3000);
      const on = isIslandRemoteMode();
      if (on && !wasOn) {
        // 刚切到外部渲染：补推一次当前内容，避免桌面窗口停在旧状态或干脆空着
        pushIslandSnapshot();
        pushMusicTrack(true);
        pushMusicProgress();
      }
      wasOn = on;
    };

    void store
      .get<unknown>("nexbox_island_external_window")
      .then((v) => {
        // 只负责「把意图恢复为开」（启动恢复未走到时的兜底）；
        // 关由设置页 / island:closed 负责，避免异步回读把用户刚拨开的开关打回去
        if (v === true || v === "true" || v === 1) {
          setIslandExternalRequested(true);
          syncAlive();
        }
      })
      .catch(() => {});

    (async () => {
      const onReady = await listen("island:ready", () => {
        lastAliveAt = Date.now();
        syncAlive();
        // 就绪即全量补推（幂等）：桌面窗口可能被销毁重建而开关全程没变，
        // 不强制推会让新窗口拿到音乐岛条目却没有歌曲数据（白空胶囊）
        pushIslandSnapshot();
        pushMusicTrack(true);
        pushMusicProgress();
      });
      const onAlive = await listen("island:alive", () => {
        lastAliveAt = Date.now();
        syncAlive();
      });
      const onClosed = await listen("island:closed", () => {
        lastAliveAt = 0;
        syncAlive();
      });
      // 意图由设置页直接写入模块；这里同步存活状态（关掉时立即丢弃心跳记录）
      const onSetting = await listen<{ enabled: boolean }>("island:setting-changed", (e) => {
        if (e.payload?.enabled === false) lastAliveAt = 0;
        syncAlive();
      });
      const onCmd = await listen<IslandCmdPayload>("island:cmd", (e) => {
        applyIslandCommand(e.payload);
      });
      const onMusicControl = await listen<{ action: string; value?: number }>(
        "island:music-control",
        (e) => {
          const s = useMusicStore.getState();
          const { action, value } = e.payload;
          switch (action) {
            case "play-pause":
              s.togglePlay();
              break;
            case "prev":
              s.prevTrack();
              break;
            case "next":
              s.nextTrack();
              break;
            case "seek":
              s.seekTo(value ?? 0);
              break;
            // 外部客户端（SMTC）播控：岛窗口把 externalControl 拆成独立 action 传回
            case "external-play-pause":
              s.externalControl("play-pause");
              break;
            case "external-prev":
              s.externalControl("prev");
              break;
            case "external-next":
              s.externalControl("next");
              break;
            case "external-seek":
              s.externalControl("seek", value);
              break;
            case "open-player":
              // 岛窗口没有路由，唤回主窗口并展开播放器
              void invoke("show_window").catch(() => {});
              navigate("/music", { state: { expandPlayer: true } });
              break;
          }
        }
      );
      if (disposed) {
        onReady();
        onAlive();
        onClosed();
        onSetting();
        onCmd();
        onMusicControl();
        return;
      }
      unlisteners.push(onReady, onAlive, onClosed, onSetting, onCmd, onMusicControl);
    })();

    // 心跳过期兜底：桌面窗口被销毁但 closed 事件丢失时，最迟 3s 后退回内嵌渲染
    const watchdog = setInterval(syncAlive, 1000);

    // 换曲/播放态变化：只在数据真的变了时全量推一次（封面体积可能很大）
    const unsubStore = useMusicStore.subscribe(() => {
      if (!isIslandRemoteMode()) return;
      pushMusicTrack();
    });

    // 进度：高频但载荷极小（秒 + 播放态）；未开启外部模式时完全不发事件
    const timer = setInterval(() => {
      if (!isIslandRemoteMode()) return;
      pushMusicProgress();
    }, 500);

    return () => {
      disposed = true;
      unlisteners.forEach((fn) => fn());
      unsubStore();
      clearInterval(timer);
      clearInterval(watchdog);
      // 故意不在这里重置 remoteMode：StrictMode 双挂载的清理阶段不能把外部渲染打掉，
      // 真正的关闭由 island:closed / 开关意图变化负责
    };
  }, [navigate]);

  return null;
}
