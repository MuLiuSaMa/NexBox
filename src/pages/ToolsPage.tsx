import {
  Box,
  Flex,
  Grid,
  Text,
  Heading,
  Icon,
  useColorModeValue,
  Badge,
  Button,
  VStack,
  HStack,
  Divider,
  IconButton,
  Tooltip,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassToolCard } from "@/components/special/liquid-glass-tool-card";
import { LiquidGlassMenuItem } from "@/components/special/liquid-glass-menu-item";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import {
  Cpu,
  Zap,
  Wrench,
  Network,
  TrendingUp,
  Play,
  Circle,
  Monitor,
  Bot,
  Volume2,
  Trash2,
  Plus,
  X,
  ExternalLink,
  Shield,
  HardDrive,
  Thermometer,
  Activity,
  Eraser,
  Scaling,
  Search,
  Settings2,
  Scissors,
  Boxes,
  PieChart,
  Archive,
  Rocket,
  Gauge,
  LineChart,
  Gamepad2,
  MousePointer,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useAppStartup } from "@/contexts/app-startup-context";
import { Image } from "@chakra-ui/react";
import { store } from "@/lib/store";
import { CommunityToolSection } from "@/components/community-tools/community-tool-section";
const CUSTOM_TOOLS_KEY = "custom-added-tools";

const toolIcons = import.meta.glob<{ default: string }>(
  "@/assets/tools/*.{png,jpg,jpeg,svg,webp}",
  { eager: true }
);

function getToolIconImage(toolId: string): string | null {
  const normalizedId = toolId.toLowerCase();
  for (const [path, module] of Object.entries(toolIcons)) {
    const fileName = path.split("/").pop()?.split(".")[0]?.toLowerCase();
    if (fileName === normalizedId) {
      return module.default;
    }
  }
  return null;
}

interface ThirdPartyTool {
  id: string;
  name: string;
  description: string;
  category: string;
  tool_type: string;
  download_url: string;
  file_name: string;
  website_url: string | null;
  check_executable: string | null;
}

const handleToolClick = async (toolId: string) => {
};

interface ToolCard {
  id: string;
  title: string;
  description: string;
  icon: React.ElementType;
  category: "hardware" | "assistant" | "network" | "optimization";
  type: "builtin" | "thirdparty";
}

const getTools = (t: (key: string) => string): ToolCard[] => [
];

const getCategoryLabels = (t: (key: string) => string): Record<string, string> => ({
  hardware: t("tools.hardware"),
  assistant: t("tools.assistant"),
  network: t("tools.network"),
  optimization: t("tools.optimization"),
});

const categoryColors: Record<string, string> = {
  hardware: "blue",
  assistant: "purple",
  network: "green",
  optimization: "orange",
};

function ToolCardComponent({
  tool,
  categoryLabels,
}: {
  tool: ToolCard;
  categoryLabels: Record<string, string>;
}) {
  const iconColor = useColorModeValue("gray.700", "#cccccc");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const { getActiveColor } = useThemeColor();

  return (
    <LiquidGlassToolCard
      size="md"
      onClick={() => handleToolClick(tool.id)}
    >
      <VStack align="start" spacing={3} h="full">
        <Flex
          h={12}
          w={12}
          align="center"
          justify="center"
          borderRadius="lg"
          bg={useColorModeValue("gray.100", "#222222")}
        >
          <Icon as={tool.icon} boxSize={6} color={iconColor} />
        </Flex>
        <Box flex={1} w="full">
          <HStack justify="space-between" align="start" mb={1}>
            <Text fontSize="sm" fontWeight="semibold" color={titleColor}>
              {tool.title}
            </Text>
            <Badge colorScheme={categoryColors[tool.category]} fontSize="xs" variant="subtle">
              {categoryLabels[tool.category]}
            </Badge>
          </HStack>
          <Text fontSize="xs" color={descColor} lineHeight="short">
            {tool.description}
          </Text>
        </Box>
      </VStack>
    </LiquidGlassToolCard>
  );
}

function ThirdPartyToolCard({
  tool,
  initialInstalled,
  categoryLabels,
  customToolPath,
  onAddCustomTool,
  onRemoveCustomTool,
}: {
  tool: ThirdPartyTool;
  initialInstalled: boolean;
  categoryLabels: Record<string, string>;
  customToolPath?: string;
  onAddCustomTool?: (toolId: string, filePath: string) => void;
  onRemoveCustomTool?: (toolId: string) => void;
}) {
  const { t } = useTranslation();
  const [installed, setInstalled] = useState(initialInstalled);
  const [isAdding, setIsAdding] = useState(false);
  const toast = useDynamicIsland("wrench");

  const iconColor = useColorModeValue("gray.700", "#cccccc");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const { getActiveColor } = useThemeColor();

  const isCustomAdded = !!customToolPath;
  const isInstalled = installed || isCustomAdded;

  useEffect(() => {
    setInstalled(initialInstalled);
  }, [initialInstalled]);

  const getToolIcon = (toolId: string) => {
    switch (toolId) {
      case "memreduct":
        return Zap;
      case "optimizer":
        return TrendingUp;
      case "cpu-z":
        return Cpu;
      case "gpu-z":
        return Monitor;

      case "gamepp":
        return Bot;
      case "fxsound":
        return Volume2;
      case "msi-afterburner":
        return Monitor;
      case "geek":
        return Trash2;
      case "hwinfo":
        return Cpu;
      case "crystaldiskinfo":
        return HardDrive;
      case "core-temp":
        return Thermometer;
      case "aida64":
        return Activity;
      case "ddu":
        return Eraser;
      case "lossless-scaling":
        return Scaling;
      case "everything":
        return Search;
      case "dismpp":
        return Settings2;
      case "snipaste":
        return Scissors;
      case "powertoys":
        return Boxes;
      case "wiztree":
        return PieChart;
      case "7zip":
        return Archive;
      case "watt-toolkit":
        return Rocket;
      case "trafficmonitor":
        return Gauge;
      case "rtss":
        return LineChart;
      case "playnite":
        return Gamepad2;
      default:
        return Wrench;
    }
  };

  const toolIconImage = getToolIconImage(tool.id);
  const FallbackIcon = getToolIcon(tool.id);

  const handleRun = async () => {
    try {
      if (customToolPath) {
        await invoke("launch_game", { gamePath: customToolPath });
      } else {
        await invoke("run_tool", { toolId: tool.id });
      }
    } catch (error) {
      console.error("Failed to run tool:", error);
      toast({
        title: t("tools.messages.runFailed"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleOpenWebsite = async () => {
    if (!tool.website_url) return;
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(tool.website_url);
    } catch (error) {
      console.error("Failed to open website:", error);
    }
  };

  const handleClick = () => {
    if (isInstalled) {
      handleRun();
    } else {
      handleOpenWebsite();
    }
  };

  const handleCustomButtonClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isAdding) return;
    setIsAdding(true);
    try {
      const selectedPath = await invoke<string | null>("select_exe_file");
      if (selectedPath) {
        onAddCustomTool?.(tool.id, selectedPath);
        toast({
          title: t("tools.customAdded.addSuccess"),
          status: "success",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to select file:", error);
      toast({
        title: t("tools.customAdded.addFailed"),
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    } finally {
      setIsAdding(false);
    }
  };

  const handleRemoveCustom = (e: React.MouseEvent) => {
    e.stopPropagation();
    onRemoveCustomTool?.(tool.id);
    toast({
      title: t("tools.customAdded.removed"),
      status: "info",
      duration: 2000,
      isClosable: true,
    });
  };

  return (
    <LiquidGlassToolCard
      size="md"
      cursor="pointer"
      onClick={handleClick}
      position="relative"
    >
      {isInstalled && !isCustomAdded && (
        <Box position="absolute" top={3} right={3}>
          <Icon as={Circle} boxSize={3} fill="green.400" color="green.400" />
        </Box>
      )}

      {isCustomAdded && (
        <HStack position="absolute" top={2} right={2} spacing={1}>
          <Icon as={Circle} boxSize={3} fill="green.400" color="green.400" />
          <Tooltip label={t("tools.customAdded.remove")} placement="top">
            <IconButton
              aria-label={t("tools.customAdded.remove")}
              icon={<Icon as={X} boxSize={3} />}
              size="xs"
              variant="ghost"
              colorScheme="red"
              onClick={handleRemoveCustom}
            />
          </Tooltip>
        </HStack>
      )}

      <VStack align="start" spacing={3} h="full">
        <Flex
          h={12}
          w={12}
          align="center"
          justify="center"
          borderRadius="lg"
          bg={useColorModeValue("gray.100", "#222222")}
          overflow="hidden"
        >
          {toolIconImage ? (
            <Image
              src={toolIconImage}
              alt={tool.name}
              w="32px"
              h="32px"
              objectFit="contain"
              fallback={<Icon as={FallbackIcon} boxSize={6} color={iconColor} />}
            />
          ) : (
            <Icon as={FallbackIcon} boxSize={6} color={iconColor} />
          )}
        </Flex>
        <Box flex={1} w="full">
          <HStack justify="space-between" align="start" mb={1}>
            <Text fontSize="sm" fontWeight="semibold" color={titleColor}>
              {t(`tools.tools.${tool.id}`, tool.name)}
            </Text>
            <Badge
              colorScheme={categoryColors[tool.category] || "gray"}
              fontSize="xs"
              variant="subtle"
            >
              {categoryLabels[tool.category] || tool.category}
            </Badge>
          </HStack>
          <Text fontSize="xs" color={descColor} lineHeight="short" mb={2}>
            {t(`tools.descriptions.${tool.id}`, tool.description)}
          </Text>

          {!isInstalled && (
            <HStack spacing={1} color={getActiveColor()}>
              <Icon as={ExternalLink} boxSize={3} />
              <Text fontSize="xs">{t("tools.buttons.website")}</Text>
            </HStack>
          )}

          {isInstalled && (
            <HStack spacing={1} color="green.500">
              <Icon as={Play} boxSize={3} />
              <Text fontSize="xs">{t("tools.buttons.run")}</Text>
            </HStack>
          )}
        </Box>
      </VStack>

      {!isInstalled && (
        <Tooltip
          label={t("tools.customAdded.add")}
          placement="top"
        >
          <IconButton
            aria-label={t("tools.customAdded.add")}
            icon={<Icon as={Plus} boxSize={3} />}
            size="xs"
            position="absolute"
            bottom={3}
            right={3}
            borderRadius="full"
            variant="outline"
            colorScheme="gray"
            bg="transparent"
            isLoading={isAdding}
            isDisabled={isAdding}
            _hover={{
              bg: useColorModeValue("gray.100", "gray.700"),
            }}
            onClick={handleCustomButtonClick}
          />
        </Tooltip>
      )}
    </LiquidGlassToolCard>
  );
}

function ToolSection({
  title,
  tools: sectionTools,
  activeCategory,
  categoryLabels,
}: {
  title: string;
  tools: ToolCard[];
  activeCategory: string;
  categoryLabels: Record<string, string>;
}) {
  const filteredTools =
    activeCategory === "all"
      ? sectionTools
      : sectionTools.filter((tool) => tool.category === activeCategory);

  if (filteredTools.length === 0) return null;

  const sectionTitleColor = useColorModeValue("gray.800", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");

  return (
    <Box mb={8}>
      <HStack mb={4} spacing={3}>
        <Text fontSize="lg" fontWeight="bold" color={sectionTitleColor}>
          {title}
        </Text>
        <Badge fontSize="xs" colorScheme="gray">
          {filteredTools.length}
        </Badge>
      </HStack>
      <Divider borderColor={dividerColor} mb={4} />
      <Grid
        templateColumns="repeat(auto-fill, minmax(240px, 1fr))"
        gap={4}
        alignItems="stretch"
      >
        {filteredTools.map((tool) => (
          <ToolCardComponent key={tool.id} tool={tool} categoryLabels={categoryLabels} />
        ))}
      </Grid>
    </Box>
  );
}

function OfficialToolSection({
  activeCategory,
}: {
  activeCategory: string;
}) {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const sectionTitleColor = useColorModeValue("gray.800", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  const iconColor = useColorModeValue("gray.700", "#cccccc");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");

  // Official recommendation only shows when category is "all"
  if (activeCategory !== "all") return null;

  const handleOpenTuba = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://tubawinui3.cn/");
    }).catch(() => {
      window.open("https://tubawinui3.cn/", "_blank");
    });
  };

  const handleOpenMCTier = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://mctier.pmhs.top/");
    }).catch(() => {
      window.open("https://mctier.pmhs.top/", "_blank");
    });
  };

  const handleOpenSjmcl = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://mc.sjtu.cn/sjmcl/");
    }).catch(() => {
      window.open("https://mc.sjtu.cn/sjmcl/", "_blank");
    });
  };

  const handleOpenAxolotl = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://axlmc.org/");
    }).catch(() => {
      window.open("https://axlmc.org/", "_blank");
    });
  };

  const handleOpenDdegame = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.ddegame.cn/");
    }).catch(() => {
      window.open("https://www.ddegame.cn/", "_blank");
    });
  };

  const handleOpenHuorong = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.huorong.cn/");
    }).catch(() => {
      window.open("https://www.huorong.cn/", "_blank");
    });
  };

  const handleOpenSteam = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://store.steampowered.com/");
    }).catch(() => {
      window.open("https://store.steampowered.com/", "_blank");
    });
  };

  const handleOpenEpicGames = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.epicgames.com/");
    }).catch(() => {
      window.open("https://www.epicgames.com/", "_blank");
    });
  };

  const handleOpenNvidiaApp = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.nvidia.cn/software/nvidia-app/");
    }).catch(() => {
      window.open("https://www.nvidia.cn/software/nvidia-app/", "_blank");
    });
  };

  const handleOpenPCL2 = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://ifdian.net/a/LTcat");
    }).catch(() => {
      window.open("https://ifdian.net/a/LTcat", "_blank");
    });
  };

  const handleOpenOBS = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://obsproject.com/download");
    }).catch(() => {
      window.open("https://obsproject.com/download", "_blank");
    });
  };

  const handleOpenWallpaper = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.wallpaperengine.io/zh-hans");
    }).catch(() => {
      window.open("https://www.wallpaperengine.io/zh-hans", "_blank");
    });
  };

  const handleOpenTieZ = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://tiez.name666.top/zh/");
    }).catch(() => {
      window.open("https://tiez.name666.top/zh/", "_blank");
    });
  };

  const handleOpenPyisland = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://pyisland.com/");
    }).catch(() => {
      window.open("https://pyisland.com/", "_blank");
    });
  };

  const handleOpenMR = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://mineradio.cn/");
    }).catch(() => {
      window.open("https://mineradio.cn/", "_blank");
    });
  };

  const handleOpenDeepseek = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.deepseek.com/harness/");
    }).catch(() => {
      window.open("https://www.deepseek.com/harness/", "_blank");
    });
  };

  const handleOpenLingTab = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://lingtab.nexbox.top/");
    }).catch(() => {
      window.open("https://lingtab.nexbox.top/", "_blank");
    });
  };

  const handleOpenLXMusic = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://lxmusic.toside.cn/");
    }).catch(() => {
      window.open("https://lxmusic.toside.cn/", "_blank");
    });
  };

  const handleOpenGamepp = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://gamepp.com/");
    }).catch(() => {
      window.open("https://gamepp.com/", "_blank");
    });
  };

  const handleOpenShudaxia = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://www.shudaxia.com/");
    }).catch(() => {
      window.open("https://www.shudaxia.com/", "_blank");
    });
  };

  const handleOpenFlyingMouse = () => {
    import("@tauri-apps/plugin-shell").then(({ open }) => {
      open("https://github.com/LaoFeng-mouse/flyingmouse-format");
    }).catch(() => {
      window.open("https://github.com/LaoFeng-mouse/flyingmouse-format", "_blank");
    });
  };

  return (
    <Box mb={8}>
      <HStack mb={4} spacing={3}>
        <Text fontSize={"lg"} fontWeight={"bold"} color={sectionTitleColor}>
          {t("tools.officialTools")}
        </Text>
        <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
          {t("tools.recommended")}
        </Badge>
      </HStack>
      <Divider borderColor={dividerColor} mb={4} />
      <Grid
        templateColumns="repeat(auto-fill, minmax(240px, 1fr))"
        gap={4}
        alignItems="stretch"
      >
        <LiquidGlassToolCard size={"md"} onClick={handleOpenPyisland}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("pyisland") || ""}
                alt={"Pyisland"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Pyisland
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                Windows 灵动岛新时代 — 打造现代控制中心
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenTuba}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("tuba") || ""}
                alt={"图吧工具箱"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  图吧工具箱WinUI3
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                WinUI 3 原生 · 完全免费 · 开源 PC 硬件检测利器
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenMCTier}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("mctier") || ""}
                alt={"MCTier"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  MCTier
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.mctierDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenSjmcl}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("sjmcl") || ""}
                alt={"SJMCL"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  SJMCL
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.sjmclDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenAxolotl}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("axolotl") || ""}
                alt={"Axolotl Launcher"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Axolotl Launcher
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                Tauri v2 · Rust · Vue 3 免费、开源、无广告的跨平台 Minecraft Java 版启动器
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenDdegame}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("ddegame") || ""}
                alt={"东东电竞"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  东东电竞
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.ddegameDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenHuorong}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("huorong") || ""}
                alt={"火绒安全"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<Shield size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  火绒安全
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.huorongDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenSteam}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("steam") || ""}
                alt={"Steam"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Steam
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.steamDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenEpicGames}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("epic-games") || ""}
                alt={"Epic Games"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Epic Games
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.epicGamesDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenNvidiaApp}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("nvidia-app") || ""}
                alt={"NVIDIA APP"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  NVIDIA APP
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.nvidiaAppDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenPCL2}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("pcl2") || ""}
                alt={"PCL2"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  PCL2
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.pcl2Desc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenOBS}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("obs") || ""}
                alt={"OBS Studio"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  OBS Studio
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.obsDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenWallpaper}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("wallpaper") || ""}
                alt={"Wallpaper Engine"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Wallpaper Engine
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                在 Windows 桌面上使用精美绝伦的动态壁纸。
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenTieZ}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("tiez") || ""}
                alt={"TieZ"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  TieZ
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                开源美观剪切板工具
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenMR}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("mr") || ""}
                alt={"Mineradio"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  Mineradio
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                随音乐律动的沉浸式可视化音乐播放器，把每一首歌，变成一场只属于你的私人视觉演出。
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenDeepseek}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("deepseek") || ""}
                alt={"DeepSeek Harness"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<Bot size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  DeepSeek Harness
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.deepseekHarnessDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenLingTab}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("lingtab") || ""}
                alt={"灵动标签"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  灵动标签
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.lingtabDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenLXMusic}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("lx") || ""}
                alt={"落雪音乐"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  落雪音乐
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.lxmusicDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenGamepp}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("gamepp") || ""}
                alt={"游戏加加"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<Bot size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  游戏加加
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.gameppDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenShudaxia}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("sdx") || ""}
                alt={"鼠大侠"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<MousePointer size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  鼠大侠
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                {t("tools.shudaxiaDesc")}
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>

        <LiquidGlassToolCard size={"md"} onClick={handleOpenFlyingMouse}>
          <VStack align={"start"} spacing={3} h="full">
            <Flex
              h={12}
              w={12}
              align={"center"}
              justify={"center"}
              borderRadius={"lg"}
              bg={useColorModeValue("gray.100", "#222222")}
              overflow={"hidden"}
            >
              <Image
                src={getToolIconImage("mouse-idle") || ""}
                alt={"鼠鼠格式"}
                w={"32px"}
                h={"32px"}
                objectFit={"contain"}
                fallback={<ExternalLink size={24} color={iconColor} />}
              />
            </Flex>
            <Box flex={1} w={"full"}>
              <HStack justify={"space-between"} align={"start"} mb={1}>
                <Text fontSize={"sm"} fontWeight={"semibold"} color={titleColor}>
                  鼠鼠格式
                </Text>
                <Badge fontSize={"xs"} variant={"subtle"} color={getActiveColor()} bg={`${getActiveColor()}20`}>
                  {t("tools.recommended")}
                </Badge>
              </HStack>
              <Text fontSize={"xs"} color={descColor} lineHeight={"short"}>
                一款鼠鼠主题、可离线使用的 Windows 文件格式转换工具。
              </Text>
            </Box>
          </VStack>
        </LiquidGlassToolCard>
      </Grid>
    </Box>
  );
}

function ThirdPartyToolSection({
  title,
  activeCategory,
  categoryLabels,
}: {
  title: string;
  activeCategory: string;
  categoryLabels: Record<string, string>;
}) {
  const { tools, initTools } = useAppStartup();
  const [customToolPaths, setCustomToolPaths] = useState<Record<string, string>>({});

  const sectionTitleColor = useColorModeValue("gray.800", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");

  useEffect(() => {
    if (tools.length === 0) {
      initTools();
    }
  }, []);

  useEffect(() => {
    const loadCustomTools = async () => {
      try {
        const savedTools = await store.get<Record<string, string>>(CUSTOM_TOOLS_KEY);
        if (savedTools && typeof savedTools === "object") {
          setCustomToolPaths(savedTools);
        }
      } catch (error) {
        console.error("Failed to load custom tools:", error);
      }
    };
    loadCustomTools();
  }, []);

  const addCustomTool = useCallback(async (toolId: string, filePath: string) => {
    setCustomToolPaths((prev) => {
      const newPaths = { ...prev, [toolId]: filePath };
      store.set(CUSTOM_TOOLS_KEY, newPaths);
      store.save();
      return newPaths;
    });
  }, []);

  const removeCustomTool = useCallback(async (toolId: string) => {
    setCustomToolPaths((prev) => {
      const newPaths = { ...prev };
      delete newPaths[toolId];
      store.set(CUSTOM_TOOLS_KEY, newPaths);
      store.save();
      return newPaths;
    });
  }, []);

  const filteredTools =
    activeCategory === "all"
      ? tools
      : tools.filter((tool) => tool.category === activeCategory);

  const sortedTools = [...filteredTools].sort((a, b) => {
    const aInstalled = customToolPaths[a.id] ? 1 : 0;
    const bInstalled = customToolPaths[b.id] ? 1 : 0;
    return bInstalled - aInstalled;
  });

  if (filteredTools.length === 0) return null;

  return (
    <Box mb={8}>
      <HStack mb={4} spacing={3}>
        <Text fontSize="lg" fontWeight="bold" color={sectionTitleColor}>
          {title}
        </Text>
        <Badge fontSize="xs" colorScheme="gray">
          {filteredTools.length}
        </Badge>
      </HStack>
      <Divider borderColor={dividerColor} mb={4} />
      <Grid
        templateColumns="repeat(auto-fill, minmax(240px, 1fr))"
        gap={4}
        alignItems="stretch"
      >
        {sortedTools.map((tool) => (
          <ThirdPartyToolCard 
            key={tool.id} 
            tool={tool} 
            initialInstalled={!!customToolPaths[tool.id]} 
            categoryLabels={categoryLabels}
            customToolPath={customToolPaths[tool.id]}
            onAddCustomTool={addCustomTool}
            onRemoveCustomTool={removeCustomTool}
          />
        ))}
      </Grid>
    </Box>
  );
}

export default function ToolsPage() {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const { config } = useThemeColor();

  const tools = getTools(t);
  const categoryLabels = getCategoryLabels(t);

  const builtinTools = tools.filter((tool) => tool.type === "builtin");

  const [activeSection, setActiveSection] = useState<"community" | "official" | "thirdparty">("official");

  const menuItems = [
    { key: "official" as const, label: t("tools.officialTools"), icon: Rocket },
    { key: "community" as const, label: t("tools.community.title"), icon: Boxes },
    { key: "thirdparty" as const, label: t("tools.thirdpartyTools"), icon: Wrench },
  ];

  return (
    <Flex gap={6} pt={8}>
      <Box w="180px" flexShrink={0} position="sticky" top={8} alignSelf="flex-start">
        <VStack spacing={0.5} align="stretch">
          {menuItems.map((item) => (
            <LiquidGlassMenuItem
              key={item.key}
              isActive={activeSection === item.key}
              onClick={() => setActiveSection(item.key)}
              icon={item.icon}
            >
              {item.label}
            </LiquidGlassMenuItem>
          ))}
        </VStack>
      </Box>
      <Box
        flex={1}
        overflowY="auto"
        overflowX="hidden"
        sx={{
          "&::-webkit-scrollbar": {
            width: "6px",
          },
          "&::-webkit-scrollbar-track": {
            background: "transparent",
            margin: "10px 0",
          },
          "&::-webkit-scrollbar-thumb": {
            background: config.primaryColor,
            borderRadius: "3px",
            minHeight: "40px",
          },
          "&::-webkit-scrollbar-thumb:hover": {
            background: config.primaryColor,
            opacity: 0.8,
            filter: "brightness(0.9)",
          },
        }}
      >
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} mb={6}>
          {t("tools.title")}
        </Heading>

        {activeSection === "official" && <OfficialToolSection activeCategory="all" />}

        {activeSection === "community" && <CommunityToolSection />}

        {activeSection === "thirdparty" && (
          <ThirdPartyToolSection
            title={t("tools.thirdpartyTools")}
            activeCategory="all"
            categoryLabels={categoryLabels}
          />
        )}

        {/* 内置工具（目前为空数据，渲染为隐藏占位） */}
        <ToolSection
          title={t("tools.builtinTools")}
          tools={builtinTools}
          activeCategory="all"
          categoryLabels={categoryLabels}
        />
      </Box>
    </Flex>
  );
}
