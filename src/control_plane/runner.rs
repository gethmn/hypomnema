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
    /// Lifecycle handles. `None` when the runner was constructed via
    /// `VaultRunner::test_only` (test fixture path with no live watcher /
    /// consumer to drain), or after `shutdown_with_timeout` has drained.
    pub(crate) lifecycle: Mutex<Option<RunnerLifecycle>>,
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
    pub(crate) fn new(entry: VaultEntry, lifecycle: RunnerLifecycle) -> Self {
        VaultRunner {
            entry: RwLock::new(Arc::new(entry)),
            op_lock: Mutex::new(()),
            lifecycle: Mutex::new(Some(lifecycle)),
        }
    }

    pub(crate) fn test_only(entry: VaultEntry) -> Self {
        VaultRunner {
            entry: RwLock::new(Arc::new(entry)),
            op_lock: Mutex::new(()),
            lifecycle: Mutex::new(None),
        }
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

    /// Cooperative shutdown of this vault's watcher + consumer. Sends the
    /// per-vault shutdown signal, awaits the consumer's drain up to
    /// `drain_timeout`, and force-aborts the consumer if the drain window
    /// expires. Drops the watcher last so any debouncer-buffered events have
    /// a chance to land in the consumer's mpsc.
    pub(crate) async fn shutdown_with_timeout(&self, drain_timeout: Duration) {
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
