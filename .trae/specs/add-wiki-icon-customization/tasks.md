# Tasks

- [x] Task 1: 新建 Wiki 图标清单 `src/assets/wiki-icons.json`
  - [x] SubTask 1.1: 从 Wiki 索引抽取全部单位 / 方块 / 液体 / 状态内部名
  - [x] SubTask 1.2: 按 `{ units: string[], blocks: string[], liquids: string[], statuses: string[] }` 结构写入 JSON
  - [x] SubTask 1.3: 在 `src/types.ts` 增加 `WikiIconManifest` 类型与 `WIKI_ICON_BASE` 常量及 `buildWikiIconUrl(type, name)` 工具函数（实现于 `src/wikiIcons.ts`）

- [x] Task 2: 新建启动期随机化 Hook `src/hooks/useWikiIcons.ts`
  - [x] SubTask 2.1: 实现 `useWikiIcons()`：首次挂载时一次性随机选取 1 个单位光标、18 个浮动图标、4 个不重复方块通道图标
  - [x] SubTask 2.2: 预加载光标图标，成功后写入 `document.documentElement.style.setProperty('--app-cursor-url', ...)`；失败则不写入（回退默认光标）
  - [x] SubTask 2.3: 为每个浮动图标生成随机位置 / 尺寸 / 漂移时长 / 方向的样式对象
  - [x] SubTask 2.4: 返回 `{ cursorUrl, floatItems, channelSprites }`，`channelSprites` 为 `Record<GameChannel, ChannelSprite>` 且包含远程 URL（失败时由消费方回退到本地精灵）

- [x] Task 3: 在 `App.tsx` 接入随机化结果
  - [x] SubTask 3.1: 调用 `useWikiIcons()`，用返回的 `channelSprites` 替换模块级静态 `channelSprites` 常量
  - [x] SubTask 3.2: 在 `ChannelBadge` / `ChannelStrip` 中给 `<img>` 增加 `onError` 回退到本地精灵（core-shard / zenith / lancer / alpha）
  - [x] SubTask 3.3: 在 `.app-shell` 内挂载浮动图标层 `<div className="wiki-float-layer">`，渲染 `floatItems`

- [x] Task 4: 在 `src/styles.css` 增加光标与浮动层样式
  - [x] SubTask 4.1: 在 `:root` 增加 `--app-cursor-url` 初始空值，`.app-shell` 设置 `cursor: var(--app-cursor-url), auto;`
  - [x] SubTask 4.2: 新增 `.wiki-float-layer`：绝对定位、`inset:0`、`pointer-events:none`、`z-index:-1`（与 `.app-backdrop` 同层但位于其上）
  - [x] SubTask 4.3: 新增 `.wiki-float-item`：绝对定位、`image-rendering: pixelated`、低透明度、`@keyframes wiki-float-drift` 漂移动画

- [x] Task 5: 调整 `src-tauri/tauri.conf.json` 的 CSP
  - [x] SubTask 5.1: 经核查 `app.security.csp` 为 `null`（无限制），远程图片可直接加载，无需改动
  - [x] SubTask 5.2: 不适用（无 connect-src 限制）

- [x] Task 6: 验证与构建检查
  - [x] SubTask 6.1: 运行 `pnpm check` 通过类型检查
  - [x] SubTask 6.2: 运行 `pnpm build:web` 确认构建无报错
  - [ ] SubTask 6.3: 人工核查 `pnpm dev:web` 下：光标变为随机单位图标、背景有低透明度浮动图标、四个通道徽章显示随机方块图标（需用户在运行环境人工确认）

# Task Dependencies
- Task 2 依赖 Task 1（需要清单与类型）
- Task 3 依赖 Task 2（需要 Hook 返回值）
- Task 4 与 Task 3 可并行（样式与组件互不阻塞，但需协调类名）
- Task 5 与 Task 1–4 并行
- Task 6 依赖 Task 1–5 全部完成
