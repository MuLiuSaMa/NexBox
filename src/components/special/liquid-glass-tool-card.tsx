"use client";

import { Box, BoxProps, useColorModeValue } from "@chakra-ui/react";
import { useBackground } from "@/contexts/background-context";
import { useGlowEffect, getBorderGlowStyle } from "@/hooks/use-glow-effect";
import { useLiquidGlassRefraction } from "@/components/special/liquid-glass-svg-filter";

interface LiquidGlassToolCardProps extends BoxProps {
  children: React.ReactNode;
  onClick?: () => void;
  cursor?: string;
  isDashed?: boolean;
  className?: string;
  size?: "sm" | "md" | "lg";
}

export function LiquidGlassToolCard({
  children,
  onClick,
  cursor = "pointer",
  isDashed = false,
  className,
  size = "sm",
  ...props
}: LiquidGlassToolCardProps) {
  const { liquidGlassEnabled, liquidGlassBlur, liquidGlassMode } = useBackground();
  const { mouseX, mouseY, isHovering, handleMouseMove, handleMouseLeave, handleMouseEnter } = useGlowEffect();
  const { svgSupported } = useLiquidGlassRefraction(liquidGlassEnabled && liquidGlassMode === "real");

  const defaultBg = useColorModeValue("gray.50", "#111111");
  const defaultHoverBg = useColorModeValue("gray.100", "#222222");
  const defaultBorder = useColorModeValue("gray.200", "#333333");

  const glassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const glassHoverBg = useColorModeValue("rgba(255,255,255,0.35)", "rgba(0,0,0,0.35)");
  const glassBorder = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");
  const glowColor = useColorModeValue("rgba(255,255,255,0.8)", "rgba(255,255,255,0.5)");

  // 模糊立即生效
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;
  const padding = size === "sm" ? 3 : size === "md" ? 4 : 5;
  const borderRadius = size === "sm" ? "lg" : "xl";
  const isReal = liquidGlassEnabled && liquidGlassMode === "real" && !isDashed;

  const backdropFilter = isReal
    ? (svgSupported
        ? `url(#nexbox-liquid-glass-filter) saturate(1.4)`
        : `saturate(1.4) brightness(1.05)`)
    : `blur(${effectiveBlur}px)`;

  if (!liquidGlassEnabled) {
    return (
      <Box
        className={`jelly-bounce-tool-card${className ? ` ${className}` : ""}`}
        onClick={onClick}
        cursor={cursor}
        bg={defaultBg}
        borderRadius={borderRadius}
        p={padding}
        border={isDashed ? "1px dashed" : "1px solid"}
        borderColor={defaultBorder}
        _hover={{ bg: defaultHoverBg }}
        transition="all 0.2s"
        sx={{
          transform: "translateZ(0)",
          WebkitTransform: "translateZ(0)",
          WebkitBackfaceVisibility: "hidden",
          backfaceVisibility: "hidden",
          willChange: "auto",
        }}
        {...props}
      >
        {children}
      </Box>
    );
  }

  return (
    <Box
      className={`jelly-bounce-tool-card${isReal ? " real-liquid-glass" : ""}${className ? ` ${className}` : ""}`}
      onClick={onClick}
      cursor={cursor}
      bg={isHovering ? glassHoverBg : glassBg}
      borderRadius={borderRadius}
      p={padding}
      border={isDashed ? "1px dashed" : "1px solid"}
      borderColor={glassBorder}
      backdropFilter={backdropFilter}
      WebkitBackdropFilter={backdropFilter}
      transition="background 0.45s cubic-bezier(0.4, 0, 0.2, 1), border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)"
      position="relative"
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      onMouseEnter={handleMouseEnter}
      sx={{
        transform: "translateZ(0)",
        WebkitTransform: "translateZ(0)",
        WebkitBackfaceVisibility: "hidden",
        backfaceVisibility: "hidden",
        willChange: "auto",
      }}
      {...props}
    >
      {!isReal && <Box style={getBorderGlowStyle(glowColor)} />}
      {children}
    </Box>
  );
}
