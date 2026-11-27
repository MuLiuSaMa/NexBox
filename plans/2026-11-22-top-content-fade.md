# 页面顶部内容渐隐（alpha mask，不加渐隐层）

日期：2026-11-22
状态：待确认（plan 模式，未改任何代码）
需求：内容滚动到顶部时不要被硬切，而是逐渐淡出；**不要**再叠一层渐变遮罩 div，要直接改内容自身不透明度。

## 目标

1. 主内容滚动区顶部出现一条 alpha 渐隐带（约 48px），内容滑到标题栏下方时平滑消失，而不是被 `overflow: auto` 硬切。
2. 实现方式必须是「改内容不透明度」：用 CSS `mask-image` 的 alpha 渐变作用于滚动容器本身，不新增任何绝对定位渐变层、不改背景层、不动 `z-index`。
3. 静止（`scrollTop = 0`）时不能有任何内容被淡掉：现在 `#app-main-scroll` 有 `pt=56px`，内容天然从 y=56 开始，渐隐带正好落在内边距里。
4. 不改变现有滚动、路由复位、悬浮滚动条、果冻弹跳等行为。

## 现状（关键位置）

- 主滚动容器：`src/components/ui/main-layout.tsx:318`（`id="app-main-scroll"`），`pt={isNavTop ? "120px" : "56px"}`（:325），`overflowY="auto" h="calc(100vh)"`（:330-332），滚动条样式写在 `sx`（:333-353），内容包在 `<Box position="relative" minHeight="100%">`（:355-357）。
- 路由切换复位滚动：`main-layout.tsx:63-69`（读 `app-main-scroll`）。
- 悬浮滚动条：`src/components/ui/floating-scrollbar.tsx`，由 `main-layout.tsx:359` 挂在滚动容器**外面**，不受 mask 影响。
- 标题栏：`src/components/ui/title-bar.tsx:219` 固定 48px 高、本身透明，只浮着搜索框和窗口按钮 → 内容在它下面滚动正是「生硬」的来源。
- 全局滚动条已隐藏：`src/index.css:66-72`。
- 液态玻璃卡片：`src/components/special/liquid-glass-card.tsx:48-64` 用 `backdrop-filter: blur() saturate()`；开关在 `src/contexts/background-context.tsx`（`liquidGlassEnabled` 默认 `false`，:69）。
- 页面内的 `position: sticky` 顶部元素：`SettingsPage.tsx:4030`（`top={8}`）、`ToolsPage.tsx:1827`（`top={8}`）、`SensorMonitorPage.tsx:642-650`（`top={0}`）。
- 页面内的 `position: fixed` 全屏浮层（都在滚动容器子树里）：`DeltaStatsPage.tsx:1102`、`DeltaStatsPage.tsx:2122`、`NvidiaRecordingPage.tsx:981`（底部批处理条）、`NvidiaRecordingPage.tsx:1031`。

## 实测发现（决定方案的两个硬事实）

在 Chromium（与 WebView2 同引擎）里做了对照实验，结论写死两点：

1. **mask 会打断 `backdrop-filter`。** 带 mask 的滚动容器内的玻璃卡片会**整块消失**（对照列正常模糊）。原因：按 Filter Effects L2，带 `mask-image` 的元素会成为 Backdrop Root，其后代 `backdrop-filter` 只能采样该根**内部**已绘制内容，而页面背景层（`MRBackground` / `<img>` / `<video>`，`main-layout.tsx:249-315`）是滚动容器的**兄弟**节点，采不到 → 模糊结果为空。
   → 结论：只要容器被 mask，玻璃卡片就失效，且**无法用任何「排除某个子元素」的办法规避**（mask 作用于整棵子树）。
2. **mask 只是 alpha 蒙版，不裁剪几何、不改变布局**；滚动容器自身的 mask 定位区是它的 border box，不随内容滚动 → 渐隐带固定在视口顶部，正是需要的效果（这也是全平台通行的做法）。

因此方案必须对「液态玻璃开关」做处理，见下方步骤 3 的决策。

## 改动步骤

### 1. 新增共享常量

新建 `src/config/ui-metrics.ts`：

```ts
/** 主内容区顶部渐隐带高度（px）：与 48px 标题栏对齐，内容滑到标题栏下开始淡出 */
export const APP_TOP_FADE_PX = 48;
/** 顶部 alpha 渐隐（纯 alpha mask，不是遮挡层） */
export const APP_TOP_FADE_MASK = `linear-gradient(to bottom, rgba(0,0,0,0) 0px, rgba(0,0,0,1) ${APP_TOP_FADE_PX}px)`;
```

### 2. 给主滚动容器加 alpha mask

`src/components/ui/main-layout.tsx`，在 `#app-main-scroll` 的 `sx`（:333-353）里追加：

```ts
maskImage: APP_TOP_FADE_MASK,
WebkitMaskImage: APP_TOP_FADE_MASK,
```

- 只在 `!liquidGlassEnabled` 时加（见步骤 3），用一个 `const topFade = !liquidGlassEnabled;` 控制，禁用时写 `maskImage: "none"`。
- 不新增任何 DOM 节点，不加渐变 div，不动 `pt`/`ml`/`h`。
- 底部**不加**渐隐：`NvidiaRecordingPage.tsx:981` 那条批处理条 `bottom={8}` 会被淡掉。

### 3. 液态玻璃的兼容处理（需要拍板，默认按 A 实现）

- **A（默认，推荐先做）**：`liquidGlassEnabled` 为真时不启用 mask（渐隐带关闭），避免所有玻璃卡片突然变平。改动只有一行条件判断，零风险。
- **B（可选增强，后续再做）**：玻璃开启时仍启用 mask，同时在滚动容器**内部**放一份 `position: fixed` 的背景副本，让玻璃有东西可采样：
  - `backgroundMode === "none"`：直接给 `#app-main-scroll` 加 `bg={bgColor}`，玻璃模糊纯色 ≈ 与现状一致，成本为零；
  - `image` / `preset`：容器内放一份 `background-size: cover` 的固定副本（与原 `<img>` 几何一致，被 mask 淡出的部分露出的正好是同一张背景，看不出接缝）；
  - `dynamic`（视频）/ `mr`（WebGL canvas）：**不复制**（会变成双解码 / 双 GL 上下文），继续按 A 关掉渐隐。
- 结论：A 必做，B 视用户是否开玻璃决定；两者互不冲突。

### 4. 顶到渐隐带里的 sticky 元素需要让位

mask 会把这些元素一起淡掉，滚动后会「半透明」甚至看不见，必须把吸附位置推到渐隐带下沿（`top = APP_TOP_FADE_PX`）：

- `src/pages/SettingsPage.tsx:4030`：`top={8}` → `top={`${APP_TOP_FADE_PX}px`}`
- `src/pages/ToolsPage.tsx:1827`：同上
- `src/pages/SensorMonitorPage.tsx:643`：`top={0}` → 该页自带 `h="100vh" overflowY="auto"` 内层滚动（:616-618），其 sticky 头吸附在自己的滚动框顶部，而该框起点在 y=56，正常情况下不进渐隐带 → **不改**，只作为验证项。

### 5. 页面内 fixed 全屏浮层的顶部 48px（需实测确认）

`DeltaStatsPage.tsx:1102`、`:2122`、`NvidiaRecordingPage.tsx:1031` 的 `position: fixed; inset: 0` 浮层是滚动容器的后代，理论上会被 mask 连带淡掉顶部 48px（露出背景）。若实测能看到：
- 用 `createPortal(document.body)` 把这三处浮层挪出滚动容器（`src/components/special/custom-color-picker.tsx:175` 已是这个套路），不改样式与逻辑；
- `NvidiaRecordingPage.tsx:981` 的底部条在 `bottom={8}`，不受顶部渐隐影响，不用动。

### 6. 不做的事

- 不加渐变遮罩层（明确要求）。
- 不改背景层结构、不动 `MRBackground` / 视频 / 图片层。
- 不引入新的动画库、不加设置项（本轮先按固定 48px）。

## 风险

1. **玻璃卡片失效**（已实测）：由步骤 3 的条件判断兜住；若选 B，需再验一遍 `preset` 背景下的接缝。
2. **mask 增加合成成本**：大滚动容器每帧可能重新光栅化被蒙版内容，重页面（DeltaStats 图表、MusicPage）滚动时可能掉帧；果冻弹跳（`transform: scale(1.05)`）期间更明显。若实测掉帧 → 只在 `scrollTop > 0` 时启用 mask，或把渐隐带缩到 32px。
3. **sticky 偏移变化**：Settings/Tools 侧栏吸附位置从 8px 变 48px，视觉上更靠下，属于预期改动。
4. **fixed 后代被淡**：见步骤 5，可能需 portal 改造（3 处）。
5. **嵌套滚动页**：`SensorMonitorPage` 自己再套一层滚动，需实测是否出现双重淡出。

## 验证

1. 类型/构建：`npx tsc --noEmit`，再 `npm run build`，贴 exit code。
2. `npm run tauri:dev` 逐项看：
   - 设置页（`/settings`）：滚动时顶部内容渐隐、左侧菜单吸附在渐隐带下沿且不透明；
   - 工具页（`/tools`）：同上；
   - 音乐页（`/music`）：有玻璃卡片时**关掉**渐隐（按方案 A）或玻璃仍正常模糊（按方案 B）；
   - 三角洲战绩（`/delta-force/stats`）：打开全屏详情浮层，确认顶部 48px 不被淡；
   - N 卡录屏（`/nvidia-recording`）：全屏预览顶部、底部批处理条正常；
   - 首页/传感器页：确认静止时不淡、无横向滚动条、悬浮滚动条位置正常。
3. 滚动性能：DevTools Performance 录制 3 秒滚动，对比改动前后 FPS；开玻璃 + MR 背景各测一次。
