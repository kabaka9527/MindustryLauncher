using System.Text.RegularExpressions;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    public async Task<List<RemoteVersion>> RefreshVersionsAsync()
    {
        var layout = Layout;
        var network = new NetworkClient(_settings, Path.Combine(layout.CacheDir, "http"));
        var instances = await LoadInstancesAsync(layout);
        var specs = ChannelFetchSpecs().ToList();
        var tasks = specs.Select(spec => FetchChannelAsync(network, spec, instances)).ToList();
        var results = await Task.WhenAll(tasks);
        var refreshed = results.Where(result => result.Versions.Count > 0).ToList();

        if (refreshed.Count == 0)
        {
            var cached = await LoadCachedVersionsAsync(layout);
            if (cached.Count > 0)
            {
                return cached;
            }

            throw new InvalidOperationException(string.Join("; ", results.Select(result => result.Error).Where(error => !string.IsNullOrWhiteSpace(error))));
        }

        var merged = await LoadCachedVersionsAsync(layout);
        var refreshedChannels = refreshed.Select(result => result.Channel).ToHashSet();
        merged.RemoveAll(version => refreshedChannels.Contains(version.Channel));
        merged.AddRange(refreshed.SelectMany(result => result.Versions));
        await SaveCachedVersionsAsync(layout, merged);
        return merged;
    }

    private async Task<(GameChannel Channel, List<RemoteVersion> Versions, string? Error)> FetchChannelAsync(
        NetworkClient network,
        ChannelFetchSpec spec,
        List<InstalledInstance> instances)
    {
        try
        {
            var apiUrl = $"https://api.github.com/repos/{spec.Owner}/{spec.Repo}/releases?per_page=20";
            foreach (var candidate in GithubUrlCandidates(apiUrl))
            {
                try
                {
                    var releases = await network.GetJsonAsync<List<GitHubRelease>>(candidate);
                    return (spec.Channel, releases
                        .Where(release => spec.FilterMatches(release.Prerelease))
                        .Select(release => MapRelease(spec.Channel, release, instances))
                        .Where(version => version is not null)
                        .Cast<RemoteVersion>()
                        .ToList(), null);
                }
                catch (Exception ex)
                {
                    AppDebugLog.Warn($"{spec.Label} GitHub API 失败：{ex.Message}");
                }
            }

            var fallback = await FetchChannelFromReleasePagesAsync(network, spec, instances);
            return (spec.Channel, fallback, fallback.Count == 0 ? $"{spec.Label}: 未找到 jar 资产" : null);
        }
        catch (Exception ex)
        {
            return (spec.Channel, [], $"{spec.Label}: {ex.Message}");
        }
    }

    private async Task<List<RemoteVersion>> FetchChannelFromReleasePagesAsync(
        NetworkClient network,
        ChannelFetchSpec spec,
        List<InstalledInstance> instances)
    {
        var atomUrl = $"https://github.com/{spec.Owner}/{spec.Repo}/releases.atom";
        var atom = await GetFirstTextAsync(network, GithubUrlCandidates(atomUrl).ToList());
        var releases = ParseReleasesAtom(spec.Owner, spec.Repo, atom)
            .Where(release => spec.FilterMatches(release.Prerelease))
            .Take(20)
            .ToList();

        var versions = new List<RemoteVersion>();
        foreach (var release in releases)
        {
            var expandedUrl = $"https://github.com/{spec.Owner}/{spec.Repo}/releases/expanded_assets/{Uri.EscapeDataString(release.Tag)}";
            var html = await GetFirstTextAsync(network, GithubUrlCandidates(expandedUrl).ToList());
            var assets = ParseExpandedAssets(html);
            var selected = SelectDesktopJar(assets);
            if (selected is null)
            {
                continue;
            }

            var id = VersionId(spec.Channel, release.Tag);
            versions.Add(new RemoteVersion
            {
                Id = id,
                Channel = spec.Channel,
                ChannelLabel = spec.Channel.ToDisplayName(),
                Version = HumanVersion(release.Tag),
                Tag = release.Tag,
                Name = release.Name,
                Prerelease = release.Prerelease,
                PublishedAt = release.PublishedAt,
                Assets = assets,
                SelectedAsset = selected,
                Installed = instances.Any(instance => instance.Id == id)
            });
        }

        return versions;
    }

    private static async Task<string> GetFirstTextAsync(NetworkClient network, IReadOnlyList<string> urls)
    {
        Exception? last = null;
        foreach (var url in urls)
        {
            try
            {
                return await network.GetTextCachedAsync(url);
            }
            catch (Exception ex)
            {
                last = ex;
            }
        }

        throw new InvalidOperationException(last?.Message ?? "没有可用 URL");
    }

    private static RemoteVersion? MapRelease(GameChannel channel, GitHubRelease release, List<InstalledInstance> instances)
    {
        var assets = release.Assets.Select(asset => new ReleaseAsset
        {
            Name = asset.Name,
            Size = asset.Size,
            DownloadUrl = asset.BrowserDownloadUrl,
            Digest = asset.Digest
        }).ToList();
        var selected = SelectDesktopJar(assets);
        if (selected is null)
        {
            return null;
        }

        var id = VersionId(channel, release.TagName);
        return new RemoteVersion
        {
            Id = id,
            Channel = channel,
            ChannelLabel = channel.ToDisplayName(),
            Version = HumanVersion(release.TagName),
            Tag = release.TagName,
            Name = string.IsNullOrWhiteSpace(release.Name) ? $"{channel.ToDisplayName()} {release.TagName}" : release.Name,
            Prerelease = release.Prerelease,
            PublishedAt = release.PublishedAt,
            Assets = assets,
            SelectedAsset = selected,
            Installed = instances.Any(instance => instance.Id == id)
        };
    }

    private static List<AtomRelease> ParseReleasesAtom(string owner, string repo, string body)
    {
        var linkRegex = new Regex($"https://github\\.com/{Regex.Escape(owner)}/{Regex.Escape(repo)}/releases/tag/([^\"<]+)", RegexOptions.IgnoreCase);
        var entries = Regex.Matches(body, "(?s)<entry>(.*?)</entry>");
        var releases = new List<AtomRelease>();

        foreach (Match entry in entries)
        {
            var block = entry.Groups[1].Value;
            var link = linkRegex.Match(block);
            if (!link.Success)
            {
                continue;
            }

            var tag = WebDecode(link.Groups[1].Value);
            var title = Regex.Match(block, "(?s)<title>(.*?)</title>");
            var updated = Regex.Match(block, "(?s)<updated>(.*?)</updated>");
            var lower = tag.ToLowerInvariant();
            releases.Add(new AtomRelease(
                tag,
                title.Success ? WebDecode(title.Groups[1].Value) : tag,
                lower.Contains("pre") || lower.Contains("alpha") || lower.Contains("beta"),
                updated.Success ? WebDecode(updated.Groups[1].Value) : null));
        }

        return releases;
    }

    private static List<ReleaseAsset> ParseExpandedAssets(string body)
    {
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var assets = new List<ReleaseAsset>();
        foreach (Match match in Regex.Matches(body, "href=\"([^\"]+?\\.jar)\"", RegexOptions.IgnoreCase))
        {
            var href = WebDecode(match.Groups[1].Value);
            var downloadUrl = href.StartsWith("https://", StringComparison.OrdinalIgnoreCase)
                ? href
                : $"https://github.com{href}";
            if (!seen.Add(downloadUrl))
            {
                continue;
            }

            assets.Add(new ReleaseAsset
            {
                Name = downloadUrl.Split('/').LastOrDefault(value => !string.IsNullOrWhiteSpace(value)) ?? "Mindustry.jar",
                DownloadUrl = downloadUrl
            });
        }

        return assets;
    }

    private static ReleaseAsset? SelectDesktopJar(IReadOnlyList<ReleaseAsset> assets)
    {
        return assets
            .Where(asset =>
            {
                var lower = asset.Name.ToLowerInvariant();
                return lower.EndsWith(".jar")
                    && !lower.Contains("server")
                    && !lower.Contains("android")
                    && !lower.Contains("source")
                    && !lower.Contains("javadoc");
            })
            .OrderByDescending(asset => JarScore(asset.Name))
            .FirstOrDefault();
    }

    private static int JarScore(string name)
    {
        var lower = name.ToLowerInvariant();
        if (lower == "mindustry.jar")
        {
            return 100;
        }

        if (lower.Contains("desktop"))
        {
            return 90;
        }

        if (lower.Contains("mindustry"))
        {
            return 80;
        }

        return 10;
    }

    private static string VersionId(GameChannel channel, string tag) => $"{channel.ToWireValue()}:{tag}";

    private static string HumanVersion(string tag) => tag.TrimStart('v');

    private static string WebDecode(string value)
    {
        return value
            .Replace("&amp;", "&", StringComparison.Ordinal)
            .Replace("&quot;", "\"", StringComparison.Ordinal)
            .Replace("&#39;", "'", StringComparison.Ordinal)
            .Replace("&lt;", "<", StringComparison.Ordinal)
            .Replace("&gt;", ">", StringComparison.Ordinal);
    }

    private static IEnumerable<ChannelFetchSpec> ChannelFetchSpecs()
    {
        yield return new ChannelFetchSpec(GameChannel.Mindustry, "Anuken", "Mindustry", ReleaseFilter.Stable, "Mindustry");
        yield return new ChannelFetchSpec(GameChannel.MindustryX, "TinyLake", "MindustryX", ReleaseFilter.Stable, "MindustryX");
        yield return new ChannelFetchSpec(GameChannel.MindustryBE, "Anuken", "MindustryBuilds", ReleaseFilter.Any, "Mindustry BE");
        yield return new ChannelFetchSpec(GameChannel.MindustryXBE, "TinyLake", "MindustryX", ReleaseFilter.Prerelease, "MindustryX BE");
    }

    private sealed record ChannelFetchSpec(GameChannel Channel, string Owner, string Repo, ReleaseFilter Filter, string Label)
    {
        public bool FilterMatches(bool prerelease)
        {
            return Filter switch
            {
                ReleaseFilter.Stable => !prerelease,
                ReleaseFilter.Prerelease => prerelease,
                _ => true
            };
        }
    }

    private sealed record AtomRelease(string Tag, string Name, bool Prerelease, string? PublishedAt);

    private enum ReleaseFilter
    {
        Stable,
        Prerelease,
        Any
    }
}
