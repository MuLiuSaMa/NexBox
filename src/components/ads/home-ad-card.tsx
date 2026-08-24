import { Box, Flex, Spinner, Text } from "@chakra-ui/react";
import { useEffect, useState } from "react";
import { useThemeColor } from "@/contexts/theme-color-context";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { HomeAd, openExternal, useAdImage } from "@/hooks/use-ads";

const CAROUSEL_INTERVAL_MS = 5000;

/** 单张主页广告卡片：图片铺满卡片，左下角悬浮叠加俱乐部名与介绍，点击跳转 */
function HomeAdCardItem({ ad }: { ad: HomeAd }) {
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const { src, loading } = useAdImage(ad.image);

  return (
    <Flex
      w="100%"
      h="150px"
      cursor={ad.link ? "pointer" : "default"}
      onClick={() => ad.link && openExternal(ad.link)}
      position="relative"
      overflow="hidden"
      border="none"
    >
      {loading || !src ? (
        <Flex w="100%" h="100%" align="center" justify="center" bg="blackAlpha.200">
          <Spinner size="md" color={activeColor} />
        </Flex>
      ) : (
        <Box
          as="img"
          src={src}
          alt={ad.name || "ad"}
          w="100%"
          h="100%"
          objectFit="cover"
          draggable={false}
        />
      )}

      {/* 左下角悬浮文字：底部渐变保证可读性 */}
      {(ad.name || ad.description) && (
        <Box
          position="absolute"
          left={0}
          right={0}
          bottom={0}
          px={2.5}
          py={2}
          bgGradient="linear(to-t, rgba(0,0,0,0.68), transparent)"
        >
          {ad.name && (
            <Text fontSize="sm" fontWeight="bold" color="white" noOfLines={1}>
              {ad.name}
            </Text>
          )}
          {ad.description && (
            <Text fontSize="xs" color="rgba(255,255,255,0.85)" noOfLines={2} mt={0.5}>
              {ad.description}
            </Text>
          )}
        </Box>
      )}
    </Flex>
  );
}

/**
 * 主页广告卡片：多条时不往下堆叠，改为轮播（每 5 秒切一张，底部指示点）。
 * 单条时仅显示一张。
 */
export function HomeAdCards({ ads }: { ads: HomeAd[] }) {
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const [index, setIndex] = useState(0);

  // 多图轮播
  useEffect(() => {
    if (ads.length <= 1) return;
    const timer = setInterval(() => {
      setIndex((i) => (i + 1) % ads.length);
    }, CAROUSEL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [ads.length]);

  if (ads.length === 0) return null;
  const ad = ads[index];

  return (
    <LiquidGlassCard
      className="no-bounce"
      p={0}
      w="260px"
      transition="border-color 0.2s"
      overflow="hidden"
      _hover={{ borderColor: ad.link ? activeColor : undefined }}
      position="relative"
    >
      {/* 滑动轨道：横向平滑平移实现轮播滚动动画 */}
      <Flex
        w={`${ads.length * 100}%`}
        transform={`translateX(-${(index * 100) / ads.length}%)`}
        transition="transform 0.7s cubic-bezier(0.25, 1, 0.5, 1)"
      >
        {ads.map((item, i) => (
          <Box key={i} w={`${100 / ads.length}%`} flexShrink={0}>
            <HomeAdCardItem ad={item} />
          </Box>
        ))}
      </Flex>
      {ads.length > 1 && (
        <Flex justify="center" gap={1} position="absolute" top={2} left="50%" transform="translateX(-50%)">
          {ads.map((_, i) => (
            <Box
              key={i}
              w="5px"
              h="5px"
              borderRadius="full"
              bg={i === index ? activeColor : "rgba(255,255,255,0.6)"}
              transition="background 0.2s"
            />
          ))}
        </Flex>
      )}
    </LiquidGlassCard>
  );
}