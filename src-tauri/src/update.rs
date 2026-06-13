use crate::{
    accelerators::github_url_candidates,
    config::InstallLayout,
    debug_console,
    error::AppResult,
    models::{AcceleratorList, LauncherUpdateInfo, Settings},
    network::NetworkClient,
};
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

const LAUNCHER_OWNER: &str = "kabaka9527";
const LAUNCHER_REPO: &str = "MindustryLauncher";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn parse_semver(version: &str) -> Option<(u32, u32, u32)> {
    let version = version.trim().trim_start_matches('v');
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let major = parts[0].parse::<u32>().ok()?;
    let minor = parts[1].parse::<u32>().ok()?;
    let patch = parts[2].parse::<u32>().ok()?;
    Some((major, minor, patch))
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    match (parse_semver(latest), parse_semver(current)) {
        (Some((lm, ln, lp)), Some((cm, cn, cp))) => {
            (lm, ln, lp) > (cm, cn, cp)
        }
        _ => false,
    }
}

/// Parse the latest release tag from the GitHub releases Atom feed as a fallback.
fn parse_latest_release_from_atom(atom: &str) -> Option<(String, String)> {
    fn entry_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?s)<entry>(.*?)</entry>").expect("valid entry regex"))
    }
    fn title_regex() -> &'static Regex {
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new(r"(?s)<title>(.*?)</title>").expect("valid title regex"))
    }
    let link_pattern = format!(
        r#"https://github\.com/{}/{}/releases/tag/([^"<]+)"#,
        regex::escape(LAUNCHER_OWNER),
        regex::escape(LAUNCHER_REPO)
    );
    let link_regex = Regex::new(&link_pattern).ok()?;

    let entry_re = entry_regex();
    let title_re = title_regex();
    let mut latest_tag: Option<String> = None;
    let mut latest_title: Option<String> = None;

    for entry_cap in entry_re.captures_iter(atom) {
        let entry = &entry_cap[1];
        let tag = match link_regex
            .captures(entry)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
        {
            Some(t) => t,
            None => continue,
        };
        let title = title_re
            .captures(entry)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_else(|| tag.clone());
        match &latest_tag {
            Some(prev) if !version_is_newer(&tag, prev) => {}
            _ => {
                latest_tag = Some(tag);
                latest_title = Some(title);
            }
        }
    }

    latest_tag.map(|tag| {
        let title = latest_title.unwrap_or_else(|| tag.clone());
        (tag, title)
    })
}

pub async fn check_launcher_update(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &AcceleratorList,
) -> AppResult<LauncherUpdateInfo> {
    let current = current_version().to_string();
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;

    let url = format!(
        "https://api.github.com/repos/{LAUNCHER_OWNER}/{LAUNCHER_REPO}/releases/latest"
    );
    let candidates = github_url_candidates(&url, settings, accelerators);

    // Primary path: GitHub API JSON
    let mut last_error = None;
    for candidate in &candidates {
        match network
            .get_json::<GithubRelease>(candidate)
            .await
        {
            Ok(release) => {
                let latest = release.tag_name.trim().trim_start_matches('v').to_string();
                let has_update = version_is_newer(&latest, &current);
                if has_update {
                    debug_console::info(format!(
                        "检测到启动器更新：{current} -> {latest}"
                    ));
                }
                return Ok(LauncherUpdateInfo {
                    current_version: current,
                    latest_version: latest,
                    has_update,
                    release_url: release.html_url,
                    release_body: release.body.unwrap_or_default(),
                    error_message: None,
                });
            }
            Err(err) => {
                debug_console::warn(format!("启动器更新检测失败 ({candidate}): {err}"));
                last_error = Some(err);
            }
        }
    }

    // Fallback: parse Atom feed
    debug_console::info("API 路径失败，尝试 Atom feed 兜底".to_string());
    let atom_url = format!(
        "https://github.com/{LAUNCHER_OWNER}/{LAUNCHER_REPO}/releases.atom"
    );
    let atom_candidates = github_url_candidates(&atom_url, settings, accelerators);
    for candidate in &atom_candidates {
        match network.get_text_cached(candidate).await {
            Ok(atom) => {
                if let Some((tag, _title)) = parse_latest_release_from_atom(&atom) {
                    let latest = tag.trim().trim_start_matches('v').to_string();
                    let has_update = version_is_newer(&latest, &current);
                    if has_update {
                        debug_console::info(format!(
                            "通过 Atom feed 检测到启动器更新：{current} -> {latest}"
                        ));
                    }
                    let release_url = format!(
                        "https://github.com/{LAUNCHER_OWNER}/{LAUNCHER_REPO}/releases/tag/{}",
                        tag
                    );
                    return Ok(LauncherUpdateInfo {
                        current_version: current,
                        latest_version: latest,
                        has_update,
                        release_url,
                        release_body: String::new(),
                        error_message: None,
                    });
                }
            }
            Err(err) => {
                debug_console::warn(format!("Atom feed 获取失败 ({candidate}): {err}"));
            }
        }
    }

    // All paths failed — report the error instead of silently pretending no update
    let error_msg = last_error
        .as_ref()
        .map(|e| format!("更新检测失败：{e}"))
        .unwrap_or_else(|| "更新检测失败：无法获取版本信息".to_string());
    debug_console::warn(error_msg.clone());

    Ok(LauncherUpdateInfo {
        current_version: current.clone(),
        latest_version: current,
        has_update: false,
        release_url: String::new(),
        release_body: String::new(),
        error_message: Some(error_msg),
    })
}
