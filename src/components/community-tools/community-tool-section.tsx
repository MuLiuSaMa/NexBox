import {
  Box,
  Flex,
  Grid,
  Text,
  Badge,
  VStack,
  HStack,
  Divider,
  Button,
  Input,
  IconButton,
  Tooltip,
  Icon,
  useColorModeValue,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalCloseButton,
  ModalFooter,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  AlertDialogCloseButton,
  Progress,
  Spinner,
} from "@chakra-ui/react";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  RefreshCw,
  Plus,
  Trash2,
  LogIn,
  User as UserIcon,
  Wrench,
  Play,
  Download,
  ExternalLink,
  Cpu,
  Monitor,
  HardDrive,
  MemoryStick,
  MousePointer,
  Gamepad2,
  Settings2,
  Boxes,
} from "lucide-react";
import { LiquidGlassToolCard } from "@/components/special/liquid-glass-tool-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useCommunityTools, type CommunityTool, type SubmitCommunityToolParams } from "@/hooks/use-community-tools";
import { GitCodeLoginDialog } from "./gitcode-login-dialog";
import { CommunityToolSubmitDialog } from "./community-tool-submit-dialog";

const GLYPH_MAP: Record<string, React.ElementType> = {
  处理器: Cpu,
  显卡: Monitor,
  显示器: Monitor,
  硬盘: HardDrive,
  内存: MemoryStick,
  外设: MousePointer,
  游戏: Gamepad2,
  系统: Settings2,
  综合: Boxes,
};

function glyphForCategory(category: string): React.ElementType {
  for (const [k, v] of Object.entries(GLYPH_MAP)) {
    if (category.includes(k)) return v;
  }
  return Wrench;
}

/** 用系统默认浏览器打开外部链接（优先 opener 插件，其次 shell，最后 window.open）。自动补 https:// 协议 */
async function openExternal(url: string) {
  const full = /^[a-zA-Z]+:/.test(url) ? url : `https://${url}`;
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(full);
    return;
  } catch {
    /* fallthrough */
  }
  try {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(full);
  } catch {
    window.open(full, "_blank");
  }
}

function formatTime(iso?: string | null): string {
  if (!iso) return "未知";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function downloadSourceText(tool: CommunityTool | null): string {
  if (!tool) return "-";
  if (tool.download_url) return "外部链接";
  if (tool.file) return "仓库压缩包";
  return "-";
}

function DetailRow({ label, value }: { label: string; value?: string | null }) {
  const textColor = useColorModeValue("gray.600", "#cccccc");
  return (
    <HStack align="start">
      <Text w="64px" flexShrink={0} opacity={0.6}>
        {label}
      </Text>
      <Text color={textColor} wordBreak="break-all">
        {value || "-"}
      </Text>
    </HStack>
  );
}

function CommunityToolCard({
  tool,
  categoryColor,
  isAuthorView,
  onAction,
  onDelete,
}: {
  tool: CommunityTool;
  categoryColor: string;
  isAuthorView: boolean;
  onAction: (tool: CommunityTool) => void;
  onDelete: (tool: CommunityTool) => void;
}) {
  const iconColor = useColorModeValue("gray.700", "#cccccc");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const bg = useColorModeValue("gray.100", "#222222");

  const installed = tool.install_status === "installed";
  const GlyphIcon = glyphForCategory(tool.category);

  return (
    <LiquidGlassToolCard size="md" cursor="pointer" onClick={() => onAction(tool)} position="relative" h="full">
      {isAuthorView && (
        <Tooltip label="删除" placement="top">
          <IconButton
            aria-label="delete"
            icon={<Icon as={Trash2} boxSize={3} />}
            size="xs"
            variant="ghost"
            colorScheme="red"
            position="absolute"
            top={2}
            right={2}
            onClick={(e) => {
              e.stopPropagation();
              onDelete(tool);
            }}
          />
        </Tooltip>
      )}
      <VStack align="start" spacing={3} h="full">
        <Flex h={12} w={12} align="center" justify="center" borderRadius="lg" bg={bg} overflow="hidden">
          {tool.icon_url ? (
            // eslint-disable-next-line jsx-a11y/alt-text
            <img src={tool.icon_url} alt={tool.name} width={32} height={32} style={{ objectFit: "contain" }} />
          ) : (
            <Icon as={GlyphIcon} boxSize={6} color={iconColor} />
          )}
        </Flex>
        <Box flex={1} w="full">
          <HStack justify="space-between" align="start" mb={1}>
            <Text fontSize="sm" fontWeight="semibold" color={titleColor} noOfLines={1}>
              {tool.name}
            </Text>
            <Badge fontSize="xs" variant="subtle" colorScheme="purple" ml={2} flexShrink={0}>
              {tool.category}
            </Badge>
          </HStack>
          <Text fontSize="xs" color={descColor} lineHeight="short" noOfLines={2} mb={1}>
            {tool.description || "-"}
          </Text>
          <HStack spacing={1} flexWrap="wrap">
            {tool.tags.slice(0, 3).map((tag) => (
              <Badge key={tag} fontSize="xs" variant="outline" colorScheme="gray">
                {tag}
              </Badge>
            ))}
          </HStack>
          {tool.author && (
            <Text fontSize="xs" color={categoryColor} mt={1}>
              @{tool.author}
            </Text>
          )}
          <HStack spacing={1} mt={2} color={installed ? "green.500" : "blue.400"}>
            <Icon as={installed ? Play : Download} boxSize={3} />
            <Text fontSize="xs">{installed ? "已下载" : "未下载"}</Text>
          </HStack>
        </Box>
      </VStack>
    </LiquidGlassToolCard>
  );
}

export function CommunityToolSection() {
  const { t } = useTranslation();
  const { getActiveColor, getHoverColor, getContrastTextColor, getBorderColor } = useThemeColor();
  const toast = useDynamicIsland("wrench");
  const {
    tools,
    categories,
    loading,
    error,
    loginStatus,
    submitting,
    submitProgress,
    removing,
    removeProgress,
    installing,
    installPercent,
    installMessage,
    refresh,
    login,
    logout,
    isAuthor,
    install,
    openZip,
    downloadDir,
    pickDownloadDir,
    submit,
    remove,
  } = useCommunityTools();

  const sectionTitleColor = useColorModeValue("gray.800", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  const iconColor = useColorModeValue("gray.700", "#cccccc");

  const [category, setCategory] = useState("all");
  const [search, setSearch] = useState("");
  const [loginOpen, setLoginOpen] = useState(false);
  const [submitOpen, setSubmitOpen] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [installTool, setInstallTool] = useState<CommunityTool | null>(null);
  const [detailTool, setDetailTool] = useState<CommunityTool | null>(null);
  const [deleteTool, setDeleteTool] = useState<CommunityTool | null>(null);
  const [prLink, setPrLink] = useState<string | null>(null);
  const cancelRef = useRef(null);

  // 主题化样式：主按钮 / 滚动条 / 输入框聚焦态，全部跟随应用主题色
  const themePrimaryBtn = {
    bg: getActiveColor(),
    color: getContrastTextColor(),
    _hover: { bg: getHoverColor() },
  };
  const themedScrollbar = {
    "&::-webkit-scrollbar": { width: "6px", height: "6px" },
    "&::-webkit-scrollbar-thumb": { background: getActiveColor(), borderRadius: "3px" },
    "&::-webkit-scrollbar-track": { background: "transparent" },
  };
  const themeFocus = { focusBorderColor: getActiveColor() };

  const filtered = useMemo(() => {
    let list = tools;
    if (category !== "all") list = list.filter((tool) => tool.category === category);
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      list = list.filter(
        (tool) =>
          tool.name.toLowerCase().includes(q) ||
          (tool.description?.toLowerCase().includes(q) ?? false) ||
          tool.tags.some((tag) => tag.toLowerCase().includes(q))
      );
    }
    return list;
  }, [tools, category, search]);

  // 详情弹窗使用列表中的最新状态（安装后能实时变成"打开"）
  const detailLive = detailTool
    ? tools.find((x) => x.id === detailTool.id && x.category === detailTool.category) ?? detailTool
    : null;

  const openSubmit = () => {
    if (!loginStatus.logged_in) {
      setLoginOpen(true);
    } else {
      setSubmitOpen(true);
    }
  };

  const doLogin = async (): Promise<boolean> => {
    setConnecting(true);
    const ok = await login();
    setConnecting(false);
    if (ok) toast({ title: "登录成功", status: "success", duration: 2000 });
    return ok;
  };

  const handleAction = (tool: CommunityTool) => {
    // 点击卡片 → 打开详情弹窗
    setDetailTool(tool);
  };

  const confirmInstall = async () => {
    if (!installTool) return;
    // 先关闭弹窗，再后台安装，避免下载时弹窗一直挂起
    const tool = installTool;
    setInstallTool(null);
    try {
      await install(tool);
      toast({ title: "下载完成", status: "success", duration: 2000 });
      await refresh();
    } catch (e) {
      toast({ title: "下载失败", description: String(e), status: "error", duration: 3000 });
    }
  };

  const confirmDelete = async () => {
    if (!deleteTool) return;
    const tool = deleteTool;
    setDeleteTool(null);
    if (!loginStatus.logged_in) {
      setLoginOpen(true);
      return;
    }
    try {
      const url = await remove(tool);
      setPrLink(url);
    } catch (e) {
      toast({ title: "删除失败", description: String(e), status: "error", duration: 3000 });
    }
  };

  const handleSubmit = async (params: SubmitCommunityToolParams) => {
    try {
      const url = await submit(params);
      setSubmitOpen(false);
      setPrLink(url);
    } catch (e) {
      toast({ title: "提交失败", description: String(e), status: "error", duration: 4000 });
    }
  };

  return (
    <Box mb={8}>
      <HStack mb={4} spacing={3}>
        <Text fontSize="lg" fontWeight="bold" color={sectionTitleColor}>
          {t("tools.community.title")}
        </Text>
        <Badge fontSize="xs" variant="subtle" color={getActiveColor()} bg={`${getActiveColor()}20`}>
          {t("tools.community.tag")}
        </Badge>
        <Badge fontSize="xs" variant="solid" color={getContrastTextColor()} bg={getActiveColor()}>
          BETA
        </Badge>
        <Badge fontSize="xs" colorScheme="gray">
          {tools.length}
        </Badge>
        <Box flex={1} />
        <IconButton aria-label="refresh" icon={<RefreshCw size={16} />} size="sm" variant="ghost" onClick={refresh} isLoading={loading} color={getActiveColor()} />
        {loginStatus.logged_in && loginStatus.user ? (
          <Button size="sm" variant="ghost" leftIcon={<UserIcon size={15} />} color={getActiveColor()} onClick={() => setLoginOpen(true)}>
            {loginStatus.user.login}
          </Button>
        ) : (
          <Button size="sm" leftIcon={<LogIn size={15} />} onClick={() => setLoginOpen(true)} {...themePrimaryBtn}>
            {t("tools.community.login")}
          </Button>
        )}
        <Button size="sm" leftIcon={<Plus size={15} />} onClick={openSubmit} {...themePrimaryBtn}>
          {t("tools.community.submitBtn")}
        </Button>
      </HStack>
      <Divider borderColor={dividerColor} mb={4} />

      <HStack spacing={3} mb={4} alignItems="center">
        <Text fontSize="sm" fontWeight="medium" color={sectionTitleColor} whiteSpace="nowrap">
          {t("tools.community.downloadDir")}
        </Text>
        <Text fontSize="xs" color={useColorModeValue("gray.500", "#999999")} isTruncated maxW="340px">
          {downloadDir}
        </Text>
        <Button size="xs" variant="outline" color={getActiveColor()} borderColor={getActiveColor()} onClick={pickDownloadDir}>
          {t("tools.community.changeDir")}
        </Button>
      </HStack>

      <HStack spacing={3} mb={4}>
        <CustomSelect
        width="150px"
        value={category}
        onChange={setCategory}
        options={[
          { value: "all", label: t("tools.community.allCategories") },
          ...categories.map((c) => ({ value: c, label: c })),
        ]}
      />
        <Input placeholder={t("tools.community.search")} value={search} onChange={(e) => setSearch(e.target.value)} size="sm" flex={1} maxW="320px" borderRadius="lg" {...themeFocus} />
      </HStack>

      {error && (
        <Text fontSize="sm" color="red.400" mb={3}>
          {t("tools.community.loadFailed")}：{error}
        </Text>
      )}

      {loading && (
        <Flex justify="center" py={10}>
          <HStack spacing={3}>
            <Spinner color={getActiveColor()} />
            <Text fontSize="sm" color={useColorModeValue("gray.600", "#cccccc")}>
              正在获取社区工具...
            </Text>
          </HStack>
        </Flex>
      )}

      {!loading && filtered.length === 0 && (
        <Box py={10} textAlign="center" border="1px dashed" borderColor={dividerColor} borderRadius="xl">
          <Icon as={Wrench} boxSize={8} color={iconColor} opacity={0.5} />
          <Text fontSize="sm" color={sectionTitleColor} mt={2}>
            {t("tools.community.empty")}
          </Text>
          <Button size="sm" mt={3} leftIcon={<Plus size={14} />} onClick={openSubmit} {...themePrimaryBtn}>
            {t("tools.community.submitBtn")}
          </Button>
        </Box>
      )}

      {!loading && filtered.length > 0 && (
        <Grid
          templateColumns="repeat(auto-fill, minmax(240px, 1fr))"
          gap={4}
          alignItems="stretch"
        >
          {filtered.map((tool) => (
            <CommunityToolCard
              key={`${tool.category}/${tool.id}`}
              tool={tool}
              categoryColor={getActiveColor()}
              isAuthorView={isAuthor(tool)}
              onAction={handleAction}
              onDelete={(t) => setDeleteTool(t)}
            />
          ))}
        </Grid>
      )}

      <GitCodeLoginDialog
        isOpen={loginOpen}
        onClose={() => setLoginOpen(false)}
        loginStatus={loginStatus}
        onLogin={doLogin}
        onLogout={logout}
        connecting={connecting}
      />

      <CommunityToolSubmitDialog
        isOpen={submitOpen}
        onClose={() => setSubmitOpen(false)}
        submitting={submitting}
        submitProgress={submitProgress}
        onSubmit={handleSubmit}
      />

      <AlertDialog isOpen={!!installTool} leastDestructiveRef={cancelRef as never} onClose={() => setInstallTool(null)}>
        <AlertDialogOverlay backdropFilter="blur(4px)" />
        <AlertDialogContent bg={useColorModeValue("white", "#000000")}>
          <AlertDialogHeader fontSize="lg" fontWeight="bold">
            {installTool?.name}
          </AlertDialogHeader>
          <AlertDialogCloseButton />
          <AlertDialogBody>
            <Text fontSize="sm">
              {t("tools.community.trustWarning")} @{installTool?.author ?? t("tools.community.unknownAuthor")}
            </Text>
          </AlertDialogBody>
          <AlertDialogFooter>
            <Button ref={cancelRef as never} onClick={() => setInstallTool(null)}>
              {t("tools.community.cancel")}
            </Button>
            <Button ml={3} {...themePrimaryBtn} onClick={confirmInstall}>
              {t("tools.community.confirmInstall")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* 详情弹窗：完整信息 + 下载/打开 + 进度条 */}
      <Modal isOpen={!!detailTool} onClose={() => setDetailTool(null)} isCentered size="lg" scrollBehavior="inside" closeOnOverlayClick={!installing}>
        <ModalOverlay backdropFilter="blur(4px)" />
        <ModalContent bg={useColorModeValue("white", "#000000")}>
          <ModalHeader fontSize="lg">
            {detailLive?.name}
            {detailLive && (
              <Badge ml={2} fontSize="xs" variant="subtle" colorScheme="purple">
                {detailLive.category}
              </Badge>
            )}
          </ModalHeader>
          <ModalCloseButton isDisabled={installing} />
          <ModalBody sx={themedScrollbar}>
            {detailLive?.description ? (
              <Text fontSize="sm" mb={3} color={useColorModeValue("gray.700", "#cccccc")}>
                {detailLive.description}
              </Text>
            ) : null}
            <VStack align="stretch" spacing={1.5} fontSize="sm">
              <DetailRow label="分类" value={detailLive?.category} />
              <DetailRow label="版本" value={detailLive?.version ?? "未知"} />
              <DetailRow label="发布者" value={detailLive?.publisher ?? "未知"} />
              <DetailRow label="提交者" value={detailLive?.author ?? "未知"} />
              <DetailRow label="提交时间" value={formatTime(detailLive?.submitted_at)} />
              <DetailRow label="标签" value={detailLive?.tags?.join("  ") ?? "-"} />
              <DetailRow label="下载源" value={downloadSourceText(detailLive)} />
              <HStack align="start">
                <Text w="64px" flexShrink={0} opacity={0.6}>
                  官网
                </Text>
                {detailLive?.homepage ? (
                  <Button
                    size="xs"
                    variant="link"
                    color={getActiveColor()}
                    leftIcon={<ExternalLink size={12} />}
                    onClick={() => detailLive.homepage && openExternal(detailLive.homepage)}
                  >
                    {detailLive.homepage}
                  </Button>
                ) : (
                  <Text color={useColorModeValue("gray.600", "#cccccc")}>-</Text>
                )}
              </HStack>
            </VStack>
          </ModalBody>
          <ModalFooter>
            {installing ? (
              <VStack w="full" align="stretch" spacing={2}>
                <Progress
                  size="sm"
                  value={installPercent ?? undefined}
                  isIndeterminate={installPercent == null}
                  colorScheme="blue"
                  sx={{ "& > div": { background: getActiveColor() } }}
                />
                <Text fontSize="xs" color={useColorModeValue("gray.600", "#cccccc")}>
                  {installMessage ?? "下载中..."}
                  {installPercent != null ? `  ${installPercent}%` : ""}
                </Text>
              </VStack>
            ) : (
              <HStack w="full" justify="flex-end" spacing={3}>
                <Button variant="ghost" onClick={() => setDetailTool(null)}>
                  {t("tools.community.close")}
                </Button>
                {detailLive?.install_status === "installed" ? (
                  <Button
                    {...themePrimaryBtn}
                    leftIcon={<Play size={15} />}
                    onClick={async () => {
                      try {
                        await openZip(detailLive);
                      } catch (e) {
                        toast({ title: "打开失败", description: String(e), status: "error", duration: 3000 });
                      }
                    }}
                  >
                    打开
                  </Button>
                ) : (
                  <Button {...themePrimaryBtn} leftIcon={<Download size={15} />} onClick={() => detailLive && setInstallTool(detailLive)}>
                    下载
                  </Button>
                )}
              </HStack>
            )}
          </ModalFooter>
        </ModalContent>
      </Modal>

      <AlertDialog isOpen={!!deleteTool} leastDestructiveRef={cancelRef as never} onClose={() => setDeleteTool(null)}>
        <AlertDialogOverlay backdropFilter="blur(4px)" />
        <AlertDialogContent bg={useColorModeValue("white", "#000000")}>
          <AlertDialogHeader fontSize="lg" fontWeight="bold">
            {t("tools.community.deleteTitle")}
          </AlertDialogHeader>
          <AlertDialogCloseButton />
          <AlertDialogBody fontSize="sm">{t("tools.community.deleteDesc")}</AlertDialogBody>
          <AlertDialogFooter>
            <Button ref={cancelRef as never} onClick={() => setDeleteTool(null)}>
              {t("tools.community.cancel")}
            </Button>
            <Button colorScheme="red" ml={3} onClick={confirmDelete}>
              {t("tools.community.confirmDelete")}
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* 删除进行中：正在提交 PR */}
      <Modal isOpen={removing} onClose={() => {}} isCentered size="sm" closeOnOverlayClick={false} closeOnEsc={false}>
        <ModalOverlay backdropFilter="blur(4px)" />
        <ModalContent bg={useColorModeValue("white", "#000000")}>
          <ModalHeader fontSize="md">正在提交 PR</ModalHeader>
          <ModalBody pb={4}>
            <HStack spacing={3}>
              <Spinner color={getActiveColor()} />
              <Text fontSize="sm" color={useColorModeValue("gray.600", "#cccccc")}>
                {removeProgress ?? "正在提交删除请求..."}
              </Text>
            </HStack>
          </ModalBody>
        </ModalContent>
      </Modal>

      <Modal isOpen={!!prLink} onClose={() => setPrLink(null)} isCentered size="md">
        <ModalOverlay backdropFilter="blur(4px)" />
        <ModalContent>
          <ModalHeader fontSize="lg">{t("tools.community.prCreated")}</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <Text fontSize="sm" mb={3}>
              {t("tools.community.prDesc")}
            </Text>
            <Button
              {...themePrimaryBtn}
              leftIcon={<ExternalLink size={16} />}
              onClick={() => {
                if (prLink) openExternal(prLink);
                setPrLink(null);
              }}
            >
              {t("tools.community.viewPr")}
            </Button>
          </ModalBody>
          <ModalFooter>
            <Button onClick={() => setPrLink(null)}>{t("tools.community.close")}</Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}