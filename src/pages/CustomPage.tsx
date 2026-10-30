import {
  Box,
  Flex,
  Text,
  Icon,
  IconButton,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  Input,
  InputGroup,
  InputLeftElement,
  SimpleGrid,
  useColorModeValue,
  useDisclosure,
  Tooltip,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  Popover,
  PopoverTrigger,
  PopoverContent,
  PopoverBody,
  PopoverArrow,
  VStack,
  HStack,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LayoutGrid, Plus, Search, Trash2, Settings as SettingsIcon } from "lucide-react";
import { useState, useCallback, useRef, useMemo, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useBackground } from "@/contexts/background-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { searchIndex, type SearchItem } from "@/config/search-index";
import { store } from "@/lib/store";
import type { CustomCardInstance, CustomDashboardConfig } from "@/types/custom-dashboard";

const STORE_CONFIG_KEY = "nexbox_custom_dashboard_config";
const LS_CONFIG_KEY = "nexbox_custom_dashboard_config";

const MIN_CARD_W = 120;
const MIN_CARD_H = 100;
const MAX_CARD_W = 500;
const MAX_CARD_H = 500;
const DEFAULT_CARD_W = 160;
const DEFAULT_CARD_H = 120;

function loadConfig(): CustomDashboardConfig {
  try {
    const saved = localStorage.getItem(LS_CONFIG_KEY);
    if (saved) {
      const parsed = JSON.parse(saved);
      return {
        cards: Array.isArray(parsed.cards) ? parsed.cards : [],
        backgroundMode: "transparent",
      };
    }
  } catch {}
  return { cards: [], backgroundMode: "transparent" };
}

function saveConfig(config: CustomDashboardConfig) {
  localStorage.setItem(LS_CONFIG_KEY, JSON.stringify(config));
  store.set(STORE_CONFIG_KEY, config).then(() => store.save());
}

/** 可添加到自定义页面的项：排除第三方工具 */
function getAvailableItems(): SearchItem[] {
  return searchIndex.filter((item) => item.category !== "thirdparty-tool");
}

/** 跑马灯文本：仅溢出时才滚动 */
function MarqueeLabel({ text, color }: { text: string; color: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const innerRef = useRef<HTMLDivElement>(null);
  const [overflow, setOverflow] = useState(false);

  useEffect(() => {
    const el = innerRef.current;
    const container = containerRef.current;
    if (!el || !container) return;
    setOverflow(el.scrollWidth > container.clientWidth);
  }, [text]);

  return (
    <Box ref={containerRef} w="100%" overflow="hidden" textAlign="center">
      <Box
        ref={innerRef}
        display="inline-flex"
        whiteSpace="nowrap"
        fontSize="xs"
        fontWeight="medium"
        color={color}
        gap={4}
        sx={overflow ? {
          animation: "marquee 6s linear infinite",
          "@keyframes marquee": {
            "0%": { transform: "translateX(0)" },
            "100%": { transform: "translateX(-50%)" },
          },
        } : undefined}
      >
        <span>{text}</span>
        {overflow && (
          <>
            <span>&nbsp;&nbsp;&nbsp;&nbsp;</span>
            <span>{text}</span>
          </>
        )}
      </Box>
    </Box>
  );
}

// ============ 添加工具弹窗 ============
function AddToolModal({
  isOpen,
  onClose,
  onAdd,
  existingCount,
}: {
  isOpen: boolean;
  onClose: () => void;
  onAdd: (item: SearchItem) => void;
  existingCount: Record<string, number>;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const cardBg = useColorModeValue("white", "#111111");
  const hoverBg = useColorModeValue("gray.100", "#2a2a2a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const iconColor = useColorModeValue("gray.700", "gray.300");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const badgeBg = useColorModeValue("green.100", "rgba(72,187,120,0.15)");
  const badgeColor = useColorModeValue("green.600", "green.300");
  const modalBg = useColorModeValue("white", "#111111");
  const searchIconColor = useColorModeValue("gray.400", "gray.500");
  const hoverBorderColor = useColorModeValue("gray.300", "#444444");
  const noResultColor = useColorModeValue("gray.400", "gray.500");

  const availableItems = useMemo(() => getAvailableItems(), []);

  const filtered = useMemo(() => {
    if (!query.trim()) return availableItems;
    const q = query.toLowerCase();
    return availableItems.filter((item) => {
      const name = t(item.nameKey).toLowerCase();
      const keywords = item.keywords?.map((k) => k.toLowerCase()) || [];
      return name.includes(q) || keywords.some((k) => k.includes(q));
    });
  }, [query, availableItems, t]);

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="xl" scrollBehavior="inside">
      <ModalOverlay />
      <ModalContent bg={modalBg} borderColor={borderColor} borderRadius="3xl" maxH="70vh">
        <ModalHeader color={textColor} fontSize="md">
          {t("customPage.addTool")}
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody pb={6}>
          <InputGroup size="sm" mb={4}>
            <InputLeftElement pointerEvents="none">
              <Icon as={Search} boxSize={4} color={searchIconColor} />
            </InputLeftElement>
            <Input
              placeholder={t("customPage.searchTools")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              bg={cardBg}
              borderColor={borderColor}
              borderRadius="lg"
            />
          </InputGroup>
          <SimpleGrid columns={{ base: 3, sm: 4, md: 5, lg: 6 }} spacing={3}>
            {filtered.map((item) => {
              const count = existingCount[item.id] || 0;
              return (
                <VStack
                  key={item.id}
                  spacing={2}
                  p={3}
                  borderRadius="lg"
                  cursor="pointer"
                  bg={cardBg}
                  border="1px solid"
                  borderColor={borderColor}
                  _hover={{ bg: hoverBg, transform: "translateY(-2px)", borderColor: hoverBorderColor }}
                  transition="all 0.2s"
                  onClick={() => onAdd(item)}
                  position="relative"
                >
                  {count > 0 && (
                    <Box
                      position="absolute"
                      top="4px"
                      right="4px"
                      bg={badgeBg}
                      color={badgeColor}
                      fontSize="9px"
                      fontWeight="bold"
                      px={1.5}
                      py={0.5}
                      borderRadius="full"
                    >
                      {count}
                    </Box>
                  )}
                  <Flex align="center" justify="center" h="32px">
                    {item.customIcon ? (
                      <Box as="img" src={item.customIcon} w="24px" h="24px" objectFit="contain" alt="" />
                    ) : (
                      <Icon as={item.icon} boxSize={6} color={iconColor} />
                    )}
                  </Flex>
                  <MarqueeLabel text={t(item.nameKey)} color={textColor} />
                </VStack>
              );
            })}
          </SimpleGrid>
          {filtered.length === 0 && (
            <Text textAlign="center" color={noResultColor} py={8} fontSize="sm">
              {t("search.noResults")}
            </Text>
          )}
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}

// ============ 单个可拖拽卡片 ============
function DraggableCard({
  card,
  item,
  onPositionChange,
  onDragBegin,
  onResizeStart,
  onRemove,
  onRadiusChange,
  onNavigate,
  zIndex: zIndexProp,
}: {
  card: CustomCardInstance;
  item: SearchItem | undefined;
  onPositionChange: (pos: { x: number; y: number }) => void;
  onDragBegin: () => void;
  onResizeStart: (e: React.MouseEvent) => void;
  onRemove: () => void;
  onRadiusChange: (value: number) => void;
  onNavigate: () => void;
  zIndex?: number;
}) {
  const { t } = useTranslation();
  const cardBg = useColorModeValue("rgba(255,255,255,1)", "rgba(17,17,17,1)");
  const glassBg = useColorModeValue("rgba(255,255,255,0.25)", "rgba(0,0,0,0.25)");
  const borderColor = useColorModeValue("rgba(200,200,200,0.3)", "rgba(51,51,51,0.5)");
  const glassBorder = useColorModeValue("rgba(255,255,255,0.2)", "rgba(255,255,255,0.1)");
  const iconColor = useColorModeValue("rgba(0,0,0,0.75)", "rgba(255,255,255,0.8)");
  const textColor = useColorModeValue("gray.700", "#ffffff");
  const resizeHandleColor = useColorModeValue("rgba(0,0,0,0.3)", "rgba(255,255,255,0.3)");
  const popoverBg = useColorModeValue("white", "#1a1a1a");
  const settingBtnBg = useColorModeValue("rgba(255,255,255,0.7)", "rgba(0,0,0,0.5)");
  const settingBtnHoverBg = useColorModeValue("rgba(255,255,255,0.95)", "rgba(0,0,0,0.75)");
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();

  const name = item ? t(item.nameKey) : "";
  const dragRef = useRef<{ sx: number; sy: number; mx: number; my: number; moved: boolean } | null>(null);

  const handleMouseDown = (e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("[data-card-action]")) return;
    e.preventDefault();
    dragRef.current = {
      sx: card.position.x,
      sy: card.position.y,
      mx: e.clientX,
      my: e.clientY,
      moved: false,
    };

    const onMove = (ev: MouseEvent) => {
      if (!dragRef.current) return;
      const dx = ev.clientX - dragRef.current.mx;
      const dy = ev.clientY - dragRef.current.my;
      if (!dragRef.current.moved && Math.abs(dx) < 3 && Math.abs(dy) < 3) return;
      if (!dragRef.current.moved) {
        dragRef.current.moved = true;
        onDragBegin();
      }
      onPositionChange({
        x: dragRef.current.sx + dx,
        y: dragRef.current.sy + dy,
      });
    };

    const onUp = () => {
      if (dragRef.current && !dragRef.current.moved) {
        onNavigate();
      }
      dragRef.current = null;
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  };

  return (
    <Box
      position="absolute"
      left={`${card.position.x}px`}
      top={`${card.position.y}px`}
      width={`${card.size.width}px`}
      height={`${card.size.height}px`}
      borderRadius={`${card.borderRadius}px`}
      bg={liquidGlassEnabled ? glassBg : cardBg}
      border="1px solid"
      borderColor={liquidGlassEnabled ? glassBorder : borderColor}
      cursor="grab"
      onMouseDown={handleMouseDown}
      zIndex={zIndexProp ?? 1}
      sx={{
        backdropFilter: liquidGlassEnabled ? `blur(${liquidGlassBlur}px)` : "none",
      }}
    >
      {/* 右上角设置按钮（在外层，不受 overflow 裁剪） */}
      <Popover placement="bottom-end" isLazy>
        <PopoverTrigger>
          <IconButton
            aria-label="card-settings"
            icon={<Icon as={SettingsIcon} boxSize={3} />}
            size="xs"
            variant="ghost"
            position="absolute"
            top="2px"
            right="2px"
            zIndex={5}
            borderRadius="full"
            minW="20px"
            h="20px"
            bg={settingBtnBg}
            _hover={{ bg: settingBtnHoverBg }}
            data-card-action
            onMouseDown={(e) => e.stopPropagation()}
          />
        </PopoverTrigger>
        <PopoverContent bg={popoverBg} borderColor={borderColor} width="180px" zIndex={10000} onMouseDown={(e) => e.stopPropagation()}>
          <PopoverArrow />
          <PopoverBody>
            <VStack spacing={2}>
              <Text fontSize="xs" color={textColor}>
                {t("customPage.borderRadius")}: {card.borderRadius}px
              </Text>
              <Slider
                value={card.borderRadius}
                min={0}
                max={30}
                step={1}
                onChange={onRadiusChange}
                colorScheme="blue"
              >
                <SliderTrack>
                  <SliderFilledTrack />
                </SliderTrack>
                <SliderThumb />
              </Slider>
              <IconButton
                aria-label="remove"
                icon={<Icon as={Trash2} boxSize={4} />}
                size="xs"
                variant="solid"
                colorScheme="red"
                borderRadius="full"
                data-card-action
                onClick={(e) => {
                  e.stopPropagation();
                  onRemove();
                }}
              />
            </VStack>
          </PopoverBody>
        </PopoverContent>
      </Popover>

      {/* 内容层（仅裁剪溢出，不承载背景/边框/模糊） */}
      <Box
        borderRadius={`${card.borderRadius}px`}
        overflow="hidden"
        w="100%"
        h="100%"
      >
        <Flex direction="column" align="center" justify="center" h="100%" p={2} gap={2}>
          {item?.customIcon ? (
            <Box as="img" src={item.customIcon} w="28px" h="28px" objectFit="contain" alt="" flexShrink={0} />
          ) : (
            item && <Icon as={item.icon} boxSize={7} color={iconColor} flexShrink={0} />
          )}
          <Text fontSize="xs" fontWeight="medium" noOfLines={2} textAlign="center" color={textColor} lineHeight="1.2">
            {name}
          </Text>
        </Flex>
      </Box>

      {/* 右下角缩放手柄（在外层，始终可见） */}
      <Box
        data-card-action
        position="absolute"
        right={0}
        bottom={0}
        w="16px"
        h="16px"
        cursor="nwse-resize"
        zIndex={5}
        onMouseDown={onResizeStart}
      >
        <Box
          position="absolute"
          right="2px"
          bottom="2px"
          w="10px"
          h="10px"
          borderRight="2.5px solid"
          borderBottom="2.5px solid"
          borderColor={resizeHandleColor}
          borderRadius="0 0 3px 0"
        />
      </Box>
    </Box>
  );
}

// ============ 主页面 ============
export default function CustomPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("layout");
  const { isOpen: isAddOpen, onOpen: onAddOpen, onClose: onAddClose } = useDisclosure();

  const [config, setConfig] = useState<CustomDashboardConfig>(() => loadConfig());
  const [zOrderMap, setZOrderMap] = useState<Record<string, number>>({});
  const [zOrderCounter, setZOrderCounter] = useState(0);
  const canvasRef = useRef<HTMLDivElement>(null);

  // 从 store 加载配置（覆盖旧 localStorage 数据）
  useEffect(() => {
    (async () => {
      const saved = await store.get<CustomDashboardConfig>(STORE_CONFIG_KEY);
      if (saved && Array.isArray(saved.cards) && saved.cards.length > 0) {
        setConfig(saved);
      }
    })();
  }, []);

  const headerColor = useColorModeValue("gray.800", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const emptyIconColor = useColorModeValue("gray.300", "gray.600");
  const scrollbarColor = useColorModeValue("rgba(0,0,0,0.15)", "rgba(255,255,255,0.1)");

  // 持久化
  const updateConfig = useCallback((updater: (prev: CustomDashboardConfig) => CustomDashboardConfig) => {
    setConfig((prev) => {
      const next = updater(prev);
      saveConfig(next);
      return next;
    });
  }, []);

  // 提升卡片的显示层级
  const bumpZOrder = useCallback((instanceId: string) => {
    setZOrderCounter((prev) => {
      const next = prev + 1;
      setZOrderMap((prevMap) => ({ ...prevMap, [instanceId]: next }));
      return next;
    });
  }, []);

  // 添加卡片（自动避开已有卡片）
  const handleAddCard = useCallback((item: SearchItem) => {
    const newW = DEFAULT_CARD_W;
    const newH = DEFAULT_CARD_H;
    const existingCards = config.cards;
    const step = 30;
    const baseX = 40;
    const baseY = 40;
    let x = baseX;
    let y = baseY;
    let col = 0;

    while (col < 200) {
      const overlaps = existingCards.some((c) =>
        x < c.position.x + c.size.width + 8 &&
        x + newW + 8 > c.position.x &&
        y < c.position.y + c.size.height + 8 &&
        y + newH + 8 > c.position.y
      );
      if (!overlaps) break;
      col++;
      x = baseX + (col % 8) * step;
      y = baseY + Math.floor(col / 8) * step;
    }

    const newCard: CustomCardInstance = {
      instanceId: `card-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
      itemId: item.id,
      position: { x, y },
      size: { width: newW, height: newH },
      borderRadius: 12,
    };
    updateConfig((prev) => ({ ...prev, cards: [...prev.cards, newCard] }));
    bumpZOrder(newCard.instanceId);
  }, [config.cards, updateConfig, bumpZOrder]);

  // 删除卡片
  const handleRemoveCard = useCallback((instanceId: string) => {
    updateConfig((prev) => ({
      ...prev,
      cards: prev.cards.filter((c) => c.instanceId !== instanceId),
    }));
  }, [updateConfig]);

  // 更新单个卡片
  const updateCard = useCallback((instanceId: string, patch: Partial<CustomCardInstance>) => {
    updateConfig((prev) => ({
      ...prev,
      cards: prev.cards.map((c) =>
        c.instanceId === instanceId ? { ...c, ...patch, position: { ...c.position, ...(patch.position || {}) }, size: { ...c.size, ...(patch.size || {}) } } : c
      ),
    }));
  }, [updateConfig]);

  // 拖拽位置更新（含边界限制）
  const handlePositionChange = useCallback((card: CustomCardInstance) => (pos: { x: number; y: number }) => {
    const canvas = canvasRef.current;
    const maxX = canvas ? canvas.scrollWidth - card.size.width : 2000;
    const maxY = canvas ? canvas.scrollHeight - card.size.height : 2000;
    updateCard(card.instanceId, {
      position: {
        x: Math.max(0, Math.min(maxX, pos.x)),
        y: Math.max(0, Math.min(maxY, pos.y)),
      },
    });
  }, [updateCard]);

  // 缩放卡片
  const handleResizeStart = useCallback((card: CustomCardInstance) => (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startY = e.clientY;
    const startSize = { ...card.size };

    const onMove = (ev: MouseEvent) => {
      const dx = ev.clientX - startX;
      const dy = ev.clientY - startY;
      updateCard(card.instanceId, {
        size: {
          width: Math.min(MAX_CARD_W, Math.max(MIN_CARD_W, startSize.width + dx)),
          height: Math.min(MAX_CARD_H, Math.max(MIN_CARD_H, startSize.height + dy)),
        },
      });
    };

    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, [updateCard]);

  // 卡片点击导航（由 DraggableCard 内部判断非拖拽时触发）
  const handleCardNavigate = useCallback((card: CustomCardInstance) => {
    const item = searchIndex.find((i) => i.id === card.itemId);
    if (item) {
      navigate(item.path);
    }
  }, [navigate]);

  // 统计每个工具已添加数量
  const existingCount = useMemo(() => {
    const map: Record<string, number> = {};
    config.cards.forEach((c) => {
      map[c.itemId] = (map[c.itemId] || 0) + 1;
    });
    return map;
  }, [config.cards]);

  // 动态计算画布最小高度，确保下方卡片不被裁剪
  const canvasMinHeight = useMemo(() => {
    if (config.cards.length === 0) return 400;
    const maxBottom = Math.max(...config.cards.map((c) => c.position.y + c.size.height));
    return Math.max(400, maxBottom + 80);
  }, [config.cards]);

  // 画布右键打开添加工具弹窗
  const handleCanvasContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    onAddOpen();
  }, [onAddOpen]);

  return (
    <Box h="100%" display="flex" flexDirection="column" overflow="hidden">
      {/* 顶部工具栏 */}
      <Flex justify="space-between" align="center" mb={3} flexShrink={0} px={1}>
        <Flex align="center" gap={2}>
          <Icon as={LayoutGrid} boxSize={5} color={headerColor} />
          <Text fontSize="lg" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            {t("customPage.title")}
          </Text>
        </Flex>
        <HStack spacing={2}>
          {/* 清空按钮 */}
          {config.cards.length > 0 && (
            <Tooltip label={t("customPage.clearAll")}>
              <IconButton
                aria-label="clear"
                size="xs"
                variant="ghost"
                colorScheme="red"
                icon={<Icon as={Trash2} boxSize={4} />}
                onClick={() => {
                  if (window.confirm(t("customPage.clearAllConfirm"))) {
                    updateConfig((prev) => ({ ...prev, cards: [] }));
                    toast({ title: t("common.success"), status: "success", duration: 1500 });
                  }
                }}
              />
            </Tooltip>
          )}
          {/* 添加按钮 */}
          <LiquidGlassButton size="sm" onClick={onAddOpen} leftIcon={<Plus size={16} />}>
            {t("customPage.addTool")}
          </LiquidGlassButton>
        </HStack>
      </Flex>

      {/* 画布区域 */}
      <Box
        ref={canvasRef}
        flex={1}
        overflow="auto"
        borderRadius="xl"
        position="relative"
        minH="300px"
        onContextMenu={handleCanvasContextMenu}
        sx={{
          "&::-webkit-scrollbar": { width: "6px", height: "6px" },
          "&::-webkit-scrollbar-thumb": { background: scrollbarColor, borderRadius: "3px" },
        }}
      >
        {config.cards.length === 0 ? (
          <Flex direction="column" align="center" justify="center" h="100%" minH="300px" gap={4}>
            <Icon as={LayoutGrid} boxSize={12} color={emptyIconColor} />
            <Text color={descColor} fontSize="sm">
              {t("customPage.emptyHint")}
            </Text>
            <LiquidGlassButton size="sm" onClick={onAddOpen} leftIcon={<Plus size={16} />}>
              {t("customPage.addTool")}
            </LiquidGlassButton>
          </Flex>
        ) : (
          <Box w="100%" minH={`${canvasMinHeight}px`} minW="100%" position="relative">
            {config.cards.map((card) => {
              const item = searchIndex.find((i) => i.id === card.itemId);
              if (!item) return null;
              return (
                <DraggableCard
                  key={card.instanceId}
                  card={card}
                  item={item}
                  onPositionChange={handlePositionChange(card)}
                  onDragBegin={() => bumpZOrder(card.instanceId)}
                  onResizeStart={handleResizeStart(card)}
                  onRemove={() => handleRemoveCard(card.instanceId)}
                  onRadiusChange={(val) => updateCard(card.instanceId, { borderRadius: val })}
                  onNavigate={() => handleCardNavigate(card)}
                  zIndex={zOrderMap[card.instanceId] ?? 1}
                />
              );
            })}
          </Box>
        )}
      </Box>

      <AddToolModal
        isOpen={isAddOpen}
        onClose={onAddClose}
        onAdd={(item) => {
          handleAddCard(item);
          onAddClose();
        }}
        existingCount={existingCount}
      />
    </Box>
  );
}
