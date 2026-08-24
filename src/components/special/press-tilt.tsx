"use client";

import { Box, BoxProps } from "@chakra-ui/react";
import { useCallback, useRef, useState } from "react";

interface PressTiltProps extends BoxProps {
  /** 下沉倾斜最大角度（deg），默认 9 */
  maxTilt?: number;
  children: React.ReactNode;
}

interface Press {
  nx: number; // -0.5..0.5，右侧为正
  ny: number; // -0.5..0.5，下方为正
}

/**
 * 「3D 局部按压下沉（跟随光标）」：按下并按住时，光标落到哪里，
 * 哪里就作为下沉支点在空间里下陷，整张卡片随之 3D 倾斜；按住期间移动
 * 光标会实时重算倾斜，让下沉点始终跟随光标；松开后弹性回弹复原。
 */
export function PressTilt({
  maxTilt = 9,
  children,
  ...boxProps
}: PressTiltProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [press, setPress] = useState<Press | null>(null);

  const computePress = useCallback((clientX: number, clientY: number): Press | null => {
    const el = containerRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    return {
      nx: (clientX - rect.left) / rect.width - 0.5,
      ny: (clientY - rect.top) / rect.height - 0.5,
    };
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      const next = computePress(e.clientX, e.clientY);
      if (!next) return;
      // 捕获指针，离开卡片后仍持续追踪移动，实现「按住滑动实时下沉」
      e.currentTarget.setPointerCapture?.(e.pointerId);
      setPress(next);
    },
    [computePress],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!press) return;
      const next = computePress(e.clientX, e.clientY);
      if (next) setPress(next);
    },
    [press, computePress],
  );

  const release = useCallback(() => setPress(null), []);

  const tiltX = press ? -press.ny * maxTilt : 0; // 下沉点在上半区 → rotateX 让上侧后仰（按哪边哪边沉）
  const tiltY = press ? press.nx * maxTilt : 0; // 下沉点在左半区 → rotateY 让左侧后仰（按哪边哪边沉）

  return (
    <Box
      ref={containerRef}
      position="relative"
      userSelect="none"
      touchAction="none"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={release}
      onPointerLeave={release}
      onPointerCancel={release}
      transform={press
        ? `perspective(760px) rotateX(${tiltX}deg) rotateY(${tiltY}deg) scale(0.975)`
        : "none"}
      transformStyle="preserve-3d"
      css={{
        // 按住时尽量跟手（短过渡），松开时弹性回弹
        transition: press
          ? "transform 0.05s linear"
          : "transform 0.42s cubic-bezier(0.34, 1.4, 0.64, 1)",
      }}
      {...boxProps}
    >
      {children}
    </Box>
  );
}