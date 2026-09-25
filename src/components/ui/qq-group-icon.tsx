import { Box, Spinner } from "@chakra-ui/react";
import { useQQIcon } from "@/hooks/use-qq-groups";

/**
 * QQ 群图标：走后端从 gitee 下载并经 Tauri 资产协议显示（WebView 直连 gitee 会加载不出）。
 * 加载期间显示旋转加载动画；下载失败才回退本地默认 QQ 图标。
 */
export function QqGroupIcon({ url, size }: { url?: string; size: number }) {
  const { src, loading } = useQQIcon(url);

  if (loading) {
    return (
      <Box
        w={`${size}px`}
        h={`${size}px`}
        display="flex"
        alignItems="center"
        justifyContent="center"
      >
        <Spinner size={size >= 24 ? "sm" : "xs"} color="#12B7F5" thickness="2px" />
      </Box>
    );
  }

  return (
    <img
      src={src || "/icons/qq.webp"}
      alt="QQ"
      width={size}
      height={size}
      style={{ objectFit: "cover" }}
    />
  );
}