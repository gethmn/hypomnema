use super::vcs_ignore::VcsIgnore;
use crate::config::CompiledIgnores;

/// Unified path-inclusion predicate shared by the initial vault walk and the
/// watcher event filter. Both sites call `includes` — no duplicated rule logic.
///
/// Precedence (first matching rule wins):
/// 1. Always exclude `.git/` (unconditional).
/// 2. Config re-include patterns (operator override — beats .gitignore).
/// 3. Config exclude patterns.
/// 4. VCS ignore (`.gitignore` chain), when `respect_gitignore` is true.
/// 5. Default: include.
pub struct InclusionFilter {
    pub config: CompiledIgnores,
    pub vcs: VcsIgnore,
    pub respect_gitignore: bool,
}

impl InclusionFilter {
    pub fn includes(&self, rel_path: &str, is_dir: bool) -> bool {
        // 1. Always exclude .git/ regardless of any other rule.
        if rel_path == ".git" || rel_path.starts_with(".git/") {
            return false;
        }
        // 2. Config re-include wins over everything below (operator override).
        if self.config.reinclude.is_match(rel_path) {
            return true;
        }
        // 3. Config exclude.
        if self.config.exclude.is_match(rel_path) {
            return false;
        }
        // 4. VCS ignore (skipped when respect_gitignore is false).
        if self.respect_gitignore && self.vcs.is_ignored(rel_path, is_dir) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WatcherConfig;
    use std::fs;
    use tempfile::tempdir;

    fn empty_filter() -> InclusionFilter {
        let config = WatcherConfig {
            ignore_patterns: vec![],
            ..WatcherConfig::default()
        }
        .compiled_ignores_split()
        .unwrap();
        InclusionFilter {
            config,
            vcs: VcsIgnore::empty(),
            respect_gitignore: true,
        }
    }

    fn filter_with_patterns(patterns: Vec<&str>, vcs: VcsIgnore) -> InclusionFilter {
        let config = WatcherConfig {
            ignore_patterns: patterns.into_iter().map(String::from).collect(),
            ..WatcherConfig::default()
        }
        .compiled_ignores_split()
        .unwrap();
        InclusionFilter {
            config,
            vcs,
            respect_gitignore: true,
        }
    }

    #[test]
    fn git_dir_always_excluded() {
        let f = empty_filter();
        assert!(!f.includes(".git", true));
        assert!(!f.includes(".git/config", false));
        assert!(!f.includes(".git/refs/heads/main", false));
    }

    #[test]
    fn git_dir_excluded_even_without_gitignore() {
        // No .gitignore anywhere, no config patterns — .git/ is still out.
        let f = InclusionFilter {
            config: WatcherConfig {
                ignore_patterns: vec![],
                ..WatcherConfig::default()
            }
            .compiled_ignores_split()
            .unwrap(),
            vcs: VcsIgnore::empty(),
            respect_gitignore: false,
        };
        assert!(!f.includes(".git", true));
        assert!(!f.includes(".git/COMMIT_EDITMSG", false));
    }

    #[test]
    fn path_matching_only_gitignore_is_excluded() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b"node_modules/\n").unwrap();
        let vcs = VcsIgnore::build(root.path()).unwrap();
        let f = filter_with_patterns(vec![], vcs);

        assert!(!f.includes("node_modules/foo.md", false));
        assert!(f.includes("notes/a.md", false));
    }

    #[test]
    fn config_reinclude_beats_gitignore_exclusion() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b".env*\n").unwrap();
        let vcs = VcsIgnore::build(root.path()).unwrap();
        let f = filter_with_patterns(vec!["!.env.example"], vcs);

        // .env and .env.local are excluded by gitignore; no override.
        assert!(!f.includes(".env", false));
        assert!(!f.includes(".env.local", false));
        // .env.example is re-included by the config override.
        assert!(f.includes(".env.example", false));
    }

    #[test]
    fn config_exclude_beats_gitignore_nonmatch() {
        let root = tempdir().unwrap();
        // gitignore doesn't mention "scratch/".
        fs::write(root.path().join(".gitignore"), b"target/\n").unwrap();
        let vcs = VcsIgnore::build(root.path()).unwrap();
        let f = filter_with_patterns(vec!["scratch/**"], vcs);

        assert!(!f.includes("scratch/draft.md", false));
    }

    #[test]
    fn config_reinclude_beats_config_exclude() {
        // A re-include pattern should beat a config exclude pattern too,
        // because re-include is checked before exclude in the precedence chain.
        let vcs = VcsIgnore::empty();
        let f = filter_with_patterns(vec!["logs/**", "!logs/important.md"], vcs);

        assert!(!f.includes("logs/debug.md", false));
        assert!(f.includes("logs/important.md", false));
    }

    #[test]
    fn path_matching_only_config_exclude_is_excluded() {
        let f = filter_with_patterns(vec!["**/*.tmp"], VcsIgnore::empty());

        assert!(!f.includes("notes/draft.tmp", false));
        assert!(f.includes("notes/real.md", false));
    }

    #[test]
    fn path_matching_nothing_is_included() {
        let f = empty_filter();
        assert!(f.includes("notes/a.md", false));
        assert!(f.includes("deep/sub/dir/note.md", false));
    }

    #[test]
    fn respect_gitignore_false_skips_vcs_check() {
        let root = tempdir().unwrap();
        fs::write(root.path().join(".gitignore"), b"node_modules/\n").unwrap();
        let vcs = VcsIgnore::build(root.path()).unwrap();
        let config = WatcherConfig {
            ignore_patterns: vec![],
            ..WatcherConfig::default()
        }
        .compiled_ignores_split()
        .unwrap();
        let f = InclusionFilter {
            config,
            vcs,
            respect_gitignore: false,
        };

        // gitignore says exclude, but respect_gitignore is false — so included.
        assert!(f.includes("node_modules/foo.md", false));
    }
}
