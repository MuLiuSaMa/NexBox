import { HStack, IconButton, Tooltip, useColorModeValue } from "@chakra-ui/react";
import { LayoutGrid, Square, List } from "lucide-react";
import { useTranslation } from "react-i18next";

export type LayoutMode = "grid" | "compact" | "list";

/** 内置工具 / 优化页共用的布局偏好键：两页布局样式保持同步 */
export const LAYOUT_STORAGE_KEY = "nexbox_tools_layout";

interface LayoutToggleProps {
  mode: LayoutMode;
  onChange: (mode: LayoutMode) => void;
}

export function LayoutToggle({ mode, onChange }: LayoutToggleProps) {
  const { t } = useTranslation();
  const activeBg = useColorModeValue("blackAlpha.100", "whiteAlpha.200");
  const inactiveColor = useColorModeValue("gray.400", "gray.500");

  const options: {
    mode: LayoutMode;
    icon: React.ComponentType<{ size?: number }>;
    labelKey: string;
    size?: number;
  }[] = [
    { mode: "grid", icon: LayoutGrid, labelKey: "view.grid" },
    { mode: "compact", icon: Square, labelKey: "view.compact", size: 14 },
    { mode: "list", icon: List, labelKey: "view.list" },
  ];

  return (
    <HStack spacing={1}>
      {options.map((opt) => {
        const Icon = opt.icon;
        const isActive = mode === opt.mode;
        const label = t(opt.labelKey);
        return (
          <Tooltip key={opt.mode} label={label} placement="top">
            <IconButton
              aria-label={label}
              icon={<Icon size={opt.size ?? 18} />}
              size="sm"
              variant="ghost"
              borderRadius="md"
              color={isActive ? undefined : inactiveColor}
              bg={isActive ? activeBg : "transparent"}
              onClick={() => onChange(opt.mode)}
              _hover={{ bg: activeBg }}
            />
          </Tooltip>
        );
      })}
    </HStack>
  );
}
