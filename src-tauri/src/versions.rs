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
use std::collections::HashSet;
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
    digest: Option<String>,
}

pub async fn refresh_versions(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &crate::models::AcceleratorList,
) -> AppResult<Vec<RemoteVersion>> {
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let instances = config::load_instances(layout)?;
    let mut versions = Vec::new();
    let mut errors = Vec::new();
    let mut refreshed_channels = HashSet::new();

    if settings
        .channel_visibility
        .is_visible(GameChannel::Mindustry, settings.show_be)
    {
        match fetch_channel(
            &network,
            settings,
            accelerators,
            GameChannel::Mindustry,
            "Anuken",
            "Mindustry",
            ReleaseFilter::Stable,
            &instances,
        )
        .await
        {
            Ok(items) => {
                refreshed_channels.insert(GameChannel::Mindustry);
                versions.extend(items);
            }
            Err(err) => errors.push(format!("Mindustry: {err}")),
        }
    }

    if settings
        .channel_visibility
        .is_visible(GameChannel::MindustryX, settings.show_be)
    {
        match fetch_channel(
            &network,
            settings,
            accelerators,
            GameChannel::MindustryX,
            "TinyLake",
            "MindustryX",
            ReleaseFilter::Stable,
            &instances,
        )
        .await
        {
            Ok(items) => {
                refreshed_channels.insert(GameChannel::MindustryX);
                versions.extend(items);
            }
            Err(err) => errors.push(format!("MindustryX: {err}")),
        }
    }

    if settings
        .channel_visibility
        .is_visible(GameChannel::MindustryBE, settings.show_be)
    {
        match fetch_channel(
            &network,
            settings,
            accelerators,
            GameChannel::MindustryBE,
            "Anuken",
            "MindustryBuilds",
            ReleaseFilter::Any,
            &instances,
        )
        .await
        {
            Ok(items) => {
                refreshed_channels.insert(GameChannel::MindustryBE);
                versions.extend(items);
            }
            Err(err) => errors.push(format!("Mindustry BE: {err}")),
        }
    }

    if settings
        .channel_visibility
        .is_visible(GameChannel::MindustryXBE, settings.show_be)
    {
        match fetch_channel(
            &network,
            settings,
            accelerators,
            GameChannel::MindustryXBE,
            "TinyLake",
            "MindustryX",
            ReleaseFilter::Prerelease,
            &instances,
        )
        .await
        {
            Ok(items) => {
                refreshed_channels.insert(GameChannel::MindustryXBE);
                versions.extend(items);
            }
            Err(err) => errors.push(format!("MindustryX BE: {err}")),
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
    let mut last_error = None;
    for candidate in github_url_candidates(&url, settings, accelerators) {
        match network.get_json::<Vec<GithubRelease>>(&candidate).await {
            Ok(releases) => {
                return Ok(releases
                    .into_iter()
                    .filter(|release| filter.matches(release.prerelease))
                    .filter_map(|release| map_release(channel, release, instances))
                    .collect());
            }
            Err(err) => last_error = Some(err),
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
    let mut versions = Vec::new();

    for release in releases
        .into_iter()
        .filter(|release| filter.matches(release.prerelease))
        .take(20)
    {
        let assets_url = expanded_assets_url(owner, repo, &release.tag)?;
        let assets_html = get_first_text(
            network,
            &github_url_candidates(&assets_url, settings, accelerators),
        )
        .await?;
        let assets = parse_expanded_assets(&assets_html);
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
    let mut last_error = None;
    for url in urls {
        match network.get_text_cached(url).await {
            Ok(text) => return Ok(text),
            Err(err) => last_error = Some(err),
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
    let entry_regex = Regex::new(r"(?s)<entry>(.*?)</entry>")
        .map_err(|err| AppError::Invalid(err.to_string()))?;
    let link_regex = Regex::new(&format!(
        r#"https://github\.com/{}/{}/releases/tag/([^"<]+)"#,
        regex::escape(owner),
        regex::escape(repo)
    ))
    .map_err(|err| AppError::Invalid(err.to_string()))?;
    let title_regex = Regex::new(r"(?s)<title>(.*?)</title>")
        .map_err(|err| AppError::Invalid(err.to_string()))?;
    let updated_regex = Regex::new(r"(?s)<updated>(.*?)</updated>")
        .map_err(|err| AppError::Invalid(err.to_string()))?;

    let mut releases = Vec::new();
    for entry in entry_regex.captures_iter(body) {
        let block = &entry[1];
        let Some(tag_match) = link_regex.captures(block).and_then(|caps| caps.get(1)) else {
            continue;
        };
        let tag = decode_html(tag_match.as_str());
        let title = title_regex
            .captures(block)
            .and_then(|caps| caps.get(1))
            .map(|value| decode_html(value.as_str()))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| tag.clone());
        let published_at = updated_regex
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
    let regex = Regex::new(r#"href="([^"]+?\.jar)""#).expect("valid asset regex");
    let mut seen = HashSet::new();
    let mut assets = Vec::new();
    for captures in regex.captures_iter(body) {
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
            digest: asset.digest,
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
    use super::select_desktop_jar;
    use crate::models::ReleaseAsset;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            size: 1,
            download_url: format!("https://github.com/example/{name}"),
            digest: None,
        }
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
