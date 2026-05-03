use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use tracing::{debug, warn};
use walkdir::WalkDir;

use crate::watcher::inclusion::InclusionFilter;

#[derive(Debug)]
pub struct WalkedFile {
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub size: i64,
    pub mtime: String,
}

#[derive(Debug, Default)]
pub struct WalkOutcome {
    pub entries: Vec<WalkedFile>,
    pub skipped_outside_vault: usize,
    pub walk_errors: usize,
}

pub fn walk_vault(vault: &Path, filter: &InclusionFilter) -> Result<WalkOutcome> {
    let canonical_vault = fs::canonicalize(vault)
        .with_context(|| format!("canonicalizing vault {}", vault.display()))?;

    let mut outcome = WalkOutcome::default();

    for result in WalkDir::new(&canonical_vault).follow_links(true) {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                warn!("walk: skipping entry due to walk error: {err}");
                outcome.walk_errors += 1;
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let canonical = match fs::canonicalize(path) {
            Ok(c) => c,
            Err(err) => {
                warn!(
                    "walk: skipping {} (canonicalize failed: {err})",
                    path.display()
                );
                outcome.walk_errors += 1;
                continue;
            }
        };
        if !canonical.starts_with(&canonical_vault) {
            warn!(
                "walk: skipping {} — canonical {} is outside vault {}",
                path.display(),
                canonical.display(),
                canonical_vault.display()
            );
            outcome.skipped_outside_vault += 1;
            continue;
        }

        let rel = match path.strip_prefix(&canonical_vault) {
            Ok(r) => r,
            Err(_) => {
                warn!(
                    "walk: skipping {} — could not strip vault prefix",
                    path.display()
                );
                continue;
            }
        };
        let rel_str = match rel.to_str() {
            Some(s) => to_forward_slash(s),
            None => {
                warn!("walk: skipping non-UTF-8 path {}", path.display());
                continue;
            }
        };
        if !filter.includes(&rel_str, false) {
            debug!("walk: ignoring {}", rel_str);
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(err) => {
                warn!("walk: skipping {} (metadata error: {err})", path.display());
                outcome.walk_errors += 1;
                continue;
            }
        };
        let size = metadata.len() as i64;
        let mtime = match metadata.modified() {
            Ok(t) => format_mtime(t),
            Err(err) => {
                warn!("walk: skipping {} (mtime error: {err})", path.display());
                outcome.walk_errors += 1;
                continue;
            }
        };
        outcome.entries.push(WalkedFile {
            rel_path: rel_str,
            abs_path: path.to_path_buf(),
            size,
            mtime,
        });
    }

    Ok(outcome)
}

fn to_forward_slash(s: &str) -> String {
    if std::path::MAIN_SEPARATOR == '/' {
        s.to_string()
    } else {
        s.replace(std::path::MAIN_SEPARATOR, "/")
    }
}

pub(crate) fn format_mtime(t: SystemTime) -> String {
    let dt: DateTime<Utc> = t.into();
    dt.to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use crate::config::WatcherConfig;
    use crate::watcher::vcs_ignore::VcsIgnore;

    fn empty_filter() -> InclusionFilter {
        InclusionFilter {
            config: WatcherConfig {
                ignore_patterns: vec![],
                ..WatcherConfig::default()
            }
            .compiled_ignores_split()
            .unwrap(),
            vcs: VcsIgnore::empty(),
            respect_gitignore: false,
        }
    }

    fn filter_with_patterns(patterns: &[&str]) -> InclusionFilter {
        InclusionFilter {
            config: WatcherConfig {
                ignore_patterns: patterns.iter().map(|s| s.to_string()).collect(),
                ..WatcherConfig::default()
            }
            .compiled_ignores_split()
            .unwrap(),
            vcs: VcsIgnore::empty(),
            respect_gitignore: false,
        }
    }

    #[test]
    fn walk_filters_to_md_only() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        fs::write(vault.join("a.md"), b"# A").unwrap();
        fs::write(vault.join("b.txt"), b"# B").unwrap();
        fs::write(vault.join("c.markdown"), b"# C").unwrap();
        fs::write(vault.join(".hidden.md"), b"# H").unwrap();

        let outcome = walk_vault(vault, &empty_filter()).unwrap();
        let mut paths: Vec<_> = outcome.entries.iter().map(|e| e.rel_path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec![".hidden.md".to_string(), "a.md".to_string()]);
    }

    #[test]
    fn walk_applies_ignore_patterns() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        fs::write(vault.join("kept.md"), b"# K").unwrap();
        fs::create_dir(vault.join(".git")).unwrap();
        fs::write(vault.join(".git/HEAD.md"), b"# H").unwrap();
        fs::create_dir_all(vault.join("notes/dir")).unwrap();
        fs::write(vault.join("notes/dir/file.md.tmp"), b"x").unwrap();

        // .git/** is always excluded by InclusionFilter step 1; **/*.tmp is a
        // config exclude pattern (note: file.md.tmp has no .md extension so
        // the extension check would already drop it, but the config pattern is
        // kept for API-equivalence with the old GlobSet test).
        let filter = filter_with_patterns(&[".git/**", "**/*.tmp"]);
        let outcome = walk_vault(vault, &filter).unwrap();
        let paths: Vec<_> = outcome.entries.iter().map(|e| e.rel_path.clone()).collect();
        assert_eq!(paths, vec!["kept.md".to_string()]);
    }

    #[test]
    fn walk_uses_forward_slash_relative_paths() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        fs::create_dir_all(vault.join("a/b")).unwrap();
        fs::write(vault.join("a/b/note.md"), b"# N").unwrap();

        let outcome = walk_vault(vault, &empty_filter()).unwrap();
        let paths: Vec<_> = outcome.entries.iter().map(|e| e.rel_path.clone()).collect();
        assert_eq!(paths, vec!["a/b/note.md".to_string()]);
    }

    #[test]
    fn walk_records_size_and_mtime() {
        let dir = tempdir().unwrap();
        let vault = dir.path();
        let payload = b"hello, walker";
        fs::write(vault.join("note.md"), payload).unwrap();

        let outcome = walk_vault(vault, &empty_filter()).unwrap();
        assert_eq!(outcome.entries.len(), 1);
        let entry = &outcome.entries[0];
        assert_eq!(entry.size, payload.len() as i64);
        // ISO-8601 with `Z` and microsecond precision per format_mtime.
        assert!(entry.mtime.ends_with('Z'));
        assert!(entry.mtime.contains('T'));
    }

    #[cfg(unix)]
    #[test]
    fn walk_follows_internal_symlink_once() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let vault = dir.path();
        fs::write(vault.join("real.md"), b"# real").unwrap();
        symlink(vault.join("real.md"), vault.join("link.md")).unwrap();

        let outcome = walk_vault(vault, &empty_filter()).unwrap();
        let mut paths: Vec<_> = outcome.entries.iter().map(|e| e.rel_path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["link.md".to_string(), "real.md".to_string()]);
        assert_eq!(outcome.skipped_outside_vault, 0);
    }

    #[cfg(unix)]
    #[test]
    fn walk_rejects_symlink_pointing_outside_vault() {
        use std::os::unix::fs::symlink;
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), b"# secret").unwrap();
        let dir = tempdir().unwrap();
        let vault = dir.path();
        fs::write(vault.join("note.md"), b"# note").unwrap();
        symlink(outside.path().join("secret.md"), vault.join("link.md")).unwrap();

        let outcome = walk_vault(vault, &empty_filter()).unwrap();
        let paths: Vec<_> = outcome.entries.iter().map(|e| e.rel_path.clone()).collect();
        assert_eq!(paths, vec!["note.md".to_string()]);
        assert!(outcome.skipped_outside_vault >= 1);
    }
}
