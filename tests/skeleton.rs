use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_tmp(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time before unix epoch")
        .as_nanos();
    let dir = env::temp_dir().join(format!("hypomnema-skel-{label}-{pid}-{nanos}-{n}"));
    fs::create_dir_all(&dir).expect("create unique tempdir");
    dir
}

struct ScopedTmp(PathBuf);

impl Drop for ScopedTmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_config(root: &Path, body: &str) -> PathBuf {
    let cfg_path = root.join("config.toml");
    fs::write(&cfg_path, body).expect("write config");
    cfg_path
}

fn hmn() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hmn"))
}

fn hmnd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hmnd"))
}

#[test]
fn hmn_help_succeeds() {
    let out = hmn().arg("--help").output().expect("run hmn --help");
    assert!(
        out.status.success(),
        "hmn --help failed: status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("search"), "help missing `search`: {stdout}");
    assert!(stdout.contains("status"), "help missing `status`: {stdout}");
}

#[test]
fn hmn_version_succeeds() {
    let out = hmn().arg("--version").output().expect("run hmn --version");
    assert!(
        out.status.success(),
        "hmn --version failed: status={:?}",
        out.status.code()
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("hmn"),
        "version output should mention `hmn`: {stdout}"
    );
}

#[test]
fn hmn_search_help_lists_modes() {
    let out = hmn()
        .args(["search", "--help"])
        .output()
        .expect("run hmn search --help");
    assert!(
        out.status.success(),
        "hmn search --help failed: status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for mode in ["filesystem", "content", "semantic"] {
        assert!(
            stdout.contains(mode),
            "search --help missing mode `{mode}`: {stdout}"
        );
    }
}

#[test]
fn hmnd_help_succeeds() {
    let out = hmnd().arg("--help").output().expect("run hmnd --help");
    assert!(
        out.status.success(),
        "hmnd --help failed: status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("config-validate"),
        "hmnd help missing `config-validate`: {stdout}"
    );
}

#[test]
fn hmnd_config_validate_succeeds_against_valid_config() {
    let root = ScopedTmp(unique_tmp("cfgvalid-ok"));
    let vault = root.0.join("vault");
    let data_dir = root.0.join("data");
    fs::create_dir_all(&vault).unwrap();
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            data_dir.display(),
        ),
    );

    let out = hmnd()
        .args(["config-validate", "--config"])
        .arg(&cfg_path)
        .output()
        .expect("run hmnd config-validate");

    assert!(
        out.status.success(),
        "hmnd config-validate against valid config should exit 0: status={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn hmnd_config_validate_accepts_missing_legacy_vault_for_migration() {
    // Step 9 onward: an inaccessible legacy [vault] path is no longer a
    // config-validate failure (Resolution E Case 1 — the daemon migrates
    // the legacy block into a registry row in `errored` status at runtime).
    // `hmnd config-validate` therefore exits 0 against a missing vault path.
    let root = ScopedTmp(unique_tmp("cfgvalid-novault"));
    let absent = root.0.join("does-not-exist");
    let cfg_path = write_config(&root.0, &format!("vault = \"{}\"\n", absent.display()));

    let out = hmnd()
        .args(["config-validate", "--config"])
        .arg(&cfg_path)
        .output()
        .expect("run hmnd config-validate");

    assert_eq!(
        out.status.code(),
        Some(0),
        "missing-vault config must now exit 0 (legacy migration handles it at runtime), got {:?}; stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn hmnd_config_validate_exits_3_when_data_dir_under_vault() {
    let root = ScopedTmp(unique_tmp("cfgvalid-dd-under-vault"));
    let vault = root.0.join("vault");
    fs::create_dir_all(&vault).unwrap();
    let bad_data_dir = vault.join("data");
    let cfg_path = write_config(
        &root.0,
        &format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            bad_data_dir.display(),
        ),
    );

    let out = hmnd()
        .args(["config-validate", "--config"])
        .arg(&cfg_path)
        .output()
        .expect("run hmnd config-validate");

    assert_eq!(
        out.status.code(),
        Some(3),
        "data_dir-under-vault must exit 3, got {:?}; stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}
