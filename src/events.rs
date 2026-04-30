//! Runtime-neutral event types and the daemon-level in-memory broadcast bus.
//!
//! The public v0 event surfaces are live-only: connected subscribers receive
//! events from a bounded `tokio::sync::broadcast` channel; disconnected
//! clients recover by querying current index state via the search/vault APIs.
//!
//! See `docs/specs/change-events.md` for the full wire-shape contract.

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::vault_registry::VaultId;

/// The maximum number of events the broadcast channel can buffer before
/// slow subscribers begin lagging. Chosen to be large enough for typical
/// burst writes while remaining small enough that memory overhead is bounded.
pub const BUS_CAPACITY: usize = 256;

// ---------------------------------------------------------------------------
// EventType
// ---------------------------------------------------------------------------

/// Per-file change classification. Serializes as lowercase per the spec.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Created,
    Modified,
    Deleted,
}

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// A `file_changed` event envelope.
///
/// Wire shape (per `docs/specs/change-events.md` § Event Envelope):
/// ```json
/// {
///   "type": "file_changed",
///   "event_type": "modified",
///   "vault": "<vault-id>",
///   "path": "notes/a.md",
///   "content_hash": "sha256:abc123...",
///   "detected_at": "2026-04-30T14:22:08.123456Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChangedEvent {
    pub event_type: EventType,
    pub vault: VaultId,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub detected_at: String,
}

impl FileChangedEvent {
    /// Construct a `FileChangedEvent` timestamped at the current wall-clock
    /// time (RFC 3339 with microsecond precision).
    pub fn now(
        vault: VaultId,
        event_type: EventType,
        path: String,
        content_hash: Option<String>,
    ) -> Self {
        Self {
            event_type,
            vault,
            path,
            content_hash,
            detected_at: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true),
        }
    }
}

/// A `stream_lagged` control event.
///
/// Wire shape (per `docs/specs/change-events.md` § Stream Control Events):
/// ```json
/// {
///   "type": "stream_lagged",
///   "vault": "<vault-id>",
///   "missed": 42,
///   "action": "resync_required",
///   "detected_at": "2026-04-30T14:24:01.000000Z"
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StreamLaggedEvent {
    /// Present when lag is known to affect one vault; omitted for
    /// daemon-wide stream loss.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault: Option<VaultId>,
    /// Number of missed events when the channel can report it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missed: Option<u64>,
    pub action: StreamLagAction,
    pub detected_at: String,
}

impl StreamLaggedEvent {
    /// Construct a `StreamLaggedEvent` for a single vault, timestamped now.
    pub fn now_for_vault(vault: VaultId, missed: Option<u64>) -> Self {
        Self {
            vault: Some(vault),
            missed,
            action: StreamLagAction::ResyncRequired,
            detected_at: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true),
        }
    }
}

/// The only `action` value defined in v0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamLagAction {
    ResyncRequired,
}

// ---------------------------------------------------------------------------
// StreamEvent — the top-level tagged union
// ---------------------------------------------------------------------------

/// The top-level event envelope sent over the live bus and serialized as
/// NDJSON on the HTTP and CLI surfaces.
///
/// Uses serde's externally-tagged representation with `"type"` as the tag
/// key, which maps directly to the wire shape in the spec:
///
/// - `{ "type": "file_changed", ... }`
/// - `{ "type": "stream_lagged", ... }`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    FileChanged(FileChangedEvent),
    StreamLagged(StreamLaggedEvent),
}

impl StreamEvent {
    /// Convenience constructor — wraps `FileChangedEvent::now`.
    pub fn file_changed(
        vault: VaultId,
        event_type: EventType,
        path: String,
        content_hash: Option<String>,
    ) -> Self {
        StreamEvent::FileChanged(FileChangedEvent::now(vault, event_type, path, content_hash))
    }

    /// Convenience constructor — wraps `StreamLaggedEvent::now_for_vault`.
    pub fn stream_lagged_for_vault(vault: VaultId, missed: Option<u64>) -> Self {
        StreamEvent::StreamLagged(StreamLaggedEvent::now_for_vault(vault, missed))
    }
}

// ---------------------------------------------------------------------------
// EventBus
// ---------------------------------------------------------------------------

/// Daemon-level in-memory broadcast bus for live change events.
///
/// One `EventBus` is created at daemon startup and shared (via `Arc`) across
/// all vault runners and HTTP handlers. Vault runners call `publish`; HTTP
/// streaming handlers call `subscribe`.
pub struct EventBus {
    sender: broadcast::Sender<StreamEvent>,
}

impl EventBus {
    /// Create a new bus with the default `BUS_CAPACITY`.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BUS_CAPACITY);
        Self { sender }
    }

    /// Publish one event to all current subscribers.
    ///
    /// - "No subscribers" is not an error: the bus is expected to exist before
    ///   any subscriber connects. The `send` error variant `SendError` happens
    ///   only when there are no receivers; we silently discard in that case.
    /// - Lagged receivers get `RecvError::Lagged` on their next `recv()`; it
    ///   is the subscriber's responsibility to detect lag and emit
    ///   `StreamLagged` toward their downstream.
    pub fn publish(&self, event: StreamEvent) {
        // `send` returns Err only when there are no active receivers.
        // That is the expected idle state; do not log it.
        let _ = self.sender.send(event);
    }

    /// Subscribe to the event stream. Returns a `broadcast::Receiver` that
    /// starts receiving events published after this call.
    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn vault_id() -> VaultId {
        VaultId::from_string("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string())
    }

    // -----------------------------------------------------------------------
    // EventType serialization
    // -----------------------------------------------------------------------

    #[test]
    fn event_type_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&EventType::Created).unwrap(),
            "\"created\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::Modified).unwrap(),
            "\"modified\""
        );
        assert_eq!(
            serde_json::to_string(&EventType::Deleted).unwrap(),
            "\"deleted\""
        );
    }

    #[test]
    fn event_type_round_trips() {
        for variant in [EventType::Created, EventType::Modified, EventType::Deleted] {
            let s = serde_json::to_string(&variant).unwrap();
            let back: EventType = serde_json::from_str(&s).unwrap();
            assert_eq!(variant, back);
        }
    }

    // -----------------------------------------------------------------------
    // FileChangedEvent wire shape
    // -----------------------------------------------------------------------

    #[test]
    fn file_changed_event_has_top_level_type_field() {
        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Modified,
            "notes/a.md".to_string(),
            Some("sha256:abc".to_string()),
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"type\":\"file_changed\""),
            "expected top-level type=file_changed; got: {s}"
        );
    }

    #[test]
    fn file_changed_event_carries_event_type_field() {
        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Created,
            "notes/a.md".to_string(),
            None,
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"event_type\":\"created\""),
            "expected event_type=created; got: {s}"
        );
    }

    #[test]
    fn file_changed_event_carries_vault_id_not_vault_name() {
        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Modified,
            "notes/a.md".to_string(),
            None,
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"vault\":\"018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0\""),
            "expected vault id in event; got: {s}"
        );
        assert!(
            !s.contains("vault_name"),
            "event must NOT carry vault_name; got: {s}"
        );
    }

    #[test]
    fn content_hash_some_serializes_field() {
        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Modified,
            "notes/a.md".to_string(),
            Some("sha256:abc".to_string()),
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"content_hash\":\"sha256:abc\""),
            "expected content_hash; got: {s}"
        );
    }

    #[test]
    fn content_hash_none_is_omitted() {
        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Deleted,
            "notes/a.md".to_string(),
            None,
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            !s.contains("content_hash"),
            "content_hash should be omitted when None; got: {s}"
        );
    }

    #[test]
    fn file_changed_event_round_trips() {
        for variant in [EventType::Created, EventType::Modified, EventType::Deleted] {
            let ev = StreamEvent::file_changed(
                vault_id(),
                variant,
                "notes/a.md".to_string(),
                Some("sha256:deadbeef".to_string()),
            );
            let s = serde_json::to_string(&ev).unwrap();
            let back: StreamEvent = serde_json::from_str(&s).unwrap();
            assert_eq!(ev, back);
        }
    }

    #[test]
    fn file_changed_now_produces_rfc3339_detected_at() {
        let ev = FileChangedEvent::now(vault_id(), EventType::Created, "a.md".to_string(), None);
        DateTime::parse_from_rfc3339(&ev.detected_at)
            .unwrap_or_else(|e| panic!("detected_at not RFC3339: {} ({e})", ev.detected_at));
    }

    // -----------------------------------------------------------------------
    // StreamLaggedEvent wire shape
    // -----------------------------------------------------------------------

    #[test]
    fn stream_lagged_event_has_top_level_type_field() {
        let ev = StreamEvent::stream_lagged_for_vault(vault_id(), Some(5));
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"type\":\"stream_lagged\""),
            "expected type=stream_lagged; got: {s}"
        );
    }

    #[test]
    fn stream_lagged_event_action_is_resync_required() {
        let ev = StreamEvent::stream_lagged_for_vault(vault_id(), Some(1));
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"action\":\"resync_required\""),
            "expected action=resync_required; got: {s}"
        );
    }

    #[test]
    fn stream_lagged_missed_none_is_omitted() {
        let ev = StreamEvent::stream_lagged_for_vault(vault_id(), None);
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            !s.contains("\"missed\""),
            "missed should be omitted when None; got: {s}"
        );
    }

    #[test]
    fn stream_lagged_event_round_trips() {
        let ev = StreamEvent::stream_lagged_for_vault(vault_id(), Some(42));
        let s = serde_json::to_string(&ev).unwrap();
        let back: StreamEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, back);
    }

    // -----------------------------------------------------------------------
    // EventBus
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn publish_without_subscribers_does_not_panic() {
        let bus = EventBus::new();
        // No subscribers — must succeed silently.
        bus.publish(StreamEvent::file_changed(
            vault_id(),
            EventType::Created,
            "notes/a.md".to_string(),
            None,
        ));
    }

    #[tokio::test]
    async fn subscribe_receives_published_event() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Modified,
            "notes/b.md".to_string(),
            Some("sha256:1".to_string()),
        );
        bus.publish(ev.clone());

        let received = rx.recv().await.expect("should receive event");
        assert_eq!(received, ev);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_receive_event() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let ev = StreamEvent::file_changed(
            vault_id(),
            EventType::Deleted,
            "notes/c.md".to_string(),
            None,
        );
        bus.publish(ev.clone());

        assert_eq!(rx1.recv().await.unwrap(), ev);
        assert_eq!(rx2.recv().await.unwrap(), ev);
    }

    #[tokio::test]
    async fn subscribe_after_publish_does_not_receive_past_event() {
        let bus = EventBus::new();

        // Publish before any subscriber exists.
        bus.publish(StreamEvent::file_changed(
            vault_id(),
            EventType::Created,
            "past.md".to_string(),
            None,
        ));

        // Subscribe after the publish.
        let mut rx = bus.subscribe();

        // Publish a second event.
        let ev2 = StreamEvent::file_changed(
            vault_id(),
            EventType::Modified,
            "future.md".to_string(),
            None,
        );
        bus.publish(ev2.clone());

        // Should receive only the second event.
        let received = rx.recv().await.unwrap();
        assert_eq!(received, ev2);
    }
}
