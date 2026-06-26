using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using System.Text.Json.Serialization;

namespace MindustryLauncher.Models;

public enum ThemePreference
{
    System,
    Light,
    Dark
}

public enum GameChannel
{
    Mindustry,
    MindustryX,
    MindustryBE,
    MindustryXBE
}

public enum RuntimeSource
{
    Launcher,
    Imported,
    Scanned,
    System,
    Unknown
}

public sealed class ChannelVisibility
{
    public bool Mindustry { get; set; } = true;
    public bool MindustryX { get; set; }
    public bool MindustryBe { get; set; }
    public bool MindustryXbe { get; set; }

    public bool IsVisible(GameChannel channel, bool showBe)
    {
        return channel switch
        {
            GameChannel.Mindustry => Mindustry,
            GameChannel.MindustryX => MindustryX,
            GameChannel.MindustryBE => showBe && MindustryBe,
            GameChannel.MindustryXBE => showBe && MindustryXbe,
            _ => false
        };
    }

    public void SelectOnly(GameChannel channel)
    {
        Mindustry = channel == GameChannel.Mindustry;
        MindustryX = channel == GameChannel.MindustryX;
        MindustryBe = channel == GameChannel.MindustryBE;
        MindustryXbe = channel == GameChannel.MindustryXBE;
    }
}

public sealed class Settings
{
    public string InstallRoot { get; set; } = string.Empty;
    public bool ShowBe { get; set; }
    public string? GithubProxyPrefix { get; set; }
    public string? HttpProxy { get; set; }
    public string? SelectedAcceleratorId { get; set; } = "hubproxy-kabaka";
    public ChannelVisibility ChannelVisibility { get; set; } = new();
    public bool RuntimePromptDismissed { get; set; }
    public bool DebugMode { get; set; }
    public List<string> IgnoredVersions { get; set; } = [];
    public ThemePreference Theme { get; set; } = ThemePreference.System;
}

public sealed class AcceleratorSupports
{
    public bool Api { get; set; }
    public bool Raw { get; set; }
    public bool ReleaseAsset { get; set; }
}

public sealed class AcceleratorRule
{
    public string From { get; set; } = string.Empty;
    public string To { get; set; } = string.Empty;
}

public sealed class Accelerator
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public string BaseUrl { get; set; } = string.Empty;
    public List<AcceleratorRule> Rules { get; set; } = [];
    public AcceleratorSupports Supports { get; set; } = new();
    public string? HealthCheckUrl { get; set; }
    public bool EnabledByDefault { get; set; }

    [JsonIgnore]
    public string DisplayName => EnabledByDefault ? $"{Name} · 默认" : Name;
}

public sealed class AcceleratorList
{
    public int Version { get; set; } = 1;
    public string UpdatedAt { get; set; } = "built-in";
    public List<Accelerator> Sources { get; set; } = [];
}

public sealed class ReleaseAsset
{
    public string Name { get; set; } = string.Empty;
    public ulong Size { get; set; }
    public string DownloadUrl { get; set; } = string.Empty;
    public string? Digest { get; set; }

    [JsonIgnore]
    public string SizeDisplay => FormatBytes(Size);

    private static string FormatBytes(ulong value)
    {
        if (value == 0)
        {
            return "未知大小";
        }

        string[] units = ["B", "KB", "MB", "GB"];
        var size = (double)value;
        var unit = 0;
        while (size >= 1024 && unit < units.Length - 1)
        {
            size /= 1024;
            unit++;
        }

        return $"{size:0.#} {units[unit]}";
    }
}

public sealed class RemoteVersion
{
    public string Id { get; set; } = string.Empty;
    public GameChannel Channel { get; set; }
    public string ChannelLabel { get; set; } = string.Empty;
    public string Version { get; set; } = string.Empty;
    public string Tag { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public bool Prerelease { get; set; }
    public string? PublishedAt { get; set; }
    public List<ReleaseAsset> Assets { get; set; } = [];
    public ReleaseAsset? SelectedAsset { get; set; }
    public bool Installed { get; set; }

    [JsonIgnore]
    public string DisplayName => string.IsNullOrWhiteSpace(Name) ? $"{ChannelDisplayName} {Tag}" : Name;

    [JsonIgnore]
    public string ChannelDisplayName => Channel.ToDisplayName();

    [JsonIgnore]
    public string PublishedDisplay => DateTimeOffset.TryParse(PublishedAt, out var value)
        ? value.ToLocalTime().ToString("yyyy-MM-dd")
        : "未标注日期";

    [JsonIgnore]
    public string ChannelImagePath => Channel.ToImagePath();

    [JsonIgnore]
    public string AssetDisplay => SelectedAsset is null ? "未找到桌面 jar" : SelectedAsset.SizeDisplay;

    [JsonIgnore]
    public string SummaryDisplay => $"{ChannelDisplayName} · {PublishedDisplay} · {AssetDisplay}";

    [JsonIgnore]
    public bool CanInstall => !Installed;

    [JsonIgnore]
    public string PrimaryActionLabel => Installed ? "已安装" : "安装";
}

public sealed class RemoteRuntime
{
    public string Id { get; set; } = string.Empty;
    public ushort JavaVersion { get; set; }
    public string Version { get; set; } = string.Empty;
    public string Os { get; set; } = string.Empty;
    public string Arch { get; set; } = string.Empty;
    public string FileName { get; set; } = string.Empty;
    public string SizeLabel { get; set; } = string.Empty;
    public ulong? SizeBytes { get; set; }
    public string UpdatedAt { get; set; } = string.Empty;
    public string DownloadUrl { get; set; } = string.Empty;
    public string? Checksum { get; set; }

    [JsonIgnore]
    public string DisplayName => $"JRE {JavaVersion} · {Version} · {SizeLabel}";
}

public sealed class InstalledInstance
{
    public string Id { get; set; } = string.Empty;
    public GameChannel Channel { get; set; }
    public string Version { get; set; } = string.Empty;
    public string InstallDir { get; set; } = string.Empty;
    public string DataDir { get; set; } = string.Empty;
    public string JarPath { get; set; } = string.Empty;
    public string? RuntimeId { get; set; }
    public string InstalledAt { get; set; } = string.Empty;
    public LaunchSettings LaunchSettings { get; set; } = new();

    [JsonIgnore]
    public string ChannelDisplayName => Channel.ToDisplayName();

    [JsonIgnore]
    public string ChannelImagePath => Channel.ToImagePath();

    [JsonIgnore]
    public string InstalledDisplay => DateTimeOffset.TryParse(InstalledAt, out var value)
        ? value.ToLocalTime().ToString("yyyy-MM-dd HH:mm")
        : "未知时间";
}

public sealed class LaunchSettings
{
    public uint? MinMemoryMb { get; set; }
    public uint? MaxMemoryMb { get; set; }
    public string ExtraJvmArgs { get; set; } = string.Empty;
    public string GameArgs { get; set; } = string.Empty;
}

public sealed class RuntimeInfo
{
    public string Id { get; set; } = string.Empty;
    public ushort JavaVersion { get; set; }
    public string? Version { get; set; }
    public string Os { get; set; } = string.Empty;
    public string Arch { get; set; } = string.Empty;
    public string Path { get; set; } = string.Empty;
    public string JavaPath { get; set; } = string.Empty;
    public bool Installed { get; set; }
    public bool Enabled { get; set; } = true;
    public RuntimeSource Source { get; set; } = RuntimeSource.Unknown;

    [JsonIgnore]
    public string DisplayName => $"JRE {JavaVersion}{(string.IsNullOrWhiteSpace(Version) ? string.Empty : $" · {Version}")}";

    [JsonIgnore]
    public string SourceDisplayName => Source switch
    {
        RuntimeSource.Launcher => "启动器",
        RuntimeSource.Imported => "导入",
        RuntimeSource.Scanned => "检索",
        RuntimeSource.System => "系统",
        _ => "本地"
    };

    [JsonIgnore]
    public bool CanDelete => Source == RuntimeSource.Launcher;
}

public sealed class LaunchResult
{
    public uint Pid { get; set; }
    public string LogPath { get; set; } = string.Empty;
}

public sealed class MigrationResult
{
    public string OldRoot { get; set; } = string.Empty;
    public string NewRoot { get; set; } = string.Empty;
    public bool Copied { get; set; }
}

public sealed class DebugLogSnapshot
{
    public bool Enabled { get; set; }
    public string LogPath { get; set; } = string.Empty;
    public string? SessionId { get; set; }
    public string? StartedAt { get; set; }
    public int LineCount { get; set; }
    public int MaxLines { get; set; }
    public bool Truncated { get; set; }
    public string Content { get; set; } = string.Empty;
}

public sealed class LauncherUpdateInfo
{
    public string CurrentVersion { get; set; } = string.Empty;
    public string LatestVersion { get; set; } = string.Empty;
    public bool HasUpdate { get; set; }
    public string ReleaseUrl { get; set; } = string.Empty;
    public string ReleaseBody { get; set; } = string.Empty;
    public string? ErrorMessage { get; set; }
}

public sealed class AppUiState
{
    public Settings Settings { get; set; } = new();
    public AcceleratorList Accelerators { get; set; } = new();
    public List<RemoteVersion> Versions { get; set; } = [];
    public List<InstalledInstance> Instances { get; set; } = [];
    public List<RuntimeInfo> Runtimes { get; set; } = [];
}

public sealed class TaskRecord : ObservableObject
{
    private ulong _downloadedBytes;
    private ulong? _totalBytes;
    private ulong? _bytesPerSecond;
    private string _status = "running";
    private string _message = string.Empty;

    public string Id { get; set; } = string.Empty;
    public string Label { get; set; } = string.Empty;

    public ulong DownloadedBytes
    {
        get => _downloadedBytes;
        set
        {
            if (SetProperty(ref _downloadedBytes, value))
            {
                OnPropertyChanged(nameof(Progress));
                OnPropertyChanged(nameof(ProgressText));
            }
        }
    }

    public ulong? TotalBytes
    {
        get => _totalBytes;
        set
        {
            if (SetProperty(ref _totalBytes, value))
            {
                OnPropertyChanged(nameof(Progress));
                OnPropertyChanged(nameof(ProgressText));
                OnPropertyChanged(nameof(HasKnownTotal));
            }
        }
    }

    public ulong? BytesPerSecond
    {
        get => _bytesPerSecond;
        set
        {
            if (SetProperty(ref _bytesPerSecond, value))
            {
                OnPropertyChanged(nameof(SpeedText));
            }
        }
    }

    public string Status
    {
        get => _status;
        set
        {
            if (SetProperty(ref _status, value))
            {
                OnPropertyChanged(nameof(Progress));
                OnPropertyChanged(nameof(ProgressText));
            }
        }
    }

    public string Message
    {
        get => _message;
        set => SetProperty(ref _message, value);
    }

    public double Progress => TotalBytes is > 0
        ? Math.Clamp((double)DownloadedBytes / TotalBytes.Value * 100.0, 0.0, 100.0)
        : Status == "finished"
            ? 100.0
            : 0.0;

    public bool HasKnownTotal => TotalBytes is > 0;

    public string ProgressText => TotalBytes is > 0
        ? $"{Progress:0}% · {FormatBytes(DownloadedBytes)} / {FormatBytes(TotalBytes.Value)}"
        : Status == "finished"
            ? "完成"
            : FormatBytes(DownloadedBytes);

    public string SpeedText => BytesPerSecond is > 0 ? $"{FormatBytes(BytesPerSecond.Value)}/s" : "-";

    public static string FormatBytes(ulong value)
    {
        if (value == 0)
        {
            return "0 B";
        }

        string[] units = ["B", "KB", "MB", "GB"];
        var size = (double)value;
        var unit = 0;
        while (size >= 1024 && unit < units.Length - 1)
        {
            size /= 1024;
            unit++;
        }

        return $"{size:0.#} {units[unit]}";
    }
}

public sealed class DashboardMetric
{
    public string Label { get; set; } = string.Empty;
    public string Value { get; set; } = string.Empty;
    public string Glyph { get; set; } = "\uE946";
}

public class ObservableObject : INotifyPropertyChanged
{
    public event PropertyChangedEventHandler? PropertyChanged;

    protected bool SetProperty<T>(ref T storage, T value, [CallerMemberName] string propertyName = "")
    {
        if (EqualityComparer<T>.Default.Equals(storage, value))
        {
            return false;
        }

        storage = value;
        OnPropertyChanged(propertyName);
        return true;
    }

    protected void OnPropertyChanged([CallerMemberName] string propertyName = "")
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}

public static class LauncherModelExtensions
{
    public static string ToWireValue(this GameChannel channel)
    {
        return channel switch
        {
            GameChannel.Mindustry => "mindustry",
            GameChannel.MindustryX => "mindustryX",
            GameChannel.MindustryBE => "mindustryBE",
            GameChannel.MindustryXBE => "mindustryXBE",
            _ => "mindustry"
        };
    }

    public static string ToDisplayName(this GameChannel channel)
    {
        return channel switch
        {
            GameChannel.Mindustry => "Mindustry",
            GameChannel.MindustryX => "MindustryX",
            GameChannel.MindustryBE => "Mindustry BE",
            GameChannel.MindustryXBE => "MindustryX BE",
            _ => "Mindustry"
        };
    }

    public static string ToImagePath(this GameChannel channel)
    {
        return channel switch
        {
            GameChannel.Mindustry => "ms-appx:///Assets/Mindustry/core-shard.png",
            GameChannel.MindustryX => "ms-appx:///Assets/Mindustry/zenith.png",
            GameChannel.MindustryBE => "ms-appx:///Assets/Mindustry/lancer.png",
            GameChannel.MindustryXBE => "ms-appx:///Assets/Mindustry/alpha.png",
            _ => "ms-appx:///Assets/Mindustry/core-shard.png"
        };
    }

    public static bool TryParseGameChannel(string? value, out GameChannel channel)
    {
        channel = value switch
        {
            "mindustry" => GameChannel.Mindustry,
            "mindustryX" => GameChannel.MindustryX,
            "mindustryBE" => GameChannel.MindustryBE,
            "mindustryXBE" => GameChannel.MindustryXBE,
            _ => GameChannel.Mindustry
        };

        return value is "mindustry" or "mindustryX" or "mindustryBE" or "mindustryXBE";
    }

    public static string ToWireValue(this RuntimeSource source)
    {
        return source switch
        {
            RuntimeSource.Launcher => "launcher",
            RuntimeSource.Imported => "imported",
            RuntimeSource.Scanned => "scanned",
            RuntimeSource.System => "system",
            _ => "unknown"
        };
    }

    public static bool TryParseRuntimeSource(string? value, out RuntimeSource source)
    {
        source = value switch
        {
            "launcher" => RuntimeSource.Launcher,
            "imported" => RuntimeSource.Imported,
            "scanned" => RuntimeSource.Scanned,
            "system" => RuntimeSource.System,
            _ => RuntimeSource.Unknown
        };

        return value is "launcher" or "imported" or "scanned" or "system" or "unknown";
    }
}
