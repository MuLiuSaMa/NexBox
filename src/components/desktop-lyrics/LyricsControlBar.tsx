/**
 * 桌面歌词控制栏（仅未锁定时悬浮显示）
 *
 * 锁定状态的解锁按钮由独立小窗口（/lyrics-unlock-btn）提供，
 * 不受 WebView2 穿透影响。
 *
 * [上一句] [播放/暂停] [下一句] [播放顺序] | [锁定] | [关闭]
 */

import { memo, useState, useCallback } from "react";
import {
  SkipBack,
  SkipForward,
  Shuffle,
  Repeat,
  Repeat1,
  HeartPulse,
  Play,
  Pause,
  Lock,
  X,
  Volume2,
  VolumeX,
  Volume1,
} from "lucide-react";
import { Tooltip } from "@chakra-ui/react";
import type { PlayMode } from "@/types/music";
import type { ControlAction } from "@/hooks/useDesktopLyricsSync";
import { useThemeColor } from "@/contexts/theme-color-context";

interface LyricsControlBarProps {
  isPlaying: boolean;
  playMode: PlayMode;
  volume: number;
  onControl: (action: ControlAction, value?: number) => void;
}

const btnBase: React.CSSProperties = {
  background: "transparent",
  border: "none",
  cursor: "pointer",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  padding: "6px",
  borderRadius: "50%",
  color: "rgba(255,255,255,0.9)",
  transition: "background 0.15s ease, transform 0.1s ease",
};

function LyricsControlBarInner({
  isPlaying,
  playMode,
  volume,
  onControl,
}: LyricsControlBarProps) {
  const { config } = useThemeColor();
  const primary = config.primaryColor;
  const [volOpen, setVolOpen] = useState(false);
  // 拖动过程中的本地预览值：避免受控 value 走异步往返被拉回旧值导致“乱跳”
  const [volPreview, setVolPreview] = useState<number | null>(null);
  const displayVol = volPreview ?? volume;

  // 音量图标：静音 / 低 / 中 / 高
  const VolIcon = volume <= 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;
  const volColor = volume > 0 ? primary : "rgba(255,255,255,0.6)";

  // 拖动时即时更新本地预览（UI 不抖），同时把目标值发给主窗口
  const handleVolumeChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const v = Number(e.target.value);
      setVolPreview(v);
      onControl("volume", v);
    },
    [onControl]
  );

  // 松开滑块 / 收起调节条时回到权威音量值（门店已按最后值回传一致）
  const handleVolPointerUp = useCallback(() => {
    setVolPreview(null);
  }, []);

  // 滚轮调节音量：上滚 +5%，下滚 -5%，四舍五入到 0.01，夹在 0~1 之间
  const handleWheelVolume = useCallback(
    (e: React.WheelEvent) => {
      e.preventDefault();
      if (e.deltaY === 0) return;
      const step = 0.05;
      const next = Math.round((volume + (e.deltaY < 0 ? step : -step)) * 100) / 100;
      onControl("volume", Math.min(1, Math.max(0, next)));
    },
    [volume, onControl]
  );

  return (
    <div
      style={{
        position: "absolute",
        top: "8px",
        left: "50%",
        transform: "translateX(-50%)",
        display: "flex",
        alignItems: "center",
        gap: "4px",
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(16px)",
        WebkitBackdropFilter: "blur(16px)",
        borderRadius: "999px",
        padding: "4px 12px",
        transition: "opacity 0.2s ease",
        boxShadow: "0 4px 16px rgba(0,0,0,0.3)",
      }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <Tooltip label="上一首">
        <button
          style={btnBase}
          onClick={() => onControl("prev")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.15)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          <SkipBack size={16} />
        </button>
      </Tooltip>

      <Tooltip label={isPlaying ? "暂停" : "播放"}>
        <button
          style={{
            ...btnBase,
            background: "rgba(255,255,255,0.2)",
          }}
          onClick={() => onControl("play-pause")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.3)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.2)")}
        >
          {isPlaying ? <Pause size={18} /> : <Play size={18} />}
        </button>
      </Tooltip>

      <Tooltip label="下一首">
        <button
          style={btnBase}
          onClick={() => onControl("next")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.15)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          <SkipForward size={16} />
        </button>
      </Tooltip>

      <Tooltip
        label={playMode === "one" ? "单曲循环" : playMode === "shuffle" ? "随机播放" : playMode === "heartbeat" ? "心动模式" : "列表循环"}
      >
        <button
          style={{
            ...btnBase,
            color: playMode !== "list" ? "#4FC3F7" : "rgba(255,255,255,0.9)",
          }}
          onClick={() => onControl("toggle-shuffle")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.15)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          {playMode === "one" ? (
            <Repeat1 size={16} />
          ) : playMode === "shuffle" ? (
            <Shuffle size={16} />
          ) : playMode === "heartbeat" ? (
            <HeartPulse size={16} />
          ) : (
            <Repeat size={16} />
          )}
        </button>
      </Tooltip>

      {/* 音量：图标 + 悬浮展开调节条（主题色） */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          borderRadius: "999px",
          overflow: "hidden",
        }}
        onWheel={handleWheelVolume}
        onMouseEnter={() => setVolOpen(true)}
        onMouseLeave={() => {
          setVolOpen(false);
          setVolPreview(null);
        }}
      >
        <Tooltip label="音量">
          <button
            style={{ ...btnBase, color: volColor }}
            onClick={() => onControl("volume", volume > 0 ? 0 : 0.7)}
          >
            <VolIcon size={16} />
          </button>
        </Tooltip>
        <div
          style={{
            width: volOpen ? 76 : 0,
            opacity: volOpen ? 1 : 0,
            transition: "width 0.25s ease, opacity 0.2s ease",
            display: "flex",
            alignItems: "center",
            paddingRight: volOpen ? 6 : 0,
          }}
        >
          <input
            type="range"
            className="lyrics-vol-slider"
            min={0}
            max={1}
            step={0.01}
            value={displayVol}
            onChange={handleVolumeChange}
            onPointerUp={handleVolPointerUp}
            aria-label="音量"
            style={
              {
                width: 76,
                margin: 0,
                cursor: "pointer",
                "--vol-color": primary,
                "--vol-fill": `${Math.round(displayVol * 100)}%`,
              } as React.CSSProperties
            }
          />
        </div>
      </div>

      {/* 分隔线 */}
      <div
        style={{
          width: "1px",
          height: "16px",
          background: "rgba(255,255,255,0.2)",
          margin: "0 2px",
        }}
      />

      <Tooltip label="锁定歌词">
        <button
          style={btnBase}
          onClick={() => onControl("lock")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.15)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          <Lock size={16} />
        </button>
      </Tooltip>

      {/* 分隔线 */}
      <div
        style={{
          width: "1px",
          height: "16px",
          background: "rgba(255,255,255,0.2)",
          margin: "0 2px",
        }}
      />

      <Tooltip label="关闭桌面歌词">
        <button
          style={btnBase}
          onClick={() => onControl("close")}
          onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(255,255,255,0.15)")}
          onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
        >
          <X size={16} />
        </button>
      </Tooltip>
    </div>
  );
}

export const LyricsControlBar = memo(LyricsControlBarInner);
