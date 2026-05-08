use chrono::SecondsFormat;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::control_plane::RescanResponse;
use crate::vault_registry::VaultRow;

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FilesystemQueryJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Vault-relative path prefix to scope results to a subdirectory (e.g. \"notes/databases/\"). Trailing `/` is normalized; absolute paths and `..` segments are rejected."
    )]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Glob pattern over vault paths (e.g. \"**/*.md\"). Single-pattern only; v0 does not support multi-pattern unions."
    )]
    pub glob: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Maximum directory depth to descend, relative to `prefix` (or vault root if no prefix). Unbounded if omitted."
    )]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Maximum number of results. Defaults to 100; results beyond this are truncated and the response carries `truncated: true`."
    )]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Restrict the search to a subset of vaults. Each entry matches against vault name first, then surrogate id. Unknown entries land in `partial_results.failed`. Omitting (or `null`) queries all currently active vaults; `[]` is rejected with `invalid_request`."
    )]
    pub vaults: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FilesystemSearchResponse {
    pub results: Vec<FilesystemResultJson>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_results: Option<PartialResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FilesystemResultJson {
    pub path: String,
    pub size: i64,
    pub mtime: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentQueryJson {
    #[schemars(
        description = "Substring or regex to match against file contents. ASCII case-insensitive by default; see `case_sensitive` and `regex`."
    )]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Search mode: `\"substring\"` (default), `\"regex\"`, or `\"ranked\"`. Ranked uses FTS5/BM25 relevance ordering; results include `score` and `rank` fields. Sending both `mode` and the legacy `regex: true` flag is rejected with `invalid_request`."
    )]
    pub mode: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Legacy flag: if true, `query` is interpreted as a Rust regex pattern. Equivalent to `mode: \"regex\"`; cannot be combined with an explicit `mode`. Prefer `mode` for new callers."
    )]
    pub regex: bool,
    #[serde(default)]
    #[schemars(
        description = "If true, match query case-sensitively. Ignored when `regex` is true or `mode` is `\"ranked\"` (ranked search is always case-insensitive via the porter tokenizer)."
    )]
    pub case_sensitive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory.")]
    pub prefix: Option<String>,
    #[serde(default = "default_include_matches")]
    #[schemars(
        description = "If true, response includes per-line match details for each file (line number + matching text). Defaults to false; opt in explicitly to receive snippets."
    )]
    pub include_matches: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Maximum match details returned per file when `include_matches` is true. Defaults to 5; `match_count` always reports the full match count."
    )]
    pub max_matches_per_file: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Maximum number of result files. Defaults to 100.")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Restrict the search to a subset of vaults. Each entry matches against vault name first, then surrogate id. Unknown entries land in `partial_results.failed`. Omitting (or `null`) queries all currently active vaults; `[]` is rejected with `invalid_request`."
    )]
    pub vaults: Option<Vec<String>>,
}

fn default_include_matches() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentSearchResponse {
    pub results: Vec<ContentResultJson>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_results: Option<PartialResults>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentResultJson {
    pub path: String,
    pub match_count: usize,
    pub matches: Vec<ContentMatchJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_name: Option<String>,
    /// BM25 relevance score (negative; lower = better match). Present only for ranked-mode results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// 1-indexed ordinal position in the ranked result set. Present only for ranked-mode results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentMatchJson {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticQueryJson {
    #[schemars(
        description = "Natural-language query. Embedded via the daemon's configured embedding service and compared against indexed chunk vectors by cosine similarity."
    )]
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory.")]
    pub prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Maximum number of result chunks. Defaults to 10.")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Filter results to those whose cosine similarity score is >= this value, in [0.0, 1.0]. Out-of-range values are clamped. Defaults to 0.0 (no filter)."
    )]
    pub min_similarity: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Restrict the search to a subset of vaults. Each entry matches against vault name first, then surrogate id. Unknown entries land in `partial_results.failed`. Omitting (or `null`) queries all currently active vaults; `[]` is rejected with `invalid_request`."
    )]
    pub vaults: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "How much chunk text to include with each result. One of `preview` (default), `full`, or `none`. `preview` returns up to `preview_bytes` bytes of the chunk text; `full` returns the complete chunk text; `none` omits the text field entirely."
    )]
    pub include_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Maximum bytes of chunk text returned when `include_text` is `preview`. Defaults to 600; server maximum is 2000 (values above this are clamped silently, not rejected). Ignored when `include_text` is `full` or `none`."
    )]
    pub preview_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Result granularity: `\"document\"` (default) groups results by parent document and returns bounded evidence chunks per document; `\"chunk\"` returns flat individual chunk results. `chunks_per_document` is accepted but ignored in chunk mode."
    )]
    pub granularity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Maximum evidence chunks per document result when `granularity` is `\"document\"`. Ignored in chunk mode. Must be in 1..=100; defaults to the daemon config value (built-in default: 3)."
    )]
    pub chunks_per_document: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticResultItem>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_results: Option<PartialResults>,
}

/// Untagged result union: the effective `granularity` determines which variant
/// appears. `Chunk` matches the pre-Step-25 flat shape; `Document` is the new
/// grouped shape. Both variants are present in the schema so callers can
/// prepare for either.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum SemanticResultItem {
    Chunk(SemanticResultJson),
    Document(SemanticDocumentResultJson),
}

/// Document-granularity result: one entry per matched document, scored by its
/// highest-scoring candidate chunk, with bounded evidence chunks nested inside.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticDocumentResultJson {
    pub score: f32,
    pub file_path: String,
    pub content_hash: String,
    pub chunks: Vec<SemanticEvidenceChunkJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_name: Option<String>,
}

/// One evidence chunk nested inside a document-granularity result.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticEvidenceChunkJson {
    pub score: f32,
    pub chunk_index: u32,
    pub heading_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticResultJson {
    pub score: f32,
    pub file_path: String,
    pub chunk_index: u32,
    pub heading_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_truncated: Option<bool>,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_name: Option<String>,
}

// ===== content_get request/response types =====

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentGetRequest {
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vaults: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
#[serde(untagged)]
pub enum ContentGetResultItem {
    Success(ContentGetSuccess),
    Error(ContentGetError),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentGetSuccess {
    pub path: String,
    pub content: String,
    pub content_hash: String,
    pub size: i64,
    pub mtime: String,
    pub vault: String,
    pub vault_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentGetError {
    pub path: String,
    pub vault: String,
    pub vault_name: String,
    pub error: ContentGetErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentGetErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentGetResponse {
    pub results: Vec<ContentGetResultItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_results: Option<PartialResults>,
}

// ===== Cross-vault partial-results diagnostics =====
//
// Pinned to docs/specs/vault-management.md § Cross-Vault Search Semantics §
// Partial-failure handling (workplan § A.6/A.7/A.8). Embedded as
// `partial_results: Option<...>` on every search response envelope; absent
// from the wire when no per-vault search was skipped or failed.
//
// `skipped` is for *intentional* exclusions (paused/errored vaults).
// `failed` is for *unexpected* runtime errors (per-vault search failure,
// unknown name in `vaults` filter).

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[schemars(crate = "rmcp::schemars")]
pub struct PartialResults {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedVault>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedVault>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[schemars(crate = "rmcp::schemars")]
pub struct SkippedVault {
    pub vault: String,
    pub vault_name: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[schemars(crate = "rmcp::schemars")]
pub struct FailedVault {
    pub vault: String,
    pub vault_name: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub vault: String,
    pub indexed_file_count: u64,
    pub last_indexed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub vaults_active: u64,
    pub vaults_errored: u64,
    pub uptime_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<EmbeddingHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingHealth {
    pub status: String,
    pub endpoint: String,
}

// ===== Vault control-plane request/response shapes =====
//
// Wire shapes pinned to docs/specs/vault-management.md § Control-Plane HTTP
// Wire Shapes. Step 10 ships the minimal `VaultRow` projection (no
// `file_count` / `last_indexed_at`); the spec marks those fields optional.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRowJson {
    pub id: String,
    pub name: String,
    pub path: String,
    pub status: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl From<VaultRow> for VaultRowJson {
    fn from(row: VaultRow) -> Self {
        VaultRowJson {
            id: row.id.to_string(),
            name: row.name,
            path: row.path.display().to_string(),
            status: row.status.as_str().to_string(),
            created_at: row.created_at.to_rfc3339_opts(SecondsFormat::Micros, true),
            last_error: row.last_error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultListResponse {
    pub vaults: Vec<VaultRowJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateVaultResponse {
    pub terminated: bool,
    pub id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResetRequest {
    #[serde(default)]
    pub rebuild: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenameRequest {
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RescanResponseJson {
    #[serde(flatten)]
    pub row: VaultRowJson,
    pub rescan_initiated_at: String,
}

impl From<RescanResponse> for RescanResponseJson {
    fn from(resp: RescanResponse) -> Self {
        RescanResponseJson {
            row: VaultRowJson::from(resp.row),
            rescan_initiated_at: resp
                .rescan_initiated_at
                .to_rfc3339_opts(SecondsFormat::Micros, true),
        }
    }
}

// ===== MCP-tool input shapes for vault control-plane wrappers =====
//
// Used by `src/mcp/server.rs` for the `vault_status` / `vault_create` /
// `vault_terminate` MCP tools. Mirrors the HTTP control-plane wire shapes,
// rephrased for MCP ergonomics (e.g. `target` rather than path-segment
// addressing, `name` defaults to config-level `default_vault_name`).

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultStatusInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Vault name or surrogate id. Defaults to the daemon's configured default vault when omitted."
    )]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultCreateInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Optional vault name. Defaults to the daemon's configured default vault name when omitted."
    )]
    pub name: Option<String>,
    #[schemars(
        description = "Absolute path to the vault directory. Must exist and be a directory; the daemon never creates the directory itself."
    )]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultTerminateInput {
    #[schemars(
        description = "Vault name or surrogate id of the vault to terminate. Permanently removes the registry row, per-vault index, and event log; never touches the vault directory itself."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultPauseInput {
    #[schemars(
        description = "Vault name or surrogate id of the vault to pause. Stops watcher and indexer; index preserved; vault is silently skipped from default search scope."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultResumeInput {
    #[schemars(
        description = "Vault name or surrogate id of the vault to resume. Restarts watcher and indexer; clears last_error if the vault was errored."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultResetInput {
    #[schemars(
        description = "Vault name or surrogate id of the vault to reset. Clears last_error and restarts watcher + indexer."
    )]
    pub target: String,
    #[serde(default)]
    #[schemars(
        description = "When true, also drop and rebuild chunks + chunks_vec (preserves files). Defaults to false."
    )]
    pub rebuild: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultRenameInput {
    #[schemars(description = "Vault name or surrogate id of the vault to rename.")]
    pub target: String,
    #[schemars(
        description = "New vault name. Must match [A-Za-z0-9_-]+ and not collide with another vault's name."
    )]
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct VaultRescanInput {
    #[schemars(
        description = "Vault name or surrogate id of the vault to rescan. Triggers a one-shot scanner pass; emits modified events for files whose stat or content_hash changed."
    )]
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::schemars::schema_for;

    fn props(schema: &rmcp::schemars::Schema) -> &serde_json::Map<String, serde_json::Value> {
        schema
            .as_object()
            .and_then(|o| o.get("properties"))
            .and_then(|p| p.as_object())
            .expect("schema has properties")
    }

    fn description(props: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
        props
            .get(key)
            .and_then(|v| v.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn filesystem_query_json_schema_has_expected_properties() {
        let schema = schema_for!(FilesystemQueryJson);
        let p = props(&schema);
        for key in ["prefix", "glob", "max_depth", "limit"] {
            assert!(p.contains_key(key), "missing property: {key}");
            assert!(
                !description(p, key).is_empty(),
                "empty description for: {key}"
            );
        }
    }

    #[test]
    fn content_query_json_schema_has_query_required_others_optional() {
        let schema = schema_for!(ContentQueryJson);
        let required: Vec<String> = schema
            .as_object()
            .and_then(|o| o.get("required"))
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(required, vec!["query".to_string()]);
    }

    #[test]
    fn semantic_query_json_schema_has_min_similarity() {
        let schema = schema_for!(SemanticQueryJson);
        let p = props(&schema);
        assert!(p.contains_key("min_similarity"));
        assert!(!description(p, "min_similarity").is_empty());
    }

    #[test]
    fn filesystem_search_response_json_schema_serializes() {
        let schema = schema_for!(FilesystemSearchResponse);
        assert!(schema.as_object().is_some_and(|o| !o.is_empty()));
    }

    #[test]
    fn filesystem_search_response_serializes_vault_and_vault_name() {
        // Per workplan § Task 9.6 + Resolution F: every step-9 search result
        // carries `vault` (id) and `vault_name`. Pin the populated wire shape
        // at the serde level so any drift in the per-result struct surfaces
        // here, independent of handler-level annotation.
        let resp = FilesystemSearchResponse {
            results: vec![FilesystemResultJson {
                path: "notes/a.md".to_string(),
                size: 5,
                mtime: "2026-04-25T00:00:00Z".to_string(),
                content_hash: "sha256:00".to_string(),
                vault: Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string()),
                vault_name: Some("default".to_string()),
            }],
            truncated: false,
            partial_results: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        let entry = &v["results"][0];
        assert_eq!(
            entry["vault"].as_str(),
            Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0")
        );
        assert_eq!(entry["vault_name"].as_str(), Some("default"));
    }

    #[test]
    fn content_search_response_serializes_vault_and_vault_name() {
        let resp = ContentSearchResponse {
            results: vec![ContentResultJson {
                path: "notes/a.md".to_string(),
                match_count: 1,
                matches: vec![ContentMatchJson {
                    line: 1,
                    text: "alpha".to_string(),
                }],
                vault: Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string()),
                vault_name: Some("default".to_string()),
                score: None,
                rank: None,
            }],
            truncated: false,
            partial_results: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        let entry = &v["results"][0];
        assert_eq!(
            entry["vault"].as_str(),
            Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0")
        );
        assert_eq!(entry["vault_name"].as_str(), Some("default"));
    }

    #[test]
    fn semantic_search_response_serializes_vault_and_vault_name() {
        let resp = SemanticSearchResponse {
            results: vec![SemanticResultItem::Chunk(SemanticResultJson {
                score: 0.95,
                file_path: "notes/a.md".to_string(),
                chunk_index: 0,
                heading_path: vec!["Intro".to_string()],
                text: Some("alpha body".to_string()),
                text_kind: None,
                text_truncated: None,
                content_hash: "sha256:00".to_string(),
                vault: Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string()),
                vault_name: Some("default".to_string()),
            })],
            truncated: false,
            hint: None,
            partial_results: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        let entry = &v["results"][0];
        assert_eq!(
            entry["vault"].as_str(),
            Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0")
        );
        assert_eq!(entry["vault_name"].as_str(), Some("default"));
        assert_eq!(entry["content_hash"].as_str(), Some("sha256:00"));
    }

    #[test]
    fn semantic_query_json_accepts_granularity_and_chunks_per_document() {
        let schema = schema_for!(SemanticQueryJson);
        let p = props(&schema);
        assert!(p.contains_key("granularity"), "missing granularity field");
        assert!(
            !description(p, "granularity").is_empty(),
            "granularity has no description"
        );
        assert!(
            p.contains_key("chunks_per_document"),
            "missing chunks_per_document field"
        );
        assert!(
            !description(p, "chunks_per_document").is_empty(),
            "chunks_per_document has no description"
        );
    }

    #[test]
    fn semantic_document_result_serializes_correctly() {
        let resp = SemanticSearchResponse {
            results: vec![SemanticResultItem::Document(SemanticDocumentResultJson {
                score: 0.92,
                file_path: "notes/b.md".to_string(),
                content_hash: "sha256:01".to_string(),
                chunks: vec![SemanticEvidenceChunkJson {
                    score: 0.92,
                    chunk_index: 1,
                    heading_path: vec!["Section".to_string()],
                    text: Some("evidence text".to_string()),
                    text_kind: Some("preview".to_string()),
                    text_truncated: Some(false),
                }],
                vault: Some("018f3a7c-0000-0000-0000-000000000001".to_string()),
                vault_name: Some("myvault".to_string()),
            })],
            truncated: false,
            hint: None,
            partial_results: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        let entry = &v["results"][0];
        // f32 serialized through JSON does not equal the f64 constant exactly;
        // verify the key is present and plausible rather than exact.
        assert!(
            entry["score"]
                .as_f64()
                .is_some_and(|s| (s - 0.92_f64).abs() < 0.01),
            "document score should be ~0.92"
        );
        assert_eq!(entry["file_path"].as_str(), Some("notes/b.md"));
        assert_eq!(entry["content_hash"].as_str(), Some("sha256:01"));
        assert_eq!(entry["vault_name"].as_str(), Some("myvault"));
        let chunk = &entry["chunks"][0];
        assert!(
            chunk["score"]
                .as_f64()
                .is_some_and(|s| (s - 0.92_f64).abs() < 0.01),
            "chunk score should be ~0.92"
        );
        assert_eq!(chunk["chunk_index"].as_u64(), Some(1));
        assert_eq!(chunk["text"].as_str(), Some("evidence text"));
    }
}
