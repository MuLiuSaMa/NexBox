import { create } from "zustand";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";
import { Store } from "@tauri-apps/plugin-store";
import type {
  Song,
  Artist,
  Playlist,
  LoginInfo,
  Lyrics,
  PlayMode,
  PlaybackQuality,
  SongUrlResult,
  KaraokeLine,
  MusicProvider,
  CommentPage,
  Album,
  Mv,
  ArtistDetail,
  ExternalPlayback,
  SmtcState,
} from "@/types/music";
import { buildKaraokeLines } from "@/lib/karaoke-lyrics";
import { store } from "@/lib/store";
import { ensureAudioContextActive } from "@/lib/audio-spectrum";

// 模块级：无版权自动跳过控制
let isAutoSkipping = false;
let unplayableSkipCount = 0;

interface MusicState {
  // 播放状态
  currentSong: Song | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  prevVolume: number;
  playMode: PlayMode;
  playQueue: Song[];
  currentIndex: number;
  // 心动模式：基于当前歌曲的相似歌曲动态队列（仅网易云源）
  heartbeatQueue: Song[];
  heartbeatLoading: boolean;
  // 心动模式已播放过的歌曲 ID（用于去重，避免相似歌曲重复播放）
  heartbeatPlayedIds: Set<string>;
  // 实际成功播放的历史栈（末尾为最近一首）：「上一首」按此回溯真实播放顺序，会话级不持久化
  playHistory: Song[];

  // 本地导入歌曲
  localSongs: Song[];
  // 本地导入进行中（文件夹/多文件导入耗时长，用于 UI 反馈避免误以为卡死）
  importingLocal: boolean;

  // 登录状态 (多平台)
  loginInfo: LoginInfo | null; // 当前播放源的登录信息 (向后兼容)
  loginInfos: Record<MusicProvider, LoginInfo | null>; // 所有平台登录信息
  playbackSource: MusicProvider; // 当前播放源

  // 外部客户端播放（SMTC 接管系统媒体会话，供灵动岛显示）
  externalTrack: ExternalPlayback["track"];
  externalPlaying: boolean;
  externalPositionMs: number;
  externalDurationMs: number;

  // 数据
  searchResults: Song[];
  userPlaylists: Playlist[];
  userPlaylistsError: string;
  // 左侧「我的歌单」面板的曲目
  leftPlaylistTracks: Song[];
  leftPlaylistMeta: Playlist | null;
  // 右侧「推荐歌单」面板的曲目
  rightPlaylistTracks: Song[];
  rightPlaylistMeta: Playlist | null;
  // 歌单分页
  leftPlaylistTotalTrackIds: string[];
  rightPlaylistTotalTrackIds: string[];
  leftPlaylistLoadingMore: boolean;
  // 歌单内搜索时「先加载全部曲目」的进行中标记
  leftPlaylistLoadingAll: boolean;
  rightPlaylistLoadingMore: boolean;
  likedSongIds: Set<string>;
  currentLyrics: Lyrics | null;
  recommendations: Playlist[];
  recommendSongs: Song[];
  dailyRecommendPlaylists: Playlist[];

  // 歌手搜索
  artistSearchResults: Artist[];
  artistSongs: Song[];
  selectedArtist: Artist | null;
  searchingArtists: boolean;
  loadingArtistSongs: boolean;
  artistDetail: ArtistDetail | null;
  artistAlbums: Album[];
  artistMvs: Mv[];
  albumDetailSongs: Song[];
  albumDetailMeta: Album | null;
  loadingArtistDetail: boolean;
  loadingArtistAlbums: boolean;
  loadingArtistMvs: boolean;
  loadingAlbumDetail: boolean;

  // 歌单搜索
  playlistSearchResults: Playlist[];
  searchingPlaylists: boolean;

  // 官方榜单
  officialCharts: Playlist[];

  // 音质
  playbackQuality: PlaybackQuality;
  currentQuality: string;
  currentBitrate: number;

  // 代理端口
  proxyPort: number;

  // 歌词字体大小
  lyricsFontSize: number;
  // 歌词高亮颜色
  lyricsHighlightColor: string;

  // 透明彩胶样式：胶片颜色模式（auto=跟随封面主色 / custom=手动固定色）与手动颜色
  vinylColorMode: "auto" | "custom";
  vinylCustomColor: string;

  // 桌面歌词设置
  desktopLyricsVisible: boolean;
  desktopLyricsFontSize: number;
  desktopLyricsFontFamily: string;
  desktopLyricsHighlightColor: string;
  desktopLyricsBaseColor: string;
  desktopLyricsLineCount: 1 | 2;
  desktopLyricsLocked: boolean;
  desktopLyricsShowTranslation: boolean;
  desktopLyricsHideUnlockBtn: boolean;

  // UI 状态
  searching: boolean;
  loadingPlaylists: boolean;
  loadingLeftTracks: boolean;
  loadingRightTracks: boolean;
  loadingLyrics: boolean;
  expandedStyle: "glass" | "modern" | "immersive" | "spectrum" | "vinyl" | "cover";
  dynamicEnabled: boolean;
  coverFilmEffect: boolean;
  // 键盘媒体键控制内置播放器（设置 → 高级 → 媒体键控制）
  mediaKeysEnabled: boolean;

  // Toast 通知
  musicToast: { type: "warning"; message: string } | null;

  // 音频元素引用
  audioRef: HTMLAudioElement | null;

  // Actions
  init: () => Promise<void>;
  setAudioRef: (audio: HTMLAudioElement | null) => void;

  // 本地导入歌曲 Actions
  loadLocalSongs: () => Promise<void>;
  importLocalSongs: (paths: string[]) => Promise<{ count: number; noCoverCount: number }>;
  importLocalFolder: (folder: string) => Promise<{ count: number; noCoverCount: number }>;
  setImportingLocal: (importing: boolean) => void;
  removeLocalSong: (id: string) => Promise<void>;
  clearLocalSongs: () => Promise<void>;

  search: (keywords: string) => Promise<void>;
  searchArtists: (keywords: string) => Promise<void>;
  loadArtistSongs: (artistId: string, offset?: number) => Promise<void>;
  loadArtistDetail: (artistId: string) => Promise<void>;
  loadArtistAlbums: (artistId: string, offset?: number) => Promise<void>;
  loadArtistMvs: (artistId: string, offset?: number) => Promise<void>;
  loadAlbumDetail: (albumId: string) => Promise<void>;
  clearArtistState: () => void;
  searchPlaylists: (keywords: string) => Promise<void>;
  playSong: (song: Song, queue?: Song[], opts?: { fromHistory?: boolean }) => Promise<void>;
  togglePlay: () => void;
  nextTrack: () => void;
  prevTrack: () => void;
  seekTo: (time: number) => void;
  setVolume: (v: number) => void;
  togglePlayMode: () => void;
  /** 外部客户端控制命令（SMTC）：play-pause | prev | next | seek */
  externalControl: (action: "play-pause" | "prev" | "next" | "seek", valueMs?: number) => void;
  loadHeartbeatSongs: (baseSong: Song | null) => Promise<void>;
  setPlaybackQuality: (quality: PlaybackQuality) => Promise<void>;
  setLyricsFontSize: (size: number) => Promise<void>;
  setLyricsHighlightColor: (color: string) => Promise<void>;
  setVinylColorMode: (mode: "auto" | "custom") => Promise<void>;
  setVinylCustomColor: (color: string) => Promise<void>;
  setExpandedStyle: (style: "glass" | "modern" | "immersive" | "spectrum" | "vinyl" | "cover") => Promise<void>;
  setDynamicEnabled: (enabled: boolean) => Promise<void>;
  setCoverFilmEffect: (enabled: boolean) => Promise<void>;
  // 键盘媒体键控制（仅更新内存态）
  setMediaKeysEnabled: (enabled: boolean) => void;
  // 立即重推一次 SMTC 播放状态（重新开启媒体键后马上恢复系统媒体会话）
  refreshSmtc: () => void;
  setCurrentTime: (t: number) => void;
  setDuration: (d: number) => void;

  // 桌面歌词 Actions
  toggleDesktopLyrics: () => Promise<void>;
  setDesktopLyricsVisible: (visible: boolean) => Promise<void>;
  setDesktopLyricsFontSize: (size: number) => Promise<void>;
  setDesktopLyricsFontFamily: (family: string) => Promise<void>;
  setDesktopLyricsHighlightColor: (color: string) => Promise<void>;
  setDesktopLyricsBaseColor: (color: string) => Promise<void>;
  setDesktopLyricsLineCount: (count: 1 | 2) => Promise<void>;
  setDesktopLyricsLocked: (locked: boolean) => Promise<void>;
  setDesktopLyricsShowTranslation: (show: boolean) => Promise<void>;
  setDesktopLyricsHideUnlockBtn: (hide: boolean) => Promise<void>;
  toggleDesktopLyricsHideUnlockBtn: () => Promise<void>;
  emitDesktopLyricsSettings: () => void;
  emitDesktopLyricsData: () => void;

  loginStatus: () => Promise<void>;
  loginStatusFor: (provider: MusicProvider) => Promise<void>;
  loginWithCookie: (cookie: string) => Promise<boolean>;
  logout: () => Promise<void>;
  logoutFor: (provider: MusicProvider) => Promise<void>;
  openLoginWindow: (provider?: MusicProvider) => Promise<void>;
  switchPlaybackSource: (provider: MusicProvider) => Promise<void>;
  loadAllLoginStatuses: () => Promise<void>;

  loadUserPlaylists: () => Promise<void>;
  loadUserPlaylistsFor: (provider: MusicProvider) => Promise<void>;
  loadLeftPlaylistTracks: (id: string) => Promise<void>;
  loadMoreLeftPlaylistTracks: () => Promise<void>;
  // 歌单内搜索：一次性把当前歌单全部剩余曲目拉齐到 leftPlaylistTracks
  loadAllLeftPlaylistTracks: () => Promise<void>;
  loadRightPlaylistTracks: (id: string) => Promise<void>;
  loadRightRankTracks: (rankId: string) => Promise<void>;
  loadMoreRightPlaylistTracks: () => Promise<void>;
  loadLikedList: () => Promise<void>;
  toggleLike: (songId: string) => Promise<void>;
  loadLyrics: (songId: string) => Promise<void>;
  loadLyricsForSong: (song: Song) => Promise<void>;
  loadRecommendations: () => Promise<void>;
  loadOfficialCharts: () => Promise<void>;
  togglePlaylistSubscribe: (playlistId: string, currentSubscribed: boolean) => Promise<void>;

  // 评论系统
  currentComments: CommentPage | null;
  loadingComments: boolean;
  sendingComment: boolean;
  commentError: string;
  loadComments: (songId: string, page?: number) => Promise<void>;
  sendComment: (songId: string, content: string) => Promise<boolean>;
  clearComments: () => void;
}

let storeInstance: Store | null = null;
const getStore = async (): Promise<Store> => {
  if (!storeInstance) {
    storeInstance = await Store.load("music-player-settings.json");
  }
  return storeInstance;
};

// 序列化本地歌曲对象用于持久化，确保字段完整
function serializeLocalSong(song: Song): Record<string, unknown> {
  return {
    provider: "local",
    id: song.id,
    name: song.name,
    artist: song.artist,
    artists: song.artists || [],
    album: song.album,
    cover: song.cover || "",
    duration: song.duration,
    fee: song.fee ?? 0,
    playable: song.playable ?? true,
    language: song.language ?? 0,
    hash: song.hash,
    _localPath: song._localPath,
    _localCoverPath: song._localCoverPath,
  };
}

/// 后端 import_local_music / import_local_music_folder 返回的单首歌曲元信息
interface LocalSongInfoPayload {
  id: string;
  name: string;
  path: string;
  size: number;
  extension: string;
  title: string;
  artist: string;
  album: string;
  duration_ms: number;
  cover_path: string;
  cover_source: string;
}

/// 把后端返回的本地歌曲元信息合并进 localSongs 并持久化（供单文件/文件夹导入复用）
async function mergeLocalSongInfos(
  infos: LocalSongInfoPayload[],
  set: (partial: (state: unknown) => unknown) => void,
  get: () => { localSongs: Song[] }
): Promise<{ count: number; noCoverCount: number }> {
  if (infos.length === 0) return { count: 0, noCoverCount: 0 };

  const newSongs: Song[] = infos.map((info) => ({
    provider: "local",
    id: info.id,
    // 优先使用音频标签中的标题，回退到文件名
    name: info.title || info.name,
    artist: info.artist || "本地音乐",
    artists: info.artist ? [{ name: info.artist }] : [],
    album: info.album,
    // 封面：后端返回缓存文件路径，转换为 asset 协议 URL 供 <img> 加载
    cover: info.cover_path ? convertFileSrc(info.cover_path) : "",
    duration: info.duration_ms,
    fee: 0,
    playable: true,
    language: 0,
    // 本地歌曲专用：文件绝对路径，用于 convertFileSrc 播放
    // 复用 hash 字段存放绝对路径，避免修改 Song 结构
    hash: info.path,
    _localPath: info.path,
    // 封面缓存文件绝对路径，用于持久化与重启后恢复封面
    _localCoverPath: info.cover_path || undefined,
  }));

  set((state) => {
    const st = state as { localSongs: Song[] };
    // 用 Map 以 id 为键做去重合并，O(n) 而非 O(n²)，避免大列表导入时卡顿
    const merged = new Map<string, Song>();
    for (const song of st.localSongs) {
      merged.set(song.id, song);
    }
    for (const song of newSongs) {
      merged.set(song.id, song);
    }
    return { localSongs: Array.from(merged.values()) };
  });

  // 持久化完整 Song 对象，确保重启后 provider 等字段不丢失
  const s = await getStore();
  const finalList = get().localSongs.map(serializeLocalSong);
  await s.set("localSongs", finalList);
  await s.save();

  return { count: newSongs.length, noCoverCount: infos.filter((info) => !info.cover_path).length };
}

// 桌面歌词时间同步定时器
let timeSyncTimer: ReturnType<typeof setInterval> | null = null;
// 防止 React Strict Mode 双重调用 init 导致重复注册 listener
let listenersRegistered = false;
// 页面关闭时仅注册一次 unload 清理，避免重复绑定
let unloadCleanupBound = false;
// 当前已绑定 SMTC 事件的 <audio> 元素，用于替换/卸载时解绑，防止监听残留
let smtcBoundAudio: HTMLAudioElement | null = null;
// SMTC 事件具名回调：绑定/解绑需引用同一函数
function onSmtcTimeUpdate() {
  pushSmtc();
}
function onSmtcPlay() {
  useMusicStore.setState({ isPlaying: true });
  pushSmtc(true);
}
function onSmtcPause() {
  useMusicStore.setState({ isPlaying: false });
  pushSmtc(true);
}
function onSmtcLoadedMetadata() {
  pushSmtc(true);
}
function onSmtcEnded() {
  pushSmtc(true);
}
function bindSmtcHandlers(audio: HTMLAudioElement) {
  audio.addEventListener("timeupdate", onSmtcTimeUpdate);
  audio.addEventListener("play", onSmtcPlay);
  audio.addEventListener("pause", onSmtcPause);
  audio.addEventListener("loadedmetadata", onSmtcLoadedMetadata);
  audio.addEventListener("ended", onSmtcEnded);
}
function unbindSmtcHandlers(audio: HTMLAudioElement | null) {
  if (!audio) return;
  audio.removeEventListener("timeupdate", onSmtcTimeUpdate);
  audio.removeEventListener("play", onSmtcPlay);
  audio.removeEventListener("pause", onSmtcPause);
  audio.removeEventListener("loadedmetadata", onSmtcLoadedMetadata);
  audio.removeEventListener("ended", onSmtcEnded);
}
// 存储 Tauri 事件监听器的取消函数，防止内存泄漏
const unlistenFns: (() => void)[] = [];

function startTimeSync() {
  if (timeSyncTimer) return;
  timeSyncTimer = setInterval(() => {
    const state = useMusicStore.getState();
    // 仅桌面歌词可见且音频实际出声时同步（含暂停缓出期间，桌面歌词需跟随渐弱的音乐）
    if (state.audioRef && state.desktopLyricsVisible && !state.audioRef.paused) {
      emit("desktop-lyrics:time", {
        currentTime: state.audioRef.currentTime,
        isPlaying: true,
      });
    }
  }, 200);
}
export function stopTimeSync() {
  if (timeSyncTimer) {
    clearInterval(timeSyncTimer);
    timeSyncTimer = null;
  }
}

/** 清理所有 Tauri 事件监听器，防止内存泄漏 */
export function cleanupMusicListeners() {
  unlistenFns.forEach((fn) => fn());
  unlistenFns.length = 0;
  listenersRegistered = false;
  unbindSmtcHandlers(smtcBoundAudio);
  smtcBoundAudio = null;
  stopTimeSync();
}

/** 绑定页面卸载时的统一清理（只注册一次），避免 Tauri listener/音频监听残留在整个会话 */
function bindUnloadCleanup() {
  if (unloadCleanupBound) return;
  unloadCleanupBound = true;
  window.addEventListener("beforeunload", cleanupMusicListeners);
}

async function getProxyAudioUrl(rawUrl: string, proxyPort: number): Promise<string> {
  if (!proxyPort) {
    proxyPort = await invoke<number>("cmd_get_proxy_port");
  }
  return `http://127.0.0.1:${proxyPort}/audio?url=${encodeURIComponent(rawUrl)}`;
}

export function coverProxyUrl(url: string, proxyPort: number): string {
  if (!url) return "";
  if (url.startsWith("data:") || url.startsWith("blob:")) return url;
  // asset 协议 URL（convertFileSrc 生成的本地文件直链）仅在 WebView 内可访问，
  // 无法被后端代理解析，原样返回即可
  if (url.startsWith("http://asset.localhost/") || url.startsWith("https://asset.localhost/")) return url;
  if (!proxyPort) return url;
  return `http://127.0.0.1:${proxyPort}/cover?url=${encodeURIComponent(url)}`;
}

/// 后端 verify_local_covers 返回的单条封面修复结果
interface CoverRepairPayload {
  id: string;
  cover_path: string;
  cover_source: string;
}

/**
 * 封面缓存自愈：把持有封面路径的本地歌曲交给后端校验，
 * 缓存文件已被清理（清除缓存/存储感知/磁盘清理工具）的歌曲从音频重新提取封面。
 * 启动后后台执行，不阻塞歌库加载；缓存完好时后端返回空数组，无任何写入。
 */
async function repairLocalCovers(
  set: (partial: (state: unknown) => unknown) => void,
  get: () => { localSongs: Song[] }
): Promise<void> {
  try {
    const candidates = get().localSongs.filter((song) => song._localCoverPath);
    if (candidates.length === 0) return;
    const repairs = await invoke<CoverRepairPayload[]>("verify_local_covers", {
      songs: candidates.map((song) => ({
        id: song.id,
        path: song._localPath ?? "",
        cover_path: song._localCoverPath ?? "",
      })),
    });
    if (repairs.length === 0) return;

    // 回填修复结果：重提取成功换新路径，彻底失败（音频也没了等）则清掉失效路径
    const repaired = new Map(repairs.map((r) => [r.id, r]));
    set((state) => {
      const st = state as { localSongs: Song[] };
      return {
        localSongs: st.localSongs.map((song) => {
          const fix = repaired.get(song.id);
          if (!fix) return song;
          return {
            ...song,
            cover: fix.cover_path ? convertFileSrc(fix.cover_path) : "",
            _localCoverPath: fix.cover_path || undefined,
          };
        }),
      };
    });

    // 持久化修复结果
    const s = await getStore();
    await s.set("localSongs", get().localSongs.map(serializeLocalSong));
    await s.save();
    console.info(
      `[Music] 封面自愈：修复 ${repairs.filter((r) => r.cover_path).length} 首`
    );
  } catch (e) {
    console.warn("[Music] 封面自愈失败:", e);
  }
}

// ── 内部播放器 SMTC（系统媒体传输控制）推送 ──────────────────────────
// 把当前播放状态推给后端注册的「新境盒」媒体会话，供音量浮层/锁屏显示、
// 系统媒体键（播放/暂停/上一曲/下一曲）与浮层拖动进度控制内部播放器。
let lastSmtcPush = 0;
let smtcCleared = true;

// SMTC 控制事件去重：物理媒体键同时触发「低层键盘钩子」和 SMTC ButtonPressed
// 两条链路，毫秒级内对同一动作各 emit 一次；窗口期内忽略相同动作的重复事件。
let lastSmtcAction: string | null = null;
let lastSmtcActionTime = 0;
function shouldHandleSmtc(action: string): boolean {
  if (action === "seek") return true; // seek 来自浮层拖动，不会重复
  const now = Date.now();
  if (action === lastSmtcAction && now - lastSmtcActionTime < 150) return false;
  lastSmtcAction = action;
  lastSmtcActionTime = now;
  return true;
}

/**
 * 计算 SMTC 封面来源，交给后端下载（后端 reqwest 带防盗链 Referer，无 CORS 限制）：
 * - data URI：后端直接解码
 * - 本地歌曲：传封面缓存文件绝对路径（file:// 前缀，后端读文件）
 * - 在线歌曲：传原始封面 URL，后端带 Referer 下载（绕过防盗链）；网易云封面追加
 *   ?param=1024y1024 尺寸参数让 CDN 直接返回缩图（高清原图可达 5MB+，SMTC 只需 ~1024px）
 */
function smtcCoverSource(song: Song): string {
  if (!song.cover) return "";
  if (song.cover.startsWith("data:")) return song.cover;
  if (song._localCoverPath) return `file://${song._localCoverPath}`;
  if (/music\.126\.net\//.test(song.cover) && !song.cover.includes("?")) {
    return `${song.cover}?param=1024y1024`;
  }
  return song.cover;
}

/**
 * 推送一次 SMTC 状态。
 * force=true 忽略 1s 节流（元数据/播放状态变化时使用）；无歌时调 smtc_clear 让会话从浮层消失。
 */
async function pushSmtc(force = false) {
  const s = useMusicStore.getState();
  if (!s.currentSong || !s.audioRef) {
    if (!smtcCleared) {
      smtcCleared = true;
      invoke("smtc_clear").catch(() => {});
    }
    return;
  }
  smtcCleared = false;

  const now = Date.now();
  if (!force && now - lastSmtcPush < 1000) return; // 进度节流 1s
  lastSmtcPush = now;

  const posMs = Math.round((s.audioRef.currentTime || 0) * 1000);
  const durMs = Math.round((s.duration || s.audioRef.duration || s.currentSong.duration || 0) * 1000);
  const artist = s.currentSong.artist || s.currentSong.artists?.map((a) => a.name).join(" / ") || "";
  const payload: SmtcState = {
    title: s.currentSong.name,
    artist,
    album: s.currentSong.album || "",
    cover: smtcCoverSource(s.currentSong),
    playing: s.isPlaying,
    positionMs: posMs,
    durationMs: durMs,
  };
  invoke("smtc_update_state", { state: payload }).catch(() => {});
}

/// 后台批量加载歌单剩余曲目到播放队列（不加入歌单列表）
/// 优化：先在本地累积所有批次，最后做一次去重 setState，避免重复歌曲和频繁 re-render
/// 限制：播放队列最大 2000 首，超出部分不再追加，防止内存无限增长
const MAX_PLAY_QUEUE = 2000;
let batchLoadGuard: string | null = null;
async function batchLoadToQueue(playlistId: string, initialSongs: Song[], totalCount: number) {
  if (initialSongs.length >= totalCount) return;
  // 防止并发执行同一歌单的后台加载
  if (batchLoadGuard === playlistId) return;
  batchLoadGuard = playlistId;

  // 本地累积，仅在结束时做一次 setState
  const collected: Song[] = [];
  const seenIds = new Set(initialSongs.map((s) => s.id));
  let offset = initialSongs.length;

  try {
    while (offset < totalCount) {
      const batch = await invoke<Song[]>("music_playlist_tracks_range", { id: playlistId, start: offset, count: 200 });
      if (batch.length === 0) break;
      // 检查用户是否已切换到其他歌单
      const state = useMusicStore.getState();
      if (state.leftPlaylistMeta?.id !== playlistId && state.rightPlaylistMeta?.id !== playlistId) break;
      // 去重：跳过已收集的歌曲
      for (const song of batch) {
        if (!seenIds.has(song.id)) {
          seenIds.add(song.id);
          collected.push(song);
        }
      }
      offset += 200;
      // 队列已达上限，停止加载
      if (initialSongs.length + collected.length >= MAX_PLAY_QUEUE) break;
    }
  } catch {
    // 网络错误中断，已收集的部分仍然写入
  } finally {
    batchLoadGuard = null;
  }

  if (collected.length === 0) return;

  // 单次 setState，并对当前 playQueue 去重
  const state = useMusicStore.getState();
  const isSameList = state.playQueue.length > 0
    && state.currentSong
    && state.playQueue.some((s) => s.id === state.currentSong!.id);
  if (isSameList) {
    const queueIds = new Set(state.playQueue.map((s) => s.id));
    const unique = collected.filter((s) => !queueIds.has(s.id));
    // 截断到最大队列长度
    const remaining = MAX_PLAY_QUEUE - state.playQueue.length;
    const toAdd = unique.slice(0, Math.max(0, remaining));
    if (toAdd.length > 0) {
      useMusicStore.setState({ playQueue: [...state.playQueue, ...toAdd] });
    }
  }
}

let playSongSeq = 0;

// ── 播放/暂停音量缓入缓出 ──
// 进度按已流逝时间计算：定时器被后台节流时只会跳变到终点，不会卡在半途
const VOLUME_FADE_MS = 350;
const VOLUME_FADE_TICK_MS = 16;
let volumeFadeTimer: ReturnType<typeof setInterval> | null = null;
let volumeFadeSeq = 0;
// 当前渐变是否为「渐出后暂停」：渐出中途调音量时需立即补上暂停，避免"已暂停"状态下仍出声
let volumeFadeIsPause = false;

function cancelVolumeFade() {
  volumeFadeSeq++;
  if (volumeFadeTimer) {
    clearInterval(volumeFadeTimer);
    volumeFadeTimer = null;
  }
}

/** 音量从当前值渐变到 to（smoothstep 缓入缓出曲线），结束时回调 onDone（如渐出后 pause） */
function startVolumeFade(audio: HTMLAudioElement, to: number, onDone?: () => void) {
  cancelVolumeFade();
  const seq = ++volumeFadeSeq;
  const from = audio.volume;
  const start = performance.now();
  volumeFadeTimer = setInterval(() => {
    if (seq !== volumeFadeSeq) {
      clearInterval(volumeFadeTimer!);
      volumeFadeTimer = null;
      return;
    }
    const t = Math.min(1, (performance.now() - start) / VOLUME_FADE_MS);
    const eased = t * t * (3 - 2 * t);
    audio.volume = from + (to - from) * eased;
    if (t >= 1) {
      clearInterval(volumeFadeTimer!);
      volumeFadeTimer = null;
      onDone?.();
    }
  }, VOLUME_FADE_TICK_MS);
}

// 播放历史上限：防止长期运行内存无限增长
const MAX_PLAY_HISTORY = 100;

// 最后一次真正成功播放的歌曲（模块级）：无版权跳过的中间曲不会更新此值，
// 确保历史栈记录的是用户实际听过的播放顺序
let lastPlayedSong: Song | null = null;

/** 播放成功后记录历史：把上一次实际播放的歌入栈；历史回溯与同曲重播不入栈 */
function recordPlayHistory(newSong: Song, fromHistory: boolean) {
  const prev = lastPlayedSong;
  lastPlayedSong = newSong;
  if (fromHistory || !prev || prev.id === newSong.id) return;
  useMusicStore.setState((st) => {
    const next = [...st.playHistory, prev];
    return next.length > MAX_PLAY_HISTORY
      ? { playHistory: next.slice(next.length - MAX_PLAY_HISTORY) }
      : { playHistory: next };
  });
}

export const useMusicStore = create<MusicState>((set, get) => ({
  currentSong: null,
  isPlaying: false,
  currentTime: 0,
  duration: 0,
  volume: 0.7,
  prevVolume: 0.7,
  playMode: "list",
  playQueue: [],
  currentIndex: -1,
  heartbeatQueue: [],
  heartbeatLoading: false,
  heartbeatPlayedIds: new Set(),
  playHistory: [],

  localSongs: [],
  importingLocal: false,

  loginInfo: null,
  loginInfos: { netease: null, kugou: null, qqmusic: null, migu: null },
  playbackSource: "netease",

  externalTrack: null,
  externalPlaying: false,
  externalPositionMs: 0,
  externalDurationMs: 0,

  searchResults: [],
  userPlaylists: [],
  userPlaylistsError: "",
  leftPlaylistTracks: [],
  leftPlaylistMeta: null,
  rightPlaylistTracks: [],
  rightPlaylistMeta: null,
  leftPlaylistTotalTrackIds: [],
  rightPlaylistTotalTrackIds: [],
  leftPlaylistLoadingMore: false,
  leftPlaylistLoadingAll: false,
  rightPlaylistLoadingMore: false,
  likedSongIds: new Set(),
  currentLyrics: null,
  recommendations: [],
  recommendSongs: [],
  dailyRecommendPlaylists: [],

  artistSearchResults: [],
  artistSongs: [],
  selectedArtist: null,
  searchingArtists: false,
  loadingArtistSongs: false,
  artistDetail: null,
  artistAlbums: [],
  artistMvs: [],
  albumDetailSongs: [],
  albumDetailMeta: null,
  loadingArtistDetail: false,
  loadingArtistAlbums: false,
  loadingArtistMvs: false,
  loadingAlbumDetail: false,

  playlistSearchResults: [],
  searchingPlaylists: false,

  officialCharts: [],

  playbackQuality: "hires",
  currentQuality: "",
  currentBitrate: 0,
  lyricsFontSize: 18,
  lyricsHighlightColor: "#fff0b8",
  vinylColorMode: "auto",
  vinylCustomColor: "#E8B04B",
  expandedStyle: "modern",
  musicToast: null,
  dynamicEnabled: false,
  coverFilmEffect: false,
  mediaKeysEnabled: true,
  proxyPort: 0,

  // 评论系统
  currentComments: null,
  loadingComments: false,
  sendingComment: false,
  commentError: "",

  desktopLyricsVisible: false,
  desktopLyricsFontSize: 36,
  desktopLyricsFontFamily: "",
  desktopLyricsHighlightColor: "#FFD700",
  desktopLyricsBaseColor: "rgba(255,255,255,0.35)",
  desktopLyricsLineCount: 2,
  desktopLyricsLocked: false,
  desktopLyricsShowTranslation: true,
  desktopLyricsHideUnlockBtn: false,

  searching: false,
  loadingPlaylists: false,
  loadingLeftTracks: false,
  loadingRightTracks: false,
  loadingLyrics: false,

  audioRef: null,

  // ── 本地导入歌曲 Actions ──
  loadLocalSongs: async () => {
    try {
      const s = await getStore();
      const stored = await s.get<Song[]>("localSongs");
      // 恢复封面：有缓存文件路径时用 asset 协议 URL，否则保留原 cover（兼容历史 data URI）
      const restored = (Array.isArray(stored) ? stored : []).map((song) =>
        song._localCoverPath
          ? { ...song, cover: convertFileSrc(song._localCoverPath) }
          : song
      );
      set({ localSongs: restored });
      // 后台自愈：封面缓存文件可能已被清理，失效时从音频重新提取（不阻塞启动）
      void repairLocalCovers(set, get);
    } catch {
      set({ localSongs: [] });
    }
  },

  importLocalSongs: async (paths) => {
    set({ importingLocal: true });
    try {
      const infos = await invoke<LocalSongInfoPayload[]>("import_local_music", { paths });
      return await mergeLocalSongInfos(infos, set, get);
    } catch (e) {
      console.error("Import local songs failed:", e);
      return { count: 0, noCoverCount: 0 };
    } finally {
      set({ importingLocal: false });
    }
  },

  importLocalFolder: async (folder) => {
    set({ importingLocal: true });
    try {
      const infos = await invoke<LocalSongInfoPayload[]>("import_local_music_folder", { folder });
      return await mergeLocalSongInfos(infos, set, get);
    } catch (e) {
      console.error("Import local music folder failed:", e);
      return { count: 0, noCoverCount: 0 };
    } finally {
      set({ importingLocal: false });
    }
  },

  setImportingLocal: (importing) => set({ importingLocal: importing }),

  removeLocalSong: async (id) => {
    const victim = get().localSongs.find((s) => s.id === id);
    set((state) => ({ localSongs: state.localSongs.filter((s) => s.id !== id) }));
    try {
      const s = await getStore();
      const list = get().localSongs.map(serializeLocalSong);
      await s.set("localSongs", list);
      await s.save();
      // 清理孤儿封面缓存：仅当没有其他歌曲共用同一封面文件（同目录歌曲会共享）
      const orphan = victim?._localCoverPath;
      if (orphan && !get().localSongs.some((song) => song._localCoverPath === orphan)) {
        await invoke("delete_cover_cache_files", { paths: [orphan] }).catch(() => {});
      }
    } catch (e) {
      console.error("Remove local song persist failed:", e);
    }
  },

  clearLocalSongs: async () => {
    // 清空前收集全部封面缓存路径（去重），清空后同步删除，避免缓存目录无限膨胀
    const coverPaths = Array.from(
      new Set(
        get()
          .localSongs.map((s) => s._localCoverPath)
          .filter(Boolean)
      )
    ) as string[];
    set({ localSongs: [] });
    try {
      const s = await getStore();
      await s.set("localSongs", []);
      await s.save();
      if (coverPaths.length > 0) {
        await invoke("delete_cover_cache_files", { paths: coverPaths }).catch(() => {});
      }
    } catch (e) {
      console.error("Clear local songs persist failed:", e);
    }
  },

  init: async () => {
    // 加载本地导入的歌曲（不阻塞其余初始化）
    get().loadLocalSongs();

    try {
      const port = await invoke<number>("cmd_get_proxy_port");
      set({ proxyPort: port });
    } catch {
      console.warn("Failed to get proxy port");
    }

    // 加载设置
    try {
      const store = await getStore();
      const vol = await store.get<number>("volume");
      const mode = await store.get<PlayMode>("playMode");
      const quality = await store.get<PlaybackQuality>("quality");
      const fontSize = await store.get<number>("lyricsFontSize");
      const highlightColor = await store.get<string>("lyricsHighlightColor");
      const dlFontSize = await store.get<number>("desktopLyricsFontSize");
      const dlHighlightColor = await store.get<string>("desktopLyricsHighlightColor");
      const dlBaseColor = await store.get<string>("desktopLyricsBaseColor");
      const dlLineCount = await store.get<1 | 2>("desktopLyricsLineCount");
      const dlLocked = await store.get<boolean>("desktopLyricsLocked");
      const dlFontFamily = await store.get<string>("desktopLyricsFontFamily");
      const dlShowTranslation = await store.get<boolean>("desktopLyricsShowTranslation");
      const dlHideUnlockBtn = await store.get<boolean>("desktopLyricsHideUnlockBtn");
      if (vol != null) set({ volume: vol, prevVolume: vol > 0 ? vol : 0.7 });
      if (mode) set({ playMode: mode });
      if (quality) set({ playbackQuality: quality });
      if (fontSize != null) set({ lyricsFontSize: fontSize });
      const expStyle = await store.get<string>("expandedStyle");
      if (highlightColor) set({ lyricsHighlightColor: highlightColor });
      const vinylMode = await store.get<"auto" | "custom">("vinylColorMode");
      const vinylColor = await store.get<string>("vinylCustomColor");
      if (vinylMode === "auto" || vinylMode === "custom") set({ vinylColorMode: vinylMode });
      if (vinylColor) set({ vinylCustomColor: vinylColor });
      // 恢复播放器样式（modern/glass/immersive/spectrum/vinyl 都需还原，否则切换后重启会退回默认 modern）
      if (expStyle === "modern" || expStyle === "glass" || expStyle === "immersive" || expStyle === "spectrum" || expStyle === "vinyl" || expStyle === "cover") set({ expandedStyle: expStyle });
      const dynamic = await store.get<boolean>("dynamicEnabled");
      if (dynamic) set({ dynamicEnabled: true });
      const filmEffect = await store.get<boolean>("coverFilmEffect");
      if (filmEffect) set({ coverFilmEffect: true });
      if (dlFontSize != null) set({ desktopLyricsFontSize: dlFontSize });
      if (dlFontFamily != null) set({ desktopLyricsFontFamily: dlFontFamily });
      if (dlHighlightColor) set({ desktopLyricsHighlightColor: dlHighlightColor });
      if (dlBaseColor) set({ desktopLyricsBaseColor: dlBaseColor });
      if (dlLineCount) set({ desktopLyricsLineCount: dlLineCount });
      if (dlShowTranslation != null) set({ desktopLyricsShowTranslation: dlShowTranslation });
      if (dlHideUnlockBtn != null) set({ desktopLyricsHideUnlockBtn: dlHideUnlockBtn });
      if (dlLocked != null) {
        // 锁定状态仅在当前会话有效，启动时始终重置为 false
        // 防止跨会话残留导致桌面歌词未开但解锁按钮仍在的问题
        if (dlLocked) {
          await store.set("desktopLyricsLocked", false);
          await store.save();
        }
      }

      // 确保内存状态与持久化一致
      set({ desktopLyricsLocked: false });
    } catch {
      // ignore
    }

    // 键盘媒体键控制开关（设置 → 高级 → 媒体键控制，存于 settings.json）
    try {
      const mediaKeys = await store.get<boolean>("nexbox_media_keys_enabled");
      if (mediaKeys != null) set({ mediaKeysEnabled: mediaKeys });
    } catch {
      // ignore
    }

    // 加载缓存的官方榜单（优先显示缓存；封面全空的旧缓存直接弃用，
    // 避免启动时闪现无封面榜单，等 loadOfficialCharts 拉到新数据）
    try {
      const s = await getStore();
      const cached = await s.get<Playlist[]>("officialCharts");
      if (cached && cached.length > 0 && cached.some((c) => c.cover)) {
        set({ officialCharts: cached });
      }
    } catch {}

    // 加载播放源 (重启后恢复上次使用的平台)
    try {
      const source = await invoke<MusicProvider>("music_get_playback_source");
      if (source) set({ playbackSource: source });
    } catch {}

    // 防止 React Strict Mode 双重调用导致重复注册
    if (listenersRegistered) {
      // 无论是否首次注册，确保绑定了页面卸载清理（唯一一次真实注册时绑定）
      bindUnloadCleanup();
      return;
    }
    listenersRegistered = true;
    bindUnloadCleanup();

    // 桌面歌词控制事件监听
    unlistenFns.push(
      await listen<{ action: string; value?: number }>("desktop-lyrics:control", (event) => {
        const { action, value } = event.payload;
        switch (action) {
          case "play-pause":
            get().togglePlay();
            break;
          case "prev":
            get().prevTrack();
            break;
          case "next":
            get().nextTrack();
            break;
          case "toggle-shuffle":
            get().togglePlayMode();
            break;
          case "volume":
            if (typeof value === "number") {
              get().setVolume(value);
            }
            break;
          case "lock":
            get().setDesktopLyricsLocked(true);
            break;
          case "unlock":
            get().setDesktopLyricsLocked(false);
            break;
          case "close":
            get().setDesktopLyricsVisible(false);
            break;
        }
      })
    );

    // 全局音乐控制热键事件监听（上一曲/下一曲/播放暂停）
    unlistenFns.push(
      await listen<{ action: string }>("music-hotkey", (event) => {
        const { action } = event.payload;
        switch (action) {
          case "play-pause":
            get().togglePlay();
            break;
          case "prev":
            get().prevTrack();
            break;
          case "next":
            get().nextTrack();
            break;
        }
      })
    );

    // 系统媒体键 / 音量浮层控制事件（后端 SMTC 转发）→ 控制内部播放器
    // 注意：物理媒体键会同时触发「低层键盘钩子」与系统的 SMTC ButtonPressed 两条链路，
    // 毫秒级内会对同一动作各 emit 一次；这里在窗口期内忽略相同动作的重复事件，避免按一次跳两首/切换两次。
    unlistenFns.push(
      await listen<{ action: string; positionMs?: number }>("smtc:control", (event) => {
        const { action, positionMs } = event.payload;
        if (!shouldHandleSmtc(action)) return;
        // 设置 → 高级 → 键盘媒体键控制 关闭时不响应任何系统媒体控制事件
        if (!get().mediaKeysEnabled) return;
        switch (action) {
          case "play-pause":
            get().togglePlay();
            break;
          case "prev":
            get().prevTrack();
            break;
          case "next":
            get().nextTrack();
            break;
          case "stop":
            cancelVolumeFade();
            get().audioRef?.pause();
            // 直接暂停不经过缓出，需立即推送桌面歌词暂停态，否则副窗口会持续插值漂移
            if (get().desktopLyricsVisible) {
              emit("desktop-lyrics:state", { isPlaying: false, playMode: get().playMode, volume: get().volume });
            }
            break;
          case "seek":
            if (typeof positionMs === "number") {
              get().seekTo(positionMs / 1000);
            }
            break;
        }
      })
    );

    // 外部客户端播放状态监听已移至 DynamicIslandHost（全局常驻组件）注册，
    // 避免仅在音乐页打开时才生效导致灵动岛不显示。

    // 桌面歌词窗口就绪后请求数据
    // 解决窗口首次打开时 emit 早于 listen 注册的时序问题
    unlistenFns.push(
      await listen("desktop-lyrics:request-data", () => {
        setTimeout(() => {
          get().emitDesktopLyricsData();
          get().emitDesktopLyricsSettings();
          emit("desktop-lyrics:state", {
            isPlaying: get().isPlaying,
            playMode: get().playMode,
            volume: get().volume,
          });
        }, 50);
      })
    );

    // 监听解锁按钮显示/隐藏热键事件（Rust 端触发，切换 hideUnlockBtn 开关）
    unlistenFns.push(
      await listen("lyrics:toggle-hide-unlock-btn", () => {
        get().toggleDesktopLyricsHideUnlockBtn();
      })
    );

    // 监听网易云登录成功事件
    unlistenFns.push(
      await listen<LoginInfo>("netease-login-success", async (event) => {
        console.log("[Music] Netease login success", event.payload);
        const info = event.payload;
        if (info && info.logged_in) {
          // 如果当前播放源未登录, 自动切换到网易云
          const currentInfo = get().loginInfos[get().playbackSource];
          const shouldSwitch = !currentInfo?.logged_in;
          if (shouldSwitch) {
            set({ playbackSource: "netease" });
            try { await invoke("music_switch_provider", { provider: "netease" }); } catch {}
          }
          set((s) => ({
            loginInfo: (shouldSwitch || s.playbackSource === "netease") ? info : s.loginInfo,
            loginInfos: { ...s.loginInfos, netease: info }
          }));
          get().loadUserPlaylists();
          get().loadLikedList();
          get().loadOfficialCharts();
          get().loadRecommendations();
        }
      })
    );

    // 监听酷狗登录成功事件
    unlistenFns.push(
      await listen<LoginInfo>("kugou-login-success", async (event) => {
        console.log("[Music] Kugou login success", event.payload);
        const info = event.payload;
        if (info && info.logged_in) {
          // 如果当前播放源未登录, 自动切换到酷狗
          const currentInfo = get().loginInfos[get().playbackSource];
          const shouldSwitch = !currentInfo?.logged_in;
          if (shouldSwitch) {
            set({ playbackSource: "kugou" });
            try { await invoke("music_switch_provider", { provider: "kugou" }); } catch {}
          }
          set((s) => ({
            loginInfo: (shouldSwitch || s.playbackSource === "kugou") ? info : s.loginInfo,
            loginInfos: { ...s.loginInfos, kugou: info }
          }));
          // 加载歌单等数据
          get().loadUserPlaylists();
          get().loadLikedList();
          get().loadRecommendations();
          get().loadOfficialCharts();
        }
      })
    );

    // 监听登录失败事件
    unlistenFns.push(
      await listen<string>("netease-login-failed", (event) => {
        console.error("[Music] Netease login failed:", event.payload);
      })
    );
    unlistenFns.push(
      await listen<string>("kugou-login-failed", (event) => {
        console.error("[Music] Kugou login failed:", event.payload);
      })
    );

    // 监听 QQ 音乐登录成功事件
    unlistenFns.push(
      await listen<LoginInfo>("qqmusic-login-success", async (event) => {
        console.log("[Music] QQ music login success", event.payload);
        const info = event.payload;
        if (info && info.logged_in) {
          const currentInfo = get().loginInfos[get().playbackSource];
          const shouldSwitch = !currentInfo?.logged_in;
          if (shouldSwitch) {
            set({ playbackSource: "qqmusic" });
            try { await invoke("music_switch_provider", { provider: "qqmusic" }); } catch {}
          }
          set((s) => ({
            loginInfo: (shouldSwitch || s.playbackSource === "qqmusic") ? info : s.loginInfo,
            loginInfos: { ...s.loginInfos, qqmusic: info }
          }));
          get().loadUserPlaylists();
          get().loadLikedList();
          get().loadOfficialCharts();
        }
      })
    );
    unlistenFns.push(
      await listen<string>("qqmusic-login-failed", (event) => {
        console.error("[Music] QQ music login failed:", event.payload);
      })
    );

    // 监听咪咕登录成功事件
    unlistenFns.push(
      await listen<LoginInfo>("migu-login-success", async (event) => {
        console.log("[Music] Migu login success", event.payload);
        const info = event.payload;
        if (info && info.logged_in) {
          const currentInfo = get().loginInfos[get().playbackSource];
          const shouldSwitch = !currentInfo?.logged_in;
          if (shouldSwitch) {
            set({ playbackSource: "migu" });
            try { await invoke("music_switch_provider", { provider: "migu" }); } catch {}
          }
          set((s) => ({
            loginInfo: (shouldSwitch || s.playbackSource === "migu") ? info : s.loginInfo,
            loginInfos: { ...s.loginInfos, migu: info }
          }));
          get().loadUserPlaylists();
          get().loadOfficialCharts();
        }
      })
    );
    unlistenFns.push(
      await listen<string>("migu-login-failed", (event) => {
        console.error("[Music] Migu login failed:", event.payload);
      })
    );

    // 加载所有平台登录状态
    await get().loadAllLoginStatuses();
    // 加载当前播放源的歌单
    if (get().loginInfo?.logged_in) {
      get().loadUserPlaylists();
      if (get().playbackSource === "netease") {
        await get().loadLikedList();
        get().loadOfficialCharts();
        get().loadRecommendations();
      } else if (get().playbackSource === "kugou") {
        await get().loadLikedList();
        get().loadOfficialCharts();
        get().loadRecommendations();
      } else if (get().playbackSource === "qqmusic") {
        await get().loadLikedList();
        get().loadOfficialCharts();
      } else if (get().playbackSource === "migu") {
        get().loadOfficialCharts();
        get().loadRecommendations();
      }
    }
  },

  setAudioRef: (audio) => {
    if (smtcBoundAudio === audio) {
      set({ audioRef: audio });
      return;
    }
    // 若已有绑定元素且被替换，先解绑旧元素监听，避免监听残留
    unbindSmtcHandlers(smtcBoundAudio);
    smtcBoundAudio = audio;
    if (audio) {
      // 绑定 SMTC 推送与播放状态同步：无论音频被前端/系统媒体键/外部路径暂停或恢复，
      // isPlaying 都跟随 <audio> 真实状态，保证灵动岛/播放器按钮图标始终同步。
      bindSmtcHandlers(audio);
    }
    set({ audioRef: audio });
  },

  search: async (keywords) => {
    if (!keywords.trim()) return;
    set({ searching: true });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_search"
        : provider === "qqmusic" ? "qq_search"
        : provider === "migu" ? "migu_search"
        : "music_search";
      const results = await invoke<Song[]>(cmd, { keywords, limit: 30 });
      set({ searchResults: results });
    } catch (e) {
      console.error("Search failed:", e);
    } finally {
      set({ searching: false });
    }
  },

  searchArtists: async (keywords) => {
    if (!keywords.trim()) return;
    set({ searchingArtists: true, artistSearchResults: [], selectedArtist: null, artistSongs: [] });
    try {
            const provider = get().playbackSource;
      if (provider === "migu") {
        // 咪咕歌手搜索 (search_all.do singerResultData，无头像)
        const results = await invoke<Artist[]>("migu_artist_search", { keywords, limit: 30 });
        set({ artistSearchResults: results });
        return;
      }
      const cmd = provider === "kugou" ? "kugou_artist_search"
        : provider === "qqmusic" ? "qq_artist_search"
        : "music_artist_search";
      const results = await invoke<Artist[]>(cmd, { keywords, limit: 30 });
      set({ artistSearchResults: results });
    } catch (e) {
      console.error("Artist search failed:", e);
      set({ artistSearchResults: [] });
    } finally {
      set({ searchingArtists: false });
    }
  },

  loadArtistSongs: async (artistId, offset = 0) => {
    set({ loadingArtistSongs: true });
    try {
            const provider = get().playbackSource;
      if (provider === "migu") {
        // 咪咕歌手歌曲接口已失效：按歌手名搜索并精确匹配过滤
        const name = get().selectedArtist?.name || "";
        const songs = await invoke<Song[]>("migu_artist_songs", { artistName: name, limit: 50, offset });
        set((state) => ({
          artistSongs: offset === 0 ? songs : [...state.artistSongs, ...songs],
        }));
        return;
      }
      const cmd = provider === "kugou" ? "kugou_artist_songs"
        : provider === "qqmusic" ? "qq_artist_songs"
        : "music_artist_songs";
      const songs = await invoke<Song[]>(cmd, { artistId, limit: 50, offset });
      set((state) => ({
        artistSongs: offset === 0 ? songs : [...state.artistSongs, ...songs],
      }));
    } catch (e) {
      console.error("Load artist songs failed:", e);
    } finally {
      set({ loadingArtistSongs: false });
    }
  },

  loadArtistDetail: async (artistId) => {
    set({ loadingArtistDetail: true });
    try {
      const detail = await invoke<ArtistDetail>("music_artist_detail", { artistId });
      set({ artistDetail: detail });
    } catch (e) {
      console.error("Load artist detail failed:", e);
      set({ artistDetail: null });
    } finally {
      set({ loadingArtistDetail: false });
    }
  },

  loadArtistAlbums: async (artistId, offset = 0) => {
    set({ loadingArtistAlbums: true });
    try {
      const albums = await invoke<Album[]>("music_artist_albums", { artistId, limit: 50, offset });
      set((state) => ({ artistAlbums: offset === 0 ? albums : [...state.artistAlbums, ...albums] }));
    } catch (e) {
      console.error("Load artist albums failed:", e);
      set({ artistAlbums: [] });
    } finally {
      set({ loadingArtistAlbums: false });
    }
  },

  loadArtistMvs: async (artistId, offset = 0) => {
    set({ loadingArtistMvs: true });
    try {
      const mvs = await invoke<Mv[]>("music_artist_mvs", { artistId, limit: 50, offset });
      set((state) => ({ artistMvs: offset === 0 ? mvs : [...state.artistMvs, ...mvs] }));
    } catch (e) {
      console.error("Load artist mvs failed:", e);
      set({ artistMvs: [] });
    } finally {
      set({ loadingArtistMvs: false });
    }
  },

  loadAlbumDetail: async (albumId) => {
    set({ loadingAlbumDetail: true });
    try {
      const [meta, songs] = await invoke<[Album, Song[]]>("music_album_detail", { albumId });
      set({ albumDetailMeta: meta, albumDetailSongs: songs });
    } catch (e) {
      console.error("Load album detail failed:", e);
      set({ albumDetailMeta: null, albumDetailSongs: [] });
    } finally {
      set({ loadingAlbumDetail: false });
    }
  },

  clearArtistState: () => {
    set({
      artistSearchResults: [],
      artistSongs: [],
      selectedArtist: null,
      artistDetail: null,
      artistAlbums: [],
      artistMvs: [],
      albumDetailSongs: [],
      albumDetailMeta: null,
    });
  },

  searchPlaylists: async (keywords) => {
    if (!keywords.trim()) return;
    set({ searchingPlaylists: true, playlistSearchResults: [] });
    try {
            const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_search"
        : provider === "qqmusic" ? "qq_playlist_search"
        : provider === "migu" ? "migu_playlist_search"
        : "music_playlist_search";
      const results = await invoke<Playlist[]>(cmd, { keywords, limit: 30 });
      // 同步已收藏状态
      const subscribedIds = new Set(get().userPlaylists.filter((pl) => pl.subscribed).map((pl) => pl.id));
      set({
        playlistSearchResults: results.map((pl) =>
          ({ ...pl, subscribed: subscribedIds.has(pl.id) })
        ),
      });
    } catch (e) {
      console.error("Playlist search failed:", e);
      set({ playlistSearchResults: [] });
    } finally {
      set({ searchingPlaylists: false });
    }
  },

  playSong: async (song, queue, opts) => {
    const state = get();
    const audio = state.audioRef;
    if (!audio) return;

    // 用户手势内激活 AudioContext（点击播放歌曲；音域回响真实频谱依赖）
    void ensureAudioContextActive();

    // 立即停止当前播放，防止旧歌在新 URL 获取期间播完并触发 ended → nextTrack 竞态
    // 同时递增序列号，使任何正在飞行中的 playSong 调用被忽略
    const mySeq = ++playSongSeq;
    // 取消进行中的音量渐变，防止旧定时器在切歌后拉低新歌音量甚至误暂停
    cancelVolumeFade();
    audio.pause();
    audio.src = "";

    // 设置播放队列
    if (queue) {
      const idx = queue.findIndex((s) => s.id === song.id);
      set({ playQueue: queue, currentIndex: idx >= 0 ? idx : 0 });
    } else {
      // 手动切歌/上下首：在已有队列中找到对应位置
      const idx = state.playQueue.findIndex((s) => s.id === song.id);
      if (idx >= 0) set({ currentIndex: idx });
    }

    // 参考 Mineradio: 在 URL 获取之前就 dispatch 歌词加载（与 URL 获取并行）
    // 不立即清空 currentLyrics，保留旧歌词避免闪烁，新歌词加载完成后自动替换
    // 用户手动点击时重置跳过计数
    if (!isAutoSkipping) {
      unplayableSkipCount = 0;
    }
    set({ currentSong: song, currentTime: 0, duration: 0, isPlaying: false });
    get().loadLyricsForSong(song);
    // 心动模式：仅对"我喜欢"歌单生效。
    // 用户手动播放（传入 queue）且队列非"我喜欢"歌单时自动降级为随机播放；
    // 心动模式自动续播（不传 queue）不触发降级，保证相似歌曲连续播放
    if (get().playMode === "heartbeat") {
      if (queue && queue.length > 0 && !queue.every((s) => get().likedSongIds.has(s.id))) {
        // 非"我喜欢"歌单：降级为随机播放（保留已播放记录，重新回到心动时不重复）
        set({ playMode: "shuffle", heartbeatQueue: [] });
        getStore().then((s) => s.set("playMode", "shuffle").then(() => s.save()));
        if (get().desktopLyricsVisible) {
          emit("desktop-lyrics:state", { isPlaying: get().isPlaying, playMode: "shuffle", volume: get().volume });
        }
      } else if (song.provider === "netease") {
        // 记录已播放，用于去重（带上限，防止长期挂机集合无限增长）
        set((st) => {
          const next = new Set(st.heartbeatPlayedIds);
          next.add(song.id);
          if (next.size > 2000) {
            return { heartbeatPlayedIds: new Set([song.id]) };
          }
          return { heartbeatPlayedIds: next };
        });
        // 以当前播放歌曲为基准预拉相似歌曲，保证连续播放
        get().loadHeartbeatSongs(song);
      }
    }

    try {
      // ── 本地歌曲：直接播放本地文件，无需获取网络 URL ──
      if (song.provider === "local") {
        const localPath = song._localPath || song.hash;
        if (!localPath) {
          set({ isPlaying: false });
          return;
        }
        if (mySeq !== playSongSeq) return;
        const audioUrl = convertFileSrc(localPath);
        audio.src = audioUrl;
        audio.volume = state.volume;
        try {
          await audio.play();
          if (mySeq !== playSongSeq) return;
          set({ isPlaying: true, currentQuality: "本地", currentBitrate: 0 });
          pushSmtc(true);
          recordPlayHistory(song, !!opts?.fromHistory);
        } catch (err) {
          if (mySeq !== playSongSeq) return;
          console.error("Play local song failed:", err);
          set({
            isPlaying: false,
            musicToast: { type: "warning", message: "无法播放该本地音频文件，请检查格式是否受支持" },
          });
        }
        return;
      }

      // 根据歌曲 provider 调用对应 API
      const result = song.provider === "kugou"
        ? await invoke<SongUrlResult>("kugou_song_url", {
            hash: song.hash || song.id,
            albumId: song.album_id,
            albumAudioId: song.album_audio_id,
            quality: state.playbackQuality,
            hqHash: song.hq_hash,
            sqHash: song.sq_hash,
            resHash: song.res_hash,
          })
        : song.provider === "qqmusic"
        ? await invoke<SongUrlResult>("qq_song_url", {
            mid: song.mid || song.id,
            mediaMid: song.media_mid,
            quality: state.playbackQuality,
          })
        : song.provider === "migu"
        ? await invoke<SongUrlResult>("migu_song_url", {
            contentId: song.content_id || song.id,
            copyrightId: song.id,
            quality: state.playbackQuality,
          })
        : await invoke<SongUrlResult>("music_song_url", {
            id: song.id,
            quality: state.playbackQuality,
          });

      // 检查是否有更新的 playSong 调用覆盖了本次请求
      if (mySeq !== playSongSeq) return;

      if (!result.playable || !result.url) {
        console.warn("Cannot play:", result.message);
        set({ isPlaying: false });

        // 检查是否因版权/会员限制无法播放，自动跳过
        if (unplayableSkipCount < 10) {
          unplayableSkipCount++;
          isAutoSkipping = true;
          // 判断是否版权相关
          const msg = result.message || "";
          const isCopyright = msg.includes("版权") || msg.includes("会员") || msg.includes("copyright") || result.reason === "QQ_URL_UNAVAILABLE";
          set({
            musicToast: {
              type: "warning",
              message: isCopyright ? "无版权" : (result.message || "无法播放"),
            },
          });
          // 延迟跳转，让 toast 可见
          setTimeout(() => {
            if (get().playQueue.length > 1) {
              get().nextTrack();
            }
          }, 800);
        } else {
          // 连续跳过太多，停下
          isAutoSkipping = false;
          unplayableSkipCount = 0;
          set({
            musicToast: {
              type: "warning",
              message: "当前队列中多首歌曲无法播放，已停止自动切换",
            },
          });
        }
        return;
      }

      const audioUrl = await getProxyAudioUrl(result.url, state.proxyPort);

      if (mySeq !== playSongSeq) return;

      audio.src = audioUrl;
      audio.volume = state.volume;
      await audio.play();

      if (mySeq !== playSongSeq) return;

      set({ isPlaying: true, proxyPort: state.proxyPort || get().proxyPort, currentQuality: result.quality, currentBitrate: result.br });
      pushSmtc(true);
      recordPlayHistory(song, !!opts?.fromHistory);
      // 推送桌面歌词状态
      // 歌词数据已由 loadLyricsForSong 并行加载完成后自动 emit，此处不再重复 emitDesktopLyricsData
      // 避免 loadLyricsForSong 未完成时推送旧歌词
      if (get().desktopLyricsVisible) {
        emit("desktop-lyrics:state", { isPlaying: true, playMode: get().playMode, volume: get().volume });
      }
      // 歌词已在 playSong 开始时并行加载，此处不再重复调用
    } catch (e) {
      if (mySeq !== playSongSeq) return;
      console.error("Play failed:", e);
    }
  },

  togglePlay: async () => {
    const { audioRef, isPlaying } = get();
    if (!audioRef) return;
    if (isPlaying) {
      // 缓出：UI 立即翻转为暂停态，音量渐弱结束后才真正 pause；
      // 渐弱期间再次点播放会取消本次渐变并从当前音量继续渐入
      volumeFadeIsPause = true;
      startVolumeFade(audioRef, 0, () => {
        audioRef.pause();
        volumeFadeIsPause = false;
        // 恢复音量，避免未走渐入的播放路径（如单曲循环重播）静音
        audioRef.volume = useMusicStore.getState().volume;
        // 真正暂停后才推送桌面歌词暂停态（缓出期间歌词继续跟随音乐）
        if (get().desktopLyricsVisible) {
          emit("desktop-lyrics:state", { isPlaying: false, playMode: get().playMode, volume: get().volume });
        }
      });
      set({ isPlaying: false });
      pushSmtc(true);
      // 桌面歌词状态延迟到真正暂停时才推送 isPlaying:false：
      // 缓出期间副窗口保持插值与逐字填充，歌词跟随渐弱的音乐（时间校正如常）
    } else {
      // 用户手势内激活 AudioContext：音域回响真实频谱依赖它（resume 成功后才接管 audio）
      void ensureAudioContextActive();
      // 从暂停恢复：从 0 渐入；渐出中途恢复：从当前音量继续渐入
      if (audioRef.paused) audioRef.volume = 0;
      volumeFadeIsPause = false;
      try {
        await audioRef.play();
        startVolumeFade(audioRef, get().volume);
        set({ isPlaying: true });
        pushSmtc(true);
        if (get().desktopLyricsVisible) {
          emit("desktop-lyrics:state", { isPlaying: true, playMode: get().playMode, volume: get().volume });
        }
      } catch {
        // 播放失败，URL 可能已过期，尝试重新获取
        cancelVolumeFade();
        audioRef.volume = get().volume;
        const state = get();
        const song = state.currentSong;
        if (song) {
          const savedTime = audioRef.currentTime;
          // 本地歌曲：直接重设 src 重试
          if (song.provider === "local") {
            const localPath = song._localPath || song.hash;
            if (localPath) {
              audioRef.src = convertFileSrc(localPath);
              audioRef.currentTime = savedTime;
              try {
                await audioRef.play();
                set({ isPlaying: true });
                if (get().desktopLyricsVisible) {
                  emit("desktop-lyrics:state", { isPlaying: true, playMode: get().playMode, volume: get().volume });
                }
                return;
              } catch {}
            }
            set({ isPlaying: false });
            if (get().desktopLyricsVisible) {
              emit("desktop-lyrics:state", { isPlaying: false, playMode: get().playMode, volume: get().volume });
            }
            return;
          }
          try {
            const result = song.provider === "kugou"
              ? await invoke<SongUrlResult>("kugou_song_url", {
                  hash: song.hash || song.id,
                  albumId: song.album_id,
                  albumAudioId: song.album_audio_id,
                  quality: state.playbackQuality,
                  hqHash: song.hq_hash,
                  sqHash: song.sq_hash,
                  resHash: song.res_hash,
                })
              : song.provider === "qqmusic"
              ? await invoke<SongUrlResult>("qq_song_url", {
                  mid: song.mid || song.id,
                  mediaMid: song.media_mid,
                  quality: state.playbackQuality,
                })
              : song.provider === "migu"
              ? await invoke<SongUrlResult>("migu_song_url", {
                  contentId: song.content_id || song.id,
                  copyrightId: song.id,
                  quality: state.playbackQuality,
                })
              : await invoke<SongUrlResult>("music_song_url", {
                  id: song.id,
                  quality: state.playbackQuality,
                });
            if (result.playable && result.url) {
              const audioUrl = await getProxyAudioUrl(result.url, state.proxyPort);
              audioRef.src = audioUrl;
              audioRef.currentTime = savedTime;
              await audioRef.play();
              set({ isPlaying: true });
              return;
            }
          } catch {}
        }
        set({ isPlaying: false });
        if (get().desktopLyricsVisible) {
          emit("desktop-lyrics:state", { isPlaying: false, playMode: get().playMode, volume: get().volume });
        }
      }
    }
  },

  nextTrack: () => {
    const { playQueue, currentIndex, playMode, audioRef, heartbeatQueue } = get();
    if (playQueue.length === 0) return;

    // 单曲循环：重新播放当前歌曲
    if (playMode === "one") {
      if (audioRef) {
        audioRef.currentTime = 0;
        audioRef.play().catch(() => {});
      }
      return;
    }

    // 心动模式：从相似歌曲队列取下一首，队列空则回退随机（队列会由 playSong 自动补充）
    if (playMode === "heartbeat") {
      let heartbeatNext: Song | null = null;
      if (heartbeatQueue.length > 0) {
        const [first, ...rest] = heartbeatQueue;
        heartbeatNext = first;
        set({ heartbeatQueue: rest });
      } else {
        heartbeatNext = playQueue[Math.floor(Math.random() * playQueue.length)] || null;
      }
      if (heartbeatNext) {
        get().playSong(heartbeatNext);
      }
      return;
    }

    let next: number;
    if (playMode === "shuffle") {
      // 随机只作用于「下一首」：队列多于一首时排除当前曲目，避免随机到正在播放的同一首
      if (playQueue.length > 1 && currentIndex >= 0) {
        next = Math.floor(Math.random() * (playQueue.length - 1));
        if (next >= currentIndex) next += 1;
      } else {
        next = Math.floor(Math.random() * playQueue.length);
      }
    } else {
      next = currentIndex + 1;
      if (next >= playQueue.length) next = 0;
    }

    const song = playQueue[next];
    if (song) {
      get().playSong(song);
    }
  },

  prevTrack: () => {
    // 「上一首」不受随机影响：优先回溯真实播放历史栈，连按可逐级回退
    const history = get().playHistory;
    if (history.length > 0) {
      const prevSong = history[history.length - 1];
      set({ playHistory: history.slice(0, -1) });
      get().playSong(prevSong, undefined, { fromHistory: true });
      return;
    }
    // 无历史（如刚启动）时回退队列顺序
    const { playQueue, currentIndex } = get();
    if (playQueue.length === 0) return;
    let prev = currentIndex - 1;
    if (prev < 0) prev = playQueue.length - 1;
    const song = playQueue[prev];
    if (song) {
      get().playSong(song);
    }
  },

  seekTo: (time) => {
    const { audioRef, isPlaying } = get();
    if (audioRef) {
      audioRef.currentTime = time;
      set({ currentTime: time });
      pushSmtc(true);
      // 在线音频 seek 后需要重新缓冲，浏览器可能自动暂停
      // 如果之前是在播放状态，确保 seek 后继续播放
      if (isPlaying) {
        audioRef.play().catch(() => {});
      }
    }
  },

  // 外部客户端控制（SMTC 会话，供灵动岛在接管外部播放时使用）
  externalControl: (action, valueMs) => {
    // 播放/暂停：先本地乐观翻转图标（外部平台有淡出/缓动，状态回调慢），
    // 避免按钮反馈延迟；后续每秒 SMTC 轮询会自动校正为平台真实状态。
    if (action === "play-pause") {
      set((s) => ({ externalPlaying: !s.externalPlaying }));
    }
    invoke("external_control", {
      action,
      valueMs: action === "seek" ? valueMs ?? 0 : undefined,
    }).catch(() => {});
  },

  setVolume: (v) => {
    const { audioRef, prevVolume } = get();
    if (audioRef) {
      if (volumeFadeTimer && volumeFadeIsPause && !audioRef.paused) {
        // 渐出暂停中途调音量：视为放弃渐出，立即暂停并应用新音量
        cancelVolumeFade();
        audioRef.pause();
      } else {
        // 渐入中途/无渐变：直接应用新音量
        cancelVolumeFade();
      }
      audioRef.volume = v;
    }
    set({ volume: v, prevVolume: v > 0 ? v : prevVolume });
    getStore().then((s) => s.set("volume", v).then(() => s.save()));
    // 桌面歌词可见时回推音量，保持调节条状态同步
    if (get().desktopLyricsVisible) {
      emit("desktop-lyrics:state", { isPlaying: get().isPlaying, playMode: get().playMode, volume: v });
    }
  },

  // 心动模式：根据基准歌曲拉取相似歌曲并追加到心动队列（自动去重已播放/已排队歌曲）
  loadHeartbeatSongs: async (baseSong) => {
    if (!baseSong || baseSong.provider !== "netease") return;
    const state = get();
    // 一次只允许一个拉取任务，避免快速切歌时并发请求造成队列错乱
    if (state.heartbeatLoading) return;
    set({ heartbeatLoading: true });
    try {
      const songs = await invoke<Song[]>("music_simi_song", { id: baseSong.id, limit: 20 });
      // 去重：排除当前播放队列、心动队列、已播放过的相似歌曲
      const playedIds = new Set<string>([...state.playQueue.map((s) => s.id), ...state.heartbeatPlayedIds]);
      const queueIds = new Set(state.heartbeatQueue.map((s) => s.id));
      const currentId = get().currentSong?.id;
      const fresh = songs.filter(
        (s) => s.id && !playedIds.has(s.id) && !queueIds.has(s.id) && s.id !== currentId
      );
      if (fresh.length > 0) {
        set((st) => ({ heartbeatQueue: [...st.heartbeatQueue, ...fresh] }));
      }
    } catch (e) {
      console.error("[Music] loadHeartbeatSongs failed:", e);
    } finally {
      set({ heartbeatLoading: false });
    }
  },

  togglePlayMode: () => {
    const modes: PlayMode[] = ["list", "heartbeat", "shuffle", "one"];
    const current = modes.indexOf(get().playMode);
    const next = modes[(current + 1) % modes.length];
    // 心动模式：仅"我喜欢"歌单（网易云）可用，否则自动切换为随机播放
    if (next === "heartbeat") {
      const st = get();
      const usable = st.playbackSource === "netease"
        && st.playQueue.length > 0
        && st.playQueue.every((s) => st.likedSongIds.has(s.id));
      if (!usable) {
        set({ playMode: "shuffle", heartbeatQueue: [] });
        getStore().then((s) => s.set("playMode", "shuffle").then(() => s.save()));
        if (get().desktopLyricsVisible) {
          emit("desktop-lyrics:state", { isPlaying: get().isPlaying, playMode: "shuffle", volume: get().volume });
        }
        return;
      }
    }
    // 离开心动模式时清空相似歌曲队列，防止残留旧数据
    set({ playMode: next, heartbeatQueue: next === "heartbeat" ? get().heartbeatQueue : [] });
    getStore().then((s) => s.set("playMode", next).then(() => s.save()));
    // 进入心动模式：以当前歌曲为基准预拉相似歌曲
    if (next === "heartbeat") {
      get().loadHeartbeatSongs(get().currentSong);
    }
    if (get().desktopLyricsVisible) {
      emit("desktop-lyrics:state", { isPlaying: get().isPlaying, playMode: next, volume: get().volume });
    }
  },

  setCurrentTime: (t) => set({ currentTime: t }),
  setDuration: (d) => set({ duration: d }),

  setPlaybackQuality: async (quality) => {
    const state = get();
    set({ playbackQuality: quality });
    getStore().then((s) => s.set("quality", quality).then(() => s.save()));

    if (state.currentSong && state.audioRef && state.audioRef.src) {
      try {
        const song = state.currentSong;
        const result = song.provider === "kugou"
          ? await invoke<SongUrlResult>("kugou_song_url", {
              hash: song.hash || song.id,
              albumId: song.album_id,
              albumAudioId: song.album_audio_id,
              quality,
              hqHash: song.hq_hash,
              sqHash: song.sq_hash,
              resHash: song.res_hash,
            })
          : song.provider === "qqmusic"
          ? await invoke<SongUrlResult>("qq_song_url", {
              mid: song.mid || song.id,
              mediaMid: song.media_mid,
              quality,
            })
          : song.provider === "migu"
          ? await invoke<SongUrlResult>("migu_song_url", {
              contentId: song.content_id || song.id,
              copyrightId: song.id,
              quality,
            })
          : await invoke<SongUrlResult>("music_song_url", {
              id: song.id,
              quality,
            });
        if (result.playable && result.url) {
          const resumeAt = state.audioRef.currentTime;
          const wasPlaying = state.isPlaying;
          const audioUrl = await getProxyAudioUrl(result.url, state.proxyPort);
          state.audioRef.src = audioUrl;
          state.audioRef.currentTime = resumeAt;
          if (wasPlaying) state.audioRef.play().catch(() => {});
          set({ currentQuality: result.quality, currentBitrate: result.br });
        }
      } catch (e) {
        console.error("Failed to switch quality:", e);
      }
    }
  },

  setLyricsFontSize: async (size) => {
    set({ lyricsFontSize: size });
    getStore().then((s) => s.set("lyricsFontSize", size).then(() => s.save()));
  },

  setLyricsHighlightColor: async (color) => {
    set({ lyricsHighlightColor: color });
    getStore().then((s) => s.set("lyricsHighlightColor", color).then(() => s.save()));
  },

  setExpandedStyle: async (style) => {
    set({ expandedStyle: style });
    getStore().then((s) => s.set("expandedStyle", style).then(() => s.save()));
  },

  setVinylColorMode: async (mode) => {
    set({ vinylColorMode: mode });
    getStore().then((s) => s.set("vinylColorMode", mode).then(() => s.save()));
  },

  setVinylCustomColor: async (color) => {
    set({ vinylCustomColor: color });
    getStore().then((s) => s.set("vinylCustomColor", color).then(() => s.save()));
  },

  setDynamicEnabled: async (enabled) => {
    set({ dynamicEnabled: enabled });
    getStore().then((s) => s.set("dynamicEnabled", enabled).then(() => s.save()));
  },

  setCoverFilmEffect: async (enabled) => {
    set({ coverFilmEffect: enabled });
    getStore().then((s) => s.set("coverFilmEffect", enabled).then(() => s.save()));
  },

  // 键盘媒体键控制（仅更新内存态；持久化由设置页「高级」开关负责）
  setMediaKeysEnabled: (enabled) => {
    set({ mediaKeysEnabled: enabled });
  },

  // 立即重推一次当前播放状态到系统媒体会话（无歌时清除会话）
  refreshSmtc: () => {
    pushSmtc(true);
  },

  // ══ 桌面歌词 Actions ══
  toggleDesktopLyrics: async () => {
    const visible = !get().desktopLyricsVisible;
    await get().setDesktopLyricsVisible(visible);
  },

  setDesktopLyricsVisible: async (visible) => {
    set({ desktopLyricsVisible: visible });
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const win = await WebviewWindow.getByLabel("desktop-lyrics");
      if (win) {
        if (visible) {
          await win.show();
          await win.setFocus();
          startTimeSync();
          // 发送当前歌曲数据（此时新窗口可能还没挂载，listener 尚未注册）
          get().emitDesktopLyricsData();
          get().emitDesktopLyricsSettings();
          emit("desktop-lyrics:state", {
            isPlaying: get().isPlaying,
            playMode: get().playMode,
            volume: get().volume,
          });
          // 延迟重推：给桌面歌词窗口足够时间完成 React 挂载和 listener 注册
          [1500, 3000].forEach((ms) => {
            setTimeout(() => {
              if (get().desktopLyricsVisible) {
                get().emitDesktopLyricsData();
                get().emitDesktopLyricsSettings();
                emit("desktop-lyrics:state", {
                  isPlaying: get().isPlaying,
                  playMode: get().playMode,
                  volume: get().volume,
                });
              }
            }, ms);
          });
        } else {
          await win.hide();
          stopTimeSync();
          // 关闭桌面歌词时同时重置锁定状态，防止重启后残留锁定状态
          await get().setDesktopLyricsLocked(false);
          // 通知桌面歌词页面解锁并停止轮询，防止在隐藏窗口后仍显示解锁按钮
          emit("desktop-lyrics:settings", {
            fontSize: get().desktopLyricsFontSize,
            highlightColor: get().desktopLyricsHighlightColor,
            baseColor: get().desktopLyricsBaseColor,
            lineCount: get().desktopLyricsLineCount,
            isLocked: false,
            showTranslation: get().desktopLyricsShowTranslation,
            hideUnlockBtn: get().desktopLyricsHideUnlockBtn,
          });
          try {
            await invoke("hide_lyrics_unlock_btn");
          } catch {
            // ignore
          }
        }
      }
    } catch (e) {
      console.error("[DesktopLyrics] toggle failed:", e);
    }
  },

  setDesktopLyricsFontSize: async (size) => {
    set({ desktopLyricsFontSize: size });
    getStore().then((s) => s.set("desktopLyricsFontSize", size).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsFontFamily: async (family) => {
    set({ desktopLyricsFontFamily: family });
    getStore().then((s) => s.set("desktopLyricsFontFamily", family).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsHighlightColor: async (color) => {
    set({ desktopLyricsHighlightColor: color });
    getStore().then((s) => s.set("desktopLyricsHighlightColor", color).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsBaseColor: async (color) => {
    set({ desktopLyricsBaseColor: color });
    getStore().then((s) => s.set("desktopLyricsBaseColor", color).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsLineCount: async (count) => {
    set({ desktopLyricsLineCount: count });
    getStore().then((s) => s.set("desktopLyricsLineCount", count).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsLocked: async (locked) => {
    set({ desktopLyricsLocked: locked });
    const s = await getStore();
    await s.set("desktopLyricsLocked", locked);
    await s.save();
  },

  setDesktopLyricsShowTranslation: async (show) => {
    set({ desktopLyricsShowTranslation: show });
    getStore().then((s) => s.set("desktopLyricsShowTranslation", show).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  setDesktopLyricsHideUnlockBtn: async (hide) => {
    set({ desktopLyricsHideUnlockBtn: hide });
    getStore().then((s) => s.set("desktopLyricsHideUnlockBtn", hide).then(() => s.save()));
    get().emitDesktopLyricsSettings();
  },

  toggleDesktopLyricsHideUnlockBtn: async () => {
    await get().setDesktopLyricsHideUnlockBtn(!get().desktopLyricsHideUnlockBtn);
  },

  emitDesktopLyricsSettings: () => {
    const s = get();
    emit("desktop-lyrics:settings", {
      fontSize: s.desktopLyricsFontSize,
      fontFamily: s.desktopLyricsFontFamily,
      highlightColor: s.desktopLyricsHighlightColor,
      baseColor: s.desktopLyricsBaseColor,
      lineCount: s.desktopLyricsLineCount,
      isLocked: s.desktopLyricsLocked,
      showTranslation: s.desktopLyricsShowTranslation,
      hideUnlockBtn: s.desktopLyricsHideUnlockBtn,
    });
  },

  emitDesktopLyricsData: () => {
    const s = get();
    const karaokeLines = buildKaraokeLines(s.currentLyrics);
    emit("desktop-lyrics:data", {
      song: s.currentSong,
      karaokeLines,
      currentTime: s.audioRef?.currentTime ?? 0,
      isPlaying: s.isPlaying,
    });
  },

  loginStatus: async () => {
    await get().loginStatusFor(get().playbackSource);
  },

  loginStatusFor: async (provider) => {
    try {
      const cmd = provider === "kugou" ? "kugou_login_status"
        : provider === "qqmusic" ? "qq_login_status"
        : provider === "migu" ? "migu_login_status"
        : "music_login_status";
      const info = await invoke<LoginInfo>(cmd);
      set((s) => ({
        loginInfos: { ...s.loginInfos, [provider]: info },
        loginInfo: s.playbackSource === provider ? info : s.loginInfo,
      }));
    } catch {
      set((s) => ({
        loginInfos: { ...s.loginInfos, [provider]: null },
        loginInfo: s.playbackSource === provider ? null : s.loginInfo,
      }));
    }
  },

  loadAllLoginStatuses: async () => {
    try {
      const statuses = await invoke<Record<string, LoginInfo>>("music_get_login_statuses");
      const currentSource = get().playbackSource;
      const loginInfos: Record<MusicProvider, LoginInfo | null> = {
        netease: statuses.netease || null,
        kugou: statuses.kugou || null,
        qqmusic: statuses.qqmusic || null,
        migu: statuses.migu || null,
      };
      set({
        loginInfos,
        loginInfo: loginInfos[currentSource],
      });
    } catch {
      // fallback to individual calls
      await get().loginStatusFor("netease");
      await get().loginStatusFor("kugou");
    }
  },

  loginWithCookie: async (cookie) => {
    try {
      const info = await invoke<LoginInfo>("music_login_cookie", { cookie });
      set((s) => ({
        loginInfos: { ...s.loginInfos, netease: info },
        loginInfo: s.playbackSource === "netease" ? info : s.loginInfo,
      }));
      if (info.logged_in) {
        get().loadUserPlaylists();
        get().loadLikedList();
      }
      return info.logged_in;
    } catch {
      return false;
    }
  },

  logout: async () => {
    await get().logoutFor(get().playbackSource);
  },

  logoutFor: async (provider) => {
    try {
    const cmd = provider === "kugou" ? "kugou_logout"
      : provider === "qqmusic" ? "qq_logout"
      : provider === "migu" ? "migu_logout"
      : "music_logout";
    await invoke(cmd);
      set((s) => ({
        loginInfos: { ...s.loginInfos, [provider]: null },
        loginInfo: s.playbackSource === provider ? null : s.loginInfo,
        userPlaylists: s.playbackSource === provider ? [] : s.userPlaylists,
        likedSongIds: s.playbackSource === provider ? new Set() : s.likedSongIds,
      }));
    } catch {
      // ignore
    }
  },

  openLoginWindow: async (provider?) => {
    const target = provider || get().playbackSource;
    // 如果已登录该平台，先退出
    if (get().loginInfos[target]?.logged_in) {
      await get().logoutFor(target);
    }
    try {
      await invoke("music_open_login_window", { provider: target });
    } catch (e) {
      console.error(`Failed to open ${target} login window:`, e);
    }
  },

  switchPlaybackSource: async (provider) => {
    set({ playbackSource: provider });
    try {
      await invoke("music_switch_provider", { provider });
    } catch {}
    // 心动模式仅支持网易云：切换到其他平台时自动降级为随机播放
    if (provider !== "netease" && get().playMode === "heartbeat") {
      set({ playMode: "shuffle", heartbeatQueue: [] });
      getStore().then((s) => s.set("playMode", "shuffle").then(() => s.save()));
    }
    // 更新 loginInfo 为当前平台的登录状态
    const info = get().loginInfos[provider];
    // 切换平台时立即清空榜单/推荐，避免旧平台数据残留闪烁
    set({ loginInfo: info, userPlaylists: [], userPlaylistsError: "", officialCharts: [], recommendations: [], recommendSongs: [], dailyRecommendPlaylists: [] });
    // 重新加载当前平台的歌单
    if (info?.logged_in) {
      get().loadUserPlaylists();
      if (provider === "netease") {
        await get().loadLikedList();
        get().loadOfficialCharts();
        get().loadRecommendations();
      } else if (provider === "kugou") {
        await get().loadLikedList();
        get().loadRecommendations();
        get().loadOfficialCharts();
      } else if (provider === "qqmusic") {
        await get().loadLikedList();
        get().loadOfficialCharts();
      } else if (provider === "migu") {
        get().loadOfficialCharts();
        get().loadRecommendations();
      }
    }
  },

  loadUserPlaylists: async () => {
    await get().loadUserPlaylistsFor(get().playbackSource);
  },

  loadUserPlaylistsFor: async (provider) => {
    set({ loadingPlaylists: true, userPlaylistsError: "" });
    try {
      const cmd = provider === "kugou" ? "kugou_user_playlists"
        : provider === "qqmusic" ? "qq_user_playlists"
        : provider === "migu" ? "migu_user_playlists"
        : "music_user_playlist";
      const playlists = await invoke<Playlist[]>(cmd);
      set({ userPlaylists: playlists, userPlaylistsError: "" });
    } catch (e) {
      const msg = typeof e === "string" && e ? e : "歌单获取失败，登录可能已过期";
      set({ userPlaylists: [], userPlaylistsError: msg });
      // 酷狗/QQ/咪咕 登录态失效时刷新登录状态，让界面提示重新登录
      if (provider === "kugou" || provider === "qqmusic" || provider === "migu") {
        get().loginStatusFor(provider);
      }
    } finally {
      set({ loadingPlaylists: false });
    }
  },

  loadLeftPlaylistTracks: async (id) => {
    set({ loadingLeftTracks: true, leftPlaylistTracks: [], leftPlaylistMeta: null });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_tracks"
        : provider === "qqmusic" ? "qq_playlist_tracks"
        : provider === "migu" ? "migu_playlist_tracks"
        : "music_playlist_tracks";
      const [meta, songs] = await invoke<[Playlist, Song[]]>(cmd, { id });
      set({ leftPlaylistMeta: meta, leftPlaylistTracks: songs });
      // 后台加载全部剩余 → 只追加到播放列表，不塞进歌单
      if (provider === "netease") {
        batchLoadToQueue(id, songs, meta.track_count);
      }
    } catch {
      set({ leftPlaylistTracks: [] });
    } finally {
      set({ loadingLeftTracks: false });
    }
  },


  loadMoreLeftPlaylistTracks: async () => {
    const state = get();
    const id = state.leftPlaylistMeta?.id;
    const total = state.leftPlaylistMeta?.track_count ?? 0;
    if (!id || state.leftPlaylistLoadingMore) return;
    const start = state.leftPlaylistTracks.length;
    if (start >= total) return;
    set({ leftPlaylistLoadingMore: true });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_tracks_range"
        : provider === "qqmusic" ? "qq_playlist_tracks_range"
        : provider === "migu" ? "migu_playlist_tracks_range"
        : "music_playlist_tracks_range";
      const songs = await invoke<Song[]>(cmd, { id, start, count: 50 });
      set((s) => {
        // 如果当前播放队列是从左侧歌单播放的，同步追加（去重）
        const shouldSync = s.playQueue.length > 0
          && s.playQueue[s.currentIndex]?.id === s.currentSong?.id
          && s.leftPlaylistTracks.length > 0
          && s.leftPlaylistTracks[0]?.id === s.playQueue[0]?.id;
        if (shouldSync) {
          const queueIds = new Set(s.playQueue.map((q) => q.id));
          const unique = songs.filter((song) => !queueIds.has(song.id));
          return {
            leftPlaylistTracks: [...s.leftPlaylistTracks, ...songs],
            playQueue: unique.length > 0 ? [...s.playQueue, ...unique] : s.playQueue,
          };
        }
        return {
          leftPlaylistTracks: [...s.leftPlaylistTracks, ...songs],
        };
      });
    } catch (e) {
      console.error("loadMore left failed:", e);
    } finally {
      set({ leftPlaylistLoadingMore: false });
    }
  },

  // 歌单内搜索：一次性把当前歌单全部剩余曲目拉齐到 leftPlaylistTracks
  loadAllLeftPlaylistTracks: async () => {
    const state = get();
    const id = state.leftPlaylistMeta?.id;
    const total = state.leftPlaylistMeta?.track_count ?? 0;
    if (!id || total === 0) return;
    // 已加载完或已在加载中则直接返回
    if (state.leftPlaylistTracks.length >= total || state.leftPlaylistLoadingAll) return;
    set({ leftPlaylistLoadingAll: true });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_tracks_range"
        : provider === "qqmusic" ? "qq_playlist_tracks_range"
        : provider === "migu" ? "migu_playlist_tracks_range"
        : "music_playlist_tracks_range";
      // 循环拉取剩余所有页；任一页返回空或报错即退出，避免死循环
      for (;;) {
        const s = get();
        const start = s.leftPlaylistTracks.length;
        if (s.leftPlaylistMeta?.id !== id || start >= total) break;
        let songs: Song[] = [];
        try {
          songs = await invoke<Song[]>(cmd, { id, start, count: 50 });
        } catch (e) {
          console.error("loadAll left page failed:", e);
          break;
        }
        if (songs.length === 0) break;
        set((s) => ({
          leftPlaylistTracks: [...s.leftPlaylistTracks, ...songs],
        }));
      }
    } finally {
      set({ leftPlaylistLoadingAll: false });
    }
  },

  loadRightPlaylistTracks: async (id) => {
    set({ loadingRightTracks: true, rightPlaylistTracks: [], rightPlaylistMeta: null });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_tracks"
        : provider === "qqmusic" ? "qq_playlist_tracks"
        : provider === "migu" ? "migu_playlist_tracks"
        : "music_playlist_tracks";
      const [meta, songs] = await invoke<[Playlist, Song[]]>(cmd, { id });
      set({ rightPlaylistMeta: meta, rightPlaylistTracks: songs });
      if (provider === "netease") {
        batchLoadToQueue(id, songs, meta.track_count);
      }
    } catch {
      set({ rightPlaylistTracks: [] });
    } finally {
      set({ loadingRightTracks: false });
    }
  },

  // QQ/咪咕榜单歌曲加载 (榜单不是普通歌单, 走各自榜单歌曲接口; meta 由前端提供)
  loadRightRankTracks: async (rankId) => {
    set({ loadingRightTracks: true, rightPlaylistTracks: [] });
    try {
      const provider = get().playbackSource;
      const songs = provider === "migu"
        ? await invoke<Song[]>("migu_rank_songs", { rankId, limit: 100 })
        : await invoke<Song[]>("qq_rank_songs", { rankId, limit: 99999 });
      set({ rightPlaylistTracks: songs });
    } catch {
      set({ rightPlaylistTracks: [] });
    } finally {
      set({ loadingRightTracks: false });
    }
  },

  loadMoreRightPlaylistTracks: async () => {
    const state = get();
    const id = state.rightPlaylistMeta?.id;
    const total = state.rightPlaylistMeta?.track_count ?? 0;
    if (!id || state.rightPlaylistLoadingMore) return;
    const start = state.rightPlaylistTracks.length;
    if (start >= total) return;
    set({ rightPlaylistLoadingMore: true });
    try {
      const provider = get().playbackSource;
      const cmd = provider === "kugou" ? "kugou_playlist_tracks_range"
        : provider === "qqmusic" ? "qq_playlist_tracks_range"
        : provider === "migu" ? "migu_playlist_tracks_range"
        : "music_playlist_tracks_range";
      const songs = await invoke<Song[]>(cmd, { id, start, count: 50 });
      set((s) => {
        const shouldSync = s.playQueue.length > 0
          && s.playQueue[s.currentIndex]?.id === s.currentSong?.id
          && s.rightPlaylistTracks.length > 0
          && s.rightPlaylistTracks[0]?.id === s.playQueue[0]?.id;
        if (shouldSync) {
          const queueIds = new Set(s.playQueue.map((q) => q.id));
          const unique = songs.filter((song) => !queueIds.has(song.id));
          return {
            rightPlaylistTracks: [...s.rightPlaylistTracks, ...songs],
            playQueue: unique.length > 0 ? [...s.playQueue, ...unique] : s.playQueue,
          };
        }
        return {
          rightPlaylistTracks: [...s.rightPlaylistTracks, ...songs],
        };
      });
    } catch (e) {
      console.error("loadMore right failed:", e);
    } finally {
      set({ rightPlaylistLoadingMore: false });
    }
  },

  loadLikedList: async () => {
    try {
      const provider = get().playbackSource;
      if (provider === "kugou") {
        // 酷狗: 从"我喜欢"歌单获取已喜欢的歌曲 hash 列表
        const hashes = await invoke<string[]>("kugou_liked_hashes").catch(() => []);
        console.log("[Music] kugou liked songs loaded:", hashes.length);
        set({ likedSongIds: new Set(hashes) });
      } else if (provider === "qqmusic") {
        // QQ 音乐: 从"我喜欢"歌单获取已喜欢的歌曲 mid 列表
        const mids = await invoke<string[]>("qq_liked_hashes").catch(() => []);
        console.log("[Music] qq liked songs loaded:", mids.length);
        set({ likedSongIds: new Set(mids) });
      } else if (provider === "migu") {
        // 咪咕暂不支持红心读取
        set({ likedSongIds: new Set() });
      } else {
        const ids = await invoke<string[]>("music_likelist");
        console.log("[Music] liked songs loaded:", ids.length);
        set({ likedSongIds: new Set(ids) });
      }
    } catch (e) {
      console.error("[Music] loadLikedList failed:", e);
    }
  },

  toggleLike: async (songId) => {
    const provider = get().playbackSource;
    // QQ 音乐暂不支持写回红心
    if (provider === "qqmusic") {
      set({ musicToast: { type: "warning", message: "QQ 音乐当前仅支持读取账号收藏，暂不支持写回" } });
      return;
    }
    // 咪咕暂不支持红心
    if (provider === "migu") {
      set({ musicToast: { type: "warning", message: "咪咕音乐暂不支持红心收藏" } });
      return;
    }
    const liked = get().likedSongIds.has(songId);
    console.log("[Music] toggleLike: provider=", provider, "songId=", songId, "liked=", liked);
    // 乐观更新：先改 UI，API 在后台执行
    const newSet = new Set(get().likedSongIds);
    if (liked) {
      newSet.delete(songId);
    } else {
      newSet.add(songId);
    }
    set({ likedSongIds: newSet });
    try {
      if (provider === "kugou") {
        // 酷狗: 需要完整的歌曲对象来执行喜欢/取消喜欢
        // 从多个来源查找完整歌曲对象 (currentSong → playQueue → searchResults → 歌单列表 → 推荐)
        let song = get().currentSong;
        if (!song || song.id !== songId) {
          song = get().playQueue.find((s) => s.id === songId) || null;
        }
        if (!song) {
          song = get().searchResults.find((s) => s.id === songId) || null;
        }
        if (!song) {
          song = get().leftPlaylistTracks.find((s) => s.id === songId) || null;
        }
        if (!song) {
          song = get().rightPlaylistTracks.find((s) => s.id === songId) || null;
        }
        if (!song) {
          song = get().recommendSongs.find((s) => s.id === songId) || null;
        }
        if (!song) {
          song = get().artistSongs.find((s) => s.id === songId) || null;
        }
        if (!song) {
          console.warn("[Music] toggleLike: song not found in any list, using minimal object");
          // 构造最小歌曲对象，QQ音乐需要 mid，酷狗需要 hash
          song = { provider, id: songId, hash: songId, mid: songId, name: "", artist: "", artists: [], album: "", cover: "", duration: 0, fee: 0, playable: true, language: 0 };
        }
        const likeCmd = provider === "kugou" ? "kugou_like_toggle" : "qq_like_toggle";
        await invoke(likeCmd, { song, like: !liked });
        // 刷新喜欢列表, 确保与服务器同步
        await get().loadLikedList();
        // 后台异步刷新歌单列表，更新"我喜欢"的歌单曲目数量
        get().loadUserPlaylists();
      } else {
        await invoke("music_like", { id: songId, like: !liked });
        // 刷新喜欢列表和歌单列表, 确保与服务器同步
        await get().loadLikedList();
        get().loadUserPlaylists();
      }
    } catch (e) {
      // 回滚
      console.error("Toggle like failed:", e, "provider:", provider, "songId:", songId, "like:", !liked);
      const rollback = new Set(get().likedSongIds);
      if (liked) {
        rollback.add(songId);
      } else {
        rollback.delete(songId);
      }
      set({ likedSongIds: rollback });
    }
  },

  loadLyrics: async (songId) => {
    set({ loadingLyrics: true });
    try {
      const lyrics = await invoke<Lyrics>("music_lyric", { id: songId });
      set({ currentLyrics: lyrics });
      if (get().desktopLyricsVisible) {
        get().emitDesktopLyricsData();
      }
    } catch {
      set({ currentLyrics: null });
      if (get().desktopLyricsVisible) {
        get().emitDesktopLyricsData();
      }
    } finally {
      set({ loadingLyrics: false });
    }
  },

  loadLyricsForSong: async (song) => {
    // 本地导入歌曲：读取同目录的同名 .lrc 歌词文件
    if (song.provider === "local") {
      set({ loadingLyrics: true });
      try {
        const localPath = song._localPath || song.hash || "";
        if (localPath) {
          const lrcText = await invoke<string>("get_local_lyric", { path: localPath });
          if (lrcText && lrcText.trim()) {
            set({ currentLyrics: { lyric: lrcText } });
            if (get().desktopLyricsVisible) {
              get().emitDesktopLyricsData();
            }
            return;
          }
        }
        set({ currentLyrics: null });
        if (get().desktopLyricsVisible) {
          get().emitDesktopLyricsData();
        }
      } catch {
        set({ currentLyrics: null });
        if (get().desktopLyricsVisible) {
          get().emitDesktopLyricsData();
        }
      } finally {
        set({ loadingLyrics: false });
      }
      return;
    }
    set({ loadingLyrics: true });
    try {
      const lyrics = song.provider === "kugou"
        ? await invoke<Lyrics>("kugou_lyric", {
            hash: song.hash || song.id,
            albumAudioId: song.album_audio_id,
            duration: Math.floor(song.duration / 1000),
          })
        : song.provider === "qqmusic"
        ? await invoke<Lyrics>("qq_lyric", {
            mid: song.mid || song.id,
            id: song.id,
          })
        : song.provider === "migu"
        ? await invoke<Lyrics>("migu_lyric", {
            contentId: song.content_id || song.id,
            copyrightId: song.id,
          })
        : await invoke<Lyrics>("music_lyric", { id: song.id });
      set({ currentLyrics: lyrics });
      if (get().desktopLyricsVisible) {
        get().emitDesktopLyricsData();
      }
    } catch {
      set({ currentLyrics: null });
      if (get().desktopLyricsVisible) {
        get().emitDesktopLyricsData();
      }
    } finally {
      set({ loadingLyrics: false });
    }
  },

  loadComments: async (songId, page = 1) => {
    set({ loadingComments: true, commentError: "" });
    try {
      const result = await invoke<CommentPage>("music_song_comments", { id: songId, page, pageSize: 20 });
      // 分页时追加到已有列表（跳过已加载的 id），第一页直接替换
      set((state) => {
        if (page === 1 || !state.currentComments) {
          return { currentComments: result, loadingComments: false };
        }
        const seen = new Set(state.currentComments.comments.map((c) => c.comment_id));
        const merged = [...state.currentComments.comments, ...result.comments.filter((c) => !seen.has(c.comment_id))];
        return {
          currentComments: { ...result, comments: merged, hot_comments: state.currentComments.hot_comments },
          loadingComments: false,
        };
      });
    } catch (e) {
      console.error("[Music] loadComments failed:", e);
      set({ loadingComments: false, commentError: String(e) || "加载评论失败" });
    }
  },

  sendComment: async (songId, content) => {
    const trimmed = content.trim();
    if (!trimmed) return false;
    set({ sendingComment: true });
    try {
      await invoke("music_send_comment", { id: songId, content: trimmed });
      // 发送成功后刷新第一页，让新评论出现在列表
      await get().loadComments(songId, 1);
      return true;
    } catch (e) {
      console.error("[Music] sendComment failed:", e);
      return false;
    } finally {
      set({ sendingComment: false });
    }
  },

  clearComments: () => {
    set({ currentComments: null });
  },

  loadRecommendations: async () => {
    try {
      const provider = get().playbackSource;
      if (provider === "kugou" || provider === "qqmusic") {
        // 酷狗/QQ 不显示推荐歌单 (QQ 推荐歌单接口受限, 与酷狗一致只显示榜单)
        set({ recommendations: [], recommendSongs: [], dailyRecommendPlaylists: [] });
      } else if (provider === "migu") {
        // 咪咕: 歌单广场推荐 (填入 dailyRecommendPlaylists 由推荐区渲染)
        const playlists = await invoke<Playlist[]>("migu_recommend_playlists").catch(() => []);
        set({ recommendations: [], recommendSongs: [], dailyRecommendPlaylists: playlists });
      } else {
        const [songs, dailyPlaylists] = await Promise.all([
          invoke<Song[]>("music_recommend_songs").catch(() => []),
          invoke<Playlist[]>("music_recommend_resource").catch(() => []),
        ]);
        set({ recommendations: [], recommendSongs: songs, dailyRecommendPlaylists: dailyPlaylists });
      }
    } catch {
      // ignore
    }
  },

  loadOfficialCharts: async () => {
    // 酷狗平台: 加载酷狗官方排行榜
    if (get().playbackSource === "kugou") {
      try {
        const charts = await invoke<Playlist[]>("kugou_rank_list").catch(() => []);
        set({ officialCharts: charts });
        // 持久化缓存
        try {
          const s = await getStore();
          await s.set("officialCharts", charts);
          await s.save();
        } catch {}
      } catch {
        set({ officialCharts: [] });
      }
      return;
    }
    // QQ 音乐: 使用预设榜单 (榜单列表接口已失效, 后端返回实测可用的 topid)
    if (get().playbackSource === "qqmusic") {
      try {
        const charts = await invoke<Playlist[]>("qq_rank_list").catch(() => []);
        set({ officialCharts: charts });
        // 持久化缓存
        try {
          const s = await getStore();
          await s.set("officialCharts", charts);
          await s.save();
        } catch {}
      } catch {
        set({ officialCharts: [] });
      }
      return;
    }
    // 咪咕: 三个内置榜单 (热歌/新歌/原创)
    if (get().playbackSource === "migu") {
      try {
        const charts = await invoke<Playlist[]>("migu_rank_list").catch(() => []);
        set({ officialCharts: charts });
        try {
          const s = await getStore();
          await s.set("officialCharts", charts);
          await s.save();
        } catch {}
      } catch {
        set({ officialCharts: [] });
      }
      return;
    }
    const chartIds = [
      "3778678",      // 热歌榜
      "19723756",     // 飙升榜
      "3779629",      // 新歌榜
      "2884035",      // 原创榜
      "112504",       // 抖音排行榜
      "6723173524",   // 网络热歌榜
      "5453912201",   // VIP热歌榜
      "6886768100",   // 中文DJ榜
      "1978921795",   // 电音榜
      "2809513713",   // 说唱榜
      "71384707",     // 古典榜
    ];
    const chartNames = [
      "热歌榜", "飙升榜", "新歌榜", "原创榜",
      "抖音排行榜", "网络热歌榜", "VIP热歌榜", "中文DJ榜",
      "电音榜", "说唱榜", "古典榜",
    ];
    try {
      const results = await Promise.all(
        chartIds.map((id) =>
          invoke<Playlist>("music_playlist_detail", { id })
            .catch(() => ({
              provider: "netease" as const,
              id,
              name: chartNames[chartIds.indexOf(id)] || "",
              cover: "",
              track_count: 0,
              creator: "网易云音乐",
              subscribed: false,
            }))
        )
      );
      set({ officialCharts: results });
      // 持久化缓存
      try {
        const s = await getStore();
        await s.set("officialCharts", results);
        await s.save();
      } catch {}
    } catch {}
  },

  togglePlaylistSubscribe: async (playlistId, currentSubscribed) => {
    const newSubscribed = !currentSubscribed;
    try {
      await invoke("music_playlist_subscribe", { id: playlistId, subscribe: newSubscribed });
      // 刷新我的歌单列表
      await get().loadUserPlaylists();
      // 获取已收藏的歌单 ID 集合，同步到所有列表
      const subscribedIds = new Set(get().userPlaylists.filter((pl) => pl.subscribed).map((pl) => pl.id));
      set((state) => ({
        playlistSearchResults: state.playlistSearchResults.map((pl) =>
          ({ ...pl, subscribed: subscribedIds.has(pl.id) })
        ),
        recommendations: state.recommendations.map((pl) =>
          ({ ...pl, subscribed: subscribedIds.has(pl.id) })
        ),
        dailyRecommendPlaylists: state.dailyRecommendPlaylists.map((pl) =>
          ({ ...pl, subscribed: subscribedIds.has(pl.id) })
        ),
        officialCharts: state.officialCharts.map((pl) =>
          ({ ...pl, subscribed: subscribedIds.has(pl.id) })
        ),
        leftPlaylistMeta: state.leftPlaylistMeta?.id === playlistId
          ? { ...state.leftPlaylistMeta, subscribed: newSubscribed } : state.leftPlaylistMeta,
        rightPlaylistMeta: state.rightPlaylistMeta?.id === playlistId
          ? { ...state.rightPlaylistMeta, subscribed: newSubscribed } : state.rightPlaylistMeta,
      }));
    } catch (e) {
      console.error("Playlist subscribe failed:", e);
    }
  },
}));
