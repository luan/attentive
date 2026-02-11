//! Telemetry record types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A turn record capturing context routing performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn_id: String,
    pub session_id: String,
    pub project: String,
    pub timestamp: DateTime<Utc>,
    pub injected_tokens: usize,
    pub used_tokens: usize,
    pub waste_ratio: f64,
    #[serde(default)]
    pub files_injected: Vec<String>,
    #[serde(default)]
    pub files_used: Vec<String>,
    #[serde(default)]
    pub was_notification: bool,
    #[serde(default)]
    pub injection_chars: usize,
    #[serde(default)]
    pub context_confidence: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_record_roundtrip() {
        let record = TurnRecord {
            turn_id: "test123".to_string(),
            session_id: "sess456".to_string(),
            project: "/tmp/test".to_string(),
            timestamp: Utc::now(),
            injected_tokens: 1000,
            used_tokens: 600,
            waste_ratio: 0.4,
            files_injected: vec![],
            files_used: vec![],
            was_notification: false,
            injection_chars: 0,
            context_confidence: None,
        };

        let json = serde_json::to_string(&record).unwrap();
        let parsed: TurnRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(record.turn_id, parsed.turn_id);
        assert_eq!(record.injected_tokens, parsed.injected_tokens);
    }

    #[test]
    fn test_turn_record_extended_fields() {
        let record = TurnRecord {
            turn_id: "test123".to_string(),
            session_id: "sess456".to_string(),
            project: "/tmp/test".to_string(),
            timestamp: Utc::now(),
            injected_tokens: 1000,
            used_tokens: 600,
            waste_ratio: 0.4,
            files_injected: vec!["router.rs".to_string(), "config.rs".to_string()],
            files_used: vec!["router.rs".to_string()],
            was_notification: false,
            injection_chars: 5000,
            context_confidence: Some(0.75),
        };

        let json = serde_json::to_string(&record).unwrap();
        let parsed: TurnRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.files_injected, vec!["router.rs", "config.rs"]);
        assert_eq!(parsed.files_used, vec!["router.rs"]);
        assert!(!parsed.was_notification);
        assert_eq!(parsed.injection_chars, 5000);
        assert_eq!(parsed.context_confidence, Some(0.75));
    }

    #[test]
    fn test_turn_record_backwards_compatible() {
        let old_json = r#"{"turn_id":"t1","session_id":"s1","project":"/tmp","timestamp":"2025-01-01T00:00:00Z","injected_tokens":100,"used_tokens":50,"waste_ratio":0.5}"#;
        let parsed: TurnRecord = serde_json::from_str(old_json).unwrap();
        assert!(parsed.files_injected.is_empty());
        assert!(parsed.files_used.is_empty());
        assert!(!parsed.was_notification);
        assert_eq!(parsed.injection_chars, 0);
        assert_eq!(parsed.context_confidence, None);
    }
}
