import { useCallback, useState } from "react";
import {
  Box,
  VStack,
  HStack,
  Text,
  Icon,
  useColorModeValue,
} from "@chakra-ui/react";
import { ChevronRight, GripVertical } from "lucide-react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
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
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { LiquidGlassCard } from "./liquid-glass-card";
import { PressTilt } from "./press-tilt";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import type { ViewItem } from "./view-types";

interface ViewListProps {
  tools: ViewItem[];
  onReorder?: (tools: ViewItem[]) => void;
}

function SortableListItem({ tool, activeDrag }: { tool: ViewItem; activeDrag: boolean }) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { config } = useThemeColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const listBg = useColorModeValue("white", "#111111");
  const listBorder = useColorModeValue("gray.200", "#333333");
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");
  const isDark = useColorModeValue(false, true);
  const dragHandleColor = useColorModeValue("gray.400", "#666666");

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

  const IconComponent = tool.icon;

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
      <HStack spacing={4} align="center">
        <Box
          w={10}
          h={10}
          borderRadius="lg"
          bg={`${tool.color}20`}
          display="flex"
          alignItems="center"
          justifyContent="center"
          color={tool.color}
          flexShrink={0}
        >
          <IconComponent size={22} />
        </Box>
        <VStack align="start" spacing={0} flex={1} minW={0}>
          <Text
            color={headingColor}
            fontSize="md"
            fontWeight="bold"
            noOfLines={1}
          >
            {t(tool.titleKey)}
          </Text>
          <Text color={subTextColor} fontSize="sm" noOfLines={1}>
            {t(tool.descriptionKey)}
          </Text>
        </VStack>
        <ChevronRight size={18} color={subTextColor} />
      </HStack>
    </>
  );

  return (
    <Box ref={setNodeRef} style={style} w="full" role="group">
      <Box position="relative" w="full">
        {/* Drag handle — left side, visible on group hover */}
        <Box
          position="absolute"
          left={2}
          top="50%"
          transform="translateY(-50%)"
          zIndex={20}
          cursor={isDragging ? "grabbing" : "grab"}
          opacity={0}
          _groupHover={{ opacity: 1 }}
          transition="opacity 0.2s"
          display="flex"
          alignItems="center"
          justifyContent="center"
          w="20px"
          h="20px"
          borderRadius="4px"
          color={dragHandleColor}
          {...attributes}
          {...listeners}
        >
          <Icon as={GripVertical} boxSize="16px" />
        </Box>

        {/* Card wrapped in Link — disable pointer events while dragging */}
        <Box
          as={Link}
          to={tool.path}
          display="block"
          w="full"
          style={{ pointerEvents: activeDrag ? "none" : "auto" }}
        >
          <PressTilt color={tool.color}>
            {liquidGlassEnabled ? (
              <LiquidGlassCard w="full" cursor="pointer" p={4} pl={10} position="relative">
                {cardContent}
              </LiquidGlassCard>
            ) : (
              <Box
                bg={listBg}
                borderRadius="xl"
                border="1px solid"
                borderColor={listBorder}
                p={4}
                pl={10}
                cursor="pointer"
                transition="all 0.2s"
                _hover={{ borderColor: tool.color, bg: hoverBg }}
                position="relative"
              >
                {cardContent}
              </Box>
            )}
          </PressTilt>
        </Box>
      </Box>
    </Box>
  );
}

export function ViewList({ tools, onReorder }: ViewListProps) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { config } = useThemeColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const listBg = useColorModeValue("white", "#111111");
  const listBorder = useColorModeValue("gray.200", "#333333");
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");
  const isDark = useColorModeValue(false, true);

  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: { distance: 8 },
    })
  );

  const [activeId, setActiveId] = useState<string | null>(null);

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

  // Lock horizontal movement — only allow vertical dragging
  const restrictToVerticalAxis = useCallback<Modifier>(
    ({ transform }) => ({ ...transform, x: 0 }),
    [],
  );

  // Without onReorder, render a simple static list (no drag-and-drop overhead)
  if (!onReorder) {
    const listCardContent = (tool: ViewItem) => {
      const IconComponent = tool.icon;
      return (
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
          <HStack spacing={4} align="center">
            <Box
              w={10}
              h={10}
              borderRadius="lg"
              bg={`${tool.color}20`}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={tool.color}
              flexShrink={0}
            >
              <IconComponent size={22} />
            </Box>
            <VStack align="start" spacing={0} flex={1} minW={0}>
              <Text
                color={headingColor}
                fontSize="md"
                fontWeight="bold"
                noOfLines={1}
              >
                {t(tool.titleKey)}
              </Text>
              <Text color={subTextColor} fontSize="sm" noOfLines={1}>
                {t(tool.descriptionKey)}
              </Text>
            </VStack>
            <ChevronRight size={18} color={subTextColor} />
          </HStack>
        </>
      );
    };

    return (
      <VStack w="full" spacing={3}>
        {tools.map((tool) => (
          <Link key={tool.id} to={tool.path} style={{ width: "100%" }}>
            <PressTilt color={tool.color}>
              {liquidGlassEnabled ? (
                <LiquidGlassCard w="full" cursor="pointer" p={4} position="relative">
                  {listCardContent(tool)}
                </LiquidGlassCard>
              ) : (
                <Box
                  bg={listBg}
                  borderRadius="xl"
                  border="1px solid"
                  borderColor={listBorder}
                  p={4}
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ borderColor: tool.color, bg: hoverBg }}
                  position="relative"
                >
                  {listCardContent(tool)}
                </Box>
              )}
            </PressTilt>
          </Link>
        ))}
      </VStack>
    );
  }

  const toolIds = tools.map((t) => t.id);

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      modifiers={[restrictToVerticalAxis]}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      <SortableContext items={toolIds} strategy={verticalListSortingStrategy}>
        <VStack w="full" spacing={3}>
          {tools.map((tool) => (
            <SortableListItem
              key={tool.id}
              tool={tool}
              activeDrag={activeId !== null}
            />
          ))}
        </VStack>
      </SortableContext>
    </DndContext>
  );
}
