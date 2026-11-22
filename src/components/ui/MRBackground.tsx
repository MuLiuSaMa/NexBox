"use client";

import { Box } from "@chakra-ui/react";
import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * MR 动态背景（独立背景模式）。
 *
 * 复刻 Mineradio（MR）播放器的动态霓虹背景：
 * 深色底 + 三色霓虹光线流的全屏 WebGL 动画，持续循环。
 * 颜色由外部通过 `colors` prop 传入（适配主题色或自定义颜色派生而来）。
 *
 * 细节：
 * - 持续环境动画（去掉启动页的一次性 intro/climax 包络）
 * - 窗口隐藏/最小化/进托盘时暂停渲染，恢复显示后继续（节省 CPU）
 * - 系统开启「减少动态效果」或 WebGL 不可用时降级为静态深色底
 * - StrictMode 兼容：不调用 loseContext()，卸载即移除 canvas 释放 GPU
 */

type Rgb = [number, number, number];

export interface MRBackgroundColors {
  c1: Rgb;
  c2: Rgb;
  c3: Rgb;
}

const VERTEX_SOURCE = [
  "attribute vec2 aPosition;",
  "varying vec2 vUv;",
  "void main(){",
  "  vUv = aPosition * 0.5 + 0.5;",
  "  gl_Position = vec4(aPosition, 0.0, 1.0);",
  "}",
].join("\n");

const FRAGMENT_SOURCE = [
  "precision highp float;",
  "varying vec2 vUv;",
  "uniform vec2 uResolution;",
  "uniform float uTime;",
  "uniform vec3 uColor1;",
  "uniform vec3 uColor2;",
  "uniform vec3 uColor3;",
  "",
  "float saturate(float v){ return clamp(v, 0.0, 1.0); }",
  "float ease(float v){ v = saturate(v); return v * v * (3.0 - 2.0 * v); }",
  "mat2 rot(float a){ float c = cos(a); float s = sin(a); return mat2(c, -s, s, c); }",
  "float hash(vec2 p){ return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123); }",
  "float noise(vec2 p){",
  "  vec2 i = floor(p);",
  "  vec2 f = fract(p);",
  "  vec2 u = f * f * (3.0 - 2.0 * f);",
  "  return mix(mix(hash(i), hash(i + vec2(1.0,0.0)), u.x), mix(hash(i + vec2(0.0,1.0)), hash(i + vec2(1.0,1.0)), u.x), u.y);",
  "}",
  "",
  "float animatedLoop(vec2 uv, float t, float channel){",
  "  vec2 q = uv;",
  "  q *= rot(0.28 + sin(t * 0.18) * 0.12);",
  "  q.x += 0.055 * sin(t * 0.30 + channel);",
  "  q.y += 0.040 * cos(t * 0.24 + channel * 1.7);",
  "  float ang = atan(q.y, q.x);",
  "  float angularShift = sin(ang * 3.0 + t * 0.72 + channel * 1.9) * 0.078;",
  "  angularShift += sin(ang * 7.0 - t * 0.54 + channel) * 0.020;",
  "  float neonD = length(q) + angularShift;",
  "  float warpD = length(q * vec2(1.34 + 0.06 * sin(t * 0.25), 0.82 + 0.04 * cos(t * 0.31)));",
  "  warpD += 0.026 * sin(q.x * 4.4 + t * 0.62) + 0.018 * sin(q.y * 5.2 - t * 0.45);",
  "  float diamondD = abs(q.x) * 1.20 + abs(q.y) * 0.84;",
  "  float d = mix(warpD, diamondD, 0.32);",
  "  d = mix(d, neonD, 0.20 + 0.04 * sin(t * 0.18 + channel));",
  "  float pattern = mod((q.x + q.y) * 0.62 + sin(q.x * 5.5 + t) * 0.015 + sin(q.y * 7.0 - t * 0.75) * 0.012, 0.20);",
  "  float acc = 0.0;",
  "  for (int i = 1; i <= 6; i++) {",
  "    float fi = float(i);",
  "    float f = fract(t * 0.152 - channel * 0.018 + 0.011 * fi) * 4.70 - d + pattern;",
  "    acc += 0.00110 * fi * fi / max(abs(f), 0.0065);",
  "  }",
  "  float threadCoord = q.x * 0.92 - q.y * 0.58 + 0.030 * sin(q.x * 5.2 + t * 0.72);",
  "  float threadLines = 0.0065 / max(abs(sin((threadCoord + t * 0.10 + channel * 0.035) * 27.0)), 0.070);",
  "  acc += threadLines * (0.50 + 0.30 * sin(ang * 1.2 + t + channel));",
  "  return min(acc, 1.95);",
  "}",
  "",
  "void main(){",
  "  vec2 p = vUv * 2.0 - 1.0;",
  "  p.x *= uResolution.x / max(uResolution.y, 1.0);",
  "  float t = uTime;",
  "",
  "  // 环境光呼吸（替代启动页的一次性包络）",
  "  float breathe = 0.88 + 0.12 * sin(t * 0.22);",
  "",
  "  vec2 uv = p * (0.98 + 0.05 * sin(t * 0.25));",
  "  uv += vec2(0.0, -0.025);",
  "  vec2 flowAxis = normalize(vec2(0.86, -0.50));",
  "  vec2 crossAxis = vec2(-flowAxis.y, flowAxis.x);",
  "  float lane = dot(p, flowAxis);",
  "  float crossLane = dot(p, crossAxis);",
  "  float syncWave = sin(crossLane * 5.4 + lane * 1.1 - t * 1.85);",
  "  uv += flowAxis * syncWave * 0.044;",
  "  uv += crossAxis * sin(lane * 7.2 + t * 1.25) * 0.027;",
  "  uv *= 1.0 + 0.02 * sin(t * 0.30);",
  "",
  "  vec3 ch1 = uColor1;",
  "  vec3 ch2 = uColor2;",
  "  vec3 ch3 = uColor3;",
  "  float a = animatedLoop(uv, t, 0.0);",
  "  float b = animatedLoop(uv * 1.018 + vec2(0.012, -0.008), t + 0.18, 1.0);",
  "  float c = animatedLoop(uv * 0.986 + vec2(-0.010, 0.010), t + 0.35, 2.0);",
  "  vec3 loopCol = ch1 * a + ch2 * b + ch3 * c;",
  "  float tunnel = animatedLoop(uv * 1.42 + vec2(sin(t * 0.2) * 0.08, cos(t * 0.17) * 0.05), t * 1.12 + 1.7, 2.7);",
  "  loopCol += mix(ch2, ch3, 0.35 + 0.25 * sin(t)) * tunnel * 0.30;",
  "",
  "  // 缓慢移动的同步光带（环境版，幅度低于启动页高潮段）",
  "  float syncBand = exp(-pow((lane + 0.08 * sin(t * 0.72)) / 0.62, 2.0));",
  "  float phaseThread = pow(0.5 + 0.5 * sin(crossLane * 13.5 + lane * 2.2 - t * 3.1), 8.0);",
  "  float phaseThread2 = pow(0.5 + 0.5 * sin(crossLane * 9.0 - lane * 5.4 + t * 2.4), 10.0);",
  "  vec3 accentCol = (mix(ch2, ch3, 0.36) * phaseThread + ch1 * phaseThread2 * 0.52) * syncBand * 0.35;",
  "",
  "  float centerBeam = exp(-abs(p.y + 0.005 * sin(t * 3.0)) * 24.0) * 0.10;",
  "  float bladeMask = smoothstep(-1.55, -0.08, p.x) * (1.0 - smoothstep(0.08, 1.55, p.x));",
  "  vec3 blade = mix(ch1, ch2, vUv.x) * centerBeam * bladeMask * 0.22;",
  "",
  "  vec3 col = vec3(0.002, 0.004, 0.005);",
  "  col += loopCol * 0.56 * breathe;",
  "  col += accentCol;",
  "  col += blade;",
  "",
  "  // 扫描线 + 噪点",
  "  float scan = 0.92 + 0.08 * sin((vUv.y * uResolution.y + t * 52.0) * 0.72);",
  "  float grain = noise(vUv * uResolution.xy * 0.52 + t * 17.0) - 0.5;",
  "  col *= scan;",
  "  col += grain * 0.018;",
  "",
  "  col = max(col - vec3(0.010, 0.012, 0.012), 0.0);",
  "  col = vec3(1.0) - exp(-max(col, 0.0) * 0.62);",
  "",
  "  // 暗角",
  "  float vignette = smoothstep(1.52, 0.20, length(p * vec2(0.78, 1.04)));",
  "  col *= 0.38 + 0.86 * vignette;",
  "  col += vec3(0.006, 0.012, 0.018) * (1.0 - vignette);",
  "  gl_FragColor = vec4(col, 1.0);",
  "}",
].join("\n");

function compileShader(
  gl: WebGLRenderingContext,
  type: number,
  source: string,
): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.warn("NexBox MR background shader compile failed:", gl.getShaderInfoLog(shader));
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

export function MRBackground({ filter, opacity = 1, colors }: { filter?: string; opacity?: number; colors: MRBackgroundColors }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [fallback, setFallback] = useState(false);
  // 颜色变化时刷新派生配色，无需重建 WebGL 上下文
  const colorsRef = useRef<MRBackgroundColors>(colors);
  useEffect(() => {
    colorsRef.current = colors;
  }, [colors]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // 系统开启“减少动态效果”时仅显示静态背景
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      setFallback(true);
      return;
    }

    let gl: WebGLRenderingContext | null = null;
    try {
      gl = canvas.getContext("webgl", {
        alpha: true,
        antialias: false,
        depth: false,
        stencil: false,
        premultipliedAlpha: false,
        preserveDrawingBuffer: false,
        powerPreference: "high-performance",
      }) as WebGLRenderingContext | null;
      if (!gl) gl = canvas.getContext("experimental-webgl") as WebGLRenderingContext | null;
    } catch {
      gl = null;
    }
    if (!gl) {
      setFallback(true);
      return;
    }

    const vertexShader = compileShader(gl, gl.VERTEX_SHADER, VERTEX_SOURCE);
    const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, FRAGMENT_SOURCE);
    if (!vertexShader || !fragmentShader) {
      setFallback(true);
      return;
    }

    const program = gl.createProgram();
    if (!program) {
      setFallback(true);
      return;
    }
    gl.attachShader(program, vertexShader);
    gl.attachShader(program, fragmentShader);
    gl.linkProgram(program);
    gl.deleteShader(vertexShader);
    gl.deleteShader(fragmentShader);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      console.warn("NexBox MR background shader link failed:", gl.getProgramInfoLog(program));
      gl.deleteProgram(program);
      setFallback(true);
      return;
    }

    const buffer = gl.createBuffer();
    if (!buffer) {
      gl.deleteProgram(program);
      setFallback(true);
      return;
    }
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);

    const positionLoc = gl.getAttribLocation(program, "aPosition");
    const resolutionLoc = gl.getUniformLocation(program, "uResolution");
    const timeLoc = gl.getUniformLocation(program, "uTime");
    const color1Loc = gl.getUniformLocation(program, "uColor1");
    const color2Loc = gl.getUniformLocation(program, "uColor2");
    const color3Loc = gl.getUniformLocation(program, "uColor3");

    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.CULL_FACE);

    const resize = () => {
      const dpr = Math.min(1.6, Math.max(1, window.devicePixelRatio || 1));
      const w = Math.max(1, Math.floor(window.innerWidth * dpr));
      const h = Math.max(1, Math.floor(window.innerHeight * dpr));
      canvas.width = w;
      canvas.height = h;
      gl!.viewport(0, 0, w, h);
    };
    resize();
    window.addEventListener("resize", resize);

    // 渲染循环 + 可见性暂停
    const startedAtRef = performance.now();
    let pauseOffset = 0;
    let lastPauseStart = 0;
    let rafId = 0;
    let running = false;
    let disposed = false;

    const stopLoop = () => {
      running = false;
      if (rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
      }
    };

    const startLoop = () => {
      if (running || disposed) return;
      running = true;
      const draw = () => {
        if (!running || disposed) return;
        const now = performance.now();
        const elapsed = (now - startedAtRef - pauseOffset) / 1000;
        gl!.viewport(0, 0, canvas.width, canvas.height);
        gl!.useProgram(program);
        gl!.bindBuffer(gl!.ARRAY_BUFFER, buffer);
        gl!.enableVertexAttribArray(positionLoc);
        gl!.vertexAttribPointer(positionLoc, 2, gl!.FLOAT, false, 0, 0);
        const col = colorsRef.current;
        gl!.uniform3f(color1Loc, col.c1[0], col.c1[1], col.c1[2]);
        gl!.uniform3f(color2Loc, col.c2[0], col.c2[1], col.c2[2]);
        gl!.uniform3f(color3Loc, col.c3[0], col.c3[1], col.c3[2]);
        gl!.uniform2f(resolutionLoc, canvas.width, canvas.height);
        gl!.uniform1f(timeLoc, elapsed);
        gl!.drawArrays(gl!.TRIANGLES, 0, 3);
        rafId = requestAnimationFrame(draw);
      };
      rafId = requestAnimationFrame(draw);
    };

    const handlePause = () => {
      lastPauseStart = performance.now();
      stopLoop();
    };
    const handleResume = () => {
      if (lastPauseStart) {
        pauseOffset += performance.now() - lastPauseStart;
        lastPauseStart = 0;
      }
      startLoop();
    };

    const onVisibilityChange = () => {
      if (document.hidden) handlePause();
      else handleResume();
    };

    let unlisten: UnlistenFn | undefined;
    listen<boolean>("window-visibility-changed", (e) => {
      if (e.payload) handleResume();
      else handlePause();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    document.addEventListener("visibilitychange", onVisibilityChange);

    if (!document.hidden) startLoop();

    return () => {
      disposed = true;
      stopLoop();
      window.removeEventListener("resize", resize);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      unlisten?.();
      gl!.deleteBuffer(buffer);
      gl!.deleteProgram(program);
      // 不调用 loseContext()：StrictMode 下二次挂载可复用同一上下文
    };
  }, []);

  if (fallback) {
    return (
      <Box
        position="fixed"
        top={0}
        left={0}
        right={0}
        bottom={0}
        zIndex={0}
        overflow="hidden"
        bg="linear-gradient(135deg, #0b0207 0%, #05070c 45%, #02101a 100%)"
        style={{ filter, opacity }}
      />
    );
  }

  return (
    <Box
      position="fixed"
      top={0}
      left={0}
      right={0}
      bottom={0}
      zIndex={0}
      overflow="hidden"
      bg="#05070c"
      style={{ filter, opacity, willChange: filter ? "filter" : undefined }}
    >
      <canvas
        ref={canvasRef}
        style={{
          width: "100%",
          height: "100%",
          display: "block",
        }}
      />
    </Box>
  );
}
