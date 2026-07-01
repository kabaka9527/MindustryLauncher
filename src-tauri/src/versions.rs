use crate::{
    accelerators::github_url_candidates,
    config::{self, InstallLayout},
    error::{AppError, AppResult},
    fs_util,
    models::{GameChannel, ReleaseAsset, RemoteVersion, Settings},
    network::NetworkClient,
};
use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use url::Url;

// Version discovery is intentionally GitHub-release based. Each source is
// mapped into a common RemoteVersion shape so the frontend can treat channels uniformly.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

pub async fn refresh_versions(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &crate::models::AcceleratorList,
    scope: VersionRefreshScope,
) -> AppResult<Vec<RemoteVersion>> {
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let instances = config::load_instances(layout)?;

    // Build a list of channel fetch tasks. Each channel is fetched in parallel
    // so the total wall-clock time is bounded by the slowest channel instead of
    // the sum of all channels.
    #[derive(Debug)]
    struct ChannelResult {
        channel: GameChannel,
        result: Result<Vec<RemoteVersion>, String>,
    }

    let mut handles = Vec::new();

    for spec in channel_fetch_specs(settings, scope) {
        let network = network.clone();
        let settings = settings.clone();
        let accelerators = accelerators.clone();
        let instances = instances.clone();
        handles.push(tokio::spawn(async move {
            let result = fetch_channel(
                &network,
                &settings,
                &accelerators,
                spec.channel,
                spec.owner,
                spec.repo,
                spec.filter,
                &instances,
            )
            .await;
            ChannelResult {
                channel: spec.channel,
                result: match result {
                    Ok(items) => Ok(items),
                    Err(err) => Err(format!("{}: {err}", spec.label)),
                },
            }
        }));
    }

    // Await all spawned tasks and collect results.
    let mut versions = Vec::new();
    let mut errors = Vec::new();
    let mut refreshed_channels = HashSet::new();

    for handle in handles {
        match handle.await {
            Ok(ChannelResult {
                channel,
                result: Ok(items),
            }) => {
                refreshed_channels.insert(channel);
                versions.extend(items);
            }
            Ok(ChannelResult {
                result: Err(err), ..
            }) => {
                errors.push(err);
            }
            Err(join_err) => {
                errors.push(format!("channel task panicked: {join_err}"));
            }
        }
    }

    if versions.is_empty() && !errors.is_empty() {
        return Err(AppError::Network(errors.join("; ")));
    }

    let mut merged = load_cached_versions(layout)?;
    if !refreshed_channels.is_empty() {
        merged.retain(|version| !refreshed_channels.contains(&version.channel));
        merged.extend(versions);
    }

    fs_util::write_json(&layout.versions_cache_path(), &merged)?;
    Ok(merged)
}

pub fn load_cached_versions(layout: &InstallLayout) -> AppResult<Vec<RemoteVersion>> {
    Ok(fs_util::read_json(&layout.versions_cache_path())?.unwrap_or_default())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VersionRefreshScope {
    All,
}

impl VersionRefreshScope {
    fn includes(self, _settings: &Settings, _channel: GameChannel) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy)]
enum ReleaseFilter {
    Stable,
    Prerelease,
    Any,
}

impl ReleaseFilter {
    fn matches(self, prerelease: bool) -> bool {
        match self {
            ReleaseFilter::Stable => !prerelease,
            ReleaseFilter::Prerelease => prerelease,
            ReleaseFilter::Any => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ChannelFetchSpec {
    channel: GameChannel,
    owner: &'static str,
    repo: &'static str,
    filter: ReleaseFilter,
    label: &'static str,
}

fn channel_fetch_specs(settings: &Settings, scope: VersionRefreshScope) -> Vec<ChannelFetchSpec> {
    [
        ChannelFetchSpec {
            channel: GameChannel::Mindustry,
            owner: "Anuken",
            repo: "Mindustry",
            filter: ReleaseFilter::Stable,
            label: "Mindustry",
        },
        ChannelFetchSpec {
            channel: GameChannel::MindustryX,
            owner: "TinyLake",
            repo: "MindustryX",
            filter: ReleaseFilter::Stable,
            label: "MindustryX",
        },
        ChannelFetchSpec {
            channel: GameChannel::MindustryBE,
            owner: "Anuken",
            repo: "MindustryBuilds",
            filter: ReleaseFilter::Any,
            label: "Mindustry BE",
        },
        ChannelFetchSpec {
            channel: GameChannel::MindustryXBE,
            owner: "TinyLake",
            repo: "MindustryX",
            filter: ReleaseFilter::Prerelease,
            label: "MindustryX BE",
        },
    ]
    .into_iter()
    .filter(|spec| scope.includes(settings, spec.channel))
    .collect()
}

async fn fetch_channel(
    network: &NetworkClient,
    settings: &Settings,
    accelerators: &crate::models::AcceleratorList,
    channel: GameChannel,
    owner: &str,
    repo: &str,
    filter: ReleaseFilter,
    instances: &[crate::models::InstalledInstance],
) -> AppResult<Vec<RemoteVersion>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=20");
    let candidates = github_url_candidates(&url, settings, accelerators);
    let mut last_error = None;

    if candidates.len() == 1 {
        match network.get_json::<Vec<GithubRelease>>(&candidates[0]).await {
            Ok(releases) => {
                return Ok(releases
                    .into_iter()
                    .filter(|release| filter.matches(release.prerelease))
                    .filter_map(|release| map_release(channel, release, instances))
                    .collect());
            }
            Err(err) => last_error = Some(err),
        }
    } else if !candidates.is_empty() {
        let mut set = tokio::task::JoinSet::new();
        for candidate in candidates {
            let network = network.clone();
            set.spawn(async move { network.get_json::<Vec<GithubRelease>>(&candidate).await });
        }
        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(releases)) => {
                    return Ok(releases
                        .into_iter()
                        .filter(|release| filter.matches(release.prerelease))
                        .filter_map(|release| map_release(channel, release, instances))
                        .collect());
                }
                Ok(Err(err)) => last_error = Some(err),
                Err(e) => last_error = Some(AppError::Network(e.to_string())),
            }
        }
    }

    match fetch_channel_from_release_pages(
        network,
        settings,
        accelerators,
        channel,
        owner,
        repo,
        filter,
        instances,
    )
    .await
    {
        Ok(items) if !items.is_empty() => Ok(items),
        Ok(_) => Err(last_error.unwrap_or_else(|| {
            AppError::NotFound(format!("no releases found for {owner}/{repo}"))
        })),
        Err(err) => Err(last_error.unwrap_or(err)),
    }
}

async fn fetch_channel_from_release_pages(
    network: &NetworkClient,
    settings: &Settings,
    accelerators: &crate::models::AcceleratorList,
    channel: GameChannel,
    owner: &str,
    repo: &str,
    filter: ReleaseFilter,
    instances: &[crate::models::InstalledInstance],
) -> AppResult<Vec<RemoteVersion>> {
    let atom_url = format!("https://github.com/{owner}/{repo}/releases.atom");
    let atom = get_first_text(
        network,
        &github_url_candidates(&atom_url, settings, accelerators),
    )
    .await?;
    let releases = parse_releases_atom(owner, repo, &atom)?;
    let filtered: Vec<AtomRelease> = releases
        .into_iter()
        .filter(|release| filter.matches(release.prerelease))
        .take(20)
        .collect();

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    // Fetch all expanded_assets pages concurrently instead of sequentially.
    let mut handles = Vec::with_capacity(filtered.len());
    for release in &filtered {
        let assets_url = expanded_assets_url(owner, repo, &release.tag)?;
        let candidates = github_url_candidates(&assets_url, settings, accelerators);
        let network = network.clone();
        let tag = release.tag.clone();
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            match get_first_text(&network, &candidates).await {
                Ok(html) => (tag, parse_expanded_assets(&html)),
                Err(_) => (tag, Vec::new()),
            }
        }));
    }

    let mut assets_by_tag = HashMap::new();
    for handle in handles {
        if let Ok((tag, assets)) = handle.await {
            assets_by_tag.insert(tag, assets);
        }
    }

    let mut versions = Vec::new();
    for release in filtered {
        let assets = assets_by_tag.remove(&release.tag).unwrap_or_default();
        let selected_asset = select_desktop_jar(&assets);
        if selected_asset.is_none() {
            continue;
        }
        let id = version_id(channel, &release.tag);
        let installed = instances.iter().any(|instance| instance.id == id);
        versions.push(RemoteVersion {
            id,
            channel,
            channel_label: channel.label().to_string(),
            version: human_version(&release.tag),
            tag: release.tag.clone(),
            name: release.name,
            prerelease: release.prerelease,
            published_at: release.published_at,
            assets,
            selected_asset,
            installed,
        });
    }

    Ok(versions)
}

async fn get_first_text(network: &NetworkClient, urls: &[String]) -> AppResult<String> {
    if urls.is_empty() {
        return Err(AppError::Network("no URL candidates".to_string()));
    }
    if urls.len() == 1 {
        return network.get_text_cached(&urls[0]).await;
    }

    let mut set = tokio::task::JoinSet::new();
    for url in urls {
        let network = network.clone();
        let url = url.clone();
        set.spawn(async move { network.get_text_cached(&url).await });
    }

    let mut last_error = None;
    while let Some(res) = set.join_next().await {
        match res {
            Ok(Ok(text)) => return Ok(text),
            Ok(Err(err)) => last_error = Some(err),
            Err(e) => last_error = Some(AppError::Network(e.to_string())),
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::Network("no URL candidates".to_string())))
}

#[derive(Debug, Clone)]
struct AtomRelease {
    tag: String,
    name: String,
    prerelease: bool,
    published_at: Option<String>,
}

fn parse_releases_atom(owner: &str, repo: &str, body: &str) -> AppResult<Vec<AtomRelease>> {
    fn entry_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?s)<entry>(.*?)</entry>").expect("valid entry regex"))
    }
    fn title_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?s)<title>(.*?)</title>").expect("valid title regex"))
    }
    fn updated_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?s)<updated>(.*?)</updated>").expect("valid updated regex"))
    }

    let link_pattern = format!(
        r#"https://github\.com/{}/{}/releases/tag/([^"<]+)"#,
        regex::escape(owner),
        regex::escape(repo)
    );
    let link_regex = Regex::new(&link_pattern).map_err(|err| AppError::Invalid(err.to_string()))?;

    let mut releases = Vec::new();
    for entry in entry_regex().captures_iter(body) {
        let block = &entry[1];
        let Some(tag_match) = link_regex.captures(block).and_then(|caps| caps.get(1)) else {
            continue;
        };
        let tag = decode_html(tag_match.as_str());
        let title = title_regex()
            .captures(block)
            .and_then(|caps| caps.get(1))
            .map(|value| decode_html(value.as_str()))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| tag.clone());
        let published_at = updated_regex()
            .captures(block)
            .and_then(|caps| caps.get(1))
            .map(|value| decode_html(value.as_str()));
        let lower_tag = tag.to_ascii_lowercase();
        releases.push(AtomRelease {
            tag,
            name: title,
            prerelease: lower_tag.contains("pre")
                || lower_tag.contains("alpha")
                || lower_tag.contains("beta"),
            published_at,
        });
    }

    Ok(releases)
}

fn expanded_assets_url(owner: &str, repo: &str, tag: &str) -> AppResult<String> {
    let mut url = Url::parse(&format!(
        "https://github.com/{owner}/{repo}/releases/expanded_assets"
    ))?;
    url.path_segments_mut()
        .map_err(|_| AppError::Invalid("cannot build GitHub assets URL".to_string()))?
        .push(tag);
    Ok(url.to_string())
}

fn parse_expanded_assets(body: &str) -> Vec<ReleaseAsset> {
    fn jar_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r#"href="([^"]+?\.jar)""#).expect("valid asset regex"))
    }

    let mut seen = HashSet::new();
    let mut assets = Vec::new();
    for captures in jar_regex().captures_iter(body) {
        let href = decode_html(&captures[1]);
        let download_url = if href.starts_with("https://") {
            href
        } else {
            format!("https://github.com{href}")
        };
        if !seen.insert(download_url.clone()) {
            continue;
        }
        let name = download_url
            .split('/')
            .next_back()
            .filter(|value| !value.is_empty())
            .unwrap_or("Mindustry.jar")
            .to_string();
        assets.push(ReleaseAsset {
            name,
            size: 0,
            download_url,
            digest: None,
        });
    }
    assets
}

fn decode_html(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn map_release(
    channel: GameChannel,
    release: GithubRelease,
    instances: &[crate::models::InstalledInstance],
) -> Option<RemoteVersion> {
    let assets: Vec<ReleaseAsset> = release
        .assets
        .into_iter()
        .map(|asset| ReleaseAsset {
            name: asset.name,
            size: asset.size,
            download_url: asset.browser_download_url,
            digest: None,
        })
        .collect();
    let selected_asset = select_desktop_jar(&assets)?;
    let id = version_id(channel, &release.tag_name);
    let installed = instances.iter().any(|instance| instance.id == id);
    Some(RemoteVersion {
        id,
        channel,
        channel_label: channel.label().to_string(),
        version: human_version(&release.tag_name),
        tag: release.tag_name.clone(),
        name: release
            .name
            .unwrap_or_else(|| format!("{} {}", channel.label(), release.tag_name)),
        prerelease: release.prerelease,
        published_at: release.published_at,
        assets,
        selected_asset: Some(selected_asset),
        installed,
    })
}

pub fn version_id(channel: GameChannel, tag: &str) -> String {
    format!("{}:{tag}", channel.as_id())
}

fn human_version(tag: &str) -> String {
    tag.trim_start_matches('v').to_string()
}

pub fn select_desktop_jar(assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
    assets
        .iter()
        .filter(|asset| {
            let lower = asset.name.to_ascii_lowercase();
            lower.ends_with(".jar")
                && !lower.contains("server")
                && !lower.contains("android")
                && !lower.contains("source")
                && !lower.contains("javadoc")
        })
        .max_by_key(|asset| jar_score(&asset.name))
        .cloned()
}

fn jar_score(name: &str) -> u8 {
    let lower = name.to_ascii_lowercase();
    if lower == "mindustry.jar" {
        100
    } else if lower.contains("desktop") {
        90
    } else if lower.contains("mindustry") {
        80
    } else {
        10
    }
}

pub fn require_selected_asset(version: &RemoteVersion) -> AppResult<ReleaseAsset> {
    version
        .selected_asset
        .clone()
        .or_else(|| select_desktop_jar(&version.assets))
        .ok_or_else(|| AppError::NotFound(format!("no desktop jar asset for {}", version.id)))
}

#[cfg(test)]
mod tests {
    use super::{channel_fetch_specs, select_desktop_jar, VersionRefreshScope};
    use crate::models::{ChannelVisibility, GameChannel, ReleaseAsset, Settings};

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            size: 1,
            download_url: format!("https://github.com/example/{name}"),
            digest: None,
        }
    }

    fn channels_for(settings: &Settings, scope: VersionRefreshScope) -> Vec<GameChannel> {
        channel_fetch_specs(settings, scope)
            .into_iter()
            .map(|spec| spec.channel)
            .collect()
    }

    #[test]
    fn all_refresh_scope_includes_every_channel() {
        let settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());

        assert_eq!(
            channels_for(&settings, VersionRefreshScope::All),
            vec![
                GameChannel::Mindustry,
                GameChannel::MindustryX,
                GameChannel::MindustryBE,
                GameChannel::MindustryXBE
            ]
        );
    }

    #[test]
    fn all_refresh_scope_ignores_disabled_visibility() {
        let mut settings = Settings::with_install_root("D:/MindustryLauncher/app-data".to_string());
        settings.show_be = false;
        settings.channel_visibility = ChannelVisibility {
            mindustry: false,
            mindustry_x: false,
            mindustry_be: false,
            mindustry_xbe: false,
        };

        assert_eq!(
            channels_for(&settings, VersionRefreshScope::All),
            vec![
                GameChannel::Mindustry,
                GameChannel::MindustryX,
                GameChannel::MindustryBE,
                GameChannel::MindustryXBE
            ]
        );
    }

    #[test]
    fn selects_desktop_jar_and_excludes_server() {
        let selected = select_desktop_jar(&[
            asset("server-release.jar"),
            asset("Mindustry-BE-Desktop-26598.jar"),
            asset("android-release.apk"),
        ])
        .unwrap();
        assert_eq!(selected.name, "Mindustry-BE-Desktop-26598.jar");
    }

    #[test]
    fn prefers_official_mindustry_jar() {
        let selected = select_desktop_jar(&[asset("desktop.jar"), asset("Mindustry.jar")]).unwrap();
        assert_eq!(selected.name, "Mindustry.jar");
    }
}
