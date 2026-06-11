mod accelerators;
mod config;
mod debug_console;
mod error;
mod fs_util;
mod instances;
mod launcher;
mod models;
mod network;
mod runtime;
mod versions;

use crate::{
    error::AppResult,
    models::{
        AcceleratorList, AppUiState, DebugLogSnapshot, InstalledInstance, LaunchResult,
        LaunchSettings, MigrationResult, RemoteRuntime, RemoteVersion, RuntimeInfo, Settings,
        TaskEvent,
    },
};
use tauri::{ipc::Channel, AppHandle, Manager, State};
use tokio::sync::RwLock;

pub struct LauncherState {
    settings: RwLock<Settings>,
    accelerators: RwLock<AcceleratorList>,
}

#[tauri::command(rename_all = "camelCase")]
async fn get_app_state(_app: AppHandle, state: State<'_, LauncherState>) -> AppResult<AppUiState> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    layout.ensure()?;
    let accelerators = state.accelerators.read().await.clone();
    Ok(AppUiState {
        settings,
        accelerators,
        versions: versions::load_cached_versions(&layout)?,
        instances: config::load_instances(&layout)?,
        runtimes: config::load_runtimes(&layout)?,
    })
}

#[tauri::command(rename_all = "camelCase")]
async fn save_settings(
    app: AppHandle,
    state: State<'_, LauncherState>,
    settings: Settings,
) -> AppResult<Settings> {
    let previous = state.settings.read().await.clone();
    let saved = config::save_settings(&app, &settings)?;
    let layout = config::layout_from_settings(&saved)?;
    debug_console::set_log_path(layout.logs_dir.join("debug.log"));
    if previous.debug_mode != saved.debug_mode {
        debug_console::info("调试模式设置已变更，重启启动器后生效");
    }
    *state.settings.write().await = saved.clone();
    Ok(saved)
}

#[tauri::command(rename_all = "camelCase")]
async fn refresh_accelerators(state: State<'_, LauncherState>) -> AppResult<AcceleratorList> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    let list = accelerators::refresh_accelerators(&settings, &layout).await?;
    *state.accelerators.write().await = list.clone();
    Ok(list)
}

#[tauri::command(rename_all = "camelCase")]
async fn refresh_versions(state: State<'_, LauncherState>) -> AppResult<Vec<RemoteVersion>> {
    let settings = state.settings.read().await.clone();
    let accelerators = state.accelerators.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    match versions::refresh_versions(&settings, &layout, &accelerators).await {
        Ok(versions) => Ok(versions),
        Err(err) => {
            let cached = versions::load_cached_versions(&layout)?;
            if cached.is_empty() {
                Err(err)
            } else {
                Ok(cached)
            }
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
async fn install_version(
    state: State<'_, LauncherState>,
    version: RemoteVersion,
    on_event: Channel<TaskEvent>,
) -> AppResult<InstalledInstance> {
    let settings = state.settings.read().await.clone();
    let accelerators = state.accelerators.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    instances::install_version(&settings, &layout, &accelerators, version, on_event).await
}

#[tauri::command(rename_all = "camelCase")]
async fn switch_version(
    state: State<'_, LauncherState>,
    version: RemoteVersion,
    on_event: Channel<TaskEvent>,
) -> AppResult<InstalledInstance> {
    let settings = state.settings.read().await.clone();
    let accelerators = state.accelerators.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    instances::switch_version(&settings, &layout, &accelerators, version, on_event).await
}

#[tauri::command(rename_all = "camelCase")]
async fn ensure_runtime(
    state: State<'_, LauncherState>,
    java_version: Option<u16>,
    on_event: Channel<TaskEvent>,
) -> AppResult<RuntimeInfo> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::ensure_runtime(&settings, &layout, java_version, on_event).await
}

#[tauri::command(rename_all = "camelCase")]
async fn list_remote_runtimes(state: State<'_, LauncherState>) -> AppResult<Vec<RemoteRuntime>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::list_remote_runtimes(&settings, &layout).await
}

#[tauri::command(rename_all = "camelCase")]
async fn install_runtime(
    state: State<'_, LauncherState>,
    runtime: RemoteRuntime,
    on_event: Channel<TaskEvent>,
) -> AppResult<RuntimeInfo> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::install_runtime(&settings, &layout, runtime, on_event).await
}

#[tauri::command(rename_all = "camelCase")]
async fn import_runtime(state: State<'_, LauncherState>, path: String) -> AppResult<RuntimeInfo> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::import_runtime(&layout, path)
}

#[tauri::command(rename_all = "camelCase")]
async fn scan_runtimes(
    state: State<'_, LauncherState>,
    path: String,
) -> AppResult<Vec<RuntimeInfo>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::scan_runtimes(&layout, path)
}

#[tauri::command(rename_all = "camelCase")]
async fn set_runtime_enabled(
    state: State<'_, LauncherState>,
    runtime_id: String,
    enabled: bool,
) -> AppResult<Vec<RuntimeInfo>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::set_runtime_enabled(&layout, runtime_id, enabled)
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_runtime(
    state: State<'_, LauncherState>,
    runtime_id: String,
) -> AppResult<Vec<RuntimeInfo>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    runtime::delete_runtime(&layout, runtime_id)
}

#[tauri::command(rename_all = "camelCase")]
async fn save_instance_launch_settings(
    state: State<'_, LauncherState>,
    instance_id: String,
    runtime_id: Option<String>,
    launch_settings: LaunchSettings,
) -> AppResult<Vec<InstalledInstance>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    instances::save_instance_launch_settings(&layout, instance_id, runtime_id, launch_settings)
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_version(
    state: State<'_, LauncherState>,
    instance_id: String,
) -> AppResult<LaunchResult> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    launcher::launch_version(&layout, instance_id).await
}

#[tauri::command(rename_all = "camelCase")]
async fn migrate_install_root(
    app: AppHandle,
    state: State<'_, LauncherState>,
    new_root: String,
) -> AppResult<MigrationResult> {
    let current = state.settings.read().await.clone();
    let (settings, result) = launcher::migrate_install_root(&app, &current, new_root)?;
    let layout = config::layout_from_settings(&settings)?;
    let accelerators = accelerators::load_startup_accelerators(&layout)?;
    *state.settings.write().await = settings;
    *state.accelerators.write().await = accelerators;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
async fn delete_instance(
    state: State<'_, LauncherState>,
    instance_id: String,
) -> AppResult<Vec<InstalledInstance>> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    instances::delete_instance(&layout, instance_id)
}

#[tauri::command(rename_all = "camelCase")]
async fn open_install_root(state: State<'_, LauncherState>) -> AppResult<()> {
    let settings = state.settings.read().await.clone();
    let layout = config::layout_from_settings(&settings)?;
    launcher::open_install_root(&layout)
}

#[tauri::command(rename_all = "camelCase")]
fn read_debug_log() -> AppResult<DebugLogSnapshot> {
    debug_console::snapshot(600)
}

#[tauri::command(rename_all = "camelCase")]
fn clear_debug_log() -> AppResult<DebugLogSnapshot> {
    debug_console::clear_log()
}

#[tauri::command(rename_all = "camelCase")]
fn open_debug_log_dir() -> AppResult<()> {
    debug_console::open_log_dir()
}

#[tauri::command(rename_all = "camelCase")]
fn open_debug_log_window(app: AppHandle) -> AppResult<()> {
    debug_console::open_debug_window(&app)
}

#[tauri::command(rename_all = "camelCase")]
fn pause_download(task_id: String) -> AppResult<()> {
    network::pause_download_task(&task_id)
}

#[tauri::command(rename_all = "camelCase")]
fn resume_download(task_id: String) -> AppResult<()> {
    network::resume_download_task(&task_id)
}

#[tauri::command(rename_all = "camelCase")]
fn cancel_download(task_id: String) -> AppResult<()> {
    network::cancel_download_task(&task_id)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            debug_console::install_panic_hook();
            let app_handle = app.handle().clone();
            let settings = config::load_settings(&app_handle)?;
            let layout = config::layout_from_settings(&settings)?;
            layout.ensure()?;
            instances::cleanup_partial_downloads(&layout)?;
            debug_console::set_log_path(layout.logs_dir.join("debug.log"));
            if settings.debug_mode {
                debug_console::start_session()?;
                debug_console::set_enabled(true);
                debug_console::section("启动器启动");
                debug_console::info(format!("安装根目录：{}", layout.root.display()));
                debug_console::info(format!("缓存目录：{}", layout.cache_dir.display()));
                debug_console::info(format!("运行时目录：{}", layout.runtimes_dir.display()));
                debug_console::open_debug_window(&app_handle)?;
            } else {
                debug_console::set_enabled(false);
            }
            match runtime::scan_system_runtimes(&layout) {
                Ok(found) => {
                    debug_console::info(format!(
                        "系统运行时检测完成：新增或确认 {} 个运行时",
                        found.len()
                    ));
                }
                Err(err) => {
                    debug_console::warn(format!("系统运行时检测失败：{err}"));
                }
            }
            let accelerators = accelerators::load_startup_accelerators(&layout)?;
            app.manage(LauncherState {
                settings: RwLock::new(settings),
                accelerators: RwLock::new(accelerators),
            });
            debug_console::info("启动器加载已结束");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            save_settings,
            refresh_accelerators,
            refresh_versions,
            install_version,
            switch_version,
            ensure_runtime,
            list_remote_runtimes,
            install_runtime,
            import_runtime,
            scan_runtimes,
            set_runtime_enabled,
            delete_runtime,
            save_instance_launch_settings,
            launch_version,
            migrate_install_root,
            delete_instance,
            open_install_root,
            read_debug_log,
            clear_debug_log,
            open_debug_log_dir,
            open_debug_log_window,
            pause_download,
            resume_download,
            cancel_download
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
