mod content;
mod filesystem;
mod semantic;

use anyhow::{Result, anyhow};

pub use content::{ContentGetRow, ContentMatch, ContentQuery, ContentResult, content_get_by_paths, search_content};
pub use filesystem::{FilesystemQuery, FilesystemResult, search_filesystem};
pub use semantic::{SemanticQuery, SemanticResult, SemanticSearchError, search_semantic};

pub(crate) fn normalize_prefix(raw: &str) -> Result<String> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    if raw.starts_with('/') {
        return Err(anyhow!("invalid_prefix: absolute paths are not allowed"));
    }
    if raw.split('/').any(|seg| seg == "..") {
        return Err(anyhow!("invalid_prefix: '..' segments are not allowed"));
    }
    if raw.ends_with('/') {
        Ok(raw.to_string())
    } else {
        Ok(format!("{raw}/"))
    }
}

// Successor of a normalized non-empty prefix, used as the exclusive upper
// bound of a SQLite range scan. Our normalized prefix always ends with '/'
// (0x2F), so incrementing the trailing byte yields valid UTF-8 ('0', 0x30)
// and excludes anything sharing the prefix's last segment but extending
// past it (e.g. `notesarchive/...` when the prefix is `notes/`).
pub(crate) fn prefix_successor(prefix: &str) -> String {
    debug_assert!(!prefix.is_empty());
    debug_assert!(prefix.ends_with('/'));
    let mut bytes = prefix.as_bytes().to_vec();
    *bytes.last_mut().expect("non-empty per debug_assert") += 1;
    String::from_utf8(bytes).expect("incrementing trailing '/' yields valid UTF-8")
}
