"use client";

import { useCallback } from "react";
import { useColorMode } from "@chakra-ui/react";

/**
 * `useColorModeValue` 的「非 hook 版」包装。
 *
 * `useColorModeValue` 本身是 hook，写在 JSX 属性或回调里会违反 React Hooks 的调用顺序约定
 * （eslint react-hooks/rules-of-hooks）。这里在组件顶部只调用一次 hook，
 * 返回的普通函数可以在 JSX、回调和条件分支里随意调用。
 *
 * 用法：
 * ```tsx
 * const cm = useColorValue();
 * ...
 * <Box bg={cm("gray.100", "#222222")} />
 * ```
 */
export function useColorValue() {
  const { colorMode } = useColorMode();
  return useCallback(
    <T,>(light: T, dark: T): T => (colorMode === "dark" ? dark : light),
    [colorMode]
  );
}
