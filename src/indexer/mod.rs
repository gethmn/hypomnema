mod hash;
mod walk;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use globset::GlobSet;
use rusqlite::{OptionalExtension, params};
use tokio::task;
use tracing::info;

use crate::config::Config;
use crate::store::{SqlitePool, Store};

pub use hash::hash_file;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanReport {
    pub inserted: usize,
    pub updated: usize,
    pub hash_unchanged: usize,
    pub deleted: usize,
    pub skipped_outside_vault: usize,
    pub walk_errors: usize,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexOutcome {
    Inserted,
    Updated,
    HashUnchanged,
    MissingFromDisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    NotPresent,
}

pub struct Scanner {
    vault: PathBuf,
    ignores: GlobSet,
    pool: SqlitePool,
}

impl Scanner {
    pub fn new(config: &Config, store: &Store) -> Result<Self> {
        let ignores = config
            .watcher
            .compiled_ignores()
            .context("compiling watcher.ignore_patterns for scanner")?;
        Ok(Self {
            vault: config.vault.0.clone(),
            ignores,
            pool: store.pool(),
        })
    }

    pub async fn run(&self) -> Result<ScanReport> {
        let vault = self.vault.clone();
        let ignores = self.ignores.clone();
        let pool = self.pool.clone();
        task::spawn_blocking(move || run_blocking(vault, ignores, pool))
            .await
            .context("spawn_blocking join error in Scanner::run")?
    }

    pub async fn reindex_path(&self, rel: &str) -> Result<ReindexOutcome> {
        let vault = self.vault.clone();
        let pool = self.pool.clone();
        let rel = rel.to_string();
        task::spawn_blocking(move || single_file_blocking(vault, rel, pool))
            .await
            .context("spawn_blocking join error in Scanner::reindex_path")?
    }

    pub async fn remove_path(&self, rel: &str) -> Result<RemoveOutcome> {
        let pool = self.pool.clone();
        let rel = rel.to_string();
        task::spawn_blocking(move || remove_blocking(rel, pool))
            .await
            .context("spawn_blocking join error in Scanner::remove_path")?
    }
}

#[derive(Debug, Clone)]
struct StoredFile {
    size: i64,
    mtime: String,
    content_hash: String,
}

// Internal effect of one upsert decision against an existing row (or absence
// thereof). The bulk and single-file paths both go through `upsert_file_in_tx`
// so they cannot disagree on stat-gate / hash-gate semantics; they only
// disagree on how to count the result.
enum UpsertEffect {
    Inserted,
    Updated,
    HashMatched,
    StatGateHit,
}

fn upsert_file_in_tx(
    tx: &rusqlite::Transaction<'_>,
    rel: &str,
    abs: &Path,
    size: i64,
    mtime: &str,
    now_iso: &str,
    existing: Option<&StoredFile>,
) -> Result<UpsertEffect> {
    match existing {
        None => {
            let hash = hash::hash_file(abs).with_context(|| format!("hashing new file {rel}"))?;
            tx.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![rel, size, mtime, hash, now_iso],
            )
            .with_context(|| format!("inserting row for {rel}"))?;
            Ok(UpsertEffect::Inserted)
        }
        Some(prev) if prev.size == size && prev.mtime == mtime => Ok(UpsertEffect::StatGateHit),
        Some(prev) => {
            let hash =
                hash::hash_file(abs).with_context(|| format!("hashing changed-stat file {rel}"))?;
            if hash == prev.content_hash {
                tx.execute(
                    "UPDATE files SET mtime = ?1, indexed_at = ?2 WHERE path = ?3",
                    params![mtime, now_iso, rel],
                )
                .with_context(|| format!("updating mtime-only for {rel}"))?;
                Ok(UpsertEffect::HashMatched)
            } else {
                tx.execute(
                    "UPDATE files SET size = ?1, mtime = ?2, content_hash = ?3, indexed_at = ?4 WHERE path = ?5",
                    params![size, mtime, hash, now_iso, rel],
                )
                .with_context(|| format!("updating row for {rel}"))?;
                Ok(UpsertEffect::Updated)
            }
        }
    }
}

fn read_existing_one(tx: &rusqlite::Transaction<'_>, rel: &str) -> Result<Option<StoredFile>> {
    tx.query_row(
        "SELECT size, mtime, content_hash FROM files WHERE path = ?1",
        params![rel],
        |row| {
            Ok(StoredFile {
                size: row.get(0)?,
                mtime: row.get(1)?,
                content_hash: row.get(2)?,
            })
        },
    )
    .optional()
    .with_context(|| format!("reading existing row for {rel}"))
}

fn run_blocking(vault: PathBuf, ignores: GlobSet, pool: SqlitePool) -> Result<ScanReport> {
    let started = Instant::now();
    let walked = walk::walk_vault(&vault, &ignores)?;
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);

    let mut conn = pool
        .get()
        .context("acquiring connection from pool for scan")?;
    let tx = conn.transaction().context("beginning scan transaction")?;

    let mut existing: HashMap<String, StoredFile> = HashMap::new();
    {
        let mut stmt = tx
            .prepare("SELECT path, size, mtime, content_hash FROM files")
            .context("preparing existing-files query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    StoredFile {
                        size: row.get(1)?,
                        mtime: row.get(2)?,
                        content_hash: row.get(3)?,
                    },
                ))
            })
            .context("querying existing files")?;
        for row in rows {
            let (path, file) = row.context("decoding existing files row")?;
            existing.insert(path, file);
        }
    }

    let mut report = ScanReport {
        skipped_outside_vault: walked.skipped_outside_vault,
        walk_errors: walked.walk_errors,
        ..Default::default()
    };
    let mut found: HashSet<String> = HashSet::with_capacity(walked.entries.len());

    for entry in &walked.entries {
        found.insert(entry.rel_path.clone());
        let effect = upsert_file_in_tx(
            &tx,
            &entry.rel_path,
            &entry.abs_path,
            entry.size,
            &entry.mtime,
            &now_iso,
            existing.get(&entry.rel_path),
        )?;
        match effect {
            UpsertEffect::Inserted => report.inserted += 1,
            UpsertEffect::Updated => report.updated += 1,
            UpsertEffect::HashMatched => report.hash_unchanged += 1,
            UpsertEffect::StatGateHit => {}
        }
    }

    let to_delete: Vec<String> = existing
        .keys()
        .filter(|p| !found.contains(*p))
        .cloned()
        .collect();
    for path in &to_delete {
        tx.execute("DELETE FROM files WHERE path = ?1", params![path])
            .with_context(|| format!("deleting row for {path}"))?;
        report.deleted += 1;
    }

    tx.commit().context("committing scan transaction")?;
    drop(conn);

    report.duration = started.elapsed();
    info!(
        inserted = report.inserted,
        updated = report.updated,
        hash_unchanged = report.hash_unchanged,
        deleted = report.deleted,
        skipped_outside_vault = report.skipped_outside_vault,
        walk_errors = report.walk_errors,
        duration_ms = report.duration.as_millis() as u64,
        "scan complete"
    );
    Ok(report)
}

fn single_file_blocking(vault: PathBuf, rel: String, pool: SqlitePool) -> Result<ReindexOutcome> {
    let abs = vault.join(&rel);

    let metadata = match fs::metadata(&abs) {
        Ok(m) => m,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(ReindexOutcome::MissingFromDisk);
        }
        Err(err) => {
            return Err(anyhow::Error::new(err))
                .with_context(|| format!("reading metadata for {}", abs.display()));
        }
    };
    if !metadata.is_file() {
        return Ok(ReindexOutcome::MissingFromDisk);
    }
    let size = metadata.len() as i64;
    let mtime_sys = metadata
        .modified()
        .with_context(|| format!("reading mtime for {}", abs.display()))?;
    let mtime = walk::format_mtime(mtime_sys);
    let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);

    let mut conn = pool
        .get()
        .context("acquiring connection from pool for reindex_path")?;
    let tx = conn
        .transaction()
        .context("beginning reindex transaction")?;
    let existing = read_existing_one(&tx, &rel)?;
    let effect = upsert_file_in_tx(&tx, &rel, &abs, size, &mtime, &now_iso, existing.as_ref())?;
    tx.commit().context("committing reindex transaction")?;

    Ok(match effect {
        UpsertEffect::Inserted => ReindexOutcome::Inserted,
        UpsertEffect::Updated => ReindexOutcome::Updated,
        UpsertEffect::HashMatched | UpsertEffect::StatGateHit => ReindexOutcome::HashUnchanged,
    })
}

fn remove_blocking(rel: String, pool: SqlitePool) -> Result<RemoveOutcome> {
    let mut conn = pool
        .get()
        .context("acquiring connection from pool for remove_path")?;
    let tx = conn.transaction().context("beginning remove transaction")?;
    let n = tx
        .execute("DELETE FROM files WHERE path = ?1", params![rel])
        .with_context(|| format!("deleting row for {rel}"))?;
    tx.commit().context("committing remove transaction")?;
    Ok(if n > 0 {
        RemoveOutcome::Removed
    } else {
        RemoveOutcome::NotPresent
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::fs;
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn smoke_config(vault: &std::path::Path) -> Config {
        let mut cfg = Config::default_for_smoke_test(vault.to_path_buf());
        // Validator normally canonicalizes vault; mimic that for tests so paths
        // line up with WalkDir's canonicalization-based outside-vault check.
        cfg.vault.0 = fs::canonicalize(vault).unwrap();
        cfg
    }

    async fn count_files(store: &Store) -> i64 {
        let pool = store.pool();
        task::spawn_blocking(move || -> i64 {
            let conn = pool.get().unwrap();
            conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
                .unwrap()
        })
        .await
        .unwrap()
    }

    async fn read_hash(store: &Store, rel: &str) -> String {
        let pool = store.pool();
        let rel = rel.to_string();
        task::spawn_blocking(move || -> String {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = ?1",
                params![rel],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn scan_inserts_one_md_file() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hello").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        let report = scanner.run().await.unwrap();
        assert_eq!(report.inserted, 1);
        assert_eq!(report.updated, 0);
        assert_eq!(report.deleted, 0);
        assert_eq!(report.hash_unchanged, 0);
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn rerun_is_idempotent_on_unchanged_vault() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hello").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        let r1 = scanner.run().await.unwrap();
        assert_eq!(r1.inserted, 1);

        let r2 = scanner.run().await.unwrap();
        assert_eq!(r2.inserted, 0);
        assert_eq!(r2.updated, 0);
        assert_eq!(r2.hash_unchanged, 0);
        assert_eq!(r2.deleted, 0);
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn editing_bytes_updates_content_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# v1").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        scanner.run().await.unwrap();

        let pool = store.pool();
        let h1: String = task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = 'hello.md'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();

        // Bump size + bytes so the stat-gate triggers a rehash.
        fs::write(&path, b"# v2 longer").unwrap();
        let report = scanner.run().await.unwrap();
        assert_eq!(report.updated, 1);
        assert_eq!(report.inserted, 0);
        assert_eq!(report.hash_unchanged, 0);

        let pool = store.pool();
        let h2: String = task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = 'hello.md'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn mtime_only_change_preserves_content_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# stable").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        scanner.run().await.unwrap();

        let pool_h = store.pool();
        let h1: String = task::spawn_blocking(move || {
            let conn = pool_h.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = 'hello.md'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();

        // Bump mtime forward without changing bytes.
        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(2))
            .unwrap();
        drop(f);

        let report = scanner.run().await.unwrap();
        assert_eq!(report.hash_unchanged, 1);
        assert_eq!(report.updated, 0);
        assert_eq!(report.inserted, 0);

        let pool_h2 = store.pool();
        let h2: String = task::spawn_blocking(move || {
            let conn = pool_h2.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = 'hello.md'",
                [],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(h1, h2);
    }

    #[tokio::test]
    async fn deleting_a_file_removes_its_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("a.md"), b"# A").unwrap();
        fs::write(vault_dir.path().join("b.md"), b"# B").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        let r1 = scanner.run().await.unwrap();
        assert_eq!(r1.inserted, 2);

        fs::remove_file(vault_dir.path().join("b.md")).unwrap();
        let r2 = scanner.run().await.unwrap();
        assert_eq!(r2.deleted, 1);
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn reindex_path_inserts_new_file() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();

        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::Inserted);
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn reindex_path_idempotent_when_bytes_unchanged() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();

        assert_eq!(
            scanner.reindex_path("hello.md").await.unwrap(),
            ReindexOutcome::Inserted
        );
        assert_eq!(
            scanner.reindex_path("hello.md").await.unwrap(),
            ReindexOutcome::HashUnchanged
        );
    }

    #[tokio::test]
    async fn reindex_path_returns_updated_when_bytes_change() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# v1").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        scanner.reindex_path("hello.md").await.unwrap();
        let h1 = read_hash(&store, "hello.md").await;

        fs::write(&path, b"# v2 longer").unwrap();
        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::Updated);
        let h2 = read_hash(&store, "hello.md").await;
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn reindex_path_mtime_only_bump_returns_hash_unchanged() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# stable").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        scanner.reindex_path("hello.md").await.unwrap();
        let h1 = read_hash(&store, "hello.md").await;

        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(2))
            .unwrap();
        drop(f);

        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::HashUnchanged);
        assert_eq!(read_hash(&store, "hello.md").await, h1);
    }

    #[tokio::test]
    async fn reindex_path_returns_missing_when_file_absent() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();

        let outcome = scanner.reindex_path("ghost.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::MissingFromDisk);
        assert_eq!(count_files(&store).await, 0);
    }

    #[tokio::test]
    async fn reindex_path_handles_nested_relative_path() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::create_dir_all(vault_dir.path().join("notes/sub")).unwrap();
        fs::write(vault_dir.path().join("notes/sub/note.md"), b"# nested").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();

        let outcome = scanner.reindex_path("notes/sub/note.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::Inserted);
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn remove_path_removes_present_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();
        scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(count_files(&store).await, 1);

        let outcome = scanner.remove_path("hello.md").await.unwrap();
        assert_eq!(outcome, RemoveOutcome::Removed);
        assert_eq!(count_files(&store).await, 0);
    }

    #[tokio::test]
    async fn remove_path_returns_not_present_for_unknown_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let config = smoke_config(vault_dir.path());
        let store = Store::open(data_dir.path(), "index.sqlite").await.unwrap();
        let scanner = Scanner::new(&config, &store).unwrap();

        let outcome = scanner.remove_path("ghost.md").await.unwrap();
        assert_eq!(outcome, RemoveOutcome::NotPresent);
        assert_eq!(count_files(&store).await, 0);
    }
}
