import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Input,
  Button,
  Badge,
  Progress,
  IconButton,
  Tooltip,
  useColorModeValue,
  useToast,
} from "@chakra-ui/react";
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { join } from "@tauri-apps/api/path";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  DownloadCloud,
  Pause,
  Play,
  X,
  Link2,
  Gauge,
  CheckCircle2,
  AlertCircle,
  Loader2,
  RotateCcw,
  FileCheck2,
  FolderOpen,
} from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useThemeColor } from "@/contexts/theme-color-context";

interface SegmentDto {
  index: number;
  startByte: number;
  endByte: number;
  downloadedBytes: number;
}

interface AccelTask {
  id: string;
  url: string;
  fileName: string;
  saveDir: string;
  totalBytes: number;
  downloadedBytes: number;
  speedBps: number;
  status: number;
  errorMessage: string;
  segments: SegmentDto[];
  activeConns?: number;
}

const STATUS_PENDING = 0;
const STATUS_DOWNLOADING = 1;
const STATUS_PAUSED = 2;
const STATUS_COMPLETED = 3;
const STATUS_ERROR = 4;
const STATUS_PREPARING = 5;

function formatBytes(n: number): string {
  if (!n || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}`;
}

function formatSpeed(bps: number): string {
  if (!bps || bps <= 0) return "0 KB/s";
  return `${formatBytes(bps)}/s`;
}

function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "--";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  if (m > 0) return `${m}:${String(sec).padStart(2, "0")}`;
  return `0:${String(sec).padStart(2, "0")}`;
}

export default function DownloadAcceleratorPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useToast();
  const { getActiveColor, getHoverColor, getContrastTextColor } = useThemeColor();

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "gray.400");
  const trackBg = useColorModeValue("gray.100", "#222222");
  const pendingSegBg = useColorModeValue("gray.200", "#333333");

  const themeColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const [url, setUrl] = useState("");
  const [tasks, setTasks] = useState<Record<string, AccelTask>>({});
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState("");
  const [resetDone, setResetDone] = useState(false);
  const prevSegmentsRef = useRef<Record<string, SegmentDto[]>>({});

  // 终态外的空分段快照不覆盖已有布局（暂停/错误/重启扫描等场景）
  const upsertTask = useCallback((snap: AccelTask) => {
    setTasks((prev) => {
      const old = prev[snap.id];
      const segs =
        (!snap.segments || snap.segments.length === 0) &&
        old &&
        old.segments.length > 0 &&
        snap.status !== STATUS_COMPLETED
          ? old.segments
          : snap.segments;
      const enriched = { ...snap, segments: segs };

      const prevSegs = prevSegmentsRef.current[snap.id];
      let activeConns = snap.activeConns ?? 0;
      if (prevSegs && segs.length > 0) {
        activeConns = segs.filter((s) => {
          const p = prevSegs.find((ps) => ps.index === s.index);
          return !p || s.downloadedBytes > p.downloadedBytes;
        }).length;
      }
      if (segs.length > 0) {
        prevSegmentsRef.current[snap.id] = segs;
      }
      return { ...prev, [snap.id]: { ...enriched, activeConns } };
    });
  }, []);

  useEffect(() => {
    invoke<AccelTask[]>("accel_list")
      .then((list) => {
        setTasks((prev) => {
          const next = { ...prev };
          for (const item of list) if (!next[item.id]) next[item.id] = item;
          return next;
        });
      })
      .catch(() => {});

    invoke<AccelTask[]>("accel_scan_unfinished")
      .then((list) => {
        setTasks((prev) => {
          const next = { ...prev };
          for (const item of list) if (!next[item.id]) next[item.id] = item;
          return next;
        });
      })
      .catch(() => {});

    const unlistenProgress = listen<AccelTask>("accel-progress", (e) =>
      upsertTask(e.payload)
    );
    return () => {
      unlistenProgress.then((fn: UnlistenFn) => fn());
    };
  }, [upsertTask]);

  const handleStart = async () => {
    if (!url.trim() || starting) return;
    setStarting(true);
    setError("");
    try {
      const snap = await invoke<AccelTask>("accel_start", {
        url: url.trim(),
        fileName: null,
        saveDir: null,
        maxSegments: 0,
      });
      upsertTask(snap);
      setUrl("");
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  const handlePause = async (id: string) => {
    await invoke("accel_pause", { id }).catch(() => {});
  };

  const handleResume = async (id: string) => {
    await invoke("accel_resume", { id }).catch(() => {});
  };

  const handleCancel = async (id: string) => {
    await invoke("accel_cancel", { id }).catch(() => {});
    setTasks((prev) => {
      const next = { ...prev };
      delete next[id];
      delete prevSegmentsRef.current[id];
      return next;
    });
  };

  const statusBadge = (status: number) => {
    switch (status) {
      case STATUS_DOWNLOADING:
        return (
          <Badge colorScheme="green" borderRadius="full" px={2}>
            <HStack spacing={1}>
              <Loader2 size={11} className="spin" />
              <Text>{t("downloadAccel.status.downloading")}</Text>
            </HStack>
          </Badge>
        );
      case STATUS_COMPLETED:
        return (
          <Badge bg={themeColor} color={contrastText} borderRadius="full" px={2}>
            <HStack spacing={1}>
              <CheckCircle2 size={11} />
              <Text>{t("downloadAccel.status.completed")}</Text>
            </HStack>
          </Badge>
        );
      case STATUS_ERROR:
        return (
          <Badge colorScheme="red" borderRadius="full" px={2}>
            <HStack spacing={1}>
              <AlertCircle size={11} />
              <Text>{t("downloadAccel.status.error")}</Text>
            </HStack>
          </Badge>
        );
      case STATUS_PREPARING:
        return (
          <Badge colorScheme="blue" borderRadius="full" px={2}>
            {t("downloadAccel.status.preparing")}
          </Badge>
        );
      case STATUS_PAUSED:
        return (
          <Badge colorScheme="orange" borderRadius="full" px={2}>
            {t("downloadAccel.status.paused")}
          </Badge>
        );
      default:
        return null;
    }
  };

  const segmentBar = (task: AccelTask) => {
    const segs = task.segments.length
      ? task.segments
      : [
          {
            index: 0,
            startByte: 0,
            endByte: task.totalBytes - 1,
            downloadedBytes: task.downloadedBytes,
          },
        ];
    const totalSpan =
      task.totalBytes > 0 ? task.totalBytes : segs.reduce((a, s) => a + (s.endByte - s.startByte + 1), 0);

    return (
      <HStack spacing={0.5} w="100%" h={3}>
        {segs.map((seg) => {
          const span = seg.endByte - seg.startByte + 1;
          const pct = totalSpan > 0 ? (span / totalSpan) * 100 : 0;
          const fillPct = span > 0 ? Math.min(100, (seg.downloadedBytes / span) * 100) : 0;
          const done = fillPct >= 99.5;
          return (
            <Box
              key={seg.index}
              h="100%"
              w={`${pct}%`}
              minW={totalSpan > segs.length * 8192 ? undefined : "6px"}
              bg={pendingSegBg}
              borderRadius="sm"
              overflow="hidden"
            >
              <Box
                h="100%"
                w={`${fillPct}%`}
                bg={themeColor}
                opacity={done ? 1 : 0.65}
                transition="width 200ms linear"
              />
            </Box>
          );
        })}
      </HStack>
    );
  };

  const taskCard = (task: AccelTask) => {
    const pct =
      task.totalBytes > 0 ? Math.min(100, (task.downloadedBytes / task.totalBytes) * 100) : 0;
    const active = task.status === STATUS_DOWNLOADING;
    return (
      <VStack align="stretch" spacing={3}>
        <HStack justify="space-between">
          <HStack minW={0} flex={1}>
            <Gauge size={16} color={active ? themeColor : subTextColor} style={{ flexShrink: 0 }} />
            <Text
              fontWeight="semibold"
              fontSize="md"
              color={textColor}
              isTruncated
              maxW="420px"
              title={task.fileName}
            >
              {task.fileName}
            </Text>
            {statusBadge(task.status)}
          </HStack>
          <HStack spacing={1}>
            {(task.status === STATUS_DOWNLOADING ||
              task.status === STATUS_PREPARING ||
              task.status === STATUS_PENDING) && (
              <Tooltip label={t("downloadAccel.pause")} hasArrow>
                <IconButton
                  aria-label="pause"
                  icon={<Pause size={14} />}
                  size="xs"
                  variant="ghost"
                  onClick={() => handlePause(task.id)}
                />
              </Tooltip>
            )}
            {(task.status === STATUS_PAUSED || task.status === STATUS_ERROR) && (
              <Tooltip label={t("downloadAccel.resume")} hasArrow>
                <IconButton
                  aria-label="resume"
                  icon={<Play size={14} />}
                  size="xs"
                  variant="ghost"
                  colorScheme="green"
                  onClick={() => handleResume(task.id)}
                />
              </Tooltip>
            )}
            {task.status === STATUS_COMPLETED && (
              <>
                <Tooltip label={t("downloadAccel.openFile")} hasArrow>
                  <IconButton
                    aria-label="open file"
                    icon={<FileCheck2 size={14} />}
                    size="xs"
                    variant="ghost"
                    onClick={async () => {
                      try {
                        const p = await join(task.saveDir, task.fileName);
                        await invoke("accel_open_file", { path: p });
                      } catch (e) {
                        toast({
                          title: String(e),
                          status: "error",
                          duration: 2500,
                          isClosable: true,
                        });
                      }
                    }}
                  />
                </Tooltip>
                <Tooltip label={t("downloadAccel.openFolder")} hasArrow>
                  <IconButton
                    aria-label="open folder"
                    icon={<FolderOpen size={14} />}
                    size="xs"
                    variant="ghost"
                    onClick={async () => {
                      try {
                        const p = await join(task.saveDir, task.fileName);
                        await invoke("accel_reveal_file", { path: p });
                      } catch (e) {
                        toast({
                          title: String(e),
                          status: "error",
                          duration: 2500,
                          isClosable: true,
                        });
                      }
                    }}
                  />
                </Tooltip>
              </>
            )}
            <Tooltip label={t("downloadAccel.cancel")} hasArrow>
              <IconButton
                aria-label="cancel"
                icon={<X size={14} />}
                size="xs"
                variant="ghost"
                colorScheme="red"
                onClick={() => handleCancel(task.id)}
              />
            </Tooltip>
          </HStack>
        </HStack>

        {segmentBar(task)}

        <Progress
          value={pct}
          size="sm"
          borderRadius="full"
          hasStripe={active}
          sx={{
            "& > div": {
              background: `linear-gradient(90deg, ${getHoverColor()}, ${themeColor})`,
            },
          }}
          bg={trackBg}
        />

        <HStack justify="space-between">
          <HStack spacing={3}>
            <Text fontSize="xs" color={subTextColor}>
              {formatBytes(task.downloadedBytes)} / {task.totalBytes > 0 ? formatBytes(task.totalBytes) : "?"}
            </Text>
            {active && (
              <Text fontSize="xs" fontWeight="bold" color={themeColor}>
                {formatSpeed(task.speedBps)}
              </Text>
            )}
            {active && task.speedBps > 0 && task.totalBytes > task.downloadedBytes && (
              <Text fontSize="xs" color={subTextColor}>
                {t("downloadAccel.eta")} {formatEta((task.totalBytes - task.downloadedBytes) / task.speedBps)}
              </Text>
            )}
            {!!task.activeConns && active && (
              <Text fontSize="xs" fontWeight="semibold" color={themeColor}>
                {t("downloadAccel.activeConnections")} ≈{task.activeConns}
              </Text>
            )}
          </HStack>
          {task.errorMessage && (
            <Text fontSize="xs" color="red.400" isTruncated maxW="380px">
              {task.errorMessage}
            </Text>
          )}
        </HStack>
      </VStack>
    );
  };

  const taskList = Object.values(tasks).sort(
    (a, b) =>
      b.downloadedBytes / Math.max(1, b.totalBytes) -
      a.downloadedBytes / Math.max(1, a.totalBytes)
  );
  const dividerColor = useColorModeValue(
    "rgba(0,0,0,0.08)",
    "rgba(255,255,255,0.10)"
  );

  return (
    <VStack align="stretch" spacing={5} pb={10}>
      <HStack>
        <IconButton
          aria-label="back"
          icon={<ArrowLeft size={18} />}
          variant="ghost"
          onClick={() => navigate("/builtin-tools")}
        />
        <Heading size="lg" color={textColor}>
          {t("sidebar.downloadAccelerator")}
        </Heading>
        <DownloadCloud size={26} color={themeColor} style={{ marginLeft: 4 }} />
        <Tooltip label={t("downloadAccel.resetLearnedTip")} hasArrow>
          <Button
            ml="auto"
            size="xs"
            variant="ghost"
            color={subTextColor}
            leftIcon={<RotateCcw size={12} />}
            onClick={async () => {
              await invoke("accel_clear_learned").catch(() => {});
              setResetDone(true);
              setTimeout(() => setResetDone(false), 2000);
            }}
          >
            {resetDone ? t("downloadAccel.resetLearnedDone") : t("downloadAccel.resetLearned")}
          </Button>
        </Tooltip>
      </HStack>

      <LiquidGlassCard forceGlass>
        <Box p={5}>
          <HStack spacing={3}>
            <Input
              placeholder="https://example.com/file.zip"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              size="md"
              flex={1}
              borderColor="rgba(255,255,255,0.25)"
              _placeholder={{ color: subTextColor }}
              _focus={{ borderColor: themeColor, boxShadow: `0 0 0 1px ${themeColor}` }}
              onKeyDown={(e) => e.key === "Enter" && handleStart()}
            />
            <Button
              leftIcon={<Link2 size={15} />}
              bg={themeColor}
              color={contrastText}
              _hover={{ opacity: 0.85 }}
              isLoading={starting}
              onClick={handleStart}
              isDisabled={!url.trim()}
              px={6}
            >
              {t("downloadAccel.start")}
            </Button>
          </HStack>
          {error && (
            <Text fontSize="xs" color="red.400" mt={2}>
              {error}
            </Text>
          )}
        </Box>
      </LiquidGlassCard>

      {taskList.length > 0 && (
        <LiquidGlassCard forceGlass>
          <VStack align="stretch" spacing={0} p={4}>
            {taskList.map((task, i) => (
              <Box
                key={task.id}
                pt={i === 0 ? 0 : 4}
                pb={i === taskList.length - 1 ? 0 : 4}
                borderTop={i > 0 ? "1px solid" : undefined}
                borderColor={i > 0 ? dividerColor : undefined}
              >
                {taskCard(task)}
              </Box>
            ))}
          </VStack>
        </LiquidGlassCard>
      )}
    </VStack>
  );
}
