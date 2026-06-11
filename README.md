# MindustryLauncher

MindustryLauncher 是一个面向 [Mindustry](https://github.com/Anuken/Mindustry) 和 [MindustryX](https://github.com/TinyLake/MindustryX) 的便携式桌面启动器。它基于 Tauri 2、Rust、React、TypeScript 和 Vite 构建，目标是提供一个免安装、易迁移、适合日常游玩和版本管理的启动器。

## 项目介绍

MindustryLauncher 主要负责游戏版本、运行时和启动配置的统一管理。启动器会将游戏实例、运行时、缓存和配置尽量放在安装根目录内，便于整体移动或备份。

主要能力包括：

- 管理 Mindustry 与 MindustryX 的本地游戏实例。
- 获取远端版本列表，下载并安装指定版本。
- 为不同实例隔离游戏目录、数据目录、日志和启动参数。
- 管理 Java/JRE 运行时，包括下载、导入、检索和启用。
- 支持网络代理、GitHub 加速源、下载进度展示和任务控制；GitHub 加速源首屏使用包内列表，启动后异步刷新远端列表，失败时继续使用包内列表。
- 提供运行日志与调试相关能力，便于排查下载、启动和运行时问题。

默认便携数据目录为同目录下的 `MindustryLauncherData`。构建默认便携 exe 输出路径为 `src-tauri\target\release\mindustry-launcher.exe`。

## 技术栈

- 桌面框架：Tauri 2
- 后端：Rust
- 前端：React、TypeScript、Vite
- 包管理器：pnpm

## 开发说明

开发前请先安装 Node.js、pnpm、Rust 和 Tauri 所需的系统依赖。所有开发缓存应优先留在仓库目录内，执行依赖安装、检查、测试或构建前先加载开发环境脚本：

```powershell
. .\scripts\dev-env.ps1
```

安装前端依赖：

```powershell
. .\scripts\dev-env.ps1
pnpm install
```

启动开发环境：

```powershell
. .\scripts\dev-env.ps1
pnpm tauri dev
```

运行前端检查：

```powershell
. .\scripts\dev-env.ps1
pnpm check
```

运行 Rust 测试：

```powershell
. .\scripts\dev-env.ps1
cargo test --manifest-path .\src-tauri\Cargo.toml
```

构建便携版：

```powershell
. .\scripts\dev-env.ps1
pnpm build
```

构建完成后，默认可执行文件位于：

```text
src-tauri\target\release\mindustry-launcher.exe
```

## 发布版本

推送到 `main` 或 `master` 分支时，提交信息首行使用以下格式会触发 GitHub Actions 构建并发布版本：

```text
release: v0.1.0
```

版本号必须与 `package.json` 和 `src-tauri\Cargo.toml` 中的版本一致。工作流会依次运行前端类型检查、Rust 测试、便携版构建，并将 `src-tauri\target\release\mindustry-launcher.exe` 作为 GitHub Release 附件发布。

## 相关项目

- [Mindustry](https://github.com/Anuken/Mindustry)
- [MindustryX](https://github.com/TinyLake/MindustryX)
