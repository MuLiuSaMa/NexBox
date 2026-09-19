import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  IconButton,
  Button,
  Badge,
  Tooltip,
  useColorModeValue,
} from "@chakra-ui/react";
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Clock, Server, ShieldCheck, RefreshCw } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useNavPosition } from "@/components/ui/main-layout";

interface NtpQueryResult {
  localTimeMs: number;
  serverTimeMs: number;
  offsetMs: number;
  latencyMs: number;
  ok: boolean;
}

interface ServerCardData {
  id: string;
  name: string;
  host: string;
  result: NtpQueryResult | null;
  error: string | null;
  applying: boolean;
  syncedMsg: string | null;
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : `${n}`;
}

// 迷你统计列：数值在上、标签在下
function MiniStat({ label, value, color }: { label: string; value: string; color: string }) {
  const subColor = useColorModeValue("gray.500", "#888888");
  return (
    <Box flex={1} textAlign="center" px={1} minW={0}>
      <Text
        fontSize="md"
        fontWeight="bold"
        color={color}
        noOfLines={1}
        style={{ fontVariantNumeric: "tabular-nums" }}
      >
        {value}
      </Text>
      <Text fontSize="10px" color={subColor} noOfLines={1}>
        {label}
      </Text>
    </Box>
  );
}

function MiniDivider() {
  const bg = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)");
  return <Box w="1px" bg={bg} my={1} flexShrink={0} />;
}

export default function TimeSyncPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { getActiveColor } = useThemeColor();

  const activeColor = getActiveColor();
  const subColor = useColorModeValue("gray.500", "#888888");
  const mainTextColor = useColorModeValue("#000000", "#ffffff");

  // 实时系统时钟（每秒跳动）
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(id);
  }, []);

  // 每个时间服务器一张卡片
  const [cards, setCards] = useState<ServerCardData[]>([]);
  const [isAdmin, setIsAdmin] = useState(false);
  const [batchRunning, setBatchRunning] = useState(false);
  // 整批统一的上次检测时间（每轮同时发起，时间一致）
  const [batchQueriedAt, setBatchQueriedAt] = useState<number | null>(null);

  const cardsRef = useRef<ServerCardData[]>([]);
  // 每台服务器各自的在途请求集合，避免同一服务器重复并发请求（不堆叠）
  const inflightRef = useRef<Set<string>>(new Set());
  const activeCountRef = useRef(0);
  useEffect(() => {
    cardsRef.current = cards;
  }, [cards]);

  // 实时轮询间隔（毫秒）
  const POLL_INTERVAL_MS = 1000;

  const setActive = useCallback((delta: number) => {
    activeCountRef.current += delta;
    setBatchRunning(activeCountRef.current > 0);
  }, []);

  // 并发发起一轮检测：所有服务器同时请求、各自独立更新、互不阻塞（严格并行）。
  // 慢/失败的服务器（如微软间歇性超时）只影响自己，下一轮 1s 后自动重试。
  const fireBatch = useCallback(() => {
    setBatchQueriedAt(Date.now());
    cardsRef.current.forEach((c) => {
      if (inflightRef.current.has(c.id)) return;
      inflightRef.current.add(c.id);
      setActive(1);
      invoke<NtpQueryResult>("ntp_query", { server: c.id })
        .then((r) =>
          setCards((prev) =>
            prev.map((x) => (x.id === c.id ? { ...x, result: r, error: null } : x)),
          ),
        )
        .catch((err) => {
          console.error("ntp query failed:", err);
          setCards((prev) =>
            prev.map((x) => (x.id === c.id ? { ...x, error: String(err) } : x)),
          );
        })
        .finally(() => {
          inflightRef.current.delete(c.id);
          setActive(-1);
        });
    });
  }, [setActive]);

  // 加载服务器列表 + 管理员状态，并立即自动检测
  useEffect(() => {
    let cancelled = false;
    invoke<{ id: string; name: string; host: string }[]>("get_ntp_servers")
      .then((list) => {
        if (cancelled || !list || list.length === 0) return;
        const mapped = list.map((s) => ({
          id: s.id,
          name: s.name,
          host: s.host,
          result: null,
          error: null,
          applying: false,
          syncedMsg: null,
        }));
        cardsRef.current = mapped;
        setCards(mapped);
        fireBatch();
      })
      .catch((err) => console.error("load ntp servers failed:", err));
    invoke<boolean>("is_app_admin")
      .then((v) => !cancelled && setIsAdmin(!!v))
      .catch((err) => console.error("check admin failed:", err));
    return () => {
      cancelled = true;
    };
  }, [fireBatch]);

  // 实时轮询：每 1s 并发检测一轮
  useEffect(() => {
    if (cards.length === 0) return;
    const id = setInterval(() => fireBatch(), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [cards.length, fireBatch]);

  // 校准：直接用最近一次检测到的偏差（毫秒），无需重新联网，秒级完成
  const handleApply = useCallback(
    async (id: string) => {
      const card = cardsRef.current.find((c) => c.id === id);
      if (!card?.result) return;
      setCards((prev) =>
        prev.map((c) => (c.id === id ? { ...c, applying: true, syncedMsg: null } : c)),
      );
      try {
        await invoke("apply_time_offset", { offsetMs: card.result.offsetMs });
        setCards((prev) =>
          prev.map((c) => (c.id === id ? { ...c, applying: false, syncedMsg: t("timeSync.synced") } : c)),
        );
      } catch (err) {
        console.error("apply time offset failed:", err);
        setCards((prev) =>
          prev.map((c) => (c.id === id ? { ...c, applying: false, error: String(err) } : c)),
        );
      }
    },
    [t],
  );

  const adaptiveTitle = useAdaptiveTextColor();
  const navPosition = useNavPosition();
  const fillMinH = navPosition === "top" ? "calc(100vh - 152px)" : "calc(100vh - 88px)";

  // 时钟/日期格式化
  const timeStr = `${pad2(now.getHours())}:${pad2(now.getMinutes())}:${pad2(now.getSeconds())}`;
  const dateStr = now.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
  const weekStr = now.toLocaleDateString(undefined, { weekday: "long" });

  return (
    <Box pt={2} minH={fillMinH} display="flex" flexDirection="column">
      <VStack align="stretch" spacing={4} flex={1}>
        <HStack justify="space-between" align="center" flexWrap="wrap">
          <HStack spacing={3}>
            <IconButton
              aria-label="back"
              icon={<ArrowLeft size={20} />}
              variant="ghost"
              onClick={() => navigate(-1)}
            />
            <HStack spacing={2}>
              <Clock color={activeColor} />
              <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
                {t("timeSync.title")}
              </Heading>
            </HStack>
          </HStack>
          <HStack spacing={2}>
            <Tooltip label={t("timeSync.redetect")}>
              <IconButton
                aria-label={t("timeSync.redetect")}
                icon={<RefreshCw size={16} />}
                variant="ghost"
                isLoading={batchRunning}
                isDisabled={batchRunning}
                onClick={() => fireBatch()}
              />
            </Tooltip>
            <Tooltip label={t("timeSync.adminRequired")} isDisabled={isAdmin}>
              <Badge
                px={2}
                py={0.5}
                borderRadius="full"
                fontSize="xs"
                colorScheme={isAdmin ? "green" : "orange"}
              >
                <HStack spacing={1}>
                  <ShieldCheck size={12} />
                  <Text as="span">
                    {isAdmin ? t("timeSync.adminOk") : t("timeSync.adminNo")}
                  </Text>
                </HStack>
              </Badge>
            </Tooltip>
          </HStack>
        </HStack>

        {/* 实时系统时钟卡片 */}
        <LiquidGlassCard p={4}>
          <HStack justify="space-between" align="center" flexWrap="wrap" spacing={4}>
            <VStack align="start" spacing={0.5}>
              <Text fontSize="xs" fontWeight="medium" color={subColor} letterSpacing="0.05em">
                {t("timeSync.systemTime")}
              </Text>
              <Text
                fontSize={{ base: "4xl", lg: "5xl" }}
                fontWeight="bold"
                color={mainTextColor}
                lineHeight="1"
                style={{ fontVariantNumeric: "tabular-nums" }}
              >
                {timeStr}
              </Text>
            </VStack>
            <VStack align="end" spacing={0.5}>
              <Text fontSize="sm" color={subColor}>{dateStr}</Text>
              <Text fontSize="sm" color={activeColor} fontWeight="medium">{weekStr}</Text>
              {batchQueriedAt && (
                <Text fontSize="xs" color={subColor}>
                  {t("timeSync.lastUpdate")}:{" "}
                  {new Date(batchQueriedAt).toLocaleTimeString(undefined, {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })}
                </Text>
              )}
            </VStack>
          </HStack>
        </LiquidGlassCard>

        {/* 每个服务器一张紧凑卡片：自动检测 + 校准按钮 */}
        <Box
          display="grid"
          gridTemplateColumns={{ base: "1fr", sm: "1fr 1fr", xl: "1fr 1fr 1fr" }}
          gap={3}
        >
          {cards.map((card) => {
            const r = card.result;
            const offsetSecs = r ? r.offsetMs / 1000 : 0;
            const offsetAbs = Math.abs(offsetSecs);
            const offsetColor = !r ? subColor : offsetAbs < 1 ? "#38A169" : offsetAbs < 10 ? "#DD6B20" : "#E53E3E";
            const offsetStr = r ? `${offsetSecs >= 0 ? "+" : ""}${offsetSecs.toFixed(2)}` : "--";
            const serverTimeStr = r
              ? new Date(r.serverTimeMs).toLocaleTimeString(undefined, {
                  hour: "2-digit",
                  minute: "2-digit",
                  second: "2-digit",
                })
              : "--";
            const latencyStr = r ? r.latencyMs.toFixed(1) : "--";

            let badgeText: string;
            let badgeScheme: string;
            if (r) {
              badgeText = t("timeSync.normal");
              badgeScheme = "green";
            } else {
              badgeText = t("timeSync.notQueried");
              badgeScheme = "gray";
            }

            return (
              <LiquidGlassCard key={card.id} p={3} display="flex" flexDirection="column" gap={2}>
                {/* 服务器名 + 状态 */}
                <HStack justify="space-between" align="center" spacing={2}>
                  <HStack spacing={1.5} minW={0}>
                    <Box color={activeColor} flexShrink={0}>
                      <Server size={14} />
                    </Box>
                    <VStack align="start" spacing={0} minW={0}>
                      <Text fontWeight="semibold" color={mainTextColor} fontSize="sm" noOfLines={1}>
                        {card.name}
                      </Text>
                      <Text fontSize="xs" color={subColor} noOfLines={1}>
                        {card.host}
                      </Text>
                    </VStack>
                  </HStack>
                  {card.error ? (
                    <Text
                      flexShrink={0}
                      maxW="55%"
                      fontSize="xs"
                      color="#E53E3E"
                      noOfLines={1}
                      textAlign="right"
                      title={card.error}
                    >
                      {card.error}
                    </Text>
                  ) : card.syncedMsg ? (
                    <Text
                      flexShrink={0}
                      maxW="55%"
                      fontSize="xs"
                      color="#38A169"
                      noOfLines={1}
                      textAlign="right"
                      title={card.syncedMsg}
                    >
                      {card.syncedMsg}
                    </Text>
                  ) : (
                    <Badge flexShrink={0} px={1.5} py={0.5} borderRadius="full" fontSize="10px" colorScheme={badgeScheme}>
                      {badgeText}
                    </Badge>
                  )}
                </HStack>

                {/* 迷你统计：服务器时间 / 偏差 / 延迟 */}
                <HStack spacing={0} align="stretch">
                  <MiniStat label={t("timeSync.serverTime")} value={serverTimeStr} color={activeColor} />
                  <MiniDivider />
                  <MiniStat label={t("timeSync.offset")} value={offsetStr} color={offsetColor} />
                  <MiniDivider />
                  <MiniStat label={t("timeSync.latency")} value={latencyStr} color="#DD6B20" />
                </HStack>

                {/* 校准按钮（自动检测，无需手动点检测） */}
                <Tooltip
                  label={!isAdmin ? t("timeSync.adminRequired") : t("timeSync.applyHint")}
                  isDisabled={isAdmin && !!r}
                >
                  <Button
                    w="full"
                    size="sm"
                    minH="34px"
                    bg="transparent"
                    border="1px solid"
                    borderColor={activeColor}
                    color={activeColor}
                    _hover={isAdmin && r ? { bg: `${activeColor}14` } : undefined}
                    _active={{ transform: "scale(0.97)" }}
                    transition="background-color 0.15s ease-in-out, transform 0.1s ease"
                    isLoading={card.applying}
                    loadingText={t("timeSync.applying")}
                    isDisabled={!isAdmin || card.applying || !r}
                    leftIcon={<RefreshCw size={13} />}
                    onClick={() => handleApply(card.id)}
                    borderRadius="lg"
                  >
                    {t("timeSync.apply")}
                  </Button>
                </Tooltip>
              </LiquidGlassCard>
            );
          })}
        </Box>

        <Text fontSize="xs" color={subColor}>
          {t("timeSync.offsetNote")}
        </Text>
      </VStack>
    </Box>
  );
}
