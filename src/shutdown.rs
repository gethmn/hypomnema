//! Process-shutdown signal handling.
//!
//! Long-running tasks should accept the receiver returned by [`install`] and
//! treat the value flipping to `true` as a request to drain in-flight work and
//! exit. Pitfall #9 (ungraceful shutdown and torn writes) is the reason every
//! long-running task in steps 2–5 must observe this signal: never call
//! [`std::process::exit`] from inside a task — let the task return naturally so
//! its drop handlers run, transactions commit, and outbox writes flush.
//!
//! A second signal force-aborts the process with exit code 130 (the shell
//! convention for SIGINT-terminated processes) so a hung clean shutdown can be
//! force-killed via Ctrl+C twice.

use tokio::sync::watch;

/// Install the shutdown signal handler and return a watch receiver that flips
/// to `true` when the first signal arrives.
///
/// On Unix, listens for SIGINT and SIGTERM. On other platforms, listens for
/// Ctrl+C only (which is what [`tokio::signal::ctrl_c`] supports portably).
///
/// **Must be called from within a tokio runtime context** — internally spawns
/// a task via [`tokio::spawn`].
pub fn install() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);

    tokio::spawn(async move {
        let mut signals = Signals::install();
        signals.recv().await;
        tracing::info!("shutdown signal received, draining tasks");
        let _ = tx.send(true);
        signals.recv().await;
        tracing::warn!("second shutdown signal received, force-exiting (130)");
        std::process::exit(130);
    });

    rx
}

#[cfg(unix)]
struct Signals {
    sigint: tokio::signal::unix::Signal,
    sigterm: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl Signals {
    fn install() -> Self {
        use tokio::signal::unix::{SignalKind, signal};
        Self {
            sigint: signal(SignalKind::interrupt()).expect("install SIGINT handler"),
            sigterm: signal(SignalKind::terminate()).expect("install SIGTERM handler"),
        }
    }

    async fn recv(&mut self) {
        tokio::select! {
            _ = self.sigint.recv() => {}
            _ = self.sigterm.recv() => {}
        }
    }
}

#[cfg(not(unix))]
struct Signals;

#[cfg(not(unix))]
impl Signals {
    fn install() -> Self {
        Self
    }

    async fn recv(&mut self) {
        let _ = tokio::signal::ctrl_c().await;
    }
}
