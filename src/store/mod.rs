mod pool;
mod schema;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::task;
use tracing::info;

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
    pub async fn open(data_dir: &Path, index_file: &str) -> Result<Self> {
        let data_dir = data_dir.to_path_buf();
        let index_file = index_file.to_string();
        task::spawn_blocking(move || open_blocking(data_dir, index_file))
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

fn open_blocking(data_dir: PathBuf, index_file: String) -> Result<Store> {
    fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data_dir {}", data_dir.display()))?;
    let db_path = data_dir.join(&index_file);
    let pool = pool::build_pool(&db_path)
        .with_context(|| format!("building connection pool for {}", db_path.display()))?;
    let mut conn = pool.get().with_context(|| {
        format!(
            "acquiring initial connection from pool for {}",
            db_path.display()
        )
    })?;
    schema::apply_migrations(&mut conn)
        .with_context(|| format!("applying migrations on {}", db_path.display()))?;
    drop(conn);
    info!("store: opened {} (migrations applied)", db_path.display());
    Ok(Store {
        inner: Arc::new(StoreInner {
            pool,
            path: db_path,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
        let store = Store::open(dir.path(), "index.sqlite").await.unwrap();
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
            let _store = Store::open(dir.path(), "index.sqlite").await.unwrap();
        }
        let store = Store::open(dir.path(), "index.sqlite").await.unwrap();
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
        let store = Store::open(dir.path(), "index.sqlite").await.unwrap();
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
        let store = Store::open(dir.path(), "index.sqlite").await.unwrap();
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
        let store = Store::open(&nested, "index.sqlite").await.unwrap();
        assert!(store.path().exists());
        assert_eq!(store.path(), &nested.join("index.sqlite"));
    }
}
