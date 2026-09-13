import {
  Box,
  Text,
  Heading,
  VStack,
  HStack,
  Badge,
  IconButton,
  Divider,
  Slider,
  SliderTrack,
  SliderFilledTrack,
  SliderThumb,
  useColorModeValue,
} from "@chakra-ui/react";
import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { ArrowLeft, Zap } from "lucide-react";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { ThemeSwitch } from "@/components/special/theme-switch";
import { keyToHotkeyFormat } from "@/components/hotkey-recorder";
import { CustomSelect } from "@/components/special/custom-select";
import { useDynamicIsland } from "@/components/ui/dynamic-island";

// 分贝（-60..0）→ 音量条百分比
const dbToPct = (db: number) => Math.max(0, Math.min(100, ((db + 60) / 60) * 100));

// ── 任意键录制器：支持键盘任意键（含 Ctrl/Shift/Alt 单独绑定）与鼠标全部按键 ──
const MOUSE_TOKEN_KEYS: Record<string, string> = {
  MouseLeft: "crosshair.mouseLeft",
  MouseRight: "crosshair.mouseRight",
  MouseMiddle: "crosshair.mouseMiddle",
  MouseX1: "crosshair.mouseX1",
  MouseX2: "crosshair.mouseX2",
};

const MOUSE_BUTTON_TOKENS: Record<number, string> = {
  0: "MouseLeft",
  1: "MouseMiddle",
  2: "MouseRight",
  3: "MouseX1",
  4: "MouseX2",
};

const MODIFIER_TOKENS: Record<string, string> = {
  Control: "Ctrl",
  Shift: "Shift",
  Alt: "Alt",
  Meta: "Meta",
};

function AnyKeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (val: string) => void;
}) {
  const { t } = useTranslation();
  const [isRecording, setIsRecording] = useState(false);
  const justCommittedRef = useRef(false);
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { getActiveColor, getHoverColor } = useThemeColor();
  const recordBg = getHoverColor();
  const recordBorder = getActiveColor();

  useEffect(() => {
    if (!isRecording) return;

    const commit = (token: string) => {
      onChange(token);
      setIsRecording(false);
      justCommittedRef.current = true;
    };

    const onMouseDown = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const token = MOUSE_BUTTON_TOKENS[e.button];
      if (token) commit(token);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setIsRecording(false);
        return;
      }
      // 修饰键单独绑定（Ctrl/Shift/Alt/Meta），其余键沿用通用映射
      const token = MODIFIER_TOKENS[e.key] ?? keyToHotkeyFormat(e.key);
      if (token) commit(token);
    };

    window.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      window.removeEventListener("mousedown", onMouseDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [isRecording, onChange]);

  const startRecording = useCallback(() => {
    // 若刚通过鼠标/键盘录制提交（该次点击的 click 事件），忽略避免立即重新进入录制
    if (justCommittedRef.current) {
      justCommittedRef.current = false;
      return;
    }
    setIsRecording(true);
  }, []);

  const displayValue = (v: string) => (MOUSE_TOKEN_KEYS[v] ? t(MOUSE_TOKEN_KEYS[v]) : v);

  return (
    <Box
      role="button"
      cursor="pointer"
      onClick={startRecording}
      onContextMenu={(e) => isRecording && e.preventDefault()}
      px={3}
      py={2}
      borderRadius="lg"
      border="2px solid"
      borderColor={isRecording ? recordBorder : borderColor}
      bg={isRecording ? recordBg : "transparent"}
      transition="all 0.2s"
      _hover={{ borderColor: recordBorder }}
      outline="none"
      minW="150px"
      textAlign="center"
      userSelect="none"
    >
      {isRecording ? (
        <Text color={recordBorder} fontSize="sm" fontWeight="medium">
          {t("deltaForce.voiceStrobe.recording")}
        </Text>
      ) : (
        <Text color={textColor} fontSize="sm" fontWeight="medium">
          {value ? displayValue(value) : t("deltaForce.voiceStrobe.keyNone")}
        </Text>
      )}
    </Box>
  );
}

interface VoiceStrobeDevice {
  id: string;
  name: string;
}

interface VoiceStrobeStatus {
  enabled: boolean;
  key: string;
  threshold_db: number;
  cooldown_ms: number;
  game_running: boolean;
  device_id: string;
}

export default function VoiceStrobePage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const toast = useDynamicIsland("target");
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const adaptiveTitle = useAdaptiveTextColor();
  const subTextColor = useColorModeValue("#000000", "#ffffff");
  const textColor = useColorModeValue("#000000", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const meterBg = useColorModeValue("gray.200", "#2a2a2a");
  const markerColor = useColorModeValue("rgba(0,0,0,0.55)", "rgba(255,255,255,0.75)");
  const pageBg = useColorModeValue("white", "#111111");
  const { liquidGlassEnabled } = useBackground();

  const [enabled, setEnabled] = useState(false);
  const [strobeKey, setStrobeKey] = useState("U");
  const [thresholdDb, setThresholdDb] = useState(-30);
  const [cooldownMs, setCooldownMs] = useState(0);
  const [gameRunning, setGameRunning] = useState(false);
  const [devices, setDevices] = useState<VoiceStrobeDevice[]>([]);
  const [deviceId, setDeviceId] = useState("");
  const [db, setDb] = useState(-100);
  const [triggered, setTriggered] = useState(false);
  const triggeredTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const status = await invoke<VoiceStrobeStatus>("voice_strobe_get_status");
      setEnabled(status.enabled);
      setStrobeKey(status.key);
      setThresholdDb(status.threshold_db);
      setCooldownMs(status.cooldown_ms);
      setGameRunning(status.game_running);
      setDeviceId(status.device_id);
    } catch {
      // silent
    }
  }, []);

  const loadDevices = useCallback(async () => {
    try {
      const list = await invoke<VoiceStrobeDevice[]>("voice_strobe_list_devices");
      setDevices(list);
    } catch {
      // silent
    }
  }, []);

  // 页面挂载期间：订阅音量事件（~20Hz）+ 轮询游戏运行状态 + 加载麦克风列表
  useEffect(() => {
    refreshStatus();
    loadDevices();
    let disposed = false;
    let unlisten: (() => void) | undefined;

    listen<{ db: number; triggered: boolean }>("voice-strobe-level", (e) => {
      setDb(e.payload.db);
      if (e.payload.triggered) {
        setTriggered(true);
        if (triggeredTimer.current) clearTimeout(triggeredTimer.current);
        triggeredTimer.current = setTimeout(() => setTriggered(false), 400);
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    const interval = setInterval(refreshStatus, 2000);
    return () => {
      disposed = true;
      unlisten?.();
      clearInterval(interval);
      if (triggeredTimer.current) clearTimeout(triggeredTimer.current);
    };
  }, [refreshStatus]);

  const updateConfig = useCallback(
    async (next: {
      key?: string;
      threshold_db?: number;
      cooldown_ms?: number;
      device_id?: string;
    }) => {
      try {
        await invoke("voice_strobe_update_config", {
          key: next.key ?? strobeKey,
          thresholdDb: next.threshold_db ?? thresholdDb,
          cooldownMs: next.cooldown_ms ?? cooldownMs,
          deviceId: next.device_id ?? deviceId,
        });
      } catch (err) {
        toast({
          title: String(err),
          status: "error",
          duration: 2000,
          isClosable: true,
        });
      }
    },
    [strobeKey, thresholdDb, cooldownMs, deviceId, toast],
  );

  const toggleEnabled = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const next = e.target.checked;
      setEnabled(next);
      try {
        await invoke("voice_strobe_set_enabled", { enabled: next });
        if (!next) {
          setGameRunning(false);
          setDb(-100);
        }
      } catch (err) {
        setEnabled(!next);
        toast({
          title: String(err),
          status: "error",
          duration: 2000,
          isClosable: true,
        });
      }
    },
    [toast],
  );

  const meterPct = dbToPct(enabled ? db : -100);
  const thresholdPct = dbToPct(thresholdDb);

  const statusBadge = !enabled ? (
    <Badge colorScheme="gray" fontSize="xs">
      {t("deltaForce.voiceStrobe.statusOff")}
    </Badge>
  ) : gameRunning ? (
    <Badge colorScheme="green" fontSize="xs">
      {t("deltaForce.voiceStrobe.statusListening")}
    </Badge>
  ) : (
    <Badge colorScheme="yellow" fontSize="xs">
      {t("deltaForce.voiceStrobe.statusWaitingGame")}
    </Badge>
  );

  const settingsContent = (
    <VStack spacing={6} align="stretch">
      {/* 总开关 */}
      <HStack justify="space-between">
        <VStack align="start" spacing={0}>
          <Text fontSize="sm" fontWeight="medium" color={textColor}>
            {t("deltaForce.voiceStrobe.enabled")}
          </Text>
          <Text fontSize="xs" color={subTextColor}>
            {t("deltaForce.voiceStrobe.enabledDesc")}
          </Text>
        </VStack>
        <ThemeSwitch isChecked={enabled} onChange={toggleEnabled} />
      </HStack>

      <Divider borderColor={borderColor} />

      {/* 实时音量条 + 阈值标记 */}
      <VStack align="stretch" spacing={2}>
        <HStack justify="space-between">
          <Text fontSize="sm" fontWeight="medium" color={textColor}>
            {t("deltaForce.voiceStrobe.volumeMeter")}
          </Text>
          {statusBadge}
        </HStack>
        <Box position="relative" h="14px" borderRadius="full" bg={meterBg} overflow="hidden">
          <Box
            position="absolute"
            left={0}
            top={0}
            bottom={0}
            width={`${meterPct}%`}
            borderRadius="full"
            bg={triggered ? "green.400" : primaryColor}
            transition="width 0.08s linear"
          />
          <Box
            position="absolute"
            left={`${thresholdPct}%`}
            top={0}
            bottom={0}
            width="2px"
            bg={markerColor}
          />
        </Box>
        <HStack justify="space-between">
          <Text fontSize="xs" color={subTextColor}>
            {db <= -99 ? "-∞" : `${db.toFixed(0)} dB`}
          </Text>
          <Text fontSize="xs" color={subTextColor}>
            {t("deltaForce.voiceStrobe.threshold")}: {thresholdDb} dB
          </Text>
        </HStack>
        <Text fontSize="xs" color={subTextColor}>
          {t("deltaForce.voiceStrobe.volumeHint")}
        </Text>
      </VStack>

      <Divider borderColor={borderColor} />

      {/* 麦克风设备选择 */}
      <VStack align="stretch" spacing={2}>
        <Text fontSize="sm" fontWeight="medium" color={textColor}>
          {t("deltaForce.voiceStrobe.device")}
        </Text>
        <CustomSelect
          value={deviceId}
          onChange={(val: string) => {
            setDeviceId(val);
            updateConfig({ device_id: val });
          }}
          options={[
            { value: "", label: t("deltaForce.voiceStrobe.deviceDefault") },
            ...devices.map((d) => ({ value: d.id, label: d.name })),
          ]}
          width="100%"
          placeholder={t("deltaForce.voiceStrobe.deviceDefault")}
        />
      </VStack>

      <Divider borderColor={borderColor} />

      {/* 阈值滑杆 */}
      <VStack align="stretch" spacing={2}>
        <HStack justify="space-between">
          <Text fontSize="sm" fontWeight="medium" color={textColor}>
            {t("deltaForce.voiceStrobe.threshold")}
          </Text>
          <Text fontSize="sm" color={primaryColor} fontWeight="bold">
            {thresholdDb} dB
          </Text>
        </HStack>
        <Slider
          min={-60}
          max={0}
          step={1}
          value={thresholdDb}
          onChange={(v: number) => setThresholdDb(v)}
          onChangeEnd={(v: number) => {
            setThresholdDb(v);
            updateConfig({ threshold_db: v });
          }}
        >
          <SliderTrack>
            <SliderFilledTrack bg={primaryColor} />
          </SliderTrack>
          <SliderThumb aria-label="threshold" />
        </Slider>
      </VStack>

      {/* 爆闪键 */}
      <HStack justify="space-between">
        <VStack align="start" spacing={0}>
          <Text fontSize="sm" fontWeight="medium" color={textColor}>
            {t("deltaForce.voiceStrobe.key")}
          </Text>
          <Text fontSize="xs" color={subTextColor}>
            {t("deltaForce.voiceStrobe.keyHint")}
          </Text>
        </VStack>
        <Box maxW="170px">
          <AnyKeyRecorder
            value={strobeKey}
            onChange={(val) => {
              setStrobeKey(val);
              updateConfig({ key: val });
            }}
          />
        </Box>
      </HStack>

      {/* 冷却时间 */}
      <VStack align="stretch" spacing={2}>
        <HStack justify="space-between">
          <VStack align="start" spacing={0}>
            <Text fontSize="sm" fontWeight="medium" color={textColor}>
              {t("deltaForce.voiceStrobe.cooldown")}
            </Text>
            <Text fontSize="xs" color={subTextColor}>
              {t("deltaForce.voiceStrobe.cooldownHint")}
            </Text>
          </VStack>
          <Text fontSize="sm" color={primaryColor} fontWeight="bold">
            {(cooldownMs / 1000).toFixed(1)} {t("deltaForce.voiceStrobe.cooldownUnit")}
          </Text>
        </HStack>
        <Slider
          min={0}
          max={3000}
          step={100}
          value={cooldownMs}
          onChange={(v: number) => setCooldownMs(v)}
          onChangeEnd={(v: number) => {
            setCooldownMs(v);
            updateConfig({ cooldown_ms: v });
          }}
        >
          <SliderTrack>
            <SliderFilledTrack bg={primaryColor} />
          </SliderTrack>
          <SliderThumb aria-label="cooldown" />
        </Slider>
      </VStack>
    </VStack>
  );

  return (
    <Box pt={8} pb={8}>
      {/* 头部：返回 + 标题 */}
      <HStack spacing={3} mb={2}>
        <IconButton
          aria-label="返回三角洲专区"
          icon={<ArrowLeft size={20} />}
          variant="ghost"
          color={textColor}
          onClick={() => navigate("/delta-force")}
        />
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
          {t("deltaForce.voiceStrobe.title")}
        </Heading>
      </HStack>
      <HStack spacing={2} mb={6}>
        <Zap size={16} color={primaryColor} />
        <Text fontSize="sm" color={subTextColor}>
          {t("deltaForce.voiceStrobe.modalDesc")}
        </Text>
      </HStack>

      {/* 设置卡片 */}
      {liquidGlassEnabled ? (
        <LiquidGlassCard p={5}>{settingsContent}</LiquidGlassCard>
      ) : (
        <Box bg={pageBg} borderRadius="xl" p={5} border="1px solid" borderColor={borderColor}>
          {settingsContent}
        </Box>
      )}
    </Box>
  );
}
