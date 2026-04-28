use rmcp::schemars;
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct FilesystemSearchResponse {
    pub results: Vec<FilesystemResultJson>,
    pub truncated: bool,
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
    #[serde(default)]
    #[schemars(
        description = "If true, `query` is interpreted as a Rust regex pattern (anchors, character classes, etc.). Catastrophic backtracking is not possible — Rust's `regex` crate is linear-time. When true, `case_sensitive` is ignored; embed `(?i)` in the pattern instead."
    )]
    pub regex: bool,
    #[serde(default)]
    #[schemars(
        description = "If true, match query case-sensitively. Ignored when `regex` is true."
    )]
    pub case_sensitive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Vault-relative path prefix to scope results to a subdirectory.")]
    pub prefix: Option<String>,
    #[serde(default = "default_include_matches")]
    #[schemars(
        description = "If true, response includes per-line match details for each file (line number + matching text). Defaults to true."
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
}

fn default_include_matches() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ContentSearchResponse {
    pub results: Vec<ContentResultJson>,
    pub truncated: bool,
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
    #[schemars(description = "Maximum number of result chunks. Defaults to 100.")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Filter results to those whose cosine similarity score is >= this value, in [0.0, 1.0]. Out-of-range values are clamped. Defaults to 0.0 (no filter)."
    )]
    pub min_similarity: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticResultJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct SemanticResultJson {
    pub score: f32,
    pub file_path: String,
    pub chunk_index: u32,
    pub heading_path: Vec<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vault_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub vault: String,
    pub indexed_file_count: u64,
    pub last_indexed_at: Option<String>,
    pub outbox: OutboxStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxStatus {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultListResponse {
    pub vaults: Vec<VaultRowJson>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateVaultResponse {
    pub terminated: bool,
    pub id: String,
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
            }],
            truncated: false,
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
            results: vec![SemanticResultJson {
                score: 0.95,
                file_path: "notes/a.md".to_string(),
                chunk_index: 0,
                heading_path: vec!["Intro".to_string()],
                text: "alpha body".to_string(),
                vault: Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string()),
                vault_name: Some("default".to_string()),
            }],
            hint: None,
        };
        let v: serde_json::Value = serde_json::to_value(&resp).unwrap();
        let entry = &v["results"][0];
        assert_eq!(
            entry["vault"].as_str(),
            Some("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0")
        );
        assert_eq!(entry["vault_name"].as_str(), Some("default"));
    }
}
