using System.Diagnostics;
using System.IO.Compression;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    private const string DefaultAcceleratorPrefix = "https://hubproxy.kabaka.xyz/";
    private const string RemoteAcceleratorList = "https://raw.githubusercontent.com/kabaka9527/MindustryLauncher/main/resources/github-accelerators.json";

    private Settings _settings = new();
    private AcceleratorList _accelerators = DefaultAccelerators();

    public event Action<TaskRecord>? TaskChanged;

    public Settings Settings => _settings;
    public AcceleratorList Accelerators => _accelerators;

    private InstallLayout Layout => new(_settings.InstallRoot);

    public async Task<AppUiState> InitializeAsync()
    {
        _settings = await LoadSettingsAsync();
        var layout = Layout;
        layout.Ensure();
        AppDebugLog.Configure(Path.Combine(layout.LogsDir, "debug.log"), _settings.DebugMode);
        await CleanupPartialDownloadsAsync(layout);
        await ScanSystemRuntimesAsync();
        _accelerators = await LoadStartupAcceleratorsAsync(layout);

        return await GetAppStateAsync();
    }

    public async Task<AppUiState> GetAppStateAsync()
    {
        var layout = Layout;
        layout.Ensure();
        return new AppUiState
        {
            Settings = _settings,
            Accelerators = _accelerators,
            Versions = await LoadCachedVersionsAsync(layout),
            Instances = await LoadInstancesWithRunningStateAsync(),
            Runtimes = await LoadRuntimesAsync(layout)
        };
    }

    public async Task<Settings> SaveSettingsAsync(Settings settings)
    {
        if (string.IsNullOrWhiteSpace(settings.InstallRoot))
        {
            throw new InvalidOperationException("安装目录不能为空");
        }

        _settings = settings;
        var layout = Layout;
        layout.Ensure();
        await FileSystemUtil.WriteJsonAsync(layout.SettingsPath, settings);
        await AppPaths.SaveInstallRootAsync(layout.Root);
        AppDebugLog.Configure(Path.Combine(layout.LogsDir, "debug.log"), settings.DebugMode);
        return settings;
    }

    public async Task<AcceleratorList> RefreshAcceleratorsAsync()
    {
        var layout = Layout;
        try
        {
            var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
            var urls = new[]
            {
                PrefixUrl(DefaultAcceleratorPrefix, RemoteAcceleratorList),
                RemoteAcceleratorList
            };

            foreach (var url in urls)
            {
                try
                {
                    var list = await network.GetJsonUncachedAsync<AcceleratorList>(url);
                    _accelerators = EnsureRequiredAccelerators(list);
                    return _accelerators;
                }
                catch (Exception ex)
                {
                    AppDebugLog.Warn($"GitHub 加速源获取失败 ({url})：{ex.Message}");
                }
            }
        }
        catch (Exception ex)
        {
            AppDebugLog.Warn($"GitHub 加速源刷新失败：{ex.Message}");
        }

        _accelerators = DefaultAccelerators();
        return _accelerators;
    }

    public void PauseDownload(string taskId) => NetworkClient.PauseDownload(taskId);

    public void ResumeDownload(string taskId) => NetworkClient.ResumeDownload(taskId);

    public void CancelDownload(string taskId) => NetworkClient.CancelDownload(taskId);

    public void OpenInstallRoot()
    {
        var layout = Layout;
        layout.Ensure();
        Process.Start(new ProcessStartInfo("explorer.exe", layout.Root) { UseShellExecute = true });
    }

    public void OpenPath(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        Process.Start(new ProcessStartInfo("explorer.exe", path) { UseShellExecute = true });
    }

    public void OpenUrl(string url)
    {
        if (string.IsNullOrWhiteSpace(url))
        {
            return;
        }

        Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
    }

    public DebugLogSnapshot ReadDebugLog() => AppDebugLog.Snapshot();

    public DebugLogSnapshot ClearDebugLog() => AppDebugLog.Clear();

    public void OpenDebugLogDir()
    {
        var snapshot = AppDebugLog.Snapshot();
        var directory = Path.GetDirectoryName(snapshot.LogPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
            OpenPath(directory);
        }
    }

    private static async Task<Settings> LoadSettingsAsync()
    {
        var root = await AppPaths.LoadInstallRootAsync();
        var layout = new InstallLayout(root);
        layout.Ensure();

        var settings = await FileSystemUtil.ReadJsonAsync<Settings>(layout.SettingsPath)
            ?? new Settings { InstallRoot = root };
        if (string.IsNullOrWhiteSpace(settings.InstallRoot))
        {
            settings.InstallRoot = root;
        }

        if (settings.ChannelVisibility is null)
        {
            settings.ChannelVisibility = new ChannelVisibility();
        }

        await FileSystemUtil.WriteJsonAsync(layout.SettingsPath, settings);
        await AppPaths.SaveInstallRootAsync(settings.InstallRoot);
        return settings;
    }

    private static async Task<AcceleratorList> LoadStartupAcceleratorsAsync(InstallLayout layout)
    {
        await Task.CompletedTask;
        return EnsureRequiredAccelerators(DefaultAccelerators());
    }

    private static AcceleratorList EnsureRequiredAccelerators(AcceleratorList list)
    {
        if (list.Sources.All(source => source.Id != "hubproxy-kabaka"))
        {
            list.Sources.Insert(0, DefaultAccelerators().Sources[0]);
        }

        if (list.Sources.All(source => source.Id != "direct"))
        {
            list.Sources.Add(DefaultAccelerators().Sources[^1]);
        }

        return list;
    }

    private static AcceleratorList DefaultAccelerators()
    {
        return new AcceleratorList
        {
            Version = 1,
            UpdatedAt = "built-in",
            Sources =
            [
                new Accelerator
                {
                    Id = "hubproxy-kabaka",
                    Name = "HubProxy",
                    BaseUrl = DefaultAcceleratorPrefix,
                    Supports = new AcceleratorSupports { Api = true, Raw = true, ReleaseAsset = true },
                    HealthCheckUrl = PrefixUrl(DefaultAcceleratorPrefix, "https://github.com/"),
                    EnabledByDefault = true
                },
                new Accelerator
                {
                    Id = "gh-proxy",
                    Name = "GHProxy GitHub 加速",
                    BaseUrl = "https://gh-proxy.com/",
                    Supports = new AcceleratorSupports { Raw = true, ReleaseAsset = true },
                    HealthCheckUrl = "https://gh-proxy.com/https://github.com/"
                },
                new Accelerator
                {
                    Id = "direct",
                    Name = "GitHub 直连",
                    Supports = new AcceleratorSupports { Api = true, Raw = true, ReleaseAsset = true },
                    Rules =
                    [
                        new AcceleratorRule { From = "https://api.github.com/", To = "https://api.github.com/" },
                        new AcceleratorRule { From = "https://raw.githubusercontent.com/", To = "https://raw.githubusercontent.com/" },
                        new AcceleratorRule { From = "https://github.com/", To = "https://github.com/" }
                    ],
                    HealthCheckUrl = "https://github.com/"
                }
            ]
        };
    }

    private static string PrefixUrl(string prefix, string originalUrl)
    {
        return $"{prefix.TrimEnd('/')}/{originalUrl.TrimStart('/')}";
    }

    private string RewriteGithubUrl(string originalUrl)
    {
        var target = ClassifyGithubUrl(originalUrl);
        if (target is null)
        {
            return originalUrl;
        }

        if (!string.IsNullOrWhiteSpace(_settings.GithubProxyPrefix))
        {
            return PrefixUrl(_settings.GithubProxyPrefix, originalUrl);
        }

        var selected = _accelerators.Sources.FirstOrDefault(source => source.Id == _settings.SelectedAcceleratorId)
            ?? _accelerators.Sources.FirstOrDefault(source => source.EnabledByDefault);
        if (selected is null || !SupportsTarget(selected, target))
        {
            return originalUrl;
        }

        foreach (var rule in selected.Rules)
        {
            if (originalUrl.StartsWith(rule.From, StringComparison.OrdinalIgnoreCase))
            {
                return rule.To + originalUrl[rule.From.Length..];
            }
        }

        return PrefixUrl(selected.BaseUrl, originalUrl);
    }

    private IEnumerable<string> GithubUrlCandidates(string originalUrl)
    {
        var rewritten = RewriteGithubUrl(originalUrl);
        yield return rewritten;
        if (!string.Equals(rewritten, originalUrl, StringComparison.OrdinalIgnoreCase))
        {
            yield return originalUrl;
        }
    }

    private static string? ClassifyGithubUrl(string url)
    {
        if (url.StartsWith("https://api.github.com/", StringComparison.OrdinalIgnoreCase))
        {
            return "api";
        }

        if (url.StartsWith("https://raw.githubusercontent.com/", StringComparison.OrdinalIgnoreCase))
        {
            return "raw";
        }

        if (url.StartsWith("https://github.com/", StringComparison.OrdinalIgnoreCase))
        {
            return "releaseAsset";
        }

        return null;
    }

    private static bool SupportsTarget(Accelerator accelerator, string? target)
    {
        return target switch
        {
            "api" => accelerator.Supports.Api,
            "raw" => accelerator.Supports.Raw,
            "releaseAsset" => accelerator.Supports.ReleaseAsset,
            _ => false
        };
    }

    private static async Task<List<InstalledInstance>> LoadInstancesAsync(InstallLayout layout)
    {
        return await FileSystemUtil.ReadJsonAsync<List<InstalledInstance>>(layout.InstancesPath) ?? [];
    }

    private static async Task SaveInstancesAsync(InstallLayout layout, List<InstalledInstance> instances)
    {
        await FileSystemUtil.WriteJsonAsync(layout.InstancesPath, instances);
    }

    private static async Task<List<RuntimeInfo>> LoadRuntimesAsync(InstallLayout layout)
    {
        var runtimes = await FileSystemUtil.ReadJsonAsync<List<RuntimeInfo>>(layout.RuntimesPath) ?? [];
        foreach (var runtime in runtimes.Where(runtime => runtime.Source == RuntimeSource.Unknown))
        {
            runtime.Source = runtime.Id switch
            {
                var id when id.StartsWith("jre-", StringComparison.OrdinalIgnoreCase) => RuntimeSource.Launcher,
                var id when id.StartsWith("imported-", StringComparison.OrdinalIgnoreCase) => RuntimeSource.Imported,
                var id when id.StartsWith("local-", StringComparison.OrdinalIgnoreCase) => RuntimeSource.Scanned,
                var id when id.StartsWith("system-", StringComparison.OrdinalIgnoreCase) => RuntimeSource.System,
                _ => RuntimeSource.Unknown
            };
        }

        return runtimes;
    }

    private static async Task SaveRuntimesAsync(InstallLayout layout, List<RuntimeInfo> runtimes)
    {
        await FileSystemUtil.WriteJsonAsync(layout.RuntimesPath, runtimes);
    }

    private static async Task<List<RemoteVersion>> LoadCachedVersionsAsync(InstallLayout layout)
    {
        return await FileSystemUtil.ReadJsonAsync<List<RemoteVersion>>(layout.VersionsCachePath) ?? [];
    }

    private static async Task SaveCachedVersionsAsync(InstallLayout layout, List<RemoteVersion> versions)
    {
        await FileSystemUtil.WriteJsonAsync(layout.VersionsCachePath, versions);
    }

    private static string SafePathPart(string value)
    {
        var sanitized = Regex.Replace(value, "[^A-Za-z0-9._-]+", "-").Trim('-', '.');
        return string.IsNullOrWhiteSpace(sanitized) ? "item" : sanitized;
    }

    private void OnTask(TaskRecord task) => TaskChanged?.Invoke(task);
}

public sealed class GitHubRelease
{
    [JsonPropertyName("tag_name")]
    public string TagName { get; set; } = string.Empty;

    public string? Name { get; set; }

    public bool Prerelease { get; set; }

    [JsonPropertyName("published_at")]
    public string? PublishedAt { get; set; }

    [JsonPropertyName("html_url")]
    public string HtmlUrl { get; set; } = string.Empty;

    public string? Body { get; set; }

    public List<GitHubAsset> Assets { get; set; } = [];
}

public sealed class GitHubAsset
{
    public string Name { get; set; } = string.Empty;
    public ulong Size { get; set; }

    [JsonPropertyName("browser_download_url")]
    public string BrowserDownloadUrl { get; set; } = string.Empty;

    public string? Digest { get; set; }
}
