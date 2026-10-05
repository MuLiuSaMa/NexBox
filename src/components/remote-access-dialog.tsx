import {
  Box,
  Text,
  VStack,
  HStack,
  Switch,
  Button,
  IconButton,
  Progress,
  Divider,
  useColorModeValue,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalFooter,
  ModalCloseButton,
} from "@chakra-ui/react";
import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  Smartphone,
  RefreshCw,
  Trash2,
  ShieldCheck,
  X,
  ArrowLeft,
  ArrowLeftRight,
  Plus,
  Save,
  FolderOpen,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { hexToRgba } from "@/lib/color-utils";

interface AccessInfo {
  enabled: boolean;
  ip: string | null;
  port: number;
  url: string | null;
  pairing_code: string | null;
  code_expires_at: number;
  device_count: number;
}

interface PairedDevice {
  device_id: string;
  name: string;
  created_at: number;
  last_seen: number;
  /** 最近一次连本机的手机端 IP，老记录可能为空 */
  ip: string;
  /** 当前是否有活动 WebSocket（即手机此刻正连着） */
  connected?: boolean;
}

/** 手机端「请求配对」：免配对码，等本机用户点允许 */
interface PairRequest {
  id: string;
  ip: string;
  device_name: string;
  created_at: number;
  expires_at: number;
}

/** 单个互传文件条目（Rust transfer::TransferFile） */
interface TransferFile {
  id: string;
  name: string;
  size: number;
  /** to_device: PC 发往手机（存 outbox）；from_device: 手机发来（存 inbox） */
  direction: "to_device" | "from_device";
  created_at: number;
  acked: boolean;
  saved_path: string;
}

interface TransferLists {
  to_device: TransferFile[];
  from_device: TransferFile[];
}

/** remote-access://transfer-progress 事件载荷 */
interface TransferProgressEv {
  deviceId: string;
  fileId: string;
  done: number;
  /** 0 = 总量未知 */
  total: number;
  /** 字节/秒 */
  speed: number;
}

/** 进度表元素：带收到时间，超龄（传输中断、事件停发）自动失效回退到静态状态 */
type ProgressEntry = TransferProgressEv & { receivedAt: number };
const PROGRESS_FRESH_MS = 3000;

const fmtSize = (n: number) => {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
};

const fmtSpeed = (bps: number) => `${fmtSize(bps)}/s`;

/**
 * 手机远程连接（局域网）弹窗，两页结构：
 * 第一页＝连接管理（开关、配对请求、设备列表；配对/加新设备时二维码居中展示）；
 * 第二页＝某台设备的文件互传——单个文件列表混排两个方向，
 * 添加即放进列表，接收方手动取；传输进度与速度两端同步显示。
 */
export function RemoteAccessDialog({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("panels");
  const { getActiveColor, getHoverColor } = useThemeColor();

  const [view, setView] = useState<"main" | "files">("main");
  const [selectedDevice, setSelectedDevice] = useState<PairedDevice | null>(null);

  const [access, setAccess] = useState<AccessInfo | null>(null);
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [pairRequests, setPairRequests] = useState<PairRequest[]>([]);
  const [secondsLeft, setSecondsLeft] = useState(0);
  const [busyControl, setBusyControl] = useState(false);

  const [transfer, setTransfer] = useState<TransferLists | null>(null);
  const [transferBusy, setTransferBusy] = useState(false);
  /** 进行中的传输进度（fileId → 进度），由后端事件驱动；带 receivedAt 用于老化失效 */
  const [progressMap, setProgressMap] = useState<Record<string, ProgressEntry>>({});
  // 有在途进度时每秒强制重渲染一次，让超龄进度自动退场（传输被中断时不会再有事件来）
  const [, setProgressTick] = useState(0);

  const textColor = useColorModeValue("gray.600", "gray.400");
  const contentColor = useColorModeValue("gray.900", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const boxBg = useColorModeValue("gray.100", "#1a1a1a");

  // 事件回调里要用最新的选中设备（deviceId 过滤），用 ref 避免反复重挂监听
  const selectedRef = useRef<PairedDevice | null>(null);
  useEffect(() => {
    selectedRef.current = selectedDevice;
  }, [selectedDevice]);

  const refresh = useCallback(async () => {
    try {
      const [a, d, r] = await Promise.all([
        invoke<AccessInfo>("cmd_get_remote_access"),
        invoke<PairedDevice[]>("cmd_list_paired_devices"),
        invoke<PairRequest[]>("cmd_list_pair_requests"),
      ]);
      setAccess(a);
      setDevices(d);
      setPairRequests(r);
    } catch {
      /* ignore */
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      refresh();
      setView("main");
    }
  }, [isOpen, refresh]);

  const refreshTransfer = useCallback(async (deviceId: string): Promise<TransferLists | null> => {
    try {
      const lists = await invoke<TransferLists>("cmd_transfer_list", { deviceId });
      setTransfer(lists);
      return lists;
    } catch {
      return null;
    }
  }, []);

  // 文件互传页有在途进度时每秒重渲染，超龄（>3s 无新事件=传输已中断）的进度自动退场
  const hasLiveProgress = view === "files" && Object.keys(progressMap).length > 0;
  useEffect(() => {
    if (!hasLiveProgress) return;
    const id = setInterval(() => setProgressTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [hasLiveProgress]);

  // 后端事件：连接状态变化（修复：手机连上后弹窗仍显示离线）、配对请求、互传变更与进度
  useEffect(() => {
    if (!isOpen) return;
    let unlisten: (() => void)[] = [];
    let disposed = false;
    const sub = (ev: string, fn: (e: { payload: unknown }) => void) =>
      listen(ev, fn)
        .then((off) => (disposed ? off() : unlisten.push(off)))
        .catch(() => {
          /* 非 Tauri 环境忽略 */
        });

    sub("remote-access://connection-changed", () => {
      void refresh();
    });
    sub("remote-access://pair-request", (e) => {
      const req = e.payload as PairRequest;
      setPairRequests((prev) => (prev.some((p) => p.id === req.id) ? prev : [...prev, req]));
      toast({
        title: `「${req.device_name}」请求配对，请确认`,
        status: "info",
        duration: 4000,
        isClosable: true,
      });
    });
    sub("remote-access://transfer-changed", () => {
      // 不整体清 progressMap（多文件并发时完成事件不能抹掉别人的进度条）；
      // 改为按最新列表裁剪：已到终态（已接收/已另存）的条目才移除其进度
      void refresh();
      const sel = selectedRef.current;
      if (sel) {
        void refreshTransfer(sel.device_id).then((lists) => {
          if (!lists) return;
          setProgressMap((prev) => {
            const all = [...lists.to_device, ...lists.from_device];
            const next: Record<string, ProgressEntry> = {};
            for (const [id, p] of Object.entries(prev)) {
              const f = all.find((x) => x.id === id);
              if (f && !f.acked && !f.saved_path) next[id] = p;
            }
            return next;
          });
        });
      }
    });
    sub("remote-access://transfer-progress", (e) => {
      const p = e.payload as TransferProgressEv;
      const sel = selectedRef.current;
      if (sel && p.deviceId !== sel.device_id) return;
      setProgressMap((prev) => ({ ...prev, [p.fileId]: { ...p, receivedAt: Date.now() } }));
    });

    return () => {
      disposed = true;
      unlisten.forEach((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, refresh, refreshTransfer]);

  // 设备列表刷新后，同步正在查看的设备的在线状态
  useEffect(() => {
    if (!selectedDevice) return;
    const fresh = devices.find((d) => d.device_id === selectedDevice.device_id);
    if (fresh && fresh !== selectedDevice) setSelectedDevice(fresh);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [devices]);

  // 文件互传页：进场拉一次列表
  useEffect(() => {
    if (view === "files" && selectedDevice) refreshTransfer(selectedDevice.device_id);
  }, [view, selectedDevice, refreshTransfer]);

  // 服务被关掉时退回第一页，避免停在失效的互传会话里
  useEffect(() => {
    if (view === "files" && access && !access.enabled) setView("main");
  }, [view, access]);

  // 配对码倒计时
  useEffect(() => {
    if (!access?.enabled || !access.code_expires_at) {
      setSecondsLeft(0);
      return;
    }
    const calc = () => {
      const left = Math.max(0, Math.floor((access.code_expires_at - Date.now()) / 1000));
      setSecondsLeft(left);
      if (left <= 0) refresh();
    };
    calc();
    const id = setInterval(calc, 1000);
    return () => clearInterval(id);
  }, [access, refresh]);

  const toggleControl = async (next: boolean) => {
    setBusyControl(true);
    try {
      const a = await invoke<AccessInfo>("cmd_enable_remote_access", { on: next });
      setAccess(a);
      if (next) setDevices(await invoke<PairedDevice[]>("cmd_list_paired_devices"));
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    } finally {
      setBusyControl(false);
    }
  };

  const rotateCode = async () => {
    try {
      setAccess(await invoke<AccessInfo>("cmd_rotate_pairing_code"));
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    }
  };

  const revokeDevice = async (id: string) => {
    try {
      setDevices(await invoke<PairedDevice[]>("cmd_revoke_device", { deviceId: id }));
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    }
  };

  const resolvePairRequest = async (id: string, approve: boolean) => {
    try {
      setPairRequests(await invoke<PairRequest[]>("cmd_resolve_pair_request", { id, approve }));
      if (approve) {
        setDevices(await invoke<PairedDevice[]>("cmd_list_paired_devices"));
        toast({ title: "已允许配对", status: "success", duration: 2000, isClosable: true });
      }
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    }
  };

  // ───────── 文件互传（第二页） ─────────

  const addFiles = async () => {
    if (!selectedDevice) return;
    try {
      const picked = await openFileDialog({
        multiple: true,
        title: t("overlayPanel.remoteAccess.transferPickTitle") || "选择要发送的文件",
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      if (paths.length === 0) return;
      setTransferBusy(true);
      await invoke("cmd_transfer_add_files", { deviceId: selectedDevice.device_id, paths });
      await refreshTransfer(selectedDevice.device_id);
      toast({
        title: `${t("overlayPanel.remoteAccess.transferAddOk") || "已加入待传列表"}（${paths.length}）`,
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 3000, isClosable: true });
    } finally {
      setTransferBusy(false);
    }
  };

  const saveAsFile = async (f: TransferFile) => {
    if (!selectedDevice) return;
    try {
      const dest = await saveFileDialog({
        defaultPath: f.name,
        title: t("overlayPanel.remoteAccess.transferSaveTitle") || "保存文件",
      });
      if (!dest) return;
      setTransferBusy(true);
      await invoke("cmd_transfer_save_file", { deviceId: selectedDevice.device_id, fileId: f.id, destPath: dest });
      await refreshTransfer(selectedDevice.device_id);
      toast({
        title: t("overlayPanel.remoteAccess.transferSaveOk") || "文件已保存",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 3000, isClosable: true });
    } finally {
      setTransferBusy(false);
    }
  };

  const revealFile = async (f: TransferFile) => {
    try {
      await invoke("cmd_transfer_reveal", { deviceId: selectedDevice?.device_id, fileId: f.id });
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    }
  };

  const removeFile = async (f: TransferFile) => {
    if (!selectedDevice) return;
    try {
      await invoke("cmd_transfer_remove", { deviceId: selectedDevice.device_id, fileId: f.id });
      await refreshTransfer(selectedDevice.device_id);
      toast({
        title: t("overlayPanel.remoteAccess.transferRemoveOk") || "已删除",
        status: "info",
        duration: 1500,
        isClosable: true,
      });
    } catch (e) {
      toast({ title: String(e), status: "error", duration: 2500, isClosable: true });
    }
  };

  const fmtLeft = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const pairQrValue =
    access && access.ip && access.port > 0 && access.pairing_code
      ? JSON.stringify({ v: 1, ip: access.ip, port: access.port, code: access.pairing_code })
      : "";

  const openFilesPage = (d: PairedDevice) => {
    setSelectedDevice(d);
    setTransfer(null);
    setProgressMap({});
    setView("files");
  };

  const backToMain = () => {
    setView("main");
    setSelectedDevice(null);
    setTransfer(null);
    setProgressMap({});
  };

  // 第二页：单个列表混排两个方向，新的在前
  const allFiles: TransferFile[] = transfer
    ? [...transfer.to_device, ...transfer.from_device].sort((a, b) => b.created_at - a.created_at)
    : [];

  const deviceListBlock = (
    <VStack align="stretch" spacing={4} w="full">
      <Text fontSize="sm" fontWeight="bold">
        {t("overlayPanel.remoteAccess.pairedDevices") || "已配对设备"}
        <Text as="span" fontSize="xs" color={textColor} ml={2}>
          ({devices.length})
        </Text>
      </Text>
      {devices.map((d, i) => (
        <Box key={d.device_id}>
          {i > 0 && <Divider mb={4} opacity={0.5} />}
          {/* 不再包一层小卡，直接摊在外层大卡里 */}
          <HStack spacing={5} align="stretch">
            {/* 左：全面屏手机图形（窄边框 + 屏幕近满铺 + 顶部打孔摄像头） */}
            <Box
              flexShrink={0}
              alignSelf="center"
              w="104px"
              h="184px"
              borderRadius="26px"
              border="3px solid"
              borderColor={getActiveColor()}
              bg={hexToRgba(getActiveColor(), 0.07)}
              position="relative"
              overflow="hidden"
            >
              {/* 屏幕：近满铺，仅极窄边框 */}
              <Box
                position="absolute"
                top="5px"
                left="5px"
                right="5px"
                bottom="5px"
                borderRadius="19px"
                bg={hexToRgba(getActiveColor(), 0.12)}
              />
              {/* 屏幕上的抽象内容条，增强"点亮的全屏"观感 */}
              <Box position="absolute" top="26px" left="16px" right="16px" h="6px" borderRadius="full" bg={hexToRgba(getActiveColor(), 0.35)} />
              <Box position="absolute" top="40px" left="16px" right="40px" h="5px" borderRadius="full" bg={hexToRgba(getActiveColor(), 0.2)} />
              <Box position="absolute" top="52px" left="16px" right="28px" h="5px" borderRadius="full" bg={hexToRgba(getActiveColor(), 0.2)} />
              {/* 打孔摄像头（居中在屏内） */}
              <Box
                position="absolute"
                top="10px"
                left="50%"
                transform="translateX(-50%)"
                w="9px"
                h="9px"
                borderRadius="full"
                bg={hexToRgba(getActiveColor(), 0.55)}
              />
            </Box>
            {/* 右列：右上角名称 + 状态，下方文件互传小方块 */}
            <VStack align="stretch" spacing={3} flex={1} minW={0}>
              <HStack justify="space-between" align="flex-start" w="full">
                <VStack align="start" spacing={0} minW={0}>
                  <HStack spacing={2}>
                    <Text fontSize="sm" fontWeight="bold" isTruncated>
                      {d.name}
                    </Text>
                    <HStack spacing={1} flexShrink={0}>
                      <Box
                        w="6px"
                        h="6px"
                        borderRadius="full"
                        bg={d.connected ? "green.400" : textColor}
                        opacity={d.connected ? 1 : 0.5}
                      />
                      <Text fontSize="2xs" color={textColor}>
                        {d.connected
                          ? t("overlayPanel.remoteAccess.transferConnected") || "在线"
                          : t("overlayPanel.remoteAccess.transferOffline") || "离线"}
                      </Text>
                    </HStack>
                  </HStack>
                  <Text fontSize="xs" color={textColor}>
                    {t("overlayPanel.remoteAccess.lastSeen") || "最后在线"}{" "}
                    {d.last_seen ? new Date(d.last_seen).toLocaleString() : "-"}
                  </Text>
                </VStack>
                <IconButton
                  aria-label={t("overlayPanel.remoteAccess.revoke") || "撤销"}
                  icon={<Trash2 size={14} />}
                  size="sm"
                  variant="ghost"
                  colorScheme="red"
                  onClick={() => revokeDevice(d.device_id)}
                />
              </HStack>

              {/* 下方：文件互传小方块，整块点击进入互传页 */}
              <Box
                w="112px"
                h="112px"
                borderRadius="xl"
                bg={hexToRgba(getActiveColor(), 0.08)}
                border="1px solid"
                borderColor={hexToRgba(getActiveColor(), 0.28)}
                display="flex"
                flexDirection="column"
                alignItems="center"
                justifyContent="center"
                gap={2}
                cursor="pointer"
                transition="border-color 0.2s, background 0.2s"
                _hover={{ borderColor: getActiveColor(), bg: hexToRgba(getActiveColor(), 0.14) }}
                onClick={() => openFilesPage(d)}
              >
                <ArrowLeftRight size={22} style={{ color: getActiveColor() }} />
                <Text fontSize="xs" fontWeight="bold">
                  {t("overlayPanel.remoteAccess.fileTransfer") || "文件互传"}
                </Text>
                <Text fontSize="2xs" color={textColor}>
                  {d.connected
                    ? t("overlayPanel.remoteAccess.transferEnterHint") || "点击收发文件"
                    : t("overlayPanel.remoteAccess.transferEnterOffline") || "离线可暂存"}
                </Text>
              </Box>
            </VStack>
          </HStack>
        </Box>
      ))}
    </VStack>
  );

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="lg">
      <ModalOverlay />
      <ModalContent bg={modalBg} color={contentColor} border="1px solid" borderColor={modalBorderColor} borderRadius="xl">
        <ModalHeader>
          <HStack spacing={2}>
            {view === "files" && (
              <IconButton
                aria-label="back"
                icon={<ArrowLeft size={16} />}
                size="xs"
                variant="ghost"
                onClick={backToMain}
              />
            )}
            <Text>
              {view === "main"
                ? t("overlayPanel.remoteAccess.title") || "手机远程连接"
                : `${t("overlayPanel.remoteAccess.fileTransfer") || "文件互传"} · ${selectedDevice?.name ?? ""}`}
            </Text>
          </HStack>
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          {view === "main" ? (
            <VStack align="stretch" spacing={4}>
              {/* 远程控制 */}
              <Box>
                <HStack justify="space-between" align="center">
                  <HStack spacing={2}>
                    <Smartphone size={16} style={{ color: getActiveColor() }} />
                    <Text fontSize="sm" fontWeight="bold">
                      {t("overlayPanel.remoteAccess.control") || "远程控制（新境盒-安卓端）"}
                    </Text>
                  </HStack>
                  <Switch
                    isChecked={!!access?.enabled}
                    isDisabled={busyControl}
                    onChange={(e) => toggleControl(e.target.checked)}
                    size="md"
                    sx={{ "& .chakra-switch__track[data-checked]": { bg: getActiveColor() } }}
                  />
                </HStack>
                <Text fontSize="xs" color={textColor} mt={1}>
                  {t("overlayPanel.remoteAccess.controlDesc") ||
                    "开启后新境盒-安卓端可扫码/搜索配对并远程控制已授权的精选功能"}
                </Text>
              </Box>

              {/* 待审批的配对请求：手机自动搜索到本机后会发过来，这里人工确认（不做卡片包裹） */}
              {pairRequests.length > 0 && (
                <VStack align="stretch" spacing={3}>
                  <HStack spacing={2}>
                    <ShieldCheck size={16} style={{ color: getActiveColor() }} />
                    <Text fontSize="sm" fontWeight="bold">
                      配对请求
                      <Text as="span" fontSize="xs" color={textColor} ml={2}>
                        ({pairRequests.length})
                      </Text>
                    </Text>
                  </HStack>
                  <Text fontSize="xs" color={textColor}>
                    以下设备正在请求连接本机，确认是你本人在操作后再点「允许」。
                  </Text>
                  {pairRequests.map((r) => (
                    <Box key={r.id} bg={boxBg} borderRadius="lg" p={3}>
                      <HStack justify="space-between" align="center">
                        <VStack align="start" spacing={0}>
                          <Text fontSize="sm" fontWeight="medium">
                            {r.device_name}
                          </Text>
                          <Text fontSize="xs" color={textColor}>
                            来自 {r.ip}
                          </Text>
                        </VStack>
                        <HStack spacing={2}>
                          <Button
                            size="xs"
                            bg={getActiveColor()}
                            color="white"
                            _hover={{ bg: getHoverColor() }}
                            onClick={() => resolvePairRequest(r.id, true)}
                          >
                            允许
                          </Button>
                          <Button
                            size="xs"
                            variant="ghost"
                            colorScheme="red"
                            leftIcon={<X size={12} />}
                            onClick={() => resolvePairRequest(r.id, false)}
                          >
                            拒绝
                          </Button>
                        </HStack>
                      </HStack>
                    </Box>
                  ))}
                </VStack>
              )}

              {/* 连接区：不做卡片包裹，直接摊在弹窗里 */}
              {access?.enabled && (
                <>
                  {!access.ip ? (
                    <Text fontSize="sm" color="orange.400" textAlign="center">
                      {t("overlayPanel.remoteAccess.noIp") || "未检测到局域网 IP，请确认电脑已连接 WiFi / 网线"}
                    </Text>
                  ) : devices.length === 0 ? (
                    /* 配对视图：无设备时二维码居中展示 */
                    <VStack align="center" spacing={2} w="full">
                      <Text fontSize="xs" color={textColor} textAlign="center">
                        {t("overlayPanel.remoteAccess.apkHint") || "新境盒-安卓端安装包可在官网或QQ群文件获取"}
                      </Text>
                      <Box bg="white" p={2} borderRadius="lg">
                        <QRCodeSVG value={pairQrValue} size={168} />
                      </Box>
                      <Button size="xs" variant="outline" leftIcon={<RefreshCw size={12} />} onClick={rotateCode}>
                        刷新二维码
                      </Button>
                      <Text fontSize="xs" color={textColor} opacity={0.8}>
                        {secondsLeft > 0
                          ? `${t("overlayPanel.remoteAccess.codeExpires") || "有效期剩余"} ${fmtLeft(secondsLeft)}`
                          : "正在刷新二维码…"}
                      </Text>
                    </VStack>
                  ) : (
                    /* 已有设备：设备块直接摊在弹窗里 */
                    deviceListBlock
                  )}
                </>
              )}
            </VStack>
          ) : (
            /* ───────── 第二页：文件互传（单列表） ───────── */
            <VStack align="stretch" spacing={4}>
              <HStack spacing={2}>
                <Box
                  w="8px"
                  h="8px"
                  borderRadius="full"
                  bg={selectedDevice?.connected ? "green.400" : textColor}
                  opacity={selectedDevice?.connected ? 1 : 0.5}
                />
                <Text fontSize="sm" color={textColor}>
                  {(selectedDevice?.connected
                    ? t("overlayPanel.remoteAccess.transferConnected") || "在线"
                    : t("overlayPanel.remoteAccess.transferOffline") || "离线")}
                  {" · "}
                  {t("overlayPanel.remoteAccess.transferLanHint") || "文件通过局域网直接传输"}
                </Text>
              </HStack>

              {/* 文件区：不做卡片包裹，直接摊在弹窗里 */}
              <VStack align="stretch" spacing={2}>
                <HStack justify="space-between" align="center">
                  <Text fontSize="sm" fontWeight="bold">
                    {t("overlayPanel.remoteAccess.transferList") || "文件"}
                    <Text as="span" fontSize="xs" color={textColor} ml={2}>
                      ({allFiles.length})
                    </Text>
                  </Text>
                  <Button
                    size="xs"
                    bg={getActiveColor()}
                    color="white"
                    _hover={{ bg: getHoverColor() }}
                    leftIcon={<Plus size={12} />}
                    isLoading={transferBusy}
                    onClick={addFiles}
                  >
                    {t("overlayPanel.remoteAccess.transferAdd") || "添加文件"}
                  </Button>
                </HStack>

                {allFiles.length === 0 ? (
                  <Text fontSize="xs" color={textColor} py={4} textAlign="center">
                    {t("overlayPanel.remoteAccess.transferEmpty") ||
                      "还没有文件。点「添加文件」传给手机；手机端发来的也会出现在这里。"}
                  </Text>
                ) : (
                  <VStack align="stretch" spacing={2}>
                    {allFiles.map((f) => {
                      const prog = progressMap[f.id];
                      // 进度事件 3 秒内没续上就视为传输中断，回退到静态状态
                      const progFresh = !!prog && Date.now() - prog.receivedAt < PROGRESS_FRESH_MS;
                      const showProgress =
                        progFresh && (f.direction === "from_device" ? !f.saved_path : !f.acked);
                      const pct = prog && prog.total > 0 ? Math.min(100, (prog.done / prog.total) * 100) : null;
                      return (
                        <Box key={f.id} bg={boxBg} borderRadius="lg" p={3}>
                          <HStack justify="space-between" align="center">
                            <HStack spacing={2} flex={1} minW={0}>
                              <Text
                                fontSize="2xs"
                                px={1.5}
                                py={0.5}
                                borderRadius="md"
                                flexShrink={0}
                                color={getActiveColor()}
                                bg={hexToRgba(getActiveColor(), 0.12)}
                              >
                                {f.direction === "to_device"
                                  ? t("overlayPanel.remoteAccess.transferTagTo") || "发给手机"
                                  : t("overlayPanel.remoteAccess.transferTagFrom") || "手机发来"}
                              </Text>
                              <VStack align="start" spacing={0} minW={0} flex={1}>
                                <Text fontSize="sm" isTruncated maxW="100%">
                                  {f.name}
                                </Text>
                                <Text fontSize="xs" color={textColor} isTruncated maxW="100%">
                                  {fmtSize(f.size)} · {new Date(f.created_at).toLocaleString()}
                                  {f.direction === "from_device" && f.saved_path ? ` · ${f.saved_path}` : ""}
                                </Text>
                              </VStack>
                            </HStack>
                            <HStack spacing={1} flexShrink={0}>
                              {f.direction === "from_device" ? (
                                f.saved_path ? (
                                  <>
                                    <Text
                                      fontSize="2xs"
                                      px={2}
                                      py={1}
                                      borderRadius="md"
                                      bg={hexToRgba("#38a169", 0.15)}
                                      color="green.500"
                                    >
                                      {t("overlayPanel.remoteAccess.transferSaved") || "已保存"}
                                    </Text>
                                    <IconButton
                                      aria-label="reveal"
                                      icon={<FolderOpen size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      onClick={() => revealFile(f)}
                                    />
                                  </>
                                ) : showProgress ? (
                                  <Text fontSize="2xs" px={2} py={1} borderRadius="md" bg={hexToRgba(getActiveColor(), 0.15)} color={getActiveColor()}>
                                    {t("overlayPanel.remoteAccess.transferRecvProgress") || "接收中"}
                                    {prog ? ` · ${fmtSpeed(prog.speed)}` : ""}
                                  </Text>
                                ) : (
                                  <>
                                    <Text
                                      fontSize="2xs"
                                      px={2}
                                      py={1}
                                      borderRadius="md"
                                      bg={hexToRgba(getActiveColor(), 0.15)}
                                      color={getActiveColor()}
                                    >
                                      {t("overlayPanel.remoteAccess.transferUnsaved") || "待另存为"}
                                    </Text>
                                    <IconButton
                                      aria-label="save"
                                      icon={<Save size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      isDisabled={transferBusy}
                                      onClick={() => saveAsFile(f)}
                                    />
                                  </>
                                )
                              ) : showProgress ? (
                                <Text fontSize="2xs" px={2} py={1} borderRadius="md" bg={hexToRgba(getActiveColor(), 0.15)} color={getActiveColor()}>
                                  {t("overlayPanel.remoteAccess.transferPhoneRecv") || "手机接收中"}
                                  {prog
                                    ? ` · ${pct != null ? `${pct.toFixed(0)}% · ` : ""}${fmtSpeed(prog.speed)}`
                                    : ""}
                                </Text>
                              ) : f.acked ? (
                                <Text
                                  fontSize="2xs"
                                  px={2}
                                  py={1}
                                  borderRadius="md"
                                  bg={hexToRgba("#38a169", 0.15)}
                                  color="green.500"
                                >
                                  {t("overlayPanel.remoteAccess.transferAcked") || "已接收"}
                                </Text>
                              ) : (
                                <Text
                                  fontSize="2xs"
                                  px={2}
                                  py={1}
                                  borderRadius="md"
                                  bg={hexToRgba(getActiveColor(), 0.15)}
                                  color={getActiveColor()}
                                >
                                  {t("overlayPanel.remoteAccess.transferPending") || "待接收"}
                                </Text>
                              )}
                              <IconButton
                                aria-label="delete"
                                icon={<Trash2 size={14} />}
                                size="sm"
                                variant="ghost"
                                colorScheme="red"
                                onClick={() => removeFile(f)}
                              />
                            </HStack>
                          </HStack>
                          {showProgress && prog && (
                            <Box mt={2}>
                              {pct != null ? (
                                <Progress
                                  size="xs"
                                  value={pct}
                                  borderRadius="md"
                                  sx={{ "& > div": { background: getActiveColor() } }}
                                />
                              ) : (
                                <Progress
                                  size="xs"
                                  isIndeterminate
                                  borderRadius="md"
                                  sx={{ "& > div": { background: getActiveColor() } }}
                                />
                              )}
                            </Box>
                          )}
                        </Box>
                      );
                    })}
                  </VStack>
                )}
              </VStack>
            </VStack>
          )}
        </ModalBody>
        <ModalFooter>
          <Button bg={getActiveColor()} color="white" _hover={{ bg: getHoverColor() }} onClick={onClose}>
            {t("common.close") || "关闭"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
