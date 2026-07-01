# MindustryLauncher

一个面向 [Mindustry](https://github.com/Anuken/Mindustry) 和 [MindustryX](https://github.com/TinyLake/MindustryX) 的便携式桌面启动器。基于 Tauri 2、Rust、React 19、TypeScript 和 Vite 构建，提供免安装、易迁移的游戏版本管理体验。

---

## 目录

- [项目概述](#项目概述)
- [核心功能](#核心功能)
- [技术栈](#技术栈)
- [快速开始](#快速开始)
- [使用指南](#使用指南)
- [配置说明](#配置说明)
- [开发说明](#开发说明)
- [项目结构](#项目结构)
- [贡献指南](#贡献指南)
- [许可证](#许可证)

---

## 项目概述

MindustryLauncher 旨在为 Mindustry 系列游戏提供统一的版本管理和启动体验。启动器将游戏实例、运行时、缓存和配置集中在便携数据目录中，支持整体移动或备份，非常适合在不同设备间迁移或进行版本回退测试。

**设计理念：**

- **便携优先**：所有数据集中管理，无需安装，即插即用
- **隔离运行**：不同游戏实例拥有独立的游戏目录、数据目录和日志
- **网络优化**：内置 GitHub 加速源，支持网络代理配置
- **调试友好**：提供完整的运行日志和调试控制台
- **定制界面**：采用无边框窗口设计，配合自定义标题栏实现流畅的视觉体验

---

## 核心功能

### 游戏版本管理

- 支持 **Mindustry**、**MindustryX**、**Mindustry BE**、**MindustryX BE** 四个游戏渠道
- 获取远端版本列表，支持预发布版本显示
- 下载并安装指定版本，支持断点续传
- 为每个版本创建独立的游戏实例，实现版本隔离

### Java 运行时管理

- 自动检测系统已安装的 Java 运行时
- 下载并安装推荐版本的 Java 运行时
- 支持导入外部 Java 运行时
- 扫描指定目录查找 Java 运行时
- 运行时启用/禁用管理

### 启动配置

- 为每个游戏实例配置独立的启动参数
- 支持设置最小/最大内存分配（Xms/Xmx）
- 自定义 JVM 参数
- 自定义游戏启动参数

### 网络与下载

- **GitHub 加速源**：内置加速源列表，首屏使用包内列表，启动后异步刷新远端列表
- **网络代理**：支持 HTTP 代理配置
- **下载控制**：支持暂停、恢复、取消下载任务
- **进度展示**：实时显示下载进度和速度

### 调试与日志

- 调试模式：启用后显示调试控制台窗口
- 运行日志：自动记录到 `logs/debug.log`
- 日志快照：读取最近 600 行日志
- 日志清理：一键清空日志

### 数据迁移

- 支持将便携数据目录迁移到新位置
- 自动复制现有数据并更新配置

---

## 技术栈

| 分类 | 技术 | 版本 |
|------|------|------|
| 桌面框架 | Tauri | 2.x |
| 后端语言 | Rust | 2021 Edition |
| 前端框架 | React | 19.x |
| 前端语言 | TypeScript | 6.x |
| 构建工具 | Vite | 8.x |
| 包管理器 | pnpm | 9.x |

---

## 快速开始

### 下载与运行

从 [GitHub Releases](https://github.com/kabaka9527/MindustryLauncher/releases) 下载最新版本的 `mindustry-launcher.exe`，双击运行即可。

启动器首次运行时会在同目录下创建 `MindustryLauncherData` 文件夹作为便携数据目录。

### 基本操作

1. **选择游戏渠道**：在左侧导航栏切换不同的游戏渠道
2. **安装版本**：选择版本列表中的版本，点击安装按钮
3. **配置运行时**：在运行时管理页面配置 Java 运行时
4. **启动游戏**：点击已安装实例的启动按钮

---

## 使用指南

### 游戏版本

- **安装版本**：从版本列表中选择需要的版本，点击安装。下载完成后自动解压并创建实例。
- **切换版本**：在已安装实例间快速切换。
- **删除实例**：右键删除不需要的游戏实例。

### 运行时管理

- **自动检测**：启动器会自动扫描系统中已安装的 Java 运行时。
- **下载运行时**：从远端列表中选择并下载推荐的 Java 版本。
- **导入运行时**：将已有的 Java 运行时导入到启动器管理。
- **扫描目录**：扫描指定目录查找 Java 运行时。

### 启动设置

- **内存配置**：为游戏设置合理的内存分配，建议根据游戏版本和电脑配置调整。
- **JVM 参数**：高级用户可添加自定义 JVM 参数。
- **游戏参数**：添加游戏启动参数，如服务器地址等。

---

## 配置说明

启动器配置文件位于便携数据目录下的 `settings.json`，支持以下配置项：

| 配置项 | 类型 | 默认值 | 说明 |
|--------|------|--------|------|
| `installRoot` | string | `MindustryLauncherData` | 便携数据目录路径 |
| `showBe` | bool | `false` | 是否显示 BE（服务端）版本 |
| `githubProxyPrefix` | string | `null` | GitHub 代理前缀 |
| `httpProxy` | string | `null` | HTTP 代理地址 |
| `selectedAcceleratorId` | string | `hubproxy-kabaka` | 选中的加速源 ID |
| `channelVisibility` | object | `{ mindustry: true, mindustryX: false, mindustryBE: false, mindustryXBE: false }` | 各游戏渠道的可见性配置 |
| `runtimePromptDismissed` | bool | `false` | 是否已关闭运行时提示 |
| `ignoredVersions` | string[] | `[]` | 已忽略的启动器更新版本列表 |
| `debugMode` | bool | `false` | 是否启用调试模式 |

### 便携数据目录结构

```
MindustryLauncherData/
├── cache/                    # 缓存目录
├── instances/                # 游戏实例目录
│   └── mindustry-v158/       # 具体版本实例
│       ├── game/             # 游戏文件
│       ├── data/             # 游戏数据（存档等）
│       └── logs/             # 游戏日志
├── runtimes/                 # Java 运行时目录
├── logs/                     # 启动器日志
│   └── debug.log             # 调试日志
└── settings.json             # 配置文件
```

---

## 开发说明

### 环境要求

- **Node.js**：22.x
- **pnpm**：9.x
- **Rust**：Stable（通过 rust-toolchain.toml 管理）
- **Tauri 系统依赖**：参考 [Tauri 安装指南](https://v2.tauri.app/guides/getting-started/prerequisites/)

### 开发环境配置

所有开发命令需先加载开发环境脚本，以确保缓存目录正确设置：

```powershell
. .\scripts\dev-env.ps1
```

### 常用命令

| 任务 | 命令 |
|------|------|
| 安装依赖 | `pnpm install` |
| 启动开发环境 | `pnpm tauri dev` |
| 仅启动前端开发 | `pnpm dev:web` |
| 前端类型检查 | `pnpm check` |
| 运行 Rust 测试 | `cargo test --manifest-path .\src-tauri\Cargo.toml` |
| 构建便携版 | `pnpm build` |
| 仅构建前端 | `pnpm build:web` |

### 构建产物

构建完成后，可执行文件位于：

```text
src-tauri/target/release/mindustry-launcher.exe
```

### 代码规范

- **TypeScript**：严格模式（`"strict": true`），无分号
- **Rust**：2021 Edition，使用 `thiserror` 和 `serde`（camelCase 序列化）
- **UI 字符串**：中文
- **环境变量**：仅 `VITE_*` 和 `TAURI_*` 前缀的变量会被暴露

---

## 项目结构

```
MindustryLauncher/
├── .github/workflows/        # GitHub Actions 工作流
│   └── release.yml           # 构建与发布工作流
├── resources/                # 资源文件
│   └── github-accelerators.json  # 内置 GitHub 加速源列表
├── scripts/                  # 脚本文件
│   ├── dev-env.ps1           # 开发环境配置脚本
│   └── inspect-cache-paths.ps1   # 缓存路径检查脚本
├── src/                      # 前端源代码
│   ├── assets/               # 静态资源
│   ├── hooks/                # React Hooks
│   ├── App.tsx               # 主应用组件
│   ├── TitleBar.tsx          # 自定义标题栏
│   ├── api.ts                # Tauri API 调用封装
│   ├── types.ts              # 前端类型定义（与 models.rs 同步）
│   └── main.tsx              # 入口文件
├── src-tauri/                # Tauri/Rust 后端
│   ├── capabilities/         # 能力配置
│   ├── icons/                # 应用图标
│   ├── src/                  # Rust 源代码
│   │   ├── lib.rs            # 主库文件，定义 IPC 命令
│   │   ├── main.rs           # 应用入口
│   │   ├── models.rs         # 数据模型（与 types.ts 同步）
│   │   ├── config.rs         # 配置管理
│   │   ├── instances.rs      # 游戏实例管理
│   │   ├── runtime.rs        # Java 运行时管理
│   │   ├── versions.rs       # 版本列表管理
│   │   ├── network.rs        # 网络请求与下载管理
│   │   ├── launcher.rs       # 游戏启动与进程管理
│   │   ├── accelerators.rs   # GitHub 加速源管理
│   │   ├── debug_console.rs  # 调试控制台
│   │   ├── error.rs          # 错误处理
│   │   ├── fs_util.rs        # 文件系统工具
│   │   └── update.rs         # 启动器更新检查
│   ├── Cargo.toml            # Rust 依赖配置
│   ├── tauri.conf.json       # Tauri 配置
│   └── build.rs              # 构建脚本
├── index.html                # HTML 入口
├── package.json              # 前端依赖配置
├── tsconfig.json             # TypeScript 配置
├── vite.config.ts            # Vite 配置
├── rust-toolchain.toml       # Rust 工具链配置
└── LICENSE                   # 许可证文件
```

---

## 贡献指南

欢迎提交 Issue 和 Pull Request！

### 开发流程

1. Fork 仓库并克隆到本地
2. 创建功能分支：`git checkout -b feature/your-feature`
3. 进行开发并提交代码
4. 确保通过前端类型检查和 Rust 测试
5. 提交 Pull Request

### 代码提交规范

提交信息使用以下格式：

```text
<type>: <description>
```

类型说明：
- `feat`：新功能
- `fix`：Bug 修复
- `refactor`：代码重构
- `docs`：文档更新
- `test`：测试更新
- `chore`：构建/工具相关

### 版本发布

推送到 `main` 或 `master` 分支时，提交信息首行使用以下格式会触发 GitHub Actions 构建并发布版本：

```text
release: v0.1.0
```

版本号必须与 `package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json` 中的版本一致。

---

## 许可证

本项目采用 [GPL-3.0](LICENSE) 许可证。

---

## 相关项目

- [Mindustry](https://github.com/Anuken/Mindustry) - 开源沙盒塔防游戏
- [MindustryX](https://github.com/TinyLake/MindustryX) - Mindustry 社区修改版本
