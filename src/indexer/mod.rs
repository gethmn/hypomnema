mod hash;
mod walk;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use globset::GlobSet;
use rusqlite::{OptionalExtension, params};
use tokio::task;
use tracing::{debug, error, info};

use crate::chunk;
use crate::config::Config;
use crate::embedding::{Embedder, EmbeddingError};
use crate::store::{SqlitePool, Store, rewrite_chunks_for_file};
use crate::vault_registry::VaultId;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReindexOutcome {
    Inserted { content_hash: String },
    Updated { content_hash: String },
    HashUnchanged,
    MissingFromDisk,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed { previous_hash: String },
    NotPresent,
}

pub struct Scanner {
    vault_id: VaultId,
    vault: PathBuf,
    ignores: GlobSet,
    pool: SqlitePool,
    embedder: Arc<dyn Embedder>,
}

impl Scanner {
    pub fn new(
        vault_path: &Path,
        config: &Config,
        store: &Store,
        embedder: Arc<dyn Embedder>,
    ) -> Result<Self> {
        let ignores = config
            .watcher
            .compiled_ignores()
            .context("compiling watcher.ignore_patterns for scanner")?;
        Ok(Self {
            vault_id: store.vault_id().clone(),
            vault: vault_path.to_path_buf(),
            ignores,
            pool: store.pool(),
            embedder,
        })
    }

    pub async fn run(&self) -> Result<ScanReport> {
        let started = Instant::now();
        let vault = self.vault.clone();
        let ignores = self.ignores.clone();
        let pool = self.pool.clone();

        info!(
            vault_id = %self.vault_id,
            vault = %vault.display(),
            "scan: walking vault"
        );

        // Phase 1 (sync): walk + load existing files into HashMap.
        let (walked, mut existing) = task::spawn_blocking(
            move || -> Result<(walk::WalkOutcome, HashMap<String, StoredFile>)> {
                let walked = walk::walk_vault(&vault, &ignores)?;
                let conn = pool
                    .get()
                    .context("acquiring connection from pool for scan preflight")?;
                let mut existing: HashMap<String, StoredFile> = HashMap::new();
                let mut stmt = conn
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
                Ok((walked, existing))
            },
        )
        .await
        .context("spawn_blocking join error in Scanner::run preflight")??;

        let mut report = ScanReport {
            skipped_outside_vault: walked.skipped_outside_vault,
            walk_errors: walked.walk_errors,
            ..Default::default()
        };
        let mut found: HashSet<String> = HashSet::with_capacity(walked.entries.len());

        let total = walked.entries.len() as u64;
        info!(
            vault_id = %self.vault_id,
            total,
            skipped_outside_vault = walked.skipped_outside_vault,
            walk_errors = walked.walk_errors,
            "scan: walk complete, starting per-file processing"
        );

        // Phase 2 (async per-file pipeline): chunk + embed lives on the runtime;
        // each file's SQL write is its own spawn_blocking transaction.
        let mut processed: u64 = 0;
        let mut last_log_at = Instant::now();
        const PROGRESS_EVERY_FILES: u64 = 100;
        const PROGRESS_EVERY: Duration = Duration::from_secs(5);

        for entry in &walked.entries {
            found.insert(entry.rel_path.clone());
            let prior = existing.remove(&entry.rel_path);
            let effect = self
                .process_entry(
                    entry.rel_path.clone(),
                    entry.abs_path.clone(),
                    entry.size,
                    entry.mtime.clone(),
                    prior,
                )
                .await?;
            match effect {
                ProcessEffect::Inserted { .. } => report.inserted += 1,
                ProcessEffect::Updated { .. } => report.updated += 1,
                ProcessEffect::HashMatched => report.hash_unchanged += 1,
                // StatGateHit / EmbeddingSkipped leave the row unchanged with
                // no observable work — match the pre-step-6 bulk-scan
                // accounting (no counter advance).
                ProcessEffect::StatGateHit | ProcessEffect::EmbeddingSkipped => {}
            }

            processed += 1;
            if processed % PROGRESS_EVERY_FILES == 0 || last_log_at.elapsed() >= PROGRESS_EVERY {
                info!(
                    vault_id = %self.vault_id,
                    processed,
                    total,
                    inserted = report.inserted,
                    updated = report.updated,
                    hash_unchanged = report.hash_unchanged,
                    current = %entry.rel_path,
                    "scan: progress"
                );
                last_log_at = Instant::now();
            }
        }

        // Phase 3 (sync): bulk-delete files that were in the index but not on disk.
        let to_delete: Vec<String> = existing.keys().cloned().collect();
        if !to_delete.is_empty() {
            let pool = self.pool.clone();
            let count = to_delete.len();
            task::spawn_blocking(move || -> Result<()> {
                let mut conn = pool
                    .get()
                    .context("acquiring connection from pool for bulk delete")?;
                let tx = conn
                    .transaction()
                    .context("beginning bulk-delete transaction")?;
                for path in &to_delete {
                    delete_file_in_tx(&tx, path)?;
                }
                tx.commit().context("committing bulk-delete transaction")?;
                Ok(())
            })
            .await
            .context("spawn_blocking join error in Scanner::run bulk delete")??;
            report.deleted = count;
        }

        report.duration = started.elapsed();
        info!(
            vault_id = %self.vault_id,
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

    /// Walk the vault and return relative `.md` paths. Used by the
    /// rescan-on-demand path: the consumer task drives one `Upsert` event
    /// per returned path through `apply_event`, re-emitting outbox events
    /// for files whose content_hash drifted from the on-disk hash.
    pub async fn vault_paths(&self) -> Result<Vec<String>> {
        let vault = self.vault.clone();
        let ignores = self.ignores.clone();
        task::spawn_blocking(move || -> Result<Vec<String>> {
            let walked = walk::walk_vault(&vault, &ignores)?;
            Ok(walked.entries.into_iter().map(|e| e.rel_path).collect())
        })
        .await
        .context("spawn_blocking join error in Scanner::vault_paths")?
    }

    pub async fn reindex_path(&self, rel: &str) -> Result<ReindexOutcome> {
        let abs = self.vault.join(rel);
        let metadata = match tokio::fs::metadata(&abs).await {
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

        let pool = self.pool.clone();
        let rel_for_lookup = rel.to_string();
        let prior = task::spawn_blocking(move || -> Result<Option<StoredFile>> {
            let conn = pool
                .get()
                .context("acquiring connection from pool for prior-row lookup")?;
            conn.query_row(
                "SELECT size, mtime, content_hash FROM files WHERE path = ?1",
                params![rel_for_lookup],
                |row| {
                    Ok(StoredFile {
                        size: row.get(0)?,
                        mtime: row.get(1)?,
                        content_hash: row.get(2)?,
                    })
                },
            )
            .optional()
            .with_context(|| format!("reading existing row for {rel_for_lookup}"))
        })
        .await
        .context("spawn_blocking join error in Scanner::reindex_path lookup")??;

        Ok(self
            .process_entry(rel.to_string(), abs, size, mtime, prior)
            .await?
            .into_outcome())
    }

    pub async fn remove_path(&self, rel: &str) -> Result<RemoveOutcome> {
        let pool = self.pool.clone();
        let rel = rel.to_string();
        task::spawn_blocking(move || remove_blocking(rel, pool))
            .await
            .context("spawn_blocking join error in Scanner::remove_path")?
    }

    /// Per-file async pipeline shared by `run()` and `reindex_path()`.
    ///
    /// Three phases per the `rusqlite-in-async` skill:
    ///   * sync decision (`spawn_blocking`): stat-gate / hash-match early-exit;
    ///     mtime-only updates commit here.
    ///   * async chunk + embed: chunking is sync but stays on the runtime per the
    ///     `markdown-chunking` skill; embedding is the network call.
    ///   * sync write (`spawn_blocking`): one transaction holds the `files`
    ///     row update and the `chunks`/`chunks_vec` rewrite, so a crash
    ///     between phases leaves the database consistent.
    ///
    /// On `EmbeddingError::Transport(_)`, `Status { code: 500..=599, .. }`,
    /// or `DimensionMismatch { .. }`, log ERROR and return `HashUnchanged`
    /// without advancing `files.content_hash` — see workplan § Resolution 1
    /// and pre-build directive 3 (service-up-with-wrong-dim must not crash
    /// the daemon at runtime).
    async fn process_entry(
        &self,
        rel: String,
        abs: PathBuf,
        size: i64,
        mtime: String,
        prior: Option<StoredFile>,
    ) -> Result<ProcessEffect> {
        let now_iso = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);

        // Phase A (sync): decide what to do.
        let pool = self.pool.clone();
        let rel_for_decide = rel.clone();
        let abs_for_decide = abs.clone();
        let mtime_for_decide = mtime.clone();
        let now_for_decide = now_iso.clone();
        let prior_for_decide = prior.clone();
        let decision = task::spawn_blocking(move || -> Result<UpsertDecision> {
            decide_upsert(
                pool,
                &rel_for_decide,
                &abs_for_decide,
                size,
                &mtime_for_decide,
                &now_for_decide,
                prior_for_decide.as_ref(),
            )
        })
        .await
        .context("spawn_blocking join error in process_entry decide")??;

        let (body, new_hash) = match decision {
            UpsertDecision::EarlyDone(effect) => return Ok(effect),
            UpsertDecision::Reindex { body, new_hash } => (body, new_hash),
        };

        // Phase B (async runtime): chunk synchronously, then embed sequentially.
        // Plain for-loop per pre-build directive 2 (futures::stream is not in
        // tree and at v0 batch_size = 1 the stream is one-element anyway).
        let chunks = chunk::chunk_file(&body);
        let chunk_count = chunks.len();
        let mut chunks_with_vecs: Vec<(chunk::Chunk, Vec<f32>)> = Vec::with_capacity(chunk_count);
        for chunk in &chunks {
            debug!(
                path = %rel,
                chunk_index = chunk.chunk_index,
                chunk_count,
                bytes = chunk.content.len(),
                "embedding: starting"
            );
            let started = Instant::now();
            let result = self.embedder.embed_text(&chunk.content).await;
            match result {
                Ok(v) => {
                    debug!(
                        path = %rel,
                        chunk_index = chunk.chunk_index,
                        chunk_count,
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "embedding: complete"
                    );
                    chunks_with_vecs.push((chunk.clone(), v));
                }
                Err(EmbeddingError::Transport(_))
                | Err(EmbeddingError::Status {
                    code: 500..=599, ..
                })
                | Err(EmbeddingError::DimensionMismatch { .. }) => {
                    error!(
                        path = %rel,
                        chunk_index = chunk.chunk_index,
                        "embedding service failure, skipping file"
                    );
                    return Ok(ProcessEffect::EmbeddingSkipped);
                }
                Err(e) => {
                    return Err(anyhow::Error::new(e)).with_context(|| {
                        format!("embedding chunk {} for {}", chunk.chunk_index, rel)
                    });
                }
            }
        }

        // Phase C (sync): files row + chunks rewrite in one transaction.
        let pool = self.pool.clone();
        let rel_for_write = rel.clone();
        let was_insert = prior.is_none();
        let body_owned = body;
        let mtime_owned = mtime;
        let new_hash_owned = new_hash.clone();
        let chunks_payload = chunks_with_vecs;
        let now_for_write = now_iso;
        task::spawn_blocking(move || -> Result<()> {
            write_blocking(
                pool,
                &rel_for_write,
                size,
                &mtime_owned,
                &new_hash_owned,
                &body_owned,
                &now_for_write,
                was_insert,
                &chunks_payload,
            )
        })
        .await
        .context("spawn_blocking join error in process_entry write")??;

        if was_insert {
            Ok(ProcessEffect::Inserted {
                content_hash: new_hash,
            })
        } else {
            Ok(ProcessEffect::Updated {
                content_hash: new_hash,
            })
        }
    }
}

#[derive(Debug, Clone)]
struct StoredFile {
    size: i64,
    mtime: String,
    content_hash: String,
}

/// Internal outcome from `process_entry`. Public `ReindexOutcome` collapses
/// `HashMatched` / `StatGateHit` / `EmbeddingSkipped` into `HashUnchanged` for
/// callers; the internal split lets `Scanner::run`'s bulk-scan accounting keep
/// matching the pre-step-6 semantics (only mtime-only re-reads count toward
/// `report.hash_unchanged`).
enum ProcessEffect {
    Inserted { content_hash: String },
    Updated { content_hash: String },
    HashMatched,
    StatGateHit,
    EmbeddingSkipped,
}

impl ProcessEffect {
    fn into_outcome(self) -> ReindexOutcome {
        match self {
            ProcessEffect::Inserted { content_hash } => ReindexOutcome::Inserted { content_hash },
            ProcessEffect::Updated { content_hash } => ReindexOutcome::Updated { content_hash },
            ProcessEffect::HashMatched
            | ProcessEffect::StatGateHit
            | ProcessEffect::EmbeddingSkipped => ReindexOutcome::HashUnchanged,
        }
    }
}

enum UpsertDecision {
    EarlyDone(ProcessEffect),
    Reindex { body: String, new_hash: String },
}

fn decide_upsert(
    pool: SqlitePool,
    rel: &str,
    abs: &Path,
    size: i64,
    mtime: &str,
    now_iso: &str,
    prior: Option<&StoredFile>,
) -> Result<UpsertDecision> {
    if let Some(prev) = prior {
        // Empty `content_hash` is the project's "needs re-embedding" sentinel
        // (set by migration-0004 and `reset --rebuild`; see
        // `src/store/schema.rs:51`). The stat gate must NOT short-circuit on
        // it: `reset --rebuild` zeroes content_hash without touching size /
        // mtime, and the operator's follow-up `rescan` walks every file
        // expecting reindex_path to re-embed each one.
        if prev.size == size && prev.mtime == mtime && !prev.content_hash.is_empty() {
            return Ok(UpsertDecision::EarlyDone(ProcessEffect::StatGateHit));
        }
    }

    let (body, new_hash) =
        hash::read_and_hash(abs).with_context(|| format!("reading file {rel}"))?;

    if let Some(prev) = prior {
        if prev.content_hash == new_hash {
            let mut conn = pool
                .get()
                .context("acquiring connection from pool for mtime-only update")?;
            let tx = conn
                .transaction()
                .context("beginning mtime-only update transaction")?;
            tx.execute(
                "UPDATE files SET mtime = ?1, content = ?2, indexed_at = ?3 WHERE path = ?4",
                params![mtime, body, now_iso, rel],
            )
            .with_context(|| format!("updating mtime-only for {rel}"))?;
            tx.commit().context("committing mtime-only update")?;
            return Ok(UpsertDecision::EarlyDone(ProcessEffect::HashMatched));
        }
    }

    Ok(UpsertDecision::Reindex { body, new_hash })
}

#[allow(clippy::too_many_arguments)]
fn write_blocking(
    pool: SqlitePool,
    rel: &str,
    size: i64,
    mtime: &str,
    new_hash: &str,
    body: &str,
    now_iso: &str,
    was_insert: bool,
    chunks_with_embeddings: &[(chunk::Chunk, Vec<f32>)],
) -> Result<()> {
    let mut conn = pool
        .get()
        .context("acquiring connection from pool for write")?;
    let tx = conn
        .transaction()
        .context("beginning per-file write transaction")?;
    if was_insert {
        tx.execute(
            "INSERT INTO files (path, size, mtime, content_hash, content, indexed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![rel, size, mtime, new_hash, body, now_iso],
        )
        .with_context(|| format!("inserting files row for {rel}"))?;
    } else {
        tx.execute(
            "UPDATE files SET size = ?1, mtime = ?2, content_hash = ?3, content = ?4, indexed_at = ?5 \
             WHERE path = ?6",
            params![size, mtime, new_hash, body, now_iso, rel],
        )
        .with_context(|| format!("updating files row for {rel}"))?;
    }
    rewrite_chunks_for_file(&tx, rel, chunks_with_embeddings, now_iso)?;
    tx.commit()
        .context("committing per-file write transaction")?;
    Ok(())
}

fn delete_file_in_tx(tx: &rusqlite::Transaction<'_>, path: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM chunks_vec WHERE chunk_id IN (SELECT id FROM chunks WHERE file_path = ?1)",
        params![path],
    )
    .with_context(|| format!("deleting chunks_vec rows for {path}"))?;
    tx.execute("DELETE FROM chunks WHERE file_path = ?1", params![path])
        .with_context(|| format!("deleting chunks rows for {path}"))?;
    tx.execute("DELETE FROM files WHERE path = ?1", params![path])
        .with_context(|| format!("deleting files row for {path}"))?;
    Ok(())
}

fn remove_blocking(rel: String, pool: SqlitePool) -> Result<RemoveOutcome> {
    let mut conn = pool
        .get()
        .context("acquiring connection from pool for remove_path")?;
    let tx = conn.transaction().context("beginning remove transaction")?;
    let prior: Option<String> = tx
        .query_row(
            "SELECT content_hash FROM files WHERE path = ?1",
            params![rel],
            |row| row.get(0),
        )
        .optional()
        .with_context(|| format!("reading prior content_hash for {rel}"))?;
    tx.execute(
        "DELETE FROM chunks_vec WHERE chunk_id IN (SELECT id FROM chunks WHERE file_path = ?1)",
        params![rel],
    )
    .with_context(|| format!("deleting chunks_vec rows for {rel}"))?;
    tx.execute("DELETE FROM chunks WHERE file_path = ?1", params![rel])
        .with_context(|| format!("deleting chunks rows for {rel}"))?;
    let n = tx
        .execute("DELETE FROM files WHERE path = ?1", params![rel])
        .with_context(|| format!("deleting files row for {rel}"))?;
    tx.commit().context("committing remove transaction")?;
    Ok(match (n, prior) {
        (0, _) => RemoveOutcome::NotPresent,
        (_, Some(h)) => RemoveOutcome::Removed { previous_hash: h },
        (_, None) => RemoveOutcome::NotPresent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::{EmbedFuture, EmbeddingError, StubEmbedder};
    use crate::store::Store;
    use crate::vault_registry::VaultId;
    use std::fs;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn smoke_config(vault: &std::path::Path) -> Config {
        let mut cfg = Config::default_for_smoke_test(vault.to_path_buf());
        // Validator normally canonicalizes vault; mimic that for tests so paths
        // line up with WalkDir's canonicalization-based outside-vault check.
        let canonical = fs::canonicalize(vault).unwrap();
        cfg.vault = Some(crate::config::ConfigPath(canonical));
        cfg
    }

    fn smoke_vault_path(config: &Config) -> std::path::PathBuf {
        config
            .vault
            .as_ref()
            .expect("smoke_config sets [vault]")
            .0
            .clone()
    }

    fn stub_scanner(config: &Config, store: &Store) -> Scanner {
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        Scanner::new(&smoke_vault_path(config), config, store, embedder).unwrap()
    }

    /// Embedder used by failure-path tests. Returns a `Status { code: 503, ... }`
    /// error which classifies as skip-and-log per Resolution 1; we use
    /// `Status` rather than `Transport` because `reqwest::Error` has no
    /// public constructor, and the workplan's prescribed assertions hinge on
    /// the skip class (not the specific variant).
    struct AlwaysSkipEmbedder {
        calls: AtomicUsize,
    }

    impl AlwaysSkipEmbedder {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Embedder for AlwaysSkipEmbedder {
        fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Err(EmbeddingError::Status {
                    code: 503,
                    body: "service unavailable (test stub)".to_string(),
                })
            })
        }
    }

    /// Embedder that always returns `EmbeddingError::DimensionMismatch`.
    /// Models a service that is reachable but returns vectors with the
    /// wrong dimension (e.g. configured against a different model than
    /// the schema expects); per directive 3 this must skip-and-log, not
    /// crash the daemon.
    struct AlwaysDimensionMismatchEmbedder {
        calls: AtomicUsize,
    }

    impl AlwaysDimensionMismatchEmbedder {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Embedder for AlwaysDimensionMismatchEmbedder {
        fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Err(EmbeddingError::DimensionMismatch {
                    expected: 768,
                    actual: 4,
                })
            })
        }
    }

    /// Embedder that records each input text, then returns deterministic zeros.
    /// Lets the prescribed tests assert which chunks were embedded.
    struct RecordingEmbedder {
        dimension: usize,
        texts: Mutex<Vec<String>>,
    }

    impl RecordingEmbedder {
        fn new(dimension: usize) -> Arc<Self> {
            Arc::new(Self {
                dimension,
                texts: Mutex::new(Vec::new()),
            })
        }
        fn texts(&self) -> Vec<String> {
            self.texts.lock().unwrap().clone()
        }
    }

    impl Embedder for RecordingEmbedder {
        fn embed_text<'a>(&'a self, text: &'a str) -> EmbedFuture<'a> {
            self.texts.lock().unwrap().push(text.to_string());
            let dim = self.dimension;
            Box::pin(async move { Ok(vec![0.0_f32; dim]) })
        }
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

    async fn count_chunks_for(store: &Store, rel: &str) -> i64 {
        let pool = store.pool();
        let rel = rel.to_string();
        task::spawn_blocking(move || -> i64 {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE file_path = ?1",
                params![rel],
                |r| r.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap()
    }

    async fn count_chunks_vec(store: &Store) -> i64 {
        let pool = store.pool();
        task::spawn_blocking(move || -> i64 {
            let conn = pool.get().unwrap();
            conn.query_row("SELECT COUNT(*) FROM chunks_vec", [], |r| r.get(0))
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

    async fn read_hash_optional(store: &Store, rel: &str) -> Option<String> {
        let pool = store.pool();
        let rel = rel.to_string();
        task::spawn_blocking(move || -> Option<String> {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content_hash FROM files WHERE path = ?1",
                params![rel],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .unwrap()
        })
        .await
        .unwrap()
    }

    async fn read_content(store: &Store, rel: &str) -> String {
        let pool = store.pool();
        let rel = rel.to_string();
        task::spawn_blocking(move || -> String {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT content FROM files WHERE path = ?1",
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
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
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
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
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
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.run().await.unwrap();

        let h1 = read_hash(&store, "hello.md").await;

        // Bump size + bytes so the stat-gate triggers a rehash.
        fs::write(&path, b"# v2 longer").unwrap();
        let report = scanner.run().await.unwrap();
        assert_eq!(report.updated, 1);
        assert_eq!(report.inserted, 0);
        assert_eq!(report.hash_unchanged, 0);

        let h2 = read_hash(&store, "hello.md").await;
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn mtime_only_change_preserves_content_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# stable").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.run().await.unwrap();

        let h1 = read_hash(&store, "hello.md").await;

        // Bump mtime forward without changing bytes.
        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(SystemTime::now() + Duration::from_secs(2))
            .unwrap();
        drop(f);

        let report = scanner.run().await.unwrap();
        assert_eq!(report.hash_unchanged, 1);
        assert_eq!(report.updated, 0);
        assert_eq!(report.inserted, 0);

        let h2 = read_hash(&store, "hello.md").await;
        assert_eq!(h1, h2);
    }

    #[tokio::test]
    async fn deleting_a_file_removes_its_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("a.md"), b"# A").unwrap();
        fs::write(vault_dir.path().join("b.md"), b"# B").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
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
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        let expected = hash_file(&path).unwrap();
        assert_eq!(
            outcome,
            ReindexOutcome::Inserted {
                content_hash: expected
            }
        );
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn reindex_path_idempotent_when_bytes_unchanged() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let expected = hash_file(&path).unwrap();
        assert_eq!(
            scanner.reindex_path("hello.md").await.unwrap(),
            ReindexOutcome::Inserted {
                content_hash: expected
            }
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
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.reindex_path("hello.md").await.unwrap();
        let h1 = read_hash(&store, "hello.md").await;

        fs::write(&path, b"# v2 longer").unwrap();
        let expected = hash_file(&path).unwrap();
        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(
            outcome,
            ReindexOutcome::Updated {
                content_hash: expected.clone()
            }
        );
        let h2 = read_hash(&store, "hello.md").await;
        assert_ne!(h1, h2);
        assert_eq!(h2, expected);
    }

    #[tokio::test]
    async fn reindex_path_mtime_only_bump_returns_hash_unchanged() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# stable").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
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
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let outcome = scanner.reindex_path("ghost.md").await.unwrap();
        assert_eq!(outcome, ReindexOutcome::MissingFromDisk);
        assert_eq!(count_files(&store).await, 0);
    }

    #[tokio::test]
    async fn reindex_path_handles_nested_relative_path() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::create_dir_all(vault_dir.path().join("notes/sub")).unwrap();
        let path = vault_dir.path().join("notes/sub/note.md");
        fs::write(&path, b"# nested").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let outcome = scanner.reindex_path("notes/sub/note.md").await.unwrap();
        let expected = hash_file(&path).unwrap();
        assert_eq!(
            outcome,
            ReindexOutcome::Inserted {
                content_hash: expected
            }
        );
        assert_eq!(count_files(&store).await, 1);
    }

    #[tokio::test]
    async fn remove_path_removes_present_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# hi").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.reindex_path("hello.md").await.unwrap();
        assert_eq!(count_files(&store).await, 1);

        let expected = hash_file(&path).unwrap();
        let outcome = scanner.remove_path("hello.md").await.unwrap();
        assert_eq!(
            outcome,
            RemoveOutcome::Removed {
                previous_hash: expected
            }
        );
        assert_eq!(count_files(&store).await, 0);
    }

    #[tokio::test]
    async fn remove_path_returns_not_present_for_unknown_row() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let outcome = scanner.remove_path("ghost.md").await.unwrap();
        assert_eq!(outcome, RemoveOutcome::NotPresent);
        assert_eq!(count_files(&store).await, 0);
    }

    #[tokio::test]
    async fn reindex_path_carries_inserted_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# carries-inserted-hash").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        let expected = hash_file(&path).unwrap();
        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        match outcome {
            ReindexOutcome::Inserted { content_hash } => assert_eq!(content_hash, expected),
            other => panic!("expected Inserted with hash, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reindex_path_carries_updated_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# v1").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.reindex_path("hello.md").await.unwrap();

        fs::write(&path, b"# v2 carries-updated-hash").unwrap();
        let expected = hash_file(&path).unwrap();
        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        match outcome {
            ReindexOutcome::Updated { content_hash } => assert_eq!(content_hash, expected),
            other => panic!("expected Updated with hash, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn scan_populates_content_for_inserted_files() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("hello.md"), b"# hello\n\nbody").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        let report = scanner.run().await.unwrap();
        assert_eq!(report.inserted, 1);

        assert_eq!(read_content(&store, "hello.md").await, "# hello\n\nbody");
    }

    #[tokio::test]
    async fn scan_populates_content_for_updated_files() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# v1").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.run().await.unwrap();
        assert_eq!(read_content(&store, "hello.md").await, "# v1");

        fs::write(&path, b"# v2 longer").unwrap();
        let report = scanner.run().await.unwrap();
        assert_eq!(report.updated, 1);
        assert_eq!(read_content(&store, "hello.md").await, "# v2 longer");
    }

    #[tokio::test]
    async fn remove_path_carries_prior_hash() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("hello.md");
        fs::write(&path, b"# carries-prior-hash").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);
        scanner.reindex_path("hello.md").await.unwrap();
        let inserted_hash = read_hash(&store, "hello.md").await;

        let outcome = scanner.remove_path("hello.md").await.unwrap();
        match outcome {
            RemoveOutcome::Removed { previous_hash } => assert_eq!(previous_hash, inserted_hash),
            other => panic!("expected Removed with prior hash, got {other:?}"),
        }
        assert_eq!(count_files(&store).await, 0);
    }

    // --- Task 6.4 prescribed tests ---

    #[tokio::test]
    async fn reindex_writes_chunks_for_simple_file() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(
            vault_dir.path().join("hello.md"),
            b"# hello\n\nA paragraph of body text.\n",
        )
        .unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let recorder = RecordingEmbedder::new(768);
        let scanner = Scanner::new(
            &smoke_vault_path(&config),
            &config,
            &store,
            recorder.clone() as Arc<dyn Embedder>,
        )
        .unwrap();

        let outcome = scanner.reindex_path("hello.md").await.unwrap();
        assert!(matches!(outcome, ReindexOutcome::Inserted { .. }));

        let n = count_chunks_for(&store, "hello.md").await;
        assert!(n >= 1, "expected ≥1 chunk row, got {n}");
        assert_eq!(count_chunks_vec(&store).await, n);
        let texts = recorder.texts();
        assert_eq!(
            texts.len(),
            n as usize,
            "embedder call count should equal chunk count"
        );
    }

    #[tokio::test]
    async fn reindex_replaces_chunks_for_modified_file() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let path = vault_dir.path().join("note.md");
        // First version: two H1 sections → two chunks.
        fs::write(&path, b"# Alpha\n\nBody one.\n\n# Bravo\n\nBody two.\n").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let scanner = stub_scanner(&config, &store);

        scanner.reindex_path("note.md").await.unwrap();
        assert_eq!(count_chunks_for(&store, "note.md").await, 2);
        assert_eq!(count_chunks_vec(&store).await, 2);

        // Second version: three H1 sections → three chunks.
        fs::write(
            &path,
            b"# Alpha\n\nBody one.\n\n# Bravo\n\nBody two.\n\n# Charlie\n\nBody three.\n",
        )
        .unwrap();
        scanner.reindex_path("note.md").await.unwrap();
        assert_eq!(count_chunks_for(&store, "note.md").await, 3);
        // chunks_vec should be in lockstep — old 2 gone.
        assert_eq!(count_chunks_vec(&store).await, 3);
    }

    #[tokio::test]
    async fn reindex_skips_on_embedding_transport_error() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("note.md"), b"# A\n\nbody.\n").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let failing = AlwaysSkipEmbedder::new();
        let scanner = Scanner::new(
            &smoke_vault_path(&config),
            &config,
            &store,
            failing.clone() as Arc<dyn Embedder>,
        )
        .unwrap();

        let outcome = scanner.reindex_path("note.md").await.unwrap();
        assert_eq!(
            outcome,
            ReindexOutcome::HashUnchanged,
            "embedding-skip must yield HashUnchanged (no advance)"
        );
        assert!(
            failing.calls() >= 1,
            "embedder should have been invoked at least once before the skip"
        );

        // Most important assertion (workplan-prescribed): no advance of files.content_hash —
        // here, no `files` row at all, since the file was new and the write tx never committed.
        assert_eq!(
            read_hash_optional(&store, "note.md").await,
            None,
            "files.content_hash must not advance on embedding skip"
        );
        assert_eq!(count_chunks_for(&store, "note.md").await, 0);
        assert_eq!(count_chunks_vec(&store).await, 0);
    }

    #[tokio::test]
    async fn reindex_skips_on_embedding_dimension_mismatch() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        fs::write(vault_dir.path().join("note.md"), b"# A\n\nbody.\n").unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let failing = AlwaysDimensionMismatchEmbedder::new();
        let scanner = Scanner::new(
            &smoke_vault_path(&config),
            &config,
            &store,
            failing.clone() as Arc<dyn Embedder>,
        )
        .unwrap();

        let outcome = scanner.reindex_path("note.md").await.unwrap();
        assert_eq!(
            outcome,
            ReindexOutcome::HashUnchanged,
            "dimension-mismatch must yield HashUnchanged (no advance)"
        );
        assert!(
            failing.calls() >= 1,
            "embedder should have been invoked at least once before the skip"
        );

        // No advance of files.content_hash — here, no `files` row at all,
        // since the file was new and the write tx never committed.
        assert_eq!(
            read_hash_optional(&store, "note.md").await,
            None,
            "files.content_hash must not advance on dimension-mismatch skip"
        );
        assert_eq!(count_chunks_for(&store, "note.md").await, 0);
        assert_eq!(count_chunks_vec(&store).await, 0);
    }

    #[tokio::test]
    async fn reindex_zero_chunks_for_frontmatter_only_file() {
        let vault_dir = tempdir().unwrap();
        let data_dir = tempdir().unwrap();
        let body = b"---\ntitle: Frontmatter only\ntags: [a, b]\n---\n";
        fs::write(vault_dir.path().join("fm.md"), body).unwrap();

        let config = smoke_config(vault_dir.path());
        let store = Store::open(
            &VaultId::new(),
            data_dir.path(),
            "index.sqlite",
            &config.embedding,
        )
        .await
        .unwrap();
        let recorder = RecordingEmbedder::new(768);
        let scanner = Scanner::new(
            &smoke_vault_path(&config),
            &config,
            &store,
            recorder.clone() as Arc<dyn Embedder>,
        )
        .unwrap();

        let outcome = scanner.reindex_path("fm.md").await.unwrap();
        match outcome {
            ReindexOutcome::Inserted { content_hash } => {
                let expected = hash_file(&vault_dir.path().join("fm.md")).unwrap();
                assert_eq!(
                    content_hash, expected,
                    "files.content_hash must advance normally for frontmatter-only files"
                );
            }
            other => panic!("expected Inserted, got {other:?}"),
        }
        assert_eq!(count_chunks_for(&store, "fm.md").await, 0);
        assert_eq!(count_chunks_vec(&store).await, 0);
        assert!(
            recorder.texts().is_empty(),
            "embedder must not be called for a zero-chunk file"
        );
    }

    // --- Task 9.3 prescribed test: per-vault isolation ---

    #[tokio::test]
    async fn indexer_writes_to_correct_per_vault_store() {
        let vault_a = tempdir().unwrap();
        let vault_b = tempdir().unwrap();
        let data_dir = tempdir().unwrap();

        fs::write(vault_a.path().join("only-in-a.md"), b"# only in a").unwrap();
        fs::write(vault_b.path().join("only-in-b.md"), b"# only in b").unwrap();

        let config_a = smoke_config(vault_a.path());
        let config_b = smoke_config(vault_b.path());

        let vault_id_a = VaultId::new();
        let vault_id_b = VaultId::new();

        let store_a = Store::open(
            &vault_id_a,
            data_dir.path(),
            "index.sqlite",
            &config_a.embedding,
        )
        .await
        .unwrap();
        let store_b = Store::open(
            &vault_id_b,
            data_dir.path(),
            "index.sqlite",
            &config_b.embedding,
        )
        .await
        .unwrap();

        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let scanner_a = Scanner::new(
            &smoke_vault_path(&config_a),
            &config_a,
            &store_a,
            embedder.clone(),
        )
        .unwrap();
        let scanner_b = Scanner::new(
            &smoke_vault_path(&config_b),
            &config_b,
            &store_b,
            embedder.clone(),
        )
        .unwrap();

        scanner_a.run().await.unwrap();
        scanner_b.run().await.unwrap();

        assert_eq!(count_files(&store_a).await, 1);
        assert_eq!(count_files(&store_b).await, 1);
        assert_eq!(
            read_hash_optional(&store_a, "only-in-a.md").await,
            Some(hash_file(&vault_a.path().join("only-in-a.md")).unwrap()),
        );
        assert_eq!(
            read_hash_optional(&store_a, "only-in-b.md").await,
            None,
            "vault A's store must not see vault B's file",
        );
        assert_eq!(
            read_hash_optional(&store_b, "only-in-b.md").await,
            Some(hash_file(&vault_b.path().join("only-in-b.md")).unwrap()),
        );
        assert_eq!(
            read_hash_optional(&store_b, "only-in-a.md").await,
            None,
            "vault B's store must not see vault A's file",
        );
    }
}
