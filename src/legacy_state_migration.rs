//! Legacy v0 single-vault state migration.
//!
//! Per step-9 workplan § Resolution B: on first startup with a populated
//! top-level `[vault]` config block and an empty `vaults.sqlite`, auto-create
//! one registry row using `default_vault_name` and the legacy `vault.path`,
//! atomically rename `<data_dir>/index.sqlite{,-wal,-shm}` into
//! `<data_dir>/vaults/<id>/`, and write a per-vault `meta.toml`.
//!
//! Idempotency / crash safety: the function is safe to run on every startup.
//! - If the registry already has rows, it skips the auto-create step.
//! - The rename pass is per-file: only renames when source exists and
//!   destination does not, so a partial-rename crash recovers cleanly on the
//!   next start (the three `rename(2)` calls are atomic per-file on POSIX,
//!   and the cross-file consistency window has a single deterministic
//!   recovery action — re-run this function).
//!
//! Resolution E Case 1: when the legacy `[vault]` path is inaccessible, the
//! auto-create step inserts the row with `status = errored` and a populated
//! `last_error`. The daemon's reconcile pass then skips spawning a watcher /
//! indexer for it.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::{info, warn};

use crate::config::Config;
use crate::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus, vault_data_dir};

/// Detect legacy v0 state and migrate it into the per-vault layout.
///
/// Returns `Ok(())` whenever the function ran cleanly (including the no-op
/// case). Errors are surfaced via `anyhow::Error` and abort daemon startup.
pub async fn run_if_needed(config: &Config, registry: &VaultRegistry) -> Result<()> {
    // Step 1: if registry is empty AND `[vault]` is present, auto-create a
    // single row for the legacy path. Best-effort canonicalize; an
    // inaccessible legacy path becomes an `errored` row (Resolution E Case 1).
    let rows = registry
        .list()
        .await
        .context("listing existing vault rows")?;
    if rows.is_empty() {
        if let Some(legacy) = &config.vault {
            warn!(
                "top-level [vault] config block is deprecated; vaults are now \
                 managed via vaults.sqlite. Remove the [vault] block from your \
                 config when convenient (it will continue to be honoured for \
                 first-run auto-migration only)."
            );

            let original = legacy.0.clone();
            let canonical = fs::canonicalize(&original).ok();
            let path = canonical.clone().unwrap_or_else(|| original.clone());
            let (status, last_error) = classify_path(&path);

            let id = VaultId::new();
            let row = VaultRow {
                id: id.clone(),
                name: config.default_vault_name.clone(),
                path: path.clone(),
                status,
                created_at: Utc::now(),
                last_error: last_error.clone(),
            };
            registry
                .insert(row.clone())
                .await
                .context("inserting auto-migrated vault row")?;

            info!(
                vault_id = %id,
                vault_name = %row.name,
                vault_path = %row.path.display(),
                vault_status = %row.status.as_str(),
                "legacy_state_migration: auto-created registry row from [vault] config"
            );

            // Best-effort: create the per-vault subdirectory + write meta.toml
            // even for errored rows. The directory makes downstream rename
            // logic uniform; meta.toml is ADR-0010's human-readable mirror.
            let vault_dir = vault_data_dir(&config.storage.data_dir.0, &id);
            fs::create_dir_all(&vault_dir)
                .with_context(|| format!("creating per-vault directory {}", vault_dir.display()))?;
            write_meta_toml(&vault_dir, &row)
                .with_context(|| format!("writing meta.toml under {}", vault_dir.display()))?;
        }
    }

    // Step 2: rename completion pass. Idempotent: only renames when source
    // exists at the legacy data_dir-root path AND destination does not yet
    // exist at the per-vault subdir. Crash-mid-rename across the four files
    // recovers naturally on the next call.
    let rows = registry.list().await.context("re-listing vault rows")?;
    rename_legacy_files_into_vault(config, &rows)?;

    Ok(())
}

fn classify_path(path: &Path) -> (VaultStatus, Option<String>) {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => match fs::read_dir(path) {
            Ok(_) => (VaultStatus::Active, None),
            Err(e) => (
                VaultStatus::Errored,
                Some(format!(
                    "vault path {} is not readable: {}",
                    path.display(),
                    e
                )),
            ),
        },
        Ok(_) => (
            VaultStatus::Errored,
            Some(format!("vault path {} is not a directory", path.display())),
        ),
        Err(e) => (
            VaultStatus::Errored,
            Some(format!(
                "vault path {} not accessible: {}",
                path.display(),
                e
            )),
        ),
    }
}

fn rename_legacy_files_into_vault(config: &Config, rows: &[VaultRow]) -> Result<()> {
    let data_dir = &config.storage.data_dir.0;
    let index_file = &config.storage.index_file;

    let candidates: Vec<String> = vec![
        index_file.clone(),
        format!("{index_file}-wal"),
        format!("{index_file}-shm"),
    ];

    let any_legacy_present = candidates.iter().any(|name| data_dir.join(name).exists());
    if !any_legacy_present {
        return Ok(());
    }

    // Pick the migration target. The unambiguous case is exactly one
    // registered vault — use it. With more than one registered vault, the
    // legacy data is ambiguous; log a warning and leave the legacy files in
    // place rather than guessing.
    let target = match rows.len() {
        0 => {
            warn!(
                data_dir = %data_dir.display(),
                "legacy_state_migration: legacy index files exist under data_dir but no \
                 registry rows are present. Skipping rename."
            );
            return Ok(());
        }
        1 => &rows[0],
        _ => {
            warn!(
                data_dir = %data_dir.display(),
                row_count = rows.len(),
                "legacy_state_migration: legacy index files exist under data_dir but \
                 multiple registry rows are present — cannot disambiguate target. Skipping rename."
            );
            return Ok(());
        }
    };

    let target_dir = vault_data_dir(data_dir, &target.id);
    fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "creating per-vault directory {} for legacy rename",
            target_dir.display()
        )
    })?;

    for name in &candidates {
        let src = data_dir.join(name);
        let dst = target_dir.join(name);
        if src.exists() {
            if dst.exists() {
                warn!(
                    src = %src.display(),
                    dst = %dst.display(),
                    "legacy_state_migration: both legacy and per-vault file exist; leaving as-is. \
                     Operator should remove the legacy file at {} when ready.",
                    src.display()
                );
                continue;
            }
            fs::rename(&src, &dst)
                .with_context(|| format!("atomic rename {} -> {}", src.display(), dst.display()))?;
            info!(
                src = %src.display(),
                dst = %dst.display(),
                "legacy_state_migration: renamed legacy file into per-vault subdirectory"
            );
        }
    }

    Ok(())
}

pub(crate) fn write_meta_toml(vault_dir: &Path, row: &VaultRow) -> Result<()> {
    let mut content = String::new();
    content.push_str("# Auto-generated by hmnd at vault registration time.\n");
    content.push_str("# This file is informational; vaults.sqlite is authoritative.\n");
    content.push_str(&format!("id = \"{}\"\n", row.id));
    content.push_str(&format!("name = \"{}\"\n", toml_escape(&row.name)));
    content.push_str(&format!(
        "path = \"{}\"\n",
        toml_escape(&row.path.display().to_string())
    ));
    content.push_str(&format!("status = \"{}\"\n", row.status.as_str()));
    content.push_str(&format!(
        "created_at = \"{}\"\n",
        row.created_at
            .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
    ));
    if let Some(err) = &row.last_error {
        content.push_str(&format!("last_error = \"{}\"\n", toml_escape(err)));
    }
    let path: PathBuf = vault_dir.join("meta.toml");
    fs::write(&path, content)
        .with_context(|| format!("writing meta.toml at {}", path.display()))?;
    Ok(())
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, ConfigPath, EmbeddingConfig, HttpConfig, LoggingConfig, McpConfig, StorageConfig,
        WatcherConfig,
    };
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_config(vault: Option<PathBuf>, data_dir: PathBuf) -> Config {
        Config {
            vault: vault.map(ConfigPath),
            http: HttpConfig::default(),
            mcp: McpConfig::default(),
            embedding: EmbeddingConfig::default(),
            watcher: WatcherConfig::default(),
            storage: StorageConfig {
                data_dir: ConfigPath(data_dir),
                index_file: "index.sqlite".to_string(),
            },
            logging: LoggingConfig::default(),
            default_vault_name: "default".to_string(),
        }
    }

    #[tokio::test]
    async fn no_op_when_vault_absent_and_registry_empty() {
        let root = TempDir::new().unwrap();
        let data_dir = root.path().join("data");
        let config = make_config(None, data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();
        run_if_needed(&config, &registry).await.unwrap();
        assert!(registry.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn auto_creates_active_row_when_legacy_vault_accessible() {
        let root = TempDir::new().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = root.path().join("data");
        let config = make_config(Some(vault.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();
        run_if_needed(&config, &registry).await.unwrap();

        let rows = registry.list().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "default");
        assert_eq!(rows[0].status, VaultStatus::Active);
        assert!(rows[0].last_error.is_none());

        let target = vault_data_dir(&data_dir, &rows[0].id);
        assert!(target.is_dir(), "per-vault dir created");
        assert!(target.join("meta.toml").is_file(), "meta.toml written");
    }

    #[tokio::test]
    async fn auto_creates_errored_row_when_legacy_vault_inaccessible() {
        let root = TempDir::new().unwrap();
        let data_dir = root.path().join("data");
        let bogus = root.path().join("does_not_exist");
        let config = make_config(Some(bogus.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();
        run_if_needed(&config, &registry).await.unwrap();

        let rows = registry.list().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, VaultStatus::Errored);
        let last_err = rows[0].last_error.as_ref().expect("last_error populated");
        assert!(
            last_err.contains("not accessible") || last_err.contains("not a directory"),
            "expected accessibility text in last_error: {last_err}"
        );

        let target = vault_data_dir(&data_dir, &rows[0].id);
        assert!(target.is_dir(), "per-vault dir created even for errored");
        assert!(target.join("meta.toml").is_file(), "meta.toml written");
    }

    #[tokio::test]
    async fn skips_auto_create_when_registry_already_populated() {
        let root = TempDir::new().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = root.path().join("data");
        let config = make_config(Some(vault.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();

        // Pre-populate.
        let pre_id = VaultId::new();
        registry
            .insert(VaultRow {
                id: pre_id.clone(),
                name: "preexisting".to_string(),
                path: vault.clone(),
                status: VaultStatus::Active,
                created_at: Utc::now(),
                last_error: None,
            })
            .await
            .unwrap();

        run_if_needed(&config, &registry).await.unwrap();

        let rows = registry.list().await.unwrap();
        assert_eq!(rows.len(), 1, "no second row inserted");
        assert_eq!(rows[0].id, pre_id);
    }

    #[tokio::test]
    async fn renames_legacy_index_into_vault_dir() {
        let root = TempDir::new().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        // Pre-populate legacy files at data_dir root.
        for name in &["index.sqlite", "index.sqlite-wal"] {
            let mut f = fs::File::create(data_dir.join(name)).unwrap();
            writeln!(f, "legacy content of {name}").unwrap();
        }

        let config = make_config(Some(vault.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();
        run_if_needed(&config, &registry).await.unwrap();

        let rows = registry.list().await.unwrap();
        let target = vault_data_dir(&data_dir, &rows[0].id);
        assert!(target.join("index.sqlite").is_file());
        assert!(target.join("index.sqlite-wal").is_file());
        assert!(!data_dir.join("index.sqlite").exists());
        assert!(!data_dir.join("index.sqlite-wal").exists());
    }

    #[tokio::test]
    async fn rename_pass_is_idempotent_under_partial_crash() {
        // Simulate a crash mid-rename: index.sqlite already moved, wal still
        // at legacy location. Re-running should complete cleanly.
        let root = TempDir::new().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = root.path().join("data");
        fs::create_dir_all(&data_dir).unwrap();

        let config = make_config(Some(vault.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();

        // Pre-create a registry row so the second invocation has a target.
        let id = VaultId::new();
        registry
            .insert(VaultRow {
                id: id.clone(),
                name: "default".to_string(),
                path: vault.clone(),
                status: VaultStatus::Active,
                created_at: Utc::now(),
                last_error: None,
            })
            .await
            .unwrap();
        let target = vault_data_dir(&data_dir, &id);
        fs::create_dir_all(&target).unwrap();

        // Pretend index.sqlite already finished its rename; wal is mid-flight.
        fs::write(target.join("index.sqlite"), b"already-moved").unwrap();
        fs::write(data_dir.join("index.sqlite-wal"), b"legacy-wal").unwrap();

        run_if_needed(&config, &registry).await.unwrap();

        assert!(target.join("index.sqlite").is_file());
        assert!(target.join("index.sqlite-wal").is_file());
        assert!(!data_dir.join("index.sqlite-wal").exists());
    }

    #[tokio::test]
    async fn rename_pass_no_op_when_nothing_to_move() {
        let root = TempDir::new().unwrap();
        let vault = root.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let data_dir = root.path().join("data");
        let config = make_config(Some(vault.clone()), data_dir.clone());
        let registry = VaultRegistry::open(&data_dir).await.unwrap();
        run_if_needed(&config, &registry).await.unwrap();
        // Second run: registry already has the row, no legacy files left.
        run_if_needed(&config, &registry).await.unwrap();
        let rows = registry.list().await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn meta_toml_round_trips_through_toml_parse() {
        let tmp = TempDir::new().unwrap();
        let row = VaultRow {
            id: VaultId::from_string("abc-123".to_string()),
            name: "my \"quoted\" vault".to_string(),
            path: PathBuf::from("/tmp/vault"),
            status: VaultStatus::Active,
            created_at: Utc::now(),
            last_error: None,
        };
        write_meta_toml(tmp.path(), &row).unwrap();
        let body = fs::read_to_string(tmp.path().join("meta.toml")).unwrap();
        let parsed: toml::Value = toml::from_str(&body).expect("meta.toml parses as TOML");
        assert_eq!(parsed["id"].as_str(), Some("abc-123"));
        assert_eq!(parsed["name"].as_str(), Some("my \"quoted\" vault"));
        assert_eq!(parsed["status"].as_str(), Some("active"));
    }
}
