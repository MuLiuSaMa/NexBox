import {
  Box,
  Flex,
  HStack,
  VStack,
  Text,
  Badge,
  Button,
  Input,
  IconButton,
  Spinner,
  Divider,
  useColorModeValue,
  useDisclosure,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
} from "@chakra-ui/react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useThemeColor } from "@/contexts/theme-color-context";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { PawnioInstallModal } from "@/components/PawnioInstallModal";
import { store } from "@/lib/store";
import { useMusicStore } from "@/stores/music-store";
import {
  LuShieldCheck,
  LuCircleAlert,
  LuRefreshCw,
  LuTrash2,
  LuPlus,
  LuHardDrive,
  LuDatabase,
  LuKeyboard,
  LuBan,
} from "react-icons/lu";
import { Download } from "lucide-react";

interface GameEntry {
  id: string;
  name: string;
  process_names: string[];
  is_builtin: boolean;
}

interface GameFilterStatus {
  enabled: boolean;
  games: GameEntry[];
}

interface StorageSizes {
  cache_size: number;
  data_size: number;
}

/** NexBoxPopNull 拦截规则（与后端 popup_blocker::Rule 字段一致） */
interface PopNullRule {
  list: string; // "B" 黑名单 / "W" 白名单
  field: string; // "exe" | "path" | "title" | "class"
  mode: string; // "contains" | "exact" | "wildcard"
  pattern: string;
}

/** 字节数格式化为可读大小 */
function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

/** 去除 .exe 后缀（大小写不敏感） */
function stripExeSuffix(name: string): string {
  return name.toLowerCase().endsWith(".exe") ? name.slice(0, -4) : name;
}

export default function AdvancedPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("settings");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  const { getActiveColor, getHoverColor, getContrastTextColor, getBorderColor } = useThemeColor();

  // ── 存储大小 ──
  const [sizes, setSizes] = useState<StorageSizes>({ cache_size: 0, data_size: 0 });
  const [clearingCache, setClearingCache] = useState(false);
  const [clearingData, setClearingData] = useState(false);
  const {
    isOpen: isClearDataConfirmOpen,
    onOpen: onClearDataConfirmOpen,
    onClose: onClearDataConfirmClose,
  } = useDisclosure();

  // ── PawnIO ──
  const [pawnioStatus, setPawnioStatus] = useState<{ installed: boolean; version?: string } | null>(null);
  const [pawnioChecking, setPawnioChecking] = useState(true);
  const [installing, setInstalling] = useState(false);
  const {
    isOpen: isPawnioModalOpen,
    onOpen: onPawnioModalOpen,
    onClose: onPawnioModalClose,
  } = useDisclosure();

  // ── 滤镜游戏名单 ──
  const [games, setGames] = useState<GameEntry[]>([]);
  const [editGame, setEditGame] = useState<{ name: string; processName: string } | null>(null);
  const [savingGame, setSavingGame] = useState(false);

  // ── 键盘媒体键控制音乐播放器 ──
  const [mediaKeysEnabled, setMediaKeysEnabled] = useState(true);
  const handleMediaKeysToggle = () => {
    const newValue = !mediaKeysEnabled;
    setMediaKeysEnabled(newValue);
    store.set("nexbox_media_keys_enabled", newValue).then(() => store.save());
    useMusicStore.getState().setMediaKeysEnabled(newValue);
    invoke("set_media_keys_enabled", { enabled: newValue })
      .then(() => {
        // 关闭时后端已同步停用本应用 SMTC 会话（把媒体键还给其他软件）；
        // 重新开启时立即恢复推送，无需等待下次播放事件
        if (newValue) useMusicStore.getState().refreshSmtc();
      })
      .catch((e) => console.error("[MediaKeys] 设置失败:", e));
  };

  // ── NexBoxPopNull 弹窗拦截 ──
  const [popnullEnabled, setPopnullEnabled] = useState(false);
  const [popnullRules, setPopnullRules] = useState<PopNullRule[]>([]);
  const [popnullList, setPopnullList] = useState("B");
  const [popnullField, setPopnullField] = useState("exe");
  const [popnullMode, setPopnullMode] = useState("contains");
  const [popnullPattern, setPopnullPattern] = useState("");

  const popnullFieldLabel = (f: string) =>
    ({ exe: t("settings.advanced.popnull.fieldExe", "进程"), path: t("settings.advanced.popnull.fieldPath", "路径"), title: t("settings.advanced.popnull.fieldTitle", "标题"), class: t("settings.advanced.popnull.fieldClass", "类名") })[f] ?? f;
  const popnullModeLabel = (m: string) =>
    ({ contains: t("settings.advanced.popnull.modeContains", "包含"), exact: t("settings.advanced.popnull.modeExact", "精确"), wildcard: t("settings.advanced.popnull.modeWildcard", "通配符") })[m] ?? m;

  const popnullRuleKey = (r: PopNullRule) => `${r.list}|${r.field}|${r.mode}|${r.pattern}`;

  /** 更新规则：写 store（settings.json）+ 同步后端内存 */
  const publishPopnullRules = useCallback(
    (rules: PopNullRule[]) => {
      setPopnullRules(rules);
      store
        .set("nexbox_popnull_rules", rules)
        .then(() => store.save())
        .catch(() => {});
      invoke("popnull_set_rules", { rules }).catch((e) => console.error("[PopNull] 同步规则失败:", e));
    },
    []
  );

  const handlePopnullToggle = () => {
    const newValue = !popnullEnabled;
    setPopnullEnabled(newValue);
    store
      .set("nexbox_popnull_enabled", newValue)
      .then(() => store.save())
      .catch(() => {});
    invoke("popnull_set_enabled", { enabled: newValue })
      .then(() => {
        if (newValue) {
          // 开启时同时把当前规则同步到引擎，确保预设/已保存规则立即生效
          invoke("popnull_set_rules", { rules: popnullRules }).catch(() => {});
        }
      })
      .catch((e) => {
        setPopnullEnabled(!newValue);
        toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
      });
  };

  const handlePopnullAddRule = () => {
    const pattern = popnullPattern.trim().toLowerCase();
    if (!pattern) return;
    const rule: PopNullRule = { list: popnullList, field: popnullField, mode: popnullMode, pattern };
    const key = popnullRuleKey(rule);
    if (popnullRules.some((r) => popnullRuleKey(r) === key)) {
      toast({ title: t("settings.advanced.popnull.duplicate", "规则已存在"), status: "error", duration: 2500, isClosable: true });
      return;
    }
    publishPopnullRules([rule, ...popnullRules]);
    setPopnullPattern("");
    toast({ title: t("settings.advanced.popnull.ruleAdded", "已添加规则"), status: "success", duration: 2000, isClosable: true });
  };

  const handlePopnullRemoveRule = (index: number) => {
    publishPopnullRules(popnullRules.filter((_, i) => i !== index));
  };

  const refreshSizes = useCallback(async () => {
    try {
      const result = await invoke<StorageSizes>("get_storage_sizes");
      setSizes(result);
    } catch (e) {
      console.error("Failed to load storage sizes:", e);
    }
  }, []);

  const refreshPawnio = useCallback(async () => {
    setPawnioChecking(true);
    try {
      const status = await invoke<{ installed: boolean; version?: string }>("check_pawnio_status");
      setPawnioStatus(status);
    } catch {
      setPawnioStatus(null);
    } finally {
      setPawnioChecking(false);
    }
  }, []);

  const refreshGames = useCallback(async () => {
    try {
      const result = await invoke<GameFilterStatus>("get_game_filter_status");
      setGames(result.games);
    } catch (e) {
      console.error("Failed to load game filter status:", e);
    }
  }, []);

  useEffect(() => {
    refreshSizes();
    refreshPawnio();
    refreshGames();
    // 加载键盘媒体键控制开关（默认开启）
    store
      .get<boolean>("nexbox_media_keys_enabled")
      .then((v) => {
        if (v != null) setMediaKeysEnabled(v);
      })
      .catch(() => {});
    // NexBoxPopNull：从引擎获取 enabled/规则（首次规则为内置预设时落库 store）
    store
      .get<boolean>("nexbox_popnull_enabled")
      .then((v) => {
        if (v != null) setPopnullEnabled(v);
      })
      .catch(() => {});
    invoke<{ enabled: boolean; rules: PopNullRule[] }>("popnull_get_state")
      .then((state) => {
        setPopnullEnabled(state.enabled);
        setPopnullRules(state.rules);
        store
          .get<PopNullRule[] | null>("nexbox_popnull_rules")
          .then((saved) => {
            if (saved == null || saved.length === 0) {
              publishPopnullRules(state.rules);
            }
          })
          .catch(() => {});
      })
      .catch((e) => console.error("[PopNull] 获取状态失败:", e));
  }, [refreshSizes, refreshPawnio, refreshGames, publishPopnullRules]);

  const activeColor = getActiveColor();
  const accent = getContrastTextColor();
  const accentBorder = getBorderColor();
  const accentSoft = getHoverColor();
  const isPawnioInstalled = !!pawnioStatus?.installed;

  // ── PawnIO 卸载 ──
  const handleUninstallPawnio = async () => {
    setInstalling(true);
    try {
      await invoke<string>("uninstall_pawnio_driver");
      await invoke("restart_monitor_process");
      toast({ title: t("settings.pawnio.uninstalled", "已卸载"), status: "success", duration: 3000, isClosable: true });
      setPawnioStatus({ installed: false });
    } catch (e) {
      toast({ title: t("settings.pawnio.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
    } finally {
      setInstalling(false);
    }
  };

  // ── 清除缓存 / 数据 ──
  const handleClearCache = async () => {
    setClearingCache(true);
    try {
      await invoke<number>("clear_cache");
      await refreshSizes();
      toast({ title: t("settings.advanced.cacheCleared", "已清除"), status: "success", duration: 3000, isClosable: true });
    } catch (e) {
      toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
    } finally {
      setClearingCache(false);
    }
  };

  const handleClearData = async () => {
    onClearDataConfirmClose();
    setClearingData(true);
    try {
      await invoke<number>("clear_data");
      toast({ title: t("settings.advanced.dataCleared", "已重置"), description: t("settings.advanced.restarting", "正在重启应用…"), status: "success", duration: 2000, isClosable: false });
      // 重启应用以彻底清理被占用的 WebView2 数据
      await invoke("restart_app");
    } catch (e) {
      toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
      setClearingData(false);
    }
  };

  // ── 添加 / 删除游戏 ──
  const handleAddGame = async () => {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "exe", extensions: ["exe"] },
          { name: "all", extensions: ["*"] },
        ],
      });
      if (!selected) return; // 用户取消
      const filePath = Array.isArray(selected) ? selected[0] : selected;
      const fileName = filePath.split(/[\\/]/).pop() ?? "";
      const processName = stripExeSuffix(fileName);
      setEditGame({ name: processName, processName });
    } catch (e) {
      toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
    }
  };

  const handleSaveGame = async () => {
    if (!editGame) return;
    const name = editGame.name.trim();
    const processName = editGame.processName.trim();
    if (!name || !processName) return;
    setSavingGame(true);
    try {
      await invoke("add_custom_game", { name, processNames: [processName] });
      setEditGame(null);
      await refreshGames();
      toast({ title: t("settings.advanced.gameAdded", "已添加"), status: "success", duration: 3000, isClosable: true });
    } catch (e) {
      toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
    } finally {
      setSavingGame(false);
    }
  };

  const handleRemoveGame = async (id: string) => {
    try {
      await invoke("remove_custom_game", { id });
      await refreshGames();
      toast({ title: t("settings.advanced.gameRemoved", "已删除"), status: "success", duration: 3000, isClosable: true });
    } catch (e) {
      toast({ title: t("settings.advanced.error", "操作失败"), description: String(e), status: "error", duration: 3000, isClosable: true });
    }
  };

  const customGames = games.filter((g) => !g.is_builtin);
  const builtinCount = games.length - customGames.length;

  return (
    <Box>
      <Text fontSize="lg" fontWeight="bold" mb={6} color={titleColor}>
        {t("settings.advanced.label", "高级")}
      </Text>

      {/* PawnIO 精简管理 */}
      <LiquidGlassCard mb={4} px={4} py={4} boxShadow="sm">
        <VStack spacing={4} align="stretch">
          <HStack spacing={4} align="center">
            <Flex
              align="center"
              justify="center"
              w="40px"
              h="40px"
              borderRadius="xl"
              flexShrink={0}
              bg={accentSoft}
              border={`1px solid ${accentBorder}`}
              color={activeColor}
            >
              {isPawnioInstalled ? <LuShieldCheck size={20} /> : <LuCircleAlert size={20} />}
            </Flex>
            <VStack align="flex-start" spacing={0.5} flex={1}>
              <HStack spacing={2}>
                <Text fontSize="sm" color={subLabelColor} fontWeight="medium">
                  {t("settings.pawnio.label", "PawnIO")}
                </Text>
                {!pawnioChecking && (
                  <Badge
                    variant="solid"
                    bg={isPawnioInstalled ? activeColor : undefined}
                    color={isPawnioInstalled ? accent : undefined}
                    fontSize="xs"
                    borderRadius="full"
                    px={2.5}
                    py={0.5}
                    textTransform="none"
                  >
                    {isPawnioInstalled
                      ? `${t("settings.pawnio.installed", "已安装")}${pawnioStatus.version ? ` v${pawnioStatus.version}` : ""}`
                      : t("settings.pawnio.notInstalled", "未安装")}
                  </Badge>
                )}
              </HStack>
              <Text fontSize="sm" color={subLabelColor}>
                {t("settings.pawnio.descriptionShort", "可选内核级驱动，提供 CPU 温度、风扇转速等更详细的硬件信息。")}
              </Text>
            </VStack>
            {pawnioChecking && <Spinner size="sm" color={activeColor} />}
          </HStack>
          <Divider borderColor={dividerColor} />
          <HStack spacing={3}>
            {isPawnioInstalled ? (
              <>
                <Button
                  size="sm"
                  bg={activeColor}
                  color={accent}
                  _hover={{ bg: activeColor, opacity: 0.85 }}
                  leftIcon={<LuRefreshCw size={14} />}
                  onClick={refreshPawnio}
                  isLoading={pawnioChecking}
                >
                  {t("settings.pawnio.refresh", "刷新状态")}
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  borderColor={accentBorder}
                  color={labelColor}
                  leftIcon={<LuTrash2 size={14} />}
                  onClick={handleUninstallPawnio}
                  isLoading={installing}
                >
                  {t("settings.pawnio.uninstall", "卸载")}
                </Button>
              </>
            ) : (
              <Button
                size="sm"
                bg={activeColor}
                color={accent}
                _hover={{ bg: activeColor, opacity: 0.85 }}
                leftIcon={<Download size={14} />}
                onClick={onPawnioModalOpen}
              >
                {t("settings.pawnio.install", "安装 PawnIO 驱动")}
              </Button>
            )}
          </HStack>
        </VStack>
      </LiquidGlassCard>

      {/* 清除缓存 / 数据 */}
      <LiquidGlassCard mb={4} px={4} py={4} boxShadow="sm">
        <VStack spacing={4} align="stretch">
          {/* 清除缓存 */}
          <HStack spacing={4}>
            <Flex
              align="center"
              justify="center"
              w="40px"
              h="40px"
              borderRadius="xl"
              flexShrink={0}
              bg={accentSoft}
              border={`1px solid ${accentBorder}`}
              color={activeColor}
            >
              <LuHardDrive size={20} />
            </Flex>
            <VStack align="flex-start" spacing={0.5} flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="semibold">
                {t("settings.advanced.cacheTitle", "清除缓存")}
              </Text>
              <Text fontSize="xs" color={subLabelColor}>
                {t("settings.advanced.cacheDesc", "图标等缓存文件，可自动重建")} · {formatSize(sizes.cache_size)}
              </Text>
            </VStack>
            <Button
              size="sm"
              bg={activeColor}
              color={accent}
              _hover={{ bg: activeColor, opacity: 0.85 }}
              leftIcon={<LuTrash2 size={14} />}
              onClick={handleClearCache}
              isLoading={clearingCache}
            >
              {t("settings.advanced.clearCache", "清除")}
            </Button>
          </HStack>
          <Divider borderColor={dividerColor} />
          {/* 清除数据 */}
          <HStack spacing={4}>
            <Flex
              align="center"
              justify="center"
              w="40px"
              h="40px"
              borderRadius="xl"
              flexShrink={0}
              bg={accentSoft}
              border={`1px solid ${accentBorder}`}
              color={activeColor}
            >
              <LuDatabase size={20} />
            </Flex>
            <VStack align="flex-start" spacing={0.5} flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="semibold">
                {t("settings.advanced.dataTitle", "清除数据")}
              </Text>
              <Text fontSize="xs" color={subLabelColor}>
                {t("settings.advanced.dataDesc", "全部数据，重置所有设置")} · {formatSize(sizes.data_size)}
              </Text>
            </VStack>
            <Button
              size="sm"
              bg={activeColor}
              color={accent}
              _hover={{ bg: activeColor, opacity: 0.85 }}
              leftIcon={<LuTrash2 size={14} />}
              onClick={onClearDataConfirmOpen}
            >
              {t("settings.advanced.clearData", "重置")}
            </Button>
          </HStack>
        </VStack>
      </LiquidGlassCard>

      {/* 键盘媒体键控制音乐播放器 */}
      <LiquidGlassCard mb={4} px={4} py={4} boxShadow="sm">
        <HStack spacing={4} align="center">
          <Flex
            align="center"
            justify="center"
            w="40px"
            h="40px"
            borderRadius="xl"
            flexShrink={0}
            bg={accentSoft}
            border={`1px solid ${accentBorder}`}
            color={activeColor}
          >
            <LuKeyboard size={20} />
          </Flex>
          <VStack align="flex-start" spacing={0.5} flex={1}>
            <Text fontSize="sm" color={labelColor} fontWeight="semibold">
              {t("settings.advanced.mediaKeysTitle", "键盘媒体键控制音乐（SMTC）")}
            </Text>
            <Text fontSize="xs" color={subLabelColor}>
              {t("settings.advanced.mediaKeysDesc", "关闭后同时停用系统媒体会话（SMTC）：音量浮层/锁屏不再显示新境盒，媒体键交还给其他软件")}
            </Text>
          </VStack>
          <ThemeSwitch
            size="md"
            isChecked={mediaKeysEnabled}
            onChange={handleMediaKeysToggle}
          />
        </HStack>
      </LiquidGlassCard>

      {/* 滤镜游戏名单 */}
      <LiquidGlassCard mb={4} px={4} py={4} boxShadow="sm">
        <VStack spacing={4} align="stretch">
          <HStack spacing={4} align="center">
            <VStack align="flex-start" spacing={0.5} flex={1}>
              <Text fontSize="sm" color={labelColor} fontWeight="semibold">
                {t("settings.advanced.gameListTitle", "滤镜游戏名单")}
              </Text>
              <Text fontSize="xs" color={subLabelColor}>
                {t("settings.advanced.builtinCount", "内置 {{count}} 款", { count: builtinCount })}
              </Text>
            </VStack>
            <Button
              size="sm"
              bg={activeColor}
              color={accent}
              _hover={{ bg: activeColor, opacity: 0.85 }}
              leftIcon={<LuPlus size={14} />}
              onClick={handleAddGame}
            >
              {t("settings.advanced.addGame", "添加游戏")}
            </Button>
          </HStack>
          <Divider borderColor={dividerColor} />
          {customGames.length === 0 ? (
            <Text fontSize="sm" color={subLabelColor}>
              {t("settings.advanced.noCustomGame", "暂无自定义游戏")}
            </Text>
          ) : (
            <VStack spacing={2} align="stretch">
              {customGames.map((game) => (
                <HStack key={game.id} spacing={3}>
                  <VStack align="flex-start" spacing={0} flex={1}>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium">
                      {game.name}
                    </Text>
                    <Text fontSize="xs" color={subLabelColor}>
                      {game.process_names.join(", ")}
                    </Text>
                  </VStack>
                  <Button
                    size="xs"
                    variant="ghost"
                    color={subLabelColor}
                    leftIcon={<LuTrash2 size={13} />}
                    onClick={() => handleRemoveGame(game.id)}
                  >
                    {t("settings.advanced.removeGame", "删除")}
                  </Button>
                </HStack>
              ))}
            </VStack>
          )}
        </VStack>
      </LiquidGlassCard>

      {/* NexBoxPopNull 弹窗拦截 */}
      <LiquidGlassCard px={4} py={4} boxShadow="sm">
        <HStack spacing={4} align="center">
          <Flex
            align="center"
            justify="center"
            w="40px"
            h="40px"
            borderRadius="xl"
            flexShrink={0}
            bg={accentSoft}
            border={`1px solid ${accentBorder}`}
            color={activeColor}
          >
            <LuBan size={20} />
          </Flex>
          <VStack align="flex-start" spacing={0.5} flex={1}>
            <HStack spacing={2}>
              <Text fontSize="sm" color={labelColor} fontWeight="semibold">
                {t("settings.advanced.popnull.title", "NexBoxPopNull 弹窗拦截")}
              </Text>
              <Badge fontSize="xs" variant="solid" color={accent} bg={activeColor} textTransform="none">
                BETA
              </Badge>
              <Badge
                variant="solid"
                bg={popnullEnabled ? activeColor : useColorModeValue("gray.300", "#3a3a3a")}
                color={popnullEnabled ? accent : useColorModeValue("gray.700", "#cccccc")}
                fontSize="xs"
                borderRadius="full"
                px={2.5}
                py={0.5}
                textTransform="none"
              >
                {popnullEnabled
                  ? t("settings.advanced.popnull.blocking", "弹窗拦截中")
                  : t("settings.advanced.popnull.disabled", "未启用")}
              </Badge>
            </HStack>
            <Text fontSize="xs" color={subLabelColor}>
              {t("settings.advanced.popnull.desc", "按黑白名单自动拦截广告弹窗：命中黑名单即关闭窗口并结束进程，白名单优先放行，系统进程不受影响")}
            </Text>
          </VStack>
          <ThemeSwitch size="md" isChecked={popnullEnabled} onChange={handlePopnullToggle} />
        </HStack>

        {popnullEnabled && (
          <>
            <Divider borderColor={dividerColor} my={3} />
            {/* 添加规则 */}
            <Flex wrap="wrap" gap={2} align="center" mb={3}>
              <CustomSelect
                value={popnullList}
                onChange={setPopnullList}
                options={[
                  { value: "B", label: t("settings.advanced.popnull.listBlack", "黑名单") },
                  { value: "W", label: t("settings.advanced.popnull.listWhite", "白名单") },
                ]}
                width="110px"
              />
              <CustomSelect
                value={popnullField}
                onChange={setPopnullField}
                options={[
                  { value: "exe", label: t("settings.advanced.popnull.fieldExe", "进程") },
                  { value: "path", label: t("settings.advanced.popnull.fieldPath", "路径") },
                  { value: "title", label: t("settings.advanced.popnull.fieldTitle", "标题") },
                  { value: "class", label: t("settings.advanced.popnull.fieldClass", "类名") },
                ]}
                width="100px"
              />
              <CustomSelect
                value={popnullMode}
                onChange={setPopnullMode}
                options={[
                  { value: "contains", label: t("settings.advanced.popnull.modeContains", "包含") },
                  { value: "exact", label: t("settings.advanced.popnull.modeExact", "精确") },
                  { value: "wildcard", label: t("settings.advanced.popnull.modeWildcard", "通配符") },
                ]}
                width="100px"
              />
              <Input
                flex={1}
                minW="120px"
                size="sm"
                value={popnullPattern}
                onChange={(e) => setPopnullPattern(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handlePopnullAddRule();
                }}
                placeholder={t("settings.advanced.popnull.patternPlaceholder", "如 flashcenter.exe 或 *pop*")}
                bg={useColorModeValue("white", "#111111")}
                color={labelColor}
                borderColor={accentBorder}
                _focus={{
                  borderColor: activeColor,
                  boxShadow: `0 0 0 1px ${activeColor}`,
                }}
              />
              <Button
                size="sm"
                bg={activeColor}
                color={accent}
                _hover={{ bg: activeColor, opacity: 0.85 }}
                leftIcon={<LuPlus size={14} />}
                onClick={handlePopnullAddRule}
                isDisabled={!popnullPattern.trim()}
              >
                {t("settings.advanced.popnull.addRule", "添加")}
              </Button>
            </Flex>

            {/* 规则列表 */}
            {popnullRules.length === 0 ? (
              <Text fontSize="sm" color={subLabelColor}>
                {t("settings.advanced.popnull.noRules", "暂无规则，添加第一条规则开始拦截")}
              </Text>
            ) : (
              <VStack spacing={2} align="stretch" maxH="260px" overflowY="auto">
                {popnullRules.map((rule, i) => (
                  <HStack key={popnullRuleKey(rule)} spacing={3}>
                    <Badge
                      variant="solid"
                      bg={rule.list === "W" ? "green.500" : "red.500"}
                      color="#fff"
                      fontSize="xs"
                      borderRadius="full"
                      px={2}
                      py={0.5}
                      flexShrink={0}
                      textTransform="none"
                    >
                      {rule.list === "W"
                        ? t("settings.advanced.popnull.listWhite", "白名单")
                        : t("settings.advanced.popnull.listBlack", "黑名单")}
                    </Badge>
                    <Text fontSize="sm" color={labelColor} fontWeight="medium" flex={1} isTruncated>
                      {rule.pattern}
                    </Text>
                    <Text fontSize="xs" color={subLabelColor} flexShrink={0}>
                      {popnullFieldLabel(rule.field)} · {popnullModeLabel(rule.mode)}
                    </Text>
                    <IconButton
                      size="xs"
                      variant="ghost"
                      aria-label={t("settings.advanced.popnull.delete", "删除")}
                      color={subLabelColor}
                      icon={<LuTrash2 size={13} />}
                      onClick={() => handlePopnullRemoveRule(i)}
                    />
                  </HStack>
                ))}
              </VStack>
            )}
          </>
        )}
      </LiquidGlassCard>

      {/* 清除数据确认框 */}
      <Modal isOpen={isClearDataConfirmOpen} onClose={onClearDataConfirmClose} isCentered>
        <ModalOverlay />
        <ModalContent bg={useColorModeValue("white", "gray.800")}>
          <ModalHeader color={labelColor}>{t("settings.advanced.clearDataConfirmTitle", "确认清除数据")}</ModalHeader>
          <ModalCloseButton color={subLabelColor} />
          <ModalBody pb={4}>
            <Text fontSize="sm" color={labelColor}>
              {t("settings.advanced.clearDataConfirm", "将清除全部数据并重置所有设置，此操作不可撤销。确认重置？")}
            </Text>
          </ModalBody>
          <ModalFooter>
            <Button size="sm" variant="ghost" mr={3} onClick={onClearDataConfirmClose}>
              {t("settings.advanced.cancel", "取消")}
            </Button>
            <Button size="sm" bg={activeColor} color={accent} _hover={{ bg: activeColor, opacity: 0.85 }} onClick={handleClearData} isLoading={clearingData}>
              {t("settings.advanced.confirmReset", "确认重置")}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* 添加游戏编辑框 */}
      <Modal isOpen={editGame !== null} onClose={() => setEditGame(null)} isCentered>
        <ModalOverlay />
        <ModalContent bg={useColorModeValue("white", "gray.800")}>
          <ModalHeader color={labelColor}>{t("settings.advanced.addGame", "添加游戏")}</ModalHeader>
          <ModalCloseButton color={subLabelColor} />
          <ModalBody pb={4}>
            <VStack spacing={3} align="stretch">
              <Box>
                <Text fontSize="xs" color={subLabelColor} mb={1}>
                  {t("settings.advanced.nameLabel", "游戏名称")}
                </Text>
                <Input
                  value={editGame?.name ?? ""}
                  onChange={(e) => setEditGame((prev) => (prev ? { ...prev, name: e.target.value } : prev))}
                  bg={accentSoft}
                  color={labelColor}
                  borderColor={accentBorder}
                />
              </Box>
              <Box>
                <Text fontSize="xs" color={subLabelColor} mb={1}>
                  {t("settings.advanced.processLabel", "进程名")}
                </Text>
                <Input
                  value={editGame?.processName ?? ""}
                  onChange={(e) => setEditGame((prev) => (prev ? { ...prev, processName: e.target.value } : prev))}
                  bg={accentSoft}
                  color={labelColor}
                  borderColor={accentBorder}
                />
              </Box>
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button size="sm" variant="ghost" mr={3} onClick={() => setEditGame(null)}>
              {t("settings.advanced.cancel", "取消")}
            </Button>
            <Button
              size="sm"
              bg={activeColor}
              color={accent}
              _hover={{ bg: activeColor, opacity: 0.85 }}
              onClick={handleSaveGame}
              isLoading={savingGame}
              isDisabled={!editGame?.name.trim() || !editGame?.processName.trim()}
            >
              {t("settings.advanced.save", "保存")}
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* PawnIO 安装对话框 */}
      <PawnioInstallModal
        isOpen={isPawnioModalOpen}
        onClose={onPawnioModalClose}
        onSuccess={refreshPawnio}
      />
    </Box>
  );
}