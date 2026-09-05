import {
  Box,
  Text,
  SimpleGrid,
  Spinner,
  VStack,
  HStack,
  useColorModeValue,
  useColorMode,
  Image,
  Button,
  IconButton,
  Input,
  InputGroup,
  InputLeftElement,
  Badge,
  Tooltip,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  useDisclosure,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  MenuDivider,
  Flex,
  Divider,
  Collapse,
  Portal,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useMemo, useCallback, useRef, memo, useDeferredValue } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import {
  LuSearch,
  LuRefreshCw,
  LuPlay,
  LuFolder,
  LuExternalLink,
  LuEllipsisVertical,
  LuTrash2,
  LuGamepad2,
  LuHardDrive,
  LuUsers,
  LuCircle,
  LuCircleDot,
  LuChevronDown,
  LuChevronUp,
  LuCircleUser,
  LuLibrary,
  LuClock3,
  LuDownload,
  LuCloudDownload,
} from "react-icons/lu";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import steamIcon from "@/assets/tools/Steam.png";

// ======================== 类型定义 ========================

interface SteamInstallInfo {
  installed: boolean;
  install_path: string | null;
  is_running: boolean;
}

interface SteamUser {
  steam_id64: string;
  account_name: string;
  persona_name: string;
  most_recent: boolean;
  remember_password: boolean;
  timestamp: number;
  avatar_url: string | null;
  avatar_medium_url: string | null;
  avatar_full_url: string | null;
}

interface SteamAvatarInfo {
  steam_id64: string;
  avatar_url: string | null;
  avatar_medium_url: string | null;
  avatar_full_url: string | null;
}

interface SteamLibrary {
  index: number;
  path: string;
  label: string;
  total_size: number;
  free_size: number;
  apps: string[];
}

interface SteamGame {
  app_id: number;
  name: string;
  install_dir: string;
  library_path: string;
  size_on_disk: number;
  state_flags: number;
  last_updated: number;
  last_owner: string;
  build_id: number;
  bytes_to_download: number;
  bytes_downloaded: number;
  playtime_minutes: number;
  last_played: number;
}

interface SteamAllData {
  install_info: SteamInstallInfo;
  users: SteamUser[];
  libraries: SteamLibrary[];
  games: SteamGame[];
}

interface SteamInventoryGame {
  app_id: number;
  name: string;
  installed: boolean;
  playtime_minutes: number;
  last_played: number;
  size_on_disk: number;
  install_dir: string;
  library_path: string;
}

interface SteamInventoryStats {
  total: number;
  installed: number;
  not_installed: number;
  total_playtime_minutes: number;
}

interface SteamInventoryUser {
  steam_id64: string;
  account_name: string;
  persona_name: string;
}

interface SteamInventoryData {
  source: "online" | "cache" | "none";
  steam_running: boolean;
  current_user: SteamInventoryUser | null;
  stats: SteamInventoryStats;
  games: SteamInventoryGame[];
  error: string | null;
}

// ======================== 工具函数 ========================

function formatSize(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(1)} ${units[unitIndex]}`;
}

function formatDate(timestamp: number): string {
  if (timestamp === 0) return "-";
  return new Date(timestamp * 1000).toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

/** 分钟 → "X小时Y分钟"，不足 1 小时只显示分钟 */
function formatPlaytime(minutes: number): string {
  if (minutes <= 0) return "";
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  if (hours > 0 && mins > 0) return `${hours}小时${mins}分钟`;
  if (hours > 0) return `${hours}小时`;
  return `${mins}分钟`;
}

/** 最近游玩相对时间（X天前 / X小时前 / 日期） */
function formatLastPlayed(timestamp: number): string {
  if (timestamp <= 0) return "";
  const diff = Date.now() / 1000 - timestamp;
  if (diff < 3600) return "刚刚";
  if (diff < 86400) return `${Math.floor(diff / 3600)}小时前`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}天前`;
  return new Date(timestamp * 1000).toLocaleDateString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  });
}

function steamId64To32(id64: string): string {
  try {
    const id = BigInt(id64);
    const base = BigInt(76561197960265728);
    if (id > base) return (id - base).toString();
    return id.toString();
  } catch {
    return id64;
  }
}

function getGameCoverUrl(appId: number): string {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/header.jpg`;
}

function normalizePath(path: string): string {
  return path.replace(/\//g, "\\").toLowerCase().replace(/\\+$/, "");
}

/** 从库路径中提取盘符，如 "D:\SteamLibrary" → "D:" */
function getDriveLabel(path: string): string {
  const m = path.match(/^[A-Za-z]:/);
  return m ? m[0].toUpperCase() : path;
}

// ======================== 组件 ========================

function StatusBadge({ running, activeColor, t }: { running: boolean; activeColor: string; t: (k: string) => string }) {
  return (
    <Badge
      colorScheme={running ? "green" : "gray"}
      variant="subtle"
      borderRadius="full"
      px={3}
      py={1}
      fontSize="xs"
    >
      <HStack spacing={1.5}>
        <Box w={2} h={2} borderRadius="full" bg={running ? "green.400" : "gray.400"} />
        <Text>{running ? t("steam.running") : t("steam.notRunning")}</Text>
      </HStack>
    </Badge>
  );
}

function UserCard({
  user,
  onSwitch,
  onDelete,
  isSwitching,
  isLoadingAvatar,
}: {
  user: SteamUser;
  onSwitch: (accountName: string) => void;
  onDelete: (user: SteamUser) => void;
  isSwitching: boolean;
  isLoadingAvatar: boolean;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { colorMode } = useColorMode();
  const { getActiveColor } = useThemeColor();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const activeColor = getActiveColor();

  const displayName = user.persona_name || user.account_name || "Unknown";
  const avatarSrc = user.avatar_medium_url || user.avatar_url;
  const isAvatarLoading = isLoadingAvatar && !avatarSrc;

  const inner = (
    <HStack spacing={3} align="center">
      {avatarSrc ? (
        <Image
          src={avatarSrc}
          alt={displayName}
          boxSize="36px"
          borderRadius="full"
          objectFit="cover"
          onError={(e) => {
            (e.target as HTMLImageElement).style.display = "none";
          }}
        />
      ) : isAvatarLoading ? (
        <Box boxSize="36px" borderRadius="full" bg={`${activeColor}15`} display="flex" alignItems="center" justifyContent="center">
          <Spinner size="sm" color={activeColor} thickness="2px" speed="0.8s" />
        </Box>
      ) : (
        <Box color={user.most_recent ? activeColor : subTextColor}>
          <LuCircleUser size={36} />
        </Box>
      )}
      <Box flex={1} minW={0}>
        <Text fontSize="sm" fontWeight="bold" color={headerColor} noOfLines={1}>
          {displayName}
        </Text>
        <Text fontSize="xs" color={subTextColor} noOfLines={1}>
          {user.account_name || "-"}
        </Text>
        <Text fontSize="2xs" color={subTextColor} mt={0.5}>
          ID: {steamId64To32(user.steam_id64)}
        </Text>
      </Box>
      <HStack spacing={1}>
        {!user.most_recent && (
          <Tooltip label={t("steam.switchAccount")} placement="top">
            <IconButton
              aria-label={t("steam.switchAccount")}
              icon={<LuCircleDot size={16} />}
              size="sm"
              variant="ghost"
              color={activeColor}
              isLoading={isSwitching}
              onClick={() => onSwitch(user.account_name)}
            />
          </Tooltip>
        )}
        <Tooltip label={t("steam.deleteAccount")} placement="top">
          <IconButton
            aria-label={t("steam.deleteAccount")}
            icon={<LuTrash2 size={16} />}
            size="sm"
            variant="ghost"
            color="red.400"
            _hover={{ bg: "red.50", color: "red.500" }}
            onClick={() => onDelete(user)}
          />
        </Tooltip>
        {user.most_recent && (
          <Box w="16px" h="16px" borderRadius="full" bg={activeColor} />
        )}
      </HStack>
    </HStack>
  );

  const badge = user.most_recent ? (
    <Badge
      position="absolute"
      top={"-1px"}
      right={"-1px"}
      bg={activeColor}
      color="white"
      variant="solid"
      borderTopRightRadius="xl"
      borderBottomLeftRadius="md"
      borderTopLeftRadius={0}
      borderBottomRightRadius={0}
      fontSize="2xs"
      px={2}
      py={0.5}
      zIndex={1}
    >
      {t("steam.currentUser")}
    </Badge>
  ) : null;

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={4} position="relative" overflow="visible">
        {badge}
        {inner}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      border="1px solid"
      borderColor={borderColor}
      borderRadius="xl"
      p={4}
      transition="all 0.2s"
      position="relative"
      overflow="visible"
    >
      {badge}
      {inner}
    </Box>
  );
}

// 分离的游戏封面组件
const GameCover = memo(function GameCover({ appId, name }: { appId: number; name: string }) {
  const [imgError, setImgError] = useState(false);
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const placeholderBg = useColorModeValue("gray.100", "#0a0a0a");

  return (
    <Box position="relative" h="90px" bg={placeholderBg} overflow="hidden" borderRadius="lg">
      {!imgError ? (
        <Image
          src={getGameCoverUrl(appId)}
          alt={name}
          w="100%"
          h="100%"
          objectFit="cover"
          onError={() => setImgError(true)}
          loading="lazy"
          referrerPolicy="no-referrer"
        />
      ) : (
        <Flex w="100%" h="100%" align="center" justify="center">
          <LuGamepad2 size={28} color={subTextColor} />
        </Flex>
      )}
      <Box
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        bg="linear-gradient(to bottom, transparent 0%, rgba(0,0,0,0.7) 100%)"
        pointerEvents="none"
      />
      <Text
        position="absolute"
        bottom={2}
        left={3}
        right={3}
        fontSize="sm"
        fontWeight="bold"
        color="white"
        noOfLines={1}
        textShadow="0 1px 4px rgba(0,0,0,0.8)"
      >
        {name}
      </Text>
    </Box>
  );
});

// 分离的操作菜单组件 - 用 Portal 渲染避免 clipping
const GameActionMenu = memo(function GameActionMenu({
  game,
  onOpenFolder,
  onStorePage,
  onUninstall,
}: {
  game: SteamGame;
  onOpenFolder: (libPath: string, installDir: string) => void;
  onStorePage: (appId: number) => void;
  onUninstall: (appId: number, name: string) => void;
}) {
  const { t } = useTranslation();

  return (
    <Menu>
      <MenuButton
        as={IconButton}
        aria-label="more"
        icon={<LuEllipsisVertical size={14} />}
        size="xs"
        variant="outline"
        borderRadius="lg"
      />
      <Portal>
        <MenuList fontSize="sm" zIndex={9999}>
          <MenuItem icon={<LuFolder size={14} />} onClick={() => onOpenFolder(game.library_path, game.install_dir)}>
            {t("steam.openFolder")}
          </MenuItem>
          <MenuItem icon={<LuExternalLink size={14} />} onClick={() => onStorePage(game.app_id)}>
            {t("steam.storePage")}
          </MenuItem>
          <MenuDivider />
          <MenuItem
            icon={<LuTrash2 size={14} />}
            color="red.500"
            onClick={() => onUninstall(game.app_id, game.name)}
          >
            {t("steam.uninstall")}
          </MenuItem>
        </MenuList>
      </Portal>
    </Menu>
  );
});

const GameCard = memo(function GameCard({
  game,
  isLaunching,
  onLaunch,
  onOpenFolder,
  onStorePage,
  onUninstall,
}: {
  game: SteamGame;
  isLaunching: boolean;
  onLaunch: (appId: number) => void;
  onOpenFolder: (libPath: string, installDir: string) => void;
  onStorePage: (appId: number) => void;
  onUninstall: (appId: number, name: string) => void;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor } = useThemeColor();
  const { colorMode } = useColorMode();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const activeColor = getActiveColor();

  const isDownloading = game.bytes_downloaded > 0 && game.bytes_downloaded < game.bytes_to_download;
  const downloadProgress = game.bytes_to_download > 0
    ? Math.round((game.bytes_downloaded / game.bytes_to_download) * 100)
    : 0;

  const cardContent = (
    <>
      <Box position="relative">
        <GameCover appId={game.app_id} name={game.name} />
        {isDownloading && (
          <Badge
            position="absolute"
            top={2}
            right={2}
            colorScheme="blue"
            variant="solid"
            fontSize="2xs"
            zIndex={1}
          >
            {downloadProgress}%
          </Badge>
        )}
      </Box>

      <Box p={3}>
        <VStack spacing={1.5} align="stretch">
          <HStack justify="space-between" fontSize="2xs" color={subTextColor}>
            <Text noOfLines={1}>AppID: {game.app_id}</Text>
            <Text>{formatSize(game.size_on_disk)}</Text>
          </HStack>
          {/* 游玩时长/最近游玩行：固定行高，无数据时也占据空间，保证卡片高度一致 */}
          <Text
            fontSize="2xs"
            color={subTextColor}
            noOfLines={1}
            h="16px"
            lineHeight="16px"
            overflow="hidden"
          >
            {game.playtime_minutes > 0 ? formatPlaytime(game.playtime_minutes) : ""}
            {game.playtime_minutes > 0 && game.last_played > 0 ? " · " : ""}
            {game.last_played > 0
              ? `${t("steam.lastPlayed")} ${formatLastPlayed(game.last_played)}`
              : ""}
          </Text>
          <Text fontSize="2xs" color={subTextColor} noOfLines={1}>
            {formatDate(game.last_updated)}
          </Text>

          <HStack spacing={1.5} mt={1}>
            <Button
              size="xs"
              style={{ backgroundColor: activeColor, borderColor: activeColor }}
              color="white"
              leftIcon={<LuPlay size={12} />}
              borderRadius="lg"
              flex={1}
              isLoading={isLaunching}
              loadingText={t("steam.launching")}
              spinnerPlacement="start"
              _hover={{ opacity: 0.85 }}
              onClick={() => onLaunch(game.app_id)}
            >
              {t("steam.launch")}
            </Button>
            <GameActionMenu
              game={game}
              onOpenFolder={onOpenFolder}
              onStorePage={onStorePage}
              onUninstall={onUninstall}
            />
          </HStack>
        </VStack>
      </Box>
    </>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard
        overflow="visible"
        transition="all 0.2s"
      >
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      border="1px solid"
      borderColor={borderColor}
      borderRadius="xl"
      overflow="visible"
      transition="all 0.2s"
      _hover={{ borderColor: activeColor, boxShadow: "lg" }}
    >
      {cardContent}
    </Box>
  );
});

const InventoryGameCard = memo(function InventoryGameCard({
  game,
  isLaunching,
  onLaunch,
  onInstall,
  onStorePage,
}: {
  game: SteamInventoryGame;
  isLaunching: boolean;
  onLaunch: (appId: number) => void;
  onInstall: (appId: number) => void;
  onStorePage: (appId: number) => void;
}) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor } = useThemeColor();
  const { colorMode } = useColorMode();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const activeColor = getActiveColor();

  const cardContent = (
    <>
      <Box position="relative">
        <GameCover appId={game.app_id} name={game.name} />
        <Badge
          position="absolute"
          top={2}
          right={2}
          colorScheme={game.installed ? "green" : "gray"}
          variant="solid"
          fontSize="2xs"
          zIndex={1}
        >
          {game.installed ? t("steam.installedBadge") : t("steam.notInstalledBadge")}
        </Badge>
      </Box>

      <Box p={3}>
        <VStack spacing={1.5} align="stretch">
          <HStack justify="space-between" fontSize="2xs" color={subTextColor}>
            <Text noOfLines={1}>AppID: {game.app_id}</Text>
            {game.size_on_disk > 0 && <Text>{formatSize(game.size_on_disk)}</Text>}
          </HStack>
          {/* 游玩时长/最近游玩行：固定行高，无数据时也占据空间，保证卡片高度一致 */}
          <Text
            fontSize="2xs"
            color={subTextColor}
            noOfLines={1}
            h="16px"
            lineHeight="16px"
            overflow="hidden"
          >
            {game.playtime_minutes > 0 ? formatPlaytime(game.playtime_minutes) : ""}
            {game.playtime_minutes > 0 && game.last_played > 0 ? " · " : ""}
            {game.last_played > 0
              ? `${t("steam.lastPlayed")} ${formatLastPlayed(game.last_played)}`
              : ""}
          </Text>

          <HStack spacing={1.5} mt={1}>
            <Button
              size="xs"
              style={{ backgroundColor: activeColor, borderColor: activeColor }}
              color="white"
              leftIcon={game.installed ? <LuPlay size={12} /> : <LuDownload size={12} />}
              borderRadius="lg"
              flex={1}
              isLoading={isLaunching}
              loadingText={t("steam.launching")}
              spinnerPlacement="start"
              _hover={{ opacity: 0.85 }}
              onClick={() => (game.installed ? onLaunch(game.app_id) : onInstall(game.app_id))}
            >
              {game.installed ? t("steam.launch") : t("steam.install")}
            </Button>
            <IconButton
              aria-label={t("steam.storePage")}
              icon={<LuExternalLink size={14} />}
              size="xs"
              variant="outline"
              borderRadius="lg"
              onClick={() => onStorePage(game.app_id)}
            />
          </HStack>
        </VStack>
      </Box>
    </>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard overflow="visible" transition="all 0.2s">
        {cardContent}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      border="1px solid"
      borderColor={borderColor}
      borderRadius="xl"
      overflow="visible"
      transition="all 0.2s"
      _hover={{ borderColor: activeColor, boxShadow: "lg" }}
    >
      {cardContent}
    </Box>
  );
});

function LibraryCard({ lib, gameCount, gameSize }: { lib: SteamLibrary; gameCount: number; gameSize: number }) {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor } = useThemeColor();
  const { colorMode } = useColorMode();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const progressBg = useColorModeValue("gray.100", "#333");
  const activeColor = getActiveColor();

  // 使用实际磁盘容量计算占用百分比
  const totalDisk = lib.total_size > 0 ? lib.total_size : gameSize;
  const usedByGames = gameSize;
  const usedPercent = totalDisk > 0 ? (usedByGames / totalDisk) * 100 : 0;
  const usedPercentStr = totalDisk > 0 ? usedPercent.toFixed(1) : "0.0";

  // 显示：游戏大小 / 磁盘总容量
  const sizeText = lib.total_size > 0
    ? `${formatSize(gameSize)} / ${formatSize(totalDisk)}`
    : formatSize(gameSize);

  const inner = (
    <HStack spacing={3} align="start">
      <Box color={activeColor} mt={1}>
        <LuHardDrive size={20} />
      </Box>
      <Box flex={1} minW={0}>
        <Tooltip label={lib.path}>
          <Text fontSize="sm" fontWeight="medium" color={headerColor} noOfLines={1}>
            {lib.path}
          </Text>
        </Tooltip>
        {lib.label && (
          <Text fontSize="xs" color={subTextColor}>
            {lib.label}
          </Text>
        )}
        <HStack spacing={3} mt={1} fontSize="2xs" color={subTextColor}>
          <Text>{gameCount} {t("steam.games")}</Text>
          <Text>{sizeText}</Text>
        </HStack>
        {/* 磁盘占用进度条 - 始终显示，最小宽度 2px */}
        <Box mt={2} w="100%" h={1.5} bg={progressBg} borderRadius="full" overflow="hidden">
          <Box
            h="100%"
            bg={activeColor}
            borderRadius="full"
            minW="2px"
            w={`${Math.min(usedPercent, 100).toFixed(1)}%`}
          />
        </Box>
        <Text fontSize="2xs" color={subTextColor} mt={1}>
          {t("steam.diskUsage")}: {usedPercentStr}% {lib.free_size > 0 ? `· ${t("steam.free")}: ${formatSize(lib.free_size)}` : ""}
        </Text>
      </Box>
    </HStack>
  );

  if (liquidGlassEnabled) {
    return <LiquidGlassCard p={4}>{inner}</LiquidGlassCard>;
  }

  return (
    <Box bg={cardBg} border="1px solid" borderColor={borderColor} borderRadius="xl" p={4}>
      {inner}
    </Box>
  );
}

// ======================== 主页面 ========================

export default function SteamPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("gamepad");
  const { liquidGlassEnabled } = useBackground();
  const { getActiveColor, getBorderColor } = useThemeColor();
  const { colorMode } = useColorMode();

  const [loading, setLoading] = useState(true);
  const [data, setData] = useState<SteamAllData | null>(null);
  const [searchText, setSearchText] = useState("");
  const [switchingAccount, setSwitchingAccount] = useState<string | null>(null);
  const [launchingAppId, setLaunchingAppId] = useState<number | null>(null);
  const [launchingSteam, setLaunchingSteam] = useState(false);
  const [showLibraries, setShowLibraries] = useState(true);
  const [showUsers, setShowUsers] = useState(true);
  const [inventory, setInventory] = useState<SteamInventoryData | null>(null);
  const [loadingInventory, setLoadingInventory] = useState(false);
  const [showInventory, setShowInventory] = useState(true);
  // 库存分页加载数量：默认只渲染前 120 张卡片，按「加载更多」递增，避免大库存一次渲染过重导致卡顿
  const [visibleCount, setVisibleCount] = useState(120);
  // 延迟值：搜索输入即时响应，列表过滤/渲染延后执行，输入上千条库存时避免每次按键都重算
  const deferredSearch = useDeferredValue(searchText);

  const [uninstallTarget, setUninstallTarget] = useState<{ appId: number; name: string } | null>(null);
  const { isOpen: isUninstallOpen, onOpen: onUninstallOpen, onClose: onUninstallClose } = useDisclosure();
  const [deleteTarget, setDeleteTarget] = useState<SteamUser | null>(null);
  const { isOpen: isDeleteOpen, onOpen: onDeleteOpen, onClose: onDeleteClose } = useDisclosure();
  const cancelRef = useRef<HTMLButtonElement>(null);

  const activeColor = getActiveColor();
  const borderColorTheme = getBorderColor();
  const headerColor = colorMode === "light" ? "#000000" : "#ffffff";
  const adaptiveTitle = useAdaptiveTextColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const inputBg = useColorModeValue("white", "#1a1a1a");
  const [loadingAvatars, setLoadingAvatars] = useState<Set<string>>(new Set());

  const fetchData = useCallback(async () => {
    setLoading(true);
    try {
      const result = await invoke<SteamAllData>("get_steam_all_data");
      setData(result);
      // 异步获取用户头像
      if (result.users.length > 0) {
        const userIds = new Set(result.users.map(u => u.steam_id64));
        setLoadingAvatars(userIds);
        invoke<SteamAvatarInfo[]>("get_steam_user_avatars").then((avatars) => {
          if (avatars.length > 0) {
            const avatarMap = new Map<string, SteamAvatarInfo>();
            for (const a of avatars) {
              avatarMap.set(a.steam_id64, a);
            }
            setData((prev) => {
              if (!prev) return prev;
              return {
                ...prev,
                users: prev.users.map((u) => {
                  const info = avatarMap.get(u.steam_id64);
                  if (info) {
                    return { ...u, avatar_url: info.avatar_url, avatar_medium_url: info.avatar_medium_url, avatar_full_url: info.avatar_full_url };
                  }
                  return u;
                }),
              };
            });
          }
          setLoadingAvatars(new Set());
        }).catch((err) => {
          console.warn("Failed to fetch Steam avatars:", err);
          setLoadingAvatars(new Set());
        });
      }
    } catch (err) {
      console.error("Failed to fetch Steam data:", err);
      toast({
        title: t("steam.fetchError"),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setLoading(false);
    }
  }, [t, toast]);

  const fetchInventory = useCallback(async (forceOnline: boolean) => {
    setLoadingInventory(true);
    try {
      const result = await invoke<SteamInventoryData>("get_steam_inventory", { forceOnline });
      setInventory(result);
    } catch (err) {
      console.error("Failed to fetch Steam inventory:", err);
    } finally {
      setLoadingInventory(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    fetchInventory(false);
  }, [fetchData, fetchInventory]);

  const filteredGames = useMemo(() => {
    if (!data?.games) return [];
    if (!searchText.trim()) return data.games;
    const lower = searchText.toLowerCase();
    return data.games.filter(
      (g) =>
        g.name.toLowerCase().includes(lower) ||
        g.app_id.toString().includes(lower)
    );
  }, [data?.games, searchText]);

  // 按库（硬盘）分组后的游戏列表
  const groupedGames = useMemo(() => {
    if (!data || filteredGames.length === 0) return [];
    const groups: { key: string; driveLabel: string; path: string; label: string; games: SteamGame[]; totalSize: number }[] = [];
    const matched = new Set<number>();
    for (const lib of data.libraries) {
      const libNorm = normalizePath(lib.path);
      const games = filteredGames.filter((g) => {
        const gameNorm = normalizePath(g.library_path);
        if (gameNorm === libNorm || gameNorm.startsWith(libNorm + "\\")) {
          matched.add(g.app_id);
          return true;
        }
        return false;
      });
      if (games.length === 0) continue;
      groups.push({
        key: `${lib.index}-${lib.path}`,
        driveLabel: getDriveLabel(lib.path),
        path: lib.path,
        label: lib.label,
        games,
        totalSize: games.reduce((s, g) => s + g.size_on_disk, 0),
      });
    }
    // 未匹配到任何库的游戏（理论上少见，兜底处理）
    const rest = filteredGames.filter((g) => !matched.has(g.app_id));
    if (rest.length > 0) {
      groups.push({
        key: "other",
        driveLabel: t("steam.other"),
        path: "",
        label: "",
        games: rest,
        totalSize: rest.reduce((s, g) => s + g.size_on_disk, 0),
      });
    }
    return groups;
  }, [data, filteredGames, t]);

  // 库存游戏过滤（共用顶部搜索框，用延迟值避免大列表频繁重算）
  const filteredInventory = useMemo(() => {
    if (!inventory?.games) return [];
    if (!deferredSearch.trim()) return inventory.games;
    const lower = deferredSearch.toLowerCase();
    return inventory.games.filter(
      (g) => g.name.toLowerCase().includes(lower) || g.app_id.toString().includes(lower)
    );
  }, [inventory?.games, deferredSearch]);

  // 库存按可见数量渲染（默认 120，点「加载更多」递增）
  const inventoryGamesShown = useMemo(() => {
    return filteredInventory.slice(0, visibleCount);
  }, [filteredInventory, visibleCount]);

  // 搜索词变化时重置分页
  useEffect(() => {
    setVisibleCount(120);
  }, [deferredSearch]);

  // 直接按路径匹配计算每个库的游戏统计
  const getLibraryStats = useCallback((libPath: string) => {
    if (!data?.games) return { count: 0, size: 0 };
    const libNorm = normalizePath(libPath);
    let count = 0, size = 0;
    for (const g of data.games) {
      const gameNorm = normalizePath(g.library_path);
      if (gameNorm === libNorm || gameNorm.startsWith(libNorm + "\\")) {
        count++;
        size += g.size_on_disk;
      }
    }
    return { count, size };
  }, [data]);

  const totalSize = useMemo(() => {
    return data?.games.reduce((sum, g) => sum + g.size_on_disk, 0) ?? 0;
  }, [data?.games]);

  const handleLaunch = useCallback(async (appId: number) => {
    setLaunchingAppId(appId);
    try {
      await invoke("launch_steam_game", { appId });
      toast({ title: t("steam.launching"), status: "info", duration: 2000, isClosable: true });
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 2000, isClosable: true });
    } finally {
      setTimeout(() => setLaunchingAppId(null), 2000);
    }
  }, [toast, t]);

  const handleLaunchSteam = useCallback(async () => {
    if (data?.install_info.is_running) return;
    setLaunchingSteam(true);
    try {
      await invoke("launch_steam_client");
      toast({ title: t("steam.launchingSteam"), status: "info", duration: 2000, isClosable: true });
      // 延迟刷新状态以检测 Steam 进程
      setTimeout(() => { fetchData(); setLaunchingSteam(false); }, 3000);
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 3000, isClosable: true });
      setLaunchingSteam(false);
    }
  }, [data, toast, t, fetchData]);

  const handleOpenFolder = useCallback(async (libraryPath: string, installDir: string) => {
    try {
      await invoke("open_game_folder", { libraryPath, installDir });
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 2000, isClosable: true });
    }
  }, [toast]);

  const handleStorePage = useCallback(async (appId: number) => {
    try {
      await invoke("open_steam_store_page", { appId });
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 2000, isClosable: true });
    }
  }, [toast]);

  const handleInstall = useCallback(async (appId: number) => {
    try {
      await invoke("install_steam_game", { appId });
      toast({ title: t("steam.installSent"), status: "info", duration: 2000, isClosable: true });
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 2000, isClosable: true });
    }
  }, [toast, t]);

  const handleSwitchAccount = useCallback(async (accountName: string) => {
    setSwitchingAccount(accountName);
    try {
      await invoke("switch_steam_account", { accountName });
      toast({ title: t("steam.switchSuccess"), status: "success", duration: 3000, isClosable: true });
      setTimeout(() => fetchData(), 3000);
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 3000, isClosable: true });
    } finally {
      setSwitchingAccount(null);
    }
  }, [toast, t, fetchData]);

  const handleUninstall = useCallback(async () => {
    if (!uninstallTarget) return;
    try {
      await invoke("uninstall_steam_game", { appId: uninstallTarget.appId });
      toast({ title: t("steam.uninstallSent"), status: "info", duration: 2000, isClosable: true });
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 2000, isClosable: true });
    } finally {
      setUninstallTarget(null);
      onUninstallClose();
    }
  }, [uninstallTarget, toast, t, onUninstallClose]);

  const onUninstallClick = useCallback((appId: number, name: string) => {
    setUninstallTarget({ appId, name });
    onUninstallOpen();
  }, [onUninstallOpen]);

  const handleDeleteAccount = useCallback(async () => {
    if (!deleteTarget) return;
    try {
      if (data?.install_info.is_running) {
        toast({ title: t("steam.deleteAccountCloseSteam"), status: "info", duration: 3000, isClosable: true });
      }
      await invoke("delete_steam_account", { steamId64: deleteTarget.steam_id64 });
      toast({ title: t("steam.deleteAccountSuccess"), status: "success", duration: 3000, isClosable: true });
      setDeleteTarget(null);
      onDeleteClose();
      // 后端删除完成后 vdf 已更新，立即刷新
      fetchData();
    } catch (err) {
      toast({ title: String(err), status: "error", duration: 3000, isClosable: true });
      setDeleteTarget(null);
      onDeleteClose();
    }
  }, [deleteTarget, data, toast, t, onDeleteClose, fetchData]);

  const onDeleteClick = useCallback((user: SteamUser) => {
    setDeleteTarget(user);
    onDeleteOpen();
  }, [onDeleteOpen]);

  // 加载中
  if (loading) {
    return (
      <Box h="calc(100vh - 100px)" display="flex" alignItems="center" justifyContent="center">
        <VStack spacing={4}>
          <Spinner size="xl" thickness="3px" speed="0.65s" color={activeColor} />
          <Text color={subTextColor} fontSize="md">
            {t("steam.loading")}
          </Text>
        </VStack>
      </Box>
    );
  }

  // Steam 未安装
  if (data && !data.install_info.installed) {
    return (
      <Box h="calc(100vh - 100px)" display="flex" alignItems="center" justifyContent="center">
        <VStack spacing={4}>
          <LuGamepad2 size={48} color={subTextColor} />
          <Text color={subTextColor} fontSize="lg">
            {t("steam.notInstalled")}
          </Text>
        </VStack>
      </Box>
    );
  }

  // 统计卡片组件
  const StatCard = ({ icon, label, value }: { icon: React.ReactNode; label: string; value: string | number }) => {
    if (liquidGlassEnabled) {
      return (
        <LiquidGlassCard px={4} py={2}>
          <HStack spacing={2}>
            <Box color={activeColor}>{icon}</Box>
            <Text fontSize="sm" color={subTextColor}>{label}</Text>
            <Text fontSize="sm" fontWeight="bold" color={headerColor}>{value}</Text>
          </HStack>
        </LiquidGlassCard>
      );
    }
    return (
      <Box bg={cardBg} border="1px solid" borderColor={borderColor} borderRadius="xl" px={4} py={2}>
        <HStack spacing={2}>
          <Box color={activeColor}>{icon}</Box>
          <Text fontSize="sm" color={subTextColor}>{label}</Text>
          <Text fontSize="sm" fontWeight="bold" color={textColor}>{value}</Text>
        </HStack>
      </Box>
    );
  };

  return (
    <Box pb={8}>
      {/* 标题栏 */}
      <HStack justify="space-between" mb={6} flexShrink={0}>
        <HStack spacing={3}>
          <Image src={steamIcon} alt="Steam" w="64px" h="64px" objectFit="contain" />
          <Text fontSize="3xl" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            Steam
          </Text>
          {data && <StatusBadge running={data.install_info.is_running} activeColor={activeColor} t={t} />}
          {data && !data.install_info.is_running && (
            <Button
              size="sm"
              leftIcon={<LuPlay size={14} />}
              variant="solid"
              borderRadius="lg"
              bg={activeColor}
              color="white"
              _hover={{ opacity: 0.85, bg: activeColor }}
              isLoading={launchingSteam}
              loadingText={t("steam.launchingSteam")}
              spinnerPlacement="start"
              onClick={handleLaunchSteam}
            >
              {t("steam.launchSteam")}
            </Button>
          )}
        </HStack>
        <HStack spacing={3}>
          <InputGroup w="220px">
            <InputLeftElement pointerEvents="none" display="flex" alignItems="center" justifyContent="center" h="full">
              <LuSearch size={16} color={subTextColor} />
            </InputLeftElement>
            <Input
              placeholder={t("steam.searchGame")}
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
              size="sm"
              borderRadius="lg"
              bg={inputBg}
              borderColor={borderColor}
            />
          </InputGroup>
          <IconButton
            aria-label={t("steam.refresh")}
            icon={<LuRefreshCw size={16} />}
            size="sm"
            variant="outline"
            borderRadius="lg"
            onClick={() => { fetchData(); fetchInventory(false); }}
            isLoading={loading}
          />
        </HStack>
      </HStack>

      {/* 统计信息 */}
      {data && (
        <HStack spacing={4} mb={6} flexWrap="wrap">
          <StatCard icon={<LuGamepad2 size={16} />} label={t("steam.totalGames")} value={data.games.length} />
          <StatCard icon={<LuHardDrive size={16} />} label={t("steam.totalSize")} value={formatSize(totalSize)} />
          <StatCard icon={<LuUsers size={16} />} label={t("steam.accountCount")} value={data.users.length} />
          <StatCard icon={<LuHardDrive size={16} />} label={t("steam.libraries")} value={data.libraries.length} />
          {data.install_info.install_path && (
            <StatCard
              icon={<LuHardDrive size={16} />}
              label={t("steam.installPath")}
              value={data.install_info.install_path}
            />
          )}
        </HStack>
      )}

      {/* 账户管理 - 始终显示 */}
      {data && (
        <Box mb={6}>
          <HStack
            justify="space-between"
            cursor="pointer"
            onClick={() => setShowUsers(!showUsers)}
            mb={showUsers ? 3 : 0}
          >
            <HStack spacing={2}>
              <Box color={activeColor}><LuUsers size={18} /></Box>
              <Text fontSize="lg" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
                {t("steam.accounts")}
              </Text>
              <Badge colorScheme="teal" variant="subtle" borderRadius="full">
                {data.users.length}
              </Badge>
            </HStack>
            <IconButton
              aria-label="toggle"
              icon={showUsers ? <LuChevronUp size={16} /> : <LuChevronDown size={16} />}
              size="sm"
              variant="ghost"
            />
          </HStack>
          <Collapse in={showUsers}>
            {data.users.length > 0 ? (
              <SimpleGrid columns={{ base: 1, sm: 2, md: 3, lg: 4 }} spacing={3}>
                {data.users.map((user) => (
                  <UserCard
                    key={user.steam_id64}
                    user={user}
                    onSwitch={handleSwitchAccount}
                    onDelete={onDeleteClick}
                    isSwitching={switchingAccount === user.account_name}
                    isLoadingAvatar={loadingAvatars.has(user.steam_id64)}
                  />
                ))}
              </SimpleGrid>
            ) : (
              <Box bg={cardBg} border="1px solid" borderColor={borderColor} borderRadius="xl" p={6} textAlign="center">
                <VStack spacing={2}>
                  <LuUsers size={32} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm">
                    {t("steam.noAccounts")}
                  </Text>
                </VStack>
              </Box>
            )}
          </Collapse>
        </Box>
      )}

      {/* 游戏库 - 始终显示 */}
      {data && (
        <Box mb={6}>
          <HStack
            justify="space-between"
            cursor="pointer"
            onClick={() => setShowLibraries(!showLibraries)}
            mb={showLibraries ? 3 : 0}
          >
            <HStack spacing={2}>
              <Box color={activeColor}><LuHardDrive size={18} /></Box>
              <Text fontSize="lg" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
                {t("steam.libraries")}
              </Text>
              <Badge colorScheme="teal" variant="subtle" borderRadius="full">
                {data.libraries.length}
              </Badge>
            </HStack>
            <IconButton
              aria-label="toggle"
              icon={showLibraries ? <LuChevronUp size={16} /> : <LuChevronDown size={16} />}
              size="sm"
              variant="ghost"
            />
          </HStack>
          <Collapse in={showLibraries}>
            {data.libraries.length > 0 ? (
              <SimpleGrid columns={{ base: 1, sm: 2, lg: 3 }} spacing={3}>
                {data.libraries.map((lib) => {
                  const stats = getLibraryStats(lib.path);
                  return (
                    <LibraryCard
                      key={`${lib.index}-${lib.path}`}
                      lib={lib}
                      gameCount={stats.count}
                      gameSize={stats.size}
                    />
                  );
                })}
              </SimpleGrid>
            ) : (
              <Box bg={cardBg} border="1px solid" borderColor={borderColor} borderRadius="xl" p={6} textAlign="center">
                <VStack spacing={2}>
                  <LuHardDrive size={32} color={subTextColor} />
                  <Text color={subTextColor} fontSize="sm">
                    {t("steam.noLibraries")}
                  </Text>
                  {data.install_info.install_path && (
                    <Text color={subTextColor} fontSize="xs">
                      {t("steam.installPath")}: {data.install_info.install_path}
                    </Text>
                  )}
                </VStack>
              </Box>
            )}
          </Collapse>
        </Box>
      )}

      {/* 库存游戏 - 可折叠区块（Steam 运行中在线获取账号库存，否则读本地缓存） */}
      {inventory && (
        <Box mb={6}>
          <HStack
            justify="space-between"
            cursor="pointer"
            onClick={() => setShowInventory(!showInventory)}
            mb={showInventory ? 3 : 0}
          >
            <HStack spacing={2}>
              <Box color={activeColor}><LuLibrary size={18} /></Box>
              <Text fontSize="lg" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
                {t("steam.inventoryGames")}
              </Text>
              <Badge colorScheme="teal" variant="subtle" borderRadius="full">
                {inventory.stats.total}
              </Badge>
              <Badge colorScheme="orange" variant="subtle" borderRadius="full">
                {t("steam.inventorySourceCache")}
              </Badge>
              {loadingInventory && <Spinner size="sm" color={activeColor} thickness="2px" speed="0.8s" />}
            </HStack>
            <IconButton
              aria-label="toggle"
              icon={showInventory ? <LuChevronUp size={16} /> : <LuChevronDown size={16} />}
              size="sm"
              variant="ghost"
            />
          </HStack>
          <Collapse in={showInventory}>
            {inventory.current_user && (
              <HStack spacing={4} mb={4} flexWrap="wrap">
                <StatCard
                  icon={<LuCircleUser size={16} />}
                  label={t("steam.currentAccount")}
                  value={inventory.current_user.persona_name || inventory.current_user.account_name}
                />
                <StatCard icon={<LuLibrary size={16} />} label={t("steam.inventoryTotal")} value={inventory.stats.total} />
                <StatCard icon={<LuCircleDot size={16} />} label={t("steam.inventoryInstalledCount")} value={inventory.stats.installed} />
                <StatCard icon={<LuCircle size={16} />} label={t("steam.inventoryNotInstalledCount")} value={inventory.stats.not_installed} />
                <StatCard
                  icon={<LuClock3 size={16} />}
                  label={t("steam.totalPlaytime")}
                  value={formatPlaytime(inventory.stats.total_playtime_minutes) || "0"}
                />
              </HStack>
            )}
            {inventoryGamesShown.length === 0 ? (
              <VStack py={16} spacing={3}>
                <LuLibrary size={40} color={subTextColor} />
                <Text color={subTextColor} fontSize="md">
                  {searchText ? t("steam.noSearchResults") : t("steam.inventoryEmpty")}
                </Text>
              </VStack>
            ) : (
              <>
                <SimpleGrid columns={{ base: 1, sm: 2, md: 3, lg: 4, xl: 5 }} spacing={4}>
                  {inventoryGamesShown.map((game) => (
                    <InventoryGameCard
                      key={game.app_id}
                      game={game}
                      isLaunching={launchingAppId === game.app_id}
                      onLaunch={handleLaunch}
                      onInstall={handleInstall}
                      onStorePage={handleStorePage}
                    />
                  ))}
                </SimpleGrid>
                {filteredInventory.length > visibleCount && (
                  <VStack mt={4} align="center">
                    <Button
                      size="sm"
                      variant="outline"
                      borderRadius="lg"
                      onClick={() => setVisibleCount((c) => c + 120)}
                    >
                      {t("steam.loadMore")} ({filteredInventory.length - visibleCount})
                    </Button>
                  </VStack>
                )}
              </>
            )}
          </Collapse>
        </Box>
      )}

      <Divider mb={6} borderColor={borderColor} />

      {/* 已安装游戏列表 */}
      <HStack spacing={2} mb={4}>
        <Box color={activeColor}><LuGamepad2 size={18} /></Box>
        <Text fontSize="lg" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("steam.installedGames")}
        </Text>
        <Badge colorScheme="teal" variant="subtle" borderRadius="full">
          {filteredGames.length}
          {searchText && ` / ${data?.games.length ?? 0}`}
        </Badge>
      </HStack>

      {filteredGames.length === 0 ? (
        <VStack py={20} spacing={3}>
          <LuGamepad2 size={40} color={subTextColor} />
          <Text color={subTextColor} fontSize="md">
            {searchText ? t("steam.noSearchResults") : t("steam.noGames")}
          </Text>
        </VStack>
      ) : (
        <VStack spacing={6} align="stretch">
          {groupedGames.map((group) => (
            <Box key={group.key}>
              {/* 分组标题：盘符 + 路径 + 游戏数 + 总大小 */}
              <HStack spacing={2} mb={3} flexWrap="wrap">
                <Box color={activeColor}>
                  <LuHardDrive size={16} />
                </Box>
                <Text fontSize="md" fontWeight="bold" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} noOfLines={1}>
                  {group.driveLabel}
                </Text>
                {group.path && (
                  <Text fontSize="xs" color={subTextColor} noOfLines={1}>
                    {group.path}
                  </Text>
                )}
                {group.label && (
                  <Badge colorScheme="teal" variant="subtle" borderRadius="full">
                    {group.label}
                  </Badge>
                )}
                <Badge colorScheme="gray" variant="subtle" borderRadius="full">
                  {group.games.length} {t("steam.games")}
                </Badge>
                <Text fontSize="xs" color={subTextColor}>
                  {formatSize(group.totalSize)}
                </Text>
              </HStack>
              <SimpleGrid columns={{ base: 1, sm: 2, md: 3, lg: 4, xl: 5 }} spacing={4}>
                {group.games.map((game) => (
                  <GameCard
                    key={game.app_id}
                    game={game}
                    isLaunching={launchingAppId === game.app_id}
                    onLaunch={handleLaunch}
                    onOpenFolder={handleOpenFolder}
                    onStorePage={handleStorePage}
                    onUninstall={onUninstallClick}
                  />
                ))}
              </SimpleGrid>
            </Box>
          ))}
        </VStack>
      )}

      {/* 卸载确认弹窗 */}
      <AlertDialog
        isOpen={isUninstallOpen}
        leastDestructiveRef={cancelRef}
        onClose={onUninstallClose}
      >
        <AlertDialogOverlay>
          <AlertDialogContent bg={cardBg} borderColor={borderColor}>
            <AlertDialogHeader fontSize="lg" fontWeight="bold" color={textColor}>
              {t("steam.uninstallTitle")}
            </AlertDialogHeader>
            <AlertDialogBody color={subTextColor}>
              {t("steam.uninstallConfirm", { name: uninstallTarget?.name })}
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={onUninstallClose}>
                {t("steam.cancel")}
              </Button>
              <Button colorScheme="red" onClick={handleUninstall} ml={3}>
                {t("steam.confirmUninstall")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>

      {/* 删除账户确认弹窗 */}
      <AlertDialog
        isOpen={isDeleteOpen}
        leastDestructiveRef={cancelRef}
        onClose={onDeleteClose}
      >
        <AlertDialogOverlay>
          <AlertDialogContent bg={cardBg} borderColor={borderColor}>
            <AlertDialogHeader fontSize="lg" fontWeight="bold" color={textColor}>
              {t("steam.deleteAccountTitle")}
            </AlertDialogHeader>
            <AlertDialogBody color={subTextColor}>
              {t("steam.deleteAccountConfirm", {
                name: deleteTarget?.account_name || deleteTarget?.persona_name || deleteTarget?.steam_id64,
              })}
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={onDeleteClose}>
                {t("steam.cancel")}
              </Button>
              <Button colorScheme="red" onClick={handleDeleteAccount} ml={3}>
                {t("steam.deleteAccount")}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </Box>
  );
}
