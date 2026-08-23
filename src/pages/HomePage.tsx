import { Box, Text, Flex, HStack, VStack } from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useTranslation } from "react-i18next";
import GameLauncher from "@/components/GameLauncher";
import { TodayPopularity, useTodayPopularityEnabled } from "@/components/TodayPopularity";
import { AnnouncementCard, useAnnouncementEnabled } from "@/components/AnnouncementCard";
import { RandomQuote, useRandomQuoteEnabled } from "@/components/RandomQuote";
import { useState, useEffect, useRef } from "react";
import HardwareModelCard from "@/components/HardwareModelCard";
import GameWinKeyCard from "@/components/GameWinKeyCard";
import GameImeLockCard from "@/components/GameImeLockCard";
import RandomImageCard, { useRandomImageEnabled } from "@/components/RandomImageCard";
import { FeedbackCard, QqGroupCard, useFeedbackEnabled, useQqGroupCardEnabled } from "@/components/QqFeedbackCards";
import { store } from "@/lib/store";
import { getGreeting, rollEasterEgg, EASTER_EGG_TEXT } from "@/lib/greetings";
import { invoke } from "@tauri-apps/api/core";

/**
 * 将问候语末尾的颜文字/表情块拆出来，用于整体换行（nowrap），
 * 避免颜文字被折行拆成上下两行。找不到颜文字起始括号时原样返回。
 */
function splitEmojiBlock(text: string): { main: string; emoji: string } {
  const startChars = ["(", "（", "[", "【", "｟", "〘", "「", "『", "［"];
  let pos = -1;
  for (const c of startChars) {
    const idx = text.lastIndexOf(c);
    if (idx > pos) pos = idx;
  }
  if (pos <= 0) return { main: text, emoji: "" };
  return { main: text.slice(0, pos).trimEnd(), emoji: text.slice(pos) };
}

export default function HomePage() {
  const { t } = useTranslation();
  const adaptiveTextColor = useAdaptiveTextColor();
  const [greetingText, setGreetingText] = useState("");
  const usernameRef = useRef("");
  const [gameLauncherEnabled, setGameLauncherEnabled] = useState(true);
  const [homeHardwareModelEnabled, setHomeHardwareModelEnabled] = useState(true);
  const [gameWinKeyCardEnabled, setGameWinKeyCardEnabled] = useState(true);
  const [gameImeLockCardEnabled, setGameImeLockCardEnabled] = useState(true);
  const [homeCardsReady, setHomeCardsReady] = useState(false);

  const computeGreeting = () => {
    if (rollEasterEgg()) return EASTER_EGG_TEXT;
    return getGreeting(new Date(), usernameRef.current).text;
  };

  // 四个本地主页开关全部加载完成后才渲染卡片区域，避免默认值导致的闪烁
  const homeCardsReadyRef = useRef(0);
  const markHomeCardLoaded = () => {
    homeCardsReadyRef.current += 1;
    if (homeCardsReadyRef.current >= 4) setHomeCardsReady(true);
  };

  // 获取用户名：优先使用自定义标题用户名，留空时回退到系统用户名
  useEffect(() => {
    (async () => {
      try {
        const custom = await store.get<string>("nexbox_home_username");
        if (custom && custom.trim()) {
          usernameRef.current = custom.trim();
        } else {
          const ls = localStorage.getItem("nexbox_home_username");
          if (ls && ls.trim()) {
            usernameRef.current = ls.trim();
          } else {
            usernameRef.current = await invoke<string>("get_system_username");
          }
        }
      } catch {
        usernameRef.current = "";
      }
      setGreetingText(computeGreeting());
    })();

    const handleUsernameChange = () => {
      (async () => {
        try {
          const custom = await store.get<string>("nexbox_home_username");
          if (custom && custom.trim()) {
            usernameRef.current = custom.trim();
          } else {
            const ls = localStorage.getItem("nexbox_home_username");
            usernameRef.current = ls && ls.trim() ? ls.trim() : await invoke<string>("get_system_username");
          }
        } catch {
          usernameRef.current = "";
        }
        setGreetingText(computeGreeting());
      })();
    };

    window.addEventListener("home-username-setting-changed", handleUsernameChange);
    return () => window.removeEventListener("home-username-setting-changed", handleUsernameChange);
  }, []);

  // 每分钟刷新一次，跨时段自动切换问候
  useEffect(() => {
    const timer = setInterval(() => setGreetingText(computeGreeting()), 60000);
    setGreetingText(computeGreeting());
    return () => clearInterval(timer);
  }, []);

  const {
    enabled: todayPopularityEnabled,
    ready: todayPopularityReady,
  } = useTodayPopularityEnabled();
  const { enabled: feedbackEnabled, ready: feedbackReady } = useFeedbackEnabled();
  const { enabled: qqGroupCardEnabled, ready: qqGroupCardReady } = useQqGroupCardEnabled();
  const { enabled: announcementEnabled, ready: announcementReady } = useAnnouncementEnabled();
  const { enabled: randomQuoteEnabled, ready: randomQuoteReady } = useRandomQuoteEnabled();
  const { enabled: randomImageEnabled, ready: randomImageReady } = useRandomImageEnabled();
  useEffect(() => {
    (async () => {
      const saved = await store.get<boolean>("nexbox_game_launcher_enabled");
      if (saved !== null && saved !== undefined) {
        setGameLauncherEnabled(saved);
      } else {
        // 兼容旧 localStorage
        const ls = localStorage.getItem("nexbox_game_launcher_enabled");
        if (ls !== null) {
          setGameLauncherEnabled(ls === "true");
        }
      }
      markHomeCardLoaded();
    })();

    const handleGameLauncherChange = (e: CustomEvent) => {
      setGameLauncherEnabled(e.detail);
    };

    window.addEventListener("game-launcher-setting-changed", handleGameLauncherChange as EventListener);
    
    return () => {
      window.removeEventListener("game-launcher-setting-changed", handleGameLauncherChange as EventListener);
    };
  }, []);

  useEffect(() => {
    (async () => {
      let saved = await store.get<boolean>("nexbox_home_hardware_model_enabled");
      if (saved !== null && saved !== undefined) {
        setHomeHardwareModelEnabled(saved);
      } else {
        const ls = localStorage.getItem("nexbox_home_hardware_model_enabled");
        if (ls !== null) setHomeHardwareModelEnabled(ls === "true");
      }
      markHomeCardLoaded();
    })();

    const handler = (e: CustomEvent) => {
      setHomeHardwareModelEnabled(e.detail);
    };

    window.addEventListener("home-hardware-model-setting-changed", handler as EventListener);
    return () => window.removeEventListener("home-hardware-model-setting-changed", handler as EventListener);
  }, []);

  useEffect(() => {
    (async () => {
      let saved = await store.get<boolean>("nexbox_game_win_key_card_enabled");
      if (saved !== null && saved !== undefined) {
        setGameWinKeyCardEnabled(saved);
      } else {
        const ls = localStorage.getItem("nexbox_game_win_key_card_enabled");
        if (ls !== null) setGameWinKeyCardEnabled(ls === "true");
      }
      markHomeCardLoaded();
    })();

    const handler = (e: CustomEvent) => {
      setGameWinKeyCardEnabled(e.detail);
    };

    window.addEventListener("game-win-key-card-setting-changed", handler as EventListener);
    return () => window.removeEventListener("game-win-key-card-setting-changed", handler as EventListener);
  }, []);

  useEffect(() => {
    (async () => {
      let saved = await store.get<boolean>("nexbox_game_ime_lock_card_enabled");
      if (saved !== null && saved !== undefined) {
        setGameImeLockCardEnabled(saved);
      } else {
        const ls = localStorage.getItem("nexbox_game_ime_lock_card_enabled");
        if (ls !== null) setGameImeLockCardEnabled(ls === "true");
      }
      markHomeCardLoaded();
    })();

    const handler = (e: CustomEvent) => {
      setGameImeLockCardEnabled(e.detail);
    };

    window.addEventListener("game-ime-lock-card-setting-changed", handler as EventListener);
    return () => window.removeEventListener("game-ime-lock-card-setting-changed", handler as EventListener);
  }, []);

  const greeting = greetingText || t("home.title");
  const { main: greetingMain, emoji: greetingEmoji } = splitEmojiBlock(greeting);

  return (
    <Box pt={8} pr={4} pb={4} pl={4} h="calc(100vh - 120px)" position="relative" overflowX="hidden">
      <Flex gap={6} h="100%" align="flex-start">
        <Box flex={1}>
          <Text fontSize="3xl" fontWeight="bold" color={adaptiveTextColor.text} textShadow={adaptiveTextColor.shadow} lineHeight="1.4">
            {greetingEmoji ? (
              <>
                {greetingMain}
                <span style={{ whiteSpace: "nowrap" }}>{greetingEmoji}</span>
              </>
            ) : (
              greeting
            )}
          </Text>
          {(todayPopularityReady && todayPopularityEnabled) ||
          (announcementReady && announcementEnabled) ||
          (randomQuoteReady && randomQuoteEnabled) ? (
            <HStack mt={3} spacing={3}>
              {todayPopularityReady && todayPopularityEnabled && <TodayPopularity />}
              {announcementReady && announcementEnabled && <AnnouncementCard />}
              {randomQuoteReady && randomQuoteEnabled && <RandomQuote />}
            </HStack>
          ) : null}
        </Box>
        {(feedbackReady && feedbackEnabled) || (qqGroupCardReady && qqGroupCardEnabled) ? (
          <Box pt={6}>
            <VStack spacing={2} align="stretch">
              {feedbackReady && feedbackEnabled && <FeedbackCard />}
              {qqGroupCardReady && qqGroupCardEnabled && <QqGroupCard />}
            </VStack>
          </Box>
        ) : null}
      </Flex>

      {homeCardsReady &&
        (randomImageEnabled ||
          gameWinKeyCardEnabled ||
          gameImeLockCardEnabled ||
          homeHardwareModelEnabled) && (
        <Box position="absolute" bottom={4} left={4}>
          <VStack spacing={2} align="stretch">
            {randomImageReady && randomImageEnabled && <RandomImageCard />}
            {(gameWinKeyCardEnabled || gameImeLockCardEnabled) && (
              <HStack spacing={2} align="stretch" w="full" justify="flex-start">
                {gameWinKeyCardEnabled && <GameWinKeyCard />}
                {gameImeLockCardEnabled && <GameImeLockCard />}
              </HStack>
            )}
            {homeHardwareModelEnabled && <HardwareModelCard />}
          </VStack>
        </Box>
        )}

      {homeCardsReady && gameLauncherEnabled && (
        <Box
          position="absolute"
          bottom={4}
          right={4}
        >
          <GameLauncher />
        </Box>
      )}
    </Box>
  );
}
