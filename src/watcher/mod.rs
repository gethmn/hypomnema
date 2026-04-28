//! Filesystem watcher for the vault.
//!
//! Translates `notify-debouncer-full` output into a stream of
//! [`WatchEvent`] values that downstream consumers (the indexer in the
//! daemon, the integration tests in `tests/watch.rs`) drive against the
//! `Scanner`'s single-file methods. The translation layer applies the
//! relevance filter, ignore globset, and sync-conflict filter at the
//! boundary so that everything on the channel is already known to be a
//! `.md` file inside the vault that the user cares about.
//!
//! Two patterns from `.claude/skills/filesystem-watching` are load-bearing:
//!
//! - `notify-debouncer-full` is mandatory; raw `notify` events are too
//!   chaotic on their own (editor write-temp-and-rename storms,
//!   sync-tool burst patterns).
//! - The `notify` callback runs on a thread `notify` owns; this module
//!   does only translation and `tx.blocking_send` inside that callback.
//!   No I/O, no SQL, no async work.
//!
//! ## Per-vault construction
//!
//! Each [`Watcher`] is bound to one vault: `spawn_watcher(vault_id, ...)`
//! registers one `notify` watcher on one vault root. A daemon running N
//! vaults runs N independent watchers + N independent debouncers — there
//! is no shared event stream, so no cross-vault event coalescing in the
//! debounce window. The vault identity threads through to each emitted
//! [`crate::outbox::ChangeEvent`] via the per-vault [`crate::outbox::Outbox`]
//! that [`run_consumer`] writes to.
//!
//! ## Known v0 limitation: symlinks
//!
//! `notify` does not follow symlinks by default. The step-2 walker does
//! (`WalkDir::follow_links(true)`), so a file reachable only via a
//! symlink inside the vault will be picked up by `hmn vault rescan` and
//! on the daemon's startup re-scan, but live edits to it will not produce
//! a watcher event until the next restart. Documented as a v0 trade-off
//! rather than worked around — the startup re-scan is the safety net.

pub mod filter;
mod translate;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use globset::GlobSet;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use notify_debouncer_full::{Debouncer, FileIdMap, new_debouncer};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::indexer::{ReindexOutcome, RemoveOutcome, Scanner};
use crate::outbox::{ChangeEvent, EventType, Outbox};
use crate::vault_registry::VaultId;
use translate::{TranslateCtx, translate};

const BACKPRESSURE_WARN_EVERY: usize = 64;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Upsert(String),
    Remove(String),
}

pub struct Watcher {
    vault_id: VaultId,
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
}

impl Watcher {
    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }
}

pub fn spawn_watcher(
    vault_id: &VaultId,
    vault: &Path,
    ignores: GlobSet,
    debounce: Duration,
    buffer: usize,
) -> Result<(Watcher, mpsc::Receiver<WatchEvent>)> {
    let canonical_vault = fs::canonicalize(vault)
        .with_context(|| format!("canonicalizing vault for watcher: {}", vault.display()))?;

    let (tx, rx) = mpsc::channel::<WatchEvent>(buffer);
    let ctx = TranslateCtx {
        canonical_vault: canonical_vault.clone(),
        ignores,
    };

    // Backpressure counter: incremented every time a `try_send` would have
    // blocked the notify thread. Per `.claude/skills/filesystem-watching`,
    // backpressure is a signal to surface, not an error to swallow — we log
    // a warn line every `BACKPRESSURE_WARN_EVERY` blocked sends and let the
    // notify thread block on `blocking_send` (the desired shape).
    let blocked = Arc::new(AtomicUsize::new(0));
    let vault_id_for_warn = vault_id.clone();
    let mut debouncer = new_debouncer(debounce, None, move |result| match result {
        Ok(events) => {
            for ev in translate(events, &ctx) {
                match tx.try_send(ev) {
                    Ok(()) => {}
                    Err(TrySendError::Full(ev)) => {
                        let n = blocked.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % BACKPRESSURE_WARN_EVERY == 0 {
                            tracing::warn!(
                                vault_id = %vault_id_for_warn,
                                blocked_total = n,
                                "watcher: channel full; notify thread blocking on send (backpressure)"
                            );
                        }
                        if tx.blocking_send(ev).is_err() {
                            break;
                        }
                    }
                    Err(TrySendError::Closed(_)) => {
                        // Receiver dropped — daemon is shutting down.
                        break;
                    }
                }
            }
        }
        Err(errs) => {
            for e in errs {
                tracing::warn!(vault_id = %vault_id_for_warn, error = ?e, "watcher: notify error");
            }
        }
    })
    .context("constructing notify debouncer")?;

    debouncer
        .watcher()
        .watch(&canonical_vault, RecursiveMode::Recursive)
        .with_context(|| {
            format!(
                "registering recursive watch on {}",
                canonical_vault.display()
            )
        })?;

    tracing::info!(
        vault_id = %vault_id,
        vault = %canonical_vault.display(),
        "watcher: spawned"
    );

    Ok((
        Watcher {
            vault_id: vault_id.clone(),
            _debouncer: debouncer,
        },
        rx,
    ))
}

/// Drive the watcher channel against a [`Scanner`], reindexing or removing
/// each watched file and applying the shutdown contract.
///
/// Hoisted out of `hmnd::run_daemon` so the binary and the integration tests
/// in `tests/watch.rs` share the same loop body. Per
/// `.claude/skills/rusqlite-in-async`, every SQL call here happens inside a
/// `Scanner` method, which wraps the work in `spawn_blocking`.
///
/// Loop shape: a `tokio::select!` with `biased` ordering favours the shutdown
/// signal so a saturated channel cannot starve the drain. On shutdown, drain
/// the channel best-effort, time-boxed to [`DRAIN_TIMEOUT`], so in-flight
/// events committed by the debouncer just before shutdown still land.
pub async fn run_consumer(
    mut rx: mpsc::Receiver<WatchEvent>,
    scanner: Scanner,
    outbox: Outbox,
    mut shutdown_rx: watch::Receiver<bool>,
    mut rescan_rx: watch::Receiver<u64>,
) {
    // Honour the initial value too — if shutdown already fired before we
    // started polling, drain and exit without waiting for another change.
    if *shutdown_rx.borrow() {
        drain_remaining(&mut rx, &scanner, &outbox).await;
        return;
    }
    loop {
        tokio::select! {
            biased;
            res = shutdown_rx.changed() => {
                // Either a new value or the sender was dropped; in both
                // cases the right move is to drain and exit.
                if res.is_err() || *shutdown_rx.borrow() {
                    drain_remaining(&mut rx, &scanner, &outbox).await;
                    break;
                }
            }
            res = rescan_rx.changed() => {
                // A rescan request was signalled. If the sender was dropped
                // (manager teardown without a graceful shutdown), fall back
                // to the shutdown path so we drain and exit.
                if res.is_err() {
                    drain_remaining(&mut rx, &scanner, &outbox).await;
                    break;
                }
                run_rescan(&scanner, &outbox).await;
            }
            event = rx.recv() => {
                match event {
                    Some(ev) => apply_event(ev, &scanner, &outbox).await,
                    None => break,
                }
            }
        }
    }
}

/// Walk the vault and re-emit `Upsert` events for every file. Drives the
/// existing per-file `apply_event` pipeline so files whose `content_hash`
/// drifted from the on-disk hash produce `modified` events; new files
/// produce `created` events. On an up-to-date vault every `reindex_path`
/// returns `HashUnchanged`, which `apply_event` silently swallows — that is
/// the documented "rescan-without-rebuild on a quiet vault produces few
/// events" edge case (see `docs/specs/vault-management.md` § rescan and
/// `notes/roadmap/step-11-workplan.md` § Task 11.2 cold-start emission
/// policy). Operators wanting cold-start emission for every file should
/// pair `rescan` with `reset --rebuild`, which clears `content_hash` and
/// forces re-emit.
async fn run_rescan(scanner: &Scanner, outbox: &Outbox) {
    let rels = match scanner.vault_paths().await {
        Ok(rels) => rels,
        Err(e) => {
            tracing::warn!(
                vault_id = %outbox.vault_id(),
                error = ?e,
                "rescan: walk failed"
            );
            return;
        }
    };
    tracing::info!(
        vault_id = %outbox.vault_id(),
        file_count = rels.len(),
        "rescan: starting"
    );
    for rel in rels {
        apply_event(WatchEvent::Upsert(rel), scanner, outbox).await;
    }
    tracing::info!(vault_id = %outbox.vault_id(), "rescan: complete");
}

async fn drain_remaining(rx: &mut mpsc::Receiver<WatchEvent>, scanner: &Scanner, outbox: &Outbox) {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match timeout(remaining, rx.recv()).await {
            Ok(Some(ev)) => apply_event(ev, scanner, outbox).await,
            Ok(None) | Err(_) => break,
        }
    }
}

// Order matters: the index update happens first, the outbox append second.
// A failed outbox append logs `warn` and the consumer continues — the index
// is already updated; the outbox simply has a missing line for that one
// event. We do not roll back the index on outbox failure (see
// docs/specs/change-events.md § Edge Cases).
async fn apply_event(ev: WatchEvent, scanner: &Scanner, outbox: &Outbox) {
    let vault_id = outbox.vault_id();
    match ev {
        WatchEvent::Upsert(rel) => match scanner.reindex_path(&rel).await {
            Ok(ReindexOutcome::Inserted { content_hash }) => {
                emit(outbox, EventType::Created, rel, Some(content_hash)).await;
            }
            Ok(ReindexOutcome::Updated { content_hash }) => {
                emit(outbox, EventType::Modified, rel, Some(content_hash)).await;
            }
            Ok(ReindexOutcome::HashUnchanged) => {}
            Ok(ReindexOutcome::MissingFromDisk) => {
                tracing::warn!(
                    vault_id = %vault_id,
                    rel,
                    "watcher: file missing on reindex; following up with remove"
                );
                match scanner.remove_path(&rel).await {
                    Ok(RemoveOutcome::Removed { previous_hash }) => {
                        emit(outbox, EventType::Deleted, rel, Some(previous_hash)).await;
                    }
                    Ok(RemoveOutcome::NotPresent) => {}
                    Err(e) => tracing::warn!(
                        vault_id = %vault_id,
                        rel,
                        error = ?e,
                        "watcher: remove follow-up failed"
                    ),
                }
            }
            Err(e) => {
                tracing::warn!(vault_id = %vault_id, rel, error = ?e, "watcher: upsert failed")
            }
        },
        WatchEvent::Remove(rel) => match scanner.remove_path(&rel).await {
            Ok(RemoveOutcome::Removed { previous_hash }) => {
                emit(outbox, EventType::Deleted, rel, Some(previous_hash)).await;
            }
            Ok(RemoveOutcome::NotPresent) => {}
            Err(e) => {
                tracing::warn!(vault_id = %vault_id, rel, error = ?e, "watcher: remove failed")
            }
        },
    }
}

async fn emit(outbox: &Outbox, event_type: EventType, rel: String, hash: Option<String>) {
    let ev = ChangeEvent::now(outbox.vault_id().clone(), event_type, rel.clone(), hash);
    if let Err(e) = outbox.append(ev).await {
        tracing::warn!(
            vault_id = %outbox.vault_id(),
            rel,
            error = ?e,
            "watcher: outbox append failed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigPath};
    use crate::embedding::{Embedder, StubEmbedder};
    use crate::indexer::Scanner;
    use crate::store::Store;
    use crate::vault_registry::vault_data_dir;
    use globset::GlobSetBuilder;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    const SETTLE: Duration = Duration::from_millis(150);
    const DEBOUNCE_MS: u64 = 50;

    fn config_for(vault_path: &Path, data_dir: &Path) -> Config {
        let mut config = Config::default_for_smoke_test(fs::canonicalize(vault_path).unwrap());
        config.storage.data_dir = ConfigPath(data_dir.to_path_buf());
        config.watcher.debounce_ms = DEBOUNCE_MS;
        config.watcher.ignore_patterns = Vec::new();
        config
    }

    struct LiveVault {
        watcher: Watcher,
        shutdown_tx: watch::Sender<bool>,
        consumer: JoinHandle<()>,
        outbox_path: PathBuf,
    }

    impl LiveVault {
        async fn shutdown(self) {
            let _ = self.shutdown_tx.send(true);
            let _ = self.consumer.await;
            drop(self.watcher);
        }
    }

    async fn start_vault(vault_path: &Path, data_dir: &Path, vault_id: VaultId) -> LiveVault {
        let config = config_for(vault_path, data_dir);
        let store = Store::open(
            &vault_id,
            data_dir,
            &config.storage.index_file,
            &config.embedding,
        )
        .await
        .expect("open store");
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let scanner =
            Scanner::new(vault_path, &config, &store, embedder).expect("construct scanner");
        scanner.run().await.expect("initial scan");

        let ignores = GlobSetBuilder::new().build().unwrap();
        let (watcher, rx) = spawn_watcher(
            &vault_id,
            vault_path,
            ignores,
            Duration::from_millis(DEBOUNCE_MS),
            64,
        )
        .expect("spawn watcher");

        let outbox_path = vault_data_dir(data_dir, &vault_id).join("outbox.jsonl");
        let outbox = Outbox::open(vault_id.clone(), outbox_path.clone())
            .await
            .expect("open outbox");
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (_rescan_tx, rescan_rx) = watch::channel(0u64);
        let consumer = tokio::spawn(run_consumer(rx, scanner, outbox, shutdown_rx, rescan_rx));

        LiveVault {
            watcher,
            shutdown_tx,
            consumer,
            outbox_path,
        }
    }

    fn read_events(path: &Path) -> Vec<ChangeEvent> {
        if !path.exists() {
            return Vec::new();
        }
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).expect("parse outbox line"))
            .collect()
    }

    #[tokio::test]
    async fn two_watchers_emit_to_separate_outboxes() {
        // Per-vault isolation: two watchers + two outbox writers under the
        // same data_dir, each bound to a distinct vault_id, must produce
        // disjoint event streams. A change in vault A's tree only ever
        // shows up in vault A's outbox; vault B's outbox stays empty (or
        // carries only its own events). This is the multi-vault watcher
        // foundation tasks 9.5/9.7 build on.
        let root = tempdir().unwrap();
        let vault_a = root.path().join("vault-a");
        let vault_b = root.path().join("vault-b");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&vault_a).unwrap();
        fs::create_dir_all(&vault_b).unwrap();

        let vault_id_a = VaultId::new();
        let vault_id_b = VaultId::new();
        assert_ne!(vault_id_a, vault_id_b);

        let live_a = start_vault(&vault_a, &data_dir, vault_id_a.clone()).await;
        let live_b = start_vault(&vault_b, &data_dir, vault_id_b.clone()).await;

        // Sanity: paths are under per-vault subdirs.
        assert_eq!(
            live_a.outbox_path,
            vault_data_dir(&data_dir, &vault_id_a).join("outbox.jsonl")
        );
        assert_eq!(
            live_b.outbox_path,
            vault_data_dir(&data_dir, &vault_id_b).join("outbox.jsonl")
        );
        assert_ne!(live_a.outbox_path, live_b.outbox_path);

        // Write a file under vault A only.
        fs::write(vault_a.join("a-only.md"), b"# only in a\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let events_a = read_events(&live_a.outbox_path);
        let events_b = read_events(&live_b.outbox_path);

        assert_eq!(
            events_a.len(),
            1,
            "vault A's outbox must carry the create event, got {events_a:?}"
        );
        assert_eq!(events_a[0].path, "a-only.md");
        assert_eq!(events_a[0].event_type, EventType::Created);
        assert_eq!(
            events_a[0].vault, vault_id_a,
            "event line must carry vault A's id"
        );
        assert!(
            events_b.is_empty(),
            "vault B's outbox must stay empty; got {events_b:?}"
        );

        // Now write under vault B and confirm A's outbox does not grow.
        fs::write(vault_b.join("b-only.md"), b"# only in b\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let events_a = read_events(&live_a.outbox_path);
        let events_b = read_events(&live_b.outbox_path);
        assert_eq!(
            events_a.len(),
            1,
            "vault A's outbox must not pick up vault B's edit, got {events_a:?}"
        );
        assert_eq!(events_b.len(), 1, "vault B's outbox must carry one event");
        assert_eq!(events_b[0].path, "b-only.md");
        assert_eq!(events_b[0].vault, vault_id_b);

        live_a.shutdown().await;
        live_b.shutdown().await;
    }
}
