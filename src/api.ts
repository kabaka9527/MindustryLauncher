import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AcceleratorList,
  AppUiState,
  DebugLogSnapshot,
  InstalledInstance,
  LaunchSettings,
  LaunchResult,
  LauncherUpdateInfo,
  MigrationResult,
  RemoteRuntime,
  RemoteVersion,
  RuntimeInfo,
  Settings,
  TaskEvent,
} from "./types";

export function getAppState() {
  return invoke<AppUiState>("get_app_state");
}

export function saveSettings(settings: Settings) {
  return invoke<Settings>("save_settings", { settings });
}

export function refreshAccelerators() {
  return invoke<AcceleratorList>("refresh_accelerators");
}

export function refreshVersions() {
  return invoke<RemoteVersion[]>("refresh_versions");
}

export function startupRefreshVersions() {
  return invoke<RemoteVersion[]>("startup_refresh_versions");
}

export function onVersionsRefreshed(callback: (versions: RemoteVersion[]) => void): Promise<UnlistenFn> {
  return listen<RemoteVersion[]>("versions-refreshed", (event) => {
    callback(event.payload);
  });
}

export function installVersion(version: RemoteVersion, onEvent: Channel<TaskEvent>) {
  return invoke<InstalledInstance>("install_version", { version, onEvent });
}

export function switchVersion(version: RemoteVersion, onEvent: Channel<TaskEvent>) {
  return invoke<InstalledInstance>("switch_version", { version, onEvent });
}

export function ensureRuntime(javaVersion: number | null, onEvent: Channel<TaskEvent>) {
  return invoke<RuntimeInfo>("ensure_runtime", { javaVersion, onEvent });
}

export function listRemoteRuntimes() {
  return invoke<RemoteRuntime[]>("list_remote_runtimes");
}

export function installRuntime(runtime: RemoteRuntime, onEvent: Channel<TaskEvent>) {
  return invoke<RuntimeInfo>("install_runtime", { runtime, onEvent });
}

export function pauseDownload(taskId: string) {
  return invoke<void>("pause_download", { taskId });
}

export function resumeDownload(taskId: string) {
  return invoke<void>("resume_download", { taskId });
}

export function cancelDownload(taskId: string) {
  return invoke<void>("cancel_download", { taskId });
}

export function importRuntime(path: string) {
  return invoke<RuntimeInfo>("import_runtime", { path });
}

export function scanRuntimes(path: string) {
  return invoke<RuntimeInfo[]>("scan_runtimes", { path });
}

export function setRuntimeEnabled(runtimeId: string, enabled: boolean) {
  return invoke<RuntimeInfo[]>("set_runtime_enabled", { runtimeId, enabled });
}

export function deleteRuntime(runtimeId: string) {
  return invoke<RuntimeInfo[]>("delete_runtime", { runtimeId });
}

export function saveInstanceLaunchSettings(
  instanceId: string,
  runtimeId: string | null,
  launchSettings: LaunchSettings,
) {
  return invoke<InstalledInstance[]>("save_instance_launch_settings", {
    instanceId,
    runtimeId,
    launchSettings,
  });
}

export function launchVersion(instanceId: string) {
  return invoke<LaunchResult>("launch_version", { instanceId });
}

export function migrateInstallRoot(newRoot: string) {
  return invoke<MigrationResult>("migrate_install_root", { newRoot });
}

export function deleteInstance(instanceId: string) {
  return invoke<InstalledInstance[]>("delete_instance", { instanceId });
}

export function openInstallRoot() {
  return invoke<void>("open_install_root");
}

export function readDebugLog() {
  return invoke<DebugLogSnapshot>("read_debug_log");
}

export function clearDebugLog() {
  return invoke<DebugLogSnapshot>("clear_debug_log");
}

export function openDebugLogDir() {
  return invoke<void>("open_debug_log_dir");
}

export function openDebugLogWindow() {
  return invoke<void>("open_debug_log_window");
}

export function checkLauncherUpdate() {
  return invoke<LauncherUpdateInfo>("check_launcher_update");
}

export function ignoreLauncherVersion(version: string) {
  return invoke<Settings>("ignore_launcher_version", { version });
}
