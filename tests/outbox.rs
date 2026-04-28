use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::indexer::{Scanner, hash_file};
use hypomnema::outbox::{ChangeEvent, EventType, Outbox};
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultId;
use hypomnema::watcher::{self, Watcher};
use tempfile::TempDir;
use tokio::sync::watch;
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SETTLE: Duration = Duration::from_millis(2 * DEBOUNCE_MS);

struct Fixture {
    _root: TempDir,
    vault: PathBuf,
    data_dir: PathBuf,
    config: Config,
    debounce_ms: u64,
    vault_id: VaultId,
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
            "vault = \"{}\"\n[storage]\ndata_dir = \"{}\"\n[watcher]\ndebounce_ms = {}\n",
            vault.display(),
            data_dir.display(),
            DEBOUNCE_MS,
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
        debounce_ms: DEBOUNCE_MS,
        vault_id: VaultId::new(),
    }
}

fn outbox_path(fx: &Fixture) -> PathBuf {
    fx.data_dir.join("outbox.jsonl")
}

struct Live {
    watcher: Watcher,
    shutdown_tx: watch::Sender<bool>,
    consumer: JoinHandle<()>,
}

impl Live {
    async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.consumer.await;
        drop(self.watcher);
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
    let scanner = Scanner::new(&fx.config, &store, embedder).expect("construct scanner");
    let _ = scanner.run().await.expect("initial scan");

    let ignores = fx
        .config
        .watcher
        .compiled_ignores()
        .expect("compile ignores");
    let (watcher, rx) = watcher::spawn_watcher(
        &fx.vault,
        ignores,
        Duration::from_millis(fx.debounce_ms),
        256,
    )
    .expect("spawn watcher");

    let outbox = Outbox::open(outbox_path(fx)).await.expect("open outbox");
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let consumer = tokio::spawn(watcher::run_consumer(rx, scanner, outbox, shutdown_rx));

    Live {
        watcher,
        shutdown_tx,
        consumer,
    }
}

fn read_events(path: &Path) -> Vec<ChangeEvent> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn editing_existing_file_emits_one_modified_line() {
    let fx = fixture();
    fs::write(fx.vault.join("hello.md"), b"# v1\n").unwrap();

    let live = start(&fx).await;

    fs::write(fx.vault.join("hello.md"), b"# v2 longer content\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert_eq!(
        events.len(),
        1,
        "expected one modified event, got {events:?}"
    );
    let ev = &events[0];
    assert_eq!(ev.event_type, EventType::Modified);
    assert_eq!(ev.path, "hello.md");
    let expected_hash = hash_file(&fx.vault.join("hello.md")).unwrap();
    assert_eq!(ev.content_hash.as_deref(), Some(expected_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn mtime_only_touch_emits_no_outbox_lines() {
    let fx = fixture();
    fs::write(fx.vault.join("stable.md"), b"# stable\n").unwrap();

    let live = start(&fx).await;

    let f = fs::OpenOptions::new()
        .write(true)
        .open(fx.vault.join("stable.md"))
        .unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    drop(f);
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert!(
        events.is_empty(),
        "mtime-only touch must produce no outbox events, got {events:?}"
    );

    live.shutdown().await;
}

#[tokio::test]
async fn deleting_file_emits_one_deleted_line_with_prior_hash() {
    let fx = fixture();
    fs::write(fx.vault.join("bye.md"), b"# bye\n").unwrap();

    let live = start(&fx).await;
    let prior_hash = hash_file(&fx.vault.join("bye.md")).unwrap();

    fs::remove_file(fx.vault.join("bye.md")).unwrap();
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert_eq!(
        events.len(),
        1,
        "expected one deleted event, got {events:?}"
    );
    let ev = &events[0];
    assert_eq!(ev.event_type, EventType::Deleted);
    assert_eq!(ev.path, "bye.md");
    assert_eq!(ev.content_hash.as_deref(), Some(prior_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn creating_new_md_emits_one_created_line() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes")).unwrap();

    let live = start(&fx).await;

    fs::write(fx.vault.join("notes/new.md"), b"# new\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert_eq!(
        events.len(),
        1,
        "expected one created event, got {events:?}"
    );
    let ev = &events[0];
    assert_eq!(ev.event_type, EventType::Created);
    assert_eq!(ev.path, "notes/new.md");
    let expected_hash = hash_file(&fx.vault.join("notes/new.md")).unwrap();
    assert_eq!(ev.content_hash.as_deref(), Some(expected_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn tail_f_shape_serves_each_new_line_from_captured_offset() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v1\n").unwrap();

    let live = start(&fx).await;
    let path = outbox_path(&fx);

    let mut offset: u64 = 0;

    fs::write(fx.vault.join("note.md"), b"# v2\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let mut f = fs::File::open(&path).unwrap();
    f.seek(SeekFrom::Start(offset)).unwrap();
    let mut buf = String::new();
    f.read_to_string(&mut buf).unwrap();
    assert!(
        buf.ends_with('\n') && buf.lines().count() == 1,
        "expected exactly one new JSONL line after first edit, got: {buf:?}"
    );
    let parsed: ChangeEvent = serde_json::from_str(buf.lines().next().unwrap()).unwrap();
    assert_eq!(parsed.event_type, EventType::Modified);
    assert_eq!(parsed.path, "note.md");
    offset = fs::metadata(&path).unwrap().len();

    fs::write(fx.vault.join("note.md"), b"# v3 even longer\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let mut f = fs::File::open(&path).unwrap();
    f.seek(SeekFrom::Start(offset)).unwrap();
    let mut buf = String::new();
    f.read_to_string(&mut buf).unwrap();
    assert!(
        buf.ends_with('\n') && buf.lines().count() == 1,
        "expected exactly one new JSONL line after second edit, got: {buf:?}"
    );
    let parsed: ChangeEvent = serde_json::from_str(buf.lines().next().unwrap()).unwrap();
    assert_eq!(parsed.event_type, EventType::Modified);
    let expected_hash = hash_file(&fx.vault.join("note.md")).unwrap();
    assert_eq!(parsed.content_hash.as_deref(), Some(expected_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn outbox_file_lives_in_data_dir_not_in_vault() {
    let fx = fixture();
    fs::write(fx.vault.join("a.md"), b"# a\n").unwrap();

    let live = start(&fx).await;

    fs::write(fx.vault.join("a.md"), b"# a v2\n").unwrap();
    tokio::time::sleep(SETTLE).await;
    fs::write(fx.vault.join("b.md"), b"# b\n").unwrap();
    tokio::time::sleep(SETTLE).await;

    let configured = outbox_path(&fx);
    assert!(
        configured.starts_with(&fx.data_dir),
        "outbox path {configured:?} must live under data_dir {:?}",
        fx.data_dir,
    );
    assert!(configured.exists(), "outbox file must exist after writes");
    let events = read_events(&configured);
    assert!(
        !events.is_empty(),
        "outbox under data_dir should have collected events"
    );

    assert!(
        !fx.vault.join("outbox.jsonl").exists(),
        "outbox file must not appear under the watched vault"
    );

    live.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn rename_emits_deleted_then_created_lines() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes")).unwrap();
    fs::write(fx.vault.join("notes/a.md"), b"# a\n").unwrap();

    let live = start(&fx).await;
    let prior_hash = hash_file(&fx.vault.join("notes/a.md")).unwrap();

    fs::rename(fx.vault.join("notes/a.md"), fx.vault.join("notes/b.md")).unwrap();
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert_eq!(
        events.len(),
        2,
        "rename must emit exactly two events, got {events:?}"
    );

    assert_eq!(events[0].event_type, EventType::Deleted);
    assert_eq!(events[0].path, "notes/a.md");
    assert_eq!(events[0].content_hash.as_deref(), Some(prior_hash.as_str()));

    assert_eq!(events[1].event_type, EventType::Created);
    assert_eq!(events[1].path, "notes/b.md");
    let new_hash = hash_file(&fx.vault.join("notes/b.md")).unwrap();
    assert_eq!(events[1].content_hash.as_deref(), Some(new_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn sync_conflict_files_emit_no_outbox_lines() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();

    let live = start(&fx).await;

    fs::write(
        fx.vault.join("My Note.sync-conflict-202604.md"),
        b"# syncthing\n",
    )
    .unwrap();
    tokio::time::sleep(SETTLE).await;
    fs::write(
        fx.vault.join("My Note (conflicted copy 2026-04-26).md"),
        b"# obsidian\n",
    )
    .unwrap();
    tokio::time::sleep(SETTLE).await;

    let events = read_events(&outbox_path(&fx));
    assert!(
        events.is_empty(),
        "conflict files must not produce outbox events, got {events:?}"
    );

    live.shutdown().await;
}

#[tokio::test]
async fn sustained_save_loop_yields_at_most_two_outbox_lines() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();

    let live = start(&fx).await;

    for _ in 0..50 {
        fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();
    }
    tokio::time::sleep(SETTLE * 4).await;

    let events = read_events(&outbox_path(&fx));
    assert!(
        events.len() <= 2,
        "sustained same-byte writes must emit ≤ 2 outbox events, got {events:?}"
    );
    if let Some(last) = events.last() {
        let on_disk = hash_file(&fx.vault.join("note.md")).unwrap();
        assert_eq!(
            last.content_hash.as_deref(),
            Some(on_disk.as_str()),
            "final emitted hash must match bytes on disk"
        );
    }

    live.shutdown().await;
}
