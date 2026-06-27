# Repository Guidelines

## Project

Unpackaged WinUI 3 desktop app (C#, .NET 10, Windows App SDK 2.2). Single project at root, no test project, no MSIX packaging. `legacy-tauri/` is archived — do not wire new behavior into it.

## Build & Run

Always load the local PowerShell environment first (pins npm/pnpm/cargo caches to the repo):

```powershell
. .\scripts\dev-env.ps1
dotnet restore .\MindustryLauncher.csproj --source https://api.nuget.org/v3/index.json
dotnet build .\MindustryLauncher.csproj -c Debug -p:Platform=x64
.\bin\x64\Debug\net10.0-windows10.0.19041.0\win-x64\MindustryLauncher.exe
```

Platform must be explicit (`x64` / `x86` / `ARM64`). `dotnet run` is not configured — launch the built exe directly. If `exe` is locked by a running instance, stop only the `MindustryLauncher` process before rebuilding.

**No linter, formatter, or typecheck step exists — build is the only verification.**

## Release

GitHub CI (`release.yml`) triggers on tags `v*` or via `workflow_dispatch`:

```powershell
# manual publish equivalent
dotnet publish .\MindustryLauncher.csproj -c Release -p:Platform=x64 -p:RuntimeIdentifier=win-x64 -o publish/MindustryLauncher
```

Version is read from `MindustryLauncher.csproj` `<Version>` element.

## Architecture

- **App.xaml.cs** — `App.Launcher` singleton (`LauncherService`). Startup creates `MainWindow` → navigates to `MainPage`.
- **MainWindow.xaml.cs** — Mica titlebar (`ExtendsContentIntoTitleBar`), window sizing (720–1120 wide, 560–760 tall), frame navigation.
- **MainPage.xaml.cs** — Single-page `NavigationView` (games/versions/settings/debug). All UI logic lives here. Responsive breakpoints defined in code-behind (720/760/820/1160 px).
- **Models/LauncherModels.cs** — All models in one file. JSON serialization uses `JsonSettings.Options` (camelCase, custom enum converters). `FileSystemUtil.WriteJsonAsync` writes atomically via `.tmp` + `File.Move`.
- **Services/** — `LauncherService` split into partials (`Runtime.cs`, `Update.cs`, `Versions.cs`, `Instances.cs`, `GameTracking.cs`). `NetworkClient` handles download with pause/cancel/resume + checksum validation. `AppDebugLog` writes to `<install-root>/logs/debug.log` with 2 MB rotation (4 archives max).
- **Portable data** — `MindustryLauncherData/` next to the exe stores an `install-root.json` pointer (default: `MindustryLauncherData/data/`). All state (settings, instances, runtimes, caches) lives under the install root.
- **GitHub network layer** — URLs are rewritten through configurable accelerators (HubProxy, GHProxy, or direct) with automatic fallback. Accelerator list fetched from `github.com/kabaka9527/MindustryLauncher/main/resources/github-accelerators.json`.

## Style

- Public types/methods: `PascalCase`. Private fields: `_camelCase`. Nullable enabled project-wide.
- XAML resources in `Styles/ThemeResources.xaml` and `Styles/ControlStyles.xaml`. Prefer theme-aware system brushes.
- UI strings are in Chinese (zh-CN). `Converters/BoolToVisibilityConverter.cs` registered globally in `App.xaml`.
