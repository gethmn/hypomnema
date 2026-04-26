// Hypomnema — a local daemon that indexes a Markdown directory
// and exposes search (filesystem, content, semantic) and change events.
//
// See AGENTS.md for orientation and docs/hypomnema-handoff.md for scope.

pub mod cli;
pub mod config;
pub mod indexer;
pub mod logging;
pub mod outbox;
pub mod search;
pub mod shutdown;
pub mod store;
pub mod watcher;
