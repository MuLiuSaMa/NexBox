import { Box, Text, VStack, useColorModeValue } from "@chakra-ui/react";
import { keyframes } from "@emotion/react";
import { useState, useEffect, useCallback, useRef } from "react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { store } from "@/lib/store";

const bounceKeyframes = keyframes`
  0% { transform: scale(1); }
  30% { transform: scale(1.3); }
  100% { transform: scale(1); }
`;

function getTodayKey(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function useTodayPopularityEnabled() {
  const [state, setState] = useState({ enabled: false, ready: false });

  useEffect(() => {
    (async () => {
      let enabled = true;
      const saved = await store.get<boolean>("nexbox_today_popularity_enabled");
      if (saved !== null && saved !== undefined) {
        enabled = saved;
      } else {
        // 兼容旧 localStorage
        const ls = localStorage.getItem("nexbox_today_popularity_enabled");
        if (ls !== null) enabled = ls === "true";
      }
      setState((s) => ({ ...s, enabled, ready: true }));
    })();
  }, []);

  useEffect(() => {
    const handler = (e: CustomEvent) => setState((s) => ({ ...s, enabled: e.detail }));
    window.addEventListener("today-popularity-setting-changed", handler as EventListener);
    return () => window.removeEventListener("today-popularity-setting-changed", handler as EventListener);
  }, []);

  return state;
}

export function TodayPopularity() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();

  const [value, setValue] = useState<number | null>(null);
  const [animating, setAnimating] = useState(false);
  // 标记今天是否已生成过，防止同一天内重复点击
  const generatedRef = useRef(false);

  const valueColor = useColorModeValue("purple.500", "#b794f4");
  const labelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");

  useEffect(() => {
    (async () => {
      const todayKey = getTodayKey();
      // 优先从 store 读取，兼容旧 localStorage
      let savedDate = await store.get<string>("nexbox_today_popularity_date");
      let savedValue = await store.get<string>("nexbox_today_popularity_value");
      if (!savedDate || !savedValue) {
        savedDate = localStorage.getItem("nexbox_today_popularity_date");
        savedValue = localStorage.getItem("nexbox_today_popularity_value");
      }

      if (savedDate === todayKey && savedValue !== null) {
        setValue(Number(savedValue));
        generatedRef.current = true;
      }
    })();
  }, []);

  const generate = useCallback(async () => {
    // 同一天内只能生成一次
    if (generatedRef.current) return;
    const todayKey = getTodayKey();

    // 同步检查 localStorage，避免页面刷新后 value 为 null 时重复生成
    const lsDate = localStorage.getItem("nexbox_today_popularity_date");
    if (lsDate === todayKey) {
      generatedRef.current = true;
      return;
    }

    // 异步检查 store 兜底（跨存储一致性）
    const savedDate = await store.get<string>("nexbox_today_popularity_date");
    if (savedDate === todayKey) {
      generatedRef.current = true;
      const savedValue = await store.get<string>("nexbox_today_popularity_value");
      if (savedValue !== null) setValue(Number(savedValue));
      return;
    }

    generatedRef.current = true;
    const randomValue = Math.floor(Math.random() * 101);
    setValue(randomValue);
    setAnimating(true);
    localStorage.setItem("nexbox_today_popularity_date", todayKey);
    localStorage.setItem("nexbox_today_popularity_value", String(randomValue));
    store.set("nexbox_today_popularity_date", todayKey).then(() => store.save());
    store.set("nexbox_today_popularity_value", String(randomValue)).then(() => store.save());
    setTimeout(() => setAnimating(false), 600);
  }, []);

  const cardContent = (
    <VStack spacing={0} align="center">
      <Text fontSize="2xs" color={labelColor}>
        {t("home.todayPopularity")}
      </Text>
      <Box
        cursor="pointer"
        onClick={generate}
        userSelect="none"
        animation={animating ? `${bounceKeyframes} 0.6s ease` : undefined}
      >
        <Text
          fontSize="2xl"
          fontWeight="bold"
          color={value !== null ? valueColor : labelColor}
          transition="color 0.3s"
        >
          {value !== null ? value : "?"}
        </Text>
      </Box>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard py={2} px={3} w="90px">
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      borderRadius="3xl"
      border="1px solid"
      borderColor={borderColor}
      py={2}
      px={3}
      w="90px"
    >
      {cardContent}
    </Box>
  );
}