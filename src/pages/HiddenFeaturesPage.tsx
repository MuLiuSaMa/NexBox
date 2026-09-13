import { memo, useCallback, useEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import {
  Alert,
  AlertDescription,
  AlertIcon,
  AlertDialog,
  AlertDialogBody,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogOverlay,
  Badge,
  Box,
  Button,
  Flex,
  Heading,
  HStack,
  IconButton,
  Input,
  InputGroup,
  InputLeftElement,
  Modal,
  ModalBody,
  ModalCloseButton,
  ModalContent,
  ModalFooter,
  ModalHeader,
  ModalOverlay,
  Spinner,
  Switch,
  Text,
  Tooltip,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  Ban,
  CheckCircle2,
  CircleDot,
  FlaskConical,
  RefreshCw,
  RotateCcw,
  Search,
  AlertTriangle,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";

interface FeatureFlagsStatus {
  supported: boolean;
  os_build: number;
  is_admin: boolean;
  boot_pending: boolean;
  dictionary_count: number;
  change_stamp: number;
}

interface FeatureFlagEntry {
  feature_id: number;
  name: string | null;
  priority: number;
  enabled_state: number;
  variant: number;
  variant_payload_kind: number;
  is_wexp: boolean;
  has_config: boolean;
}

type StoreKind = "runtime" | "boot";
type ActionKind = "enable" | "disable" | "reset";
type PendingAction = { entry: FeatureFlagEntry; kind: ActionKind };

const QUERY_LIMIT = 5000;
const PAGE_SIZE = 100;
const TIP_LS_KEY = "nexbox_hidden_features_tip_ack";

const PRIORITY_NAMES: Record<number, string> = {
  0: "ImageDefault",
  1: "EKB",
  2: "Safeguard",
  3: "EditionOverride",
  4: "Service",
  6: "Dynamic",
  8: "User",
  9: "Security",
  10: "UserPolicy",
  12: "Test",
  15: "ImageOverride",
};

function priorityColor(p: number, fallback: string): string {
  if (p === 8) return "#4A90E2";
  if (p === 4) return "#9B59B6";
  if (p === 15 || p === 0) return "#E67E22";
  if (p === 9) return "#E53E3E";
  return fallback;
}

/** 单行条目。memo 隔离:父组件任何状态变化(如输入框打字)都不重渲染列表 */
const EntryRow = memo(function EntryRow({
  entry,
  disabled,
  onAction,
}: {
  entry: FeatureFlagEntry;
  disabled: boolean;
  onAction: (entry: FeatureFlagEntry, kind: ActionKind) => void;
}) {
  const { t } = useTranslation();
  const labelColor = useColorModeValue("gray.700", "#e0e0e0");
  const subLabelColor = useColorModeValue("gray.500", "#969696");

  let stateBadge: ReactElement;
  if (!entry.has_config) {
    stateBadge = (
      <Badge colorScheme="gray" variant="subtle" borderRadius="full" px={2}>
        {t("hiddenFeatures.noConfig")}
      </Badge>
    );
  } else if (entry.enabled_state === 2) {
    stateBadge = (
      <Badge variant="subtle" borderRadius="full" px={2} color="#38A169" bg={hexToRgba("#38A169", 0.12)}>
        {t("hiddenFeatures.stateEnabled")}
      </Badge>
    );
  } else if (entry.enabled_state === 1) {
    stateBadge = (
      <Badge variant="subtle" borderRadius="full" px={2} color="#E53E3E" bg={hexToRgba("#E53E3E", 0.12)}>
        {t("hiddenFeatures.stateDisabled")}
      </Badge>
    );
  } else {
    stateBadge = (
      <Badge colorScheme="gray" variant="subtle" borderRadius="full" px={2}>
        {t("hiddenFeatures.stateDefault")}
      </Badge>
    );
  }

  const pc = priorityColor(entry.priority, subLabelColor);

  return (
    <Flex align="center" justify="space-between" gap={3} py={2.5} wrap="wrap">
      <Box flex="1" minW={{ base: "full", md: "260px" }}>
        <Text
          fontSize="sm"
          fontWeight={600}
          color={labelColor}
          noOfLines={1}
          title={entry.name ?? undefined}
        >
          {entry.name ?? entry.feature_id.toString()}
        </Text>
        <HStack spacing={2} mt={0.5}>
          <Text fontSize="xs" color={subLabelColor} fontFamily="mono">
            {entry.feature_id}
          </Text>
          <Badge borderRadius="full" px={2} fontSize="10px" color={pc} bg={hexToRgba(pc, 0.12)}>
            {PRIORITY_NAMES[entry.priority] ?? `P${entry.priority}`}
          </Badge>
          {entry.is_wexp && (
            <Badge borderRadius="full" px={2} fontSize="10px" color="#E5A50A" bg={hexToRgba("#E5A50A", 0.12)}>
              {t("hiddenFeatures.wexp")}
            </Badge>
          )}
        </HStack>
      </Box>
      <HStack spacing={2} flexShrink={0}>
        {stateBadge}
        <Tooltip label={t("hiddenFeatures.actionEnable")} placement="top">
          <IconButton
            aria-label={t("hiddenFeatures.actionEnable")}
            icon={<CheckCircle2 size={15} />}
            size="sm"
            variant="ghost"
            color="#38A169"
            isDisabled={disabled || (entry.has_config && entry.enabled_state === 2)}
            onClick={() => onAction(entry, "enable")}
          />
        </Tooltip>
        <Tooltip label={t("hiddenFeatures.actionDisable")} placement="top">
          <IconButton
            aria-label={t("hiddenFeatures.actionDisable")}
            icon={<Ban size={15} />}
            size="sm"
            variant="ghost"
            color="#E53E3E"
            isDisabled={disabled || (entry.has_config && entry.enabled_state === 1)}
            onClick={() => onAction(entry, "disable")}
          />
        </Tooltip>
        <Tooltip label={t("hiddenFeatures.actionReset")} placement="top">
          <IconButton
            aria-label={t("hiddenFeatures.actionReset")}
            icon={<RotateCcw size={15} />}
            size="sm"
            variant="ghost"
            color={subLabelColor}
            isDisabled={disabled || !entry.has_config}
            onClick={() => onAction(entry, "reset")}
          />
        </Tooltip>
      </HStack>
    </Flex>
  );
});

export default function HiddenFeaturesPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const toast = useDynamicIsland();

  const adaptiveTitle = useAdaptiveTextColor();

  const [status, setStatus] = useState<FeatureFlagsStatus | null>(null);
  const [store, setStore] = useState<StoreKind>("runtime");
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const [entries, setEntries] = useState<FeatureFlagEntry[]>([]);
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const [isLoading, setIsLoading] = useState(true);
  const [isOperating, setIsOperating] = useState(false);
  const [persistBoot, setPersistBoot] = useState(true);
  // 默认展示全部功能（ViVeTool /query 同款），无名字的显示 ID；可手动开启"只看有名字"
  const [namedOnly, setNamedOnly] = useState(false);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [tipOpen, setTipOpen] = useState(() => {
    try {
      return !localStorage.getItem(TIP_LS_KEY);
    } catch {
      return false;
    }
  });
  const cancelRef = useRef<HTMLButtonElement>(null);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#e0e0e0");
  const subLabelColor = useColorModeValue("gray.500", "#969696");
  const borderColor = useColorModeValue("gray.200", "rgba(255,255,255,0.16)");
  const switchOffBg = useColorModeValue("gray.200", "gray.600");

  const refresh = useCallback(async () => {
    setIsLoading(true);
    try {
      const [nextStatus, nextEntries] = await Promise.all([
        invoke<FeatureFlagsStatus>("feature_flags_status"),
        invoke<FeatureFlagEntry[]>("feature_flags_query", {
          store,
          search,
          namedOnly,
          limit: QUERY_LIMIT,
        }),
      ]);
      setStatus(nextStatus);
      setEntries(nextEntries);
      setVisibleCount(PAGE_SIZE);
    } catch (error) {
      toast({
        title: t("hiddenFeatures.loadFailed"),
        description: String(error),
        status: "error",
        duration: 6000,
        isClosable: true,
      });
    } finally {
      setIsLoading(false);
    }
  }, [store, search, namedOnly, t, toast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 打字不触发查询,仅回车/点击搜索按钮时提交
  const submitSearch = useCallback(() => {
    setSearch(searchInput.trim());
  }, [searchInput]);

  const requestAction = useCallback((entry: FeatureFlagEntry, kind: ActionKind) => {
    setPending({ entry, kind });
  }, []);

  const runAction = async (action: PendingAction) => {
    setIsOperating(true);
    try {
      let message: string;
      if (action.kind === "reset") {
        message = await invoke<string>("feature_flags_reset", {
          id: action.entry.feature_id,
          store: "both",
        });
      } else {
        message = await invoke<string>("feature_flags_set", {
          id: action.entry.feature_id,
          state: action.kind === "enable" ? "enabled" : "disabled",
          persistBoot,
        });
      }
      toast({
        title: t("hiddenFeatures.success"),
        description: message,
        status: "success",
        duration: 5000,
        isClosable: true,
      });
      await refresh();
    } catch (error) {
      toast({
        title: t("hiddenFeatures.failed"),
        description: String(error),
        status: "error",
        duration: 8000,
        isClosable: true,
      });
    } finally {
      setIsOperating(false);
      setPending(null);
    }
  };

  const confirmTitleKey =
    pending?.kind === "enable"
      ? "hiddenFeatures.confirmEnableTitle"
      : pending?.kind === "disable"
        ? "hiddenFeatures.confirmDisableTitle"
        : "hiddenFeatures.confirmResetTitle";
  const confirmBodyKey =
    pending?.kind === "enable"
      ? "hiddenFeatures.confirmEnableBody"
      : pending?.kind === "disable"
        ? "hiddenFeatures.confirmDisableBody"
        : "hiddenFeatures.confirmResetBody";

  const actionsDisabled = isOperating || !status?.supported || !status?.is_admin;
  const visibleEntries = entries.slice(0, visibleCount);
  // 仅浏览模式可能被后端 limit 截断；搜索模式返回全部匹配，不提示
  const mayTruncate = search === "" && entries.length >= QUERY_LIMIT;

  return (
    <Box pt={8} w="full">
      <VStack align="stretch" spacing={5} w="full">
        {/* 标题栏 */}
        <Flex
          direction={{ base: "column", md: "row" }}
          justify="space-between"
          align={{ base: "stretch", md: "center" }}
          gap={4}
          wrap="wrap"
        >
          <HStack spacing={3} minW={0}>
            <IconButton
              aria-label={t("builtinTools.back")}
              icon={<ArrowLeft size={20} />}
              variant="ghost"
              onClick={() => navigate("/builtin-tools")}
              color={headingColor}
              flexShrink={0}
            />
            <Box minW={0}>
              <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} noOfLines={1}>
                {t("hiddenFeatures.title")}
              </Heading>
              <Text mt={1} fontSize="sm" color={subLabelColor} noOfLines={2}>
                {t("hiddenFeatures.subtitle")}
              </Text>
            </Box>
            <Badge
              borderRadius="full"
              px={2}
              py={0.5}
              fontSize="xs"
              fontWeight={700}
              color="#E5A50A"
              bg={hexToRgba("#E5A50A", 0.14)}
              flexShrink={0}
            >
              BETA
            </Badge>
          </HStack>
          <Button
            size="sm"
            variant="outline"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void refresh()}
            isLoading={isLoading}
            isDisabled={isOperating}
            flexShrink={0}
          >
            {t("hiddenFeatures.refresh")}
          </Button>
        </Flex>

        {/* 风险提示 */}
        <LiquidGlassCard p={5} boxShadow="sm" w="full">
          <HStack spacing={2} mb={2} wrap="wrap">
            <AlertTriangle size={16} color="#E5A50A" />
            <Text color={labelColor} fontSize="sm" fontWeight="700">
              {t("hiddenFeatures.warningTitle")}
            </Text>
            {status && !status.is_admin && (
              <Tooltip label={t("hiddenFeatures.notAdminHint")} placement="top">
                <Flex
                  align="center"
                  gap={1}
                  bg={hexToRgba("#E53E3E", 0.1)}
                  color="#E53E3E"
                  px={2}
                  py={0.5}
                  borderRadius="full"
                >
                  <AlertTriangle size={12} />
                  <Text fontSize="xs" fontWeight="600">
                    {t("hiddenFeatures.notAdmin")}
                  </Text>
                </Flex>
              </Tooltip>
            )}
          </HStack>
          <Text color={subLabelColor} fontSize="xs" lineHeight="1.7">
            {t("hiddenFeatures.warningBody")}
          </Text>
          <Text color={subLabelColor} fontSize="xs" mt={2} opacity={0.75}>
            {t("hiddenFeatures.viveCredit")}
          </Text>
        </LiquidGlassCard>

        {status && !status.supported && (
          <Alert status="error" variant="subtle" borderRadius="md" fontSize="sm">
            <AlertIcon />
            <AlertDescription>
              {t("hiddenFeatures.unsupportedBody")} (OS Build {status.os_build})
            </AlertDescription>
          </Alert>
        )}

        {status?.boot_pending && (
          <Alert status="info" variant="subtle" borderRadius="md" fontSize="sm">
            <AlertIcon />
            <AlertDescription>{t("hiddenFeatures.rebootPending")}</AlertDescription>
          </Alert>
        )}

        {status?.dictionary_count === 0 && (
          <Alert status="warning" variant="subtle" borderRadius="md" fontSize="sm">
            <AlertIcon />
            <AlertDescription>{t("hiddenFeatures.dictMissing")}</AlertDescription>
          </Alert>
        )}

        {/* 加载中 */}
        {!status && isLoading && (
          <LiquidGlassCard p={6} boxShadow="sm" w="full">
            <Flex align="center" justify="center" gap={3} py={4}>
              <Spinner size="sm" color={primaryColor} />
              <Text color={subLabelColor} fontSize="sm">
                {t("hiddenFeatures.loading")}
              </Text>
            </Flex>
          </LiquidGlassCard>
        )}

        {/* 列表 */}
        {status?.supported && (
          <LiquidGlassCard p={5} boxShadow="sm" w="full">
            <VStack spacing={4} w="full" align="stretch">
              <Flex gap={3} wrap="wrap" align="center" justify="space-between">
                <HStack spacing={2} flex="1" minW={{ base: "full", md: "300px" }}>
                  <InputGroup size="sm">
                    <InputLeftElement pointerEvents="none">
                      <Search size={14} color={subLabelColor} />
                    </InputLeftElement>
                    <Input
                      placeholder={t("hiddenFeatures.searchPlaceholder")}
                      value={searchInput}
                      onChange={(e) => setSearchInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") submitSearch();
                      }}
                      borderRadius="md"
                      focusBorderColor={primaryColor}
                    />
                  </InputGroup>
                  <Button
                    size="sm"
                    bg={primaryColor}
                    color={contrastText}
                    leftIcon={<Search size={14} />}
                    onClick={submitSearch}
                    isLoading={isLoading}
                    flexShrink={0}
                    _hover={{ bg: hexToRgba(primaryColor, 0.82) }}
                  >
                    {t("hiddenFeatures.searchButton")}
                  </Button>
                </HStack>
                <HStack spacing={4} wrap="wrap">
                  <HStack spacing={1}>
                    <Text fontSize="xs" color={subLabelColor} whiteSpace="nowrap">
                      {t("hiddenFeatures.storeLabel")}
                    </Text>
                    {(["runtime", "boot"] as StoreKind[]).map((s) => (
                      <Tooltip
                        key={s}
                        label={t(
                          s === "runtime"
                            ? "hiddenFeatures.storeRuntimeHint"
                            : "hiddenFeatures.storeBootHint"
                        )}
                        placement="top"
                      >
                        <Button
                          size="xs"
                          variant={store === s ? "solid" : "ghost"}
                          bg={store === s ? primaryColor : "transparent"}
                          color={store === s ? contrastText : subLabelColor}
                          onClick={() => setStore(s)}
                          _hover={{ bg: store === s ? primaryColor : hexToRgba(primaryColor, 0.1) }}
                        >
                          {t(s === "runtime" ? "hiddenFeatures.storeRuntime" : "hiddenFeatures.storeBoot")}
                        </Button>
                      </Tooltip>
                    ))}
                  </HStack>
                  <Tooltip label={t("hiddenFeatures.namedOnlyHint")} placement="top">
                    <HStack spacing={2}>
                      <Switch
                        size="sm"
                        isChecked={namedOnly}
                        onChange={(e) => setNamedOnly(e.target.checked)}
                        sx={{
                          "& > span": { bg: namedOnly ? primaryColor : switchOffBg },
                        }}
                      />
                      <Text fontSize="xs" color={labelColor} whiteSpace="nowrap">
                        {t("hiddenFeatures.namedOnly")}
                      </Text>
                    </HStack>
                  </Tooltip>
                  <Tooltip label={t("hiddenFeatures.persistBootHint")} placement="top">
                    <HStack spacing={2}>
                      <Switch
                        size="sm"
                        isChecked={persistBoot}
                        onChange={(e) => setPersistBoot(e.target.checked)}
                        sx={{
                          "& > span": { bg: persistBoot ? primaryColor : switchOffBg },
                        }}
                      />
                      <Text fontSize="xs" color={labelColor} whiteSpace="nowrap">
                        {t("hiddenFeatures.persistBoot")}
                      </Text>
                    </HStack>
                  </Tooltip>
                </HStack>
              </Flex>

              <Text fontSize="xs" color={subLabelColor} lineHeight="1.6">
                {t("hiddenFeatures.numbersExplain")}
              </Text>

              <Flex justify="space-between" align="center" wrap="wrap" gap={2}>
                <Text fontSize="xs" color={subLabelColor}>
                  {t("hiddenFeatures.entryCount", { count: entries.length })}
                </Text>
                {isLoading && <Spinner size="xs" color={primaryColor} />}
              </Flex>

              {mayTruncate && (
                <Text fontSize="xs" color="#E5A50A">
                  {t("hiddenFeatures.truncated", { count: QUERY_LIMIT })}
                </Text>
              )}

              <VStack spacing={0} align="stretch" divider={<Box h="1px" bg={borderColor} />}>
                {visibleEntries.map((entry) => (
                  <EntryRow
                    key={`${entry.feature_id}-${entry.priority}-${entry.has_config}`}
                    entry={entry}
                    disabled={actionsDisabled}
                    onAction={requestAction}
                  />
                ))}
                {!isLoading && entries.length === 0 && (
                  <Flex align="center" justify="center" gap={2} py={8}>
                    <CircleDot size={16} color={subLabelColor} />
                    <Text color={subLabelColor} fontSize="sm">
                      {t("hiddenFeatures.noResults")}
                    </Text>
                  </Flex>
                )}
              </VStack>

              {visibleCount < entries.length && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setVisibleCount((c) => c + PAGE_SIZE)}
                  isDisabled={isOperating}
                >
                  {t("hiddenFeatures.loadMore")}
                </Button>
              )}
            </VStack>
          </LiquidGlassCard>
        )}
      </VStack>

      {/* 首次进入温馨提示 */}
      <Modal isOpen={tipOpen} onClose={() => setTipOpen(false)} isCentered size="lg">
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>
            <HStack spacing={2}>
              <FlaskConical size={18} color="#E5A50A" />
              <Text>{t("hiddenFeatures.tipTitle")}</Text>
            </HStack>
          </ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <Text fontSize="sm" whiteSpace="pre-line" lineHeight="1.8">
              {t("hiddenFeatures.tipBody")}
            </Text>
          </ModalBody>
          <ModalFooter>
            <Button
              bg={primaryColor}
              color={contrastText}
              _hover={{ bg: hexToRgba(primaryColor, 0.82) }}
              onClick={() => {
                try {
                  localStorage.setItem(TIP_LS_KEY, "1");
                } catch {
                  /* ignore */
                }
                setTipOpen(false);
              }}
            >
              {t("hiddenFeatures.tipAck")}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* 操作确认 */}
      <AlertDialog
        isOpen={pending !== null}
        leastDestructiveRef={cancelRef}
        onClose={() => setPending(null)}
      >
        <AlertDialogOverlay>
          <AlertDialogContent>
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              {t(confirmTitleKey)}
            </AlertDialogHeader>
            <AlertDialogBody>
              {pending &&
                t(confirmBodyKey, {
                  id: pending.entry.feature_id,
                  name: pending.entry.name ?? pending.entry.feature_id.toString(),
                })}
              {pending?.kind !== "reset" && persistBoot && (
                <Text fontSize="xs" color={subLabelColor} mt={2}>
                  {t("hiddenFeatures.persistBootHint")}
                </Text>
              )}
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={() => setPending(null)} isDisabled={isOperating}>
                {t("hiddenFeatures.cancel")}
              </Button>
              <Button
                bg={pending?.kind === "disable" ? "#E53E3E" : primaryColor}
                color={contrastText}
                onClick={() => pending && void runAction(pending)}
                isLoading={isOperating}
                loadingText={t("hiddenFeatures.operating")}
                ml={3}
                leftIcon={
                  pending?.kind === "enable" ? (
                    <FlaskConical size={14} />
                  ) : pending?.kind === "disable" ? (
                    <Ban size={14} />
                  ) : (
                    <RotateCcw size={14} />
                  )
                }
              >
                {t(
                  pending?.kind === "enable"
                    ? "hiddenFeatures.actionEnable"
                    : pending?.kind === "disable"
                      ? "hiddenFeatures.actionDisable"
                      : "hiddenFeatures.actionReset"
                )}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </Box>
  );
}
