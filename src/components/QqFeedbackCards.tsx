import { Box, Text, HStack, VStack, useDisclosure, useColorModeValue } from "@chakra-ui/react";
import { FaBug, FaBook } from "react-icons/fa6";
import type { ReactNode } from "react";
import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { QqGroupModal } from "@/components/ui/qq-group-modal";
import { QqGroupIcon } from "@/components/ui/qq-group-icon";
import { openExternal, useQQGroups } from "@/hooks/use-qq-groups";
import { useThemeColor } from "@/contexts/theme-color-context";
import { store } from "@/lib/store";

const FEEDBACK_URL = "https://nexbox.top/feedback";
const DOCS_URL = "https://docs.nexbox.top";
/** 本地兜底的 QQ 群图标 */
const LOCAL_QQ_ICON = "/icons/qq.png";

/** 读取持久化开关（兼容旧 localStorage），并订阅设置页变更事件 */
function useCardEnabled(key: string, event: string) {
  const [state, setState] = useState({ enabled: false, ready: false });

  useEffect(() => {
    (async () => {
      let enabled = true;
      const saved = await store.get<boolean>(key);
      if (saved !== null && saved !== undefined) {
        enabled = saved;
      } else {
        const ls = localStorage.getItem(key);
        if (ls !== null) enabled = ls === "true";
      }
      setState((s) => ({ ...s, enabled, ready: true }));
    })();
  }, [key]);

  useEffect(() => {
    const handler = (e: CustomEvent) => setState((s) => ({ ...s, enabled: e.detail }));
    window.addEventListener(event, handler as EventListener);
    return () => window.removeEventListener(event, handler as EventListener);
  }, [event]);

  return state;
}

/** 首页「问题反馈」卡片显示开关（持久化，默认开启） */
export function useFeedbackEnabled() {
  return useCardEnabled("nexbox_feedback_enabled", "feedback-setting-changed");
}

/** 首页「官方QQ群」卡片显示开关（持久化，默认开启） */
export function useQqGroupCardEnabled() {
  return useCardEnabled("nexbox_qq_group_card_enabled", "qq-group-card-setting-changed");
}

/** 首页「使用文档」卡片显示开关（持久化，默认开启） */
export function useDocsCardEnabled() {
  return useCardEnabled("nexbox_docs_card_enabled", "docs-card-setting-changed");
}

/** 单卡片外壳：纯浅色/深色模式适配（不使用主题主色），且不参与路由切换弹跳动画 */
function FeedbackLinkCard({
  icon,
  title,
  subtitle,
  onClick,
}: {
  icon: ReactNode;
  title: string;
  subtitle: string;
  onClick: () => void;
}) {
  const { getActiveColor } = useThemeColor();
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const subColor = useColorModeValue("gray.500", "#9ca3af");
  const iconBg = useColorModeValue("#f1f2f4", "#262626");
  const arrowColor = useColorModeValue("gray.400", "#6b7280");

  return (
    <LiquidGlassCard
      className="no-bounce"
      role="group"
      py={2.5}
      px={3}
      w="260px"
      cursor="pointer"
      onClick={onClick}
      transition="border-color 0.2s"
      _hover={{ borderColor: getActiveColor() }}
    >
      <HStack spacing={3}>
        <Box
          w="34px"
          h="34px"
          borderRadius="lg"
          bg={iconBg}
          display="flex"
          alignItems="center"
          justifyContent="center"
          flexShrink={0}
          overflow="hidden"
        >
          {icon}
        </Box>
        <VStack spacing={0} align="start">
          <Text fontSize="sm" fontWeight="bold" color={titleColor} noOfLines={1}>
            {title}
          </Text>
          <Text fontSize="xs" color={subColor} noOfLines={1}>
            {subtitle}
          </Text>
        </VStack>
        <Box ml="auto" color={arrowColor} _groupHover={{ color: getActiveColor() }}>
          <Text fontSize="lg" lineHeight="1">›</Text>
        </Box>
      </HStack>
    </LiquidGlassCard>
  );
}

/** 首页「问题反馈」卡片：点击跳转官网反馈 */
export function FeedbackCard() {
  const { t } = useTranslation();
  // 注意：react-icons 的 <svg color> 不经过 Chakra 主题解析，必须传真实 hex，
  // 不能用 Chakra token（如 gray.800），否则浅色模式下图标会退化为白色
  const iconColor = useColorModeValue("#1a202c", "#ffffff");
  return (
    <FeedbackLinkCard
      icon={<FaBug size={18} color={iconColor} />}
      title={t("home.feedbackCard.title")}
      subtitle={t("home.feedbackCard.subtitle")}
      onClick={() => openExternal(FEEDBACK_URL)}
    />
  );
}

/** 首页「使用文档」卡片：点击跳转在线文档 */
export function DocsCard() {
  const { t } = useTranslation();
  // 注意：react-icons 的 <svg color> 不经过 Chakra 主题解析，必须传真实 hex，
  // 不能用 Chakra token（如 gray.800），否则浅色模式下图标会退化为白色
  const iconColor = useColorModeValue("#1a202c", "#ffffff");
  return (
    <FeedbackLinkCard
      icon={<FaBook size={18} color={iconColor} />}
      title={t("home.docsCard.title")}
      subtitle={t("home.docsCard.subtitle")}
      onClick={() => openExternal(DOCS_URL)}
    />
  );
}

/** 首页「官方QQ群」卡片：图标取①群的 gitee 图标（后端下载显示），打开弹窗 */
export function QqGroupCard() {
  const { t } = useTranslation();
  const { isOpen, onOpen, onClose } = useDisclosure();
  const { groups } = useQQGroups();

  return (
    <>
      <FeedbackLinkCard
        icon={<QqGroupIcon url={groups[0]?.icon} size={34} />}
        title={t("home.qqGroup.cardTitle")}
        subtitle={t("home.qqGroup.cardSubtitle")}
        onClick={onOpen}
      />
      <QqGroupModal isOpen={isOpen} onClose={onClose} />
    </>
  );
}