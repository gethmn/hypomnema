use std::env;

use anyhow::{Context, Result};
use tracing::Level;
use tracing_subscriber::EnvFilter;

use crate::config::LoggingConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryKind {
    Hmnd,
    Hmn,
    HmnMcp,
}

pub fn init(config: &LoggingConfig, verbose: u8, binary: BinaryKind) -> Result<()> {
    let directive = compose_filter(
        config,
        verbose,
        binary,
        env::var("RUST_LOG").ok().as_deref(),
    );
    let env_filter = EnvFilter::try_new(&directive)
        .with_context(|| format!("invalid log filter directive: {directive}"))?;

    let json_format = matches!(env::var("HYPOMNEMA_LOG_FORMAT").as_deref(), Ok("json"));

    // Discard try_init's error: it fires when a subscriber is already installed,
    // which happens when tests in the same process call init more than once.
    match binary {
        BinaryKind::HmnMcp => {
            // Stdout is owned by the MCP transport in this mode; route logs to
            // stderr and disable ANSI to avoid polluting the JSON-RPC framing.
            let _ = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .try_init();
        }
        BinaryKind::Hmnd | BinaryKind::Hmn => {
            if json_format {
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .json()
                    .try_init();
            } else {
                let _ = tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .try_init();
            }
        }
    }

    Ok(())
}

pub(crate) fn compose_filter(
    config: &LoggingConfig,
    verbose: u8,
    binary: BinaryKind,
    rust_log: Option<&str>,
) -> String {
    if let Some(rl) = rust_log {
        return rl.to_string();
    }

    match binary {
        BinaryKind::Hmnd => {
            let base = config.level.parse::<Level>().unwrap_or(Level::INFO);
            let bumped = level_str(bump(base, verbose));
            format!(
                "hypomnema={bumped},hmnd={bumped},notify={notify},tokio={tokio}",
                notify = config.notify_level,
                tokio = config.tokio_level,
            )
        }
        BinaryKind::Hmn => {
            let bumped = level_str(bump(Level::WARN, verbose));
            format!("error,hypomnema={bumped},hmn={bumped}")
        }
        BinaryKind::HmnMcp => {
            let bumped = level_str(bump(Level::WARN, verbose));
            format!("error,hypomnema={bumped},hmn={bumped}")
        }
    }
}

fn bump(base: Level, verbose: u8) -> Level {
    let mut level = base;
    for _ in 0..verbose {
        level = match level {
            Level::ERROR => Level::WARN,
            Level::WARN => Level::INFO,
            Level::INFO => Level::DEBUG,
            Level::DEBUG | Level::TRACE => Level::TRACE,
        };
    }
    level
}

fn level_str(level: Level) -> &'static str {
    match level {
        Level::ERROR => "error",
        Level::WARN => "warn",
        Level::INFO => "info",
        Level::DEBUG => "debug",
        Level::TRACE => "trace",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cfg() -> LoggingConfig {
        LoggingConfig::default()
    }

    #[test]
    fn hmnd_default_filter() {
        let s = compose_filter(&default_cfg(), 0, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=info,hmnd=info,notify=warn,tokio=error");
    }

    #[test]
    fn hmn_default_filter() {
        let s = compose_filter(&default_cfg(), 0, BinaryKind::Hmn, None);
        assert_eq!(s, "error,hypomnema=warn,hmn=warn");
    }

    #[test]
    fn hmnd_v_bumps_hypomnema_and_hmnd_targets() {
        let s = compose_filter(&default_cfg(), 1, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=debug,hmnd=debug,notify=warn,tokio=error");
    }

    #[test]
    fn hmnd_vv_promotes_both_targets_to_trace() {
        let s = compose_filter(&default_cfg(), 2, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=trace,hmnd=trace,notify=warn,tokio=error");
    }

    #[test]
    fn hmnd_vvv_caps_at_trace() {
        let s = compose_filter(&default_cfg(), 3, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=trace,hmnd=trace,notify=warn,tokio=error");
    }

    #[test]
    fn hmn_verbose_walks_warn_info_debug_trace() {
        let cfg = default_cfg();
        assert_eq!(
            compose_filter(&cfg, 0, BinaryKind::Hmn, None),
            "error,hypomnema=warn,hmn=warn"
        );
        assert_eq!(
            compose_filter(&cfg, 1, BinaryKind::Hmn, None),
            "error,hypomnema=info,hmn=info"
        );
        assert_eq!(
            compose_filter(&cfg, 2, BinaryKind::Hmn, None),
            "error,hypomnema=debug,hmn=debug"
        );
        assert_eq!(
            compose_filter(&cfg, 3, BinaryKind::Hmn, None),
            "error,hypomnema=trace,hmn=trace"
        );
    }

    #[test]
    fn rust_log_overrides_entire_directive() {
        let s = compose_filter(
            &default_cfg(),
            5,
            BinaryKind::Hmnd,
            Some("my_crate=trace,other=info"),
        );
        assert_eq!(s, "my_crate=trace,other=info");
    }

    #[test]
    fn hmnd_honors_config_overrides() {
        let cfg = LoggingConfig {
            level: "warn".to_string(),
            notify_level: "info".to_string(),
            tokio_level: "warn".to_string(),
        };
        let s = compose_filter(&cfg, 0, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=warn,hmnd=warn,notify=info,tokio=warn");
    }

    #[test]
    fn hmnd_verbose_walks_from_warn_base() {
        let cfg = LoggingConfig {
            level: "warn".to_string(),
            ..LoggingConfig::default()
        };
        let s = compose_filter(&cfg, 1, BinaryKind::Hmnd, None);
        assert_eq!(s, "hypomnema=info,hmnd=info,notify=warn,tokio=error");
    }

    #[test]
    fn hmn_mcp_filter_matches_hmn() {
        let s = compose_filter(&default_cfg(), 0, BinaryKind::HmnMcp, None);
        assert_eq!(s, "error,hypomnema=warn,hmn=warn");
    }

    #[test]
    fn composed_directive_parses_for_hmn_mcp() {
        for v in 0u8..=3 {
            let directive = compose_filter(&default_cfg(), v, BinaryKind::HmnMcp, None);
            EnvFilter::try_new(&directive)
                .unwrap_or_else(|e| panic!("directive {directive:?} failed to parse: {e}"));
        }
    }

    #[test]
    fn composed_directive_parses_as_envfilter() {
        for binary in [BinaryKind::Hmnd, BinaryKind::Hmn, BinaryKind::HmnMcp] {
            for v in 0u8..=3 {
                let directive = compose_filter(&default_cfg(), v, binary, None);
                EnvFilter::try_new(&directive)
                    .unwrap_or_else(|e| panic!("directive {directive:?} failed to parse: {e}"));
            }
        }
    }

    #[test]
    fn bumped_directive_parses_as_envfilter() {
        let cfg = LoggingConfig {
            level: "warn".to_string(),
            notify_level: "info".to_string(),
            tokio_level: "debug".to_string(),
        };
        for binary in [BinaryKind::Hmnd, BinaryKind::Hmn, BinaryKind::HmnMcp] {
            for v in 0u8..=3 {
                let directive = compose_filter(&cfg, v, binary, None);
                EnvFilter::try_new(&directive)
                    .unwrap_or_else(|e| panic!("directive {directive:?} failed to parse: {e}"));
            }
        }
    }

    #[test]
    fn init_is_idempotent_within_a_process() {
        init(&LoggingConfig::default(), 0, BinaryKind::Hmnd).expect("first init");
        init(&LoggingConfig::default(), 0, BinaryKind::Hmnd).expect("second init");
    }
}
