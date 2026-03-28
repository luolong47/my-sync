# Vite 8 最佳实践（my-sync 项目版）

## 1. 目的

本文档不是通用模板，而是结合本仓库当前技术栈（`Vue 3 + Quasar + Tailwind + Tauri 2`）整理的 Vite 8 落地建议，目标是：

- 升级到 Vite 8 时尽量平滑，不破坏现有 `tauri dev / tauri build` 流程。
- 保持开发体验（固定端口、HMR、错误可见性）与当前一致。
- 给出可执行、可回滚的升级检查清单。

## 2. 项目现状（2026-03-29）

- 前端构建核心：`vite@^6.0.3`
- 关键插件：`@vitejs/plugin-vue@^5.2.1`、`@quasar/vite-plugin@^1.11.0`、`@tailwindcss/vite@^4.2.2`
- Vite 配置文件：`vite.config.ts`
- Tauri 构建联动：
  - `src-tauri/tauri.conf.json` 中 `beforeDevCommand = "pnpm dev"`
  - `src-tauri/tauri.conf.json` 中 `beforeBuildCommand = "pnpm build:web"`
  - `frontendDist = "../dist/app"`，与 `vite.config.ts` 的 `build.outDir = "dist/app"` 对齐
- 当前开发约束：
  - 固定端口 `1420`，`strictPort: true`
  - `clearScreen: false`（保留 Rust 错误输出）
  - 通过 `TAURI_DEV_HOST` 注入 HMR 主机

## 3. 升级 Vite 8 前置条件

1. Node 版本先满足 Vite 8 最低要求：`20.19+` 或 `22.12+`（推荐直接用 Node 22 LTS）。
2. 升级前确保工作区干净，便于定位问题与回滚。
3. 先看插件的 `peerDependencies`，确保 Vue/Quasar/Tailwind 的 Vite 插件版本与 Vite 8 匹配。

## 4. 结合本项目的最佳实践

### 4.1 迁移策略：先最小升级，再做优化

建议分两阶段执行：

- 阶段 A（最小可用）：只做依赖升级，让 `pnpm dev`、`pnpm tauri dev`、`pnpm build:web` 跑通。
- 阶段 B（性能与可维护）：再引入 `server.warmup`、`optimizeDeps.include`、按需拆分等优化项。

这样可以把“兼容性问题”和“优化引入的问题”分开，排查成本最低。

### 4.2 保留并强化当前 Tauri 友好配置

当前 `vite.config.ts` 已经有三项对 Tauri 很重要的设置，应继续保留：

- `clearScreen: false`
- `server.strictPort: true` + `server.port: 1420`
- `server.watch.ignored: ["**/src-tauri/**"]`

同时建议增加两条安全习惯：

- 不要把 `server.cors` 全局开成 `true`。
- 不要把 `server.allowedHosts` 直接设成 `true`。

### 4.3 插件与配置组织

- 插件顺序维持“样式增强 -> Vue 编译 -> Quasar”：`tailwindcss()` -> `vue()` -> `quasar()`。
- 升级后优先保持配置简洁，避免一次引入过多新选项。
- 只有在需要时再引入 `build.rolldownOptions` / `worker.rolldownOptions` 等高级配置，避免过早复杂化。

### 4.4 构建流程与版本策略

本项目 `build:web` 会先执行 `scripts/version-bump.mjs patch`，再执行 `vite build`。建议：

- 将 `build:web` 视为“发布构建”命令，日常本地验证优先用 `pnpm dev` 或 `pnpm tauri dev`。
- 如需纯验证构建而不改版本，可新增脚本（例如 `build:web:check`）跳过版本号递增。
- 继续保持 `dist/app` 与 `tauri.conf.json.frontendDist` 一致，避免桌面构建找不到前端产物。

### 4.5 性能实践（针对当前代码结构）

当前 `src/App.vue` 顶层静态引入了多个 View 组件。建议在确认体验可接受时逐步改为按需异步加载，降低首屏构建与启动体积。

可考虑的优化方向：

- 高频依赖预构建：在 Vite 扫描不到但经常使用时，增加 `optimizeDeps.include`。
- 热路径预热：为固定高频入口增加 `server.warmup`（例如主布局和常用视图）。
- 继续避免 barrel file（`index.ts` 全量 re-export）模式，减少无效模块扫描。

### 4.6 环境变量与安全边界

- 只把可公开信息放入 `VITE_*` 变量（这些会进入前端包）。
- 密钥类信息不要放在前端环境变量里，放到后端或安全存储侧。
- 当前使用的 `TAURI_DEV_HOST` 属于开发端注入，继续用于 dev/HMR 场景即可。

## 5. 升级执行清单（可直接照做）

1. 检查 Node 版本：
   - `node -v`
2. 升级 Vite 及相关插件（按实际 peer 提示调整）：
   - `pnpm up vite @vitejs/plugin-vue @quasar/vite-plugin @tailwindcss/vite --latest`
3. 安装并检查依赖一致性：
   - `pnpm install`
4. 验证前端开发：
   - `pnpm dev`
5. 验证桌面联调：
   - `pnpm tauri dev`
6. 验证前端发布构建：
   - `pnpm build:web`
7. 验证桌面产物链路：
   - `pnpm build:portable`

## 6. 常见风险与回滚建议

- 风险 1：插件 `peerDependencies` 不匹配，导致启动报错。
  - 处理：按报错提示对齐插件主版本，不要只升 `vite` 单包。
- 风险 2：`TAURI_DEV_HOST` 场景下 HMR 连接异常。
  - 处理：优先检查 `host / hmr.port(1421)` 和本机防火墙配置。
- 风险 3：构建通过但 Tauri 找不到前端资源。
  - 处理：核对 `vite.build.outDir` 与 `tauri.conf.json.frontendDist` 是否仍然一致。

回滚建议：

- 保留一次“升级前”提交点。
- 升级失败时优先回退 `package.json` 与锁文件，再逐步重试。

## 7. 参考（官方）

- https://vite.dev/blog/announcing-vite8
- https://vite.dev/guide/migration
- https://vite.dev/guide/performance.html
- https://vite.dev/guide/dep-pre-bundling.html
- https://vite.dev/config/server-options
- https://vite.dev/config/build-options
- https://vite.dev/guide/env-and-mode
