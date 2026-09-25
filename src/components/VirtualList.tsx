/**
 * 虚拟滚动列表 — 真·窗口化（windowing）
 *
 * 无论列表多大，DOM 中始终只保留「可视区 + overscan」约 15~20 个节点，
 * 滚动到底也不会累积节点。适用于播放队列、歌单曲目这类可能高达数千项
 * 的超大列表，彻底解决「歌太多就卡」。
 *
 * 要求每一项高度固定（itemHeight），以便通过 scrollTop 直接换算区间。
 *
 * 两种高度模式：
 * - 传入 height：固定像素高度（如 Popover/Menu 里的播放队列）
 * - 省略 height：自适应父容器（父级需为 flex 容器，本组件 flex=1 填充）
 *
 * 其它能力：
 * - scrollToIndex：挂载时把指定项滚动到视口中间（定位当前播放曲目）
 * - onEndReached / hasMore：滚动接近底部时回调，用于从后端分页加载更多
 * - loading / emptyText / resetKey：整体加载态、空态、切换列表时复位
 */

import { useRef, useState, useLayoutEffect, useEffect, useCallback, memo, type ReactNode } from "react";
import { Box, VStack, Spinner, Text } from "@chakra-ui/react";

interface VirtualListProps<T> {
  items: T[];
  /** 每一项的固定高度（px） */
  itemHeight: number;
  /** 固定高度（px）；省略则自适应父容器（父级需为 flex 容器） */
  height?: number;
  renderItem: (item: T, index: number) => ReactNode;
  /** 视口上下额外多渲染的项数，缓冲快速滚动 */
  overscan?: number;
  /** 挂载时自动滚动到该项（居中显示），用于定位当前播放曲目 */
  scrollToIndex?: number;
  /** 自定义 React key，默认使用绝对索引 */
  getKey?: (item: T, index: number) => string | number;
  /** 自定义滚动条样式 */
  scrollbarSx?: Record<string, unknown>;
  /** 变化时滚回顶部并复位触底状态（切换列表用） */
  resetKey?: string | number;
  /** 整体加载中（首次加载），显示居中 spinner */
  loading?: boolean;
  loadingText?: string;
  emptyText?: string;
  /** 滚动接近底部时触发（从后端加载更多） */
  onEndReached?: () => void;
  /** 后端是否还有更多数据 */
  hasMore?: boolean;
  /** 距底部多少 px 触发 onEndReached */
  endReachedThreshold?: number;
}

const FOOTER_HEIGHT = 44;

function VirtualListInner<T>({
  items,
  itemHeight,
  height,
  renderItem,
  overscan = 6,
  scrollToIndex,
  getKey,
  scrollbarSx,
  resetKey,
  loading = false,
  loadingText = "加载中...",
  emptyText = "暂无内容",
  onEndReached,
  hasMore = false,
  endReachedThreshold = 400,
}: VirtualListProps<T>) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [measuredHeight, setMeasuredHeight] = useState(height ?? 0);
  const lastLoadLenRef = useRef(0);
  const prevResetKeyRef = useRef(resetKey);
  const rafIdRef = useRef<number | null>(null);

  const total = items.length;
  const totalHeight = total * itemHeight;
  const viewportHeight = height ?? measuredHeight;

  // 自适应高度：测量滚动容器实际高度，并随窗口变化实时更新
  useLayoutEffect(() => {
    if (height != null) return;
    const el = scrollRef.current;
    if (!el) return;
    setMeasuredHeight(el.clientHeight);
    const ro = new ResizeObserver((entries) => {
      const h = entries[0]?.contentRect.height;
      if (h != null) setMeasuredHeight(h);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [height]);

  // resetKey 变化：滚回顶部并复位分页触底标记
  // 用「上一次值」判断，兼容 StrictMode 双调用，且不影响 scrollToIndex 定位
  useEffect(() => {
    if (prevResetKeyRef.current === resetKey) return;
    prevResetKeyRef.current = resetKey;
    lastLoadLenRef.current = 0;
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
    setScrollTop(0);
  }, [resetKey]);

  // 挂载时定位到指定项（居中），用于播放队列跳到当前曲目
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || scrollToIndex == null || scrollToIndex < 0 || viewportHeight === 0) return;
    const centered = scrollToIndex * itemHeight - viewportHeight / 2 + itemHeight / 2;
    const target = Math.max(0, Math.min(centered, Math.max(0, totalHeight - viewportHeight)));
    el.scrollTop = target;
    setScrollTop(target);
    // 仅挂载时定位一次，避免打断用户手动滚动
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 滚动处理：rAF 节流 + 仅在可见区间跨越项边界时才重渲染
  // 播放时主线程更繁忙，减少无效重渲染是滚动保持流畅的关键
  const handleScroll = useCallback(() => {
    if (rafIdRef.current != null) return;
    rafIdRef.current = requestAnimationFrame(() => {
      rafIdRef.current = null;
      const el = scrollRef.current;
      if (!el) return;
      const st = el.scrollTop;
      // 顶部项未跨越边界（仍在同一项内滚动）则跳过 setState，靠 overscan 吸收误差
      setScrollTop((prev) =>
        Math.floor(prev / itemHeight) === Math.floor(st / itemHeight) ? prev : st
      );
      if (
        onEndReached &&
        hasMore &&
        !loading &&
        st + viewportHeight >= totalHeight - endReachedThreshold &&
        lastLoadLenRef.current !== total
      ) {
        lastLoadLenRef.current = total;
        onEndReached();
      }
    });
  }, [itemHeight, onEndReached, hasMore, loading, viewportHeight, totalHeight, endReachedThreshold, total]);

  // 卸载时取消挂起的 rAF
  useEffect(() => () => {
    if (rafIdRef.current != null) cancelAnimationFrame(rafIdRef.current);
  }, []);

  const startIndex = Math.max(0, Math.floor(scrollTop / itemHeight) - overscan);
  const endIndex = Math.min(total, Math.ceil((scrollTop + viewportHeight) / itemHeight) + overscan);
  const visible = viewportHeight > 0 ? items.slice(startIndex, endIndex) : [];

  const footerHeight = hasMore ? FOOTER_HEIGHT : 0;
  const sizeProps = height != null ? { height: `${height}px` } : { flex: 1, minHeight: 0 };

  return (
    <Box
      ref={scrollRef}
      overflowY="auto"
      overflowX="hidden"
      position="relative"
      onScroll={handleScroll}
      sx={{
        ...scrollbarSx,
        // 列表外层是带 backdrop-filter 的液态玻璃卡片：不提升成独立合成层的话，
        // 每次滚动重绘都会连带重跑背景模糊，表现为滚动掉帧。
        // contain 把布局/绘制影响范围限制在滚动容器内，避免冒泡到玻璃层。
        willChange: "transform",
        contain: "layout paint",
        WebkitOverflowScrolling: "touch",
      }}
      {...sizeProps}
    >
      {loading ? (
        <VStack h="100%" justify="center" py={6} spacing={2}>
          <Spinner size="sm" />
          <Text fontSize="xs" color="gray.500">{loadingText}</Text>
        </VStack>
      ) : total === 0 ? (
        <Text fontSize="xs" color="gray.500" py={4} textAlign="center">{emptyText}</Text>
      ) : (
        // 撑高占位容器，保证滚动条比例正确
        <Box height={`${totalHeight + footerHeight}px`} position="relative">
          {visible.map((item, i) => {
            const index = startIndex + i;
            return (
              <Box
                key={getKey ? getKey(item, index) : index}
                position="absolute"
                top={`${index * itemHeight}px`}
                left={0}
                right={0}
                height={`${itemHeight}px`}
              >
                {renderItem(item, index)}
              </Box>
            );
          })}
          {hasMore && (
            <Box
              position="absolute"
              top={`${totalHeight}px`}
              left={0}
              right={0}
              height={`${FOOTER_HEIGHT}px`}
              display="flex"
              alignItems="center"
              justifyContent="center"
              gap={2}
            >
              <Spinner size="xs" />
              <Text fontSize="xs" color="gray.500">加载更多...</Text>
            </Box>
          )}
        </Box>
      )}
    </Box>
  );
}

export const VirtualList = memo(VirtualListInner) as <T>(
  props: VirtualListProps<T>
) => React.JSX.Element;
