# 全局搜索加一个「最近访问」区

日期：2026-11-14
状态：待确认（plan 模式，未改动任何代码）
说明：这是一份试水用的计划草稿，题目选的是小而完整、不会碰系统底层的一处改动。

## 目标

全局搜索框（Ctrl+K / 点搜索框）现在只能「输入关键词 → 出结果」。打开时是一片空白，用户每次都得重新打一遍「显卡」「内存」。要做的是：

1. 输入框为空、面板刚展开时，直接显示最近点开过的 5 个条目，按时间倒序。
2. 有输入关键词时，照旧显示原有分组结果，不掺「最近访问」。
3. 最近访问列表能清空（面板右上角一个小叉），最多存 20 条，展示前 5 条。
4. 关掉应用再打开，记录还在。

## 现状

- 搜索面板：`src/components/ui/global-search.tsx`
  - `isOpen` 控制展开，`query` / `debouncedQuery` 控制关键词，`matchSearch`（:37）做匹配。
  - `allSearchItems`（:141）把静态索引和第三方工具拼在一起。
  - 结果渲染在 `:333` 起的 `categoryOrder.map` 里，`handleSelect`（:203）负责跳转或 `invoke("run_tool")`。
  - `totalResults === 0` 时显示「未找到 / 输入关键词搜索」（:326）。
- 索引定义：`src/config/search-index.ts`，`SearchItem` 有 `id` / `nameKey` / `path` / `icon` / `category`。
- 持久化：`src/lib/store.ts` 导出的 `store` 是 `settings.json` 的 `LazyStore`，音乐播放器等已经用它存配置。

## 改动步骤

### 1. 加一个小的存储工具

新增 `src/lib/search-history.ts`：

- `const KEY = "globalSearchRecent"`，走 `store`（`get` / `set` / `save`）。
- `loadRecentSearches(): Promise<string[]>` —— 读出来，过滤掉不存在的 id。
- `pushRecentSearch(id: string): Promise<void>` —— 去重后插到最前，截断到 20 条，写回并 `save()`。
- `clearRecentSearches(): Promise<void>`。
- 内部加一个模块级缓存，避免每次展开面板都读一遍磁盘。

### 2. 面板里接上

`src/components/ui/global-search.tsx`：

- 新增 state：`recentIds: string[]`。
- 面板展开时（`isOpen` 变 true）`loadRecentSearches()` 一次，`setRecentIds`。
- `handleSelect`（:203）里，跳转/运行工具成功之后调 `pushRecentSearch(item.id)`，并同步更新本地 `recentIds`（乐观更新，不 await 阻塞关闭）。
- 新增一个 `recentItems` memo：`recentIds.map(id => allSearchItems.find(i => i.id === id)).filter(Boolean).slice(0, 5)`。
- 渲染分支改成三段：
  - `query.trim() === "" && recentItems.length > 0` → 渲染「最近访问」标题 + 列表，复用现有结果行样式（`:346` 那个 `Box`，可以抽成局部 `ResultRow` 组件避免复制粘贴）。
  - `query.trim() === "" && recentItems.length === 0` → 保持现在的「输入关键词搜索」。
  - 有关键词 → 现有分组逻辑不动。

### 3. 清空按钮

- 在「最近访问」标题行右侧放一个小 `IconButton`（`lucide-react` 的 `X`，`boxSize={3}`），点击调 `clearRecentSearches()` 并 `setRecentIds([])`。
- 只在「最近访问」区显示，不影响原有布局。

### 4. 文案

六个语言文件都要加两个 key，放在 `search` 下（`src/locales/{zh,zh-TW,en,ja,fr,de}.json`，`"search"` 节点在 zh.json:3365 附近）：

```json
"search": {
  "recent": "最近访问",
  "clearRecent": "清空最近访问"
}
```

其余五个文件对应翻译；缺 key 时 i18next 会回落成 key 名，所以不能漏。

## 风险

- **键盘导航**：`flatResults` 是方向键 + Enter 的索引来源。最近访问列表必须也进 `flatResults`，否则鼠标能点、键盘选不中。处理方式：关键词为空时把 `flatResults` 设成 `recentItems`，非空时保持现有逻辑。
- **`SearchItem` 复用**：最近访问按 id 反查，如果某个条目被删（第三方工具卸载、索引改名），反查得到 `undefined`，必须 filter 掉，否则渲染会崩。
- **store 竞态**：`LazyStore` 的 `save()` 是异步的，连续点两下可能乱序。做法是写操作串行化（一个 pending Promise 链），不并发 `set`。
- **首次展开闪一下**：读磁盘是异步的，会先渲染「输入关键词搜索」再跳到最近访问。可以接受；要更稳就先渲染骨架或干脆在 `isOpen` 打开前预读。

## 验证

1. `npm run build`（内含 `tsc`），exit code 0。
2. `npm run tauri:dev`，手动走一遍：
   - 打开面板 → 空关键词显示「最近访问」（首次为空，显示原提示语）。
   - 点三个不同条目 → 再打开，顺序是「最近点的最前」。
   - 输入关键词 → 只剩分组结果，无「最近访问」。
   - 方向键 ↓ 能选到最近访问里的项，Enter 能跳转。
   - 点清空 → 列表消失，提示语回来。
   - 完全退出应用再开 → 记录仍在。
3. 商店版（`npm run build:store`）下第三方工具不进结果，最近访问里也不应出现第三方工具条目。
