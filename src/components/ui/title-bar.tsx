"use client";

import { Box, Flex, HStack, IconButton, Image, useColorModeValue } from "@chakra-ui/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { LuMinus, LuSquare, LuCopy, LuX } from "react-icons/lu";
import { useCallback, useState, useEffect, useRef } from "react";
import { GlobalSearch } from "./global-search";
import { GameModeSwitch } from "./game-mode-switch";
import { CloseConfirmDialog } from "../CloseConfirmDialog";
import { store } from "@/lib/store";

export function TitleBar() {
  const iconColor = useColorModeValue("gray.600", "gray.400");
  const hoverColor = useColorModeValue("gray.800", "gray.200");
  const closeHoverColor = useColorModeValue("red.600", "red.400");
  const minimizeHoverBg = useColorModeValue("gray.100", "gray.700");
  const closeHoverBg = useColorModeValue("red.50", "red.900");
  const bgColor = useColorModeValue("whiteAlpha.800", "blackAlpha.800");
  const logoSrc = useColorModeValue("/logo/NexBoxW.png", "/logo/NexBoxB.png");

  const [showCloseDialog, setShowCloseDialog] = useState(false);
  const [searchBarVisible, setSearchBarVisible] = useState(true);
  const [gameModeVisible, setGameModeVisible] = useState(true);
  const [navPosition, setNavPosition] = useState<"left" | "top">("left");
  const [isMaximized, setIsMaximized] = useState(false);
  // 音乐沉浸页面级全屏状态：全屏时淡出搜索框/游戏模式，左上角显示"缩小"按钮
  const [pageFs, setPageFs] = useState(false);

  useEffect(() => {
    const handler = (e: Event) => {
      setPageFs(!!(e as CustomEvent<boolean>).detail);
    };
    window.addEventListener("immersive-page-fullscreen", handler as EventListener);
    return () => {
      window.removeEventListener("immersive-page-fullscreen", handler as EventListener);
    };
  }, []);

  useEffect(() => {
    (async () => {
      // nexbox_search_bar_enabled
      let sv = await store.get<boolean>("nexbox_search_bar_enabled");
      if (sv !== null && sv !== undefined) {
        setSearchBarVisible(sv);
      } else {
        const ls = localStorage.getItem("nexbox_search_bar_enabled");
        setSearchBarVisible(ls === null ? true : ls === "true");
      }
      // nexbox_game_mode_enabled（顶栏游戏模式切换条显示）
      let gm = await store.get<boolean>("nexbox_game_mode_enabled");
      if (gm !== null && gm !== undefined) {
        setGameModeVisible(gm);
      } else {
        const ls = localStorage.getItem("nexbox_game_mode_enabled");
        setGameModeVisible(ls === null ? true : ls === "true");
      }
      // nexbox_nav_position
      let nv = await store.get<string>("nexbox_nav_position");
      if (nv === "top" || nv === "left") {
        setNavPosition(nv);
      } else {
        const ls = localStorage.getItem("nexbox_nav_position");
        setNavPosition(ls === "top" ? "top" : "left");
      }
    })();
  }, []);

  useEffect(() => {
    const handler = (e: CustomEvent) => {
      setGameModeVisible(!!e.detail);
    };
    window.addEventListener("game-mode-setting-changed", handler as EventListener);
    return () => {
      window.removeEventListener("game-mode-setting-changed", handler as EventListener);
    };
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

  useEffect(() => {
    const handler = (e: CustomEvent) => {
      setSearchBarVisible(!!e.detail);
    };
    window.addEventListener("search-bar-setting-changed", handler as EventListener);
    return () => {
      window.removeEventListener("search-bar-setting-changed", handler as EventListener);
    };
  }, []);

  const getCloseBehavior = useCallback((): string => {
    return "ask"; // 默认值，实际值在 useEffect 中异步加载
  }, []);

  // 存储 close_behavior 的引用，供 handleClose 使用
  const closeBehaviorRef = useRef<string>("ask");
  useEffect(() => {
    (async () => {
      let cb = await store.get<string>("nexbox_close_behavior");
      if (!cb) {
        cb = localStorage.getItem("nexbox_close_behavior") || "ask";
      }
      closeBehaviorRef.current = cb;
    })();
  }, []);

  const handleMouseDown = useCallback(async (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.closest("button") || target.closest("input") || target.closest('[role="search"]')) {
      return;
    }
    try {
      const appWindow = getCurrentWindow();
      await appWindow.startDragging();
    } catch (error) {
      console.error("Failed to start dragging:", error);
    }
  }, []);

  const handleMinimize = async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.minimize();
    } catch (error) {
      console.error("Failed to minimize window:", error);
    }
  };

  // 同步窗口最大化状态（拖拽到屏幕边缘触发最大化时也能同步图标）
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        setIsMaximized(await appWindow.isMaximized());
        unlisten = await appWindow.onResized(async () => {
          setIsMaximized(await appWindow.isMaximized());
        });
      } catch (error) {
        console.error("Failed to init maximize state:", error);
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  const handleMaximize = async () => {
    try {
      const appWindow = getCurrentWindow();
      const maximized = await appWindow.isMaximized();
      if (maximized) {
        await appWindow.unmaximize();
      } else {
        await appWindow.maximize();
      }
      setIsMaximized(await appWindow.isMaximized());
    } catch (error) {
      console.error("Failed to toggle maximize:", error);
    }
  };

  const handleClose = async () => {
    const behavior = closeBehaviorRef.current;
    switch (behavior) {
      case "close":
        await performClose();
        break;
      case "minimize":
        await performMinimizeToTray();
        break;
      case "ask":
      default:
        setShowCloseDialog(true);
        break;
    }
  };

  const performMinimizeToTray = async (savePreference: boolean = false) => {
    if (savePreference) {
      localStorage.setItem("nexbox_close_behavior", "minimize");
      await store.set("nexbox_close_behavior", "minimize");
      await store.save();
      closeBehaviorRef.current = "minimize";
    }
    try {
      await invoke("minimize_to_tray");
    } catch (error) {
      console.error("Failed to minimize to tray:", error);
    }
    setShowCloseDialog(false);
  };

  const performClose = async (savePreference: boolean = false) => {
    if (savePreference) {
      localStorage.setItem("nexbox_close_behavior", "close");
      await store.set("nexbox_close_behavior", "close");
      await store.save();
      closeBehaviorRef.current = "close";
    }
    try {
      await invoke("exit_app");
    } catch (error) {
      console.error("Failed to exit app:", error);
    }
    setShowCloseDialog(false);
  };

  return (
    <>
      <Box
        position="fixed"
        top={0}
        left={0}
        right={0}
        h="48px"
        zIndex={999}
        onMouseDown={handleMouseDown}
      >
        <Flex justify="space-between" align="center" h="full" pl={4} pr={4}>
          <Flex
            align="center"
            gap={3}
            ml={navPosition === "top" ? "16px" : "112px"}
            transition="margin 0.4s cubic-bezier(0.4, 0, 0.2, 1)"
            onMouseDown={(e) => e.stopPropagation()}
          >
            {/* 沉浸页面全屏：搜索框淡出，左上角显示"缩小"（退出全屏）按钮 */}
            {searchBarVisible && (
              <Box
                opacity={pageFs ? 0 : 1}
                visibility={pageFs ? "hidden" : "visible"}
                transition="opacity 0.3s ease"
                pointerEvents={pageFs ? "none" : "auto"}
              >
                <GlobalSearch />
              </Box>
            )}
          </Flex>
          <HStack id="window-controls" spacing={1} h="40px" align="center">
            {/* 沉浸页面全屏时游戏模式切换淡出，保留最小化/最大化/关闭 */}
            <Box
              transform="translateX(-12px)"
              opacity={pageFs ? 0 : 1}
              visibility={pageFs ? "hidden" : "visible"}
              transition="opacity 0.3s ease"
              pointerEvents={pageFs ? "none" : "auto"}
            >
              {gameModeVisible && <GameModeSwitch />}
            </Box>
            <IconButton
              icon={<LuMinus size={18} />}
              aria-label="最小化"
              variant="solid"
              borderRadius="full"
              bg={bgColor}
              backdropFilter="blur(10px)"
              color={iconColor}
              h="36px"
              minW="36px"
              w="36px"
              _hover={{
                color: hoverColor,
                bg: minimizeHoverBg,
              }}
              onClick={handleMinimize}
            />
            <IconButton
              icon={isMaximized ? <LuCopy size={15} /> : <LuSquare size={15} />}
              aria-label={isMaximized ? "还原" : "最大化"}
              variant="solid"
              borderRadius="full"
              bg={bgColor}
              backdropFilter="blur(10px)"
              color={iconColor}
              h="36px"
              minW="36px"
              w="36px"
              _hover={{
                color: hoverColor,
                bg: minimizeHoverBg,
              }}
              onClick={handleMaximize}
            />
            <IconButton
              icon={<LuX size={18} />}
              aria-label="关闭"
              variant="solid"
              borderRadius="full"
              bg={bgColor}
              backdropFilter="blur(10px)"
              color={iconColor}
              h="36px"
              minW="36px"
              w="36px"
              _hover={{
                color: closeHoverColor,
                bg: closeHoverBg,
              }}
              onClick={handleClose}
            />
          </HStack>
        </Flex>
      </Box>

      <CloseConfirmDialog
        isOpen={showCloseDialog}
        onClose={() => setShowCloseDialog(false)}
        onCloseApp={(savePreference) => performClose(savePreference)}
        onMinimizeToTray={(savePreference) => performMinimizeToTray(savePreference)}
      />
    </>
  );
}
