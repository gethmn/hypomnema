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

use crate::indexer::{ReindexOutcome, Scanner};
use translate::{TranslateCtx, translate};

const BACKPRESSURE_WARN_EVERY: usize = 64;
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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

    // Backpressure counter: incremented every time a `try_send` would have
    // blocked the notify thread. Per `.claude/skills/filesystem-watching`,
    // backpressure is a signal to surface, not an error to swallow — we log
    // a warn line every `BACKPRESSURE_WARN_EVERY` blocked sends and let the
    // notify thread block on `blocking_send` (the desired shape).
    let blocked = Arc::new(AtomicUsize::new(0));
    let mut debouncer = new_debouncer(debounce, None, move |result| match result {
        Ok(events) => {
            for ev in translate(events, &ctx) {
                match tx.try_send(ev) {
                    Ok(()) => {}
                    Err(TrySendError::Full(ev)) => {
                        let n = blocked.fetch_add(1, Ordering::Relaxed) + 1;
                        if n % BACKPRESSURE_WARN_EVERY == 0 {
                            tracing::warn!(
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
    mut shutdown_rx: watch::Receiver<bool>,
) {
    // Honour the initial value too — if shutdown already fired before we
    // started polling, drain and exit without waiting for another change.
    if *shutdown_rx.borrow() {
        drain_remaining(&mut rx, &scanner).await;
        return;
    }
    loop {
        tokio::select! {
            biased;
            res = shutdown_rx.changed() => {
                // Either a new value or the sender was dropped; in both
                // cases the right move is to drain and exit.
                if res.is_err() || *shutdown_rx.borrow() {
                    drain_remaining(&mut rx, &scanner).await;
                    break;
                }
            }
            event = rx.recv() => {
                match event {
                    Some(ev) => apply_event(ev, &scanner).await,
                    None => break,
                }
            }
        }
    }
}

async fn drain_remaining(rx: &mut mpsc::Receiver<WatchEvent>, scanner: &Scanner) {
    let deadline = Instant::now() + DRAIN_TIMEOUT;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match timeout(remaining, rx.recv()).await {
            Ok(Some(ev)) => apply_event(ev, scanner).await,
            Ok(None) | Err(_) => break,
        }
    }
}

async fn apply_event(ev: WatchEvent, scanner: &Scanner) {
    match ev {
        WatchEvent::Upsert(rel) => match scanner.reindex_path(&rel).await {
            Ok(ReindexOutcome::MissingFromDisk) => {
                tracing::warn!(
                    rel,
                    "watcher: file missing on reindex; following up with remove"
                );
                match scanner.remove_path(&rel).await {
                    Ok(outcome) => {
                        tracing::debug!(rel, ?outcome, "watcher: remove follow-up complete")
                    }
                    Err(e) => tracing::warn!(
                        rel,
                        error = ?e,
                        "watcher: remove follow-up failed"
                    ),
                }
            }
            Ok(outcome) => tracing::debug!(rel, ?outcome, "watcher: upsert applied"),
            Err(e) => tracing::warn!(rel, error = ?e, "watcher: upsert failed"),
        },
        WatchEvent::Remove(rel) => match scanner.remove_path(&rel).await {
            Ok(outcome) => tracing::debug!(rel, ?outcome, "watcher: remove applied"),
            Err(e) => tracing::warn!(rel, error = ?e, "watcher: remove failed"),
        },
    }
}
