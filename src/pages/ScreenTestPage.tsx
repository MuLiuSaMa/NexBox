import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Box,
  Button,
  Heading,
  HStack,
  IconButton,
  Slider,
  SliderFilledTrack,
  SliderThumb,
  SliderTrack,
  Switch,
  Tab,
  TabList,
  TabPanel,
  TabPanels,
  Tabs,
  Text,
  useColorModeValue,
  VStack,
} from "@chakra-ui/react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeft, Blend, Contrast, Expand, Eye, Gauge, Grid3x3, LayoutGrid, Moon, RefreshCw, Ruler, ScanLine, Shrink, SquareStack, Type, Waves } from "lucide-react";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";
import { useNavPosition } from "@/components/ui/main-layout";
import { hexToRgba } from "@/lib/color-utils";

/**
 * 从帧间隔样本中剔除异常值（掉帧 / 跨屏移动抖动）后取稳健均值，返回稳定 FPS 与帧时间(ms)。
 * 多屏不同刷新率下 rAF 间隔会偶发跳变，简单求平均会被离群帧拉偏，故先求中位数再保留邻近样本求均值。
 */
function stableFps(samples: number[]): { fps: number; frameMs: number } {
  if (samples.length === 0) return { fps: 0, frameMs: 0 };
  const sorted = [...samples].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  const good = sorted.filter((d) => d <= median * 1.8 && d >= median * 0.5);
  const use = good.length >= 3 ? good : sorted;
  const avg = use.reduce((s, d) => s + d, 0) / use.length;
  return { fps: Math.round(1000 / avg), frameMs: avg };
}

/**
 * 全屏能力：对容器 DOM 元素调用原生 Fullscreen API。
 * 进入全屏后该元素铺满整屏（侧边栏/导航被隐藏），ESC 由浏览器原生退出。
 */
function useFullscreen() {
  const ref = useRef<HTMLDivElement>(null);
  const [isFs, setIsFs] = useState(false);

  useEffect(() => {
    const onChange = () => setIsFs(document.fullscreenElement === ref.current);
    document.addEventListener("fullscreenchange", onChange);
    return () => document.removeEventListener("fullscreenchange", onChange);
  }, []);

  const enter = useCallback(() => {
    ref.current?.requestFullscreen?.().catch(() => {
      /* 用户手势缺失等情况静默失败 */
    });
  }, []);

  const exit = useCallback(() => {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
  }, []);

  return { ref, isFs, enter, exit };
}

/** 全屏测试舞台容器：统一处理全屏尺寸 / 圆角 / 比例，并淡显退出提示 */
function Stage({
  fsRef,
  isFs,
  active,
  bg,
  hint,
  children,
  onClick,
}: {
  fsRef: React.RefObject<HTMLDivElement>;
  isFs: boolean;
  active: boolean;
  bg?: string;
  hint?: string;
  children?: React.ReactNode;
  onClick?: () => void;
}) {
  return (
    <Box
      ref={fsRef}
      onClick={onClick}
      position="relative"
      w="100%"
      bg={bg}
      cursor={onClick ? "pointer" : "default"}
      overflow="hidden"
      userSelect="none"
      borderRadius={isFs ? 0 : "lg"}
      aspectRatio={isFs ? "auto" : "16 / 9"}
      minH={isFs ? "100vh" : "240px"}
    >
      {children}
      {active && hint && (
        <Text
          position="absolute"
          top={4}
          left="50%"
          transform="translateX(-50%)"
          fontSize="sm"
          color="rgba(255,255,255,0.55)"
          textShadow="0 1px 3px rgba(0,0,0,0.6)"
          pointerEvents="none"
          whiteSpace="nowrap"
        >
          {hint}
        </Text>
      )}
    </Box>
  );
}

/** 坏点检测：纯色全屏轮换，点击/方向键切换 */
function DeadPixelTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const colors = useMemo(
    () => ["#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#00ffff", "#ff00ff"],
    [],
  );
  const labels = [
    t("screenTest.colorBlack"),
    t("screenTest.colorWhite"),
    t("screenTest.colorRed"),
    t("screenTest.colorGreen"),
    t("screenTest.colorBlue"),
    t("screenTest.colorYellow"),
    t("screenTest.colorCyan"),
    t("screenTest.colorMagenta"),
  ];
  const [idx, setIdx] = useState(0);
  const cycle = useCallback(
    (dir: number) => setIdx((i) => (i + dir + colors.length) % colors.length),
    [colors.length],
  );

  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowRight" || e.key === "ArrowDown" || e.key === " ") {
        e.preventDefault();
        cycle(1);
      } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
        e.preventDefault();
        cycle(-1);
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [active, cycle]);

  return (
    <TestLayout
      stage={
        <Stage
          fsRef={ref}
          isFs={isFs}
          active={active}
          bg={colors[idx]}
          onClick={() => cycle(1)}
          hint={isFs ? t("screenTest.exitFullscreenHint") : undefined}
        />
      }
      controls={
        <>
          <HStack spacing={2} flexWrap="wrap">
            <Button size="sm" variant="outline" onClick={() => cycle(-1)}>
              {t("screenTest.prevColor")}
            </Button>
            <Text fontSize="sm" fontWeight="bold" minW="72px" textAlign="center">
              {labels[idx]}
            </Text>
            <Button size="sm" variant="outline" onClick={() => cycle(1)}>
              {t("screenTest.nextColor")}
            </Button>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.deadPixelDesc")}
    />
  );
}

/** 渐变断层：水平/垂直灰阶与彩色渐变，目测 banding */
function GradientTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const [vertical, setVertical] = useState(false);
  const [colorful, setColorful] = useState(false);

  const gradient = useMemo(() => {
    const dir = vertical ? "180deg" : "90deg";
    if (colorful) {
      return `linear-gradient(${dir}, #000 0%, #ff0000 16%, #ffff00 33%, #00ff00 50%, #00ffff 66%, #0000ff 83%, #ffffff 100%)`;
    }
    return `linear-gradient(${dir}, #000 0%, #fff 100%)`;
  }, [vertical, colorful]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg={gradient}>
          {!isFs && (
            <Box position="absolute" inset={0} display="flex" alignItems="center" justifyContent="center">
              <Text fontSize="sm" color="rgba(255,255,255,0.5)" textShadow="0 1px 3px rgba(0,0,0,0.7)">
                {t("screenTest.gradientDesc")}
              </Text>
            </Box>
          )}
        </Stage>
      }
      controls={
        <>
          <HStack spacing={4}>
            <HStack spacing={2}>
              <Switch size="sm" isChecked={vertical} onChange={(e) => setVertical(e.target.checked)} />
              <Text fontSize="sm">{t("screenTest.vertical")}</Text>
            </HStack>
            <HStack spacing={2}>
              <Switch size="sm" isChecked={colorful} onChange={(e) => setColorful(e.target.checked)} />
              <Text fontSize="sm">{t("screenTest.colorful")}</Text>
            </HStack>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.gradientDesc")}
    />
  );
}

/** 响应时间（UFO 式）：深色方块中灰背景高速横移，实时帧率读数 */
function ResponseTimeTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const { getActiveColor } = useThemeColor();
  const activeColor = getActiveColor();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [fps, setFps] = useState(0);
  const [speed, setSpeed] = useState(1200); // css px / 秒
  const speedRef = useRef(speed);
  speedRef.current = speed;

  useEffect(() => {
    if (!active) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;

    let raf = 0;
    let x = 0;
    let last = performance.now();
    const buf: number[] = [];

    const resize = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const loop = (now: number) => {
      const dt = now - last;
      last = now;
      buf.push(dt);
      if (buf.length >= 60) {
        setFps(stableFps(buf).fps);
        buf.length = 0;
      }
      const w = canvas.width;
      const h = canvas.height;
      ctx.fillStyle = "#808080";
      ctx.fillRect(0, 0, w, h);
      const size = Math.round(h * 0.16);
      x += (speedRef.current / 1000) * dt * dpr;
      if (x > w + size) x = -size;
      ctx.fillStyle = "#000000";
      ctx.fillRect(x, (h - size) / 2, size, size);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [active]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#808080">
          <Box as="canvas" ref={canvasRef} position="absolute" inset={0} w="100%" h="100%" />
          <Text
            position="absolute"
            top={3}
            right={4}
            fontSize={isFs ? "lg" : "sm"}
            fontWeight="bold"
            color="rgba(255,255,255,0.85)"
            textShadow="0 1px 3px rgba(0,0,0,0.7)"
            pointerEvents="none"
          >
            {fps} FPS
          </Text>
        </Stage>
      }
      controls={
        <>
          <HStack spacing={3} w="100%" maxW="320px">
            <Text fontSize="sm" whiteSpace="nowrap">
              {t("screenTest.speed")}
            </Text>
            <Slider value={speed} min={400} max={4000} step={100} onChange={setSpeed} flex={1}>
              <SliderTrack>
                <SliderFilledTrack bg={activeColor} />
              </SliderTrack>
              <SliderThumb bg={activeColor} borderColor={activeColor} boxSize={4} />
            </Slider>
            <Text fontSize="sm" whiteSpace="nowrap" minW="64px">
              {speed} px/s
            </Text>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.responseTimeDesc")}
    />
  );
}

/** 原生上报的单显示器当前模式（get_screen_modes） */
interface ScreenMode {
  device_name: string;
  model: string;
  is_primary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  refresh_rate: number;
}

/** 刷新率：原生读取主显示器的设置刷新率（权威）+ rAF 实测合成帧率对照 */
function RefreshRateTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [fps, setFps] = useState(0);
  const [frameMs, setFrameMs] = useState(0);
  const [configuredHz, setConfiguredHz] = useState(0);

  // 主显示器的设置刷新率（EnumDisplaySettingsW 权威值）
  useEffect(() => {
    if (!active) return;
    invoke<ScreenMode[]>("get_screen_modes")
      .then((list) => {
        const primary = list.find((m) => m.is_primary) || list[0];
        setConfiguredHz(primary?.refresh_rate ?? 0);
      })
      .catch(() => {
        /* 非 Windows 或命令失败时静默，仅显示 rAF 实测 */
      });
  }, [active]);

  useEffect(() => {
    if (!active) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;

    let raf = 0;
    let last = performance.now();
    const buf: number[] = [];
    let phase = 0;

    const resize = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const loop = (now: number) => {
      const dt = now - last;
      last = now;
      buf.push(dt);
      if (buf.length >= 60) {
        const s = stableFps(buf);
        setFps(s.fps);
        setFrameMs(s.frameMs);
        buf.length = 0;
      }
      const w = canvas.width;
      const h = canvas.height;
      ctx.fillStyle = "#000000";
      ctx.fillRect(0, 0, w, h);
      // 匀速扫描亮条
      phase += (dt / 2000) * w;
      if (phase > w) phase = -w * 0.06;
      const barW = Math.round(w * 0.06);
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(phase, 0, barW, h);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [active]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#000000">
          <Box as="canvas" ref={canvasRef} position="absolute" inset={0} w="100%" h="100%" />
          <VStack
            position="absolute"
            bottom={4}
            left={4}
            align="start"
            spacing={0}
            pointerEvents="none"
          >
            <Text fontSize={isFs ? "4xl" : "2xl"} fontWeight="bold" color="#fff" lineHeight="1">
              {fps} Hz
            </Text>
            <Text fontSize="sm" color="rgba(255,255,255,0.7)">
              {configuredHz > 0 ? `${t("screenTest.configured")} ${configuredHz} Hz` : `${frameMs.toFixed(2)} ms`}
            </Text>
          </VStack>
        </Stage>
      }
      controls={<FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />}
      desc={
        configuredHz > 0
          ? `${t("screenTest.refreshRateDesc")} · ${t("screenTest.multiMonitorNote")}`
          : t("screenTest.refreshRateDesc")
      }
    />
  );
}

/** 均匀性：纯黑/纯白全屏 + 可选网格参考线，观察漏光/亮度差异 */
function UniformityTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const [white, setWhite] = useState(false);
  const [grid, setGrid] = useState(false);

  const gridStyle = useMemo(() => {
    if (!grid) return undefined;
    const line = white ? "rgba(0,0,0,0.18)" : "rgba(255,255,255,0.18)";
    return {
      backgroundImage: `linear-gradient(${line} 1px, transparent 1px), linear-gradient(90deg, ${line} 1px, transparent 1px)`,
      backgroundSize: "20% 25%",
    } as const;
  }, [grid, white]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg={white ? "#ffffff" : "#000000"}>
          {grid && <Box position="absolute" inset={0} style={gridStyle} pointerEvents="none" />}
        </Stage>
      }
      controls={
        <>
          <HStack spacing={4}>
            <HStack spacing={2}>
              <Switch size="sm" isChecked={white} onChange={(e) => setWhite(e.target.checked)} />
              <Text fontSize="sm">{t("screenTest.whiteField")}</Text>
            </HStack>
            <HStack spacing={2}>
              <Switch size="sm" isChecked={grid} onChange={(e) => setGrid(e.target.checked)} />
              <Text fontSize="sm">{t("screenTest.grid")}</Text>
            </HStack>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.uniformityDesc")}
    />
  );
}

/** 画面撕裂：高速纵向滚动的粗条纹 + 竖网格，观察撕裂错位 */
function TearingTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!active) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    let raf = 0;
    let offset = 0;
    let last = performance.now();

    const resize = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const loop = (now: number) => {
      const dt = now - last;
      last = now;
      const w = canvas.width;
      const h = canvas.height;
      const band = 44 * dpr;
      const period = band * 2;
      offset = (offset + (dt / 1000) * 520 * dpr) % period;
      ctx.fillStyle = "#111111";
      ctx.fillRect(0, 0, w, h);
      ctx.fillStyle = "#f2f2f2";
      for (let y = -period + offset; y < h; y += period) ctx.fillRect(0, y, w, band);
      ctx.fillStyle = "#00a0ff";
      for (let x = 0; x < w; x += 140 * dpr) ctx.fillRect(x, 0, Math.max(1, Math.round(dpr)), h);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [active]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#111111">
          <Box as="canvas" ref={canvasRef} position="absolute" inset={0} w="100%" h="100%" />
        </Stage>
      }
      controls={<FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />}
      desc={t("screenTest.tearingDesc")}
    />
  );
}

/** 子像素排列：按物理像素逐列画纯 R/G/B 竖条纹，判断排列方向与 ClearType 对齐 */
function SubpixelTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (!active) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;

    const draw = () => {
      const r = canvas.getBoundingClientRect();
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
      const w = canvas.width;
      const h = canvas.height;
      for (let x = 0; x < w; x++) {
        const m = x % 3;
        ctx.fillStyle = m === 0 ? "#ff0000" : m === 1 ? "#00ff00" : "#0000ff";
        ctx.fillRect(x, 0, 1, h);
      }
    };
    draw();
    const ro = new ResizeObserver(draw);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [active]);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#000000">
          <Box as="canvas" ref={canvasRef} position="absolute" inset={0} w="100%" h="100%" />
        </Stage>
      }
      controls={<FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />}
      desc={t("screenTest.subpixelDesc")}
    />
  );
}

/** 黑阶暗部细节：从纯黑递增的深灰块阵，检测黑位抬升与暗部死黑 */
function BlackLevelTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const cells = useMemo(() => Array.from({ length: 12 }, (_, i) => Math.round((i / 11) * 30)), []);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#000000">
          <Box
            position="absolute"
            inset={0}
            style={{ display: "grid", gridTemplateColumns: "repeat(6, 1fr)", gridTemplateRows: "repeat(2, 1fr)" }}
          >
            {cells.map((v, i) => (
              <Box key={i} bg={`rgb(${v},${v},${v})`} />
            ))}
          </Box>
        </Stage>
      }
      controls={<FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />}
      desc={t("screenTest.blackLevelDesc")}
    />
  );
}

/** 灰阶对比度阶梯：0-255 等差灰块，可切换白/黑底，级数反映灰阶对比度 */
function ContrastStepsTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const [white, setWhite] = useState(false);
  const cells = useMemo(() => Array.from({ length: 20 }, (_, i) => Math.round((i * 255) / 19)), []);

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg={white ? "#ffffff" : "#000000"}>
          <Box
            position="absolute"
            inset={0}
            style={{ display: "grid", gridTemplateColumns: "repeat(10, 1fr)", gridTemplateRows: "repeat(2, 1fr)" }}
          >
            {cells.map((v, i) => (
              <Box key={i} bg={`rgb(${v},${v},${v})`} outline={white ? "1px solid rgba(0,0,0,0.15)" : "1px solid rgba(255,255,255,0.1)"} />
            ))}
          </Box>
        </Stage>
      }
      controls={
        <>
          <HStack spacing={2}>
            <Switch size="sm" isChecked={white} onChange={(e) => setWhite(e.target.checked)} />
            <Text fontSize="sm">{t("screenTest.whiteField")}</Text>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.contrastStepsDesc")}
    />
  );
}

/** 文本清晰度：多字号中英文样本行，可反色，判边缘发虚与彩边 */
function TextClarityTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const [invert, setInvert] = useState(false);
  const sizes = useMemo(() => [12, 14, 16, 18, 22, 28], []);
  const fg = invert ? "#ffffff" : "#000000";

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg={invert ? "#000000" : "#ffffff"}>
          <VStack position="absolute" inset={0} align="start" justify="center" spacing={1} px={6} color={fg}>
            {sizes.map((s) => (
              <Text key={s} fontSize={`${s}px`} lineHeight="1.3" style={{ fontVariantNumeric: "tabular-nums" }}>
                {s}px · The quick brown fox 永字八法 Ag01 · 高清文本
              </Text>
            ))}
          </VStack>
        </Stage>
      }
      controls={
        <>
          <HStack spacing={2}>
            <Switch size="sm" isChecked={invert} onChange={(e) => setInvert(e.target.checked)} />
            <Text fontSize="sm">{t("screenTest.invert")}</Text>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.textClarityDesc")}
    />
  );
}

/** 几何直线：全屏横竖细网格 + 两条对角线，黑白可切换，观察线条抖动/弯曲 */
function GeometryTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const [white, setWhite] = useState(false);
  const line = white ? "#000000" : "#ffffff";
  const bg = white ? "#ffffff" : "#000000";

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg={bg}>
          <Box
            position="absolute"
            inset={0}
            style={{
              backgroundImage: `repeating-linear-gradient(0deg, ${line} 0 1px, transparent 1px 48px), repeating-linear-gradient(90deg, ${line} 0 1px, transparent 1px 48px)`,
            }}
          />
          <Box
            position="absolute"
            top="50%"
            left="-25%"
            right="-25%"
            h="1px"
            bg={line}
            style={{ transform: "rotate(45deg)" }}
          />
          <Box
            position="absolute"
            top="50%"
            left="-25%"
            right="-25%"
            h="1px"
            bg={line}
            style={{ transform: "rotate(-45deg)" }}
          />
        </Stage>
      }
      controls={
        <>
          <HStack spacing={2}>
            <Switch size="sm" isChecked={white} onChange={(e) => setWhite(e.target.checked)} />
            <Text fontSize="sm">{t("screenTest.whiteField")}</Text>
          </HStack>
          <FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />
        </>
      }
      desc={t("screenTest.geometryDesc")}
    />
  );
}

/** 可视角度色偏：四角 + 中心相同色块簇，观察边缘色相偏移 */
function ViewingAngleTest({ active }: { active: boolean }) {
  const { t } = useTranslation();
  const { ref, isFs, enter, exit } = useFullscreen();
  const swatches = useMemo(() => ["#808080", "#ff0000", "#00ff00", "#0000ff"], []);
  const positions = useMemo<React.CSSProperties[]>(
    () => [
      { top: 8, left: 8 },
      { top: 8, right: 8 },
      { bottom: 8, left: 8 },
      { bottom: 8, right: 8 },
      { top: "50%", left: "50%", transform: "translate(-50%, -50%)" },
    ],
    [],
  );

  return (
    <TestLayout
      stage={
        <Stage fsRef={ref} isFs={isFs} active={active} bg="#000000">
          {positions.map((pos, i) => (
            <Box key={i} position="absolute" style={pos}>
              <HStack spacing={0}>
                {swatches.map((c) => (
                  <Box key={c} w="56px" h="56px" bg={c} />
                ))}
              </HStack>
            </Box>
          ))}
        </Stage>
      }
      controls={<FullscreenButton isFs={isFs} onEnter={enter} onExit={exit} />}
      desc={t("screenTest.viewingAngleDesc")}
    />
  );
}

/** 进入/退出全屏按钮 */
function FullscreenButton({
  isFs,
  onEnter,
  onExit,
}: {
  isFs: boolean;
  onEnter: () => void;
  onExit: () => void;
}) {
  const { t } = useTranslation();
  return (
    <Button
      size="sm"
      leftIcon={isFs ? <Shrink size={14} /> : <Expand size={14} />}
      onClick={isFs ? onExit : onEnter}
    >
      {isFs ? t("screenTest.exitFullscreen") : t("screenTest.startFullscreen")}
    </Button>
  );
}

/** 单个检测项的通用布局：舞台 + 控制条 + 说明 */
function TestLayout({
  stage,
  controls,
  desc,
}: {
  stage: React.ReactNode;
  controls: React.ReactNode;
  desc: string;
}) {
  const descColor = useColorModeValue("gray.500", "gray.400");
  return (
    <VStack align="stretch" spacing={4}>
      {stage}
      <HStack justify="space-between" align="center" flexWrap="wrap" spacing={3}>
        {controls}
      </HStack>
      <Text fontSize="sm" color={descColor}>
        {desc}
      </Text>
    </VStack>
  );
}

export default function ScreenTestPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { config: themeConfig, getActiveColor, getContrastTextColor } = useThemeColor();
  const activeColor = getActiveColor();
  const themeColorHex = themeConfig.primaryColor;
  const themeColorRgba = (opacity: number) => hexToRgba(themeColorHex, opacity);
  const adaptiveTitle = useAdaptiveTextColor();
  const subColor = useColorModeValue("gray.500", "#888888");
  const navPosition = useNavPosition();
  const fillMinH = navPosition === "top" ? "calc(100vh - 152px)" : "calc(100vh - 88px)";

  const [tabIndex, setTabIndex] = useState(0);

  const tabs = [
    { key: "deadPixel", icon: Contrast, node: <DeadPixelTest active={tabIndex === 0} /> },
    { key: "gradient", icon: Blend, node: <GradientTest active={tabIndex === 1} /> },
    { key: "responseTime", icon: Gauge, node: <ResponseTimeTest active={tabIndex === 2} /> },
    { key: "refreshRate", icon: RefreshCw, node: <RefreshRateTest active={tabIndex === 3} /> },
    { key: "uniformity", icon: LayoutGrid, node: <UniformityTest active={tabIndex === 4} /> },
    { key: "tearing", icon: Waves, node: <TearingTest active={tabIndex === 5} /> },
    { key: "subpixel", icon: Grid3x3, node: <SubpixelTest active={tabIndex === 6} /> },
    { key: "blackLevel", icon: Moon, node: <BlackLevelTest active={tabIndex === 7} /> },
    { key: "contrastSteps", icon: SquareStack, node: <ContrastStepsTest active={tabIndex === 8} /> },
    { key: "textClarity", icon: Type, node: <TextClarityTest active={tabIndex === 9} /> },
    { key: "geometry", icon: Ruler, node: <GeometryTest active={tabIndex === 10} /> },
    { key: "viewingAngle", icon: Eye, node: <ViewingAngleTest active={tabIndex === 11} /> },
  ];

  return (
    <Box pt={2} minH={fillMinH} display="flex" flexDirection="column">
      <VStack align="stretch" spacing={4} flex={1}>
        <HStack spacing={3}>
          <IconButton
            aria-label="back"
            icon={<ArrowLeft size={20} />}
            variant="ghost"
            onClick={() => navigate(-1)}
          />
          <HStack spacing={2}>
            <ScanLine color={activeColor} />
            <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow}>
              {t("screenTest.title")}
            </Heading>
          </HStack>
        </HStack>
        <Text fontSize="sm" color={subColor}>
          {t("screenTest.desc")}
        </Text>

        <Tabs variant="soft-rounded" index={tabIndex} onChange={setTabIndex} isLazy>
          <TabList gap={2} mb={4} flexWrap="wrap">
            {tabs.map((tab) => {
              const Icon = tab.icon;
              return (
                <Tab
                  key={tab.key}
                  _selected={{
                    bg: themeColorHex,
                    color: getContrastTextColor(),
                    boxShadow: `0 2px 14px -3px ${themeColorRgba(0.5)}`,
                    _hover: { bg: themeColorHex },
                  }}
                  _hover={{ bg: themeColorRgba(0.15) }}
                  borderRadius="full"
                  fontWeight="600"
                  fontSize="sm"
                  px={5}
                  py={1.5}
                >
                  <Icon size={15} style={{ marginRight: 6 }} />
                  {t(`screenTest.tabs.${tab.key}`)}
                </Tab>
              );
            })}
          </TabList>
          <TabPanels>
            {tabs.map((tab) => (
              <TabPanel key={tab.key} px={0} pt={0}>
                {tab.node}
              </TabPanel>
            ))}
          </TabPanels>
        </Tabs>
      </VStack>
    </Box>
  );
}
