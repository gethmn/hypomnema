use anyhow::{Context, Result, anyhow};
use globset::{Glob, GlobMatcher};
use rusqlite::params;
use tokio::task;

use super::{normalize_prefix, prefix_successor};
use crate::store::SqlitePool;

#[derive(Debug, Clone, Default)]
pub struct FilesystemQuery {
    pub prefix: Option<String>,
    pub glob: Option<String>,
    pub max_depth: Option<usize>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemResult {
    pub path: String,
    pub size: i64,
    pub mtime: String,
    pub content_hash: String,
}

pub async fn search_filesystem(
    pool: SqlitePool,
    q: FilesystemQuery,
) -> Result<(Vec<FilesystemResult>, bool)> {
    task::spawn_blocking(move || run_blocking(pool, q))
        .await
        .context("spawn_blocking join error in search_filesystem")?
}

fn passes_filters(path: &str, matcher: Option<&GlobMatcher>, max_depth: Option<usize>) -> bool {
    if let Some(matcher) = matcher {
        if !matcher.is_match(path) {
            return false;
        }
    }
    if let Some(max_depth) = max_depth {
        let depth = path.split('/').filter(|s| !s.is_empty()).count();
        if depth > max_depth {
            return false;
        }
    }
    true
}

fn run_blocking(pool: SqlitePool, q: FilesystemQuery) -> Result<(Vec<FilesystemResult>, bool)> {
    let prefix = match q.prefix.as_deref() {
        Some(raw) => normalize_prefix(raw)?,
        None => String::new(),
    };
    let matcher = match q.glob.as_deref() {
        Some(g) => Some(
            Glob::new(g)
                .map_err(|e| anyhow!("invalid_glob: {e}"))?
                .compile_matcher(),
        ),
        None => None,
    };

    let conn = pool
        .get()
        .context("acquiring connection from pool for search_filesystem")?;

    let mut results: Vec<FilesystemResult> = Vec::new();
    let mut truncated = false;

    let collect = |path: String,
                   row: &rusqlite::Row<'_>,
                   results: &mut Vec<FilesystemResult>,
                   truncated: &mut bool|
     -> rusqlite::Result<bool> {
        if !passes_filters(&path, matcher.as_ref(), q.max_depth) {
            return Ok(true);
        }
        if results.len() == q.limit {
            *truncated = true;
            return Ok(false);
        }
        results.push(FilesystemResult {
            path,
            size: row.get(1)?,
            mtime: row.get(2)?,
            content_hash: row.get(3)?,
        });
        Ok(true)
    };

    if prefix.is_empty() {
        let mut stmt = conn
            .prepare(
                "SELECT path, size, mtime, content_hash FROM files \
                 ORDER BY path ASC",
            )
            .context("preparing filesystem search query")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            if !collect(path, row, &mut results, &mut truncated)? {
                break;
            }
        }
    } else {
        let upper = prefix_successor(&prefix);
        let mut stmt = conn
            .prepare(
                "SELECT path, size, mtime, content_hash FROM files \
                 WHERE path >= ?1 AND path < ?2 ORDER BY path ASC",
            )
            .context("preparing filesystem prefix-scoped search query")?;
        let mut rows = stmt.query(params![prefix, upper])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            if !collect(path, row, &mut results, &mut truncated)? {
                break;
            }
        }
    }

    Ok((results, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmbeddingConfig;
    use crate::store::Store;
    use rusqlite::params;
    use tempfile::tempdir;

    async fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path(), "index.sqlite", &EmbeddingConfig::default())
            .await
            .unwrap();
        (dir, store)
    }

    async fn seed(store: &Store, rows: Vec<(&'static str, i64, &'static str, &'static str)>) {
        let pool = store.pool();
        task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            for (path, size, mtime, hash) in rows {
                conn.execute(
                    "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![path, size, mtime, hash, "2026-01-01T00:00:00Z", ""],
                )
                .unwrap();
            }
        })
        .await
        .unwrap();
    }

    fn fr(path: &str, size: i64) -> FilesystemResult {
        FilesystemResult {
            path: path.to_string(),
            size,
            mtime: "2026-01-01T00:00:00Z".to_string(),
            content_hash: "sha256:00".to_string(),
        }
    }

    #[tokio::test]
    async fn filesystem_returns_empty_when_index_is_empty() {
        let (_dir, store) = open_store().await;
        let q = FilesystemQuery {
            limit: 100,
            ..Default::default()
        };
        let (results, truncated) = search_filesystem(store.pool(), q).await.unwrap();
        assert!(results.is_empty());
        assert!(!truncated);
    }

    #[tokio::test]
    async fn filesystem_returns_all_paths_when_no_filters() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("b.md", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("c.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        let q = FilesystemQuery {
            limit: 100,
            ..Default::default()
        };
        let (results, truncated) = search_filesystem(store.pool(), q).await.unwrap();
        assert_eq!(results, vec![fr("a.md", 1), fr("b.md", 2), fr("c.md", 3)]);
        assert!(!truncated);
    }

    #[tokio::test]
    async fn filesystem_glob_filter_matches_extension() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("a.txt", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("notes/b.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
                ("notes/b.txt", 4, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        let q = FilesystemQuery {
            glob: Some("**/*.md".to_string()),
            limit: 100,
            ..Default::default()
        };
        let (results, _) = search_filesystem(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "notes/b.md"]);
    }

    #[tokio::test]
    async fn filesystem_prefix_filter_excludes_outside_subdir() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("notes/a.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("notes/sub/b.md", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("notesarchive/c.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
                ("other/d.md", 4, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        // No trailing slash — exercises normalization.
        let q = FilesystemQuery {
            prefix: Some("notes".to_string()),
            limit: 100,
            ..Default::default()
        };
        let (results, _) = search_filesystem(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["notes/a.md", "notes/sub/b.md"]);

        // Trailing slash — same result.
        let q = FilesystemQuery {
            prefix: Some("notes/".to_string()),
            limit: 100,
            ..Default::default()
        };
        let (results, _) = search_filesystem(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["notes/a.md", "notes/sub/b.md"]);
    }

    #[tokio::test]
    async fn filesystem_max_depth_caps_descent() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("top.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("a/b.md", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("a/b/c.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
                ("a/b/c/d.md", 4, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        let q = FilesystemQuery {
            max_depth: Some(2),
            limit: 100,
            ..Default::default()
        };
        let (results, _) = search_filesystem(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a/b.md", "top.md"]);
    }

    #[tokio::test]
    async fn filesystem_truncated_when_more_than_limit() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("b.md", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("c.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        let q = FilesystemQuery {
            limit: 2,
            ..Default::default()
        };
        let (results, truncated) = search_filesystem(store.pool(), q).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(truncated);
    }

    #[tokio::test]
    async fn filesystem_invalid_glob_returns_invalid_glob_error() {
        let (_dir, store) = open_store().await;
        let q = FilesystemQuery {
            glob: Some("[unterminated".to_string()),
            limit: 100,
            ..Default::default()
        };
        let err = search_filesystem(store.pool(), q).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_glob"));
    }

    #[tokio::test]
    async fn filesystem_invalid_prefix_returns_invalid_prefix_error() {
        let (_dir, store) = open_store().await;
        let absolute = FilesystemQuery {
            prefix: Some("/abs/path".to_string()),
            limit: 100,
            ..Default::default()
        };
        let err = search_filesystem(store.pool(), absolute).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_prefix"));

        let parent = FilesystemQuery {
            prefix: Some("notes/../escape".to_string()),
            limit: 100,
            ..Default::default()
        };
        let err = search_filesystem(store.pool(), parent).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_prefix"));
    }

    #[tokio::test]
    async fn filesystem_results_are_sorted_ascending_by_path() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("z.md", 1, "2026-01-01T00:00:00Z", "sha256:00"),
                ("a.md", 2, "2026-01-01T00:00:00Z", "sha256:00"),
                ("m.md", 3, "2026-01-01T00:00:00Z", "sha256:00"),
            ],
        )
        .await;
        let q = FilesystemQuery {
            limit: 100,
            ..Default::default()
        };
        let (results, _) = search_filesystem(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "m.md", "z.md"]);
    }
}
