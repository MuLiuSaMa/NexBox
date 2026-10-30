"use client";

import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
  Button,
  useColorModeValue,
  Text,
  VStack,
  HStack,
  Box,
} from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useUpdate } from "@/contexts/update-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { LuDownload, LuRefreshCw } from "react-icons/lu";

/** 统一更新弹窗：版本号 / 更新日志 / 主题色进度条 / 可关闭（关闭后继续下载） */
export function UpdateModal() {
  const { t } = useTranslation();
  const {
    isModalOpen,
    closeModal,
    latestRelease,
    isDownloading,
    downloadProgress,
    isDownloadComplete,
    manualDownload,
    handleInstall,
    handleSkip,
  } = useUpdate();
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();

  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const notesBg = useColorModeValue("gray.50", "#1a1a1a");
  const completeBg = useColorModeValue("green.50", "rgba(72, 187, 120, 0.1)");
  const completeBorder = useColorModeValue("green.200", "rgba(72, 187, 120, 0.3)");
  const completeText = useColorModeValue("green.600", "green.300");

  return (
    <Modal isOpen={isModalOpen} onClose={closeModal} isCentered closeOnOverlayClick>
      <ModalOverlay />
      <ModalContent bg={modalBg} borderColor={modalBorderColor} borderRadius="3xl">
        <ModalHeader color={labelColor}>
          {t("settings.aboutSettings.updateModal.title")}
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody>
          <VStack align="start" spacing={4}>
            <HStack>
              <Text color={subLabelColor} fontSize="sm">
                {t("settings.aboutSettings.updateModal.version")}:
              </Text>
              <Text color={labelColor} fontWeight="medium">
                {latestRelease?.tag_name}
              </Text>
            </HStack>
            <Box w="full">
              <Text color={subLabelColor} fontSize="sm" mb={2}>
                {t("settings.aboutSettings.updateModal.releaseNotes")}:
              </Text>
              <Box
                p={3}
                borderRadius="lg"
                bg={notesBg}
                maxH="200px"
                overflowY="auto"
                sx={{
                  scrollbarGutter: "stable",
                  "&::-webkit-scrollbar": { width: "4px" },
                  "&::-webkit-scrollbar-track": { background: "transparent" },
                  "&::-webkit-scrollbar-thumb": {
                    background: `${activeColor}88`,
                    borderRadius: "2px",
                  },
                  "&::-webkit-scrollbar-thumb:hover": { background: activeColor },
                }}
              >
                <Text color={labelColor} fontSize="sm" whiteSpace="pre-wrap">
                  {latestRelease?.body || "无更新说明"}
                </Text>
              </Box>
            </Box>
            {isDownloading && !isDownloadComplete && (
              <Box w="full">
                <Box w="full" h="6px" borderRadius="full" bg={notesBg} overflow="hidden">
                  <Box
                    h="full"
                    borderRadius="full"
                    bg={activeColor}
                    width={`${Math.max(0, Math.min(100, downloadProgress))}%`}
                    transition="width 0.15s"
                  />
                </Box>
                <Text
                  color={subLabelColor}
                  fontSize="xs"
                  mt={1}
                  minW="44px"
                >
                  {t("settings.aboutSettings.updateModal.downloading")} {Math.round(downloadProgress)}%
                </Text>
              </Box>
            )}
            {isDownloadComplete && (
              <Box
                w="full"
                p={3}
                borderRadius="lg"
                bg={completeBg}
                border="1px solid"
                borderColor={completeBorder}
              >
                <Text color={completeText} fontSize="sm" fontWeight="medium">
                  {t("settings.aboutSettings.updateModal.downloadComplete")}
                </Text>
              </Box>
            )}
          </VStack>
        </ModalBody>
        <ModalFooter>
          {isDownloadComplete ? (
            <>
              <Button variant="ghost" mr={3} onClick={handleSkip}>
                {t("settings.aboutSettings.updateModal.skip")}
              </Button>
              <LiquidGlassButton
                colorScheme="teal"
                onClick={handleInstall}
                leftIcon={<LuRefreshCw size={14} />}
              >
                {t("settings.aboutSettings.updateModal.restartInstall")}
              </LiquidGlassButton>
            </>
          ) : (
            <>
              {!isDownloading && (
                <Button variant="ghost" mr={3} onClick={closeModal}>
                  {t("settings.aboutSettings.updateModal.cancel")}
                </Button>
              )}
              <LiquidGlassButton
                colorScheme="teal"
                onClick={manualDownload}
                isDisabled={isDownloading}
                isLoading={isDownloading}
                leftIcon={!isDownloading ? <LuDownload size={14} /> : undefined}
              >
                {isDownloading
                  ? t("settings.aboutSettings.updateModal.downloading")
                  : t("settings.aboutSettings.updateModal.download")}
              </LiquidGlassButton>
            </>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
