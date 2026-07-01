use crate::error::{AppError, AppResult};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn ensure_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&content)?))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub fn canonicalize_or_create(path: &Path) -> AppResult<PathBuf> {
    ensure_dir(path)?;
    Ok(path.canonicalize()?)
}

pub fn assert_inside_root(root: &Path, target: &Path) -> AppResult<()> {
    let root = root.canonicalize()?;
    let target = if target.exists() {
        target.canonicalize()?
    } else if let Some(parent) = target.parent() {
        parent.canonicalize()?
    } else {
        return Err(AppError::Invalid("target has no parent".to_string()));
    };
    if target.starts_with(root) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "refusing to operate outside install root: {}",
            target.display()
        )))
    }
}

pub fn copy_dir_recursive(from: &Path, to: &Path) -> AppResult<()> {
    ensure_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(&source, &dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                ensure_dir(parent)?;
            }
            fs::copy(&source, &dest)?;
        }
    }
    Ok(())
}

pub fn remove_dir_all_retry(path: &Path) -> AppResult<()> {
    let mut last_err = None;
    for _ in 0..10 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Err(AppError::Io(
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Unknown IO error".to_string()),
    ))
}

pub fn remove_file_retry(path: &Path) -> AppResult<()> {
    let mut last_err = None;
    for _ in 0..10 {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_err = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Err(AppError::Io(
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "Unknown IO error".to_string()),
    ))
}

pub fn open_path(path: &Path) -> AppResult<()> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(path);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map_err(|err| AppError::Command(format!("failed to open {}: {err}", path.display())))?;
    Ok(())
}
