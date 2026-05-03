use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::EventKind;
use notify::event::{ModifyKind, RenameMode};
use notify_debouncer_full::DebouncedEvent;

use super::WatchEvent;
use super::filter;
use super::inclusion::InclusionFilter;

pub(super) struct TranslateCtx {
    pub vault_roots: Vec<PathBuf>,
    pub filter: Arc<InclusionFilter>,
}

pub(super) fn translate(events: Vec<DebouncedEvent>, ctx: &TranslateCtx) -> Vec<WatchEvent> {
    let mut out = Vec::new();
    for event in events {
        translate_event(&event, ctx, &mut out);
    }
    out
}

fn translate_event(event: &DebouncedEvent, ctx: &TranslateCtx, out: &mut Vec<WatchEvent>) {
    match &event.event.kind {
        EventKind::Create(_)
        | EventKind::Modify(ModifyKind::Any)
        | EventKind::Modify(ModifyKind::Other)
        | EventKind::Modify(ModifyKind::Data(_)) => {
            push_upserts(&event.event.paths, ctx, out);
        }
        EventKind::Modify(ModifyKind::Name(rename_mode)) => {
            translate_rename(*rename_mode, &event.event.paths, ctx, out);
        }
        EventKind::Remove(_) => {
            push_removes(&event.event.paths, ctx, out);
        }
        EventKind::Modify(ModifyKind::Metadata(_)) => {
            // mtime / permissions only — no content change. Drop early to
            // avoid a downstream syscall storm; the content-hash gate would
            // otherwise catch them.
        }
        EventKind::Access(_) => {
            // Reads, opens, closes — never relevant.
        }
        EventKind::Other | EventKind::Any => {
            tracing::trace!(?event.event, "watcher: dropping ambiguous event kind");
        }
    }
}

fn translate_rename(
    mode: RenameMode,
    paths: &[PathBuf],
    ctx: &TranslateCtx,
    out: &mut Vec<WatchEvent>,
) {
    match mode {
        RenameMode::Both if paths.len() == 2 => {
            if let Some(rel) = filter_pass(&paths[0], ctx) {
                out.push(WatchEvent::Remove(rel));
            }
            if let Some(rel) = filter_pass(&paths[1], ctx) {
                out.push(WatchEvent::Upsert(rel));
            }
        }
        RenameMode::Both => {
            tracing::trace!(
                paths = ?paths,
                "watcher: rename Both with unexpected path count, dropping"
            );
        }
        RenameMode::From => push_removes(paths, ctx, out),
        RenameMode::To => push_upserts(paths, ctx, out),
        RenameMode::Any | RenameMode::Other => {
            // Direction not reported by the platform. Treat as upsert and
            // let the consumer's MissingFromDisk path handle the case where
            // the file no longer exists.
            tracing::trace!(?mode, paths = ?paths, "watcher: undirected rename treated as upsert");
            push_upserts(paths, ctx, out);
        }
    }
}

fn push_upserts(paths: &[PathBuf], ctx: &TranslateCtx, out: &mut Vec<WatchEvent>) {
    for path in paths {
        if let Some(rel) = filter_pass(path, ctx) {
            out.push(WatchEvent::Upsert(rel));
        }
    }
}

fn push_removes(paths: &[PathBuf], ctx: &TranslateCtx, out: &mut Vec<WatchEvent>) {
    for path in paths {
        if let Some(rel) = filter_pass(path, ctx) {
            out.push(WatchEvent::Remove(rel));
        }
    }
}

fn filter_pass(abs: &Path, ctx: &TranslateCtx) -> Option<String> {
    let rel = ctx
        .vault_roots
        .iter()
        .find_map(|root| filter::vault_relative(root, abs))?;
    let rel_path = Path::new(&rel);
    if !filter::is_relevant_path(rel_path) {
        return None;
    }
    if !ctx.filter.includes(&rel, false) {
        return None;
    }
    if filter::is_sync_conflict(rel_path) {
        return None;
    }
    Some(rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::Event;
    use notify::event::{AccessKind, AccessMode, CreateKind, DataChange, MetadataKind, RemoveKind};
    use std::time::Instant;

    use crate::config::WatcherConfig;
    use crate::watcher::vcs_ignore::VcsIgnore;

    fn empty_filter() -> Arc<InclusionFilter> {
        Arc::new(InclusionFilter {
            config: WatcherConfig {
                ignore_patterns: vec![],
                ..WatcherConfig::default()
            }
            .compiled_ignores_split()
            .unwrap(),
            vcs: VcsIgnore::empty(),
            respect_gitignore: false,
        })
    }

    fn filter_with_patterns(patterns: &[&str]) -> Arc<InclusionFilter> {
        Arc::new(InclusionFilter {
            config: WatcherConfig {
                ignore_patterns: patterns.iter().map(|s| s.to_string()).collect(),
                ..WatcherConfig::default()
            }
            .compiled_ignores_split()
            .unwrap(),
            vcs: VcsIgnore::empty(),
            respect_gitignore: false,
        })
    }

    fn ctx_with(vault: &str, filter: Arc<InclusionFilter>) -> TranslateCtx {
        TranslateCtx {
            vault_roots: vec![PathBuf::from(vault)],
            filter,
        }
    }

    fn event(kind: EventKind, paths: Vec<PathBuf>) -> DebouncedEvent {
        let mut ev = Event::new(kind);
        ev.paths = paths;
        DebouncedEvent::new(ev, Instant::now())
    }

    #[test]
    fn create_file_becomes_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/vault/notes/a.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("notes/a.md".to_string())]
        );
    }

    #[test]
    fn modify_data_becomes_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            vec![PathBuf::from("/vault/note.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("note.md".to_string())]
        );
    }

    #[test]
    fn modify_any_becomes_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Any),
            vec![PathBuf::from("/vault/note.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("note.md".to_string())]
        );
    }

    #[test]
    fn remove_file_becomes_remove() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Remove(RemoveKind::File),
            vec![PathBuf::from("/vault/notes/old.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Remove("notes/old.md".to_string())]
        );
    }

    #[test]
    fn modify_metadata_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
            vec![PathBuf::from("/vault/note.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn access_events_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Access(AccessKind::Open(AccessMode::Read)),
            vec![PathBuf::from("/vault/note.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn other_kind_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(EventKind::Other, vec![PathBuf::from("/vault/note.md")]);
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn rename_both_decomposes_into_remove_then_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![
                PathBuf::from("/vault/notes/a.md"),
                PathBuf::from("/vault/notes/b.md"),
            ],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![
                WatchEvent::Remove("notes/a.md".to_string()),
                WatchEvent::Upsert("notes/b.md".to_string()),
            ]
        );
    }

    #[test]
    fn rename_from_becomes_remove() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            vec![PathBuf::from("/vault/notes/gone.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Remove("notes/gone.md".to_string())]
        );
    }

    #[test]
    fn rename_to_becomes_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            vec![PathBuf::from("/vault/notes/new.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("notes/new.md".to_string())]
        );
    }

    #[test]
    fn rename_any_treated_as_upsert() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            vec![PathBuf::from("/vault/notes/uncertain.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("notes/uncertain.md".to_string())]
        );
    }

    #[test]
    fn rename_both_with_wrong_path_count_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![PathBuf::from("/vault/only.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn outside_vault_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/elsewhere/strange.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn alternate_vault_root_spelling_is_accepted() {
        let ctx = TranslateCtx {
            vault_roots: vec![
                PathBuf::from("/private/var/folders/example/vault"),
                PathBuf::from("/var/folders/example/vault"),
            ],
            filter: empty_filter(),
        };
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/var/folders/example/vault/note.md")],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("note.md".to_string())]
        );
    }

    #[test]
    fn non_md_extension_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/vault/note.txt")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn dotfile_component_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/vault/.git/HEAD.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn ignore_globset_filters_match() {
        let ctx = ctx_with("/vault", filter_with_patterns(&["**/*.tmp.md"]));
        let ev = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            vec![PathBuf::from("/vault/notes/draft.tmp.md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn sync_conflict_dropped() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Create(CreateKind::File),
            vec![PathBuf::from("/vault/Note (conflicted copy 2026-04-25).md")],
        );
        assert!(translate(vec![ev], &ctx).is_empty());
    }

    #[test]
    fn rename_both_one_side_filtered_other_side_emitted() {
        let ctx = ctx_with("/vault", empty_filter());
        let ev = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
            vec![
                PathBuf::from("/vault/.obsidian/workspace.md"),
                PathBuf::from("/vault/notes/real.md"),
            ],
        );
        assert_eq!(
            translate(vec![ev], &ctx),
            vec![WatchEvent::Upsert("notes/real.md".to_string())]
        );
    }

    #[test]
    fn batch_preserves_order_across_events() {
        let ctx = ctx_with("/vault", empty_filter());
        let events = vec![
            event(
                EventKind::Create(CreateKind::File),
                vec![PathBuf::from("/vault/a.md")],
            ),
            event(
                EventKind::Remove(RemoveKind::File),
                vec![PathBuf::from("/vault/b.md")],
            ),
        ];
        assert_eq!(
            translate(events, &ctx),
            vec![
                WatchEvent::Upsert("a.md".to_string()),
                WatchEvent::Remove("b.md".to_string()),
            ]
        );
    }
}
