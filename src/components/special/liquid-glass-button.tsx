"use client";

import { Button} from "@chakra-ui/react";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";

interface LiquidGlassButtonProps {
  children: React.ReactNode;
  className?: string;
  [key: string]: unknown;
}

export function LiquidGlassButton({ children, className, ...props }: LiquidGlassButtonProps) {
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();


  const glassBg = getActiveColor();

  // 模糊立即生效
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;
  const backdropFilter = `blur(${effectiveBlur}px)`;

  return (
    <Button
      className={`jelly-bounce-button${className ? ` ${className}` : ""}`}
      bg={glassBg}
      color={getContrastTextColor()}
      border="1px solid"
      borderColor={liquidGlassEnabled ? getHoverColor() : glassBg}
      backdropFilter={backdropFilter}
      sx={{
        WebkitBackdropFilter: backdropFilter,
        transform: "translateZ(0)",
        WebkitTransform: "translateZ(0)",
        WebkitBackfaceVisibility: "hidden",
        backfaceVisibility: "hidden",
        willChange: "auto",
        transition: "border-color 0.45s cubic-bezier(0.4, 0, 0.2, 1), backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)",
      }}
      _hover={{
        bg: getHoverColor(),
      }}
      {...props}
    >
      {children}
    </Button>
  );
}
