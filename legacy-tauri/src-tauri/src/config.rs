use crate::{
    error::{AppError, AppResult},
    fs_util,
    models::{InstalledInstance, RuntimeInfo, RuntimeSource, Settings},
};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const PORTABLE_DATA_DIR: &str = "MindustryLauncherData";

#[derive(Debug, Clone)]
pub struct InstallLayout {
    pub root: PathBuf,
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub tmp_downloads_dir: PathBuf,
    pub versions_dir: PathBuf,
    pub instances_dir: PathBuf,
    pub runtimes_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl InstallLayout {
    pub fn new(root: PathBuf) -> Self {
        Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            downloads_dir: root.join("downloads"),
            tmp_downloads_dir: root.join("downloads").join("tmp"),
            versions_dir: root.join("versions"),
            instances_dir: root.join("instances"),
            runtimes_dir: root.join("runtimes"),
            logs_dir: root.join("logs"),
            root,
        }
    }

    pub fn ensure(&self) -> AppResult<()> {
        for path in [
            &self.config_dir,
            &self.cache_dir,
            &self.downloads_dir,
            &self.tmp_downloads_dir,
            &self.versions_dir,
            &self.instances_dir,
            &self.runtimes_dir,
            &self.logs_dir,
        ] {
            fs_util::ensure_dir(path)?;
        }
        Ok(())
    }

    pub fn settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn instances_path(&self) -> PathBuf {
        self.config_dir.join("instances.json")
    }

    pub fn runtimes_path(&self) -> PathBuf {
        self.config_dir.join("runtimes.json")
    }

    pub fn legacy_accelerators_path(&self) -> PathBuf {
        self.config_dir.join("accelerators.json")
    }

    pub fn versions_cache_path(&self) -> PathBuf {
        self.cache_dir.join("versions.json")
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootPointer {
    install_root: String,
}

fn pointer_path(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = portable_data_dir(app)?;
    fs_util::ensure_dir(&dir)?;
    Ok(dir.join("install-root.json"))
}

pub fn default_install_root(app: &AppHandle) -> AppResult<PathBuf> {
    Ok(portable_data_dir(app)?.join("data"))
}

pub fn portable_data_dir(_app: &AppHandle) -> AppResult<PathBuf> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| {
            AppError::Invalid("cannot resolve portable launcher directory".to_string())
        })?;
    Ok(portable_data_dir_from_base(&exe_dir))
}

fn portable_data_dir_from_base(base: &Path) -> PathBuf {
    base.join(PORTABLE_DATA_DIR)
}

pub fn load_install_root(app: &AppHandle) -> AppResult<PathBuf> {
    let pointer = pointer_path(app)?;
    if let Some(pointer) = fs_util::read_json::<RootPointer>(&pointer)? {
        if !pointer.install_root.trim().is_empty() {
            return Ok(PathBuf::from(pointer.install_root));
        }
    }
    default_install_root(app)
}

pub fn write_install_root(app: &AppHandle, root: &Path) -> AppResult<()> {
    fs_util::write_json(
        &pointer_path(app)?,
        &RootPointer {
            install_root: root.to_string_lossy().to_string(),
        },
    )
}

pub fn layout_from_settings(settings: &Settings) -> AppResult<InstallLayout> {
    let root = PathBuf::from(settings.install_root.trim());
    if root.as_os_str().is_empty() {
        return Err(AppError::Invalid(
            "install root cannot be empty".to_string(),
        ));
    }
    Ok(InstallLayout::new(root))
}

pub fn load_settings(app: &AppHandle) -> AppResult<Settings> {
    let root = load_install_root(app)?;
    let layout = InstallLayout::new(root.clone());
    layout.ensure()?;
    if let Some(mut settings) = fs_util::read_json::<Settings>(&layout.settings_path())? {
        if settings.install_root.trim().is_empty() {
            settings.install_root = root.to_string_lossy().to_string();
        }
        Ok(settings)
    } else {
        let settings = Settings::with_install_root(root.to_string_lossy().to_string());
        fs_util::write_json(&layout.settings_path(), &settings)?;
        write_install_root(app, &root)?;
        Ok(settings)
    }
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> AppResult<Settings> {
    let layout = layout_from_settings(settings)?;
    layout.ensure()?;
    fs_util::write_json(&layout.settings_path(), settings)?;
    write_install_root(app, &layout.root)?;
    Ok(settings.clone())
}

pub fn load_instances(layout: &InstallLayout) -> AppResult<Vec<InstalledInstance>> {
    Ok(fs_util::read_json(&layout.instances_path())?.unwrap_or_default())
}

pub fn save_instances(layout: &InstallLayout, instances: &[InstalledInstance]) -> AppResult<()> {
    fs_util::write_json(&layout.instances_path(), &instances)
}

pub fn load_runtimes(layout: &InstallLayout) -> AppResult<Vec<RuntimeInfo>> {
    let mut runtimes =
        fs_util::read_json::<Vec<RuntimeInfo>>(&layout.runtimes_path())?.unwrap_or_default();
    for runtime in &mut runtimes {
        if runtime.source == RuntimeSource::Unknown {
            runtime.source = if runtime.id.starts_with("jre-") {
                RuntimeSource::Launcher
            } else if runtime.id.starts_with("imported-") {
                RuntimeSource::Imported
            } else if runtime.id.starts_with("local-") {
                RuntimeSource::Scanned
            } else if runtime.id.starts_with("system-") {
                RuntimeSource::System
            } else {
                RuntimeSource::Unknown
            };
        }
    }
    Ok(runtimes)
}

pub fn save_runtimes(layout: &InstallLayout, runtimes: &[RuntimeInfo]) -> AppResult<()> {
    fs_util::write_json(&layout.runtimes_path(), &runtimes)
}

#[cfg(test)]
mod tests {
    use super::portable_data_dir_from_base;
    use std::path::Path;

    #[test]
    fn portable_data_lives_next_to_launcher() {
        let path = portable_data_dir_from_base(Path::new("D:/Tools/MindustryLauncher"));
        assert_eq!(
            path.to_string_lossy().replace('\\', "/"),
            "D:/Tools/MindustryLauncher/MindustryLauncherData"
        );
    }
}
