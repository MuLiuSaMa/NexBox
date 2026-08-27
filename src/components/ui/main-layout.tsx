"use client";

import { Box, useColorModeValue } from "@chakra-ui/react";
import { ReactNode, useEffect, useRef, useState, useMemo } from "react";
import { useLocation } from "react-router-dom";
import { Sidebar } from "./sidebar";
import { TitleBar } from "./title-bar";
import { MRBackground } from "./MRBackground";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { store } from "@/lib/store";
import { deriveSplashColors } from "@/lib/color-utils";

function useNavPosition() {
  const [navPosition, setNavPosition] = useState<"left" | "top">("left");

  useEffect(() => {
    (async () => {
      let nv = await store.get<string>("nexbox_nav_position");
      if (nv === "top" || nv === "left") {
        setNavPosition(nv);
      } else {
        setNavPosition(localStorage.getItem("nexbox_nav_position") === "top" ? "top" : "left");
      }
    })();
  }, []);

  useEffect(() => {
    const handler = (e: CustomEvent) => {
      setNavPosition(e.detail === "top" ? "top" : "left");
    };
    window.addEventListener("nav-position-changed", handler as EventListener);
    return () => {
      window.removeEventListener("nav-position-changed", handler as EventListener);
    };
  }, []);

  return navPosition;
}

interface MainLayoutProps {
  children: ReactNode;
}

export function MainLayout({ children }: MainLayoutProps) {
  const bgColor = useColorModeValue("#fafafa", "#0a0a0a");
  const { backgroundMode, customBgImages, activeBgIndex, dynamicBgVideo, activePresetIndex, presetBackgrounds, backgroundBlur, jellyBounceEnabled, mrColorMode, mrCustomColor } = useBackground();
  const { config } = useThemeColor();
  const navPosition = useNavPosition();
  const location = useLocation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoReady, setVideoReady] = useState(false);
  const idCounter = useRef(0);

  // MR 背景配色：适配主题色（config.primaryColor）或自定义颜色，派生三通道霓虹色
  const mrColors = useMemo(() => {
    const baseColor = mrColorMode === "theme" ? config.primaryColor : mrCustomColor;
    return deriveSplashColors(baseColor);
  }, [mrColorMode, mrCustomColor, config.primaryColor]);

  const activeImage = customBgImages[activeBgIndex];
  const activePreset = presetBackgrounds[activePresetIndex];
  const showImageBg = backgroundMode === "image" && activeImage;
  const showDynamicBg = backgroundMode === "dynamic" && dynamicBgVideo;
  const showPresetBg = backgroundMode === "preset" && activePreset;
  const showMrBg = backgroundMode === "mr";
  // 始终保存最新的"是否显示动态背景"，供窗口可见性监听使用（避免 effect 反复重订阅）
  const showDynamicBgRef = useRef(showDynamicBg);
  useEffect(() => {
    showDynamicBgRef.current = showDynamicBg;
  }, [showDynamicBg]);
  const videoSrc = useMemo(() => {
    if (!dynamicBgVideo) return null;
    return convertFileSrc(dynamicBgVideo);
  }, [dynamicBgVideo]);

  // 背景交叉淡化
  interface BgLayer {
    url: string;
    id: number;
    fading: boolean;
  }
  const [bgLayers, setBgLayers] = useState<BgLayer[]>([]);

  useEffect(() => {
    const src = showPresetBg
      ? activePreset?.path ?? null
      : showImageBg
        ? activeImage
        : null;

    if (!src) {
      setBgLayers([]);
      return;
    }

    const newId = ++idCounter.current;

    setBgLayers((prev) => [
      ...prev.map((l) => ({ ...l, fading: true })),
      { url: src, id: newId, fading: false },
    ]);

    // 动画完成后移除旧的图层
    const timer = setTimeout(() => {
      setBgLayers((prev) => prev.filter((l) => l.id === newId));
    }, 600);

    return () => clearTimeout(timer);
  }, [showPresetBg, activePresetIndex, showImageBg, activeBgIndex, activeImage, activePreset]);

  useEffect(() => {
    if (!dynamicBgVideo) {
      setVideoReady(false);
    } else {
      setVideoReady(false);
    };
  }, [dynamicBgVideo]);

  useEffect(() => {
    let bgColorToUse = bgColor;

    if (showImageBg || showPresetBg || showDynamicBg || showMrBg) {
      bgColorToUse = "transparent";
    }

    document.body.style.backgroundColor = bgColorToUse;

    return () => {
      document.body.style.backgroundColor = "";
    };
  }, [showImageBg, showDynamicBg, showPresetBg, showMrBg, bgColor]);

  useEffect(() => {
    // 窗口隐藏时保持暂停，避免切回动态背景后视频在托盘状态下继续解码
    if (videoRef.current && dynamicBgVideo && !document.hidden) {
      videoRef.current.play().catch(() => {});
    }
  }, [dynamicBgVideo, showDynamicBg]);

  // 窗口最小化 / 隐藏到托盘时自动暂停动态背景视频，减少 CPU 占用；
  // 恢复显示后自动继续播放。双保险：
  //   1) 标准 Web API visibilitychange（覆盖最小化场景）
  //   2) Rust 端 window-visibility-changed 事件（覆盖隐藏到托盘等 visibilitychange 不可靠的场景）
  useEffect(() => {
    const applyVisibility = (visible: boolean) => {
      // 隐藏时停止 WebView2 GPU 渲染，释放 GPU 资源
      document.body.style.contentVisibility = visible ? "" : "hidden";
      // 暂停/恢复视频背景
      if (!showDynamicBgRef.current) return;
      const video = videoRef.current;
      if (!video) return;
      if (visible) {
        video.play().catch(() => {});
      } else {
        video.pause();
      }
    };

    const onVisibilityChange = () => applyVisibility(!document.hidden);
    document.addEventListener("visibilitychange", onVisibilityChange);

    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    listen<boolean>("window-visibility-changed", (e) => {
      applyVisibility(e.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisten?.();
    };
  }, []);

  // 顶部导航栏模式下，body 不滚动，内容区域自行管理滚动
  useEffect(() => {
    if (navPosition === "top") {
      document.body.style.overflow = "hidden";
    } else {
      document.body.style.overflow = "";
    }
    return () => {
      document.body.style.overflow = "";
    };
  }, [navPosition]);

  // 果冻弹跳效果开关：在 body 上添加/移除 class，供全局 CSS 使用
  useEffect(() => {
    if (jellyBounceEnabled) {
      document.body.classList.add("jelly-bounce-enabled");
    } else {
      document.body.classList.remove("jelly-bounce-enabled");
      document.body.classList.remove("jelly-bounce-active");
    }
    return () => {
      document.body.classList.remove("jelly-bounce-enabled");
      document.body.classList.remove("jelly-bounce-active");
    };
  }, [jellyBounceEnabled]);

  // 路由切换时触发果冻弹跳动画：在 body 添加 .jelly-bounce-active，
  // 所有带 jelly-bounce-* className 的元素会播放动画。动画时长后移除 class。
  // 这样异步加载的内容（如三角洲改枪码列表）不会在挂载时触发动画，避免布局跳动。
  useEffect(() => {
    if (!jellyBounceEnabled) return;
    // 移除再添加以重新触发 CSS animation
    document.body.classList.remove("jelly-bounce-active");
    void document.body.offsetWidth; // 强制 reflow
    document.body.classList.add("jelly-bounce-active");
    const timer = setTimeout(() => {
      document.body.classList.remove("jelly-bounce-active");
    }, 700);
    return () => {
      clearTimeout(timer);
      document.body.classList.remove("jelly-bounce-active");
    };
  }, [location.pathname, jellyBounceEnabled]);

  const isNavTop = navPosition === "top";

  return (
    <Box
      position="relative"
      minHeight="100vh"
      bg="transparent"
    >
      {/* 背景交叉淡化层：用 <img> 而非 CSS background-image，
          原因：CSS value 有约 2MB 上限，大图 data URL 会被截断导致黑色 */}
      <Box sx={{
        "@keyframes bgFadeIn": {
          from: { opacity: 0 },
          to: { opacity: 1 },
        },
      }}>
        {bgLayers.map((layer) => (
          <Box
            key={layer.id}
            position="fixed"
            top={0}
            left={0}
            right={0}
            bottom={0}
            zIndex={0}
            opacity={layer.fading ? 0 : 1}
            overflow="hidden"
            transition={layer.fading ? "opacity 0.5s ease-in-out" : undefined}
            animation={!layer.fading ? "bgFadeIn 0.5s ease-in-out" : undefined}
          >
            <img
              src={layer.url}
              alt=""
              style={{
                width: "100%",
                height: "100%",
                objectFit: "cover",
                display: "block",
                filter: backgroundBlur > 0 ? `blur(${backgroundBlur}px)` : undefined,
                willChange: backgroundBlur > 0 ? "filter" : undefined,
              }}
            />
          </Box>
        ))}
      </Box>
      {showDynamicBg && (
        <Box
          position="fixed"
          top={0}
          left={0}
          right={0}
          bottom={0}
          zIndex={0}
          overflow="hidden"
          opacity={videoReady ? 1 : 0}
          transition="opacity 0.6s ease-in"
        >
          <video
            ref={videoRef}
            src={videoSrc!}
            autoPlay
            muted
            loop
            playsInline
            preload="auto"
            onLoadedData={() => setVideoReady(true)}
            onError={(e) => console.error("视频加载失败:", e)}
            style={{
              width: "100%",
              height: "100%",
              objectFit: "cover",
              filter: backgroundBlur > 0 ? `blur(${backgroundBlur}px)` : undefined,
            }}
          />
        </Box>
      )}
      {showMrBg && <MRBackground blur={backgroundBlur} colors={mrColors} />}
      <TitleBar />
      <Sidebar />
      <Box 
        position="relative"
        zIndex={1}
        ml={isNavTop ? 0 : "96px"}
        pt={isNavTop ? "120px" : "56px"}
        pb={8}
        transition="margin 0.4s cubic-bezier(0.4, 0, 0.2, 1), padding 0.4s cubic-bezier(0.4, 0, 0.2, 1)"
        px={8} 
        pr="40px" 
        overflowY="auto"
        overflowX="hidden"
        h="calc(100vh)"
        sx={{
          scrollbarGutter: "stable",
          "&::-webkit-scrollbar": {
            width: "6px",
            height: "6px",
          },
          "&::-webkit-scrollbar-track": {
            background: "transparent",
            margin: "10px 0",
          },
          "&::-webkit-scrollbar-thumb": {
            background: config.primaryColor,
            borderRadius: "3px",
            minHeight: "40px",
          },
          "&::-webkit-scrollbar-thumb:hover": {
            background: config.primaryColor,
            opacity: 0.8,
            filter: "brightness(0.9)",
          },
        }}
      >
        <Box position="relative" minHeight="100%">
          {children}
        </Box>
      </Box>
    </Box>
  );
}
