using System.Text.Json.Serialization;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

[JsonSerializable(typeof(Settings))]
[JsonSerializable(typeof(ChannelVisibility))]
[JsonSerializable(typeof(LaunchSettings))]
[JsonSerializable(typeof(AcceleratorList))]
[JsonSerializable(typeof(RemoteVersion))]
[JsonSerializable(typeof(List<RemoteVersion>))]
[JsonSerializable(typeof(InstalledInstance))]
[JsonSerializable(typeof(List<InstalledInstance>))]
[JsonSerializable(typeof(RuntimeInfo))]
[JsonSerializable(typeof(List<RuntimeInfo>))]
[JsonSerializable(typeof(GitHubRelease))]
[JsonSerializable(typeof(List<GitHubRelease>))]
[JsonSerializable(typeof(GitHubAsset))]
[JsonSerializable(typeof(InstallRootPointer))]
[JsonSerializable(typeof(CachedResponse))]
internal partial class AppJsonContext : JsonSerializerContext
{
}
