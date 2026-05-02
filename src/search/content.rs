use std::borrow::Cow;

use anyhow::{Context, Result, anyhow};
use regex::Regex;
use rusqlite::OptionalExtension;
use rusqlite::params;
use tokio::task;

use super::{normalize_prefix, prefix_successor};
use crate::store::SqlitePool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ContentMode {
    #[default]
    Substring,
    Regex,
    Ranked,
}

#[derive(Debug, Clone)]
pub struct ContentQuery {
    pub query: String,
    pub mode: ContentMode,
    pub regex: bool,
    pub case_sensitive: bool,
    pub prefix: Option<String>,
    pub include_matches: bool,
    pub max_matches_per_file: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentResult {
    pub path: String,
    pub match_count: usize,
    pub matches: Vec<ContentMatch>,
    /// BM25 score (negative; lower = better). Set only for ranked-mode results.
    pub score: Option<f64>,
    /// 1-indexed rank in the result set. Set only for ranked-mode results.
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub line: usize,
    pub text: String,
}

const TEXT_TRIM_BYTES: usize = 240;

enum Matcher {
    Substring { needle: String, lower: bool },
    Regex(Regex),
}

pub async fn search_content(
    pool: SqlitePool,
    q: ContentQuery,
) -> Result<(Vec<ContentResult>, bool)> {
    task::spawn_blocking(move || run_blocking(pool, q))
        .await
        .context("spawn_blocking join error in search_content")?
}

fn build_matcher(q: &ContentQuery) -> Result<Matcher> {
    if q.regex {
        let re = Regex::new(&q.query).map_err(|e| anyhow!("invalid_regex: {e}"))?;
        return Ok(Matcher::Regex(re));
    }
    let lower = !q.case_sensitive;
    let needle = if lower {
        q.query.to_ascii_lowercase()
    } else {
        q.query.clone()
    };
    Ok(Matcher::Substring { needle, lower })
}

fn collect_match_starts(body: &str, matcher: &Matcher) -> Vec<usize> {
    match matcher {
        Matcher::Substring { needle, lower } => {
            if needle.is_empty() {
                return Vec::new();
            }
            // ASCII-folded lowercasing preserves byte offsets — non-ASCII
            // bytes pass through unchanged — so the indices we return are
            // valid into either the original or the lowercased body.
            let haystack: Cow<str> = if *lower {
                Cow::Owned(body.to_ascii_lowercase())
            } else {
                Cow::Borrowed(body)
            };
            haystack
                .match_indices(needle.as_str())
                .map(|(i, _)| i)
                .collect()
        }
        Matcher::Regex(re) => re.find_iter(body).map(|m| m.start()).collect(),
    }
}

fn match_line_text(body: &str, start: usize) -> (usize, String) {
    let line_no = 1 + body[..start].matches('\n').count();
    let line_start = body[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = body[start..]
        .find('\n')
        .map(|i| start + i)
        .unwrap_or(body.len());
    let line = &body[line_start..line_end];
    (line_no, trim_text_bytes(line, TEXT_TRIM_BYTES))
}

// UTF-8-safe truncation: take at most `max` bytes of `s`, snapping back to
// the nearest character boundary. `floor_char_boundary` is unstable as of
// 1.86, so we walk back manually.
fn trim_text_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn process_row(
    path: String,
    content: &str,
    q: &ContentQuery,
    matcher: &Matcher,
    results: &mut Vec<ContentResult>,
    truncated: &mut bool,
) -> bool {
    let starts = collect_match_starts(content, matcher);
    if starts.is_empty() {
        return true;
    }
    let match_count = starts.len();
    let matches: Vec<ContentMatch> = if q.include_matches {
        starts
            .into_iter()
            .take(q.max_matches_per_file)
            .map(|start| {
                let (line, text) = match_line_text(content, start);
                ContentMatch { line, text }
            })
            .collect()
    } else {
        Vec::new()
    };
    if results.len() == q.limit {
        *truncated = true;
        return false;
    }
    results.push(ContentResult {
        path,
        match_count,
        matches,
        score: None,
        rank: None,
    });
    true
}

fn run_blocking(pool: SqlitePool, q: ContentQuery) -> Result<(Vec<ContentResult>, bool)> {
    if q.mode == ContentMode::Ranked {
        return run_ranked_blocking(pool, q);
    }

    let prefix = match q.prefix.as_deref() {
        Some(raw) => normalize_prefix(raw)?,
        None => String::new(),
    };
    let matcher = build_matcher(&q)?;
    let conn = pool
        .get()
        .context("acquiring connection from pool for search_content")?;

    let mut results: Vec<ContentResult> = Vec::new();
    let mut truncated = false;

    if prefix.is_empty() {
        let mut stmt = conn
            .prepare("SELECT path, content FROM files ORDER BY path ASC")
            .context("preparing content search query")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let content: String = row.get(1)?;
            if !process_row(path, &content, &q, &matcher, &mut results, &mut truncated) {
                break;
            }
        }
    } else {
        let upper = prefix_successor(&prefix);
        let mut stmt = conn
            .prepare(
                "SELECT path, content FROM files \
                 WHERE path >= ?1 AND path < ?2 ORDER BY path ASC",
            )
            .context("preparing content prefix-scoped search query")?;
        let mut rows = stmt.query(params![prefix, upper])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let content: String = row.get(1)?;
            if !process_row(path, &content, &q, &matcher, &mut results, &mut truncated) {
                break;
            }
        }
    }

    Ok((results, truncated))
}

fn run_ranked_blocking(pool: SqlitePool, q: ContentQuery) -> Result<(Vec<ContentResult>, bool)> {
    let conn = pool
        .get()
        .context("acquiring connection from pool for ranked search_content")?;

    // FTS5 `rank` column is negative BM25; ORDER BY rank puts best matches first.
    // The local limit is q.limit + 1 so we can detect truncation.
    let results = if let Some(raw) = q.prefix.as_deref() {
        let prefix = normalize_prefix(raw)?;
        let upper = prefix_successor(&prefix);
        let mut stmt = conn
            .prepare(
                "SELECT f.path, fts.rank, f.content_hash, f.size, f.mtime \
                 FROM files_fts fts \
                 JOIN files f ON f.rowid = fts.rowid \
                 WHERE files_fts MATCH ?1 \
                   AND f.path >= ?2 AND f.path < ?3 \
                 ORDER BY fts.rank \
                 LIMIT ?4",
            )
            .context("preparing prefix-scoped ranked search query")?;
        collect_ranked_rows(&mut stmt, params![q.query, prefix, upper, q.limit + 1])?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT f.path, fts.rank, f.content_hash, f.size, f.mtime \
                 FROM files_fts fts \
                 JOIN files f ON f.rowid = fts.rowid \
                 WHERE files_fts MATCH ?1 \
                 ORDER BY fts.rank \
                 LIMIT ?2",
            )
            .context("preparing ranked search query")?;
        collect_ranked_rows(&mut stmt, params![q.query, q.limit + 1])?
    };

    let truncated = results.len() > q.limit;
    let mut results = results;
    if truncated {
        results.truncate(q.limit);
    }

    // Assign per-vault 1-indexed ranks before returning (cross-vault merge
    // re-ranks globally in the API layer).
    let ranked: Vec<ContentResult> = results
        .into_iter()
        .enumerate()
        .map(
            |(i, (path, score, _content_hash, _size, _mtime))| ContentResult {
                path,
                match_count: 0,
                matches: Vec::new(),
                score: Some(score),
                rank: Some((i + 1) as u32),
            },
        )
        .collect();

    Ok((ranked, truncated))
}

/// FTS5 row data: (path, BM25 score, content_hash, size, mtime_iso)
type RankedRow = (String, f64, String, i64, String);

fn collect_ranked_rows(
    stmt: &mut rusqlite::Statement<'_>,
    params: impl rusqlite::Params,
) -> Result<Vec<RankedRow>> {
    let rows = stmt
        .query_map(params, |row| {
            Ok((
                row.get::<_, String>(0)?, // path
                row.get::<_, f64>(1)?,    // rank (BM25, negative)
                row.get::<_, String>(2)?, // content_hash
                row.get::<_, i64>(3)?,    // size
                row.get::<_, String>(4)?, // mtime
            ))
        })
        .context("executing ranked FTS5 query")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| {
            // FTS5 parse errors surface as rusqlite errors containing "fts5"
            // in the message. Classify as invalid_query so callers can return
            // HTTP 400 rather than 500.
            let msg = e.to_string();
            if msg.contains("fts5") || msg.contains("syntax error") || msg.contains("unknown") {
                anyhow::anyhow!("invalid_query: {msg}")
            } else {
                anyhow::anyhow!("{msg}")
            }
        })?;
    Ok(rows)
}

// ===== content_get: per-vault blocking retrieval by path =====

#[derive(Debug)]
pub struct ContentGetRow {
    pub content: String,
    pub content_hash: String,
    pub size: i64,
    pub mtime: String,
}

pub async fn content_get_by_paths(
    pool: SqlitePool,
    paths: Vec<String>,
) -> Result<Vec<(String, Option<ContentGetRow>)>> {
    tokio::task::spawn_blocking(move || content_get_blocking(pool, paths))
        .await
        .context("spawn_blocking join error in content_get_by_paths")?
}

fn content_get_blocking(
    pool: SqlitePool,
    paths: Vec<String>,
) -> Result<Vec<(String, Option<ContentGetRow>)>> {
    let conn = pool.get().context("acquiring connection for content_get")?;
    let mut results = Vec::with_capacity(paths.len());
    for path in &paths {
        let row = conn
            .query_row(
                "SELECT content, content_hash, size, mtime FROM files WHERE path = ?1",
                rusqlite::params![path],
                |row| {
                    Ok(ContentGetRow {
                        content: row.get(0)?,
                        content_hash: row.get(1)?,
                        size: row.get(2)?,
                        mtime: row.get(3)?,
                    })
                },
            )
            .optional()
            .context("querying files for content_get")?;
        results.push((path.clone(), row));
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmbeddingConfig;
    use crate::store::Store;
    use rusqlite::params;
    use tempfile::tempdir;

    fn base_query(query: &str) -> ContentQuery {
        ContentQuery {
            query: query.to_string(),
            mode: ContentMode::Substring,
            regex: false,
            case_sensitive: false,
            prefix: None,
            include_matches: true,
            max_matches_per_file: 5,
            limit: 100,
        }
    }

    async fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let store = Store::open(
            &crate::vault_registry::VaultId::new(),
            dir.path(),
            "index.sqlite",
            &EmbeddingConfig::default(),
        )
        .await
        .unwrap();
        (dir, store)
    }

    async fn seed<P, C>(store: &Store, rows: Vec<(P, C)>)
    where
        P: Into<String> + Send + 'static,
        C: Into<String> + Send + 'static,
    {
        let pool = store.pool();
        let owned: Vec<(String, String)> = rows
            .into_iter()
            .map(|(p, c)| (p.into(), c.into()))
            .collect();
        task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            for (path, content) in owned {
                conn.execute(
                    "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        path,
                        content.len() as i64,
                        "2026-01-01T00:00:00Z",
                        "sha256:00",
                        "2026-01-01T00:00:00Z",
                        content
                    ],
                )
                .unwrap();
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn content_substring_matches_case_insensitive_by_default() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", "Hello World"),
                ("b.md", "no match here"),
                ("c.md", "say HELLO again"),
            ],
        )
        .await;
        let (results, _) = search_content(store.pool(), base_query("hello"))
            .await
            .unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "c.md"]);
    }

    #[tokio::test]
    async fn content_substring_case_sensitive_when_flag_set() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![("a.md", "Hello World"), ("b.md", "say HELLO again")],
        )
        .await;
        let q = ContentQuery {
            case_sensitive: true,
            ..base_query("HELLO")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["b.md"]);
    }

    #[tokio::test]
    async fn content_regex_matches_alternation() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", "use pgvector for ANN"),
                ("b.md", "sqlite-vec is portable"),
                ("c.md", "no relevant content"),
            ],
        )
        .await;
        let q = ContentQuery {
            regex: true,
            ..base_query("pgvector|sqlite-vec")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "b.md"]);
    }

    #[tokio::test]
    async fn content_regex_invalid_returns_invalid_regex_error() {
        let (_dir, store) = open_store().await;
        let q = ContentQuery {
            regex: true,
            ..base_query("[unterminated")
        };
        let err = search_content(store.pool(), q).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_regex"));
    }

    #[tokio::test]
    async fn content_regex_ignores_case_sensitive_flag() {
        // When regex is on, case_sensitive is ignored — the regex's own
        // case mode wins (default = case-sensitive).
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![("a.md", "Hello World"), ("b.md", "say HELLO again")],
        )
        .await;
        // case_sensitive=false would normally lowercase; with regex=true
        // it must be ignored, so a literal lowercase pattern only matches
        // the lowercase occurrence (none here -> no results).
        let q = ContentQuery {
            regex: true,
            case_sensitive: false,
            ..base_query("hello")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        assert!(
            results.is_empty(),
            "regex mode must not lowercase the query; got {results:?}"
        );

        // Sanity check: the case-sensitive uppercase pattern does match.
        let q = ContentQuery {
            regex: true,
            ..base_query("HELLO")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["b.md"]);
    }

    #[tokio::test]
    async fn content_match_count_reflects_full_count_not_truncated() {
        let (_dir, store) = open_store().await;
        seed(&store, vec![("a.md", "foo foo foo foo foo foo foo")]).await;
        let q = ContentQuery {
            max_matches_per_file: 2,
            ..base_query("foo")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_count, 7);
        assert_eq!(results[0].matches.len(), 2);
    }

    #[tokio::test]
    async fn content_matches_truncated_at_max_matches_per_file() {
        let (_dir, store) = open_store().await;
        seed(&store, vec![("a.md", "x x x x x x")]).await;
        let q = ContentQuery {
            max_matches_per_file: 3,
            ..base_query("x")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        assert_eq!(results[0].matches.len(), 3);
        assert_eq!(results[0].match_count, 6);
    }

    #[tokio::test]
    async fn content_truncated_at_file_limit() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("a.md", "hit"),
                ("b.md", "hit"),
                ("c.md", "hit"),
                ("d.md", "hit"),
            ],
        )
        .await;
        let q = ContentQuery {
            limit: 2,
            ..base_query("hit")
        };
        let (results, truncated) = search_content(store.pool(), q).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(truncated);
    }

    #[tokio::test]
    async fn content_phrase_spans_line_boundary() {
        // Substring matching is line-agnostic; a phrase that wraps across
        // a soft-wrap line break is still found.
        let (_dir, store) = open_store().await;
        seed(&store, vec![("a.md", "pgvector\nsupports HNSW indexes.")]).await;
        let q = ContentQuery {
            // The `\n` between words must not block the match.
            ..base_query("pgvector\nsupports")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_count, 1);
        // Anchor at the match's start line (line 1).
        assert_eq!(results[0].matches[0].line, 1);
        assert_eq!(results[0].matches[0].text, "pgvector");
    }

    #[tokio::test]
    async fn content_match_text_trimmed_at_240_bytes() {
        let (_dir, store) = open_store().await;
        let body = format!("needle {}", "a".repeat(500));
        seed(&store, vec![("a.md".to_string(), body)]).await;
        let (results, _) = search_content(store.pool(), base_query("needle"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matches[0].text.len(), TEXT_TRIM_BYTES);
        assert!(results[0].matches[0].text.starts_with("needle "));
    }

    #[tokio::test]
    async fn content_match_line_is_one_indexed() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![("a.md", "first line\nsecond line\nthird HIT here")],
        )
        .await;
        let (results, _) = search_content(store.pool(), base_query("hit"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matches[0].line, 3);
        assert_eq!(results[0].matches[0].text, "third HIT here");
    }

    #[tokio::test]
    async fn content_omits_matches_when_include_matches_false() {
        let (_dir, store) = open_store().await;
        seed(&store, vec![("a.md", "foo foo foo")]).await;
        let q = ContentQuery {
            include_matches: false,
            ..base_query("foo")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_count, 3);
        assert!(results[0].matches.is_empty());
    }

    #[tokio::test]
    async fn content_invalid_prefix_returns_invalid_prefix_error() {
        let (_dir, store) = open_store().await;
        let absolute = ContentQuery {
            prefix: Some("/abs/path".to_string()),
            ..base_query("anything")
        };
        let err = search_content(store.pool(), absolute).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_prefix"));

        let parent = ContentQuery {
            prefix: Some("notes/../escape".to_string()),
            ..base_query("anything")
        };
        let err = search_content(store.pool(), parent).await.unwrap_err();
        assert!(format!("{err:#}").starts_with("invalid_prefix"));
    }

    #[tokio::test]
    async fn content_prefix_excludes_outside_subdir() {
        let (_dir, store) = open_store().await;
        seed(
            &store,
            vec![
                ("notes/a.md", "alpha"),
                ("notesarchive/b.md", "alpha"),
                ("other/c.md", "alpha"),
            ],
        )
        .await;
        let q = ContentQuery {
            prefix: Some("notes".to_string()),
            ..base_query("alpha")
        };
        let (results, _) = search_content(store.pool(), q).await.unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, vec!["notes/a.md"]);
    }
}
