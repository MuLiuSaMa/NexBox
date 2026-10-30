import {
  Box,
  VStack,
  HStack,
  Text,
  Checkbox,
  useColorModeValue,
} from "@chakra-ui/react";
import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { useState } from "react";
import { LiquidGlassCard } from "./special/liquid-glass-card";
import { LiquidGlassButton } from "./special/liquid-glass-button";

interface CloseConfirmDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onCloseApp: (savePreference: boolean) => void;
  onMinimizeToTray: (savePreference: boolean) => void;
}

export function CloseConfirmDialog({
  isOpen,
  onClose,
  onCloseApp,
  onMinimizeToTray,
}: CloseConfirmDialogProps) {
  const { t } = useTranslation();
  const [dontAskAgain, setDontAskAgain] = useState(false);

  const overlayBg = useColorModeValue(
    "rgba(0, 0, 0, 0.5)",
    "rgba(0, 0, 0, 0.7)"
  );
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const textColor = useColorModeValue("gray.600", "#ffffff");

  const handleMinimizeToTray = () => {
    onMinimizeToTray(dontAskAgain);
  };

  const handleCloseApp = () => {
    onCloseApp(dontAskAgain);
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <motion.div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            zIndex: 9999,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
        >
          <Box
            position="absolute"
            top={0}
            left={0}
            right={0}
            bottom={0}
            bg={overlayBg}
            onClick={onClose}
          />
          
          <motion.div
            initial={{ scale: 0.9, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            exit={{ scale: 0.9, opacity: 0 }}
            transition={{ duration: 0.2 }}
          >
            <LiquidGlassCard
              p={6}
              maxW="360px"
              w="90%"
              position="relative"
              zIndex={1}
            >
              <VStack spacing={4} align="stretch">
                <Text fontSize="lg" fontWeight="bold" color={titleColor} textAlign="center">
                  {t("closeDialog.title")}
                </Text>
                
                <Text fontSize="sm" color={textColor} textAlign="center">
                  {t("closeDialog.message")}
                </Text>

                <HStack spacing={3} justify="center">
                  <LiquidGlassButton
                    size="sm"
                    variant="outline"
                    borderRadius="lg"
                    onClick={handleCloseApp}
                    minW="100px"
                  >
                    {t("closeDialog.closeApp")}
                  </LiquidGlassButton>
                  
                  <LiquidGlassButton
                    size="sm"
                    variant="solid"
                    borderRadius="lg"
                    onClick={handleMinimizeToTray}
                    minW="100px"
                  >
                    {t("closeDialog.minimizeToTray")}
                  </LiquidGlassButton>
                </HStack>

                <HStack justify="center" pt={2}>
                  <Checkbox
                    size="sm"
                    isChecked={dontAskAgain}
                    onChange={(e) => setDontAskAgain(e.target.checked)}
                    colorScheme="teal"
                  >
                    <Text fontSize="xs" color={textColor}>
                      {t("closeDialog.dontAskAgain")}
                    </Text>
                  </Checkbox>
                </HStack>
              </VStack>
            </LiquidGlassCard>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
