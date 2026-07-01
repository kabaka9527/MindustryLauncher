use crate::{
    config::{self, InstallLayout},
    error::{AppError, AppResult},
    fs_util,
    models::{LaunchResult, MigrationResult, Settings},
    runtime,
};
use chrono::Utc;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
};
use tauri::AppHandle;
use tokio::process::Command;

// Launching avoids the shell completely. The instance data directory is passed
// both as a JVM property and as an environment variable for Mindustry isolation.
pub async fn launch_version(
    layout: &InstallLayout,
    instance_id: String,
) -> AppResult<LaunchResult> {
    let instances = config::load_instances(layout)?;
    let instance = instances
        .iter()
        .find(|item| item.id == instance_id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("instance {instance_id}")))?;

    let jar_path = PathBuf::from(&instance.jar_path);
    let data_dir = PathBuf::from(&instance.data_dir);
    if !jar_path.exists() {
        return Err(AppError::NotFound(format!("jar {}", jar_path.display())));
    }
    fs_util::ensure_dir(&data_dir)?;

    let required_java = runtime::required_java_from_jar(&jar_path)?;
    let runtimes = config::load_runtimes(layout)?;
    let runtime = if let Some(runtime_id) = instance.runtime_id.as_deref() {
        runtimes.iter().find(|item| {
            item.enabled && item.id == runtime_id && Path::new(&item.java_path).exists()
        })
    } else {
        runtimes
            .iter()
            .filter(|item| {
                item.enabled
                    && item.java_version >= required_java
                    && Path::new(&item.java_path).exists()
            })
            .min_by_key(|item| item.java_version)
    }
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "JRE {required_java} or newer is missing; install the runtime first"
        ))
    })?;

    let log_dir = PathBuf::from(&instance.install_dir).join("logs");
    fs_util::ensure_dir(&log_dir)?;
    let log_path = log_dir.join(format!("launch-{}.log", Utc::now().format("%Y%m%d-%H%M%S")));
    let stdout = fs::File::create(&log_path)?;
    let stderr = stdout.try_clone()?;

    let mut command = Command::new(&runtime.java_path);
    if let Some(memory) = instance
        .launch_settings
        .min_memory_mb
        .filter(|value| *value > 0)
    {
        command.arg(format!("-Xms{memory}m"));
    }
    if let Some(memory) = instance
        .launch_settings
        .max_memory_mb
        .filter(|value| *value > 0)
    {
        command.arg(format!("-Xmx{memory}m"));
    }
    for arg in split_command_args(&instance.launch_settings.extra_jvm_args)? {
        command.arg(arg);
    }
    command
        .arg(format!(
            "-Dmindustry.data.dir={}",
            data_dir.to_string_lossy()
        ))
        .arg("-jar")
        .arg(&jar_path);
    for arg in split_command_args(&instance.launch_settings.game_args)? {
        command.arg(arg);
    }

    let mut child = command
        .env("MINDUSTRY_DATA_DIR", &data_dir)
        .current_dir(PathBuf::from(&instance.install_dir))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|err| AppError::Command(err.to_string()))?;

    let pid = child.id().unwrap_or_default();
    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    Ok(LaunchResult {
        pid,
        log_path: log_path.to_string_lossy().to_string(),
    })
}

pub fn migrate_install_root(
    app: &AppHandle,
    current_settings: &Settings,
    new_root: String,
) -> AppResult<(Settings, MigrationResult)> {
    let old_layout = config::layout_from_settings(current_settings)?;
    let new_root = PathBuf::from(new_root.trim());
    if new_root.as_os_str().is_empty() {
        return Err(AppError::Invalid(
            "new install root cannot be empty".to_string(),
        ));
    }
    let new_root = fs_util::canonicalize_or_create(&new_root)?;
    let new_layout = InstallLayout::new(new_root.clone());
    new_layout.ensure()?;

    if old_layout.root != new_layout.root && old_layout.root.exists() {
        let old_root = old_layout.root.canonicalize()?;
        if new_layout.root.starts_with(&old_root) {
            return Err(AppError::Invalid(
                "new install root cannot be inside the current install root".to_string(),
            ));
        }
        fs_util::copy_dir_recursive(&old_layout.root, &new_layout.root)?;
        rewrite_metadata_paths(&old_root, &new_layout.root, &new_layout)?;
    }

    let mut settings = current_settings.clone();
    settings.install_root = new_root.to_string_lossy().to_string();
    config::save_settings(app, &settings)?;

    Ok((
        settings,
        MigrationResult {
            old_root: old_layout.root.to_string_lossy().to_string(),
            new_root: new_layout.root.to_string_lossy().to_string(),
            copied: old_layout.root != new_layout.root,
        },
    ))
}

fn rewrite_metadata_paths(
    old_root: &Path,
    new_root: &Path,
    layout: &InstallLayout,
) -> AppResult<()> {
    let mut instances = config::load_instances(layout)?;
    for instance in &mut instances {
        instance.install_dir = rewrite_path_string(old_root, new_root, &instance.install_dir);
        instance.data_dir = rewrite_path_string(old_root, new_root, &instance.data_dir);
        instance.jar_path = rewrite_path_string(old_root, new_root, &instance.jar_path);
    }
    config::save_instances(layout, &instances)?;

    let mut runtimes = config::load_runtimes(layout)?;
    for runtime in &mut runtimes {
        runtime.path = rewrite_path_string(old_root, new_root, &runtime.path);
        runtime.java_path = rewrite_path_string(old_root, new_root, &runtime.java_path);
    }
    config::save_runtimes(layout, &runtimes)?;
    Ok(())
}

fn rewrite_path_string(old_root: &Path, new_root: &Path, value: &str) -> String {
    let path = PathBuf::from(value);
    match path.strip_prefix(old_root) {
        Ok(relative) => new_root.join(relative).to_string_lossy().to_string(),
        Err(_) => value.to_string(),
    }
}

fn split_command_args(input: &str) -> AppResult<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                args.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }

    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return Err(AppError::Invalid(
            "launch arguments contain an unclosed quote".to_string(),
        ));
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

pub fn open_install_root(layout: &InstallLayout) -> AppResult<()> {
    layout.ensure()?;
    fs_util::open_path(&layout.root)
}

pub fn open_url(url: &str) -> AppResult<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "start", "", url]);
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map_err(|err| AppError::Command(format!("failed to open url {url}: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{rewrite_path_string, split_command_args};
    use std::path::Path;

    #[test]
    fn rewrites_absolute_paths_inside_install_root() {
        let rewritten = rewrite_path_string(
            Path::new("D:/OldRoot"),
            Path::new("E:/NewRoot"),
            "D:/OldRoot/instances/mindustry-v1/data",
        );
        assert_eq!(
            rewritten.replace('\\', "/"),
            "E:/NewRoot/instances/mindustry-v1/data"
        );
    }

    #[test]
    fn leaves_external_paths_unchanged() {
        let rewritten = rewrite_path_string(
            Path::new("D:/OldRoot"),
            Path::new("E:/NewRoot"),
            "D:/Elsewhere/file.jar",
        );
        assert_eq!(rewritten, "D:/Elsewhere/file.jar");
    }

    #[test]
    fn splits_quoted_launch_args() {
        let args = split_command_args(r#"-Dfoo="bar baz" --flag value"#).unwrap();
        assert_eq!(args, vec!["-Dfoo=bar baz", "--flag", "value"]);
    }
}
