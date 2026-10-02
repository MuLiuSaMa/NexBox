import { useState, useCallback } from "react";
import {
  Box,
  Heading,
  VStack,
  Flex,
  useColorModeValue,
} from "@chakra-ui/react";
import {
  Cpu,
  Trash2,
  MemoryStick,
  Gauge,
  Zap,
  HardDrive,
  List,
  Settings2,
  Network,
  MousePointer2,
  Download,
  Gamepad2,
  Database,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ViewGrid } from "@/components/special/view-grid";
import { ViewList } from "@/components/special/view-list";
import { LayoutToggle, type LayoutMode } from "@/components/special/layout-toggle";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { store } from "@/lib/store";
import type { ViewItem } from "@/components/special/view-types";

const STORE_KEY = "nexbox_optimize_tools_order";
const LS_KEY = "nexbox_optimize_tools_order";

const defaultTools: ViewItem[] = [
  {
    id: "storage-clean",
    path: "/optimize/storage-clean",
    icon: HardDrive,
    titleKey: "storageClean.title",
    descriptionKey: "storageClean.description",
    color: "#3182CE",
  },
  {
    id: "memory-cleanup",
    path: "/optimize/memory-cleanup",
    icon: MemoryStick,
    titleKey: "optimization.memoryCleanup.title",
    descriptionKey: "optimization.memoryCleanup.description",
    color: "#38A169",
  },
  {
    id: "ace-optimize",
    path: "/optimize/ace-optimize",
    icon: Gauge,
    titleKey: "optimization.aceOptimize.title",
    descriptionKey: "optimization.aceOptimize.description",
    color: "#DD6B20",
  },
  {
    id: "memory-limit",
    path: "/optimize/memory-limit",
    icon: Cpu,
    titleKey: "optimization.memoryLimit.title",
    descriptionKey: "optimization.memoryLimit.description",
    color: "#FF6B9D",
  },
  {
    id: "shader-cache",
    path: "/optimize/shader-cache",
    icon: Trash2,
    titleKey: "shaderCache.title",
    descriptionKey: "builtinTools.shaderCacheDesc",
    color: "#EF4444",
  },
  {
    id: "power-management",
    path: "/optimize/power-management",
    icon: Zap,
    titleKey: "optimization.powerManagement.title",
    descriptionKey: "optimization.powerManagement.description",
    color: "#F6AD55",
  },
  {
    id: "startup-manager",
    path: "/optimize/startup-manager",
    icon: List,
    titleKey: "optimization.startupManager.title",
    descriptionKey: "optimization.startupManager.description",
    color: "#805AD5",
  },
  {
    id: "system-optimizer",
    path: "/optimize/system-optimizer",
    icon: Settings2,
    titleKey: "systemOptimizer.pageTitle",
    descriptionKey: "systemOptimizer.pageDesc",
    color: "#667EEA",
  },
  {
    id: "network-optimizer",
    path: "/optimize/network-optimizer",
    icon: Network,
    titleKey: "networkOptimize.pageTitle",
    descriptionKey: "networkOptimize.pageDesc",
    color: "#38A169",
  },
  {
    id: "peripheral-optimize",
    path: "/optimize/peripheral-optimize",
    icon: MousePointer2,
    titleKey: "peripheralOptimize.pageTitle",
    descriptionKey: "peripheralOptimize.pageDesc",
    color: "#E53E3E",
  },
  {
    id: "windows-update",
    path: "/optimize/windows-update",
    icon: Download,
    titleKey: "windowsUpdate.pageTitle",
    descriptionKey: "windowsUpdate.pageDesc",
    color: "#E53E3E",
  },
  {
    id: "cpu-scheduler",
    path: "/optimize/cpu-scheduler",
    icon: Cpu,
    titleKey: "optimization.cpuScheduler.title",
    descriptionKey: "optimization.cpuScheduler.description",
    color: "#3182CE",
    beta: true,
  },
  {
    id: "game-process-optimize",
    path: "/optimize/game-process-optimize",
    icon: Gamepad2,
    titleKey: "optimization.gameProcessOptimize.title",
    descriptionKey: "optimization.gameProcessOptimize.description",
    color: "#8B5CF6",
  },
  {
    id: "pso-cache",
    path: "/optimize/pso-cache",
    icon: Database,
    titleKey: "psoCache.title",
    descriptionKey: "builtinTools.psoCacheDesc",
    color: "#0EA5E9",
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
  for (const tool of map.values()) {
    ordered.push(tool);
  }
  return ordered;
}

export default function OptimizePage() {
  const { t } = useTranslation();
  const adaptiveTitle = useAdaptiveTextColor();
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

  const content = (
    <VStack align="start" spacing={6}>
      <Flex w="full" justify="space-between" align="center">
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("optimization.pageTitle")}
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
