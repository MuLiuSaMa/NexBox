/**
 * 桌面歌词窗口管理工具
 *
 * 在桌面歌词窗口中运行，提供：
 * - 窗口拖动
 * - 鼠标穿透切换 (setIgnoreCursorEvents)
 * - 位置记忆
 */

import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Store } from "@tauri-apps/plugin-store";

const lyricsWindow = getCurrentWindow();

let storeInstance: Store | null = null;
async function getStore(): Promise<Store> {
  if (!storeInstance) {
    storeInstance = await Store.load("music-player-settings.json");
  }
  return storeInstance;
}

/** 开始拖动窗口 */
export async function startDragging() {
  try {
    await lyricsWindow.startDragging();
  } catch (e) {
    console.error("[DesktopLyrics] startDragging failed:", e);
  }
}

/** 设置鼠标穿透（使用 Tauri 标准 API）
 *
 * 关键前提：WebView2 的 CSS 背景必须为 transparent，
 * 否则 solid background 会阻止鼠标穿透。
 * 见 DesktopLyricsPage 中的 useEffect 强制设置背景透明。
 */
export async function setIgnoreCursorEvents(ignore: boolean) {
  try {
    await lyricsWindow.setIgnoreCursorEvents(ignore);
  } catch (e) {
    console.error("[DesktopLyrics] setIgnoreCursorEvents failed:", e);
  }
}

/** 将窗口位置钳制到所在显示器工作区（排除任务栏）内 */
export async function clampToWorkArea(): Promise<boolean> {
  try {
    return await invoke<boolean>("clamp_lyrics_window_position");
  } catch (e) {
    console.error("[DesktopLyrics] clampToWorkArea failed:", e);
    return false;
  }
}

/** 保存窗口位置（先钳制到工作区内再保存，避免记录越界坐标） */
export async function saveWindowPosition() {
  try {
    await clampToWorkArea();
    const pos = await lyricsWindow.outerPosition();
    const store = await getStore();
    await store.set("desktopLyricsPosition", { x: pos.x, y: pos.y });
    await store.save();
  } catch (e) {
    console.error("[DesktopLyrics] saveWindowPosition failed:", e);
  }
}

/** 恢复窗口位置（恢复后立即钳制到工作区内，兼容分辨率变化/副屏移除场景） */
export async function restoreWindowPosition() {
  try {
    const store = await getStore();
    const pos = await store.get<{ x: number; y: number }>("desktopLyricsPosition");
    if (pos) {
      const { LogicalPosition } = await import("@tauri-apps/api/window");
      await lyricsWindow.setPosition(new LogicalPosition(pos.x, pos.y));
    }
    await clampToWorkArea();
  } catch (e) {
    console.error("[DesktopLyrics] restoreWindowPosition failed:", e);
  }
}

/**
 * 将桌面歌词窗口居中到屏幕中央并保存位置。
 *
 * 注意：本函数可能从主窗口（设置弹窗）调用，不能使用模块级
 * `lyricsWindow`（那是 getCurrentWindow()），必须按 label 获取
 * desktop-lyrics 窗口句柄。
 */
export async function centerLyricsWindow() {
  try {
    await invoke("center_lyrics_window");
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const win = await WebviewWindow.getByLabel("desktop-lyrics");
    if (win) {
      const pos = await win.outerPosition();
      const store = await getStore();
      await store.set("desktopLyricsPosition", { x: pos.x, y: pos.y });
      await store.save();
    }
  } catch (e) {
    console.error("[DesktopLyrics] centerLyricsWindow failed:", e);
  }
}

/** 启用 Rust 端解锁点击钩子(锁定进入时调用,幂等) */
export async function enableUnlockHook() {
  try {
    await invoke("enable_lyrics_unlock_hook");
  } catch (e) {
    console.error("[DesktopLyrics] enableUnlockHook failed:", e);
  }
}

/** 停用 Rust 端解锁点击钩子(解锁/关闭时调用,幂等) */
export async function disableUnlockHook() {
  try {
    await invoke("disable_lyrics_unlock_hook");
  } catch (e) {
    console.error("[DesktopLyrics] disableUnlockHook failed:", e);
  }
}

/** 激活/停用钩子拦截:仅当内嵌解锁按钮可见时置 true,钩子才拦截点击 */
export async function setUnlockHookArmed(armed: boolean) {
  try {
    await invoke("set_lyrics_unlock_hook_armed", { armed });
  } catch (e) {
    console.error("[DesktopLyrics] setUnlockHookArmed failed:", e);
  }
}

/**
 * 监听解锁事件(由 Rust 端鼠标钩子命中解锁区域时发射)
 * 在 DesktopLyricsPage 中调用,收到事件后执行解锁
 */
export async function onUnlockBtnClicked(callback: () => void) {
  const { listen } = await import("@tauri-apps/api/event");
  return await listen("lyrics:unlock-triggered", () => {
    callback();
  });
}
export async function isCursorInWindow(): Promise<boolean> {
  try {
    const cursor = await invoke<{ x: number; y: number }>("get_cursor_position");
    const [pos, size] = await Promise.all([
      lyricsWindow.outerPosition(),
      lyricsWindow.outerSize(),
    ]);
    // Tauri v2 的 outerPosition()/outerSize() 以及 Win32 GetCursorPos
    // 均返回物理屏幕坐标，无需乘以 devicePixelRatio。
    return (
      cursor.x >= pos.x &&
      cursor.x <= pos.x + size.width &&
      cursor.y >= pos.y &&
      cursor.y <= pos.y + size.height
    );
  } catch {
    return false;
  }
}

/**
 * 检查光标是否在解锁按钮区域内（锁定状态下使用）
 *
 * 解锁按钮位于窗口顶部中央，约 100x50 物理像素的区域。
 * 只在这个小区域内关闭穿透显示解锁按钮，其余区域保持穿透。
 */
export async function isCursorInUnlockArea(): Promise<boolean> {
  try {
    const cursor = await invoke<{ x: number; y: number }>("get_cursor_position");
    const [pos, size] = await Promise.all([
      lyricsWindow.outerPosition(),
      lyricsWindow.outerSize(),
    ]);
    // 解锁按钮位于窗口顶部中央
    const unlockWidth = Math.min(120, size.width as number);
    const unlockHeight = 50;
    const unlockX = pos.x + ((size.width as number) - unlockWidth) / 2;
    const unlockY = pos.y;
    return (
      cursor.x >= unlockX &&
      cursor.x <= unlockX + unlockWidth &&
      cursor.y >= unlockY &&
      cursor.y <= unlockY + unlockHeight
    );
  } catch {
    return false;
  }
}
