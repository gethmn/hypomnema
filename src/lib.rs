// Hypomnema — a local daemon that indexes a Markdown directory
// and exposes search (filesystem, content, semantic) and change events.
//
// See AGENTS.md for orientation and docs/hypomnema-handoff.md for scope.

pub mod api;
pub mod chunk;
pub mod cli;
pub mod client;
pub mod config;
pub mod control_plane;
pub mod embedding;
pub mod events;
pub mod indexer;
pub mod legacy_state_migration;
pub mod logging;
pub mod mcp;
pub mod search;
pub mod shutdown;
pub mod store;
pub mod vault_registry;
pub mod watcher;
