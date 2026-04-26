use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::{ScanReport, Scanner};
use hypomnema::store::Store;
use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    config: Config,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create root tempdir");
    let vault = root.path().join("vault");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&vault).expect("create vault dir");

    let cfg_path = root.path().join("config.toml");
    fs::write(
        &cfg_path,
        format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n",
            vault.display(),
            data_dir.display(),
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let vault = config.vault.0.clone();
    let data_dir = config.storage.data_dir.0.clone();
    Fixture {
        _root: root,
        vault,
        data_dir,
        config,
    }
}

async fn run_scan(fx: &Fixture) -> ScanReport {
    let store = Store::open(
        &fx.data_dir,
        &fx.config.storage.index_file,
        &fx.config.embedding,
    )
    .await
    .expect("open store");
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let scanner = Scanner::new(&fx.config, &store, embedder).expect("construct scanner");
    scanner.run().await.expect("run scan")
}

fn indexed_paths(data_dir: &Path) -> Vec<String> {
    let db_path = data_dir.join("index.sqlite");
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open index.sqlite read-only");
    let mut stmt = conn
        .prepare("SELECT path FROM files ORDER BY path")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    let mut out: Vec<String> = rows.map(|r| r.unwrap()).collect();
    out.sort();
    out
}

#[tokio::test]
async fn obsidian_directory_is_not_indexed() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join(".obsidian")).unwrap();
    fs::write(fx.vault.join(".obsidian/foo.md"), b"# obsidian").unwrap();
    fs::write(fx.vault.join("kept.md"), b"# kept").unwrap();

    run_scan(&fx).await;
    assert_eq!(indexed_paths(&fx.data_dir), vec!["kept.md".to_string()]);
}

#[tokio::test]
async fn dot_git_directory_is_not_indexed() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join(".git")).unwrap();
    fs::write(fx.vault.join(".git/HEAD"), b"ref: refs/heads/main\n").unwrap();
    fs::write(fx.vault.join(".git/HEAD.md"), b"# H").unwrap();
    fs::write(fx.vault.join("kept.md"), b"# kept").unwrap();

    run_scan(&fx).await;
    assert_eq!(indexed_paths(&fx.data_dir), vec!["kept.md".to_string()]);
}

#[tokio::test]
async fn sync_conflict_files_are_not_indexed() {
    let fx = fixture();
    fs::write(
        fx.vault.join("My Note.sync-conflict-202604.md"),
        b"# conflict",
    )
    .unwrap();
    fs::write(fx.vault.join("kept.md"), b"# kept").unwrap();

    run_scan(&fx).await;
    assert_eq!(indexed_paths(&fx.data_dir), vec!["kept.md".to_string()]);
}

#[tokio::test]
async fn nested_md_tmp_files_are_not_indexed() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes/dir")).unwrap();
    fs::write(fx.vault.join("notes/dir/file.md.tmp"), b"x").unwrap();
    fs::write(fx.vault.join("kept.md"), b"# kept").unwrap();

    run_scan(&fx).await;
    assert_eq!(indexed_paths(&fx.data_dir), vec!["kept.md".to_string()]);
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_inside_vault_is_indexed_under_link_name() {
    use std::os::unix::fs::symlink;

    let fx = fixture();
    fs::write(fx.vault.join("real.md"), b"# real").unwrap();
    symlink(fx.vault.join("real.md"), fx.vault.join("link.md")).unwrap();

    let report = run_scan(&fx).await;
    assert_eq!(
        indexed_paths(&fx.data_dir),
        vec!["link.md".to_string(), "real.md".to_string()]
    );
    assert_eq!(report.skipped_outside_vault, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_pointing_outside_vault_is_skipped() {
    use std::os::unix::fs::symlink;

    let fx = fixture();
    let outside = tempfile::tempdir().expect("create outside tempdir");
    fs::write(outside.path().join("secret.md"), b"# secret").unwrap();
    fs::write(fx.vault.join("kept.md"), b"# kept").unwrap();
    symlink(outside.path().join("secret.md"), fx.vault.join("link.md")).unwrap();

    let report = run_scan(&fx).await;
    assert_eq!(indexed_paths(&fx.data_dir), vec!["kept.md".to_string()]);
    assert!(
        report.skipped_outside_vault >= 1,
        "expected ScanReport.skipped_outside_vault >= 1, got {}",
        report.skipped_outside_vault
    );
}
