import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalCloseButton,
  VStack,
  HStack,
  Text,
  Box,
  Spinner,
  useToast,
  useColorModeValue,
} from "@chakra-ui/react";
import { FaQq } from "react-icons/fa6";
import { LuCopy, LuExternalLink } from "react-icons/lu";
import { useTranslation } from "react-i18next";
import { useQQGroups, openExternal, QqGroup } from "@/hooks/use-qq-groups";
import { QqGroupIcon } from "@/components/ui/qq-group-icon";
import { useThemeColor } from "@/contexts/theme-color-context";

interface QqGroupModalProps {
  isOpen: boolean;
  onClose: () => void;
  /** 点击某个群后的行为，默认打开加群链接（link 为空则复制群号） */
  onJoin?: (group: QqGroup) => void;
}

/** 官方 QQ 群弹窗：主题色适配，获取 gitee 上的群号列表 */
export function QqGroupModal({ isOpen, onClose, onJoin }: QqGroupModalProps) {
  const { t } = useTranslation();
  const toast = useToast();
  const { getActiveColor } = useThemeColor();
  const { groups, loading } = useQQGroups();

  const modalBg = useColorModeValue("white", "#171717");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#9ca3af");
  const numberColor = useColorModeValue("gray.500", "#9ca3af");
  const borderColor = useColorModeValue("gray.200", "#2d2d2d");

  const copyNumber = async (group: QqGroup) => {
    try {
      await navigator.clipboard.writeText(group.number);
      toast({
        title: `${group.name} ${group.number}`,
        description: t("home.qqGroup.copied"),
        status: "success",
        duration: 1500,
        isClosable: false,
      });
    } catch {
      toast({
        title: group.number,
        status: "error",
        duration: 1500,
        isClosable: false,
      });
    }
  };

  const handleJoin = (group: QqGroup) => {
    if (onJoin) {
      onJoin(group);
      return;
    }
    if (group.link) {
      openExternal(group.link);
    } else {
      copyNumber(group);
    }
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="md" isCentered>
      <ModalOverlay backdropFilter="blur(4px)" />
      <ModalContent bg={modalBg} borderRadius="3xl" boxShadow="xl">
        <ModalHeader color={textColor} fontSize="lg">
          <HStack spacing={2}>
            <FaQq color="#12B7F5" />
            <Text as="span">{t("home.qqGroup.title")}</Text>
          </HStack>
        </ModalHeader>
        <ModalCloseButton color={textColor} />
        <ModalBody pb={6}>
          {loading ? (
            <VStack py={6} spacing={3}>
              <Spinner size="lg" color={getActiveColor()} />
              <Text fontSize="sm" color={subTextColor}>
                {t("home.qqGroup.loading")}
              </Text>
            </VStack>
          ) : groups.length === 0 ? (
            <VStack py={6}>
              <Text color={subTextColor}>{t("home.qqGroup.empty")}</Text>
            </VStack>
          ) : (
            <VStack spacing={3} align="stretch">
              {groups.map((group, index) => (
                <HStack
                  key={index}
                  justify="space-between"
                  p={3}
                  borderRadius="lg"
                  border="1px solid"
                  borderColor={borderColor}
                  cursor="pointer"
                  transition="all 0.2s"
                  _hover={{ borderColor: getActiveColor(), bg: `${getActiveColor()}14` }}
                  onClick={() => handleJoin(group)}
                >
                  <HStack spacing={2}>
                    <Box
                      w="32px"
                      h="32px"
                      borderRadius="md"
                      bg={group.icon ? "transparent" : getActiveColor()}
                      display="flex"
                      alignItems="center"
                      justifyContent="center"
                      flexShrink={0}
                      overflow="hidden"
                    >
                      {group.icon ? (
                        <QqGroupIcon url={group.icon} size={32} />
                      ) : (
                        <FaQq color={useColorModeValue("#ffffff", "#1a1a1a")} size={16} />
                      )}
                    </Box>
                    <VStack spacing={0} align="start">
                      <Text fontSize="sm" fontWeight="bold" color={textColor}>
                        {group.name}
                      </Text>
                      <HStack spacing={1}>
                        <Text fontSize="xs" color={numberColor}>
                          {t("home.qqGroup.number")}：{group.number}
                        </Text>
                        <LuCopy
                          size={12}
                          color={numberColor}
                          style={{ cursor: "pointer", flexShrink: 0 }}
                          onClick={(e) => {
                            e.stopPropagation();
                            copyNumber(group);
                          }}
                        />
                      </HStack>
                    </VStack>
                  </HStack>
                  <LuExternalLink size={16} color={getActiveColor()} />
                </HStack>
              ))}
              <Text fontSize="xs" color={subTextColor} textAlign="center">
                {t("home.qqGroup.tip")}
              </Text>
            </VStack>
          )}
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}