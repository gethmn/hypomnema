use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use hypomnema::config::Config;
use hypomnema::embedding::{Embedder, StubEmbedder};
use hypomnema::events::{EventBus, EventType, FileChangedEvent, StreamEvent};
use hypomnema::indexer::{Scanner, hash_file};
use hypomnema::store::Store;
use hypomnema::vault_registry::VaultId;
use hypomnema::watcher::{self, Watcher};
use tempfile::TempDir;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

const DEBOUNCE_MS: u64 = 50;
const SETTLE: Duration = Duration::from_millis(2 * DEBOUNCE_MS);
const EVENT_TIMEOUT: Duration = Duration::from_secs(2);

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
        debounce_ms: DEBOUNCE_MS,
        vault_id: VaultId::new(),
    }
}

struct Live {
    _watcher: Watcher,
    shutdown_tx: watch::Sender<bool>,
    _rescan_tx: watch::Sender<u64>,
    consumer: JoinHandle<()>,
    event_bus: Arc<EventBus>,
}

impl Live {
    fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_bus.subscribe()
    }

    async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.consumer.await;
        drop(self._watcher);
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

    let ignores = fx
        .config
        .watcher
        .compiled_ignores()
        .expect("compile ignores");
    let (watcher, rx) = watcher::spawn_watcher(
        &fx.vault_id,
        &fx.vault,
        ignores,
        Duration::from_millis(fx.debounce_ms),
        256,
    )
    .expect("spawn watcher");

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

    Live {
        _watcher: watcher,
        shutdown_tx,
        _rescan_tx: rescan_tx,
        consumer,
        event_bus,
    }
}

async fn recv_file_changed(rx: &mut broadcast::Receiver<StreamEvent>) -> Option<FileChangedEvent> {
    match tokio::time::timeout(EVENT_TIMEOUT, rx.recv()).await {
        Ok(Ok(StreamEvent::FileChanged(ev))) => Some(ev),
        Ok(Ok(other)) => panic!("unexpected stream event: {other:?}"),
        Ok(Err(e)) => panic!("event receive failed: {e:?}"),
        Err(_) => None,
    }
}

fn drain_available(rx: &mut broadcast::Receiver<StreamEvent>) -> Vec<FileChangedEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(StreamEvent::FileChanged(ev)) => events.push(ev),
            Ok(other) => panic!("unexpected stream event: {other:?}"),
            Err(broadcast::error::TryRecvError::Empty) => break,
            Err(e) => panic!("event receive failed: {e:?}"),
        }
    }
    events
}

#[tokio::test]
async fn editing_existing_file_publishes_one_modified_event() {
    let fx = fixture();
    fs::write(fx.vault.join("hello.md"), b"# v1\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    fs::write(fx.vault.join("hello.md"), b"# v2 longer content\n").unwrap();

    let ev = recv_file_changed(&mut rx)
        .await
        .expect("expected one modified event");
    assert_eq!(ev.event_type, EventType::Modified);
    assert_eq!(ev.path, "hello.md");
    let expected_hash = hash_file(&fx.vault.join("hello.md")).unwrap();
    assert_eq!(ev.content_hash.as_deref(), Some(expected_hash.as_str()));
    tokio::time::sleep(SETTLE).await;
    assert!(drain_available(&mut rx).is_empty());

    live.shutdown().await;
}

#[tokio::test]
async fn mtime_only_touch_publishes_no_events() {
    let fx = fixture();
    fs::write(fx.vault.join("stable.md"), b"# stable\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    let f = fs::OpenOptions::new()
        .write(true)
        .open(fx.vault.join("stable.md"))
        .unwrap();
    f.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    drop(f);

    tokio::time::sleep(SETTLE * 2).await;
    assert!(
        drain_available(&mut rx).is_empty(),
        "mtime-only touch must produce no live events"
    );

    live.shutdown().await;
}

#[tokio::test]
async fn deleting_file_publishes_one_deleted_event_with_prior_hash() {
    let fx = fixture();
    fs::write(fx.vault.join("bye.md"), b"# bye\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();
    let prior_hash = hash_file(&fx.vault.join("bye.md")).unwrap();

    fs::remove_file(fx.vault.join("bye.md")).unwrap();

    let ev = recv_file_changed(&mut rx)
        .await
        .expect("expected one deleted event");
    assert_eq!(ev.event_type, EventType::Deleted);
    assert_eq!(ev.path, "bye.md");
    assert_eq!(ev.content_hash.as_deref(), Some(prior_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn creating_new_md_publishes_one_created_event() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes")).unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    fs::write(fx.vault.join("notes/new.md"), b"# new\n").unwrap();

    let ev = recv_file_changed(&mut rx)
        .await
        .expect("expected one created event");
    assert_eq!(ev.event_type, EventType::Created);
    assert_eq!(ev.path, "notes/new.md");
    let expected_hash = hash_file(&fx.vault.join("notes/new.md")).unwrap();
    assert_eq!(ev.content_hash.as_deref(), Some(expected_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn live_bus_serves_each_new_event_after_subscription() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v1\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    fs::write(fx.vault.join("note.md"), b"# v2\n").unwrap();
    let first = recv_file_changed(&mut rx)
        .await
        .expect("expected first modified event");
    assert_eq!(first.event_type, EventType::Modified);
    assert_eq!(first.path, "note.md");

    fs::write(fx.vault.join("note.md"), b"# v3 even longer\n").unwrap();
    let second = recv_file_changed(&mut rx)
        .await
        .expect("expected second modified event");
    assert_eq!(second.event_type, EventType::Modified);
    let expected_hash = hash_file(&fx.vault.join("note.md")).unwrap();
    assert_eq!(second.content_hash.as_deref(), Some(expected_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn live_events_do_not_create_outbox_file_in_vault() {
    let fx = fixture();
    fs::write(fx.vault.join("a.md"), b"# a\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    fs::write(fx.vault.join("a.md"), b"# a v2\n").unwrap();
    assert!(recv_file_changed(&mut rx).await.is_some());
    fs::write(fx.vault.join("b.md"), b"# b\n").unwrap();
    assert!(recv_file_changed(&mut rx).await.is_some());

    assert!(
        !fx.vault.join("outbox.jsonl").exists(),
        "outbox file must not appear under the watched vault"
    );

    live.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn rename_publishes_deleted_then_created_events() {
    let fx = fixture();
    fs::create_dir_all(fx.vault.join("notes")).unwrap();
    fs::write(fx.vault.join("notes/a.md"), b"# a\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();
    let prior_hash = hash_file(&fx.vault.join("notes/a.md")).unwrap();

    fs::rename(fx.vault.join("notes/a.md"), fx.vault.join("notes/b.md")).unwrap();

    let first = recv_file_changed(&mut rx)
        .await
        .expect("expected deleted event");
    let second = recv_file_changed(&mut rx)
        .await
        .expect("expected created event");

    assert_eq!(first.event_type, EventType::Deleted);
    assert_eq!(first.path, "notes/a.md");
    assert_eq!(first.content_hash.as_deref(), Some(prior_hash.as_str()));

    assert_eq!(second.event_type, EventType::Created);
    assert_eq!(second.path, "notes/b.md");
    let new_hash = hash_file(&fx.vault.join("notes/b.md")).unwrap();
    assert_eq!(second.content_hash.as_deref(), Some(new_hash.as_str()));

    live.shutdown().await;
}

#[tokio::test]
async fn sync_conflict_files_publish_no_events() {
    let fx = fixture();
    fs::write(fx.vault.join("kept.md"), b"# kept\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

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

    assert!(
        drain_available(&mut rx).is_empty(),
        "conflict files must not produce live events"
    );

    live.shutdown().await;
}

#[tokio::test]
async fn sustained_save_loop_yields_at_most_two_events() {
    let fx = fixture();
    fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();

    let live = start(&fx).await;
    let mut rx = live.subscribe();

    for _ in 0..50 {
        fs::write(fx.vault.join("note.md"), b"# v0\n").unwrap();
    }
    tokio::time::sleep(SETTLE * 4).await;

    let events = drain_available(&mut rx);
    assert!(
        events.len() <= 2,
        "sustained same-byte writes must emit at most 2 events, got {events:?}"
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
