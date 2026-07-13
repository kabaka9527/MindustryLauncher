use crate::{
    config::InstallLayout,
    debug_console,
    error::{AppError, AppResult},
    fs_util,
    models::{
        Accelerator, AcceleratorList, PingResult, Settings, DEFAULT_ACCELERATOR_PREFIX,
        REMOTE_ACCELERATOR_LIST,
    },
    network::NetworkClient,
};
use std::{fs, io::ErrorKind};

const BUNDLED_ACCELERATORS_JSON: &str = include_str!("../../resources/github-accelerators.json");
const REQUIRED_ACCELERATOR_IDS: &[&str] = &["hubproxy-kabaka", "direct"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GithubTarget {
    Api,
    Raw,
    ReleaseAsset,
}

pub fn load_startup_accelerators(layout: &InstallLayout) -> AppResult<AcceleratorList> {
    discard_legacy_accelerator_cache(layout)?;
    let list = bundled_accelerators();
    debug_console::info(format!(
        "GitHub 加速源已使用包内启动列表：{} 个",
        list.sources.len()
    ));
    Ok(list)
}

pub async fn refresh_accelerators(
    settings: &Settings,
    layout: &InstallLayout,
) -> AppResult<AcceleratorList> {
    discard_legacy_accelerator_cache(layout)?;
    match fetch_remote_accelerators(settings, layout).await {
        Ok(list) => {
            debug_console::info(format!(
                "GitHub 加速源已使用远端列表：{} 个",
                list.sources.len()
            ));
            Ok(list)
        }
        Err(err) => {
            debug_console::warn(format!("GitHub 加速源远端获取失败，使用包内列表：{err}"));
            Ok(bundled_accelerators())
        }
    }
}

async fn fetch_remote_accelerators(
    settings: &Settings,
    layout: &InstallLayout,
) -> AppResult<AcceleratorList> {
    let accelerated_url = prefix_url(DEFAULT_ACCELERATOR_PREFIX, REMOTE_ACCELERATOR_LIST);
    fetch_accelerator_list_from_urls(
        settings,
        layout,
        vec![accelerated_url, REMOTE_ACCELERATOR_LIST.to_string()],
    )
    .await
}

async fn fetch_accelerator_list_from_urls(
    settings: &Settings,
    layout: &InstallLayout,
    urls: Vec<String>,
) -> AppResult<AcceleratorList> {
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let mut set = tokio::task::JoinSet::new();
    for url in urls {
        let network = network.clone();
        set.spawn(async move {
            (
                network.get_json_uncached::<AcceleratorList>(&url).await,
                url,
            )
        });
    }

    let mut last_error = None;
    while let Some(res) = set.join_next().await {
        match res {
            Ok((Ok(remote), _)) => {
            if remote.sources.is_empty() {
                last_error = Some(AppError::Invalid(
                    "GitHub accelerator list has no sources".to_string(),
                ));
            } else {
                return Ok(ensure_required_sources(remote));
            }
        }
            Ok((Err(err), _)) => last_error = Some(err),
            Err(e) => last_error = Some(AppError::Network(e.to_string())),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        AppError::Network("no GitHub accelerator source could be loaded".to_string())
    }))
}

pub fn bundled_accelerators() -> AcceleratorList {
    let list = match parse_bundled_accelerators() {
        Ok(list) => list,
        Err(err) => {
            debug_console::warn(format!(
                "包内 GitHub 加速源列表解析失败，使用内置默认值：{err}"
            ));
            AcceleratorList::default()
        }
    };
    sort_by_priority(list)
}

/// 按优先级升序排序：数值越小越靠前（1 为最高优先级）。
pub fn sort_by_priority(mut list: AcceleratorList) -> AcceleratorList {
    list.sources.sort_by_key(|source| source.priority);
    list
}

fn parse_bundled_accelerators() -> AppResult<AcceleratorList> {
    let list = serde_json::from_str::<AcceleratorList>(BUNDLED_ACCELERATORS_JSON)?;
    if list.sources.is_empty() {
        return Err(AppError::Invalid(
            "GitHub accelerator list has no sources".to_string(),
        ));
    }
    Ok(list)
}

fn ensure_required_sources(mut list: AcceleratorList) -> AcceleratorList {
    let bundled = bundled_accelerators();
    for required_id in REQUIRED_ACCELERATOR_IDS {
        if list.sources.iter().any(|source| source.id == *required_id) {
            continue;
        }
        if let Some(source) = bundled
            .sources
            .iter()
            .find(|source| source.id == *required_id)
            .cloned()
        {
            list.sources.push(source);
        }
    }
    sort_by_priority(list)
}

fn discard_legacy_accelerator_cache(layout: &InstallLayout) -> AppResult<()> {
    let path = layout.legacy_accelerators_path();
    fs_util::assert_inside_root(&layout.root, &path)?;
    match fs::remove_file(&path) {
        Ok(()) => {
            debug_console::info(format!("已删除旧 GitHub 加速源缓存：{}", path.display()));
            Ok(())
        }
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::Io(err.to_string())),
    }
}

// 通用健康检查：以后端随时下发的加速源列表为准，不依赖各源硬编码的 healthCheckUrl。
// 优先使用源显式指定的健康检查地址；否则根据该源自身的 baseUrl / rules / supports
// 自动推导一个稳定且轻量的探测目标（优先 Raw 小文件，其次 Release / API），
// 从而适配未来新增或更新的加速源。
const HEALTH_PROBE_TARGETS: &[(&str, GithubTarget)] = &[
    (
        "https://raw.githubusercontent.com/kabaka9527/MindustryLauncher/main/README.md",
        GithubTarget::Raw,
    ),
    (
        "https://github.com/kabaka9527/MindustryLauncher",
        GithubTarget::ReleaseAsset,
    ),
    (
        "https://api.github.com/repos/kabaka9527/MindustryLauncher",
        GithubTarget::Api,
    ),
];

// 推导某加速源的健康检查地址：显式覆盖优先，缺失时按源自身重写规则自动生成。
pub fn derive_health_check_url(source: &Accelerator) -> Option<String> {
    if let Some(url) = source
        .health_check_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(url.to_string());
    }
    let (target, _) = HEALTH_PROBE_TARGETS
        .iter()
        .find(|(_, target)| supports_target(source, *target))?;
    Some(apply_source_rewrite(source, target))
}

// 按单个加速源自身的 baseUrl / rules 重写目标 URL（等价于 rewrite_github_url 的单源逻辑）。
fn apply_source_rewrite(source: &Accelerator, original: &str) -> String {
    if !source.base_url.trim().is_empty() {
        return prefix_url(&source.base_url, original);
    }
    for rule in &source.rules {
        if original.starts_with(&rule.from) {
            return original.replacen(&rule.from, &rule.to, 1);
        }
    }
    original.to_string()
}

// 生成加速源的健康检查候选地址（按优先级）：显式 healthCheckUrl 优先，
// 其余为该源支持的通用探测目标（Raw -> Release -> Api）。依次尝试，
// 确保即使某类目标（如镜像不代理 raw）不可达，也能回退到其他可用目标。
fn health_check_candidates(source: &Accelerator) -> Vec<String> {
    let mut candidates = Vec::new();
    // 首个候选为显式地址或自动推导地址（Raw 优先），其余为该源支持的其他通用目标作为回退。
    if let Some(url) = derive_health_check_url(source) {
        candidates.push(url);
    }
    for (target, github_target) in HEALTH_PROBE_TARGETS {
        if supports_target(source, *github_target) {
            let url = apply_source_rewrite(source, *target);
            if !candidates.iter().any(|existing| existing == &url) {
                candidates.push(url);
            }
        }
    }
    candidates
}

pub async fn ping_source(
    settings: &Settings,
    layout: &InstallLayout,
    source: Accelerator,
) -> AppResult<PingResult> {
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    // 依次尝试候选地址，任一成功即返回延迟；全部失败才记为不可达（详见调试日志）。
    for url in health_check_candidates(&source) {
        debug_console::info(format!(
            "[加速源健康检测] 探测源 {} 地址 {}",
            source.id, url
        ));
        match network.probe_latency(&url).await {
            Ok(ms) => {
                debug_console::info(format!(
                    "[加速源健康检测] 源 {} 探测成功，延迟 {}ms",
                    source.id, ms
                ));
                return Ok(PingResult {
                    source_id: source.id,
                    latency_ms: Some(ms),
                    error: None,
                });
            }
            Err(err) => {
                debug_console::warn(format!(
                    "[加速源健康检测] 源 {} 候选地址探测失败：{}",
                    source.id, err
                ));
            }
        }
    }
    debug_console::error(format!(
        "[加速源健康检测] 源 {} 所有候选地址均探测失败",
        source.id
    ));
    Ok(PingResult {
        source_id: source.id,
        latency_ms: None,
        error: Some("健康检查失败：所有候选地址均不可达".to_string()),
    })
}

pub fn rewrite_github_url(
    original_url: &str,
    settings: &Settings,
    accelerators: &AcceleratorList,
) -> String {
    let Some(target) = classify_github_url(original_url) else {
        return original_url.to_string();
    };

    if let Some(prefix) = settings
        .github_proxy_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return prefix_url(prefix, original_url);
    }

    let selected = settings
        .selected_accelerator_id
        .as_deref()
        .and_then(|id| accelerators.sources.iter().find(|source| source.id == id))
        .or_else(|| {
            accelerators
                .sources
                .iter()
                .min_by_key(|source| source.priority)
        });

    if let Some(source) = selected {
        if !supports_target(source, target) {
            return original_url.to_string();
        }
        for rule in &source.rules {
            if original_url.starts_with(&rule.from) {
                return original_url.replacen(&rule.from, &rule.to, 1);
            }
        }
        return prefix_url(&source.base_url, original_url);
    }

    original_url.to_string()
}

pub fn github_url_candidates(
    original_url: &str,
    settings: &Settings,
    accelerators: &AcceleratorList,
) -> Vec<String> {
    let rewritten = rewrite_github_url(original_url, settings, accelerators);
    let mut urls = vec![rewritten];
    if urls.first().map(String::as_str) != Some(original_url) {
        urls.push(original_url.to_string());
    }
    urls
}

pub fn prefix_url(prefix: &str, original_url: &str) -> String {
    format!(
        "{}/{}",
        prefix.trim_end_matches('/'),
        original_url.trim_start_matches('/')
    )
}

fn classify_github_url(url: &str) -> Option<GithubTarget> {
    if url.starts_with("https://api.github.com/") {
        Some(GithubTarget::Api)
    } else if url.starts_with("https://raw.githubusercontent.com/") {
        Some(GithubTarget::Raw)
    } else if url.starts_with("https://github.com/") {
        Some(GithubTarget::ReleaseAsset)
    } else {
        None
    }
}

fn supports_target(source: &Accelerator, target: GithubTarget) -> bool {
    match target {
        GithubTarget::Api => source.supports.api,
        GithubTarget::Raw => source.supports.raw,
        GithubTarget::ReleaseAsset => source.supports.release_asset,
    }
}

#[cfg(test)]
mod tests {
use super::{
    bundled_accelerators, derive_health_check_url, discard_legacy_accelerator_cache,
    fetch_accelerator_list_from_urls, github_url_candidates, prefix_url, rewrite_github_url,
};
    use crate::{
        config::InstallLayout,
        models::{Accelerator, AcceleratorList, AcceleratorSupports, Settings},
    };
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        path::PathBuf,
        thread,
        time::{Duration, Instant},
    };

    const TEST_ACCELERATOR_LIST_JSON: &str = r#"{
      "version": 7,
      "updatedAt": "2026-06-13T00:00:00Z",
      "sources": [
        {
          "id": "fast-local",
          "name": "Fast Local Mirror",
          "baseUrl": "https://fast.example/",
          "rules": [],
          "supports": {
            "api": true,
            "raw": true,
            "releaseAsset": true
          },
          "healthCheckUrl": "https://fast.example/https://github.com/",
          "enabledByDefault": true
        }
      ]
    }"#;

    #[test]
    fn bundled_accelerators_include_required_sources() {
        let list = bundled_accelerators();

        assert!(list
            .sources
            .iter()
            .any(|source| source.id == "hubproxy-kabaka"));
        assert!(list.sources.iter().any(|source| source.id == "direct"));
    }

    #[test]
    fn missing_legacy_accelerator_cache_is_ignored() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("test-missing-accelerators-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let layout = InstallLayout::new(root.clone());
        layout.ensure().unwrap();

        assert!(!layout.legacy_accelerators_path().exists());
        discard_legacy_accelerator_cache(&layout).unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn fetches_and_parses_remote_list_without_waiting_for_slow_candidate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "test-fetch-accelerators-speed-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&root);
        let layout = InstallLayout::new(root.clone());
        layout.ensure().unwrap();

        let slow_url = spawn_json_server(TEST_ACCELERATOR_LIST_JSON, Duration::from_millis(1_200));
        let fast_url = spawn_json_server(TEST_ACCELERATOR_LIST_JSON, Duration::ZERO);
        let settings = Settings::with_install_root(root.to_string_lossy().to_string());

        let started = Instant::now();
        let list = fetch_accelerator_list_from_urls(&settings, &layout, vec![slow_url, fast_url])
            .await
            .unwrap();
        let elapsed = started.elapsed();
        let display_names: Vec<&str> = list
            .sources
            .iter()
            .map(|source| source.name.as_str())
            .collect();

        assert!(
            elapsed < Duration::from_millis(900),
            "accelerator fetch and display parsing took {elapsed:?}"
        );
        assert!(display_names.contains(&"Fast Local Mirror"));
        assert!(list
            .sources
            .iter()
            .any(|source| source.id == "hubproxy-kabaka"));
        assert!(list.sources.iter().any(|source| source.id == "direct"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn default_accelerator_is_direct_connection() {
        let settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        assert_eq!(settings.selected_accelerator_id.as_deref(), Some("direct"));
        let rewritten = rewrite_github_url(
            "https://api.github.com/repos/Anuken/Mindustry/releases",
            &settings,
            &AcceleratorList::default(),
        );
        assert_eq!(
            rewritten,
            "https://api.github.com/repos/Anuken/Mindustry/releases"
        );
    }

    #[test]
    fn candidates_keep_original_for_direct_default() {
        let settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        let urls = github_url_candidates(
            "https://api.github.com/repos/Anuken/Mindustry/releases",
            &settings,
            &AcceleratorList::default(),
        );
        assert_eq!(
            urls,
            vec!["https://api.github.com/repos/Anuken/Mindustry/releases"]
        );
    }

    #[test]
    fn direct_accelerator_keeps_original_url() {
        let mut settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        settings.selected_accelerator_id = Some("direct".to_string());
        let rewritten = rewrite_github_url(
            "https://github.com/Anuken/Mindustry/releases/download/v158/Mindustry.jar",
            &settings,
            &bundled_accelerators(),
        );
        assert_eq!(
            rewritten,
            "https://github.com/Anuken/Mindustry/releases/download/v158/Mindustry.jar"
        );
    }

    #[test]
    fn explicit_prefix_wins() {
        let mut settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        settings.github_proxy_prefix = Some("https://mirror.example".to_string());
        let rewritten = rewrite_github_url(
            "https://github.com/Anuken/Mindustry/releases/download/v158/Mindustry.jar",
            &settings,
            &AcceleratorList::default(),
        );
        assert_eq!(
            rewritten,
            "https://mirror.example/https://github.com/Anuken/Mindustry/releases/download/v158/Mindustry.jar"
        );
    }

    #[test]
    fn prefix_trims_slashes() {
        assert_eq!(
            prefix_url("https://a.example/", "/https://github.com/test"),
            "https://a.example/https://github.com/test"
        );
    }

    #[test]
    fn derive_health_check_url_uses_explicit_when_present() {
        let source = Accelerator {
            id: "mirror-x".to_string(),
            name: "Mirror X".to_string(),
            base_url: "https://mirror-x.example/".to_string(),
            rules: vec![],
            supports: AcceleratorSupports {
                api: true,
                raw: true,
                release_asset: true,
            },
            health_check_url: Some("https://mirror-x.example/healthz".to_string()),
            enabled_by_default: false,
            priority: 100,
        };
        assert_eq!(
            derive_health_check_url(&source),
            Some("https://mirror-x.example/healthz".to_string())
        );
    }

    #[test]
    fn derive_health_check_url_falls_back_when_absent() {
        let source = Accelerator {
            id: "mirror-x".to_string(),
            name: "Mirror X".to_string(),
            base_url: "https://mirror-x.example/".to_string(),
            rules: vec![],
            supports: AcceleratorSupports {
                api: true,
                raw: true,
                release_asset: true,
            },
            health_check_url: None,
            enabled_by_default: false,
            priority: 100,
        };
        assert_eq!(
            derive_health_check_url(&source),
            Some(
                "https://mirror-x.example/https://raw.githubusercontent.com/kabaka9527/MindustryLauncher/main/README.md"
                    .to_string()
            )
        );
    }

    #[test]
    fn derive_health_check_url_uses_rules_when_no_base_url() {
        let source = Accelerator {
            id: "mirror-rules".to_string(),
            name: "Mirror Rules".to_string(),
            base_url: String::new(),
            rules: vec![crate::models::AcceleratorRule {
                from: "https://raw.githubusercontent.com/".to_string(),
                to: "https://rules.example/raw/".to_string(),
            }],
            supports: AcceleratorSupports {
                api: false,
                raw: true,
                release_asset: false,
            },
            health_check_url: None,
            enabled_by_default: false,
            priority: 100,
        };
        assert_eq!(
            derive_health_check_url(&source),
            Some(
                "https://rules.example/raw/kabaka9527/MindustryLauncher/main/README.md"
                    .to_string()
            )
        );
    }

    fn spawn_json_server(body: &'static str, delay: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0_u8; 1024];
                let _ = stream.read(&mut buffer);
                if !delay.is_zero() {
                    thread::sleep(delay);
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        format!("http://{address}/github-accelerators.json")
    }
}
