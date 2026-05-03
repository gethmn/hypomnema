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
//! is no cross-vault event coalescing in the debounce window. The vault
//! identity threads through to each emitted [`crate::events::StreamEvent`]
//! published to the daemon-level [`crate::events::EventBus`].
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
pub mod inclusion;
mod translate;
pub mod vcs_ignore;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{Debouncer, RecommendedCache, new_debouncer};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use crate::events::{EventBus, EventType, StreamEvent};
use crate::indexer::{ReindexOutcome, RemoveOutcome, Scanner};
use crate::vault_registry::VaultId;
use inclusion::InclusionFilter;
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
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl Watcher {
    pub fn vault_id(&self) -> &VaultId {
        &self.vault_id
    }
}

pub fn spawn_watcher(
    vault_id: &VaultId,
    vault: &Path,
    filter: Arc<InclusionFilter>,
    debounce: Duration,
    buffer: usize,
) -> Result<(Watcher, mpsc::Receiver<WatchEvent>)> {
    let canonical_vault = fs::canonicalize(vault)
        .with_context(|| format!("canonicalizing vault for watcher: {}", vault.display()))?;
    let watched_vault = absolute_path(vault)
        .with_context(|| format!("resolving watched vault path: {}", vault.display()))?;

    let (tx, rx) = mpsc::channel::<WatchEvent>(buffer);
    let ctx = TranslateCtx {
        vault_roots: distinct_roots([canonical_vault.clone(), watched_vault.clone()]),
        filter,
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
        .watch(&watched_vault, RecursiveMode::Recursive)
        .with_context(|| format!("registering recursive watch on {}", watched_vault.display()))?;

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

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn distinct_roots<const N: usize>(roots: [PathBuf; N]) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(N);
    for root in roots {
        if !out.iter().any(|existing| existing == &root) {
            out.push(root);
        }
    }
    out
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
    vault_id: VaultId,
    events: Arc<EventBus>,
    mut shutdown_rx: watch::Receiver<bool>,
    mut rescan_rx: watch::Receiver<u64>,
) {
    // Honour the initial value too — if shutdown already fired before we
    // started polling, drain and exit without waiting for another change.
    if *shutdown_rx.borrow() {
        drain_remaining(&mut rx, &scanner, &vault_id, &events).await;
        return;
    }
    loop {
        tokio::select! {
            biased;
            res = shutdown_rx.changed() => {
                // Either a new value or the sender was dropped; in both
                // cases the right move is to drain and exit.
                if res.is_err() || *shutdown_rx.borrow() {
                    drain_remaining(&mut rx, &scanner, &vault_id, &events).await;
                    break;
                }
            }
            res = rescan_rx.changed() => {
                // A rescan request was signalled. If the sender was dropped
                // (manager teardown without a graceful shutdown), fall back
                // to the shutdown path so we drain and exit.
                if res.is_err() {
                    drain_remaining(&mut rx, &scanner, &vault_id, &events).await;
                    break;
                }
                run_rescan(&scanner, &vault_id, &events).await;
            }
            event = rx.recv() => {
                match event {
                    Some(ev) => apply_event(ev, &scanner, &vault_id, &events).await,
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
async fn run_rescan(scanner: &Scanner, vault_id: &VaultId, events: &EventBus) {
    let rels = match scanner.vault_paths().await {
        Ok(rels) => rels,
        Err(e) => {
            tracing::warn!(
                vault_id = %vault_id,
                error = ?e,
                "rescan: walk failed"
            );
            return;
        }
    };
    tracing::info!(
        vault_id = %vault_id,
        file_count = rels.len(),
        "rescan: starting"
    );
    for rel in rels {
        apply_event(WatchEvent::Upsert(rel), scanner, vault_id, events).await;
    }
    tracing::info!(vault_id = %vault_id, "rescan: complete");
}

async fn drain_remaining(
    rx: &mut mpsc::Receiver<WatchEvent>,
    scanner: &Scanner,
    vault_id: &VaultId,
    events: &EventBus,
) {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match timeout(remaining, rx.recv()).await {
            Ok(Some(ev)) => apply_event(ev, scanner, vault_id, events).await,
            Ok(None) | Err(_) => break,
        }
    }
}

// Order matters: the index update happens first, then the live event is
// published. Hash-unchanged and not-present outcomes stay silent.
async fn apply_event(ev: WatchEvent, scanner: &Scanner, vault_id: &VaultId, events: &EventBus) {
    match ev {
        WatchEvent::Upsert(rel) => match scanner.reindex_path(&rel).await {
            Ok(ReindexOutcome::Inserted { content_hash }) => {
                emit(
                    events,
                    vault_id,
                    EventType::Created,
                    rel,
                    Some(content_hash),
                );
            }
            Ok(ReindexOutcome::Updated { content_hash }) => {
                emit(
                    events,
                    vault_id,
                    EventType::Modified,
                    rel,
                    Some(content_hash),
                );
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
                        emit(
                            events,
                            vault_id,
                            EventType::Deleted,
                            rel,
                            Some(previous_hash),
                        );
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
                emit(
                    events,
                    vault_id,
                    EventType::Deleted,
                    rel,
                    Some(previous_hash),
                );
            }
            Ok(RemoveOutcome::NotPresent) => {}
            Err(e) => {
                tracing::warn!(vault_id = %vault_id, rel, error = ?e, "watcher: remove failed")
            }
        },
    }
}

fn emit(
    events: &EventBus,
    vault_id: &VaultId,
    event_type: EventType,
    rel: String,
    hash: Option<String>,
) {
    events.publish(StreamEvent::file_changed(
        vault_id.clone(),
        event_type,
        rel,
        hash,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigPath};
    use crate::embedding::{Embedder, StubEmbedder};
    use crate::events::{FileChangedEvent, StreamEvent};
    use crate::indexer::Scanner;
    use crate::store::Store;
    use crate::watcher::vcs_ignore::VcsIgnore;
    use std::io::Write;
    use tempfile::tempdir;
    use tokio::sync::broadcast;
    use tokio::sync::watch;
    use tokio::task::JoinHandle;

    const SETTLE: Duration = Duration::from_millis(500);
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
        _rescan_tx: watch::Sender<u64>,
        consumer: JoinHandle<()>,
        event_bus: Arc<EventBus>,
    }

    impl LiveVault {
        async fn shutdown(self) {
            let _ = self.shutdown_tx.send(true);
            let _ = self.consumer.await;
            drop(self.watcher);
        }

        fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
            self.event_bus.subscribe()
        }
    }

    async fn start_vault(vault_path: &Path, data_dir: &Path, vault_id: VaultId) -> LiveVault {
        let config = config_for(vault_path, data_dir);
        start_vault_with_config(vault_path, vault_id, config).await
    }

    async fn start_vault_with_config(
        vault_path: &Path,
        vault_id: VaultId,
        config: Config,
    ) -> LiveVault {
        let store = Store::open(
            &vault_id,
            &config.storage.data_dir.0,
            &config.storage.index_file,
            &config.embedding,
        )
        .await
        .expect("open store");
        let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
        let scanner =
            Scanner::new(vault_path, &config, &store, embedder).expect("construct scanner");
        scanner.run().await.expect("initial scan");

        let filter = Arc::new(InclusionFilter {
            config: config
                .watcher
                .compiled_ignores_split()
                .expect("compile ignores"),
            vcs: VcsIgnore::build(vault_path).expect("build vcs ignores"),
            respect_gitignore: config.watcher.respect_gitignore,
        });
        let (watcher, rx) = spawn_watcher(
            &vault_id,
            vault_path,
            filter,
            Duration::from_millis(DEBOUNCE_MS),
            64,
        )
        .expect("spawn watcher");

        let event_bus = Arc::new(EventBus::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (rescan_tx, rescan_rx) = watch::channel(0u64);
        let consumer = tokio::spawn(run_consumer(
            rx,
            scanner,
            vault_id,
            event_bus.clone(),
            shutdown_rx,
            rescan_rx,
        ));
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;

        LiveVault {
            watcher,
            shutdown_tx,
            _rescan_tx: rescan_tx,
            consumer,
            event_bus,
        }
    }

    async fn recv_file_changed(
        rx: &mut broadcast::Receiver<StreamEvent>,
    ) -> Option<FileChangedEvent> {
        match tokio::time::timeout(SETTLE, rx.recv()).await {
            Ok(Ok(StreamEvent::FileChanged(ev))) => Some(ev),
            Ok(Ok(other)) => panic!("unexpected stream event: {other:?}"),
            Ok(Err(e)) => panic!("event receive failed: {e:?}"),
            Err(_) => None,
        }
    }

    #[tokio::test]
    async fn live_watcher_smoke_surfaces_create_modify_delete() {
        // Real-file smoke: a watched vault should surface a create, a modify,
        // and a delete through the same event pipeline the daemon uses.
        let root = tempdir().unwrap();
        let vault = root.path().join("vault");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&vault).unwrap();

        let vault_id = VaultId::new();
        let live = start_vault(&vault, &data_dir, vault_id.clone()).await;
        let mut rx = live.subscribe();

        // Give the watcher thread a brief chance to finish arming before the
        // first file write. This smoke only needs to prove the file-change
        // surface, not the registration latency.
        tokio::time::sleep(SETTLE).await;

        // Create.
        let file = vault.join("smoke.md");
        fs::write(&file, b"# smoke\n").unwrap();
        tokio::time::sleep(SETTLE).await;
        fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"\nmore smoke\n")
            .unwrap();
        let event_create_or_modify = recv_file_changed(&mut rx)
            .await
            .expect("watched vault should publish a create-or-modify event");

        assert_eq!(event_create_or_modify.path, "smoke.md");
        assert_eq!(event_create_or_modify.vault, vault_id);

        // Modify.
        fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap()
            .write_all(b"\nmore smoke again\n")
            .unwrap();
        let event_modify = recv_file_changed(&mut rx)
            .await
            .expect("watched vault should publish a modify event");
        assert_eq!(event_modify.path, "smoke.md");
        assert_eq!(event_modify.vault, vault_id);

        // Delete.
        fs::remove_file(&file).unwrap();
        tokio::time::sleep(SETTLE).await;
        let event_delete = recv_file_changed(&mut rx)
            .await
            .expect("watched vault should publish a delete event");
        assert_eq!(event_delete.path, "smoke.md");
        assert_eq!(event_delete.vault, vault_id);

        live.shutdown().await;
    }

    #[tokio::test]
    async fn watcher_respects_gitignore_excludes() {
        // A vault with a .gitignore that excludes `node_modules/` should not
        // emit watcher events for files created inside that directory.
        let root = tempdir().unwrap();
        let vault = root.path().join("vault");
        let data_dir = root.path().join("data");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join(".gitignore"), b"node_modules/\n").unwrap();
        fs::create_dir_all(vault.join("node_modules")).unwrap();

        let vault_id = VaultId::new();
        let live = start_vault(&vault, &data_dir, vault_id.clone()).await;
        let mut rx = live.subscribe();

        tokio::time::sleep(SETTLE).await;

        // Write a file outside node_modules/ — should surface an event.
        let kept = vault.join("kept.md");
        fs::write(&kept, b"# kept\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let ev = recv_file_changed(&mut rx)
            .await
            .expect("kept.md should emit a watcher event");
        assert_eq!(ev.path, "kept.md");

        // Write a file inside node_modules/ — gitignore says exclude, so no event.
        let ignored = vault.join("node_modules/pkg.md");
        fs::write(&ignored, b"# pkg\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let no_ev = recv_file_changed(&mut rx).await;
        assert!(
            no_ev.is_none(),
            "node_modules/pkg.md must be silenced by .gitignore; got event: {no_ev:?}"
        );

        live.shutdown().await;
    }

    #[tokio::test]
    async fn watcher_config_reinclude_beats_gitignore() {
        // .gitignore excludes the entire `drafts/` directory. The operator adds
        // `!drafts/important.md` to watcher.ignore_patterns to re-include one
        // file. Only drafts/important.md should surface watcher events;
        // drafts/other.md remains silenced by .gitignore.
        let root = tempdir().unwrap();
        let vault = root.path().join("vault");
        let data_dir = root.path().join("data");
        fs::create_dir_all(vault.join("drafts")).unwrap();
        fs::write(vault.join(".gitignore"), b"drafts/\n").unwrap();

        let mut config = config_for(&vault, &data_dir);
        config.watcher.ignore_patterns = vec!["!drafts/important.md".to_string()];

        let vault_id = VaultId::new();
        let live = start_vault_with_config(&vault, vault_id.clone(), config).await;
        let mut rx = live.subscribe();

        tokio::time::sleep(SETTLE).await;

        // drafts/important.md is re-included by the config override — should emit an event.
        let important = vault.join("drafts/important.md");
        fs::write(&important, b"# important\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let ev = recv_file_changed(&mut rx).await.expect(
            "drafts/important.md must emit a watcher event (config re-include beats .gitignore)",
        );
        assert_eq!(ev.path, "drafts/important.md");

        // drafts/other.md is excluded by .gitignore with no override — no event.
        fs::write(vault.join("drafts/other.md"), b"# other\n").unwrap();
        tokio::time::sleep(SETTLE).await;

        let no_ev = recv_file_changed(&mut rx).await;
        assert!(
            no_ev.is_none(),
            "drafts/other.md must be silenced by .gitignore; got event: {no_ev:?}"
        );

        live.shutdown().await;
    }
}
