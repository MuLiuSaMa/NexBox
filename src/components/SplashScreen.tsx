import { useAppStartup } from "@/contexts/app-startup-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { Box } from "@chakra-ui/react";
import { useEffect, useRef, useState } from "react";
import { store } from "@/lib/store";
import { useSplashWebgl } from "@/hooks/use-splash-webgl";

const DEFAULT_LOGO = "/logo/Chinesew.webp";

export function SplashScreen() {
  const { startupProgress } = useAppStartup();
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();
  // WebGL 霓虹线动画背景（配色跟随主题色，与 Mineradio 区分）
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  useSplashWebgl(canvasRef, primaryColor);
  // 同步从 localStorage 读取自定义 LOGO，首帧即显示，避免先用默认 LOGO 再切换
  const [logoSrc, setLogoSrc] = useState<string>(() => {
    try {
      return localStorage.getItem("nexbox_splash_logo") || DEFAULT_LOGO;
    } catch {
      return DEFAULT_LOGO;
    }
  });

  useEffect(() => {
    // 仅当 localStorage 无自定义 LOGO 时，回退读取 store（存量迁移）
    try {
      if (localStorage.getItem("nexbox_splash_logo")) return;
    } catch {
      /* ignore */
    }
    (async () => {
      const customLogo = await store.get<string>("nexbox_splash_logo");
      if (customLogo) {
        setLogoSrc(customLogo);
      }
    })();
  }, []);

  return (
    <Box
      w="100vw"
      h="100vh"
      className="nexbox-splash"
      position="fixed"
      top="0"
      left="0"
      zIndex="9999"
      data-tauri-drag-region
      opacity={startupProgress >= 100 ? 0 : 1}
      transition="opacity 0.4s ease-out"
      willChange="opacity"
    >
      {/* 背景层：渐变斜光 + 网格线 + 呼吸（对应 Mineradio #splash::before） */}
      <Box className="nexbox-splash-grid" />
      {/* 背景层：WebGL 霓虹线动画（对应 Mineradio #splash-canvas） */}
      <canvas className="nexbox-splash-canvas" ref={canvasRef} />
      {/* 背景层：四周暗角（对应 Mineradio #splash::after） */}
      <Box className="nexbox-splash-vignette" />
      {/* 背景层：胶片噪点（对应 Mineradio .splash-bg-noise） */}
      <Box className="nexbox-splash-noise" />

      {/* Logo - 居中 */}
      <Box
        position="absolute"
        top="50%"
        left="50%"
        transform="translate(-50%, -50%)"
        zIndex="10"
        data-tauri-drag-region
      >
        <Box
          as="img"
          src={logoSrc}
          alt="NexBox Logo"
          maxH="150px"
          maxW="300px"
          objectFit="contain"
          draggable={false}
        />
      </Box>

      {/* 进度条 - 中下方 */}
      <Box
        position="absolute"
        bottom="25%"
        left="50%"
        transform="translateX(-50%)"
        w="40%"
        maxW="160px"
        zIndex="10"
        pointerEvents="none"
      >
        <Box
          bg="rgba(255,255,255,0.15)"
          h="2px"
          borderRadius="full"
          overflow="hidden"
        >
          <Box
            h="100%"
            bg={primaryColor}
            w={`${startupProgress}%`}
            transition="width 200ms ease-out"
            willChange="width"
          />
        </Box>
      </Box>
    </Box>
  );
}
