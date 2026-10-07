import { Box, HStack, Text, Switch, useColorModeValue, Icon, Button, IconButton, useDisclosure } from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { GripVertical, Cpu, Thermometer, Activity, HardDrive, Key, Gauge, Fan, Zap, Clock, Download, Settings } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import {
  DndContext,
  closestCenter,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  DragEndEvent,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { IS_STORE_BUILD } from "@/lib/build-flags";
import { PawnioInstallModal } from "./PawnioInstallModal";

export interface DisplayItem {
  id: string;
  label: string;
  enabled: boolean;
}

interface DraggableDisplayItemsProps {
  items: DisplayItem[];
  onReorder: (items: DisplayItem[]) => void;
  onToggle: (id: string, enabled: boolean) => void;
  disabledItems?: string[];
  onDeltaPasswordSettings?: () => void;
}

const ITEM_ICONS: Record<string, React.FC<{ size?: number }>> = {
  time: Clock,
  cpu_usage: Cpu,
  cpu_temp: Thermometer,
  cpu_clock: Clock,
  cpu_voltage: Zap,
  cpu_power: Zap,
  cpu_fan_speed: Fan,
  gpu_temp: Thermometer,
  gpu_usage: Activity,
  gpu_fan_speed: Fan,
  gpu_power: Zap,
  gpu_clock: Clock,
  gpu_memory_clock: Clock,
  gpu_voltage: Zap,
  gpu_vram: HardDrive,
  memory_usage: HardDrive,
  ssd_temp: HardDrive,
  delta_password: Key,
  game_ping: Gauge,
  fps: Activity,
  fps_1low: Activity,
  fps_01low: Activity,
};

function SortableItem({
  item,
  onToggle,
  enabledCount,
  disabled,
  onSettingsClick,
  onInstallClick,
}: {
  item: DisplayItem;
  onToggle: (id: string, enabled: boolean) => void;
  enabledCount: number;
  disabled?: boolean;
  onSettingsClick?: () => void;
  onInstallClick?: () => void;
}) {
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const iconColor = useColorModeValue("gray.500", "#999999");
  const hoverBg = useColorModeValue("gray.50", "#1a1a1a");
  const dragBg = useColorModeValue("gray.100", "#222222");
  const toast = useDynamicIsland("layout");
  const { getActiveColor } = useThemeColor();

  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: item.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.8 : 1,
    zIndex: isDragging ? 10 : 1,
  };

  const IconComponent = ITEM_ICONS[item.id] || Activity;

  const handleToggle = (checked: boolean) => {
    if (!checked && enabledCount <= 1) {
      toast({
        title: "至少需要保留一个显示项",
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    onToggle(item.id, checked);
  };

  return (
    <HStack
      ref={setNodeRef}
      style={style}
      py={2}
      px={3}
      borderRadius="lg"
      bg={isDragging ? dragBg : "transparent"}
      _hover={{ bg: hoverBg }}
      transition="background 0.15s"
      spacing={3}
    >
      <Box
        cursor="grab"
        color={iconColor}
        display="flex"
        alignItems="center"
        {...attributes}
        {...listeners}
      >
        <GripVertical size={16} />
      </Box>
      <Icon as={() => <IconComponent size={18} />} color={item.enabled ? getActiveColor() : "gray.400"} />
      <Text color={textColor} fontSize="sm" flex={1}>
        {item.label}
      </Text>
      {item.id === "delta_password" && onSettingsClick && (
        <IconButton
          aria-label="地图设置"
          icon={<Settings size={15} />}
          size="xs"
          variant="ghost"
          color={getActiveColor()}
          _hover={{ bg: hexToRgba(getActiveColor(), 0.1) }}
          onClick={(e) => {
            e.stopPropagation();
            onSettingsClick();
          }}
          mr={1}
        />
      )}
      {item.id === "cpu_temp" && !IS_STORE_BUILD && (
        <Button
          size="xs"
          variant="outline"
          color={getActiveColor()}
          borderColor={getActiveColor()}
          _hover={{ bg: hexToRgba(getActiveColor(), 0.1) }}
          leftIcon={<Download size={12} />}
          onClick={onInstallClick}
          mr={1}
        >
          安装驱动
        </Button>
      )}
      <Switch
        isChecked={item.enabled}
        onChange={(e) => handleToggle(e.target.checked)}
        size="sm"
        isDisabled={disabled}
        sx={{
          '& .chakra-switch__track[data-checked]': {
            bg: getActiveColor(),
          },
        }}
      />
    </HStack>
  );
}

export function DraggableDisplayItems({
  items,
  onReorder,
  onToggle,
  disabledItems = [],
  onDeltaPasswordSettings,
}: DraggableDisplayItemsProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, {
      activationConstraint: {
        distance: 8,
      },
    }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    })
  );

  const { isOpen: isPawnioModalOpen, onOpen: onPawnioModalOpen, onClose: onPawnioModalClose } = useDisclosure();
  const enabledCount = items.filter((item) => item.enabled).length;

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;

    if (over && active.id !== over.id) {
      const oldIndex = items.findIndex((item) => item.id === active.id);
      const newIndex = items.findIndex((item) => item.id === over.id);

      onReorder(arrayMove(items, oldIndex, newIndex));
    }
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragEnd={handleDragEnd}
    >
      <SortableContext
        items={items.map((item) => item.id)}
        strategy={verticalListSortingStrategy}
      >
        <Box>
          {items.map((item) => (
            <SortableItem
              key={item.id}
              item={item}
              onToggle={onToggle}
              enabledCount={enabledCount}
              disabled={disabledItems.includes(item.id)}
              onSettingsClick={item.id === "delta_password" ? onDeltaPasswordSettings : undefined}
              onInstallClick={item.id === "cpu_temp" ? onPawnioModalOpen : undefined}
            />
          ))}
        </Box>
      </SortableContext>

      {/* PawnIO 安装对话框：商店版不提供安装功能 */}
      {!IS_STORE_BUILD && (
        <PawnioInstallModal
          isOpen={isPawnioModalOpen}
          onClose={onPawnioModalClose}
        />
      )}
    </DndContext>
  );
}
