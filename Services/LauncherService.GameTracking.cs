using System.Collections.Concurrent;
using System.Diagnostics;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed partial class LauncherService
{
    private readonly ConcurrentDictionary<string, (System.Diagnostics.Process Proc, DateTime StartTime)> _runningGames = new();

    public event Action<string, TimeSpan>? GameExited;

    public bool IsGameRunning(string instanceId) => _runningGames.ContainsKey(instanceId);

    public DateTime? GetGameStartTime(string instanceId) =>
        _runningGames.TryGetValue(instanceId, out var info) ? info.StartTime : null;

    public void TrackGame(string instanceId, System.Diagnostics.Process process, DateTime startTime)
    {
        _runningGames[instanceId] = (process, startTime);

        _ = process.WaitForExitAsync().ContinueWith(t =>
        {
            var sessionDuration = DateTime.Now - startTime;

            if (_runningGames.TryRemove(instanceId, out _))
            {
                try
                {
                    var layout = Layout;
                    var instances = LoadInstancesAsync(layout).Result;
                    var instance = instances.FirstOrDefault(i => i.Id == instanceId);
                    if (instance is not null)
                    {
                        instance.TotalPlayTimeTicks += sessionDuration.Ticks;
                        SaveInstancesAsync(layout, instances).Wait();
                    }
                }
                catch (Exception ex)
                {
                    AppDebugLog.Error($"保存游戏时长失败：{ex.Message}");
                }

                GameExited?.Invoke(instanceId, sessionDuration);
                AppDebugLog.Info($"游戏已退出（{instanceId}），本次 {sessionDuration:hh\\:mm\\:ss}");
            }
        }, TaskContinuationOptions.ExecuteSynchronously);
    }

    public async Task<List<InstalledInstance>> LoadInstancesWithRunningStateAsync()
    {
        var layout = Layout;
        var instances = await LoadInstancesAsync(layout);

        foreach (var instance in instances)
        {
            if (_runningGames.TryGetValue(instance.Id, out var info))
            {
                if (!info.Proc.HasExited)
                {
                    instance.IsRunning = true;
                    instance.CurrentSessionStart = info.StartTime;
                }
                else
                {
                    _runningGames.TryRemove(instance.Id, out _);
                }
            }
        }

        return instances;
    }
}
