use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FilesystemQueryJson {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilesystemSearchResponse {
    pub results: Vec<FilesystemResultJson>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilesystemResultJson {
    pub path: String,
    pub size: i64,
    pub mtime: String,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ContentQueryJson {
    pub query: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default = "default_include_matches")]
    pub include_matches: bool,
    #[serde(default)]
    pub max_matches_per_file: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn default_include_matches() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentSearchResponse {
    pub results: Vec<ContentResultJson>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentResultJson {
    pub path: String,
    pub match_count: usize,
    pub matches: Vec<ContentMatchJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContentMatchJson {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusResponse {
    pub vault: String,
    pub indexed_file_count: u64,
    pub last_indexed_at: Option<String>,
    pub outbox: OutboxStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutboxStatus {
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}
