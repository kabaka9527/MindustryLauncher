using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Text.Json;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using MindustryLauncher.Models;
using MindustryLauncher.Services;
using Windows.Storage.Pickers;
using Windows.System;
using WinRT.Interop;

namespace MindustryLauncher;

public sealed partial class MainPage : Page, INotifyPropertyChanged
{
    private readonly LauncherService _launcher = App.Launcher;
    private bool _hydrating;
    private string _currentView = "games";
    private Settings _draft = new();

    public ObservableCollection<DashboardMetric> DashboardMetrics { get; } = [];
    public ObservableCollection<InstalledInstance> InstalledInstances { get; } = [];
    public ObservableCollection<RemoteVersion> VisibleVersions { get; } = [];
    public ObservableCollection<RuntimeInfo> Runtimes { get; } = [];
    public ObservableCollection<RemoteRuntime> RemoteRuntimes { get; } = [];
    public ObservableCollection<TaskRecord> ActiveTasks { get; } = [];

    public event PropertyChangedEventHandler? PropertyChanged;

    public MainPage()
    {
        InitializeComponent();
        DataContext = this;
        KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden;
        _launcher.TaskChanged += OnTaskChanged;
        RegisterKeyboardAccelerators();
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        await RunAsync("正在读取本地状态", async () =>
        {
            var state = await _launcher.InitializeAsync();
            ApplyState(state);
            SetNotice("状态已同步");
        });
    }

    private void OnPageSizeChanged(object sender, SizeChangedEventArgs e)
    {
        ApplyResponsiveLayout(e.NewSize.Width);
    }

    private void OnNavigationSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItem is not NavigationViewItem item || item.Tag is not string tag)
        {
            return;
        }

        ShowView(tag);
    }

    private void ShowView(string view)
    {
        _currentView = view;
        GamesView.Visibility = view == "games" ? Visibility.Visible : Visibility.Collapsed;
        VersionsView.Visibility = view == "versions" ? Visibility.Visible : Visibility.Collapsed;
        SettingsView.Visibility = view == "settings" ? Visibility.Visible : Visibility.Collapsed;
        DebugView.Visibility = view == "debug" ? Visibility.Visible : Visibility.Collapsed;

        PageTitleText.Text = view switch
        {
            "versions" => "版本",
            "settings" => "设置",
            "debug" => "调试",
            _ => "游戏"
        };
        PageSubtitleText.Text = view switch
        {
            "versions" => "选择、安装或切换游戏版本",
            "settings" => "管理安装目录、网络和 Java 运行时",
            "debug" => "查看本地诊断日志",
            _ => "管理已安装实例并启动游戏"
        };

        if (view == "debug")
        {
            RefreshDebugLog();
        }

        ApplyResponsiveLayout(ActualWidth);
    }

    private void RegisterKeyboardAccelerators()
    {
        AddKeyboardAccelerator(VirtualKey.F5, RefreshCurrentViewAsync);
        AddKeyboardAccelerator(VirtualKey.R, VirtualKeyModifiers.Control, RefreshCurrentViewAsync);
        AddKeyboardAccelerator(VirtualKey.O, VirtualKeyModifiers.Control, () =>
        {
            _launcher.OpenInstallRoot();
            return Task.CompletedTask;
        });
        AddKeyboardAccelerator(VirtualKey.S, VirtualKeyModifiers.Control, async () =>
        {
            if (_currentView == "settings")
            {
                await SaveSettingsAsync();
            }
        });

        AddNavigationAccelerator(VirtualKey.Number1, "games");
        AddNavigationAccelerator(VirtualKey.Number2, "versions");
        AddNavigationAccelerator(VirtualKey.Number3, "settings");
        AddNavigationAccelerator(VirtualKey.Number4, "debug");
    }

    private void AddKeyboardAccelerator(VirtualKey key, Func<Task> action)
    {
        var accelerator = new KeyboardAccelerator { Key = key };
        accelerator.Invoked += async (_, args) =>
        {
            args.Handled = true;
            await action();
        };
        KeyboardAccelerators.Add(accelerator);
    }

    private void AddKeyboardAccelerator(VirtualKey key, VirtualKeyModifiers modifiers, Func<Task> action)
    {
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += async (_, args) =>
        {
            args.Handled = true;
            await action();
        };
        KeyboardAccelerators.Add(accelerator);
    }

    private void AddNavigationAccelerator(VirtualKey key, string view)
    {
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = VirtualKeyModifiers.Control };
        accelerator.Invoked += (_, args) =>
        {
            args.Handled = true;
            SelectView(view);
        };
        KeyboardAccelerators.Add(accelerator);
    }

    private void SelectView(string view)
    {
        var navigationItem = RootNavigation.MenuItems
            .OfType<NavigationViewItem>()
            .FirstOrDefault(item => item.Tag as string == view);

        if (navigationItem is null)
        {
            ShowView(view);
            return;
        }

        if (ReferenceEquals(RootNavigation.SelectedItem as NavigationViewItem, navigationItem))
        {
            ShowView(view);
            return;
        }

        RootNavigation.SelectedItem = navigationItem;
    }

    private async Task RefreshCurrentViewAsync()
    {
        switch (_currentView)
        {
            case "versions":
                await RefreshVersionsAsync();
                break;
            case "settings":
                await RefreshAcceleratorsAsync();
                break;
            case "debug":
                RefreshDebugLog();
                SetNotice("日志已刷新");
                break;
            default:
                await RefreshAllAsync();
                break;
        }
    }

    private async Task RefreshAllAsync()
    {
        await RunAsync("正在刷新", async () =>
        {
            await _launcher.RefreshAcceleratorsAsync();
            await _launcher.RefreshVersionsAsync();
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("已刷新");
        });
    }

    private async Task RefreshVersionsAsync()
    {
        await RunAsync("正在刷新版本", async () =>
        {
            await _launcher.RefreshVersionsAsync();
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("版本列表已刷新");
        });
    }

    private async Task RefreshAcceleratorsAsync()
    {
        await RunAsync("正在刷新加速源", async () =>
        {
            await _launcher.RefreshAcceleratorsAsync();
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("GitHub 加速源已刷新");
        });
    }

    private async void OnRefreshAllClick(object sender, RoutedEventArgs e)
    {
        await RefreshAllAsync();
    }

    private async void OnRefreshVersionsClick(object sender, RoutedEventArgs e)
    {
        await RefreshVersionsAsync();
    }

    private async void OnRefreshAcceleratorsClick(object sender, RoutedEventArgs e)
    {
        await RefreshAcceleratorsAsync();
    }

    private void OnOpenInstallRootClick(object sender, RoutedEventArgs e)
    {
        _launcher.OpenInstallRoot();
    }

    private async void OnCheckUpdateClick(object sender, RoutedEventArgs e)
    {
        await RunAsync("正在检查更新", async () =>
        {
            var info = await _launcher.CheckLauncherUpdateAsync();
            if (info.HasUpdate)
            {
                await ShowUpdateDialogAsync(info);
            }
            else
            {
                SetNotice(info.ErrorMessage ?? $"当前已是最新版本 {info.CurrentVersion}");
            }
        });
    }

    private async void OnChannelClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: string tag }
            || !LauncherModelExtensions.TryParseGameChannel(tag, out var channel))
        {
            return;
        }

        _draft.ShowBe = channel is GameChannel.MindustryBE or GameChannel.MindustryXBE || _draft.ShowBe;
        _draft.ChannelVisibility.SelectOnly(channel);
        await RunAsync("正在切换频道", async () =>
        {
            await _launcher.SaveSettingsAsync(_draft);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice($"已切换至 {channel.ToDisplayName()}");
        });
    }

    private async void OnToggleBeClick(object sender, RoutedEventArgs e)
    {
        _draft.ShowBe = !_draft.ShowBe;
        await RunAsync(_draft.ShowBe ? "正在显示 BE 频道" : "正在隐藏 BE 频道", async () =>
        {
            await _launcher.SaveSettingsAsync(_draft);
            ApplyState(await _launcher.GetAppStateAsync());
        });
    }

    private void OnGoToVersionsClick(object sender, RoutedEventArgs e)
    {
        SelectView("versions");
    }

    private async void OnInstallVersionClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not RemoteVersion version || version.Installed)
        {
            return;
        }

        var sameChannel = InstalledInstances.Where(instance => instance.Channel == version.Channel).ToList();
        if (sameChannel.Count > 0)
        {
            var confirm = await ConfirmAsync(
                "切换版本",
                $"切换到 {version.DisplayName} 会移除当前 {version.ChannelDisplayName} 实例。",
                "切换");
            if (!confirm)
            {
                return;
            }
        }

        await RunAsync("正在安装版本", async () =>
        {
            if (sameChannel.Count > 0)
            {
                await _launcher.SwitchVersionAsync(version);
            }
            else
            {
                await _launcher.InstallVersionAsync(version);
            }

            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice($"{version.DisplayName} 已准备");
        });
    }

    private async void OnLaunchInstanceClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not InstalledInstance instance)
        {
            return;
        }

        await RunAsync("正在启动游戏", async () =>
        {
            var result = await _launcher.LaunchVersionAsync(instance.Id);
            SetNotice($"游戏已启动，PID {result.Pid}");
        });
    }

    private async void OnEditInstanceClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not InstalledInstance instance)
        {
            return;
        }

        await ShowInstanceSettingsDialogAsync(instance);
    }

    private async void OnDeleteInstanceClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not InstalledInstance instance)
        {
            return;
        }

        if (!await ConfirmAsync("删除实例", $"将删除 {instance.Version} 的版本文件和隔离数据。", "删除"))
        {
            return;
        }

        await RunAsync("正在删除实例", async () =>
        {
            await _launcher.DeleteInstanceAsync(instance.Id);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("实例已删除");
        });
    }

    private async void OnPickInstallRootClick(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync();
        if (folder is not null)
        {
            InstallRootBox.Text = folder.Path;
        }
    }

    private async void OnMigrateInstallRootClick(object sender, RoutedEventArgs e)
    {
        var target = InstallRootBox.Text.Trim();
        if (string.IsNullOrWhiteSpace(target))
        {
            SetNotice("请选择安装目录");
            return;
        }

        await RunAsync("正在迁移安装目录", async () =>
        {
            var result = await _launcher.MigrateInstallRootAsync(target);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice(result.Copied ? "安装目录已迁移" : "安装目录已切换");
        });
    }

    private async void OnSaveSettingsClick(object sender, RoutedEventArgs e)
    {
        await SaveSettingsAsync();
    }

    private async Task SaveSettingsAsync()
    {
        HydrateDraftFromSettingsControls();
        await RunAsync("正在保存设置", async () =>
        {
            await _launcher.SaveSettingsAsync(_draft);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("设置已保存");
        });
    }

    private async void OnThemeSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_hydrating || ThemeCombo.SelectedItem is not ComboBoxItem { Tag: string tag })
        {
            return;
        }

        _draft.Theme = tag switch
        {
            "light" => ThemePreference.Light,
            "dark" => ThemePreference.Dark,
            _ => ThemePreference.System
        };
        ApplyTheme(_draft.Theme);
        await _launcher.SaveSettingsAsync(_draft);
    }

    private void OnAcceleratorSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_hydrating || AcceleratorCombo.SelectedItem is not Accelerator accelerator)
        {
            return;
        }

        _draft.SelectedAcceleratorId = accelerator.Id;
    }

    private async void OnDebugModeToggled(object sender, RoutedEventArgs e)
    {
        if (_hydrating)
        {
            return;
        }

        _draft.DebugMode = DebugModeSwitch.IsOn;
        await _launcher.SaveSettingsAsync(_draft);
        RefreshDebugLog();
    }

    private async void OnLoadRemoteRuntimesClick(object sender, RoutedEventArgs e)
    {
        await LoadRemoteRuntimeCatalogAsync(true);
    }

    private async void OnInstallRuntimeClick(object sender, RoutedEventArgs e)
    {
        if (RemoteRuntimeCombo.SelectedItem is not RemoteRuntime runtime)
        {
            SetNotice("请先选择可下载 JRE");
            return;
        }

        await RunAsync("正在下载运行时", async () =>
        {
            await _launcher.InstallRuntimeAsync(runtime);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice($"JRE {runtime.JavaVersion} 已准备");
        });
    }

    private async void OnImportRuntimeClick(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync();
        if (folder is null)
        {
            return;
        }

        await RunAsync("正在导入运行时", async () =>
        {
            await _launcher.ImportRuntimeAsync(folder.Path);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("运行时已导入");
        });
    }

    private async void OnScanRuntimeClick(object sender, RoutedEventArgs e)
    {
        var folder = await PickFolderAsync();
        if (folder is null)
        {
            return;
        }

        await RunAsync("正在扫描运行时", async () =>
        {
            await _launcher.ScanRuntimesAsync(folder.Path);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("运行时扫描完成");
        });
    }

    private async void OnRuntimeEnabledToggled(object sender, RoutedEventArgs e)
    {
        if (_hydrating || sender is not ToggleSwitch toggle || toggle.Tag is not RuntimeInfo runtime)
        {
            return;
        }

        await RunAsync("正在更新运行时", async () =>
        {
            await _launcher.SetRuntimeEnabledAsync(runtime.Id, toggle.IsOn);
            ApplyState(await _launcher.GetAppStateAsync());
        });
    }

    private async void OnDeleteRuntimeClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is not RuntimeInfo runtime)
        {
            return;
        }

        if (!await ConfirmAsync("删除运行时", $"将删除 {runtime.DisplayName} 的启动器管理文件。", "删除"))
        {
            return;
        }

        await RunAsync("正在删除运行时", async () =>
        {
            await _launcher.DeleteRuntimeAsync(runtime.Id);
            ApplyState(await _launcher.GetAppStateAsync());
            SetNotice("运行时已删除");
        });
    }

    private void OnRefreshDebugLogClick(object sender, RoutedEventArgs e) => RefreshDebugLog();

    private void OnClearDebugLogClick(object sender, RoutedEventArgs e)
    {
        _launcher.ClearDebugLog();
        RefreshDebugLog();
    }

    private void OnOpenDebugLogDirClick(object sender, RoutedEventArgs e) => _launcher.OpenDebugLogDir();

    private void OnPauseTaskClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is TaskRecord task)
        {
            _launcher.PauseDownload(task.Id);
        }
    }

    private void OnResumeTaskClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is TaskRecord task)
        {
            _launcher.ResumeDownload(task.Id);
        }
    }

    private void OnCancelTaskClick(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.Tag is TaskRecord task)
        {
            _launcher.CancelDownload(task.Id);
        }
    }

    private void ApplyState(AppUiState state)
    {
        _hydrating = true;
        _draft = Clone(state.Settings);
        ApplyTheme(_draft.Theme);

        InstalledMetricValue.Text = state.Instances.Count.ToString();
        RuntimeMetricValue.Text = state.Runtimes.Count(item => item.Enabled).ToString();
        InstallRootMetricValue.Text = state.Settings.InstallRoot;
        InstalledInstances.ReplaceWith(state.Instances.OrderByDescending(item => item.InstalledAt));
        VisibleVersions.ReplaceWith(state.Versions
            .Where(version => state.Settings.ChannelVisibility.IsVisible(version.Channel, state.Settings.ShowBe))
            .OrderBy(version => version.Channel)
            .ThenByDescending(version => version.PublishedAt));
        Runtimes.ReplaceWith(state.Runtimes.OrderByDescending(item => item.JavaVersion).ThenBy(item => item.SourceDisplayName));

        InstallRootBox.Text = _draft.InstallRoot;
        GithubProxyBox.Text = _draft.GithubProxyPrefix ?? string.Empty;
        HttpProxyBox.Text = _draft.HttpProxy ?? string.Empty;
        DebugModeSwitch.IsOn = _draft.DebugMode;
        AcceleratorCombo.ItemsSource = state.Accelerators.Sources;
        AcceleratorCombo.SelectedItem = state.Accelerators.Sources.FirstOrDefault(source => source.Id == _draft.SelectedAcceleratorId)
            ?? state.Accelerators.Sources.FirstOrDefault();
        SelectThemeCombo(_draft.Theme);
        _hydrating = false;

        UpdateCollectionVisibility();
        ApplyResponsiveLayout(ActualWidth);
    }

    private void HydrateDraftFromSettingsControls()
    {
        _draft.InstallRoot = InstallRootBox.Text.Trim();
        _draft.GithubProxyPrefix = NullIfBlank(GithubProxyBox.Text);
        _draft.HttpProxy = NullIfBlank(HttpProxyBox.Text);
        if (AcceleratorCombo.SelectedItem is Accelerator accelerator)
        {
            _draft.SelectedAcceleratorId = accelerator.Id;
        }
    }

    private async Task LoadRemoteRuntimeCatalogAsync(bool force)
    {
        if (!force && RemoteRuntimes.Count > 0)
        {
            return;
        }

        await RunAsync("正在加载运行时列表", async () =>
        {
            RemoteRuntimes.ReplaceWith(await _launcher.ListRemoteRuntimesAsync());
            RemoteRuntimeCombo.ItemsSource = RemoteRuntimes;
            RemoteRuntimeCombo.SelectedIndex = RemoteRuntimes.Count > 0 ? 0 : -1;
            SetNotice("远端运行时列表已加载");
        });
    }

    private async Task ShowInstanceSettingsDialogAsync(InstalledInstance instance)
    {
        var runtimeCombo = new ComboBox
        {
            Header = "运行时",
            DisplayMemberPath = "DisplayName",
            ItemsSource = Runtimes,
            SelectedItem = Runtimes.FirstOrDefault(runtime => runtime.Id == instance.RuntimeId)
        };
        var minMemory = new NumberBox { Header = "最小内存 MB", Minimum = 0, Value = instance.LaunchSettings.MinMemoryMb ?? double.NaN };
        var maxMemory = new NumberBox { Header = "最大内存 MB", Minimum = 0, Value = instance.LaunchSettings.MaxMemoryMb ?? double.NaN };
        var jvmArgs = new TextBox { Header = "JVM 参数", Text = instance.LaunchSettings.ExtraJvmArgs };
        var gameArgs = new TextBox { Header = "游戏参数", Text = instance.LaunchSettings.GameArgs };

        var panel = new StackPanel { Spacing = 12 };
        panel.Children.Add(runtimeCombo);
        panel.Children.Add(minMemory);
        panel.Children.Add(maxMemory);
        panel.Children.Add(jvmArgs);
        panel.Children.Add(gameArgs);

        var dialog = new ContentDialog
        {
            Title = instance.Version,
            Content = panel,
            PrimaryButtonText = "保存",
            CloseButtonText = "取消",
            XamlRoot = XamlRoot
        };

        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await RunAsync("正在保存启动参数", async () =>
            {
                await _launcher.SaveInstanceLaunchSettingsAsync(
                    instance.Id,
                    (runtimeCombo.SelectedItem as RuntimeInfo)?.Id,
                    new LaunchSettings
                    {
                        MinMemoryMb = DoubleToUInt(minMemory.Value),
                        MaxMemoryMb = DoubleToUInt(maxMemory.Value),
                        ExtraJvmArgs = jvmArgs.Text,
                        GameArgs = gameArgs.Text
                    });
                ApplyState(await _launcher.GetAppStateAsync());
                SetNotice("启动参数已保存");
            });
        }
    }

    private async Task ShowUpdateDialogAsync(LauncherUpdateInfo info)
    {
        var panel = new StackPanel { Spacing = 10 };
        panel.Children.Add(new TextBlock { Text = $"当前版本 {info.CurrentVersion}，最新版本 {info.LatestVersion}", TextWrapping = TextWrapping.WrapWholeWords });
        if (!string.IsNullOrWhiteSpace(info.ReleaseBody))
        {
            panel.Children.Add(new TextBlock { Text = info.ReleaseBody, TextWrapping = TextWrapping.WrapWholeWords, MaxHeight = 260 });
        }

        var dialog = new ContentDialog
        {
            Title = "发现新版本",
            Content = panel,
            PrimaryButtonText = "前往下载",
            SecondaryButtonText = "忽略此版本",
            CloseButtonText = "稍后",
            XamlRoot = XamlRoot
        };

        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.Primary)
        {
            _launcher.OpenUrl(info.ReleaseUrl);
        }
        else if (result == ContentDialogResult.Secondary)
        {
            await _launcher.IgnoreLauncherVersionAsync(info.LatestVersion);
        }
    }

    private async Task<bool> ConfirmAsync(string title, string message, string primaryText)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.WrapWholeWords },
            PrimaryButtonText = primaryText,
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = XamlRoot
        };

        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private async Task<Windows.Storage.StorageFolder?> PickFolderAsync()
    {
        var picker = new FolderPicker();
        InitializeWithWindow.Initialize(picker, MainWindow.WindowHandle);
        picker.FileTypeFilter.Add("*");
        return await picker.PickSingleFolderAsync();
    }

    private async Task RunAsync(string busyNotice, Func<Task> action)
    {
        SetNotice(busyNotice);
        try
        {
            await action();
        }
        catch (Exception ex)
        {
            AppDebugLog.Error(ex.ToString());
            SetNotice(ex.Message);
        }
    }

    private CancellationTokenSource? _noticeCts;

    private void SetNotice(string text)
    {
        NoticeText.Text = text;
        NoticeInfoBar.IsOpen = true;
        NoticeInfoBar.Severity =
            text.Contains("失败", StringComparison.Ordinal) || text.Contains("错误", StringComparison.Ordinal)
                ? InfoBarSeverity.Error
                : text.Contains("正在", StringComparison.Ordinal)
                    ? InfoBarSeverity.Informational
                    : InfoBarSeverity.Success;

        _noticeCts?.Cancel();
        _noticeCts = new CancellationTokenSource();
        var token = _noticeCts.Token;
        _ = Task.Delay(5000, token).ContinueWith(_ =>
        {
            if (!token.IsCancellationRequested)
            {
                DispatcherQueue.TryEnqueue(() => NoticeInfoBar.IsOpen = false);
            }
        }, TaskContinuationOptions.NotOnCanceled);
    }

    private void RefreshDebugLog()
    {
        var snapshot = _launcher.ReadDebugLog();
        DebugLogBox.Text = snapshot.Content;
        DebugModeSwitch.IsOn = snapshot.Enabled;
    }

    private void ApplyTheme(ThemePreference theme)
    {
        ContentRoot.RequestedTheme = theme switch
        {
            ThemePreference.Light => ElementTheme.Light,
            ThemePreference.Dark => ElementTheme.Dark,
            _ => ElementTheme.Default
        };
    }

    private void ApplyResponsiveLayout(double suggestedWidth)
    {
        var layoutWidth = ContentRoot.ActualWidth > 0 ? ContentRoot.ActualWidth : suggestedWidth;
        if (layoutWidth <= 0)
        {
            return;
        }

        ContentRoot.Padding = layoutWidth < 720
            ? (Thickness)Application.Current.Resources["LauncherCompactPagePadding"]
            : (Thickness)Application.Current.Resources["LauncherPagePadding"];

        var padding = ContentRoot.Padding;
        var maxContentWidth = (double)Application.Current.Resources["LauncherContentMaxWidth"];
        var panelWidth = Math.Max(0, Math.Min(maxContentWidth, layoutWidth - padding.Left - padding.Right));

        foreach (var element in new FrameworkElement[]
        {
            PageHeader,
            NoticeInfoBar,
            GamesPanel,
            VersionsPanel,
            SettingsPanel,
            DebugPanel,
            TasksList
        })
        {
            element.Width = panelWidth;
        }

        ApplyMetricLayout(panelWidth);
        ApplyChannelLayout(panelWidth);
        ApplySettingsLayout(panelWidth);
        ApplyCommandDensity(panelWidth);
    }

    private void ApplyMetricLayout(double width)
    {
        var wide = width >= 900;
        var medium = width >= 560;
        var columnCount = wide ? 3 : medium ? 2 : 1;
        var rowCount = wide ? 1 : medium ? 2 : 3;

        for (var index = 0; index < MetricsGrid.ColumnDefinitions.Count; index++)
        {
            MetricsGrid.ColumnDefinitions[index].Width = index < columnCount
                ? new GridLength(1, GridUnitType.Star)
                : new GridLength(0);
        }

        for (var index = 0; index < MetricsGrid.RowDefinitions.Count; index++)
        {
            MetricsGrid.RowDefinitions[index].Height = index < rowCount ? GridLength.Auto : new GridLength(0);
        }

        Grid.SetColumn(InstalledMetricCard, 0);
        Grid.SetRow(InstalledMetricCard, 0);
        Grid.SetColumnSpan(InstalledMetricCard, 1);

        Grid.SetColumn(RuntimeMetricCard, wide || medium ? 1 : 0);
        Grid.SetRow(RuntimeMetricCard, wide || medium ? 0 : 1);
        Grid.SetColumnSpan(RuntimeMetricCard, 1);

        Grid.SetColumn(InstallRootMetricCard, wide ? 2 : 0);
        Grid.SetRow(InstallRootMetricCard, wide ? 0 : medium ? 1 : 2);
        Grid.SetColumnSpan(InstallRootMetricCard, medium && !wide ? 2 : 1);
    }

    private void ApplyChannelLayout(double width)
    {
        var columnCount = width < 520 ? 1 : width < 820 ? 2 : 4;
        var buttons = new[]
        {
            MindustryChannelButton,
            MindustryXChannelButton,
            MindustryBeChannelButton,
            MindustryXBeChannelButton
        };
        var rowCount = (int)Math.Ceiling(buttons.Length / (double)columnCount);

        for (var index = 0; index < ChannelGrid.ColumnDefinitions.Count; index++)
        {
            ChannelGrid.ColumnDefinitions[index].Width = index < columnCount
                ? new GridLength(1, GridUnitType.Star)
                : new GridLength(0);
        }

        for (var index = 0; index < ChannelGrid.RowDefinitions.Count; index++)
        {
            ChannelGrid.RowDefinitions[index].Height = index < rowCount ? GridLength.Auto : new GridLength(0);
        }

        for (var index = 0; index < buttons.Length; index++)
        {
            Grid.SetColumn(buttons[index], index % columnCount);
            Grid.SetRow(buttons[index], index / columnCount);
        }
    }

    private void ApplySettingsLayout(double width)
    {
        var twoColumns = width >= 760;
        var fieldWidth = Math.Max(0, (twoColumns ? (width - SettingsTopGrid.ColumnSpacing) / 2 : width) - 32);
        ThemeCombo.Width = Math.Min(320, fieldWidth);

        ApplyColumnMode(SettingsTopGrid, twoColumns ? 2 : 1);
        ApplyColumnMode(SettingsBottomGrid, twoColumns ? 2 : 1);

        Grid.SetColumn(AppearanceCard, 0);
        Grid.SetRow(AppearanceCard, 0);

        Grid.SetColumn(InstallRootCard, twoColumns ? 1 : 0);
        Grid.SetRow(InstallRootCard, twoColumns ? 0 : 1);

        Grid.SetColumn(NetworkCard, 0);
        Grid.SetRow(NetworkCard, 0);

        Grid.SetColumn(RuntimeCard, twoColumns ? 1 : 0);
        Grid.SetRow(RuntimeCard, twoColumns ? 0 : 1);
    }

    private static void ApplyColumnMode(Grid grid, int columnCount)
    {
        var rowCount = (int)Math.Ceiling(grid.Children.Count / (double)Math.Max(1, columnCount));

        for (var index = 0; index < grid.ColumnDefinitions.Count; index++)
        {
            grid.ColumnDefinitions[index].Width = index < columnCount
                ? new GridLength(1, GridUnitType.Star)
                : new GridLength(0);
        }

        for (var index = 0; index < grid.RowDefinitions.Count; index++)
        {
            grid.RowDefinitions[index].Height = index < rowCount
                ? GridLength.Auto
                : new GridLength(0);
        }
    }

    private void ApplyCommandDensity(double width)
    {
        var labelPosition = width < 620
            ? CommandBarDefaultLabelPosition.Collapsed
            : CommandBarDefaultLabelPosition.Right;

        foreach (var commandBar in new[]
        {
            GamesCommandBar,
            VersionsCommandBar,
            InstallRootCommandBar,
            NetworkCommandBar,
            RuntimeCommandBar,
            DebugCommandBar
        })
        {
            commandBar.DefaultLabelPosition = labelPosition;
        }
    }

    private void UpdateCollectionVisibility()
    {
        var hasInstances = InstalledInstances.Count > 0;
        InstancesList.Visibility = hasInstances ? Visibility.Visible : Visibility.Collapsed;
        InstancesEmptyState.Visibility = hasInstances ? Visibility.Collapsed : Visibility.Visible;

        var hasVersions = VisibleVersions.Count > 0;
        VersionsList.Visibility = hasVersions ? Visibility.Visible : Visibility.Collapsed;
        VersionsEmptyState.Visibility = hasVersions ? Visibility.Collapsed : Visibility.Visible;
    }

    private void SelectThemeCombo(ThemePreference theme)
    {
        var tag = theme switch
        {
            ThemePreference.Light => "light",
            ThemePreference.Dark => "dark",
            _ => "system"
        };

        ThemeCombo.SelectedItem = ThemeCombo.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(item => (string)item.Tag == tag);
    }

    private void OnTaskChanged(TaskRecord task)
    {
        _ = DispatcherQueue.TryEnqueue(() =>
        {
            var existing = ActiveTasks.FirstOrDefault(item => item.Id == task.Id);
            if (existing is null)
            {
                existing = new TaskRecord { Id = task.Id, Label = task.Label };
                ActiveTasks.Add(existing);
            }

            existing.DownloadedBytes = task.DownloadedBytes;
            existing.TotalBytes = task.TotalBytes;
            existing.BytesPerSecond = task.BytesPerSecond;
            existing.Status = task.Status;
            existing.Message = task.Message;

            if (task.Status is "finished" or "canceled")
            {
                _ = Task.Delay(3200).ContinueWith(_task =>
                {
                    _ = DispatcherQueue.TryEnqueue(() => ActiveTasks.Remove(existing));
                });
            }
        });
    }

    private static Settings Clone(Settings value)
    {
        return JsonSerializer.Deserialize<Settings>(JsonSerializer.Serialize(value, JsonSettings.Options), JsonSettings.Options) ?? new Settings();
    }

    private static string? NullIfBlank(string value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static uint? DoubleToUInt(double value) => double.IsNaN(value) || value <= 0 ? null : (uint)Math.Round(value);

    private void OnPropertyChanged(string propertyName)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}

public static class ObservableCollectionExtensions
{
    public static void ReplaceWith<T>(this ObservableCollection<T> collection, IEnumerable<T> items)
    {
        collection.Clear();
        foreach (var item in items)
        {
            collection.Add(item);
        }
    }
}
