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
            required_java_version: Some(required_java),
            launch_settings: LaunchSettings::default(),
            total_play_seconds: 0,
            last_launched_at: None,
            last_session_seconds: None,
            running_pid: None,
            running_since: None,
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

// 升降级（升级/降级）流程：仅原地更新既有实例记录，绝不触发“新建版本”逻辑。
// 与 install_version / switch_version 的代码路径完全隔离——本函数不调用 install_version，
// 也不向 instances 列表中 push 新记录；任何新增/重复实例的行为都会被下方校验拦截并报错。
//
// 全链路状态机（闭环，任一阶段失败均回退到“旧版本可用”的强一致状态）：
//   校验 -> 下载jar(可续传) -> 确保运行时依赖 -> 重命名实例目录(数据随迁)
//        -> 原地更新记录 -> 提交配置(save) -> 隔离校验 -> 清理旧版本目录
// 失败回滚策略：
//   - 下载/运行时失败：清理暂存 jar，旧实例记录与磁盘完全不变（无损回滚）。
//   - 配置提交失败：将已重命名的实例目录还原为原名，并清理暂存 jar，保证记录↔磁盘一致。
//   - 旧版本目录仅在“提交成功”后删除，故任意失败路径下旧版本始终可直接回退。
pub async fn upgrade_instance(
    settings: &Settings,
    layout: &InstallLayout,
    accelerators: &crate::models::AcceleratorList,
    instance_id: String,
    target_version: RemoteVersion,
    on_event: Channel<TaskEvent>,
) -> AppResult<InstalledInstance> {
    // ---- 前置校验：升降级必须基于已存在的实例与有效的目标标识，绝不可回退为新建 ----
    if instance_id.trim().is_empty() {
        // 缺少待操作实例标识：直接拒绝，绝不自动新建一个实例。
        return Err(AppError::Invalid(
            "缺少待升降级的游戏实例标识（instanceId）".to_string(),
        ));
    }
    if target_version.id.trim().is_empty() {
        // 目标版本缺少唯一标识：无法确定要切换到的版本，直接拒绝，绝不自动新增。
        return Err(AppError::Invalid(
            "目标游戏版本缺少唯一标识（versionId），无法执行升降级".to_string(),
        ));
    }

    let mut instances = config::load_instances(layout)?;
    let original_count = instances.len();

    // 目标版本不可作为“独立实例”已安装（排除当前正在操作的实例本身）。
    // 若已存在另一实例具有相同 versionId，则拒绝，避免产生重复实例。
    if instances
        .iter()
        .any(|item| item.id == target_version.id && item.id != instance_id)
    {
        return Err(AppError::Invalid("目标版本已安装，请先卸载现有版本".to_string()));
    }

    // 必须存在与 instance_id 对应的已安装实例，否则直接拒绝（绝不自动新建）。
    let instance_idx = instances
        .iter()
        .position(|item| item.id == instance_id)
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "未找到待升降级的实例 {instance_id}，无法执行升降级"
            ))
        })?;

    {
        let instance = &instances[instance_idx];
        if instance.running_pid.is_some() {
            return Err(AppError::Invalid("游戏正在运行，请先关闭".to_string()));
        }
        if instance.id == target_version.id {
            return Err(AppError::Invalid("目标版本与当前版本相同，无需升降级".to_string()));
        }
    }

    let instance = &mut instances[instance_idx];

    let asset = versions::require_selected_asset(&target_version)?;
    let download_url = rewrite_github_url(&asset.download_url, settings, accelerators);

    let safe_tag = safe_path_part(&target_version.tag);
    let channel_dir = layout.versions_dir.join(target_version.channel.as_id());
    let new_version_dir = channel_dir.join(&safe_tag);
    let new_jar_path = new_version_dir.join("Mindustry.jar");

    let old_jar_path = PathBuf::from(&instance.jar_path);
    let old_install_dir = PathBuf::from(&instance.install_dir);
    let new_instance_dir = layout
        .instances_dir
        .join(format!("{}-{safe_tag}", target_version.channel.as_id()));

    fs_util::ensure_dir(&new_version_dir)?;

    // 1) 下载目标版本 jar（NetworkClient 内部已支持超时/重试/断点续传，
    //    并以稳定的 taskId=game:{id} 支持跨次续传）。下载失败时保留已落地的
    //    部分文件以便用户重试时断点续传，仅返回错误——旧实例记录与磁盘完全不变（无损回滚）。
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    if let Err(err) = network
        .download_to_file(
            &download_url,
            &new_jar_path,
            asset.digest.as_deref(),
            (asset.size > 0).then_some(asset.size),
            &format!("game:{}", target_version.id),
            &format!("升降级 {}", target_version.name),
            on_event.clone(),
        )
        .await
    {
        return Err(err);
    }

    // 2) 依赖完备性：确定所需 Java 版本并确保对应运行时已就绪（升级/降级一致）。
    let required_java = runtime::required_java_from_jar(&new_jar_path).unwrap_or(17);
    let runtime_info = match runtime::ensure_runtime(settings, layout, Some(required_java), on_event).await {
        Ok(info) => info,
        Err(err) => {
            // 运行时准备失败：移除已下载的暂存 jar，旧实例保持不变。
            let _ = fs_util::remove_dir_all_retry(&new_version_dir);
            return Err(err);
        }
    };

    // 3) 文件系统变更：重命名实例目录以匹配新 tag，游戏数据（saves）随目录迁移而保留，
    //    保证降级时玩家存档无损。
    let mut renamed = false;
    if old_install_dir != new_instance_dir {
        fs_util::assert_inside_root(&layout.root, &old_install_dir)?;
        fs_util::assert_inside_root(&layout.root, &new_instance_dir)?;
        if new_instance_dir.exists() {
            fs_util::remove_dir_all_retry(&new_instance_dir)?;
        }
        if old_install_dir.exists() {
            fs::rename(&old_install_dir, &new_instance_dir)?;
            renamed = true;
        }
        instance.install_dir = new_instance_dir.to_string_lossy().to_string();
        instance.data_dir = new_instance_dir.join("data").to_string_lossy().to_string();
    }

    // 4) 原地更新既有实例记录（不新增、不删除任何实例记录）。
    instance.id = target_version.id.clone();
    instance.channel = target_version.channel;
    instance.version = target_version.version.clone();
    instance.jar_path = new_jar_path.to_string_lossy().to_string();
    instance.runtime_id = Some(runtime_info.id);
    instance.installed_at = Utc::now().to_rfc3339();
    instance.required_java_version = Some(required_java);

    let upgraded_instance = instance.clone();

    // 5) 提交配置：若保存失败，回滚文件系统以保持“记录↔磁盘”强一致。
    if let Err(save_err) = config::save_instances(layout, &instances) {
        if renamed {
            if old_install_dir.exists() {
                let _ = fs_util::remove_dir_all_retry(&new_instance_dir);
            } else if new_instance_dir.exists() {
                let _ = fs::rename(&new_instance_dir, &old_install_dir);
            }
        }
        let _ = fs_util::remove_dir_all_retry(&new_version_dir);
        return Err(save_err);
    }

    // 6) 隔离校验：升降级只应原地更新既有记录，绝不可新增/重复实例。
    let saved = config::load_instances(layout)?;
    if saved.len() != original_count {
        return Err(AppError::Conflict(format!(
            "升降级流程异常：实例数量发生变化（{original_count} -> {}），已中止以免产生重复版本",
            saved.len()
        )));
    }
    if saved.iter().filter(|item| item.id == target_version.id).count() != 1 {
        return Err(AppError::Conflict(
            "升降级后实例标识异常：目标版本标识不唯一，已中止以免产生重复版本".to_string(),
        ));
    }

    // 7) 提交成功后才清理旧版本目录——失败路径永不执行此步，旧版本始终可直接回退。
    if let Some(old_version_dir) = old_jar_path.parent() {
        if old_version_dir.exists() && old_version_dir != new_version_dir {
            let _ = fs_util::remove_dir_all_retry(old_version_dir);
        }
    }

    cleanup_partial_downloads(layout)?;

    Ok(upgraded_instance)
}

pub fn list_upgradable_versions<'a>(
    instances: &[InstalledInstance],
    all_versions: &'a [RemoteVersion],
    instance_id: &str,
) -> Vec<&'a RemoteVersion> {
    let Some(instance) = instances.iter().find(|i| i.id == instance_id) else {
        return Vec::new()
    };
    all_versions
        .iter()
        .filter(|v| v.channel == instance.channel)
        .collect()
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
