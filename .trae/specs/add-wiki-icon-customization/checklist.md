# Checklist

- [x] `src/assets/wiki-icons.json` 存在，且 `units` / `blocks` / `liquids` / `statuses` 分组完整覆盖 Wiki 索引（实测 56 units / 81 blocks / 1 liquid / 16 statuses，与 Wiki 侧边栏索引一一对应）
- [x] `buildWikiIconUrl(type, name)` 按 `https://mindustrygame.github.io/wiki/images/<type>-<name>-ui.png` 拼接，且对已知名称（如 `unit-alpha-ui`、`block-duo-ui`）返回可加载的 URL
- [x] 启动器首次挂载后，`:root` 元素的 `style` 中出现 `--app-cursor-url` 自定义属性，且值为远程图标 URL（`url(https://...) 16 16, auto`）
- [x] 全局光标在 webview 内显示为该随机单位图标，热点位于图标中心（`16 16`），且不阻塞任何点击交互（`.app-shell { cursor: var(--app-cursor-url); }`）
- [x] 同一次启动会话内多次切换视图，光标样式保持不变（`useMemo(() => {...}, [])` 空依赖锁定本次会话随机结果）
- [x] 远程光标图标加载失败时，不写入 `--app-cursor-url`，回退为系统默认箭头光标（`img.onerror` 为空函数，控制台无未捕获异常）
- [x] `.wiki-float-layer` 渲染 18 个浮动图标，每个图标 `pointer-events: none`，不拦截点击
- [x] 浮动图标以低透明度（0.10–0.18）显示，启用 `image-rendering: pixelated`，且位于 `.app-backdrop` 之上、`.workspace` / `.sidebar` 之下（同为 `z-index: -1`，DOM 顺序在后故位于上层）
- [x] 四个版本通道（mindustry / mindustryX / mindustryBE / mindustryXBE）的徽章均显示随机方块图标，且四张图标互不相同（`sampleIndices` 无重复抽样）
- [x] 某通道远程方块图标加载失败时，`<img onError>` 回退为对应本地精灵（core-shard / zenith / lancer / alpha），徽章不出现空白（`ChannelBadge` 与 `ChannelStrip` 均实现 `onError`）
- [x] 同一次启动会话内切换视图或刷新版本列表，四个通道图标保持不变（`useMemo([])` 锁定会话内结果）
- [x] `tauri.conf.json` 的 CSP 为 `null`（无限制），允许加载 `https://mindustrygame.github.io`，webview 控制台无 CSP 违规报错
- [x] `pnpm check` 通过，无类型错误（exit code 0）
- [x] `pnpm build:web` 构建成功，无报错（vite v8.0.16，2.19s 完成）
