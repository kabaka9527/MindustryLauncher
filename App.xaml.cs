using Microsoft.UI.Xaml;
using MindustryLauncher.Services;

namespace MindustryLauncher;

public partial class App : Application
{
    public static LauncherService Launcher { get; } = new();

    private Window? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            AppDebugLog.Error($"未处理异常：{args.Exception}");
            args.Handled = true;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Activate();
    }
}
