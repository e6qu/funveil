use crate::error::Result;
use rand::Rng;
use std::path::Path;
use tracing::subscriber::set_global_default;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

const BASE62_CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const TRACE_ID_LEN: usize = 8;
const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024; // 5MB

/// Generate an 8-character base62 trace ID.
pub fn generate_trace_id() -> String {
    let mut rng = rand::thread_rng();
    (0..TRACE_ID_LEN)
        .map(|_| BASE62_CHARS[rng.gen_range(0..BASE62_CHARS.len())] as char)
        .collect()
}

/// Resolve log level with precedence: CLI > env `FUNVEIL_LOG` > config > default `warn`.
pub fn resolve_log_level(cli: Option<&str>, config: Option<&str>) -> LevelFilter {
    if let Some(level) = cli.and_then(parse_level) {
        return level;
    }
    if let Some(level) = std::env::var("FUNVEIL_LOG")
        .ok()
        .as_deref()
        .and_then(parse_level)
    {
        return level;
    }
    if let Some(level) = config.and_then(parse_level) {
        return level;
    }
    LevelFilter::WARN
}

fn parse_level(s: &str) -> Option<LevelFilter> {
    match s.to_lowercase().as_str() {
        "trace" => Some(LevelFilter::TRACE),
        "debug" => Some(LevelFilter::DEBUG),
        "info" => Some(LevelFilter::INFO),
        "warn" => Some(LevelFilter::WARN),
        "error" => Some(LevelFilter::ERROR),
        "off" => Some(LevelFilter::OFF),
        _ => None,
    }
}

/// Map a command name to its category.
pub fn command_category(name: &str) -> &'static str {
    match name {
        "veil" | "unveil" | "apply" | "restore" | "checkpoint" | "gc" | "clean" => "operation",
        _ => "meta",
    }
}

/// Initialize the tracing subscriber with a JSON file writer.
///
/// Returns a `WorkerGuard` that must be held alive for the duration of the program
/// to ensure all log records are flushed.
pub fn init_tracing(root: &Path, level: LevelFilter) -> Result<WorkerGuard> {
    let logs_dir = root.join(crate::config::LOGS_DIR);
    std::fs::create_dir_all(&logs_dir)?;

    let log_file = logs_dir.join("funveil.log");
    if log_file.exists() {
        if let Ok(meta) = std::fs::metadata(&log_file) {
            if meta.len() > MAX_LOG_SIZE {
                let _ = std::fs::remove_file(&log_file);
            }
        }
    }

    let file_appender = tracing_appender::rolling::never(&logs_dir, "funveil.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let json_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_timer(fmt::time::UtcTime::rfc_3339())
        .with_span_list(true)
        .with_target(false);

    let filter = tracing_subscriber::filter::EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    let subscriber = tracing_subscriber::registry().with(json_layer).with(filter);

    set_global_default(subscriber).ok();

    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_trace_id_length() {
        let id = generate_trace_id();
        assert_eq!(id.len(), 8);
    }

    #[test]
    fn test_generate_trace_id_charset() {
        let id = generate_trace_id();
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_generate_trace_id_unique() {
        let ids: std::collections::HashSet<String> =
            (0..100).map(|_| generate_trace_id()).collect();
        assert_eq!(ids.len(), 100);
    }

    #[test]
    fn test_resolve_log_level_cli_wins() {
        assert_eq!(
            resolve_log_level(Some("debug"), Some("error")),
            LevelFilter::DEBUG
        );
    }

    #[test]
    fn test_resolve_log_level_config_fallback() {
        assert_eq!(resolve_log_level(None, Some("info")), LevelFilter::INFO);
    }

    #[test]
    fn test_resolve_log_level_default() {
        assert_eq!(resolve_log_level(None, None), LevelFilter::WARN);
    }

    #[test]
    fn test_resolve_log_level_invalid_ignored() {
        assert_eq!(
            resolve_log_level(Some("nonsense"), Some("error")),
            LevelFilter::ERROR
        );
    }

    #[test]
    fn test_command_category_operations() {
        assert_eq!(command_category("veil"), "operation");
        assert_eq!(command_category("unveil"), "operation");
        assert_eq!(command_category("apply"), "operation");
        assert_eq!(command_category("restore"), "operation");
        assert_eq!(command_category("checkpoint"), "operation");
        assert_eq!(command_category("gc"), "operation");
        assert_eq!(command_category("clean"), "operation");
    }

    #[test]
    fn test_command_category_meta() {
        assert_eq!(command_category("init"), "meta");
        assert_eq!(command_category("mode"), "meta");
        assert_eq!(command_category("status"), "meta");
        assert_eq!(command_category("parse"), "meta");
        assert_eq!(command_category("trace"), "meta");
        assert_eq!(command_category("entrypoints"), "meta");
        assert_eq!(command_category("cache"), "meta");
        assert_eq!(command_category("doctor"), "meta");
        assert_eq!(command_category("show"), "meta");
    }
}
