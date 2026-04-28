mod chunks;
mod pool;
mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use rusqlite::Connection;
use tokio::task;
use tracing::info;

use crate::config::{EmbeddingConfig, VEC_EXT_PATH_ENV};
use crate::vault_registry::{VaultId, vault_data_dir};

pub use chunks::rewrite_chunks_for_file;
pub use pool::SqlitePool;
pub use schema::MIGRATIONS;

#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    pool: SqlitePool,
    path: PathBuf,
    vault_id: VaultId,
}

impl Store {
    /// Open the per-vault index database for `vault_id`. The on-disk path is
    /// `<data_dir>/vaults/<vault_id>/<index_file>` per the step-9 § Goal
    /// recap layout; `data_dir` is the top-level `storage.data_dir`, not a
    /// pre-resolved per-vault directory. Creates the per-vault subdirectory
    /// idempotently on first open.
    pub async fn open(
        vault_id: &VaultId,
        data_dir: &Path,
        index_file: &str,
        embedding: &EmbeddingConfig,
    ) -> Result<Self> {
        let vault_id = vault_id.clone();
        let data_dir = data_dir.to_path_buf();
        let index_file = index_file.to_string();
        let extension_path = embedding.resolved_extension_path();
        let expected_dim = embedding.dimension;
        task::spawn_blocking(move || {
            open_blocking(vault_id, data_dir, index_file, extension_path, expected_dim)
        })
        .await
        .context("spawn_blocking join error in Store::open")?
    }

    pub fn pool(&self) -> SqlitePool {
        self.inner.pool.clone()
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn vault_id(&self) -> &VaultId {
        &self.inner.vault_id
    }
}

fn open_blocking(
    vault_id: VaultId,
    data_dir: PathBuf,
    index_file: String,
    extension_path: PathBuf,
    expected_dim: u32,
) -> Result<Store> {
    let per_vault_dir = vault_data_dir(&data_dir, &vault_id);
    fs::create_dir_all(&per_vault_dir)
        .with_context(|| format!("creating per-vault data dir {}", per_vault_dir.display()))?;
    if !extension_path.exists() {
        return Err(anyhow!(
            "sqlite-vec extension binary not found at {}. Set the {} env var to override the path, \
             or place the dylib at the configured location (see docs/reference/configuration.md \
             § embedding.extension_path).",
            extension_path.display(),
            VEC_EXT_PATH_ENV,
        ));
    }
    let db_path = per_vault_dir.join(&index_file);
    let pool = pool::build_pool(&db_path, &extension_path).with_context(|| {
        format!(
            "building connection pool for {} (sqlite-vec extension at {})",
            db_path.display(),
            extension_path.display()
        )
    })?;
    let mut conn = pool.get().with_context(|| {
        format!(
            "acquiring initial connection from pool for {}",
            db_path.display()
        )
    })?;
    schema::apply_migrations(&mut conn)
        .with_context(|| format!("applying migrations on {}", db_path.display()))?;
    validate_dimension(&conn, expected_dim)
        .with_context(|| format!("validating chunks_vec dimension on {}", db_path.display()))?;
    drop(conn);
    info!(
        vault_id = %vault_id,
        "store: opened {} (migrations applied; sqlite-vec extension at {})",
        db_path.display(),
        extension_path.display()
    );
    Ok(Store {
        inner: Arc::new(StoreInner {
            pool,
            path: db_path,
            vault_id,
        }),
    })
}

/// Probe `chunks_vec`'s CREATE statement and parse the schema-baked vector
/// dimension. Returns an error if the dimension does not equal `expected`.
///
/// The schema dimension is immutable for the life of the database file per
/// ADR-0007; the only repair is to delete the database and re-index, or to
/// adjust `embedding.dimension` in the config back to what the schema baked.
pub(crate) fn validate_dimension(conn: &Connection, expected: u32) -> Result<()> {
    let actual = read_chunks_vec_dimension(conn)?;
    if actual == expected {
        return Ok(());
    }
    Err(anyhow!(
        "embedding dimension mismatch: config.embedding.dimension = {expected}, \
         but the chunks_vec schema baked dimension {actual}. The schema is immutable \
         per ADR-0007; resolve by either (a) setting config.embedding.dimension = {actual}, \
         or (b) deleting the per-vault index file (storage.data_dir/vaults/<vault_id>/storage.index_file) \
         and re-indexing from scratch under the new dimension."
    ))
}

fn read_chunks_vec_dimension(conn: &Connection) -> Result<u32> {
    let create_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'chunks_vec'",
            [],
            |row| row.get(0),
        )
        .context("probing sqlite_master.sql for chunks_vec CREATE statement")?;
    let re = Regex::new(r"embedding\s+FLOAT\[(\d+)\]")
        .context("compiling regex for FLOAT[<dim>] parse")?;
    let caps = re
        .captures(&create_sql)
        .ok_or_else(|| anyhow!("could not locate `embedding FLOAT[<dim>]` in {create_sql:?}"))?;
    let n: u32 = caps[1]
        .parse()
        .with_context(|| format!("parsing chunks_vec dimension from {:?}", &caps[1]))?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmbeddingConfig;
    use tempfile::tempdir;

    fn embedding_for_tests() -> EmbeddingConfig {
        EmbeddingConfig::default()
    }

    async fn run_blocking<F, T>(f: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        task::spawn_blocking(f).await.unwrap()
    }

    #[tokio::test]
    async fn fresh_db_runs_migrations_to_target_version() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let v: i64 = run_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap()
        })
        .await;
        assert_eq!(v, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn reopen_does_not_re_run_migrations() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        {
            let _store = Store::open(
                &vault_id,
                dir.path(),
                "index.sqlite",
                &embedding_for_tests(),
            )
            .await
            .unwrap();
        }
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let v: i64 = run_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row("PRAGMA user_version", [], |row| row.get(0))
                .unwrap()
        })
        .await;
        assert_eq!(v, MIGRATIONS.len() as i64);
    }

    #[tokio::test]
    async fn files_table_exists_after_open() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let count: i64 = run_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'files'",
                [],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn wal_journal_mode_is_set() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let mode: String = run_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap()
        })
        .await;
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[tokio::test]
    async fn open_creates_missing_per_vault_data_dir() {
        // The top-level data_dir AND the per-vault subdirectory are both
        // created on demand. Confirm both legs of `<data_dir>/vaults/<id>/`.
        let parent = tempdir().unwrap();
        let nested = parent.path().join("nested/data_dir");
        assert!(!nested.exists());
        let vault_id = VaultId::new();
        let store = Store::open(&vault_id, &nested, "index.sqlite", &embedding_for_tests())
            .await
            .unwrap();
        let expected_per_vault = nested.join("vaults").join(vault_id.as_str());
        assert!(store.path().exists());
        assert_eq!(store.path(), &expected_per_vault.join("index.sqlite"));
        assert!(expected_per_vault.is_dir());
    }

    #[tokio::test]
    async fn validate_dimension_matches() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        run_blocking(move || {
            let conn = pool.get().unwrap();
            validate_dimension(&conn, 768).expect("dimension should match the baked 768");
        })
        .await;
    }

    #[tokio::test]
    async fn validate_dimension_mismatch_errors_with_path_and_values() {
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        let store = Store::open(
            &vault_id,
            dir.path(),
            "index.sqlite",
            &embedding_for_tests(),
        )
        .await
        .unwrap();
        let pool = store.pool();
        let err = run_blocking(move || {
            let conn = pool.get().unwrap();
            validate_dimension(&conn, 512)
                .expect_err("expected mismatch error for config 512 vs schema 768")
        })
        .await;
        let text = format!("{err:#}");
        assert!(
            text.contains("512"),
            "error should mention config dim 512: {text}"
        );
        assert!(
            text.contains("768"),
            "error should mention schema dim 768: {text}"
        );
        assert!(
            text.contains("ADR-0007"),
            "error should reference the ADR (resolution path): {text}"
        );
    }

    #[tokio::test]
    async fn store_open_fails_loudly_on_dimension_mismatch() {
        // Build the database under the default 768 dimension, then re-open with
        // a config claiming 512. Store::open() should refuse to return a Store.
        let dir = tempdir().unwrap();
        let vault_id = VaultId::new();
        {
            let _store = Store::open(
                &vault_id,
                dir.path(),
                "index.sqlite",
                &embedding_for_tests(),
            )
            .await
            .unwrap();
        }
        let bad = EmbeddingConfig {
            dimension: 512,
            ..EmbeddingConfig::default()
        };
        let err = match Store::open(&vault_id, dir.path(), "index.sqlite", &bad).await {
            Ok(_) => panic!("re-open with wrong dimension should fail"),
            Err(e) => e,
        };
        let text = format!("{err:#}");
        assert!(text.contains("512") && text.contains("768"), "{text}");
    }

    #[tokio::test]
    async fn two_stores_at_different_vault_ids_are_independent() {
        // Per-vault isolation: two stores at distinct vault_ids under the
        // same `data_dir` write to separate `<data_dir>/vaults/<id>/index.sqlite`
        // files and do not see each other's data. This is the multi-vault
        // foundation downstream tasks (9.3-9.5) build on.
        let dir = tempdir().unwrap();
        let id_a = VaultId::new();
        let id_b = VaultId::new();
        assert_ne!(id_a, id_b);

        let store_a = Store::open(&id_a, dir.path(), "index.sqlite", &embedding_for_tests())
            .await
            .unwrap();
        let store_b = Store::open(&id_b, dir.path(), "index.sqlite", &embedding_for_tests())
            .await
            .unwrap();

        // Sanity: distinct paths under distinct per-vault subdirectories.
        assert_ne!(store_a.path(), store_b.path());
        assert_eq!(
            store_a.path(),
            &dir.path()
                .join("vaults")
                .join(id_a.as_str())
                .join("index.sqlite")
        );
        assert_eq!(
            store_b.path(),
            &dir.path()
                .join("vaults")
                .join(id_b.as_str())
                .join("index.sqlite")
        );
        assert_eq!(store_a.vault_id(), &id_a);
        assert_eq!(store_b.vault_id(), &id_b);

        // Insert a `files` row into A only. The `files` table is the
        // simplest store-level write surface that doesn't require the
        // vec extension or the chunk pipeline.
        let pool_a = store_a.pool();
        run_blocking(move || {
            let conn = pool_a.get().unwrap();
            conn.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "a-only.md",
                    1_i64,
                    "2026-01-01T00:00:00Z",
                    "sha256:00",
                    "2026-01-01T00:00:00Z",
                    "",
                ],
            )
            .unwrap();
        })
        .await;

        // A sees the row; B does not.
        let pool_a = store_a.pool();
        let count_a: i64 = run_blocking(move || {
            let conn = pool_a.get().unwrap();
            conn.query_row("SELECT count(*) FROM files", [], |row| row.get(0))
                .unwrap()
        })
        .await;
        let pool_b = store_b.pool();
        let count_b: i64 = run_blocking(move || {
            let conn = pool_b.get().unwrap();
            conn.query_row("SELECT count(*) FROM files", [], |row| row.get(0))
                .unwrap()
        })
        .await;
        assert_eq!(count_a, 1, "store A should hold the row it inserted");
        assert_eq!(
            count_b, 0,
            "store B at a different vault_id must not see store A's row"
        );
    }
}
