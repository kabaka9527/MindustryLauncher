# MindustryLauncher

MindustryLauncher is a modern WinUI 3 desktop launcher for Mindustry. The app uses C#, Windows App SDK, native WinUI controls, Mica, centralized theme resources, and adaptive layouts for Windows-style light/dark experiences.

The active project is now located directly at this repository root. The previous Tauri implementation is preserved under `legacy-tauri/` for reference.

## Requirements

- Windows 10 19041 or later
- .NET SDK with WinUI templates/support
- Windows App SDK packages restored from NuGet

## Build

Load the local development environment first:

```powershell
. .\scripts\dev-env.ps1
```

Restore and build:

```powershell
dotnet restore .\MindustryLauncher.csproj --source https://api.nuget.org/v3/index.json
dotnet build .\MindustryLauncher.csproj -c Debug -p:Platform=x64
```

Run the debug executable:

```powershell
.\bin\x64\Debug\net10.0-windows10.0.19041.0\win-x64\MindustryLauncher.exe
```

## Project Layout

- `App.xaml` / `App.xaml.cs`: app startup and shared resources
- `MainWindow.xaml` / `MainWindow.xaml.cs`: native window, title bar, and Mica shell
- `MainPage.xaml` / `MainPage.xaml.cs`: main NavigationView UI and responsive page behavior
- `Models/`: UI and launcher data models
- `Services/`: settings, network, runtime, version, update, and launch services
- `Styles/`: Fluent theme tokens and shared control styles
- `Assets/`: app icons and Mindustry imagery
- `legacy-tauri/`: archived pre-migration implementation
