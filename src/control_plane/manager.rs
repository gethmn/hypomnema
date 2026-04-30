use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::watch;
use tokio::task;
use tracing::{info, warn};

use crate::api::VaultEntry;
use crate::config::Config;
use crate::embedding::Embedder;
use crate::events::EventBus;
use crate::indexer::Scanner;
use crate::legacy_state_migration;
use crate::store::Store;
use crate::vault_registry::{VaultId, VaultRegistry, VaultRow, VaultStatus, vault_data_dir};
use crate::watcher;

use super::runner::{RunnerLifecycle, VaultRunner};

/// One vault's view from the perspective of cross-vault search. Combines a
/// registry row's identity + lifecycle status + (when active) the live
/// runner's `VaultEntry`. Search handlers iterate these:
///
/// - `entry: Some(_)` → run the per-vault search.
/// - `entry: None` with `status: Paused | Errored` → skip and add a
///   `partial_results.skipped` diagnostic to the response envelope.
/// - `entry: None` with `status: Active` → registry says active but no live
///   runner; treat as a `partial_results.failed` (`vault_search_failed`)
///   case. Should not happen in step-10's static manager but is defended
///   against because step 11's pause/resume mutates the runner set.
pub struct VaultScopeRow {
    pub id: VaultId,
    pub name: String,
    pub status: VaultStatus,
    pub last_error: Option<String>,
    pub entry: Option<Arc<VaultEntry>>,
}

/// Per-vault drain timeout used by every lifecycle op (terminate, pause,
/// reset). Bounds the consumer-handle await before the runner force-aborts.
const LIFECYCLE_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);
const WATCHER_BUFFER: usize = 256;
const NAME_HINT_MAX_DISTANCE: usize = 3;
const VAULT_NAME_PATTERN: &str = "^[A-Za-z0-9_-]+$";

/// Control-plane errors. Each variant maps 1:1 to the HTTP error-code table
/// in `docs/specs/vault-management.md` § Error Handling (workplan § D).
#[derive(Debug)]
pub enum ControlPlaneError {
    VaultNotFound {
        name_or_id: String,
        hint: Option<String>,
    },
    VaultPathConflict {
        existing_name: String,
        path: PathBuf,
    },
    VaultNameConflict {
        existing_path: PathBuf,
        name: String,
    },
    VaultPathInvalid {
        detail: String,
    },
    /// Reserved for step 11: an op requires `Active` status but the vault is
    /// `Errored`. Not emitted by step-10's create/list/get/terminate.
    VaultErrored {
        name_or_id: String,
        last_error: Option<String>,
    },
    /// Reserved for step-11 non-blocking conflicts (e.g. terminate-while-
    /// create-in-flight where waiting would deadlock). Step-10 emits the
    /// outer-write-lock-serialised path instead.
    VaultOpConflict {
        detail: String,
    },
    RegistryCorrupt {
        detail: String,
    },
    Internal(anyhow::Error),
}

impl std::fmt::Display for ControlPlaneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VaultNotFound { name_or_id, hint } => match hint {
                Some(h) => write!(f, "vault_not_found: {name_or_id} (did you mean {h}?)"),
                None => write!(f, "vault_not_found: {name_or_id}"),
            },
            Self::VaultPathConflict {
                existing_name,
                path,
            } => write!(
                f,
                "vault_path_conflict: path {} is already registered as vault {}",
                path.display(),
                existing_name
            ),
            Self::VaultNameConflict {
                existing_path,
                name,
            } => write!(
                f,
                "vault_name_conflict: name {} is already in use by vault at {}",
                name,
                existing_path.display()
            ),
            Self::VaultPathInvalid { detail } => write!(f, "vault_path_invalid: {detail}"),
            Self::VaultErrored {
                name_or_id,
                last_error,
            } => match last_error {
                Some(e) => write!(f, "vault_errored: {name_or_id}: {e}"),
                None => write!(f, "vault_errored: {name_or_id}"),
            },
            Self::VaultOpConflict { detail } => write!(f, "vault_op_conflict: {detail}"),
            Self::RegistryCorrupt { detail } => write!(f, "registry_corrupt: {detail}"),
            Self::Internal(e) => write!(f, "internal: {e:#}"),
        }
    }
}

impl std::error::Error for ControlPlaneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Internal(e) => e.source(),
            _ => None,
        }
    }
}

impl From<anyhow::Error> for ControlPlaneError {
    fn from(e: anyhow::Error) -> Self {
        ControlPlaneError::Internal(e)
    }
}

/// Response payload for `VaultManager::rescan`. The rescan itself is
/// asynchronous; this payload carries the timestamp at which the daemon
/// accepted the request, plus the vault row as currently persisted in the
/// registry.
#[derive(Debug, Clone)]
pub struct RescanResponse {
    pub row: VaultRow,
    pub rescan_initiated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateVaultRequest {
    /// `None` resolves to `config.default_vault_name`; if that is empty
    /// (Resolution C exception), `create` returns `VaultPathInvalid`.
    pub name: Option<String>,
    /// Must be absolute (or `~`-expandable) and `canonicalize`-able. Rejected
    /// if `data_dir` is under the canonicalized vault path.
    pub path: PathBuf,
}

pub struct VaultManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    runners: RwLock<HashMap<VaultId, Arc<VaultRunner>>>,
    embedder: Arc<dyn Embedder>,
    event_bus: Arc<EventBus>,
    embedding_dimension: u32,
    /// Production spawn context. `None` for `for_tests`-constructed managers,
    /// in which case `create`/`terminate` return `Internal`.
    spawn: Option<SpawnCtx>,
    /// Test-only: paused/errored row stubs for cross-vault search fixtures.
    /// Empty in production; production gets paused/errored rows from the
    /// registry's `list()` instead.
    test_inactive_rows: Vec<VaultRow>,
}

struct SpawnCtx {
    registry: Arc<VaultRegistry>,
    config: Arc<Config>,
    data_dir: PathBuf,
    shutdown_rx: watch::Receiver<bool>,
}

impl VaultManager {
    /// Production constructor. Reconciles registry rows against the
    /// filesystem, removes orphan per-vault subdirs left by a crashed
    /// terminate, and spawns a `VaultRunner` for each active row.
    pub async fn open(
        registry: Arc<VaultRegistry>,
        config: Arc<Config>,
        embedder: Arc<dyn Embedder>,
        embedding_dimension: u32,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Result<Self> {
        let data_dir = config.storage.data_dir.0.clone();
        let event_bus = Arc::new(EventBus::new());

        let active_rows = reconcile_active_rows(&registry, &data_dir).await?;

        let active_ids: Vec<VaultId> = active_rows.iter().map(|r| r.id.clone()).collect();
        if let Err(e) = task::spawn_blocking({
            let data_dir = data_dir.clone();
            move || reconcile_orphan_subdirs(&data_dir, &active_ids)
        })
        .await
        {
            warn!(error = %e, "control_plane: orphan-subdir reconcile join error");
        }

        let mut runners: HashMap<VaultId, Arc<VaultRunner>> = HashMap::new();
        for row in &active_rows {
            let runner = spawn_runner_for_row(
                row,
                config.as_ref(),
                embedder.clone(),
                event_bus.clone(),
                shutdown_rx.clone(),
            )
            .await
            .with_context(|| format!("spawning runner for vault {}", row.id))?;
            runners.insert(row.id.clone(), Arc::new(runner));
        }

        info!(
            vault_count = runners.len(),
            "control_plane: VaultManager opened"
        );

        Ok(VaultManager {
            inner: Arc::new(ManagerInner {
                runners: RwLock::new(runners),
                embedder,
                event_bus,
                embedding_dimension,
                spawn: Some(SpawnCtx {
                    registry,
                    config,
                    data_dir,
                    shutdown_rx,
                }),
                test_inactive_rows: Vec::new(),
            }),
        })
    }

    /// Test-only constructor. Builds a manager populated with
    /// lifecycle-less runners around pre-built `VaultEntry` values; useful
    /// for HTTP-handler test fixtures that don't exercise create/terminate.
    /// Calls to `create` / `terminate` on a `for_tests` manager return
    /// `Internal`.
    pub fn for_tests(
        entries: Vec<VaultEntry>,
        embedder: Arc<dyn Embedder>,
        embedding_dimension: u32,
    ) -> Self {
        Self::for_tests_full(entries, Vec::new(), embedder, embedding_dimension)
    }

    /// Test-only constructor that additionally accepts paused/errored row
    /// stubs surfaced by `search_scope()` for cross-vault search fixtures.
    /// Each entry in `inactive_rows` is *not* given a runner — its `status`
    /// and `last_error` flow straight into the search handler's
    /// `partial_results.skipped` diagnostic.
    pub fn for_tests_full(
        active_entries: Vec<VaultEntry>,
        inactive_rows: Vec<VaultRow>,
        embedder: Arc<dyn Embedder>,
        embedding_dimension: u32,
    ) -> Self {
        let mut runners: HashMap<VaultId, Arc<VaultRunner>> = HashMap::new();
        for entry in active_entries {
            let id = entry.id.clone();
            runners.insert(id, Arc::new(VaultRunner::test_only(entry)));
        }
        VaultManager {
            inner: Arc::new(ManagerInner {
                runners: RwLock::new(runners),
                embedder,
                event_bus: Arc::new(EventBus::new()),
                embedding_dimension,
                spawn: None,
                test_inactive_rows: inactive_rows,
            }),
        }
    }

    pub fn embedder(&self) -> Arc<dyn Embedder> {
        self.inner.embedder.clone()
    }

    pub fn event_bus(&self) -> Arc<EventBus> {
        self.inner.event_bus.clone()
    }

    pub fn embedding_dimension(&self) -> u32 {
        self.inner.embedding_dimension
    }

    /// Snapshot of currently-active vault entries. Synchronous on purpose:
    /// search handlers call this once per request and the read-window
    /// (acquire, clone Arcs, release) holds for microseconds. With std's
    /// RwLock, contended readers wait briefly during a create/terminate's
    /// outer write-lock window.
    pub fn active_vaults(&self) -> Vec<Arc<VaultEntry>> {
        let guard = self
            .inner
            .runners
            .read()
            .expect("vault manager runners RwLock poisoned");
        guard
            .values()
            .filter_map(|runner| {
                let entry = runner.entry();
                if entry.is_active() { Some(entry) } else { None }
            })
            .collect()
    }

    /// Snapshot of every registered vault for cross-vault search. Active
    /// vaults carry their live `VaultEntry` (so handlers can run a per-vault
    /// search); paused/errored vaults carry status + last_error so handlers
    /// can build the `partial_results.skipped` diagnostic. Async because
    /// production reads the registry; for_tests synthesizes from runners +
    /// injected inactive rows.
    pub async fn search_scope(&self) -> Result<Vec<VaultScopeRow>, ControlPlaneError> {
        if let Some(spawn) = self.inner.spawn.as_ref() {
            let rows =
                spawn
                    .registry
                    .list()
                    .await
                    .map_err(|e| ControlPlaneError::RegistryCorrupt {
                        detail: format!("{e:#}"),
                    })?;
            let runners_guard = self
                .inner
                .runners
                .read()
                .expect("vault manager runners RwLock poisoned");
            let scope = rows
                .into_iter()
                .map(|row| {
                    let entry = if matches!(row.status, VaultStatus::Active) {
                        runners_guard.get(&row.id).map(|r| r.entry())
                    } else {
                        None
                    };
                    VaultScopeRow {
                        id: row.id,
                        name: row.name,
                        status: row.status,
                        last_error: row.last_error,
                        entry,
                    }
                })
                .collect();
            return Ok(scope);
        }
        let runners_guard = self
            .inner
            .runners
            .read()
            .expect("vault manager runners RwLock poisoned");
        let mut scope: Vec<VaultScopeRow> = runners_guard
            .values()
            .map(|runner| {
                let entry = runner.entry();
                let active = entry.is_active();
                VaultScopeRow {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    status: entry.status,
                    last_error: None,
                    entry: if active { Some(entry) } else { None },
                }
            })
            .collect();
        drop(runners_guard);
        for row in &self.inner.test_inactive_rows {
            scope.push(VaultScopeRow {
                id: row.id.clone(),
                name: row.name.clone(),
                status: row.status,
                last_error: row.last_error.clone(),
                entry: None,
            });
        }
        Ok(scope)
    }

    /// Resolve a name-or-id to a `VaultId`. Tries id-match first, then
    /// name-match. Returns `VaultNotFound` (with optional Levenshtein hint)
    /// when neither matches.
    pub fn resolve(&self, name_or_id: &str) -> Result<VaultId, ControlPlaneError> {
        let guard = self
            .inner
            .runners
            .read()
            .expect("vault manager runners RwLock poisoned");
        for (id, runner) in guard.iter() {
            if id.as_str() == name_or_id {
                return Ok(id.clone());
            }
            if runner.entry().name == name_or_id {
                return Ok(id.clone());
            }
        }
        let names: Vec<String> = guard.values().map(|r| r.entry().name.clone()).collect();
        drop(guard);
        Err(ControlPlaneError::VaultNotFound {
            name_or_id: name_or_id.to_string(),
            hint: closest_name(name_or_id, &names),
        })
    }

    pub async fn list(&self) -> Result<Vec<VaultRow>, ControlPlaneError> {
        let registry = self.spawn_ctx_or_err()?.registry.clone();
        registry
            .list()
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })
    }

    pub async fn get(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError> {
        let registry = self.spawn_ctx_or_err()?.registry.clone();
        if let Some(row) = registry
            .get_by_id(&VaultId::from_string(name_or_id.to_string()))
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })?
        {
            return Ok(row);
        }
        if let Some(row) = registry.get_by_name(name_or_id).await.map_err(|e| {
            ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            }
        })? {
            return Ok(row);
        }
        let names = self.list_names();
        Err(ControlPlaneError::VaultNotFound {
            name_or_id: name_or_id.to_string(),
            hint: closest_name(name_or_id, &names),
        })
    }

    pub async fn create(&self, req: CreateVaultRequest) -> Result<VaultRow, ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        let canonical_path = canonicalize_for_create(&req.path)?;
        let data_dir = spawn.data_dir.clone();
        if path_under(&data_dir, &canonical_path) {
            return Err(ControlPlaneError::VaultPathInvalid {
                detail: format!(
                    "data_dir {} is under vault path {}",
                    data_dir.display(),
                    canonical_path.display()
                ),
            });
        }

        let resolved_name = match req.name.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                let default = spawn.config.default_vault_name.trim();
                if default.is_empty() {
                    return Err(ControlPlaneError::VaultPathInvalid {
                        detail: "name is required when default_vault_name is empty".to_string(),
                    });
                }
                default.to_string()
            }
        };

        if let Some(existing) = spawn
            .registry
            .get_by_name(&resolved_name)
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })?
        {
            return Err(ControlPlaneError::VaultNameConflict {
                existing_path: existing.path,
                name: resolved_name,
            });
        }

        let existing_rows =
            spawn
                .registry
                .list()
                .await
                .map_err(|e| ControlPlaneError::RegistryCorrupt {
                    detail: format!("{e:#}"),
                })?;
        if let Some(existing) = existing_rows
            .iter()
            .find(|r| paths_equal(&r.path, &canonical_path))
        {
            return Err(ControlPlaneError::VaultPathConflict {
                existing_name: existing.name.clone(),
                path: canonical_path,
            });
        }

        let id = VaultId::new();
        let row = VaultRow {
            id: id.clone(),
            name: resolved_name,
            path: canonical_path,
            status: VaultStatus::Active,
            created_at: Utc::now(),
            last_error: None,
        };
        spawn
            .registry
            .insert(row.clone())
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("inserting vault row")))?;

        let vault_dir = vault_data_dir(&data_dir, &id);
        if let Err(e) = create_subdir_and_meta(&vault_dir, &row) {
            return Err(ControlPlaneError::Internal(e));
        }

        let runner = spawn_runner_for_row(
            &row,
            spawn.config.as_ref(),
            self.inner.embedder.clone(),
            self.inner.event_bus.clone(),
            spawn.shutdown_rx.clone(),
        )
        .await
        .map_err(|e| ControlPlaneError::Internal(e.context("spawning runner for new vault")))?;

        {
            let mut guard = self
                .inner
                .runners
                .write()
                .expect("vault manager runners RwLock poisoned");
            guard.insert(id.clone(), Arc::new(runner));
        }

        info!(
            vault_id = %row.id,
            vault_name = %row.name,
            vault_path = %row.path.display(),
            "control_plane: created vault"
        );

        Ok(row)
    }

    pub async fn terminate(&self, name_or_id: &str) -> Result<(), ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        let id = self.resolve(name_or_id)?;

        let runner = {
            let mut guard = self
                .inner
                .runners
                .write()
                .expect("vault manager runners RwLock poisoned");
            guard.remove(&id)
        };

        if let Some(runner) = &runner {
            runner.shutdown_with_timeout(LIFECYCLE_DRAIN_TIMEOUT).await;
        }

        let _deleted = spawn
            .registry
            .delete(&id)
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("deleting vault row")))?;

        let vault_dir = vault_data_dir(&spawn.data_dir, &id);
        if vault_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&vault_dir) {
                return Err(ControlPlaneError::Internal(anyhow::anyhow!(
                    "removing vault subdir {}: {e}",
                    vault_dir.display()
                )));
            }
        }

        info!(vault_id = %id, "control_plane: terminated vault");

        Ok(())
    }

    /// Pause an active vault: drain its watcher + consumer, mark the
    /// registry row paused, and publish a `Paused` snapshot to readers. The
    /// runner stays in the map (with `lifecycle = None`) so resume can swap
    /// a fresh lifecycle in without re-keying the map.
    pub async fn pause(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        let row = self.get(name_or_id).await?;
        let id = row.id.clone();

        let runner = self
            .runner_for(&id)
            .ok_or_else(|| ControlPlaneError::VaultNotFound {
                name_or_id: name_or_id.to_string(),
                hint: None,
            })?;
        let _op_guard = runner.op_lock.lock().await;

        let current = runner.entry();
        if matches!(current.status, VaultStatus::Paused) {
            return Ok(row);
        }

        runner.shutdown_with_timeout(LIFECYCLE_DRAIN_TIMEOUT).await;

        spawn
            .registry
            .update_status(&id, VaultStatus::Paused, None)
            .await
            .map_err(|e| {
                ControlPlaneError::Internal(e.context("updating registry status to paused"))
            })?;

        let new_entry = VaultEntry {
            status: VaultStatus::Paused,
            ..(*current).clone()
        };
        runner.replace_entry(Arc::new(new_entry));

        let updated = spawn
            .registry
            .get_by_id(&id)
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })?
            .ok_or_else(|| ControlPlaneError::VaultNotFound {
                name_or_id: name_or_id.to_string(),
                hint: None,
            })?;

        info!(vault_id = %id, vault_name = %updated.name, "control_plane: paused vault");
        Ok(updated)
    }

    /// Resume a paused or errored vault: validate path accessibility, spawn
    /// a fresh `RunnerLifecycle`, and clear `last_error`. Handles two
    /// shapes: the runner is already in the map (paused mid-run) — install
    /// a new lifecycle and update entry/registry; or the runner is absent
    /// (errored at startup, or paused before a daemon restart) — spawn a
    /// full new runner and insert it.
    pub async fn resume(&self, name_or_id: &str) -> Result<VaultRow, ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        let row = self.get(name_or_id).await?;
        let id = row.id.clone();

        if let Some(runner) = self.runner_for(&id) {
            let _op_guard = runner.op_lock.lock().await;

            let current = runner.entry();
            if matches!(current.status, VaultStatus::Active) {
                return Ok(row);
            }

            if matches!(current.status, VaultStatus::Errored) && !path_is_accessible(&row.path) {
                return Err(ControlPlaneError::VaultErrored {
                    name_or_id: name_or_id.to_string(),
                    last_error: row.last_error.clone(),
                });
            }

            let (entry, lifecycle) = spawn_runner_parts(
                &row,
                spawn.config.as_ref(),
                self.inner.embedder.clone(),
                self.inner.event_bus.clone(),
                spawn.shutdown_rx.clone(),
                false,
            )
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("spawning lifecycle for resume")))?;

            *runner.lifecycle.lock().await = Some(lifecycle);

            spawn
                .registry
                .update_status(&id, VaultStatus::Active, None)
                .await
                .map_err(|e| {
                    ControlPlaneError::Internal(e.context("updating registry status to active"))
                })?;

            let new_entry = VaultEntry {
                status: VaultStatus::Active,
                ..entry
            };
            runner.replace_entry(Arc::new(new_entry));
        } else {
            if matches!(row.status, VaultStatus::Active) {
                return Ok(row);
            }
            if matches!(row.status, VaultStatus::Errored) && !path_is_accessible(&row.path) {
                return Err(ControlPlaneError::VaultErrored {
                    name_or_id: name_or_id.to_string(),
                    last_error: row.last_error.clone(),
                });
            }

            spawn
                .registry
                .update_status(&id, VaultStatus::Active, None)
                .await
                .map_err(|e| {
                    ControlPlaneError::Internal(e.context("updating registry status to active"))
                })?;

            let updated = spawn
                .registry
                .get_by_id(&id)
                .await
                .map_err(|e| ControlPlaneError::RegistryCorrupt {
                    detail: format!("{e:#}"),
                })?
                .ok_or_else(|| ControlPlaneError::VaultNotFound {
                    name_or_id: name_or_id.to_string(),
                    hint: None,
                })?;

            let runner = spawn_runner_for_row(
                &updated,
                spawn.config.as_ref(),
                self.inner.embedder.clone(),
                self.inner.event_bus.clone(),
                spawn.shutdown_rx.clone(),
            )
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("spawning runner for resume")))?;

            let mut guard = self
                .inner
                .runners
                .write()
                .expect("vault manager runners RwLock poisoned");
            guard.insert(id.clone(), Arc::new(runner));
        }

        let updated = spawn
            .registry
            .get_by_id(&id)
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })?
            .ok_or_else(|| ControlPlaneError::VaultNotFound {
                name_or_id: name_or_id.to_string(),
                hint: None,
            })?;
        info!(vault_id = %id, vault_name = %updated.name, "control_plane: resumed vault");
        Ok(updated)
    }

    /// Rename a vault: validate the new name, pre-check uniqueness, update
    /// the registry and the per-vault meta.toml, and replace the runner's
    /// entry with the new name. Watcher / indexer don't read `name`, so the
    /// lifecycle is unchanged. If the runner is not in the map (paused or
    /// errored vault), only the registry + meta.toml are updated.
    pub async fn rename(
        &self,
        name_or_id: &str,
        new_name: &str,
    ) -> Result<VaultRow, ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        if !is_valid_vault_name(new_name) {
            return Err(ControlPlaneError::VaultPathInvalid {
                detail: format!(
                    "new_name {new_name:?} must match {VAULT_NAME_PATTERN} (ASCII letters, digits, '_', '-')",
                ),
            });
        }

        let row = self.get(name_or_id).await?;
        let id = row.id.clone();

        if row.name == new_name {
            return Ok(row);
        }

        let runner = self.runner_for(&id);
        let _op_guard = if let Some(r) = runner.as_ref() {
            Some(r.op_lock.lock().await)
        } else {
            None
        };

        if let Some(existing) = spawn.registry.get_by_name(new_name).await.map_err(|e| {
            ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            }
        })? {
            if existing.id != id {
                return Err(ControlPlaneError::VaultNameConflict {
                    existing_path: existing.path,
                    name: new_name.to_string(),
                });
            }
        }

        spawn
            .registry
            .update_name(&id, new_name)
            .await
            .map_err(|e| {
                ControlPlaneError::Internal(e.context("updating vault name in registry"))
            })?;

        let updated = spawn
            .registry
            .get_by_id(&id)
            .await
            .map_err(|e| ControlPlaneError::RegistryCorrupt {
                detail: format!("{e:#}"),
            })?
            .ok_or_else(|| ControlPlaneError::VaultNotFound {
                name_or_id: name_or_id.to_string(),
                hint: None,
            })?;

        let vault_dir = vault_data_dir(&spawn.data_dir, &id);
        legacy_state_migration::write_meta_toml(&vault_dir, &updated).map_err(|e| {
            ControlPlaneError::Internal(e.context("rewriting meta.toml after rename"))
        })?;

        if let Some(r) = runner.as_ref() {
            let current = r.entry();
            let new_entry = VaultEntry {
                name: new_name.to_string(),
                ..(*current).clone()
            };
            r.replace_entry(Arc::new(new_entry));
        }

        info!(vault_id = %id, new_name, "control_plane: renamed vault");
        Ok(updated)
    }

    /// Reset a vault: drain its lifecycle, optionally rebuild its index
    /// tables, clear `last_error`, and bring it back up Active.
    ///
    /// Without `--rebuild` this is the cheap "kick the vault" path: the
    /// content_hash gate keeps untouched files from re-embedding when the
    /// fresh lifecycle's initial scan reads them. With `--rebuild` the
    /// per-vault `chunks_vec` / `chunks` are emptied and `files.content_hash`
    /// is zeroed; the spawn skips the initial scan so `content_hash` stays
    /// empty and a follow-up `rescan` re-emits `modified` events for every
    /// file (the operator workflow for "force re-embed everything").
    ///
    /// Two paths mirror `resume`'s shape:
    /// - **Runner in map** (Active or Paused with the runner still installed):
    ///   take `op_lock`, drain via `shutdown_with_timeout`, optionally run
    ///   the rebuild SQL on the runner's existing `Arc<Store>`, spawn a
    ///   fresh `RunnerLifecycle`, and install it.
    /// - **Runner not in map** (Errored at startup-reconcile): validate path
    ///   accessibility, optionally open the store fresh and run the rebuild
    ///   SQL, then spawn a full new runner under the outer write-lock.
    pub async fn reset(
        &self,
        name_or_id: &str,
        rebuild: bool,
    ) -> Result<VaultRow, ControlPlaneError> {
        let spawn = self.spawn_ctx_or_err()?;

        let row = self.get(name_or_id).await?;
        let id = row.id.clone();

        if let Some(runner) = self.runner_for(&id) {
            let _op_guard = runner.op_lock.lock().await;

            // Soft-deletion ordering (workplan § Task 11.2): drain the
            // lifecycle first, then run rebuild SQL on the now-quiet store,
            // then spawn a fresh lifecycle. Running the SQL while the old
            // consumer was still draining could let it observe partially-
            // deleted chunks_vec rows mid-transaction; running it after the
            // new consumer was spawned would race the consumer's reads.
            runner.shutdown_with_timeout(LIFECYCLE_DRAIN_TIMEOUT).await;

            if rebuild {
                let store = runner.entry().store.clone();
                run_rebuild_sql(store)
                    .await
                    .map_err(|e| ControlPlaneError::Internal(e.context("running rebuild SQL")))?;
            }

            spawn
                .registry
                .update_status(&id, VaultStatus::Active, None)
                .await
                .map_err(|e| {
                    ControlPlaneError::Internal(e.context("updating registry status to active"))
                })?;

            let updated = spawn
                .registry
                .get_by_id(&id)
                .await
                .map_err(|e| ControlPlaneError::RegistryCorrupt {
                    detail: format!("{e:#}"),
                })?
                .ok_or_else(|| ControlPlaneError::VaultNotFound {
                    name_or_id: name_or_id.to_string(),
                    hint: None,
                })?;

            // skip_initial_scan = rebuild: the rebuild SQL just zeroed
            // content_hash; running the initial scan would re-populate it
            // and defeat the "rebuild then rescan" cold-start workflow.
            let (entry, lifecycle) = spawn_runner_parts(
                &updated,
                spawn.config.as_ref(),
                self.inner.embedder.clone(),
                self.inner.event_bus.clone(),
                spawn.shutdown_rx.clone(),
                rebuild,
            )
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("spawning lifecycle for reset")))?;

            *runner.lifecycle.lock().await = Some(lifecycle);
            runner.replace_entry(Arc::new(entry));

            info!(
                vault_id = %id,
                vault_name = %updated.name,
                rebuild,
                "control_plane: reset vault"
            );
            Ok(updated)
        } else {
            // Errored row at startup-reconcile: no runner in the map. Treat
            // it the same as the runner-in-map path modulo "no lifecycle to
            // drain"; the rebuild SQL still needs the store, opened fresh.
            if !path_is_accessible(&row.path) {
                return Err(ControlPlaneError::VaultErrored {
                    name_or_id: name_or_id.to_string(),
                    last_error: row.last_error.clone(),
                });
            }

            if rebuild {
                let store = Store::open(
                    &id,
                    &spawn.config.storage.data_dir.0,
                    &spawn.config.storage.index_file,
                    &spawn.config.embedding,
                )
                .await
                .map_err(|e| {
                    ControlPlaneError::Internal(
                        e.context("opening per-vault store for rebuild on errored vault"),
                    )
                })?;
                let store = Arc::new(store);
                run_rebuild_sql(store)
                    .await
                    .map_err(|e| ControlPlaneError::Internal(e.context("running rebuild SQL")))?;
            }

            spawn
                .registry
                .update_status(&id, VaultStatus::Active, None)
                .await
                .map_err(|e| {
                    ControlPlaneError::Internal(e.context("updating registry status to active"))
                })?;

            let updated = spawn
                .registry
                .get_by_id(&id)
                .await
                .map_err(|e| ControlPlaneError::RegistryCorrupt {
                    detail: format!("{e:#}"),
                })?
                .ok_or_else(|| ControlPlaneError::VaultNotFound {
                    name_or_id: name_or_id.to_string(),
                    hint: None,
                })?;

            let (entry, lifecycle) = spawn_runner_parts(
                &updated,
                spawn.config.as_ref(),
                self.inner.embedder.clone(),
                self.inner.event_bus.clone(),
                spawn.shutdown_rx.clone(),
                rebuild,
            )
            .await
            .map_err(|e| ControlPlaneError::Internal(e.context("spawning runner for reset")))?;

            let runner = VaultRunner::new(entry, lifecycle);
            {
                let mut guard = self
                    .inner
                    .runners
                    .write()
                    .expect("vault manager runners RwLock poisoned");
                guard.insert(id.clone(), Arc::new(runner));
            }

            info!(
                vault_id = %id,
                vault_name = %updated.name,
                rebuild,
                "control_plane: reset vault"
            );
            Ok(updated)
        }
    }

    /// Trigger a fresh scanner walk on an active vault. Asynchronous: the
    /// response returns immediately with `rescan_initiated_at`; the
    /// consumer task picks up the rescan signal and walks/emits in the
    /// background.
    ///
    /// **Cold-start emission policy**: `rescan` walks the vault and drives
    /// each file through the same `apply_event(Upsert)` pipeline the live
    /// watcher uses. Files whose `content_hash` matches on disk are silent
    /// (the indexer's hash-comparison short-circuit). On an up-to-date
    /// vault this means few change events. Operators wanting "every file
    /// re-emits" should pair `rescan` with `reset --rebuild` (which clears
    /// `content_hash` and forces re-emit).
    ///
    /// Rescan on a paused or errored vault (no live consumer to signal) is
    /// a no-op that still returns a successful response with the timestamp
    /// — the operator's expectation is "I asked, the daemon acknowledged."
    /// The `lifecycle.is_some()` gate decides whether the signal is sent.
    pub async fn rescan(&self, name_or_id: &str) -> Result<RescanResponse, ControlPlaneError> {
        let _spawn = self.spawn_ctx_or_err()?;

        let row = self.get(name_or_id).await?;
        let id = row.id.clone();

        let runner = self.runner_for(&id);
        let _op_guard = if let Some(r) = runner.as_ref() {
            Some(r.op_lock.lock().await)
        } else {
            None
        };

        if let Some(r) = runner.as_ref() {
            let lifecycle_guard = r.lifecycle.lock().await;
            if let Some(lc) = lifecycle_guard.as_ref() {
                lc.rescan_tx.send_modify(|v| *v = v.wrapping_add(1));
            }
        }

        let rescan_initiated_at = Utc::now();
        info!(vault_id = %id, vault_name = %row.name, "control_plane: rescan initiated");
        Ok(RescanResponse {
            row,
            rescan_initiated_at,
        })
    }

    fn runner_for(&self, id: &VaultId) -> Option<Arc<VaultRunner>> {
        let guard = self
            .inner
            .runners
            .read()
            .expect("vault manager runners RwLock poisoned");
        guard.get(id).cloned()
    }

    fn spawn_ctx_or_err(&self) -> Result<&SpawnCtx, ControlPlaneError> {
        self.inner.spawn.as_ref().ok_or_else(|| {
            ControlPlaneError::Internal(anyhow::anyhow!(
                "VaultManager constructed via for_tests; create/list/get/terminate unavailable"
            ))
        })
    }

    fn list_names(&self) -> Vec<String> {
        let guard = self
            .inner
            .runners
            .read()
            .expect("vault manager runners RwLock poisoned");
        guard.values().map(|r| r.entry().name.clone()).collect()
    }
}

/// Validate registry rows against the filesystem and return the active
/// subset. Mirrors the step-9 `reconcile()` previously in `src/bin/hmnd.rs`,
/// folded into the manager so a single startup path owns all of it.
async fn reconcile_active_rows(registry: &VaultRegistry, data_dir: &Path) -> Result<Vec<VaultRow>> {
    let rows = registry.list().await.context("listing registry rows")?;
    let mut active: Vec<VaultRow> = Vec::new();
    for row in rows {
        match row.status {
            VaultStatus::Paused => {
                info!(vault_id = %row.id, vault_name = %row.name, "reconcile: vault paused; skipping");
                continue;
            }
            VaultStatus::Errored => {
                warn!(
                    vault_id = %row.id,
                    vault_name = %row.name,
                    last_error = %row.last_error.as_deref().unwrap_or(""),
                    "reconcile: vault errored; skipping"
                );
                continue;
            }
            VaultStatus::Active => {}
        }

        match std::fs::metadata(&row.path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => {
                let err = format!("vault path {} is not a directory", row.path.display());
                warn!(vault_id = %row.id, vault = %row.path.display(), "reconcile: marking errored: not a directory");
                registry
                    .update_status(&row.id, VaultStatus::Errored, Some(&err))
                    .await
                    .with_context(|| format!("updating status to errored for {}", row.id))?;
                continue;
            }
            Err(e) => {
                let err = format!("vault path {} not accessible: {}", row.path.display(), e);
                warn!(vault_id = %row.id, vault = %row.path.display(), error = %e, "reconcile: marking errored: not accessible");
                registry
                    .update_status(&row.id, VaultStatus::Errored, Some(&err))
                    .await
                    .with_context(|| format!("updating status to errored for {}", row.id))?;
                continue;
            }
        }

        let target = vault_data_dir(data_dir, &row.id);
        std::fs::create_dir_all(&target).with_context(|| {
            format!(
                "creating per-vault directory {} during reconcile",
                target.display()
            )
        })?;

        active.push(row);
    }
    Ok(active)
}

/// Remove `<data_dir>/vaults/<x>` subdirs whose `<x>` is not in
/// `active_ids`. Best-effort: log and continue on individual failures.
/// Recovers from a `terminate` that crashed between row-delete and
/// subdir-remove.
fn reconcile_orphan_subdirs(data_dir: &Path, active_ids: &[VaultId]) {
    let vaults_root = data_dir.join("vaults");
    let entries = match std::fs::read_dir(&vaults_root) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => {
            warn!(
                vaults_root = %vaults_root.display(),
                error = %e,
                "reconcile_orphan_subdirs: cannot read vaults root"
            );
            return;
        }
    };
    let known: std::collections::HashSet<&str> = active_ids.iter().map(|id| id.as_str()).collect();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if known.contains(name) {
            continue;
        }
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    warn!(
                        orphan = %path.display(),
                        error = %e,
                        "reconcile_orphan_subdirs: failed to remove orphan vault subdir"
                    );
                } else {
                    info!(
                        orphan = %path.display(),
                        "reconcile_orphan_subdirs: removed orphan vault subdir"
                    );
                }
            }
            _ => {}
        }
    }
}

async fn spawn_runner_for_row(
    row: &VaultRow,
    config: &Config,
    embedder: Arc<dyn Embedder>,
    event_bus: Arc<EventBus>,
    parent_shutdown_rx: watch::Receiver<bool>,
) -> Result<VaultRunner> {
    let (entry, lifecycle) =
        spawn_runner_parts(row, config, embedder, event_bus, parent_shutdown_rx, false).await?;
    Ok(VaultRunner::new(entry, lifecycle))
}

/// Open the per-vault store, run the initial scan, and spawn the watcher +
/// consumer. Returns the entry/lifecycle pair separately so step-11 ops
/// (resume / reset) can install a fresh lifecycle into an existing
/// `Arc<VaultRunner>` without minting a new runner.
///
/// `skip_initial_scan` is true on the `reset --rebuild` path: rebuild has
/// just zeroed `files.content_hash` in the per-vault store, and the operator
/// is expected to follow up with `rescan` to re-emit `modified` events for
/// every file. Running the initial scan here would silently re-populate
/// `content_hash` (matching the on-disk hash again), defeating the rescan's
/// re-emit and contradicting the `reset_with_rebuild_clears_*` test
/// assertion that `content_hash = ''` after reset --rebuild. All other call
/// sites (open/create/resume) keep the initial scan.
async fn spawn_runner_parts(
    row: &VaultRow,
    config: &Config,
    embedder: Arc<dyn Embedder>,
    event_bus: Arc<EventBus>,
    parent_shutdown_rx: watch::Receiver<bool>,
    skip_initial_scan: bool,
) -> Result<(VaultEntry, RunnerLifecycle)> {
    let store = Store::open(
        &row.id,
        &config.storage.data_dir.0,
        &config.storage.index_file,
        &config.embedding,
    )
    .await
    .with_context(|| format!("opening store for {}", row.id))?;
    let store = Arc::new(store);

    if !skip_initial_scan {
        let scanner = Scanner::new(&row.path, config, &store, embedder.clone())
            .with_context(|| format!("constructing scanner for {}", row.id))?;
        let report = scanner
            .run()
            .await
            .with_context(|| format!("running initial scan for {}", row.id))?;
        info!(
            vault_id = %row.id,
            vault_name = %row.name,
            "control_plane: scan complete: inserted={} updated={} hash_unchanged={} deleted={} in {:.2}s",
            report.inserted,
            report.updated,
            report.hash_unchanged,
            report.deleted,
            report.duration.as_secs_f64()
        );
    }

    let ignores = config
        .watcher
        .compiled_ignores()
        .context("compiling watcher.ignore_patterns")?;
    let (watcher_handle, rx) = watcher::spawn_watcher(
        &row.id,
        &row.path,
        ignores,
        Duration::from_millis(config.watcher.debounce_ms),
        WATCHER_BUFFER,
    )
    .with_context(|| format!("spawning watcher for {}", row.id))?;

    let scanner_for_consumer = Scanner::new(&row.path, config, &store, embedder)
        .with_context(|| format!("constructing scanner (consumer) for {}", row.id))?;

    // Per-vault shutdown signal. The consumer exits when *either* this
    // per-vault sender fires OR the parent (daemon-wide) sender fires —
    // achieved by spawning a small joiner task that mirrors the parent into
    // the per-vault channel.
    let (per_vault_tx, per_vault_rx) = watch::channel(false);
    let mut parent_rx_for_join = parent_shutdown_rx.clone();
    let per_vault_tx_for_join = per_vault_tx.clone();
    tokio::spawn(async move {
        let _ = parent_rx_for_join.wait_for(|v| *v).await;
        let _ = per_vault_tx_for_join.send(true);
    });

    // Per-vault rescan signal. Manager.rescan() bumps this counter; the
    // consumer's `select!` covers both shutdown and rescan.
    let (rescan_tx, rescan_rx) = watch::channel(0u64);

    let consumer_handle = tokio::spawn(watcher::run_consumer(
        rx,
        scanner_for_consumer,
        row.id.clone(),
        event_bus,
        per_vault_rx,
        rescan_rx,
    ));

    let entry = VaultEntry {
        id: row.id.clone(),
        name: row.name.clone(),
        vault_path: row.path.clone(),
        store,
        status: row.status,
    };

    Ok((
        entry,
        RunnerLifecycle {
            shutdown_tx: per_vault_tx,
            rescan_tx,
            consumer_handle,
            watcher: watcher_handle,
        },
    ))
}

/// Run the `--rebuild` SQL on a per-vault store: drop every chunk row,
/// drop every chunks_vec row, and zero `files.content_hash` so the next
/// rescan re-embeds every file. Mirrors migration 0004's chunks-only
/// pattern (per `src/store/schema.rs`'s "needs re-embedding" sentinel).
/// Outbox is preserved per spec — the durable event log is independent of
/// the per-vault index tables.
async fn run_rebuild_sql(store: Arc<Store>) -> Result<()> {
    task::spawn_blocking(move || -> Result<()> {
        let mut conn = store
            .pool()
            .get()
            .context("acquiring connection from pool for rebuild")?;
        let tx = conn
            .transaction()
            .context("beginning rebuild transaction")?;
        tx.execute("DELETE FROM chunks_vec", [])
            .context("deleting chunks_vec rows")?;
        tx.execute("DELETE FROM chunks", [])
            .context("deleting chunks rows")?;
        tx.execute("UPDATE files SET content_hash = ''", [])
            .context("zeroing files.content_hash")?;
        tx.commit().context("committing rebuild transaction")?;
        Ok(())
    })
    .await
    .context("spawn_blocking join error in run_rebuild_sql")?
}

fn create_subdir_and_meta(vault_dir: &Path, row: &VaultRow) -> Result<()> {
    std::fs::create_dir_all(vault_dir)
        .with_context(|| format!("creating per-vault directory {}", vault_dir.display()))?;
    legacy_state_migration::write_meta_toml(vault_dir, row)
        .with_context(|| format!("writing meta.toml under {}", vault_dir.display()))?;
    Ok(())
}

fn canonicalize_for_create(path: &Path) -> Result<PathBuf, ControlPlaneError> {
    let expanded = expand_tilde(path);
    if !expanded.is_absolute() {
        return Err(ControlPlaneError::VaultPathInvalid {
            detail: format!("vault path {} must be absolute", expanded.display()),
        });
    }
    std::fs::canonicalize(&expanded).map_err(|e| ControlPlaneError::VaultPathInvalid {
        detail: format!("cannot canonicalize {}: {e}", expanded.display()),
    })
}

fn expand_tilde(path: &Path) -> PathBuf {
    if let Ok(s) = path.strip_prefix("~") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(s);
        }
    }
    path.to_path_buf()
}

fn path_under(child: &Path, ancestor: &Path) -> bool {
    let child = std::fs::canonicalize(child).unwrap_or_else(|_| child.to_path_buf());
    child.starts_with(ancestor)
}

fn is_valid_vault_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn path_is_accessible(path: &Path) -> bool {
    matches!(std::fs::metadata(path), Ok(m) if m.is_dir())
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a == b
}

/// Tiny Levenshtein for the closest-name hint. O(N·M) per pair; the name
/// list is small (single-digit vaults in practice) so this is fine.
fn closest_name(target: &str, candidates: &[String]) -> Option<String> {
    let mut best: Option<(usize, &str)> = None;
    for c in candidates {
        let d = levenshtein(target, c);
        if d <= NAME_HINT_MAX_DISTANCE {
            best = match best {
                Some((bd, _)) if bd <= d => best,
                _ => Some((d, c.as_str())),
            };
        }
    }
    best.map(|(_, n)| n.to_string())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}
