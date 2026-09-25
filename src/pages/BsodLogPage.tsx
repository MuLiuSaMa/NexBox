import { useState, useEffect, useCallback, useMemo } from "react";
import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  SimpleGrid,
  IconButton,
  Button,
  Badge,
  Spinner,
  Tooltip,
  Tabs,
  TabList,
  Tab,
  TabPanels,
  TabPanel,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  useColorModeValue,
  useClipboard,
  useToast,
  Collapse,
} from "@chakra-ui/react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  ScrollText,
  RefreshCw,
  FolderOpen,
  AlertTriangle,
  Info,
  Copy,
  ChevronDown,
  ChevronRight,
  Search as IconSearch,
} from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { hexToRgba } from "@/lib/color-utils";

// ---------------------------------------------------------------------------
// 与后端 BsodEvidence 保持一致的 TS 类型
// ---------------------------------------------------------------------------
interface SystemInfo {
  caption: string;
  version: string;
  build: string;
  arch: string;
  install_date: string;
  last_boot: string;
  mem_total: number;
  mem_free: number;
  cpu_name: string;
  cpu_cores: number;
  cpu_threads: number;
  machine_vendor: string;
  machine_model: string;
}
interface DumpConfig {
  crash_dump_enabled: number;
  dump_file: string;
  pagefile_alloc_mb: number;
  pagefile_current_mb: number;
  pagefile_name: string;
}
interface EventRecord {
  t: string;
  id: number;
  prov: string;
  level: string;
  task: string;
  msg: string;
  props: string[];
  group: string;
}
interface DumpInfo {
  path: string;
  file_size: number;
  mtime: string;
  format: string;
  bugcheck: number | null;
  bugcheck_hex: string;
  args: number[];
  exception_address: number | null;
  arch: string;
  os_build: string;
  processor_count: number;
  ok: boolean;
  error: string;
  is_live: boolean;
}
interface ArgExplain {
  value: number;
  hex: string;
  meaning: string;
  note: string;
}
interface CrashRecord {
  time: string;
  bugcheck: number | null;
  code_hex: string;
  name: string;
  bugcheck_cn: string;
  category: string;
  severity: string;
  args: number[];
  explains: ArgExplain[];
  causes: string[];
  fixes: string[];
  dump_path: string;
  dump_format: string;
  dump_size: number;
  dump_ok: boolean;
  sources: string[];
  related: EventRecord[];
  is_live: boolean;
  note: string;
  confidence: number;
}
interface Finding {
  severity: string;
  title: string;
  detail: string;
  evidence: string[];
  actions: string[];
  confidence: number;
  tag: string;
}
interface Summary {
  status: string;
  headline: string;
  sub: string;
  count_crashes: number;
  count_whea_events: number;
  count_suspects: number;
  count_dumps_ok: number;
}
interface BsodEvidence {
  system: SystemInfo;
  config: DumpConfig;
  dumps: DumpInfo[];
  events: EventRecord[];
  crashes: CrashRecord[];
  findings: Finding[];
  summary: Summary;
  errors: string[];
  generated_at: string;
}

// ---------------------------------------------------------------------------
// 工具函数
// ---------------------------------------------------------------------------
const SEV_COLORS: Record<string, string> = {
  critical: "red",
  high: "orange",
  medium: "yellow",
  low: "gray",
  info: "blue",
};
const SEV_FALLBACK = "gray";

function SevBadge({ severity }: { severity: string }) {
  const { t } = useTranslation();
  const cs = SEV_COLORS[severity] ?? SEV_FALLBACK;
  return (
    <Badge colorScheme={cs} px={2} py={0.5} borderRadius="md" fontSize="xs" textTransform="uppercase">
      {t(`bsodLog.severity.${severity}`, severity)}
    </Badge>
  );
}

function formatBytes(b: number): string {
  if (!b || b < 0) return "--";
  const gb = b / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(2)} GB`;
  const mb = b / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${(b / 1024).toFixed(1)} KB`;
}

// ---------------------------------------------------------------------------
// 概览卡片：单个 metric
// ---------------------------------------------------------------------------
function MetricCard({ value, label, color }: { value: string | number; label: string; color: string }) {
  const headingColor = useColorModeValue("black", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  return (
    <LiquidGlassCard p={4} textAlign="center">
      <Text fontSize="3xl" fontWeight="bold" color={color === "auto" ? headingColor : color}>
        {value}
      </Text>
      <Text fontSize="sm" color={subTextColor}>{label}</Text>
    </LiquidGlassCard>
  );
}

// ---------------------------------------------------------------------------
// 崩溃记录展开行
// ---------------------------------------------------------------------------
function CrashRow({ c }: { c: CrashRecord }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const dividerColor = useColorModeValue("gray.200", "#333333");
  const toast = useToast();
  const { onCopy: copyText } = useClipboard("");

  const doCopy = (text: string) => {
    navigator.clipboard?.writeText(text).then(() => {
      toast({ title: t("bsodLog.copied"), status: "success", duration: 1200, isClosable: true });
    });
    copyText(text);
  };

  return (
    <>
      <Tr cursor="pointer" onClick={() => setOpen(!open)}>
        <Td>{open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Td>
        <Td sx={{ whiteSpace: "nowrap" }} fontSize="sm">{c.time || "--"}</Td>
        <Td fontSize="sm" fontFamily="mono" color={headingColor}>{c.code_hex || "--"}</Td>
        <Td fontSize="sm" color={headingColor}>{c.name || t("bsodLog.unknownCode")}</Td>
        <Td fontSize="sm" color={subTextColor}>{c.bugcheck_cn || "--"}</Td>
        <Td fontSize="sm">{c.dump_ok ? t("bsodLog.dumpParsed") : c.dump_path ? t("bsodLog.dumpNotParsed") : t("bsodLog.noDump")}</Td>
        <Td><Text fontSize="sm">{Math.round(c.confidence * 100)}%</Text></Td>
        <Td><SevBadge severity={c.severity} /></Td>
      </Tr>
      {open && (
        <Tr>
          <Td colSpan={8} bg={useColorModeValue("gray.50", "#111111")}>
            <VStack align="stretch" spacing={3} py={2}>
              {c.sources.length > 0 && (
                <Box>
                  <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.sources")}</Text>
                  <HStack spacing={2} wrap="wrap">
                    {c.sources.map((s, i) => <Badge key={i} colorScheme="telegram" variant="subtle">{s}</Badge>)}
                  </HStack>
                </Box>
              )}
              {c.args.length > 0 && (
                <Box>
                  <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.argMeaning")}</Text>
                  <VStack align="stretch" spacing={1}>
                    {c.explains.map((e, i) => (
                      <HStack key={i} spacing={2} fontSize="sm">
                        <Text fontFamily="mono" color={headingColor} minW="180px">Arg{i + 1} = {e.hex}</Text>
                        <Text color={subTextColor}>{e.meaning}</Text>
                        {e.note && <Badge colorScheme="purple" fontSize="xs">{e.note}</Badge>}
                      </HStack>
                    ))}
                  </VStack>
                </Box>
              )}
              {c.causes.length > 0 && (
                <Box>
                  <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.causes")}</Text>
                  <VStack align="stretch" spacing={1}>
                    {c.causes.map((cs, i) => <Text key={i} fontSize="sm" color={headingColor}>· {cs}</Text>)}
                  </VStack>
                </Box>
              )}
              {c.fixes.length > 0 && (
                <Box>
                  <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.fixes")}</Text>
                  <VStack align="stretch" spacing={1}>
                    {c.fixes.map((fx, i) => (
                      <HStack key={i} justify="space-between" align="start">
                        <Text fontSize="sm" color={headingColor} flex={1}>· {fx}</Text>
                        <Tooltip label={t("bsodLog.copy")}>
                          <IconButton aria-label="copy" size="xs" variant="ghost" icon={<Copy size={12} />} onClick={() => doCopy(fx)} />
                        </Tooltip>
                      </HStack>
                    ))}
                  </VStack>
                </Box>
              )}
              {c.related.length > 0 && (
                <Box>
                  <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.relatedEvents")}</Text>
                  <VStack align="stretch" spacing={1}>
                    {c.related.slice(0, 8).map((e, i) => (
                      <Text key={i} fontSize="xs" color={subTextColor}>
                        {e.t} ｜ {e.prov} #{e.id} ｜ {e.msg}
                      </Text>
                    ))}
                  </VStack>
                </Box>
              )}
              {c.dump_path && (
                <Box borderTopWidth="1px" borderColor={dividerColor} pt={2}>
                  <Text fontSize="xs" color={subTextColor}>{t("bsodLog.dumpPath")}: {c.dump_path}</Text>
                </Box>
              )}
            </VStack>
          </Td>
        </Tr>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Finding 卡片
// ---------------------------------------------------------------------------
function FindingCard({ f, index }: { f: Finding; index: number }) {
  const { t } = useTranslation();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const subTextColor = useColorModeValue("gray.600", "#cccccc");
  const toast = useToast();
  const [open, setOpen] = useState(f.severity === "critical" || f.severity === "high");

  const copy = (text: string) => {
    navigator.clipboard?.writeText(text).then(() => {
      toast({ title: t("bsodLog.copied"), status: "success", duration: 1200, isClosable: true });
    });
  };

  return (
    <LiquidGlassCard p={5}>
      <VStack align="stretch" spacing={3}>
        <HStack justify="space-between" align="start">
          <HStack spacing={2} flex={1}>
            <Text fontSize="sm" fontWeight="bold" color={subTextColor}>#{index + 1}</Text>
            <SevBadge severity={f.severity} />
            {f.tag && <Badge colorScheme="purple" variant="subtle">{f.tag}</Badge>}
            <Text fontSize="md" fontWeight="semibold" color={headingColor} flex={1}>{f.title}</Text>
          </HStack>
          <HStack spacing={2}>
            <Badge colorScheme="gray">{Math.round(f.confidence * 100)}%</Badge>
            <IconButton aria-label="toggle" size="xs" variant="ghost"
              icon={open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              onClick={() => setOpen(!open)} />
          </HStack>
        </HStack>
        <Collapse in={open} animateOpacity>
          <VStack align="stretch" spacing={3} pl={1}>
            <Text fontSize="sm" color={subTextColor}>{f.detail}</Text>
            {f.evidence.length > 0 && (
              <Box>
                <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.evidence")}</Text>
                <VStack align="stretch" spacing={1}>
                  {f.evidence.map((e, i) => (
                    <HStack key={i} align="start" spacing={2}>
                      <Box as="span" color={subTextColor}>·</Box>
                      <Text fontSize="sm" color={headingColor} flex={1}>{e}</Text>
                      <Tooltip label={t("bsodLog.copy")}>
                        <IconButton aria-label="copy" size="xs" variant="ghost" icon={<Copy size={12} />} onClick={() => copy(e)} />
                      </Tooltip>
                    </HStack>
                  ))}
                </VStack>
              </Box>
            )}
            {f.actions.length > 0 && (
              <Box>
                <Text fontSize="xs" fontWeight="semibold" color={subTextColor} mb={1}>{t("bsodLog.actions")}</Text>
                <VStack align="stretch" spacing={1}>
                  {f.actions.map((a, i) => (
                    <HStack key={i} align="start" spacing={2}>
                      <Box as="span" color={subTextColor}>{i + 1}.</Box>
                      <Text fontSize="sm" color={headingColor} flex={1}>{a}</Text>
                      <Tooltip label={t("bsodLog.copy")}>
                        <IconButton aria-label="copy" size="xs" variant="ghost" icon={<Copy size={12} />} onClick={() => copy(a)} />
                      </Tooltip>
                    </HStack>
                  ))}
                </VStack>
              </Box>
            )}
          </VStack>
        </Collapse>
      </VStack>
    </LiquidGlassCard>
  );
}

// ============================================================================
// 主组件
// ============================================================================
export default function BsodLogPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { getActiveColor, config: themeConfig, getContrastTextColor } = useThemeColor();
  const themeColorHex = themeConfig.primaryColor;
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);
  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");

  const [data, setData] = useState<BsodEvidence | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [eventFilter, setEventFilter] = useState<string>("all");

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const resp = await invoke<BsodEvidence>("bsod_collect_evidence");
      setData(resp);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleOpenDumpDir = useCallback(async () => {
    try { await invoke<string>("bsod_open_dump_dir"); } catch (e) { setError(String(e)); }
  }, []);
  const handleReveal = useCallback(async (path: string) => {
    try { await invoke("bsod_reveal_dump", { path }); } catch (e) { setError(String(e)); }
  }, []);

  const filteredEvents = useMemo(() => {
    if (!data) return [];
    if (eventFilter === "all") return data.events;
    return data.events.filter((e) => e.group === eventFilter);
  }, [data, eventFilter]);

  return (
    <Box pt={8} pb={8}>
      <HStack justify="space-between" mb={6} flexWrap="wrap" gap={2}>
        <HStack>
          <IconButton aria-label={t("builtinTools.back")} icon={<ArrowLeft size={20} />}
            variant="ghost" onClick={() => navigate("/builtin-tools")} color={headingColor} />
          <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
            <HStack spacing={2}>
              <ScrollText size={24} />
              <Text>{t("bsodLog.title")}</Text>
            </HStack>
          </Heading>
        </HStack>
        <HStack spacing={2}>
          <Button leftIcon={<FolderOpen size={16} />} variant="outline" size="sm" onClick={handleOpenDumpDir}>
            {t("bsodLog.openDumpDir")}
          </Button>
          <Button leftIcon={<RefreshCw size={16} />} variant="outline" size="sm"
            onClick={fetchData} isLoading={loading} loadingText={t("bsodLog.collecting")}>
            {t("bsodLog.refresh")}
          </Button>
        </HStack>
      </HStack>

      {loading && !data && (
        <VStack py={20} spacing={4}>
          <Spinner size="xl" color={getActiveColor()} />
          <Text color={subTextColor}>{t("bsodLog.collecting")}</Text>
        </VStack>
      )}

      {error && (
        <LiquidGlassCard p={6} mb={6}>
          <VStack spacing={3}>
            <AlertTriangle size={32} color="red" />
            <Text color="red.400" fontWeight="medium">{t("bsodLog.loadError")}</Text>
            <Text color={subTextColor} fontSize="sm" textAlign="center">{error}</Text>
            <Button size="sm" onClick={fetchData}>{t("bsodLog.retry")}</Button>
          </VStack>
        </LiquidGlassCard>
      )}

      {data && (
        <VStack align="stretch" spacing={6}>
          {/* 顶部：Summary + 系统信息 */}
          <LiquidGlassCard p={5}>
            <VStack align="stretch" spacing={3}>
              <HStack justify="space-between" flexWrap="wrap" gap={2}>
                <HStack spacing={3}>
                  <Box color={data.summary.status === "alert" ? "red.400" : data.summary.status === "warn" ? "yellow.400" : "green.400"}>
                    {data.summary.status === "ok" ? <Info size={22} /> : <AlertTriangle size={22} />}
                  </Box>
                  <VStack align="start" spacing={0}>
                    <Text fontSize="lg" fontWeight="bold" color={headingColor}>{data.summary.headline}</Text>
                    <Text fontSize="sm" color={subTextColor}>{data.summary.sub}</Text>
                  </VStack>
                </HStack>
                <Text fontSize="xs" color={subTextColor}>{t("bsodLog.generatedAt")}: {data.generated_at}</Text>
              </HStack>
              <Box borderTopWidth="1px" borderColor={useColorModeValue("gray.200", "#333")} pt={3}>
                <SimpleGrid columns={{ base: 1, md: 2, lg: 4 }} spacing={2} fontSize="sm">
                  <Text color={subTextColor}>{t("bsodLog.os")}: <Text as="span" color={headingColor}>{data.system.caption} {data.system.build}</Text></Text>
                  <Text color={subTextColor}>{t("bsodLog.cpu")}: <Text as="span" color={headingColor}>{data.system.cpu_name} ({data.system.cpu_cores}C/{data.system.cpu_threads}T)</Text></Text>
                  <Text color={subTextColor}>{t("bsodLog.memory")}: <Text as="span" color={headingColor}>{(data.system.mem_total / 1024 / 1024 / 1024).toFixed(1)} GB</Text></Text>
                  <Text color={subTextColor}>{t("bsodLog.lastBoot")}: <Text as="span" color={headingColor}>{data.system.last_boot || "--"}</Text></Text>
                </SimpleGrid>
              </Box>
            </VStack>
          </LiquidGlassCard>

          <Tabs isFitted variant="soft-rounded" colorScheme="telegram">
            <TabList mb={4} gap={2} flexWrap="wrap">
              <BsodTab label={t("bsodLog.tabs.overview")} themeColorHex={themeColorHex} themeColorRgba={themeColorRgba} contrastColor={getContrastTextColor()} />
              <BsodTab label={`${t("bsodLog.tabs.crashes")} (${data.crashes.length})`} themeColorHex={themeColorHex} themeColorRgba={themeColorRgba} contrastColor={getContrastTextColor()} />
              <BsodTab label={`${t("bsodLog.tabs.findings")} (${data.findings.length})`} themeColorHex={themeColorHex} themeColorRgba={themeColorRgba} contrastColor={getContrastTextColor()} />
              <BsodTab label={`${t("bsodLog.tabs.events")} (${data.events.length})`} themeColorHex={themeColorHex} themeColorRgba={themeColorRgba} contrastColor={getContrastTextColor()} />
              <BsodTab label={`${t("bsodLog.tabs.dumps")} (${data.dumps.length})`} themeColorHex={themeColorHex} themeColorRgba={themeColorRgba} contrastColor={getContrastTextColor()} />
            </TabList>
            <TabPanels>
              {/* ---- 概览 ---- */}
              <TabPanel px={0}>
                <VStack align="stretch" spacing={5}>
                  <SimpleGrid columns={{ base: 2, md: 4 }} spacing={4}>
                    <MetricCard value={data.summary.count_crashes} label={t("bsodLog.metrics.crashes")} color={data.summary.count_crashes > 0 ? "red.400" : "auto"} />
                    <MetricCard value={data.summary.count_whea_events} label={t("bsodLog.metrics.wheaEvents")} color={data.summary.count_whea_events > 0 ? "orange.400" : "auto"} />
                    <MetricCard value={data.summary.count_suspects} label={t("bsodLog.metrics.suspects")} color="auto" />
                    <MetricCard value={data.summary.count_dumps_ok} label={t("bsodLog.metrics.dumps")} color="auto" />
                  </SimpleGrid>
                  {data.crashes[0] && (
                    <LiquidGlassCard p={5}>
                      <VStack align="stretch" spacing={2}>
                        <Text fontSize="sm" fontWeight="semibold" color={subTextColor}>{t("bsodLog.latestCrash")}</Text>
                        <HStack spacing={3} flexWrap="wrap">
                          <SevBadge severity={data.crashes[0].severity} />
                          <Text fontFamily="mono" fontSize="lg" color={headingColor}>{data.crashes[0].code_hex}</Text>
                          <Text fontSize="lg" fontWeight="bold" color={headingColor}>{data.crashes[0].name}</Text>
                        </HStack>
                        <Text fontSize="sm" color={subTextColor}>{data.crashes[0].bugcheck_cn}</Text>
                        <Text fontSize="xs" color={subTextColor}>{t("bsodLog.atTime")}: {data.crashes[0].time}</Text>
                      </VStack>
                    </LiquidGlassCard>
                  )}
                  {data.findings.slice(0, 3).map((f, i) => <FindingCard key={i} f={f} index={i} />)}
                </VStack>
              </TabPanel>

              {/* ---- 崩溃记录 ---- */}
              <TabPanel px={0}>
                {data.crashes.length === 0 ? (
                  <LiquidGlassCard p={10}>
                    <VStack spacing={3}>
                      <Info size={32} color={subTextColor} />
                      <Text color={subTextColor}>{t("bsodLog.empty.noCrash")}</Text>
                    </VStack>
                  </LiquidGlassCard>
                ) : (
                  <LiquidGlassCard p={0} overflow="hidden">
                    <Table size="sm" variant="ghost">
                      <Thead>
                        <Tr>
                          <Th width="30px" />
                          <Th>{t("bsodLog.col.time")}</Th>
                          <Th>{t("bsodLog.col.code")}</Th>
                          <Th>{t("bsodLog.col.name")}</Th>
                          <Th>{t("bsodLog.col.meaning")}</Th>
                          <Th>{t("bsodLog.col.dump")}</Th>
                          <Th>{t("bsodLog.col.confidence")}</Th>
                          <Th>{t("bsodLog.col.severity")}</Th>
                        </Tr>
                      </Thead>
                      <Tbody>
                        {data.crashes.map((c, i) => <CrashRow key={i} c={c} />)}
                      </Tbody>
                    </Table>
                  </LiquidGlassCard>
                )}
              </TabPanel>

              {/* ---- 原因分析 ---- */}
              <TabPanel px={0}>
                <VStack align="stretch" spacing={4}>
                  {data.findings.length === 0 ? (
                    <Text color={subTextColor}>{t("bsodLog.empty.noFindings")}</Text>
                  ) : (
                    data.findings.map((f, i) => <FindingCard key={i} f={f} index={i} />)
                  )}
                </VStack>
              </TabPanel>

              {/* ---- 事件日志 ---- */}
              <TabPanel px={0}>
                <VStack align="stretch" spacing={3}>
                  <HStack spacing={2} flexWrap="wrap">
                    {["all", "crash", "whea", "disk"].map((g) => (
                      <Button key={g} size="xs" variant={eventFilter === g ? "solid" : "ghost"}
                        colorScheme="telegram" onClick={() => setEventFilter(g)}>
                        {t(`bsodLog.eventGroups.${g}`)}
                      </Button>
                    ))}
                    <HStack spacing={1} ml="auto">
                      <IconSearch size={14} color={subTextColor} />
                      <Box as="input" borderWidth="1px" borderColor={useColorModeValue("gray.200", "#333")}
                        borderRadius="md" px={2} py={1} fontSize="xs" w="220px"
                        placeholder={t("bsodLog.searchPlaceholder")} />
                    </HStack>
                  </HStack>
                  <LiquidGlassCard p={0} overflow="hidden">
                    <Table size="sm" variant="ghost">
                      <Thead>
                        <Tr>
                          <Th>{t("bsodLog.col.time")}</Th>
                          <Th width="80px">ID</Th>
                          <Th>{t("bsodLog.col.provider")}</Th>
                          <Th>{t("bsodLog.col.level")}</Th>
                          <Th>{t("bsodLog.col.message")}</Th>
                        </Tr>
                      </Thead>
                      <Tbody>
                        {filteredEvents.slice(0, 500).map((e, i) => (
                          <Tr key={i}>
                            <Td fontSize="xs" sx={{ whiteSpace: "nowrap" }}>{e.t}</Td>
                            <Td fontSize="xs" fontFamily="mono">{e.id}</Td>
                            <Td fontSize="xs" maxW="220px" isTruncated>{e.prov}</Td>
                            <Td fontSize="xs">{e.level}</Td>
                            <Td fontSize="xs" color={subTextColor}>{e.msg}</Td>
                          </Tr>
                        ))}
                      </Tbody>
                    </Table>
                  </LiquidGlassCard>
                  {filteredEvents.length > 500 && (
                    <Text fontSize="xs" color={subTextColor} textAlign="center">
                      {t("bsodLog.eventsTruncated", { count: filteredEvents.length })}
                    </Text>
                  )}
                </VStack>
              </TabPanel>

              {/* ---- 转储文件 ---- */}
              <TabPanel px={0}>
                {data.dumps.length === 0 ? (
                  <LiquidGlassCard p={10}>
                    <VStack spacing={3}>
                      <Info size={32} color={subTextColor} />
                      <Text color={subTextColor}>{t("bsodLog.empty.noDumps")}</Text>
                    </VStack>
                  </LiquidGlassCard>
                ) : (
                  <LiquidGlassCard p={0} overflow="hidden">
                    <Table size="sm" variant="ghost">
                      <Thead>
                        <Tr>
                          <Th>{t("bsodLog.col.time")}</Th>
                          <Th>{t("bsodLog.col.path")}</Th>
                          <Th>{t("bsodLog.col.size")}</Th>
                          <Th>{t("bsodLog.col.format")}</Th>
                          <Th>{t("bsodLog.col.code")}</Th>
                          <Th>{t("bsodLog.col.parsed")}</Th>
                          <Th />
                        </Tr>
                      </Thead>
                      <Tbody>
                        {data.dumps.map((d, i) => (
                          <Tr key={i}>
                            <Td fontSize="xs" sx={{ whiteSpace: "nowrap" }}>{d.mtime}</Td>
                            <Td fontSize="xs" maxW="280px" isTruncated fontFamily="mono">{d.path}</Td>
                            <Td fontSize="xs">{formatBytes(d.file_size)}</Td>
                            <Td fontSize="xs"><Badge colorScheme="telegram">{d.format}</Badge></Td>
                            <Td fontSize="xs" fontFamily="mono">{d.bugcheck_hex || "--"}</Td>
                            <Td fontSize="xs">{d.ok ? t("bsodLog.yes") : d.error || t("bsodLog.no")}</Td>
                            <Td>
                              <Button size="xs" variant="ghost" leftIcon={<FolderOpen size={12} />}
                                onClick={() => handleReveal(d.path)}>{t("bsodLog.reveal")}</Button>
                            </Td>
                          </Tr>
                        ))}
                      </Tbody>
                    </Table>
                  </LiquidGlassCard>
                )}
              </TabPanel>
            </TabPanels>
          </Tabs>
        </VStack>
      )}
    </Box>
  );
}

function BsodTab({
  label,
  themeColorHex,
  themeColorRgba,
  contrastColor,
}: {
  label: string;
  themeColorHex: string;
  themeColorRgba: (opacity: number) => string;
  contrastColor: string;
}) {
  return (
    <Tab
      _selected={{
        bg: themeColorHex,
        color: contrastColor,
        boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
        // 悬停已选中的胶囊时保持实心主题色，避免被下面的半透明 hover 底色冲淡成「透明」
        _hover: { bg: themeColorHex },
      }}
      _hover={{ bg: themeColorRgba(0.15) }}
      borderRadius="full"
      fontWeight="600"
      fontSize="sm"
      px={5}
      py={1.5}
    >
      {label}
    </Tab>
  );
}
