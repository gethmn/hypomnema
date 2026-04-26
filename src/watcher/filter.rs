use std::path::{Component, Path};

pub fn is_relevant_path(path: &Path) -> bool {
    if path.components().any(|c| match c {
        Component::Normal(os) => os.to_str().is_some_and(|s| s.starts_with('.')),
        _ => false,
    }) {
        return false;
    }
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

pub fn is_sync_conflict(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.contains(".sync-conflict-")
        || name.contains("(conflicted copy")
        || name.contains("conflicted copy)")
}

pub fn vault_relative(canonical_vault: &Path, abs_path: &Path) -> Option<String> {
    let rel = abs_path.strip_prefix(canonical_vault).ok()?;
    let mut out = String::new();
    for component in rel.components() {
        let Component::Normal(os) = component else {
            return None;
        };
        let part = os.to_str()?;
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(part);
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn relevant_accepts_simple_md_file() {
        assert!(is_relevant_path(Path::new("note.md")));
        assert!(is_relevant_path(Path::new("notes/sub/deep.md")));
    }

    #[test]
    fn relevant_rejects_non_md_extensions() {
        assert!(!is_relevant_path(Path::new("note.txt")));
        assert!(!is_relevant_path(Path::new("notes/data.json")));
        assert!(!is_relevant_path(Path::new("README")));
    }

    #[test]
    fn relevant_rejects_dotfile_components_anywhere() {
        assert!(!is_relevant_path(Path::new(".obsidian/workspace.md")));
        assert!(!is_relevant_path(Path::new("notes/.git/HEAD.md")));
        assert!(!is_relevant_path(Path::new(".trash/old.md")));
        assert!(!is_relevant_path(Path::new("notes/.hidden.md")));
    }

    #[test]
    fn relevant_accepts_absolute_md_path() {
        assert!(is_relevant_path(Path::new("/vault/notes/a.md")));
    }

    #[test]
    fn sync_conflict_matches_syncthing() {
        assert!(is_sync_conflict(Path::new(
            "My Note.sync-conflict-20260422-a1b2c3d4.md"
        )));
        assert!(is_sync_conflict(Path::new(
            "vault/sub/note.sync-conflict-x.md"
        )));
    }

    #[test]
    fn sync_conflict_matches_obsidian() {
        assert!(is_sync_conflict(Path::new(
            "Note (conflicted copy 2026-04-25).md"
        )));
    }

    #[test]
    fn sync_conflict_matches_dropbox() {
        assert!(is_sync_conflict(Path::new(
            "Note (Beau's conflicted copy).md"
        )));
    }

    #[test]
    fn sync_conflict_rejects_normal_names() {
        assert!(!is_sync_conflict(Path::new("note.md")));
        assert!(!is_sync_conflict(Path::new("My Note (draft).md")));
        assert!(!is_sync_conflict(Path::new("Sync Notes.md")));
    }

    #[test]
    fn vault_relative_strips_prefix_and_uses_forward_slashes() {
        let vault = PathBuf::from("/vault");
        let abs = PathBuf::from("/vault/notes/sub/a.md");
        assert_eq!(
            vault_relative(&vault, &abs),
            Some("notes/sub/a.md".to_string())
        );
    }

    #[test]
    fn vault_relative_returns_none_on_prefix_mismatch() {
        let vault = PathBuf::from("/vault");
        let abs = PathBuf::from("/elsewhere/note.md");
        assert_eq!(vault_relative(&vault, &abs), None);
    }

    #[test]
    fn vault_relative_returns_none_when_paths_equal() {
        let vault = PathBuf::from("/vault");
        assert_eq!(vault_relative(&vault, &vault), None);
    }

    #[test]
    fn vault_relative_handles_top_level_file() {
        let vault = PathBuf::from("/vault");
        let abs = PathBuf::from("/vault/note.md");
        assert_eq!(vault_relative(&vault, &abs), Some("note.md".to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn vault_relative_returns_none_on_non_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let vault = PathBuf::from("/vault");
        let mut abs = vault.clone();
        abs.push(OsStr::from_bytes(b"bad\xffname.md"));
        assert_eq!(vault_relative(&vault, &abs), None);
    }
}
