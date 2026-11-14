/**
 * 卡拉OK歌词解析与进度算法
 * 参考 Mineradio 开源项目的 YRC 逐字歌词实现
 */

import type { Lyrics, LyricWord, KaraokeLine } from "@/types/music";

// ═══════════════════════════════════════════════
// YRC 逐字歌词解析
// ═══════════════════════════════════════════════

/**
 * 解析 YRC 逐字歌词格式
 * 格式: [lineStartMs,lineDurMs](wordStartMs,wordDurMs,0)word(wordStartMs,wordDurMs,0)word...
 */
export function parseYrc(yrcText: string): KaraokeLine[] {
  if (!yrcText || !yrcText.trim()) return [];

  const lines: KaraokeLine[] = [];

  for (const rawLine of yrcText.split(/\r?\n/)) {
    // 匹配行头: [startMs,durMs]
    const lineMatch = rawLine.match(/^\[(\d+),(\d+)\](.*)$/);
    if (!lineMatch) continue;

    const lineStartMs = parseInt(lineMatch[1], 10) || 0;
    const lineDurMs = parseInt(lineMatch[2], 10) || 0;
    const body = lineMatch[3] || "";

    // 匹配每个词: (startMs,durMs,0)text
    const wordRegex = /\((\d+),(\d+),\d+\)([^()]*)/g;
    const words: LyricWord[] = [];
    let fullText = "";
    let wm: RegExpExecArray | null;

    while ((wm = wordRegex.exec(body)) !== null) {
      const txt = (wm[3] || "").replace(/\s+/g, " ");
      if (!txt) continue;

      const rawStart = parseInt(wm[1], 10) || 0;
      const rawDur = parseInt(wm[2], 10) || 0;
      // 判断是绝对时间戳还是相对行头的偏移
      const absStartMs = rawStart >= lineStartMs - 500 ? rawStart : lineStartMs + rawStart;

      const c0 = fullText.length;
      fullText += txt;

      words.push({
        text: txt,
        t: absStartMs / 1000,
        d: Math.max(0.06, rawDur / 1000),
        c0,
        c1: fullText.length,
      });
    }

    // 如果没有匹配到词，尝试提取纯文本
    if (!fullText) {
      fullText = body.replace(/\(\d+,\d+,\d+\)/g, "").replace(/\s+/g, " ");
    }

    // 清理文本
    const leading = (fullText.match(/^\s+/) || [""])[0].length;
    fullText = fullText.replace(/\s+/g, " ").trim();
    if (!fullText) continue;

    // 修正词的字符偏移
    if (words.length) {
      words.forEach((w) => {
        w.c0 = Math.max(0, Math.min(fullText.length, w.c0 - leading));
        w.c1 = Math.max(w.c0, Math.min(fullText.length, w.c1 - leading));
      });
      // 过滤掉无效的词
      const validWords = words.filter((w) => w.c1 > w.c0);
      if (validWords.length === 0) continue;
      lines.push({
        time: lineStartMs / 1000,
        duration: lineDurMs / 1000,
        text: fullText,
        words: validWords,
        charCount: Math.max(1, fullText.length),
        hasKaraoke: true,
      });
    } else {
      lines.push({
        time: lineStartMs / 1000,
        duration: lineDurMs / 1000,
        text: fullText,
        charCount: Math.max(1, fullText.length),
        hasKaraoke: false,
      });
    }
  }

  return finalizeLineDurations(lines);
}

// ═══════════════════════════════════════════════
// LRC 逐行歌词解析（增强版，兼容旧格式）
// ═══════════════════════════════════════════════

/**
 * 解析 LRC 格式歌词，附带翻译
 */
export function parseLrcEnhanced(lyric: string, translation?: string): KaraokeLine[] {
  if (!lyric) return [];

  const lines: KaraokeLine[] = [];
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
      lines.push({
        time,
        duration: 0,
        text,
        translation: transMap.get(time),
        charCount: Math.max(1, text.length),
        hasKaraoke: false,
      });
    }
  }

  return finalizeLineDurations(lines);
}

// ═══════════════════════════════════════════════
// 辅助函数
// ═══════════════════════════════════════════════

/**
 * 排序、填充持续时间、确保 charCount 有效
 */
function finalizeLineDurations(lines: KaraokeLine[]): KaraokeLine[] {
  lines.sort((a, b) => a.time - b.time);

  for (let i = 0; i < lines.length; i++) {
    const next = lines[i + 1];
    const inferred = next && next.time > lines[i].time ? next.time - lines[i].time : 4.8;
    if (!isFinite(lines[i].duration) || lines[i].duration <= 0) {
      lines[i].duration = inferred;
    }
    lines[i].duration = Math.max(0.45, Math.min(12, lines[i].duration));
    lines[i].charCount = Math.max(1, lines[i].charCount || String(lines[i].text || "").length);
  }

  return lines;
}

// ═══════════════════════════════════════════════
// 行进度计算（核心算法）
// ═══════════════════════════════════════════════

/**
 * 计算当前歌词行的填充进度 (0~1)
 * 参考 Mineradio 的 getLyricLineProgress
 *
 * - 有逐字数据时：遍历每个词，精确计算已唱到的字符位置
 * - 无逐字数据时：行级线性插值 + smoothstep 缓动
 */
export function getLineProgress(
  line: KaraokeLine | undefined,
  nextLine: KaraokeLine | undefined,
  currentTime: number
): number {
  if (!line) return 0;

  // 微小偏移，让进度略超前于实际音频（视觉补偿）
  const now = currentTime + (line.words && line.words.length ? 0.03 : 0.02);

  // 有逐字数据：精确计算已唱字符位置
  if (line.words && line.words.length && line.charCount > 0) {
    let lastP = 0;
    for (const w of line.words) {
      const ws = w.t;
      const we = w.t + Math.max(0.08, w.d || 0.24);
      if (now < ws) return lastP;
      const local = now >= we ? 1 : (now - ws) / Math.max(0.08, we - ws);
      const clamped = Math.max(0, Math.min(1, local));
      const p = (w.c0 + (w.c1 - w.c0) * clamped) / line.charCount;
      lastP = Math.max(lastP, p);
      if (now < we) return lastP;
    }
    return 1;
  }

  // 无逐字数据：行级线性插值 + smoothstep 缓动
  const nextT =
    nextLine && nextLine.time > line.time
      ? nextLine.time
      : line.time + (line.duration || 4.8);
  const span = Math.max(0.75, nextT - line.time);
  const prog = Math.max(0, Math.min(1, (now - line.time) / span));
  return prog * prog * (3 - 2 * prog); // smoothstep
}

// ═══════════════════════════════════════════════
// 超长歌词水平滚动算法
// ═══════════════════════════════════════════════

/**
 * 超长歌词水平滚动偏移量计算
 *
 * @param progress 行播放进度 0~1
 * @param from     行首贴齐容器左边缘时的位移（滚动起点）
 * @param to       行尾贴齐容器右边缘时的位移（滚动终点，通常为负）
 *
 * from / to 由调用方用实际布局测量得到，不能按对齐方式硬编码：
 * nowrap 的 inline-block 超宽时，Chromium 会保持左端对齐、只向右溢出
 * （即使容器是 text-align:center），此时 from = 0、to = -overflow。
 * 旧实现按「居中各溢出 half」把终点算成 -overflow/2，滚动只走了一半，
 * 于是行尾永远露不全，同时因为起点不是 0 还会把行首切掉。
 *
 * 策略：前 8% 停留在行首 → 中段 smoothstep 缓动 → 78% 起到结束停在行尾。
 */
export function calculateScrollOffset(
  progress: number,
  from: number,
  to: number
): number {
  if (!(to < from)) return from;

  const p = Math.max(0, Math.min(1, progress));
  const startGate = 0.08;
  const endGate = 0.78;

  if (p <= startGate) return from;
  if (p >= endGate) return to;

  const t = (p - startGate) / (endGate - startGate);
  const eased = t * t * (3 - 2 * t);
  return from + (to - from) * eased;
}

/**
 * 测量某一行歌词的可滚动区间。
 * 返回 from/to；不需要滚动时两者都为 0。
 */
export function measureScrollRange(
  scrollEl: HTMLElement,
  container: HTMLElement
): { from: number; to: number } {
  const cs = getComputedStyle(container);
  const padLeft = parseFloat(cs.paddingLeft) || 0;
  const padRight = parseFloat(cs.paddingRight) || 0;

  // offsetLeft 是相对 offsetParent（容器，position:relative）padding 左边缘的布局距离，
  // 不含 transform，父级 scale 也不会干扰
  const contentWidth = container.clientWidth - padLeft - padRight;
  const baseOffset = scrollEl.offsetLeft - padLeft;
  const textWidth = scrollEl.scrollWidth;

  // 起点让行首贴左边缘，终点让行尾贴右边缘
  const from = -baseOffset;
  const to = contentWidth - baseOffset - textWidth;

  // 文字比容器窄（或刚好多一点）就没必要滚动
  return to < from - 4 ? { from, to } : { from: 0, to: 0 };
}

// ═══════════════════════════════════════════════
// 统一入口：从 Lyrics 构建卡拉OK歌词行
// ═══════════════════════════════════════════════

/**
 * 从 Lyrics 数据构建卡拉OK歌词行
 * 优先使用 YRC 逐字歌词，降级为 LRC 逐行歌词
 */
export function buildKaraokeLines(lyrics: Lyrics | null | undefined): KaraokeLine[] {
  if (!lyrics) return [];

  // 优先使用 YRC 逐字歌词
  if (lyrics.yrc) {
    const yrcLines = parseYrc(lyrics.yrc);
    if (yrcLines.length > 0) {
      // 尝试合并翻译
      if (lyrics.translation) {
        const transMap = new Map<number, string>();
        for (const line of lyrics.translation.split("\n")) {
          const match = line.match(/\[(\d+):(\d+(?:\.\d+)?)\]/);
          if (match) {
            const time = parseInt(match[1]) * 60 + parseFloat(match[2]);
            const text = line.replace(/\[\d+:\d+(?:\.\d+)?\]/, "").trim();
            if (text) transMap.set(time, text);
          }
        }
        // 为 YRC 行匹配翻译（按时间近似匹配）
        yrcLines.forEach((line) => {
          // 精确匹配
          if (transMap.has(line.time)) {
            line.translation = transMap.get(line.time);
            return;
          }
          // 模糊匹配：找最接近的时间
          let bestMatch: string | undefined;
          let bestDiff = Infinity;
          for (const [t, txt] of transMap) {
            const diff = Math.abs(t - line.time);
            if (diff < bestDiff && diff < 0.5) {
              bestDiff = diff;
              bestMatch = txt;
            }
          }
          if (bestMatch) line.translation = bestMatch;
        });
      }
      return yrcLines;
    }
  }

  // 降级为 LRC 逐行歌词
  if (lyrics.lyric) {
    return parseLrcEnhanced(lyrics.lyric, lyrics.translation);
  }

  return [];
}
