use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::Match;
use walkdir::WalkDir;

/// Per-vault `.gitignore` matcher built once at watcher startup.
///
/// Walks the vault root to collect all `.gitignore` files (root + nested),
/// building one `ignore::gitignore::Gitignore` per file. Matchers are stored
/// deepest-first so that the most-specific rule wins when multiple `.gitignore`
/// files cover a path.
///
/// This type is pure in-memory predicate evaluation — no I/O on the hot path.
pub struct VcsIgnore {
    matchers: Vec<(PathBuf, ignore::gitignore::Gitignore)>,
}

impl VcsIgnore {
    /// Walk `vault`, load every `.gitignore` found, and build the matcher chain.
    pub fn build(vault: &Path) -> Result<Self> {
        let mut matchers = Vec::new();

        for entry in WalkDir::new(vault)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_name() != ".gitignore" {
                continue;
            }
            let gitignore_path = entry.path();
            let dir = gitignore_path.parent().unwrap_or(vault);
            let mut builder = ignore::gitignore::GitignoreBuilder::new(dir);
            builder.add(gitignore_path);
            match builder.build() {
                Ok(gi) => {
                    let rel_dir = dir.strip_prefix(vault).unwrap_or(Path::new(""));
                    matchers.push((rel_dir.to_path_buf(), gi));
                }
                Err(e) => {
                    tracing::warn!(
                        path = %gitignore_path.display(),
                        error = ?e,
                        "vcs_ignore: failed to parse .gitignore, skipping"
                    );
                }
            }
        }

        // Deepest directories first — most-specific rule checked first.
        matchers.sort_by_key(|(dir, _)| Reverse(dir.components().count()));

        Ok(Self { matchers })
    }

    /// Returns `true` if `rel_path` (vault-relative, forward-slash) is ignored
    /// by any applicable `.gitignore` in the matcher chain.
    ///
    /// Checks matchers deepest-first; the first definitive match (ignore or
    /// whitelist/negation) wins. Returns `false` if no matcher matches.
    pub fn is_ignored(&self, rel_path: &str, is_dir: bool) -> bool {
        let rel = Path::new(rel_path);

        for (dir, gi) in &self.matchers {
            let path_to_check = if dir.as_os_str().is_empty() {
                // Root-level gitignore: check against full rel_path.
                rel
            } else {
                // Nested gitignore: only applicable when rel_path is under dir.
                match rel.strip_prefix(dir) {
                    Ok(p) => p,
                    Err(_) => continue,
                }
            };

            match gi.matched_path_or_any_parents(path_to_check, is_dir) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => continue,
            }
        }

        false
    }

    /// Empty matcher — equivalent to "no `.gitignore` files present".
    /// `is_ignored` always returns `false`.
    pub fn empty() -> Self {
        Self {
            matchers: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn empty_matcher_never_ignores() {
        let vcs = VcsIgnore::empty();
        assert!(!vcs.is_ignored("anything.md", false));
        assert!(!vcs.is_ignored("node_modules/foo.md", false));
        assert!(!vcs.is_ignored(".git/config", false));
    }

    #[test]
    fn root_gitignore_ignores_listed_patterns() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b"target/\nnode_modules/\n").unwrap();
        fs::create_dir_all(root.path().join("target/debug")).unwrap();
        fs::create_dir_all(root.path().join("node_modules/pkg")).unwrap();

        let vcs = VcsIgnore::build(root.path()).unwrap();

        assert!(vcs.is_ignored("target/debug/foo.md", false));
        assert!(vcs.is_ignored("node_modules/pkg/index.md", false));
        assert!(!vcs.is_ignored("notes/a.md", false));
    }

    #[test]
    fn root_gitignore_does_not_ignore_unmatched_paths() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b"target/\n").unwrap();

        let vcs = VcsIgnore::build(root.path()).unwrap();

        assert!(!vcs.is_ignored("notes/a.md", false));
        assert!(!vcs.is_ignored("src/lib.rs", false));
    }

    #[test]
    fn nested_gitignore_applies_within_its_subtree() {
        let root = tempdir().unwrap();
        let src = root.path().join("src");
        fs::create_dir_all(&src).unwrap();
        // Nested gitignore excludes `build/` only under `src/`.
        fs::write(src.join(".gitignore"), b"build/\n").unwrap();

        let vcs = VcsIgnore::build(root.path()).unwrap();

        assert!(vcs.is_ignored("src/build/output.md", false));
        // `build/` at vault root is not excluded (no root gitignore).
        assert!(!vcs.is_ignored("build/output.md", false));
        assert!(!vcs.is_ignored("notes/a.md", false));
    }

    #[test]
    fn negation_pattern_reincluded_over_broader_exclusion() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b"*.log\n!important.log\n").unwrap();

        let vcs = VcsIgnore::build(root.path()).unwrap();

        assert!(vcs.is_ignored("debug.log", false));
        assert!(vcs.is_ignored("app.log", false));
        // `!important.log` negation in the same gitignore re-includes it.
        assert!(!vcs.is_ignored("important.log", false));
    }

    #[test]
    fn no_gitignore_file_means_nothing_ignored() {
        let root = tempdir().unwrap();
        // No .gitignore at all.
        let vcs = VcsIgnore::build(root.path()).unwrap();

        assert!(!vcs.is_ignored("notes/a.md", false));
        assert!(!vcs.is_ignored("target/foo.md", false));
    }

    #[test]
    fn deeper_gitignore_negation_wins_over_shallower_exclusion() {
        let root = tempdir().unwrap();
        let sub = root.path().join("src");
        fs::create_dir_all(&sub).unwrap();
        // Root excludes everything under `src/generated/`.
        fs::write(root.path().join(".gitignore"), b"src/generated/\n").unwrap();
        // Nested gitignore re-includes `src/generated/keep.md`.
        fs::write(sub.join(".gitignore"), b"!generated/keep.md\n").unwrap();

        let vcs = VcsIgnore::build(root.path()).unwrap();

        // The nested re-include (`!generated/keep.md`) is checked first (deepest)
        // and whitelists the path before the root exclusion is reached.
        assert!(!vcs.is_ignored("src/generated/keep.md", false));
        assert!(vcs.is_ignored("src/generated/other.md", false));
    }
}
