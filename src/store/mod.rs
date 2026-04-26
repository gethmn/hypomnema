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

pub use pool::SqlitePool;
pub use schema::MIGRATIONS;

#[derive(Clone)]
pub struct Store {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    pool: SqlitePool,
    path: PathBuf,
}

impl Store {
    pub async fn open(
        data_dir: &Path,
        index_file: &str,
        embedding: &EmbeddingConfig,
    ) -> Result<Self> {
        let data_dir = data_dir.to_path_buf();
        let index_file = index_file.to_string();
        let extension_path = embedding.resolved_extension_path();
        let expected_dim = embedding.dimension;
        task::spawn_blocking(move || {
            open_blocking(data_dir, index_file, extension_path, expected_dim)
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
}

fn open_blocking(
    data_dir: PathBuf,
    index_file: String,
    extension_path: PathBuf,
    expected_dim: u32,
) -> Result<Store> {
    fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data_dir {}", data_dir.display()))?;
    if !extension_path.exists() {
        return Err(anyhow!(
            "sqlite-vec extension binary not found at {}. Set the {} env var to override the path, \
             or place the dylib at the configured location (see docs/reference/configuration.md \
             § embedding.extension_path).",
            extension_path.display(),
            VEC_EXT_PATH_ENV,
        ));
    }
    let db_path = data_dir.join(&index_file);
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
        "store: opened {} (migrations applied; sqlite-vec extension at {})",
        db_path.display(),
        extension_path.display()
    );
    Ok(Store {
        inner: Arc::new(StoreInner {
            pool,
            path: db_path,
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
         or (b) deleting the database file (storage.data_dir/storage.index_file) and \
         re-indexing from scratch under the new dimension."
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
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
        {
            let _store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
                .await
                .unwrap();
        }
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
    async fn open_creates_missing_data_dir() {
        let parent = tempdir().unwrap();
        let nested = parent.path().join("nested/data_dir");
        assert!(!nested.exists());
        let store = Store::open(&nested, "index.sqlite", &embedding_for_tests())
            .await
            .unwrap();
        assert!(store.path().exists());
        assert_eq!(store.path(), &nested.join("index.sqlite"));
    }

    #[tokio::test]
    async fn validate_dimension_matches() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
        let store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
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
        {
            let _store = Store::open(dir.path(), "index.sqlite", &embedding_for_tests())
                .await
                .unwrap();
        }
        let bad = EmbeddingConfig {
            dimension: 512,
            ..EmbeddingConfig::default()
        };
        let err = match Store::open(dir.path(), "index.sqlite", &bad).await {
            Ok(_) => panic!("re-open with wrong dimension should fail"),
            Err(e) => e,
        };
        let text = format!("{err:#}");
        assert!(text.contains("512") && text.contains("768"), "{text}");
    }
}
