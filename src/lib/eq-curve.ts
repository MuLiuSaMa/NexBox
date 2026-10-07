// EQ 曲线与滤波器组之间的转换工具。
//
// 与 src-tauri/src/audio_engine.rs 的 EqChain 保持同一滤波器模型：
// 第一段 low shelf、最后一段 high shelf、中间 peaking；
// Q 按相邻频段的倍频程间距自适应（10 波段 ≈ 1 倍频程 → Q≈1.41，
// 31 波段 ≈ 1/3 倍频程 → Q≈4.32，与引擎 band_q_values 同一规则）。
//
// 滑块/预设/.fac 导出的频段值是"目标曲线"的控制点（对数频率轴上线性插值）；
// 引擎实际接收的增益是对该曲线做最小二乘拟合后的结果。这样无论 5/10/15/20/31
// 波段，同一条曲线的实测频响都一致——切换波段不再改变音色。

const NOMINAL_FS = 48000; // 模型用标称采样率（引擎按设备实际采样率运算，仅影响高频段模型精度）
const GRID_SIZE = 128; // 拟合频率网格点数
const GAIN_LIMIT = 12; // 引擎增益限幅（与滑块量程一致）

interface BiquadCoeffs {
  b0: number; b1: number; b2: number;
  a1: number; a2: number;
}

type BandType = "low_shelf" | "peaking" | "high_shelf";

// ── RBJ biquad 系数（与 audio_engine.rs 逐式一致） ──

function peakingCoeffs(fs: number, fc: number, gainDb: number, q: number): BiquadCoeffs {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * fc) / fs;
  const cosW0 = Math.cos(w0);
  const sinW0 = Math.sin(w0);
  const alpha = sinW0 / (2 * q);
  const b0 = 1 + alpha * a, b1 = -2 * cosW0, b2 = 1 - alpha * a;
  const a0 = 1 + alpha / a, a1 = -2 * cosW0, a2 = 1 - alpha / a;
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

function lowShelfCoeffs(fs: number, fc: number, gainDb: number, q: number): BiquadCoeffs {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * fc) / fs;
  const cosW0 = Math.cos(w0);
  const sinW0 = Math.sin(w0);
  const alpha = sinW0 / (2 * q);
  const sqA = Math.sqrt(a);
  const b0 = a * (a + 1 - (a - 1) * cosW0 + 2 * alpha * sqA);
  const b1 = 2 * a * (a - 1 - (a + 1) * cosW0);
  const b2 = a * (a + 1 - (a - 1) * cosW0 - 2 * alpha * sqA);
  const a0 = a + 1 + (a - 1) * cosW0 + 2 * alpha * sqA;
  const a1 = -2 * (a - 1 + (a + 1) * cosW0);
  const a2 = a + 1 + (a - 1) * cosW0 - 2 * alpha * sqA;
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

function highShelfCoeffs(fs: number, fc: number, gainDb: number, q: number): BiquadCoeffs {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * fc) / fs;
  const cosW0 = Math.cos(w0);
  const sinW0 = Math.sin(w0);
  const alpha = sinW0 / (2 * q);
  const sqA = Math.sqrt(a);
  const b0 = a * (a + 1 + (a - 1) * cosW0 + 2 * alpha * sqA);
  const b1 = -2 * a * (a - 1 + (a + 1) * cosW0);
  const b2 = a * (a + 1 + (a - 1) * cosW0 - 2 * alpha * sqA);
  const a0 = a + 1 - (a - 1) * cosW0 + 2 * alpha * sqA;
  const a1 = 2 * (a - 1 - (a + 1) * cosW0);
  const a2 = a + 1 - (a - 1) * cosW0 - 2 * alpha * sqA;
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}

/** biquad 在频率 f 处的幅度响应（dB） */
function biquadGainDb(c: BiquadCoeffs, f: number): number {
  const w = (2 * Math.PI * f) / NOMINAL_FS;
  const cw = Math.cos(w);
  const c2w = Math.cos(2 * w);
  const num = c.b0 * c.b0 + c.b1 * c.b1 + c.b2 * c.b2
    + 2 * (c.b0 * c.b1 + c.b1 * c.b2) * cw + 2 * c.b0 * c.b2 * c2w;
  const den = 1 + c.a1 * c.a1 + c.a2 * c.a2
    + 2 * (c.a1 + c.a1 * c.a2) * cw + 2 * c.a2 * c2w;
  return 10 * Math.log10(Math.max(num, 1e-12) / Math.max(den, 1e-12));
}

// ── 滤波器组模型（与引擎 band_q_values 同一规则） ──

/** 每段 Q 按相邻频段的倍频程间距自适应 */
export function bandQValues(freqs: number[]): number[] {
  const n = freqs.length;
  if (n === 0) return [];
  if (n === 1) return [1.41];
  const safe = freqs.map((f) => Math.max(f, 1.0));
  const qs: number[] = [];
  for (let i = 0; i < n; i++) {
    let oct: number;
    if (i === 0) oct = Math.log2(safe[1] / safe[0]);
    else if (i === n - 1) oct = Math.log2(safe[n - 1] / safe[n - 2]);
    else oct = (Math.log2(safe[i] / safe[i - 1]) + Math.log2(safe[i + 1] / safe[i])) / 2;
    oct = Math.min(3, Math.max(0.1, oct));
    const q = Math.sqrt(Math.pow(2, oct)) / (Math.pow(2, oct) - 1);
    qs.push(Math.min(8, Math.max(0.4, q)));
  }
  return qs;
}

function bandTypeAt(n: number, i: number): BandType {
  // 与引擎一致：首段 low shelf、末段 high shelf、中间 peaking（单段视为 low shelf）
  if (i === 0) return "low_shelf";
  if (i === n - 1) return "high_shelf";
  return "peaking";
}

function bandCoeffs(type: BandType, fc: number, gainDb: number, q: number): BiquadCoeffs {
  if (type === "low_shelf") return lowShelfCoeffs(NOMINAL_FS, fc, gainDb, q);
  if (type === "high_shelf") return highShelfCoeffs(NOMINAL_FS, fc, gainDb, q);
  return peakingCoeffs(NOMINAL_FS, fc, gainDb, q);
}

// ── 曲线求值与插值 ──

/** 折线曲线在 f 处的增益（对数频率轴线性插值，端点水平延伸） */
function curveGainAt(freqs: number[], gains: number[], f: number): number {
  const n = freqs.length;
  if (n === 0) return 0;
  if (f <= freqs[0]) return gains[0];
  if (f >= freqs[n - 1]) return gains[n - 1];
  let lo = 0, hi = n - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (freqs[mid] <= f) lo = mid; else hi = mid;
  }
  if (freqs[hi] <= freqs[lo]) return gains[hi];
  const t = Math.log(f / freqs[lo]) / Math.log(freqs[hi] / freqs[lo]);
  return gains[lo] + t * (gains[hi] - gains[lo]);
}

/**
 * 将任意频段曲线映射到目标布局频率（对数轴线性插值，端点水平延伸）。
 * 频率与目标布局完全一致时原样保留（含用户微调的中心频率/增益）。
 */
export function interpolateCurveToLayout(
  source: { freq: number; gain: number }[],
  targetFreqs: number[],
): { freq: number; gain: number }[] {
  const sorted = [...source].sort((a, b) => a.freq - b.freq);
  if (
    sorted.length === targetFreqs.length &&
    sorted.every((b, i) => Math.abs(b.freq - targetFreqs[i]) < 0.01)
  ) {
    return sorted.map((b) => ({ ...b }));
  }
  if (sorted.length === 0) return targetFreqs.map((f) => ({ freq: f, gain: 0 }));
  const freqs = sorted.map((b) => b.freq);
  const gains = sorted.map((b) => b.gain);
  return targetFreqs.map((f) => ({ freq: f, gain: curveGainAt(freqs, gains, f) }));
}

// ── 最小二乘拟合 ──

/** 滤波器组在网格上的级联响应（dB），用于拟合与测试 */
export function bankResponseDb(
  freqs: number[],
  gains: number[],
  grid: number[],
): number[] {
  const qs = bandQValues(freqs);
  const n = freqs.length;
  return grid.map((f) => {
    let sum = 0;
    for (let i = 0; i < n; i++) {
      sum += biquadGainDb(bandCoeffs(bandTypeAt(n, i), freqs[i], gains[i], qs[i]), f);
    }
    return sum;
  });
}

/** 高斯消元求解线性方程组（部分主元） */
function solveLinearSystem(A: number[][], b: number[]): number[] {
  const n = b.length;
  const M = A.map((row, i) => [...row, b[i]]);
  for (let col = 0; col < n; col++) {
    let piv = col;
    for (let r = col + 1; r < n; r++) {
      if (Math.abs(M[r][col]) > Math.abs(M[piv][col])) piv = r;
    }
    if (piv !== col) {
      const tmp = M[col]; M[col] = M[piv]; M[piv] = tmp;
    }
    const d = M[col][col];
    if (Math.abs(d) < 1e-12) { M[col][col] = d + 1e-6; continue; }
    for (let r = col + 1; r < n; r++) {
      const factor = M[r][col] / d;
      if (factor === 0) continue;
      for (let c = col; c <= n; c++) M[r][c] -= factor * M[col][c];
    }
  }
  const x = new Array<number>(n).fill(0);
  for (let r = n - 1; r >= 0; r--) {
    let s = M[r][n];
    for (let c = r + 1; c < n; c++) s -= M[r][c] * x[c];
    x[r] = s / M[r][r];
  }
  return x;
}

/**
 * 求引擎增益 h，使滤波器组实测频响逼近滑块定义的折线曲线。
 * 先用线性响应模型解正规方程，再用精确级联响应做 3 轮弦法迭代校正，
 * 结果限幅在 ±12 dB（滑块量程）。
 */
export function fitEngineGains(freqs: number[], curveGains: number[]): number[] {
  const n = freqs.length;
  if (n === 0) return [];
  if (n === 1) return [Math.max(-GAIN_LIMIT, Math.min(GAIN_LIMIT, curveGains[0] ?? 0))];

  // 输入可能乱序（预设文件）：内部按频率升序处理，结果按原顺序返回
  const order = freqs.map((_, i) => i).sort((a, b) => freqs[a] - freqs[b]);
  const sf = order.map((i) => Math.max(freqs[i], 1.0));
  const sg0 = order.map((i) => curveGains[i] ?? 0);

  const qs = bandQValues(sf);

  // 频率网格：覆盖所有滤波器的作用范围
  const lo = Math.max(10, sf[0] / 4);
  const hi = Math.max(lo * 2, Math.min(NOMINAL_FS * 0.45, sf[n - 1] * 2));
  const grid: number[] = [];
  for (let k = 0; k < GRID_SIZE; k++) {
    grid.push(lo * Math.pow(hi / lo, k / (GRID_SIZE - 1)));
  }

  // 线性响应模型 S[k][i]：滤波器 i 在 +1dB 时于网格点的 dB 响应
  const S: number[][] = grid.map((f) =>
    sf.map((fc, i) => biquadGainDb(bandCoeffs(bandTypeAt(n, i), fc, 1, qs[i]), f)),
  );
  // 目标 t[k]：曲线在网格点的 dB
  const t = grid.map((f) => curveGainAt(sf, sg0, f));

  // 正规方程 (SᵀS + λI) h = Sᵀ t
  const AtA: number[][] = Array.from({ length: n }, () => new Array<number>(n).fill(0));
  const Atb = new Array<number>(n).fill(0);
  for (let k = 0; k < GRID_SIZE; k++) {
    for (let i = 0; i < n; i++) {
      Atb[i] += S[k][i] * t[k];
      for (let j = i; j < n; j++) AtA[i][j] += S[k][i] * S[k][j];
    }
  }
  let diagMean = 0;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < i; j++) AtA[i][j] = AtA[j][i];
    diagMean += AtA[i][i];
  }
  diagMean /= n;
  const ridge = 1e-4 * diagMean + 1e-8;
  for (let i = 0; i < n; i++) AtA[i][i] += ridge;

  let h = solveLinearSystem(AtA, Atb);

  // 弦法迭代：用精确级联响应的残差修正线性模型的非线性误差
  for (let iter = 0; iter < 8; iter++) {
    const exact = sf.map((fc, i) => bandCoeffs(bandTypeAt(n, i), fc, h[i], qs[i]));
    const residual = grid.map((f, k) => {
      let sum = 0;
      for (let i = 0; i < n; i++) sum += biquadGainDb(exact[i], f);
      return t[k] - sum;
    });
    const rhs = new Array<number>(n).fill(0);
    for (let k = 0; k < GRID_SIZE; k++) {
      for (let i = 0; i < n; i++) rhs[i] += S[k][i] * residual[k];
    }
    const delta = solveLinearSystem(AtA, rhs);
    if (delta.some((v) => !Number.isFinite(v))) break;
    h = h.map((v, i) => Math.max(-GAIN_LIMIT, Math.min(GAIN_LIMIT, v + 0.65 * delta[i])));
  }

  const result = new Array<number>(n);
  order.forEach((origIdx, i) => {
    result[origIdx] = Math.max(-GAIN_LIMIT, Math.min(GAIN_LIMIT, h[i]));
  });
  return result;
}
