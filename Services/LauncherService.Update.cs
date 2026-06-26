using System.Text.RegularExpressions;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    private const string LauncherOwner = "kabaka9527";
    private const string LauncherRepo = "MindustryLauncher";

    public async Task<LauncherUpdateInfo> CheckLauncherUpdateAsync()
    {
        var current = "0.1.1";
        var layout = Layout;
        var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
        var api = $"https://api.github.com/repos/{LauncherOwner}/{LauncherRepo}/releases/latest";

        foreach (var candidate in GithubUrlCandidates(api))
        {
            try
            {
                var release = await network.GetJsonAsync<GitHubRelease>(candidate);
                var latest = release.TagName.Trim().TrimStart('v');
                return new LauncherUpdateInfo
                {
                    CurrentVersion = current,
                    LatestVersion = latest,
                    HasUpdate = VersionIsNewer(latest, current),
                    ReleaseUrl = release.HtmlUrl,
                    ReleaseBody = release.Body ?? string.Empty
                };
            }
            catch (Exception ex)
            {
                AppDebugLog.Warn($"启动器更新检测失败 ({candidate})：{ex.Message}");
            }
        }

        try
        {
            var atom = await GetFirstTextAsync(
                network,
                GithubUrlCandidates($"https://github.com/{LauncherOwner}/{LauncherRepo}/releases.atom").ToList());
            var latest = ParseLatestReleaseFromAtom(atom);
            if (!string.IsNullOrWhiteSpace(latest))
            {
                var normalized = latest.TrimStart('v');
                return new LauncherUpdateInfo
                {
                    CurrentVersion = current,
                    LatestVersion = normalized,
                    HasUpdate = VersionIsNewer(normalized, current),
                    ReleaseUrl = $"https://github.com/{LauncherOwner}/{LauncherRepo}/releases/tag/{latest}",
                    ReleaseBody = string.Empty
                };
            }
        }
        catch (Exception ex)
        {
            AppDebugLog.Warn($"Atom 更新检测失败：{ex.Message}");
        }

        return new LauncherUpdateInfo
        {
            CurrentVersion = current,
            LatestVersion = current,
            ErrorMessage = "更新检测失败：无法获取版本信息"
        };
    }

    public async Task<Settings> IgnoreLauncherVersionAsync(string version)
    {
        if (!_settings.IgnoredVersions.Contains(version))
        {
            _settings.IgnoredVersions.Add(version);
            await SaveSettingsAsync(_settings);
        }

        return _settings;
    }

    private static bool VersionIsNewer(string latest, string current)
    {
        return ParseSemVer(latest) is { } latestVersion
            && ParseSemVer(current) is { } currentVersion
            && latestVersion.CompareTo(currentVersion) > 0;
    }

    private static Version? ParseSemVer(string value)
    {
        var normalized = value.Trim().TrimStart('v');
        var match = Regex.Match(normalized, "^(\\d+)\\.(\\d+)\\.(\\d+)");
        return match.Success
            ? new Version(
                int.Parse(match.Groups[1].Value),
                int.Parse(match.Groups[2].Value),
                int.Parse(match.Groups[3].Value))
            : null;
    }

    private static string? ParseLatestReleaseFromAtom(string atom)
    {
        var tagRegex = new Regex($"https://github\\.com/{LauncherOwner}/{LauncherRepo}/releases/tag/([^\"<]+)", RegexOptions.IgnoreCase);
        return Regex.Matches(atom, "(?s)<entry>(.*?)</entry>")
            .Select(match => tagRegex.Match(match.Groups[1].Value))
            .Where(match => match.Success)
            .Select(match => WebDecode(match.Groups[1].Value))
            .OrderByDescending(tag => ParseSemVer(tag) ?? new Version(0, 0, 0))
            .FirstOrDefault();
    }
}
