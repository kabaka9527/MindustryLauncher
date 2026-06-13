use crate::{
    accelerators::github_url_candidates,
    config::InstallLayout,
    debug_console,
    error::AppResult,
    models::{AcceleratorList, LauncherUpdateInfo, Settings},
    network::NetworkClient,
};
use serde::Deserialize;

const LAUNCHER_OWNER: &str = "kabaka9527";
const LAUNCHER_REPO: &str = "MindustryLauncher";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
                });
            }
            Err(err) => {
                debug_console::warn(format!("启动器更新检测失败 ({candidate}): {err}"));
                last_error = Some(err);
            }
        }
    }

    if let Some(err) = last_error {
        debug_console::warn(format!("启动器更新检测最终失败：{err}"));
    }

    Ok(LauncherUpdateInfo {
        current_version: current.clone(),
        latest_version: current,
        has_update: false,
        release_url: String::new(),
        release_body: String::new(),
    })
}
