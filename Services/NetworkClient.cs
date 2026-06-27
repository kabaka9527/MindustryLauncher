using System.Collections.Concurrent;
using System.Net;
using System.Net.Http.Headers;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization.Metadata;
using MindustryLauncher.Models;

namespace MindustryLauncher.Services;

public sealed class DownloadControl
{
    private TaskCompletionSource _resumeSignal = new(TaskCreationOptions.RunContinuationsAsynchronously);

    public bool Paused { get; private set; }
    public bool Canceled { get; private set; }

    public void Pause()
    {
        if (!Paused)
        {
            _resumeSignal = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        }

        Paused = true;
    }

    public void Resume()
    {
        Paused = false;
        _resumeSignal.TrySetResult();
    }

    public void Cancel()
    {
        Canceled = true;
        Paused = false;
        _resumeSignal.TrySetResult();
    }

    public async Task WaitWhilePausedAsync(CancellationToken cancellationToken)
    {
        while (Paused && !Canceled)
        {
            await _resumeSignal.Task.WaitAsync(cancellationToken);
        }
    }
}

public sealed class NetworkClient
{
    private const string UserAgent = "MindustryLauncher/0.1.1";

    private static readonly ConcurrentDictionary<string, DownloadControl> DownloadControls = new();

    private readonly HttpClient _http;
    private readonly string _cacheDir;

    public NetworkClient(Settings settings, string cacheDir)
    {
        Directory.CreateDirectory(cacheDir);
        _cacheDir = cacheDir;

        var handler = new HttpClientHandler
        {
            AutomaticDecompression = DecompressionMethods.None,
            AllowAutoRedirect = true,
            MaxAutomaticRedirections = 5
        };

        if (!string.IsNullOrWhiteSpace(settings.HttpProxy))
        {
            handler.Proxy = new WebProxy(settings.HttpProxy);
            handler.UseProxy = true;
        }

        _http = new HttpClient(handler)
        {
            Timeout = TimeSpan.FromSeconds(45)
        };
        _http.DefaultRequestHeaders.UserAgent.ParseAdd(UserAgent);
    }

    public static void PauseDownload(string taskId)
    {
        if (DownloadControls.TryGetValue(taskId, out var control))
        {
            control.Pause();
        }
    }

    public static void ResumeDownload(string taskId)
    {
        if (DownloadControls.TryGetValue(taskId, out var control))
        {
            control.Resume();
        }
    }

    public static void CancelDownload(string taskId)
    {
        if (DownloadControls.TryGetValue(taskId, out var control))
        {
            control.Cancel();
        }
    }

    public async Task<T> GetJsonAsync<T>(string url, JsonTypeInfo<T> jsonTypeInfo, CancellationToken cancellationToken = default)
    {
        var body = await GetTextCachedAsync(url, cancellationToken);
        return JsonSerializer.Deserialize(body, jsonTypeInfo)
            ?? throw new InvalidOperationException($"无法解析远端 JSON：{url}");
    }

    public async Task<T> GetJsonUncachedAsync<T>(string url, JsonTypeInfo<T> jsonTypeInfo, CancellationToken cancellationToken = default)
    {
        var body = await GetTextUncachedAsync(url, cancellationToken);
        return JsonSerializer.Deserialize(body, jsonTypeInfo)
            ?? throw new InvalidOperationException($"无法解析远端 JSON：{url}");
    }

    public async Task<string> GetTextUncachedAsync(string url, CancellationToken cancellationToken = default)
    {
        Exception? last = null;
        for (var attempt = 0; attempt < 3; attempt++)
        {
            try
            {
                using var response = await _http.GetAsync(url, cancellationToken);
                if (response.IsSuccessStatusCode)
                {
                    return await response.Content.ReadAsStringAsync(cancellationToken);
                }

                if (attempt == 2 || (int)response.StatusCode < 500)
                {
                    throw new HttpRequestException($"{url} 返回 HTTP {(int)response.StatusCode}");
                }
            }
            catch (Exception ex) when (attempt < 2)
            {
                last = ex;
            }

            await Task.Delay(350 * (attempt + 1), cancellationToken);
        }

        throw new HttpRequestException(last?.Message ?? $"请求失败：{url}");
    }

    public async Task<string> GetTextCachedAsync(string url, CancellationToken cancellationToken = default)
    {
        var cachePath = CachePathFor(url);
        CachedResponse? cached = null;
        if (File.Exists(cachePath))
        {
            cached = await FileSystemUtil.ReadJsonAsync(cachePath, AppJsonContext.Default.CachedResponse);
        }

        Exception? last = null;
        for (var attempt = 0; attempt < 3; attempt++)
        {
            try
            {
                using var request = new HttpRequestMessage(HttpMethod.Get, url);
                if (!string.IsNullOrWhiteSpace(cached?.Etag))
                {
                    request.Headers.TryAddWithoutValidation("If-None-Match", cached.Etag);
                }

                using var response = await _http.SendAsync(request, cancellationToken);
                if (response.StatusCode == HttpStatusCode.NotModified && cached is not null)
                {
                    return cached.Body;
                }

                if (response.IsSuccessStatusCode)
                {
                    var body = await response.Content.ReadAsStringAsync(cancellationToken);
                    await FileSystemUtil.WriteJsonAsync(cachePath, new CachedResponse
                    {
                        Etag = response.Headers.ETag?.Tag,
                        Body = body
                    }, AppJsonContext.Default.CachedResponse);
                    return body;
                }

                if (attempt == 2 || (int)response.StatusCode < 500)
                {
                    throw new HttpRequestException($"{url} 返回 HTTP {(int)response.StatusCode}");
                }
            }
            catch (Exception ex) when (attempt < 2)
            {
                last = ex;
            }

            await Task.Delay(350 * (attempt + 1), cancellationToken);
        }

        if (cached is not null)
        {
            return cached.Body;
        }

        throw new HttpRequestException(last?.Message ?? $"请求失败：{url}");
    }

    public async Task DownloadToFileAsync(
        string url,
        string destination,
        string? expectedDigest,
        ulong? knownTotalBytes,
        string taskId,
        string label,
        Action<TaskRecord> onTask,
        CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        var tmp = Path.ChangeExtension(destination, ".download");
        var control = new DownloadControl();
        DownloadControls[taskId] = control;
        var task = new TaskRecord { Id = taskId, Label = label };
        onTask(task);
        void Publish() => onTask(task);

        AppDebugLog.Info($"开始下载：{label}（{url}）");

        try
        {
            var total = knownTotalBytes ?? await ResolveDownloadSizeAsync(url, cancellationToken);
            task.TotalBytes = total;
            task.Message = "连接中";
            Publish();

            for (var attempt = 0; attempt < 3; attempt++)
            {
                cancellationToken.ThrowIfCancellationRequested();
                if (control.Canceled)
                {
                    task.Status = "canceled";
                    task.Message = "已取消";
                    Publish();
                    AppDebugLog.Warn($"下载已取消：{label}");
                    TryDelete(tmp);
                    throw new OperationCanceledException("下载已取消");
                }

                var resumeFrom = PartialDownloadLength(tmp, total);
                using var request = new HttpRequestMessage(HttpMethod.Get, url);
                request.Headers.AcceptEncoding.Add(new StringWithQualityHeaderValue("identity"));
                if (resumeFrom > 0)
                {
                    request.Headers.Range = new RangeHeaderValue((long)resumeFrom, null);
                    task.Message = "准备续传";
                    Publish();
                }

                try
                {
                    using var response = await _http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken);
                    if (response.StatusCode == HttpStatusCode.RequestedRangeNotSatisfiable && total is not null && resumeFrom >= total)
                    {
                        break;
                    }

                    response.EnsureSuccessStatusCode();
                    var appending = resumeFrom > 0 && response.StatusCode == HttpStatusCode.PartialContent;
                    if (resumeFrom > 0 && !appending)
                    {
                        TryDelete(tmp);
                        resumeFrom = 0;
                    }

                    var resolvedTotal = ResolveTotalFromHeaders(response, total, resumeFrom);
                    if (resolvedTotal is not null)
                    {
                        task.TotalBytes = resolvedTotal;
                        Publish();
                    }

                    await using var input = await response.Content.ReadAsStreamAsync(cancellationToken);
                    await using var output = new FileStream(tmp, appending ? FileMode.Append : FileMode.Create, FileAccess.Write, FileShare.Read);
                    var buffer = new byte[64 * 1024];
                    var downloaded = resumeFrom;
                    var started = DateTimeOffset.UtcNow;
                    var lastPublishBytes = resumeFrom;
                    var lastPublishTime = started;
                    while (true)
                    {
                        if (control.Canceled)
                        {
                            task.Status = "canceled";
                            task.Message = "已取消";
                            Publish();
                            TryDelete(tmp);
                            throw new OperationCanceledException("下载已取消");
                        }

                        if (control.Paused)
                        {
                            task.Status = "paused";
                            task.Message = "已暂停";
                            Publish();
                            await output.FlushAsync(cancellationToken);
                            await control.WaitWhilePausedAsync(cancellationToken);
                            task.Status = "running";
                            task.Message = "继续下载";
                            downloaded = 0;
                            started = DateTimeOffset.UtcNow;
                            lastPublishBytes = 0;
                            lastPublishTime = started;
                            Publish();
                        }

                        var read = await input.ReadAsync(buffer, cancellationToken);
                        if (read == 0)
                        {
                            break;
                        }

                        await output.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
                        downloaded += (ulong)read;
                        task.DownloadedBytes = resumeFrom + downloaded;

                        var now = DateTimeOffset.UtcNow;
                        var elapsed = (now - lastPublishTime).TotalSeconds;
                        if (elapsed >= 0.3)
                        {
                            var delta = downloaded - lastPublishBytes;
                            task.BytesPerSecond = delta > 0
                                ? (ulong)(delta / Math.Max(0.001, elapsed))
                                : 0;
                            lastPublishBytes = downloaded;
                            lastPublishTime = now;
                            task.Message = "下载中";
                            Publish();
                        }
                    }

                    break;
                }
                catch (Exception ex) when (attempt < 2)
                {
                    task.Message = $"重试 {attempt + 2}/3";
                    Publish();
                    AppDebugLog.Warn($"下载重试 {attempt + 2}/3：{label}（{ex.Message}）");
                    await Task.Delay(500 * (attempt + 1), cancellationToken);
                }
            }

            if (!string.IsNullOrWhiteSpace(expectedDigest))
            {
                await VerifySha256Async(tmp, expectedDigest, cancellationToken);
            }

            if (File.Exists(destination))
            {
                File.Delete(destination);
            }

            File.Move(tmp, destination, true);
            task.Status = "finished";
            task.Message = "完成";
            task.DownloadedBytes = task.TotalBytes ?? (ulong)new FileInfo(destination).Length;
            Publish();
            AppDebugLog.Info($"下载完成：{label}");
        }
        catch (Exception ex)
        {
            if (task.Status != "canceled")
            {
                task.Status = "failed";
                task.Message = "下载失败";
                Publish();
                AppDebugLog.Error($"下载失败：{label}（{ex.Message}）");
            }

            throw;
        }
        finally
        {
            DownloadControls.TryRemove(taskId, out _);
        }
    }

    private static ulong? ResolveTotalFromHeaders(HttpResponseMessage response, ulong? knownTotal, ulong resumeFrom)
    {
        if (response.Content.Headers.ContentRange?.Length is long rangeLength and > 0)
        {
            return (ulong)rangeLength;
        }

        if (response.Content.Headers.ContentLength is long contentLength and > 0)
        {
            return resumeFrom + (ulong)contentLength;
        }

        return knownTotal;
    }

    private async Task<ulong?> ResolveDownloadSizeAsync(string url, CancellationToken cancellationToken)
    {
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Head, url);
            using var response = await _http.SendAsync(request, cancellationToken);
            if (response.IsSuccessStatusCode && response.Content.Headers.ContentLength is long length and > 0)
            {
                return (ulong)length;
            }
        }
        catch
        {
        }

        return null;
    }

    private static ulong PartialDownloadLength(string path, ulong? totalBytes)
    {
        if (!File.Exists(path))
        {
            return 0;
        }

        var length = (ulong)new FileInfo(path).Length;
        if (totalBytes is not null && length > totalBytes.Value)
        {
            TryDelete(path);
            return 0;
        }

        return length;
    }

    private static async Task VerifySha256Async(string path, string expectedDigest, CancellationToken cancellationToken)
    {
        var expected = expectedDigest.Replace("sha256:", string.Empty, StringComparison.OrdinalIgnoreCase).Trim().ToLowerInvariant();
        if (string.IsNullOrWhiteSpace(expected))
        {
            return;
        }

        await using var stream = File.OpenRead(path);
        var hash = await SHA256.HashDataAsync(stream, cancellationToken);
        var actual = Convert.ToHexString(hash).ToLowerInvariant();
        if (actual != expected)
        {
            throw new InvalidOperationException($"校验失败：expected {expected}, got {actual}");
        }
    }

    private string CachePathFor(string url)
    {
        var bytes = SHA256.HashData(Encoding.UTF8.GetBytes(url));
        return Path.Combine(_cacheDir, $"{Convert.ToHexString(bytes).ToLowerInvariant()}.json");
    }

    private static void TryDelete(string path)
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

}

internal sealed class CachedResponse
{
    public string? Etag { get; set; }
    public string Body { get; set; } = string.Empty;
}
