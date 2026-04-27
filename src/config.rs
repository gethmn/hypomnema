use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer};
use tracing::Level;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPath(pub PathBuf);

impl<'de> Deserialize<'de> for ConfigPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(ConfigPath(expand_tilde(Path::new(&raw))))
    }
}

impl AsRef<Path> for ConfigPath {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

pub fn expand_tilde(p: &Path) -> PathBuf {
    let s = match p.to_str() {
        Some(s) => s,
        None => return p.to_path_buf(),
    };
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    if s == "~" {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    p.to_path_buf()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub vault: ConfigPath,
    #[serde(default)]
    pub http: HttpConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub watcher: WatcherConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    #[serde(default = "default_http_bind")]
    pub bind: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: default_http_bind(),
        }
    }
}

fn default_http_bind() -> String {
    "127.0.0.1:7777".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    #[serde(default = "default_mcp_transport")]
    pub transport: String,
    #[serde(default = "default_mcp_socket")]
    pub socket: ConfigPath,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: default_mcp_transport(),
            socket: default_mcp_socket(),
        }
    }
}

fn default_mcp_transport() -> String {
    "stdio".to_string()
}

fn default_mcp_socket() -> ConfigPath {
    ConfigPath(expand_tilde(Path::new("~/.local/share/hypomnema/mcp.sock")))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimension")]
    pub dimension: u32,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_embedding_extension_path")]
    pub extension_path: ConfigPath,
    #[serde(default = "default_embedding_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_embedding_max_retries")]
    pub max_retries: u8,
    #[serde(default = "default_embedding_batch_size")]
    pub batch_size: u8,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            endpoint: default_embedding_endpoint(),
            model: default_embedding_model(),
            dimension: default_embedding_dimension(),
            api_key: String::new(),
            extension_path: default_embedding_extension_path(),
            timeout_ms: default_embedding_timeout_ms(),
            max_retries: default_embedding_max_retries(),
            batch_size: default_embedding_batch_size(),
        }
    }
}

impl EmbeddingConfig {
    /// Returns the sqlite-vec extension path with the `HYPOMNEMA_VEC_EXT_PATH`
    /// env-var override applied. The override wins over the configured path.
    pub fn resolved_extension_path(&self) -> PathBuf {
        if let Ok(p) = env::var(VEC_EXT_PATH_ENV) {
            return PathBuf::from(p);
        }
        self.extension_path.0.clone()
    }
}

pub const VEC_EXT_PATH_ENV: &str = "HYPOMNEMA_VEC_EXT_PATH";

fn default_embedding_endpoint() -> String {
    "http://127.0.0.1:8080/v1/embeddings".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text-v1.5".to_string()
}

fn default_embedding_dimension() -> u32 {
    768
}

fn default_embedding_extension_path() -> ConfigPath {
    let filename = format!("sqlite-vec.{}", platform_extension_suffix());
    ConfigPath(expand_tilde(
        &Path::new("~/.local/share/hypomnema").join(filename),
    ))
}

fn default_embedding_timeout_ms() -> u64 {
    30_000
}

fn default_embedding_max_retries() -> u8 {
    1
}

fn default_embedding_batch_size() -> u8 {
    1
}

fn platform_extension_suffix() -> &'static str {
    if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatcherConfig {
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    #[serde(default = "default_ignore_patterns")]
    pub ignore_patterns: Vec<String>,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            ignore_patterns: default_ignore_patterns(),
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}

fn default_ignore_patterns() -> Vec<String> {
    vec![
        ".git/**".to_string(),
        ".obsidian/**".to_string(),
        ".trash/**".to_string(),
        "*.sync-conflict-*".to_string(),
        "**/*.tmp".to_string(),
    ]
}

impl WatcherConfig {
    pub fn compiled_ignores(&self) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.ignore_patterns {
            let glob = Glob::new(pattern).with_context(|| {
                format!("invalid ignore pattern in watcher.ignore_patterns: {pattern:?}")
            })?;
            builder.add(glob);
        }
        builder
            .build()
            .context("compiling watcher.ignore_patterns into GlobSet")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: ConfigPath,
    #[serde(default = "default_index_file")]
    pub index_file: String,
    #[serde(default = "default_outbox_file")]
    pub outbox_file: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            index_file: default_index_file(),
            outbox_file: default_outbox_file(),
        }
    }
}

fn default_data_dir() -> ConfigPath {
    ConfigPath(expand_tilde(Path::new("~/.local/share/hypomnema")))
}

fn default_index_file() -> String {
    "index.sqlite".to_string()
}

fn default_outbox_file() -> String {
    "outbox.jsonl".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "default_logging_level")]
    pub level: String,
    #[serde(default = "default_notify_level")]
    pub notify_level: String,
    #[serde(default = "default_tokio_level")]
    pub tokio_level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_logging_level(),
            notify_level: default_notify_level(),
            tokio_level: default_tokio_level(),
        }
    }
}

fn default_logging_level() -> String {
    "info".to_string()
}

fn default_notify_level() -> String {
    "warn".to_string()
}

fn default_tokio_level() -> String {
    "error".to_string()
}

impl Config {
    pub fn load(path: Option<&Path>) -> Result<Config> {
        let resolved = match path {
            Some(p) => p.to_path_buf(),
            None => default_config_path()?,
        };
        let text = fs::read_to_string(&resolved)
            .with_context(|| format!("reading config from {}", resolved.display()))?;
        let mut config: Config = toml::from_str(&text)
            .with_context(|| format!("parsing TOML config at {}", resolved.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&mut self) -> Result<()> {
        let vault = &self.vault.0;
        let canonical_vault = fs::canonicalize(vault).with_context(|| {
            format!(
                "vault path is not accessible: {} (does it exist?)",
                vault.display()
            )
        })?;
        let meta = fs::metadata(&canonical_vault)
            .with_context(|| format!("reading metadata for vault {}", canonical_vault.display()))?;
        if !meta.is_dir() {
            bail!(
                "vault path must be a directory: {}",
                canonical_vault.display()
            );
        }
        fs::read_dir(&canonical_vault).with_context(|| {
            format!(
                "vault directory is not readable: {}",
                canonical_vault.display()
            )
        })?;
        self.vault.0 = canonical_vault.clone();

        let resolved_data_dir = resolve_existing_ancestors(&self.storage.data_dir.0);
        if resolved_data_dir.starts_with(&canonical_vault) {
            bail!(
                "storage.data_dir ({}) must not be under vault ({}). See ADR-0006.",
                self.storage.data_dir.0.display(),
                canonical_vault.display()
            );
        }

        parse_level(&self.logging.level, "logging.level")?;
        parse_level(&self.logging.notify_level, "logging.notify_level")?;
        parse_level(&self.logging.tokio_level, "logging.tokio_level")?;

        Ok(())
    }

    #[cfg(test)]
    pub fn default_for_smoke_test(vault: PathBuf) -> Self {
        Self {
            vault: ConfigPath(vault),
            http: HttpConfig::default(),
            mcp: McpConfig::default(),
            embedding: EmbeddingConfig::default(),
            watcher: WatcherConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

fn parse_level(s: &str, field: &str) -> Result<Level> {
    s.parse::<Level>()
        .map_err(|e| anyhow!("{field} = \"{s}\" is not a valid tracing level: {e}"))
}

fn default_config_path() -> Result<PathBuf> {
    if let Ok(p) = env::var("HYPOMNEMA_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("hypomnema/config.toml"));
    }
    let home =
        env::var("HOME").context("no HYPOMNEMA_CONFIG, no XDG_CONFIG_HOME, and HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/hypomnema/config.toml"))
}

fn resolve_existing_ancestors(p: &Path) -> PathBuf {
    let mut suffix: Vec<OsString> = Vec::new();
    let mut current = p.to_path_buf();
    while !current.exists() {
        let name = match current.file_name() {
            Some(n) => n.to_os_string(),
            None => return p.to_path_buf(),
        };
        let parent = match current.parent() {
            Some(par) => par.to_path_buf(),
            None => return p.to_path_buf(),
        };
        if parent.as_os_str().is_empty() {
            return p.to_path_buf();
        }
        suffix.push(name);
        current = parent;
    }
    let mut resolved = match fs::canonicalize(&current) {
        Ok(c) => c,
        Err(_) => return p.to_path_buf(),
    };
    for name in suffix.into_iter().rev() {
        resolved.push(name);
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_handles_leading_tilde_slash() {
        // SAFETY: tests in this crate may run in parallel. We read HOME, not write.
        if let Ok(home) = env::var("HOME") {
            let expanded = expand_tilde(Path::new("~/foo/bar"));
            assert_eq!(expanded, PathBuf::from(home).join("foo/bar"));
        }
    }

    #[test]
    fn expand_tilde_leaves_other_paths_alone() {
        assert_eq!(
            expand_tilde(Path::new("/abs/path")),
            PathBuf::from("/abs/path")
        );
        assert_eq!(
            expand_tilde(Path::new("rel/path")),
            PathBuf::from("rel/path")
        );
        assert_eq!(
            expand_tilde(Path::new("~tilde-user")),
            PathBuf::from("~tilde-user")
        );
    }

    #[test]
    fn parse_level_accepts_standard_levels() {
        for s in ["trace", "debug", "info", "warn", "error", "INFO", "Warn"] {
            parse_level(s, "test").unwrap_or_else(|_| panic!("expected {s} to parse"));
        }
    }

    #[test]
    fn parse_level_rejects_garbage() {
        assert!(parse_level("plaid", "test").is_err());
    }

    #[test]
    fn smoke_default_uses_caller_vault() {
        let cfg = Config::default_for_smoke_test(PathBuf::from("/tmp/smoke-vault"));
        assert_eq!(cfg.vault.0, PathBuf::from("/tmp/smoke-vault"));
        assert_eq!(cfg.http.bind, "127.0.0.1:7777");
        assert_eq!(cfg.embedding.dimension, 768);
        assert_eq!(cfg.logging.level, "info");
    }
}
