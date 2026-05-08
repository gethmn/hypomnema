//! Control-plane module: owns the `VaultManager` that drives per-vault
//! lifecycle (open at startup, create, terminate) and exposes the active
//! vault snapshot used by HTTP search handlers and (in step 11) per-vault
//! pause/resume/reset/rename/rescan operations.
//!
//! Concurrency posture per workplan § E + ADR-0010 § Concurrency:
//! - Outer `std::sync::RwLock<HashMap<VaultId, Arc<VaultRunner>>>` for the
//!   runners map. Sync read-side (`active_vaults()`) keeps the search-handler
//!   call-sites clean; the read window holds for microseconds (Arc-clone the
//!   filtered entries and release).
//! - Per-vault `tokio::sync::Mutex<()>` (`VaultRunner.op_lock`) reserved for
//!   step-11 ops (pause/resume/reset/rename/rescan) that mutate vault state
//!   without changing the runners-map membership. Step-10's `create` and
//!   `terminate` take the outer write-lock; the op_lock field is constructed
//!   and held but unused this step.
//! - Per-vault shutdown signal (`tokio::sync::watch::Sender<bool>`) inside
//!   each runner's lifecycle, so `terminate` can drain a single vault's
//!   consumer without disturbing the others.
//!
//! Crash safety:
//! - `create` between row-insert and subdir-create: next-startup reconcile
//!   recreates the missing subdir from the row (handled by `Store::open` in
//!   `spawn_runner_for_row`).
//! - `terminate` between row-delete and subdir-remove: orphan subdir is
//!   harmless until next-startup `reconcile_orphan_subdirs` removes it.

mod manager;
mod runner;

#[cfg(test)]
mod tests;

pub use manager::{
    ControlPlaneError, CreateVaultRequest, RescanResponse, VaultManager, VaultScopeRow,
};
pub use runner::VaultRunner;

#[cfg(test)]
pub(crate) use manager::wait_for_bootstrap;
