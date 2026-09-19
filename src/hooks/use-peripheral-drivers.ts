import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useState, useEffect, useCallback } from "react";

/** 单个外设驱动品牌 */
export interface PeripheralDriver {
  id: string;
  /** 品牌显示名，如 "雷蛇 Razer" */
  name: string;
  /** 图标 URL（远程，如 gitee icons/），为空时前端显示品牌首字母 */
  icon?: string;
  /** 驱动地址：在线驱动优先，否则官方驱动/下载页 */
  url: string;
  /** online=网页在线驱动，download=官方驱动下载页 */
  kind?: string;
}

/**
 * 拉取外设驱动列表（后端从 gitee peripheral_drivers.json 获取，含内存缓存与内置兜底），
 * 返回 { drivers, loading, reload }。
 */
export function usePeripheralDrivers() {
  const [drivers, setDrivers] = useState<PeripheralDriver[]>([]);
  const [loading, setLoading] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<PeripheralDriver[]>("get_peripheral_drivers");
      setDrivers(data || []);
    } catch (e) {
      console.error("[PeripheralDrivers] get_peripheral_drivers failed:", e);
      setDrivers([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  return { drivers, loading, reload };
}

/**
 * 解析品牌图标 URL 为可显示地址：让后端从 gitee 下载到缓存，再用 convertFileSrc 走 Tauri 资产协议。
 * 返回 { src, loading }：loading 为 true 表示图标还没下载好。
 */
export function usePeripheralDriverIcon(url?: string) {
  const [state, setState] = useState<{ src?: string; loading: boolean }>({ src: undefined, loading: false });

  useEffect(() => {
    if (!url) {
      setState({ src: undefined, loading: false });
      return;
    }
    setState((s) => ({ ...s, loading: true }));
    let alive = true;
    (async () => {
      try {
        const path = await invoke<string>("get_peripheral_driver_icon", { url });
        if (alive) setState({ src: path ? convertFileSrc(path) : undefined, loading: false });
      } catch (e) {
        console.error("[PeripheralDrivers] get_peripheral_driver_icon failed:", e);
        if (alive) setState({ src: undefined, loading: false });
      }
    })();
    return () => {
      alive = false;
    };
  }, [url]);

  return state;
}

/**
 * 调用系统默认浏览器打开外设驱动页面（优先 Rust 端，兜底 plugin-opener）。
 */
export async function openDriverLink(driver: PeripheralDriver) {
  try {
    await invoke("open_system_browser", { url: driver.url });
    return;
  } catch (error) {
    console.warn("[PeripheralDrivers] open_system_browser failed:", error);
  }
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(driver.url);
  } catch (error) {
    console.error(`打开 ${driver.name} 驱动链接失败:`, error);
  }
}