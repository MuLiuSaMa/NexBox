# 液态玻璃「折射版」实现计划

日期：2026-11-27
范围：`D:\NexBox` 前端玻璃观感（不改 Tauri Rust 侧）
参考：`FlClash-main`（Flutter + `prism_ui` 的 Liquid Glass：小模糊 + 边缘弯折 + 边缘高光）

## 目标

在现有「毛玻璃」（`backdrop-filter: blur(Npx) saturate(1.3)`）之外，新增一种**有折射**的玻璃渲染方式：

- 边缘把背后的画面**弯折**（像凸透镜/玻璃边缘），中间区域基本平直；
- **模糊比 FlClash 小得多**：FlClash 菜单/弹窗的 frost sigma 约 10–16（`.agents/ios-measurements.md:48-49`），
  这里默认 3px、上限 12px；
- 可在 设置 → 外观 里切换，关闭/环境不支持时自动退回现在的毛玻璃，观感与今天一致。

## 当前状态（读码结论）

- 现有玻璃统一由 `liquidGlassEnabled / liquidGlassBlur`（`src/contexts/background-context.tsx:74-75,119-120,176-177`）驱动，
  默认 blur=10（`background-context.tsx:177`），设置页滑块 1–100（`src/pages/SettingsPage.tsx:1916-1927`）。
- 所有玻璃组件都是同一句式 `blur(${liquidGlassBlur}px) saturate(1.3)`：
  - `src/components/special/liquid-glass-card.tsx:67-83`（主卡片，带 `noBlur` 嵌套豁免）
  - `src/components/special/liquid-glass-tool-card.tsx:38-41,80-82`
  - `src/components/special/liquid-glass-menu-item.tsx:41-43,56,63`
  - `src/components/special/liquid-glass-button.tsx:21-40`
  - `src/components/ui/sidebar.tsx:373-375,737`、`global-search.tsx:260,296`、`game-mode-switch.tsx:45-48,54-56,240`、`title-bar.tsx:263,281,299`
- SVG 位移滤镜已经存在，但**全局滤镜等于没开**：
  - `src/components/special/liquid-glass-svg-filter.tsx:5` 的 `nexbox-liquid-glass-filter` 用 400×200 贴图、
    边带 `E=10`（高度 5%、宽度 2.5%），`feDisplacementMap scale="1"`（同文件 `:30,112-118`）→ 位移最大 0.5px，肉眼不可见。
  - 真正可见的是灵动岛专用滤镜 `ISLAND_FILTER_ID`：`ISLAND_SCALE=30`、垂直边带 20%（`:9-11,120-135`），
    被 `dynamic-island.tsx:1772` 以 `url(#…) blur(2px) saturate(1.35)` 使用。
  - 贴图 `generateDisplacementMap`（`:30-80`）是**直角矩形边带 + 四角补丁**，不跟随卡片圆角，圆角大的卡片边缘会出现方角折痕。
  - 全局滤镜按 `preserveAspectRatio="none"` 拉伸到任意元素尺寸（`:107-108`）：同一张贴图套到 900×60 的搜索框和 320×800 的侧栏，
    边带宽度在 px 上完全不成比例。
- 设置页现有外观卡片：`SettingsPage.tsx:1892-1972`（总开关、模糊滑块、灵动岛子开关 + 玻璃样式下拉）。
- 旧 key `liquid-glass-mode` / `liquid-glass-enhanced` 在启动时会被主动删除（`background-context.tsx:241-251`），
  **新开关不能复用这两个 key 名**。
- 页面滚动容器是 `#app-main-scroll`；顶部渐隐带用 mask 包裹内容，会成为 Backdrop Root，
  已靠「背景镜像」解决采样问题（`src/components/ui/main-layout.tsx:254-323`）——折射同样走这条路径，不需要额外处理。
- 性能背景：`plans/2026-11-10-settings-sponsor-liquid-glass-perf.md` 已把赞助名单从 O(N) 个 backdrop 表面降到 1 个；
  折射比纯模糊更贵，不能把 200 张卡重新变成 200 个折射表面。

可行性已验证（仅在浏览器里做过原型，Chromium 系列：`url(#f) blur(2px)` 的 backgdrop-filter 生效、
`feImage` 用 data URL + `feDisplacementMap` 能看到边缘弯折，`filterUnits="userSpaceOnUse"` 可做「恒定 px 边带」）。
**WebView2 里必须实机复验一次**，见验证第 3 条。

## 方案

### 核心思路

沿用已有的 SVG `feDisplacementMap` 机制，但换成「按元素尺寸生成的位移贴图」：

- 位移贴图尺寸 = 元素宽高 + `pad`，`filterUnits="userSpaceOnUse"`，边带宽度用**绝对 px**，
  圆角用与元素 `border-radius` 相同的圆弧；
- 中心一律 `rgb(128,128,128)`（零位移），只有边缘一圈有 R/B 梯度 → 只有边缘弯折，中间不扭曲；
- `feDisplacementMap.scale = 2 * band * strength`（最大位移约一个边带宽，够看出「玻璃厚度」又不至于糊）；
- 每个尺寸量化后生成一个 `<filter>`，相同尺寸共享，LRU 上限 32 个，避免几百个滤镜节点。

### 参数（默认值，集中在一个文件里方便调）

| 名称 | 默认 | 说明 |
| --- | --- | --- |
| 边带 `band` | `clamp(6, radius * 0.35, 12)` px | 只影响边缘一圈 |
| 外扩 `pad` | `band * 2` | 滤镜区域外扩，避免取样越界出现暗边 |
| 折射强度 `strength` | 100%（0–200） | 乘到 `feDisplacementMap.scale` |
| 模糊 `blur` | 3px（0–12） | 对应「我的模糊要小一些」；与现有 1–100 滑块分开存 |
| 饱和 | 1.25 | 略低于现有 1.3，折射已提供通透感 |

## 分步改动

### 步骤 1：新增折射滤镜模块 `src/lib/glass-refraction.ts`（新文件）

- `buildDisplacementMapSvg({ w, h, radius, band, pad })`：返回 data URL。
  - 基底 `rect` 填 `rgb(128,128,128)`；
  - 四边 `linearGradient` 用 `gradientUnits="userSpaceOnUse"` 精确覆盖 `band` 宽度（上 B 0→128、下 B 255→128、左 R 0→128、右 R 255→128）；
  - 四角用与 `radius` 匹配的 1/4 圆弧（`<path>` 环形带），避免现在的方角折痕；
  - 圆角外侧区域不画（保持 128），让圆角外的取样不产生位移。
- `quantize(w,h,radius)`：尺寸按 8px、半径按 4px 取整，作为缓存 key。
- `getRefractionFilterId({ w, h, radius, strength })`：
  - 命中缓存直接返回 id；未命中则在隐藏 svg 的 `<defs>` 里创建
    `<filter id filterUnits=userSpaceOnUse x=-pad y=-pad width=Wt height=Ht color-interpolation-filters=sRGB>`
    + `<feImage href=dataURL preserveAspectRatio="none" result=map>` + `<feDisplacementMap in=SourceGraphic in2=map scale=… R/B>`；
  - 超过 32 个时移除最久未用的节点。
- `supportsBackdropUrlFilter()`：复用并导出 `liquid-glass-svg-filter.tsx:13-23` 的探测（内部加一次 `CSS.supports("backdrop-filter","url(#x)")` 双保险）。

### 步骤 2：改造 `src/components/special/liquid-glass-svg-filter.tsx`

- 保留 `ISLAND_FILTER_ID` 及其现有强度（灵动岛已调好，不动观感）。
- 删除/停用 `scale="1"` 的 `nexbox-liquid-glass-filter`（`:112-118`）——它就是「看起来只有毛玻璃」的原因；
  组件不再引用它，`defs` 容器只作为共享注册表（暴露 `getDefsElement()` 给步骤 1，`:101-139` 保持组件签名不变，`main.tsx:128` 无需改）。
- `generateDisplacementMap` 的直角版本保留给灵动岛；通用版逻辑移到 `glass-refraction.ts`。

### 步骤 3：新增 hook `src/hooks/use-glass-refraction.ts`

- 入参：`{ enabled, blur, radius? }`，返回 `{ ref, backdropFilter }`。
- `ref` 挂到玻璃元素；`ResizeObserver` + `getComputedStyle(ref.current).borderRadius` 取真实尺寸/圆角，
  量化后调 `getRefractionFilterId`，重新拼 `url(#id) blur(bpx) saturate(1.25)`。
- 尺寸变化用 `requestAnimationFrame` 合并，避免侧栏收起/展开时每帧重建滤镜。
- 不支持 / 未开启 / 元素面积过小（< 20000 px²）时直接返回纯 `blur(...)` 字符串 → 组件代码只有一条拼装路径。

### 步骤 4：状态与持久化（`src/contexts/background-context.tsx`）

- 新增两个 state + 类型字段：`liquidGlassRefraction: boolean`（默认 false）、`glassRefractionBlur: number`（默认 3）、
  `glassRefractionStrength: number`（默认 100）；`G` 常量放到文件顶部与 `DEFAULT_BACKGROUND_ADJUSTMENTS` 旁。
- 持久化 key（**不要**用 `liquid-glass-mode`）：`liquid-glass-refraction`、`glass-refraction-blur`、`glass-refraction-strength`；
  照现有写法加 `setXxxPersist`（参 `:419-437`）并在 `loadSettings` 的 `Promise.all`（`:217-239`）与 `useMemo` 依赖（`:612`）里补齐。
- 对外 `setXxx` 三件套（`:74-112` 的接口、`:590-599` 的 value、provider 默认值 `:119-145`）。

### 步骤 5：组件接入

优先级从高到低（先做能立刻看出效果的三处）：

1. `src/components/special/liquid-glass-card.tsx`：`cardStyles`（`:72-86`）改用 hook 的 `backdropFilter`；
   加 `disableRefraction?: boolean`（嵌套 `noBlur`、赞助名单这类「一张面板 + 很多小卡」的场景不要折射）。
2. `src/components/ui/sidebar.tsx:737`、`src/components/ui/global-search.tsx:260,296`
   （侧栏和搜索框是「宽而矮/窄而高」两类极端尺寸，正好验证恒定 px 边带）。
3. `src/components/special/liquid-glass-tool-card.tsx:41`、`menu-item.tsx:43`、`liquid-glass-button.tsx:22`
   （小圆角/小尺寸，`band` 走 `clamp` 下限，避免 40px 按钮被整块弯折）。
4. `src/components/ui/game-mode-switch.tsx:46,240`、`title-bar.tsx:263,281,299`（36px 圆形按钮：`band` 取 4–5，
   或者保持纯模糊，因为圆按钮折射收益低）。
5. 灵动岛维持现状（`:1772` 已有专用滤镜），只在间距/强度不协调时微调 `ISLAND_SCALE`。

### 步骤 6：设置页 UI + i18n

- `src/pages/SettingsPage.tsx`（外观卡片内，模糊滑块之后 `:1929` 之前）：
  - 开关「折射玻璃（BETA）」+ 说明「边缘弯折 + 更小模糊」；
  - 开关打开后显示「折射模糊」滑块 0–12（默认 3）与「折射强度」滑块 0–200%（默认 100）；
  - 关掉总开关时三个控件与现在一样整体隐藏（沿用 `liquidGlassEnabled &&` 分支）。
- i18n 六份文件，key 加在同一段（`zh.json:2250` 附近、`en.json:2329` 附近，另 `zh-TW/ja/fr/de` 同段）：
  `liquidGlassRefractionLabel / Desc / BlurLabel / StrengthLabel`，文案统一「默认模糊更小、边缘有折射」。

### 步骤 7：性能与降级

- `LiquidGlassCard` 在 `noBlur` 或 `disableRefraction` 时不动；赞助名单（`SettingsPage.tsx` 的 `SponsorNameCard`）显式传 `disableRefraction`。
- 同一屏折射表面数难以计数，先按「面积阈值 + 页面清单」控制，别做成全局默认；默认关闭，用户自己开。
- 一旦 `supportsBackdropUrlFilter()` 为假（非 Chromium、未来 WebView2 行为变化），所有组件自动只剩 blur，不需要分支代码。

## 风险

- **WebView2 兼容性**：`backdrop-filter: url(#f)` 在 Chromium 里可用，但 data-URL `feImage` + `preserveAspectRatio="none"`
  在 WebView2 的具体版本上是否一致必须实测；不行就退到「把贴图做成 canvas→dataURL」或「只做边缘高光不折射」。
- **滤镜节点数量**：每个 `(w,h,radius)` 一个滤镜；窗口拉伸/侧栏收起会不断产生新尺寸，靠 8px 量化 + LRU 32 压住，
  但要注意 `ResizeObserver` 抖动（rAF 合并）。
- **视觉副作用**：边带取样会越出元素边界（`pad` 必须够），否则边缘出现暗边/透明缝；
  圆角半径大而元素矮（如 60px 高的搜索框 + 20px 圆角）时上下边带会碰在一起，需要用 `min(band, H/3)` 兜底。
- **性能**：`url()` 滤镜的 backdrop 比纯高斯模糊贵，滚动时不带缓存（Chromium 每帧重新取样）。
  已有先例（赞助名单 200 个表面卡顿），所以默认关闭、且小卡不做。
- **嵌套**：`main-layout.tsx:256-261` 说明 mask 子树会成为 Backdrop Root；折射同样只能采样到镜像背景层——
  与现状一致，但视频/MR 背景（该处已停用顶部渐隐）下不要新增额外滤镜层。
- **不要把 `rx` 写死**：`border-radius` 可能来自 Chakra token（`3xl` 等），必须读计算样式，不能假设 24px。

## 验证

1. 静态：`npx tsc --noEmit`；`npx eslint src/lib/glass-refraction.ts src/hooks/use-glass-refraction.ts src/contexts/background-context.tsx src/pages/SettingsPage.tsx`（目标 0 problems，不留新告警）。
2. 构建：`npm run build`（tsc + vite），确认 data-URL 生成代码能被打包且无 SSR 期 `document` 访问。
3. 实机（`npm run dev` 或 `npm run tauri:dev`）：
   - 设置 → 外观 打开「折射玻璃」→ 首页卡片、侧栏、顶部搜索框边缘应能看到\"弯折\"（背后是网格状桌面或预设壁纸时最明显），
     中间文字区域不应变形；与关闭状态对比，模糊明显变小。
   - 深/浅色、Windows 缩放 100% / 150% 各看一遍（`band` 是 CSS px，缩放后观感会变）。
   - 关掉开关 → 外观与改动前完全一致（截图对比）。
   - 侧栏收起/展开、窗口拉伸时不应出现闪黑边或持续 CPU 占用（DevTools 看元素数、`filter` 节点数）。
4. 性能：DevTools Performance 录制「在开启折射玻璃时滚动 设置→赞助 / DeltaForce 页」，与纯模糊对比帧时间；
   若明显掉帧，按步骤 7 缩小适用范围（先只保留卡片 + 侧栏）。
5. 回归：`npx tsc --noEmit` 与首页/工具页/三角洲页各点一遍，确认 `noBlur` 嵌套场景没有变成「玻璃套玻璃」的整块白。
