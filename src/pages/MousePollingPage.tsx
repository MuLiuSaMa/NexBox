import {
  Badge,
  Box,
  Button,
  HStack,
  SimpleGrid,
  Text,
  VStack,
  useColorModeValue,
} from "@chakra-ui/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MousePointer2 } from "lucide-react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { PageHeader } from "@/components/special/page-header";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";

/** 后端每 200ms 推送的统计快照（None 字段 = 样本不足） */
interface PollStats {
  moving: boolean;
  hz: number | null;
  total: number;
  avg_ms: number | null;
  median_ms: number | null;
  min_ms: number | null;
  max_ms: number | null;
  jitter_ms: number | null;
  spikes: number;
}

/** 标称轮询率档位 */
const GEARS = [125, 250, 500, 1000, 2000, 4000, 8000];

function nearestGear(hz: number): number {
  let best = GEARS[0];
  for (const g of GEARS) {
    if (Math.abs(hz - g) < Math.abs(hz - best)) best = g;
  }
  return best;
}

const fmtMs = (v: number | null | undefined) =>
  v == null ? "--" : `${v.toFixed(2)} ms`;
const fmtInt = (v: number | null | undefined) =>
  v == null ? "--" : String(Math.round(v));

// 统一区块卡片容器（与其他工具页一致：全宽大卡 p={6}，玻璃开关双形态）
function BlockCard({ children }: { children: React.ReactNode }) {
  const { liquidGlassEnabled } = useBackground();
  const cardBg = useColorModeValue("white", "#111111");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const inner = <VStack align="stretch" spacing={4}>{children}</VStack>;

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard w="full" p={6}>
        {inner}
      </LiquidGlassCard>
    );
  }

  return (
    <Box
      bg={cardBg}
      borderRadius="3xl"
      border="1px solid"
      borderColor={borderColor}
      w="full"
      p={6}
    >
      {inner}
    </Box>
  );
}

// 区块头部：标题 + 描述（本页不用图标）
function SectionHeader({
  title,
  desc,
}: {
  title: string;
  desc?: string;
}) {
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  return (
    <HStack spacing={3} align="center">
      <Box flex={1} minW={0}>
        <Text fontWeight="medium" fontSize="sm" color={textColor}>
          {title}
        </Text>
        {desc ? (
          <Text fontSize="xs" color={subTextColor} mt={0.5}>
            {desc}
          </Text>
        ) : null}
      </Box>
    </HStack>
  );
}

// 内嵌统计小卡：玻璃开=嵌套 LiquidGlassCard，玻璃关=浅色底（与磁盘健康页一致）
function StatCell({ label, value }: { label: string; value: string }) {
  const { liquidGlassEnabled } = useBackground();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const flatBg = useColorModeValue("gray.50", "rgba(255,255,255,0.03)");

  const inner = (
    <>
      <Text fontSize="xs" color={subTextColor} mb={1}>
        {label}
      </Text>
      <Text
        fontSize="lg"
        fontWeight="bold"
        color={headingColor}
        sx={{ fontVariantNumeric: "tabular-nums" }}
      >
        {value}
      </Text>
    </>
  );

  if (liquidGlassEnabled) {
    return (
      <LiquidGlassCard p={3} textAlign="center">
        {inner}
      </LiquidGlassCard>
    );
  }

  return (
    <Box textAlign="center" p={3} borderRadius="lg" bg={flatBg}>
      {inner}
    </Box>
  );
}

export default function MousePollingPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("mouse");
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");

  const [testing, setTesting] = useState(false);
  const [stats, setStats] = useState<PollStats | null>(null);
  const testingRef = useRef(testing);
  testingRef.current = testing;

  useEffect(() => {
    let disposed = false;
    const unlistenPromise = listen<PollStats>("mouse-poll-stats", (event) => {
      if (!disposed && testingRef.current) {
        setStats(event.payload);
      }
    });
    return () => {
      disposed = true;
      void unlistenPromise.then((fn) => fn());
      // 离开页面自动停止并卸载钩子
      invoke("mouse_poll_stop").catch(() => {});
    };
  }, []);

  const handleToggle = async () => {
    if (testing) {
      setTesting(false);
      try {
        await invoke("mouse_poll_stop");
      } catch (error) {
        toast({
          title: t("mousePolling.stopFailed"),
          description: String(error),
          status: "error",
          duration: 4000,
          isClosable: true,
        });
      }
      return;
    }
    try {
      await invoke("mouse_poll_start");
      setStats(null);
      setTesting(true);
    } catch (error) {
      toast({
        title: t("mousePolling.startFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    }
  };

  const hasData = stats != null;
  const hz = stats?.moving && stats.hz != null ? stats.hz : null;
  const gear = hz != null ? nearestGear(hz) : null;
  const deviation =
    hz != null && gear != null ? ((hz - gear) / gear) * 100 : null;

  const statItems = [
    { label: t("mousePolling.avgInterval"), value: fmtMs(stats?.avg_ms) },
    { label: t("mousePolling.medianInterval"), value: fmtMs(stats?.median_ms) },
    { label: t("mousePolling.minInterval"), value: fmtMs(stats?.min_ms) },
    { label: t("mousePolling.maxInterval"), value: fmtMs(stats?.max_ms) },
    { label: t("mousePolling.jitter"), value: fmtMs(stats?.jitter_ms) },
    { label: t("mousePolling.spikes"), value: fmtInt(stats?.spikes) },
    { label: t("mousePolling.totalSamples"), value: fmtInt(stats?.total) },
  ];

  // 未测试：有历史结果则安静展示，无结果提示如何开始；测试中未移动则提示移动
  const statusText = !testing
    ? hasData
      ? ""
      : t("mousePolling.startHint")
    : !stats?.moving
      ? t("mousePolling.moveHint")
      : "";

  return (
    <Box pt={8} pb={8}>
      <PageHeader title={t("mousePolling.title")} mb={6}>
        <Badge
          bg={testing ? primaryColor : undefined}
          color={testing ? contrastText : subTextColor}
          border={testing ? undefined : "1px solid"}
          borderColor={testing ? undefined : borderColor}
          px={3}
          py={1}
          borderRadius="full"
          fontSize="sm"
        >
          {testing ? t("mousePolling.testing") : t("mousePolling.stopped")}
        </Badge>
      </PageHeader>

      <VStack align="stretch" spacing={6}>
        {/* 区块1：实时轮询率 */}
        <BlockCard>
          <SectionHeader title={t("mousePolling.realtimeHz")} />

          <VStack spacing={3} py={6}>
            <HStack spacing={2} align="baseline">
              <Text
                fontSize={{ base: "56px", md: "72px" }}
                fontWeight="700"
                lineHeight="1"
                color={hz != null ? primaryColor : subTextColor}
                transition="color 0.2s"
                sx={{ fontVariantNumeric: "tabular-nums" }}
              >
                {hz != null ? hz.toFixed(1) : "--"}
              </Text>
              <Text fontSize="lg" fontWeight="600" color={subTextColor}>
                Hz
              </Text>
            </HStack>

            {gear != null && (
              <HStack spacing={2}>
                <Badge
                  borderRadius="full"
                  px={3}
                  py={1}
                  fontSize="sm"
                  color={contrastText}
                  bg={primaryColor}
                >
                  ≈ {gear}Hz
                </Badge>
                <Text fontSize="xs" color={subTextColor}>
                  {t("mousePolling.deviation")}{" "}
                  {deviation != null && deviation >= 0 ? "+" : ""}
                  {deviation?.toFixed(1)}%
                </Text>
              </HStack>
            )}

            {statusText && (
              <HStack spacing={1.5}>
                <MousePointer2
                  size={13}
                  color={subTextColor}
                  style={{ flexShrink: 0 }}
                />
                <Text fontSize="xs" color={subTextColor}>
                  {statusText}
                </Text>
              </HStack>
            )}
          </VStack>

          <Button
            size="lg"
            borderRadius="xl"
            w="full"
            bg={testing ? "transparent" : primaryColor}
            color={testing ? headingColor : contrastText}
            border={testing ? "1px solid" : "none"}
            borderColor={borderColor}
            fontWeight="600"
            onClick={() => void handleToggle()}
            _hover={
              testing ? { bg: hexToRgba(primaryColor, 0.08) } : { bg: hexToRgba(primaryColor, 0.85) }
            }
            _active={{ bg: hexToRgba(primaryColor, 0.7) }}
          >
            {testing ? t("mousePolling.stop") : t("mousePolling.start")}
          </Button>
        </BlockCard>

        {/* 区块2：统计明细 */}
        <BlockCard>
          <SectionHeader title={t("mousePolling.statsTitle")} />

          <SimpleGrid columns={{ base: 2, md: 4 }} spacing={4}>
            {statItems.map((item) => (
              <StatCell key={item.label} label={item.label} value={item.value} />
            ))}
          </SimpleGrid>

          <Text fontSize="xs" color={subTextColor} lineHeight="1.7">
            {t("mousePolling.hint")}
          </Text>
        </BlockCard>
      </VStack>
    </Box>
  );
}
