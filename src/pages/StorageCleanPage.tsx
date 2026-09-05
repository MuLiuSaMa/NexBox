import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Badge,
  Checkbox,
  Button,
  useColorModeValue,
  Spinner,
  SimpleGrid,
  Icon,
  Progress,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel,
  Tooltip,
  Menu,
  MenuButton,
  MenuList,
  MenuItemOption,
  MenuOptionGroup,
  AlertDialog,
  AlertDialogOverlay,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogBody,
  AlertDialogFooter,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { motion } from "framer-motion";
import { useTransitionMode, getVariants, getTransitionConfig } from "@/components/ui/animated-page";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback, useRef, memo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Trash2,
  RefreshCw,
  ArrowLeft,
  HardDrive,
  FileText,
  Image,
  AlertTriangle,
  Database,
  Folder,
  File,
  ScanSearch,
  XCircle,
  FolderSearch,
  ShieldCheck,
  Zap,
  Check,
  ChevronDown,
  FolderOpen,
  Trash,
  Download,
} from "lucide-react";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useNavigate } from "react-router-dom";
import { hexToRgba } from "@/lib/color-utils";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

// ============================================================================
// Types
// ============================================================================

interface CleanItem {
  id: string;
  name: string;
  path: string;
  exists: boolean;
  size_bytes: number;
  requires_admin: boolean;
  description: string;
}

interface QuickScanResult {
  items: CleanItem[];
  total_size: number;
  total_items: number;
}

interface CleanResult {
  success: boolean;
  message: string;
  freed_bytes: number;
  skipped_files: string[];
}

interface JunkFileInfo {
  path: string;
  name: string;
  size: number;
  original_path?: string | null;
  modified_time: number;
  is_dir: boolean;
  category: string;
}

interface RegistryItemInfo {
  key_path: string;
  value_name?: string | null;
  description: string;
}

interface CategoryScanResult {
  category: string;
  display_name: string;
  description: string;
  risk_level: number;
  files: JunkFileInfo[];
  registry_items: RegistryItemInfo[];
  default_select: boolean;
  total_size: number;
  file_count: number;
}

interface JunkScanResult {
  categories: CategoryScanResult[];
  total_size: number;
  total_file_count: number;
  scan_duration_ms: number;
  scan_timestamp: number;
}

interface JunkDeleteResult {
  success_count: number;
  failed_count: number;
  reboot_pending_count: number;
  freed_size: number;
  needs_reboot: boolean;
  failed_files: { path: string; reason: string }[];
}

/** 删除目标:携带扫描时已知的文件大小,避免删除阶段重复 stat */
interface DeleteJunkTarget {
  path: string;
  size: number | null;
  /** 注册表残留目标(深度清理);为 true 时 path 是 "HIVE\\子键" 路径 */
  is_registry?: boolean;
  /** 注册表目标要删除的值名;省略表示删除整个键 */
  value_name?: string;
}

/** Winapp2 规则库信息(仿照图吧工具箱的规则库卡片) */
interface Winapp2RuleInfo {
  version: string;
  entry_count: number;
  file_size_bytes: number;
  is_bundled: boolean;
  effective_path: string;
}

interface LargeFileEntry {
  path: string;
  size: number;
  modified: number;
  risk_level: number;
  source_label: string;
}

interface LargeFileScanProgress {
  current_path: string;
  scanned_count: number;
  found_count: number;
  backend: string;
  stage: string;
  message: string;
  elapsed_ms: number;
}

// ============================================================================
// Helpers
// ============================================================================

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatTime(timestamp: number): string {
  if (!timestamp) return "--";
  const date = new Date(timestamp * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours()
  )}:${pad(date.getMinutes())}`;
}

function getRiskColor(level: number): string {
  switch (level) {
    case 1:
      return "green";
    case 2:
      return "teal";
    case 3:
      return "yellow";
    case 4:
      return "orange";
    case 5:
      return "red";
    default:
      return "gray";
  }
}

// ============================================================================
// 快速清理 - 单项卡片 (memo 优化:切换勾选时只重渲染受影响的卡片)
// ============================================================================

function getItemIcon(id: string) {
  switch (id) {
    case "temp_user":
    case "temp_system":
      return FileText;
    case "recycle_bin":
      return Trash2;
    case "thumbnail_cache":
      return Image;
    case "prefetch":
      return Database;
    case "wer_archive":
    case "wer_queue":
    case "crash_dumps":
    case "memory_dmp":
    case "minidump":
      return AlertTriangle;
    case "windows_logs":
      return FileText;
    case "thumbs_db":
      return Image;
    default:
      return Folder;
  }
}

interface CleanItemCardProps {
  item: CleanItem;
  isSelected: boolean;
  onToggleSelect: (id: string) => void;
  primaryColor: string;
}

const CleanItemCard = memo(function CleanItemCard({
  item,
  isSelected,
  onToggleSelect,
  primaryColor,
}: CleanItemCardProps) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const pathColor = useColorModeValue("gray.400", "#666666");
  const hoverBorderColor = liquidGlassEnabled
    ? "rgba(255,255,255,0.5)"
    : useColorModeValue("gray.300", "#444444");
  const IconComponent = getItemIcon(item.id);

  const hasContent = item.exists && item.size_bytes > 0;

  return (
    <LiquidGlassCard
      borderRadius="lg"
      p={4}
      cursor={hasContent ? "pointer" : "not-allowed"}
      onClick={hasContent ? () => onToggleSelect(item.id) : undefined}
      transition="all 0.2s ease"
      _hover={
        hasContent && !isSelected
          ? {
              borderColor: hoverBorderColor,
            }
          : undefined
      }
      opacity={hasContent ? 1 : 0.45}
      {...(isSelected ? { borderColor: primaryColor } : {})}
    >
      <Flex justify="space-between" align="start">
        <HStack spacing={3}>
          <Checkbox
            isChecked={isSelected}
            onChange={(e) => {
              // 阻止事件冒泡到卡片 onClick,避免复选框被双重切换
              e.stopPropagation();
              onToggleSelect(item.id);
            }}
            isDisabled={!hasContent}
            sx={{
              "& .chakra-checkbox__control": {
                borderColor: isSelected ? primaryColor : undefined,
              },
              "& .chakra-checkbox__control[data-checked]": {
                bg: primaryColor,
                borderColor: primaryColor,
                color: "white",
              },
            }}
          />
          <Box
            w={10}
            h={10}
            borderRadius="lg"
            bg={hasContent ? `${primaryColor}20` : "gray.50"}
            display="flex"
            alignItems="center"
            justifyContent="center"
            color={hasContent ? primaryColor : "gray.400"}
          >
            <IconComponent size={20} />
          </Box>
          <VStack align="start" spacing={0}>
            <Text fontSize="md" fontWeight="semibold" color={headingColor}>
              {t(`storageClean.items.${item.id}.name`)}
            </Text>
            <Text fontSize="xs" color={pathColor}>
              {item.path}
            </Text>
          </VStack>
        </HStack>
        <VStack align="end" spacing={1}>
          {item.requires_admin && (
            <Badge size="sm" colorScheme="orange" variant="subtle" fontSize="xs">
              {t("storageClean.adminRequired")}
            </Badge>
          )}
          <Badge
            size="sm"
            variant="subtle"
            fontSize="xs"
            bg={hasContent ? `${primaryColor}20` : undefined}
            color={hasContent ? primaryColor : undefined}
          >
            {formatSize(item.size_bytes)}
          </Badge>
        </VStack>
      </Flex>
      <Text fontSize="xs" color={descColor} mt={2}>
        {t(`storageClean.items.${item.id}.description`)}
      </Text>
    </LiquidGlassCard>
  );
});

// ============================================================================
// 垃圾清理 - 分类卡片 (memo 优化 + 文件列表仅展开时渲染)
// ============================================================================

interface JunkCategoryCardProps {
  category: CategoryScanResult;
  isSelected: boolean;
  isExpanded: boolean;
  onToggleSelect: (name: string) => void;
  onToggleExpand: (name: string) => void;
  primaryColor: string;
}

const JunkCategoryCard = memo(function JunkCategoryCard({
  category,
  isSelected,
  isExpanded,
  onToggleSelect,
  onToggleExpand,
  primaryColor,
}: JunkCategoryCardProps) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const pathColor = useColorModeValue("gray.400", "#666666");
  const rowBg = useColorModeValue("#f7fafc", "#232323");
  const dividerColor = useColorModeValue("gray.200", "#333333");

  const hasContent = category.file_count > 0 || category.registry_items.length > 0;

  // 展开时一次性渲染全部文件会导致界面卡死(如 WindowsTemp 可能上万条)。
  // 每次只渲染前 100 条,点击「加载更多」再追加。
  const PAGE_SIZE = 100;
  const [fileLimit, setFileLimit] = useState(PAGE_SIZE);
  useEffect(() => {
    setFileLimit(PAGE_SIZE);
  }, [category.display_name]);

  const visibleFiles = category.files.slice(0, fileLimit);
  const hasMoreFiles = category.files.length > fileLimit;

  return (
    <LiquidGlassCard
      borderRadius="lg"
      overflow="hidden"
      transition="all 0.2s ease"
      {...(isSelected ? { borderColor: primaryColor } : {})}
    >
      <Flex justify="space-between" align="start" p={4}>
        <HStack spacing={3} align="start">
          <Checkbox
            isChecked={isSelected}
            onChange={(e) => {
              e.stopPropagation();
              onToggleSelect(category.display_name);
            }}
            isDisabled={!hasContent}
            mt={1}
            sx={{
              "& .chakra-checkbox__control": {
                borderColor: isSelected ? primaryColor : undefined,
              },
              "& .chakra-checkbox__control[data-checked]": {
                bg: primaryColor,
                borderColor: primaryColor,
                color: "white",
              },
            }}
          />
          <VStack align="start" spacing={1}>
            <HStack spacing={2}>
              <Text fontSize="md" fontWeight="semibold" color={headingColor}>
                {category.display_name}
              </Text>
              <Badge
                size="sm"
                colorScheme={getRiskColor(category.risk_level)}
                variant="subtle"
                fontSize="xs"
              >
                {t(`storageClean.risk${category.risk_level}`)}
              </Badge>
            </HStack>
            <Text fontSize="xs" color={descColor}>
              {category.description}
            </Text>
            {hasContent && (
              <Button
                size="xs"
                variant="ghost"
                color={primaryColor}
                mt={1}
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleExpand(category.display_name);
                }}
              >
                {isExpanded
                  ? t("storageClean.junkHideFiles")
                  : `${t("storageClean.junkViewFiles")} (${category.file_count + category.registry_items.length})`}
              </Button>
            )}
          </VStack>
        </HStack>
        <VStack align="end" spacing={1}>
          <Badge
            size="sm"
            variant="subtle"
            fontSize="xs"
            bg={hasContent ? `${primaryColor}20` : undefined}
            color={hasContent ? primaryColor : undefined}
          >
            {formatSize(category.total_size)}
          </Badge>
          <Text fontSize="xs" color={pathColor}>
            {hasContent
              ? t("storageClean.junkFiles", { count: category.file_count })
              : "0 B"}
          </Text>
        </VStack>
      </Flex>

      {/* 仅展开时才挂载文件列表,避免大量 DOM 常驻导致勾选/切换卡顿 */}
      {isExpanded && (
        <Box maxH="320px" overflowY="auto" borderTop="1px solid" borderColor={dividerColor}>
          {category.registry_items.length > 0 && (
            <Box px={4} py={2}>
              <Text fontSize="xs" fontWeight="semibold" color={primaryColor} mb={1}>
                {t("storageClean.registryItemLabel")} ({category.registry_items.length})
              </Text>
              {category.registry_items.map((item, index) => (
                <Flex
                  key={`reg-${item.key_path}-${index}`}
                  justify="space-between"
                  align="center"
                  bg={index % 2 === 0 ? rowBg : "transparent"}
                  px={2}
                  py={1}
                >
                  <Tooltip label={item.key_path}>
                    <Text fontSize="xs" color={headingColor} isTruncated w="100%">
                      {item.key_path}
                      {item.value_name ? `  [${item.value_name}]` : ""}
                    </Text>
                  </Tooltip>
                  <Text fontSize="10px" color={pathColor} ml={3} flexShrink={0}>
                    {item.description}
                  </Text>
                </Flex>
              ))}
            </Box>
          )}
          {visibleFiles.map((file, index) => (
            <Flex
              key={`${file.path}-${index}`}
              justify="space-between"
              align="center"
              bg={index % 2 === 0 ? rowBg : "transparent"}
              px={4}
              py={1.5}
            >
              <VStack align="start" spacing={0} minW={0} flex={1}>
                <Tooltip label={file.path}>
                  <Text fontSize="xs" color={headingColor} isTruncated w="100%">
                    {file.path}
                  </Text>
                </Tooltip>
                <Text fontSize="10px" color={pathColor}>
                  {file.original_path
                    ? `原位置: ${file.original_path}`
                    : formatTime(file.modified_time)}
                </Text>
              </VStack>
              <Text fontSize="xs" color={headingColor} ml={3} flexShrink={0}>
                {formatSize(file.size)}
              </Text>
            </Flex>
          ))}
          {hasMoreFiles && (
            <Button
              size="xs"
              variant="ghost"
              color={primaryColor}
              w="full"
              onClick={() => setFileLimit((limit) => limit + PAGE_SIZE)}
            >
              {t("storageClean.junkLoadMore", {
                count: Math.min(category.files.length - fileLimit, PAGE_SIZE),
              })}
            </Button>
          )}
        </Box>
      )}
    </LiquidGlassCard>
  );
});

// ============================================================================
// 大文件扫描结果表格(memo 化:删除/跳转操作时表格不重渲染,避免卡顿)
// ============================================================================

interface BigFilesTableProps {
  files: LargeFileEntry[];
  revealingPath: string | null;
  onReveal: (path: string) => void;
  onDelete: (file: LargeFileEntry) => void;
  headingColor: string;
  subTextColor: string;
  themeColorHex: string;
  themeColorRgba: (opacity: number) => string;
}

const BigFilesTable = memo(function BigFilesTable({
  files,
  revealingPath,
  onReveal,
  onDelete,
  headingColor,
  subTextColor,
  themeColorHex,
  themeColorRgba,
}: BigFilesTableProps) {
  const { t } = useTranslation();

  return (
    <LiquidGlassCard borderRadius="xl" overflow="hidden">
      <HStack justify="space-between" px={4} py={3}>
        <Text fontSize="sm" fontWeight="bold" color={headingColor}>
          {t("storageClean.bigResults")}
        </Text>
        <Badge
          variant="subtle"
          fontSize="xs"
          bg={themeColorRgba(0.15)}
          color={themeColorHex}
        >
          {t("storageClean.bigFound", { count: files.length })}
        </Badge>
      </HStack>
      <Box maxH="480px" overflowY="auto">
        <Table size="sm" variant="simple">
          <Thead>
            <Tr>
              <Th>#</Th>
              <Th>{t("storageClean.bigPath")}</Th>
              <Th isNumeric>{t("storageClean.bigSize")}</Th>
              <Th>{t("storageClean.bigSource")}</Th>
              <Th>{t("storageClean.bigRisk")}</Th>
              <Th>{t("storageClean.bigAction")}</Th>
            </Tr>
          </Thead>
          <Tbody>
            {files.map((file, index) => (
              <Tr key={file.path}>
                <Td fontSize="xs" color={subTextColor}>
                  {index + 1}
                </Td>
                <Td fontSize="xs" maxW="300px">
                  <Tooltip label={file.path}>
                    <Text isTruncated color={headingColor}>
                      {file.path}
                    </Text>
                  </Tooltip>
                </Td>
                <Td fontSize="xs" isNumeric fontWeight="semibold" color={headingColor}>
                  {formatSize(file.size)}
                </Td>
                <Td fontSize="xs" color={subTextColor}>
                  {file.source_label}
                </Td>
                <Td>
                  <Badge
                    size="sm"
                    colorScheme={getRiskColor(file.risk_level)}
                    variant="subtle"
                    fontSize="xs"
                  >
                    {t(`storageClean.risk${file.risk_level}`)}
                  </Badge>
                </Td>
                <Td>
                  <HStack spacing={1}>
                    <Tooltip label={t("storageClean.bigReveal")}>
                      <Button
                        size="xs"
                        variant="ghost"
                        p={1}
                        minW="auto"
                        color={themeColorHex}
                        isLoading={revealingPath === file.path}
                        onClick={() => onReveal(file.path)}
                      >
                        <FolderOpen size={14} />
                      </Button>
                    </Tooltip>
                    <Tooltip label={t("storageClean.bigDelete")}>
                      <Button
                        size="xs"
                        variant="ghost"
                        p={1}
                        minW="auto"
                        color="red.400"
                        _hover={{ bg: "red.50", color: "red.500" }}
                        _dark={{ _hover: { bg: "red.900", color: "red.300" } }}
                        onClick={() => onDelete(file)}
                      >
                        <Trash size={14} />
                      </Button>
                    </Tooltip>
                  </HStack>
                </Td>
              </Tr>
            ))}
          </Tbody>
        </Table>
      </Box>
    </LiquidGlassCard>
  );
});

// ============================================================================
// 主页面
// ============================================================================

export default function StorageCleanPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("disk");
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const navigate = useNavigate();

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  const themeColorHex = themeConfig.primaryColor;
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);

  // 使用提示框改为跟随主题色
  const tipTitleColor = themeConfig.primaryColor;
  const tipTextColor = useColorModeValue("gray.600", "rgba(200,200,200,0.85)");
  const selectBg = useColorModeValue("#ffffff", "#1e2024");

  const [tabIndex, setTabIndex] = useState(0);

  // ---------- 快速清理 ----------
  const [scanResult, setScanResult] = useState<QuickScanResult | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [isCleaning, setIsCleaning] = useState(false);
  const [selectedItems, setSelectedItems] = useState<Set<string>>(new Set());

  // ---------- 垃圾清理(深度清理) ----------
  const [junkResult, setJunkResult] = useState<JunkScanResult | null>(null);
  const [junkScanning, setJunkScanning] = useState(false);
  const [junkCleaning, setJunkCleaning] = useState(false);
  const [selectedCategories, setSelectedCategories] = useState<Set<string>>(new Set());
  const [expandedCategories, setExpandedCategories] = useState<Set<string>>(new Set());

  // ---------- Winapp2 规则库(仿照图吧工具箱) ----------
  const [ruleInfo, setRuleInfo] = useState<Winapp2RuleInfo | null>(null);
  const [updatingRules, setUpdatingRules] = useState(false);
  const [ruleUpdateMsg, setRuleUpdateMsg] = useState("");

  // ---------- 大文件扫描 ----------
  const [drives, setDrives] = useState<string[]>([]);
  const [selectedDrive, setSelectedDrive] = useState("");
  const [bigScanning, setBigScanning] = useState(false);
  const [bigProgress, setBigProgress] = useState<LargeFileScanProgress | null>(null);
  const [bigResults, setBigResults] = useState<LargeFileEntry[]>([]);

  // 扫描去重:进行中的请求复用同一 Promise;自动扫描仅在首次挂载时触发一次,
  // 避免 StrictMode 双执行/依赖重建导致对后端重复发起全量扫描
  const scanInFlight = useRef<Promise<void> | null>(null);
  const autoScanDone = useRef(false);

  const doScan = useCallback(async () => {
    if (scanInFlight.current) return scanInFlight.current;
    const run = (async () => {
      setIsScanning(true);
      try {
        const result = await invoke<QuickScanResult>("scan_storage_items");
        setScanResult(result);
        const defaultSelected = new Set(
          result.items
            .filter((item) => item.exists && item.size_bytes > 0 && !item.requires_admin)
            .map((item) => item.id)
        );
        setSelectedItems(defaultSelected);
      } catch (error) {
        console.error("Failed to scan storage items:", error);
        toast({
          title: t("storageClean.scanError") || "扫描失败",
          description: String(error),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      setIsScanning(false);
    })();
    scanInFlight.current = run;
    try {
      await run;
    } finally {
      scanInFlight.current = null;
    }
  }, [t, toast]);

  useEffect(() => {
    if (autoScanDone.current) return;
    autoScanDone.current = true;
    doScan();
  }, [doScan]);

  // 垃圾扫描同样做 in-flight 去重 + 自动只跑一次
  const junkScanInFlight = useRef<Promise<void> | null>(null);
  const autoJunkScanDone = useRef(false);

  const doJunkScan = useCallback(async () => {
    if (junkScanInFlight.current) return junkScanInFlight.current;
    const run = (async () => {
      setJunkScanning(true);
      try {
        const result = await invoke<JunkScanResult>("scan_junk_categories", {});
        setJunkResult(result);
        const defaultSelected = new Set(
          result.categories
            .filter(
              (category) =>
                (category.file_count > 0 || category.registry_items.length > 0) &&
                category.default_select
            )
            .map((category) => category.display_name)
        );
        setSelectedCategories(defaultSelected);
      } catch (error) {
        console.error("Failed to scan junk files:", error);
        toast({
          title: t("storageClean.scanError") || "扫描失败",
          description: String(error),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      setJunkScanning(false);
    })();
    junkScanInFlight.current = run;
    try {
      await run;
    } finally {
      junkScanInFlight.current = null;
    }
  }, [t, toast]);

  // 进入页面自动扫描垃圾文件,无需手动点击
  useEffect(() => {
    if (autoJunkScanDone.current) return;
    autoJunkScanDone.current = true;
    doJunkScan();
  }, [doJunkScan]);

  // ---------- 稳定回调(memo 卡片依赖,避免每次渲染重建) ----------
  const handleToggleItem = useCallback((id: string) => {
    setSelectedItems((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  const handleToggleJunkCategory = useCallback((name: string) => {
    setSelectedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  const handleToggleJunkExpand = useCallback((name: string) => {
    setExpandedCategories((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }, []);

  const handleSelectAllQuick = useCallback(() => {
    if (scanResult) {
      const allIds = scanResult.items
        .filter((item) => item.exists && item.size_bytes > 0)
        .map((item) => item.id);
      setSelectedItems(new Set(allIds));
    }
  }, [scanResult]);

  const handleDeselectAllQuick = useCallback(() => {
    setSelectedItems(new Set());
  }, []);

  const handleJunkSelectAll = useCallback(() => {
    if (junkResult) {
      setSelectedCategories(
        new Set(junkResult.categories.filter((c) => c.file_count > 0).map((c) => c.display_name))
      );
    }
  }, [junkResult]);

  const handleJunkDeselectAll = useCallback(() => {
    setSelectedCategories(new Set());
  }, []);

  const handleClean = async () => {
    if (selectedItems.size === 0) {
      toast({
        title: t("storageClean.noItemSelected"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    setIsCleaning(true);
    try {
      const result = await invoke<CleanResult>("clean_storage_items", {
        itemIds: Array.from(selectedItems),
      });

      if (result.success) {
        toast({
          title: t("storageClean.cleanSuccess", { size: formatSize(result.freed_bytes) }),
          description:
            result.skipped_files.length > 0
              ? t("storageClean.skippedFiles", { count: result.skipped_files.length })
              : undefined,
          status: "success",
          duration: 4000,
          isClosable: true,
        });
      } else {
        toast({
          title: t("storageClean.cleanError"),
          description: result.message,
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to clean storage items:", error);
      toast({
        title: t("storageClean.cleanError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setIsCleaning(false);

    await doScan();
  };

  const handleJunkClean = async () => {
    if (!junkResult || selectedCategories.size === 0) {
      toast({
        title: t("storageClean.junkNoSelection"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    const targets: DeleteJunkTarget[] = [];
    for (const category of junkResult.categories) {
      if (selectedCategories.has(category.display_name)) {
        for (const file of category.files) {
          targets.push({ path: file.path, size: file.size });
        }
      }
    }
    if (targets.length === 0) {
      toast({
        title: t("storageClean.junkNoSelection"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }

    setJunkCleaning(true);
    try {
      const result = await invoke<JunkDeleteResult>("delete_junk_files", { targets });
      const parts: string[] = [];
      if (result.freed_size > 0) {
        parts.push(t("storageClean.junkCleanSuccess", { size: formatSize(result.freed_size) }));
      }
      if (result.reboot_pending_count > 0) {
        parts.push(t("storageClean.junkRebootPending", { count: result.reboot_pending_count }));
      }
      toast({
        title: parts.join("，") || t("storageClean.cleanSuccess", { size: "0 B" }),
        description:
          result.failed_count > 0
            ? t("storageClean.junkCleanFailed", { count: result.failed_count })
            : undefined,
        status: result.failed_count > 0 ? "warning" : "success",
        duration: 4000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to delete junk files:", error);
      toast({
        title: t("storageClean.cleanError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setJunkCleaning(false);

    await doJunkScan();
  };

  // ---------- Winapp2 规则库更新逻辑(仿照图吧工具箱) ----------
  // 挂载时拉取当前规则库信息
  useEffect(() => {
    invoke<Winapp2RuleInfo>("get_winapp2_rule_info")
      .then(setRuleInfo)
      .catch((error) => console.error("Failed to get rule info:", error));
  }, []);

  // 监听规则库更新进度事件
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    const setup = async () => {
      unlisten = await listen<{ message: string }>("winapp2-update:progress", (event) => {
        setRuleUpdateMsg(event.payload.message);
      });
    };
    setup();
    return () => {
      unlisten?.();
    };
  }, []);

  const handleUpdateRules = async () => {
    setUpdatingRules(true);
    setRuleUpdateMsg("");
    const oldVersion = ruleInfo?.version;
    try {
      const result = await invoke<Winapp2RuleInfo>("update_winapp2_rules");
      setRuleInfo(result);
      toast({
        title:
          result.version === oldVersion && !result.is_bundled
            ? t("storageClean.ruleDbLatest")
            : t("storageClean.ruleDbUpdated", { version: result.version || "--" }),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
      await doJunkScan();
    } catch (error) {
      console.error("Failed to update rule database:", error);
      toast({
        title: t("storageClean.ruleDbUpdateFailed"),
        description: String(error),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    }
    setUpdatingRules(false);
    setRuleUpdateMsg("");
  };

  // ---------- 大文件扫描逻辑 ----------
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const result = await invoke<string[]>("get_drive_list");
        if (!cancelled) {
          setDrives(result);
          if (result.length > 0) setSelectedDrive(result[0]);
        }
      } catch (error) {
        console.error("Failed to get drive list:", error);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenCancelled: UnlistenFn | undefined;

    const setup = async () => {
      const p = await listen<LargeFileScanProgress>("large-file-scan:progress", (event) => {
        if (!cancelled) setBigProgress(event.payload);
      });
      const c = await listen("large-file-scan:cancelled", () => {
        if (!cancelled) {
          setBigScanning(false);
          toast({
            title: t("storageClean.bigCancelTip"),
            status: "info",
            duration: 2000,
            isClosable: true,
          });
        }
      });
      if (cancelled) {
        p();
        c();
      } else {
        unlistenProgress = p;
        unlistenCancelled = c;
      }
    };
    setup();

    return () => {
      cancelled = true;
      unlistenProgress?.();
      unlistenCancelled?.();
    };
  }, [t, toast]);

  const handleBigScan = async () => {
    if (!selectedDrive) {
      toast({
        title: t("storageClean.bigDriveRequired"),
        status: "warning",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    setBigScanning(true);
    setBigProgress(null);
    setBigResults([]);
    try {
      const result = await invoke<LargeFileEntry[]>("scan_large_files", {
        driveLetter: selectedDrive,
      });
      setBigResults(result);
    } catch (error) {
      console.error("Failed to scan large files:", error);
      toast({
        title: t("storageClean.bigScanError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setBigScanning(false);
  };

  const handleBigCancel = async () => {
    try {
      await invoke("cancel_large_file_scan");
    } catch (error) {
      console.error("Failed to cancel large file scan:", error);
    }
  };

  // 大文件操作:跳转到文件位置
  const [revealingPath, setRevealingPath] = useState<string | null>(null);
  const handleRevealFile = useCallback(async (path: string) => {
    setRevealingPath(path);
    try {
      await invoke("reveal_large_file", { path });
    } catch (error) {
      console.error("Failed to reveal file:", error);
      toast({
        title: t("storageClean.bigRevealError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setRevealingPath(null);
  }, [t, toast]);

  // 大文件操作:强制删除(需弹窗确认)
  const [deleteTarget, setDeleteTarget] = useState<LargeFileEntry | null>(null);
  const [deleting, setDeleting] = useState(false);
  const cancelDeleteRef = useRef<HTMLButtonElement>(null);
  const handleDeleteFile = useCallback((file: LargeFileEntry) => {
    setDeleteTarget(file);
  }, []);
  const handleConfirmDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      const result = await invoke<JunkDeleteResult>("delete_large_file", {
        paths: [deleteTarget.path],
      });
      if (result.freed_size > 0) {
        toast({
          title: t("storageClean.bigDeleteSuccess", {
            size: formatSize(result.freed_size),
          }),
          status: "success",
          duration: 3000,
          isClosable: true,
        });
      } else if (result.failed_count > 0) {
        toast({
          title: t("storageClean.bigDeleteFailed"),
          description:
            result.failed_files.map((f) => f.reason).join("; ") || String(result.failed_count),
          status: "error",
          duration: 4000,
          isClosable: true,
        });
      }
      // 从结果列表中移除已删除的文件
      setBigResults((prev) => prev.filter((f) => f.path !== deleteTarget.path));
    } catch (error) {
      console.error("Failed to delete file:", error);
      toast({
        title: t("storageClean.bigDeleteError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setDeleting(false);
    setDeleteTarget(null);
  };

  // 隐藏无内容(0B)的卡片:快速清理项 / 垃圾分类
  const quickItems = scanResult
    ? scanResult.items.filter((item) => item.exists && item.size_bytes > 0)
    : [];
  const junkCategories = junkResult
    ? junkResult.categories.filter((c) => c.file_count > 0)
    : [];

  const selectedSize = scanResult
    ? scanResult.items
        .filter((item) => selectedItems.has(item.id))
        .reduce((sum, item) => sum + item.size_bytes, 0)
    : 0;

  const junkSelectedSize = junkResult
    ? junkResult.categories
        .filter((c) => selectedCategories.has(c.display_name))
        .reduce((sum, c) => sum + c.total_size, 0)
    : 0;

  const transitionMode = useTransitionMode();

  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
      <HStack justifyContent="space-between" alignItems="center" w="full" position="relative">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/optimization")}
          color={headingColor}
        >
          返回
        </Button>
        <Heading
          position="absolute"
          left="50%"
          transform="translateX(-50%)"
          size="lg"
          color={adaptiveTitle.text}
          textShadow={adaptiveTitle.shadow}
          fontWeight="700"
          whiteSpace="nowrap"
        >
          {t("storageClean.title")}
        </Heading>
        {/* 快速清理主操作按钮固定右上角,不再放在底部 */}
        {tabIndex === 0 && (
          <HStack spacing={2}>
            <LiquidGlassButton
              size="sm"
              leftIcon={isCleaning ? <Spinner size="sm" /> : <Trash2 size={15} />}
              onClick={handleClean}
              isLoading={isCleaning}
              loadingText={t("storageClean.cleaning")}
              disabled={isScanning || selectedItems.size === 0}
              bg={themeConfig.primaryColor}
              color={getContrastTextColor()}
              _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
              _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
            >
              {t("storageClean.cleanButton")}
            </LiquidGlassButton>
            <Button
              size="sm"
              variant="outline"
              leftIcon={<RefreshCw size={15} />}
              onClick={doScan}
              isLoading={isScanning}
              borderColor={themeConfig.primaryColor}
              color={themeConfig.primaryColor}
            >
              {t("storageClean.scanButton")}
            </Button>
          </HStack>
        )}
        {/* 深度清理(垃圾清理)主操作按钮固定右上角,不再放在底部 */}
        {tabIndex === 1 && (
          <HStack spacing={2}>
            <LiquidGlassButton
              size="sm"
              leftIcon={junkCleaning ? <Spinner size="sm" /> : <Trash2 size={15} />}
              onClick={handleJunkClean}
              isLoading={junkCleaning}
              loadingText={t("storageClean.junkCleaning")}
              disabled={junkScanning || selectedCategories.size === 0}
              bg={themeConfig.primaryColor}
              color={getContrastTextColor()}
              _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
              _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
            >
              {t("storageClean.junkCleanButton")}
            </LiquidGlassButton>
            <Button
              size="sm"
              variant="outline"
              leftIcon={<ScanSearch size={15} />}
              onClick={doJunkScan}
              isLoading={junkScanning}
              borderColor={themeConfig.primaryColor}
              color={themeConfig.primaryColor}
            >
              {t("storageClean.junkScanButton")}
            </Button>
          </HStack>
        )}
      </HStack>

      <Tabs variant="soft-rounded" index={tabIndex} onChange={setTabIndex} isLazy>
        <TabList gap={2} mb={4}>
          <Tab
            _selected={{
              bg: themeColorHex,
              color: getContrastTextColor(),
              boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
            }}
            _hover={{ bg: themeColorRgba(0.15) }}
            borderRadius="full"
            fontWeight="600"
            fontSize="sm"
            px={5}
            py={1.5}
          >
            <Zap size={15} style={{ marginRight: 6 }} />
            {t("storageClean.tabQuick")}
          </Tab>
          <Tab
            _selected={{
              bg: themeColorHex,
              color: getContrastTextColor(),
              boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
            }}
            _hover={{ bg: themeColorRgba(0.15) }}
            borderRadius="full"
            fontWeight="600"
            fontSize="sm"
            px={5}
            py={1.5}
          >
            <Trash2 size={15} style={{ marginRight: 6 }} />
            {t("storageClean.tabJunk")}
          </Tab>
          <Tab
            _selected={{
              bg: themeColorHex,
              color: getContrastTextColor(),
              boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
            }}
            _hover={{ bg: themeColorRgba(0.15) }}
            borderRadius="full"
            fontWeight="600"
            fontSize="sm"
            px={5}
            py={1.5}
          >
            <FolderSearch size={15} style={{ marginRight: 6 }} />
            {t("storageClean.tabBigFiles")}
          </Tab>
        </TabList>

        <TabPanels>
          {/* ================= 快速清理 ================= */}
          <TabPanel px={0} pt={0}>
            <VStack align="stretch" spacing={6}>
              {scanResult && (
                <LiquidGlassCard
                  p={4}
                  borderRadius="xl">
                  <SimpleGrid columns={3} spacing={4}>
                    <VStack align="center">
                      <Icon as={HardDrive} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.totalScanned")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {formatSize(scanResult.total_size)}
                      </Text>
                    </VStack>
                    <VStack align="center">
                      <Icon as={Folder} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.itemsFound")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {scanResult.total_items}
                      </Text>
                    </VStack>
                    <VStack align="center">
                      <Icon as={Trash2} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.selectedSize")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {formatSize(selectedSize)}
                      </Text>
                    </VStack>
                  </SimpleGrid>
                </LiquidGlassCard>
              )}

              <HStack spacing={2}>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={handleSelectAllQuick}
                  borderColor={themeConfig.primaryColor}
                  color={themeConfig.primaryColor}
                >
                  {t("storageClean.selectAll")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={handleDeselectAllQuick}
                  color={themeConfig.primaryColor}
                >
                  {t("storageClean.deselectAll")}
                </Button>
              </HStack>

              {isScanning ? (
                <VStack py={8}>
                  <Spinner size="lg" color={themeColorHex} />
                  <Text color={subTextColor}>{t("storageClean.scanning")}</Text>
                </VStack>
              ) : quickItems.length > 0 ? (
                <SimpleGrid columns={{ base: 1, md: 2 }} spacing={3}>
                  {quickItems.map((item) => (
                    <CleanItemCard
                      key={item.id}
                      item={item}
                      isSelected={selectedItems.has(item.id)}
                      onToggleSelect={handleToggleItem}
                      primaryColor={themeConfig.primaryColor}
                    />
                  ))}
                </SimpleGrid>
              ) : (
                !isScanning && (
                  <VStack py={8} spacing={3}>
                    <Trash2 size={40} color={subTextColor} />
                    <Text color={subTextColor}>{t("storageClean.junkEmpty")}</Text>
                  </VStack>
                )
              )}
            </VStack>
          </TabPanel>

          {/* ================= 深度清理 ================= */}
          <TabPanel px={0} pt={0}>
            <VStack align="stretch" spacing={6}>
              {junkResult && (
                <LiquidGlassCard
                  p={4}
                  borderRadius="xl">
                  <SimpleGrid columns={3} spacing={4}>
                    <VStack align="center">
                      <Icon as={Trash2} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.totalScanned")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {formatSize(junkResult.total_size)}
                      </Text>
                    </VStack>
                    <VStack align="center">
                      <Icon as={File} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.junkFileTotal")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {junkResult.total_file_count}
                      </Text>
                    </VStack>
                    <VStack align="center">
                      <Icon as={ShieldCheck} color={themeConfig.primaryColor} boxSize={6} />
                      <Text fontSize="sm" color={subTextColor}>
                        {t("storageClean.selectedSize")}
                      </Text>
                      <Text fontSize="lg" fontWeight="bold" color={headingColor}>
                        {formatSize(junkSelectedSize)}
                      </Text>
                    </VStack>
                  </SimpleGrid>
                </LiquidGlassCard>
              )}

              {/* 全选/取消全选 与 规则库更新 同行(规则库在右侧) */}
              <HStack justify="space-between" align="center" w="full" spacing={3} flexWrap="wrap">
                <HStack spacing={2}>
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={handleJunkSelectAll}
                    borderColor={themeConfig.primaryColor}
                    color={themeConfig.primaryColor}
                  >
                    {t("storageClean.selectAll")}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={handleJunkDeselectAll}
                    color={themeConfig.primaryColor}
                  >
                    {t("storageClean.deselectAll")}
                  </Button>
                </HStack>
                <HStack spacing={2} align="center">
                  <Text fontSize="xs" color={subTextColor}>
                    {ruleInfo
                      ? t("storageClean.ruleDbInfo", {
                          version: ruleInfo.version || "--",
                          count: ruleInfo.entry_count,
                          source: ruleInfo.is_bundled
                            ? t("storageClean.ruleDbSourceBundled")
                            : t("storageClean.ruleDbSourceUpdated"),
                        })
                      : t("storageClean.ruleDbFetching")}
                  </Text>
                  <Button
                    size="sm"
                    leftIcon={updatingRules ? <Spinner size="sm" /> : <Download size={14} />}
                    onClick={handleUpdateRules}
                    isLoading={updatingRules}
                    loadingText={t("storageClean.ruleDbUpdating")}
                    isDisabled={!ruleInfo}
                    borderColor={themeConfig.primaryColor}
                    color={themeConfig.primaryColor}
                    variant="outline"
                  >
                    {t("storageClean.ruleDbUpdateButton")}
                  </Button>
                </HStack>
              </HStack>
              {ruleUpdateMsg && (
                <Text fontSize="10px" color={themeConfig.primaryColor} alignSelf="flex-end">
                  {ruleUpdateMsg}
                </Text>
              )}

              {junkScanning ? (
                <VStack py={8}>
                  <Spinner size="lg" color={themeColorHex} />
                  <Text color={subTextColor}>{t("storageClean.junkScanning")}</Text>
                </VStack>
              ) : junkCategories.length > 0 ? (
                <SimpleGrid columns={{ base: 1, md: 2 }} spacing={3}>
                  {junkCategories.map((category) => (
                    <JunkCategoryCard
                      key={category.display_name}
                      category={category}
                      isSelected={selectedCategories.has(category.display_name)}
                      isExpanded={expandedCategories.has(category.display_name)}
                      onToggleSelect={handleToggleJunkCategory}
                      onToggleExpand={handleToggleJunkExpand}
                      primaryColor={themeConfig.primaryColor}
                    />
                  ))}
                </SimpleGrid>
              ) : (
                !junkScanning && (
                  <VStack py={8} spacing={3}>
                    <ScanSearch size={40} color={subTextColor} />
                    <Text color={subTextColor}>{t("storageClean.junkEmpty")}</Text>
                  </VStack>
                )
              )}
            </VStack>
          </TabPanel>

          {/* ================= 大文件扫描 ================= */}
          <TabPanel px={0} pt={0}>
            <VStack align="stretch" spacing={6}>
              <LiquidGlassCard
                p={4}
                borderRadius="xl">
                {/* 磁盘选择 + 开始扫描 同一行,按钮紧贴选择框右侧 */}
                <HStack spacing={3} alignItems="end" flexWrap="wrap">
                  <VStack align="start" spacing={2}>
                    <Text fontSize="sm" color={subTextColor}>
                      {t("storageClean.bigDrive")}
                    </Text>
                    {/* 磁盘选择:ChakraUI Menu,带磁碟图标 + 单选对勾 + 主题色 */}
                    <Menu isLazy>
                      <MenuButton
                        as={Button}
                        size="sm"
                        w="150px"
                        leftIcon={<HardDrive size={15} color={themeColorHex} />}
                        rightIcon={<ChevronDown size={14} color={subTextColor} />}
                        isDisabled={bigScanning || drives.length === 0}
                        bg={selectBg}
                        border="1px solid"
                        borderColor={themeColorRgba(0.35)}
                        borderRadius="lg"
                        textAlign="left"
                        fontWeight="medium"
                        color={headingColor}
                        _hover={{ borderColor: themeColorHex, bg: themeColorRgba(0.08) }}
                        _active={{ bg: themeColorRgba(0.15) }}
                      >
                        {selectedDrive || "--"}
                      </MenuButton>
                      <MenuList
                        w="150px"
                        minW="150px"
                        bg={selectBg}
                        borderColor={themeColorRgba(0.3)}
                        boxShadow={`0 8px 24px -6px ${themeColorRgba(0.4)}`}
                        maxH="260px"
                        overflowY="auto"
                      >
                        <MenuOptionGroup
                          type="radio"
                          value={selectedDrive}
                          onChange={(value) => setSelectedDrive(value as string)}
                        >
                          {drives.map((drive) => (
                            <MenuItemOption
                              key={drive}
                              value={drive}
                              icon={<Check size={14} color={themeColorHex} />}
                              _selected={{ bg: themeColorRgba(0.12), color: themeColorHex, fontWeight: "600" }}
                              _hover={{ bg: themeColorRgba(0.08) }}
                              fontSize="sm"
                            >
                              {drive}
                            </MenuItemOption>
                          ))}
                        </MenuOptionGroup>
                      </MenuList>
                    </Menu>
                  </VStack>
                  <HStack spacing={2}>
                    <Button
                      size="sm"
                      leftIcon={bigScanning ? <Spinner size="sm" /> : <ScanSearch size={15} />}
                      onClick={handleBigScan}
                      disabled={bigScanning || !selectedDrive}
                      bg={themeConfig.primaryColor}
                      color={getContrastTextColor()}
                      _hover={{ bg: themeConfig.primaryColor, filter: "brightness(0.9)" }}
                      _active={{ bg: themeConfig.primaryColor, filter: "brightness(0.8)" }}
                    >
                      {bigScanning ? t("storageClean.bigScanning") : t("storageClean.bigScanButton")}
                    </Button>
                    {bigScanning && (
                      <Button
                        size="sm"
                        leftIcon={<XCircle size={15} />}
                        onClick={handleBigCancel}
                        variant="outline"
                        colorScheme="red"
                      >
                        {t("storageClean.bigCancelButton")}
                      </Button>
                    )}
                  </HStack>
                </HStack>

                {bigProgress && bigScanning && (
                  <VStack align="stretch" spacing={2} mt={4}>
                    <HStack justify="space-between">
                      <Text fontSize="xs" color={subTextColor} isTruncated>
                        {bigProgress.current_path}
                      </Text>
                      <Text fontSize="xs" color={subTextColor} flexShrink={0}>
                        {t("storageClean.bigScanned", { count: bigProgress.scanned_count })}
                      </Text>
                    </HStack>
                    <Progress
                      size="xs"
                      value={bigProgress.found_count > 0 ? 100 : 35}
                      isIndeterminate={bigProgress.found_count === 0}
                      sx={{ "& > div": { bg: themeColorHex } }}
                      bg={themeColorRgba(0.15)}
                    />
                    <Text fontSize="xs" color={subTextColor}>
                      {t("storageClean.bigFound", { count: bigProgress.found_count })}
                    </Text>
                  </VStack>
                )}
              </LiquidGlassCard>

              {bigResults.length > 0 ? (
                <BigFilesTable
                  files={bigResults}
                  revealingPath={revealingPath}
                  onReveal={handleRevealFile}
                  onDelete={handleDeleteFile}
                  headingColor={headingColor}
                  subTextColor={subTextColor}
                  themeColorHex={themeColorHex}
                  themeColorRgba={themeColorRgba}
                />
              ) : (
                !bigScanning && (
                  <VStack py={8} spacing={3}>
                    <FolderSearch size={40} color={subTextColor} />
                    <Text color={subTextColor}>{t("storageClean.bigEmpty")}</Text>
                  </VStack>
                )
              )}
            </VStack>
          </TabPanel>
        </TabPanels>
      </Tabs>

      {/* 大文件强制删除确认弹窗 */}
      <AlertDialog
        isOpen={!!deleteTarget}
        onClose={() => {
          if (!deleting) setDeleteTarget(null);
        }}
        leastDestructiveRef={cancelDeleteRef}
        isCentered
      >
        <AlertDialogOverlay />
        <AlertDialogContent bg={selectBg} borderColor={themeColorRgba(0.3)}>
          <AlertDialogHeader fontSize="md" fontWeight="bold" color={headingColor}>
            {t("storageClean.bigDeleteConfirmTitle")}
          </AlertDialogHeader>
          <AlertDialogBody>
            <VStack align="start" spacing={2}>
              <Text fontSize="sm" color={subTextColor}>
                {t("storageClean.bigDeleteConfirmDesc")}
              </Text>
              {deleteTarget && (
                <Box
                  w="full"
                  p={2}
                  borderRadius="md"
                  bg={themeColorRgba(0.08)}
                  border="1px solid"
                  borderColor={themeColorRgba(0.25)}
                >
                  <Text fontSize="xs" color={headingColor} wordBreak="break-all">
                    {deleteTarget.path}
                  </Text>
                  <Text fontSize="xs" color={themeColorHex} mt={1} fontWeight="600">
                    {formatSize(deleteTarget.size)}
                  </Text>
                </Box>
              )}
              <Text fontSize="xs" color="red.400" fontWeight="500">
                {t("storageClean.bigDeleteConfirmWarn")}
              </Text>
            </VStack>
          </AlertDialogBody>
          <AlertDialogFooter>
            <Button
              ref={cancelDeleteRef}
              size="sm"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
              color={subTextColor}
            >
              {t("common.cancel")}
            </Button>
            <Button
              size="sm"
              ml={3}
              leftIcon={deleting ? <Spinner size="sm" /> : <Trash size={14} />}
              onClick={handleConfirmDelete}
              isLoading={deleting}
              loadingText={t("storageClean.bigDeleting")}
              bg="red.500"
              color="#fff"
              _hover={{ bg: "red.600" }}
              _active={{ bg: "red.700" }}
            >
              {t("storageClean.bigDeleteConfirmButton")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <LiquidGlassCard p={5} borderRadius="xl">
        <HStack mb={3}>
          <Text fontSize="sm" fontWeight="bold" color={tipTitleColor}>
            {t("storageClean.tip.title")}
          </Text>
        </HStack>
        <VStack align="start" spacing={2} pl={1}>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("storageClean.tip.description")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("storageClean.tip.note1")}
          </Text>
          <Text fontSize="xs" color={tipTextColor} lineHeight="tall">
            {t("storageClean.tip.note2")}
          </Text>
        </VStack>
      </LiquidGlassCard>
    </VStack>
  );

  return transitionMode !== "off" ? (
    <motion.div
      initial="initial"
      animate="enter"
      exit="exit"
      variants={getVariants(transitionMode)}
      transition={getTransitionConfig(transitionMode)}
    >
      {content}
    </motion.div>
  ) : (
    <div>{content}</div>
  );
}
