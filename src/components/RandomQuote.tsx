import { Box, Text, VStack, useColorModeValue } from "@chakra-ui/react";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { store } from "@/lib/store";

const quotes = [
  { text: "游戏不仅仅是游戏，游戏可以走进生活，改变生活", author: "刘旭东" },
  { text: "游戏可以重开，人生没有读档。", author: "木流" },
  { text: "真正的对手，从来都是自己。", author: "木流" },
  { text: "我希望你是在享受游戏。", author: "木流" },
  { text: "输赢是一瞬的结果，尽兴才是整场的意义。", author: "木流" },
  { text: "放下胜负心，游戏才真正属于自己。", author: "木流" },
  { text: "为了点游戏币，人都不做了。", author: "老飞宇66" },
  { text: "真正的强者不一定是赢家。", author: "深蓝" },
  { text: "当你觉得精疲力尽的时候，就是突破自己的最佳时机。", author: "老黑" },
  { text: "游戏怎么了，游戏提供了千千万万就业岗位。", author: "木流" },
  { text: "不是游戏害人，是不懂节制的心态害人。", author: "木流" },
  { text: "游戏非洪水猛兽，害人的是放纵，不是游戏本身。", author: "木流" },
  { text: "我们是在玩游戏，不是游戏玩我们。", author: "木流" },
  { text: "要懂得分清游戏和现实。", author: "木流" },
  { text: "游戏里的等级会清零，但快乐和经验不会。", author: "匿名" },
  { text: "一局游戏的胜负，改变不了热爱游戏的初心。", author: "匿名" },
  { text: "队友会掉线，但对胜利的渴望不会。", author: "匿名" },
  { text: "技术可以练，心态才是真正的上分密码。", author: "匿名" },
  { text: "游戏的意义，是遇见同样热爱的人。", author: "匿名" },
  { text: "稳住，我们能赢。", author: "匿名" },
  { text: "每一把都是新的开局，别让上一把影响你。", author: "匿名" },
  { text: "操作可以糙，意识不能少。", author: "匿名" },
  { text: "团战可以输，气势不能丢。", author: "匿名" },
  { text: "真正的老玩家，不是赢了才玩，而是玩了就开心。", author: "匿名" },
  { text: "游戏没有终点，乐趣才是最好的奖励。", author: "匿名" },
  { text: "手速不够，脑子来凑。", author: "匿名" },
  { text: "匹配到的队友，都是命中注定的缘分。", author: "匿名" },
  { text: "carry 是本事，躺赢也是实力的一种。", author: "匿名" },
  { text: "没有垃圾游戏，只有不会玩的心态。", author: "匿名" },
  { text: "连跪不可怕，可怕的是把心态也连输了。", author: "匿名" },
  { text: "一币通关的快乐，胜过任何充值。", author: "匿名" },
  { text: "存档可以重读，但此刻的快乐只有一次。", author: "匿名" },
  { text: "升不完的等级，永远热血的心。", author: "匿名" },
  { text: "赢了说句辛苦了，输了说句再来。", author: "匿名" },
  { text: "游戏是避风港，不是逃避现实的借口。", author: "匿名" },
  { text: "屏幕前认真的你，就是最强玩家。", author: "匿名" },
  { text: "从菜鸟到大神，只差一次次不服输。", author: "匿名" },
  { text: "先手必胜，后手吃土，运气也是实力。", author: "匿名" },
  { text: "残血反杀的那一刻，心跳比夺冠还快。", author: "匿名" },
  { text: "键盘声起，江湖再见。", author: "匿名" },
  { text: "装备可以换，手感不能丢。", author: "匿名" },
  { text: "一分钟就能秒杀，一辈子却打不过自己。", author: "匿名" },
  { text: "游戏教会我们：失误不可怕，可怕的是不再尝试。", author: "匿名" },
  { text: "副本再难，也总有通关的那一天。", author: "匿名" },
  { text: "每一帧都是热爱，每一局都是青春。", author: "匿名" },
  { text: "队友的鼓励，是比 buff 还强力的加成。", author: "匿名" },
  { text: "掉线重连，回来还能翻盘。", author: "匿名" },
  { text: "游戏打得好不好不重要，开心最重要。", author: "匿名" },
  { text: "像素世界里的友情，比钻石还珍贵。", author: "匿名" },
  { text: "复活币有限，快乐无限。", author: "匿名" },
  { text: "装备会过时，热爱永不过时。", author: "匿名" },
  { text: "打完这把就睡，然后就是下一把。", author: "匿名" },
  { text: "上分路上最大的对手，是那个想投降的自己。", author: "匿名" },
  { text: "游戏里没有终点线，只有新的起跑线。", author: "匿名" },
  { text: "猥琐发育别浪，稳健才能笑到最后。", author: "匿名" },
  { text: "一个游戏，一群人，一段回不去的时光。", author: "匿名" },
  { text: "操作或许生疏了，但热血从未冷却。", author: "匿名" },
  { text: "无论玩什么游戏，快乐永远第一位。", author: "匿名" },
];

function getRandomQuote() {
  const index = Math.floor(Math.random() * quotes.length);
  return quotes[index];
}

export function useRandomQuoteEnabled() {
  const [state, setState] = useState({ enabled: false, ready: false });

  useEffect(() => {
    (async () => {
      let enabled = true;
      const saved = await store.get<boolean>("nexbox_random_quote_enabled");
      if (saved !== null && saved !== undefined) {
        enabled = saved;
      } else {
        const ls = localStorage.getItem("nexbox_random_quote_enabled");
        if (ls !== null) enabled = ls === "true";
      }
      setState((s) => ({ ...s, enabled, ready: true }));
    })();
  }, []);

  useEffect(() => {
    const handler = (e: CustomEvent) => setState((s) => ({ ...s, enabled: e.detail }));
    window.addEventListener("random-quote-setting-changed", handler as EventListener);
    return () => window.removeEventListener("random-quote-setting-changed", handler as EventListener);
  }, []);

  return state;
}

export function RandomQuote() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();

  const [quote, setQuote] = useState(() => getRandomQuote());

  const labelColor = useColorModeValue("gray.500", "#ffffff");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const authorColor = useColorModeValue("gray.400", "#888888");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const hoverBorderColor = useColorModeValue("purple.500", "#b794f4");

  const handleRefresh = () => {
    setQuote(getRandomQuote());
  };

  const cardContent = (
    <VStack spacing={0.5} align="flex-start" minW="90px">
      <Text fontSize="2xs" color={labelColor}>
        {t("home.randomQuote")}
      </Text>
      <Box cursor="pointer" onClick={handleRefresh} userSelect="none">
        <Text fontSize="sm" color={textColor} fontWeight="medium" whiteSpace="nowrap">
          {quote.text}
        </Text>
        <Text fontSize="2xs" color={authorColor} textAlign="right">
          -{quote.author}
        </Text>
      </Box>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard py={2} px={3} minW="90px">
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
      minW="90px"
      cursor="pointer"
      _hover={{ borderColor: hoverBorderColor }}
      transition="border-color 0.2s"
    >
      {cardContent}
    </Box>
  );
}
