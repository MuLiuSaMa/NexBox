# 贡献指南

感谢你对 NexBox（新境盒）的关注！欢迎以任何形式参与贡献：报告问题、提出建议、完善文档或提交代码。

## 提出问题与建议

提交 Issue 前请先搜索现有 Issue，避免重复。报告 Bug 时请尽量附上：

- 操作系统版本与 NexBox 版本号
- 完整的复现步骤（能稳定复现最佳）
- 期望行为与实际行为
- 相关日志或截图（如有）

功能建议请说明使用场景与期望效果，越具体越容易被采纳。

## 开发环境

| 工具                          | 版本要求                             |
| ----------------------------- | ------------------------------------ |
| **Node.js**                   | >= 18.x（建议 20+）                  |
| **Rust**                      | >= 1.77.2                            |
| **Visual Studio Build Tools** | Windows 专用（C++ 桌面开发工作负载） |

```bash
# 1. 克隆仓库
git clone https://github.com/MuLiuSaMa/NexBox.git
cd NexBox

# 2. 安装依赖
npm install

# 3. 启动开发模式（带热重载）
npm run tauri:dev
```

项目结构与技术栈详见 [README](README.md)。

## 代码规范

### 前端（React + TypeScript）

- 提交前运行 `npm run lint` 与 `npm run format`，确保通过 ESLint 检查并统一 Prettier 格式
- `npm run build` 会执行 TypeScript 类型检查，类型错误会导致 CI 失败

### 后端（Rust）

- 新增 Tauri 命令后需在 `src-tauri/src/lib.rs` 中注册
- 提交前运行 `cargo check` 确认编译通过

### 功能开发约定

- 设置项的存储键使用 kebab-case（如 `island-liquid-glass-enabled`）
- 新增用户可见文案时，需同步更新全部 6 个语言包：`zh` / `zh-TW` / `en` / `ja` / `fr` / `de`
- 新界面控件遵循现有主题规范：使用主题色变量（边框、焦点色等），不写死颜色值

## 提交规范

Commit message 使用 [Conventional Commits](https://www.conventionalcommits.org/zh-hans/) 风格：

```
feat: 新增音频均衡器预设导入
fix: 修复桌面歌词在多显示器下的位置偏移
docs: 更新 README 功能列表
```

常用类型：`feat`（新功能）、`fix`（修复）、`docs`（文档）、`refactor`（重构）、`perf`（性能）、`chore`（杂项）。描述建议使用中文，简洁说明改动内容。

## Pull Request 流程

1. Fork 本仓库并创建特性分支：`git checkout -b feature/amazing-feature`
2. 提交更改（遵循上述提交规范）
3. 推送分支：`git push origin feature/amazing-feature`
4. 发起 Pull Request，清晰描述改动内容与动机

CI 会自动执行前端 lint / build 与 Rust `cargo check`，请确保全部通过。PR 尽量保持小而聚焦，一个 PR 只解决一件事。

## 许可证

提交代码即表示你同意将贡献内容以 [GPL-3.0](LICENSE) 协议授权发布。
