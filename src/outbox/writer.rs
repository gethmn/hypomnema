use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::task;

use super::event::ChangeEvent;

pub struct Outbox {
    path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
}

impl Outbox {
    pub async fn open(path: PathBuf) -> Result<Self> {
        let path_for_blocking = path.clone();
        let file = task::spawn_blocking(move || {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path_for_blocking)
                .with_context(|| format!("opening outbox at {}", path_for_blocking.display()))
        })
        .await
        .context("spawn_blocking join error in Outbox::open")??;
        Ok(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub async fn append(&self, event: ChangeEvent) -> Result<()> {
        let file = self.file.clone();
        task::spawn_blocking(move || -> Result<()> {
            let line = serde_json::to_string(&event).context("serializing change event")?;
            let mut g = file.lock().expect("outbox mutex poisoned");
            writeln!(*g, "{line}").context("writing outbox line")?;
            g.sync_data().context("fdatasync on outbox")?;
            Ok(())
        })
        .await
        .context("spawn_blocking join error in Outbox::append")?
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::EventType;
    use tempfile::TempDir;

    fn ev(kind: EventType, path: &str, hash: Option<&str>) -> ChangeEvent {
        ChangeEvent::now(kind, path.to_string(), hash.map(|s| s.to_string()))
    }

    fn read_lines(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    #[tokio::test]
    async fn append_three_events_round_trips_through_disk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("outbox.jsonl");
        let outbox = Outbox::open(path.clone()).await.unwrap();

        let inputs = [
            ev(EventType::Created, "notes/a.md", Some("sha256:aaa")),
            ev(EventType::Modified, "notes/a.md", Some("sha256:bbb")),
            ev(EventType::Deleted, "notes/a.md", Some("sha256:bbb")),
        ];
        for e in &inputs {
            outbox.append(e.clone()).await.unwrap();
        }

        let lines = read_lines(&path);
        assert_eq!(lines.len(), 3);
        for (i, line) in lines.iter().enumerate() {
            let parsed: ChangeEvent = serde_json::from_str(line).unwrap();
            assert_eq!(parsed, inputs[i]);
        }
    }

    #[tokio::test]
    async fn open_on_existing_non_empty_file_preserves_prior_contents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("outbox.jsonl");

        std::fs::write(&path, "{\"prior\":\"line\"}\n").unwrap();

        let outbox = Outbox::open(path.clone()).await.unwrap();
        outbox
            .append(ev(EventType::Modified, "x.md", Some("sha256:1")))
            .await
            .unwrap();

        let lines = read_lines(&path);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "{\"prior\":\"line\"}");
        let parsed: ChangeEvent = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(parsed.path, "x.md");
    }

    #[tokio::test]
    async fn open_on_missing_parent_directory_returns_err() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does/not/exist/outbox.jsonl");
        let result = Outbox::open(path).await;
        assert!(result.is_err(), "expected Err for missing parent dir");
    }

    #[tokio::test]
    async fn concurrent_appends_all_land() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("outbox.jsonl");
        let outbox = Arc::new(Outbox::open(path.clone()).await.unwrap());

        let mut handles = Vec::new();
        for i in 0..10 {
            let outbox = outbox.clone();
            handles.push(tokio::spawn(async move {
                outbox
                    .append(ev(
                        EventType::Modified,
                        &format!("notes/{i}.md"),
                        Some(&format!("sha256:{i}")),
                    ))
                    .await
                    .unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }

        let lines = read_lines(&path);
        assert_eq!(lines.len(), 10);
        for line in &lines {
            let _: ChangeEvent = serde_json::from_str(line).expect("each line is valid JSON");
        }
    }

    #[tokio::test]
    async fn path_returns_open_path() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("outbox.jsonl");
        let outbox = Outbox::open(path.clone()).await.unwrap();
        assert_eq!(outbox.path(), path.as_path());
    }

    #[tokio::test]
    async fn drop_and_reopen_preserves_prior_contents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("outbox.jsonl");

        {
            let outbox = Outbox::open(path.clone()).await.unwrap();
            outbox
                .append(ev(EventType::Created, "first.md", Some("sha256:1")))
                .await
                .unwrap();
        }

        let outbox = Outbox::open(path.clone()).await.unwrap();
        outbox
            .append(ev(EventType::Modified, "first.md", Some("sha256:2")))
            .await
            .unwrap();

        let lines = read_lines(&path);
        assert_eq!(lines.len(), 2);
        let first: ChangeEvent = serde_json::from_str(&lines[0]).unwrap();
        let second: ChangeEvent = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(first.event_type, EventType::Created);
        assert_eq!(second.event_type, EventType::Modified);
    }
}
