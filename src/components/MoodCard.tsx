import React from "react";
import { Text, HStack, VStack, useColorModeValue } from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { FaHeart } from "react-icons/fa6";
import { invoke } from "@tauri-apps/api/core";

/** 主页「心境」卡片：图标 + 标题 + 介绍，点击打开独立心境窗口（非浏览器） */
export default function MoodCard() {
  const { t } = useTranslation();
  // react-icons 的 color 不经 Chakra 解析，必须用真实色值（token 在浅色下会退化成白色）
  const textColor = useColorModeValue("#1a202c", "#ffffff");
  const subTextColor = useColorModeValue("gray.600", "#ffffff");

  const handleOpen = async () => {
    try {
      await invoke("open_mood_window");
    } catch (e) {
      console.error("[MoodCard] open_mood_window failed:", e);
    }
  };

  return (
    <LiquidGlassCard px={3} py={2} boxShadow="sm" flex="1" minW="0" maxW="full" w="full" cursor="pointer" onClick={handleOpen}>
      <HStack spacing={2} align="center">
        <FaHeart size={18} color={textColor} style={{ flexShrink: 0 }} />
        <VStack spacing={0} align="start">
          <Text fontSize="sm" color={textColor} fontWeight="semibold" whiteSpace="nowrap">
            {t("home.mood.title") || "心境"}
          </Text>
          <Text fontSize="xs" color={subTextColor} noOfLines={1}>
            {t("home.mood.subtitle") || "点击打开心境"}
          </Text>
        </VStack>
      </HStack>
    </LiquidGlassCard>
  );
}
