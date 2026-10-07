import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  useColorModeValue,
  Button,
  Badge,
  Spinner,
  IconButton,
  Tabs,
  TabList,
  TabPanels,
  TabPanel,
  Tab,
  Input,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  useDisclosure,
  FormControl,
  FormLabel,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import {
  ArrowLeft,
  Volume2,
  Trash2,
  Download,
  Activity,
  Music,
  Gamepad2,
  Mic,
  Headphones,
  Play,
  Square,
  RefreshCw,
  CheckCircle,
  XCircle,
  AlertTriangle,
  Upload,
  Save,
} from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { CustomSelect } from "@/components/special/custom-select";
import { useThemeColor } from "@/contexts/theme-color-context";
import { store } from "@/lib/store";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { fitEngineGains, interpolateCurveToLayout } from "@/lib/eq-curve";

// ===== 类型定义 =====
interface DriverStatus {
  installed: boolean;
  service_exists: boolean;
  service_running: boolean;
  device_name: string;
  needs_reboot: boolean;
}

interface EqBand {
  freq: number;
  gain: number;
}

interface FxPresetParams {
  clarity: number;
  ambience: number;
  width: number;
  dynamics: number;
  bass: number;
}

interface EqPreset {
  id: string;
  name: string;
  bands: EqBand[];
  enabled: boolean;
  effects?: FxPresetParams;
}

interface EngineStatus {
  running: boolean;
  pid: number | null;
}

interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}

// ===== EQ 波段布局定义（5/10/15/20/31 波段，可切换） =====
interface BandLayoutDef {
  count: number;
  freqs: number[];              // 各频段默认中心频率
  ranges?: [number, number][];  // 频率旋钮可调范围（仅 5/10 波段提供）
  rangeLabels?: string[];       // 5 波段显示范围标签
}

const LAYOUT_5: BandLayoutDef = {
  count: 5,
  // 默认中心频率 = 各范围几何平均
  freqs: [88, 251, 1001, 4010, 11320],
  ranges: [[62, 124], [126, 500], [501, 2000], [2010, 8000], [8010, 16000]],
  rangeLabels: ["62-124", "126-500", "501-2k", "2.01k-8k", "8.01k-16k"],
};
const LAYOUT_10: BandLayoutDef = {
  count: 10,
  freqs: [31.25, 62.5, 125, 250, 500, 1000, 2000, 4000, 8000, 16000],
  // 每个频段的固定频率范围
  ranges: [
    [22, 41], [44, 81], [88, 162], [175, 325], [350, 650],
    [700, 1300], [1400, 2600], [2800, 5200], [5600, 10400], [11200, 20800],
  ],
};
const LAYOUT_15: BandLayoutDef = {
  count: 15,
  freqs: [25, 40, 63, 100, 160, 250, 400, 630, 1000, 1600, 2500, 4000, 6300, 10000, 16000],
};
const LAYOUT_20: BandLayoutDef = {
  count: 20,
  freqs: [20, 32, 40, 63, 80, 125, 160, 250, 315, 500, 630, 1000, 1200, 2000, 2500, 4000, 5000, 8000, 10000, 16000],
};
const LAYOUT_31: BandLayoutDef = {
  count: 31,
  freqs: [20, 25, 32, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1200, 1600, 2000, 2500, 3200, 4000, 5000, 6300, 8000, 10000, 12000, 16000, 20000],
};
const BAND_LAYOUTS: BandLayoutDef[] = [LAYOUT_5, LAYOUT_10, LAYOUT_15, LAYOUT_20, LAYOUT_31];

const EQ_STORE_SELECTED = "eq-selected-preset-id";
const EQ_STORE_BANDS = "eq-current-bands";
const EQ_STORE_IMPORTED = "eq-imported-preset-ids";
const EQ_STORE_SAVED = "eq-saved-preset-ids";
const EQ_STORE_CUSTOM_BANDS = "eq-custom-bands";
const EQ_STORE_PREAMP = "eq-preamp";
const EQ_STORE_FX = "eq-effects";
const EQ_STORE_DEVICE = "eq-device";
const EQ_STORE_LAYOUT = "eq-band-layout";

// ===== 辅助函数 =====
/** 将任意来源的 EQ 频段曲线映射到目标波段布局（对数轴线性插值，切换波段音色不变） */
function mapBandsToLayout(source: EqBand[], layout: BandLayoutDef): EqBand[] {
  return interpolateCurveToLayout(source, layout.freqs) as EqBand[];
}

/** 曲线 → 引擎增益：对曲线做最小二乘拟合，使实测频响与曲线一致 */
function engineBandsFromCurve(curve: EqBand[]): [number, number][] {
  const freqs = curve.map((b) => b.freq);
  const fitted = fitEngineGains(freqs, curve.map((b) => b.gain));
  return freqs.map((f, i) => [f, fitted[i]] as [number, number]);
}

function formatFreq(freq: number): string {
  if (freq >= 1000) {
    const k = freq / 1000;
    return `${k >= 10 ? k.toFixed(0) : k.toFixed(1)}k`;
  }
  return `${Math.round(freq)}`;
}

function getPresetIcon(name: string) {
  const lower = name.toLowerCase();
  if (lower.includes("人声") || lower.includes("voice") || lower.includes("vocal")) return Mic;
  if (lower.includes("低音") || lower.includes("bass") || lower.includes("哈曼")) return Headphones;
  if (lower.includes("音乐") || lower.includes("music")) return Music;
  if (lower.includes("竞技") || lower.includes("脚步") || lower.includes("枪声") || lower.includes("gam")) return Gamepad2;
  return Activity;
}

// ===== 折线：直接连接 10 个调节旋钮（无弧度） =====
function FrequencyResponseCurve({ bands }: { bands: EqBand[] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const fillColor = useColorModeValue("rgba(49,130,206,0.10)", "rgba(99,179,237,0.08)");
  const zeroColor = useColorModeValue("rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)");

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    const W = rect.width, H = rect.height;
    ctx.clearRect(0, 0, W, H);
    if (bands.length === 0) return;

    const padX = 4; // 匹配 HStack px={1} = 4px

    // 旋钮 Y 坐标：模仿滑动条布局
    // 增益文本 ~16px + gap 4px = 20px，120px 轨道，旋钮居轨道内
    const trackTop = 20; // 轨道顶部偏移
    const trackH = H - trackTop - 18; // 轨道可用高度（底部留 ~18px 给 freq 标签）
    const zeroY = trackTop + trackH / 2;

    // 10 个等距点，X 匹配 HStack 布局中心
    const pts = bands.map((b, i) => {
      const x = padX + (i + 0.5) / bands.length * (W - padX * 2);
      const y = trackTop + (1 - (b.gain + 12) / 24) * trackH;
      return { x, y };
    });

    // 填充（折线到底部再闭合）
    ctx.beginPath();
    ctx.moveTo(pts[0].x, zeroY);
    pts.forEach((p) => ctx.lineTo(p.x, p.y));
    ctx.lineTo(pts[pts.length - 1].x, zeroY);
    ctx.closePath();
    ctx.fillStyle = fillColor;
    ctx.fill();

    // 零线
    ctx.strokeStyle = zeroColor;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, zeroY);
    ctx.lineTo(W, zeroY);
    ctx.stroke();

    // 折线：直连各点
    ctx.strokeStyle = activeColor;
    ctx.lineWidth = 2;
    ctx.beginPath();
    pts.forEach((p, i) => (i === 0 ? ctx.moveTo(p.x, p.y) : ctx.lineTo(p.x, p.y)));
    ctx.stroke();

    // 在同位置画小圆点
    ctx.fillStyle = activeColor;
    pts.forEach((p) => {
      ctx.beginPath();
      ctx.arc(p.x, p.y, 3.5, 0, Math.PI * 2);
      ctx.fill();
    });
  }, [bands, activeColor, fillColor, zeroColor]);

  return <canvas ref={ref} style={{ width: "100%", height: "160px", display: "block", pointerEvents: "none" }} />;
}

// ===== 频率旋钮组件（可调节中心频率） =====
function FreqKnob({
  freq, minFreq, maxFreq, onChange
}: {
  freq: number; minFreq: number; maxFreq: number; onChange: (f: number) => void;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const bgColor = useColorModeValue("#ddd", "#444");

  const [isDragging, setIsDragging] = useState(false);

  const START = Math.PI * 0.75; // 135° bottom-left
  const RANGE = Math.PI * 1.5;  // 270°

  // 绘制
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    const W = rect.width, H = rect.height;
    ctx.clearRect(0, 0, W, H);
    const cx = W / 2, cy = H / 2, r = Math.min(W, H) / 2 - 3;

    const ratio = (freq - minFreq) / (maxFreq - minFreq);
    const endA = (START + RANGE) % (Math.PI * 2);
    const curA = (START + ratio * RANGE) % (Math.PI * 2);

    ctx.strokeStyle = bgColor;
    ctx.lineWidth = 4;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.arc(cx, cy, r, START, endA);
    ctx.stroke();

    ctx.strokeStyle = activeColor;
    ctx.beginPath();
    if (curA > START) {
      ctx.arc(cx, cy, r, START, curA);
    } else {
      ctx.arc(cx, cy, r, START, Math.PI * 2);
      ctx.arc(cx, cy, r, 0, curA);
    }
    ctx.stroke();

    const px = cx + r * Math.cos(curA);
    const py = cy + r * Math.sin(curA);
    ctx.fillStyle = activeColor;
    ctx.beginPath();
    ctx.arc(px, py, 3.5, 0, Math.PI * 2);
    ctx.fill();
  }, [freq, minFreq, maxFreq, activeColor, bgColor, START, RANGE]);

  // 鼠标在 arc 范围内计算频率，gap 区域不响应（避免绕回跳变）
  const getAngle = (clientX: number, clientY: number) => {
    const c = canvasRef.current;
    if (!c) return 0;
    const r = c.getBoundingClientRect();
    return Math.atan2(clientY - (r.top + r.height / 2), clientX - (r.left + r.width / 2));
  };

  useEffect(() => {
    if (!isDragging) return;
    const step = (maxFreq - minFreq) / 100;
    const onMove = (e: MouseEvent) => {
      let a = (getAngle(e.clientX, e.clientY) - START) % (Math.PI * 2);
      if (a < 0) a += Math.PI * 2;
      if (a > RANGE) return; // gap 区域：不更新，值保持不变
      const t = a / RANGE;
      const v = minFreq + t * (maxFreq - minFreq);
      onChange(Math.min(maxFreq, Math.max(minFreq, Math.round(v / step) * step)));
    };
    const onUp = () => setIsDragging(false);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [isDragging, minFreq, maxFreq, START, RANGE]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "100%", maxWidth: "36px", height: "36px", cursor: "pointer", display: "block", margin: "0 auto" }}
      onMouseDown={() => setIsDragging(true)}
      onWheel={(e) => {
        e.preventDefault();
        const step = (maxFreq - minFreq) / 100;
        const dir = e.deltaY > 0 ? -1 : 1;
        onChange(Math.min(maxFreq, Math.max(minFreq, freq + dir * step * 10)));
      }}
    />
  );
}

// ===== EQ 频段滑块组件 =====
function EqBandSlider({
  band,
  onChange,
  label,
  compact,
}: {
  band: EqBand;
  onChange: (gain: number) => void;
  label?: string;
  compact?: boolean;
}) {
  const labelColor = useColorModeValue("gray.600", "#ffffff");
  const valueColor = useColorModeValue("gray.800", "#e0e0e0");
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();

  const gain = band.gain;
  const gainPercent = ((gain + 12) / 24) * 100;
  const sTrackBg = useColorModeValue("gray.200", "#333333");
  const sZeroBg = useColorModeValue("gray.400", "#555555");

  return (
    <VStack spacing={1} align="center" w="full" minW={compact ? 0 : undefined}>
      {/* 增益值 */}
      <Text
        fontSize={compact ? "10px" : "xs"}
        fontWeight="bold"
        color={gain > 0 ? "green.500" : gain < 0 ? "orange.500" : valueColor}
        minH="16px"
        whiteSpace="nowrap"
      >
        {gain > 0 ? "+" : ""}{gain.toFixed(0)}
      </Text>

      {/* 垂直滑块 */}
      <Box position="relative" h="120px" w={compact ? "full" : "32px"} minW={compact ? 0 : undefined} display="flex" justifyContent="center">
        {/* 滑轨 */}
        <Box
          position="absolute"
          left="50%"
          transform="translateX(-50%)"
          w="6px"
          h="full"
          borderRadius="full"
          bg={sTrackBg}
        />
        {/* 0dB 基准线 */}
        <Box
          position="absolute"
          left="50%"
          transform="translateX(-50%)"
          top="50%"
          w={compact ? "12px" : "16px"}
          h="2px"
          bg={sZeroBg}
          borderRadius="full"
        />
        {/* 填充 */}
        <Box
          position="absolute"
          left="50%"
          transform="translateX(-50%)"
          w="6px"
          borderRadius="full"
          bg={activeColor}
          top={gain >= 0 ? `${100 - gainPercent}%` : "50%"}
          height={`${Math.abs(gainPercent - 50)}%`}
          opacity={0.6}
        />
        {/* 滑块手柄（实心） */}
        <Box
          position="absolute"
          left="50%"
          transform="translate(-50%, -50%)"
          top={`${100 - gainPercent}%`}
          w={compact ? "12px" : "16px"}
          h={compact ? "12px" : "16px"}
          borderRadius="full"
          bg={activeColor}
          cursor="pointer"
          _active={{ transform: "translate(-50%, -50%) scale(0.85)" }}
        />
        {/* 隐藏的 input range */}
        <input
          type="range"
          min={-12}
          max={12}
          step={1}
          value={gain}
          onChange={(e) => onChange(parseFloat(e.target.value))}
          style={{
            position: "absolute",
            width: "120px",
            height: compact ? "20px" : "32px",
            top: "50%",
            left: "50%",
            transform: "translate(-50%, -50%) rotate(-90deg)",
            opacity: 0,
            cursor: "pointer",
          }}
        />
      </Box>

      {/* 频率标签 */}
      <Text fontSize={compact ? "10px" : "xs"} color={labelColor} fontWeight="medium" whiteSpace="nowrap">
        {label ?? formatFreq(band.freq)}
      </Text>
    </VStack>
  );
}

// ===== 主页面组件 =====
export default function AudioEqPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("audio");

  const headingColor = useColorModeValue("#1A202C", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const descColor = useColorModeValue("gray.500", "#ffffff");
  const presetIconColor = useColorModeValue("#4A5568", "#aaaaaa");
  const iconBg = useColorModeValue("gray.100", "#222222");
  const warningBg = useColorModeValue("orange.50", "rgba(237, 137, 54, 0.1)");
  const warningColor = useColorModeValue("orange.600", "orange.300");
  const switcherBg = useColorModeValue("gray.100", "#2a2a2a");
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();

  const [driverStatus, setDriverStatus] = useState<DriverStatus | null>(null);
  const [engineStatus, setEngineStatus] = useState<EngineStatus>({ running: false, pid: null });
  const [presets, setPresets] = useState<EqPreset[]>([]);
  const [selectedPresetId, setSelectedPresetId] = useState<string>("");
  const [bandLayout, setBandLayout] = useState<BandLayoutDef>(LAYOUT_10);
  const [bands, setBands] = useState<EqBand[]>(
    LAYOUT_10.freqs.map((f) => ({ freq: f, gain: 0 }))
  );
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [uninstalling, setUninstalling] = useState(false);
  const [startingEngine, setStartingEngine] = useState(false);
  const [tabIndex, setTabIndex] = useState(0);
  const [importing, setImporting] = useState(false);
  const [importedIds, setImportedIds] = useState<string[]>([]);
  const [savedIds, setSavedIds] = useState<string[]>([]);
  const [presetName, setPresetName] = useState("");
  const [preamp, setPreamp] = useState(0);
  const defaultFx = { clarity: 0, ambience: 0, width: 0, dynamics: 0, bass: 0 };
  const [fx, setFx] = useState(defaultFx);
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [selectedDevice, setSelectedDevice] = useState("");

  const { isOpen: saveModalOpen, onOpen: onSaveModalOpen, onClose: onSaveModalClose } = useDisclosure();
  const CUSTOM_PRESET_ID = "__custom__";
  const selectedPresetIdRef = useRef<string>("");
  const lastEngineSyncRef = useRef(0); // 后端同步节流时间戳
  const fileInputRef = useRef<HTMLInputElement>(null);

  const builtInPresets = presets.filter((p) => !importedIds.includes(p.id) && !savedIds.includes(p.id));
  const savedPresets = presets.filter((p) => savedIds.includes(p.id));
  const importedPresets = presets.filter((p) => importedIds.includes(p.id));

  // 加载数据
  const loadData = useCallback(async () => {
    try {
      setLoading(true);
      const [status, engine, presetList, savedId, savedBands, savedImported, savedUser, savedPreamp, savedFx, devList, savedDevice, savedLayout] = await Promise.all([
        invoke<DriverStatus>("check_virtual_audio_driver"),
        invoke<EngineStatus>("get_eq_engine_status"),
        invoke<EqPreset[]>("get_eq_presets"),
        store.get<string>(EQ_STORE_SELECTED),
        store.get<EqBand[]>(EQ_STORE_BANDS),
        store.get<string[]>(EQ_STORE_IMPORTED),
        store.get<string[]>(EQ_STORE_SAVED),
        store.get<number>(EQ_STORE_PREAMP),
        store.get<typeof defaultFx>(EQ_STORE_FX),
        invoke<AudioDevice[]>("list_audio_devices"),
        store.get<string>(EQ_STORE_DEVICE),
        store.get<number>(EQ_STORE_LAYOUT),
      ]);
      // 恢复波段布局（缺省 10 波段）
      const layout = BAND_LAYOUTS.find((l) => l.count === savedLayout) ?? LAYOUT_10;
      setBandLayout(layout);
      setDriverStatus(status);
      setEngineStatus(engine);
      setPresets(presetList);
      setImportedIds(savedImported ?? []);
      setSavedIds(savedUser ?? []);
      setPreamp(savedPreamp ?? 0);
      setFx(savedFx ?? defaultFx);
      setDevices(devList ?? []);
      // 恢复已保存的设备选择；若已保存设备仍存在则沿用，否则回退到默认设备
      const savedDevName = savedDevice?.trim();
      if (savedDevName && (devList ?? []).some((d) => d.name === savedDevName)) {
        setSelectedDevice(savedDevName);
      } else {
        const def = (devList ?? []).find((d) => d.is_default);
        setSelectedDevice(def?.name ?? "");
      }

      if (presetList.length === 0) {
        // 无预设：恢复上次频段或布局默认值
        setBands(mapBandsToLayout(savedBands ?? [], layout));
        return;
      }

      // 恢复上次选中的预设和频段
      if (savedId) {
        // 自定义：恢复之前保存的频段
        if (savedId === CUSTOM_PRESET_ID) {
          setSelectedPresetId(CUSTOM_PRESET_ID);
          selectedPresetIdRef.current = CUSTOM_PRESET_ID;
          setBands(mapBandsToLayout(savedBands ?? [], layout));
          return;
        }
        // 已保存的预设
        const saved = presetList.find((p) => p.id === savedId);
        if (saved) {
          setSelectedPresetId(saved.id);
          selectedPresetIdRef.current = saved.id;
          setBands(mapBandsToLayout(savedBands ?? saved.bands, layout));
          return;
        }
      }

      // 兜底：选第一个
      const first = presetList[0];
      setSelectedPresetId(first.id);
      selectedPresetIdRef.current = first.id;
      setBands(mapBandsToLayout(savedBands?.length === layout.count ? savedBands : first.bands, layout));
    } catch (error) {
      console.error("Failed to load EQ data:", error);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // 定时刷新引擎状态
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const status = await invoke<EngineStatus>("get_eq_engine_status");
        setEngineStatus(status);
      } catch (e) {
        // ignore
      }
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  // 注入主题色滚动条
  useEffect(() => {
    const style = document.createElement("style");
    style.id = "eq-scrollbar-theme";
    style.textContent = `.eq-preset-scroll::-webkit-scrollbar{width:4px}
.eq-preset-scroll::-webkit-scrollbar-track{background:transparent}
.eq-preset-scroll::-webkit-scrollbar-thumb{background:${activeColor}88;border-radius:2px}
.eq-preset-scroll::-webkit-scrollbar-thumb:hover{background:${activeColor}}`;
    document.head.appendChild(style);
    return () => {
      const el = document.getElementById("eq-scrollbar-theme");
      if (el) el.remove();
    };
  }, [activeColor]);

  // 处理安装驱动
  const handleInstall = async () => {
    setInstalling(true);
    try {
      const result = await invoke<string>("install_virtual_audio_driver");
      toast({
        title: t("audioEq.installSuccess"),
        description: result,
        status: "success",
        duration: 3000,
        isClosable: true,
      });
      await loadData();
    } catch (error) {
      toast({
        title: t("audioEq.installFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setInstalling(false);
    }
  };

  // 处理卸载驱动
  const handleUninstall = async () => {
    setUninstalling(true);
    try {
      const result = await invoke<string>("uninstall_virtual_audio_driver");
      toast({
        title: t("audioEq.uninstallSuccess"),
        description: result,
        status: "success",
        duration: 3000,
        isClosable: true,
      });
      await loadData();
      await loadData();
    } catch (error) {
      toast({
        title: t("audioEq.uninstallFailed"),
        description: String(error),
        status: "error",
        duration: 8000,
        isClosable: true,
      });
    } finally {
      setUninstalling(false);
    }
  };

  // 启动 EQ 引擎
  const handleStartEngine = async () => {
    setStartingEngine(true);
    try {
      await invoke("start_eq_engine", { deviceName: selectedDevice });
      // 引擎启动后，同步当前设置（频段用拟合增益）
      await invoke("update_eq_bands", { bands: engineBandsFromCurve(bands) }).catch(() => {});
      await invoke("update_eq_preamp", { gain: preamp }).catch(() => {});
      await invoke("update_eq_effects", { ...fx }).catch(() => {});
      toast({
        title: t("audioEq.engineStarted"),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      const status = await invoke<EngineStatus>("get_eq_engine_status");
      setEngineStatus(status);
    } catch (error) {
      toast({
        title: t("audioEq.engineStartFailed"),
        description: String(error),
        status: "error",
        duration: 5000,
        isClosable: true,
      });
    } finally {
      setStartingEngine(false);
    }
  };

  // 停止 EQ 引擎
  const handleStopEngine = async () => {
    try {
      await invoke("stop_eq_engine");
      toast({
        title: t("audioEq.engineStopped"),
        status: "info",
        duration: 2000,
        isClosable: true,
      });
      setEngineStatus({ running: false, pid: null });
    } catch (error) {
      toast({
        title: t("audioEq.engineStopFailed"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  // 总增益调节
  const handlePreampChange = (gain: number) => {
    setPreamp(gain);
    store.set(EQ_STORE_PREAMP, gain).then(() => store.save()).catch(() => {});
    invoke("update_eq_preamp", { gain }).catch(() => {});
  };

  // 选择输出设备
  const handleDeviceChange = (deviceName: string) => {
    setSelectedDevice(deviceName);
    store.set(EQ_STORE_DEVICE, deviceName).then(() => store.save()).catch(() => {});
  };

  // 刷新设备列表
  const handleRefreshDevices = async () => {
    try {
      const devList = await invoke<AudioDevice[]>("list_audio_devices");
      setDevices(devList ?? []);
    } catch (e) {
      console.error("Failed to list audio devices:", e);
    }
  };

  // 选择预设
  const handleSelectPreset = async (preset: EqPreset) => {
    setSelectedPresetId(preset.id);
    selectedPresetIdRef.current = preset.id;
    const displayBands = mapBandsToLayout(preset.bands, bandLayout);
    setBands(displayBands);

    await store.set(EQ_STORE_SELECTED, preset.id);
    await store.set(EQ_STORE_BANDS, displayBands);

    // 音效参数：导入预设用预设值，内置/自定义重置为 0
    const fx = preset.effects
      ? {
          clarity: preset.effects.clarity,
          ambience: preset.effects.ambience,
          width: preset.effects.width,
          dynamics: preset.effects.dynamics,
          bass: preset.effects.bass,
        }
      : { clarity: 0, ambience: 0, width: 0, dynamics: 0, bass: 0 };
    setFx(fx);
    invoke("update_eq_effects", { ...fx }).catch(() => {});
    await store.set(EQ_STORE_FX, fx);

    await store.save();
    try {
      await invoke("apply_eq_preset", { presetId: preset.id });
      // 引擎统一接收拟合增益（与滑块曲线一致，切换波段音色不变）
      if (engineStatus.running) {
        invoke("update_eq_bands", { bands: engineBandsFromCurve(displayBands) }).catch(() => {});
      }
      toast({
        title: t("audioEq.presetApplied"),
        description: preset.name,
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      console.error("Failed to apply preset:", error);
    }
  };

  // 节流同步到音频引擎（避免拖拽时每帧都发送）
  // 引擎接收拟合增益而非曲线原值，保证实测频响与滑块曲线一致
  const syncBandsToEngine = (bands: EqBand[]) => {
    if (!engineStatus.running) return;
    const now = Date.now();
    if (now - lastEngineSyncRef.current < 50) return; // 50ms 节流
    lastEngineSyncRef.current = now;
    invoke("update_eq_bands", { bands: engineBandsFromCurve(bands) }).catch(() => {});
  };

  // 更新频段频率（实时同步到音频引擎）
  const handleFreqChange = (index: number, freq: number) => {
    setBands((prev) => {
      const newBands = [...prev];
      newBands[index] = { ...newBands[index], freq };
      syncBandsToEngine(newBands);
      store.set(EQ_STORE_BANDS, newBands).then(() => store.save()).catch(() => {});
      return newBands;
    });
  };

  // 更新频段增益（实时同步到音频引擎）
  const handleBandChange = (index: number, gain: number) => {
    setBands((prev) => {
      const newBands = [...prev];
      newBands[index] = { ...newBands[index], gain };
      syncBandsToEngine(newBands);
      // 保存当前频段状态 + 当前选中的预设 ID（原子保存）
      Promise.all([
        store.set(EQ_STORE_BANDS, newBands),
        store.set(EQ_STORE_SELECTED, selectedPresetIdRef.current),
      ]).then(() => store.save()).catch(() => {});
      // 自定义模式：额外保存自定义频段供下次切换恢复
      if (selectedPresetIdRef.current === CUSTOM_PRESET_ID) {
        store.set(EQ_STORE_CUSTOM_BANDS, newBands).catch(() => {});
      }
      return newBands;
    });
  };

  // 重置频段（实时同步）
  const handleResetBands = () => {
    const resetBands = bands.map((b) => ({ ...b, gain: 0 }));
    setBands(resetBands);
    store.set(EQ_STORE_BANDS, resetBands).then(() => store.save()).catch(() => {});
    if (selectedPresetIdRef.current === CUSTOM_PRESET_ID) {
      store.set(EQ_STORE_CUSTOM_BANDS, resetBands).catch(() => {});
    }
    if (engineStatus.running) {
      invoke("update_eq_bands", { bands: engineBandsFromCurve(resetBands) }).catch(() => {});
    }
  };

  // 导入 FAC 预设文件
  const handleImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setImporting(true);
    try {
      const content = await file.text();
      const imported = await invoke<EqPreset>("import_eq_preset", { content });
      setPresets((prev) => [...prev, imported]);
      const newImportedIds = [...importedIds, imported.id];
      setImportedIds(newImportedIds);
      await store.set(EQ_STORE_IMPORTED, newImportedIds);
      await store.save();
      toast({
        title: t("audioEq.importSuccess") ?? "导入成功",
        description: imported.name,
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      // 重置 input 以便重复导入同一文件
      if (fileInputRef.current) fileInputRef.current.value = "";
    } catch (error) {
      toast({
        title: t("audioEq.importFailed") ?? "导入失败",
        description: String(error),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setImporting(false);
    }
  };

  // 构建 FAC 文件内容（含当前音效参数）
  const buildFacContent = (name: string): string => {
    const midi = (v: number) => Math.round(v * 127);
    const bandsStr = bands
      .map(
        (b, i) =>
          `Band ${i + 1}\n   ${b.freq}: CF\n   ${Math.round(b.gain)}: Boost/Cut`
      )
      .join("\n");
    return `CLASS1 : Effect Type
9: Version
${name}
0: Double Params Flag
1: Total number of elements
${midi(fx.clarity)}: Main 0
${midi(fx.width)}: Main 1
0: Main 2
${midi(fx.ambience)}: Main 3
${midi(fx.dynamics)}: Main 4
${midi(fx.bass)}: Main 5
0: Element Number
   0: Param 0
   0: Param 1
   0: Param 2
   0: Param 3
   0: Param 4
   0: Param 5
   0: Param 6
7: Number of Application Dependent Integers
0: Number of Application Dependent Reals
0: Number of Application Dependent Strings
${fx.clarity > 0 ? 1 : 0}: Integer[0]
${fx.width > 0 ? 1 : 0}: Integer[1]
${fx.ambience > 0 ? 1 : 0}: Integer[2]
${fx.dynamics > 0 ? 1 : 0}: Integer[3]
${fx.bass > 0 ? 1 : 0}: Integer[4]
0: Integer[5]
2: Integer[6]
${bands.length}: Number of EQ Bands
1: On/Off Flag
${bandsStr}`;
  };

  // 保存当前频段为预设
  const handleSavePreset = async () => {
    const name = presetName.trim();
    if (!name) {
      toast({
        title: t("audioEq.saveFailed") ?? "保存失败",
        description: "请输入预设名称",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    try {
      const content = buildFacContent(name);
      const saved = await invoke<EqPreset>("import_eq_preset", { content });
      setPresets((prev) => [...prev, saved]);
      const newSavedIds = [...savedIds, saved.id];
      setSavedIds(newSavedIds);
      await store.set(EQ_STORE_SAVED, newSavedIds);
      await store.save();
      setPresetName("");
      onSaveModalClose();
      // 保持在预设 Tab，选中新预设
      setSelectedPresetId(saved.id);
      selectedPresetIdRef.current = saved.id;
      toast({
        title: t("audioEq.saveSuccess") ?? "保存成功",
        description: name,
        status: "success",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("audioEq.saveFailed") ?? "保存失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  // 切换波段布局（5/10/15/20/31）：当前曲线按频率就近映射到新布局
  const handleLayoutChange = async (layout: BandLayoutDef) => {
    if (layout.count === bandLayout.count) return;
    const mapped = mapBandsToLayout(bands, layout);
    setBandLayout(layout);
    setBands(mapped);
    Promise.all([
      store.set(EQ_STORE_LAYOUT, layout.count),
      store.set(EQ_STORE_BANDS, mapped),
      ...(selectedPresetIdRef.current === CUSTOM_PRESET_ID ? [store.set(EQ_STORE_CUSTOM_BANDS, mapped)] : []),
    ]).then(() => store.save()).catch(() => {});
    if (engineStatus.running) {
      invoke("update_eq_bands", { bands: engineBandsFromCurve(mapped) }).catch(() => {});
    }
  };

  // 切换到自定义模式（恢复上次自定义值，首次则为全零）
  const handleCustomPreset = async () => {
    setSelectedPresetId(CUSTOM_PRESET_ID);
    selectedPresetIdRef.current = CUSTOM_PRESET_ID;
    // 读取上次保存的自定义频段（波段数一致才按索引恢复）
    const savedCustom = await store.get<EqBand[]>(EQ_STORE_CUSTOM_BANDS);
    const customBands = savedCustom && savedCustom.length === bands.length
      ? bands.map((b, i) => ({ ...b, gain: savedCustom[i]?.gain ?? 0 }))
      : bands.map((b) => ({ ...b, gain: 0 }));
    setBands(customBands);
    // 读取上次保存的音效值
    const savedFx = await store.get<typeof defaultFx>(EQ_STORE_FX);
    if (savedFx) {
      setFx(savedFx);
      invoke("update_eq_effects", { ...savedFx }).catch(() => {});
    }
    store.set(EQ_STORE_SELECTED, CUSTOM_PRESET_ID).then(() => store.save()).catch(() => {});
    invoke("update_eq_bands", { bands: engineBandsFromCurve(customBands) }).catch(() => {});
  };

  // 删除保存的预设（从 presets tab 中删除）
  const handleDeleteSavedPreset = async (presetId: string, name: string) => {
    try {
      await invoke("delete_eq_preset", { presetId });
      setPresets((prev) => prev.filter((p) => p.id !== presetId));
      const newSavedIds = savedIds.filter((id) => id !== presetId);
      setSavedIds(newSavedIds);
      await store.set(EQ_STORE_SAVED, newSavedIds);
      await store.save();
      if (selectedPresetId === presetId) {
        setSelectedPresetId("");
        selectedPresetIdRef.current = "";
      }
      toast({
        title: t("audioEq.deleteSuccess") ?? "已删除",
        description: name,
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("audioEq.deleteFailed") ?? "删除失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  // 删除导入的预设
  const handleDeletePreset = async (presetId: string, name: string) => {
    try {
      await invoke("delete_eq_preset", { presetId });
      setPresets((prev) => prev.filter((p) => p.id !== presetId));
      const newImportedIds = importedIds.filter((id) => id !== presetId);
      setImportedIds(newImportedIds);
      await store.set(EQ_STORE_IMPORTED, newImportedIds);
      await store.save();
      // 如果删除的是当前选中的，清除选中
      if (selectedPresetId === presetId) {
        setSelectedPresetId("");
        selectedPresetIdRef.current = "";
      }
      toast({
        title: t("audioEq.deleteSuccess") ?? "已删除",
        description: name,
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    } catch (error) {
      toast({
        title: t("audioEq.deleteFailed") ?? "删除失败",
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  if (loading) {
    return (
      <Box pt={8} display="flex" justifyContent="center" alignItems="center" minH="400px">
        <VStack spacing={4}>
          <Spinner size="xl" color={activeColor} />
          <Text color={descColor}>{t("audioEq.loading")}</Text>
        </VStack>
      </Box>
    );
  }

  return (
    <Box pt={8} pb={8}>
      {/* 顶部导航 */}
      <HStack mb={6} spacing={3}>
        <IconButton
          aria-label={t("common.back")}
          icon={<ArrowLeft size={20} />}
          variant="ghost"
          onClick={() => navigate("/builtin-tools")}
          color={headingColor}
        />
        <Volume2 size={24} color={headingColor} />
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("audioEq.title")}
        </Heading>
      </HStack>

      <VStack spacing={4} align="stretch">
        {/* 虚拟声卡 & EQ引擎 合并顶部卡片 */}
        <LiquidGlassCard p={4}>
          <VStack align="stretch" spacing={3}>
            {/* 标题行 + 状态 */}
            <HStack justify="space-between" align="start">
              <HStack spacing={3}>
                <Box
                  w={10}
                  h={10}
                  borderRadius="lg"
                  bg={iconBg}
                  display="flex"
                  alignItems="center"
                  justifyContent="center"
                >
                  <Volume2 size={20} color={activeColor} />
                </Box>
                <VStack align="start" spacing={0}>
                  <Text fontWeight="bold" fontSize="md" color={headingColor}>
                    {t("audioEq.virtualSoundCard")} & {t("audioEq.eqEngine")}
                  </Text>
                  <Text fontSize="xs" color={descColor}>
                    {driverStatus?.device_name ?? t("audioEq.virtualSoundCardDesc")}
                  </Text>
                </VStack>
              </HStack>

              <HStack spacing={2}>
                {engineStatus.running && (
                  <Badge colorScheme="green" px={2} py={1} borderRadius="full" variant="subtle">
                    <HStack spacing={1}>
                      <Box w={2} h={2} borderRadius="full" bg="green.400" />
                      <Text fontSize="xs">{t("audioEq.running")}</Text>
                    </HStack>
                  </Badge>
                )}
                {driverStatus?.needs_reboot ? (
                  <Badge colorScheme="orange" px={2} py={1} borderRadius="full" variant="subtle">
                    <HStack spacing={1}>
                      <AlertTriangle size={12} />
                      <Text fontSize="xs">{t("audioEq.needsReboot")}</Text>
                    </HStack>
                  </Badge>
                ) : driverStatus?.installed ? (
                  <Badge colorScheme="green" px={2} py={1} borderRadius="full" variant="subtle">
                    <HStack spacing={1}>
                      <CheckCircle size={12} />
                      <Text fontSize="xs">{t("audioEq.installed")}</Text>
                    </HStack>
                  </Badge>
                ) : (
                  <Badge colorScheme="gray" px={2} py={1} borderRadius="full" variant="subtle">
                    <HStack spacing={1}>
                      <XCircle size={12} />
                      <Text fontSize="xs">{t("audioEq.notInstalled")}</Text>
                    </HStack>
                  </Badge>
                )}
              </HStack>
            </HStack>

            {/* 操作按钮行 */}
            <HStack spacing={2} flexWrap="wrap" justify="space-between">
              {/* 左侧：卸载 */}
              <HStack spacing={2}>
                {driverStatus?.installed && !driverStatus?.needs_reboot && (
                  <Button
                    leftIcon={<Trash2 size={16} />}
                    colorScheme="red"
                    variant="ghost"
                    size="sm"
                    onClick={handleUninstall}
                    isLoading={uninstalling}
                  >
                    {t("audioEq.uninstallDriver")}
                  </Button>
                )}
              </HStack>

              {/* 右侧：安装/启动/停止/刷新 */}
              <HStack spacing={2}>
                {!driverStatus?.installed && !driverStatus?.needs_reboot && (
                  <Button
                    leftIcon={<Download size={16} />}
                    colorScheme="blue"
                    size="sm"
                    onClick={handleInstall}
                    isLoading={installing}
                    loadingText={t("audioEq.installing")}
                  >
                    {t("audioEq.installDriver")}
                  </Button>
                )}
                {!engineStatus.running && (
                  <Button
                    leftIcon={<Play size={16} />}
                    colorScheme="green"
                    size="sm"
                    onClick={handleStartEngine}
                    isLoading={startingEngine}
                    loadingText={t("audioEq.starting")}
                    isDisabled={!driverStatus?.installed}
                  >
                    {t("audioEq.startEngine")}
                  </Button>
                )}
                {engineStatus.running && (
                  <Button
                    leftIcon={<Square size={16} />}
                    colorScheme="red"
                    variant="outline"
                    size="sm"
                    onClick={handleStopEngine}
                  >
                    {t("audioEq.stopEngine")}
                  </Button>
                )}
                <IconButton
                  aria-label={t("common.refresh")}
                  icon={<RefreshCw size={16} />}
                  size="sm"
                  variant="ghost"
                  onClick={loadData}
                />
              </HStack>
            </HStack>

            {/* 警告提示 */}
            {driverStatus?.needs_reboot && (
              <HStack spacing={2} p={2} borderRadius="md" bg={warningBg}>
                <AlertTriangle size={14} color="orange" />
                <Text fontSize="xs" color={warningColor}>
                  {t("audioEq.rebootHint")}
                </Text>
              </HStack>
            )}
            {!driverStatus?.installed && !driverStatus?.needs_reboot && (
              <HStack spacing={2} p={2} borderRadius="md" bg={warningBg}>
                <AlertTriangle size={14} color="orange" />
                <Text fontSize="xs" color={warningColor}>
                  {t("audioEq.installHint")}
                </Text>
              </HStack>
            )}
            {!driverStatus?.installed && (
              <Text fontSize="xs" color="orange.500">
                {t("audioEq.installDriverFirst")}
              </Text>
            )}
          </VStack>
        </LiquidGlassCard>

        {/* 音频设备选择 + 总增益 */}
        <LiquidGlassCard p={4}>
          <HStack spacing={4} align="center" flexWrap="wrap">
            {/* 左侧：音频设备选择（Chakra UI FormControl） */}
            <FormControl
              minW="240px"
              maxW="340px"
              display="inline-flex"
              flexDirection="row"
              alignItems="center"
              gap={2}
              isDisabled={engineStatus.running}
            >
              <FormLabel
                m={0}
                fontSize="sm"
                fontWeight="bold"
                color={headingColor}
                whiteSpace="nowrap"
              >
                音频设备
              </FormLabel>
              <Box
                flex={1}
                minW={0}
                pointerEvents={engineStatus.running ? "none" : "auto"}
                opacity={engineStatus.running ? 0.5 : 1}
                transition="opacity 0.2s"
              >
                <CustomSelect
                  value={selectedDevice}
                  onChange={handleDeviceChange}
                  options={devices.map((dev) => ({ value: dev.name, label: dev.name }))}
                  width="100%"
                  placeholder="选择音频设备"
                />
              </Box>
              <IconButton
                aria-label="刷新设备列表"
                icon={<RefreshCw size={14} />}
                size="sm"
                variant="ghost"
                onClick={handleRefreshDevices}
                isDisabled={engineStatus.running}
              />
            </FormControl>

            {/* 总增益 */}
            <Text fontSize="sm" fontWeight="bold" color={headingColor} whiteSpace="nowrap" ml={2}>
              总增益
            </Text>
            <Box flex={1}>
              <input
                type="range"
                min={-12}
                max={12}
                step={0.5}
                value={preamp}
                onChange={(e) => handlePreampChange(parseFloat(e.target.value))}
                style={{
                  width: "100%",
                  height: "6px",
                  appearance: "none",
                  background: `linear-gradient(to right, #8884 0%, ${activeColor} ${((preamp + 12) / 24) * 100}%, #8884 ${((preamp + 12) / 24) * 100}%, #8884 100%)`,
                  borderRadius: "3px",
                  outline: "none",
                  cursor: "pointer",
                }}
              />
            </Box>
            <Text fontSize="sm" fontWeight="bold" color={preamp === 0 ? descColor : (preamp > 0 ? "#38A169" : "#E53E3E")} minW="40px" textAlign="right">
              {preamp > 0 ? "+" : ""}{preamp} dB
            </Text>
          </HStack>
        </LiquidGlassCard>

        {/* 均衡器 + 预设 双栏布局（绝对定位限制卡片高度） */}
        <HStack spacing={4} align="stretch" position="relative">
          {/* 左侧：均衡器 + 音效处理 */}
          <Box flex="1" minW={0} display="flex" flexDirection="column">
            <LiquidGlassCard p={4}>
              <VStack align="stretch" spacing={4}>
                <HStack justify="space-between">
                  <Text fontWeight="bold" fontSize="md" color={headingColor}>
                    {t("audioEq.equalizer")}
                  </Text>
                  <HStack spacing={2}>
                    {/* 波段数切换器 */}
                    <HStack spacing={0} p="2px" borderRadius="full" bg={switcherBg}>
                      {BAND_LAYOUTS.map((l) => {
                        const active = l.count === bandLayout.count;
                        return (
                          <Box
                            key={l.count}
                            as="button"
                            px={2}
                            py="2px"
                            borderRadius="full"
                            fontSize="xs"
                            fontWeight={active ? "bold" : "medium"}
                            bg={active ? activeColor : "transparent"}
                            color={active ? "white" : descColor}
                            _hover={{ color: active ? "white" : headingColor }}
                            transition="all 0.15s"
                            cursor="pointer"
                            lineHeight="1.5"
                            onClick={() => handleLayoutChange(l)}
                          >
                            {l.count}
                          </Box>
                        );
                      })}
                    </HStack>
                    <Button leftIcon={<RefreshCw size={14} />} size="xs" variant="ghost" onClick={handleResetBands}>
                      {t("audioEq.reset")}
                    </Button>
                  </HStack>
                </HStack>
                <Box position="relative">
                  <Box position="absolute" inset={0} zIndex={1} pointerEvents="none">
                    <FrequencyResponseCurve bands={bands} />
                  </Box>
                  <HStack justify="space-between" align="stretch" spacing={bandLayout.count > 16 ? 0.5 : 1} px={1}>
                    {bands.map((band, index) => (
                      <EqBandSlider
                        key={index}
                        band={band}
                        compact={bandLayout.count > 16}
                        label={bandLayout.rangeLabels?.[index]}
                        onChange={(gain) => handleBandChange(index, gain)}
                      />
                    ))}
                  </HStack>
                </Box>
                {bandLayout.ranges && (
                  <HStack spacing={1} justify="space-between" px={1}>
                    {bands.map((band, index) => {
                      const range = bandLayout.ranges![index];
                      const minF = range?.[0] ?? band.freq * 0.7;
                      const maxF = range?.[1] ?? band.freq * 1.4;
                      return (
                        <VStack key={index} spacing={0.5} w="full" align="center">
                          <FreqKnob freq={band.freq} minFreq={minF} maxFreq={maxF} onChange={(f) => handleFreqChange(index, f)} />
                        </VStack>
                      );
                    })}
                  </HStack>
                )}
              </VStack>
            </LiquidGlassCard>
            <Box mt={4}>
              <LiquidGlassCard p={4}>
                <Text fontWeight="bold" fontSize="md" color={headingColor} mb={3}>音效处理</Text>
                <VStack spacing={3} align="stretch">
                  {([
                    { key: "clarity", label: "清晰度", desc: "提升声音清晰度" },
                    { key: "ambience", label: "环境", desc: "模拟空间混响" },
                    { key: "width", label: "环绕", desc: "拓宽声场" },
                    { key: "dynamics", label: "动态", desc: "提升响度" },
                    { key: "bass", label: "低音", desc: "增强低频谐波" },
                  ] as const).map(({ key, label, desc }) => (
                    <Box key={key}>
                      <HStack justify="space-between" mb={1}>
                        <HStack spacing={2}>
                          <Text fontSize="sm" fontWeight="medium" color={labelColor}>{label}</Text>
                          <Text fontSize="xs" color={descColor}>{desc}</Text>
                        </HStack>
                        <Text fontSize="sm" fontWeight="bold" color={fx[key] > 0 ? activeColor : descColor} minW="28px" textAlign="right">
                          {Math.round(fx[key] * 10)}
                        </Text>
                      </HStack>
                      <Box position="relative" h="20px" display="flex" alignItems="center">
                        <Box w="full" h="6px" borderRadius="3px" bg="#8884" position="relative" overflow="visible">
                          <Box h="full" borderRadius="3px" bg={activeColor} w={`${fx[key] * 100}%`} transition="width 0.12s ease-out" />
                          <Box position="absolute" top="50%" left={`${fx[key] * 100}%`} transform="translate(-50%, -50%)"
                            w="16px" h="16px" borderRadius="full" bg={activeColor}
                            border="2px solid" borderColor={useColorModeValue("white", "#222")}
                            boxShadow="0 1px 4px rgba(0,0,0,0.3)" pointerEvents="none" transition="left 0.12s ease-out" />
                        </Box>
                        <input type="range" min={0} max={10} step={1} value={Math.round(fx[key] * 10)}
                          onChange={(e) => {
                            const v = parseInt(e.target.value) / 10;
                            setFx((prev) => {
                              const newFx = { ...prev, [key]: v };
                              invoke("update_eq_effects", { ...newFx }).catch(() => {});
                              Promise.all([store.set(EQ_STORE_FX, newFx), store.set(EQ_STORE_SELECTED, selectedPresetIdRef.current)])
                                .then(() => store.save()).catch(() => {});
                              return newFx;
                            });
                          }}
                          style={{ position: "absolute", top: 0, left: 0, width: "100%", height: "20px",
                            margin: 0, padding: 0, opacity: 0, cursor: "pointer",
                            appearance: "none", WebkitAppearance: "none", MozAppearance: "none" }}
                        />
                      </Box>
                    </Box>
                  ))}
                </VStack>
              </LiquidGlassCard>
            </Box>
          </Box>

          {/* 右侧占位（保持宽度） */}
          <Box w="230px" flexShrink={0} />

          {/* 预设卡片：绝对定位，top=0 bottom=0 强制限高 */}
          <Box position="absolute" right={0} top={0} bottom={0} w="230px" display="flex" flexDirection="column" overflow="hidden">
            <LiquidGlassCard p={4} flex={1} display="flex" flexDirection="column" minH={0} overflow="hidden">
              <Tabs index={tabIndex} onChange={setTabIndex} variant="soft-rounded" size="sm" isFitted
                display="flex" flexDirection="column" flex={1} minH={0}>
                <TabList mb={3} flexShrink={0}>
                  <Tab fontSize="xs" py={1} _selected={{ color: activeColor }}>{t("audioEq.presets")}</Tab>
                  <Tab fontSize="xs" py={1} _selected={{ color: activeColor }}>{t("audioEq.import") ?? "导入"}</Tab>
                </TabList>
                <TabPanels display="flex" flexDirection="column" flex={1} minH={0}>
                  <TabPanel p={0} flex={1} overflowY="auto" minH={0} className="eq-preset-scroll">
                    <VStack spacing={1} align="stretch">
                      {builtInPresets.map((preset) => {
                        const Icon = getPresetIcon(preset.name);
                        const isSelected = selectedPresetId === preset.id;
                        return (
                          <Box key={preset.id} onClick={() => handleSelectPreset(preset)} cursor="pointer"
                            px={3} py={2.5} borderRadius="lg" border="1px solid"
                            borderColor={isSelected ? activeColor : `${activeColor}18`}
                            bg={isSelected ? `${activeColor}12` : `${activeColor}04`}
                            _hover={{ bg: `${activeColor}10`, borderColor: isSelected ? activeColor : `${activeColor}40` }}
                            transition="all 0.2s">
                            <HStack spacing={2}>
                              <Icon size={16} color={isSelected ? activeColor : presetIconColor} />
                              <Text fontSize="sm" fontWeight={isSelected ? "bold" : "medium"}
                                color={isSelected ? activeColor : labelColor} isTruncated>{preset.name}</Text>
                            </HStack>
                          </Box>
                        );
                      })}
                      {savedPresets.map((preset) => {
                        const Icon = getPresetIcon(preset.name);
                        const isSelected = selectedPresetId === preset.id;
                        return (
                          <HStack key={preset.id} spacing={0}>
                            <Box flex={1} onClick={() => handleSelectPreset(preset)} cursor="pointer"
                              px={3} py={2} borderRadius="lg" border="1px solid"
                              borderColor={isSelected ? activeColor : `${activeColor}18`}
                              bg={isSelected ? `${activeColor}12` : `${activeColor}04`}
                              _hover={{ bg: `${activeColor}10`, borderColor: isSelected ? activeColor : `${activeColor}40` }}
                              transition="all 0.2s">
                              <HStack spacing={2}>
                                <Icon size={14} color={isSelected ? activeColor : presetIconColor} />
                                <Text fontSize="xs" fontWeight={isSelected ? "bold" : "medium"}
                                  color={isSelected ? activeColor : labelColor} isTruncated>{preset.name}</Text>
                              </HStack>
                            </Box>
                            <IconButton aria-label={t("audioEq.deletePreset") ?? "删除"}
                              icon={<Trash2 size={12} />} size="xs" variant="ghost" color={descColor}
                              _hover={{ color: "red.400", bg: "transparent" }}
                              onClick={(e) => { e.stopPropagation(); handleDeleteSavedPreset(preset.id, preset.name); }} ml={1} />
                          </HStack>
                        );
                      })}
                      <Box onClick={handleCustomPreset} cursor="pointer" px={3} py={2.5} borderRadius="lg"
                        border="1px solid"
                        borderColor={selectedPresetId === CUSTOM_PRESET_ID ? activeColor : `${activeColor}18`}
                        bg={selectedPresetId === CUSTOM_PRESET_ID ? `${activeColor}12` : `${activeColor}04`}
                        _hover={{ bg: `${activeColor}10`, borderColor: selectedPresetId === CUSTOM_PRESET_ID ? activeColor : `${activeColor}40` }}
                        transition="all 0.2s">
                        <HStack spacing={2}>
                          <Music size={16} color={selectedPresetId === CUSTOM_PRESET_ID ? activeColor : presetIconColor} />
                          <Text fontSize="sm" fontWeight={selectedPresetId === CUSTOM_PRESET_ID ? "bold" : "medium"}
                            color={selectedPresetId === CUSTOM_PRESET_ID ? activeColor : labelColor}>自定义</Text>
                        </HStack>
                      </Box>
                    </VStack>
                  </TabPanel>
                  <TabPanel p={0}>
                    <VStack spacing={3} align="stretch" pt={2}>
                      <Input ref={fileInputRef} type="file" accept=".fac" onChange={handleImportFile} display="none" id="fac-file-input" />
                      <Button as="label" htmlFor="fac-file-input" leftIcon={<Upload size={14} />}
                        size="sm" w="full" variant="outline" isLoading={importing} cursor="pointer"
                        borderColor={`${activeColor}40`} color={activeColor}
                        _hover={{ bg: `${activeColor}10`, borderColor: activeColor }}>
                        {t("audioEq.chooseFacFile") ?? "选择 .fac 文件"}
                      </Button>
                      <Text fontSize="xs" color={descColor}>{t("audioEq.importDesc") ?? "导入 .fac 预设文件。"}</Text>
                      {importedPresets.length === 0 ? (
                        <Text fontSize="xs" color={descColor} textAlign="center" py={2}>
                          {t("audioEq.noImported") ?? "暂无导入的预设"}
                        </Text>
                      ) : (
                        <VStack spacing={1} align="stretch">
                          {importedPresets.map((preset) => {
                            const Icon = getPresetIcon(preset.name);
                            const isSelected = selectedPresetId === preset.id;
                            return (
                              <HStack key={preset.id} spacing={0}>
                                <Box flex={1} onClick={() => handleSelectPreset(preset)} cursor="pointer"
                                  px={3} py={2} borderRadius="lg" border="1px solid"
                                  borderColor={isSelected ? activeColor : `${activeColor}18`}
                                  bg={isSelected ? `${activeColor}12` : `${activeColor}04`}
                                  _hover={{ bg: `${activeColor}10`, borderColor: isSelected ? activeColor : `${activeColor}40` }}
                                  transition="all 0.2s">
                                  <HStack spacing={2}>
                                    <Icon size={14} color={isSelected ? activeColor : presetIconColor} />
                                    <Text fontSize="xs" fontWeight={isSelected ? "bold" : "medium"}
                                      color={isSelected ? activeColor : labelColor} isTruncated>{preset.name}</Text>
                                  </HStack>
                                </Box>
                                <IconButton aria-label={t("audioEq.deletePreset") ?? "删除"}
                                  icon={<Trash2 size={12} />} size="xs" variant="ghost" color={descColor}
                                  _hover={{ color: "red.400", bg: "transparent" }}
                                  onClick={(e) => { e.stopPropagation(); handleDeletePreset(preset.id, preset.name); }} ml={1} />
                              </HStack>
                            );
                          })}
                        </VStack>
                      )}
                    </VStack>
                  </TabPanel>
                </TabPanels>
              </Tabs>
              {selectedPresetId && (
                <VStack spacing={2} mt={2} flexShrink={0}>
                  {importedPresets.some((p) => p.id === selectedPresetId) ? (
                    <Button size="sm" w="full" bg={activeColor} color="white"
                      _hover={{ opacity: 0.85, bg: activeColor }} _active={{ opacity: 0.7, bg: activeColor }}
                      leftIcon={<Save size={14} />}
                      onClick={async () => {
                        const preset = importedPresets.find((p) => p.id === selectedPresetId);
                        if (!preset) return;
                        try {
                          const content = buildFacContent(preset.name);
                          await invoke("save_eq_preset", { presetId: preset.id, content });
                          toast({ title: "保存成功", description: preset.name, status: "success", duration: 2000, isClosable: true });
                          await loadData();
                        } catch (e) {
                          toast({ title: "保存失败", description: String(e), status: "error", duration: 3000, isClosable: true });
                        }
                      }}>保存更改</Button>
                  ) : (
                    <Button size="sm" w="full" bg={activeColor} color="white"
                      _hover={{ opacity: 0.85, bg: activeColor }} _active={{ opacity: 0.7, bg: activeColor }}
                      leftIcon={<Save size={14} />}
                      onClick={() => { setPresetName(""); onSaveModalOpen(); }}>保存为预设</Button>
                  )}
                  <Button size="sm" w="full" variant="outline" borderColor={`${activeColor}40`} color={activeColor}
                    _hover={{ bg: `${activeColor}10`, borderColor: activeColor }} leftIcon={<Download size={14} />}
                    onClick={async () => {
                      try {
                        const exportName = "新境盒-EQ调音";
                        const content = buildFacContent(exportName);
                        const { save } = await import("@tauri-apps/plugin-dialog");
                        const path = await save({ defaultPath: `${exportName}.fac`, filters: [{ name: "FxSound 预设", extensions: ["fac"] }] });
                        if (!path) return;
                        await invoke("export_fac_file", { path, content });
                        toast({ title: "导出成功", description: path, status: "success", duration: 3000, isClosable: true });
                      } catch (e) {
                        toast({ title: "导出失败", description: String(e), status: "error", duration: 3000, isClosable: true });
                      }
                    }}>导出 .fac</Button>
                </VStack>
              )}
            </LiquidGlassCard>
          </Box>
        </HStack>

        {/* 说明区域 */}
        <LiquidGlassCard p={4}>
          <VStack align="start" spacing={3}>
            <Text fontWeight="bold" fontSize="sm" color={headingColor}>
              {t("audioEq.usageGuide")}
            </Text>
            <VStack align="start" spacing={2}>
              <HStack spacing={2} align="start">
                <Text fontSize="xs" color={activeColor} fontWeight="bold">1.</Text>
                <Text fontSize="xs" color={descColor} lineHeight="1.6">
                  {t("audioEq.step1")}
                </Text>
              </HStack>
              <HStack spacing={2} align="start">
                <Text fontSize="xs" color={activeColor} fontWeight="bold">2.</Text>
                <Text fontSize="xs" color={descColor} lineHeight="1.6">
                  {t("audioEq.step2")}
                </Text>
              </HStack>
              <HStack spacing={2} align="start">
                <Text fontSize="xs" color={activeColor} fontWeight="bold">3.</Text>
                <Text fontSize="xs" color={descColor} lineHeight="1.6">
                  {t("audioEq.step3")}
                </Text>
              </HStack>
              <HStack spacing={2} align="start">
                <Text fontSize="xs" color={activeColor} fontWeight="bold">4.</Text>
                <Text fontSize="xs" color={descColor} lineHeight="1.6">
                  {t("audioEq.step4")}
                </Text>
              </HStack>
            </VStack>
          </VStack>
        </LiquidGlassCard>
      </VStack>

      {/* 保存为预设 模态框 */}
      <Modal isOpen={saveModalOpen} onClose={onSaveModalClose} isCentered size="sm">
        <ModalOverlay bg="blackAlpha.600" />
        <ModalContent bg={useColorModeValue("white", "#1a1a1a")} border="1px solid" borderColor={useColorModeValue("gray.100", "#333")}>
          <ModalHeader fontSize="md" color={headingColor}>保存为预设</ModalHeader>
          <ModalCloseButton color={descColor} />
          <ModalBody>
            <VStack spacing={3}>
              <Text fontSize="sm" color={descColor}>
                将当前的频段设置保存为新预设
              </Text>
              <Input
                placeholder="输入预设名称"
                value={presetName}
                onChange={(e) => setPresetName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleSavePreset(); }}
                autoFocus
                focusBorderColor={activeColor}
              />
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button size="sm" variant="ghost" color={descColor} onClick={onSaveModalClose} mr={2}>
              取消
            </Button>
            <Button size="sm" bg={activeColor} color="white" _hover={{ opacity: 0.9 }} onClick={handleSavePreset} leftIcon={<Save size={14} />}>
              保存
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}
