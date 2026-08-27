/**
 * 音频频谱基础设施（「音域回响」spectrum 样式专用）
 *
 * 纯真实频谱，无任何模拟：
 * - createMediaElementSource 接管 <audio> 输出（不受 CORS 限制），链路
 *   source → analyser → destination 保证出声。
 * - 若 WebView 的 AudioContext 未激活（suspended）会整体静音（dynamic-island 教训），
 *   因此：1) 播放动作（用户手势）里调用 ensureAudioContextActive() 激活；
 *   2) tauri.conf.json 已加 `--autoplay-policy=no-user-gesture-required`（需重启生效）；
 *   3) getAnalyser() 仅在 ctx.state === "running" 时才接管（否则返回 null，场景静止）。
 * - 同一 <audio> 元素只能 createMediaElementSource 一次，用 WeakMap 缓存。
 */

export interface BandValues {
  subBass: number;
  bass: number;
  lowMid: number;
  mid: number;
  highMid: number;
  presence: number;
  brilliance: number;
  air: number;
  treble: number;
  kickEnvelope: number;
  energy: number;
  sharpness: number;
  smoothness: number;
  density: number;
}

export function zeroBands(): BandValues {
  return {
    subBass: 0, bass: 0, lowMid: 0, mid: 0, highMid: 0,
    presence: 0, brilliance: 0, air: 0,
    treble: 0, kickEnvelope: 0, energy: 0,
    sharpness: 0, smoothness: 0.8, density: 0.45,
  };
}

function clamp01(v: number): number {
  return Math.max(0, Math.min(1, Number.isFinite(v) ? v : 0));
}

// ── AudioContext 单例 ──
let sharedCtx: AudioContext | null = null;

/** 获取（并复用）全局 AudioContext；suspended 时尝试恢复（须在用户手势内才有效） */
export function getAudioContext(): AudioContext | null {
  try {
    if (!sharedCtx) {
      const Ctor =
        window.AudioContext ??
        (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!Ctor) return null;
      sharedCtx = new Ctor();
    }
    return sharedCtx;
  } catch {
    return null;
  }
}

/** 在用户手势中调用：激活 AudioContext（resume），返回最终是否 running */
export async function ensureAudioContextActive(): Promise<boolean> {
  const ctx = getAudioContext();
  if (!ctx) return false;
  if (ctx.state === "running") return true;
  try {
    await ctx.resume();
    return (ctx.state as string) === "running";
  } catch {
    return false;
  }
}

// ── 频谱分析器（createMediaElementSource 接管，真实频谱）──
// 用户环境实测：captureStream 拿不到信号（WebView2 CORS 限制），
// createMediaElementSource 是唯一可靠方案：source → analyser → destination 保证出声。
// ⚠️ 同一 <audio> 只能 createMediaElementSource 一次（永久接管输出），WeakMap 缓存永不释放。
// 媒体需为 CORS 模式（audio.crossOrigin="anonymous"）才有非零输出——全局已设置。
const analyserCache = new WeakMap<HTMLAudioElement, AnalyserNode>();

/**
 * 为 audio 元素创建（或复用）AnalyserNode（createMediaElementSource 接管）。
 * 前置：AudioContext running（播放手势 ensureAudioContextActive）+ 媒体 CORS 模式。
 */
export function getAnalyser(audio: HTMLAudioElement): AnalyserNode | null {
  const cached = analyserCache.get(audio);
  if (cached) return cached;
  try {
    const ctx = getAudioContext();
    if (!ctx || ctx.state !== "running") {
      console.warn("[spectrum] AudioContext not running, 跳过接管:", ctx?.state);
      return null;
    }
    const source = ctx.createMediaElementSource(audio);
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 1024; // 512 bins
    analyser.smoothingTimeConstant = 0.8;
    source.connect(analyser);
    analyser.connect(ctx.destination);
    analyserCache.set(audio, analyser);
    console.info("[spectrum] Analyser 接管成功（createMediaElementSource）");
    return analyser;
  } catch (e) {
    console.warn("[spectrum] 接管失败:", e);
    return null;
  }
}

/** MediaElementSource 永久接管：不能断开（断开即静音且无法重建），保留导出为 no-op */
export function releaseAnalyser(_audio: HTMLAudioElement | null | undefined) {
  // no-op：接管后的 audio 输出走 Web Audio 图，释放节点会导致静音/无法重建
}

/** AudioContext 必须保持 running（接管后音频走 Web Audio 图，suspend 即静音），no-op */
export function suspendAudioContext() {
  // no-op：不能挂起
}

// 8 频段 Hz 边界（参照 Mineradio SONIC_AUDIO_BAND_EDGES）
const BAND_EDGES: ReadonlyArray<readonly [string, number, number]> = [
  ["subBass", 32, 58],
  ["bass", 58, 118],
  ["lowMid", 118, 260],
  ["mid", 260, 720],
  ["highMid", 720, 1800],
  ["presence", 1800, 4200],
  ["brilliance", 4200, 9000],
  ["air", 9000, 16000],
];

/** 从 getByteFrequencyData 数据读取 8 频段（段内均值，归一化 0-1）并派生特征量。
 *  全程零对象分配（RAF 高频调用，避免垃圾产生） */
export function readBands(
  data: Uint8Array,
  sampleRate = 44100,
  fftSize = 1024,
  out?: BandValues
): BandValues {
  const binHz = sampleRate / fftSize;
  const len = data.length;
  let subBass = 0, bass = 0, lowMid = 0, mid = 0;
  let highMid = 0, presence = 0, brilliance = 0, air = 0;
  for (const [name, startHz, endHz] of BAND_EDGES) {
    const startBin = Math.max(1, Math.round(startHz / binHz));
    const endBin = Math.min(len - 1, Math.round(endHz / binHz));
    let sum = 0;
    let count = 0;
    for (let b = startBin; b <= endBin; b++) {
      sum += data[b] / 255;
      count++;
    }
    const v = count ? sum / count : 0;
    if (name === "subBass") subBass = v;
    else if (name === "bass") bass = v;
    else if (name === "lowMid") lowMid = v;
    else if (name === "mid") mid = v;
    else if (name === "highMid") highMid = v;
    else if (name === "presence") presence = v;
    else if (name === "brilliance") brilliance = v;
    else air = v;
  }
  subBass = clamp01(subBass);
  bass = clamp01(bass);
  lowMid = clamp01(lowMid);
  mid = clamp01(mid);
  highMid = clamp01(highMid);
  presence = clamp01(presence);
  brilliance = clamp01(brilliance);
  air = clamp01(air);
  const treble = clamp01((highMid + presence + brilliance + air) / 4);
  const energy = clamp01((subBass + bass + lowMid + mid + highMid + presence + brilliance + air) / 8);
  // kick 信号：低频加权增强（真实频谱低频段能量偏小，过弱会导致方块不律动/涟漪不触发）
  const kickEnvelope = clamp01(subBass * 1.2 + bass * 1.0 + lowMid * 0.35 + energy * 0.25);
  const r = out ?? ({} as BandValues);
  r.subBass = subBass;
  r.bass = bass;
  r.lowMid = lowMid;
  r.mid = mid;
  r.highMid = highMid;
  r.presence = presence;
  r.brilliance = brilliance;
  r.air = air;
  r.treble = treble;
  r.kickEnvelope = kickEnvelope;
  r.energy = energy;
  r.sharpness = clamp01(treble * 0.7 + kickEnvelope * 0.2);
  r.smoothness = clamp01(1 - treble * 0.42 + mid * 0.14);
  r.density = clamp01(0.45 + treble * 0.35 + kickEnvelope * 0.1);
  return r;
}

/** 频谱指数平滑器（触发回调由调用方做冷却限频） */
export class BandSmoother {
  values: BandValues = zeroBands();
  rate: number;

  constructor(rate = 10) {
    this.rate = rate;
  }

  /** 将目标频段向当前值平滑逼近；onTrigger 在 kick/snare 上升沿回调 */
  step(target: BandValues, dt: number, onTrigger?: (kind: "kick" | "snare", level: number) => void): BandValues {
    const blend = 1 - Math.exp(-Math.max(0.001, dt) * this.rate);
    const v = this.values;
    v.subBass += (target.subBass - v.subBass) * blend;
    v.bass += (target.bass - v.bass) * blend;
    v.lowMid += (target.lowMid - v.lowMid) * blend;
    v.mid += (target.mid - v.mid) * blend;
    v.highMid += (target.highMid - v.highMid) * blend;
    v.presence += (target.presence - v.presence) * blend;
    v.brilliance += (target.brilliance - v.brilliance) * blend;
    v.air += (target.air - v.air) * blend;
    v.treble += (target.treble - v.treble) * blend;
    v.kickEnvelope += (target.kickEnvelope - v.kickEnvelope) * blend;
    v.energy += (target.energy - v.energy) * blend;
    v.sharpness += (target.sharpness - v.sharpness) * blend;
    v.smoothness += (target.smoothness - v.smoothness) * blend;
    v.density += (target.density - v.density) * blend;

    if (onTrigger) {
      // 阈值触发（不依赖上升沿）——频率由调用方冷却控制；阈值适中避免波浪过密
      const kick = target.kickEnvelope;
      const snare = target.presence * 0.6 + target.brilliance * 0.7;
      if (kick > 0.3) onTrigger("kick", Math.min(kick, 1));
      if (snare > 0.25) onTrigger("snare", Math.min(snare, 1));
    }
    return v;
  }

  /** 暂停/无信号时向全 0 回落 */
  idle(dt: number): BandValues {
    return this.step(zeroBands(), dt);
  }
}
