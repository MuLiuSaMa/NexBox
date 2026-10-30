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
  Play,
} from "lucide-react";
import { PageBackButton } from "@/components/special/page-back-button";
import { PageHeader } from "@/components/special/page-header";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";

interface GameOptimizeConfig {
  id: string;
  name: string;
  process_names: string[];
  priority: "realtime" | "high" | "abovenormal" | "normal" | "belownormal" | "idle";
  affinity_mask: number | null;
  auto_optimize_priority: boolean;
  auto_optimize_affinity: boolean;
  auto_optimize_ifeo: boolean;
}

interface GameAutoStatus {
  game_id: string;
  running: boolean;
  optimized: boolean;
  priority_applied: boolean;
  affinity_applied: boolean;
  ifeo_applied: boolean;
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

type AutoKind = "priority" | "affinity" | "ifeo";

const EMPTY_CONFIG = (id: string, name: string, process_names: string[]): GameOptimizeConfig => ({
  id,
  name,
  process_names,
  priority: "realtime",
  affinity_mask: null,
  auto_optimize_priority: false,
  auto_optimize_affinity: false,
  auto_optimize_ifeo: false,
});

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
  onApplyPriority,
  onApplyAffinity,
  onApplyIfeo,
  onRestoreIfeo,
  onOpenCoreSettings,
  onToggleAuto,
  onDelete,
  onOptimizeAll,
  busy,
}: {
  game: GameOptimizeConfig;
  status: GameAutoStatus | undefined;
  isAdmin: boolean;
  onApplyPriority: (id: string) => void;
  onApplyAffinity: (id: string) => void;
  onApplyIfeo: (id: string) => void;
  onRestoreIfeo: (id: string) => void;
  onOpenCoreSettings: (id: string) => void;
  onToggleAuto: (id: string, kind: AutoKind, checked: boolean) => void;
  onDelete: (id: string) => void;
  onOptimizeAll: (id: string) => void;
  busy: { priority: boolean; affinity: boolean; ifeo: boolean; ifeoRestore: boolean; all: boolean };
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
  const ifeoApplied = status?.ifeo_applied ?? false;

  const row = (icon: React.ReactNode, title: React.ReactNode, desc: React.ReactNode, action: React.ReactNode) => (
    <HStack align="center" spacing={3} p={3} borderRadius="lg" bg={rowBg} border="1px solid" borderColor={borderColor} flexWrap="wrap">
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

  const autoSwitchRow = (
    icon: React.ReactNode,
    title: string,
    checked: boolean,
    applied: boolean,
    kind: AutoKind,
  ) => (
    <HStack justify="space-between" align="center" spacing={2} w="full">
      <HStack spacing={2} flex={1} minW="0">
        <Box w={7} h={7} borderRadius="md" bg={`${getActiveColor()}15`} display="flex" alignItems="center" justifyContent="center" color={getActiveColor()} flexShrink={0}>
          {icon}
        </Box>
        <VStack align="flex-start" spacing={0} flex={1} minW="0">
          <HStack spacing={1} flexWrap="wrap">
            <Text fontWeight="bold" fontSize="xs" color={textColor} whiteSpace="nowrap">
              {title}
            </Text>
            {applied && (
              <Badge colorScheme="green" variant="subtle" fontSize="2xs" px={1.5} py={0} borderRadius="full" whiteSpace="nowrap">
                {t("gameProcessOptimize.status.applied")}
              </Badge>
            )}
          </HStack>
        </VStack>
      </HStack>
      <Switch
        isChecked={checked}
        onChange={e => onToggleAuto(game.id, kind, e.target.checked)}
        isDisabled={!isAdmin}
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
          >
            <Gamepad2 size={20} />
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
              {(priorityApplied || affinityApplied || ifeoApplied) && (
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
          <Zap size={16} />,
          t("gameProcessOptimize.processOptimize.title"),
          t("gameProcessOptimize.processOptimize.description"),
          <Button
            size="sm"
            bg={getActiveColor()}
            color="white"
            _hover={{ bg: getActiveColor(), opacity: 0.9 }}
            onClick={() => onApplyPriority(game.id)}
            isLoading={busy.priority}
            loadingText=""
            px={4}
            borderRadius="lg"
            minW="72px"
          >
            {t("gameProcessOptimize.apply")}
          </Button>
        )}
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
        {row(
          <ShieldCheck size={16} />,
          t("gameProcessOptimize.ifeoOptimize.title"),
          t("gameProcessOptimize.ifeoOptimize.description"),
          <HStack spacing={2}>
            <Button
              size="sm"
              variant="outline"
              color={getActiveColor()}
              borderColor={getActiveColor()}
              _hover={{ bg: getActiveColor(), color: "white", opacity: 0.9 }}
              onClick={() => onApplyIfeo(game.id)}
              isLoading={busy.ifeo}
              loadingText=""
              px={4}
              borderRadius="lg"
              minW="72px"
              disabled={!isAdmin}
            >
              {t("gameProcessOptimize.apply")}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              colorScheme="red"
              onClick={() => onRestoreIfeo(game.id)}
              isLoading={busy.ifeoRestore}
              loadingText=""
              px={4}
              borderRadius="lg"
              minW="72px"
              disabled={!isAdmin}
            >
              {t("gameProcessOptimize.restore")}
            </Button>
          </HStack>
        )}
      </VStack>

      <VStack
        align="stretch"
        spacing={3}
        p={3}
        borderRadius="lg"
        border="1px solid"
        borderColor={borderColor}
      >
        <HStack justify="space-between" align="center" flexWrap="wrap" gap={2}>
          <VStack align="flex-start" spacing={0}>
            <Text fontWeight="bold" fontSize="sm" color={textColor}>
              {t("gameProcessOptimize.autoOptimize.title")}
            </Text>
            <Text fontSize="xs" color={subTextColor}>
              {t("gameProcessOptimize.autoOptimize.description")}
            </Text>
          </VStack>
          <Button
            size="sm"
            variant="outline"
            color={getActiveColor()}
            borderColor={getActiveColor()}
            _hover={{ bg: getActiveColor(), color: "white", opacity: 0.9 }}
            onClick={() => onOptimizeAll(game.id)}
            isLoading={busy.all}
            loadingText=""
            px={3}
            borderRadius="lg"
            leftIcon={<Play size={12} />}
          >
            {t("gameProcessOptimize.optimizeNow")}
          </Button>
        </HStack>
        <Box borderTop="1px solid" borderColor={borderColor} pt={3}>
          <VStack align="stretch" spacing={2}>
            {autoSwitchRow(
              <Zap size={14} />,
              t("gameProcessOptimize.autoOptimize.priority"),
              game.auto_optimize_priority,
              priorityApplied,
              "priority",
            )}
            {autoSwitchRow(
              <Cpu size={14} />,
              t("gameProcessOptimize.autoOptimize.affinity"),
              game.auto_optimize_affinity,
              affinityApplied,
              "affinity",
            )}
          </VStack>
        </Box>
      </VStack>
    </VStack>
  );

  if (liquidGlassEnabled) {
    return <LiquidGlassCard p={4}>{content}</LiquidGlassCard>;
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
  const [busy, setBusy] = useState<Record<string, { priority: boolean; affinity: boolean; ifeo: boolean; ifeoRestore: boolean; all: boolean }>>({});
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

  const runBusy = useCallback((id: string, key: "priority" | "affinity" | "ifeo" | "ifeoRestore" | "all", val: boolean) => {
    setBusy(prev => ({ ...prev, [id]: { ...prev[id], [key]: val } }));
  }, []);

  const handleApplyPriority = useCallback(async (id: string) => {
    runBusy(id, "priority", true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("optimize_game_priority", { gameId: id });
      toast({ title: result.message, status: result.success ? "success" : "info", duration: 2000 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    } finally {
      runBusy(id, "priority", false);
    }
  }, [toast, runBusy]);

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

  const handleApplyIfeo = useCallback(async (id: string) => {
    runBusy(id, "ifeo", true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("apply_game_ifeo", { gameId: id });
      toast({ title: result.message, status: result.success ? "success" : "info", duration: 2000 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    } finally {
      runBusy(id, "ifeo", false);
    }
  }, [toast, runBusy]);

  const handleRestoreIfeo = useCallback(async (id: string) => {
    runBusy(id, "ifeoRestore", true);
    try {
      const result = await invoke<{ success: boolean; message: string }>("restore_game_ifeo", { gameId: id });
      toast({ title: result.message, status: result.success ? "success" : "info", duration: 2000 });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    } finally {
      runBusy(id, "ifeoRestore", false);
    }
  }, [toast, runBusy]);

  const handleOptimizeAll = useCallback(async (id: string) => {
    runBusy(id, "all", true);
    try {
      const pr = await invoke<{ success: boolean; message: string }>("optimize_game_priority", { gameId: id });
      const ar = await invoke<{ success: boolean; message: string }>("optimize_game_affinity", { gameId: id, mask: null });
      toast({
        title: pr.success || ar.success ? t("gameProcessOptimize.optimizeNowDone") : "优化失败",
        status: (pr.success || ar.success) ? "success" : "info",
        duration: 2500,
      });
    } catch (e: any) {
      toast({ title: String(e), status: "error", duration: 2000 });
    } finally {
      runBusy(id, "all", false);
    }
  }, [toast, runBusy, t]);

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

  // 独立开关：进程自动优化 / 核心自动优化
  const handleToggleAuto = useCallback(async (id: string, kind: AutoKind, checked: boolean) => {
    const key = `${id}:${kind}`;
    if (lastAutoRef.current[key] === checked) return;
    lastAutoRef.current[key] = checked;
    try {
      await invoke("set_game_auto_optimize", { gameId: id, kind, enabled: checked });
      setConfigs(prev => prev.map(c =>
        c.id === id
          ? kind === "priority"
            ? { ...c, auto_optimize_priority: checked }
            : kind === "affinity"
              ? { ...c, auto_optimize_affinity: checked }
              : { ...c, auto_optimize_ifeo: checked }
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
    if (!window.confirm(t("gameProcessOptimize.deleteConfirm", { name: game.name }))) return;
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
        EMPTY_CONFIG(`game_${Date.now()}`, info.game_name, [info.process_name]),
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
      EMPTY_CONFIG(`game_${Date.now()}`, p.name, [p.name]),
    ]);
    toast({ title: t("gameProcessOptimize.gameAdded"), status: "success", duration: 1500 });
  }, [configs, persist, toast, t, isDuplicateGame]);

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
      return <LiquidGlassCard p={4} _hover={{ transform: "translateY(-2px)" }} transition="transform 0.2s">{content}</LiquidGlassCard>;
    }
    return (
      <Box p={4} borderRadius="3xl" bg={cardBg} border="1px solid" borderColor={borderColorVal} _hover={{ transform: "translateY(-2px)" }} transition="transform 0.2s">
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
        <LiquidGlassCard p={10} isDashed>
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
              onApplyPriority={handleApplyPriority}
              onApplyAffinity={handleApplyAffinity}
              onApplyIfeo={handleApplyIfeo}
              onRestoreIfeo={handleRestoreIfeo}
              onOpenCoreSettings={handleOpenCoreSettings}
              onToggleAuto={handleToggleAuto}
              onDelete={handleDelete}
              onOptimizeAll={handleOptimizeAll}
              busy={busy[game.id] ?? { priority: false, affinity: false, ifeo: false, ifeoRestore: false, all: false }}
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
