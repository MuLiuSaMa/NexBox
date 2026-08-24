import {
  Modal,
  ModalOverlay,
  ModalContent,
  Box,
  Flex,
  IconButton,
  Spinner,
} from "@chakra-ui/react";
import { keyframes } from "@emotion/react";
import { FaXmark } from "react-icons/fa6";
import { useEffect, useRef, useState } from "react";
import { SplashAd, openExternal, useAdImage } from "@/hooks/use-ads";
import { useThemeColor } from "@/contexts/theme-color-context";

const CAROUSEL_INTERVAL_MS = 5000;

// 轮播切换入场动画：淡入 + 左滑
const fadeSlide = keyframes`
  from { opacity: 0; transform: translateX(40px); }
  to { opacity: 1; transform: translateX(0); }
`;

/**
 * 开屏广告弹窗：启动完成展示一次。单图固定显示；多图每 5 秒轮播；图片铺满弹窗，点击跳转浏览器。
 * 右上角为圆形关闭按钮（适配主题色）。ads 为空时不渲染任何内容。
 */
export function StartupAdModal({ ads }: { ads: SplashAd[] }) {
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const [isOpen, setIsOpen] = useState(true);
  const [index, setIndex] = useState(0);
  const shownRef = useRef(false);

  // 单次会话只展示第一次挂载的结果：若组件在 ads 为空时挂载则不再补弹
  useEffect(() => {
    if (shownRef.current) return;
    if (ads.length > 0) {
      shownRef.current = true;
      setIsOpen(true);
    } else {
      setIsOpen(false);
    }
  }, [ads]);

  // 多图轮播
  useEffect(() => {
    if (!isOpen || ads.length <= 1) return;
    const timer = setInterval(() => {
      setIndex((i) => (i + 1) % ads.length);
    }, CAROUSEL_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [isOpen, ads.length]);

  const ad = ads[index];
  const { src, loading } = useAdImage(ad?.image);

  if (!isOpen || ads.length === 0) return null;

  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} isCentered size="lg" closeOnOverlayClick>
      <ModalOverlay backdropFilter="blur(6px)" />
      <ModalContent
        className="no-bounce"
        bg="transparent"
        boxShadow="none"
        border="none"
        borderRadius="2xl"
        overflow="hidden"
        position="relative"
        p={0}
      >
        {loading || !src ? (
          <Flex
            key={`splash-${index}`}
            w="100%"
            h="300px"
            align="center"
            justify="center"
            bg="blackAlpha.300"
            borderRadius="2xl"
            animation={`${fadeSlide} 0.5s ease`}
          >
            <Spinner size="xl" color={activeColor} />
          </Flex>
        ) : (
          <Box
            key={`splash-${index}`}
            as="img"
            src={src}
            alt="ad"
            w="auto"
            h="auto"
            maxW="90vw"
            maxH="80vh"
            objectFit="contain"
            draggable={false}
            borderRadius="2xl"
            cursor={ad.link ? "pointer" : "default"}
            onClick={() => ad.link && openExternal(ad.link)}
            animation={`${fadeSlide} 0.5s ease`}
          />
        )}

        {/* 圆形关闭按钮：透明无底、去聚焦描边，仅显示一个 X；hover 用主题色 */}
        <IconButton
          aria-label="close"
          icon={<FaXmark size={18} />}
          size="sm"
          variant="ghost"
          onClick={() => setIsOpen(false)}
          position="absolute"
          top={3}
          right={3}
          borderRadius="full"
          bg="transparent"
          color={activeColor}
          _hover={{ bg: "transparent", color: activeColor }}
          _focus={{ boxShadow: "none" }}
          _focusVisible={{ boxShadow: "none" }}
          boxShadow="none"
        />

        {ads.length > 1 && (
          <Flex justify="center" gap={1.5} position="absolute" bottom={3} left="50%" transform="translateX(-50%)">
            {ads.map((_, i) => (
              <Box
                key={i}
                w="7px"
                h="7px"
                borderRadius="full"
                bg={i === index ? activeColor : "rgba(255,255,255,0.6)"}
                transition="background 0.2s"
              />
            ))}
          </Flex>
        )}
      </ModalContent>
    </Modal>
  );
}