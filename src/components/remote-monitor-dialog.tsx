import {
  Box,
  Text,
  VStack,
  HStack,
  Switch,
  Button,
  IconButton,
  useColorModeValue,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  useDisclosure,
} from "@chakra-ui/react";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { Smartphone, Copy, Check } from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { hexToRgba } from "@/lib/color-utils";

interface RemoteInfo {
  ip: string | null;
  port: number;
  code: string | null;
  url: string | null;
  enabled: boolean;
}

/**
 * 手机远程监控（局域网）：触发按钮 + 弹窗。
 * 悬浮框页面与硬件信息页面共用；样式切换在手机端进行，此处不提供。
 */
export function RemoteMonitorDialog({ size = "sm" }: { size?: "sm" | "md" }) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("panels");
  const { getActiveColor, getHoverColor } = useThemeColor();
  const { isOpen, onOpen, onClose } = useDisclosure();

  const [enabled, setEnabled] = useState(false);
  const [info, setInfo] = useState<RemoteInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [copied, setCopied] = useState(false);

  const textColor = useColorModeValue("gray.600", "gray.400");
  const boxBg = useColorModeValue("gray.100", "#1a1a1a");
  const contentColor = useColorModeValue("gray.900", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");

  useEffect(() => {
    invoke<RemoteInfo>("cmd_get_remote_monitor")
      .then((i) => {
        setInfo(i);
        setEnabled(i.enabled);
      })
      .catch(() => {});
  }, []);

  const toggle = async (next: boolean) => {
    setLoading(true);
    try {
      const i = await invoke<RemoteInfo>(next ? "cmd_enable_remote_monitor" : "cmd_disable_remote_monitor");
      setInfo(i);
      setEnabled(i.enabled);
    } catch (error) {
      toast({ title: String(error), status: "error", duration: 2500, isClosable: true });
    } finally {
      setLoading(false);
    }
  };

  const copyUrl = async () => {
    if (!info?.url) return;
    try {
      await navigator.clipboard.writeText(info.url);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* ignore */
    }
  };

  return (
    <>
      <Button
        leftIcon={<Smartphone size={16} />}
        size={size}
        variant="outline"
        color={getActiveColor()}
        borderColor={getActiveColor()}
        onClick={onOpen}
      >
        {t("overlayPanel.remoteMonitor.title") || "手机远程监控"}
        {enabled && <Box w={2} h={2} borderRadius="full" bg="green.400" ml={2} />}
      </Button>

      <Modal isOpen={isOpen} onClose={onClose} isCentered>
        <ModalOverlay />
        <ModalContent
          maxW="md"
          bg={modalBg}
          color={contentColor}
          border="1px solid"
          borderColor={modalBorderColor}
          borderRadius="xl"
        >
          <ModalHeader>{t("overlayPanel.remoteMonitor.title") || "手机远程监控"}</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <VStack align="stretch" spacing={4}>
              <Text fontSize="sm" color={textColor}>
                {t("overlayPanel.remoteMonitor.description") || "开启后可用手机浏览器在同一 WiFi 下实时查看硬件监控数据"}
              </Text>

              <HStack justify="space-between" align="center">
                <Text fontSize="sm" color={textColor}>
                  {t("overlayPanel.remoteMonitor.enable") || "启用远程监控"}
                </Text>
                <Switch
                  isChecked={enabled}
                  isDisabled={loading}
                  onChange={(e) => toggle(e.target.checked)}
                  size="lg"
                  sx={{
                    "& .chakra-switch__track[data-checked]": { bg: getActiveColor() },
                  }}
                />
              </HStack>

              {enabled && (
                <Box
                  bg={hexToRgba(getActiveColor(), 0.08)}
                  border="1px solid"
                  borderColor={hexToRgba(getActiveColor(), 0.28)}
                  borderRadius="xl"
                  p={4}
                >
                  {info?.url ? (
                    <VStack spacing={3} align="center">
                      <Box bg="white" p={2} borderRadius="lg" flexShrink={0}>
                        <QRCodeSVG value={info.url} size={148} />
                      </Box>
                      <HStack spacing={2} justify="center">
                        <Smartphone size={14} style={{ color: getActiveColor() }} />
                        <Text fontSize="sm" color={textColor}>
                          {t("overlayPanel.remoteMonitor.scanHint") || "手机扫码或输入以下地址"}
                        </Text>
                      </HStack>
                      <HStack spacing={2} justify="center" w="100%">
                        <Box as="span" flex={1} maxW={320} px={3} py={1.5} borderRadius="lg" bg={boxBg} fontSize="sm" userSelect="all" isTruncated>
                          {info.url}
                        </Box>
                        <IconButton
                          aria-label={t("overlayPanel.remoteMonitor.copy") || "复制地址"}
                          icon={copied ? <Check size={14} /> : <Copy size={14} />}
                          size="sm"
                          variant="ghost"
                          colorScheme={copied ? "green" : "gray"}
                          onClick={copyUrl}
                        />
                      </HStack>
                      {info.code && (
                        <HStack spacing={2} justify="center">
                          <Text fontSize="sm" color={textColor}>
                            {t("overlayPanel.remoteMonitor.pin") || "访问码"}
                          </Text>
                          <Box px={3} py={1} borderRadius="lg" bg={hexToRgba(getActiveColor(), 0.16)} fontWeight="700" letterSpacing="4px" fontSize="md">
                            {info.code}
                          </Box>
                        </HStack>
                      )}
                      <Text fontSize="xs" color={textColor} opacity={0.75} textAlign="center">
                        {t("overlayPanel.remoteMonitor.firewallHint") || "手机打不开？请允许 Windows 防火墙放行，或临时关闭防火墙测试"}
                      </Text>
                    </VStack>
                  ) : (
                    <Text fontSize="sm" color="orange.400" textAlign="center">
                      {t("overlayPanel.remoteMonitor.noIp") || "未检测到局域网 IP，请确认电脑已连接 WiFi / 网线"}
                    </Text>
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
    </>
  );
}
