# 设置页「赞助者名单」在液态玻璃下卡顿 — 修复计划

日期：2026-11-10
范围：设置页赞助者列表渲染性能（开启液态玻璃后卡片卡顿、滚动掉帧）

## 目标

开启「液态玻璃」后进入 设置 → 赞助，赞助者名单（约 200 条）的卡片不再拖慢滚动与交互；
视觉上仍保留玻璃观感，且液态玻璃关闭时外观与现在一致。

## 当前状态（读码结论）

- 赞助者数据：`src/pages/SettingsPage.tsx:2764-2798` 通过 `invoke("get_sponsors")`
  拉取远程 `sponsors.json`（`src-tauri/src/sponsor.rs:49-76`，5s 超时），
  先渲染 24 条，之后每 33ms 追加 24 条（`SPONSOR_CHUNK_SIZE`，`SettingsPage.tsx:2762`）。
  远端实际条目约 200 条（gitee `sponsors.json`）。
- 卡片：`SponsorNameCard`（`SettingsPage.tsx:2725-2748`）用 `LiquidGlassCard`。
- `LiquidGlassCard`（`src/components/special/liquid-glass-card.tsx:48-82`）在玻璃模式下给
  **每一个**卡片加：
  - `backdrop-filter: blur(Npx) saturate(1.3)` + 相同的 `WebkitBackdropFilter`（第 48-60 行），
    每个元素都会各自生成一个渲染表面并对背后画面做一次模糊；
  - 一层绝对定位、`WebkitMask`/`maskComposite: exclude` 的边框辉光子树（第 66、76-82 行）；
  - `transition: ... backdrop-filter 0.45s`（第 61 行）。
- 因此玻璃模式下赞助页大约有 200 个 backdrop-filter 表面（每个还带一层被遮罩的辉光层），
  而页面滚动容器是 `#app-main-scroll`（`src/components/ui/main-layout.tsx:318-358`），
  每一帧滚动都要重新模糊这 200 个表面 → 滚动/切换卡顿、hover 反应迟钝。
- 追加放大的因素：子菜单切换时 `SettingsPage.tsx:3933-3949` 会给 body 加
  `jelly-bounce-active`，`src/index.css:208-213` 让所有 `.jelly-bounce-card` 同时播放
  `jellyBounce`（scale 动画，200 个带 backdrop-filter 的元素同帧缩放），这是最重的一帧。
- `content-visibility: auto`（`SettingsPage.tsx:2741`）只跳过视口外元素的绘制，
  无法消除「视口内每个卡片各做一次 backdrop 模糊」的开销，也解决不了 200 个表面同时缩放的问题。

## 方案选择

- 采用「单面板玻璃 + 内部小卡片不各自模糊」：
  用一个 `LiquidGlassCard` 作为名单网格的外层玻璃面板（1 次模糊 + 1 层辉光），
  内部每个名字卡片退化为半透明 `Box`（无 `backdrop-filter`、无辉光层）。
  观感仍是玻璃卡片压在玻璃面板上，成本从 O(N) 次模糊降到 O(1)。
- 不采用「虚拟滚动/只渲染可见卡片」：`flex-wrap` 流式布局做虚拟化的复杂度远高于收益，
  且池化后仍要为可见卡片逐个模糊。
- 不改 `LiquidGlassCard` 公共组件的默认行为，避免影响其它页面。

## 分步改动

### 步骤 1：`SponsorNameCard` 退化成轻量玻璃小卡（`src/pages/SettingsPage.tsx:2725-2748`）

- 顶部新增一段颜色常量（在 `SponsorNameCard` 内用 `useColorModeValue`，
  与 `liquid-glass-card.tsx:32-42` 的取值对齐）：
  - 玻璃开启：浅色 `rgba(255,255,255,0.35)` / 深色 `rgba(0,0,0,0.35)` 作为底色，
    边框 `rgba(255,255,255,0.25)` / `rgba(255,255,255,0.12)`；
  - 玻璃关闭：沿用现有 `white` / `#111111`，边框 `gray.200` / `#333333`。
  这需要把 `liquidGlassEnabled` 传进来（或让 `SponsorNameCard` 自己 `useBackground()`；
  后者会让每张卡都订阅 context，建议由父级传一个 `glass?: boolean`）。
- 外层元素从 `LiquidGlassCard` 换成 `Box`：保留 `px={4} py={3} textAlign="center"
  minW="96px" flex="0 1 auto"`、`borderRadius="3xl"`、`border="1px solid"`，
  以及 `sx={{ contentVisibility: "auto", containIntrinsicSize: "auto 47px" }}`。
- 不给小卡片加 `backdrop-filter`、不渲染辉光子树、不加 `jelly-bounce-card` class。
- `memo` 保留。

### 步骤 2：名单区包一层单玻璃面板（`src/pages/SettingsPage.tsx:2877-2900`）

- `SponsorSettings` 内取 `const { liquidGlassEnabled } = useBackground();`
  （该 hook 已在第 58 行导入）。
- 把现有的 `<Flex flexWrap="wrap" gap={3} justify="center">…</Flex>` 包进容器：
  - 玻璃开启：`<LiquidGlassCard className="no-bounce" p={4} boxShadow="sm">`；
  - 玻璃关闭：`<Box p={4} borderRadius="3xl" bg={useColorModeValue("white", "#111111")}
    border="1px solid" borderColor={useColorModeValue("gray.200", "#333333")}>`，
    即与今天「无面板、白底小卡片」的观感差异最小（若希望玻璃关闭时完全保持现状，
    可让容器在关闭态只渲染无背景的 `<Box p={0}>`，由用户确认取哪一种）。
- 容器加 `no-bounce`：避免子菜单切换时 1 个面板 + 200 个小卡一起播放果冻动画
  （`src/index.css:215-218` 已有 `.jelly-bounce-card.no-bounce` 的豁免规则）。
- `SponsorNameCard` 调用处（第 2895-2897 行）多传 `glass={liquidGlassEnabled}`；
  顺手把 `key={index}` 改成稳定 key（`key={`${sponsor.name}-${sponsor.amount}-${index}`}`）
  以免分批追加时无谓的复用抖动（行为不变）。

### 步骤 3（可选，独立可回退）：赞助数据模块级缓存

- `SettingsPage.tsx` 顶部加模块级 `let sponsorsCache: { update_time: string; list: SponsorItem[] } | null`
  与进行中的 Promise，`useEffect` 命中缓存时直接 `setSponsors(list)` 跳过 IPC + 5s 等待，
  并清除定时器逻辑的重复分批（缓存命中一次性渲染，200 条纯 Box 渲染成本可接受）。
- 若担心首次进入仍分批，可只缓存不改变分批阈值。
- 这一步只解决「重复进 tab 重新拉网络 + 重新分批」，与主卡顿无直接关系，可延后。

### 步骤 4（可选）：扫一遍同页其它高频卡片

- `SponsorSettings` 的两个二维码卡片（第 2810-2870 行）只有 2 个，无需处理。
- 若用户反馈其它设置子页也卡，再按同一思路处理列表型 `LiquidGlassCard`
  （如 `NetworkSettings` 第 2637-2710 行、`HotkeySettings` 第 3601-3909 行）；
  本计划不预先改动，避免超出范围。

## 风险

- 视觉差异：小卡片不再对背景做折射，只保留半透明 + 边框；多数情况下肉眼接近，
  但与「每卡独立折射」不完全一致。若用户要求严格保留单卡折射，需要走虚拟滚动或
  「只对 hover 卡片临时开启 blur」的替代方案，成本更高。
- 嵌套：面板自身有 `backdrop-filter`，其子元素改为 `content-visibility: auto` 后
  是否正确缓存面板背景模糊需实测（Chromium 一般正常；若发现面板模糊闪烁，
  就把小卡片的 `contentVisibility` 去掉）。小卡片目前仍是普通元素，风险低。
- 暗色模式取色：新常量必须与 `liquid-glass-card.tsx:32-42` 一致，否则深浅色下观感跳变。
- 玻璃关闭态的外观：取决于步骤 2 选「带面板」还是「无面板」，需在实现前用
  `ask_user` 或由用户直接指定，避免来回返工。
- 不要顺手改 `LiquidGlassCard` 默认样式（会波及 `Surface`、`DownloadAcceleratorPage` 等页面）。

## 验证方式

1. 类型/静态检查：`npx tsc --noEmit`；改动文件单独 lint：`npx eslint src/pages/SettingsPage.tsx`
   （仓库存在约 149 条历史 lint 错误，只需确认本次改动未引入新错误）。
2. 手工验证（`npm run dev` 或 `npm run tauri:dev`）：
   - 设置 → 外观 → 开启液态玻璃，进 设置 → 赞助：
     滚动名单应顺滑，鼠标划过卡片无掉帧；DevTools 元素面板里 backdrop-filter 元素数
     应从「≈ N（名单条数）」降到 1（可先用 `$(".jelly-bounce-card")` 计数对比）。
   - 子菜单来回切换（sponsor ↔ general）：不再出现整屏缩放卡顿。
   - 深色/浅色模式各看一遍；关闭液态玻璃后外观与改动前一致。
3. 性能对比（可选）：DevTools Performance 录制「滚动赞助名单 3 秒」，
   修复前后对比 `Paint`/`Raster` 时间与帧率（预期从掉帧明显到接近 60fps）。
