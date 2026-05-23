use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hypomnema::config::{Config, SemanticSearchConfig, WatcherConfig};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_tmp(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time before unix epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!("hypomnema-test-{label}-{pid}-{nanos}-{n}"));
    fs::create_dir_all(&dir).expect("create unique tempdir");
    dir
}

struct ScopedTmp(PathBuf);

impl Drop for ScopedTmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_config(root: &std::path::Path, body: &str) -> PathBuf {
    let cfg_path = root.join("config.toml");
    fs::write(&cfg_path, body).expect("write config");
    cfg_path
}

#[test]
fn default_round_trip() {
    let root = ScopedTmp(unique_tmp("default-roundtrip"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", vault.display()));

    let config = Config::load(Some(&cfg_path)).expect("default round-trip");

    assert_eq!(config.http.bind, "127.0.0.1:7777");
    assert_eq!(config.mcp.transport, "stdio");
    assert_eq!(
        config.embedding.endpoint,
        "http://127.0.0.1:8080/v1/embeddings"
    );
    assert_eq!(config.embedding.model, "nomic-embed-text-v1.5");
    assert_eq!(config.embedding.dimension, 768);
    assert_eq!(config.embedding.api_key, "");
    assert_eq!(config.watcher.debounce_ms, 500);
    assert!(
        config
            .watcher
            .ignore_patterns
            .iter()
            .any(|s| s == ".obsidian/**")
    );
    assert_eq!(config.storage.index_file, "index.sqlite");
    assert_eq!(config.logging.level, "info");
    assert_eq!(config.logging.notify_level, "warn");
    assert_eq!(config.logging.tokio_level, "error");
}

#[test]
fn rejects_unknown_top_level_key() {
    let root = ScopedTmp(unique_tmp("unknown-top"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!("vault = \"{}\"\nbogus_key = true\n", vault.display()),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("expected unknown-key rejection");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("bogus_key"),
        "error must name unknown key: {msg}"
    );
}

#[test]
fn rejects_unknown_nested_key() {
    let root = ScopedTmp(unique_tmp("unknown-nested"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[logging]\nnonsense = \"x\"\n",
            vault.display()
        ),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("expected unknown-nested-key rejection");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("nonsense"),
        "error must name unknown key: {msg}"
    );
}

#[test]
fn tilde_expands_for_paths() {
    // HOME is required for this test to exercise tilde expansion.
    let Ok(home) = env::var("HOME") else {
        eprintln!("HOME not set; skipping tilde expansion test");
        return;
    };

    let root = ScopedTmp(unique_tmp("tilde-expansion"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    // Use a HOME-relative data_dir that won't exist (validation only checks
    // ancestor existence, not the leaf), and isn't under the tmp vault.
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"~/.hypomnema-tilde-test-{}-{}\"\n",
            vault.display(),
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst),
        ),
    );

    let config = Config::load(Some(&cfg_path)).expect("tilde config should load");
    let dd = config.storage.data_dir.0.to_string_lossy().to_string();
    assert!(!dd.starts_with('~'), "~ should be expanded, got {dd}");
    assert!(
        dd.starts_with(&home),
        "expanded path should start with HOME ({home}), got {dd}"
    );
}

#[test]
fn rejects_data_dir_under_vault() {
    let root = ScopedTmp(unique_tmp("dd-under-vault"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let bad_data_dir = vault.join("data");
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            bad_data_dir.display()
        ),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("data_dir") || msg.contains("ADR-0006"),
        "error must mention data_dir or ADR-0006: {msg}"
    );
}

#[test]
fn rejects_data_dir_equal_to_vault() {
    let root = ScopedTmp(unique_tmp("dd-equals-vault"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            vault.display()
        ),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection when data_dir == vault");
    let msg = format!("{err:#}");
    assert!(msg.contains("data_dir"), "{msg}");
}

#[test]
fn rejects_bad_logging_level() {
    let root = ScopedTmp(unique_tmp("bad-log-level"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[logging]\nlevel = \"plaid\"\n",
            vault.display()
        ),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("plaid") || msg.contains("logging.level"),
        "{msg}"
    );
}

#[test]
fn accepts_missing_legacy_vault_for_migration_to_errored() {
    // Step 9 onward: an inaccessible legacy [vault] path no longer fails
    // config validation. The legacy-state migration handles it by inserting
    // a registry row in `errored` status (Resolution E Case 1). Config::load
    // therefore succeeds and leaves the path as-given.
    let root = ScopedTmp(unique_tmp("missing-vault"));
    let absent = root.0.join("does-not-exist");
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", absent.display()));

    let cfg = Config::load(Some(&cfg_path)).expect("inaccessible legacy [vault] must parse");
    assert_eq!(
        cfg.vault.as_ref().map(|c| c.0.clone()),
        Some(absent),
        "vault path is preserved as-given when canonicalize fails"
    );
}

#[test]
fn default_ignore_patterns_include_dot_git() {
    let cfg = WatcherConfig::default();
    assert!(
        cfg.ignore_patterns.iter().any(|p| p == ".git/**"),
        "default ignore_patterns must include .git/**: {:?}",
        cfg.ignore_patterns
    );
}

#[test]
fn compiled_ignores_matches_defaults() {
    let set = WatcherConfig::default()
        .compiled_ignores()
        .expect("default ignore patterns must compile");

    assert!(set.is_match(".git/objects/abc"), ".git/** should match");
    assert!(set.is_match(".git/HEAD"), ".git/** should match .git/HEAD");
    assert!(
        set.is_match(".obsidian/workspace.json"),
        ".obsidian/** should match"
    );
    assert!(
        set.is_match("notes/foo.md.tmp"),
        "**/*.tmp should match nested .tmp files"
    );
    assert!(
        set.is_match("My Note .sync-conflict-202604.md"),
        "*.sync-conflict-* should match a top-level sync-conflict file"
    );

    assert!(
        !set.is_match("notes/foo.md"),
        "ordinary note must not match any default ignore"
    );
}

#[test]
fn compiled_ignores_reports_offending_pattern() {
    let cfg = WatcherConfig {
        debounce_ms: 500,
        ignore_patterns: vec!["valid/**".to_string(), "[".to_string()],
        respect_gitignore: true,
    };

    let err = cfg
        .compiled_ignores()
        .expect_err("invalid pattern must error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("\"[\""),
        "error must quote the offending pattern: {msg}"
    );
}

#[test]
fn rejects_vault_that_is_a_file() {
    let root = ScopedTmp(unique_tmp("vault-is-file"));
    let file_vault = root.0.join("not-a-dir");
    fs::write(&file_vault, b"i am a file").unwrap();
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", file_vault.display()));

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection for file vault");
    let msg = format!("{err:#}");
    assert!(msg.contains("directory"), "{msg}");
}

// ===== Step 25 Task 1: [search.semantic] config defaults and validation =====

#[test]
fn search_semantic_defaults_are_correct() {
    let cfg = SemanticSearchConfig::default();
    assert_eq!(cfg.default_granularity, "document");
    assert_eq!(cfg.default_chunks_per_document, 3);
    assert_eq!(cfg.document_candidate_multiplier, 10);
    assert_eq!(cfg.document_candidate_limit, 1000);
}

#[test]
fn search_semantic_parses_from_toml() {
    let root = ScopedTmp(unique_tmp("search-semantic-toml"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             default_granularity = \"chunk\"\n\
             default_chunks_per_document = 5\n\
             document_candidate_multiplier = 20\n\
             document_candidate_limit = 500\n",
            vault.display()
        ),
    );
    let cfg = Config::load(Some(&cfg_path)).expect("should parse");
    assert_eq!(cfg.search.semantic.default_granularity, "chunk");
    assert_eq!(cfg.search.semantic.default_chunks_per_document, 5);
    assert_eq!(cfg.search.semantic.document_candidate_multiplier, 20);
    assert_eq!(cfg.search.semantic.document_candidate_limit, 500);
}

#[test]
fn search_semantic_default_granularity_round_trips_as_document() {
    let root = ScopedTmp(unique_tmp("search-semantic-default-gran"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", vault.display()));
    let cfg = Config::load(Some(&cfg_path)).expect("should parse");
    assert_eq!(cfg.search.semantic.default_granularity, "document");
}

#[test]
fn search_semantic_rejects_invalid_granularity() {
    let root = ScopedTmp(unique_tmp("search-semantic-bad-gran"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             default_granularity = \"paragraph\"\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("invalid granularity should reject");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("paragraph") || msg.contains("granularity"),
        "error must mention the bad value: {msg}"
    );
}

#[test]
fn search_semantic_rejects_chunks_per_document_zero() {
    let root = ScopedTmp(unique_tmp("search-semantic-cpd-zero"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             default_chunks_per_document = 0\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("0 should reject");
    let msg = format!("{err:#}");
    assert!(msg.contains("chunks_per_document"), "{msg}");
}

#[test]
fn search_semantic_rejects_chunks_per_document_over_100() {
    let root = ScopedTmp(unique_tmp("search-semantic-cpd-over"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             default_chunks_per_document = 101\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("101 should reject");
    let msg = format!("{err:#}");
    assert!(msg.contains("chunks_per_document"), "{msg}");
}

#[test]
fn search_semantic_rejects_candidate_multiplier_zero() {
    let root = ScopedTmp(unique_tmp("search-semantic-mult-zero"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             document_candidate_multiplier = 0\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("0 should reject");
    let msg = format!("{err:#}");
    assert!(msg.contains("candidate_multiplier"), "{msg}");
}

#[test]
fn search_semantic_rejects_candidate_limit_over_10000() {
    let root = ScopedTmp(unique_tmp("search-semantic-limit-over"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             document_candidate_limit = 10001\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("10001 should reject");
    let msg = format!("{err:#}");
    assert!(msg.contains("candidate_limit"), "{msg}");
}

#[test]
fn search_semantic_rejects_unknown_key() {
    let root = ScopedTmp(unique_tmp("search-semantic-unknown-key"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n\
             [search.semantic]\n\
             bogus_field = true\n",
            vault.display()
        ),
    );
    let err = Config::load(Some(&cfg_path)).expect_err("unknown field should reject");
    let msg = format!("{err:#}");
    assert!(msg.contains("bogus_field"), "{msg}");
}

#[test]
fn extension_path_is_rejected_as_unknown_field() {
    // Decision 2: extension_path is immediately removed, not deferred or warned.
    // This test documents the expected behavior and prevents accidental re-introduction.
    // See notes/roadmap/step-22-workplan.md § Phase 3 § Task 3.3.
    let root = ScopedTmp(unique_tmp("extension-path-reject"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[embedding]\nextension_path = \"/tmp/x.dylib\"\n",
            vault.display()
        ),
    );

    let err = Config::load(Some(&cfg_path)).expect_err("extension_path must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("extension_path"),
        "error must name extension_path as unknown: {msg}"
    );
}
