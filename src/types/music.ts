/// 统一歌曲结构
export interface Song {
  provider: string;
  id: string;
  mid?: string;
  media_mid?: string;
  name: string;
  artist: string;
  artists: Artist[];
  album: string;
  cover: string;
  duration: number;
  fee: number;
  playable: boolean;
  language: number;
  // === 酷狗扩展字段 ===
  hash?: string;
  album_id?: string;
  album_audio_id?: string;
  hq_hash?: string;
  sq_hash?: string;
  res_hash?: string;
  // === QQ 音乐扩展字段 ===
  qq_song_id?: number;
  // === 咪咕扩展字段 (id 存 copyrightId，播放用 contentId) ===
  content_id?: string;
  // === 本地导入扩展字段 ===
  _localPath?: string;
  _localCoverPath?: string;
}

export interface Artist {
  id?: string;
  mid?: string;
  name: string;
  pic_url?: string;
  music_size?: number;
}

export interface Playlist {
  provider: string;
  id: string;
  name: string;
  cover: string;
  track_count: number;
  creator: string;
  subscribed: boolean;
}

export interface SongUrlResult {
  url: string | null;
  playable: boolean;
  trial: boolean;
  level: string;
  quality: string;
  br: number;
  reason?: string;
  message?: string;
  fee?: number;
}

export interface LoginInfo {
  provider: string;
  logged_in: boolean;
  user_id: string;
  nickname: string;
  avatar: string;
  vip_type: number;
  vip_level: string;
  is_vip: boolean;
  is_svip: boolean;
}

export interface Lyrics {
  lyric: string;
  translation?: string;
  roma?: string;
  yrc?: string; // YRC 逐字歌词
}

/** 逐词数据 */
export interface LyricWord {
  text: string;
  t: number;     // 开始时间（秒）
  d: number;     // 持续时间（秒）
  c0: number;    // 在整行文本中的起始字符索引
  c1: number;    // 在整行文本中的结束字符索引
}

/** 卡拉OK歌词行 */
export interface KaraokeLine {
  time: number;          // 行开始时间（秒）
  duration: number;      // 行持续时间（秒）
  text: string;          // 整行文本
  translation?: string;  // 翻译
  words?: LyricWord[];   // 逐词数据（有 YRC 时存在）
  charCount: number;     // 字符数
  hasKaraoke: boolean;   // 是否有逐字数据
}

export interface QrCheckResult {
  code: number; // 801=等待扫码, 802=待确认, 803=成功, 800=过期
  message: string;
  cookie?: string;
  nickname?: string;
  avatar?: string;
}

/** 评论 */
export interface MusicComment {
  comment_id: number;
  content: string;
  time: number;       // 毫秒时间戳
  liked_count: number;
  liked: boolean;
  user_id: number;
  nickname: string;
  avatar: string;
}

/** 评论分页结果 */
export interface CommentPage {
  total: number;
  has_more: boolean;
  comments: MusicComment[];
  hot_comments: MusicComment[];
}

/** 专辑 */
export interface Album {
  id: string;
  name: string;
  cover: string;
  publish_time: number;
  song_count: number;
  artist_name: string;
}

/** 歌手 MV */
export interface Mv {
  id: string;
  name: string;
  cover: string;
  duration: number;
  play_count: number;
  artist_name: string;
}

/** 歌手简介 */
export interface ArtistDetail {
  id: string;
  name: string;
  brief_desc: string;
}

/// 播放模式: list=列表循环, heartbeat=心动模式(相似歌曲动态续播), shuffle=随机播放, one=单曲循环
export type PlayMode = "list" | "heartbeat" | "shuffle" | "one";

export type PlaybackQuality = "jymaster" | "hires" | "lossless" | "exhigh" | "standard";

/// 音乐平台类型
export type MusicProvider = "netease" | "kugou" | "qqmusic" | "migu";

/// 外部客户端播放状态（SMTC 接管，非登录平台）
export interface ExternalTrack {
  title: string;
  artist: string;
  album: string;
  cover: string | null;
  sourceAppId: string;
}

export interface ExternalPlayback {
  track: ExternalTrack | null;
  isPlaying: boolean;
  positionMs: number;
  durationMs: number;
}

/// 内部播放器推送给系统 SMTC 的状态（音量浮层/锁屏显示）
export interface SmtcState {
  title: string;
  artist: string;
  album: string;
  /** 封面来源：base64 data URI / http(s) URL / file:// 本地路径；空 = 不更新封面 */
  cover?: string | null;
  playing: boolean;
  positionMs: number;
  durationMs: number;
}

/// 平台显示信息
export interface ProviderInfo {
  id: MusicProvider;
  name: string;
  icon: string;
  color: string;
}
