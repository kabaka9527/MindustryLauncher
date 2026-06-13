use crate::{
    config::InstallLayout,
    debug_console,
    error::{AppError, AppResult},
    fs_util,
    models::{
        Accelerator, AcceleratorList, Settings, DEFAULT_ACCELERATOR_PREFIX, REMOTE_ACCELERATOR_LIST,
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
            Ok((Ok(remote), _)) => match validate_accelerator_list(remote) {
                Ok(list) => return Ok(ensure_required_sources(list)),
                Err(err) => last_error = Some(err),
            },
            Ok((Err(err), _)) => last_error = Some(err),
            Err(e) => last_error = Some(AppError::Network(e.to_string())),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        AppError::Network("no GitHub accelerator source could be loaded".to_string())
    }))
}

pub fn bundled_accelerators() -> AcceleratorList {
    match parse_bundled_accelerators() {
        Ok(list) => list,
        Err(err) => {
            debug_console::warn(format!(
                "包内 GitHub 加速源列表解析失败，使用内置默认值：{err}"
            ));
            AcceleratorList::default()
        }
    }
}

fn parse_bundled_accelerators() -> AppResult<AcceleratorList> {
    let list = serde_json::from_str::<AcceleratorList>(BUNDLED_ACCELERATORS_JSON)?;
    validate_accelerator_list(list)
}

fn validate_accelerator_list(list: AcceleratorList) -> AppResult<AcceleratorList> {
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
    list
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
                .find(|source| source.enabled_by_default)
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
        bundled_accelerators, discard_legacy_accelerator_cache, fetch_accelerator_list_from_urls,
        github_url_candidates, prefix_url, rewrite_github_url,
    };
    use crate::{
        config::InstallLayout,
        models::{AcceleratorList, Settings},
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
    fn prefixes_default_github_url() {
        let settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        let rewritten = rewrite_github_url(
            "https://api.github.com/repos/Anuken/Mindustry/releases",
            &settings,
            &AcceleratorList::default(),
        );
        assert_eq!(
            rewritten,
            "https://hubproxy.kabaka.xyz/https://api.github.com/repos/Anuken/Mindustry/releases"
        );
    }

    #[test]
    fn candidates_include_direct_fallback() {
        let settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        let urls = github_url_candidates(
            "https://api.github.com/repos/Anuken/Mindustry/releases",
            &settings,
            &AcceleratorList::default(),
        );
        assert_eq!(
            urls,
            vec![
                "https://hubproxy.kabaka.xyz/https://api.github.com/repos/Anuken/Mindustry/releases",
                "https://api.github.com/repos/Anuken/Mindustry/releases"
            ]
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
