//! Semantic search over `chunks_vec` (cosine kNN) joined to `chunks` for
//! metadata. Pure-logic surface for step 7: the HTTP handler and CLI wire it
//! through; this module owns the query/embedding/SQL contract.
//!
//! Async/blocking split: the embedding call runs on the runtime (network HTTP)
//! per the [`sqlite-vec-extension`](../../.claude/skills/sqlite-vec-extension/SKILL.md)
//! smell about "embedding goes on the runtime, not in spawn_blocking"; the
//! kNN SQL runs inside `spawn_blocking` per the
//! [`rusqlite-in-async`](../../.claude/skills/rusqlite-in-async/SKILL.md) skill.
//!
//! Score conversion: sqlite-vec returns cosine *distance* in `[0, 2]`; we
//! convert to cosine *similarity* in `[0, 1]` via `1.0 - distance / 2.0` and
//! clamp as a defensive guard against floating-point edge cases per step-7
//! workplan § Resolution F.

use std::sync::Arc;

use anyhow::Context;
use rusqlite::{Connection, params};
use tokio::task;

use super::{normalize_prefix, prefix_successor};
use crate::embedding::{Embedder, EmbeddingError};
use crate::store::SqlitePool;

#[derive(Debug, Clone)]
pub struct SemanticQuery {
    pub query: String,
    pub prefix: Option<String>,
    pub limit: usize,
    pub min_similarity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticResult {
    pub score: f32,
    pub file_path: String,
    pub chunk_index: u32,
    pub heading_path: String,
    pub text: String,
    pub content_hash: String,
}

#[derive(Debug)]
pub enum SemanticSearchError {
    EmbeddingUnavailable { detail: String },
    Internal(anyhow::Error),
    InvalidPrefix(String),
}

impl std::fmt::Display for SemanticSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmbeddingUnavailable { detail } => {
                write!(f, "embedding_unavailable: {detail}")
            }
            Self::Internal(e) => write!(f, "internal: {e:#}"),
            Self::InvalidPrefix(detail) => write!(f, "invalid_prefix: {detail}"),
        }
    }
}

impl std::error::Error for SemanticSearchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Internal(e) => e.source(),
            _ => None,
        }
    }
}

const HINT_INDEX_BUILDING: &str = "semantic index is building";

pub async fn search_semantic(
    pool: SqlitePool,
    embedder: Arc<dyn Embedder>,
    dimension: u32,
    q: SemanticQuery,
) -> Result<(Vec<SemanticResult>, Option<String>, bool), SemanticSearchError> {
    let prefix = match q.prefix.as_deref() {
        Some(raw) => normalize_prefix(raw).map_err(|e| {
            let msg = format!("{e:#}");
            let detail = msg
                .strip_prefix("invalid_prefix:")
                .map(|s| s.trim().to_string())
                .unwrap_or(msg);
            SemanticSearchError::InvalidPrefix(detail)
        })?,
        None => String::new(),
    };

    let query_vec = embedder
        .embed_text(&q.query)
        .await
        .map_err(classify_embedding_error)?;

    if query_vec.len() != dimension as usize {
        return Err(SemanticSearchError::EmbeddingUnavailable {
            detail: format!(
                "embedding service returned a vector with dimension {actual}; \
                 daemon expected {expected}",
                actual = query_vec.len(),
                expected = dimension,
            ),
        });
    }

    let min_similarity = q.min_similarity.clamp(0.0, 1.0);
    let limit = q.limit;

    task::spawn_blocking(move || run_blocking_query(pool, query_vec, prefix, limit, min_similarity))
        .await
        .map_err(|e| {
            SemanticSearchError::Internal(anyhow::anyhow!(
                "spawn_blocking join error in search_semantic: {e}"
            ))
        })?
}

fn classify_embedding_error(e: EmbeddingError) -> SemanticSearchError {
    match e {
        EmbeddingError::Transport(err) => SemanticSearchError::EmbeddingUnavailable {
            detail: format!("embedding service is unreachable: {err}"),
        },
        EmbeddingError::Status { code, .. } if (500..=599).contains(&code) => {
            SemanticSearchError::EmbeddingUnavailable {
                detail: format!("embedding service returned HTTP {code}"),
            }
        }
        EmbeddingError::DimensionMismatch { expected, actual } => {
            SemanticSearchError::EmbeddingUnavailable {
                detail: format!(
                    "embedding service returned a vector with dimension {actual}; \
                     daemon expected {expected}"
                ),
            }
        }
        EmbeddingError::Status { code, body } => SemanticSearchError::Internal(anyhow::anyhow!(
            "embedding service returned unexpected HTTP {code}: {body}"
        )),
        EmbeddingError::BodyParse(err) => SemanticSearchError::Internal(anyhow::anyhow!(
            "embedding service response could not be parsed: {err}"
        )),
    }
}

fn distance_to_score(distance: f64) -> f32 {
    (1.0 - (distance as f32) / 2.0).clamp(0.0, 1.0)
}

fn run_blocking_query(
    pool: SqlitePool,
    query_vec: Vec<f32>,
    prefix: String,
    limit: usize,
    min_similarity: f32,
) -> Result<(Vec<SemanticResult>, Option<String>, bool), SemanticSearchError> {
    let conn = pool.get().map_err(|e| {
        SemanticSearchError::Internal(anyhow::anyhow!(
            "acquiring connection from pool for search_semantic: {e}"
        ))
    })?;

    let upper = if prefix.is_empty() {
        String::new()
    } else {
        prefix_successor(&prefix)
    };

    let blob = bytemuck::cast_slice::<f32, u8>(&query_vec);
    // CTE materializes the kNN candidates first, then the outer SELECT joins
    // to `chunks` and applies the prefix filter. Per the upstream docs, the
    // canonical form binds the query vector through `MATCH ?` (raw f32 byte
    // blob is accepted directly — no `vec_f32(?)` wrapper required) and binds
    // the neighbor count through `k = ?`. CTE-vs-inline is a free choice; we
    // pick the CTE form for readability.
    let sql = "WITH knn AS (
        SELECT chunk_id, distance
        FROM chunks_vec
        WHERE embedding MATCH ?1
          AND k = ?2
    )
    SELECT
        c.file_path,
        c.chunk_index,
        c.heading_path,
        c.content,
        c.content_hash,
        knn.distance
    FROM knn
    JOIN chunks c ON c.id = knn.chunk_id
    WHERE (?3 = '' OR (c.file_path >= ?3 AND c.file_path < ?4))
    ORDER BY knn.distance ASC, c.file_path ASC, c.chunk_index ASC";

    let mut stmt = conn
        .prepare(sql)
        .context("preparing semantic kNN query")
        .map_err(SemanticSearchError::Internal)?;

    let rows: Vec<SemanticResult> = stmt
        .query_map(params![blob, limit as i64, &prefix, &upper], |row| {
            let file_path: String = row.get(0)?;
            let chunk_index: i64 = row.get(1)?;
            let heading_path: String = row.get(2)?;
            let content: String = row.get(3)?;
            let content_hash: String = row.get(4)?;
            let distance: f64 = row.get(5)?;
            Ok(SemanticResult {
                score: distance_to_score(distance),
                file_path,
                chunk_index: chunk_index as u32,
                heading_path,
                text: content,
                content_hash,
            })
        })
        .context("executing semantic kNN query")
        .map_err(SemanticSearchError::Internal)?
        .collect::<Result<Vec<_>, _>>()
        .context("collecting semantic kNN rows")
        .map_err(SemanticSearchError::Internal)?;

    let rows_count = rows.len();
    let filtered: Vec<SemanticResult> = rows
        .into_iter()
        .filter(|r| r.score >= min_similarity)
        .collect();

    let was_capped = rows_count >= limit;

    if filtered.is_empty() {
        let hint = decide_hint(&conn).map_err(SemanticSearchError::Internal)?;
        return Ok((filtered, hint, was_capped));
    }

    Ok((filtered, None, was_capped))
}

fn decide_hint(conn: &Connection) -> anyhow::Result<Option<String>> {
    let chunks_vec_count: i64 = conn
        .query_row("SELECT count(*) FROM chunks_vec", [], |row| row.get(0))
        .context("counting chunks_vec rows for empty-result hint")?;
    let files_count: i64 = conn
        .query_row("SELECT count(*) FROM files", [], |row| row.get(0))
        .context("counting files rows for empty-result hint")?;
    if chunks_vec_count == 0 && files_count > 0 {
        Ok(Some(HINT_INDEX_BUILDING.to_string()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use rusqlite::params;
    use tempfile::tempdir;
    use tokio::task;

    use super::*;
    use crate::config::EmbeddingConfig;
    use crate::embedding::{EmbedFuture, EmbeddingError};
    use crate::store::Store;

    const DIM: usize = 768;

    fn base_query(query: &str) -> SemanticQuery {
        SemanticQuery {
            query: query.to_string(),
            prefix: None,
            limit: 10,
            min_similarity: 0.0,
        }
    }

    /// Test embedder that yields a single result on the first call.
    /// Construct fresh per test; each test issues exactly one `search_semantic`
    /// invocation, which calls `embed_text` exactly once.
    struct OneShotEmbedder {
        slot: Mutex<Option<Result<Vec<f32>, EmbeddingError>>>,
    }

    impl OneShotEmbedder {
        fn ok(v: Vec<f32>) -> Arc<Self> {
            Arc::new(Self {
                slot: Mutex::new(Some(Ok(v))),
            })
        }
        fn err(e: EmbeddingError) -> Arc<Self> {
            Arc::new(Self {
                slot: Mutex::new(Some(Err(e))),
            })
        }
    }

    impl Embedder for OneShotEmbedder {
        fn embed_text<'a>(&'a self, _text: &'a str) -> EmbedFuture<'a> {
            let r = self
                .slot
                .lock()
                .unwrap()
                .take()
                .expect("OneShotEmbedder called more than once");
            Box::pin(async move { r })
        }
    }

    fn unit_vec(positions: &[(usize, f32)]) -> Vec<f32> {
        let mut v = vec![0.0f32; DIM];
        for (i, x) in positions {
            v[*i] = *x;
        }
        v
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

    async fn seed_file(store: &Store, path: &'static str) {
        let pool = store.pool();
        task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            conn.execute(
                "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    path,
                    1i64,
                    "2026-01-01T00:00:00Z",
                    "sha256:00",
                    "2026-01-01T00:00:00Z",
                    ""
                ],
            )
            .unwrap();
        })
        .await
        .unwrap();
    }

    async fn seed_chunk(
        store: &Store,
        file_path: &'static str,
        chunk_index: u32,
        heading_path: &'static str,
        content: &'static str,
        embedding: Vec<f32>,
    ) {
        let pool = store.pool();
        task::spawn_blocking(move || {
            let mut conn = pool.get().unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    file_path,
                    chunk_index,
                    heading_path,
                    content,
                    "sha256:00",
                    0i64,
                    content.len() as i64,
                    "2026-01-01T00:00:00Z"
                ],
            ).unwrap();
            let chunk_id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO chunks_vec (chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk_id, bytemuck::cast_slice::<f32, u8>(&embedding)],
            ).unwrap();
            tx.commit().unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn search_semantic_returns_results_for_known_chunks() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;
        seed_chunk(
            &store,
            "a.md",
            0,
            "Intro",
            "alpha body",
            unit_vec(&[(0, 1.0)]),
        )
        .await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let (results, hint, _truncated) =
            search_semantic(store.pool(), embedder, 768, base_query("alpha"))
                .await
                .expect("search ok");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "a.md");
        assert_eq!(results[0].chunk_index, 0);
        assert_eq!(results[0].heading_path, "Intro");
        assert_eq!(results[0].text, "alpha body");
        assert_eq!(results[0].content_hash, "sha256:00");
        // Identical vectors → cosine distance 0 → score 1.0.
        assert!(
            (results[0].score - 1.0).abs() < 1e-6,
            "expected score ≈ 1.0, got {}",
            results[0].score
        );
        assert!(hint.is_none(), "no hint when results are non-empty");
    }

    #[tokio::test]
    async fn search_semantic_returns_hint_when_chunks_vec_empty_and_files_present() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let (results, hint, _truncated) =
            search_semantic(store.pool(), embedder, 768, base_query("alpha"))
                .await
                .expect("search ok");

        assert!(results.is_empty());
        assert_eq!(hint.as_deref(), Some(HINT_INDEX_BUILDING));
    }

    #[tokio::test]
    async fn search_semantic_returns_no_hint_when_files_empty() {
        let (_dir, store) = open_store().await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let (results, hint, _truncated) =
            search_semantic(store.pool(), embedder, 768, base_query("alpha"))
                .await
                .expect("search ok");

        assert!(results.is_empty());
        assert!(hint.is_none(), "no hint when both tables empty");
    }

    #[tokio::test]
    async fn search_semantic_returns_no_hint_when_chunks_present_but_no_match() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;
        // Orthogonal chunk: cos_sim 0, distance 1, score 0.5.
        seed_chunk(&store, "a.md", 0, "", "body", unit_vec(&[(1, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            min_similarity: 0.6,
            ..base_query("alpha")
        };
        let (results, hint, _truncated) = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect("search ok");

        assert!(results.is_empty(), "0.5 < 0.6 filter, no results");
        assert!(
            hint.is_none(),
            "no hint when chunks_vec has rows but min_similarity filtered them out"
        );
    }

    #[tokio::test]
    async fn search_semantic_respects_limit() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;
        seed_file(&store, "b.md").await;
        seed_file(&store, "c.md").await;
        seed_chunk(&store, "a.md", 0, "", "a", unit_vec(&[(0, 1.0)])).await;
        seed_chunk(&store, "b.md", 0, "", "b", unit_vec(&[(0, 1.0)])).await;
        seed_chunk(&store, "c.md", 0, "", "c", unit_vec(&[(0, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            limit: 2,
            ..base_query("alpha")
        };
        let (results, _, _) = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect("search ok");

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn search_semantic_respects_min_similarity() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;
        seed_file(&store, "b.md").await;
        // Identical chunk → score 1.0; orthogonal chunk → score 0.5.
        seed_chunk(&store, "a.md", 0, "", "a", unit_vec(&[(0, 1.0)])).await;
        seed_chunk(&store, "b.md", 0, "", "b", unit_vec(&[(1, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            min_similarity: 0.6,
            ..base_query("alpha")
        };
        let (results, _, _) = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect("search ok");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "a.md");
    }

    #[tokio::test]
    async fn search_semantic_clamps_min_similarity_negatives_to_zero() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "a.md").await;
        seed_chunk(&store, "a.md", 0, "", "a", unit_vec(&[(1, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            min_similarity: -1.0,
            ..base_query("alpha")
        };
        let (results, _, _) = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect("search ok");

        // -1.0 clamps to 0.0 → all kNN candidates returned (orthogonal scores 0.5).
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn search_semantic_respects_prefix_scoping() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "notes/a.md").await;
        seed_file(&store, "archive/b.md").await;
        seed_chunk(&store, "notes/a.md", 0, "", "n", unit_vec(&[(0, 1.0)])).await;
        seed_chunk(&store, "archive/b.md", 0, "", "a", unit_vec(&[(0, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            prefix: Some("notes/".to_string()),
            ..base_query("alpha")
        };
        let (results, _, _) = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect("search ok");

        let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
        assert_eq!(paths, vec!["notes/a.md"]);
    }

    #[tokio::test]
    async fn search_semantic_orders_ties_deterministically() {
        let (_dir, store) = open_store().await;
        seed_file(&store, "z.md").await;
        seed_file(&store, "a.md").await;
        // Byte-identical embeddings under different files → kNN distances tie
        // → secondary sort by file_path ASC.
        seed_chunk(&store, "z.md", 0, "", "z", unit_vec(&[(0, 1.0)])).await;
        seed_chunk(&store, "a.md", 0, "", "a", unit_vec(&[(0, 1.0)])).await;

        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let (results, _, _) = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect("search ok");

        let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "z.md"]);
    }

    /// Fabricate a real `reqwest::Error` of the Transport kind by sending to
    /// a port nothing is listening on. Mirrors `embed_retries_once_on_connection_refused`
    /// in `src/embedding.rs::tests`.
    async fn make_transport_error() -> reqwest::Error {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        // 127.0.0.1:1 — privileged port, nothing listens. Reliably yields a
        // connection-refused transport error.
        client
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .expect_err("connect to closed port should fail")
    }

    #[tokio::test]
    async fn search_semantic_classifies_embedding_transport_error() {
        let (_dir, store) = open_store().await;
        let transport_err = make_transport_error().await;
        let embedder = OneShotEmbedder::err(EmbeddingError::Transport(transport_err));
        let err = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect_err("transport classifies as embedding_unavailable");
        match err {
            SemanticSearchError::EmbeddingUnavailable { detail } => {
                assert!(
                    detail.contains("embedding service is unreachable"),
                    "detail: {detail}"
                );
            }
            other => panic!("expected EmbeddingUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_semantic_classifies_embedding_5xx() {
        let (_dir, store) = open_store().await;
        let embedder = OneShotEmbedder::err(EmbeddingError::Status {
            code: 503,
            body: "service unavailable".to_string(),
        });
        let err = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect_err("5xx classifies as embedding_unavailable");
        match err {
            SemanticSearchError::EmbeddingUnavailable { detail } => {
                assert!(detail.contains("HTTP 503"), "detail: {detail}");
            }
            other => panic!("expected EmbeddingUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_semantic_classifies_embedding_dimension_mismatch() {
        let (_dir, store) = open_store().await;
        // Stub returns 4 floats; daemon expects 768 → defense-in-depth assert
        // in `search_semantic` triggers `EmbeddingUnavailable`.
        let embedder = OneShotEmbedder::ok(vec![0.0_f32; 4]);
        let err = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect_err("wrong-dim vector classifies as embedding_unavailable");
        match err {
            SemanticSearchError::EmbeddingUnavailable { detail } => {
                assert!(detail.contains("dimension 4"), "detail: {detail}");
                assert!(detail.contains("768"), "detail: {detail}");
            }
            other => panic!("expected EmbeddingUnavailable, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_semantic_classifies_4xx_as_internal() {
        let (_dir, store) = open_store().await;
        let embedder = OneShotEmbedder::err(EmbeddingError::Status {
            code: 400,
            body: "bad request".to_string(),
        });
        let err = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect_err("4xx classifies as internal");
        match err {
            SemanticSearchError::Internal(_) => {}
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_semantic_classifies_body_parse_as_internal() {
        let (_dir, store) = open_store().await;
        let parse_err =
            serde_json::from_str::<serde_json::Value>("not json").expect_err("invalid JSON");
        let embedder = OneShotEmbedder::err(EmbeddingError::BodyParse(parse_err));
        let err = search_semantic(store.pool(), embedder, 768, base_query("alpha"))
            .await
            .expect_err("body parse classifies as internal");
        match err {
            SemanticSearchError::Internal(_) => {}
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_semantic_invalid_prefix() {
        let (_dir, store) = open_store().await;
        let embedder = OneShotEmbedder::ok(unit_vec(&[(0, 1.0)]));
        let q = SemanticQuery {
            prefix: Some("/abs".to_string()),
            ..base_query("alpha")
        };
        let err = search_semantic(store.pool(), embedder, 768, q)
            .await
            .expect_err("absolute prefix is invalid");
        match err {
            SemanticSearchError::InvalidPrefix(detail) => {
                assert!(detail.contains("absolute"), "detail: {detail}");
            }
            other => panic!("expected InvalidPrefix, got {other:?}"),
        }
    }
}
