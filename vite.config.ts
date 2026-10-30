import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig(async () => ({
  plugins: [
    react({
      jsxImportSource: "@emotion/react",
      babel: {
        plugins: ["@emotion/babel-plugin"],
      },
    }),
  ],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 3000,
    host: "127.0.0.1",
    strictPort: true,
    watch: {
      // 这些目录是构建产物 / 其它 Rust 工程，不需要热更新监听。
      // 尤其 dist 与临时构建目录：dev 期间往里写文件再删除，会让 chokidar
      // 抛 EBUSY（resource busy or locked）并直接终止 tauri dev。
      ignored: [
        "**/src-tauri/**",
        "**/dist/**",
        "**/.verify-build*/**",
        "**/installer/**",
        "**/uninstaller/**",
      ],
    },
  },
}));
