use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::events::EventBus;
use hypomnema::indexer::Scanner;
use hypomnema::store::Store;
use hypomnema::vault_registry::{VaultId, vault_data_dir};
use hypomnema::watcher::{self, WatchEvent};
use rusqlite::{Connection, OpenFlags};
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SETTLE: Duration = Duration::from_millis(500);

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    config: Config,
    debounce_ms: u64,
    vault_id: VaultId,
}

impl Fixture {
    fn index_dir(&self) -> PathBuf {
        vault_data_dir(&self.data_dir, &self.vault_id)
    }
}

fn fixture() -> Fixture {
    fixture_with_debounce(DEBOUNCE_MS)
}

fn fixture_with_debounce(debounce_ms: u64) -> Fixture {
    let root = tempfile::tempdir().expect("create root tempdir");
    let vault = root.path().join("vault");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&vault).expect("create vault dir");

    let cfg_path = root.path().join("config.toml");
    fs::write(
        &cfg_path,
        format!(
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n[watcher]\ndebounce_ms = {}\n",
            vault.display(),
            data_dir.display(),
            debounce_ms,
        ),
    )
    .expect("write config.toml");
    let config = Config::load(Some(&cfg_path)).expect("load config");
    let vault = config
        .vault
        .as_ref()
        .expect("test config must define [vault] block")
        .0
        .clone();
    let data_dir = config.storage.data_dir.0.clone();
    Fixture {
        _root: root,
        vault,
        data_dir,
        config,
        debounce_ms,
        vault_id: VaultId::new(),
    }
}

struct Live {
    tx: mpsc::Sender<WatchEvent>,
    shutdown_tx: watch::Sender<bool>,
    _rescan_tx: watch::Sender<u64>,
    consumer: JoinHandle<()>,
    _event_bus: Arc<EventBus>,
}

impl Live {
    async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.consumer.await;
    }
}

async fn start(fx: &Fixture) -> Live {
    let store = Store::open(
        &fx.vault_id,
        &fx.data_dir,
        &fx.config.storage.index_file,
        &fx.config.embedding,
    )
    .await
    .expect("open store");
    let embedder: Arc<dyn Embedder> = Arc::new(StubEmbedder::new(768));
    let scanner = Scanner::new(&fx.vault, &fx.config, &store, embedder).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let (tx, rx) = mpsc::channel(256);

    let event_bus = Arc::new(EventBus::new());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (rescan_tx, rescan_rx) = watch::channel(0u64);
    let consumer = tokio::spawn(watcher::run_consumer(
        rx,
        scanner,
        fx.vault_id.clone(),
        event_bus.clone(),
        shutdown_rx,
        rescan_rx,
    ));
    tokio::time::sleep(Duration::from_millis(fx.debounce_ms)).await;
    assert!(
        !consumer.is_finished(),
        "watcher consumer exited during startup"
    );

    Live {
        tx,
        shutdown_tx,
        _rescan_tx: rescan_tx,
        consumer,
        _event_bus: event_bus,
    }
}

async fn send_upsert(live: &Live, rel: &str) {
    live.tx
        .send(WatchEvent::Upsert(rel.to_string()))
        .await
        .expect("send upsert watch event");
}

async fn send_remove(live: &Live, rel: &str) {
    live.tx
        .send(WatchEvent::Remove(rel.to_string()))
        .await
        .expect("send remove watch event");
}

fn open_index(index_dir: &Path) -> Connection {
    let db_path = index_dir.join("index.sqlite");
    Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("open index.sqlite read-only")
}

fn paths_in_index(index_dir: &Path) -> Vec<String> {
    let conn = open_index(index_dir);
    let mut stmt = conn
        .prepare("SELECT path FROM files ORDER BY path")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn hash_for(index_dir: &Path, path: &str) -> Option<String> {
    let conn = open_index(index_dir);
    conn.query_row(
        "SELECT content_hash FROM files WHERE path = ?1",
        rusqlite::params![path],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

#[tokio::test]
async fn edit_updates_content_hash() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();

    let live = start(&fx).await;
    let initial_hash = hash_for(&fx.index_dir(), "note.md").expect("initial row");

    fs::write(fx.vault.join("note.md"), b"# v1 changed bytes\n").unwrap();
    send_upsert(&live, "note.md").await;
    tokio::time::sleep(SETTLE).await;

    let new_hash = hash_for(&fx.index_dir(), "note.md").expect("row after edit");
    assert_ne!(initial_hash, new_hash, "content_hash should change on edit");
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["note.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn dropping_sync_conflict_file_is_ignored() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();

    let live = start(&fx).await;
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    fs::write(
        fx.vault.join("My Note.sync-conflict-202604.md"),
        b"# conflict\n",
    )
    .unwrap();
    tokio::time::sleep(SETTLE).await;

    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn dropping_obsidian_conflict_file_is_ignored() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();

    let live = start(&fx).await;
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    fs::write(
        fx.vault.join("My Note (conflicted copy 2026-04-25).md"),
        b"# conflict\n",
    )
    .unwrap();
    tokio::time::sleep(SETTLE).await;

    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn deleting_md_removes_row() {
    let fx = fixture();
    fs::write(fx.vault.join("a.md"), b"# a\n").unwrap();
    fs::write(fx.vault.join("b.md"), b"# b\n").unwrap();

    let live = start(&fx).await;
    assert_eq!(
        paths_in_index(&fx.index_dir()),
        vec!["a.md".to_string(), "b.md".to_string()]
    );

    fs::remove_file(fx.vault.join("a.md")).unwrap();
    send_remove(&live, "a.md").await;
    tokio::time::sleep(SETTLE).await;

    assert_eq!(paths_in_index(&fx.index_dir()), vec!["b.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn set_modified_without_byte_change_leaves_hash() {
    use std::time::SystemTime;

    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();

    let live = start(&fx).await;
    let original_hash = hash_for(&fx.index_dir(), "note.md").expect("initial row");

    let f = fs::OpenOptions::new()
        .write(true)
        .open(fx.vault.join("note.md"))
        .unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    drop(f);
    send_upsert(&live, "note.md").await;
    tokio::time::sleep(SETTLE).await;

    let after_hash = hash_for(&fx.index_dir(), "note.md").expect("row after touch");
    assert_eq!(
        original_hash, after_hash,
        "content_hash must not change for an mtime-only bump"
    );
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["note.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn creating_nested_md_appears_with_forward_slash_path() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();
    fs::create_dir_all(fx.vault.join("notes/sub")).unwrap();

    let live = start(&fx).await;
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    fs::write(fx.vault.join("notes/sub/new.md"), b"# new\n").unwrap();
    send_upsert(&live, "notes/sub/new.md").await;
    tokio::time::sleep(SETTLE).await;

    assert_eq!(
        paths_in_index(&fx.index_dir()),
        vec!["kept.md".to_string(), "notes/sub/new.md".to_string()]
    );

    live.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn rename_decomposes_into_remove_and_upsert() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes")).unwrap();
    fs::write(fx.vault.join("notes/a.md"), b"# a\n").unwrap();

    let live = start(&fx).await;
    assert_eq!(
        paths_in_index(&fx.index_dir()),
        vec!["notes/a.md".to_string()]
    );

    fs::rename(fx.vault.join("notes/a.md"), fx.vault.join("notes/b.md")).unwrap();
    send_remove(&live, "notes/a.md").await;
    send_upsert(&live, "notes/b.md").await;
    tokio::time::sleep(SETTLE).await;

    assert_eq!(
        paths_in_index(&fx.index_dir()),
        vec!["notes/b.md".to_string()]
    );

    live.shutdown().await;
}

#[tokio::test]
async fn dropping_dot_git_md_is_ignored() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();
    fs::create_dir_all(fx.vault.join(".git")).unwrap();

    let live = start(&fx).await;
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    fs::write(fx.vault.join(".git/HEAD.md"), b"ref: refs/heads/main\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    assert_eq!(paths_in_index(&fx.index_dir()), vec!["kept.md".to_string()]);

    live.shutdown().await;
}

#[tokio::test]
async fn sustained_save_loop_completes_with_consistent_row() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();

    let live = start(&fx).await;
    let initial_hash = hash_for(&fx.index_dir(), "note.md").expect("initial row");

    let started = Instant::now();
    for _ in 0..50 {
        fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();
        send_upsert(&live, "note.md").await;
    }
    // Sustained writes keep extending the debouncer's quiet window; settle
    // generously so the final batch has time to fire and the consumer to
    // reindex once on the stabilised file.
    tokio::time::sleep(SETTLE * 4).await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "sustained save loop took {elapsed:?}, expected < 5s (criterion 5 smoke)"
    );

    let final_hash =
        hash_for(&fx.index_dir(), "note.md").expect("row must still exist after save loop");
    assert_eq!(
        initial_hash, final_hash,
        "same-bytes loop must leave content_hash equal to the original"
    );
    assert_eq!(paths_in_index(&fx.index_dir()), vec!["note.md".to_string()]);

    live.shutdown().await;
}
