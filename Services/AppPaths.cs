using System.Text.Json;
using System.Text.Json.Serialization.Metadata;

namespace MindustryLauncher.Services;

public sealed class InstallLayout
{
    public InstallLayout(string root)
    {
        Root = Path.GetFullPath(root);
        ConfigDir = Path.Combine(Root, "config");
        CacheDir = Path.Combine(Root, "cache");
        DownloadsDir = Path.Combine(Root, "downloads");
        TempDownloadsDir = Path.Combine(DownloadsDir, "tmp");
        VersionsDir = Path.Combine(Root, "versions");
        InstancesDir = Path.Combine(Root, "instances");
        RuntimesDir = Path.Combine(Root, "runtimes");
        LogsDir = Path.Combine(Root, "logs");
    }

    public string Root { get; }
    public string ConfigDir { get; }
    public string CacheDir { get; }
    public string DownloadsDir { get; }
    public string TempDownloadsDir { get; }
    public string VersionsDir { get; }
    public string InstancesDir { get; }
    public string RuntimesDir { get; }
    public string LogsDir { get; }

    public string SettingsPath => Path.Combine(ConfigDir, "settings.json");
    public string InstancesPath => Path.Combine(ConfigDir, "instances.json");
    public string RuntimesPath => Path.Combine(ConfigDir, "runtimes.json");
    public string VersionsCachePath => Path.Combine(CacheDir, "versions.json");

    public void Ensure()
    {
        foreach (var path in new[]
        {
            ConfigDir,
            CacheDir,
            DownloadsDir,
            TempDownloadsDir,
            VersionsDir,
            InstancesDir,
            RuntimesDir,
            LogsDir
        })
        {
            Directory.CreateDirectory(path);
        }
    }
}

internal sealed class InstallRootPointer
{
    public string InstallRoot { get; set; } = string.Empty;
}

public static class AppPaths
{
    private const string PortableDataDirName = "MindustryLauncherData";

    public static string PortableDataDir
    {
        get
        {
            var exeDir = AppContext.BaseDirectory;
            return Path.Combine(exeDir, PortableDataDirName);
        }
    }

    public static string InstallRootPointerPath => Path.Combine(PortableDataDir, "install-root.json");

    public static string DefaultInstallRoot => Path.Combine(PortableDataDir, "data");

    public static async Task<string> LoadInstallRootAsync()
    {
        Directory.CreateDirectory(PortableDataDir);
        if (!File.Exists(InstallRootPointerPath))
        {
            return DefaultInstallRoot;
        }

        try
        {
            var pointer = await File.ReadAllTextAsync(InstallRootPointerPath);
            using var doc = JsonDocument.Parse(pointer);
            if (doc.RootElement.TryGetProperty("installRoot", out var value))
            {
                var root = value.GetString();
                if (!string.IsNullOrWhiteSpace(root))
                {
                    return root;
                }
            }
        }
        catch (Exception ex)
        {
            AppDebugLog.Warn($"安装根目录指针读取失败：{ex.Message}");
        }

        return DefaultInstallRoot;
    }

    public static async Task SaveInstallRootAsync(string root)
    {
        Directory.CreateDirectory(PortableDataDir);
        var pointer = new InstallRootPointer { InstallRoot = root };
        await File.WriteAllTextAsync(
            InstallRootPointerPath,
            JsonSerializer.Serialize(pointer, AppJsonContext.Default.InstallRootPointer));
    }
}

public static class FileSystemUtil
{
    public static async Task<T?> ReadJsonAsync<T>(string path, JsonTypeInfo<T> jsonTypeInfo)
    {
        if (!File.Exists(path))
        {
            return default;
        }

        await using var stream = File.OpenRead(path);
        return await JsonSerializer.DeserializeAsync(stream, jsonTypeInfo);
    }

    public static async Task WriteJsonAsync<T>(string path, T value, JsonTypeInfo<T> jsonTypeInfo)
    {
        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var tmp = Path.ChangeExtension(path, ".tmp");
        await using (var stream = File.Create(tmp))
        {
            await JsonSerializer.SerializeAsync(stream, value, jsonTypeInfo);
        }

        File.Move(tmp, path, true);
    }

    public static void AssertInsideRoot(string root, string target)
    {
        var rootFull = Path.GetFullPath(root);
        var targetFull = Path.GetFullPath(File.Exists(target) || Directory.Exists(target)
            ? target
            : Path.GetDirectoryName(target) ?? target);

        if (!targetFull.StartsWith(rootFull, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"拒绝操作安装目录外的路径：{targetFull}");
        }
    }

    public static void CopyDirectory(string source, string destination)
    {
        Directory.CreateDirectory(destination);
        foreach (var directory in Directory.EnumerateDirectories(source, "*", SearchOption.AllDirectories))
        {
            Directory.CreateDirectory(directory.Replace(source, destination, StringComparison.OrdinalIgnoreCase));
        }

        foreach (var file in Directory.EnumerateFiles(source, "*", SearchOption.AllDirectories))
        {
            var target = file.Replace(source, destination, StringComparison.OrdinalIgnoreCase);
            Directory.CreateDirectory(Path.GetDirectoryName(target)!);
            File.Copy(file, target, true);
        }
    }

    public static void DeleteDirectoryRetry(string path)
    {
        if (!Directory.Exists(path))
        {
            return;
        }

        Exception? last = null;
        for (var i = 0; i < 10; i++)
        {
            try
            {
                Directory.Delete(path, true);
                return;
            }
            catch (Exception ex)
            {
                last = ex;
                Thread.Sleep(100);
            }
        }

        throw new IOException(last?.Message ?? "删除目录失败");
    }
}
