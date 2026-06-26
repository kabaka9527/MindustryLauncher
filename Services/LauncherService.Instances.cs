using System.Diagnostics;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    public async Task<InstalledInstance> InstallVersionAsync(RemoteVersion version)
    {
        var layout = Layout;
        layout.Ensure();
        var asset = version.SelectedAsset ?? SelectDesktopJar(version.Assets)
            ?? throw new InvalidOperationException($"未找到 {version.DisplayName} 的桌面 jar");
        var safeTag = SafePathPart(version.Tag);
        var versionDir = Path.Combine(layout.VersionsDir, version.Channel.ToWireValue(), safeTag);
        var jarPath = Path.Combine(versionDir, "Mindustry.jar");
        var instanceDir = Path.Combine(layout.InstancesDir, $"{version.Channel.ToWireValue()}-{safeTag}");
        var dataDir = Path.Combine(instanceDir, "data");
        var logDir = Path.Combine(instanceDir, "logs");
        var hadExisting = (await LoadInstancesAsync(layout)).Any(instance => instance.Id == version.Id);

        Directory.CreateDirectory(versionDir);
        Directory.CreateDirectory(dataDir);
        Directory.CreateDirectory(logDir);

        try
        {
            var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
            await network.DownloadToFileAsync(
                RewriteGithubUrl(asset.DownloadUrl),
                jarPath,
                asset.Digest,
                asset.Size > 0 ? asset.Size : null,
                $"game:{version.Id}",
                $"下载 {version.DisplayName}",
                OnTask);

            var requiredJava = RequiredJavaFromJar(jarPath);
            var runtime = await EnsureRuntimeAsync(requiredJava);
            var instance = new InstalledInstance
            {
                Id = version.Id,
                Channel = version.Channel,
                Version = version.Version,
                InstallDir = instanceDir,
                DataDir = dataDir,
                JarPath = jarPath,
                RuntimeId = runtime.Id,
                InstalledAt = DateTimeOffset.UtcNow.ToString("O"),
                LaunchSettings = new LaunchSettings()
            };

            var instances = await LoadInstancesAsync(layout);
            instances.RemoveAll(item => item.Id == instance.Id);
            instances.Add(instance);
            await SaveInstancesAsync(layout, instances);
            return instance;
        }
        catch
        {
            if (!hadExisting)
            {
                CleanupInstallArtifacts(layout, instanceDir, versionDir);
            }
            else
            {
                TryDeleteFile(Path.ChangeExtension(jarPath, ".download"));
                await CleanupPartialDownloadsAsync(layout);
            }

            throw;
        }
    }

    public async Task<InstalledInstance> SwitchVersionAsync(RemoteVersion version)
    {
        var layout = Layout;
        var oldInstances = (await LoadInstancesAsync(layout))
            .Where(instance => instance.Channel == version.Channel && instance.Id != version.Id)
            .ToList();
        var installed = await InstallVersionAsync(version);
        foreach (var instance in oldInstances)
        {
            await DeleteInstanceAsync(instance.Id);
        }

        return installed;
    }

    public async Task<List<InstalledInstance>> DeleteInstanceAsync(string instanceId)
    {
        var layout = Layout;
        var instances = await LoadInstancesAsync(layout);
        var instance = instances.FirstOrDefault(item => item.Id == instanceId)
            ?? throw new InvalidOperationException($"未找到实例：{instanceId}");

        FileSystemUtil.AssertInsideRoot(layout.Root, instance.InstallDir);
        FileSystemUtil.DeleteDirectoryRetry(instance.InstallDir);
        if (!string.IsNullOrWhiteSpace(instance.JarPath))
        {
            var versionDir = Path.GetDirectoryName(instance.JarPath);
            if (!string.IsNullOrWhiteSpace(versionDir))
            {
                FileSystemUtil.AssertInsideRoot(layout.Root, versionDir);
                FileSystemUtil.DeleteDirectoryRetry(versionDir);
            }
        }

        instances.RemoveAll(item => item.Id == instanceId);
        await SaveInstancesAsync(layout, instances);
        await CleanupPartialDownloadsAsync(layout);
        return instances;
    }

    public async Task<List<InstalledInstance>> SaveInstanceLaunchSettingsAsync(string instanceId, string? runtimeId, LaunchSettings launchSettings)
    {
        if (launchSettings.MinMemoryMb is uint min && launchSettings.MaxMemoryMb is uint max && min > max)
        {
            throw new InvalidOperationException("最小内存不能大于最大内存");
        }

        var layout = Layout;
        var instances = await LoadInstancesAsync(layout);
        var instance = instances.FirstOrDefault(item => item.Id == instanceId)
            ?? throw new InvalidOperationException($"未找到实例：{instanceId}");
        instance.RuntimeId = string.IsNullOrWhiteSpace(runtimeId) ? null : runtimeId;
        instance.LaunchSettings = launchSettings;
        await SaveInstancesAsync(layout, instances);
        return instances;
    }

    public async Task<LaunchResult> LaunchVersionAsync(string instanceId)
    {
        var layout = Layout;
        var instance = (await LoadInstancesAsync(layout)).FirstOrDefault(item => item.Id == instanceId)
            ?? throw new InvalidOperationException($"未找到实例：{instanceId}");
        if (!File.Exists(instance.JarPath))
        {
            throw new FileNotFoundException($"未找到 jar：{instance.JarPath}");
        }

        Directory.CreateDirectory(instance.DataDir);
        var requiredJava = RequiredJavaFromJar(instance.JarPath);
        var runtimes = await LoadRuntimesAsync(layout);
        var runtime = !string.IsNullOrWhiteSpace(instance.RuntimeId)
            ? runtimes.FirstOrDefault(item => item.Enabled && item.Id == instance.RuntimeId && File.Exists(item.JavaPath))
            : runtimes
                .Where(item => item.Enabled && item.JavaVersion >= requiredJava && File.Exists(item.JavaPath))
                .OrderBy(item => item.JavaVersion)
                .FirstOrDefault();
        if (runtime is null)
        {
            throw new InvalidOperationException($"缺少 JRE {requiredJava} 或更高版本");
        }

        var logDir = Path.Combine(instance.InstallDir, "logs");
        Directory.CreateDirectory(logDir);
        var logPath = Path.Combine(logDir, $"launch-{DateTime.Now:yyyyMMdd-HHmmss}.log");
        var args = new List<string>();
        if (instance.LaunchSettings.MinMemoryMb is > 0)
        {
            args.Add($"-Xms{instance.LaunchSettings.MinMemoryMb}m");
        }

        if (instance.LaunchSettings.MaxMemoryMb is > 0)
        {
            args.Add($"-Xmx{instance.LaunchSettings.MaxMemoryMb}m");
        }

        args.AddRange(SplitCommandArgs(instance.LaunchSettings.ExtraJvmArgs));
        args.Add($"-Dmindustry.data.dir={instance.DataDir}");
        args.Add("-jar");
        args.Add(instance.JarPath);
        args.AddRange(SplitCommandArgs(instance.LaunchSettings.GameArgs));

        var process = Process.Start(new ProcessStartInfo(runtime.JavaPath)
        {
            WorkingDirectory = instance.InstallDir,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true
        }.WithArguments(args, instance.DataDir, logPath)) ?? throw new InvalidOperationException("启动游戏失败");

        _ = Task.Run(async () =>
        {
            await using var log = File.Open(logPath, FileMode.Append, FileAccess.Write, FileShare.Read);
            await Task.WhenAll(process.StandardOutput.BaseStream.CopyToAsync(log), process.StandardError.BaseStream.CopyToAsync(log));
        });

        return new LaunchResult { Pid = (uint)process.Id, LogPath = logPath };
    }

    public async Task<MigrationResult> MigrateInstallRootAsync(string newRoot)
    {
        if (string.IsNullOrWhiteSpace(newRoot))
        {
            throw new InvalidOperationException("新的安装目录不能为空");
        }

        var oldLayout = Layout;
        var newRootFull = Path.GetFullPath(newRoot);
        Directory.CreateDirectory(newRootFull);
        var newLayout = new InstallLayout(newRootFull);
        newLayout.Ensure();
        var copied = false;

        if (!oldLayout.Root.Equals(newLayout.Root, StringComparison.OrdinalIgnoreCase) && Directory.Exists(oldLayout.Root))
        {
            if (newLayout.Root.StartsWith(oldLayout.Root, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("新的安装目录不能位于当前安装目录内部");
            }

            FileSystemUtil.CopyDirectory(oldLayout.Root, newLayout.Root);
            await RewriteMetadataPathsAsync(oldLayout.Root, newLayout.Root, newLayout);
            copied = true;
        }

        _settings.InstallRoot = newLayout.Root;
        await SaveSettingsAsync(_settings);
        return new MigrationResult { OldRoot = oldLayout.Root, NewRoot = newLayout.Root, Copied = copied };
    }

    private static async Task CleanupPartialDownloadsAsync(InstallLayout layout)
    {
        await Task.CompletedTask;
        if (!Directory.Exists(layout.VersionsDir))
        {
            return;
        }

        foreach (var file in Directory.EnumerateFiles(layout.VersionsDir, "*.download", SearchOption.AllDirectories))
        {
            FileSystemUtil.AssertInsideRoot(layout.Root, file);
            TryDeleteFile(file);
        }
    }

    private static void CleanupInstallArtifacts(InstallLayout layout, string instanceDir, string versionDir)
    {
        FileSystemUtil.AssertInsideRoot(layout.Root, instanceDir);
        FileSystemUtil.AssertInsideRoot(layout.Root, versionDir);
        FileSystemUtil.DeleteDirectoryRetry(instanceDir);
        FileSystemUtil.DeleteDirectoryRetry(versionDir);
    }

    private static async Task RewriteMetadataPathsAsync(string oldRoot, string newRoot, InstallLayout layout)
    {
        var instances = await LoadInstancesAsync(layout);
        foreach (var instance in instances)
        {
            instance.InstallDir = RewritePath(oldRoot, newRoot, instance.InstallDir);
            instance.DataDir = RewritePath(oldRoot, newRoot, instance.DataDir);
            instance.JarPath = RewritePath(oldRoot, newRoot, instance.JarPath);
        }

        await SaveInstancesAsync(layout, instances);

        var runtimes = await LoadRuntimesAsync(layout);
        foreach (var runtime in runtimes)
        {
            runtime.Path = RewritePath(oldRoot, newRoot, runtime.Path);
            runtime.JavaPath = RewritePath(oldRoot, newRoot, runtime.JavaPath);
        }

        await SaveRuntimesAsync(layout, runtimes);
    }

    private static string RewritePath(string oldRoot, string newRoot, string value)
    {
        var full = Path.GetFullPath(value);
        return full.StartsWith(oldRoot, StringComparison.OrdinalIgnoreCase)
            ? Path.Combine(newRoot, Path.GetRelativePath(oldRoot, full))
            : value;
    }

    private static List<string> SplitCommandArgs(string input)
    {
        var args = new List<string>();
        var current = string.Empty;
        char? quote = null;
        var escaped = false;

        foreach (var ch in input ?? string.Empty)
        {
            if (escaped)
            {
                current += ch;
                escaped = false;
                continue;
            }

            if (ch == '\\')
            {
                escaped = true;
                continue;
            }

            if (quote is char active)
            {
                if (ch == active)
                {
                    quote = null;
                }
                else
                {
                    current += ch;
                }

                continue;
            }

            if (ch is '"' or '\'')
            {
                quote = ch;
            }
            else if (char.IsWhiteSpace(ch))
            {
                if (current.Length > 0)
                {
                    args.Add(current);
                    current = string.Empty;
                }
            }
            else
            {
                current += ch;
            }
        }

        if (escaped)
        {
            current += '\\';
        }

        if (quote is not null)
        {
            throw new InvalidOperationException("启动参数存在未闭合引号");
        }

        if (current.Length > 0)
        {
            args.Add(current);
        }

        return args;
    }
}

public static class ProcessStartInfoExtensions
{
    public static ProcessStartInfo WithArguments(this ProcessStartInfo info, IEnumerable<string> args, string dataDir, string logPath)
    {
        foreach (var arg in args)
        {
            info.ArgumentList.Add(arg);
        }

        info.Environment["MINDUSTRY_DATA_DIR"] = dataDir;
        info.Environment["MINDUSTRY_LAUNCHER_LOG"] = logPath;
        return info;
    }
}
