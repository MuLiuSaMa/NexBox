import {
  Box,
  Text,
  VStack,
  HStack,
  Switch,
  Button,
  IconButton,
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
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Smartphone, RefreshCw, Trash2, ShieldCheck, X } from "lucide-react";
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
}

/** 手机端「请求配对」：免配对码，等本机用户点允许 */
interface PairRequest {
  id: string;
  ip: string;
  device_name: string;
  created_at: number;
  expires_at: number;
}

/**
 * 手机远程连接（局域网）：详细连接设置弹窗。
 * 由主页「手机远程」卡片打开；包含远程控制开关、
 * 连接二维码（可手动刷新）、待审批配对请求与已配对设备管理。
 */
export function RemoteAccessDialog({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("panels");
  const { getActiveColor, getHoverColor } = useThemeColor();

  const [access, setAccess] = useState<AccessInfo | null>(null);
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [pairRequests, setPairRequests] = useState<PairRequest[]>([]);
  const [secondsLeft, setSecondsLeft] = useState(0);
  const [busyControl, setBusyControl] = useState(false);

  const textColor = useColorModeValue("gray.600", "gray.400");
  const contentColor = useColorModeValue("gray.900", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const boxBg = useColorModeValue("gray.100", "#1a1a1a");

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
    if (isOpen) refresh();
  }, [isOpen, refresh]);

  // 手机发起配对请求时，后端会推事件过来：立刻入列并提醒用户确认
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<PairRequest>("remote-access://pair-request", (e) => {
      const req = e.payload;
      setPairRequests((prev) => (prev.some((p) => p.id === req.id) ? prev : [...prev, req]));
      toast({
        title: `「${req.device_name}」请求配对，请确认`,
        status: "info",
        duration: 4000,
        isClosable: true,
      });
    })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        /* 非 Tauri 环境忽略 */
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [toast]);

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

  const fmtLeft = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const pairQrValue =
    access && access.ip && access.port > 0 && access.pairing_code
      ? JSON.stringify({ v: 1, ip: access.ip, port: access.port, code: access.pairing_code })
      : "";

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="lg">
      <ModalOverlay />
      <ModalContent bg={modalBg} color={contentColor} border="1px solid" borderColor={modalBorderColor} borderRadius="xl">
        <ModalHeader>{t("overlayPanel.remoteAccess.title") || "手机远程连接"}</ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          <VStack align="stretch" spacing={4}>
            {/* 远程控制 */}
            <Box>
              <HStack justify="space-between" align="center">
                <HStack spacing={2}>
                  <Smartphone size={16} style={{ color: getActiveColor() }} />
                  <Text fontSize="sm" fontWeight="bold">
                    {t("overlayPanel.remoteAccess.control") || "远程控制（手机 App）"}
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
                  "开启后安卓原生 App 可自动搜索并请求配对（本机确认即可，无需配对码），也可扫码连接"}
              </Text>
            </Box>

            {/* 待审批的配对请求：手机自动搜索到本机后会发过来，这里人工确认 */}
            {pairRequests.length > 0 && (
              <Box
                bg={hexToRgba(getActiveColor(), 0.1)}
                border="1px solid"
                borderColor={hexToRgba(getActiveColor(), 0.45)}
                borderRadius="xl"
                p={4}
              >
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
              </Box>
            )}

            {access?.enabled && (
              <Box bg={hexToRgba(getActiveColor(), 0.08)} border="1px solid" borderColor={hexToRgba(getActiveColor(), 0.28)} borderRadius="xl" p={4}>
                {!access.ip ? (
                  <Text fontSize="sm" color="orange.400" textAlign="center">
                    {t("overlayPanel.remoteAccess.noIp") || "未检测到局域网 IP，请确认电脑已连接 WiFi / 网线"}
                  </Text>
                ) : (
                  <>
                    {/* 左右排版：左＝连接二维码（内嵌 ip/端口/配对码，点刷新即轮换配对码），右＝已配对设备 */}
                    <HStack align="stretch" spacing={4}>
                      <VStack spacing={2} align="center" flexShrink={0}>
                        <Box bg="white" p={2} borderRadius="lg">
                          <QRCodeSVG value={pairQrValue} size={148} />
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

                      <Divider orientation="vertical" />

                      <VStack align="stretch" spacing={2} flex={1} minW={0}>
                        <Text fontSize="sm" fontWeight="bold">
                          {t("overlayPanel.remoteAccess.pairedDevices") || "已配对设备"}
                          <Text as="span" fontSize="xs" color={textColor} ml={2}>
                            ({devices.length})
                          </Text>
                        </Text>
                        {devices.length === 0 ? (
                          <Text fontSize="xs" color={textColor}>
                            {t("overlayPanel.remoteAccess.noDevices") || "暂无已配对设备"}
                          </Text>
                        ) : (
                          devices.map((d) => (
                            <HStack key={d.device_id} justify="space-between" py={1}>
                              <VStack align="start" spacing={0}>
                                <Text fontSize="sm">{d.name}</Text>
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
                          ))
                        )}
                      </VStack>
                    </HStack>

                    <Text fontSize="xs" color={textColor} textAlign="center" mt={3} opacity={0.85}>
                      {t("overlayPanel.remoteAccess.discoveryHint") ||
                        "手机 App 打开连接页会自动搜索到本机并发起配对请求，在这里点「允许」即可；也可直接扫描二维码连接。"}
                    </Text>
                  </>
                )}
              </Box>
            )}
          </VStack>
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
