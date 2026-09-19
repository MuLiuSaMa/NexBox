/**
 * 桌面歌词独立窗口页面
 *
 * 特性：
 * - 卡拉OK逐字高亮歌词渲染
 * - 单行/双行模式
 * - 悬浮控制栏（上一句/播放/下一句/随机/锁定）
 * - 未锁定：可拖动 + 悬浮显示背景轮廓 + 完整控制
 * - 锁定：鼠标穿透 + 内嵌解锁按钮（点击由 Rust 鼠标钩子拦截）
 * - 窗口位置记忆
 */

import { useEffect, useRef, useState, useCallback } from "react";
import { Tooltip } from "@chakra-ui/react";
import { Unlock } from "lucide-react";
import { useDesktopLyricsSync } from "@/hooks/useDesktopLyricsSync";
import type { ControlAction } from "@/hooks/useDesktopLyricsSync";
import { LyricsCanvas } from "@/components/desktop-lyrics/LyricsCanvas";
import { LyricsControlBar } from "@/components/desktop-lyrics/LyricsControlBar";
import {
  startDragging,
  setIgnoreCursorEvents,
  saveWindowPosition,
  restoreWindowPosition,
  enableUnlockHook,
  disableUnlockHook,
  setUnlockHookArmed,
  onUnlockBtnClicked,
  isCursorInWindow,
  isCursorInUnlockArea,
} from "@/lib/desktop-lyrics-window";

/**
 * 锁定状态下的内嵌解锁按钮
 *
 * 锁定时歌词窗口整窗鼠标穿透,按钮无法接收鼠标事件:
 * - 显隐由轮询驱动(btnVisible),悬停态由轮询判断光标是否落在按钮区域(btnHover)
 * - 实际点击由 Rust 端 WH_MOUSE_LL 钩子拦截并触发解锁事件
 */
function LockedUnlockBtn({ hover }: { hover: boolean }) {
  return (
    <div
      style={{
        position: "absolute",
        top: "8px",
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 10,
      }}
    >
      <Tooltip label="解锁歌词">
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: "36px",
            height: "36px",
            borderRadius: "50%",
            cursor: "pointer",
            opacity: hover ? 1 : 0.35,
            transition: "opacity 0.2s ease, background 0.15s ease",
            background: hover ? "rgba(0,0,0,0.35)" : "transparent",
          }}
        >
          <Unlock size={20} color="rgba(255,255,255,0.95)" />
        </div>
      </Tooltip>
    </div>
  );
}

export default function DesktopLyricsPage() {
  const {
    song,
    karaokeLines,
    estimatedTime,
    isPlaying,
    playMode,
    settings,
    isLocked,
    volume,
    sendControl,
    setVolume,
    lock,
    unlock,
  } = useDesktopLyricsSync();

  const [isHovered, setIsHovered] = useState(false);
  const [btnVisible, setBtnVisible] = useState(false);
  const [btnHover, setBtnHover] = useState(false);
  const isLockedRef = useRef(isLocked);
  isLockedRef.current = isLocked;
  const hideUnlockBtnRef = useRef(settings.hideUnlockBtn);
  hideUnlockBtnRef.current = settings.hideUnlockBtn;

  // 强制 html/body/#root 背景透明
  // index.css 中 :root 设置了 background-color: #242424，
  // 这会填充 WebView2 导致 setIgnoreCursorEvents 穿透失效。
  // 桌面歌词窗口必须确保背景完全透明。
  useEffect(() => {
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("root");
    const prevHtmlBg = html.style.background;
    const prevBodyBg = body.style.background;
    const prevRootBg = root?.style.background;

    html.style.background = "transparent";
    body.style.background = "transparent";
    if (root) root.style.background = "transparent";

    return () => {
      html.style.background = prevHtmlBg;
      body.style.background = prevBodyBg;
      if (root) root.style.background = prevRootBg || "";
    };
  }, []);

  // 恢复窗口位置
  useEffect(() => {
    restoreWindowPosition();
  }, []);

  // 锁定/解锁状态处理
  // 锁定：开启歌词窗口穿透，启用 Rust 鼠标钩子拦截解锁按钮区域的点击；
  //       轮询光标位置驱动内嵌解锁按钮的显隐与悬停态
  // 解锁：关闭穿透，停用钩子，隐藏解锁按钮
  useEffect(() => {
    if (!isLocked) {
      setIgnoreCursorEvents(false);
      setBtnVisible(false);
      setBtnHover(false);
      setUnlockHookArmed(false);
      disableUnlockHook();
      return;
    }

    // 锁定状态：开启穿透 + 启用解锁点击钩子
    setIgnoreCursorEvents(true);
    setBtnVisible(false);
    setBtnHover(false);
    enableUnlockHook();

    let active = true;
    let intervalId: ReturnType<typeof setInterval>;

    // 每 200ms 轮询：光标在窗口内且未隐藏按钮 → 显示内嵌按钮并激活钩子拦截，
    // 否则隐藏；悬停态由光标是否落在按钮区域内决定
    intervalId = setInterval(async () => {
      if (!active || !isLockedRef.current) return;
      try {
        // 隐藏解锁按钮开关开启时，强制隐藏，鼠标移入也不自动显示
        const inside = await isCursorInWindow();
        const armed = !hideUnlockBtnRef.current && inside;
        setBtnVisible(armed);
        setBtnHover(armed && (await isCursorInUnlockArea()));
        await setUnlockHookArmed(armed);
      } catch {
        // ignore
      }
    }, 200);

    return () => {
      active = false;
      clearInterval(intervalId);
      setBtnVisible(false);
      setBtnHover(false);
      setUnlockHookArmed(false);
      disableUnlockHook();
    };
  }, [isLocked]);

  // 解锁后，解锁按钮消失但光标已在桌面歌词窗口内（未锁定态窗口不再穿透），
  // 需要手动显示控制栏
  useEffect(() => {
    if (!isLocked) {
      setIsHovered(true);
    }
  }, [isLocked]);

  // 监听 Rust 鼠标钩子发出的解锁事件（命中内嵌解锁按钮区域时触发）
  useEffect(() => {
    const setup = async () => {
      const unlisten = await onUnlockBtnClicked(() => {
        unlock();
      });
      return unlisten;
    };

    let unlisten: (() => void) | undefined;
    setup().then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 未锁定状态：正常鼠标事件
  const handleMouseEnter = useCallback(() => {
    if (!isLockedRef.current) {
      setIsHovered(true);
    }
  }, []);

  const handleMouseLeave = useCallback(() => {
    if (!isLockedRef.current) {
      setIsHovered(false);
    }
  }, []);

  // 拖动窗口（未锁定时）
  const handleMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (isLockedRef.current) return;
      // 仅在非控制栏区域触发拖动
      // 控制栏内部会 stopPropagation，所以这里收到的都是背景区域
      startDragging();
      // 拖动结束后保存位置
      const handleUp = () => {
        saveWindowPosition();
        window.removeEventListener("mouseup", handleUp);
      };
      window.addEventListener("mouseup", handleUp);
    },
    []
  );

  // 控制指令
  const handleControl = useCallback(
    (action: ControlAction, value?: number) => {
      if (action === "lock") {
        lock();
      } else if (action === "unlock") {
        unlock();
      } else if (action === "volume") {
        setVolume(value ?? 0.7);
      } else {
        sendControl(action);
      }
    },
    [sendControl, lock, unlock, setVolume]
  );

  // 窗口样式
  const containerStyle: React.CSSProperties = {
    width: "100%",
    height: "100%",
    position: "relative",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    cursor: isLocked ? "default" : isHovered ? "move" : "default",
    transition: "background 0.2s ease, border-radius 0.2s ease",
    // 未锁定 + 悬浮时显示背景轮廓
    ...(isHovered && !isLocked
      ? {
          background: "rgba(0, 0, 0, 0.15)",
          borderRadius: "12px",
          border: "1px solid rgba(255, 255, 255, 0.12)",
        }
      : {}),
  };

  // 无歌曲时显示占位
  if (!song) {
    return (
      <div
        style={containerStyle}
        onMouseEnter={handleMouseEnter}
        onMouseLeave={handleMouseLeave}
        onMouseDown={handleMouseDown}
      >
        <span
          style={{
            fontSize: `${settings.fontSize * 0.7}px`,
            color: settings.baseColor,
            fontWeight: "bold",
            textShadow: `
              -1px -1px 0 rgba(0,0,0,0.8),
              1px -1px 0 rgba(0,0,0,0.8),
              -1px 1px 0 rgba(0,0,0,0.8),
              1px 1px 0 rgba(0,0,0,0.8)
            `,
          }}
        >
          ♪ NexBox 桌面歌词 ♪
        </span>
        {/* 控制栏仅未锁定时显示；锁定态的解锁按钮内嵌于同窗口，由钩子响应点击 */}
        {isHovered && !isLocked && (
          <LyricsControlBar
            isPlaying={isPlaying}
            playMode={playMode}
            volume={volume}
            onControl={handleControl}
          />
        )}
        {isLocked && btnVisible && <LockedUnlockBtn hover={btnHover} />}
      </div>
    );
  }

  return (
    <div
      style={containerStyle}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onMouseDown={handleMouseDown}
    >
      <div
        style={{
          width: "100%",
          height: "100%",
          padding: "8px 16px",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <LyricsCanvas
          lines={karaokeLines}
          currentTime={estimatedTime}
          fontSize={settings.fontSize}
          fontFamily={settings.fontFamily}
          highlightColor={settings.highlightColor}
          baseColor={settings.baseColor}
          lineCount={settings.lineCount}
          isPlaying={isPlaying}
          showTranslation={settings.showTranslation}
        />
      </div>

      {/* 控制栏仅未锁定时悬浮显示；锁定态的解锁按钮内嵌于同窗口，由钩子响应点击 */}
      {isHovered && !isLocked && (
        <LyricsControlBar
          isPlaying={isPlaying}
          playMode={playMode}
          volume={volume}
          onControl={handleControl}
        />
      )}
      {isLocked && btnVisible && <LockedUnlockBtn hover={btnHover} />}
    </div>
  );
}
