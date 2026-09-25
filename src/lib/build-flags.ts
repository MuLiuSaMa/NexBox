/**
 * 构建期标志：商店版（MSIX）构建。
 *
 * 由 `npm run build:store`（vite build --mode store，读取 .env.store）注入。
 * 商店政策 10.1.5 / 10.2.3 要求产品不得引导用户到商店之外获取、安装非本人发布的软件，
 * 因此商店版不展示「工具箱」的全部栏目（官方推荐、社区工具、第三方工具、外设驱动）。
 * 官网安装版（npm run build / npm run tauri:build）该标志为 false，功能不受影响。
 */
export const IS_STORE_BUILD = import.meta.env.VITE_STORE_BUILD === "true";