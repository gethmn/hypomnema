use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tracing::warn;

use crate::api::VaultEntry;
use crate::watcher::Watcher;

pub struct VaultRunner {
    entry: Arc<VaultEntry>,
    /// Per-vault operation lock. Reserved for step-11 ops
    /// (pause/resume/reset/rename/rescan) that mutate vault state without
    /// changing the runners-map membership. Step-10's create/terminate take
    /// the outer RwLock instead, so this field is constructed but not yet
    /// acquired.
    #[allow(dead_code)]
    pub(crate) op_lock: Mutex<()>,
    /// Lifecycle handles. `None` when the runner was constructed via
    /// `VaultRunner::test_only` (test fixture path with no live watcher /
    /// consumer to drain).
    lifecycle: Mutex<Option<RunnerLifecycle>>,
}

pub(crate) struct RunnerLifecycle {
    pub shutdown_tx: watch::Sender<bool>,
    pub consumer_handle: JoinHandle<()>,
    pub watcher: Watcher,
}

impl VaultRunner {
    pub(crate) fn new(entry: VaultEntry, lifecycle: RunnerLifecycle) -> Self {
        VaultRunner {
            entry: Arc::new(entry),
            op_lock: Mutex::new(()),
            lifecycle: Mutex::new(Some(lifecycle)),
        }
    }

    pub(crate) fn test_only(entry: VaultEntry) -> Self {
        VaultRunner {
            entry: Arc::new(entry),
            op_lock: Mutex::new(()),
            lifecycle: Mutex::new(None),
        }
    }

    pub fn entry(&self) -> &Arc<VaultEntry> {
        &self.entry
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
                vault_id = %self.entry.id,
                drain_ms = %drain_timeout.as_millis(),
                "vault runner: consumer drain exceeded timeout; force-aborting"
            );
            abort.abort();
        }
        drop(lc.watcher);
    }
}
