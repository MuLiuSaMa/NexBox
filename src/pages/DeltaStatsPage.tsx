import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Button,
  Badge,
  Spinner,
  SimpleGrid,
  useColorModeValue,
  useColorMode,
  useToast,
  IconButton,
  Tooltip,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
} from "@chakra-ui/react";
import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  BarChart3,
  Package,
  Trophy,
  UserRound,
  LogOut,
  RefreshCw,
  QrCode,
  Gamepad2,
  Flame,
  Crosshair,
  Shield,
  Swords,
  Wallet,
  Crown,
  Skull,
  Target,
  MapPin,
  Filter,
  Clock,
  X,
  TrendingUp,
  Coins,
  CheckCircle2,
  AlertCircle,
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import qqPlatformLogo from "@/assets/df-qq.png";
import wechatPlatformLogo from "@/assets/df-wechat.png";

// ═══ 类型 ═══
interface DfLoginState {
  logged_in: boolean;
  openid: string;
  area: string;
  nickname: string;
  avatar: string;
  level: string;
  tgp_id: string;
  account_type?: number;
  login_time: number;
}

interface RoleInfo {
  openid: string;
  area: string;
  name?: string;
  icon?: string;
  level?: number | string;
  tgp_id?: string;
  sol_level?: number | string;
  tdm_level?: number | string;
  [key: string]: unknown;
}

// ═══ 干员/地图/皮肤 字典辅助 ═══
interface AgentDictEntry {
  id: number;
  name: string;
  avatar: string;
  avatar2?: string;
}
interface MapDetailEntry {
  id: number;
  name: string;
  pic: string;
}
interface SkinDictEntry {
  objectID: number;
  objectName: string;
  pic?: string;
  grade?: number;
  secondClass?: string;
  secondClassCN?: string;
  primaryClass?: string;
  assetsDetail?: { isArchive?: boolean; seasonID?: number; tags?: string };
  [key: string]: unknown;
}

// 从磁盘 data_agents / data_maps / data_skins 加载字典（后端采集时已落盘）
async function loadAgentMapDict() {
  const [agentsRaw, mapsRaw, skinsRaw] = await Promise.all([
    invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "agents" }).catch(() => null),
    invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "maps" }).catch(() => null),
    invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "skins" }).catch(() => null),
  ]);
  const agentMap = new Map<number, AgentDictEntry>();
  const agentList = (agentsRaw?.data as AgentDictEntry[] | undefined) || [];
  agentList.forEach((a) => { if (a && a.id != null) agentMap.set(Number(a.id), a); });

  const mapDetail = (mapsRaw?.mapDetail as MapDetailEntry[] | undefined) || [];
  const mapMap = new Map<number, MapDetailEntry>();
  mapDetail.forEach((m) => { if (m && m.id != null) mapMap.set(Number(m.id), m); });

  const skinList = (skinsRaw?.jData?.data?.data?.list as SkinDictEntry[] | undefined)
    || (skinsRaw?.jData?.data?.list as SkinDictEntry[] | undefined) || [];
  const skinMap = new Map<number, SkinDictEntry>();
  skinList.forEach((s) => { if (s && s.objectID != null) skinMap.set(Number(s.objectID), s); });

  return { agentMap, mapMap, skinMap };
}

function getAgentName(id: number | string | null | undefined, agentMap: Map<number, AgentDictEntry>): string {
  if (id == null) return "";
  return agentMap.get(Number(id))?.name || "";
}
function getAgentAvatar(id: number | string | null | undefined, agentMap: Map<number, AgentDictEntry>): string {
  if (id == null) return "";
  return agentMap.get(Number(id))?.avatar || "";
}
function getMapName(id: number | string | null | undefined, mapMap: Map<number, MapDetailEntry>): string {
  if (id == null) return "";
  const m = mapMap.get(Number(id));
  if (m) return m.name;
  return "";
}
function getMapPic(id: number | string | null | undefined, mapMap: Map<number, MapDetailEntry>): string {
  if (id == null) return "";
  return mapMap.get(Number(id))?.pic || "";
}

// ═══ 数据格式化 ═══
function formatNum(n: number | string | undefined | null): string {
  const v = Number(n ?? 0);
  if (isNaN(v)) return "0";
  if (Math.abs(v) >= 100000000) return (v / 100000000).toFixed(2) + "亿";
  if (Math.abs(v) >= 10000) return (v / 10000).toFixed(1) + "万";
  return v.toLocaleString();
}

function formatRate(n: number | string | undefined | null): string {
  const v = Number(n ?? 0);
  if (isNaN(v)) return "0%";
  return v + "%";
}

function formatTime(sec: number | string | undefined | null): string {
  const s = Number(sec ?? 0);
  if (isNaN(s)) return "0h";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h >= 100) return `${h}h`;
  if (h > 0) return `${h}h${m}m`;
  return `${m}m`;
}

// ═══ 通用统计卡片 ═══
function StatCard({
  label,
  value,
  icon: Icon,
  color,
  sub,
}: {
  label: string;
  value: string;
  icon: React.ComponentType<{ size?: number; color?: string }>;
  color: string;
  sub?: string;
}) {
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  return (
    <LiquidGlassCard p={4}>
      <VStack align="flex-start" spacing={1.5}>
        <HStack spacing={2}>
          <Box bg={`${color}1a`} borderRadius="md" p={1.5} display="flex" alignItems="center" justifyContent="center">
            <Icon size={14} color={color} />
          </Box>
          <Text fontSize="xs" color={subTextColor}>{label}</Text>
        </HStack>
        <Text fontSize="xl" fontWeight="800" color={textColor} lineHeight="1.2">{value}</Text>
        {sub && <Text fontSize="xs" color={subTextColor}>{sub}</Text>}
      </VStack>
    </LiquidGlassCard>
  );
}

// ═══ 获取数据源（未登录）页面：QQ / 微信 扫码登录 ═══
type LoginPhase = "idle" | "waiting" | "scanned" | "success" | "expired";

// 二维码卡片四角主题色装饰
function QrCorners({ color }: { color: string }) {
  const base: React.CSSProperties = { position: "absolute", width: 18, height: 18, borderColor: color, borderStyle: "solid" };
  return (
    <>
      <Box style={{ ...base, top: -1, left: -1, borderWidth: "2.5px 0 0 2.5px", borderTopLeftRadius: 8 }} />
      <Box style={{ ...base, top: -1, right: -1, borderWidth: "2.5px 2.5px 0 0", borderTopRightRadius: 8 }} />
      <Box style={{ ...base, bottom: -1, left: -1, borderWidth: "0 0 2.5px 2.5px", borderBottomLeftRadius: 8 }} />
      <Box style={{ ...base, bottom: -1, right: -1, borderWidth: "0 2.5px 2.5px 0", borderBottomRightRadius: 8 }} />
    </>
  );
}

function DataSourceLogin({ onLogin }: { onLogin: () => void }) {
  const { getActiveColor, getBorderColor, getHoverColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const borderColor = getBorderColor();
  const hoverBg = getHoverColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const cardBorder = useColorModeValue("gray.200", "#333333");

  const [method, setMethod] = useState<"qq" | "wx">("qq");
  const [phase, setPhase] = useState<LoginPhase>("idle");
  const [qr, setQr] = useState("");
  const [qrImgType, setQrImgType] = useState("image/png");
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState("");

  const qrsigRef = useRef("");
  const uuidRef = useRef("");
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const expiredCountRef = useRef(0);
  const genSeqRef = useRef(0);
  const onLoginRef = useRef(onLogin);
  onLoginRef.current = onLogin;

  // 生成二维码（同时作为「过期后刷新」「切换平台」入口）
  const generate = useCallback(async (m: "qq" | "wx") => {
    const seq = ++genSeqRef.current;
    setGenerating(true);
    setError("");
    expiredCountRef.current = 0;
    try {
      if (m === "qq") {
        const r = await invoke<{ qr_base64: string; qrsig: string }>("df_stats_qr_gen");
        if (seq !== genSeqRef.current) return;
        qrsigRef.current = r.qrsig;
        uuidRef.current = "";
        setQrImgType("image/png");
        setQr(r.qr_base64);
      } else {
        const r = await invoke<{ qr_base64: string; uuid: string; img_type?: string }>("df_stats_qr_wx_gen");
        if (seq !== genSeqRef.current) return;
        uuidRef.current = r.uuid;
        qrsigRef.current = "";
        setQrImgType(r.img_type || "image/jpeg");
        setQr(r.qr_base64);
      }
      if (seq !== genSeqRef.current) return;
      setPhase("waiting");
    } catch (e) {
      if (seq === genSeqRef.current) setError(String(e));
    } finally {
      if (seq === genSeqRef.current) setGenerating(false);
    }
  }, []);

  // 切换登录平台：清除旧状态并立即生成所选平台二维码（选哪个就更新哪个）
  const switchMethod = (m: "qq" | "wx") => {
    if (m === method) return;
    setMethod(m);
    setPhase("idle");
    setQr("");
    setError("");
    expiredCountRef.current = 0;
    void generate(m);
  };

  // 扫码状态轮询：单飞递归 setTimeout（上一轮完成后才调度下一轮，永不并发叠加，
  // 避免微信长轮询并发请求导致会话作废）
  useEffect(() => {
    if (phase !== "waiting" && phase !== "scanned") return;
    let cancelled = false;

    const doPoll = async () => {
      if (cancelled) return;
      try {
        if (method === "qq") {
          const qrsig = qrsigRef.current;
          if (qrsig) {
            let h = 0;
            for (let i = 0; i < qrsig.length; i++) {
              h = ((h * 33) + qrsig.charCodeAt(i)) & 0x7fffffff;
            }
            const r = await invoke<{ code: string; raw: string }>("df_stats_qr_poll", { qrsig, ptqrtoken: String(h) });
            if (cancelled) return;
            if (r.code === "0") {
              setPhase("success");
              onLoginRef.current();
              return;
            } else if (r.code === "65") {
              setPhase("expired");
              return;
            } else if (r.code === "66") {
              setPhase("waiting");
            } else if (r.code === "-1" && String(r.raw || "").includes("'67'")) {
              // QQ 已扫码待确认
              setPhase("scanned");
            }
          }
        } else {
          const uuid = uuidRef.current;
          if (uuid) {
            const r = await invoke<{ state: string; code?: string }>("df_stats_qr_wx_poll", { uuid });
            if (cancelled) return;
            if (r.code === "0") {
              setPhase("success");
              onLoginRef.current();
              return;
            } else if (r.state === "scanned") {
              setPhase("scanned");
            } else if (r.state === "expired") {
              // 微信长轮询偶发 404（会话竞争/首个请求），连续多次才判定真正过期
              expiredCountRef.current += 1;
              if (expiredCountRef.current >= 4) {
                setPhase("expired");
                return;
              }
            }
            // waiting / retry / unknown：保持当前阶段继续轮询
          }
        }
      } catch {
        // 网络抖动：忽略，继续轮询
      }
      if (!cancelled) pollTimerRef.current = setTimeout(doPoll, method === "qq" ? 2000 : 3000);
    };

    doPoll();
    return () => {
      cancelled = true;
      if (pollTimerRef.current) {
        clearTimeout(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    };
  }, [phase, method]);

  // 首次进入直接生成默认平台（QQ）二维码，右侧直接展示
  useEffect(() => {
    void generate("qq");
    return () => { genSeqRef.current += 1; };
  }, [generate]);

  const statusText =
    phase === "waiting" ? (method === "qq" ? "请打开 QQ 扫一扫" : "请打开微信扫一扫")
    : phase === "scanned" ? "已扫码，请在手机上确认"
    : phase === "success" ? "登录成功，正在加载数据…"
    : "二维码已失效";
  const statusOk = phase === "scanned" || phase === "success";

  const platforms = [
    { id: "qq", name: "QQ 登录", desc: "使用 QQ 扫一扫授权", logo: qqPlatformLogo, logoBg: "#ffffff", imgSize: 28 },
    { id: "wx", name: "微信登录", desc: "使用微信扫一扫授权", logo: wechatPlatformLogo, logoBg: "transparent", imgSize: 38 },
  ] as const;

  return (
    <VStack spacing={4} py={8} justify="center" minH="62vh">
      {/* 大包裹：左侧平台选择 + 右侧二维码 */}
      <LiquidGlassCard p={8} w="100%" maxW="980px" mx="auto">
        <HStack spacing={10} align="stretch" flexDir={{ base: "column", md: "row" }}>
          {/* 左侧：标题 + 登录平台（QQ / 微信 竖排） */}
          <VStack w={{ base: "100%", md: "280px" }} flexShrink={0} spacing={7} align={{ base: "center", md: "flex-start" }} justify="center">
            <VStack spacing={4} align={{ base: "center", md: "flex-start" }}>
              <Box bg={`${primaryColor}1a`} borderRadius="lg" p={3.5} border="1px solid" borderColor={`${primaryColor}44`}>
                <QrCode size={30} color={primaryColor} />
              </Box>
              <VStack spacing={1.5} align={{ base: "center", md: "flex-start" }}>
                <Heading size="md" color={textColor}>登录获取数据源</Heading>
                <Text color={subTextColor} fontSize="sm" textAlign={{ base: "center", md: "left" }} lineHeight="1.7">
                  扫码登录 WeGame 账号，自动同步战绩与藏品数据，登录凭证仅保存在本机。
                </Text>
              </VStack>
            </VStack>

            <VStack spacing={3} w="100%" align="stretch">
              {platforms.map((p) => {
                const active = method === p.id;
                return (
                  <Button
                    key={p.id}
                    variant="ghost"
                    h="66px"
                    px={3.5}
                    borderRadius="lg"
                    bg={active ? `${primaryColor}16` : "transparent"}
                    border="1px solid"
                    borderColor={active ? `${primaryColor}55` : borderColor}
                    _hover={{ bg: active ? `${primaryColor}24` : hoverBg }}
                    transition="all 0.18s"
                    onClick={() => switchMethod(p.id)}
                  >
                    <HStack spacing={3} w="100%">
                      <Box
                        w="38px"
                        h="38px"
                        borderRadius="md"
                        overflow="hidden"
                        bg={p.logoBg}
                        display="flex"
                        alignItems="center"
                        justifyContent="center"
                        flexShrink={0}
                      >
                        <img src={p.logo} alt={p.name} style={{ width: p.imgSize, height: p.imgSize, objectFit: "contain" }} />
                      </Box>
                      <VStack align="flex-start" spacing={0.5} flex={1}>
                        <Text fontSize="sm" fontWeight="700" color={active ? primaryColor : textColor}>{p.name}</Text>
                        <Text fontSize="xs" color={subTextColor}>{p.desc}</Text>
                      </VStack>
                      {active && <CheckCircle2 size={16} color={primaryColor} />}
                    </HStack>
                  </Button>
                );
              })}
            </VStack>
          </VStack>

          {/* 分隔线 */}
          <Box w="1px" bg={borderColor} alignSelf="stretch" display={{ base: "none", md: "block" }} />

          {/* 右侧：二维码（直接展示，随所选平台立即更新） */}
          <VStack flex={1} spacing={4} justify="center" minW={0}>
            <Box position="relative" p={3} borderRadius="lg" bg="white" border="1px solid" borderColor={cardBorder} boxShadow="sm">
              <QrCorners color={primaryColor} />
              {qr ? (
                <img
                  src={`data:${qrImgType};base64,${qr}`}
                  alt="登录二维码"
                  style={{
                    width: 236,
                    height: 236,
                    objectFit: "contain",
                    display: "block",
                    opacity: phase === "expired" ? 0.18 : 1,
                    transition: "opacity 0.3s",
                  }}
                />
              ) : (
                <Box
                  w="236px"
                  h="236px"
                  display="flex"
                  flexDir="column"
                  alignItems="center"
                  justifyContent="center"
                  gap={2}
                  cursor={generating ? "default" : "pointer"}
                  onClick={() => { if (!generating) void generate(method); }}
                >
                  {generating ? (
                    <Spinner size="md" color={primaryColor} />
                  ) : (
                    <>
                      <AlertCircle size={22} color="#ed8936" />
                      <Text fontSize="xs" fontWeight="600" color="#666666">生成失败，点击重试</Text>
                    </>
                  )}
                </Box>
              )}
              {phase === "expired" && (
                <Box
                  position="absolute"
                  top={0}
                  left={0}
                  right={0}
                  bottom={0}
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                  borderRadius="lg"
                  cursor="pointer"
                  bg="rgba(255,255,255,0.85)"
                  _hover={{ bg: "rgba(255,255,255,0.95)" }}
                  transition="background 0.2s"
                  onClick={() => void generate(method)}
                >
                  <VStack spacing={1.5}>
                    <RefreshCw size={26} color={primaryColor} />
                    <Text fontSize="xs" fontWeight="700" color="gray.600">二维码已失效，点击刷新</Text>
                  </VStack>
                </Box>
              )}
              {phase === "success" && (
                <Box position="absolute" top={-2} right={-2} bg="green.400" borderRadius="full" p={1} boxShadow="md">
                  <CheckCircle2 size={16} color="white" />
                </Box>
              )}
            </Box>

            {/* 状态提示 */}
            <HStack spacing={2} minH="20px">
              {phase === "scanned" ? (
                <CheckCircle2 size={15} color="#48bb78" />
              ) : phase === "success" ? (
                <>
                  <CheckCircle2 size={15} color="#48bb78" />
                  <Spinner size="xs" color={primaryColor} />
                </>
              ) : phase === "waiting" ? (
                <Spinner size="xs" color={primaryColor} />
              ) : (
                <AlertCircle size={15} color="#ed8936" />
              )}
              <Text fontSize="sm" fontWeight="600" color={statusOk ? "green.400" : phase === "expired" ? "orange.400" : subTextColor}>
                {statusText}
              </Text>
            </HStack>

            {error && <Text color="red.400" fontSize="sm" textAlign="center">{error}</Text>}
          </VStack>
        </HStack>
      </LiquidGlassCard>
    </VStack>
  );
}

// ═══ 头像地址解析：纯数字 ID 拼官方 CDN URL ═══
function resolveAvatarUrl(ic: string | undefined | null): string {
  if (!ic) return "";
  if (ic.startsWith("//")) return "https:" + ic;
  if (ic.startsWith("http")) return ic;
  if (/^[0-9]+$/.test(ic)) return "https://playerhub.df.qq.com/playerhub/60004/object/" + ic + ".png";
  return ic;
}

// ═══ 主题化分段标签组（顶部页签 / 烽火·战场模式切换通用）═══
function SegmentedTabs({
  items,
  value,
  onChange,
  size = "sm",
}: {
  items: { id: string; label: string; icon: React.ComponentType<{ size?: number; strokeWidth?: number }> }[];
  value: string;
  onChange: (id: string) => void;
  size?: "xs" | "sm";
}) {
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = getContrastTextColor();
  const textColor = useColorModeValue("gray.600", "#cccccc");
  const hoverBg = useColorModeValue("blackAlpha.50", "whiteAlpha.100");
  const trackBorder = useColorModeValue("gray.200", "#333333");

  return (
    <HStack
      spacing={1}
      p={1}
      borderRadius="lg"
      border="1px solid"
      borderColor={trackBorder}
      overflowX="auto"
      maxW="100%"
    >
      {items.map(({ id, label, icon: Icon }) => {
        const active = value === id;
        return (
          <Button
            key={id}
            size={size}
            borderRadius="md"
            flexShrink={0}
            leftIcon={<Icon size={size === "xs" ? 13 : 15} strokeWidth={2.2} />}
            bg={active ? primaryColor : "transparent"}
            color={active ? contrastText : textColor}
            fontWeight={active ? 700 : 500}
            _hover={{ bg: active ? primaryColor : hoverBg }}
            onClick={() => onChange(id)}
          >
            {label}
          </Button>
        );
      })}
    </HStack>
  );
}

const MODE_TABS = [
  { id: "sol", label: "烽火地带", icon: Flame },
  { id: "tdm", label: "全面战场", icon: Crosshair },
];

// ═══ 个人详情 ═══
function PersonalDetail({
  roleInfo,
  onSwitchMode,
  onSessionExpired,
}: {
  roleInfo: RoleInfo;
  onSwitchMode: (mode: "sol" | "tdm") => void;
  onSessionExpired?: () => void;
}) {
  const [mode, setMode] = useState<"sol" | "tdm">("sol");
  const { getActiveColor, getBorderColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const borderColor = getBorderColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  // 战绩数据：烽火地带 sol / 全面战场 mp
  const [solData, setSolData] = useState<Record<string, unknown> | null>(null);
  const [mpData, setMpData] = useState<Record<string, unknown> | null>(null);
  // 赛季筛选
  const [seasonList, setSeasonList] = useState<{ season_id: string; season_name: string }[]>([]);
  const [curSeason, setCurSeason] = useState("0");
  const [seasonLoading, setSeasonLoading] = useState(false);

  const loadSeasons = useCallback(async () => {
    const data = await invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "seasons" }).catch(() => null);
    const items = (data?.seasons as { season_id: string; season_name: string }[] | undefined) || [];
    // 过滤出 总览(0) + S1-S11
    const valid = items.filter((s) => s && /^(0|[1-9]|1[01])$/.test(String(s.season_id)));
    if (valid.length) setSeasonList(valid);
  }, []);

  const pickStats = useCallback((r: Record<string, unknown> | null): Record<string, unknown> | null => {
    // 优先取 season.sol / season.mp（含 total_fight/total_kill 等当季字段）
    const season = r?.season as Record<string, unknown> | undefined;
    const sol = season?.sol as Record<string, unknown> | null | undefined;
    const mp = season?.mp as Record<string, unknown> | null | undefined;
    if (sol && typeof sol === "object" && Object.keys(sol).length && Number(sol.total_fight ?? sol.total_price ?? 0) > 0) return sol;
    if (mp && typeof mp === "object" && Object.keys(mp).length && Number(mp.total_fight ?? mp.total_score ?? 0) > 0) return mp;
    if (season?.stats && typeof season.stats === "object") return season.stats as Record<string, unknown>;
    if (r?.stats && typeof r.stats === "object") return r.stats as Record<string, unknown>;
    return null;
  }, []);

  useEffect(() => {
    void loadSeasons();
  }, [loadSeasons]);

  // 切换赛季 → 拉对应赛季战报（后端返回 season.sol/mp 当季数据）
  const switchSeason = async (sid: string) => {
    setCurSeason(sid);
    setSeasonLoading(true);
    try {
      const [solResp, tdmResp] = await Promise.all([
        invoke<{ ok: boolean; season: { sol?: Record<string, unknown> | null; mp?: Record<string, unknown> | null; stats?: Record<string, unknown> | null } | null; session_valid?: boolean }>("df_stats_battle_report_season", { sid, queue: "sol" }),
        invoke<{ ok: boolean; season: { sol?: Record<string, unknown> | null; mp?: Record<string, unknown> | null; stats?: Record<string, unknown> | null } | null; session_valid?: boolean }>("df_stats_battle_report_season", { sid, queue: "tdm" }),
      ]).catch(() => [{ ok: false, season: null, session_valid: false }, { ok: false, season: null, session_valid: false }]);
      if (solResp.session_valid === false || tdmResp.session_valid === false) {
        onSessionExpired?.();
      }
      // 合并 season.sol + season.stats：sol 优先（当季字段），stats 补全 KD/排位/等级（总览权威值）
      const pick = (s: { sol?: Record<string, unknown> | null; mp?: Record<string, unknown> | null; stats?: Record<string, unknown> | null } | null): Record<string, unknown> | null => {
        if (!s) return null;
        const merged: Record<string, unknown> = {};
        if (s.stats && typeof s.stats === "object") Object.assign(merged, s.stats as Record<string, unknown>);
        if (s.sol && typeof s.sol === "object") Object.assign(merged, s.sol as Record<string, unknown>);
        if (s.mp && typeof s.mp === "object") Object.assign(merged, s.mp as Record<string, unknown>);
        if (Object.keys(merged).length) return merged;
        return null;
      };
      setSolData(pick(solResp.season));
      setMpData(pick(tdmResp.season));
    } catch {
      setSolData(null);
      setMpData(null);
    } finally {
      setSeasonLoading(false);
    }
  };

  // 初始加载赛季总览（sid="0"），保证首次进入就有数据
  useEffect(() => {
    if (seasonList.length) {
      void switchSeason("0");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seasonList]);

  const data = mode === "sol" ? solData : mpData;
  const num = (k: string): number => Number((data as Record<string, unknown>)?.[k] ?? 0);

  // 头像
  const avatarUrl = resolveAvatarUrl(roleInfo.icon as string | undefined);

  const switchMode = (m: "sol" | "tdm") => {
    setMode(m);
    onSwitchMode(m);
  };

  return (
    <VStack align="stretch" spacing={5}>
      {/* 角色头 */}
      <LiquidGlassCard p={5}>
        <HStack spacing={4}>
          <Box
            w="64px"
            h="64px"
            borderRadius="full"
            overflow="hidden"
            bg={`${primaryColor}22`}
            display="flex"
            alignItems="center"
            justifyContent="center"
            flexShrink={0}
            border="1px solid"
            borderColor={borderColor}
          >
            {avatarUrl ? (
              <img src={avatarUrl} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }}
                onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
            ) : (
              <UserRound size={30} color={primaryColor} />
            )}
          </Box>
          <VStack align="flex-start" spacing={1}>
            <HStack spacing={2}>
              <Text fontSize="xl" fontWeight="800" color={textColor}>
                {roleInfo.name || "未知干员"}
              </Text>
              <Badge
                fontSize="xs"
                borderRadius="md"
                px={2}
                bg={`${primaryColor}1a`}
                color={primaryColor}
                border="1px solid"
                borderColor={borderColor}
              >
                {mode === "sol" ? "烽火地带" : "全面战场"}
              </Badge>
            </HStack>
            <Text fontSize="sm" color={subTextColor}>
              {mode === "sol" ? `行动等级 ${roleInfo.level ?? "-"} / 排位 ${roleInfo.sol_level ?? "-"}` : `战场等级 ${roleInfo.tdmLevel ?? roleInfo.level ?? "-"}`}
            </Text>
          </VStack>
        </HStack>
      </LiquidGlassCard>

      {/* 模式切换 + 赛季选择框 */}
      <HStack spacing={3} wrap="wrap">
        <SegmentedTabs size="xs" items={MODE_TABS} value={mode} onChange={(id) => switchMode(id as "sol" | "tdm")} />
        {seasonList.length > 0 && (
          <CustomSelect
            value={curSeason}
            onChange={(v) => void switchSeason(v)}
            options={seasonList.map((s) => ({
              value: String(s.season_id),
              label: String(s.season_id) === "0" ? "赛季总览" : String(s.season_name || `S${s.season_id}`),
            }))}
            width="130px"
          />
        )}
      </HStack>

      {seasonLoading ? (
        <HStack justify="center" py={8}><Spinner size="md" color={primaryColor} /></HStack>
      ) : (
      /* 核心指标（完整战绩统计，当季字段） */
      <SimpleGrid columns={{ base: 2, md: 3 }} spacing={3}>
        {mode === "sol" ? (
          <>
            <StatCard label="总战斗场次" value={data ? formatNum(Number(num("total_fight") ?? num("solTotal") ?? 0)) : "-"} icon={Swords} color={primaryColor} />
            <StatCard label="撤离次数" value={data ? formatNum(Number(num("total_escape") ?? num("solTotalEscape") ?? 0)) : "-"} icon={Shield} color={primaryColor} />
            <StatCard label="撤离率" value={data ? formatRate(Number(num("escape_ratio") ?? num("solEscaperatio") ?? 0)) : "-"} icon={Flame} color={primaryColor} />
            <StatCard label="总击杀" value={data ? formatNum(Number(num("total_kill") ?? num("solTotalKill") ?? 0)) : "-"} icon={Skull} color={primaryColor} />
            <StatCard label="KD" value={data ? formatNum(Math.max(0, Number(num("solKdratio") ?? num("low_kill_death_ratio") ?? 0))) : "-"} icon={Target} color={primaryColor} />
            <StatCard label="带出价值" value={data ? formatNum(Number(num("total_gained_price") ?? num("total_price") ?? 0)) : "-"} icon={Wallet} color={primaryColor} />
            <StatCard label="排位分" value={data ? formatNum(Number(num("level_score") ?? num("rankpoint") ?? 0)) : "-"} icon={Trophy} color={primaryColor} />
            <StatCard label="行动时长" value={data ? formatTime(Number(num("total_game_time") ?? num("solDuration") ?? 0)) : "-"} icon={Gamepad2} color={primaryColor} />
            <StatCard label="行动等级" value={String(roleInfo.level ?? num("major_level") ?? num("ranklevel") ?? "-")} icon={Shield} color={primaryColor} />
          </>
        ) : (
          <>
            <StatCard label="总战斗场次" value={data ? formatNum(Number(num("total_fight") ?? num("tdmTotalFight") ?? 0)) : "-"} icon={Swords} color={primaryColor} />
            <StatCard label="获胜场次" value={data ? formatNum(Number(num("total_win") ?? num("tdmTotalWin") ?? 0)) : "-"} icon={Trophy} color={primaryColor} />
            <StatCard label="胜率" value={data ? formatRate(Number(num("win_ratio") ?? num("tdmSuccessRatio") ?? 0)) : "-"} icon={Target} color={primaryColor} />
            <StatCard label="总击杀" value={data ? formatNum(Number(num("total_kill") ?? num("tdmTotalKill") ?? 0)) : "-"} icon={Skull} color={primaryColor} />
            <StatCard label="KD" value={data ? formatNum(Math.max(0, Number(num("tdmKdRatio") ?? num("kill_death_ratio") ?? 0))) : "-"} icon={Crown} color={primaryColor} />
            <StatCard label="MVP" value={data ? formatNum(Number(num("total_mvp") ?? num("tdmTotalMVP") ?? 0)) : "-"} icon={BarChart3} color={primaryColor} />
            <StatCard label="排位分" value={data ? formatNum(Number(num("level_score") ?? num("tdmRankpoint") ?? 0)) : "-"} icon={Trophy} color={primaryColor} />
            <StatCard label="战斗时长" value={data ? formatTime(Number(num("total_game_time") ?? num("tdmDuration") ?? 0)) : "-"} icon={Gamepad2} color={primaryColor} />
            <StatCard label="战场等级" value={data ? formatNum(Number(num("major_level") ?? num("tdmRanklevel") ?? 0)) : String(roleInfo.level ?? "-")} icon={Shield} color={primaryColor} />
          </>
        )}
      </SimpleGrid>
      )}

      <Text fontSize="xs" color={subTextColor}>
        {String(curSeason) === "0" ? "当前为赛季总览数据；用上方选择框切换 S1-S11 查看对应赛季" : `当前为 ${seasonList.find((s) => String(s.season_id) === String(curSeason))?.season_name || `S${curSeason}`} 赛季数据`}
      </Text>
    </VStack>
  );
}

// ═══ 对局战绩：展示对局记录列表，可按干员 / 地图筛选 ═══
function BattleStats({ onSessionExpired }: { onSessionExpired?: () => void }) {
  const { getActiveColor, getHoverColor, getBorderColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const hoverColor = getHoverColor();
  const borderColor = getBorderColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  // 提升为组件级常量（避免在 map 回调内调用 useColorModeValue 造成 hook 顺序违规）
  const trackBorder = useColorModeValue("gray.200", "#333333");
  const imgBg = useColorModeValue("blackAlpha.50", "whiteAlpha.50");
  const rowBg = useColorModeValue("rgba(255,255,255,0.55)", "rgba(255,255,255,0.06)");
  const rowBorder = useColorModeValue("rgba(180,190,205,0.5)", "rgba(255,255,255,0.1)");
  const modalBorder = useColorModeValue("rgba(255,255,255,0.75)", "rgba(255,255,255,0.14)");
  const modalBg = useColorModeValue("rgba(250,250,252,0.82)", "rgba(22,26,33,0.88)");
  const [mode, setMode] = useState<"sol" | "tdm">("sol");
  const [records, setRecords] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(true);
  const [agentMap, setAgentMap] = useState<Map<number, AgentDictEntry>>(new Map());
  const [mapMap, setMapMap] = useState<Map<number, MapDetailEntry>>(new Map());
  const [filterAgent, setFilterAgent] = useState<string>("");
  const [filterMap, setFilterMap] = useState<string>("");
  // 对局详情弹层
  const [detail, setDetail] = useState<Record<string, unknown>[] | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailErr, setDetailErr] = useState("");
  const [detailMatch, setDetailMatch] = useState<Record<string, unknown> | null>(null);
  // 刷新状态（触发后端再采集时置 true，收到数据事件后复位 → 图标旋转动画）
  const [refreshing, setRefreshing] = useState(false);
  // 列表分页（每页 10 条）
  const [page, setPage] = useState(1);

  // 切换模式 / 筛选条件时回到第一页
  useEffect(() => { setPage(1); }, [mode, filterAgent, filterMap]);

  const resetRefresh = useCallback(() => setRefreshing(false), []);

  const handleRefresh = useCallback(() => {
    setRefreshing(true);
    void invoke("refresh_df_stats_collect");
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    const [solResp, tdmResp, { agentMap: am, mapMap: mm }] = await Promise.all([
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list_tdm" }),
      loadAgentMapDict(),
    ]).catch(() => [null, null, { agentMap: new Map(), mapMap: new Map() } as { agentMap: Map<number, AgentDictEntry>; mapMap: Map<number, MapDetailEntry> }]);
    setAgentMap(am);
    setMapMap(mm);
    const useSol = mode === "sol";
    const src = (useSol ? solResp : tdmResp) as Record<string, unknown> | null;
    const list = (src?.[useSol ? "sols" : "tdms"] as Record<string, unknown>[] | undefined) || [];
    setRecords(list);
    if (!src) void invoke("refresh_df_stats_collect");
    setLoading(false);
  }, [mode]);

  useEffect(() => {
    void load();
    let unlisten: (() => void) | undefined;
    void listen<[string, unknown]>("df-stats-data", () => { resetRefresh(); void load(); }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [load, resetRefresh]);

  // 干员 / 地图筛选（过滤空值与 0，避免「干员 0」这类无效筛选项）
  const agentIds = [...new Set(records.map((r) => String(r.armedForceId ?? "")))]
    .filter((id) => id && id !== "0")
    .sort();
  const mapIds = [...new Set(records.map((r) => String(r.mapId ?? "")))]
    .filter((id) => id && id !== "0")
    .sort();
  const shown = records.filter((r) =>
    (!filterAgent || String(r.armedForceId ?? "") === filterAgent) &&
    (!filterMap || String(r.mapId ?? "") === filterMap)
  );
  // 分页切片（每页 10 条）
  const PAGE_SIZE = 10;
  const totalPages = Math.max(1, Math.ceil(shown.length / PAGE_SIZE));
  const safePage = Math.min(Math.max(1, page), totalPages);
  const pageRecords = shown.slice((safePage - 1) * PAGE_SIZE, safePage * PAGE_SIZE);

  const gameTimeStr = (tsSec: string | number | null | undefined): string => {
    const n = Number(tsSec ?? 0);
    if (!n) return "";
    const d = new Date(n * 1000);
    const p = (x: number) => String(x).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };
  const durStr = (sec: number | string | null | undefined): string => {
    const s = Number(sec ?? 0);
    if (!s) return "-";
    const m = Math.floor(s / 60);
    return `${m}分${Math.floor(s % 60)}秒`;
  };

  // 点击对局行 → 拉取对局详情并展开
  const openDetail = async (r: Record<string, unknown>) => {
    setDetailMatch(r);
    setDetail(null);
    setDetailErr("");
    setDetailLoading(true);
    try {
      const res = await invoke<{ ok: boolean; players: Record<string, unknown>[]; session_valid?: boolean }>("df_stats_battle_detail", {
        roomId: String(r.roomId ?? ""),
        startTime: String(r.startTime ?? ""),
        queue: mode,
      });
      if (res.session_valid === false) {
        onSessionExpired?.();
        return;
      }
      setDetail(res.players || []);
      if (!res.ok && !res.players?.length) setDetailErr("暂无该对局详情");
    } catch (e) {
      setDetailErr(String(e));
    } finally {
      setDetailLoading(false);
    }
  };

  // 带出的高价值物品：优先取选中对局的 collections，其次从 players 中自己那行取
  const selfCols2 = useMemo(() => {
    if (detailMatch && Array.isArray(detailMatch.collections)) {
      return detailMatch.collections as Record<string, unknown>[];
    }
    if (detail) {
      const me = detail.find((p) => p.playerId === detailMatch?.playerId || p.roomId === detailMatch?.roomId && p.teamId === detailMatch?.teamId);
      if (me && Array.isArray(me.collections)) return me.collections as Record<string, unknown>[];
    }
    return [];
  }, [detail, detailMatch]);

  return (
    <VStack align="stretch" spacing={4}>
      <HStack spacing={3} justify="space-between" wrap="wrap">
        <SegmentedTabs size="xs" items={MODE_TABS} value={mode} onChange={(id) => setMode(id as "sol" | "tdm")} />
        <HStack spacing={2} wrap="wrap">
          <Text fontSize="xs" color={subTextColor}>共 {shown.length} 场</Text>
          <IconButton
            aria-label="refresh"
            size="xs"
            variant="ghost"
            icon={refreshing ? <Spinner size="sm" color={primaryColor} /> : <RefreshCw size={14} />}
            isDisabled={refreshing}
            onClick={handleRefresh}
            title={refreshing ? "采集刷新中…" : "刷新战绩"}
          />
        </HStack>
      </HStack>

      {/* 筛选：干员 / 地图 */}
      <HStack spacing={2} wrap="wrap">
        <HStack spacing={1} flexShrink={0}>
          <Filter size={13} color={subTextColor} />
          <Text fontSize="xs" color={subTextColor}>干员</Text>
        </HStack>
        <CustomSelect
          width="120px"
          value={filterAgent}
          onChange={setFilterAgent}
          options={[
            { value: "", label: "全部干员" },
            ...agentIds.map((id) => ({ value: id, label: getAgentName(id, agentMap) || `干员 ${id}` })),
          ]}
        />
        <HStack spacing={1} flexShrink={0}>
          <MapPin size={13} color={subTextColor} />
          <Text fontSize="xs" color={subTextColor}>地图</Text>
        </HStack>
        <CustomSelect
          width="120px"
          value={filterMap}
          onChange={setFilterMap}
          options={[
            { value: "", label: "全部地图" },
            ...mapIds.map((id) => ({ value: id, label: getMapName(id, mapMap) || `地图 ${id}` })),
          ]}
        />
      </HStack>

      {loading ? (
        <HStack justify="center" py={10}><Spinner size="md" color={primaryColor} /></HStack>
      ) : shown.length === 0 ? (
        <Text fontSize="sm" color={subTextColor} textAlign="center" py={10}>
          {filterAgent || filterMap ? "无符合条件的对局记录" : "暂无对局记录，请先登录数据源"}
        </Text>
      ) : (
        <VStack align="stretch" spacing={3}>
          {pageRecords.map((r, idx) => {
            const agentId = r.armedForceId as number | string | undefined;
            const mid = r.mapId as number | string | undefined;
            const agentName = getAgentName(agentId, agentMap);
            const agentAvatar = getAgentAvatar(agentId, agentMap);
            const mapName = getMapName(mid, mapMap) || "未知地图";
            const mapPic = getMapPic(mid, mapMap);
            const win = mode === "sol" ? Number(r.gameResult) === 0 : Number(r.isWinner) === 1;
            const profit = Number(r.ProfitLoss ?? 0);
            // 官方数据结构：killPlayer=击杀(真人) / killCnt=总击杀(含AI) / assistCnt=助攻 / rescue=救援；无阵亡字段
            const killPlayer = Number(r.killPlayer ?? 0);
            const killCnt = Number(r.killCnt ?? r.killNum ?? 0);
            const assist = Number(r.assistCnt ?? r.assist ?? 0);
            const rescue = Number(r.rescue ?? 0);
            return (
              <LiquidGlassCard key={idx} p={0} overflow="hidden">
                <Box
                  as="button"
                  w="100%"
                  textAlign="left"
                  p={3}
                  cursor="pointer"
                  onClick={() => void openDetail(r)}
                  _hover={{ bg: hoverColor }}
                  transition="background 0.15s"
                  style={{ background: "transparent", border: "none" }}
                >
                  <HStack spacing={3} align="center" wrap="wrap">
                    <Box borderRadius="md" overflow="hidden" bg={imgBg} h="52px" w="72px" flexShrink={0} display="flex" alignItems="center" justifyContent="center">
                      {mapPic ? (
                        <img src={mapPic} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }}
                          onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                      ) : <MapPin size={18} color={subTextColor} />}
                    </Box>
                    <VStack align="flex-start" spacing={0.5} flex={1}>
                      <HStack spacing={2}>
                        <Badge colorScheme={win ? "green" : "red"}>{mode === "sol" ? (win ? "撤离成功" : "撤离失败") : (win ? "胜利" : "失败")}</Badge>
                        <Text fontSize="sm" fontWeight="700" color={textColor} noOfLines={1}>{mapName}</Text>
                      </HStack>
                      <HStack spacing={2} fontSize="xs" color={subTextColor}>
                        {agentAvatar ? (
                          <img src={agentAvatar} alt="" style={{ width: 14, height: 14, borderRadius: "50%", objectFit: "cover" }}
                            onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                        ) : <UserRound size={12} />}
                        <Text>{agentName || "未知干员"}</Text>
                        <Clock size={11} />
                        <Text>{gameTimeStr(r.startTime as string)}</Text>
                      </HStack>
                    </VStack>
                    <HStack spacing={4} fontSize="sm" flexShrink={0}>
                      <VStack align="flex-start" spacing={0}>
                        <Text fontSize="xs" color={subTextColor}>击败干员 / 击败敌人 / 助攻</Text>
                        <Text fontWeight="700" color={textColor}>{killPlayer} / {killCnt} / {assist}</Text>
                      </VStack>
                      <VStack align="flex-start" spacing={0}>
                        <Text fontSize="xs" color={subTextColor}>时长</Text>
                        <Text fontWeight="600" color={textColor}>{durStr(r.gameTime as number)}</Text>
                      </VStack>
                      <VStack align="flex-start" spacing={0}>
                        <Text fontSize="xs" color={subTextColor}>收支</Text>
                        <Text fontWeight="800" color={profit >= 0 ? "green.400" : "red.400"}>
                          {profit >= 0 ? "+" : ""}{formatNum(profit)}
                        </Text>
                      </VStack>
                    </HStack>
                  </HStack>
                </Box>
              </LiquidGlassCard>
            );
          })}
        </VStack>
      )}

      {/* 分页控件 */}
      {!loading && totalPages > 1 && (
        <HStack justify="center" spacing={3} pt={1}>
          <IconButton
            aria-label="上一页"
            icon={<ChevronLeft size={16} />}
            size="xs"
            variant="ghost"
            color={textColor}
            isDisabled={safePage <= 1}
            onClick={() => setPage(safePage - 1)}
          />
          <Text fontSize="xs" color={subTextColor}>第 {safePage} / {totalPages} 页 · 共 {shown.length} 场</Text>
          <IconButton
            aria-label="下一页"
            icon={<ChevronRight size={16} />}
            size="xs"
            variant="ghost"
            color={textColor}
            isDisabled={safePage >= totalPages}
            onClick={() => setPage(safePage + 1)}
          />
        </HStack>
      )}

      {/* N 卡录屏式全屏遮罩预览：对局详情 */}
      {detailMatch && (
        <Box
          position="fixed"
          top={0}
          left={0}
          right={0}
          bottom={0}
          zIndex={9998}
          bg="rgba(0,0,0,0.6)"
          display="flex"
          alignItems="center"
          justifyContent="center"
          p={6}
          onClick={() => setDetailMatch(null)}
          sx={{ backdropFilter: "blur(14px)", WebkitBackdropFilter: "blur(14px)" }}
        >
          <Box
            w="100%"
            maxW="min(880px, 92vw)"
            maxH="90vh"
            overflowY="auto"
            borderRadius="2xl"
            border="1px solid"
            borderColor={modalBorder}
            bg={modalBg}
            boxShadow="2xl"
            onClick={(e) => e.stopPropagation()}
            sx={{ backdropFilter: "blur(22px) saturate(130%)", WebkitBackdropFilter: "blur(22px) saturate(130%)" }}
          >
            {/* 标题栏 */}
            <HStack justify="space-between" p={4} borderBottom="1px solid" borderColor={trackBorder}>
              <VStack spacing={0} align="start">
                <Text fontSize="lg" fontWeight="bold" color={textColor} noOfLines={1}>
                  {getMapName(detailMatch.mapId as number, mapMap) || "对局详情"}
                </Text>
                <Text fontSize="xs" color={subTextColor}>
                  {gameTimeStr(detailMatch.startTime as string)} · {mode === "sol" ? "烽火地带" : "全面战场"} · {durStr(detailMatch.gameTime as number)}
                </Text>
              </VStack>
              <IconButton aria-label="关闭" icon={<X size={20} />} size="sm" variant="ghost" color={textColor}
                _hover={{ bg: useColorModeValue("blackAlpha.100", "whiteAlpha.100") }}
                onClick={() => setDetailMatch(null)} />
            </HStack>

            <Box p={4}>
              {detailLoading ? (
                <HStack justify="center" py={8}><Spinner size="md" color={primaryColor} /></HStack>
              ) : detailErr ? (
                <Text fontSize="sm" color="red.300" textAlign="center" py={6}>{detailErr}</Text>
              ) : (
                <VStack align="stretch" spacing={4}>
                  {/* 带出的高价值物品 */}
                  <VStack align="stretch" spacing={1.5}>
                    <Text fontSize="sm" fontWeight="700" color={textColor}>带出的高价值物品（{selfCols2.length} 件）</Text>
                    {selfCols2.length > 0 ? (
                      <HStack spacing={2} wrap="wrap">
                        {selfCols2.map((c, ci) => (
                          <HStack key={ci} spacing={1.5} bg={rowBg} borderRadius="md" px={2} py={1} border="1px solid" borderColor={rowBorder}>
                            {typeof c.pic === "string" && (
                              <img src={c.pic} alt="" style={{ width: 28, height: 28, objectFit: "contain" }}
                                onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                            )}
                            <VStack spacing={0} align="flex-start">
                              <Text fontSize="xs" fontWeight="600" color={textColor} noOfLines={1}>{String(c.name ?? "物品")}</Text>
                              <Text fontSize="xs" color={primaryColor}>x{String(c.num ?? 1)} · {formatNum(c.price as number)}</Text>
                            </VStack>
                          </HStack>
                        ))}
                      </HStack>
                    ) : (
                      <Text fontSize="xs" color={subTextColor}>无带出物品</Text>
                    )}
                  </VStack>

                  {/* 局内玩家（含击败干员数） */}
                  <VStack align="stretch" spacing={1.5}>
                    <Text fontSize="sm" fontWeight="700" color={textColor}>对局玩家（{detail?.length ?? 0} 人）</Text>
                    <VStack align="stretch" spacing={1.5}>
                      {(detail || []).map((p, pi) => {
                        const pAgent = getAgentName(p.armedForceId as number, agentMap);
                        const pAvatar = getAgentAvatar(p.armedForceId as number, agentMap);
                        const pWin = mode === "sol" ? Number(p.gameResult) === 0 : Number(p.isWinner) === 1;
                        return (
                          <HStack key={pi} spacing={3} bg={rowBg} borderRadius="md" p={2.5} border="1px solid" borderColor={rowBorder}>
                            {pAvatar ? (
                              <img src={pAvatar} alt="" style={{ width: 28, height: 28, borderRadius: "50%", objectFit: "cover" }}
                                onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                            ) : <UserRound size={24} color={subTextColor} />}
                            <VStack align="flex-start" spacing={0} flex={1}>
                              <HStack spacing={2}>
                                <Text fontSize="sm" fontWeight="600" color={textColor} noOfLines={1}>{String(p.name ?? "未知玩家")}</Text>
                                {pAgent && <Badge fontSize="xs" borderRadius="md" bg={`${primaryColor}1a`} color={primaryColor}>{pAgent}</Badge>}
                              </HStack>
                              <Text fontSize="xs" color={subTextColor}>
                                {mode === "sol" ? (pWin ? "撤离成功" : "撤离失败") : (pWin ? "胜利" : "失败")} · 击败敌人 {String(p.killCnt ?? p.killNum ?? 0)} · 击败干员 {String(p.killPlayer ?? 0)}
                              </Text>
                            </VStack>
                            <Text fontSize="sm" fontWeight="700" color={Number(p.ProfitLoss ?? 0) >= 0 ? "green.400" : "red.400"}>
                              {Number(p.ProfitLoss ?? 0) >= 0 ? "+" : ""}{formatNum(p.ProfitLoss as number)}
                            </Text>
                          </HStack>
                        );
                      })}
                    </VStack>
                  </VStack>
                </VStack>
              )}
            </Box>
          </Box>
        </Box>
      )}
    </VStack>
  );
}

// ═══ 战绩分析：PPS / MMR / MES / 未来5局压力预测（独立页签）═══
function AnalyzerStats({ onSessionExpired }: { onSessionExpired?: () => void }) {
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const trackBg = useColorModeValue("blackAlpha.100", "whiteAlpha.100");
  const [mode, setMode] = useState<"sol" | "tdm">("sol");
  const [records, setRecords] = useState<Record<string, unknown>[]>([]);
  const [report, setReport] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(true);
  const [agentMap, setAgentMap] = useState<Map<number, AgentDictEntry>>(new Map());
  const [mapMap, setMapMap] = useState<Map<number, MapDetailEntry>>(new Map());

  const load = useCallback(async () => {
    setLoading(true);
    const [solResp, tdmResp, solReport, tdmReport, { agentMap: am, mapMap: mm }] = await Promise.all([
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list_tdm" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_report_sol" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_report_tdm" }),
      loadAgentMapDict(),
    ]).catch(() => [null, null, null, null, { agentMap: new Map(), mapMap: new Map() }] as [Record<string, unknown> | null, Record<string, unknown> | null, Record<string, unknown> | null, Record<string, unknown> | null, { agentMap: Map<number, AgentDictEntry>; mapMap: Map<number, MapDetailEntry> }]);
    const useSol = mode === "sol";
    const src = (useSol ? solResp : tdmResp) as Record<string, unknown> | null;
    const list = (src?.[useSol ? "sols" : "tdms"] as Record<string, unknown>[] | undefined) || [];
    setRecords(list);
    setReport(useSol ? solReport : tdmReport);
    setAgentMap(am);
    setMapMap(mm);
    if (!src && (!solResp || !tdmResp)) void invoke("refresh_df_stats_collect");
    setLoading(false);
  }, [mode]);

  useEffect(() => {
    void load();
    let unlisten: (() => void) | undefined;
    void listen<[string, unknown]>("df-stats-data", () => { void load(); }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [load]);

  const analysis = useMemo(() => {
    if (!records.length) return null;
    const total = records.length;
    const profs = records.map((r) => Number(r.ProfitLoss ?? 0));
    const totalProfit = profs.reduce((a, b) => a + b, 0);
    const totalGained = records.reduce((a, r) => a + Number(r.gainedPrice ?? r.gained_price ?? 0), 0);
    const totalGameTime = records.reduce((a, r) => a + Number(r.gameTime ?? 0), 0);
    const totalKill = records.reduce((a, r) => a + Number(r.killCnt ?? 0), 0);
    const totalDeath = records.reduce((a, r) => a + Number(r.death ?? 0), 0);
    const wins = records.filter((r) => (mode === "sol" ? Number(r.gameResult) === 0 : Number(r.isWinner) === 1)).length;

    const avgGain = totalGameTime ? totalGained / (totalGameTime / 60) : 0;
    const kd = totalDeath > 0 ? totalKill / totalDeath : totalKill;
    const winRate = total ? wins / total : 0;
    const profitPer = total ? totalProfit / total : 0;

    // 权威数据（官方战报）：KD / 撤离率 / 场次（比 battle_list 更准确）
    const season = report?.season as Record<string, unknown> | undefined;
    const stats = season?.stats as Record<string, unknown> | undefined;
    const sol = season?.sol as Record<string, unknown> | undefined;
    const mp = season?.mp as Record<string, unknown> | undefined;
    const isSol = mode === "sol";
    // 优先用 stats 字段（solKdratio / solEscaperatio），其次当季 sol/mp
    const officialKd = Number((isSol ? stats?.solKdratio : stats?.tdmKdRatio) ?? 0) || 0;
    const officialEscape = Number((isSol ? stats?.solEscaperatio : stats?.tdmSuccessRatio) ?? (isSol ? sol?.escape_ratio : mp?.win_ratio) ?? 0) || 0;
    const effKd = officialKd > 0 ? officialKd : (kd || 0);
    const effWinRate = officialEscape > 0 ? officialEscape : (isSol ? winRate : winRate);

    const ppsBase = Math.min(Math.max(avgGain / 100, 0), 12);
    // PPS 表现分：0-2000，体现每分钟得分 + KD + 撤离/胜率
    const pps = Math.round(Math.min(2000,
      340 * ppsBase + 300 * Math.min(effKd, 5) + 520 * effWinRate + 200 * Math.min(Math.max(profitPer / 100000, -2), 4)
    ));
    // MMR 实力分：0-3000，偏稳定实力（KD、撤离率权重更高）
    const mmr = Math.round(Math.min(3000,
      600 * Math.min(effKd, 5) + 850 * effWinRate + 280 * Math.min(ppsBase, 12) + 400 * Math.min(Math.max(profitPer / 500000, -2), 6)
    ));

    // MES 压力分：0-1000，近期 10 局盈亏波动
    const recent = profs.slice(Math.max(0, profs.length - 10));
    const absSum = recent.reduce((a, b) => a + Math.abs(b), 0);
    const mes = Math.round(Math.min(1000, 330 + (absSum / Math.max(recent.length, 1)) / 40000));

    const recentWins = records.slice(Math.max(0, records.length - 10));
    const recentWinRate = recentWins.length ? recentWins.filter((r) => (mode === "sol" ? Number(r.gameResult) === 0 : Number(r.isWinner) === 1)).length / recentWins.length : effWinRate;
    const recentProfit = recentWins.reduce((a, r) => a + Number(r.ProfitLoss ?? 0), 0);
    const trend = recentProfit >= 0 ? 1 : -1;
    const base = mes / 1000;

    const low = Math.max(0, Math.min(1, (1 - base) * (1.1 + 0.25 * trend) - (recentWinRate - 0.4) * 0.3));
    const normal = Math.max(0, Math.min(1, 0.62 - low * 0.5 + base * 0.1));
    const high = Math.max(0, Math.min(1, base * 0.65 + (recentWinRate < 0.35 ? 0.35 : 0)));
    const ultra = Math.max(0, Math.min(1, base * 0.3 + (recentWinRate < 0.25 ? 0.3 : 0) + (recentProfit < -3000000 ? 0.25 : 0)));
    const sumP = low + normal + high + ultra || 1;
    const pct = (v: number) => Math.round((v / sumP) * 100);

    return {
      pps, mmr, mes,
      low: pct(low), normal: pct(normal), high: pct(high), ultra: pct(ultra),
      total, winRate, kd, avgGain, avgProfit: profitPer,
    };
  }, [records, mode, report]);

  const pressureColor = (key: "low" | "normal" | "high" | "ultra"): string => {
    switch (key) {
      case "low": return "green.400";
      case "normal": return "blue.400";
      case "high": return "orange.400";
      case "ultra": return "red.400";
    }
  };

  // ── 六项偏好 / 频率 / 风格 ──
  const prefs = useMemo(() => {
    if (!records.length) return null;
    const total = records.length;
    // 地图偏好：各图场次数
    const mapCnt = new Map<string, number>();
    records.forEach((r) => {
      const mid = String(r.mapId ?? "");
      const name = getMapName(mid, mapMap) || `地图 ${mid}`;
      mapCnt.set(name, (mapCnt.get(name) || 0) + 1);
    });
    const topMap = [...mapCnt.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([n, c]) => ({ name: n, pct: Math.round((c / total) * 100) }));
    // 角色偏好：各干员使用次数
    const agentCnt = new Map<string, number>();
    records.forEach((r) => {
      const aid = String(r.armedForceId ?? "");
      const name = getAgentName(aid, agentMap) || `干员 ${aid}`;
      agentCnt.set(name, (agentCnt.get(name) || 0) + 1);
    });
    const topAgent = [...agentCnt.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3).map(([n, c]) => ({ name: n, pct: Math.round((c / total) * 100) }));
    // 装备偏好：按每局带入成本判定跑刀 / 普通 / 猛攻
    let runKnife = 0; // 跑刀：带入成本 < 62 万
    let fullAttack = 0; // 猛攻：带入成本 ≥ 250 万
    let midGear = 0; // 常规
    records.forEach((r) => {
      const equip = Number(r.originalEquipmentPriceWithoutKeyChain ?? r.originalEquipmentPrice ?? 0) || 0;
      if (equip < 620000) runKnife += 1;
      else if (equip >= 2500000) fullAttack += 1;
      else midGear += 1;
    });
    const gearTotal = Math.max(total, 1);
    const gearPcts = {
      runKnife: Math.round((runKnife / gearTotal) * 100),
      midGear: Math.round((midGear / gearTotal) * 100),
      fullAttack: Math.round((fullAttack / gearTotal) * 100),
    };
    // 战斗偏好：偏向哪种打法（以小博大 ≈ 低带入高产出；全装猛攻 ≈ 高带入直接刚）
    const avgEquip = records.reduce((a, r) => a + (Number(r.originalEquipmentPriceWithoutKeyChain ?? 0) || 0), 0) / gearTotal;
    const avgGain = records.reduce((a, r) => a + (Number(r.gainedPrice ?? 0) || 0), 0) / gearTotal;
    const smallToBig = avgEquip > 0 && avgGain / Math.max(avgEquip, 1) >= 3; // 产出≥3倍带入 → 以小博大
    const fightStyle = avgEquip >= 2500000 ? "全装猛攻" : (smallToBig ? "以小博大" : (avgEquip < 620000 ? "跑刀发育" : "均衡打法"));
    // 战斗偏好条：跑刀 / 均衡 / 猛攻 占比
    const combatPref = [
      { name: "跑刀", pct: gearPcts.runKnife },
      { name: "均衡", pct: gearPcts.midGear },
      { name: "猛攻", pct: gearPcts.fullAttack },
    ];
    // 战斗偏好：烽火 / 全面战场占比（从 battle_list + battle_list_tdm 计算，此处用当前 records + 模式标签）
    // 游戏频率：日均局数（按时间跨度）
    const times = records.map((r) => Number(r.startTime ?? 0)).filter(Boolean).sort((a, b) => a - b);
    let freqPerDay = 0;
    if (times.length >= 2) {
      const spanDays = Math.max((times[times.length - 1] - times[0]) / 86400, 0.1);
      freqPerDay = total / spanDays;
    } else {
      freqPerDay = total;
    }
    // 战斗风格：基于带入成本与产出比判定
    const totalKill = records.reduce((a, r) => a + Number(r.killCnt ?? 0), 0);
    const avgKill = total / Math.max(total, 1) > 0 ? totalKill / total : 0;
    const kd = analysis?.kd ?? 0;
    const style = fightStyle;
    return { topMap, topAgent, topEquip: [], freqPerDay, avgKill, style, total, gearPcts, avgEquip, combatPref };
  }, [records, mapMap, agentMap, analysis]);

  return (
    <VStack align="stretch" spacing={4}>
      <SegmentedTabs size="xs" items={MODE_TABS} value={mode} onChange={(id) => setMode(id as "sol" | "tdm")} />

      {loading ? (
        <HStack justify="center" py={10}><Spinner size="md" color={primaryColor} /></HStack>
      ) : !analysis ? (
        <Text fontSize="sm" color={subTextColor} textAlign="center" py={10}>暂无对局数据，请先登录数据源</Text>
      ) : (
        <>
        <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={4}>
          {/* PPS / MMR / MES */}
          <LiquidGlassCard p={3}>
            <VStack align="stretch" spacing={2}>
              <Text fontSize="sm" fontWeight="700" color={textColor}>综合评分</Text>
              <HStack justify="space-between">
                <Text fontSize="xs" color={subTextColor}>PPS 表现分</Text>
                <Text fontSize="xl" fontWeight="800" color={primaryColor}>{analysis.pps}</Text>
              </HStack>
              <HStack justify="space-between">
                <Text fontSize="xs" color={subTextColor}>MMR 实力分</Text>
                <Text fontSize="xl" fontWeight="800" color={textColor}>{analysis.mmr}</Text>
              </HStack>
              <HStack justify="space-between">
                <Text fontSize="xs" color={subTextColor}>MES 压力分</Text>
                <Text fontSize="xl" fontWeight="800" color={analysis.mes > 650 ? "red.400" : analysis.mes > 450 ? "orange.400" : "green.400"}>{analysis.mes}</Text>
              </HStack>
            </VStack>
          </LiquidGlassCard>

          {/* 未来5局压力预测 */}
          <LiquidGlassCard p={3}>
            <VStack align="stretch" spacing={2}>
              <Text fontSize="sm" fontWeight="700" color={textColor}>未来 5 局压力预测</Text>
              {([
                ["low", "低压"],
                ["normal", "普通"],
                ["high", "高压"],
                ["ultra", "特高压"],
              ] as const).map(([key, label]) => (
                <HStack key={key} spacing={2} fontSize="xs">
                  <Text color={subTextColor} w="44px" flexShrink={0}>{label}</Text>
                  <Box flex={1} h="6px" borderRadius="full" bg={trackBg} overflow="hidden">
                    <Box h="100%" w={`${analysis[key]}%`} bg={pressureColor(key)} borderRadius="full" />
                  </Box>
                  <Text color={textColor} w="36px" textAlign="right" fontWeight="600">{analysis[key]}%</Text>
                </HStack>
              ))}
            </VStack>
          </LiquidGlassCard>

          {/* 统计信息 */}
          <LiquidGlassCard p={3}>
            <VStack align="stretch" spacing={2}>
              <Text fontSize="sm" fontWeight="700" color={textColor}>基础统计</Text>
              <HStack justify="space-between"><Text fontSize="xs" color={subTextColor}>对局数</Text><Text fontSize="md" fontWeight="700" color={textColor}>{analysis.total}</Text></HStack>
              <HStack justify="space-between"><Text fontSize="xs" color={subTextColor}>胜率</Text><Text fontSize="md" fontWeight="700" color={textColor}>{(analysis.winRate * 100).toFixed(1)}%</Text></HStack>
              <HStack justify="space-between"><Text fontSize="xs" color={subTextColor}>KD</Text><Text fontSize="md" fontWeight="700" color={textColor}>{analysis.kd.toFixed(2)}</Text></HStack>
              <HStack justify="space-between"><Text fontSize="xs" color={subTextColor}>场均盈亏</Text><Text fontSize="md" fontWeight="700" color={analysis.avgProfit >= 0 ? "green.400" : "red.400"}>{analysis.avgProfit >= 0 ? "+" : ""}{formatNum(analysis.avgProfit)}</Text></HStack>
            </VStack>
          </LiquidGlassCard>
        </SimpleGrid>

        {/* 六项偏好 / 频率 / 风格 */}
        {prefs && (
          <SimpleGrid columns={{ base: 2, md: 3 }} spacing={4}>
            {/* 地图偏好 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>地图偏好</Text>
                {prefs.topMap.map((m, i) => (
                  <HStack key={i} spacing={2} fontSize="xs">
                    <Text color={subTextColor} w="70px" noOfLines={1}>{m.name}</Text>
                    <Box flex={1} h="6px" borderRadius="full" bg={trackBg} overflow="hidden">
                      <Box h="100%" w={`${m.pct}%`} bg={primaryColor} borderRadius="full" />
                    </Box>
                    <Text color={textColor} w="32px" textAlign="right" fontWeight="600">{m.pct}%</Text>
                  </HStack>
                ))}
              </VStack>
            </LiquidGlassCard>

            {/* 角色偏好 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>角色偏好</Text>
                {prefs.topAgent.map((a, i) => (
                  <HStack key={i} spacing={2} fontSize="xs">
                    <Text color={subTextColor} w="70px" noOfLines={1}>{a.name}</Text>
                    <Box flex={1} h="6px" borderRadius="full" bg={trackBg} overflow="hidden">
                      <Box h="100%" w={`${a.pct}%`} bg="blue.400" borderRadius="full" />
                    </Box>
                    <Text color={textColor} w="32px" textAlign="right" fontWeight="600">{a.pct}%</Text>
                  </HStack>
                ))}
              </VStack>
            </LiquidGlassCard>

            {/* 装备偏好 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>装备偏好</Text>
                {([
                  ["runKnife", "跑刀", "green.400"],
                  ["midGear", "常规", "blue.400"],
                  ["fullAttack", "猛攻", "red.400"],
                ] as const).map(([key, label, color]) => (
                  <HStack key={key} spacing={2} fontSize="xs">
                    <Text color={subTextColor} w="40px" flexShrink={0}>{label}</Text>
                    <Box flex={1} h="6px" borderRadius="full" bg={trackBg} overflow="hidden">
                      <Box h="100%" w={`${prefs.gearPcts[key]}%`} bg={color} borderRadius="full" />
                    </Box>
                    <Text color={textColor} w="32px" textAlign="right" fontWeight="600">{prefs.gearPcts[key]}%</Text>
                  </HStack>
                ))}
              </VStack>
            </LiquidGlassCard>

            {/* 战斗偏好 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>战斗偏好</Text>
                <Text fontSize="lg" fontWeight="800" color={primaryColor}>{prefs.style}</Text>
                {prefs.combatPref.map((c, i) => (
                  <HStack key={i} spacing={2} fontSize="xs">
                    <Text color={subTextColor} w="40px" flexShrink={0}>{c.name}</Text>
                    <Box flex={1} h="6px" borderRadius="full" bg={trackBg} overflow="hidden">
                      <Box h="100%" w={`${c.pct}%`} bg={c.name === "跑刀" ? "green.400" : c.name === "猛攻" ? "red.400" : "blue.400"} borderRadius="full" />
                    </Box>
                    <Text color={textColor} w="32px" textAlign="right" fontWeight="600">{c.pct}%</Text>
                  </HStack>
                ))}
              </VStack>
            </LiquidGlassCard>

            {/* 游戏频率 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>游戏频率</Text>
                <HStack justify="space-between" fontSize="xs">
                  <Text color={subTextColor}>日均局数</Text>
                  <Text color={textColor} fontWeight="700">{prefs.freqPerDay.toFixed(1)} 局/天</Text>
                </HStack>
                <HStack justify="space-between" fontSize="xs">
                  <Text color={subTextColor}>统计局数</Text>
                  <Text color={textColor} fontWeight="700">{prefs.total} 局</Text>
                </HStack>
              </VStack>
            </LiquidGlassCard>

            {/* 战斗风格 */}
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="700" color={textColor}>战斗风格</Text>
                <Text fontSize="lg" fontWeight="800" color={primaryColor}>{prefs.style}</Text>
                <Text fontSize="xs" color={subTextColor}>基于场均击杀与 KD 综合判定</Text>
              </VStack>
            </LiquidGlassCard>
          </SimpleGrid>
        )}
        </>
      )}
    </VStack>
  );
}

// ═══ 出货统计 ═══
function LootStats({ onSessionExpired }: { onSessionExpired?: () => void }) {
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const [daily, setDaily] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(true);
  // 地图-赛季统计
  const [solBattles, setSolBattles] = useState<Record<string, unknown>[]>([]);
  const [tdmBattles, setTdmBattles] = useState<Record<string, unknown>[]>([]);
  const [seasonList, setSeasonList] = useState<{ season_id: number; season_name: string; start_time?: string; end_time?: string }[]>([]);
  const [mapMap, setMapMap] = useState<Map<number, MapDetailEntry>>(new Map());
  const [curSeason, setCurSeason] = useState("0");
  // 是否用户手动切换过赛季（默认自动用最新赛季）
  const seasonTouchedRef = useRef(false);

  const load = useCallback(async () => {
    setLoading(true);
    const [data, solResp, tdmResp, seasonsRaw, { mapMap: mm }] = await Promise.all([
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "daily_stats" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list_tdm" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "seasons" }),
      loadAgentMapDict(),
    ]).catch(() => [null, null, null, null, { mapMap: new Map() }] as [Record<string, unknown> | null, Record<string, unknown> | null, Record<string, unknown> | null, Record<string, unknown> | null, { mapMap: Map<number, MapDetailEntry> }]);
    if (data) setDaily(data);
    else void invoke("refresh_df_stats_collect");
    setSolBattles((solResp?.sols as Record<string, unknown>[] | undefined) || []);
    setTdmBattles((tdmResp?.tdms as Record<string, unknown>[] | undefined) || []);
    setMapMap(mm);
    const items = (seasonsRaw?.seasons as { season_id: number; season_name: string; start_time?: string; end_time?: string }[] | undefined) || [];
    const valid = items.filter((s) => s && /^(0|[1-9]|1[01])$/.test(String(s.season_id)));
    if (valid.length) {
      setSeasonList(valid);
      // 默认选中最新赛季（最大 season_id），除非用户已手动选择过
      if (!seasonTouchedRef.current) {
        const latest = valid.filter((s) => String(s.season_id) !== "0").sort((a, b) => Number(b.season_id) - Number(a.season_id))[0];
        if (latest) setCurSeason(String(latest.season_id));
      }
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void load();
    let unlisten: (() => void) | undefined;
    void listen<[string, unknown]>("df-stats-data", () => { void load(); }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [load]);

  const assets = (daily?.daily_assets as unknown[] || []).map((v) => Number(v));
  const hafcoins = (daily?.daily_hafcoins as unknown[] || []).map((v) => Number(v));
  const values = (daily?.daily_values as unknown[] || []).map((v) => Number(v));

  // ── 资产盈亏曲线：固定最近31天窗口，每天一根柱 ──
  // 官方 GetDailyStats 的 daily_assets 是近31天滚动窗口，但资产值含「未游玩日」市场再估值波动；
  // 盈亏改用真实对局 ProfitLoss 按日聚合，无对局的日期盈亏记 0（显示灰色空柱），保证日期连续。
  const dailyBars = useMemo(() => {
    const len = assets.length;
    if (!len) return { profits: [] as number[], labels: [] as string[], endDate: "" };
    const all: Record<string, unknown>[] = [...solBattles, ...tdmBattles];
    const byDay = new Map<string, number>();
    for (const b of all) {
      const dts = String(b.dtEventTime ?? "");
      if (!/^\d{4}-\d{2}-\d{2}/.test(dts)) continue;
      const p = Number(b.ProfitLoss ?? 0);
      const day = dts.slice(0, 10);
      byDay.set(day, (byDay.get(day) ?? 0) + p);
    }
    const ref = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const ds = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    const profits: number[] = [];
    const labels: string[] = [];
    let lastPlay: string | null = null;
    for (let i = 0; i < len; i++) {
      const date = new Date(ref.getTime() - (len - 1 - i) * 86400000);
      const dayStr = ds(date);
      const p = byDay.get(dayStr) ?? 0;
      if (byDay.has(dayStr)) lastPlay = dayStr;
      profits.push(p);
      labels.push(`${date.getMonth() + 1}/${date.getDate()}`);
    }
    const endDate = lastPlay ? lastPlay.replace(/-/g, "/") : "";
    return { profits, labels, endDate };
  }, [assets, solBattles, tdmBattles]);
  const profits = dailyBars.profits;

  const latestAsset = assets.length > 0 ? assets[assets.length - 1] : 0;
  const latestHafcoin = hafcoins.length > 0 ? hafcoins[hafcoins.length - 1] : 0;
  const latestValue = values.length > 0 ? values[values.length - 1] : 0;
  // 排位分：daily_stats.rankpoints 数据源常为全 0，改从 battle_report 读真实 rankpoint
  const [rankPoint, setRankPoint] = useState(0);
  useEffect(() => {
    void Promise.all([
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_report_sol" }),
      invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_report_tdm" }),
    ]).then(([sr, tr]) => {
      const sSeason = sr?.season as Record<string, unknown> | undefined;
      const tSeason = tr?.season as Record<string, unknown> | undefined;
      const sStats = sSeason?.stats as Record<string, unknown> | undefined;
      const tStats = tSeason?.stats as Record<string, unknown> | undefined;
      const v = Number(sStats?.rankpoint ?? tStats?.tdmRankpoint ?? 0);
      setRankPoint(v);
    }).catch(() => {});
  }, []);
  const rankArr = (daily?.rankpoints as unknown[] | undefined)?.map((v) => Number(v)) || [];
  const latestRankRaw = rankArr.length > 0 ? rankArr[rankArr.length - 1] : 0;
  const latestRank = rankPoint > 0 ? rankPoint : latestRankRaw;
  const maxAbsProfit = Math.max(...profits.map((p) => Math.abs(p)), 1);

  // ── 按赛季统计各地图（官方 GetMapStats 接口，按 sid 返回真实赛季数据）──
  // 固定地图顺序（用户指定）：零号大坝 / 巴克什 / AZ3 / 航天基地 / 长弓溪谷 / 潮汐监狱
  const MAP_ORDER: { match: (id: string) => boolean; name: string }[] = [
    { match: (id) => /^(2201|2202|2211|2212|2231|2232|2233|2242|2251)$/.test(id), name: "零号大坝" },
    { match: (id) => /^(8102|8103|8151)$/.test(id), name: "巴克什" },
    { match: (id) => /^(8901|8902|8921|8922|8923)$/.test(id), name: "AZ3" },
    { match: (id) => /^(3901|3902|3951)$/.test(id), name: "航天基地" },
    { match: (id) => /^(1901|1902|1911|1912)$/.test(id), name: "长弓溪谷" },
    { match: (id) => /^(8802|8803)$/.test(id), name: "潮汐监狱" },
  ];
  // 官方接口返回的地图统计（按选中赛季）
  const [mapStats, setMapStats] = useState<Record<string, unknown>[] | null>(null);
  const [mapStatsLoading, setMapStatsLoading] = useState(false);
  const loadMapStats = useCallback(async (sid: string, queue: string) => {
    setMapStatsLoading(true);
    try {
      const res = await invoke<{ ok: boolean; maps: Record<string, unknown>[] | null; session_valid?: boolean }>("df_stats_map_stats", { sid, queue });
      if (res.session_valid === false) {
        onSessionExpired?.();
        setMapStats([]);
        return;
      }
      setMapStats(res.maps || []);
    } catch {
      setMapStats([]);
    } finally {
      setMapStatsLoading(false);
    }
  }, []);
  useEffect(() => {
    void loadMapStats(curSeason, "sol");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [curSeason]);

  // 将官方地图统计映射为表行（合并同地图不同模式）
  const mapSeasonStats = useMemo(() => {
    if (!mapStats) return [];
    const grouped = new Map<string, {
      name: string; total: number; win: number; kill: number; death: number; loss: number;
      totalValue: number;
    }>();
    for (const m of mapStats) {
      const mid = String(m.mapid ?? "");
      const fixedName = MAP_ORDER.find((x) => x.match(mid))?.name || getMapName(mid, mapMap) || `地图 ${mid}`;
      const row = grouped.get(fixedName) || { name: fixedName, total: 0, win: 0, kill: 0, death: 0, loss: 0, totalValue: 0 };
      row.total += Number(m.total ?? 0);
      row.win += Number(m.win ?? 0);
      row.kill += Number(m.kill ?? 0);
      row.death += Number(m.death ?? 0);
      row.loss += Math.max(0, Number(m.total ?? 0) - Number(m.win ?? 0));
      row.totalValue += Number(m.total_value ?? 0);
      grouped.set(fixedName, row);
    }
    const rows = [...grouped.values()].map((row) => {
      const winRate = row.total ? row.win / row.total : 0;
      const kda = row.death > 0 ? (row.kill + 0) / row.death : row.kill; // KDA
      const tradeRatio = row.loss > 0 ? row.kill / row.loss : row.kill; // 战损比 击杀/败场
      return {
        ...row,
        winRate,
        kda,
        tradeRatio,
        avgProfit: row.total ? row.totalValue / row.total : 0,
        profit: row.totalValue,
      };
    });
    // 固定顺序：按 MAP_ORDER 排
    const orderIdx = (name: string) => MAP_ORDER.findIndex((m) => m.name === name);
    rows.sort((a, b) => {
      const ia = orderIdx(a.name);
      const ib = orderIdx(b.name);
      if (ia >= 0 && ib >= 0) return ia - ib;
      if (ia >= 0) return -1;
      if (ib >= 0) return 1;
      return b.total - a.total;
    });
    return rows;
  }, [mapStats, mapMap]);

  const seasonName = seasonList.find((s) => String(s.season_id) === String(curSeason))?.season_name
    || (curSeason === "0" ? (seasonList.length ? "最新赛季" : "全部赛季") : `S${curSeason}`);

  return (
    <VStack align="stretch" spacing={5}>
      {loading ? (
        <HStack justify="center" py={10}><Spinner size="md" color={primaryColor} /></HStack>
      ) : !daily ? (
        <Text fontSize="sm" color={subTextColor} textAlign="center" py={10}>
          暂无出货数据，请先登录数据源
        </Text>
      ) : (
        <>
          <SimpleGrid columns={{ base: 2, md: 3 }} spacing={3}>
            <StatCard label="当前总资产" value={formatNum(latestAsset)} icon={Wallet} color={primaryColor} />
            <StatCard label="当前哈夫币" value={formatNum(latestHafcoin)} icon={Coins} color={primaryColor} />
            <StatCard label="当前装备价值" value={formatNum(latestValue)} icon={Package} color={primaryColor} />
            <StatCard label="资产峰值" value={formatNum(Math.max(...assets, 0))} icon={Crown} color={primaryColor} />
            <StatCard label="哈夫币峰值" value={formatNum(Math.max(...hafcoins, 0))} icon={Coins} color={primaryColor} />
            <StatCard label="排位分" value={formatNum(latestRank)} icon={Trophy} color={primaryColor} />
          </SimpleGrid>
          {profits.length > 1 && (
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={2}>
                <Text fontSize="sm" fontWeight="600" color={textColor}>
                  资产盈亏曲线{dailyBars.endDate ? `（按对局日聚合 · 截至 ${dailyBars.endDate}）` : `（近 ${profits.length} 日）`}
                </Text>
                {/* 所有柱统一从底部向上生长，盈利绿色 / 亏损红色 / 无对局日 0 为灰色矮柱 */}
                <Box position="relative" h="120px">
                  <HStack spacing={1} h="100%" align="flex-end">
                    {profits.map((p, idx) => {
                      const isZero = p === 0;
                      const isNeg = p < 0;
                      const barH = isZero ? 2 : Math.max(2, (Math.abs(p) / maxAbsProfit) * 100);
                      const label = dailyBars.labels[idx] ? `${dailyBars.labels[idx]}` : `第 ${idx + 1} 天`;
                      return (
                        <Tooltip
                          key={idx}
                          label={`${label}：${p >= 0 ? "+" : ""}${formatNum(p)} 哈夫币${isZero ? "（无对局）" : ""}`}
                          placement="top"
                          hasArrow
                        >
                          <Box
                            flex={1}
                            h={`${barH}%`}
                            bg={isZero ? "gray.300" : isNeg ? "red.400" : "green.400"}
                            borderRadius="sm"
                            cursor="pointer"
                            opacity={isZero ? 0.45 : 0.75}
                            _hover={{ opacity: 1 }}
                            transition="opacity 0.15s"
                          />
                        </Tooltip>
                      );
                    })}
                  </HStack>
                </Box>
                <HStack spacing={3} fontSize="xs" color={subTextColor}>
                  <HStack spacing={1}><Box boxSize={2} borderRadius="full" bg="green.400" /><Text>盈利</Text></HStack>
                  <HStack spacing={1}><Box boxSize={2} borderRadius="full" bg="red.400" /><Text>亏损</Text></HStack>
                  <HStack spacing={1}><Box boxSize={2} borderRadius="full" bg="gray.300" /><Text>无对局（0）</Text></HStack>
                  <Text>单位：哈夫币（每天取真实对局盈亏之和，无对局显示 0，鼠标悬停查看数值）</Text>
                </HStack>
              </VStack>
            </LiquidGlassCard>
          )}

          {/* 按赛季统计各地图：盈亏 / 撤离率 / KDA / 战损比 */}
          <LiquidGlassCard p={4}>
            <VStack align="stretch" spacing={3}>
              <HStack justify="space-between" wrap="wrap" spacing={2}>
                <Text fontSize="sm" fontWeight="600" color={textColor}>地图收益统计（{seasonName}）</Text>
                <CustomSelect
                  width="130px"
                  value={curSeason}
                  onChange={(v) => { seasonTouchedRef.current = true; setCurSeason(v); }}
                  options={seasonList.filter((s) => String(s.season_id) !== "0").map((s) => ({
                    value: String(s.season_id),
                    label: String(s.season_name || `S${s.season_id}`),
                  }))}
                />
              </HStack>
              {mapStatsLoading ? (
                <HStack justify="center" py={4}><Spinner size="sm" color={primaryColor} /></HStack>
              ) : mapSeasonStats.length === 0 ? (
                <Text fontSize="xs" color={subTextColor} textAlign="center" py={4}>该赛季暂无对局数据</Text>
              ) : (
                <Box overflowX="auto">
                  <Table size="sm" variant="simple">
                    <Thead>
                      <Tr>
                        <Th color={subTextColor} fontSize="xs">地图</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>场次</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>撤离率</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>KDA</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>战损比</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>场均盈亏</Th>
                        <Th color={subTextColor} fontSize="xs" isNumeric>总盈亏</Th>
                      </Tr>
                    </Thead>
                    <Tbody>
                      {mapSeasonStats.map((row, idx) => (
                        <Tr key={idx}>
                          <Td fontSize="xs" color={textColor} fontWeight="600">{row.name}</Td>
                          <Td fontSize="xs" color={subTextColor} isNumeric>{row.total}</Td>
                          <Td fontSize="xs" color={textColor} isNumeric>{(row.winRate * 100).toFixed(1)}%</Td>
                          <Td fontSize="xs" color={textColor} isNumeric>{row.kda.toFixed(2)}</Td>
                          <Td fontSize="xs" color={textColor} isNumeric>{row.tradeRatio.toFixed(2)}</Td>
                          <Td fontSize="xs" color={row.avgProfit >= 0 ? "green.400" : "red.400"} isNumeric>{row.avgProfit >= 0 ? "+" : ""}{formatNum(row.avgProfit)}</Td>
                          <Td fontSize="xs" color={row.profit >= 0 ? "green.400" : "red.400"} isNumeric>{row.profit >= 0 ? "+" : ""}{formatNum(row.profit)}</Td>
                        </Tr>
                      ))}
                    </Tbody>
                  </Table>
                </Box>
              )}
            </VStack>
          </LiquidGlassCard>
        </>
      )}
    </VStack>
  );
}

// ═══ 我的藏品（分类：武器 / 干员 / 载具 / 挂饰 / 典藏）═══
function Collections() {
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const { colorMode } = useColorMode();
  const isDark = colorMode === "dark";
  const previewDivider = useColorModeValue("rgba(160,170,190,0.4)", "rgba(255,255,255,0.1)");
  const modalBorder = useColorModeValue("rgba(255,255,255,0.75)", "rgba(255,255,255,0.14)");
  const modalBg = useColorModeValue("rgba(250,250,252,0.82)", "rgba(22,26,33,0.88)");
  const [items, setItems] = useState<Record<string, unknown>[]>([]);
  const [loading, setLoading] = useState(false);
  const [activeCat, setActiveCat] = useState("all");
  const [skinMap, setSkinMap] = useState<Map<number, SkinDictEntry>>(new Map());
  const [agentMap, setAgentMap] = useState<Map<number, AgentDictEntry>>(new Map());
  // 点击放大预览
  const [preview, setPreview] = useState<{ it: Record<string, unknown>; total: number; name: string; cat: string; img: string } | null>(null);

  // 等级 → 颜色映射（红色/橙色/紫色/蓝色/绿色），整卡渐变毛玻璃背景
  // 注意：本函数会在 map 回调与 useMemo 内调用，禁止内部使用 hook，深浅色由组件级 isDark 传入
  const gradeColor = (it: Record<string, unknown>): { border: string; cardBg: string; isDarkMode: boolean; text: string } => {
    const id = Number(it.id ?? it.gid ?? 0);
    const skin = skinMap.get(id);
    const grade = Number(skin?.grade ?? it.grade ?? it.quality ?? 0);
    // 官方网页卡片风格：整卡等级色渐变 + 毛玻璃质感
    if (grade >= 6) {
      // 红（红装：含红枪皮与红干员等，均保持红色显示；分类上典藏仅取枪皮）
      return { border: "rgba(229,62,62,0.7)", cardBg: isDark ? "linear-gradient(135deg, rgba(229,62,62,0.55), rgba(127,29,29,0.5))" : "linear-gradient(135deg, rgba(229,62,62,0.55), rgba(127,29,29,0.45))", isDarkMode: true, text: "#ffffff" };
    }
    if (grade >= 5) {
      // 橙
      return { border: "rgba(221,107,32,0.7)", cardBg: isDark ? "linear-gradient(135deg, rgba(221,107,32,0.55), rgba(124,45,18,0.5))" : "linear-gradient(135deg, rgba(221,107,32,0.55), rgba(124,45,18,0.45))", isDarkMode: true, text: "#ffffff" };
    }
    if (grade >= 4) {
      // 紫
      return { border: "rgba(128,90,213,0.7)", cardBg: isDark ? "linear-gradient(135deg, rgba(128,90,213,0.55), rgba(66,32,128,0.5))" : "linear-gradient(135deg, rgba(128,90,213,0.55), rgba(66,32,128,0.45))", isDarkMode: true, text: "#ffffff" };
    }
    if (grade >= 3) {
      // 蓝
      return { border: "rgba(66,153,225,0.7)", cardBg: isDark ? "linear-gradient(135deg, rgba(66,153,225,0.55), rgba(23,64,120,0.5))" : "linear-gradient(135deg, rgba(66,153,225,0.55), rgba(23,64,120,0.45))", isDarkMode: true, text: "#ffffff" };
    }
    if (grade >= 2) {
      // 绿
      return { border: "rgba(72,187,120,0.7)", cardBg: isDark ? "linear-gradient(135deg, rgba(72,187,120,0.55), rgba(20,100,60,0.5))" : "linear-gradient(135deg, rgba(72,187,120,0.55), rgba(20,100,60,0.45))", isDarkMode: true, text: "#ffffff" };
    }
    return { border: isDark ? "#444" : "gray.300", cardBg: isDark ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.4)", isDarkMode: false, text: subTextColor };
  };

  const loadDicts = useCallback(async () => {
    const { skinMap: sm, agentMap: am } = await loadAgentMapDict();
    setSkinMap(sm);
    setAgentMap(am);
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    const data = await invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "collectibles" }).catch(() => null);
    if (data) {
      const list: Record<string, unknown>[] = [];
      const col = data.collectibles as Record<string, unknown>[] | undefined;
      const ope = data.opers as Record<string, unknown>[] | undefined;
      if (Array.isArray(col)) list.push(...col);
      if (Array.isArray(ope)) list.push(...ope);
      setItems(list);
    } else {
      void invoke("refresh_df_stats_collect");
    }
    setLoading(false);
  }, []);

  useEffect(() => {
    void load();
    void loadDicts();
    let unlisten: (() => void) | undefined;
    void listen<[string, unknown]>("df-stats-data", () => { void load(); }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, [load, loadDicts]);

  // 分类：典藏(红装 grade>=6 的武器/近战枪皮) → 武器 → 载具 → 干员 → 挂饰
  const categorize = (it: Record<string, unknown>): string => {
    const id = Number(it.id ?? it.gid ?? 0);
    const skin = skinMap.get(id);
    const grade = Number(skin?.grade ?? it.grade ?? it.quality ?? 0);
    const second = String(skin?.secondClass ?? it.secondClass ?? it.type ?? "");
    // 典藏：红色红装（grade>=6）且为武器枪皮（隐藏款/联动等普通品质挂饰不算典藏）
    if (grade >= 6 && (second === "gun" || second === "dagger")) return "collection";
    if (second === "gun" || second === "dagger") return "weapon";
    if (second === "vehicle") return "vehicle";
    if (second === "operator") return "operator";
    if (second === "pendant") return "pendant";
    // 兜底：标明了 IsCollectibles/UniqueNo 的道具按 secondClass 初步归类
    if (String(it.secondClass ?? "") === "gun" || String(it.secondClass ?? "") === "dagger") return "weapon";
    if (String(it.secondClass ?? "") === "vehicle") return "vehicle";
    if (String(it.secondClass ?? "") === "operator") return "operator";
    if (String(it.secondClass ?? "") === "pendant") return "pendant";
    return "other";
  };

  const CATS: { key: string; label: string }[] = [
    { key: "all", label: "全部" },
    { key: "weapon", label: "武器" },
    { key: "operator", label: "干员" },
    { key: "vehicle", label: "载具" },
    { key: "pendant", label: "挂饰" },
    { key: "collection", label: "典藏" },
  ];

  const shown = activeCat === "all" ? items : items.filter((it) => categorize(it) === activeCat);

  // 合并同名同类藏品：紧急会议 ×30、紧急撤离 ×32 等挂饰（按 name+secondClass 聚合并累加 num）
  const merged = useMemo(() => {
    const map = new Map<string, { it: Record<string, unknown>; total: number }>();
    for (const it of shown) {
      const cat = categorize(it);
      const name = String(it.name ?? it.gun_name ?? it.item_name ?? it.objName ?? "藏品");
      const key = `${cat}|${name}`;
      const num = Number(it.num ?? 1) || 1;
      const prev = map.get(key);
      if (prev) prev.total += num;
      else map.set(key, { it, total: num });
    }
    return [...map.values()];
  }, [shown, categorize]);

  const imgOf = (it: Record<string, unknown>): string => {
    const keys = [
      "icon", "img", "image", "img_url", "icon_url", "pic_url", "url", "pic",
      "skin_img", "skin_image", "skinImg", "item_img", "goods_img", "avatar",
    ];
    for (const k of keys) {
      const v = it[k];
      if (typeof v === "string" && v.length > 4 && v.length < 400 && /^[a-zA-Z0-9\/\:\.\-]+$/.test(v) && (v.includes("/") || v.includes("."))) {
        return v.startsWith("http") || v.startsWith("//") ? v : "https://" + v;
      }
    }
    const id = String(it.id ?? it.gid ?? "");
    if (id && /^[0-9]{5,}$/.test(id)) {
      return "https://playerhub.df.qq.com/playerhub/60004/object/" + id + ".png";
    }
    return "";
  };

  // 全量分类计数：不随 activeCat 变化（用原始 items，不经过当前筛选）
  const allMerged = useMemo(() => {
    const map = new Map<string, { it: Record<string, unknown>; total: number }>();
    for (const it of items) {
      const cat = categorize(it);
      const name = String(it.name ?? it.gun_name ?? it.item_name ?? it.objName ?? "藏品");
      const key = `${cat}|${name}`;
      const num = Number(it.num ?? 1) || 1;
      const prev = map.get(key);
      if (prev) prev.total += num;
      else map.set(key, { it, total: num });
    }
    return [...map.values()];
  }, [items, categorize]);

  const countOf = (key: string) => {
    if (key === "all") return allMerged.length;
    return allMerged.filter((m) => categorize(m.it) === key).length;
  };

  return (
    <VStack align="stretch" spacing={4}>
      <HStack justify="space-between">
        <Text fontSize="sm" color={subTextColor}>我的藏品（{merged.length} 种）</Text>
        <IconButton aria-label="refresh" size="xs" variant="ghost" icon={<RefreshCw size={14} />}
          onClick={() => { void invoke("refresh_df_stats_collect"); }} />
      </HStack>

      {/* 分类标签 */}
      <HStack spacing={2} overflowX="auto" pb={1}>
        {CATS.map((c) => (
          <Button
            key={c.key}
            size="xs"
            variant="ghost"
            flexShrink={0}
            bg={activeCat === c.key ? primaryColor : "transparent"}
            color={activeCat === c.key ? "white" : subTextColor}
            onClick={() => setActiveCat(c.key)}
          >
            {c.label} ({countOf(c.key)})
          </Button>
        ))}
      </HStack>

      {loading ? (
        <HStack justify="center" py={8}><Spinner size="sm" color={primaryColor} /></HStack>
      ) : merged.length > 0 ? (
        <SimpleGrid columns={{ base: 2, md: 4 }} spacing={3}>
          {merged.map(({ it, total }, idx) => {
            const img = imgOf(it);
            const cat = categorize(it);
            const catLabel = CATS.find((c) => c.key === cat)?.label || (cat === "other" ? "其他" : "");
            const name = String(it.name ?? it.gun_name ?? it.item_name ?? it.objName ?? "藏品");
            const gc = gradeColor(it);
            return (
              <LiquidGlassCard
                key={idx}
                p={3}
                borderColor={gc.border}
                bg={gc.cardBg}
                cursor="pointer"
                onClick={() => setPreview({ it, total, name, cat: catLabel, img })}
              >
                <VStack spacing={1.5} align="stretch">
                  {img ? (
                    <Box h="72px" borderRadius="md" overflow="hidden" bg="rgba(0,0,0,0.18)" display="flex" alignItems="center" justifyContent="center" position="relative">
                      <img src={img.startsWith("//") ? "https:" + img : img} alt="" style={{ maxWidth: "100%", maxHeight: "100%", objectFit: "contain" }}
                        onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                      {total > 1 && (
                        <Badge position="absolute" top={1} right={1} fontSize="xs" borderRadius="full" bg="rgba(0,0,0,0.55)" color="#ffffff">
                          ×{total}
                        </Badge>
                      )}
                    </Box>
                  ) : (
                    <Box h="72px" borderRadius="md" bg="rgba(0,0,0,0.18)" display="flex" alignItems="center" justifyContent="center" position="relative">
                      <Package size={22} color={gc.isDarkMode ? "rgba(255,255,255,0.8)" : subTextColor} />
                      {total > 1 && (
                        <Badge position="absolute" top={1} right={1} fontSize="xs" borderRadius="full" bg="rgba(0,0,0,0.55)" color="#ffffff">
                          ×{total}
                        </Badge>
                      )}
                    </Box>
                  )}
                  <Text fontSize="sm" fontWeight="700" color={gc.isDarkMode ? "#ffffff" : textColor} noOfLines={1} textShadow={gc.isDarkMode ? "0 1px 2px rgba(0,0,0,0.4)" : undefined}>
                    {name}
                  </Text>
                  <Text fontSize="xs" color={gc.isDarkMode ? "rgba(255,255,255,0.9)" : gc.text} fontWeight={600} noOfLines={1}>
                    {catLabel}
                  </Text>
                </VStack>
              </LiquidGlassCard>
            );
          })}
        </SimpleGrid>
      ) : (
        <Text fontSize="sm" color={subTextColor} textAlign="center" py={8}>
          {activeCat === "all" ? "暂无藏品数据，请先登录数据源" : "该分类下暂无藏品"}
        </Text>
      )}

      {/* N 卡式全屏放大预览 */}
      {preview && (
        <Box
          position="fixed"
          top={0}
          left={0}
          right={0}
          bottom={0}
          zIndex={9999}
          bg="rgba(0,0,0,0.65)"
          display="flex"
          alignItems="center"
          justifyContent="center"
          p={6}
          onClick={() => setPreview(null)}
          sx={{ backdropFilter: "blur(14px)", WebkitBackdropFilter: "blur(14px)" }}
        >
          <Box
            w="100%"
            maxW="min(520px, 90vw)"
            borderRadius="2xl"
            border="1px solid"
            borderColor={modalBorder}
            bg={modalBg}
            boxShadow="2xl"
            onClick={(e) => e.stopPropagation()}
            sx={{ backdropFilter: "blur(22px) saturate(135%)", WebkitBackdropFilter: "blur(22px) saturate(135%)" }}
          >
            <HStack justify="space-between" p={4} borderBottom="1px solid" borderColor={previewDivider}>
              <VStack spacing={0} align="start">
                <Text fontSize="lg" fontWeight="bold" color={textColor} noOfLines={1}>{preview.name}</Text>
                <Text fontSize="xs" color={subTextColor} fontWeight={600}>{preview.cat}{preview.total > 1 ? ` · ×${preview.total}` : ""}</Text>
              </VStack>
              <IconButton aria-label="关闭" icon={<X size={20} />} size="sm" variant="ghost" color={textColor}
                _hover={{ bg: useColorModeValue("blackAlpha.100", "whiteAlpha.100") }}
                onClick={() => setPreview(null)} />
            </HStack>
            <Box p={6} display="flex" alignItems="center" justifyContent="center" minH="260px">
              {preview.img ? (
                <img
                  src={preview.img.startsWith("//") ? "https:" + preview.img : preview.img}
                  alt=""
                  style={{ maxWidth: "100%", maxHeight: "320px", objectFit: "contain" }}
                  onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
                />
              ) : (
                <Package size={80} color={subTextColor} />
              )}
            </Box>
          </Box>
        </Box>
      )}
    </VStack>
  );
}

// ═══ 主页面 ═══
export default function DeltaStatsPage() {
  const navigate = useNavigate();
  const { getActiveColor, getBorderColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const borderColor = getBorderColor();
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#888888");
  const toast = useToast();
  const adaptiveTitle = useAdaptiveTextColor();

  const [login, setLogin] = useState<DfLoginState | null>(null);
  const [loading, setLoading] = useState(true);
  const [roleInfo, setRoleInfo] = useState<RoleInfo | null>(null);
  const [activeTab, setActiveTab] = useState("personal");

  // 初始化：检查登录状态
  useEffect(() => {
    (async () => {
      try {
        const state = await invoke<DfLoginState | null>("check_df_stats_login");
        if (state?.logged_in) {
          setLogin(state);
          // 已登录但缺少新版字典/对局数据时，自动触发一次补采
          const [agents, maps, skins, listTdm, solRep] = await Promise.all([
            invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "agents" }).catch(() => null),
            invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "maps" }).catch(() => null),
            invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "skins" }).catch(() => null),
            invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_list_tdm" }).catch(() => null),
            invoke<Record<string, unknown> | null>("get_df_stats_cached_data", { kind: "battle_report_sol" }).catch(() => null),
          ]);
          const solErrCode = (solRep?.result as Record<string, unknown> | undefined)?.error_code;
          const solOk = !solRep || solErrCode === 0 || solErrCode === undefined;
          if (!agents || !maps || !skins || !listTdm || !solOk) {
            void invoke("refresh_df_stats_collect");
          }
        }
        const role = await invoke<RoleInfo | null>("get_df_stats_cached_data", { kind: "role_info" });
        if (role) setRoleInfo(role);
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  // 监听页面回传的登录/数据事件
  useEffect(() => {
    let unlistenLogin: (() => void) | undefined;
    let unlistenData: (() => void) | undefined;
    (async () => {
      unlistenLogin = await listen<DfLoginState>("df-stats-login-changed", (e) => {
        setLogin(e.payload);
        toast({
          title: "登录成功",
          description: "已获取战绩数据源，正在加载数据...",
          status: "success",
          duration: 2500,
          isClosable: true,
        });
      });
      unlistenData = await listen<[string, unknown]>("df-stats-data", (e) => {
        const [kind] = e.payload;
        if (kind === "role_info") {
          setRoleInfo(e.payload[1] as RoleInfo);
        }
      });
    })();
    return () => {
      unlistenLogin?.();
      unlistenData?.();
    };
  }, [toast]);

  // 菜单栏
  const menuItems = [
    { id: "personal", label: "个人详情", icon: UserRound },
    { id: "battle", label: "对局战绩", icon: BarChart3 },
    { id: "analysis", label: "战绩分析", icon: TrendingUp },
    { id: "loot", label: "出货统计", icon: Package },
    { id: "collection", label: "我的藏品", icon: Trophy },
  ];

  const handleLogout = async () => {
    try {
      await invoke("logout_df_stats");
      setLogin(null);
      setRoleInfo(null);
      toast({ title: "已退出登录", status: "info", duration: 2000, isClosable: true });
    } catch {
      toast({ title: "退出失败", status: "error", duration: 2000, isClosable: true });
    }
  };

  // 会话过期（令牌被吊销/过期）：清内存登录态，返回未登录页，引导重新扫码
  const handleSessionExpired = useCallback(() => {
    setLogin(null);
    setRoleInfo(null);
    toast({
      title: "登录已过期",
      description: "请重新扫码登录",
      status: "warning",
      duration: 3000,
      isClosable: true,
    });
  }, [toast]);

  const userAvatar = resolveAvatarUrl(login?.avatar);

  return (
    <Box position="relative" w="100%" minW={0}>
      {/* 顶部标题栏 */}
      <HStack justify="space-between" align="center" mb={5} wrap="wrap" spacing={3}>
        <HStack spacing={3} align="center">
          <Tooltip label="返回三角洲专区">
            <IconButton
              aria-label="返回三角洲专区"
              icon={<ArrowLeft size={20} />}
              variant="ghost"
              color={textColor}
              onClick={() => navigate("/delta-force")}
            />
          </Tooltip>
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>战绩分析</Heading>
          <Text fontSize="xs" color={subTextColor}>三角洲行动 · WeGame 数据</Text>
        </HStack>
        {login?.logged_in && (
          <HStack spacing={2}>
            <HStack
              spacing={2}
              px={2.5}
              py={1.5}
              borderRadius="lg"
              bg={`${primaryColor}14`}
              border="1px solid"
              borderColor={borderColor}
              maxW="220px"
            >
              <Box w="20px" h="20px" borderRadius="full" overflow="hidden" bg={`${primaryColor}22`} display="flex" alignItems="center" justifyContent="center" flexShrink={0}>
                {userAvatar ? (
                  <img src={userAvatar} alt="" style={{ width: "100%", height: "100%", objectFit: "cover" }}
                    onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
                ) : (
                  <UserRound size={12} color={primaryColor} />
                )}
              </Box>
              <Text fontSize="xs" fontWeight="600" color={textColor} noOfLines={1}>{login.nickname || "已登录"}</Text>
            </HStack>
            <Tooltip label="退出登录">
              <IconButton
                aria-label="退出登录"
                size="xs"
                variant="ghost"
                icon={<LogOut size={14} />}
                color={subTextColor}
                onClick={() => void handleLogout()}
              />
            </Tooltip>
          </HStack>
        )}
      </HStack>

      {loading ? (
        <HStack justify="center" py={20}><Spinner size="md" color={primaryColor} /></HStack>
      ) : !login?.logged_in ? (
        <DataSourceLogin
          onLogin={async () => {
            // 扫码成功：主动刷新一次登录态，让界面立即切到已登录视图
            try {
              const st = await invoke<DfLoginState | null>("check_df_stats_login");
              if (st?.logged_in) setLogin(st);
            } catch {
              // ignore，事件监听兜底
            }
          }}
        />
      ) : (
        <VStack align="stretch" spacing={5}>
          {/* 顶部页签 */}
          <SegmentedTabs items={menuItems} value={activeTab} onChange={setActiveTab} />

          {activeTab === "personal" && (
            <PersonalDetail
              roleInfo={roleInfo || ({ openid: "", area: "" } as RoleInfo)}
              onSwitchMode={() => {}}
              onSessionExpired={handleSessionExpired}
            />
          )}
          {activeTab === "battle" && <BattleStats onSessionExpired={handleSessionExpired} />}
          {activeTab === "analysis" && <AnalyzerStats onSessionExpired={handleSessionExpired} />}
          {activeTab === "loot" && <LootStats onSessionExpired={handleSessionExpired} />}
          {activeTab === "collection" && <Collections />}
        </VStack>
      )}
    </Box>
  );
}