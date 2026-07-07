# Mindustry Wiki 图标定制（光标 / 浮动背景 / 版本图标）Spec

## Why
当前启动器视觉较为朴素，仅以本地精灵图与栅格底纹装饰。Mindustry 官方 Wiki（`https://mindustrygame.github.io/wiki/`）托管了全部单位 / 方块 / 物品 / 液体的像素图标，可复用为启动器的鼠标光标、背景浮动装饰与版本通道图标，让界面每次启动都呈现不同观感，增强趣味性与品牌一致性。

## What Changes
- 新增 `src/assets/wiki-icons.json` 图标清单：按 `units` / `blocks` / `liquids` / `statuses` 分组，记录从 Wiki 索引抽取的全部内部名（internal name），并给出图标远程 URL 模板常量。
- 新增 `src/hooks/useWikiIcons.ts` 启动期随机化 Hook：在应用首次挂载时基于清单一次性随机选取——
  - 1 个单位图标作为本次启动的全局鼠标光标；
  - N（默认 18）个随机图标（混合各类）作为背景浮动元素；
  - 4 个不同方块图标分别映射到四个版本通道（mindustry / mindustryX / mindustryBE / mindustryXBE）。
- 在 `App.tsx` 中接入该 Hook，将随机出的 4 个方块图标替换现有 `channelSprites` 静态映射（保留本地精灵作为远程图标加载失败时的回退）。
- 新增背景浮动图标层 `.wiki-float-layer`：绝对定位、置于 `.app-backdrop` 之上、主内容之下，图标以随机位置 / 尺寸 / 速度缓慢漂移，低透明度保证不干扰阅读。
- 通过 CSS 自定义属性 `--app-cursor-url` 在 `:root` / `.app-shell` 上设置 `cursor: var(--app-cursor-url), auto;`，实现全局光标替换；光标热点统一取图标中心。
- 调整 `tauri.conf.json` 的 CSP（如需）允许加载 `https://mindustrygame.github.io` 下的图片资源。
- **BREAKING**：`channelSprites` 由静态本地资源映射改为每次启动随机化的远程 Wiki 方块图标映射（带本地回退）。

## Impact
- 受影响代码：
  - `src/App.tsx`：`channelSprites` 定义、`ChannelBadge` / `ChannelStrip` 用法、根节点光标与浮动层挂载。
  - `src/styles.css`：新增 `.wiki-float-layer` / `.wiki-float-item` 样式与 `--app-cursor-url` 变量。
  - `src/hooks/useWikiIcons.ts`：新增。
  - `src/assets/wiki-icons.json`：新增。
  - `src-tauri/tauri.conf.json`：CSP `img-src` 增补。
- 受影响能力：版本列表通道徽章、游戏页通道徽章、整体背景氛围、全局鼠标交互。

## ADDED Requirements

### Requirement: Wiki 图标清单
系统 SHALL 内置一份从 Mindustry Wiki 索引抽取的图标清单（`wiki-icons.json`），包含全部单位、方块、液体、状态图标 的内部名，并提供远程 URL 构造规则。

#### Scenario: 构造图标 URL
- **WHEN** 任意模块需要取得某图标的远程地址
- **THEN** 系统按 `https://mindustrygame.github.io/wiki/images/<type>-<name>-ui.png` 拼接，其中 `<type>` ∈ `unit` / `block` / `liquid` / `status`，`<name>` 取自清单对应分组

#### Scenario: 清单覆盖范围
- **WHEN** 读取清单
- **THEN** 至少包含 Wiki 索引中列出的全部单位（约 57 个）与方块（约 98 个）内部名

### Requirement: 启动期随机鼠标光标
系统 SHALL 在每次启动器启动时，从单位图标清单中随机选取一个图标，作为整个应用的全局鼠标光标。

#### Scenario: 光标随机化
- **WHEN** 启动器完成首次挂载
- **THEN** 从 `units` 分组随机选取一个图标 URL，经预加载成功后写入 `--app-cursor-url`
- **AND** `:root` / `.app-shell` 应用 `cursor: var(--app-cursor-url), auto;`

#### Scenario: 同次会话内保持稳定
- **WHEN** 用户在本次启动的会话内切换视图
- **THEN** 光标样式保持为本次启动随机出的那一个，不随视图切换变化

#### Scenario: 光标加载失败回退
- **WHEN** 远程图标预加载失败（网络异常或 404）
- **THEN** 不写入 `--app-cursor-url`，回退为系统默认光标，且不阻塞其余启动流程

### Requirement: 背景浮动图标层
系统 SHALL 在主背景层之上、主内容之下渲染一层随机选取的 Wiki 图标，图标以随机位置、尺寸与速度缓慢漂移，营造氛围。

#### Scenario: 浮动图标选取
- **WHEN** 启动器完成首次挂载
- **THEN** 从全部清单（混合单位 / 方块 / 液体 / 状态）中随机选取 18 个图标
- **AND** 每个图标被赋予随机初始位置、尺寸（24–56px）、漂移时长（30–80s）与方向

#### Scenario: 浮动图标视觉
- **WHEN** 浮动层渲染
- **THEN** 图标以低透明度（约 0.10–0.18）显示，启用 `image-rendering: pixelated`，且不接收指针事件（`pointer-events: none`）
- **AND** 浮动层位于 `.app-backdrop` 之上、`.workspace` / `.sidebar` 之下

#### Scenario: 不干扰交互
- **WHEN** 用户在任意区域点击 / 悬停
- **THEN** 浮动层不拦截任何指针事件，所有交互正常落入下方控件

### Requirement: 版本通道图标随机化
系统 SHALL 在每次启动器启动时，从方块图标清单中随机选取 4 个不同图标，分别作为四个版本通道（mindustry / mindustryX / mindustryBE / mindustryXBE）的展示图标。

#### Scenario: 通道图标随机化
- **WHEN** 启动器完成首次挂载
- **THEN** 从 `blocks` 分组无重复地随机选取 4 个图标
- **AND** 按通道顺序映射，替换原有本地精灵映射

#### Scenario: 通道图标加载失败回退
- **WHEN** 某通道随机出的远程方块图标加载失败
- **THEN** 该通道回退为原有本地精灵图标（core-shard / zenith / lancer / alpha），保证徽章不出现空白

#### Scenario: 同次会话内通道图标稳定
- **WHEN** 用户在本次启动会话内切换视图或刷新版本列表
- **THEN** 四个通道的图标保持为本次启动随机出的那一组，不随视图切换变化

## MODIFIED Requirements

### Requirement: 通道徽章图标来源
原 `channelSprites` 为静态导入的本地 PNG 资源映射。修改后：`channelSprites` 在每次启动时由 `useWikiIcons` 返回的随机方块图标 URL 映射构造，`ChannelBadge` / `ChannelStrip` 直接消费该映射；远程加载失败时回退到原本地资源。

## REMOVED Requirements
无。
