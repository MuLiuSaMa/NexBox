import { useState, useCallback } from "react";
import {
  Box,
  Heading,
  VStack,
  Flex,
  useColorModeValue,
} from "@chakra-ui/react";
import {
  Palette,
  Crosshair,
  Layout,
  Cpu,
  Zap,
  Monitor,
  HardDrive,
  Volume2,
  MousePointerClick,
  MousePointer2,
  Gauge,
  Braces,
  ShieldCheck,
  FlaskConical,
  DownloadCloud,
} from "lucide-react";
import nvidiaLogoImg from "@/assets/nvidia.png";
import { useTranslation } from "react-i18next";

// NVIDIA Logo 图片组件
function NvidiaLogo({ size = 24 }: { size?: number; color?: string }) {
  return <img src={nvidiaLogoImg} width={size} height={size} style={{ objectFit: 'contain' }} alt="NVIDIA" />;
}

import { ViewGrid } from "@/components/special/view-grid";
import { ViewList } from "@/components/special/view-list";
import { LayoutToggle, type LayoutMode } from "@/components/special/layout-toggle";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { store } from "@/lib/store";
import type { ViewItem } from "@/components/special/view-types";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

const STORE_KEY = "nexbox_builtin_tools_order";
const LS_KEY = "nexbox_builtin_tools_order";

const defaultTools: ViewItem[] = [
  {
    id: "display-filter",
    path: "/display-filter",
    icon: Palette,
    titleKey: "sidebar.displayFilter",
    descriptionKey: "builtinTools.displayFilterDesc",
    color: "#98DDD0",
  },
  {
    id: "crosshair",
    path: "/crosshair",
    icon: Crosshair,
    titleKey: "sidebar.crosshair",
    descriptionKey: "builtinTools.crosshairDesc",
    color: "#FF6B9D",
  },
  {
    id: "overlay-panel",
    path: "/overlay-panel",
    icon: Layout,
    titleKey: "sidebar.overlayPanel",
    descriptionKey: "builtinTools.overlayPanelDesc",
    color: "#9B59B6",
  },
  {
    id: "gpu-rename",
    path: "/gpu-rename",
    icon: Cpu,
    titleKey: "sidebar.gpuRename",
    descriptionKey: "builtinTools.gpuRenameDesc",
    color: "#F39C12",
  },
  {
    id: "resolution-converter",
    path: "/resolution-converter",
    icon: Monitor,
    titleKey: "sidebar.resolutionConverter",
    descriptionKey: "builtinTools.resolutionConverterDesc",
    color: "#4A90E2",
  },
  {
    id: "dlss-preset",
    path: "/dlss-preset",
    icon: Zap,
    titleKey: "sidebar.dlssPreset",
    descriptionKey: "builtinTools.dlssPresetDesc",
    color: "#76B900",
  },
  {
    id: "disk-health",
    path: "/disk-health",
    icon: HardDrive,
    titleKey: "sidebar.diskHealth",
    descriptionKey: "builtinTools.diskHealthDesc",
    color: "#00B4D8",
  },
  {
    id: "nvidia-driver",
    path: "/nvidia-driver",
    icon: NvidiaLogo,
    titleKey: "sidebar.nvidiaDriver",
    descriptionKey: "builtinTools.nvidiaDriverDesc",
    color: "#76B900",
  },
  {
    id: "audio-eq",
    path: "/audio-eq",
    icon: Volume2,
    titleKey: "sidebar.audioEq",
    descriptionKey: "builtinTools.audioEqDesc",
    color: "#E74C3C",
  },
  {
    id: "nvidia-driver-download",
    path: "/nvidia-driver-download",
    icon: NvidiaLogo,
    titleKey: "sidebar.nvidiaDriverDownload",
    descriptionKey: "builtinTools.nvidiaDriverDownloadDesc",
    color: "#76B900",
  },
  {
    id: "autoclicker",
    path: "/autoclicker",
    icon: MousePointerClick,
    titleKey: "sidebar.autoclicker",
    descriptionKey: "builtinTools.autoclickerDesc",
    color: "#FF8C00",
  },
  {
    id: "speedtest",
    path: "/speedtest",
    icon: Gauge,
    titleKey: "sidebar.speedtest",
    descriptionKey: "builtinTools.speedtestDesc",
    color: "#00B4D8",
  },
  {
    id: "runtime-repair",
    path: "/runtime-repair",
    icon: Braces,
    titleKey: "sidebar.runtimeRepair",
    descriptionKey: "builtinTools.runtimeRepairDesc",
    color: "#4A90E2",
  },
  {
    id: "vtx-virtualization",
    path: "/vtx-virtualization",
    icon: ShieldCheck,
    titleKey: "sidebar.vtxVirtualization",
    descriptionKey: "builtinTools.vtxVirtualizationDesc",
    color: "#38A169",
  },
  {
    id: "hidden-features",
    path: "/hidden-features",
    icon: FlaskConical,
    titleKey: "sidebar.hiddenFeatures",
    descriptionKey: "builtinTools.hiddenFeaturesDesc",
    color: "#E67E22",
    beta: true,
  },
  {
    id: "context-menu",
    path: "/context-menu",
    icon: MousePointer2,
    titleKey: "sidebar.contextMenu",
    descriptionKey: "builtinTools.contextMenuDesc",
    color: "#3B82F6",
  },
  {
    id: "download-accelerator",
    path: "/download-accelerator",
    icon: DownloadCloud,
    titleKey: "sidebar.downloadAccelerator",
    descriptionKey: "builtinTools.downloadAcceleratorDesc",
    color: "#00C896",
  },
];

function loadOrder(): string[] | null {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (raw) {
      const ids: string[] = JSON.parse(raw);
      if (Array.isArray(ids) && ids.length > 0) return ids;
    }
  } catch { /* ignore */ }
  return null;
}

function saveOrder(ids: string[]) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(ids));
    store.set(STORE_KEY, ids).then(() => store.save());
  } catch { /* ignore */ }
}

function applyOrder(allTools: ViewItem[], orderIds: string[]): ViewItem[] {
  const map = new Map(allTools.map((t) => [t.id, t]));
  const ordered: ViewItem[] = [];
  for (const id of orderIds) {
    const tool = map.get(id);
    if (tool) {
      ordered.push(tool);
      map.delete(id);
    }
  }
  // Append any new tools not in the saved order
  for (const tool of map.values()) {
    ordered.push(tool);
  }
  return ordered;
}

export default function BuiltinToolsPage() {
  const { t } = useTranslation();
  const [layoutMode, setLayoutMode] = useState<LayoutMode>("grid");

  const [tools, setTools] = useState<ViewItem[]>(() => {
    const saved = loadOrder();
    if (saved) return applyOrder(defaultTools, saved);
    return defaultTools;
  });

  const handleReorder = useCallback((newTools: ViewItem[]) => {
    setTools(newTools);
    saveOrder(newTools.map((t) => t.id));
  }, []);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();

  const content = (
    <VStack align="start" spacing={6}>
      <Flex w="full" justify="space-between" align="center">
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("builtinTools.title")}
        </Heading>
        <LiquidGlassCard display="inline-flex" p={1} boxShadow="sm">
          <LayoutToggle mode={layoutMode} onChange={setLayoutMode} />
        </LiquidGlassCard>
      </Flex>
      {layoutMode === "grid" ? (
        <ViewGrid tools={tools} onReorder={handleReorder} />
      ) : (
        <ViewList tools={tools} onReorder={handleReorder} />
      )}
    </VStack>
  );

  return (
    <Box pt={8}>
      {content}
    </Box>
  );
}
