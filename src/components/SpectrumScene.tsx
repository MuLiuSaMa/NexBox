/**
 * SpectrumScene — 「音域回响」样式全屏 3D 地形律动场景
 *
 * 移植自 Mineradio「音域回响」壁纸（sonic-topography-preset.js，yin-yizhen/sonic-topography）：
 * - 底部 3D 体素地形：InstancedMesh + GLSL，顶点按 8 频段频谱抬升 + 涟漪扩散 + 噪声 idle 起伏
 * - 浮空方块：随节拍（kickEnvelope）脉冲缩放旋转
 * - 流星 + 拖尾：kick 强时从天而降，落地激发白色涟漪
 * - 配色：从封面色（coverColor.rgb）派生冷暖双色，uniform 平滑过渡
 * - 音频：**纯真实频谱**（createMediaElementSource 接管 + AnalyserNode，无任何模拟）——
 *   播放中读 8 频段驱动地形/涟漪/流星，暂停/无信号时静止回落
 * 本组件只渲染场景，不渲染歌词。
 */

import { useEffect, useRef } from "react";
import * as THREE from "three";
import { BandSmoother, getAnalyser, readBands, zeroBands } from "@/lib/audio-spectrum";

interface SpectrumSceneProps {
  audioRef: HTMLAudioElement | null;
  isPlaying: boolean;
  coverColor: { isLight: boolean; rgb: [number, number, number] };
  /** 音域回响样式是否激活：组件常驻不销毁（WebGL context 保留，切回立即有效），
   *  active 控制显示/渲染循环/CORS 切换 */
  active: boolean;
}

// ── 常量（移植自 sonic-topography-preset.js）──
const RIPPLE_MAX = 10;
const RIPPLE_LIFETIME = 4.8;
const RIPPLE_SOFT_FADE_START = 2.1;
const METEOR_MAX = 20;
const TRAIL_MAX = 200;
const FLOATING_COUNT = 80;
const TERRAIN_BASE_SIZE = 168;
const MAX_KICK_DEFORM = 0.75;

// 模块级零值频段：smoother.step 只读 target，可安全复用（避免暂停/无信号时每帧 new 对象）
const ZERO_BANDS = zeroBands();

function clamp(v: number, a: number, b: number) {
  return Math.max(a, Math.min(b, v));
}
function clamp01(v: number) {
  return clamp(Number.isFinite(v) ? v : 0, 0, 1);
}
function smoothstep01(v: number) {
  const t = clamp01(v);
  return t * t * (3 - 2 * t);
}
function lerp(a: number, b: number, t: number) {
  return a + (b - a) * t;
}

// ── 地形顶点着色器（完整移植，含 simplex noise / 涟漪 / 频段抬升）──
const TERRAIN_VERTEX_SHADER = [
  "precision highp float;",
  "uniform float uTime;",
  "uniform float uSubBass;",
  "uniform float uBass;",
  "uniform float uLowMid;",
  "uniform float uMid;",
  "uniform float uHighMid;",
  "uniform float uSmoothness;",
  "uniform float uDensity;",
  "uniform float uEnergy;",
  "uniform float uAmplitude;",
  `uniform vec4 uRipples[${RIPPLE_MAX}];`,
  "varying vec2 vUv;",
  "varying float vElevation;",
  "varying float vDistance;",
  "varying vec2 vRippleAnim;",
  "varying vec3 vNormal;",
  "varying float vRelativeY;",
  "varying vec2 vInstancePos;",
  "vec3 mod289(vec3 x){return x-floor(x*(1.0/289.0))*289.0;}",
  "vec2 mod289(vec2 x){return x-floor(x*(1.0/289.0))*289.0;}",
  "vec3 permute(vec3 x){return mod289(((x*34.0)+1.0)*x);}",
  "float snoise(vec2 v){",
  "  const vec4 C=vec4(0.211324865405187,0.366025403784439,-0.577350269189626,0.024390243902439);",
  "  vec2 i=floor(v+dot(v,C.yy));",
  "  vec2 x0=v-i+dot(i,C.xx);",
  "  vec2 i1=(x0.x>x0.y)?vec2(1.0,0.0):vec2(0.0,1.0);",
  "  vec4 x12=x0.xyxy+C.xxzz; x12.xy-=i1;",
  "  i=mod289(i);",
  "  vec3 p=permute(permute(i.y+vec3(0.0,i1.y,1.0))+i.x+vec3(0.0,i1.x,1.0));",
  "  vec3 m=max(0.5-vec3(dot(x0,x0),dot(x12.xy,x12.xy),dot(x12.zw,x12.zw)),0.0);",
  "  m=m*m; m=m*m;",
  "  vec3 x=2.0*fract(p*C.www)-1.0;",
  "  vec3 h=abs(x)-0.5;",
  "  vec3 ox=floor(x+0.5);",
  "  vec3 a0=x-ox;",
  "  m*=1.79284291400159-0.85373472095314*(a0*a0+h*h);",
  "  vec3 g; g.x=a0.x*x0.x+h.x*x0.y; g.yz=a0.yz*x12.xz+h.yz*x12.yw;",
  "  return 130.0*dot(m,g);",
  "}",
  "float random(vec2 st){return fract(sin(dot(st.xy,vec2(12.9898,78.233)))*43758.5453123);}",
  "void main(){",
  "  vUv=uv;",
  "  vNormal=normal;",
  "  vec4 instancePos=instanceMatrix*vec4(0.0,0.0,0.0,1.0);",
  "  vec2 pos2D=instancePos.xz;",
  "  vInstancePos=pos2D;",
  "  float centerDist=length(pos2D);",
  "  vDistance=centerDist;",
  "  float rnd=random(pos2D);",
  "  vec2 movingPos=pos2D*0.05+vec2(uTime*0.1,uTime*0.05);",
  "  float baseNoise=(snoise(movingPos)+1.0)*0.5;",
  "  float wave=sin(pos2D.x*0.07+pos2D.y*0.05-uTime*0.45)*0.5+0.5;",
  "  float globalFalloff=smoothstep(60.0,30.0,centerDist);",
  "  float idleElevation=mix(baseNoise,wave,uSmoothness*0.5+0.2)*1.3*globalFalloff;",
  "  float subRegion=smoothstep(25.0,0.0,centerDist);",
  "  float subLift=uSubBass*subRegion*5.0;",
  "  float bassNoise=snoise(pos2D*0.1-vec2(0.0,uTime*0.2));",
  "  float bassRegion=smoothstep(35.0,5.0,centerDist+bassNoise*5.0);",
  "  float bassLift=uBass*bassRegion*(smoothstep(0.0,1.0,rnd+uDensity*0.5))*4.0;",
  "  float lowMidNoise=snoise(pos2D*0.05+vec2(uTime*0.1,0.0));",
  "  float lowMidLift=uLowMid*(lowMidNoise*0.5+0.5)*2.5;",
  "  float riverFlow=sin(pos2D.x*0.2+pos2D.y*0.2+snoise(pos2D*0.1)*2.0-uTime*2.0);",
  "  float midLift=uMid*max(0.0,riverFlow)*3.0;",
  "  float highMidRegion=smoothstep(10.0,45.0,centerDist);",
  "  float highMidLift=0.0;",
  "  if(fract(rnd*13.3)>0.8){highMidLift=uHighMid*highMidRegion*fract(rnd*7.7)*2.5;}",
  "  float audioElevation=subLift+bassLift+lowMidLift+midLift+highMidLift;",
  "  if(rnd>0.99){audioElevation+=uEnergy*5.0;}",
  "  audioElevation*=globalFalloff;",
  "  audioElevation=max(0.0,audioElevation-0.2);",
  "  audioElevation*=uAmplitude;",
  "  float elevation=idleElevation+audioElevation;",
  "  float rippleElevation=0.0;",
  "  float rippleIntensityNormal=0.0;",
  "  float rippleIntensityWhite=0.0;",
  `  for(int i=0;i<${RIPPLE_MAX};i++){`,
  "    vec4 rd=uRipples[i];",
  "    if(rd.w!=0.0){",
  "      float strength=abs(rd.w);",
  "      bool whiteRipple=rd.w<0.0;",
  "      float dist=length(pos2D-rd.xy);",
  "      float timeSince=uTime-rd.z;",
  "      float curSpeed=whiteRipple?18.0:13.0;",
  "      float curWidth=whiteRipple?1.35:5.5;",
  "      float curFadeDist=whiteRipple?12.0:26.0;",
  "      float elevationScale=whiteRipple?1.15:3.35;",
  "      float waveRadius=timeSince*curSpeed;",
  "      float d=dist-waveRadius;",
  "      float rippleWave=exp(-d*d/curWidth);",
  "      float fade=exp(-waveRadius/curFadeDist);",
  "      float lifeFade=1.0-smoothstep(2.10,4.80,timeSince);",
  "      float rPulse=rippleWave*fade*lifeFade*strength;",
  "      rippleElevation+=rPulse*elevationScale;",
  "      if(whiteRipple){rippleIntensityWhite+=rPulse;}else{rippleIntensityNormal+=rPulse;}",
  "    }",
  "  }",
  "  elevation+=rippleElevation;",
  "  vRippleAnim=vec2(clamp(rippleIntensityNormal,0.0,1.0),clamp(rippleIntensityWhite,0.0,1.0));",
  "  vElevation=elevation;",
  "  float yPos=position.y+0.5;",
  "  vRelativeY=yPos;",
  "  float totalHeight=1.0+elevation;",
  "  vec3 pos=position;",
  "  pos.y=-0.5+yPos*totalHeight;",
  "  vec4 worldPosition=modelMatrix*instanceMatrix*vec4(pos,1.0);",
  "  gl_Position=projectionMatrix*viewMatrix*worldPosition;",
  "}",
].join("\n");

// ── 地形片元着色器（完整移植：冷暖双色发光 + 涟漪 + 雾化淡出）──
const TERRAIN_FRAGMENT_SHADER = [
  "precision highp float;",
  "uniform float uTime;",
  "uniform float uPresence;",
  "uniform float uBrilliance;",
  "uniform float uAir;",
  "uniform float uWarmth;",
  "uniform float uBrightness;",
  "uniform float uSharpness;",
  "uniform vec3 uBaseColor1;",
  "uniform vec3 uBaseColor2;",
  "uniform vec3 uFogColor;",
  "uniform vec3 uCoolCore;",
  "uniform vec3 uCoolEdge;",
  "uniform vec3 uWarmCore;",
  "uniform vec3 uWarmEdge;",
  "uniform vec3 uRippleColor;",
  "uniform float uGlowIntensity;",
  "varying vec2 vUv;",
  "varying float vElevation;",
  "varying float vDistance;",
  "varying vec2 vRippleAnim;",
  "varying vec3 vNormal;",
  "varying float vRelativeY;",
  "varying vec2 vInstancePos;",
  "float random(vec2 st){return fract(sin(dot(st.xy,vec2(12.9898,78.233)))*43758.5453123);}",
  "void main(){",
  "  bool isTop=vNormal.y>0.5;",
  "  float distFromTop=1.0-vRelativeY;",
  "  float rnd=random(vInstancePos);",
  "  float centerDist=length(vInstancePos);",
  "  float normElevation=clamp(vElevation/8.0,0.0,1.0);",
  "  vec3 cBase1=uBaseColor1;",
  "  vec3 cBase2=uBaseColor2;",
  "  float warmBlend=smoothstep(0.0,1.0,uWarmth*1.5+(0.5-centerDist/80.0));",
  "  vec3 zoneCore=mix(uCoolCore,uWarmCore,warmBlend);",
  "  vec3 zoneEdge=mix(uCoolEdge,uWarmEdge,warmBlend);",
  "  vec3 targetGlow=mix(zoneCore,zoneEdge,fract(rnd*11.0));",
  "  float distFade=1.0-smoothstep(40.0,75.0,centerDist);",
  "  vec3 brightCool=mix(uCoolCore,vec3(1.0),0.24);",
  "  targetGlow=mix(targetGlow,brightCool,uBrightness*0.6);",
  "  vec3 currentGlow=mix(cBase2,targetGlow,normElevation)*uGlowIntensity*distFade;",
  "  currentGlow=mix(currentGlow,uRippleColor,clamp(vRippleAnim.x*0.82,0.0,0.72));",
  "  currentGlow=mix(currentGlow,vec3(1.0),vRippleAnim.y);",
  "  vec3 bodyColor=mix(cBase1,cBase2,vRelativeY*distFade);",
  "  vec3 finalColor;",
  "  if(isTop){",
  "    float topIntensity=smoothstep(0.0,0.4,normElevation);",
  "    float twinkleDistFalloff=smoothstep(60.0,30.0,centerDist);",
  "    float twinkleMultiplier=mix(twinkleDistFalloff,1.0,smoothstep(0.01,0.1,normElevation));",
  "    if(fract(rnd*31.0)>0.97&&normElevation<0.1){topIntensity+=uAir*0.8*twinkleMultiplier;}",
  "    finalColor=mix(cBase2,currentGlow,topIntensity);",
  "    float edgeX=smoothstep(0.05,0.01,vUv.x)+smoothstep(0.95,0.99,vUv.x);",
  "    float edgeY=smoothstep(0.05,0.01,vUv.y)+smoothstep(0.95,0.99,vUv.y);",
  "    float edge=min(edgeX+edgeY,1.0);",
  "    finalColor+=currentGlow*edge*0.8*(topIntensity+0.3);",
  "    float flashChance=smoothstep(0.65,1.0,uPresence);",
  "    if(fract(rnd*53.0)>0.99-flashChance*0.06){",
  "      float flashSync=sin(uTime*40.0+rnd*100.0)*0.5+0.5;",
  "      finalColor+=mix(vec3(1.0),vec3(0.5,1.0,1.0),rnd)*flashSync*uPresence*0.5*(1.0+uSharpness*0.8)*twinkleMultiplier;",
  "    }",
  "    if(edge>0.5&&fract(rnd*89.0+uTime*2.0)>0.995){finalColor+=vec3(1.0)*uBrilliance*1.2*twinkleMultiplier;}",
  "  }else{",
  "    float verticalFalloff=mix(1.0,3.0,uSharpness);",
  "    float sideGlow=smoothstep(0.5/verticalFalloff,0.0,distFromTop)*normElevation;",
  "    if(normElevation<0.02)sideGlow=0.0;",
  "    finalColor=mix(bodyColor,currentGlow,sideGlow*1.5);",
  "    float rimGlow=smoothstep(0.03,0.0,distFromTop)*normElevation;",
  "    finalColor+=currentGlow*rimGlow;",
  "  }",
  "  finalColor+=uRippleColor*vRippleAnim.x*0.86;",
  "  finalColor+=vec3(1.0)*vRippleAnim.y*1.2;",
  "  float aerialFog=smoothstep(30.0,65.0,vDistance);",
  "  vec3 atmosphericColor=mix(cBase1,cBase2,0.4);",
  "  finalColor=mix(finalColor,atmosphericColor,aerialFog*0.35);",
  "  float alphaFade=1.0-smoothstep(55.0,78.0,vDistance);",
  "  float alphaBlend=1.0-alphaFade;",
  "  finalColor=mix(finalColor,uFogColor,alphaBlend*0.45);",
  "  gl_FragColor=vec4(finalColor,alphaFade);",
  "}",
].join("\n");

// ── 浮空方块着色器（复用地形片元 + uPulse）──
const FLOATING_VERTEX_SHADER = [
  "precision highp float;",
  "uniform float uPulse;",
  "varying vec2 vUv;",
  "varying float vElevation;",
  "varying float vDistance;",
  "varying vec2 vRippleAnim;",
  "varying vec3 vNormal;",
  "varying float vRelativeY;",
  "varying vec2 vInstancePos;",
  "void main(){",
  "  vUv=uv;",
  "  vNormal=normal;",
  "  vec4 instancePos=instanceMatrix*vec4(0.0,0.0,0.0,1.0);",
  "  vec2 pos2D=instancePos.xz;",
  "  vInstancePos=pos2D;",
  "  vDistance=length(pos2D);",
  "  vRippleAnim=vec2(uPulse*0.8,uPulse*0.3);",
  "  vElevation=uPulse*20.0;",
  "  vRelativeY=position.y+0.5;",
  "  vec4 worldPosition=modelMatrix*instanceMatrix*vec4(position,1.0);",
  "  gl_Position=projectionMatrix*viewMatrix*worldPosition;",
  "}",
].join("\n");

export default function SpectrumScene({ audioRef, isPlaying, coverColor, active }: SpectrumSceneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  // ref 同步最新 props，供 RAF 闭包读取
  const audioRefRef = useRef(audioRef);
  const playingRef = useRef(isPlaying);
  const coverRef = useRef(coverColor);
  const activeRef = useRef(active);
  // 场景 effect 内定义的 animate，供 active effect 唤醒渲染循环
  const startRef = useRef<(() => void) | null>(null);
  // 频谱分析器（组件级 ref：active effect 在 CORS 重载完成后创建，animate 每帧读取）
  const analyserRef = useRef<AnalyserNode | null>(null);
  const freqDataRef = useRef<Uint8Array<ArrayBuffer> | null>(null);
  const ensureAnalyserRef = useRef<(() => void) | null>(null);
  // 场景清理函数（active effect 创建/销毁 WebGL 时管理）
  const sceneCleanupRef = useRef<(() => void) | null>(null);
  audioRefRef.current = audioRef;
  playingRef.current = isPlaying;
  coverRef.current = coverColor;
  activeRef.current = active;

  // ── 场景构建：由 active effect 创建/销毁（非激活时释放 WebGL context，内存平；
  //   激活时延迟重建，避免立即重建失败）──
  const buildScene = (container: HTMLDivElement): (() => void) => {
    // ── 渲染器 / 场景 / 相机 ──
    // alpha:false + 不透明清屏色：背景本就是固定深色（父容器 #05070c），
    // 透明 canvas 会让 Chromium 每帧做 alpha 合成（合成层开销 + 内存缓涨），不透明更省
    const renderer = new THREE.WebGLRenderer({ alpha: false, antialias: false, powerPreference: "high-performance" });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.25));
    renderer.setClearColor(0x05070c, 1);
    renderer.domElement.style.width = "100%";
    renderer.domElement.style.height = "100%";
    renderer.domElement.style.display = "block";
    container.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(58, 1, 0.1, 400);
    // 相机贴近、俯角更大：地形铺满画面底部（壁纸观感）
    camera.position.set(0, 20, 17);
    camera.lookAt(0, -2, 0);

    // ── 自由视角（orbit，MR 风格）：拖拽旋转（反向+惯性）、滚轮缩放 ──
    let orbitTheta = 0;
    let orbitPhi = 0;
    let thetaVel = 0;
    let phiVel = 0;
    let orbitDragging = false;
    let orbitLastX = 0;
    let orbitLastY = 0;
    let zoomLevel = 0; // 滚轮缩放偏移
    const onOrbitDown = (e: PointerEvent) => {
      orbitDragging = true;
      orbitLastX = e.clientX;
      orbitLastY = e.clientY;
    };
    const onOrbitMove = (e: PointerEvent) => {
      if (!orbitDragging) return;
      const dx = e.clientX - orbitLastX;
      const dy = e.clientY - orbitLastY;
      orbitLastX = e.clientX;
      orbitLastY = e.clientY;
      // 拖拽方向：拖右视角向左、拖下视角向上（MR orbit 惯例）
      orbitTheta -= dx * 0.006;
      orbitPhi -= dy * 0.0045;
      orbitPhi = clamp(orbitPhi, -0.7, 0.7);
      thetaVel = -dx * 0.006 * 60;
      phiVel = -dy * 0.0045 * 60;
    };
    const onOrbitUp = () => {
      orbitDragging = false;
    };
    const onOrbitWheel = (e: WheelEvent) => {
      e.preventDefault();
      zoomLevel = clamp(zoomLevel + e.deltaY * 0.003, -2, 5);
    };
    container.addEventListener("pointerdown", onOrbitDown);
    window.addEventListener("pointermove", onOrbitMove);
    window.addEventListener("pointerup", onOrbitUp);
    container.addEventListener("wheel", onOrbitWheel, { passive: false });

    const resize = () => {
      const w = container.clientWidth || 800;
      const h = container.clientHeight || 500;
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    // ── 网格密度（按窗口面积）：MR 原密度（160/128/96），大方块粗犷排列 ──
    const maxDim = Math.max(container.clientWidth || 800, container.clientHeight || 500);
    const gridSize = maxDim > 1600 ? 160 : maxDim > 1100 ? 128 : 96;
    const spacing = TERRAIN_BASE_SIZE / gridSize;
    const boxWidth = spacing * (0.9 / 1.05);

    // ── 根节点（整体自动旋转）；scale 放大让地形铺满画面下部 ──
    const root = new THREE.Group();
    root.position.set(0, -3.8, -3.2);
    root.scale.setScalar(0.32);
    scene.add(root);

    // ── 共享 dummy ──
    const dummyObj = new THREE.Object3D();
    const dummyPos = new THREE.Vector3();
    const dummyQuat = new THREE.Quaternion();
    const dummyScale = new THREE.Vector3();
    const dummyMat4 = new THREE.Matrix4();
    const dummyEuler = new THREE.Euler();

    const WHITE = new THREE.Color(0xffffff);
    let themeKey = "";
    let theme = defaultTheme();
    function ensureTheme() {
      const rgb = coverRef.current.rgb;
      const key = `${rgb[0]},${rgb[1]},${rgb[2]}`;
      if (key !== themeKey) {
        themeKey = key;
        theme = deriveTheme(rgb);
      }
      return theme;
    }

    // ── uniforms ──
    function makeTerrainUniforms() {
      return {
        uTime: { value: 0 },
        uSubBass: { value: 0 },
        uBass: { value: 0 },
        uLowMid: { value: 0 },
        uMid: { value: 0 },
        uHighMid: { value: 0 },
        uPresence: { value: 0 },
        uBrilliance: { value: 0 },
        uAir: { value: 0 },
        uWarmth: { value: 0 },
        uBrightness: { value: 0 },
        uSharpness: { value: 0 },
        uSmoothness: { value: 0 },
        uDensity: { value: 0 },
        uEnergy: { value: 0 },
        uAmplitude: { value: 1 },
        uRipples: { value: Array.from({ length: RIPPLE_MAX }, () => new THREE.Vector4(0, 0, -100, 0)) },
        uBaseColor1: { value: new THREE.Color(0.01, 0.02, 0.04) },
        uBaseColor2: { value: new THREE.Color(0.03, 0.05, 0.09) },
        uFogColor: { value: new THREE.Color(0.01, 0.02, 0.04) },
        uCoolCore: { value: new THREE.Color(0, 0.3, 1) },
        uCoolEdge: { value: new THREE.Color(0.6, 0.2, 1) },
        uWarmCore: { value: new THREE.Color(1, 0.2, 0.1) },
        uWarmEdge: { value: new THREE.Color(1, 0.6, 0) },
        uRippleColor: { value: new THREE.Color(0.2, 0.9, 1) },
        uGlowIntensity: { value: 1 },
      };
    }

    // ── 地形 InstancedMesh ──
    const terrainGeo = new THREE.BoxGeometry(boxWidth, 1, boxWidth);
    const terrainMat = new THREE.ShaderMaterial({
      uniforms: makeTerrainUniforms(),
      vertexShader: TERRAIN_VERTEX_SHADER,
      fragmentShader: TERRAIN_FRAGMENT_SHADER,
      transparent: true,
      depthWrite: true,
      depthTest: true,
    });
    const terrain = new THREE.InstancedMesh(terrainGeo, terrainMat, gridSize * gridSize);
    terrain.frustumCulled = false;
    const offset = (gridSize * spacing) / 2;
    let n = 0;
    for (let x = 0; x < gridSize; x++) {
      for (let z = 0; z < gridSize; z++) {
        dummyMat4.makeTranslation(x * spacing - offset, 0.5, z * spacing - offset);
        terrain.setMatrixAt(n++, dummyMat4);
      }
    }
    terrain.instanceMatrix.needsUpdate = true;
    root.add(terrain);

    // ── 浮空方块 ──
    const floatingGeo = new THREE.BoxGeometry(1, 1, 1);
    const floatingMat = new THREE.ShaderMaterial({
      uniforms: { ...makeTerrainUniforms(), uPulse: { value: 0 } },
      vertexShader: FLOATING_VERTEX_SHADER,
      fragmentShader: TERRAIN_FRAGMENT_SHADER,
      transparent: true,
      depthWrite: false,
      depthTest: true,
    });
    const floatingBlocks = new THREE.InstancedMesh(floatingGeo, floatingMat, FLOATING_COUNT);
    floatingBlocks.frustumCulled = false;
    const floatingData: Array<{ x: number; z: number; y: number; baseScale: number; phase: number; rotationSpeed: number }> = [];
    for (let i = 0; i < FLOATING_COUNT; i++) {
      const ring = i / Math.max(1, FLOATING_COUNT);
      const angle = ring * Math.PI * 2 * 5.0 + Math.sin(i * 12.9898) * 0.7;
      const radius = 14 + ((i * 37) % 62);
      const height = 6 + ((i * 17) % 19);
      floatingData.push({
        x: Math.cos(angle) * radius,
        z: Math.sin(angle) * radius,
        y: height,
        baseScale: 0.42 + ((i * 11) % 9) * 0.03,
        phase: i * 0.73,
        rotationSpeed: 0.18 + ((i * 7) % 10) * 0.035,
      });
    }
    root.add(floatingBlocks);

    // ── 主题色 lerp 目标列表（初始化一次，每帧循环复用，零闭包零分配）──
    const themeLerpTargets: Array<{ target: { value: unknown }; key: keyof TerrainTheme }> = [];
    {
      const tU = terrainMat.uniforms;
      const fU = floatingMat.uniforms;
      const push = (target: { value: unknown }, key: keyof TerrainTheme) => themeLerpTargets.push({ target, key });
      push(tU.uBaseColor1, "base1");
      push(tU.uBaseColor2, "base2");
      push(tU.uFogColor, "base1");
      push(tU.uCoolCore, "coolCore");
      push(tU.uCoolEdge, "coolEdge");
      push(tU.uWarmCore, "warmCore");
      push(tU.uWarmEdge, "warmEdge");
      push(tU.uRippleColor, "ripple");
      push(fU.uBaseColor1, "base1");
      push(fU.uBaseColor2, "base2");
      push(fU.uFogColor, "base1");
      push(fU.uCoolCore, "coolCore");
      push(fU.uCoolEdge, "coolEdge");
      push(fU.uWarmCore, "warmCore");
      push(fU.uWarmEdge, "warmEdge");
      push(fU.uRippleColor, "ripple");
    }

    // ── 流星 + 拖尾（基础材质，简单色块）──
    const meteorGeo = new THREE.BoxGeometry(0.4, 1.2, 0.4);
    const meteorMat = new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 1, depthWrite: false, toneMapped: false });
    const meteors = new THREE.InstancedMesh(meteorGeo, meteorMat, METEOR_MAX);
    meteors.frustumCulled = false;
    const trailGeo = new THREE.BoxGeometry(0.8, 0.8, 0.8);
    const trailMat = new THREE.MeshBasicMaterial({ color: 0xa8ecff, transparent: true, opacity: 0.6, depthWrite: false, toneMapped: false });
    const trails = new THREE.InstancedMesh(trailGeo, trailMat, TRAIL_MAX);
    trails.frustumCulled = false;
    root.add(meteors);
    root.add(trails);

    // 初始隐藏（scale 0 放远处）
    for (let i = 0; i < METEOR_MAX; i++) {
      dummyScale.set(0, 0, 0);
      dummyMat4.compose(dummyPos.set(0, -1000, 0), dummyQuat, dummyScale);
      meteors.setMatrixAt(i, dummyMat4);
    }
    for (let i = 0; i < TRAIL_MAX; i++) {
      dummyScale.set(0, 0, 0);
      dummyMat4.compose(dummyPos.set(0, -1000, 0), dummyQuat, dummyScale);
      trails.setMatrixAt(i, dummyMat4);
    }
    meteors.instanceMatrix.needsUpdate = true;
    trails.instanceMatrix.needsUpdate = true;

    // ── 数据状态 ──
    const ripples: Array<{ x: number; z: number; start: number; strength: number; white: boolean }> = [];
    for (let i = 0; i < RIPPLE_MAX; i++) ripples.push({ x: 0, z: 0, start: -100, strength: 0, white: false });
    let rippleIdx = 0;
    // 涟漪冷却时间戳（限制触发频率，避免真实频谱下涟漪满屏）
    let lastKickRippleAt = -10;
    let lastSnareRippleAt = -10;
    const meteorsData: Array<{ active: boolean; x: number; y: number; z: number; speed: number; strength: number }> = [];
    for (let i = 0; i < METEOR_MAX; i++) meteorsData.push({ active: false, x: 0, y: -1000, z: 0, speed: 0, strength: 0 });
    let meteorIdx = 0;
    let lastMeteorAt = -999;
    const trailsData: Array<{ active: boolean; x: number; y: number; z: number; vx: number; vy: number; vz: number; life: number; maxLife: number; scale: number }> = [];
    for (let i = 0; i < TRAIL_MAX; i++) {
      trailsData.push({ active: false, x: 0, y: -1000, z: 0, vx: 0, vy: 0, vz: 0, life: 0, maxLife: 1, scale: 1 });
    }
    let trailIdx = 0;

    function addRipple(x: number, z: number, strength: number, white: boolean) {
      const r = ripples[rippleIdx];
      r.x = x;
      r.z = z;
      r.start = sonicTime;
      r.strength = clamp(strength, 0.1, 3.0);
      r.white = white;
      rippleIdx = (rippleIdx + 1) % RIPPLE_MAX;
    }

    function addMeteor(strength: number) {
      if (sonicTime - lastMeteorAt < 0.55) return;
      lastMeteorAt = sonicTime;
      const m = meteorsData[meteorIdx];
      const angle = Math.random() * Math.PI * 2;
      const dist = Math.random() * 25;
      m.active = true;
      m.x = Math.cos(angle) * dist;
      m.z = Math.sin(angle) * dist;
      m.y = 30 + Math.random() * 10;
      m.speed = 1.0 + Math.random() * 0.5 + strength * 1.5;
      m.strength = strength;
      meteorIdx = (meteorIdx + 1) % METEOR_MAX;
    }

    function spawnTrail(x: number, y: number, z: number, speedMul: number) {
      const p = trailsData[trailIdx];
      p.active = true;
      p.x = x + (Math.random() - 0.5) * 1.5;
      p.y = y + (Math.random() - 0.5) * 1.5;
      p.z = z + (Math.random() - 0.5) * 1.5;
      p.vx = (Math.random() - 0.5) * 2.0;
      p.vy = Math.random() * 2.0 + speedMul * 10.0;
      p.vz = (Math.random() - 0.5) * 2.0;
      p.life = 0;
      p.maxLife = 0.5 + Math.random() * 0.5;
      p.scale = Math.random() * 0.6 + 0.2;
      trailIdx = (trailIdx + 1) % TRAIL_MAX;
    }

    // ── 音频：真实频谱优先（懒接管，仅 ctx running 时创建），失败/暂停时模拟兜底 ──
    const smoother = new BandSmoother(10);
    const bandTarget = zeroBands(); // 复用输出对象，避免每帧分配
    let sonicTime = 0;
    let autoYaw = 0;
    let floatingPulse = 0;

    // 频谱分析器：由 active effect 在 CORS 媒体重载完成后创建（captureStream 需 CORS 媒体才有信号）。
    // 存组件级 ref，active effect 可访问/重置
    const ensureAnalyser = () => {
      if (!analyserRef.current && audioRefRef.current && playingRef.current) {
        const a = getAnalyser(audioRefRef.current);
        if (a) {
          analyserRef.current = a;
          freqDataRef.current = new Uint8Array(a.frequencyBinCount);
        }
      }
    };
    ensureAnalyserRef.current = ensureAnalyser;

    // ── 主循环 ──
    let raf = 0;
    let lastT = performance.now();

    const syncRippleUniforms = () => {
      const arr = terrainMat.uniforms.uRipples.value as THREE.Vector4[];
      for (let i = 0; i < RIPPLE_MAX; i++) {
        const r = ripples[i];
        const age = sonicTime - r.start;
        const active = r.strength > 0.001 && age >= 0 && age < RIPPLE_LIFETIME;
        if (!active) {
          arr[i].set(0, 0, -100, 0);
          if (r.strength > 0) r.strength = 0;
          continue;
        }
        const fade = 1 - smoothstep01((age - RIPPLE_SOFT_FADE_START) / (RIPPLE_LIFETIME - RIPPLE_SOFT_FADE_START));
        const strength = r.strength * fade;
        arr[i].set(r.x, r.z, r.start, r.white ? -strength : strength);
      }
    };

    // 涟漪/流星触发回调：定义一次复用，避免每帧新建闭包
    const onBeatTrigger = (kind: "kick" | "snare", level: number) => {
      if (kind === "kick") {
        if (sonicTime - lastKickRippleAt < 1.0) return;
        lastKickRippleAt = sonicTime;
        const angle = Math.random() * Math.PI * 2;
        const dist = Math.random() * 20;
        addRipple(Math.cos(angle) * dist, Math.sin(angle) * dist, Math.min(level * 2.0, 3.0), false);
        if (level > 0.62 && Math.random() < 0.045) addMeteor(clamp(level, 0.28, 0.9));
      } else {
        if (sonicTime - lastSnareRippleAt < 1.4) return;
        lastSnareRippleAt = sonicTime;
        const angle2 = Math.random() * Math.PI * 2;
        const dist2 = 10 + Math.random() * 35;
        addRipple(Math.cos(angle2) * dist2, Math.sin(angle2) * dist2, Math.min(level * 1.2, 3.0), true);
      }
    };

    const animate = () => {
      // 非激活 / 页面隐藏：停止调度（保留 WebGL context，active 恢复时由唤醒回调重启）
      if (!activeRef.current || document.hidden) {
        raf = 0;
        return;
      }
      raf = requestAnimationFrame(animate);
      const now = performance.now();
      // 暂停：不硬冻结——继续渲染让地形平滑回落到静止（频谱目标切为全零，
      // 律动自然衰减归零，无"假律动"；画面从暂停瞬间平滑过渡到静止，不卡住）
      const dt = Math.min(0.05, (now - lastT) / 1000);
      lastT = now;
      sonicTime += dt * (0.45 + 0.85);
      autoYaw += dt * 0.12;
      root.rotation.y = autoYaw;

      // 懒创建频谱分析器（仅激活且 CORS 已就绪时；进入音域回响的重载完成时由 active effect 主动创建）
      if (!analyserRef.current && audioRefRef.current && playingRef.current && activeRef.current && !!audioRefRef.current.crossOrigin) {
        ensureAnalyser();
      }
      // 音频：播放中读真实频谱（律动严格跟随音乐——静音段落自然低值、暂停即静止），
      // 暂停/未接管时向 0 回落；不做模拟兜底（避免"没声音还在动"）
      let target;
      const analyser = analyserRef.current;
      const freqData = freqDataRef.current;
      if (analyser && freqData && playingRef.current) {
        analyser.getByteFrequencyData(freqData);
        target = readBands(freqData, 44100, 1024, bandTarget); // 复用对象减少分配
      } else {
        target = ZERO_BANDS; // 复用模块级常量，零分配
      }
      const smooth = smoother.step(target, dt, onBeatTrigger);

      // 主题色平滑（地形 + 浮空方块同步跟随封面色；配对数组循环，零闭包零分配）
      const th = ensureTheme();
      const ca = lerp(0.06, 0.4, dt * 4);
      for (let i = 0; i < themeLerpTargets.length; i++) {
        const p = themeLerpTargets[i];
        (p.target.value as THREE.Color).lerp(th[p.key], ca);
      }
      meteorMat.color.copy(th.warmCore).lerp(WHITE, 0.7);
      trailMat.color.copy(th.ripple);

      // 地形 uniforms
      const u = terrainMat.uniforms;
      const low = smooth.subBass + smooth.bass + smooth.lowMid + smooth.mid;
      const high = smooth.presence + smooth.brilliance + smooth.air;
      const sum = Math.max(0.001, low + high);
      u.uTime.value = sonicTime;
      u.uSubBass.value = smooth.subBass * 1.2;
      u.uBass.value = smooth.bass;
      u.uLowMid.value = smooth.lowMid;
      u.uMid.value = smooth.mid;
      u.uHighMid.value = smooth.highMid;
      u.uPresence.value = smooth.presence;
      u.uBrilliance.value = smooth.brilliance;
      u.uAir.value = smooth.air;
      u.uWarmth.value = clamp(low / sum, 0, 1);
      u.uBrightness.value = clamp(high / sum, 0, 1);
      u.uSharpness.value = smooth.sharpness;
      u.uSmoothness.value = smooth.smoothness;
      u.uDensity.value = smooth.density;
      u.uEnergy.value = smooth.energy;
      u.uAmplitude.value = 2.2; // 律动高度 2.2
      syncRippleUniforms();

      // 浮空方块（MR 同款参数：intensity 55 / minSize 9 / maxSize 26 / speed 77）
      // 用未平滑的原始频谱（target）驱动，鼓点脉冲突跳明显，方块随节拍一鼓一鼓放大
      const fU = floatingMat.uniforms;
      const FLOATING_INTENSITY = 0.55;
      const pulseTarget = clamp01(target.kickEnvelope * 2.0 + target.bass * 0.8);
      const speedRate = 3 + (36 - 3) * 0.77; // ≈ 28.4，跟随快
      const pulseBlend = 1 - Math.exp(-speedRate * dt);
      floatingPulse += (pulseTarget - floatingPulse) * pulseBlend;
      const pulse = floatingPulse;
      fU.uTime.value = sonicTime;
      fU.uPulse.value = pulse;
      // MR 尺寸映射（场景 scale 0.32 比 MR 大 2 倍，方块尺寸相应减半对齐观感）
      const minVisualScale = 0.09 + (0.38 - 0.09) * 0.09;
      const maxVisualScale = Math.max(minVisualScale + 0.05, 0.22 + (1.6 - 0.22) * 0.26);
      const sizeMix = clamp(pulse * (0.5 + FLOATING_INTENSITY * 1.7), 0, 1);
      const pulseScale = lerp(minVisualScale, maxVisualScale, sizeMix);
      for (let i = 0; i < FLOATING_COUNT; i++) {
        const b = floatingData[i];
        const bob = Math.sin(sonicTime * (0.55 + b.rotationSpeed) + b.phase) * 0.45;
        dummyPos.set(b.x, b.y + bob + pulse * FLOATING_INTENSITY * 1.4, b.z);
        dummyEuler.set(
          sonicTime * b.rotationSpeed + b.phase,
          sonicTime * b.rotationSpeed * 0.7 + b.phase,
          sonicTime * b.rotationSpeed * 0.45
        );
        dummyQuat.setFromEuler(dummyEuler);
        const s = b.baseScale * pulseScale;
        dummyScale.set(s, s, s);
        dummyMat4.compose(dummyPos, dummyQuat, dummyScale);
        floatingBlocks.setMatrixAt(i, dummyMat4);
      }
      floatingBlocks.instanceMatrix.needsUpdate = true;

      // 流星 + 拖尾
      for (let i = 0; i < METEOR_MAX; i++) {
        const m = meteorsData[i];
        if (!m.active) {
          dummyPos.set(0, -1000, 0);
          dummyScale.set(0, 0, 0);
        } else {
          m.y -= m.speed * 60 * dt;
          if (m.y <= 0) {
            m.active = false;
            addRipple(m.x, m.z, Math.min(m.strength, 1.2), true);
            for (let t = 0; t < 10; t++) spawnTrail(m.x, 0.5, m.z, m.speed * 1.5);
            dummyPos.set(0, -1000, 0);
            dummyScale.set(0, 0, 0);
          } else {
            if (Math.random() > 0.3) spawnTrail(m.x, m.y, m.z, m.speed * 0.2);
            dummyPos.set(m.x, Math.max(0, m.y), m.z);
            dummyScale.set(1.5, 1.5, 1.5);
          }
        }
        dummyQuat.identity();
        dummyMat4.compose(dummyPos, dummyQuat, dummyScale);
        meteors.setMatrixAt(i, dummyMat4);
      }
      meteors.instanceMatrix.needsUpdate = true;
      for (let i = 0; i < TRAIL_MAX; i++) {
        const p = trailsData[i];
        if (!p.active) {
          dummyPos.set(0, -1000, 0);
          dummyScale.set(0, 0, 0);
        } else {
          p.life += dt;
          if (p.life >= p.maxLife) {
            p.active = false;
            dummyScale.set(0, 0, 0);
          } else {
            p.x += p.vx * dt * 10;
            p.y += p.vy * dt * 10;
            p.z += p.vz * dt * 10;
            const s = p.scale * (1.0 - p.life / p.maxLife);
            dummyPos.set(p.x, p.y, p.z);
            dummyScale.set(s, s, s);
          }
        }
        dummyQuat.identity();
        dummyMat4.compose(dummyPos, dummyQuat, dummyScale);
        trails.setMatrixAt(i, dummyMat4);
      }
      trails.instanceMatrix.needsUpdate = true;

      // 视角：球面轨道（拖拽 theta/phi + 惯性滑行）+ 律动缩放 + 滚轮缩放
      if (!orbitDragging) {
        // 松手后惯性滑行（指数衰减，约 0.4s 停）
        thetaVel *= Math.pow(0.003, dt);
        phiVel *= Math.pow(0.003, dt);
        orbitTheta += thetaVel * dt;
        orbitPhi += phiVel * dt;
        orbitPhi = clamp(orbitPhi, -0.7, 0.7);
      }
      const zoomPulse = clamp01(smooth.kickEnvelope * 0.6 + smooth.energy * 0.9);
      const radius = 27.6 - zoomPulse * 7.0 + zoomLevel;
      const phi = 0.76 + orbitPhi;
      const theta = orbitTheta; // 地形自转由 root.rotation.y 承担
      camera.position.set(
        radius * Math.sin(phi) * Math.sin(theta),
        radius * Math.cos(phi),
        radius * Math.sin(phi) * Math.cos(theta)
      );
      camera.lookAt(0, -2, 0);

      renderer.render(scene, camera);
    };
    // 注册唤醒回调（active effect 在组件常驻期间切换激活时调用，重启渲染循环）
    startRef.current = () => {
      if (raf === 0) animate();
    };
    animate();

    // ── 清理（组件真正卸载时——MusicPage 卸载，非样式切换）──
    return () => {
      startRef.current = null;
      cancelAnimationFrame(raf);
      ro.disconnect();
      container.removeEventListener("pointerdown", onOrbitDown);
      window.removeEventListener("pointermove", onOrbitMove);
      window.removeEventListener("pointerup", onOrbitUp);
      container.removeEventListener("wheel", onOrbitWheel);

      terrainGeo.dispose();
      terrainMat.dispose();
      floatingGeo.dispose();
      floatingMat.dispose();
      meteorGeo.dispose();
      meteorMat.dispose();
      trailGeo.dispose();
      trailMat.dispose();
      renderer.dispose();
      // 强制释放 WebGL 上下文：否则反复进出音域回响会累积多个 context（内存持续增长）
      renderer.forceContextLoss();
      if (renderer.domElement.parentNode === container) {
        container.removeChild(renderer.domElement);
      }
    };
  };

  // ── 音域回响进入/离开：场景创建/销毁（WebGL 随 active 释放，内存平）+ 频谱分析器 ──
  useEffect(() => {
    const container = containerRef.current;
    if (active && container) {
      // 延迟重建：前一个 WebGL context 释放是异步的，立即创建可能失败（黑屏/无效果）
      const t = setTimeout(() => {
        if (!container.isConnected) return;
        sceneCleanupRef.current?.(); // 双保险清理旧的
        sceneCleanupRef.current = buildScene(container);
        // 创建频谱分析器（懒，createMediaElementSource 永久接管，缓存复用）
        ensureAnalyserRef.current?.();
      }, 250);
      return () => clearTimeout(t);
    }
    // active=false：释放 WebGL 场景（renderer.dispose + forceContextLoss）
    sceneCleanupRef.current?.();
    sceneCleanupRef.current = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active]);

  return (
    <div
      ref={containerRef}
      style={{ position: "absolute", inset: 0, zIndex: 1, overflow: "hidden", display: active ? "block" : "none" }}
    />
  );
}

// ── 主题派生 ──
interface TerrainTheme {
  base1: THREE.Color;
  base2: THREE.Color;
  coolCore: THREE.Color;
  coolEdge: THREE.Color;
  warmCore: THREE.Color;
  warmEdge: THREE.Color;
  ripple: THREE.Color;
}

function deriveTheme(rgb: [number, number, number]): TerrainTheme {
  const primary = new THREE.Color(rgb[0] / 255, rgb[1] / 255, rgb[2] / 255);
  const accent = new THREE.Color("#33e6ff");
  const cool = new THREE.Color("#0066ff");
  const warm = new THREE.Color("#ff3c19");
  // 封面色权重提高：切歌时场景色调明显跟随（底色不再被压到几乎不变）
  const base1 = primary.clone().lerp(new THREE.Color("#05070c"), 0.68);
  const base2 = base1.clone().lerp(accent, 0.22);
  const coolCore = primary.clone().lerp(new THREE.Color("#ffffff"), 0.14);
  const coolEdge = coolCore.clone().lerp(base1, 0.3);
  const warmCore = cool.clone().lerp(new THREE.Color("#ffb15a"), 0.18).lerp(primary, 0.42);
  const warmEdge = warmCore.clone().lerp(base1, 0.22);
  const ripple = accent.clone().lerp(primary, 0.4);
  return { base1, base2, coolCore, coolEdge, warmCore, warmEdge, ripple };
}

function defaultTheme(): TerrainTheme {
  return deriveTheme([18, 90, 200]);
}
