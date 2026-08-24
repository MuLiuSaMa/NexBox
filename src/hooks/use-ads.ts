import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useState, useEffect, useCallback } from "react";
import { openExternal } from "@/hooks/use-qq-groups";

/** 开屏广告项 */
export interface SplashAd {
  image?: string;
  link?: string;
  name?: string;
}

/** 主页广告卡片项 */
export interface HomeAd {
  image?: string;
  name?: string;
  description?: string;
  link?: string;
}

/** 广告配置（splash 为空 → 不弹开屏；home 为空 → 不显示主页卡片） */
export interface AdsConfig {
  update_time?: string;
  splash: SplashAd[];
  home: HomeAd[];
}

/**
 * 拉取广告配置（后端从 gitee ads.json 获取，带内存缓存），
 * 返回 { splash, home, loading }。网络失败时为空数组，不显示任何广告。
 */
export function useAds() {
  const [splash, setSplash] = useState<SplashAd[]>([]);
  const [home, setHome] = useState<HomeAd[]>([]);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<AdsConfig>("get_ads");
      setSplash(data?.splash || []);
      setHome(data?.home || []);
    } catch (e) {
      console.error("[Ads] get_ads failed:", e);
      setSplash([]);
      setHome([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  return { splash, home, loading, reload };
}

export { openExternal };

/**
 * 解析广告图片 URL 为可显示地址：让后端从 gitee 下载到缓存，再用 convertFileSrc 走 Tauri 资产协议。
 * 返回 { src, loading }：loading 为 true 表示图片还没下载好。
 */
export function useAdImage(url?: string) {
  const [state, setState] = useState<{ src?: string; loading: boolean }>({
    src: undefined,
    loading: false,
  });

  useEffect(() => {
    if (!url) {
      setState({ src: undefined, loading: false });
      return;
    }
    setState((s) => ({ ...s, loading: true }));
    let alive = true;
    (async () => {
      try {
        const path = await invoke<string>("get_ad_image", { url });
        if (alive) setState({ src: path ? convertFileSrc(path) : undefined, loading: false });
      } catch (e) {
        console.error("[Ads] get_ad_image failed:", e);
        if (alive) setState({ src: undefined, loading: false });
      }
    })();
    return () => {
      alive = false;
    };
  }, [url]);

  return state;
}