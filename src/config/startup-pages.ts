/**
 * 可选的「默认启动页面」清单。
 * 只列一级页面（侧边栏能直接到达的），避免启动时落在一个需要上下文参数的子页面上。
 * App.tsx 的启动跳转与设置页的下拉框共用这份清单，改这里即可同时生效。
 */
export interface StartupPageOption {
  value: string;
  labelKey: string;
}

export const STARTUP_PAGES: StartupPageOption[] = [
  { value: "/", labelKey: "sidebar.home" },
  { value: "/hardware", labelKey: "sidebar.hardware" },
  { value: "/builtin-tools", labelKey: "sidebar.builtinTools" },
  { value: "/optimization", labelKey: "sidebar.optimization" },
  { value: "/system-optimizer", labelKey: "sidebar.systemOptimizer" },
  { value: "/music", labelKey: "sidebar.music" },
  { value: "/delta-force", labelKey: "sidebar.deltaForce" },
  { value: "/steam", labelKey: "sidebar.steam" },
  { value: "/epic-free", labelKey: "sidebar.epicFree" },
  { value: "/custom", labelKey: "sidebar.custom" },
  { value: "/settings", labelKey: "sidebar.settings" },
];

/** 白名单：防止历史脏数据把启动页跳到不存在的路由 */
export const STARTUP_PAGE_VALUES: string[] = STARTUP_PAGES.map((p) => p.value);

export const STARTUP_PAGE_STORE_KEY = "nexbox_startup_page";
