import React, { useEffect, useState } from "react";
import { Box, Text, HStack, VStack, useColorModeValue, useDisclosure } from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import { FaImage } from "react-icons/fa6";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { store } from "@/lib/store";
import RandomImageModal from "@/components/RandomImageModal";

/** 主页「随机图片」卡片显示开关（store 持久化，默认开启） */
export function useRandomImageEnabled() {
  const [state, setState] = useState({ enabled: false, ready: false });

  useEffect(() => {
    (async () => {
      let enabled = true;
      const saved = await store.get<boolean>("nexbox_random_image_enabled");
      if (saved !== null && saved !== undefined) {
        enabled = saved;
      } else {
        const ls = localStorage.getItem("nexbox_random_image_enabled");
        if (ls !== null) enabled = ls === "true";
      }
      setState((s) => ({ ...s, enabled, ready: true }));
    })();

    const handler = (e: CustomEvent) => setState((s) => ({ ...s, enabled: e.detail }));
    window.addEventListener("random-image-setting-changed", handler as EventListener);
    return () => window.removeEventListener("random-image-setting-changed", handler as EventListener);
  }, []);

  return state;
}

/** 主页「随机图片」卡片：与 Win 键卡片同尺寸，点击打开弹窗选择类别并生成图片 */
export default function RandomImageCard() {
  const { t } = useTranslation();
  const { isOpen, onOpen, onClose } = useDisclosure();

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.600", "#ffffff");

  return (
    <>
      <LiquidGlassCard px={3} py={2} boxShadow="sm" flex="1" minW="0" maxW="full" w="full" cursor="pointer" onClick={onOpen}>
        <HStack spacing={2} align="center">
          <FaImage size={18} color={textColor} style={{ flexShrink: 0 }} />
          <VStack spacing={0} align="start">
            <Text fontSize="sm" color={textColor} fontWeight="semibold" whiteSpace="nowrap">
              {t("home.randomImage.title") || "随机图片"}
            </Text>
            <Text fontSize="xs" color={subTextColor} noOfLines={1}>
              {t("home.randomImage.subtitle") || "点击打开随机图片"}
            </Text>
          </VStack>
        </HStack>
      </LiquidGlassCard>
      <RandomImageModal isOpen={isOpen} onClose={onClose} />
    </>
  );
}
