"use client";

import { Box, useColorModeValue } from "@chakra-ui/react";
import { ReactNode, useEffect, useLayoutEffect, useRef, useState, useMemo } from "react";
import { useLocation } from "react-router-dom";
import { Sidebar } from "./sidebar";
import { TitleBar } from "./title-bar";
import { MRBackground } from "./MRBackground";
import { FloatingScrollbar } from "./floating-scrollbar";
import { useBackground, buildBackgroundFilter } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { store } from "@/lib/store";
import { deriveSplashColors } from "@/lib/color-utils";
import { useEscBack } from "@/hooks/use-esc-back";
import { APP_TOP_FADE_MASK } from "@/config/ui-metrics";

export function useNavPosition() {
  const [navPosition, setNavPosition] = useState<"left" | "top">("left");

  useEffect(() => {
    (async () => {
      const nv = await store.get<string>("nexbox_nav_position");
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
  const { backgroundMode, customBgImages, activeBgIndex, dynamicBgVideo, activePresetIndex, presetBackgrounds, backgroundBlur, backgroundOpacity, backgroundBrightness, backgroundContrast, backgroundSaturation, jellyBounceEnabled, mrColorMode, mrCustomColor, liquidGlassEnabled } = useBackground();
  const { config } = useThemeColor();
  const navPosition = useNavPosition();
  useEscBack();
  const location = useLocation();
  const videoRef = useRef<HTMLVideoElement>(null);
  const [videoReady, setVideoReady] = useState(false);
  const idCounter = useRef(0);

  // 切换页面时把主滚动容器复位到顶部。
  // MainLayout 在路由切换时不重新挂载，滚动容器是同一个 DOM，
  // 不复位的话「上一页滑到底 → 进新页面」会直接停在新页面底部。
  useLayoutEffect(() => {
    const el = document.getElementById("app-main-scroll");
    if (el) {
      el.scrollTop = 0;
      el.scrollLeft = 0;
    }
  }, [location.pathname]);

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
  const showAnyBg = Boolean(showImageBg || showPresetBg || showDynamicBg || showMrBg);
  // 模糊 + 亮度 / 对比度 / 饱和度统一走一条 filter
  const backgroundFilter = useMemo(
    () => buildBackgroundFilter({
      blur: backgroundBlur,
      brightness: backgroundBrightness,
      contrast: backgroundContrast,
      saturation: backgroundSaturation,
    }),
    [backgroundBlur, backgroundBrightness, backgroundContrast, backgroundSaturation],
  );
  const backgroundLayerOpacity = backgroundOpacity / 100;
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

  // 顶部内容渐隐：给内容外层加 alpha mask（只改内容不透明度，不新增遮挡层）。
  //
  // 难点：带 mask 的元素会成为 Backdrop Root，其内玻璃卡片的 backdrop-filter 就采样不到
  // 容器外的固定背景层（图片 / 视频 / MR 都是它的兄弟节点），玻璃会整块失效。
  // 解法：在 mask 子树内再放一份与视口完全同框的「背景镜像」，玻璃有东西可采样；
  // 渐隐把镜像淡掉时露出的又是同一张底层背景，两者像素对齐，所以看不出接缝。
  // 视频 / MR(WebGL) 背景无法低成本镜像（双解码 / 双 GL 上下文），这两种模式停用渐隐。
  const fadeMirrorUrl = showPresetBg
    ? activePreset?.path ?? null
    : showImageBg
      ? activeImage
      : null;
  const topFadeEnabled = !showDynamicBg && !showMrBg;

  return (
    <Box
      position="relative"
      minHeight="100vh"
      bg="transparent"
    >
      {/* 背景交叉淡化层：用 <img> 而非 CSS background-image，
          原因：CSS value 有约 2MB 上限，大图 data URL 会被截断导致黑色 */}
      {/* 不透明度 < 100% 时由这层主题底色垫底，避免露出 WebView 默认黑底 */}
      {showAnyBg && (
        <Box
          position="fixed"
          top={0}
          left={0}
          right={0}
          bottom={0}
          zIndex={0}
          bg={bgColor}
        />
      )}
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
                filter: backgroundFilter,
                willChange: backgroundFilter ? "filter" : undefined,
                opacity: backgroundLayerOpacity,
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
              filter: backgroundFilter,
              opacity: backgroundLayerOpacity,
            }}
          />
        </Box>
      )}
      {showMrBg && <MRBackground filter={backgroundFilter} opacity={backgroundLayerOpacity} colors={mrColors} />}
      <TitleBar />
      <Sidebar />
      {/* 顶部渐隐包装层：只在液态玻璃关闭时启用 mask（开启时 mask 会切断玻璃的背景采样）。 */}
      <Box
        position="fixed"
        left={0}
        right={0}
        top={0}
        bottom={0}
        zIndex={1}
        pointerEvents="none"
        sx={{
          maskImage: topFadeEnabled ? APP_TOP_FADE_MASK : "none",
          WebkitMaskImage: topFadeEnabled ? APP_TOP_FADE_MASK : "none",
        }}
      >
        {/* 背景镜像：与视口完全同框（fixed 包装层内 absolute inset 0），
            玻璃卡片在同一个 backdrop root 里采样它；被渐隐淡掉时露出同一张底层背景。 */}
        {liquidGlassEnabled && topFadeEnabled && (
          <Box
            aria-hidden
            position="absolute"
            top={0}
            right={0}
            bottom={0}
            left={0}
            zIndex={0}
            overflow="hidden"
            bg={showAnyBg ? bgColor : undefined}
          >
            {fadeMirrorUrl && (
              <img
                src={fadeMirrorUrl}
                alt=""
                style={{
                  width: "100%",
                  height: "100%",
                  objectFit: "cover",
                  display: "block",
                  filter: backgroundFilter,
                  willChange: backgroundFilter ? "filter" : undefined,
                  opacity: backgroundLayerOpacity,
                }}
              />
            )}
          </Box>
        )}
      <Box 
        id="app-main-scroll"
        tabIndex={-1}
        outline="none"
        position="absolute"
        top={0}
        right={0}
        bottom={0}
        left={0}
        zIndex={1}
        pointerEvents="auto"
        pt={isNavTop ? "120px" : "56px"}
        pb={8}
        pl={isNavTop ? 8 : "128px"}
        transition="padding 0.4s cubic-bezier(0.4, 0, 0.2, 1)"
        pr="40px" 
        overflowY="auto"
        overflowX="hidden"
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
        <Box position="relative" zIndex={1} minHeight="100%">
          {children}
        </Box>
      </Box>
      </Box>
      <FloatingScrollbar targetId="app-main-scroll" />
    </Box>
  );
}
