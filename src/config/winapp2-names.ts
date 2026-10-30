/**
 * Winapp2 规则库名称汉化
 *
 * Winapp2.ini 的条目名是社区维护的英文（如 `Windows Temporary Files`、
 * `Microsoft Edge Privacy Sandbox`），后端把它当作分类标识原样返回，所以深度清理里会看到英文。
 *
 * 翻译分三级（命中即返回）：
 *   1. EXACT  —— 常见条目整句翻译（质量最好）
 *   2. TAIL   —— 尾缀短语翻译，保留产品名前缀
 *   3. GLOSSARY —— 拆出品牌前缀后，用词表逐词翻译（`Windows Taskbar` → `Windows 任务栏`）
 * 三级都没命中就原样返回英文。
 *
 * 注意：翻译只用于「显示」，分类的 key / 选中状态仍然使用原始英文名，
 * 否则前后端匹配会失败。
 */

type Entry = { zh: string; tw?: string };

/** 一级：常见条目整句翻译 */
const EXACT: Record<string, Entry> = {
  "Windows Temporary Files": { zh: "系统临时文件", tw: "系統暫存檔案" },
  "Windows Logs": { zh: "系统日志", tw: "系統日誌" },
  "Windows Error Reporting": { zh: "Windows 错误报告", tw: "Windows 錯誤報告" },
  "Windows Error Reports": { zh: "Windows 错误报告", tw: "Windows 錯誤報告" },
  "Windows Font Cache": { zh: "Windows 字体缓存", tw: "Windows 字型快取" },
  "Windows Icon Cache": { zh: "Windows 图标缓存", tw: "Windows 圖示快取" },
  "Windows Thumbnail Cache": { zh: "Windows 缩略图缓存", tw: "Windows 縮圖快取" },
  "Windows.old": { zh: "旧版 Windows 安装文件", tw: "舊版 Windows 安裝檔案" },
  "Windows Defender": { zh: "Microsoft Defender 防病毒", tw: "Microsoft Defender 防毒" },
  "Windows Update": { zh: "Windows 更新缓存", tw: "Windows 更新快取" },
  "Windows Update Logs": { zh: "Windows 更新日志", tw: "Windows 更新日誌" },
  "Windows Prefetch": { zh: "Windows 预读取文件", tw: "Windows 預讀取檔案" },
  Prefetch: { zh: "预读取文件", tw: "預讀取檔案" },
  "Thumbnail Cache": { zh: "缩略图缓存", tw: "縮圖快取" },
  "Recycle Bin": { zh: "回收站", tw: "資源回收筒" },
  "Font Cache": { zh: "字体缓存", tw: "字型快取" },
  "Memory Dumps": { zh: "内存转储文件", tw: "記憶體傾印檔案" },
  "Old Windows Installation": { zh: "旧版 Windows 安装文件", tw: "舊版 Windows 安裝檔案" },
  "Clipboard Cache": { zh: "剪贴板缓存", tw: "剪貼簿快取" },
  "Delivery Optimization": { zh: "传递优化文件", tw: "傳遞最佳化檔案" },
  "DirectX Shader Cache": { zh: "DirectX 着色器缓存", tw: "DirectX 著色器快取" },
  "NVIDIA Shader Cache": { zh: "NVIDIA 着色器缓存", tw: "NVIDIA 著色器快取" },
  "AMD Shader Cache": { zh: "AMD 着色器缓存", tw: "AMD 著色器快取" },

  // Windows 内置应用 / 系统组件
  "Windows Taskbar": { zh: "Windows 任务栏", tw: "Windows 工作列" },
  "Windows Photos": { zh: "Windows 照片", tw: "Windows 相片" },
  "Windows Notepad": { zh: "Windows 记事本", tw: "Windows 記事本" },
  "Windows Search": { zh: "Windows 搜索", tw: "Windows 搜尋" },
  "Windows Widgets": { zh: "Windows 小组件", tw: "Windows 小工具" },
  "Windows Paint": { zh: "Windows 画图", tw: "Windows 小畫家" },
  "Windows Camera": { zh: "Windows 相机", tw: "Windows 相機" },
  "Windows Store": { zh: "Microsoft Store 缓存", tw: "Microsoft Store 快取" },
  "Windows Jump Lists": { zh: "Windows 跳转列表", tw: "Windows 跳躍清單" },
  "Windows Search Index": { zh: "Windows 搜索索引", tw: "Windows 搜尋索引" },
  "Windows Reliability Monitor": { zh: "Windows 可靠性监视器", tw: "Windows 可靠性監視器" },
  "Windows Notification Cache": { zh: "Windows 通知缓存", tw: "Windows 通知快取" },
  "Windows Event Logs": { zh: "Windows 事件日志", tw: "Windows 事件日誌" },
  "Windows Installer Cache": { zh: "Windows 安装包缓存", tw: "Windows 安裝程式快取" },
  "Windows Program Compatibility Assistant": {
    zh: "Windows 程序兼容性助手",
    tw: "Windows 程式相容性助理",
  },
  "Program Compatibility Assistant": { zh: "程序兼容性助手", tw: "程式相容性助理" },

  // Edge / Chrome 子项
  "Microsoft Edge Privacy Sandbox": { zh: "Microsoft Edge 隐私沙盒", tw: "Microsoft Edge 隱私沙盒" },
  "Microsoft Edge Shopping Insights": { zh: "Microsoft Edge 购物洞察", tw: "Microsoft Edge 購物洞察" },
  "Google Chrome Privacy Sandbox": { zh: "Google Chrome 隐私沙盒", tw: "Google Chrome 隱私沙盒" },
  "Google Chrome Shopping Insights": { zh: "Google Chrome 购物洞察", tw: "Google Chrome 購物洞察" },

  // 常见第三方
  "Microsoft Edge Caches": { zh: "Microsoft Edge 缓存", tw: "Microsoft Edge 快取" },
  "Google Chrome Caches": { zh: "Google Chrome 缓存", tw: "Google Chrome 快取" },
  "Firefox Cache": { zh: "Firefox 缓存", tw: "Firefox 快取" },
  "Discord Cache": { zh: "Discord 缓存", tw: "Discord 快取" },
  "Steam Logs": { zh: "Steam 日志", tw: "Steam 日誌" },
  "Steam Cache": { zh: "Steam 缓存", tw: "Steam 快取" },
  "Steam Shader Cache": { zh: "Steam 着色器缓存", tw: "Steam 著色器快取" },
  "Spotify Cache": { zh: "Spotify 缓存", tw: "Spotify 快取" },
  "Telegram Cache": { zh: "Telegram 缓存", tw: "Telegram 快取" },
  "WeChat Cache": { zh: "微信缓存", tw: "微信快取" },
  "QQ Cache": { zh: "QQ 缓存", tw: "QQ 快取" },
  "Adobe Cache": { zh: "Adobe 缓存", tw: "Adobe 快取" },
  "Office Cache": { zh: "Office 缓存", tw: "Office 快取" },
  "Visual Studio Cache": { zh: "Visual Studio 缓存", tw: "Visual Studio 快取" },
};

/** 二级：尾缀短语（按长度从长到短匹配，保留前面的产品名） */
const TAIL_PHRASES: Array<{ en: string } & Entry> = [
  { en: "Autofill Data & Search Engine Preferences", zh: "自动填充与搜索引擎设置", tw: "自動填入與搜尋引擎設定" },
  { en: "Saved Usernames & Passwords", zh: "保存的账号与密码", tw: "儲存的帳號與密碼" },
  { en: "Security & Threat Detection", zh: "安全与威胁检测数据", tw: "安全性與威脅偵測資料" },
  { en: "Progressive Web Apps", zh: "PWA 应用数据", tw: "PWA 應用程式資料" },
  { en: "Storage Quota Manager", zh: "存储配额数据", tw: "儲存配額資料" },
  { en: "Web Browsing Cookies", zh: "浏览记录 Cookie", tw: "瀏覽紀錄 Cookie" },
  { en: "Web Browsing History", zh: "浏览历史", tw: "瀏覽歷史" },
  { en: "Web Browsing Session", zh: "浏览会话数据", tw: "瀏覽工作階段資料" },
  { en: "Bookmark Backups", zh: "书签备份", tw: "書籤備份" },
  { en: "Bookmark Favicons", zh: "书签图标缓存", tw: "書籤圖示快取" },
  { en: "Download History", zh: "下载历史", tw: "下載歷史" },
  { en: "Default App Handlers", zh: "默认应用关联", tw: "預設應用程式關聯" },
  { en: "Temporary Files", zh: "临时文件", tw: "暫存檔案" },
  { en: "Temp Files", zh: "临时文件", tw: "暫存檔案" },
  { en: "Cache Files", zh: "缓存文件", tw: "快取檔案" },
  { en: "Crash Reports", zh: "崩溃报告", tw: "當機報告" },
  { en: "Crash Dumps", zh: "崩溃转储", tw: "當機傾印" },
  { en: "Error Reports", zh: "错误报告", tw: "錯誤報告" },
  { en: "Setup Logs", zh: "安装日志", tw: "安裝日誌" },
  { en: "Update Cache", zh: "更新缓存", tw: "更新快取" },
  { en: "Installer Cache", zh: "安装包缓存", tw: "安裝程式快取" },
  { en: "Session Data", zh: "会话数据", tw: "工作階段資料" },
  { en: "Web Storage", zh: "网页存储数据", tw: "網頁儲存資料" },
  { en: "Sync Data", zh: "同步数据", tw: "同步資料" },
  { en: "Telemetry", zh: "遥测数据", tw: "遙測資料" },
  { en: "Pinned Tabs", zh: "固定标签页", tw: "釘選分頁" },
  { en: "Log Files", zh: "日志文件", tw: "日誌檔案" },
  { en: "Caches", zh: "缓存", tw: "快取" },
  { en: "Cache", zh: "缓存", tw: "快取" },
  { en: "Logs", zh: "日志", tw: "日誌" },
  { en: "Log", zh: "日志", tw: "日誌" },
  { en: "History", zh: "历史记录", tw: "歷史紀錄" },
  { en: "Cookies", zh: "Cookie", tw: "Cookie" },
  { en: "Reports", zh: "报告", tw: "報告" },
  { en: "Dumps", zh: "转储文件", tw: "傾印檔案" },
  { en: "Settings", zh: "设置", tw: "設定" },
  { en: "Backups", zh: "备份", tw: "備份" },
];

/** 三级：品牌前缀（保留原样，不翻译） */
const BRANDS = [
  "Microsoft Edge",
  "Google Chrome",
  "Microsoft Office",
  "Microsoft Store",
  "Microsoft Teams",
  "Mozilla Firefox",
  "Visual Studio Code",
  "Visual Studio",
  "Epic Games",
  "Battle.net",
  "NVIDIA",
  "Realtek",
  "Logitech",
  "JetBrains",
  "OneDrive",
  "Dropbox",
  "Firefox",
  "Chrome",
  "Edge",
  "Windows",
  "Microsoft",
  "Google",
  "Adobe",
  "Steam",
  "Discord",
  "Spotify",
  "Telegram",
  "WhatsApp",
  "WeChat",
  "Zoom",
  "Slack",
  "Teams",
  "Office",
  "Opera",
  "Brave",
  "Vivaldi",
  "Safari",
  "Unity",
  "Unreal",
  "Intel",
  "AMD",
  "Apple",
  "iTunes",
  "OBS",
  "VLC",
  "WinRAR",
  "7-Zip",
  "Notepad++",
  "Tencent",
  "Baidu",
  "WPS",
  "Xbox",
  "Bing",
  "Cortana",
  "Skype",
  "LinkedIn",
  "PayPal",
  "Java",
  "Python",
  "Node.js",
  "Git",
  "Docker",
];

/** 三级：词表（小写 key，支持多词短语，匹配时优先长词） */
const GLOSSARY: Record<string, string> = {
  // 系统组件
  "program compatibility assistant": "程序兼容性助手",
  "privacy sandbox": "隐私沙盒",
  "shopping insights": "购物洞察",
  "search index": "搜索索引",
  "jump lists": "跳转列表",
  "event logs": "事件日志",
  "reliability monitor": "可靠性监视器",
  "notification cache": "通知缓存",
  "start menu": "开始菜单",
  "task view": "任务视图",
  "action center": "操作中心",
  "control panel": "控制面板",
  "device manager": "设备管理器",
  "event viewer": "事件查看器",
  "scheduled tasks": "计划任务",
  "system restore": "系统还原",
  "system volume information": "系统卷信息",
  "file explorer": "文件资源管理器",
  "internet explorer": "Internet Explorer",
  "windows media player": "Windows Media Player",
  "media player": "媒体播放器",
  "photo viewer": "照片查看器",
  "sticky notes": "便笺",
  "snipping tool": "截图工具",
  "task scheduler": "任务计划程序",
  "credential manager": "凭据管理器",
  "font cache": "字体缓存",
  "thumbnail cache": "缩略图缓存",
  "icon cache": "图标缓存",
  "shell bags": "Shell 记录",
  "recent items": "最近使用记录",

  // 通用名词
  cleanup: "清理",
  font: "字体",
  thumbnail: "缩略图",
  icon: "图标",
  driver: "驱动",
  virtual: "虚拟",
  shared: "共享",
  user: "用户",
  users: "用户",
  system: "系统",
  prefetch: "预读取",
  minidump: "小型转储",
  feedback: "反馈",
  troubleshooting: "故障排除",
  support: "支持",
  help: "帮助",
  tips: "提示",
  "search history": "搜索历史",
  "typed urls": "输入的网址",
  "top sites": "常用网站",
  favicons: "网站图标",
  tabs: "标签页",
  tab: "标签页",
  lockscreen: "锁屏",
  "lock screen": "锁屏",
  tiles: "磁贴",
  "live tiles": "动态磁贴",
  cloud: "云",
  "sync data": "同步数据",
  "autofill data": "自动填充数据",
  "shader cache": "着色器缓存",
  shader: "着色器",
  "cache data": "缓存数据",
  "installer logs": "安装日志",
  "app data": "应用数据",
  "user data": "用户数据",
  taskbar: "任务栏",
  widgets: "小组件",
  photos: "照片",
  notepad: "记事本",
  search: "搜索",
  paint: "画图",
  camera: "相机",
  clock: "时钟",
  weather: "天气",
  news: "新闻",
  store: "商店",
  gaming: "游戏",
  game: "游戏",
  games: "游戏",
  assistant: "助手",
  sandbox: "沙盒",
  insights: "洞察",
  compatibility: "兼容性",
  program: "程序",
  programs: "程序",
  applications: "应用",
  application: "应用",
  apps: "应用",
  app: "应用",
  components: "组件",
  component: "组件",
  packages: "安装包",
  package: "安装包",
  installer: "安装程序",
  setup: "安装",
  uninstall: "卸载",
  update: "更新",
  updates: "更新",
  upgrade: "升级",
  "upgrade logs": "升级日志",
  temporary: "临时",
  temp: "临时",
  files: "文件",
  file: "文件",
  folders: "文件夹",
  folder: "文件夹",
  directory: "目录",
  data: "数据",
  database: "数据库",
  settings: "设置",
  preferences: "偏好设置",
  profiles: "配置文件",
  profile: "配置文件",
  sessions: "会话",
  session: "会话",
  storage: "存储",
  downloads: "下载",
  uploads: "上传",
  history: "历史记录",
  bookmarks: "书签",
  passwords: "密码",
  cookies: "Cookie",
  extensions: "扩展",
  plugins: "插件",
  themes: "主题",
  wallpaper: "壁纸",
  icons: "图标",
  thumbnails: "缩略图",
  fonts: "字体",
  drivers: "驱动",
  backup: "备份",
  restore: "还原",
  recovery: "恢复",
  diagnostics: "诊断",
  telemetry: "遥测",
  reports: "报告",
  report: "报告",
  dumps: "转储文件",
  crash: "崩溃",
  crashes: "崩溃",
  errors: "错误",
  error: "错误",
  warnings: "警告",
  logs: "日志",
  log: "日志",
  cache: "缓存",
  caches: "缓存",
  memory: "内存",
  clipboard: "剪贴板",
  notification: "通知",
  notifications: "通知",
  registry: "注册表",
  services: "服务",
  service: "服务",
  processes: "进程",
  process: "进程",
  network: "网络",
  internet: "互联网",
  browser: "浏览器",
  browsers: "浏览器",
  media: "媒体",
  music: "音乐",
  videos: "视频",
  video: "视频",
  pictures: "图片",
  images: "图片",
  documents: "文档",
  desktop: "桌面",
  shortcuts: "快捷方式",
  previews: "预览",
  preview: "预览",
  autofill: "自动填充",
  "passwords manager": "密码管理器",
  sync: "同步",
  "telemetry data": "遥测数据",
  tracking: "跟踪",
  statistics: "统计",
  metrics: "指标",
  "logs and reports": "日志与报告",
  bin: "箱",
  player: "播放器",
  tools: "工具",
  tool: "工具",
  utility: "实用工具",
  utilities: "实用工具",
  client: "客户端",
  launcher: "启动器",
  loader: "加载器",
  core: "核心",
  engine: "引擎",
  module: "模块",
  library: "库",
  libraries: "库",
  test: "测试",
  debug: "调试",
  dump: "转储",
  map: "地图",
  maps: "地图",
  "weather data": "天气数据",
  voice: "语音",
  audio: "音频",
  sound: "声音",
  recorder: "录制器",
  capture: "捕获",
  screenshot: "截图",
  screenshots: "截图",
  editor: "编辑器",
  viewer: "查看器",
  manager: "管理器",
  center: "中心",
  panel: "面板",
  bar: "栏",
  menu: "菜单",
  list: "列表",
  lists: "列表",
  index: "索引",
  queue: "队列",
  job: "作业",
  tasks: "任务",
  task: "任务",
  "history data": "历史数据",
};

const isCJK = (s: string) => /[\u4e00-\u9fff]/.test(s);

/** 中文与英文之间加空格，中文之间直接相连 */
function joinPieces(pieces: string[]): string {
  let out = "";
  let last = "";
  for (const piece of pieces) {
    if (!piece) continue;
    if (piece === last) continue;
    last = piece;
    if (!out) {
      out = piece;
      continue;
    }
    const needSpace = !(isCJK(out.slice(-1)) && isCJK(piece[0]));
    out += (needSpace ? " " : "") + piece;
  }
  return out;
}

/** 用词表逐词翻译（优先匹配最长短语），返回结果与命中次数 */
function translateByGlossary(text: string): { out: string; hits: number } {
  const words = text.split(/\s+/).filter(Boolean);
  const pieces: string[] = [];
  let i = 0;
  let hits = 0;
  while (i < words.length) {
    let matched = false;
    for (let len = Math.min(4, words.length - i); len >= 1; len--) {
      const phrase = words.slice(i, i + len).join(" ").toLowerCase();
      const zh = GLOSSARY[phrase];
      if (zh) {
        pieces.push(zh);
        i += len;
        hits++;
        matched = true;
        break;
      }
    }
    if (!matched) {
      pieces.push(words[i]);
      i++;
    }
  }
  return { out: joinPieces(pieces), hits };
}

/** 拆出品牌前缀（最长匹配） */
function splitBrand(raw: string): { brand: string; rest: string } {
  const lower = raw.toLowerCase();
  let best = "";
  for (const brand of BRANDS) {
    if (lower === brand.toLowerCase()) return { brand, rest: "" };
    if (lower.startsWith(brand.toLowerCase() + " ") && brand.length > best.length) {
      best = brand;
    }
  }
  if (!best) return { brand: "", rest: raw };
  return { brand: raw.slice(0, best.length), rest: raw.slice(best.length + 1) };
}

const SORTED_TAILS = [...TAIL_PHRASES].sort((a, b) => b.en.length - a.en.length);

/**
 * 把 Winapp2 条目名翻成中文（仅 zh 系列语言生效）。
 * 其它语言、或完全没有命中翻译时，原样返回英文。
 */
export function localizeWinapp2Name(raw: string, lang: string): string {
  if (!raw) return raw;
  const lowerLang = lang.toLowerCase();
  if (!lowerLang.startsWith("zh")) return raw;
  const isTW = lowerLang.startsWith("zh-tw") || lowerLang.startsWith("zh-hant");
  const pick = (entry: Entry) => (isTW ? entry.tw || entry.zh : entry.zh);

  // 1) 整句
  const exact = EXACT[raw];
  if (exact) return pick(exact);

  // 2) 尾缀短语
  const lower = raw.toLowerCase();
  for (const tail of SORTED_TAILS) {
    const suffix = tail.en.toLowerCase();
    if (lower === suffix) return pick(tail);
    if (lower.endsWith(" " + suffix)) {
      const prefix = raw.slice(0, raw.length - tail.en.length).trim();
      return prefix ? `${prefix} ${pick(tail)}` : pick(tail);
    }
  }

  // 3) 品牌 + 词表逐词翻译
  const { brand, rest } = splitBrand(raw);
  if (rest) {
    const { out, hits } = translateByGlossary(rest);
    if (hits > 0) return brand ? `${brand} ${out}` : out;
    return raw;
  }
  const { out, hits } = translateByGlossary(raw);
  return hits > 0 ? out : raw;
}
