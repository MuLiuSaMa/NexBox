#!/usr/bin/env node
/**
 * 打包构建：类型检查与 Vite 打包并行跑（两者互不依赖），总耗时从
 * `tsc` + `vite build` 串行的 40s 降到两者较慢的那个。
 *
 * 用法：
 *   node scripts/build.mjs            # 正式构建
 *   node scripts/build.mjs store      # 商店构建（vite build --mode store）
 */
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const mode = process.argv[2];

const tscBin = path.join(root, "node_modules/typescript/bin/tsc");
const viteBin = path.join(root, "node_modules/vite/bin/vite.js");

function run(label, args) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, args, { cwd: root, stdio: "inherit" });
    child.on("error", (err) => {
      console.error(`[build] ${label} 启动失败:`, err.message);
      resolve(1);
    });
    child.on("exit", (code) => {
      if (code !== 0) console.error(`[build] ${label} 失败，退出码 ${code}`);
      resolve(code ?? 1);
    });
  });
}

if (!fs.existsSync(tscBin) || !fs.existsSync(viteBin)) {
  console.error("[build] 找不到 tsc / vite，请先 npm install");
  process.exit(1);
}

const viteArgs = [viteBin, "build", ...(mode ? ["--mode", mode] : [])];
const [typecheckCode, bundleCode] = await Promise.all([
  run("类型检查", [tscBin, "-p", "tsconfig.json"]),
  run("Vite 打包", viteArgs),
]);

process.exit(typecheckCode === 0 && bundleCode === 0 ? 0 : 1);
