using System.Collections.Concurrent;
using System.Text.RegularExpressions;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public static partial class AppDebugLog
{
    private const int DefaultTailLines = 600;
    private const int MaxTailLines = 2000;
    private const long MaxLogBytes = 2 * 1024 * 1024;
    private const int MaxArchives = 4;

    private static readonly object Gate = new();
    private static bool _enabled;
    private static string _logPath = string.Empty;
    private static string? _sessionId;
    private static string? _startedAt;

    public static void Configure(string logPath, bool enabled)
    {
        lock (Gate)
        {
            _logPath = logPath;
        }

        SetEnabled(enabled);
        if (enabled)
        {
            StartSession();
        }
    }

    public static void SetEnabled(bool enabled)
    {
        lock (Gate)
        {
            _enabled = enabled;
        }
    }

    public static void StartSession()
    {
        var path = CurrentPath();
        if (string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        if (File.Exists(path) && new FileInfo(path).Length > 0)
        {
            File.Move(path, Path.Combine(Path.GetDirectoryName(path)!, $"debug-{DateTime.Now:yyyyMMdd-HHmmss}.log"), true);
        }

        File.WriteAllText(path, string.Empty);
        lock (Gate)
        {
            _sessionId = DateTime.Now.ToString("yyyyMMdd-HHmmss");
            _startedAt = DateTimeOffset.Now.ToString("O");
        }

        Info($"调试会话已开始 session={_sessionId}", force: true);
        PruneArchives(path);
    }

    public static void Info(string message, bool force = false)
    {
        Write("INFO", message, force);
    }

    public static void Warn(string message)
    {
        Write("WARN", message, false);
    }

    public static void Error(string message)
    {
        Write("ERROR", message, true);
    }

    public static DebugLogSnapshot Snapshot(int maxLines = DefaultTailLines)
    {
        maxLines = Math.Clamp(maxLines, 50, MaxTailLines);
        var path = CurrentPath();
        var lines = new ConcurrentQueue<string>();
        var count = 0;

        if (!string.IsNullOrWhiteSpace(path) && File.Exists(path))
        {
            foreach (var line in File.ReadLines(path))
            {
                count++;
                lines.Enqueue(line);
                while (lines.Count > maxLines && lines.TryDequeue(out _))
                {
                }
            }
        }

        lock (Gate)
        {
            return new DebugLogSnapshot
            {
                Enabled = _enabled,
                LogPath = path,
                SessionId = _sessionId,
                StartedAt = _startedAt,
                LineCount = count,
                MaxLines = maxLines,
                Truncated = count > maxLines,
                Content = string.Join(Environment.NewLine, lines)
            };
        }
    }

    public static DebugLogSnapshot Clear()
    {
        var path = CurrentPath();
        if (!string.IsNullOrWhiteSpace(path))
        {
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, string.Empty);
            Info("调试日志已清空", true);
        }

        return Snapshot();
    }

    private static void Write(string level, string message, bool force)
    {
        var path = CurrentPath();
        bool enabled;
        lock (Gate)
        {
            enabled = _enabled;
        }

        if (!force && !enabled || string.IsNullOrWhiteSpace(path))
        {
            return;
        }

        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        RotateIfLarge(path);
        var safeMessage = SensitiveQueryRegex().Replace(
            UrlCredentialRegex().Replace(message, "://$1:***@"),
            "$1=***");
        File.AppendAllText(path, $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss.fff}] [{level}] [pid:{Environment.ProcessId}] {safeMessage}{Environment.NewLine}");
    }

    private static string CurrentPath()
    {
        lock (Gate)
        {
            return _logPath;
        }
    }

    private static void RotateIfLarge(string path)
    {
        if (!File.Exists(path) || new FileInfo(path).Length <= MaxLogBytes)
        {
            return;
        }

        File.Move(path, Path.Combine(Path.GetDirectoryName(path)!, $"debug-{DateTime.Now:yyyyMMdd-HHmmss}.log"), true);
        PruneArchives(path);
    }

    private static void PruneArchives(string path)
    {
        var dir = Path.GetDirectoryName(path);
        if (string.IsNullOrWhiteSpace(dir) || !Directory.Exists(dir))
        {
            return;
        }

        var archives = Directory
            .EnumerateFiles(dir, "debug-*.log")
            .Select(file => new FileInfo(file))
            .OrderByDescending(file => file.LastWriteTimeUtc)
            .Skip(MaxArchives);

        foreach (var file in archives)
        {
            try
            {
                file.Delete();
            }
            catch
            {
            }
        }
    }

    [GeneratedRegex(@"://([^:/@\s]+):([^/@\s]+)@")]
    private static partial Regex UrlCredentialRegex();

    [GeneratedRegex(@"(?i)\b(token|password|passwd|secret|access_token)=([^\s&]+)")]
    private static partial Regex SensitiveQueryRegex();
}
