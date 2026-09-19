import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Button,
  Input,
  Badge,
  Image,
  Skeleton,
  Grid,
  GridItem,
  IconButton,
  Tooltip,
  useColorModeValue,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalCloseButton,
  AlertDialog,
  AlertDialogBody,
  AlertDialogContent,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogOverlay,
} from "@chakra-ui/react";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ArrowLeft,
  RefreshCw,
  Search,
  Play,
  Trash2,
  FolderOpen,
  AppWindow,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useBackground } from "@/contexts/background-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface InstalledApp {
  name: string;
  display_version: string | null;
  publisher: string | null;
  install_location: string | null;
  install_date: string | null;
  estimated_size_kb: number | null;
  display_icon: string | null;
  app_path: string | null;
  uninstall_string: string | null;
  quiet_uninstall_string: string | null;
  registry_source: string;
}

type SortKey = "name" | "publisher" | "date" | "size";

/** 注册表 EstimatedSize（KB）=> 可读大小 */
function formatSize(kb: number | null): string {
  if (!kb || kb <= 0) return "—";
  if (kb < 1024 * 1024) return `${(kb / 1024).toFixed(1)} MB`;
  return `${(kb / 1024 / 1024).toFixed(2)} GB`;
}

/** 注册表 InstallDate（YYYYMMDD / YYYY-MM-DD）=> YYYY-MM-DD */
function formatDate(raw: string | null): string {
  if (!raw) return "—";
  const s = raw.trim();
  if (/^\d{8}$/.test(s)) {
    return `${s.slice(0, 4)}-${s.slice(4, 6)}-${s.slice(6, 8)}`;
  }
  return s;
}

function AppIcon({
  name,
  uri,
  size = 48,
}: {
  name: string;
  uri: string;
  size?: number;
}) {
  const bg = useColorModeValue("#F1F2F4", "#2A2A2A");
  const textColor = useColorModeValue("#5A5A5A", "#BDBDBD");
  return (
    <Box
      w={`${size}px`}
      h={`${size}px`}
      minW={`${size}px`}
      borderRadius="8px"
      bg={bg}
      display="flex"
      alignItems="center"
      justifyContent="center"
      overflow="hidden"
      flexShrink={0}
    >
      {uri ? (
        <Image src={uri} w="full" h="full" objectFit="cover" draggable={false} />
      ) : (
        <Text fontSize="lg" fontWeight="bold" color={textColor}>
          {name.trim().charAt(0).toUpperCase() || "?"}
        </Text>
      )}
    </Box>
  );
}

export default function AppManagerPage() {
  const toast = useDynamicIsland("rocket");
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const { liquidGlassEnabled, liquidGlassBlur } = useBackground();
  const navigate = useNavigate();
  const adaptiveTitle = useAdaptiveTextColor();

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#a0a0a0");
  const secondaryColor = useColorModeValue("gray.500", "#888888");
  const appNameColor = useColorModeValue("gray.800", "#ffffff");
  const inputTextColor = useColorModeValue("gray.700", "#ffffff");
  // 输入框适配液态玻璃：开启时用半透明玻璃底 + 轻微高透，关闭时用实底
  const inputBg = useColorModeValue(
    liquidGlassEnabled ? "rgba(255,255,255,0.25)" : "rgba(255,255,255,0.9)",
    liquidGlassEnabled ? "rgba(0,0,0,0.25)" : "rgba(17,17,17,0.95)"
  );
  const inputBorderColor = useColorModeValue(
    liquidGlassEnabled ? "rgba(255,255,255,0.2)" : "rgba(200,200,200,0.3)",
    liquidGlassEnabled ? "rgba(255,255,255,0.1)" : "rgba(51,51,51,0.5)"
  );
  const placeholderColor = useColorModeValue("gray.500", "gray.400");
  const activeBg = themeConfig.primaryColor;
  const effectiveBlur = liquidGlassEnabled ? liquidGlassBlur : 0;
  const dangerColor = useColorModeValue("red.500", "red.400");

  const [apps, setApps] = useState<InstalledApp[]>([]);
  const [icons, setIcons] = useState<Record<string, string>>({});
  const [isLoading, setIsLoading] = useState(true);
  const [keyword, setKeyword] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("name");
  const [opening, setOpening] = useState<Set<string>>(new Set());
  const [uninstalling, setUninstalling] = useState<Set<string>>(new Set());
  const [confirmApp, setConfirmApp] = useState<InstalledApp | null>(null);
  const [detailApp, setDetailApp] = useState<InstalledApp | null>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  const loadApps = useMemo(
    () => async (silent = false) => {
      if (!silent) setIsLoading(true);
      try {
        const list = await invoke<InstalledApp[]>("list_installed_apps");
        setApps(list);
        setIcons({});
        // 批量懒加载图标（不阻塞列表渲染）
        try {
          const iconUris = await invoke<string[]>("get_app_icons", {
            items: list.map((a) => ({ display_icon: a.display_icon, app_path: a.app_path })),
          });
          const next: Record<string, string> = {};
          list.forEach((a, i) => {
            if (iconUris[i]) next[a.name] = iconUris[i];
          });
          setIcons(next);
        } catch (e) {
          console.error("Failed to load app icons:", e);
        }
      } catch (e) {
        console.error("Failed to list installed apps:", e);
        toast({
          title: "加载应用列表失败",
          description: String(e),
          status: "error",
          duration: 3000,
        });
      } finally {
        setIsLoading(false);
      }
    },
    [toast]
  );

  useEffect(() => {
    loadApps();
  }, [loadApps]);

  const filtered = useMemo(() => {
    const kw = keyword.trim().toLowerCase();
    let list = apps;
    if (kw) {
      list = list.filter(
        (a) =>
          a.name.toLowerCase().includes(kw) ||
          (a.publisher ?? "").toLowerCase().includes(kw) ||
          (a.install_location ?? "").toLowerCase().includes(kw)
      );
    }
    const sorted = [...list];
    switch (sortKey) {
      case "name":
        sorted.sort((a, b) => a.name.localeCompare(b.name, "zh-Hans-CN"));
        break;
      case "publisher":
        sorted.sort((a, b) =>
          (a.publisher ?? "").localeCompare(b.publisher ?? "", "zh-Hans-CN")
        );
        break;
      case "date":
        sorted.sort((a, b) => (b.install_date ?? "").localeCompare(a.install_date ?? ""));
        break;
      case "size":
        sorted.sort((a, b) => (b.estimated_size_kb ?? 0) - (a.estimated_size_kb ?? 0));
        break;
    }
    return sorted;
  }, [apps, keyword, sortKey]);

  const handleOpen = async (app: InstalledApp) => {
    setOpening((prev) => new Set(prev).add(app.name));
    try {
      await invoke("open_app", { app });
    } catch (e) {
      console.error("Failed to open app:", e);
      toast({
        title: "打开应用失败",
        description: String(e),
        status: "error",
        duration: 3000,
      });
    }
    setOpening((prev) => {
      const next = new Set(prev);
      next.delete(app.name);
      return next;
    });
  };

  const handleUninstallConfirm = async () => {
    if (!confirmApp) return;
    const app = confirmApp;
    setConfirmApp(null);
    setUninstalling((prev) => new Set(prev).add(app.name));
    try {
      await invoke("uninstall_app", { app, quiet: false });
      toast({
        title: `已启动 ${app.name} 的卸载程序`,
        description: "卸载器可能弹出 UAC 授权窗口，请按提示操作",
        status: "info",
        duration: 4000,
      });
    } catch (e) {
      console.error("Failed to uninstall app:", e);
      toast({
        title: "启动卸载失败",
        description: String(e),
        status: "error",
        duration: 3000,
      });
    }
    setUninstalling((prev) => {
      const next = new Set(prev);
      next.delete(app.name);
      return next;
    });
  };

  const openDisabled = (app: InstalledApp) =>
    !app.app_path && !app.install_location;

  return (
    <VStack align="stretch" spacing={5} pt={8} px={1}>
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/builtin-tools")}
          color={headingColor}
        >
          返回
        </Button>
        <Heading
          size="lg"
          color={adaptiveTitle.text}
          textShadow={adaptiveTitle.shadow}
          fontWeight="700"
        >
          应用管理
        </Heading>
        <HStack spacing={2}>
          <Tooltip label="刷新列表" hasArrow>
            <IconButton
              aria-label="刷新"
              icon={<RefreshCw size={18} />}
              variant="ghost"
              onClick={() => loadApps()}
              isLoading={isLoading}
              color={headingColor}
            />
          </Tooltip>
          <Box w="10px" />
        </HStack>
      </HStack>

      <HStack spacing={3} w="full">
        <Box position="relative" flex={1} minW="200px">
          <Search
            size={16}
            style={{
              position: "absolute",
              left: 12,
              top: "50%",
              transform: "translateY(-50%)",
              color: placeholderColor,
              zIndex: 1,
            }}
          />
          <Input
            placeholder="搜索应用名称 / 发布者 / 路径"
            value={keyword}
            onChange={(e) => setKeyword(e.target.value)}
            bg={inputBg}
            borderColor={inputBorderColor}
            color={inputTextColor}
            pl={9}
            borderRadius="8px"
            _placeholder={{ color: placeholderColor }}
            _focus={{
              borderColor: activeBg,
              boxShadow: `0 0 0 1px ${activeBg}`,
            }}
            backdropFilter={`blur(${effectiveBlur}px)`}
            sx={{
              transform: "translateZ(0)",
              WebkitTransform: "translateZ(0)",
              transition: "backdrop-filter 0.45s cubic-bezier(0.4, 0, 0.2, 1)",
            }}
          />
        </Box>
        <CustomSelect
          value={sortKey}
          onChange={(v) => setSortKey(v as SortKey)}
          width="180px"
          options={[
            { value: "name", label: "按名称排序" },
            { value: "publisher", label: "按发布者排序" },
            { value: "date", label: "按安装日期排序" },
            { value: "size", label: "按占用大小排序" },
          ]}
        />
      </HStack>

      <HStack spacing={1}>
        <Text fontSize="sm" color={subTextColor}>
          共 {filtered.length} 个应用
          {keyword.trim() && `（匹配 "${keyword.trim()}"）`}
        </Text>
      </HStack>

      {isLoading ? (
        <Grid templateColumns="repeat(auto-fill, minmax(340px, 1fr))" gap={4}>
          {Array.from({ length: 6 }).map((_, i) => (
            <LiquidGlassCard key={i} p={4} borderRadius="8px">
              <HStack spacing={3}>
                <Skeleton w="48px" h="48px" borderRadius="8px" />
                <VStack align="start" spacing={1.5} flex={1}>
                  <Skeleton h="16px" w="60%" />
                  <Skeleton h="12px" w="40%" />
                </VStack>
              </HStack>
            </LiquidGlassCard>
          ))}
        </Grid>
      ) : filtered.length === 0 ? (
        <LiquidGlassCard p={10} borderRadius="8px">
          <VStack spacing={2} py={8}>
            <AppWindow size={40} color={secondaryColor} />
            <Text color={subTextColor} fontWeight="medium">
              {apps.length === 0 ? "未检测到已安装的应用" : "没有匹配的应用"}
            </Text>
          </VStack>
        </LiquidGlassCard>
      ) : (
        <Grid templateColumns="repeat(auto-fill, minmax(340px, 1fr))" gap={4}>
          {filtered.map((app, idx) => {
            const busy = uninstalling.has(app.name) || opening.has(app.name);
            return (
              <LiquidGlassCard
                key={`${app.name}-${idx}`}
                p={4}
                borderRadius="8px"
                transition="all 0.2s"
                cursor="pointer"
                onClick={() => setDetailApp(app)}
              >
                <HStack align="start" spacing={3}>
                  <AppIcon name={app.name} uri={icons[app.name]} />
                  <VStack align="start" spacing={0.5} flex={1} minW={0}>
                    <HStack spacing={2} maxW="full">
                      <Text
                        fontWeight="600"
                        fontSize="sm"
                        noOfLines={1}
                        color={appNameColor}
                      >
                        {app.name}
                      </Text>
                      {app.display_version && (
                        <Badge
                          fontSize="10px"
                          colorScheme="blue"
                          borderRadius="6px"
                          variant="subtle"
                          flexShrink={0}
                        >
                          v{app.display_version}
                        </Badge>
                      )}
                    </HStack>
                    <Text fontSize="xs" color={subTextColor} noOfLines={1}>
                      {app.publisher ?? "未知发布者"}
                    </Text>
                    <HStack spacing={3} mt={0.5} fontSize="xs" color={secondaryColor}>
                      <Text>安装于 {formatDate(app.install_date)}</Text>
                      <Text>占用 {formatSize(app.estimated_size_kb)}</Text>
                    </HStack>
                  </VStack>
                </HStack>

                <HStack spacing={2} mt={3} w="full">
                  <Button
                    size="sm"
                    flex={1}
                    leftIcon={<Play size={14} />}
                    borderRadius="8px"
                    bg={themeConfig.primaryColor}
                    color={getContrastTextColor()}
                    _hover={{ opacity: 0.92 }}
                    isDisabled={openDisabled(app) || busy}
                    isLoading={opening.has(app.name)}
                    loadingText="打开中"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleOpen(app);
                    }}
                  >
                    打开
                  </Button>
                  {app.install_location ? (
                    <Button
                      size="sm"
                      variant="outline"
                      borderRadius="8px"
                      leftIcon={<FolderOpen size={14} />}
                      isDisabled={busy}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleOpen({ ...app, app_path: null });
                      }}
                    >
                      目录
                    </Button>
                  ) : null}
                  <Tooltip
                    label={app.uninstall_string || app.quiet_uninstall_string ? "卸载" : "该应用未提供卸载程序"}
                    hasArrow
                  >
                    <Button
                      size="sm"
                      variant="ghost"
                      borderRadius="8px"
                      color={dangerColor}
                      leftIcon={<Trash2 size={14} />}
                      isDisabled={
                        !(app.uninstall_string || app.quiet_uninstall_string) || busy
                      }
                      isLoading={uninstalling.has(app.name)}
                      loadingText="卸载中"
                      onClick={(e) => {
                        e.stopPropagation();
                        setConfirmApp(app);
                      }}
                    >
                      卸载
                    </Button>
                  </Tooltip>
                </HStack>
              </LiquidGlassCard>
            );
          })}
        </Grid>
      )}

      {/* 卸载确认 */}
      <AlertDialog
        isOpen={!!confirmApp}
        leastDestructiveRef={cancelRef}
        onClose={() => setConfirmApp(null)}
      >
        <AlertDialogOverlay>
          <AlertDialogContent borderRadius="12px">
            <AlertDialogHeader fontSize="lg" fontWeight="bold">
              确认卸载「{confirmApp?.name}」？
            </AlertDialogHeader>
            <AlertDialogBody>
              将调用该应用自带的卸载程序，可能弹出 UAC 授权窗口。
              <br />
              此操作将从本机移除该软件，请谨慎确认。
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelRef} borderRadius="8px" onClick={() => setConfirmApp(null)}>
                取消
              </Button>
              <Button
                colorScheme="red"
                borderRadius="8px"
                ml={3}
                onClick={handleUninstallConfirm}
              >
                确认卸载
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>

      {/* 详情 */}
      <Modal isOpen={!!detailApp} onClose={() => setDetailApp(null)} size="md">
        <ModalOverlay />
        <ModalContent borderRadius="12px">
          <ModalHeader>应用详情</ModalHeader>
          <ModalCloseButton />
          <ModalBody pb={6}>
            {detailApp && (
              <VStack align="stretch" spacing={2} fontSize="sm">
                <HStack spacing={3}>
                  <AppIcon name={detailApp.name} uri={icons[detailApp.name]} size={56} />
                  <Box>
                    <Text fontWeight="600" fontSize="md">
                      {detailApp.name}
                    </Text>
                    {detailApp.display_version && (
                      <Text color={secondaryColor}>v{detailApp.display_version}</Text>
                    )}
                  </Box>
                </HStack>
                <DetailRow label="发布者" value={detailApp.publisher ?? "—"} />
                <DetailRow label="占用大小" value={formatSize(detailApp.estimated_size_kb)} />
                <DetailRow label="安装日期" value={formatDate(detailApp.install_date)} />
                <DetailRow
                  label="安装路径"
                  value={detailApp.install_location ?? "—"}
                />
                <DetailRow label="可执行文件" value={detailApp.app_path ?? "—"} />
                <DetailRow
                  label="卸载命令"
                  value={detailApp.uninstall_string ?? "—"}
                />
                <DetailRow label="注册表来源" value={detailApp.registry_source} />
              </VStack>
            )}
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" borderRadius="8px" onClick={() => setDetailApp(null)}>
              关闭
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </VStack>
  );
}

function DetailRow({
  label,
  value,
}: {
  label: string;
  value: string;
}) {
  const labelColor = useColorModeValue("gray.500", "#888888");
  return (
    <HStack align="start" spacing={3}>
      <Text color={labelColor} w="76px" flexShrink={0}>
        {label}
      </Text>
      <Text flex={1} wordBreak="break-all" fontSize="xs">
        {value}
      </Text>
    </HStack>
  );
}