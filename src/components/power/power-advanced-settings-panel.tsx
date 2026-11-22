import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Button,
  Spinner,
  Checkbox,
  useDisclosure,
  useColorModeValue,
  AlertDialog,
  AlertDialogBody,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogContent,
  AlertDialogOverlay,
  AlertDialogCloseButton,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { AnimatePresence, motion } from "framer-motion";
import { useEffect, useRef, useState, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { store } from "@/lib/store";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { powerAdvancedSettings } from "@/config/power-advanced-settings";
import PowerAdvancedSettingCard, {
  type PowerAdvancedSettingInfo,
} from "@/components/power/power-advanced-setting-card";
import { Lock, ShieldAlert, RefreshCw, Battery} from "lucide-react";

export interface PowerAdvancedSettingsResponse {
  schemeGuid: string;
  schemeName: string;
  hasBattery: boolean;
  settings: PowerAdvancedSettingInfo[];
}

interface UnhideResult {
  success: boolean;
  message: string;
  total: number;
  updated: number;
}

const UNLOCK_STORE_KEY = "powerAdvancedUnlocked";

export default function PowerAdvancedSettingsPanel() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("power");
  const { getActiveColor, getBorderColor } = useThemeColor();

  const primaryColor = getActiveColor();


  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "rgba(200,200,200,0.85)");


  const [data, setData] = useState<PowerAdvancedSettingsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [unlocked, setUnlocked] = useState(false);
  const [unlockLoading, setUnlockLoading] = useState(false);
  const [agreeChecked, setAgreeChecked] = useState(false);
  const [applyingId, setApplyingId] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<Record<string, { ac: number; dc: number | null }>>({});
  const { isOpen, onOpen, onClose } = useDisclosure();
  const cancelRef = useRef<HTMLButtonElement>(null);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const res = await invoke<PowerAdvancedSettingsResponse>("get_power_advanced_settings");
      setData(res);
      const next: Record<string, { ac: number; dc: number | null }> = {};
      for (const s of res.settings) {
        next[s.id] = { ac: s.acValue ?? 0, dc: s.dcValue };
      }
      setDrafts(next);
    } catch (error) {
      console.error("Failed to load power advanced settings:", error);
      toast({
        title: t("powerAdvanced.loadFailed") || "加载失败",
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setLoading(false);
  }, [t, toast]);

  // 读取解锁标记
  useEffect(() => {
    store.get<boolean>(UNLOCK_STORE_KEY).then((v) => {
      setUnlocked(v === true);
    });
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleChange = useCallback((id: string, ac: number, dc: number | null) => {
    setDrafts((prev) => ({ ...prev, [id]: { ac, dc } }));
  }, []);

  const applySetting = useCallback(async (id: string, ac: number, dc: number | null, successKey?: string) => {
    setApplyingId(id);
    try {
      const res = await invoke<PowerAdvancedSettingsResponse>("set_power_advanced_setting", {
        id,
        acValue: ac,
        dcValue: dc,
      });
      setData(res);
      const next: Record<string, { ac: number; dc: number | null }> = {};
      for (const s of res.settings) {
        next[s.id] = { ac: s.acValue ?? 0, dc: s.dcValue };
      }
      setDrafts(next);
      toast({
        title: successKey ? t(successKey) : t("powerAdvanced.applySuccess"),
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to apply power setting:", error);
      toast({
        title: t("powerAdvanced.applyFailed") || "修改失败",
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setApplyingId(null);
  }, [t, toast]);

  const handleApplyPreset = useCallback((id: string, ac: number, dc: number | null, kind: "recommended" | "default") => {
    applySetting(
      id,
      ac,
      data?.hasBattery ? dc : null,
      kind === "recommended" ? "powerAdvanced.setRecommendedSuccess" : "powerAdvanced.setDefaultSuccess"
    );
  }, [applySetting, data?.hasBattery]);

  const handleUnlock = async () => {
    setUnlockLoading(true);
    try {
      await store.set(UNLOCK_STORE_KEY, true);
      await store.save();
      // 解除隐藏（失败不阻塞解锁，仅提示）
      try {
        const res = await invoke<UnhideResult>("unhide_power_advanced_settings");
        if (!res.success) {
          toast({
            title: t("powerAdvanced.unlockSuccess"),
            description: res.message,
            status: "warning",
            duration: 5000,
            isClosable: true,
          });
        } else {
          toast({
            title: t("powerAdvanced.unlockSuccess"),
            status: "success",
            duration: 3000,
            isClosable: true,
          });
        }
      } catch {
        toast({
          title: t("powerAdvanced.unlockSuccess"),
          description: t("powerAdvanced.unhideFailed") || "",
          status: "warning",
          duration: 5000,
          isClosable: true,
        });
      }
      setUnlocked(true);
      onClose();
      await loadData();
    } catch (error) {
      console.error("Failed to unlock power advanced settings:", error);
      toast({
        title: t("powerAdvanced.unlockFailed") || "解锁失败",
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
    setUnlockLoading(false);
  };



  return (
    <VStack align="stretch" spacing={5}>
      {/* 页头：标题 + 当前方案 + 刷新 */}
      <Flex justify="space-between" align="center" wrap="wrap" gap={3}>
        <VStack align="start" spacing={0.5}>
          <Heading size="md" color={headingColor} fontWeight="700">
            {t("powerAdvanced.title")}
          </Heading>
          <Text fontSize="xs" color={subTextColor}>
            {t("powerAdvanced.description")}
          </Text>
        </VStack>
        <HStack spacing={3}>
          {data && (
            <HStack spacing={1.5} px={3} py={1} borderRadius="full" data-corner="round" border="1px solid" borderColor={getBorderColor()} bg={hexToRgba(primaryColor, 0.06)}>
              <Battery size={13} color={primaryColor} />
              <Text fontSize="xs" color={subTextColor}>
                {t("powerAdvanced.currentScheme")}
              </Text>
              <Text fontSize="xs" fontWeight="bold" color={headingColor}>
                {data.schemeName || data.schemeGuid}
              </Text>
            </HStack>
          )}
          <LiquidGlassButton
            size="sm"
            leftIcon={<RefreshCw size={14} />}
            onClick={loadData}
            isLoading={loading}
            variant="outline"
          >
            {t("powerAdvanced.refresh")}
          </LiquidGlassButton>
        </HStack>
      </Flex>

      {/* 锁定横幅（未解锁时） */}
      {!unlocked && (
        <LiquidGlassCard w="full" p={5}>
          <VStack align="center" spacing={3} py={2}>
            <Box
              w={12}
              h={12}
              borderRadius="full"
              data-corner="round"
              bg={hexToRgba(primaryColor, 0.12)}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={primaryColor}
            >
              <Lock size={24} />
            </Box>
            <VStack align="center" spacing={1}>
              <Text fontSize="md" fontWeight="bold" color={headingColor}>
                {t("powerAdvanced.lockedTitle")}
              </Text>
              <Text fontSize="xs" color={subTextColor} textAlign="center" maxW="lg" lineHeight="tall">
                {t("powerAdvanced.lockedDesc")}
              </Text>
            </VStack>
            <LiquidGlassButton
              leftIcon={<ShieldAlert size={15} />}
              onClick={onOpen}
              isLoading={unlockLoading}
              loadingText={t("powerAdvanced.unlocking") || "解锁中"}
            >
              {t("powerAdvanced.unlockButton")}
            </LiquidGlassButton>
          </VStack>
        </LiquidGlassCard>
      )}

      {/* 设置卡片列表 */}
      {loading ? (
        <Flex justify="center" py={12}>
          <Spinner size="lg" color={primaryColor} />
        </Flex>
      ) : (
        <VStack align="stretch" spacing={3}>
          <AnimatePresence>
            {data &&
              powerAdvancedSettings.map((def, idx) => {
                const info = data.settings.find((s) => s.id === def.id);
                if (!info) return null;
                const draft = drafts[def.id];
                return (
                  <motion.div
                    key={def.id}
                    initial={{ opacity: 0, y: 12 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ duration: 0.25, delay: Math.min(idx * 0.03, 0.3) }}
                  >
                    <PowerAdvancedSettingCard
                      def={def}
                      info={info}
                      locked={!unlocked}
                      hasBattery={data.hasBattery}
                      ac={draft?.ac ?? info.acValue ?? 0}
                      dc={draft?.dc ?? info.dcValue}
                      applying={applyingId === def.id}
                      onChange={handleChange}
                      onApply={applySetting}
                      onApplyPreset={handleApplyPreset}
                    />
                  </motion.div>
                );
              })}
          </AnimatePresence>
        </VStack>
      )}

      {/* 免责声明弹窗 */}
      <AlertDialog isOpen={isOpen} leastDestructiveRef={cancelRef} onClose={onClose} isCentered>
        <AlertDialogOverlay>
          <AlertDialogContent bg={useColorModeValue("#ffffff", "#171923")} maxW="md">
            <AlertDialogHeader fontSize="lg" fontWeight="bold" color={useColorModeValue("#C53030", "#FC8181")}>
              {t("powerAdvanced.disclaimerTitle")}
            </AlertDialogHeader>
            <AlertDialogCloseButton />
            <AlertDialogBody>
              <Text fontSize="sm" color={subTextColor} lineHeight="tall" whiteSpace="pre-wrap">
                {t("powerAdvanced.disclaimerText")}
              </Text>
              <Checkbox
                mt={4}
                size="sm"
                isChecked={agreeChecked}
                onChange={(e) => setAgreeChecked(e.target.checked)}
                colorScheme="red"
                sx={{
                  ".chakra-checkbox__control": { borderRadius: "sm" },
                }}
              >
                <Text fontSize="sm" color={headingColor}>
                  {t("powerAdvanced.agreeCheck")}
                </Text>
              </Checkbox>
            </AlertDialogBody>
            <AlertDialogFooter>
              <Button ref={cancelRef} onClick={onClose}>
                {t("powerAdvanced.cancel")}
              </Button>
              <LiquidGlassButton
                ml={3}
                isDisabled={!agreeChecked}
                isLoading={unlockLoading}
                onClick={handleUnlock}
                bg="#E53E3E"
                _hover={{ bg: "#C53030" }}
              >
                <Text color="#ffffff">{t("powerAdvanced.confirmUnlock")}</Text>
              </LiquidGlassButton>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialogOverlay>
      </AlertDialog>
    </VStack>
  );
}
