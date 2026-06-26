using System.Diagnostics;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.RegularExpressions;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    private const string TunaAdoptiumRoot = "https://mirrors.tuna.tsinghua.edu.cn/Adoptium";

    public async Task<List<RemoteRuntime>> ListRemoteRuntimesAsync()
    {
        var layout = Layout;
        var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
        var root = await network.GetTextCachedAsync($"{TunaAdoptiumRoot}/");
        var versions = ParseTunaJavaVersions(root);
        var os = AdoptiumOs();
        var arch = AdoptiumArch();
        var result = new List<RemoteRuntime>();

        foreach (var version in versions)
        {
            var url = $"{TunaAdoptiumRoot}/{version}/jre/{arch}/{os}/";
            try
            {
                var body = await network.GetTextCachedAsync(url);
                result.AddRange(ParseTunaRuntimePackages(version, os, arch, url, body));
            }
            catch (Exception ex)
            {
                AppDebugLog.Warn($"JRE {version} 列表读取失败：{ex.Message}");
            }
        }

        return result.OrderByDescending(runtime => runtime.JavaVersion).ToList();
    }

    public async Task<RuntimeInfo> EnsureRuntimeAsync(ushort? javaVersion = null)
    {
        var required = javaVersion ?? 17;
        var layout = Layout;
        var runtimes = await LoadRuntimesAsync(layout);
        var preferred = runtimes.FirstOrDefault(runtime =>
            runtime.Enabled
            && runtime.Id == $"jre-{required}-{AdoptiumOs()}-{AdoptiumArch()}"
            && File.Exists(runtime.JavaPath));
        if (preferred is not null)
        {
            return preferred;
        }

        var compatible = runtimes
            .Where(runtime => runtime.Enabled && runtime.JavaVersion >= required && File.Exists(runtime.JavaPath))
            .OrderBy(runtime => runtime.JavaVersion)
            .FirstOrDefault();
        if (compatible is not null)
        {
            return compatible;
        }

        var remote = (await ListRemoteRuntimesAsync())
            .Where(runtime => runtime.JavaVersion >= required)
            .OrderBy(runtime => runtime.JavaVersion)
            .ThenByDescending(runtime => runtime.UpdatedAt)
            .FirstOrDefault();
        if (remote is null)
        {
            throw new InvalidOperationException($"未找到 JRE {required} 或更高版本");
        }

        return await InstallRuntimeAsync(remote);
    }

    public async Task<RuntimeInfo> InstallRuntimeAsync(RemoteRuntime runtime)
    {
        var layout = Layout;
        layout.Ensure();
        var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
        var checksum = runtime.Checksum;
        if (string.IsNullOrWhiteSpace(checksum))
        {
            checksum = await ResolveJreChecksumAsync(network, runtime.JavaVersion, runtime.Os, runtime.Arch, runtime.FileName);
        }

        return await InstallJrePackageAsync(layout, network, new JrePackage(
            runtime.JavaVersion,
            runtime.Version,
            runtime.Os,
            runtime.Arch,
            runtime.FileName,
            checksum,
            runtime.DownloadUrl,
            runtime.SizeBytes));
    }

    public async Task<RuntimeInfo> ImportRuntimeAsync(string source)
    {
        var layout = Layout;
        layout.Ensure();
        if (string.IsNullOrWhiteSpace(source))
        {
            throw new InvalidOperationException("运行时路径不能为空");
        }

        var javaPath = File.Exists(source)
            ? source
            : FindJavaBinaryLimited(source, 5) ?? throw new FileNotFoundException($"未找到 Java 可执行文件：{source}");
        var runtimeRoot = RuntimeRootFromJava(javaPath);
        var details = DetectJavaDetails(javaPath);
        var runtimeId = $"imported-jre-{details.JavaVersion}-{DateTime.Now:yyyyMMddHHmmss}";
        var destination = runtimeRoot.StartsWith(layout.RuntimesDir, StringComparison.OrdinalIgnoreCase)
            ? runtimeRoot
            : Path.Combine(layout.RuntimesDir, runtimeId);

        if (!destination.Equals(runtimeRoot, StringComparison.OrdinalIgnoreCase))
        {
            FileSystemUtil.CopyDirectory(runtimeRoot, destination);
            javaPath = Path.Combine(destination, Path.GetRelativePath(runtimeRoot, javaPath));
        }

        var runtime = new RuntimeInfo
        {
            Id = runtimeId,
            JavaVersion = details.JavaVersion,
            Version = details.Version,
            Os = AdoptiumOs(),
            Arch = AdoptiumArch(),
            Path = destination,
            JavaPath = javaPath,
            Installed = true,
            Enabled = true,
            Source = RuntimeSource.Imported
        };

        var runtimes = await LoadRuntimesAsync(layout);
        runtimes.RemoveAll(item => item.JavaPath.Equals(runtime.JavaPath, StringComparison.OrdinalIgnoreCase));
        runtimes.Add(runtime);
        await SaveRuntimesAsync(layout, runtimes);
        return runtime;
    }

    public async Task<List<RuntimeInfo>> ScanRuntimesAsync(string source)
    {
        if (string.IsNullOrWhiteSpace(source) || !Directory.Exists(source) && !File.Exists(source))
        {
            throw new InvalidOperationException("请选择有效的 Java 搜索路径");
        }

        var candidates = File.Exists(source)
            ? (IsJavaBinary(source) ? [source] : new List<string>())
            : FindJavaBinariesLimited(source, 7, 32);
        return await RegisterRuntimeCandidatesAsync(candidates, RuntimeSource.Scanned);
    }

    public async Task<List<RuntimeInfo>> ScanSystemRuntimesAsync()
    {
        var candidates = new List<string>();
        foreach (var variable in new[] { "JAVA_HOME", "JRE_HOME" })
        {
            var value = Environment.GetEnvironmentVariable(variable);
            if (!string.IsNullOrWhiteSpace(value))
            {
                candidates.Add(Path.Combine(value, "bin", JavaBinaryName()));
                candidates.Add(Path.Combine(value, JavaBinaryName()));
            }
        }

        var path = Environment.GetEnvironmentVariable("PATH");
        if (!string.IsNullOrWhiteSpace(path))
        {
            candidates.AddRange(path.Split(Path.PathSeparator)
                .Where(item => !string.IsNullOrWhiteSpace(item))
                .Select(item => Path.Combine(item, JavaBinaryName())));
        }

        return await RegisterRuntimeCandidatesAsync(candidates, RuntimeSource.System);
    }

    public async Task<List<RuntimeInfo>> SetRuntimeEnabledAsync(string runtimeId, bool enabled)
    {
        var layout = Layout;
        var runtimes = await LoadRuntimesAsync(layout);
        var runtime = runtimes.FirstOrDefault(item => item.Id == runtimeId)
            ?? throw new InvalidOperationException($"未找到运行时：{runtimeId}");
        runtime.Enabled = enabled;
        await SaveRuntimesAsync(layout, runtimes);
        return runtimes;
    }

    public async Task<List<RuntimeInfo>> DeleteRuntimeAsync(string runtimeId)
    {
        var layout = Layout;
        var runtimes = await LoadRuntimesAsync(layout);
        var runtime = runtimes.FirstOrDefault(item => item.Id == runtimeId)
            ?? throw new InvalidOperationException($"未找到运行时：{runtimeId}");
        if (runtime.Source != RuntimeSource.Launcher)
        {
            throw new InvalidOperationException("只能删除启动器下载的运行时");
        }

        FileSystemUtil.AssertInsideRoot(layout.Root, runtime.Path);
        FileSystemUtil.DeleteDirectoryRetry(runtime.Path);
        runtimes.RemoveAll(item => item.Id == runtimeId);
        await SaveRuntimesAsync(layout, runtimes);

        var instances = await LoadInstancesAsync(layout);
        foreach (var instance in instances.Where(instance => instance.RuntimeId == runtimeId))
        {
            instance.RuntimeId = null;
        }

        await SaveInstancesAsync(layout, instances);
        return runtimes;
    }

    public static ushort RequiredJavaFromJar(string path)
    {
        using var archive = ZipFile.OpenRead(path);
        ushort? maxMajor = null;
        var header = new byte[8];
        foreach (var entry in archive.Entries.Where(entry => entry.FullName.EndsWith(".class", StringComparison.OrdinalIgnoreCase)))
        {
            using var stream = entry.Open();
            Array.Clear(header);
            if (stream.Read(header, 0, header.Length) == 8
                && header[0] == 0xCA
                && header[1] == 0xFE
                && header[2] == 0xBA
                && header[3] == 0xBE)
            {
                var major = (ushort)((header[6] << 8) + header[7]);
                maxMajor = maxMajor is null ? major : Math.Max(maxMajor.Value, major);
            }
        }

        return maxMajor is null ? (ushort)17 : JavaFeatureFromClassMajor(maxMajor.Value);
    }

    private async Task<RuntimeInfo> InstallJrePackageAsync(InstallLayout layout, NetworkClient network, JrePackage package)
    {
        var runtimeId = $"jre-{package.JavaVersion}-{package.Os}-{package.Arch}";
        var runtimeDir = Path.Combine(layout.RuntimesDir, runtimeId);
        var archivePath = Path.Combine(layout.TempDownloadsDir, SafePathPart(package.FileName));
        var taskId = $"runtime:{runtimeId}";

        Directory.CreateDirectory(layout.TempDownloadsDir);
        if (Directory.Exists(runtimeDir))
        {
            FileSystemUtil.DeleteDirectoryRetry(runtimeDir);
        }

        Directory.CreateDirectory(runtimeDir);

        try
        {
            await network.DownloadToFileAsync(
                package.TunaUrl,
                archivePath,
                package.Checksum,
                package.SizeBytes,
                taskId,
                $"下载 JRE {package.JavaVersion}",
                OnTask);

            OnTask(new TaskRecord { Id = taskId, Label = $"解压 JRE {package.JavaVersion}", Message = "解压中" });
            ZipFile.ExtractToDirectory(archivePath, runtimeDir, true);
            var javaPath = FindJavaBinaryLimited(runtimeDir, 12)
                ?? throw new FileNotFoundException($"解压后未找到 Java：{archivePath}");

            var runtime = new RuntimeInfo
            {
                Id = runtimeId,
                JavaVersion = package.JavaVersion,
                Version = package.Version,
                Os = package.Os,
                Arch = package.Arch,
                Path = runtimeDir,
                JavaPath = javaPath,
                Installed = true,
                Enabled = true,
                Source = RuntimeSource.Launcher
            };

            var runtimes = await LoadRuntimesAsync(layout);
            runtimes.RemoveAll(item => item.Id == runtimeId);
            runtimes.Add(runtime);
            await SaveRuntimesAsync(layout, runtimes);
            return runtime;
        }
        catch
        {
            FileSystemUtil.DeleteDirectoryRetry(runtimeDir);
            throw;
        }
        finally
        {
            TryDeleteFile(archivePath);
        }
    }

    private async Task<List<RuntimeInfo>> RegisterRuntimeCandidatesAsync(IEnumerable<string> candidates, RuntimeSource source)
    {
        var layout = Layout;
        layout.Ensure();
        var runtimes = await LoadRuntimesAsync(layout);
        var seen = runtimes.Select(runtime => runtime.JavaPath).ToHashSet(StringComparer.OrdinalIgnoreCase);
        var found = new List<RuntimeInfo>();

        foreach (var candidate in candidates.Distinct(StringComparer.OrdinalIgnoreCase))
        {
            if (!File.Exists(candidate) || !IsJavaBinary(candidate))
            {
                continue;
            }

            var javaPath = Path.GetFullPath(candidate);
            if (seen.Contains(javaPath))
            {
                var existing = runtimes.FirstOrDefault(runtime => runtime.JavaPath.Equals(javaPath, StringComparison.OrdinalIgnoreCase));
                if (existing is not null)
                {
                    found.Add(existing);
                }

                continue;
            }

            try
            {
                var details = DetectJavaDetails(javaPath);
                var root = RuntimeRootFromJava(javaPath);
                var runtime = new RuntimeInfo
                {
                    Id = RuntimeIdForSource(source, details.JavaVersion, root),
                    JavaVersion = details.JavaVersion,
                    Version = details.Version,
                    Os = AdoptiumOs(),
                    Arch = AdoptiumArch(),
                    Path = root,
                    JavaPath = javaPath,
                    Installed = true,
                    Enabled = true,
                    Source = source
                };
                runtimes.Add(runtime);
                found.Add(runtime);
                seen.Add(javaPath);
            }
            catch (Exception ex)
            {
                AppDebugLog.Warn($"Java 运行时检测失败 ({candidate})：{ex.Message}");
            }
        }

        if (found.Count > 0)
        {
            await SaveRuntimesAsync(layout, runtimes);
        }

        return found;
    }

    private async Task<string> ResolveJreChecksumAsync(NetworkClient network, ushort javaVersion, string os, string arch, string fileName)
    {
        var api = $"https://api.adoptium.net/v3/assets/latest/{javaVersion}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse&heap_size=normal";
        using var doc = JsonDocument.Parse(await network.GetTextCachedAsync(api));
        foreach (var item in doc.RootElement.EnumerateArray())
        {
            if (!item.TryGetProperty("binary", out var binary)
                || !binary.TryGetProperty("package", out var package))
            {
                continue;
            }

            if (package.TryGetProperty("name", out var name)
                && name.GetString() == fileName
                && package.TryGetProperty("checksum", out var checksum))
            {
                return checksum.GetString() ?? string.Empty;
            }
        }

        throw new InvalidOperationException($"未找到 {fileName} 的校验和");
    }

    private static List<ushort> ParseTunaJavaVersions(string body)
    {
        return Regex.Matches(body, "<a href=\"(\\d+)/\" title=\"\\d+\">\\d+/</a>")
            .Select(match => ushort.TryParse(match.Groups[1].Value, out var value) ? value : (ushort)0)
            .Where(value => value > 0)
            .Distinct()
            .OrderBy(value => value)
            .ToList();
    }

    private static List<RemoteRuntime> ParseTunaRuntimePackages(ushort javaVersion, string os, string arch, string baseUrl, string body)
    {
        var sizeRegex = new Regex("<td class=\"size\">([^<]*)</td>");
        var dateRegex = new Regex("<td class=\"date\">([^<]*)</td>");
        var result = new List<RemoteRuntime>();
        foreach (var chunk in body.Split("href=\"").Skip(1))
        {
            var end = chunk.IndexOf('"');
            if (end <= 0)
            {
                continue;
            }

            var href = WebDecode(chunk[..end]);
            if (!href.EndsWith(".zip", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            var fileName = href.Split('/').LastOrDefault() ?? href;
            if (!fileName.Contains("-jre_", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            var tail = chunk[end..];
            var version = PackageVersionFromFileName(fileName) ?? javaVersion.ToString();
            var sizeLabel = sizeRegex.Match(tail).Groups[1].Value;
            result.Add(new RemoteRuntime
            {
                Id = $"jre-{javaVersion}-{os}-{arch}-{SafePathPart(version)}",
                JavaVersion = javaVersion,
                Version = version,
                Os = os,
                Arch = arch,
                FileName = fileName,
                SizeLabel = WebDecode(sizeLabel),
                SizeBytes = ParseSizeLabel(sizeLabel),
                UpdatedAt = WebDecode(dateRegex.Match(tail).Groups[1].Value),
                DownloadUrl = $"{baseUrl.TrimEnd('/')}/{href}",
            });
        }

        return result;
    }

    private static string? PackageVersionFromFileName(string fileName)
    {
        var value = fileName.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)
            ? fileName[..^4]
            : fileName;
        var marker = value.IndexOf("_hotspot_", StringComparison.OrdinalIgnoreCase);
        return marker > 0 ? value[..marker] : null;
    }

    private static ulong? ParseSizeLabel(string value)
    {
        var parts = value.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2 || !double.TryParse(parts[0], out var amount))
        {
            return null;
        }

        var multiplier = parts[1].ToLowerInvariant() switch
        {
            "b" => 1d,
            "kb" or "kib" => 1024d,
            "mb" or "mib" => 1024d * 1024d,
            "gb" or "gib" => 1024d * 1024d * 1024d,
            _ => 0d
        };

        return multiplier <= 0 ? null : (ulong)(amount * multiplier);
    }

    private static JavaRuntimeDetails DetectJavaDetails(string javaPath)
    {
        var process = Process.Start(new ProcessStartInfo(javaPath, "-version")
        {
            RedirectStandardError = true,
            RedirectStandardOutput = true,
            UseShellExecute = false,
            CreateNoWindow = true
        }) ?? throw new InvalidOperationException($"无法启动 {javaPath}");
        var text = process.StandardOutput.ReadToEnd() + Environment.NewLine + process.StandardError.ReadToEnd();
        process.WaitForExit(5000);

        var match = Regex.Match(text, "(?:openjdk|java) version \"([^\"]+)\"", RegexOptions.IgnoreCase);
        if (!match.Success)
        {
            throw new InvalidOperationException($"无法解析 Java 版本：{text}");
        }

        var version = match.Groups[1].Value;
        var feature = version.StartsWith("1.", StringComparison.Ordinal)
            ? version.Split('.')[1]
            : version.Split('.')[0];
        return new JavaRuntimeDetails(ushort.Parse(feature), version);
    }

    private static string RuntimeRootFromJava(string javaPath)
    {
        var parent = Directory.GetParent(javaPath);
        if (parent is null)
        {
            throw new InvalidOperationException($"无效 Java 路径：{javaPath}");
        }

        return string.Equals(parent.Name, "bin", StringComparison.OrdinalIgnoreCase)
            ? parent.Parent?.FullName ?? parent.FullName
            : parent.FullName;
    }

    private static string? FindJavaBinaryLimited(string root, int maxDepth)
    {
        if (!Directory.Exists(root))
        {
            return null;
        }

        return FindFileLimited(root, JavaBinaryName(), maxDepth);
    }

    private static List<string> FindJavaBinariesLimited(string root, int maxDepth, int maxItems)
    {
        var result = new List<string>();
        CollectJavaBinaries(root, maxDepth, maxItems, result);
        return result;
    }

    private static void CollectJavaBinaries(string root, int depth, int maxItems, List<string> result)
    {
        if (depth <= 0 || result.Count >= maxItems)
        {
            return;
        }

        foreach (var file in Directory.EnumerateFiles(root, JavaBinaryName()))
        {
            if (string.Equals(Path.GetFileName(Path.GetDirectoryName(file)), "bin", StringComparison.OrdinalIgnoreCase))
            {
                result.Add(file);
                if (result.Count >= maxItems)
                {
                    return;
                }
            }
        }

        foreach (var directory in Directory.EnumerateDirectories(root))
        {
            CollectJavaBinaries(directory, depth - 1, maxItems, result);
            if (result.Count >= maxItems)
            {
                return;
            }
        }
    }

    private static string? FindFileLimited(string root, string fileName, int depth)
    {
        if (depth <= 0)
        {
            return null;
        }

        foreach (var file in Directory.EnumerateFiles(root, fileName))
        {
            return file;
        }

        foreach (var directory in Directory.EnumerateDirectories(root))
        {
            var found = FindFileLimited(directory, fileName, depth - 1);
            if (found is not null)
            {
                return found;
            }
        }

        return null;
    }

    private static string RuntimeIdForSource(RuntimeSource source, ushort javaVersion, string runtimeRoot)
    {
        var prefix = source switch
        {
            RuntimeSource.System => "system-jre",
            RuntimeSource.Scanned => "local-jre",
            RuntimeSource.Imported => "imported-jre",
            _ => "local-jre"
        };
        var digest = Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(runtimeRoot))).ToLowerInvariant()[..10];
        return $"{prefix}-{javaVersion}-{digest}";
    }

    private static bool IsJavaBinary(string path)
    {
        return string.Equals(Path.GetFileName(path), JavaBinaryName(), StringComparison.OrdinalIgnoreCase);
    }

    private static string JavaBinaryName() => OperatingSystem.IsWindows() ? "java.exe" : "java";

    private static string AdoptiumOs() => OperatingSystem.IsWindows() ? "windows" : OperatingSystem.IsMacOS() ? "mac" : "linux";

    private static string AdoptiumArch()
    {
        return System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture switch
        {
            System.Runtime.InteropServices.Architecture.X64 => "x64",
            System.Runtime.InteropServices.Architecture.Arm64 => "aarch64",
            System.Runtime.InteropServices.Architecture.Arm => "arm",
            _ => "x64"
        };
    }

    private static ushort JavaFeatureFromClassMajor(ushort major) => major <= 44 ? (ushort)8 : (ushort)(major - 44);

    private static void TryDeleteFile(string path)
    {
        try
        {
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch
        {
        }
    }

    private sealed record JrePackage(ushort JavaVersion, string Version, string Os, string Arch, string FileName, string Checksum, string TunaUrl, ulong? SizeBytes);

    private sealed record JavaRuntimeDetails(ushort JavaVersion, string Version);
}
