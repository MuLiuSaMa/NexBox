import React, { useEffect, useState } from "react";
import { Text, HStack, VStack, useColorModeValue, Spinner } from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { FaWindows } from "react-icons/fa6";
import { invoke } from "@tauri-apps/api/core";

export default function GameWinKeyCard() {
  const { t } = useTranslation();
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);

  // react-icons 的 color 不经 Chakra 解析，必须用真实色值（token 在浅色下会退化成白色）
  const textColor = useColorModeValue("#1a202c", "#ffffff");

  useEffect(() => {
    let mounted = true;
    (async () => {
      try {
        const v = await invoke<boolean>("get_game_win_key_status");
        if (mounted) setEnabled(v);
      } catch {
        // ignore
      } finally {
        if (mounted) setLoading(false);
      }
    })();

    // 监听设置页开关变化，保持状态同步
    const handler = (e: CustomEvent) => {
      if (mounted) setEnabled(e.detail);
    };
    window.addEventListener("game-win-key-setting-changed", handler as EventListener);
    return () => {
      mounted = false;
      window.removeEventListener("game-win-key-setting-changed", handler as EventListener);
    };
  }, []);

  const handleToggle = async (val: boolean) => {
    setBusy(true);
    try {
      await invoke("set_game_win_key_enabled", { enabled: val });
      setEnabled(val);
      // 广播事件同步设置页开关
      window.dispatchEvent(new CustomEvent("game-win-key-setting-changed", { detail: val }));
    } catch {
      // ignore
    } finally {
      setBusy(false);
    }
  };

  return (
    <LiquidGlassCard px={3} py={2} boxShadow="sm" flex="1" minW="0" maxW="full" w="full">
      {loading ? (
        <HStack spacing={2} align="center" justify="center" minH="42px">
          <Spinner size="sm" />
        </HStack>
      ) : (
        <HStack spacing={2} align="center">
          <FaWindows size={18} color={textColor} style={{ flexShrink: 0 }} />
          <VStack spacing={0} align="start">
            <Text fontSize="sm" color={textColor} fontWeight="semibold" whiteSpace="nowrap">
              {t("home.gameWinKey.line1") || "游戏时"}
            </Text>
            <Text fontSize="sm" color={textColor} fontWeight="semibold" whiteSpace="nowrap">
              {t("home.gameWinKey.line2") || "禁用 Win 键"}
            </Text>
          </VStack>
          <ThemeSwitch
            size="sm"
            isChecked={enabled}
            onChange={(e) => handleToggle(e.target.checked)}
            isDisabled={busy}
          />
        </HStack>
      )}
    </LiquidGlassCard>
  );
}
