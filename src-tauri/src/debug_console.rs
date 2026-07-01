use crate::{error::AppResult, fs_util, models::DebugLogSnapshot};
use chrono::Local;
use regex::Regex;
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, Once, OnceLock,
    },
};
use tauri::{AppHandle, Emitter};

const DEFAULT_TAIL_LINES: usize = 600;
const MAX_TAIL_LINES: usize = 2_000;
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ARCHIVES: usize = 4;

static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static LOG_STATE: Mutex<DebugLogState> = Mutex::new(DebugLogState {
    log_path: None,
    session_id: None,
    started_at: None,
});
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

#[derive(Debug, Clone)]
struct DebugLogState {
    log_path: Option<PathBuf>,
    session_id: Option<String>,
    started_at: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum DebugLevel {
    Info,
    Warn,
    Error,
}

impl DebugLevel {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

pub fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            error(format!("应用线程发生 panic：{panic_info}"));
            previous(panic_info);
        }));
    });
}

pub fn set_log_path(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut state) = LOG_STATE.lock() {
        state.log_path = Some(path);
    }
}

pub fn set_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

fn emit_entry(level: &DebugLevel, message: &str) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit(
            "debug-log-entry",
            serde_json::json!({
                "level": level.label(),
                "message": message,
                "timestamp": Local::now().to_rfc3339(),
            }),
        );
    }
}

pub fn set_enabled(enabled: bool) {
    let was_enabled = DEBUG_ENABLED.swap(enabled, Ordering::SeqCst);
    if enabled && !was_enabled {
        write_entry(DebugLevel::Info, "调试模式已开启", true);
    } else if !enabled && was_enabled {
        write_entry(DebugLevel::Info, "调试模式已关闭", true);
    }
}

pub fn start_session() -> AppResult<()> {
    let Some(path) = log_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs_util::ensure_dir(parent)?;
        archive_current_log(&path)?;
        prune_archives(parent)?;
    }
    fs::write(&path, [])?;

    let session_id = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let started_at = Local::now().to_rfc3339();
    if let Ok(mut state) = LOG_STATE.lock() {
        state.session_id = Some(session_id.clone());
        state.started_at = Some(started_at.clone());
    }

    write_entry(
        DebugLevel::Info,
        format!("调试会话已开始 session={session_id}"),
        true,
    );
    write_entry(
        DebugLevel::Info,
        format!("日志文件：{}", path.display()),
        true,
    );
    Ok(())
}

pub fn log(message: impl AsRef<str>) {
    info(message);
}

pub fn info(message: impl AsRef<str>) {
    write_entry(DebugLevel::Info, message.as_ref(), false);
}

pub fn warn(message: impl AsRef<str>) {
    write_entry(DebugLevel::Warn, message.as_ref(), false);
}

pub fn error(message: impl AsRef<str>) {
    write_entry(DebugLevel::Error, message.as_ref(), false);
}

pub fn section(title: impl AsRef<str>) {
    write_entry(
        DebugLevel::Info,
        format!("========== {} ==========", title.as_ref()),
        false,
    );
}

pub fn snapshot(max_lines: usize) -> AppResult<DebugLogSnapshot> {
    let max_lines = max_lines.clamp(50, MAX_TAIL_LINES);
    let (log_path, session_id, started_at) = LOG_STATE
        .lock()
        .ok()
        .map(|state| {
            (
                state.log_path.clone(),
                state.session_id.clone(),
                state.started_at.clone(),
            )
        })
        .unwrap_or_default();

    let Some(path) = log_path else {
        return Ok(DebugLogSnapshot {
            enabled: DEBUG_ENABLED.load(Ordering::SeqCst),
            log_path: String::new(),
            session_id,
            started_at,
            line_count: 0,
            max_lines,
            truncated: false,
            content: String::new(),
        });
    };

    let (line_count, lines) = read_tail_lines(&path, max_lines)?;
    Ok(DebugLogSnapshot {
        enabled: DEBUG_ENABLED.load(Ordering::SeqCst),
        log_path: path.to_string_lossy().to_string(),
        session_id,
        started_at,
        line_count,
        max_lines,
        truncated: line_count > max_lines,
        content: lines.join("\n"),
    })
}

pub fn clear_log() -> AppResult<DebugLogSnapshot> {
    let Some(path) = log_path() else {
        return snapshot(DEFAULT_TAIL_LINES);
    };
    if let Some(parent) = path.parent() {
        fs_util::ensure_dir(parent)?;
    }
    fs::write(&path, [])?;
    write_entry(DebugLevel::Info, "调试日志已清空", true);
    snapshot(DEFAULT_TAIL_LINES)
}

pub fn open_log_dir() -> AppResult<()> {
    let Some(path) = log_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs_util::ensure_dir(parent)?;
        fs_util::open_path(parent)?;
    }
    Ok(())
}

fn write_entry(level: DebugLevel, message: impl AsRef<str>, force: bool) {
    if !force && !DEBUG_ENABLED.load(Ordering::SeqCst) {
        return;
    }
    let Some(path) = log_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = rotate_if_large(&path);

    let message = redact_sensitive(message.as_ref());
    emit_entry(&level, &message);

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let pid = std::process::id();
    let line = format!("[{timestamp}] [{}] [pid:{pid}] {message}", level.label());
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

fn log_path() -> Option<PathBuf> {
    LOG_STATE
        .lock()
        .ok()
        .and_then(|state| state.log_path.clone())
}

fn archive_current_log(path: &Path) -> AppResult<()> {
    if !path.exists() || path.metadata()?.len() == 0 {
        return Ok(());
    }
    let archive_name = format!("debug-{}.log", Local::now().format("%Y%m%d-%H%M%S"));
    let archive_path = path.with_file_name(archive_name);
    fs::rename(path, archive_path)?;
    Ok(())
}

fn rotate_if_large(path: &Path) -> AppResult<()> {
    if !path.exists() || path.metadata()?.len() <= MAX_LOG_BYTES {
        return Ok(());
    }
    archive_current_log(path)?;
    if let Some(parent) = path.parent() {
        prune_archives(parent)?;
    }
    Ok(())
}

fn prune_archives(dir: &Path) -> AppResult<()> {
    let mut archives = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|value| value.to_str())
                .map(|name| name.starts_with("debug-") && name.ends_with(".log"))
                .unwrap_or(false)
        })
        .filter_map(|path| {
            let modified = path.metadata().and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect::<Vec<_>>();
    archives.sort_by_key(|(modified, _)| *modified);

    while archives.len() > MAX_ARCHIVES {
        if let Some((_, path)) = archives.first() {
            let _ = fs::remove_file(path);
        }
        archives.remove(0);
    }
    Ok(())
}

fn read_tail_lines(path: &Path, max_lines: usize) -> AppResult<(usize, Vec<String>)> {
    if !path.exists() {
        return Ok((0, Vec::new()));
    }

    let file = fs::File::open(path)?;
    let mut line_count = 0_usize;
    let mut tail = VecDeque::with_capacity(max_lines);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        line_count += 1;
        if tail.len() == max_lines {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    Ok((line_count, tail.into_iter().collect()))
}

fn redact_sensitive(message: &str) -> String {
    let value = url_credentials_regex()
        .replace_all(message, "://$1:***@")
        .to_string();
    sensitive_query_regex()
        .replace_all(&value, "$1=***")
        .to_string()
}

fn url_credentials_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"://([^:/@\s]+):([^/@\s]+)@").expect("valid regex"))
}

fn sensitive_query_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)\b(token|password|passwd|secret|access_token)=([^\s&]+)")
            .expect("valid regex")
    })
}



#[cfg(test)]
mod tests {
    use super::redact_sensitive;

    #[test]
    fn redacts_url_credentials_and_sensitive_query_values() {
        let value =
            redact_sensitive("proxy https://user:secret@example.com?token=abc&password=hidden");

        assert_eq!(
            value,
            "proxy https://user:***@example.com?token=***&password=***"
        );
    }
}
