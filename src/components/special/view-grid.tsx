import { useCallback, useRef, useState } from "react";
import {
  Box,
  SimpleGrid,
  VStack,
  Text,
  Icon,
  useColorModeValue,
} from "@chakra-ui/react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { Grip } from "lucide-react";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
  type Modifier,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  rectSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { LiquidGlassCard } from "./liquid-glass-card";
import { PressTilt } from "./press-tilt";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import type { ViewItem } from "./view-types";

interface ViewGridProps {
  tools: ViewItem[];
  onReorder?: (tools: ViewItem[]) => void;
}

function SortableGridCard({ tool }: { tool: ViewItem }) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { config } = useThemeColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const IconComponent = tool.icon;
  const isDark = useColorModeValue(false, true);
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");
  const dragHandleColor = useColorModeValue("gray.400", "#666666");
  const dragHandleBg = useColorModeValue("rgba(0,0,0,0.06)", "rgba(255,255,255,0.06)");
  const dragHandleHoverBg = useColorModeValue("rgba(0,0,0,0.12)", "rgba(255,255,255,0.12)");

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    isDragging,
  } = useSortable({ id: tool.id });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    // 拖动中的卡片快速跟随指针；其它卡片用弹性过冲曲线产生"被挤"果冻 + 回弹
    transition: isDragging
      ? "transform 0ms"
      : "transform 380ms cubic-bezier(0.34, 1.56, 0.64, 1)",
    opacity: 1,
    zIndex: isDragging ? 100 : 1,
    position: "relative",
  };

  const cardContent = (
    <>
      {tool.beta && (
        <Box
          position="absolute"
          top={2}
          right={2}
          fontSize="10px"
          fontWeight="700"
          color={config.primaryColor}
          bg={hexToRgba(config.primaryColor, isDark ? 0.18 : 0.1)}
          px={1.5}
          py={0.5}
          borderRadius="full"
          zIndex={1}
        >
          BETA
        </Box>
      )}
      <VStack align="start" spacing={4}>
        <Box
          w={12}
          h={12}
          borderRadius="xl"
          bg={`${tool.color}20`}
          display="flex"
          alignItems="center"
          justifyContent="center"
          color={tool.color}
        >
          <IconComponent size={28} />
        </Box>
        <VStack align="start" spacing={1}>
          <Text color={headingColor} fontSize="lg" fontWeight="bold">
            {t(tool.titleKey)}
          </Text>
          <Text color={subTextColor} fontSize="sm">
            {t(tool.descriptionKey)}
          </Text>
        </VStack>
      </VStack>
    </>
  );

  const innerCard = liquidGlassEnabled ? (
    <LiquidGlassCard
      w="full"
      h="full"
      minH="200px"
      p={6}
      position="relative"
    >
      {cardContent}
    </LiquidGlassCard>
  ) : (
    <Box
      bg={cardBg}
      borderRadius="xl"
      p={6}
      minH="200px"
      border="2px solid"
      borderColor={cardBorder}
      transition="all 0.2s"
      _hover={{
        borderColor: isDragging ? cardBorder : tool.color,
        bg: hoverBg,
      }}
      position="relative"
      overflow="hidden"
    >
      {cardContent}
    </Box>
  );

  return (
    <Box ref={setNodeRef} style={style} role="group">
      {/* Drag handle — square, top-left corner, visible on hover */}
      <Box
        position="absolute"
        top="6px"
        left="6px"
        zIndex={20}
        cursor={isDragging ? "grabbing" : "grab"}
        opacity={0}
        _groupHover={{ opacity: 1 }}
        transition="opacity 0.2s"
        display="flex"
        alignItems="center"
        justifyContent="center"
        w="22px"
        h="22px"
        bg={dragHandleBg}
        _hover={{ bg: dragHandleHoverBg }}
        borderRadius="5px"
        {...attributes}
        {...listeners}
      >
        <Icon as={Grip} boxSize="14px" color={dragHandleColor} />
      </Box>

      {/* Card: disable pointer events while dragging to prevent accidental navigation */}
      <Box
        as={Link}
        to={tool.path}
        display="block"
        style={{ pointerEvents: isDragging ? "none" : "auto" }}
        cursor={isDragging ? "grabbing" : "pointer"}
      >
        <PressTilt color={tool.color}>{innerCard}</PressTilt>
      </Box>
    </Box>
  );
}

function StaticGridCard({ tool }: { tool: ViewItem }) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { config } = useThemeColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const cardBorder = useColorModeValue("gray.200", "#333333");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const IconComponent = tool.icon;
  const isDark = useColorModeValue(false, true);
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");

  const cardContent = (
    <>
      {tool.beta && (
        <Box
          position="absolute"
          top={2}
          right={2}
          fontSize="10px"
          fontWeight="700"
          color={config.primaryColor}
          bg={hexToRgba(config.primaryColor, isDark ? 0.18 : 0.1)}
          px={1.5}
          py={0.5}
          borderRadius="full"
          zIndex={1}
        >
          BETA
        </Box>
      )}
      <VStack align="start" spacing={4}>
        <Box
          w={12}
          h={12}
          borderRadius="xl"
          bg={`${tool.color}20`}
          display="flex"
          alignItems="center"
          justifyContent="center"
          color={tool.color}
        >
          <IconComponent size={28} />
        </Box>
        <VStack align="start" spacing={1}>
          <Text color={headingColor} fontSize="lg" fontWeight="bold">
            {t(tool.titleKey)}
          </Text>
          <Text color={subTextColor} fontSize="sm">
            {t(tool.descriptionKey)}
          </Text>
        </VStack>
      </VStack>
    </>
  );

  if (liquidGlassEnabled) {
    return (
      <Link to={tool.path}>
        <PressTilt color={tool.color}>
          <LiquidGlassCard
            w="full"
            h="full"
            minH="200px"
            cursor="pointer"
            p={6}
            position="relative"
          >
            {cardContent}
          </LiquidGlassCard>
        </PressTilt>
      </Link>
    );
  }

  return (
    <Link to={tool.path}>
      <PressTilt color={tool.color}>
        <Box
          bg={cardBg}
          borderRadius="xl"
          p={6}
          minH="200px"
          cursor="pointer"
          border="2px solid"
          borderColor={cardBorder}
          transition="all 0.2s"
          _hover={{
            borderColor: tool.color,
            bg: hoverBg,
          }}
          position="relative"
          overflow="hidden"
        >
          {cardContent}
        </Box>
      </PressTilt>
    </Link>
  );
}

export function ViewGrid({ tools, onReorder }: ViewGridProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    })
  );

  const [activeId, setActiveId] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Custom modifier: keep the dragged card inside the grid container
  const restrictToContainer = useCallback<Modifier>(
    ({ transform, draggingNodeRect }) => {
      if (!draggingNodeRect || !containerRef.current) return transform;
      const container = containerRef.current.getBoundingClientRect();

      return {
        ...transform,
        x: Math.min(
          Math.max(transform.x, container.left - draggingNodeRect.left),
          container.right - draggingNodeRect.right,
        ),
        y: Math.min(
          Math.max(transform.y, container.top - draggingNodeRect.top),
          container.bottom - draggingNodeRect.bottom,
        ),
      };
    },
    [],
  );

  const handleDragStart = useCallback((event: DragStartEvent) => {
    setActiveId(String(event.active.id));
  }, []);

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      setActiveId(null);
      const { active, over } = event;
      if (over && active.id !== over.id && onReorder) {
        const oldIndex = tools.findIndex((t) => t.id === active.id);
        const newIndex = tools.findIndex((t) => t.id === over.id);
        if (oldIndex !== -1 && newIndex !== -1) {
          onReorder(arrayMove(tools, oldIndex, newIndex));
        }
      }
    },
    [tools, onReorder],
  );

  const handleDragCancel = useCallback(() => {
    setActiveId(null);
  }, []);

  // Without onReorder, render a simple static grid (no drag-and-drop overhead)
  if (!onReorder) {
    return (
      <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={4} w="full">
        {tools.map((tool) => (
          <StaticGridCard key={tool.id} tool={tool} />
        ))}
      </SimpleGrid>
    );
  }

  const toolIds = tools.map((t) => t.id);

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      modifiers={[restrictToContainer]}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      <SortableContext items={toolIds} strategy={rectSortingStrategy}>
        <Box ref={containerRef} overflow="hidden" w="full">
          <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={4} w="full">
            {tools.map((tool) => (
              <SortableGridCard key={tool.id} tool={tool} />
            ))}
          </SimpleGrid>
        </Box>
      </SortableContext>
    </DndContext>
  );
}
