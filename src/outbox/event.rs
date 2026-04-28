use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::vault_registry::VaultId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeEvent {
    pub vault: VaultId,
    pub event_type: EventType,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub detected_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventType {
    Created,
    Modified,
    Deleted,
}

impl ChangeEvent {
    pub fn now(
        vault: VaultId,
        event_type: EventType,
        path: String,
        content_hash: Option<String>,
    ) -> Self {
        Self {
            vault,
            event_type,
            path,
            content_hash,
            detected_at: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn vault() -> VaultId {
        VaultId::from_string("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string())
    }

    #[test]
    fn event_type_serializes_with_lowercase_discriminants() {
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
    fn event_type_round_trips_through_serde_json() {
        for variant in [EventType::Created, EventType::Modified, EventType::Deleted] {
            let s = serde_json::to_string(&variant).unwrap();
            let back: EventType = serde_json::from_str(&s).unwrap();
            assert_eq!(variant, back);
        }
    }

    #[test]
    fn content_hash_some_serializes_field() {
        let ev = ChangeEvent {
            vault: vault(),
            event_type: EventType::Modified,
            path: "notes/a.md".to_string(),
            content_hash: Some("sha256:abc".to_string()),
            detected_at: "2026-04-25T00:00:00.000000Z".to_string(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("\"content_hash\":\"sha256:abc\""), "got: {s}");
    }

    #[test]
    fn content_hash_none_is_omitted_not_null() {
        let ev = ChangeEvent {
            vault: vault(),
            event_type: EventType::Deleted,
            path: "notes/a.md".to_string(),
            content_hash: None,
            detected_at: "2026-04-25T00:00:00.000000Z".to_string(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            !s.contains("content_hash"),
            "field should be omitted, got: {s}"
        );
        assert!(!s.contains("null"), "no null sentinel, got: {s}");
    }

    #[test]
    fn change_event_round_trips_for_all_event_types() {
        for variant in [EventType::Created, EventType::Modified, EventType::Deleted] {
            let ev = ChangeEvent {
                vault: vault(),
                event_type: variant,
                path: "notes/a.md".to_string(),
                content_hash: Some("sha256:deadbeef".to_string()),
                detected_at: "2026-04-25T12:34:56.789012Z".to_string(),
            };
            let s = serde_json::to_string(&ev).unwrap();
            let back: ChangeEvent = serde_json::from_str(&s).unwrap();
            assert_eq!(ev, back);
        }
    }

    #[test]
    fn now_produces_rfc3339_parseable_detected_at() {
        let ev = ChangeEvent::now(vault(), EventType::Created, "notes/a.md".to_string(), None);
        DateTime::parse_from_rfc3339(&ev.detected_at)
            .unwrap_or_else(|e| panic!("detected_at not RFC3339: {} ({e})", ev.detected_at));
    }

    #[test]
    fn outbox_event_serializes_vault_id_only() {
        // Per workplan § Task 9.6 + ADR-0009: outbox lines carry `vault: <id>`
        // but never `vault_name` (durable channel; names rot). This test pins
        // the no-name invariant so a future drift adding a name field shows
        // up as a clear assertion failure.
        let id = VaultId::from_string("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string());
        let ev = ChangeEvent::now(
            id,
            EventType::Modified,
            "notes/a.md".to_string(),
            Some("sha256:abc".to_string()),
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"vault\":"),
            "outbox line must carry vault id; got: {s}"
        );
        assert!(
            !s.contains("vault_name"),
            "outbox line must NOT carry vault_name; got: {s}"
        );
    }

    #[test]
    fn outbox_event_carries_vault_id() {
        // Per workplan § Task 9.4: every outbox event line carries the
        // vault: <id> field. This test pins the wire shape against a known
        // vault_id string so a future regression on the field name or shape
        // shows up as a clear assertion failure.
        let id = VaultId::from_string("018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0".to_string());
        let ev = ChangeEvent::now(
            id.clone(),
            EventType::Created,
            "notes/a.md".to_string(),
            Some("sha256:abc".to_string()),
        );
        let s = serde_json::to_string(&ev).unwrap();
        assert!(
            s.contains("\"vault\":\"018f3a7c-9b4e-7d2a-95f1-c8a6e3b2d1f0\""),
            "outbox line must carry vault id; got: {s}"
        );
        let back: ChangeEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back.vault, id);
    }
}
