"use client";

import { Box, BoxProps, useColorModeValue } from "@chakra-ui/react";
import { useBackground } from "@/contexts/background-context";
import { getBorderGlowStyle } from "@/hooks/use-glow-effect";
import { useLiquidGlassRefraction } from "@/components/special/liquid-glass-svg-filter";
import { useMemo } from "react";

interface LiquidGlassCardProps extends BoxProps {
  children: React.ReactNode;
  className?: string;
  isDashed?: boolean;
  /** 强制启用液态玻璃效果（忽略全局开关） */
  forceGlass?: boolean;
}

export function LiquidGlassCard({
  children,
  className,
  isDashed = false,
  forceGlass = false,
  ...props
}: LiquidGlassCardProps) {
  const { liquidGlassEnabled, liquidGlassBlur, liquidGlassMode } = useBackground();
  const glassOn = forceGlass || liquidGlassEnabled;
  const { svgSupported } = useLiquidGlassRefraction(glassOn && liquidGlassMode === "real");

  // 强制玻璃时使用明显的模糊值，避免全局默认 1px 看起来像没有效果
  const effectiveGlassBlur = forceGlass && !liquidGlassEnabled ? 16 : liquidGlassBlur;

  // 强制玻璃时背景稍微加一点不透明度，保证内容可读、不会透到看不清
  const glassBgColor = useColorModeValue(
    forceGlass ? "rgba(255,255,255,0.4)" : "rgba(255,255,255,0.25)",
    forceGlass ? "rgba(0,0,0,0.45)" : "rgba(0,0,0,0.25)"
  );
  const glassBorderColor = useColorModeValue(
    forceGlass ? "rgba(255,255,255,0.35)" : "rgba(255,255,255,0.2)",
    forceGlass ? "rgba(255,255,255,0.16)" : "rgba(255,255,255,0.1)"
  );
  const glowColor = useColorModeValue("rgba(255,255,255,0.8)", "rgba(255,255,255,0.5)");
  const defaultBg = useColorModeValue("white", "#111111");
  const defaultBorder = useColorModeValue("gray.200", "#333333");

  // 模糊立即生效
  const effectiveBlur = glassOn ? effectiveGlassBlur : 0;
  const isReal = glassOn && liquidGlassMode === "real" && !isDashed;

  // 卡片主体：零滤镜
  const backdropFilter = useMemo(() => {
    if (!glassOn) return "none";
    if (isReal) return "none";
    return `blur(${effectiveBlur}px) saturate(1.3)`;
  }, [glassOn, isReal, effectiveBlur]);

  // 边缘折射条用的 filter
  const edgeFilter = svgSupported ? "url(#nexbox-liquid-glass-filter)" : "none";

  const cardStyles = useMemo(() => {
    const base: any = {
      bg: isReal ? "transparent" : (glassOn ? glassBgColor : defaultBg),
      borderRadius: "xl" as const,
      border: isDashed ? "1px dashed" : "1px solid",
      borderColor: isReal ? "rgba(255,255,255,0.06)" : (glassOn ? glassBorderColor : defaultBorder),
      backdropFilter,
      WebkitBackdropFilter: backdropFilter,
      transition: "background 0.45s, border-color 0.45s, backdrop-filter 0.45s",
    };
    return base;
  }, [backdropFilter, glassOn, glassBgColor, glassBorderColor, defaultBg, defaultBorder, isDashed, isReal, forceGlass]);

  const borderGlowStyle = useMemo(() => getBorderGlowStyle(glowColor), [glowColor]);

  return (
    <Box
      className={`jelly-bounce-card${isReal ? " real-liquid-glass" : ""}${className ? ` ${className}` : ""}`}
      {...cardStyles}
      position="relative"
      overflow="visible"
      {...props}
    >
      {/* 边缘折射条 —— 独立叠加层，仅覆盖边缘 10px，卡片内部完全不受影响 */}
      {isReal && svgSupported && (
        <>
          <Box position="absolute" top={0} left={0} w="100%" h="10px" zIndex={0}
            backdropFilter={edgeFilter} sx={{ WebkitBackdropFilter: edgeFilter }}
            bg="transparent" pointerEvents="none" />
          <Box position="absolute" bottom={0} left={0} w="100%" h="10px" zIndex={0}
            backdropFilter={edgeFilter} sx={{ WebkitBackdropFilter: edgeFilter }}
            bg="transparent" pointerEvents="none" />
          <Box position="absolute" left={0} top={0} w="10px" h="100%" zIndex={0}
            backdropFilter={edgeFilter} sx={{ WebkitBackdropFilter: edgeFilter }}
            bg="transparent" pointerEvents="none" />
          <Box position="absolute" right={0} top={0} w="10px" h="100%" zIndex={0}
            backdropFilter={edgeFilter} sx={{ WebkitBackdropFilter: edgeFilter }}
            bg="transparent" pointerEvents="none" />
        </>
      )}

      {!isReal && (
        <Box
          style={borderGlowStyle}
          opacity={glassOn ? 1 : 0}
          transition="opacity 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
        />
      )}
      {children}
    </Box>
  );
}
