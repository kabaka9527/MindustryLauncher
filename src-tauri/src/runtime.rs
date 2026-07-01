use crate::{
    config::{self, InstallLayout},
    debug_console,
    error::{AppError, AppResult},
    fs_util,
    instances::safe_path_part,
    models::{RemoteRuntime, RuntimeInfo, RuntimeSource, Settings, TaskEvent},
    network::NetworkClient,
};
use chrono::Utc;
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
};
use tauri::ipc::Channel;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const TUNA_ADOPTIUM_ROOT: &str = "https://mirrors.tuna.tsinghua.edu.cn/Adoptium";

// Runtime provisioning uses Adoptium metadata for checksum/source truth, then
// rewrites the package URL to TUNA. By requirement, only JRE zip archives are accepted.
pub async fn ensure_runtime(
    settings: &Settings,
    layout: &InstallLayout,
    java_version: Option<u16>,
    on_event: Channel<TaskEvent>,
) -> AppResult<RuntimeInfo> {
    let java_version = java_version.unwrap_or(17);
    let os = adoptium_os();
    let arch = adoptium_arch();
    let runtime_id = format!("jre-{java_version}-{os}-{arch}");

    let runtimes = config::load_runtimes(layout)?;
    if let Some(runtime) = runtimes.iter().find(|item| item.id == runtime_id) {
        if runtime.enabled && Path::new(&runtime.java_path).exists() {
            return Ok(runtime.clone());
        }
    }
    if let Some(runtime) = runtimes
        .iter()
        .filter(|item| {
            item.enabled && item.java_version >= java_version && Path::new(&item.java_path).exists()
        })
        .min_by_key(|item| item.java_version)
    {
        return Ok(runtime.clone());
    }

    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let package = resolve_jre_zip(&network, java_version, os, arch).await?;
    install_jre_package(layout, &network, package, on_event).await
}

pub async fn list_remote_runtimes(
    settings: &Settings,
    layout: &InstallLayout,
) -> AppResult<Vec<RemoteRuntime>> {
    let runtimes = fetch_remote_runtimes(settings, layout).await?;
    if let Err(err) = save_cached_remote_runtimes(layout, &runtimes) {
        debug_console::warn(format!("远端运行时列表缓存失败：{err}"));
    }
    Ok(runtimes)
}

async fn fetch_remote_runtimes(
    settings: &Settings,
    layout: &InstallLayout,
) -> AppResult<Vec<RemoteRuntime>> {
    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    let root = network
        .get_text_cached(&format!("{TUNA_ADOPTIUM_ROOT}/"))
        .await?;
    let versions = parse_tuna_java_versions(&root)?;
    let os = adoptium_os();
    let arch = adoptium_arch();
    let mut runtimes = Vec::new();

    for java_version in versions {
        let url = format!("{TUNA_ADOPTIUM_ROOT}/{java_version}/jre/{arch}/{os}/");
        let Ok(body) = network.get_text_cached(&url).await else {
            continue;
        };
        let mut packages = parse_tuna_runtime_packages(java_version, os, arch, &url, &body)?;
        for package in &mut packages {
            if package.checksum.is_none() {
                package.checksum =
                    resolve_jre_checksum(&network, java_version, os, arch, &package.file_name)
                        .await
                        .ok();
            }
        }
        runtimes.extend(packages);
    }

    runtimes.sort_by(|a, b| b.java_version.cmp(&a.java_version));
    Ok(runtimes)
}

pub fn load_cached_remote_runtimes(layout: &InstallLayout) -> AppResult<Vec<RemoteRuntime>> {
    Ok(fs_util::read_json(&layout.remote_runtimes_cache_path())?.unwrap_or_default())
}

fn save_cached_remote_runtimes(layout: &InstallLayout, runtimes: &Vec<RemoteRuntime>) -> AppResult<()> {
    fs_util::write_json(&layout.remote_runtimes_cache_path(), runtimes)
}

pub async fn install_runtime(
    settings: &Settings,
    layout: &InstallLayout,
    runtime: RemoteRuntime,
    on_event: Channel<TaskEvent>,
) -> AppResult<RuntimeInfo> {
    let checksum = match runtime.checksum.clone() {
        Some(value) => value,
        None => {
            let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
            resolve_jre_checksum(
                &network,
                runtime.java_version,
                &runtime.os,
                &runtime.arch,
                &runtime.file_name,
            )
            .await?
        }
    };

    let network = NetworkClient::new(settings, layout.cache_dir.join("http"))?;
    install_jre_package(
        layout,
        &network,
        JrePackage {
            java_version: runtime.java_version,
            version: runtime.version,
            os: runtime.os,
            arch: runtime.arch,
            file_name: runtime.file_name,
            checksum,
            tuna_url: runtime.download_url,
            size_bytes: runtime.size_bytes,
        },
        on_event,
    )
    .await
}

pub fn import_runtime(layout: &InstallLayout, source: String) -> AppResult<RuntimeInfo> {
    layout.ensure()?;
    let source = PathBuf::from(source.trim());
    if source.as_os_str().is_empty() {
        return Err(AppError::Invalid(
            "runtime import path cannot be empty".to_string(),
        ));
    }

    let java_path = if source.is_file() {
        source.clone()
    } else {
        find_java_binary_limited(&source, 5).ok_or_else(|| {
            AppError::NotFound(format!("java executable under {}", source.display()))
        })?
    };
    let runtime_root = runtime_root_from_java(&java_path)?;
    let details = detect_java_details(&java_path)?;
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let runtime_id = format!("imported-jre-{}-{timestamp}", details.java_version);

    let (runtime_dir, imported_java_path) = if runtime_root.starts_with(&layout.runtimes_dir) {
        (runtime_root.clone(), java_path.clone())
    } else {
        let destination = layout.runtimes_dir.join(&runtime_id);
        fs_util::copy_dir_recursive(&runtime_root, &destination)?;
        let relative_java = java_path.strip_prefix(&runtime_root).map_err(|_| {
            AppError::Invalid(format!(
                "java executable is not inside {}",
                runtime_root.display()
            ))
        })?;
        (destination.clone(), destination.join(relative_java))
    };

    let runtime = RuntimeInfo {
        id: runtime_id.clone(),
        java_version: details.java_version,
        version: Some(details.version),
        os: adoptium_os().to_string(),
        arch: adoptium_arch().to_string(),
        path: runtime_dir.to_string_lossy().to_string(),
        java_path: imported_java_path.to_string_lossy().to_string(),
        installed: true,
        enabled: true,
        source: RuntimeSource::Imported,
    };

    let mut runtimes = config::load_runtimes(layout)?;
    runtimes.retain(|item| item.java_path != runtime.java_path);
    runtimes.push(runtime.clone());
    config::save_runtimes(layout, &runtimes)?;
    Ok(runtime)
}

pub fn scan_runtimes(layout: &InstallLayout, source: String) -> AppResult<Vec<RuntimeInfo>> {
    layout.ensure()?;
    let source = PathBuf::from(source.trim());
    if source.as_os_str().is_empty() {
        return Err(AppError::Invalid(
            "runtime scan path cannot be empty".to_string(),
        ));
    }
    if !source.exists() {
        return Err(AppError::NotFound(format!("path {}", source.display())));
    }

    let candidates = if source.is_file() {
        if is_java_binary(&source) {
            vec![source.clone()]
        } else {
            Vec::new()
        }
    } else {
        find_java_binaries_limited(&source, 7, 32)
    };

    let found = register_runtime_candidates(layout, candidates, RuntimeSource::Scanned)?;

    if found.is_empty() {
        return Err(AppError::NotFound(format!(
            "no Java runtime was found under {}",
            source.display()
        )));
    }

    Ok(found)
}

pub fn scan_system_runtimes(layout: &InstallLayout) -> AppResult<Vec<RuntimeInfo>> {
    let mut candidates = Vec::new();
    for var in ["JAVA_HOME", "JRE_HOME"] {
        if let Some(path) = env::var_os(var).map(PathBuf::from) {
            candidates.push(path.join("bin").join(java_binary_name()));
            candidates.push(path.join(java_binary_name()));
        }
    }
    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            candidates.push(dir.join(java_binary_name()));
        }
    }
    register_runtime_candidates(layout, candidates, RuntimeSource::System)
}

pub fn set_runtime_enabled(
    layout: &InstallLayout,
    runtime_id: String,
    enabled: bool,
) -> AppResult<Vec<RuntimeInfo>> {
    let mut runtimes = config::load_runtimes(layout)?;
    let runtime = runtimes
        .iter_mut()
        .find(|runtime| runtime.id == runtime_id)
        .ok_or_else(|| AppError::NotFound(format!("runtime {runtime_id}")))?;
    runtime.enabled = enabled;
    config::save_runtimes(layout, &runtimes)?;
    Ok(runtimes)
}

pub fn delete_runtime(layout: &InstallLayout, runtime_id: String) -> AppResult<Vec<RuntimeInfo>> {
    let mut runtimes = config::load_runtimes(layout)?;
    let runtime = runtimes
        .iter()
        .find(|runtime| runtime.id == runtime_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("runtime {runtime_id}")))?;

    if runtime.source != RuntimeSource::Launcher {
        return Err(AppError::Invalid(
            "only launcher-managed runtimes can be deleted".to_string(),
        ));
    }

    let runtime_dir = PathBuf::from(&runtime.path);
    fs_util::assert_inside_root(&layout.root, &runtime_dir)?;
    if runtime_dir.exists() {
        fs_util::remove_dir_all_retry(&runtime_dir)?;
    }

    runtimes.retain(|runtime| runtime.id != runtime_id);
    config::save_runtimes(layout, &runtimes)?;

    let mut instances = config::load_instances(layout)?;
    let mut changed = false;
    for instance in &mut instances {
        if instance.runtime_id.as_deref() == Some(&runtime_id) {
            instance.runtime_id = None;
            changed = true;
        }
    }
    if changed {
        config::save_instances(layout, &instances)?;
    }

    Ok(runtimes)
}

fn register_runtime_candidates(
    layout: &InstallLayout,
    candidates: Vec<PathBuf>,
    source: RuntimeSource,
) -> AppResult<Vec<RuntimeInfo>> {
    layout.ensure()?;
    let mut runtimes = config::load_runtimes(layout)?;
    let mut seen_java_paths: HashSet<String> = runtimes
        .iter()
        .map(|runtime| runtime.java_path.clone())
        .collect();
    let mut seen_candidates = HashSet::new();
    let mut found = Vec::new();

    for java_path in candidates {
        if !java_path.exists() || !is_java_binary(&java_path) {
            continue;
        }
        let java_path = java_path.canonicalize().unwrap_or(java_path);
        let java_path_string = java_path.to_string_lossy().to_string();
        if !seen_candidates.insert(java_path_string.clone()) {
            continue;
        }
        if !seen_java_paths.insert(java_path_string.clone()) {
            if let Some(existing) = runtimes
                .iter()
                .find(|runtime| runtime.java_path == java_path_string)
                .cloned()
            {
                found.push(existing);
            }
            continue;
        }
        let Ok(runtime_root) = runtime_root_from_java(&java_path) else {
            continue;
        };
        let Ok(details) = detect_java_details(&java_path) else {
            continue;
        };

        let runtime = RuntimeInfo {
            id: runtime_id_for_source(source, details.java_version, &runtime_root),
            java_version: details.java_version,
            version: Some(details.version),
            os: adoptium_os().to_string(),
            arch: adoptium_arch().to_string(),
            path: runtime_root.to_string_lossy().to_string(),
            java_path: java_path_string,
            installed: true,
            enabled: true,
            source,
        };
        runtimes.push(runtime.clone());
        found.push(runtime);
    }

    if !found.is_empty() {
        config::save_runtimes(layout, &runtimes)?;
    }
    Ok(found)
}

fn runtime_id_for_source(source: RuntimeSource, java_version: u16, runtime_root: &Path) -> String {
    let prefix = match source {
        RuntimeSource::System => "system-jre",
        RuntimeSource::Scanned => "local-jre",
        RuntimeSource::Imported => "imported-jre",
        RuntimeSource::Launcher | RuntimeSource::Unknown => "local-jre",
    };
    let mut hasher = Sha256::new();
    hasher.update(runtime_root.to_string_lossy().as_bytes());
    let digest = hex_encode(&hasher.finalize());
    format!("{prefix}-{java_version}-{}", &digest[..10])
}

#[derive(Debug, Clone)]
struct JrePackage {
    java_version: u16,
    version: String,
    os: String,
    arch: String,
    file_name: String,
    checksum: String,
    tuna_url: String,
    size_bytes: Option<u64>,
}

async fn install_jre_package(
    layout: &InstallLayout,
    network: &NetworkClient,
    package: JrePackage,
    on_event: Channel<TaskEvent>,
) -> AppResult<RuntimeInfo> {
    let runtime_id = format!(
        "jre-{}-{}-{}",
        package.java_version, package.os, package.arch
    );
    let runtime_dir = layout.runtimes_dir.join(&runtime_id);
    let archive_path = layout
        .tmp_downloads_dir
        .join(safe_path_part(&package.file_name));
    let task_id = format!("runtime:{runtime_id}");

    fs_util::ensure_dir(&layout.tmp_downloads_dir)?;
    if runtime_dir.exists() {
        fs_util::remove_dir_all_retry(&runtime_dir)?;
    }
    fs_util::ensure_dir(&runtime_dir)?;

    let result = async {
        network
            .download_to_file(
                &package.tuna_url,
                &archive_path,
                Some(&package.checksum),
                package.size_bytes,
                &task_id,
                &format!("下载 JRE {}", package.java_version),
                on_event.clone(),
            )
            .await?;

        let _ = on_event.send(TaskEvent::Started {
            task_id: task_id.clone(),
            label: format!("解压 JRE {}", package.java_version),
            total_bytes: None,
            message: Some("解压中".to_string()),
        });
        extract_zip(&archive_path, &runtime_dir)?;
        let java_path = find_java_binary(&runtime_dir).ok_or_else(|| {
            AppError::NotFound(format!(
                "java executable was not found after extracting {}",
                archive_path.display()
            ))
        })?;

        let runtime = RuntimeInfo {
            id: runtime_id.clone(),
            java_version: package.java_version,
            version: Some(package.version),
            os: package.os,
            arch: package.arch,
            path: runtime_dir.to_string_lossy().to_string(),
            java_path: java_path.to_string_lossy().to_string(),
            installed: true,
            enabled: true,
            source: RuntimeSource::Launcher,
        };

        let mut runtimes = config::load_runtimes(layout)?;
        runtimes.retain(|item| item.id != runtime_id);
        runtimes.push(runtime.clone());
        config::save_runtimes(layout, &runtimes)?;
        let _ = on_event.send(TaskEvent::Finished {
            task_id,
            message: format!("JRE {} 已准备", package.java_version),
        });
        Ok(runtime)
    }
    .await;

    if result.is_err() {
        let _ = fs_util::remove_dir_all_retry(&runtime_dir);
        let _ = fs_util::remove_file_retry(&archive_path);
    } else {
        let _ = fs_util::remove_file_retry(&archive_path);
    }

    result
}

fn parse_tuna_java_versions(body: &str) -> AppResult<Vec<u16>> {
    let regex = Regex::new(r#"<a href="(\d+)/" title="\d+">\d+/</a>"#)
        .map_err(|err| AppError::Invalid(err.to_string()))?;
    let mut versions = Vec::new();
    for captures in regex.captures_iter(body) {
        if let Ok(value) = captures[1].parse::<u16>() {
            versions.push(value);
        }
    }
    versions.sort_unstable();
    versions.dedup();
    Ok(versions)
}

fn parse_tuna_runtime_packages(
    java_version: u16,
    os: &str,
    arch: &str,
    base_url: &str,
    body: &str,
) -> AppResult<Vec<RemoteRuntime>> {
    let size_regex = Regex::new(r#"<td class="size">([^<]*)</td>"#)
        .map_err(|err| AppError::Invalid(err.to_string()))?;
    let date_regex = Regex::new(r#"<td class="date">([^<]*)</td>"#)
        .map_err(|err| AppError::Invalid(err.to_string()))?;
    let mut runtimes = Vec::new();
    for chunk in body.split("href=\"").skip(1) {
        let Some((href, tail)) = chunk.split_once('"') else {
            continue;
        };
        if !href.ends_with(".zip") {
            continue;
        }
        let href = decode_html(href);
        let file_name = href
            .split('/')
            .next_back()
            .filter(|value| !value.is_empty())
            .unwrap_or(&href)
            .to_string();
        let lower = file_name.to_ascii_lowercase();
        if !lower.contains("-jre_") {
            continue;
        }
        let version =
            package_version_from_file_name(&file_name).unwrap_or_else(|| java_version.to_string());
        let id = format!(
            "jre-{java_version}-{os}-{arch}-{}",
            safe_path_part(&version)
        );
        let size_label = size_regex
            .captures(tail)
            .and_then(|value| value.get(1))
            .map(|value| decode_html(value.as_str()))
            .unwrap_or_default();
        let size_bytes = parse_size_label(&size_label);
        let updated_at = date_regex
            .captures(tail)
            .and_then(|value| value.get(1))
            .map(|value| decode_html(value.as_str()))
            .unwrap_or_default();
        runtimes.push(RemoteRuntime {
            id,
            java_version,
            version,
            os: os.to_string(),
            arch: arch.to_string(),
            file_name: file_name.clone(),
            size_label,
            size_bytes,
            updated_at,
            download_url: format!("{}/{}", base_url.trim_end_matches('/'), href),
            checksum: None,
        });
    }
    Ok(runtimes)
}

fn package_version_from_file_name(file_name: &str) -> Option<String> {
    file_name
        .strip_suffix(".zip")
        .and_then(|value| value.rsplit("_hotspot_").next())
        .map(ToOwned::to_owned)
}

fn parse_size_label(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let amount = parts.next()?.parse::<f64>().ok()?;
    let unit = parts.next()?.to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" => 1.0,
        "kib" | "kb" => 1024.0,
        "mib" | "mb" => 1024.0 * 1024.0,
        "gib" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((amount * multiplier) as u64)
}

async fn resolve_jre_checksum(
    network: &NetworkClient,
    java_version: u16,
    os: &str,
    arch: &str,
    file_name: &str,
) -> AppResult<String> {
    let api = format!(
        "https://api.adoptium.net/v3/assets/latest/{java_version}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse&heap_size=normal"
    );
    let value = network.get_json::<Value>(&api).await?;
    let array = value.as_array().ok_or_else(|| {
        AppError::Invalid("Adoptium API returned a non-array response".to_string())
    })?;

    for item in array {
        let Some(package) = item.get("binary").and_then(|binary| binary.get("package")) else {
            continue;
        };
        if package.get("name").and_then(Value::as_str) == Some(file_name) {
            return package
                .get("checksum")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or_else(|| AppError::Invalid(format!("{file_name} has no checksum")));
        }
    }

    Err(AppError::NotFound(format!(
        "checksum for {file_name} was not found in Adoptium metadata"
    )))
}

fn runtime_root_from_java(java_path: &Path) -> AppResult<PathBuf> {
    if java_path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        == Some("bin")
    {
        java_path
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Invalid(format!("invalid java path {}", java_path.display())))
    } else {
        java_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Invalid(format!("invalid java path {}", java_path.display())))
    }
}

#[derive(Debug, Clone)]
struct JavaRuntimeDetails {
    java_version: u16,
    version: String,
}

fn detect_java_details(java_path: &Path) -> AppResult<JavaRuntimeDetails> {
    let mut command = std::process::Command::new(java_path);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_command_window(&mut command);
    let output = command.output().map_err(|err| {
        AppError::Command(format!("failed to run {}: {err}", java_path.display()))
    })?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_java_runtime_details(&text)
        .ok_or_else(|| AppError::Invalid(format!("cannot parse Java version from {text}")))
}

fn parse_java_runtime_details(text: &str) -> Option<JavaRuntimeDetails> {
    let regex = Regex::new(r#"(?:openjdk|java) version "([^"]+)""#).ok()?;
    let version = regex.captures(text)?.get(1)?.as_str();
    let java_version = if let Some(rest) = version.strip_prefix("1.") {
        rest.split('.').next()?.parse().ok()?
    } else {
        version.split('.').next()?.parse().ok()?
    };
    Some(JavaRuntimeDetails {
        java_version,
        version: version.to_string(),
    })
}

#[cfg(windows)]
fn hide_command_window(command: &mut std::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_command_window(_command: &mut std::process::Command) {}

fn find_java_binary_limited(root: &Path, max_depth: usize) -> Option<PathBuf> {
    let binary = if cfg!(windows) { "java.exe" } else { "java" };
    find_file_limited(root, binary, max_depth)
}

fn find_java_binaries_limited(root: &Path, max_depth: usize, max_items: usize) -> Vec<PathBuf> {
    let mut items = Vec::new();
    collect_java_binaries_limited(root, max_depth, max_items, &mut items);
    items
}

fn collect_java_binaries_limited(
    root: &Path,
    max_depth: usize,
    max_items: usize,
    items: &mut Vec<PathBuf>,
) {
    if max_depth == 0 || items.len() >= max_items {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if items.len() >= max_items {
            break;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_java_binaries_limited(&path, max_depth - 1, max_items, items);
        } else if is_java_binary(&path)
            && path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                == Some("bin")
        {
            items.push(path);
        }
    }
}

fn is_java_binary(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(java_binary_name())
}

fn java_binary_name() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

fn find_file_limited(root: &Path, file_name: &str, max_depth: usize) -> Option<PathBuf> {
    if max_depth == 0 {
        return None;
    }
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_limited(&path, file_name, max_depth - 1) {
                return Some(found);
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name) {
            return Some(path);
        }
    }
    None
}

fn decode_html(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

async fn resolve_jre_zip(
    network: &NetworkClient,
    java_version: u16,
    os: &str,
    arch: &str,
) -> AppResult<JrePackage> {
    let api = format!(
        "https://api.adoptium.net/v3/assets/latest/{java_version}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse&heap_size=normal"
    );
    let value = network.get_json::<Value>(&api).await?;
    let array = value.as_array().ok_or_else(|| {
        AppError::Invalid("Adoptium API returned a non-array response".to_string())
    })?;

    for item in array {
        let Some(package) = item.get("binary").and_then(|binary| binary.get("package")) else {
            continue;
        };
        let Some(file_name) = package.get("name").and_then(Value::as_str) else {
            continue;
        };
        if !file_name.to_ascii_lowercase().ends_with(".zip") {
            continue;
        }
        if let Some(package) = package_from_adoptium_value(package, java_version, os, arch)? {
            return Ok(package);
        }
    }

    Err(AppError::NotFound(format!(
        "no JRE zip found for Java {java_version} on {os}/{arch}"
    )))
}

fn package_from_adoptium_value(
    package: &Value,
    java_version: u16,
    os: &str,
    arch: &str,
) -> AppResult<Option<JrePackage>> {
    let Some(file_name) = package.get("name").and_then(Value::as_str) else {
        return Ok(None);
    };
    if !file_name.to_ascii_lowercase().ends_with(".zip") {
        return Ok(None);
    }
    let checksum = package
        .get("checksum")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Invalid(format!("{file_name} has no checksum")))?;
    Ok(Some(JrePackage {
        java_version,
        version: package_version_from_file_name(file_name)
            .unwrap_or_else(|| java_version.to_string()),
        os: os.to_string(),
        arch: arch.to_string(),
        file_name: file_name.to_string(),
        checksum: checksum.to_string(),
        tuna_url: build_tuna_jre_url(java_version, os, arch, file_name),
        size_bytes: package.get("size").and_then(Value::as_u64),
    }))
}

fn build_tuna_jre_url(java_version: u16, os: &str, arch: &str, file_name: &str) -> String {
    format!("{TUNA_ADOPTIUM_ROOT}/{java_version}/jre/{arch}/{os}/{file_name}")
}

pub fn required_java_from_jar(path: &Path) -> AppResult<u16> {
    let file = fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut max_major = None;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if !entry.name().ends_with(".class") {
            continue;
        }
        let mut header = [0_u8; 8];
        if entry.read_exact(&mut header).is_ok() && &header[0..4] == b"\xCA\xFE\xBA\xBE" {
            let major = u16::from_be_bytes([header[6], header[7]]);
            max_major = Some(max_major.map_or(major, |current: u16| current.max(major)));
        }
    }
    Ok(max_major.map(java_feature_from_class_major).unwrap_or(17))
}

fn java_feature_from_class_major(major: u16) -> u16 {
    if major <= 44 {
        8
    } else {
        major - 44
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> AppResult<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let enclosed = entry.enclosed_name().ok_or_else(|| {
            AppError::Invalid(format!("unsafe path in zip entry: {}", entry.name()))
        })?;
        let output_path = destination.join(enclosed);
        if entry.is_dir() {
            fs_util::ensure_dir(&output_path)?;
        } else {
            if let Some(parent) = output_path.parent() {
                fs_util::ensure_dir(parent)?;
            }
            let mut output = fs::File::create(&output_path)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}

fn find_java_binary(root: &Path) -> Option<PathBuf> {
    let binary = if cfg!(windows) { "java.exe" } else { "java" };
    find_file(root, binary)
}

fn find_file(root: &Path, file_name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, file_name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some(file_name)
            && path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                == Some("bin")
        {
            return Some(path);
        }
    }
    None
}

pub fn adoptium_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        "linux" => "linux",
        other => other,
    }
}

pub fn adoptium_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        "arm" => "arm",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_tuna_jre_url, java_feature_from_class_major, package_from_adoptium_value,
        parse_java_runtime_details, parse_tuna_java_versions, parse_tuna_runtime_packages,
    };
    use serde_json::json;

    #[test]
    fn maps_class_major_to_java_feature() {
        assert_eq!(java_feature_from_class_major(52), 8);
        assert_eq!(java_feature_from_class_major(61), 17);
        assert_eq!(java_feature_from_class_major(69), 25);
    }

    #[test]
    fn builds_tuna_jre_zip_url() {
        assert_eq!(
            build_tuna_jre_url(
                17,
                "windows",
                "x64",
                "OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.zip"
            ),
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jre/x64/windows/OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.zip"
        );
    }

    #[test]
    fn accepts_only_jre_zip_packages() {
        let zip = json!({
            "name": "OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.zip",
            "checksum": "abc"
        });
        let tar = json!({
            "name": "OpenJDK17U-jre_x64_linux_hotspot_17.0.19_10.tar.gz",
            "checksum": "abc"
        });
        assert!(package_from_adoptium_value(&zip, 17, "windows", "x64")
            .unwrap()
            .is_some());
        assert!(package_from_adoptium_value(&tar, 17, "linux", "x64")
            .unwrap()
            .is_none());
    }

    #[test]
    fn parses_tuna_runtime_index() {
        let root = r#"
            <a href="11/" title="11">11/</a>
            <a href="17/" title="17">17/</a>
            <a href="deb/" title="deb">deb/</a>
        "#;
        assert_eq!(parse_tuna_java_versions(root).unwrap(), vec![11, 17]);

        let body = r#"
            <a href="OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.msi" title="">OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.msi</a></td><td class="size">30.2 MiB</td><td class="date">04 May 2026 12:21:32 +0000</td>
            <a href="OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.zip" title="">OpenJDK17U-jre_x64_windows_hotspot_17.0.19_10.zip</a></td><td class="size">41.7 MiB</td><td class="date">04 May 2026 12:21:42 +0000</td>
        "#;
        let runtimes = parse_tuna_runtime_packages(
            17,
            "windows",
            "x64",
            "https://mirrors.tuna.tsinghua.edu.cn/Adoptium/17/jre/x64/windows/",
            body,
        )
        .unwrap();
        assert_eq!(runtimes.len(), 1);
        assert_eq!(runtimes[0].version, "17.0.19_10");
        assert!(runtimes[0].download_url.ends_with(".zip"));
    }

    #[test]
    fn parses_java_version_output() {
        assert_eq!(
            parse_java_runtime_details(r#"openjdk version "17.0.11" 2024-04-16"#)
                .map(|d| d.java_version),
            Some(17)
        );
        assert_eq!(
            parse_java_runtime_details(r#"java version "1.8.0_402""#)
                .map(|d| d.java_version),
            Some(8)
        );
    }
}
