use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use hypomnema::config::Config;

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
    assert_eq!(config.watcher.debounce_ms, 400);
    assert!(
        config
            .watcher
            .ignore_patterns
            .iter()
            .any(|s| s == ".obsidian/**")
    );
    assert_eq!(config.storage.index_file, "index.sqlite");
    assert_eq!(config.storage.outbox_file, "outbox.jsonl");
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
    let msg = format!("{:#}", err);
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
    let msg = format!("{:#}", err);
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
    let msg = format!("{:#}", err);
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
    let msg = format!("{:#}", err);
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
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("plaid") || msg.contains("logging.level"),
        "{msg}"
    );
}

#[test]
fn rejects_missing_vault() {
    let root = ScopedTmp(unique_tmp("missing-vault"));
    let absent = root.0.join("does-not-exist");
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", absent.display()));

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection for missing vault");
    let msg = format!("{:#}", err);
    assert!(msg.contains("vault"), "{msg}");
}

#[test]
fn rejects_vault_that_is_a_file() {
    let root = ScopedTmp(unique_tmp("vault-is-file"));
    let file_vault = root.0.join("not-a-dir");
    fs::write(&file_vault, b"i am a file").unwrap();
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", file_vault.display()));

    let err = Config::load(Some(&cfg_path)).expect_err("expected rejection for file vault");
    let msg = format!("{:#}", err);
    assert!(msg.contains("directory"), "{msg}");
}
