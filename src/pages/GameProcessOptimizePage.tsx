import {
  Box,
  Text,
  VStack,
  HStack,
  Button,
  SimpleGrid,
  useColorModeValue,
  Badge,
  IconButton,
  Switch,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
  Checkbox,
  useDisclosure,
  Tooltip,
  Input,
  InputGroup,
  InputLeftElement,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useProcessIcons } from "@/hooks/use-process-icons";
import { useState, useCallback, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import {
  Gamepad2,
  FileSearch,
  ListFilter,
  MonitorCog,
  Trash2,
  Cpu,
  Zap,
  Settings2,
  ShieldAlert,
  ShieldCheck,
  Search,
  HardDrive,
  Gauge,
  MemoryStick,
  Rocket,
} from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import deltaforceIcon from "@/assets/deltaforce.png";

interface GameOptimizeConfig {
  id: string;
  name: string;
  process_names: string[];
  priority: "realtime" | "high" | "abovenormal" | "normal" | "belownormal" | "idle";
  affinity_mask: number | null;
  auto_optimize_priority: boolean;
  auto_optimize_affinity: boolean;
  auto_optimize_ifeo: boolean;
  auto_optimize_io: boolean;
  auto_optimize_memory: boolean;
  auto_optimize_gpu: boolean;
  auto_optimize_gpu_pref: boolean;
  /** 游戏图标（exe 提取的 64px PNG data URI；空 = 无图标走兜底） */
  icon: string;
}

interface GameAutoStatus {
  game_id: string;
  running: boolean;
  optimized: boolean;
  priority_applied: boolean;
  affinity_applied: boolean;
  ifeo_applied: boolean;
  io_applied: boolean;
  memory_applied: boolean;
  gpu_applied: boolean;
  gpu_pref_applied: boolean;
  last_apply: string | null;
}

interface RunningProcessInfo {
  name: string;
  pid: number;
  memory_mb: number;
  exe_path: string;
}

interface GameExecutableInfo {
  path: string;
  process_name: string;
  game_name: string;
  icon: string;
}

interface FilterGameEntry {
  id: string;
  name: string;
  process_names: string[];
  is_builtin: boolean;
}

interface FilterStatus {
  enabled: boolean;
  games: FilterGameEntry[];
}

type AutoKind = "priority" | "affinity" | "ifeo" | "io" | "memory" | "gpu" | "gpu_pref";

/** AutoKind → GameOptimizeConfig 对应开关字段 */
const AUTO_KIND_FIELD: Record<
  Exclude<AutoKind, "ifeo">,
  "auto_optimize_priority" | "auto_optimize_affinity" | "auto_optimize_io" | "auto_optimize_memory" | "auto_optimize_gpu" | "auto_optimize_gpu_pref"
> = {
  priority: "auto_optimize_priority",
  affinity: "auto_optimize_affinity",
  io: "auto_optimize_io",
  memory: "auto_optimize_memory",
  gpu: "auto_optimize_gpu",
  gpu_pref: "auto_optimize_gpu_pref",
};

const EMPTY_CONFIG = (id: string, name: string, process_names: string[]): GameOptimizeConfig => ({
  id,
  name,
  process_names,
  priority: "realtime",
  affinity_mask: null,
  auto_optimize_priority: false,
  auto_optimize_affinity: false,
  auto_optimize_ifeo: false,
  auto_optimize_io: false,
  auto_optimize_memory: false,
  auto_optimize_gpu: false,
  auto_optimize_gpu_pref: false,
  icon: "",
});

/** 内置图标资产的进程名映射（小写去 .exe）：滤镜名单添加的游戏没有 exe 可提取，命中即用资产 */
const BUILTIN_GAME_ICON_ASSETS: Record<string, string> = {
  "deltaforceclient-win64-shipping": deltaforceIcon,
};

// 核心选择弹窗：显示所有逻辑核心的复选框网格
function CoreSelectionModal({
  isOpen,
  onClose,
  coreCount,
  currentSavedMask,
  onSave,
}: {
  isOpen: boolean;
  onClose: () => void;
  coreCount: number;
  currentSavedMask: number;
  onSave: (mask: number) => void;
}) {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const modalBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const headingColor = useColorModeValue("gray.900", "#ffffff");

  const [selectedCores, setSelectedCores] = useState<number[]>([]);

  useEffect(() => {
    const cores: number[] = [];
    for (let i = 0; i < coreCount; i++) {
      if (((currentSavedMask / Math.pow(2, i)) % 2) >= 1) {
        cores.push(i);
      }
    }
    setSelectedCores(cores);
  }, [currentSavedMask, coreCount, isOpen]);

  const handleToggle = (coreIndex: number) => {
    setSelectedCores(prev =>
      prev.includes(coreIndex)
        ? prev.filter(c => c !== coreIndex)
        : [...prev, coreIndex]
    );
  };

  const handleSave = () => {
    const mask = selectedCores.reduce((acc, core) => acc + Math.pow(2, core), 0);
    onSave(mask);
    onClose();
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="md">
      <ModalOverlay backdropFilter="blur(4px)" />
      <ModalContent bg={modalBg} borderColor={borderColor} borderRadius="3xl">
        <ModalHeader color={headingColor}>
          {t("gameProcessOptimize.affinitySettings.title")}
          <Text fontSize="xs" color="gray.500" fontWeight="normal" mt={1}>
            {t("gameProcessOptimize.affinitySettings.description", { count: coreCount })}
          </Text>
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          <VStack align="stretch" spacing={1}>
            <HStack justify="space-between" mb={2} flexWrap="wrap" gap={2}>
              <Text fontSize="xs" color="gray.500">
                CPU0 - CPU{coreCount - 1}
              </Text>
              <HStack spacing={2}>
                <Button size="xs" variant="ghost" onClick={() => setSelectedCores(Array.from({ length: coreCount }, (_, i) => i))}>
                  {t("gameProcessOptimize.affinitySettings.selectAll")}
                </Button>
                <Button size="xs" variant="ghost" onClick={() => setSelectedCores([])}>
                  {t("gameProcessOptimize.affinitySettings.clearAll")}
                </Button>
              </HStack>
            </HStack>
            <Box
              maxH="300px"
              overflowY="auto"
              sx={{
                "&::-webkit-scrollbar": { width: "4px" },
                "&::-webkit-scrollbar-thumb": { bg: "gray.600", borderRadius: "full" },
              }}
            >
              <SimpleGrid columns={{ base: 3, sm: 4, md: 6 }} spacing={2}>
                {Array.from({ length: coreCount }, (_, i) => (
                  <Checkbox
                    key={i}
                    isChecked={selectedCores.includes(i)}
                    onChange={() => handleToggle(i)}
                    size="sm"
                    sx={{
                      "& .chakra-checkbox__control": {
                        borderColor: getActiveColor(),
                      },
                      "& .chakra-checkbox__control[data-checked]": {
                        bg: getActiveColor(),
                        borderColor: getActiveColor(),
                        color: "white",
                      },
                    }}
                  >
                    <Text fontSize="xs" color={headingColor}>
                      CPU{i}
                    </Text>
                  </Checkbox>
                ))}
              </SimpleGrid>
            </Box>
          </VStack>
        </ModalBody>
        <ModalFooter gap={3}>
          <Button variant="ghost" onClick={onClose}>
            {t("gameProcessOptimize.affinitySettings.cancel")}
          </Button>
          <Button
            bg={getActiveColor()}
            color="white"
            _hover={{ bg: getActiveColor(), opacity: 0.9 }}
            onClick={handleSave}
          >
            {t("gameProcessOptimize.affinitySettings.save")}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

// 带搜索的进程选择弹窗
function ProcessPickerModal({
  isOpen,
  onClose,
  processes,
  onPick,
  icons,
}: {
  isOpen: boolean;
  onClose: () => void;
  processes: RunningProcessInfo[];
  onPick: (p: RunningProcessInfo) => void;
  icons: Record<string, string>;
}) {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const modalBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const headingColor = useColorModeValue("gray.900", "#ffffff");

  const [query, setQuery] = useState("");

  // 弹窗关闭时清空搜索词，避免下次打开还残留
  useEffect(() => {
    if (!isOpen) {
      setQuery("");
    }
  }, [isOpen]);

  const filtered = query
    ? processes.filter(p => p.name.toLowerCase().includes(query.toLowerCase()))
    : processes;

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="lg">
      <ModalOverlay backdropFilter="blur(4px)" />
      <ModalContent bg={modalBg} borderColor={borderColor} borderRadius="3xl" maxH="70vh">
        <ModalHeader color={headingColor}>
          {t("gameProcessOptimize.fromProcess")}
          <Text fontSize="xs" color="gray.500" fontWeight="normal" mt={1}>
            {t("gameProcessOptimize.fromProcessDesc")}
          </Text>
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody pb={4}>
          <InputGroup mb={3} size="sm">
            <InputLeftElement pointerEvents="none">
              <Search size={14} color="gray.500" />
            </InputLeftElement>
            <Input
              placeholder={t("gameProcessOptimize.searchProcess")}
              value={query}
              onChange={e => setQuery(e.target.value)}
            />
          </InputGroup>
          <Box maxH="45vh" overflowY="auto" sx={{
            "&::-webkit-scrollbar": { width: "4px" },
            "&::-webkit-scrollbar-thumb": { bg: "gray.600", borderRadius: "full" },
          }}>
            {filtered.length === 0 ? (
              <Text fontSize="sm" color="gray.500" textAlign="center" py={8}>
                {t("gameProcessOptimize.noResults")}
              </Text>
            ) : (
              <VStack align="stretch" spacing={1}>
                {filtered.map(p => (
                  <Box
                    key={`${p.pid}-${p.name}`}
                    px={3}
                    py={2}
                    borderRadius="lg"
                    cursor="pointer"
                    _hover={{ bg: getActiveColor(), opacity: 0.15 }}
                    onClick={() => { onPick(p); onClose(); }}
                  >
                    <HStack justify="space-between">
                      <HStack spacing={2.5} minW="0">
                        <Box
                          w={7}
                          h={7}
                          borderRadius="md"
                          bg={icons[p.exe_path] ? "transparent" : `${getActiveColor()}15`}
                          display="flex"
                          alignItems="center"
                          justifyContent="center"
                          flexShrink={0}
                          overflow="hidden"
                        >
                          {icons[p.exe_path] ? (
                            <img
                              src={icons[p.exe_path]}
                              alt=""
                              style={{ width: "100%", height: "100%", objectFit: "contain" }}
                              loading="lazy"
                            />
                          ) : (
                            <MonitorCog size={14} color={getActiveColor()} />
                          )}
                        </Box>
                        <Text fontSize="sm" color={headingColor} fontWeight="medium" isTruncated>
                          {p.name}
                        </Text>
                      </HStack>
                      <Text fontSize="2xs" color="gray.500">
                        {p.memory_mb.toFixed(0)} MB
                      </Text>
                    </HStack>
                  </Box>
                ))}
              </VStack>
            )}
          </Box>
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}

// 带搜索的滤镜游戏名单选择弹窗
function FilterPickerModal({
  isOpen,
  onClose,
  games,
  onPick,
}: {
  isOpen: boolean;
  onClose: () => void;
  games: FilterGameEntry[];
  onPick: (g: FilterGameEntry) => void;
}) {
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const modalBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const headingColor = useColorModeValue("gray.900", "#ffffff");

  const [query, setQuery] = useState("");

  // 弹窗关闭时清空搜索词，避免下次打开还残留
  useEffect(() => {
    if (!isOpen) {
      setQuery("");
    }
  }, [isOpen]);

  const filtered = query
    ? games.filter(g => g.name.toLowerCase().includes(query.toLowerCase()) || g.process_names.some(n => n.toLowerCase().includes(query.toLowerCase())))
    : games;

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="lg">
      <ModalOverlay backdropFilter="blur(4px)" />
      <ModalContent bg={modalBg} borderColor={borderColor} borderRadius="3xl" maxH="70vh">
        <ModalHeader color={headingColor}>
          {t("gameProcessOptimize.fromFilterList")}
          <Text fontSize="xs" color="gray.500" fontWeight="normal" mt={1}>
            {t("gameProcessOptimize.fromFilterListDesc")}
          </Text>
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody pb={4}>
          <InputGroup mb={3} size="sm">
            <InputLeftElement pointerEvents="none">
              <Search size={14} color="gray.500" />
            </InputLeftElement>
            <Input
              placeholder={t("gameProcessOptimize.searchGame")}
              value={query}
              onChange={e => setQuery(e.target.value)}
            />
          </InputGroup>
          <Box maxH="45vh" overflowY="auto" sx={{
            "&::-webkit-scrollbar": { width: "4px" },
            "&::-webkit-scrollbar-thumb": { bg: "gray.600", borderRadius: "full" },
          }}>
            {filtered.length === 0 ? (
              <Text fontSize="sm" color="gray.500" textAlign="center" py={8}>
                {t("gameProcessOptimize.noResults")}
              </Text>
            ) : (
              <VStack align="stretch" spacing={1}>
                {filtered.map(g => (
                  <Box
                    key={g.id}
                    px={3}
                    py={2}
                    borderRadius="lg"
                    cursor="pointer"
                    _hover={{ bg: getActiveColor(), opacity: 0.15 }}
                    onClick={() => { onPick(g); onClose(); }}
                  >
                    <HStack justify="space-between">
                      <Text fontSize="sm" color={headingColor} fontWeight="medium">
                        {g.name}
                      </Text>
                      <Badge colorScheme={g.is_builtin ? "gray" : "purple"} variant="subtle" fontSize="2xs">
                        {g.process_names.length} proc
                      </Badge>
                    </HStack>
                  </Box>
                ))}
              </VStack>
            )}
          </Box>
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}

// 单个游戏卡片
function GameCard({
  game,
  status,
  isAdmin,
  onApplyAffinity,
  onOpenCoreSettings,
  onToggleAuto,
  onDelete,
  busy,
}: {
  game: GameOptimizeConfig;
  status: GameAutoStatus | undefined;
  isAdmin: boolean;
  onApplyAffinity: (id: string) => void;
  onOpenCoreSettings: (id: string) => void;
  onToggleAuto: (id: string, kind: AutoKind, checked: boolean) => void;
  onDelete: (id: string) => void;
  busy: { affinity: boolean };
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#a1a1aa");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const rowBg = useColorModeValue("gray.50", "#1a1a1a");
  const { getActiveColor } = useThemeColor();

  const running = status?.running ?? false;
  const priorityApplied = status?.priority_applied ?? false;
  const affinityApplied = status?.affinity_applied ?? false;
  const ioApplied = status?.io_applied ?? false;
  const gpuApplied = status?.gpu_applied ?? false;
  const memoryApplied = status?.memory_applied ?? false;
  const gpuPrefApplied = status?.gpu_pref_applied ?? false;
  const ifeoApplied = status?.ifeo_applied ?? false;

  // 头像：添加时提取的 exe 图标优先，其次内置资产映射（如预置三角洲），最后通用手柄兜底
  const builtinIcon = game.process_names
    .map(n => BUILTIN_GAME_ICON_ASSETS[n.toLowerCase().replace(/\.exe$/i, "")])
    .find(Boolean);
  const displayIcon = game.icon || builtinIcon;

  // 内嵌小卡片：液态玻璃开启时用嵌套玻璃卡（圆角比外层卡小一档：2xl），关闭时用实底色（DiskHealthPage 模式）
  const cell = (children: React.ReactNode) =>
    liquidGlassEnabled ? (
      <LiquidGlassCard w="full" p={2.5} borderRadius="2xl" className="no-bounce">{children}</LiquidGlassCard>
    ) : (
      <Box w="full" p={2.5} borderRadius="lg" bg={rowBg} border="1px solid" borderColor={borderColor}>
        {children}
      </Box>
    );

  const row = (icon: React.ReactNode, title: React.ReactNode, desc: React.ReactNode, action: React.ReactNode) =>
    cell(
      <HStack align="center" spacing={3} flexWrap="wrap">
        <Box w={8} h={8} borderRadius="md" bg={`${getActiveColor()}15`} display="flex" alignItems="center" justifyContent="center" color={getActiveColor()} flexShrink={0}>
          {icon}
        </Box>
        <VStack align="flex-start" spacing={0} flex={1} minW="160px">
          <Text fontSize="sm" fontWeight="bold" color={textColor}>
            {title}
          </Text>
          <Text fontSize="xs" color={subTextColor}>
            {desc}
          </Text>
        </VStack>
        {action}
      </HStack>
    );

  // 自动优化开关：小卡片（图标 + 标题 + 已应用徽章 + 开关），两列网格
  const autoSwitchRow = (
    icon: React.ReactNode,
    title: string,
    checked: boolean,
    applied: boolean,
    kind: AutoKind,
    disabled: boolean,
  ) =>
    cell(
      <HStack justify="space-between" align="center" spacing={2} w="full">
        <HStack spacing={2} flex={1} minW="0">
          <Box w={6} h={6} borderRadius="md" bg={`${getActiveColor()}15`} display="flex" alignItems="center" justifyContent="center" color={getActiveColor()} flexShrink={0}>
            {icon}
          </Box>
          <HStack spacing={1} flex={1} minW="0">
            <Text fontSize="xs" fontWeight="bold" color={textColor} isTruncated>
              {title}
            </Text>
            {applied && (
              <Badge colorScheme="green" variant="subtle" fontSize="2xs" px={1.5} py={0} borderRadius="full" whiteSpace="nowrap" flexShrink={0}>
                {t("gameProcessOptimize.status.applied")}
              </Badge>
            )}
          </HStack>
        </HStack>
        <Switch
          isChecked={checked}
          onChange={e => onToggleAuto(game.id, kind, e.target.checked)}
          isDisabled={disabled}
          size="sm"
          sx={{
            "& > span": { bg: checked ? getActiveColor() : useColorModeValue("gray.200", "gray.600") },
            "& > span > span": { bg: "white" },
            "&:hover > span": { bg: checked ? getActiveColor() : useColorModeValue("gray.200", "gray.600") },
          }}
          aria-label={title}
        />
      </HStack>
    );

  const content = (
    <VStack align="stretch" spacing={4}>
      <HStack justify="space-between" align="flex-start">
        <HStack spacing={3} align="center" minW="0">
          <Box
            w={10} h={10}
            borderRadius="lg"
            bg={`${getActiveColor()}18`}
            display="flex"
            alignItems="center"
            justifyContent="center"
            color={getActiveColor()}
            flexShrink={0}
            overflow="hidden"
          >
            {displayIcon ? (
              <img
                src={displayIcon}
                alt={game.name}
                draggable={false}
                style={{ width: 26, height: 26, objectFit: "contain" }}
              />
            ) : (
              <Gamepad2 size={20} />
            )}
          </Box>
          <VStack align="flex-start" spacing={1} minW="0">
            <HStack spacing={2} flexWrap="wrap">
              <Text fontWeight="bold" fontSize="md" color={textColor}>
                {game.name}
              </Text>
              {running && (
                <Badge colorScheme="green" variant="subtle" fontSize="2xs" px={2} py={0.5} borderRadius="full">
                  {t("gameProcessOptimize.status.running")}
                </Badge>
              )}
              {(priorityApplied || affinityApplied || ioApplied || gpuApplied || memoryApplied || gpuPrefApplied) && (
                <Badge colorScheme="blue" variant="subtle" fontSize="2xs" px={2} py={0.5} borderRadius="full">
                  {t("gameProcessOptimize.status.optimized")}
                </Badge>
              )}
            </HStack>
            <HStack spacing={1} wrap="wrap">
              {game.process_names.map(p => (
                <Badge key={p} variant="outline" fontSize="2xs" px={2} py={0.5} borderRadius="full" color={getActiveColor()}>
                  {p}
                </Badge>
              ))}
            </HStack>
          </VStack>
        </HStack>
        <IconButton
          aria-label={t("gameProcessOptimize.delete")}
          icon={<Trash2 size={16} />}
          size="sm"
          variant="ghost"
          colorScheme="red"
          onClick={() => onDelete(game.id)}
        />
      </HStack>

      <VStack align="stretch" spacing={2}>
        {row(
          <Cpu size={16} />,
          t("gameProcessOptimize.affinityOptimize.title"),
          t("gameProcessOptimize.affinityOptimize.description"),
          <HStack spacing={2}>
            <Tooltip label={t("gameProcessOptimize.affinitySettings.title")}>
              <IconButton
                aria-label={t("gameProcessOptimize.affinitySettings.title")}
                icon={<Settings2 size={16} />}
                size="sm"
                variant="outline"
                color={getActiveColor()}
                borderColor={getActiveColor()}
                _hover={{ bg: getActiveColor(), color: "white", opacity: 0.9 }}
                onClick={() => onOpenCoreSettings(game.id)}
              />
            </Tooltip>
            <Button
              size="sm"
              variant="outline"
              color={getActiveColor()}
              borderColor={getActiveColor()}
              _hover={{ bg: getActiveColor(), color: "white", opacity: 0.9 }}
              onClick={() => onApplyAffinity(game.id)}
              isLoading={busy.affinity}
              loadingText=""
              px={4}
              borderRadius="lg"
              minW="72px"
            >
              {t("gameProcessOptimize.apply")}
            </Button>
          </HStack>
        )}
      </VStack>

      {/* 自动优化开关：每项一张小卡片，两列网格（IFEO 开启即写注册表、关闭即清除，无需手动应用/恢复） */}
      <SimpleGrid columns={2} spacing={2}>
        {autoSwitchRow(
          <Zap size={13} />,
          t("gameProcessOptimize.autoOptimize.priority"),
          game.auto_optimize_priority,
          priorityApplied,
          "priority",
          !isAdmin,
        )}
        {autoSwitchRow(
          <Cpu size={13} />,
          t("gameProcessOptimize.autoOptimize.affinity"),
          game.auto_optimize_affinity,
          affinityApplied,
          "affinity",
          !isAdmin,
        )}
        {autoSwitchRow(
          <HardDrive size={13} />,
          t("gameProcessOptimize.autoOptimize.io"),
          game.auto_optimize_io,
          ioApplied,
          "io",
          !isAdmin,
        )}
        {autoSwitchRow(
          <Gauge size={13} />,
          t("gameProcessOptimize.autoOptimize.gpu"),
          game.auto_optimize_gpu,
          gpuApplied,
          "gpu",
          !isAdmin,
        )}
        {autoSwitchRow(
          <MemoryStick size={13} />,
          t("gameProcessOptimize.autoOptimize.memory"),
          game.auto_optimize_memory,
          memoryApplied,
          "memory",
          !isAdmin,
        )}
        {/* GPU 高性能走 HKCU 图形偏好注册表，无需管理员，不随权限禁用 */}
        {autoSwitchRow(
          <Rocket size={13} />,
          t("gameProcessOptimize.autoOptimize.gpuPref"),
          game.auto_optimize_gpu_pref,
          gpuPrefApplied,
          "gpu_pref",
          false,
        )}
        {/* IFEO 写 HKLM 注册表，需要管理员；开启即应用、关闭即恢复 */}
        {autoSwitchRow(
          <ShieldCheck size={13} />,
          t("gameProcessOptimize.autoOptimize.ifeo"),
          game.auto_optimize_ifeo,
          ifeoApplied,
          "ifeo",
          !isAdmin,
        )}
      </SimpleGrid>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return <LiquidGlassCard p={4} className="no-bounce">{content}</LiquidGlassCard>;
  }
  return (
    <Box p={4} borderRadius="xl" bg={rowBg} border="1px solid" borderColor={borderColor}>
      {content}
    </Box>
  );
}

export default function GameProcessOptimizePage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("gamepad");
  const { icons, ensureIcons } = useProcessIcons();

  const { liquidGlassEnabled } = useBackground();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const cardBg = useColorModeValue("white", "#111111");
  const borderColorVal = useColorModeValue("gray.200", "#333333");
  const { getActiveColor } = useThemeColor();

  const [configs, setConfigs] = useState<GameOptimizeConfig[]>([]);
  const [autoStatuses, setAutoStatuses] = useState<Record<string, GameAutoStatus>>({});
  const [isAdmin, setIsAdmin] = useState(true);
  const [coreCount, setCoreCount] = useState(4);
  const [busy, setBusy] = useState<Record<string, { affinity: boolean }>>({});
  // 大小核拓扑：是否有 E 核 + 后端实际使用的默认掩码
  const [topology, setTopology] = useState<{ has_e_cores: boolean; default_mask: number }>({ has_e_cores: false, default_mask: 0 });

  // 核心选择弹窗
  const [coreTarget, setCoreTarget] = useState<string | null>(null);
  const { isOpen: isCoreOpen, onOpen: onCoreOpen, onClose: onCoreClose } = useDisclosure();

  // 进程/名单选择弹窗
  const { isOpen: isProcOpen, onOpen: onProcOpen, onClose: onProcClose } = useDisclosure();
  const { isOpen: isFilterOpen, onOpen: onFilterOpen, onClose: onFilterClose } = useDisclosure();
  const [processes, setProcesses] = useState<RunningProcessInfo[]>([]);
  const [filterGames, setFilterGames] = useState<FilterGameEntry[]>([]);

  const lastAutoRef = useRef<Record<string, boolean>>({});
  const isMountedRef = useRef(true);

  const getDefaultMask = useCallback((cores: number) => cores > 0 ? Math.pow(2, cores) - 2 : 0, []);

  // 初始化：加载配置、核心数、管理员状态
  useEffect(() => {
    isMountedRef.current = true;
    setCoreCount(navigator.hardwareConcurrency || 4);
    invoke<boolean>("check_game_optimize_admin").then(setIsAdmin).catch(() => setIsAdmin(true));
    invoke<{ has_e_cores: boolean; default_mask: number }>("get_affinity_topology")
      .then(tp => { if (isMountedRef.current) setTopology(tp); })
      .catch(e => console.error("加载 CPU 拓扑信息失败:", e));
    invoke<GameOptimizeConfig[]>("get_game_optimize_configs")
      .then(cfg => setConfigs(cfg))
      .catch(e => console.error("加载游戏优化配置失败:", e));
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  // 3s 轮询自动优化状态
  useEffect(() => {
    const loadStatus = async () => {
      try {
        const list = await invoke<GameAutoStatus[]>("get_game_auto_optimize_status");
        if (!isMountedRef.current) return;
        const map: Record<string, GameAutoStatus> = {};
        for (const s of list) map[s.game_id] = s;
        setAutoStatuses(map);
      } catch (e) {
        console.error("加载自动优化状态失败:", e);
      }
    };
    loadStatus();
    const interval = window.setInterval(loadStatus, 3000);
    return () => window.clearInterval(interval);
  }, []);

  const persist = useCallback(async (next: GameOptimizeConfig[]) => {
    setConfigs(next);
    await invoke("save_game_optimize_configs", { configs: next });
  }, []);

  const runBusy = useCallback((id: string, key: "affinity", val: boolean) => {
    setBusy(prev => ({ ...prev, [id]: { ...prev[id], [key]: val } }));
  }, []);

  const handleApplyAffinity = useCallback(async (id: string) => {
    runBusy(id, "affinity", true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("optimize_game_affinity", { gameId: id, mask: null });
      toast({ title: result.message, status: result.success ? "success" : "info", duration: 2000 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    } finally {
      runBusy(id, "affinity", false);
    }
  }, [toast, runBusy]);

  const handleOpenCoreSettings = useCallback((id: string) => {
    setCoreTarget(id);
    onCoreOpen();
  }, [onCoreOpen]);

  const handleSaveCoreConfig = useCallback(async (mask: number) => {
    if (!coreTarget) return;
    const game = configs.find(c => c.id === coreTarget);
    if (!game) return;
    // 若选择的是后端默认掩码（有 E 核则排除 E 核，否则全核心）则存 null，由后端运行时计算
    const defaultMask = topology.default_mask || getDefaultMask(coreCount);
    const normalized: number | null = mask === defaultMask ? null : mask;
    await persist(configs.map(c => c.id === coreTarget ? { ...c, affinity_mask: normalized } : c));
    toast({ title: t("gameProcessOptimize.affinitySettings.configSaved"), status: "success", duration: 1500 });
  }, [configs, coreTarget, persist, getDefaultMask, coreCount, toast, t, topology]);

  // 独立开关：进程/核心/IO/内存/GPU 调度/GPU 高性能
  const handleToggleAuto = useCallback(async (id: string, kind: AutoKind, checked: boolean) => {
    const key = `${id}:${kind}`;
    if (lastAutoRef.current[key] === checked) return;
    lastAutoRef.current[key] = checked;
    try {
      await invoke("set_game_auto_optimize", { gameId: id, kind, enabled: checked });
      setConfigs(prev => prev.map(c =>
        c.id === id
          ? kind === "ifeo"
            ? { ...c, auto_optimize_ifeo: checked }
            : { ...c, [AUTO_KIND_FIELD[kind]]: checked }
          : c
      ));
      toast({
        title: checked
          ? t("gameProcessOptimize.autoOptimize.enabled")
          : t("gameProcessOptimize.autoOptimize.disabled"),
        status: "success",
        duration: 2000,
      });
    } catch (e: any) {
      lastAutoRef.current[key] = !checked;
      toast({ title: String(e), status: "error", duration: 2000 });
    }
  }, [toast, t]);

  const handleDelete = useCallback(async (id: string) => {
    const game = configs.find(c => c.id === id);
    if (!game) return;
    // 不弹确认框（用户要求）：误删代价低，重新添加即可
    await persist(configs.filter(c => c.id !== id));
    toast({ title: t("gameProcessOptimize.gameDeleted"), status: "success", duration: 1500 });
  }, [configs, persist, toast, t]);

  // 去重：新进程名与现有游戏任一进程名相同（忽略大小写、.exe 后缀）则视为重复
  const isDuplicateGame = useCallback((processNames: string[]) => {
    const normalize = (s: string) => s.trim().replace(/\.exe$/i, "").toLowerCase();
    const existing = new Set(configs.flatMap(c => c.process_names.map(normalize)));
    return processNames.some(n => existing.has(normalize(n)));
  }, [configs]);

  // 三种添加方式
  const handleAddFromExe = useCallback(async () => {
    try {
      const info = await invoke<GameExecutableInfo | null>("select_game_executable");
      if (!info) return;
      if (isDuplicateGame([info.process_name])) {
        toast({ title: t("gameProcessOptimize.gameExists"), status: "warning", duration: 2000 });
        return;
      }
      await persist([
        ...configs,
        { ...EMPTY_CONFIG(`game_${Date.now()}`, info.game_name, [info.process_name]), icon: info.icon },
      ]);
      toast({ title: t("gameProcessOptimize.gameAdded"), status: "success", duration: 1500 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    }
  }, [configs, persist, toast, t, isDuplicateGame]);

  const handleOpenProcessPicker = useCallback(async () => {
    try {
      const list = await invoke<RunningProcessInfo[]>("list_running_processes");
      setProcesses(list);
      ensureIcons(list.map(p => p.exe_path));
      onProcOpen();
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    }
  }, [onProcOpen, toast, ensureIcons]);

  const handlePickProcess = useCallback(async (p: RunningProcessInfo) => {
    if (isDuplicateGame([p.name])) {
      toast({ title: t("gameProcessOptimize.gameExists"), status: "warning", duration: 2000 });
      return;
    }
    await persist([
      ...configs,
      // 进程弹窗打开时已批量提取图标，直接复用（取不到留空走兜底）
      { ...EMPTY_CONFIG(`game_${Date.now()}`, p.name, [p.name]), icon: icons[p.exe_path] ?? "" },
    ]);
    toast({ title: t("gameProcessOptimize.gameAdded"), status: "success", duration: 1500 });
  }, [configs, persist, toast, t, isDuplicateGame, icons]);

  const handleOpenFilterPicker = useCallback(async () => {
    try {
      const status = await invoke<FilterStatus>("get_game_filter_status");
      setFilterGames(status.games || []);
      onFilterOpen();
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    }
  }, [onFilterOpen, toast]);

  const handlePickFilterGame = useCallback(async (g: FilterGameEntry) => {
    const processNames = g.process_names.length > 0 ? g.process_names : [g.name];
    if (isDuplicateGame(processNames)) {
      toast({ title: t("gameProcessOptimize.gameExists"), status: "warning", duration: 2000 });
      return;
    }
    await persist([
      ...configs,
      EMPTY_CONFIG(`game_${Date.now()}`, g.name, processNames),
    ]);
    toast({ title: t("gameProcessOptimize.gameAdded"), status: "success", duration: 1500 });
  }, [configs, persist, toast, t, isDuplicateGame]);

  const addEntryCard = (
    title: string,
    desc: string,
    icon: React.ReactNode,
    onClick: () => void,
  ) => {
    const content = (
      <HStack spacing={3} align="center" cursor="pointer" onClick={onClick} _hover={{ opacity: 0.85 }} transition="opacity 0.15s">
        <Box w={10} h={10} borderRadius="lg" bg={`${getActiveColor()}15`} display="flex" alignItems="center" justifyContent="center" color={getActiveColor()} flexShrink={0}>
          {icon}
        </Box>
        <VStack align="flex-start" spacing={0}>
          <Text fontWeight="bold" fontSize="sm" color={headingColor}>
            {title}
          </Text>
          <Text fontSize="xs" color="gray.500">
            {desc}
          </Text>
        </VStack>
      </HStack>
    );
    if (liquidGlassEnabled) {
      // no-bounce：跳过路由切换时的卡片弹跳动画；不加 hover 位移（用户不要上浮效果）
      return <LiquidGlassCard p={4} className="no-bounce">{content}</LiquidGlassCard>;
    }
    return (
      <Box p={4} borderRadius="3xl" bg={cardBg} border="1px solid" borderColor={borderColorVal}>
        {content}
      </Box>
    );
  };

  return (
    <Box pt={8} pb={8}>
      <PageHeader title={t("gameProcessOptimize.title")} mb={6}>
        {!isAdmin && (
          <Badge colorScheme="orange" variant="subtle" px={3} py={1} borderRadius="full" fontSize="xs">
            <HStack spacing={1}>
              <ShieldAlert size={14} />
              <Text>{t("gameProcessOptimize.needsAdmin")}</Text>
            </HStack>
          </Badge>
        )}
      </PageHeader>

      {/* 添加游戏区 */}
      <SimpleGrid columns={{ base: 1, md: 3 }} spacing={4} mb={6}>
        {addEntryCard(
          t("gameProcessOptimize.fromExe"),
          t("gameProcessOptimize.fromExeDesc"),
          <FileSearch size={20} />,
          handleAddFromExe,
        )}
        {addEntryCard(
          t("gameProcessOptimize.fromProcess"),
          t("gameProcessOptimize.fromProcessDesc"),
          <MonitorCog size={20} />,
          handleOpenProcessPicker,
        )}
        {addEntryCard(
          t("gameProcessOptimize.fromFilterList"),
          t("gameProcessOptimize.fromFilterListDesc"),
          <ListFilter size={20} />,
          handleOpenFilterPicker,
        )}
      </SimpleGrid>

      {/* 游戏列表 */}
      {configs.length === 0 ? (
        <LiquidGlassCard p={10} isDashed className="no-bounce">
          <VStack align="center" spacing={2}>
            <Gamepad2 size={32} color="gray.500" />
            <Text fontWeight="bold" color={headingColor}>
              {t("gameProcessOptimize.emptyTitle")}
            </Text>
            <Text fontSize="sm" color="gray.500">
              {t("gameProcessOptimize.emptyDesc")}
            </Text>
          </VStack>
        </LiquidGlassCard>
      ) : (
        <SimpleGrid columns={2} spacing={5}>
          {configs.map(game => (
            <GameCard
              key={game.id}
              game={game}
              status={autoStatuses[game.id]}
              isAdmin={isAdmin}
              onApplyAffinity={handleApplyAffinity}
              onOpenCoreSettings={handleOpenCoreSettings}
              onToggleAuto={handleToggleAuto}
              onDelete={handleDelete}
              busy={busy[game.id] ?? { affinity: false }}
            />
          ))}
        </SimpleGrid>
      )}

      <CoreSelectionModal
        isOpen={isCoreOpen}
        onClose={onCoreClose}
        coreCount={coreCount}
        currentSavedMask={
          coreTarget
            ? (configs.find(c => c.id === coreTarget)?.affinity_mask ?? (topology.default_mask || getDefaultMask(coreCount)))
            : (topology.default_mask || getDefaultMask(coreCount))
        }
        onSave={handleSaveCoreConfig}
      />
      <ProcessPickerModal
        isOpen={isProcOpen}
        onClose={onProcClose}
        processes={processes}
        onPick={handlePickProcess}
        icons={icons}
      />
      <FilterPickerModal
        isOpen={isFilterOpen}
        onClose={onFilterClose}
        games={filterGames}
        onPick={handlePickFilterGame}
      />
    </Box>
  );
}
