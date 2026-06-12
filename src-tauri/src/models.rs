use serde::{Deserialize, Serialize};

pub const DEFAULT_ACCELERATOR_PREFIX: &str = "https://hubproxy.kabaka.xyz/";
pub const REMOTE_ACCELERATOR_LIST: &str =
    "https://raw.githubusercontent.com/kabaka9527/MindustryLauncher/main/resources/github-accelerators.json";
pub const USER_AGENT: &str = "MindustryLauncher/0.1.1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum GameChannel {
    #[serde(rename = "mindustry")]
    Mindustry,
    #[serde(rename = "mindustryX")]
    MindustryX,
    #[serde(rename = "mindustryBE")]
    MindustryBE,
    #[serde(rename = "mindustryXBE")]
    MindustryXBE,
}

impl GameChannel {
    pub fn as_id(self) -> &'static str {
        match self {
            Self::Mindustry => "mindustry",
            Self::MindustryX => "mindustryX",
            Self::MindustryBE => "mindustryBE",
            Self::MindustryXBE => "mindustryXBE",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mindustry => "Mindustry",
            Self::MindustryX => "MindustryX",
            Self::MindustryBE => "Mindustry BE",
            Self::MindustryXBE => "MindustryX BE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelVisibility {
    pub mindustry: bool,
    pub mindustry_x: bool,
    pub mindustry_be: bool,
    pub mindustry_xbe: bool,
}

impl Default for ChannelVisibility {
    fn default() -> Self {
        Self {
            mindustry: true,
            mindustry_x: true,
            mindustry_be: false,
            mindustry_xbe: false,
        }
    }
}

impl ChannelVisibility {
    pub fn is_visible(&self, channel: GameChannel, show_be: bool) -> bool {
        match channel {
            GameChannel::Mindustry => self.mindustry,
            GameChannel::MindustryX => self.mindustry_x,
            GameChannel::MindustryBE => show_be && self.mindustry_be,
            GameChannel::MindustryXBE => show_be && self.mindustry_xbe,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub install_root: String,
    pub show_be: bool,
    pub github_proxy_prefix: Option<String>,
    pub http_proxy: Option<String>,
    pub selected_accelerator_id: Option<String>,
    pub channel_visibility: ChannelVisibility,
    #[serde(default)]
    pub runtime_prompt_dismissed: bool,
    #[serde(default)]
    pub debug_mode: bool,
}

impl Settings {
    pub fn with_install_root(install_root: String) -> Self {
        Self {
            install_root,
            show_be: false,
            github_proxy_prefix: None,
            http_proxy: None,
            selected_accelerator_id: Some("hubproxy-kabaka".to_string()),
            channel_visibility: ChannelVisibility::default(),
            runtime_prompt_dismissed: false,
            debug_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorSupports {
    pub api: bool,
    pub raw: bool,
    pub release_asset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorRule {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accelerator {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub rules: Vec<AcceleratorRule>,
    pub supports: AcceleratorSupports,
    pub health_check_url: Option<String>,
    pub enabled_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceleratorList {
    pub version: u32,
    pub updated_at: String,
    pub sources: Vec<Accelerator>,
}

impl Default for AcceleratorList {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: "built-in".to_string(),
            sources: vec![
                Accelerator {
                    id: "hubproxy-kabaka".to_string(),
                    name: "HubProxy GitHub 加速".to_string(),
                    base_url: DEFAULT_ACCELERATOR_PREFIX.to_string(),
                    rules: vec![],
                    supports: AcceleratorSupports {
                        api: true,
                        raw: true,
                        release_asset: true,
                    },
                    health_check_url: Some(format!(
                        "{DEFAULT_ACCELERATOR_PREFIX}https://github.com/"
                    )),
                    enabled_by_default: true,
                },
                Accelerator {
                    id: "direct".to_string(),
                    name: "GitHub 直连".to_string(),
                    base_url: String::new(),
                    rules: vec![
                        AcceleratorRule {
                            from: "https://api.github.com/".to_string(),
                            to: "https://api.github.com/".to_string(),
                        },
                        AcceleratorRule {
                            from: "https://raw.githubusercontent.com/".to_string(),
                            to: "https://raw.githubusercontent.com/".to_string(),
                        },
                        AcceleratorRule {
                            from: "https://github.com/".to_string(),
                            to: "https://github.com/".to_string(),
                        },
                    ],
                    supports: AcceleratorSupports {
                        api: true,
                        raw: true,
                        release_asset: true,
                    },
                    health_check_url: Some("https://github.com/".to_string()),
                    enabled_by_default: false,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersion {
    pub id: String,
    pub channel: GameChannel,
    pub channel_label: String,
    pub version: String,
    pub tag: String,
    pub name: String,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub assets: Vec<ReleaseAsset>,
    pub selected_asset: Option<ReleaseAsset>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntime {
    pub id: String,
    pub java_version: u16,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub file_name: String,
    pub size_label: String,
    pub size_bytes: Option<u64>,
    pub updated_at: String,
    pub download_url: String,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledInstance {
    pub id: String,
    pub channel: GameChannel,
    pub version: String,
    pub install_dir: String,
    pub data_dir: String,
    pub jar_path: String,
    pub runtime_id: Option<String>,
    pub installed_at: String,
    #[serde(default)]
    pub launch_settings: LaunchSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSettings {
    pub min_memory_mb: Option<u32>,
    pub max_memory_mb: Option<u32>,
    #[serde(default)]
    pub extra_jvm_args: String,
    #[serde(default)]
    pub game_args: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub id: String,
    pub java_version: u16,
    #[serde(default)]
    pub version: Option<String>,
    pub os: String,
    pub arch: String,
    pub path: String,
    pub java_path: String,
    pub installed: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub source: RuntimeSource,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeSource {
    Launcher,
    Imported,
    Scanned,
    System,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: u32,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResult {
    pub old_root: String,
    pub new_root: String,
    pub copied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugLogSnapshot {
    pub enabled: bool,
    pub log_path: String,
    pub session_id: Option<String>,
    pub started_at: Option<String>,
    pub line_count: usize,
    pub max_lines: usize,
    pub truncated: bool,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "event",
    content = "data"
)]
pub enum TaskEvent {
    Started {
        task_id: String,
        label: String,
        total_bytes: Option<u64>,
        message: Option<String>,
    },
    Progress {
        task_id: String,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        bytes_per_second: Option<u64>,
        message: Option<String>,
    },
    Paused {
        task_id: String,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        message: String,
    },
    Finished {
        task_id: String,
        message: String,
    },
    Canceled {
        task_id: String,
        message: String,
    },
    Failed {
        task_id: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUiState {
    pub settings: Settings,
    pub accelerators: AcceleratorList,
    pub versions: Vec<RemoteVersion>,
    pub instances: Vec<InstalledInstance>,
    pub runtimes: Vec<RuntimeInfo>,
}

#[cfg(test)]
mod tests {
    use super::TaskEvent;
    use serde_json::json;

    #[test]
    fn task_event_fields_are_camel_case_for_frontend() {
        let value = serde_json::to_value(TaskEvent::Progress {
            task_id: "game:mindustry-v158".to_string(),
            downloaded_bytes: 1024,
            total_bytes: Some(2048),
            bytes_per_second: Some(512),
            message: Some("downloading".to_string()),
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "event": "progress",
                "data": {
                    "taskId": "game:mindustry-v158",
                    "downloadedBytes": 1024,
                    "totalBytes": 2048,
                    "bytesPerSecond": 512,
                    "message": "downloading"
                }
            })
        );
    }
}
