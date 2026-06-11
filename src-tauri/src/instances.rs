use crate::{
    accelerators::rewrite_github_url,
    config::{self, InstallLayout},
    error::{AppError, AppResult},
    fs_util,
    models::{InstalledInstance, LaunchSettings, RemoteVersion, Settings, TaskEvent},
    network::NetworkClient,
    runtime, versions,
};
use chrono::Utc;
use regex::Regex;
use std::{fs, path::PathBuf};
use tauri::ipc::Channel;

// Installation creates two separate surfaces: an immutable game jar under
// versions/ and a mutable isolated runtime directory under instances/.
pub async fn install_version(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &crate::models::AcceleratorList,
    version: RemoteVersion,
    on_event: Channel<TaskEvent>,
) -> AppResult<InstalledInstance> {
    layout.ensure()?;
    let asset = versions::require_selected_asset(&version)?;
    let digest = asset.digest.as_deref();

    let safe_tag = safe_path_part(&version.tag);
    let channel_dir = layout.versions_dir.join(version.channel.as_id());
    let version_dir = channel_dir.join(&safe_tag);
    let jar_path = version_dir.join("Mindustry.jar");
    let instance_dir = layout
        .instances_dir
        .join(format!("{}-{safe_tag}", version.channel.as_id()));
    let data_dir = instance_dir.join("data");
    let log_dir = instance_dir.join("logs");

    let had_existing_instance = config::load_instances(layout)?
        .iter()
        .any(|item| item.id == version.id);

    fs_util::ensure_dir(&version_dir)?;
    fs_util::ensure_dir(&data_dir)?;
    fs_util::ensure_dir(&log_dir)?;

    let download_url = rewrite_github_url(&asset.download_url, settings, accelerators);
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let result = async {
        network
            .download_to_file(
                &download_url,
                &jar_path,
                digest,
                (asset.size > 0).then_some(asset.size),
                &format!("game:{}", version.id),
                &format!("下载 {}", version.name),
                on_event.clone(),
            )
            .await?;

        let required_java = runtime::required_java_from_jar(&jar_path).unwrap_or(17);
        let runtime_info =
            runtime::ensure_runtime(settings, layout, Some(required_java), on_event).await?;

        let instance = InstalledInstance {
            id: version.id.clone(),
            channel: version.channel,
            version: version.version.clone(),
            install_dir: instance_dir.to_string_lossy().to_string(),
            data_dir: data_dir.to_string_lossy().to_string(),
            jar_path: jar_path.to_string_lossy().to_string(),
            runtime_id: Some(runtime_info.id),
            installed_at: Utc::now().to_rfc3339(),
            launch_settings: LaunchSettings::default(),
        };

        let mut instances = config::load_instances(layout)?;
        instances.retain(|item| item.id != instance.id);
        instances.push(instance.clone());
        config::save_instances(layout, &instances)?;

        Ok(instance)
    }
    .await;

    if result.is_err() {
        if !had_existing_instance {
            cleanup_install_artifacts(layout, &instance_dir, &version_dir)?;
        } else {
            let failed_download = jar_path.with_extension("download");
            if failed_download.exists() {
                let _ = fs_util::remove_file_retry(&failed_download);
            }
            let _ = cleanup_partial_downloads(layout);
        }
    }

    result
}

pub async fn switch_version(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &crate::models::AcceleratorList,
    version: RemoteVersion,
    on_event: Channel<TaskEvent>,
) -> AppResult<InstalledInstance> {
    let old_instances: Vec<_> = config::load_instances(layout)?
        .into_iter()
        .filter(|item| item.channel == version.channel && item.id != version.id)
        .collect();

    let instance = install_version(settings, layout, accelerators, version, on_event).await?;

    for old in old_instances {
        delete_instance(layout, old.id)?;
    }

    Ok(instance)
}

pub fn delete_instance(
    layout: &InstallLayout,
    instance_id: String,
) -> AppResult<Vec<InstalledInstance>> {
    let mut instances = config::load_instances(layout)?;
    let instance = instances
        .iter()
        .find(|item| item.id == instance_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("instance {instance_id}")))?;

    let install_dir = PathBuf::from(&instance.install_dir);
    fs_util::assert_inside_root(&layout.root, &install_dir)?;
    if install_dir.exists() {
        fs_util::remove_dir_all_retry(&install_dir)?;
    }

    let jar_path = PathBuf::from(&instance.jar_path);
    if let Some(version_dir) = jar_path.parent() {
        fs_util::assert_inside_root(&layout.root, version_dir)?;
        if version_dir.exists() {
            fs_util::remove_dir_all_retry(version_dir)?;
        }
    }

    instances.retain(|item| item.id != instance_id);
    config::save_instances(layout, &instances)?;
    cleanup_partial_downloads(layout)?;
    Ok(instances)
}

pub fn cleanup_partial_downloads(layout: &InstallLayout) -> AppResult<()> {
    if layout.versions_dir.exists() {
        remove_download_files(&layout.root, &layout.versions_dir)?;
        remove_empty_dirs(&layout.root, &layout.versions_dir, &layout.versions_dir)?;
    }
    Ok(())
}

fn cleanup_install_artifacts(
    layout: &InstallLayout,
    instance_dir: &std::path::Path,
    version_dir: &std::path::Path,
) -> AppResult<()> {
    fs_util::assert_inside_root(&layout.root, instance_dir)?;
    if instance_dir.exists() {
        let _ = fs_util::remove_dir_all_retry(instance_dir);
    }
    fs_util::assert_inside_root(&layout.root, version_dir)?;
    if version_dir.exists() {
        let _ = fs_util::remove_dir_all_retry(version_dir);
    }
    Ok(())
}

fn remove_download_files(root: &std::path::Path, dir: &std::path::Path) -> AppResult<()> {
    fs_util::assert_inside_root(root, dir)?;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            remove_download_files(root, &path)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("download") {
            fs_util::assert_inside_root(root, &path)?;
            let _ = fs_util::remove_file_retry(&path);
        }
    }
    Ok(())
}

fn remove_empty_dirs(
    root: &std::path::Path,
    keep: &std::path::Path,
    dir: &std::path::Path,
) -> AppResult<bool> {
    fs_util::assert_inside_root(root, dir)?;
    let mut is_empty = true;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if !remove_empty_dirs(root, keep, &path)? {
                is_empty = false;
            }
        } else {
            is_empty = false;
        }
    }
    if is_empty && dir != keep {
        let _ = fs_util::remove_dir_all_retry(dir);
        return Ok(true);
    }
    Ok(false)
}

pub fn save_instance_launch_settings(
    layout: &InstallLayout,
    instance_id: String,
    runtime_id: Option<String>,
    launch_settings: LaunchSettings,
) -> AppResult<Vec<InstalledInstance>> {
    if let (Some(min), Some(max)) = (launch_settings.min_memory_mb, launch_settings.max_memory_mb) {
        if min > max {
            return Err(AppError::Invalid(
                "min memory cannot be greater than max memory".to_string(),
            ));
        }
    }

    let mut instances = config::load_instances(layout)?;
    let instance = instances
        .iter_mut()
        .find(|item| item.id == instance_id)
        .ok_or_else(|| AppError::NotFound(format!("instance {instance_id}")))?;
    instance.runtime_id = runtime_id.filter(|value| !value.trim().is_empty());
    instance.launch_settings = launch_settings;
    config::save_instances(layout, &instances)?;
    Ok(instances)
}

pub fn safe_path_part(value: &str) -> String {
    let regex = Regex::new(r#"[^A-Za-z0-9._-]+"#).expect("valid path sanitizer");
    let sanitized = regex.replace_all(value, "-");
    let sanitized = sanitized
        .trim_matches(|ch| ch == '-' || ch == '.')
        .to_string();
    if sanitized.is_empty() {
        "item".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::safe_path_part;

    #[test]
    fn sanitizes_tag_for_paths() {
        assert_eq!(safe_path_part("v8 Build 158.1"), "v8-Build-158.1");
        assert_eq!(safe_path_part("../26598"), "26598");
    }
}
