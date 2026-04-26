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
//! ## Known v0 limitation: symlinks
//!
//! `notify` does not follow symlinks by default. The step-2 walker does
//! (`WalkDir::follow_links(true)`), so a file reachable only via a
//! symlink inside the vault will be picked up by `hmnd scan` and on the
//! daemon's startup re-scan, but live edits to it will not produce a
//! watcher event until the next restart. Documented as a v0 trade-off
//! rather than worked around — the startup re-scan is the safety net.

pub mod filter;
mod translate;

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use globset::GlobSet;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use notify_debouncer_full::{Debouncer, FileIdMap, new_debouncer};
use tokio::sync::mpsc;

use translate::{TranslateCtx, translate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    Upsert(String),
    Remove(String),
}

pub struct Watcher {
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
}

pub fn spawn_watcher(
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

    let mut debouncer = new_debouncer(debounce, None, move |result| match result {
        Ok(events) => {
            for ev in translate(events, &ctx) {
                if tx.blocking_send(ev).is_err() {
                    // Receiver dropped — daemon is shutting down.
                    break;
                }
            }
        }
        Err(errs) => {
            for e in errs {
                tracing::warn!(error = ?e, "watcher: notify error");
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

    Ok((
        Watcher {
            _debouncer: debouncer,
        },
        rx,
    ))
}
