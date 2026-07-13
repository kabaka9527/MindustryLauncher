use crate::{
    debug_console,
    error::{AppError, AppResult},
    fs_util,
    models::{Settings, TaskEvent, USER_AGENT},
};
use reqwest::{
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
    redirect::Policy,
    Client, ClientBuilder, StatusCode,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tauri::ipc::Channel;
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Notify,
    time::sleep,
};

// All outbound requests go through this client so proxy, timeout, retry,
// cache, and progress behavior stay consistent across GitHub and runtime fetches.
#[derive(Debug, Clone)]
pub struct NetworkClient {
    client: Client,
    cache_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedResponse {
    etag: Option<String>,
    body: String,
}

#[derive(Debug)]
struct DownloadControl {
    paused: AtomicBool,
    canceled: AtomicBool,
    notify: Notify,
}

#[derive(Debug, Clone, Copy)]
enum DownloadFlow {
    Completed,
    Paused,
    Canceled,
}

struct DownloadRegistration {
    task_id: String,
}

impl Drop for DownloadRegistration {
    fn drop(&mut self) {
        unregister_download_task(&self.task_id);
    }
}

static DOWNLOAD_CONTROLS: OnceLock<Mutex<HashMap<String, Arc<DownloadControl>>>> = OnceLock::new();

pub fn pause_download_task(task_id: &str) -> AppResult<()> {
    let control = download_control(task_id)?;
    control.paused.store(true, Ordering::SeqCst);
    control.notify.notify_waiters();
    Ok(())
}

pub fn resume_download_task(task_id: &str) -> AppResult<()> {
    let control = download_control(task_id)?;
    control.paused.store(false, Ordering::SeqCst);
    control.notify.notify_waiters();
    Ok(())
}

pub fn cancel_download_task(task_id: &str) -> AppResult<()> {
    let control = download_control(task_id)?;
    control.canceled.store(true, Ordering::SeqCst);
    control.paused.store(false, Ordering::SeqCst);
    control.notify.notify_waiters();
    Ok(())
}

fn register_download_task(task_id: &str) -> (Arc<DownloadControl>, DownloadRegistration) {
    let control = Arc::new(DownloadControl {
        paused: AtomicBool::new(false),
        canceled: AtomicBool::new(false),
        notify: Notify::new(),
    });
    if let Ok(mut controls) = download_controls().lock() {
        controls.insert(task_id.to_string(), control.clone());
    }
    (
        control,
        DownloadRegistration {
            task_id: task_id.to_string(),
        },
    )
}

fn unregister_download_task(task_id: &str) {
    if let Ok(mut controls) = download_controls().lock() {
        controls.remove(task_id);
    }
}

fn download_control(task_id: &str) -> AppResult<Arc<DownloadControl>> {
    download_controls()
        .lock()
        .ok()
        .and_then(|controls| controls.get(task_id).cloned())
        .ok_or_else(|| AppError::NotFound(format!("download task {task_id}")))
}

fn download_controls() -> &'static Mutex<HashMap<String, Arc<DownloadControl>>> {
    DOWNLOAD_CONTROLS.get_or_init(|| Mutex::new(HashMap::new()))
}

impl NetworkClient {
    pub fn new(settings: &Settings, cache_dir: PathBuf) -> AppResult<Self> {
        fs_util::ensure_dir(&cache_dir)?;
        // reqwest's default features include system-proxy. When the user sets
        // a proxy, disable system proxies first so the explicit value wins.
        let builder = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .redirect(Policy::limited(5))
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .user_agent(USER_AGENT);

        let builder = if let Some(proxy) = settings
            .http_proxy
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            // 显式代理优先：禁用系统代理，仅用用户指定值。
            builder.no_proxy().proxy(reqwest::Proxy::all(proxy)?)
        } else {
            // 未显式配置时：环境变量代理交给 reqwest 默认读取；
            // 否则在 Windows 上读取系统代理（设置-网络-代理 / IE 代理），
            // 覆盖仅通过 GUI 配置、未设 HTTP_PROXY 环境变量的场景。
            apply_system_proxy(builder)?
        };

        Ok(Self {
            client: builder.build()?,
            cache_dir,
        })
    }

    // 探测镜像延迟：优先 HEAD（最轻量），若服务器不支持 HEAD（多数 ghproxy 类
    // 镜像代理对代理路径不实现 HEAD，会返回 405/400 等非 2xx，导致测不到延迟），
    // 则回退到 GET + Range(仅取首字节) 探测，保证延迟可被正常测得。
    // 每一步均写入调试日志（仅在调试模式开启时落盘/推送），便于排查探测失败原因。
    pub async fn probe_latency(&self, url: &str) -> AppResult<u64> {
        if let Ok(ms) = self.try_head(url).await {
            return Ok(ms);
        }
        debug_console::log(format!(
            "[加速源健康检测] HEAD 不可用，回退 GET 探测 {url}"
        ));
        let start = Instant::now();
        match self
            .client
            .get(url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await
        {
            Ok(response) => {
                let elapsed = start.elapsed();
                if response.status().is_success() {
                    debug_console::log(format!(
                        "[加速源健康检测] GET 成功 {url} -> {}ms",
                        elapsed.as_millis()
                    ));
                    Ok(elapsed.as_millis() as u64)
                } else {
                    let status = response.status();
                    debug_console::log(format!(
                        "[加速源健康检测] GET 非 2xx {url} -> HTTP {status}"
                    ));
                    Err(AppError::Network(format!("{url} returned HTTP {status}")))
                }
            }
            Err(err) => {
                debug_console::log(format!(
                    "[加速源健康检测] GET 请求失败 {url} -> {err}"
                ));
                Err(AppError::Network(err.to_string()))
            }
        }
    }

    async fn try_head(&self, url: &str) -> AppResult<u64> {
        let start = Instant::now();
        match self.client.head(url).send().await {
            Ok(response) => {
                let elapsed = start.elapsed();
                if response.status().is_success() {
                    debug_console::log(format!(
                        "[加速源健康检测] HEAD 成功 {url} -> {}ms",
                        elapsed.as_millis()
                    ));
                    Ok(elapsed.as_millis() as u64)
                } else {
                    let status = response.status();
                    debug_console::log(format!(
                        "[加速源健康检测] HEAD 非 2xx/不支持 {url} -> HTTP {status}"
                    ));
                    Err(AppError::Network(format!("{url} returned HTTP {status}")))
                }
            }
            Err(err) => {
                debug_console::log(format!(
                    "[加速源健康检测] HEAD 请求失败 {url} -> {err}"
                ));
                Err(AppError::Network(err.to_string()))
            }
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> AppResult<T> {
        let body = self.get_text_cached(url).await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn get_json_uncached<T: DeserializeOwned>(&self, url: &str) -> AppResult<T> {
        let body = self.get_text_uncached(url).await?;
        Ok(serde_json::from_str(&body)?)
    }

    async fn get_text_uncached(&self, url: &str) -> AppResult<String> {
        for attempt in 0..3 {
            match self.client.get(url).send().await {
                Ok(response) if response.status().is_success() => {
                    return Ok(response.text().await?);
                }
                Ok(response) => {
                    let status = response.status();
                    if attempt == 2 || !status.is_server_error() {
                        return Err(AppError::Network(format!("{url} returned HTTP {status}")));
                    }
                }
                Err(err) => {
                    if attempt == 2 {
                        return Err(AppError::Network(err.to_string()));
                    }
                }
            }
            sleep(Duration::from_millis(350 * (attempt + 1) as u64)).await;
        }

        Err(AppError::Network(format!("request failed: {url}")))
    }

    pub async fn get_text_cached(&self, url: &str) -> AppResult<String> {
        let cache_path = self.cache_path_for(url);
        let cached = fs_util::read_json::<CachedResponse>(&cache_path)?;

        for attempt in 0..3 {
            let mut request = self.client.get(url);
            if let Some(etag) = cached.as_ref().and_then(|item| item.etag.as_deref()) {
                request = request.header("If-None-Match", etag);
            }

            match request.send().await {
                Ok(response) if response.status() == StatusCode::NOT_MODIFIED => {
                    if let Some(cached) = cached {
                        return Ok(cached.body);
                    }
                    return Err(AppError::Network(format!(
                        "{url} returned 304 without cache"
                    )));
                }
                Ok(response) if response.status().is_success() => {
                    let etag = response
                        .headers()
                        .get("etag")
                        .and_then(|value| value.to_str().ok())
                        .map(ToOwned::to_owned);
                    let body = response.text().await?;
                    fs_util::write_json(
                        &cache_path,
                        &CachedResponse {
                            etag,
                            body: body.clone(),
                        },
                    )?;
                    return Ok(body);
                }
                Ok(response) => {
                    let status = response.status();
                    if attempt == 2 || !status.is_server_error() {
                        return Err(AppError::Network(format!("{url} returned HTTP {status}")));
                    }
                }
                Err(err) => {
                    if attempt == 2 {
                        if let Some(cached) = cached {
                            return Ok(cached.body);
                        }
                        return Err(AppError::Network(err.to_string()));
                    }
                }
            }
            sleep(Duration::from_millis(350 * (attempt + 1) as u64)).await;
        }

        Err(AppError::Network(format!("request failed: {url}")))
    }

    pub async fn download_to_file(
        &self,
        url: &str,
        destination: &Path,
        expected_digest: Option<&str>,
        known_total_bytes: Option<u64>,
        task_id: &str,
        label: &str,
        on_event: Channel<TaskEvent>,
    ) -> AppResult<()> {
        if let Some(parent) = destination.parent() {
            fs_util::ensure_dir(parent)?;
        }

        let (control, _registration) = register_download_task(task_id);
        let tmp = destination.with_extension("download");
        let resolved_total_bytes = match known_total_bytes {
            Some(value) => Some(value),
            None => self.resolve_download_size(url).await.ok().flatten(),
        };
        let mut attempt = 0_usize;
        let mut started_sent = false;
        while attempt < 3 {
            if control.canceled.load(Ordering::SeqCst) {
                let _ = fs::remove_file(&tmp).await;
                let _ = on_event.send(TaskEvent::Canceled {
                    task_id: task_id.to_string(),
                    message: "下载已取消".to_string(),
                });
                return Err(AppError::Network("下载已取消".to_string()));
            }
            let resume_from = partial_download_len(&tmp, resolved_total_bytes).await?;
            let send_started = !started_sent;
            started_sent = true;
            match self
                .download_once(
                    url,
                    &tmp,
                    resolved_total_bytes,
                    resume_from,
                    send_started,
                    task_id,
                    label,
                    on_event.clone(),
                    control.clone(),
                )
                .await
            {
                Ok(DownloadFlow::Completed) => {
                    if let Some(expected) = expected_digest {
                        if let Err(err) = verify_file_digest(&tmp, expected).await {
                            let _ = fs::remove_file(&tmp).await;
                            let _ = on_event.send(TaskEvent::Failed {
                                task_id: task_id.to_string(),
                                message: err.to_string(),
                            });
                            return Err(err);
                        }
                    }
                    if destination.exists() {
                        fs::remove_file(destination).await?;
                    }
                    fs::rename(&tmp, destination).await?;
                    let _ = on_event.send(TaskEvent::Finished {
                        task_id: task_id.to_string(),
                        message: "downloaded".to_string(),
                    });
                    return Ok(());
                }
                Ok(DownloadFlow::Paused) => {
                    wait_for_download_resume_or_cancel(
                        task_id,
                        &tmp,
                        resolved_total_bytes,
                        on_event.clone(),
                        control.clone(),
                    )
                    .await?;
                }
                Ok(DownloadFlow::Canceled) => {
                    let _ = fs::remove_file(&tmp).await;
                    let _ = on_event.send(TaskEvent::Canceled {
                        task_id: task_id.to_string(),
                        message: "下载已取消".to_string(),
                    });
                    return Err(AppError::Network("下载已取消".to_string()));
                }
                Err(err) if attempt < 2 => {
                    let downloaded_bytes = partial_download_len(&tmp, resolved_total_bytes)
                        .await
                        .unwrap_or(0);
                    let _ = on_event.send(TaskEvent::Progress {
                        task_id: task_id.to_string(),
                        downloaded_bytes,
                        total_bytes: resolved_total_bytes,
                        bytes_per_second: None,
                        message: Some(format!(
                            "重试 {}/3{}",
                            attempt + 2,
                            if downloaded_bytes > 0 {
                                "，尝试续传"
                            } else {
                                ""
                            }
                        )),
                    });
                    debug_console::log(format!("下载重试 {}/3: {label}: {err}", attempt + 2));
                    sleep(Duration::from_millis(500 * (attempt + 1) as u64)).await;
                    if err.to_string().contains("checksum") {
                        return Err(err);
                    }
                    attempt += 1;
                }
                Err(err) => {
                    let _ = fs::remove_file(&tmp).await;
                    let _ = on_event.send(TaskEvent::Failed {
                        task_id: task_id.to_string(),
                        message: err.to_string(),
                    });
                    return Err(err);
                }
            }
        }
        let _ = fs::remove_file(&tmp).await;
        Err(AppError::Network(format!("download failed: {url}")))
    }

    async fn download_once(
        &self,
        url: &str,
        tmp: &Path,
        known_total_bytes: Option<u64>,
        resume_from: u64,
        send_started: bool,
        task_id: &str,
        label: &str,
        on_event: Channel<TaskEvent>,
        control: Arc<DownloadControl>,
    ) -> AppResult<DownloadFlow> {
        debug_console::log(format!("开始下载: {label} <- {url}"));
        if send_started {
            let _ = on_event.send(TaskEvent::Started {
                task_id: task_id.to_string(),
                label: label.to_string(),
                total_bytes: known_total_bytes,
                message: Some(
                    if resume_from > 0 {
                        "准备续传"
                    } else {
                        "连接中"
                    }
                    .to_string(),
                ),
            });
        }

        let mut request = self.client.get(url).header(ACCEPT_ENCODING, "identity");
        if resume_from > 0 {
            request = request.header(RANGE, format!("bytes={resume_from}-"));
        }
        let mut response = request.send().await?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            if known_total_bytes.is_some_and(|total| resume_from >= total) {
                debug_console::log(format!("下载临时文件已完整: {label}, bytes={resume_from}"));
                return Ok(DownloadFlow::Completed);
            }
            let _ = fs::remove_file(tmp).await;
            return Err(AppError::Network(format!(
                "{url} returned HTTP {} for resume request",
                response.status()
            )));
        }
        if !response.status().is_success() {
            return Err(AppError::Network(format!(
                "{url} returned HTTP {}",
                response.status()
            )));
        }

        let status = response.status();
        let can_resume = resume_from > 0 && status == StatusCode::PARTIAL_CONTENT;
        let downloaded_offset = if can_resume {
            resume_from
        } else if resume_from > 0 {
            debug_console::log(format!(
                "下载续传被服务器忽略，重新下载: {label}, previous_bytes={resume_from}, status={status}"
            ));
            let _ = fs::remove_file(tmp).await;
            0
        } else {
            0
        };
        let total = if can_resume {
            response
                .headers()
                .get(CONTENT_RANGE)
                .and_then(|value| value.to_str().ok())
                .and_then(parse_content_range_total)
                .or(known_total_bytes)
                .or_else(|| {
                    response
                        .content_length()
                        .map(|value| value + downloaded_offset)
                })
        } else {
            response
                .content_length()
                .filter(|value| *value > 0)
                .or(known_total_bytes)
        };
        debug_console::log(format!(
            "下载响应: {label}, status={status}, resume_from={downloaded_offset}, total={}",
            total
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ));
        let _ = on_event.send(TaskEvent::Progress {
            task_id: task_id.to_string(),
            downloaded_bytes: downloaded_offset,
            total_bytes: total,
            bytes_per_second: None,
            message: Some(if downloaded_offset > 0 {
                "续传中".to_string()
            } else {
                "等待首包".to_string()
            }),
        });

        let mut file = if downloaded_offset > 0 {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(tmp)
                .await?
        } else {
            File::create(tmp).await?
        };
        let mut downloaded = downloaded_offset;
        let mut speed_last_bytes = downloaded_offset;
        let mut speed_last_time = Instant::now();
        let mut current_speed = 0u64;
        let mut last_event = Instant::now();
        loop {
            if control.canceled.load(Ordering::SeqCst) {
                return Ok(DownloadFlow::Canceled);
            }
            if control.paused.load(Ordering::SeqCst) {
                file.flush().await?;
                let _ = on_event.send(TaskEvent::Paused {
                    task_id: task_id.to_string(),
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                    message: "已暂停".to_string(),
                });
                return Ok(DownloadFlow::Paused);
            }

            let chunk = tokio::select! {
                _ = control.notify.notified() => {
                    continue;
                }
                chunk = response.chunk() => chunk?,
            };
            let Some(chunk) = chunk else {
                break;
            };
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if speed_last_time.elapsed() >= Duration::from_secs(2) {
                let elapsed = speed_last_time.elapsed().as_secs_f64().max(0.001);
                current_speed = ((downloaded - speed_last_bytes) as f64 / elapsed) as u64;
                speed_last_bytes = downloaded;
                speed_last_time = Instant::now();
            }
            if last_event.elapsed() >= Duration::from_millis(300) {
                let _ = on_event.send(TaskEvent::Progress {
                    task_id: task_id.to_string(),
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                    bytes_per_second: Some(current_speed),
                    message: Some("下载中".to_string()),
                });
                last_event = Instant::now();
            }
        }
        file.flush().await?;
        debug_console::log(format!("下载完成: {label}, bytes={downloaded}"));
        Ok(DownloadFlow::Completed)
    }

    async fn resolve_download_size(&self, url: &str) -> AppResult<Option<u64>> {
        let response = self
            .client
            .head(url)
            .header(ACCEPT_ENCODING, "identity")
            .send()
            .await?;
        Ok(response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|size| *size > 0))
    }

    fn cache_path_for(&self, url: &str) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        self.cache_dir
            .join(format!("{}.json", hex_encode(&hasher.finalize())))
    }
}

pub async fn verify_file_digest(path: &Path, expected_digest: &str) -> AppResult<()> {
    let expected = expected_digest
        .strip_prefix("sha256:")
        .unwrap_or(expected_digest)
        .trim()
        .to_ascii_lowercase();
    if expected.is_empty() {
        return Ok(());
    }

    let mut file = File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 64];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hex_encode(&hasher.finalize());
    if actual == expected {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "checksum mismatch for {}: expected {expected}, got {actual}",
            path.display()
        )))
    }
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.parse::<u64>().ok()
}

async fn partial_download_len(path: &Path, total_bytes: Option<u64>) -> AppResult<u64> {
    match fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => {
            let len = metadata.len();
            if len == 0 {
                return Ok(0);
            }
            if total_bytes.is_some_and(|total| len > total) {
                let _ = fs::remove_file(path).await;
                return Ok(0);
            }
            Ok(len)
        }
        Ok(_) => Ok(0),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(0),
        Err(err) => Err(AppError::Io(err.to_string())),
    }
}

async fn wait_for_download_resume_or_cancel(
    task_id: &str,
    tmp: &Path,
    total_bytes: Option<u64>,
    on_event: Channel<TaskEvent>,
    control: Arc<DownloadControl>,
) -> AppResult<()> {
    loop {
        if control.canceled.load(Ordering::SeqCst) {
            let _ = fs::remove_file(tmp).await;
            let _ = on_event.send(TaskEvent::Canceled {
                task_id: task_id.to_string(),
                message: "下载已取消".to_string(),
            });
            return Err(AppError::Network("下载已取消".to_string()));
        }
        if !control.paused.load(Ordering::SeqCst) {
            let downloaded_bytes = partial_download_len(tmp, total_bytes).await.unwrap_or(0);
            let _ = on_event.send(TaskEvent::Progress {
                task_id: task_id.to_string(),
                downloaded_bytes,
                total_bytes,
                bytes_per_second: None,
                message: Some(if downloaded_bytes > 0 {
                    "继续下载，准备续传".to_string()
                } else {
                    "继续下载".to_string()
                }),
            });
            return Ok(());
        }
        control.notify.notified().await;
    }
}

// 未显式配置代理时的系统代理解析：环境变量优先（reqwest 默认会读取），
// Windows 上再回退到系统代理（设置-网络-代理 / IE 代理），
// 覆盖仅通过 GUI 配置、未设 HTTP_PROXY 环境变量的场景。
// 注意：仅手动代理服务器（lpszProxy）生效；PAC（自动配置脚本）与自动检测未覆盖。
fn apply_system_proxy(builder: ClientBuilder) -> AppResult<ClientBuilder> {
    if env_proxy_configured() {
        return Ok(builder);
    }
    #[cfg(windows)]
    {
        if let Some(server) = windows_ie_proxy_server() {
            let (http, https) = parse_ie_proxy_server(&server);
            let mut builder = builder;
            if let Some(http) = http {
                builder = builder.proxy(reqwest::Proxy::http(http)?);
            }
            if let Some(https) = https {
                builder = builder.proxy(reqwest::Proxy::https(https)?);
            }
            debug_console::log(format!("[系统代理] 使用 Windows 系统代理：{server}"));
            return Ok(builder);
        }
    }
    Ok(builder)
}

fn env_proxy_configured() -> bool {
    const KEYS: &[&str] = &[
        "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy",
    ];
    KEYS
        .iter()
        .any(|key| std::env::var(key).map(|value| !value.trim().is_empty()).unwrap_or(false))
}

// 解析 IE/系统代理服务器字符串（如 "http=host:80;https=host:443" 或裸 "host:8080"），
// 返回 (http, https) 两个代理 URL。任一缺失时回退到另一个，保证 https 下载（GitHub）可用。
fn parse_ie_proxy_server(server: &str) -> (Option<String>, Option<String>) {
    let mut http = None;
    let mut https = None;
    for part in server.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((scheme, addr)) = part.split_once('=') {
            let addr = addr.trim();
            if addr.is_empty() {
                continue;
            }
            match scheme.to_ascii_lowercase().as_str() {
                "http" => http = Some(format!("http://{addr}")),
                "https" => https = Some(format!("https://{addr}")),
                _ => {}
            }
        } else {
            let url = format!("http://{part}");
            if http.is_none() {
                http = Some(url.clone());
            }
            if https.is_none() {
                https = Some(url);
            }
        }
    }
    let http = http.or_else(|| https.clone());
    let https = https.or_else(|| http.clone());
    (http, https)
}

#[cfg(windows)]
fn windows_ie_proxy_server() -> Option<String> {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpGetIEProxyConfigForCurrentUser, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    };
    unsafe {
        let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG {
            fAutoDetect: 0,
            lpszAutoConfigUrl: std::ptr::null_mut(),
            lpszProxy: std::ptr::null_mut(),
            lpszProxyBypass: std::ptr::null_mut(),
        };
        if WinHttpGetIEProxyConfigForCurrentUser(&mut config) == 0 {
            return None;
        }
        let proxy = pwstr_to_string(config.lpszProxy);
        for ptr in [
            config.lpszAutoConfigUrl,
            config.lpszProxy,
            config.lpszProxyBypass,
        ] {
            if !ptr.is_null() {
                let _ = GlobalFree(ptr as *mut _);
            }
        }
        proxy
    }
}

#[cfg(windows)]
unsafe fn pwstr_to_string(ptr: windows_sys::core::PWSTR) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    if len == 0 {
        return None;
    }
    let slice = std::slice::from_raw_parts(ptr, len);
    let value = String::from_utf16_lossy(slice);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{partial_download_len, parse_ie_proxy_server, verify_file_digest};
    use std::path::PathBuf;
    use tokio::{fs, io::AsyncWriteExt};

    #[test]
    fn parses_bare_ie_proxy_as_http_and_https() {
        let (http, https) = parse_ie_proxy_server("proxy.example.com:8080");
        assert_eq!(http.as_deref(), Some("http://proxy.example.com:8080"));
        assert_eq!(https.as_deref(), Some("http://proxy.example.com:8080"));
    }

    #[test]
    fn parses_per_scheme_ie_proxy() {
        let (http, https) =
            parse_ie_proxy_server("http=10.0.0.1:80;https=10.0.0.1:443");
        assert_eq!(http.as_deref(), Some("http://10.0.0.1:80"));
        assert_eq!(https.as_deref(), Some("https://10.0.0.1:443"));
    }

    #[test]
    fn parses_https_only_ie_proxy_falls_back_to_http() {
        let (http, https) = parse_ie_proxy_server("https=proxy:8443");
        assert_eq!(https.as_deref(), Some("https://proxy:8443"));
        assert_eq!(http.as_deref(), Some("https://proxy:8443"));
    }

    #[tokio::test]
    async fn verifies_sha256_digest() {
        let path = PathBuf::from("target/test-digest.txt");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.unwrap();
        }
        let mut file = fs::File::create(&path).await.unwrap();
        file.write_all(b"mindustry").await.unwrap();
        file.flush().await.unwrap();

        verify_file_digest(
            &path,
            "sha256:b2117912f074f3dc461fd2a501ba055f68f6699b0926840afe415592769e57ad",
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn keeps_partial_download_when_inside_expected_size() {
        let path = PathBuf::from("target/test-partial.download");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.unwrap();
        }
        fs::write(&path, vec![1_u8; 32]).await.unwrap();

        let len = partial_download_len(&path, Some(64)).await.unwrap();

        assert_eq!(len, 32);
        assert!(path.exists());
        let _ = fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn drops_partial_download_larger_than_expected_size() {
        let path = PathBuf::from("target/test-oversized.download");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.unwrap();
        }
        fs::write(&path, vec![1_u8; 65]).await.unwrap();

        let len = partial_download_len(&path, Some(64)).await.unwrap();

        assert_eq!(len, 0);
        assert!(!path.exists());
    }
}
