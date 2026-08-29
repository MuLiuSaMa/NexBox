import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Box, useColorMode, useColorModeValue } from "@chakra-ui/react";
import { Monitor, MemoryStick, RefreshCw, LogOut } from "lucide-react";
import { motion } from "framer-motion";

interface MenuItemProps {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  color?: string;
}

function MenuItem({ icon, label, onClick, color }: MenuItemProps) {
  const hoverBg = useColorModeValue("rgba(0,0,0,0.06)", "rgba(255,255,255,0.08)");
  const activeBg = useColorModeValue("rgba(0,0,0,0.10)", "rgba(255,255,255,0.12)");
  const labelColor = color ?? useColorModeValue("#1a1a1a", "#ffffff");

  return (
    <Box
      as="button"
      w="full"
      display="flex"
      alignItems="center"
      gap={2}
      px={2.5}
      h="44px"
      border="none"
      bg="transparent"
      color={labelColor}
      fontSize="13px"
      cursor="pointer"
      transition="background 0.15s ease"
      _hover={{ bg: hoverBg }}
      _active={{ bg: activeBg }}
      onClick={onClick}
    >
      <Box display="flex" alignItems="center" justifyContent="center" w="18px">
        {icon}
      </Box>
      <span>{label}</span>
    </Box>
  );
}

interface CleanupResult {
  success: boolean;
  message: string;
  freed_mb: number;
}

export default function TrayMenuPage() {
  const { colorMode, setColorMode } = useColorMode();
  const bg = useColorModeValue("#ffffff", "#1a1a1a");
  const borderColor = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)");
  const dividerColor = useColorModeValue("rgba(0,0,0,0.06)", "rgba(255,255,255,0.06)");
  // 清理内存状态：菜单项原地显示进度与结果，完成后短暂停留再隐藏菜单
  const [cleanLabel, setCleanLabel] = useState<string | null>(null);

  // 与主窗口同步深浅主题：主窗口切换主题会写入 localStorage(chakra-ui-color-mode)，
  // 本窗口监听 storage 事件实时跟随，保证托盘菜单每次打开都是当前主题
  useEffect(() => {
    const handler = (e: StorageEvent) => {
      if (e.key === "chakra-ui-color-mode" && (e.newValue === "light" || e.newValue === "dark")) {
        if (e.newValue !== colorMode) setColorMode(e.newValue);
      }
    };
    window.addEventListener("storage", handler);
    return () => window.removeEventListener("storage", handler);
  }, [colorMode, setColorMode]);

  // 确保窗口背景完全透明（该窗口设置了 transparent: true）
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

  useEffect(() => {
    const unlisten = getCurrentWindow().onFocusChanged((event) => {
      if (event.payload) {
        // 每次重新打开菜单时复位清理状态，避免残留上次的“清理中/已释放”文案
        setCleanLabel(null);
      } else {
        getCurrentWindow().hide();
      }
    });

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      unlisten.then((fn) => fn());
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  const handleShowWindow = async () => {
    // 复用后端 show_window 统一打开主窗口（恢复任务栏、若处于离屏预热则归位到屏幕内）
    await invoke("show_window");
    await getCurrentWindow().hide();
  };

  const handleCleanMemory = async () => {
    if (cleanLabel !== null) return; // 清理进行中或结果展示期间不再触发
    setCleanLabel("清理中…");
    try {
      // 与内存清理页“一键清理”一致：并行执行待机缓存清理 + 工作集收紧
      const [r1, r2] = await Promise.all([
        invoke<CleanupResult>("clean_standby_memory"),
        invoke<CleanupResult>("trim_system_working_set"),
      ]);
      const totalFreed = r1.freed_mb + r2.freed_mb;
      setCleanLabel(totalFreed > 0 ? `已释放 ${totalFreed} MB` : "内存已清理");
    } catch {
      setCleanLabel("清理失败");
    }
    // 结果短暂停留后自动收起菜单
    setTimeout(() => {
      getCurrentWindow().hide();
    }, 1200);
  };

  const handleCheckUpdate = () => {
    invoke("check_update_and_show");
    getCurrentWindow().hide();
  };

  const handleExit = () => {
    invoke("exit_app");
  };

  return (
    <Box w="100vw" h="100vh" p={0} m={0} bg="transparent">
      <motion.div
        initial={{ opacity: 0, scale: 0.95, y: -4 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: 0.12, ease: "easeOut" }}
        style={{ height: "100%" }}
      >
        <Box
          w="full"
          h="full"
          bg={bg}
          border="1px solid"
          borderColor={borderColor}
          borderRadius="12px"
          overflow="hidden"
        >
          <MenuItem
            icon={<Monitor size={16} strokeWidth={2} />}
            label="打开主窗口"
            onClick={handleShowWindow}
          />
          <Box h="1px" bg={dividerColor} mx={3} />
          <MenuItem
            icon={<MemoryStick size={16} strokeWidth={2} />}
            label={cleanLabel ?? "清理内存"}
            onClick={handleCleanMemory}
          />
          <Box h="1px" bg={dividerColor} mx={3} />
          <MenuItem
            icon={<RefreshCw size={16} strokeWidth={2} />}
            label="检查更新"
            onClick={handleCheckUpdate}
          />
          <Box h="1px" bg={dividerColor} mx={3} />
          <MenuItem
            icon={<LogOut size={16} strokeWidth={2} />}
            label="退出"
            onClick={handleExit}
            color="#e74c3c"
          />
        </Box>
      </motion.div>
    </Box>
  );
}
