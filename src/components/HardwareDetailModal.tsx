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
  Divider,
  useColorModeValue,
  Box,
} from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import type { ElementType } from "react";

export interface SpecItem {
  label: string;
  value: string;
}

interface HardwareDetailModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  icon: ElementType;
  specs: SpecItem[];
  type: string;
}

function iconColor(type: string): string {
  switch (type) {
    case "cpu": return "#3b82f6";
    case "gpu": return "#22c55e";
    case "memory": return "#06b6d4";
    case "storage": return "#a855f7";
    case "motherboard": return "#f59e0b";
    case "sound": return "#f97316";
    case "network": return "#14b8a6";
    case "monitor": return "#ec4899";
    default: return "#f59e0b";
  }
}

export default function HardwareDetailModal({
  isOpen,
  onClose,
  title,
  icon: IconComponent,
  specs,
  type,
}: HardwareDetailModalProps) {
  const { t } = useTranslation();

  const overlayBg = useColorModeValue("blackAlpha.600", "blackAlpha.700");
  const contentBg = useColorModeValue("white", "#1a1a1a");
  const borderColor = useColorModeValue("gray.200", "#2a2a2a");
  const labelColor = useColorModeValue("gray.500", "#ffffff");
  const valueColor = useColorModeValue("gray.800", "#e0e0e0");
  const headerColor = useColorModeValue("gray.800", "#e0e0e0");

  const color = iconColor(type);

  // Filter out empty/meaningless values
  const filteredSpecs = specs.filter(
    (s) =>
      s.value &&
      s.value !== "未知" &&
      s.value !== "0" &&
      s.value !== "--" &&
      s.value !== "0 GB" &&
      s.value !== "0 GB (0GB)" &&
      s.value.trim() !== ""
  );

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="md" isCentered>
      <ModalOverlay bg={overlayBg} backdropFilter="blur(4px)" />
      <ModalContent
        bg={contentBg}
        border="1px solid"
        borderColor={borderColor}
        borderRadius="3xl"
        mx={4}
        maxH="80vh"
        overflow="hidden"
      >
        <ModalHeader>
          <HStack spacing={2}>
            <IconComponent size={20} color={color} />
            <Text fontSize="lg" fontWeight="bold" color={headerColor}>
              {title}
            </Text>
          </HStack>
        </ModalHeader>
        <ModalCloseButton color={labelColor} />
        <Divider borderColor={borderColor} />
        <ModalBody pb={6} overflowY="auto" maxH="60vh">
          <VStack align="stretch" spacing={0} divider={<Divider borderColor={borderColor} />}>
            {filteredSpecs.map((spec, i) => (
              <HStack key={i} justify="space-between" py={2.5} px={1}>
                <Text fontSize="sm" color={labelColor} flexShrink={0}>
                  {spec.label}
                </Text>
                <Text
                  fontSize="sm"
                  color={valueColor}
                  fontWeight="medium"
                  textAlign="right"
                  wordBreak="break-all"
                  maxW="60%"
                >
                  {spec.value}
                </Text>
              </HStack>
            ))}
            {filteredSpecs.length === 0 && (
              <Text fontSize="sm" color={labelColor} textAlign="center" py={4}>
                {t("hardware.noDetail") || "暂无详细信息"}
              </Text>
            )}
          </VStack>
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}
