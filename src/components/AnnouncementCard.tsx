import { Box, Text, VStack, useColorModeValue, Modal, ModalOverlay, ModalContent, ModalHeader, ModalBody, ModalCloseButton, useDisclosure, Badge, Divider, HStack, Spinner } from "@chakra-ui/react";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { store } from "@/lib/store";

interface Announcement {
  title: string;
  content: string;
  important: boolean;
  create_time: string;
}

interface AnnouncementResponse {
  version: number;
  announce_list: Announcement[];
}

export function useAnnouncementEnabled() {
  const [state, setState] = useState({ enabled: false, ready: false });

  useEffect(() => {
    (async () => {
      let enabled = true;
      const saved = await store.get<boolean>("nexbox_announcement_enabled");
      if (saved !== null && saved !== undefined) {
        enabled = saved;
      } else {
        const ls = localStorage.getItem("nexbox_announcement_enabled");
        if (ls !== null) enabled = ls === "true";
      }
      setState((s) => ({ ...s, enabled, ready: true }));
    })();
  }, []);

  useEffect(() => {
    const handler = (e: CustomEvent) => setState((s) => ({ ...s, enabled: e.detail }));
    window.addEventListener("announcement-setting-changed", handler as EventListener);
    return () => window.removeEventListener("announcement-setting-changed", handler as EventListener);
  }, []);

  return state;
}

export function AnnouncementCard() {
  const { t } = useTranslation();
  const { liquidGlassEnabled } = useBackground();
  const { isOpen, onOpen, onClose } = useDisclosure();
  const [announcements, setAnnouncements] = useState<Announcement[]>([]);
  const [loading, setLoading] = useState(false);
  const [hasUnread, setHasUnread] = useState(false);

  const labelColor = useColorModeValue("gray.500", "#ffffff");
  const cardBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const contentColor = useColorModeValue("gray.700", "#cccccc");
  const dateColor = useColorModeValue("gray.400", "#666666");
  const countColor = useColorModeValue("purple.500", "#b794f4");
  const modalBg = useColorModeValue("white", "#1a1a1a");

  useEffect(() => {
    const fetchAnnouncements = async () => {
      try {
        const response = await invoke<AnnouncementResponse>("get_announcements");
        setAnnouncements(response.announce_list);

        // 优先从 store 读取，兼容旧 localStorage
        let lastReadTime = await store.get<string>("nexbox_announcement_last_read");
        if (!lastReadTime) {
          lastReadTime = localStorage.getItem("nexbox_announcement_last_read");
        }

        if (response.announce_list.length > 0 && lastReadTime) {
          const lastRead = new Date(lastReadTime);
          const hasNew = response.announce_list.some(a => new Date(a.create_time) > lastRead);
          setHasUnread(hasNew);
        } else if (response.announce_list.length > 0) {
          setHasUnread(true);
        }
      } catch (e) {
        console.error("Failed to fetch announcements:", e);
      }
    };
    fetchAnnouncements();
  }, []);

  const handleOpenModal = async () => {
    setLoading(true);
    onOpen();
    try {
      const response = await invoke<AnnouncementResponse>("get_announcements");
      setAnnouncements(response.announce_list);
      const now = new Date().toISOString();
      localStorage.setItem("nexbox_announcement_last_read", now);
      await store.set("nexbox_announcement_last_read", now);
      await store.save();
      setHasUnread(false);
    } catch (e) {
      console.error("Failed to fetch announcements:", e);
    } finally {
      setLoading(false);
    }
  };

  const cardContent = (
    <VStack spacing={0} align="center">
      <Text fontSize="2xs" color={labelColor}>
        {t("home.announcement")}
      </Text>
      <Box
        cursor="pointer"
        userSelect="none"
        position="relative"
      >
        <Text
          fontSize="2xl"
          fontWeight="bold"
          color={hasUnread ? countColor : labelColor}
          transition="color 0.3s"
        >
          {announcements.length}
        </Text>
        {hasUnread && (
          <Box
            position="absolute"
            top="-2px"
            right="-8px"
            w="8px"
            h="8px"
            bg="red.500"
            borderRadius="full"
          />
        )}
      </Box>
    </VStack>
  );

  const modalContent = (
    <Modal isOpen={isOpen} onClose={onClose} size="lg" scrollBehavior="inside">
      <ModalOverlay />
      <ModalContent maxH="80vh" bg={modalBg}>
        <ModalHeader>{t("home.announcementList")}</ModalHeader>
        <ModalCloseButton />
        <ModalBody pb={6} overflowY="auto" sx={{
          "&::-webkit-scrollbar": {
            width: "6px",
          },
          "&::-webkit-scrollbar-track": {
            background: "transparent",
          },
          "&::-webkit-scrollbar-thumb": {
            background: useColorModeValue("gray.300", "gray.600"),
            borderRadius: "3px",
          },
        }}>
          {loading ? (
            <VStack py={8}>
              <Spinner size="lg" color={countColor} />
              <Text color={labelColor} fontSize="sm">{t("home.loading")}</Text>
            </VStack>
          ) : announcements.length === 0 ? (
            <VStack py={8}>
              <Text color={labelColor}>{t("home.noAnnouncement")}</Text>
            </VStack>
          ) : (
            <VStack spacing={4} align="stretch">
              {announcements.map((announcement, index) => (
                <Box key={index}>
                  <VStack align="stretch" spacing={2}>
                    <HStack justify="space-between">
                      <HStack>
                        <Text fontWeight="bold" fontSize="md" color={contentColor}>
                          {announcement.title}
                        </Text>
                        {announcement.important && (
                          <Badge colorScheme="red" fontSize="xs">
                            {t("home.important")}
                          </Badge>
                        )}
                      </HStack>
                      <Text fontSize="xs" color={dateColor}>
                        {announcement.create_time}
                      </Text>
                    </HStack>
                    <Text fontSize="sm" color={contentColor} whiteSpace="pre-wrap">
                      {announcement.content}
                    </Text>
                  </VStack>
                  {index < announcements.length - 1 && <Divider mt={2} />}
                </Box>
              ))}
            </VStack>
          )}
        </ModalBody>
      </ModalContent>
    </Modal>
  );

  return (
    <>
      {liquidGlassEnabled ? (
        <LiquidGlassCard py={2} px={3} w="90px" onClick={handleOpenModal}>
          {cardContent}
        </LiquidGlassCard>
      ) : (
        <Box
          bg={cardBg}
          borderRadius="3xl"
          border="1px solid"
          borderColor={borderColor}
          py={2}
          px={3}
          w="90px"
          onClick={handleOpenModal}
          cursor="pointer"
          _hover={{ borderColor: countColor }}
          transition="border-color 0.2s"
        >
          {cardContent}
        </Box>
      )}
      {modalContent}
    </>
  );
}
