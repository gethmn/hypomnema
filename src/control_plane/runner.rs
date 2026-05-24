use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::api::VaultEntry;
use crate::watcher::Watcher;

pub struct VaultRunner {
    /// Live snapshot of this vault's view exposed to search handlers.
    /// Wrapped in `RwLock` so step-11 ops (pause/resume/reset/rename) can
    /// swap a fresh `Arc<VaultEntry>` in without disturbing readers — search
    /// handlers clone the inner Arc and release the read-lock in
    /// microseconds.
    entry: RwLock<Arc<VaultEntry>>,
    /// Per-vault operation lock. Held by step-11 ops
    /// (pause/resume/reset/rename/rescan) that mutate vault state without
    /// changing the runners-map membership. Step-10's create/terminate take
    /// the outer RwLock instead.
    pub(crate) op_lock: Mutex<()>,
    /// Background bootstrap (initial-scan) task. Holds the bootstrap task's
    /// shutdown sender + join handle (see step 24's async-bootstrap path).
    /// `Some` once the manager installs the lifecycle after
    /// `spawn_runner_parts` returns; the slot is *not* cleared by the
    /// bootstrap task on completion — it stays `Some` until
    /// `shutdown_with_timeout` `take`s it (or a resume/reset overwrites it
    /// with a fresh bootstrap). Held in an `Arc<Mutex<>>` so the manager can
    /// install it post-spawn and shutdown can drain it without an exclusive
    /// borrow of the runner.
    pub(crate) bootstrap: Arc<Mutex<Option<BootstrapLifecycle>>>,
    /// Lifecycle handles for the watcher + consumer task. `None` until the
    /// bootstrap task installs them on scan success; remains `None` after
    /// `shutdown_with_timeout` has drained or after a `for_tests`
    /// construction. Wrapped in `Arc<Mutex<>>` so the bootstrap task can
    /// install a new lifecycle into the runner's slot.
    pub(crate) lifecycle: Arc<Mutex<Option<RunnerLifecycle>>>,
    /// Latched signal flipped from `false` → `true` once the in-memory
    /// `BootstrapState` reaches a terminal value (`Ready` or `Errored`).
    /// Surfaced to test code via `wait_for_bootstrap`. The runner holds the
    /// `Sender` to keep the channel open for late subscribers; the bootstrap
    /// task holds a clone and sends `true` from its terminal arm. `for_tests`
    /// runners create the channel already in the `true` state so existing
    /// fixtures (which build entries with `BootstrapState::ready_state()`)
    /// satisfy `wait_for_bootstrap` immediately.
    pub(crate) bootstrap_done_tx: watch::Sender<bool>,
}

/// Tracks an in-flight bootstrap (initial-scan) task. The shutdown channel
/// is shared with the eventual `RunnerLifecycle` so a single send fires both
/// the scan-cancel and the consumer-drain paths.
pub(crate) struct BootstrapLifecycle {
    pub shutdown_tx: watch::Sender<bool>,
    pub handle: JoinHandle<()>,
}

pub(crate) struct RunnerLifecycle {
    pub shutdown_tx: watch::Sender<bool>,
    /// Rescan request channel. The manager increments the inner counter via
    /// `send_modify(|v| *v = v.wrapping_add(1))` to wake the consumer's
    /// rescan arm; the consumer walks the vault and emits `created` /
    /// `modified` events for each file via the same `apply_event` path that
    /// drives live watcher events. Mirrors the shutdown-channel pattern so
    /// the consumer's `select!` covers both signals.
    pub rescan_tx: watch::Sender<u64>,
    pub consumer_handle: JoinHandle<()>,
    pub watcher: Watcher,
}

impl VaultRunner {
    /// Construct a runner with a bootstrap task already in flight. The
    /// `lifecycle` slot is provided pre-built so the bootstrap task can hold
    /// a clone and install the watcher + consumer when the initial scan
    /// completes; the manager shares the same Arc here.
    pub(crate) fn new_bootstrapping(
        entry: VaultEntry,
        lifecycle: Arc<Mutex<Option<RunnerLifecycle>>>,
        bootstrap: BootstrapLifecycle,
        bootstrap_done_tx: watch::Sender<bool>,
    ) -> Self {
        VaultRunner {
            entry: RwLock::new(Arc::new(entry)),
            op_lock: Mutex::new(()),
            bootstrap: Arc::new(Mutex::new(Some(bootstrap))),
            lifecycle,
            bootstrap_done_tx,
        }
    }

    pub(crate) fn test_only(entry: VaultEntry) -> Self {
        let (bootstrap_done_tx, _) = watch::channel(true);
        VaultRunner {
            entry: RwLock::new(Arc::new(entry)),
            op_lock: Mutex::new(()),
            bootstrap: Arc::new(Mutex::new(None)),
            lifecycle: Arc::new(Mutex::new(None)),
            bootstrap_done_tx,
        }
    }

    /// Subscribe a fresh receiver to the latched bootstrap-done signal. The
    /// channel value flips to `true` once the in-memory `BootstrapState`
    /// reaches `Ready` or `Errored`; receivers obtained after that point
    /// observe `true` immediately. Test-only.
    #[cfg(test)]
    pub(crate) fn bootstrap_done_rx(&self) -> watch::Receiver<bool> {
        self.bootstrap_done_tx.subscribe()
    }

    /// Snapshot the current entry. Search handlers call this once per
    /// request, so the read-window holds for an Arc-clone and is released.
    pub fn entry(&self) -> Arc<VaultEntry> {
        self.entry.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Replace the entry snapshot. Used by step-11 ops after an in-place
    /// status / name mutation to publish the new view to readers.
    pub(crate) fn replace_entry(&self, entry: Arc<VaultEntry>) {
        *self.entry.write().unwrap_or_else(|e| e.into_inner()) = entry;
    }

    /// Cooperative shutdown of this vault's bootstrap task (if any) and
    /// watcher + consumer (if installed). Bootstrap drains first because the
    /// bootstrap task may be the one that's about to install the lifecycle;
    /// awaiting it avoids a window where shutdown observes `lifecycle = None`
    /// and exits early while the bootstrap task then races to spawn a fresh
    /// watcher.
    pub(crate) async fn shutdown_with_timeout(&self, drain_timeout: Duration) {
        // Drain bootstrap first. Sending the per-vault shutdown signal here
        // also shuts down the consumer (they share the channel) when the
        // bootstrap task installs a lifecycle as part of its drain.
        let bootstrap = self.bootstrap.lock().await.take();
        if let Some(bs) = bootstrap {
            let _ = bs.shutdown_tx.send(true);
            let abort = bs.handle.abort_handle();
            if tokio::time::timeout(drain_timeout, bs.handle)
                .await
                .is_err()
            {
                warn!(
                    vault_id = %self.entry().id,
                    drain_ms = %drain_timeout.as_millis(),
                    "vault runner: bootstrap drain exceeded timeout; force-aborting"
                );
                abort.abort();
            }
        }

        let mut guard = self.lifecycle.lock().await;
        let Some(lc) = guard.take() else {
            return;
        };
        let _ = lc.shutdown_tx.send(true);
        let abort = lc.consumer_handle.abort_handle();
        let drained = tokio::time::timeout(drain_timeout, lc.consumer_handle).await;
        if drained.is_err() {
            warn!(
                vault_id = %self.entry().id,
                drain_ms = %drain_timeout.as_millis(),
                "vault runner: consumer drain exceeded timeout; force-aborting"
            );
            abort.abort();
        }
        drop(lc.watcher);
    }
}
