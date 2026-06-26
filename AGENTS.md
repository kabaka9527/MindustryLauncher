# Repository Guidelines

## Project Structure & Module Organization

MindustryLauncher is now an unpackaged, self-contained WinUI 3 desktop app built with C# and the Windows App SDK. The active project lives at the repository root. `App.xaml` owns app resources and startup, `MainWindow.xaml` hosts the Mica-backed shell and title bar, and `MainPage.xaml` contains the current NavigationView experience. Shared models live in `Models/`, application services in `Services/`, theme and control dictionaries in `Styles/`, and bundled app imagery/icons in `Assets/`.

The previous Tauri + React + Rust implementation has been moved to `legacy-tauri/` for reference only. Do not wire new behavior into that legacy tree unless explicitly asked.

## Build, Test, and Development Commands

Always load the local PowerShell environment first:

```powershell
. .\scripts\dev-env.ps1
```

- `dotnet restore .\MindustryLauncher.csproj --source https://api.nuget.org/v3/index.json`: restore WinUI and Windows App SDK packages.
- `dotnet build .\MindustryLauncher.csproj -c Debug -p:Platform=x64`: build the local debug app.
- `dotnet build .\MindustryLauncher.csproj -c Release -p:Platform=x64`: build the release executable.
- `.\bin\x64\Debug\net10.0-windows10.0.19041.0\win-x64\MindustryLauncher.exe`: run the unpackaged debug build after a successful build.

## Coding Style & Naming Conventions

Use nullable C# with standard .NET naming: public types and XAML controls use `PascalCase`, private fields use `_camelCase`, and methods use `PascalCase`. XAML resources should be centralized in `Styles/ThemeResources.xaml` and `Styles/ControlStyles.xaml`; prefer theme resources, WinUI system brushes, and built-in controls before introducing custom styling.

## WinUI Design Guidelines

Keep the app aligned with Windows native behavior: use `NavigationView`, `CommandBar`, `InfoBar`, `ListView`, `ContentDialog`, `ToggleSwitch`, `ComboBox`, `NumberBox`, and other stock controls where they fit. Maintain light, dark, and high-contrast compatibility through theme-aware resources. Responsive layout should be handled through explicit width breakpoints in code-behind or reusable resources, not hard-coded one-size layouts.

## Testing Guidelines

After changing WinUI UI, resources, startup, packaging, or services, run:

```powershell
. .\scripts\dev-env.ps1
dotnet build .\MindustryLauncher.csproj -c Debug -p:Platform=x64
```

For UI changes, launch the built executable and verify a real top-level window appears, responds, and shows the expected shell/page. If the executable is locked by a running app instance, stop only the `MindustryLauncher` process before rebuilding.

## Security & Configuration Tips

Do not commit build output, local caches, generated binaries, private tokens, proxy credentials, or portable user data. Preserve checksum validation, safe path checks, download pause/cancel behavior, and conservative file-system handling in launcher services.
