/**
 * 主内容区顶部渐隐带高度（px）。
 * 与 48px 标题栏同高：内容滑到标题栏下方时开始淡出，而不是被 overflow 硬切。
 */
export const APP_TOP_FADE_PX = 48;

/**
 * 顶部 alpha 渐隐：作用在滚动容器自身，只改内容不透明度，不新增任何遮挡层。
 *
 * 注意：带 mask 的元素会成为 Backdrop Root，其后代 `backdrop-filter`（液态玻璃卡片）
 * 采样不到容器外的背景层，玻璃会失效。因此液态玻璃开启时必须停用（见 MainLayout）。
 */
export const APP_TOP_FADE_MASK = `linear-gradient(to bottom, rgba(0,0,0,0) 0px, rgba(0,0,0,1) ${APP_TOP_FADE_PX}px)`;
