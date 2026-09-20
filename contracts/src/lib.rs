//! Shared HTTP API types between `backend` and `client`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Body of `POST /captures` — a typed or dictated capture.
///
/// Here the text is the original: nothing derived it, so it is stored as
/// capture content directly. Spoken captures go to `POST /captures/audio`
/// instead, as multipart, because there the recording is the original and
/// the transcript is already an interpretation of it (ADR 0004).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaptureRequest {
    pub transcript_text: String,
    pub device: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureAccepted {
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    /// Earlier captures that are semantically close to the one just
    /// recorded, returned inline so the client can show them immediately
    /// without a second round trip. Empty when nothing clears the
    /// similarity threshold — which is the common case early on.
    pub echo: Vec<EchoItem>,
}

/// One earlier capture surfaced as an echo. Deliberately carries the
/// verbatim transcript and its date rather than a generated summary: the
/// whole point is to show the user their own words back, never a
/// paraphrase that could be wrong.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoItem {
    pub capture_event_id: Uuid,
    pub transcript_text: String,
    pub occurred_at: DateTime<Utc>,
    /// Cosine similarity in [0, 1]; higher is closer.
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySummary {
    pub id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub current_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub capture_event_id: Uuid,
    pub transcript_text: String,
    pub occurred_at: DateTime<Utc>,
    pub score: f32,
    pub related_entities: Vec<EntitySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    /// Restrict to captures that produced at least one entity of this type.
    #[serde(default)]
    pub entity_type: Option<String>,
    /// Inclusive lower bound on capture time. "When" is the primary
    /// retrieval key in a memory system, so both bounds are first-class.
    #[serde(default)]
    pub from: Option<DateTime<Utc>>,
    /// Inclusive upper bound on capture time.
    #[serde(default)]
    pub to: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_defaults_limit_when_omitted() {
        let query: SearchQuery = serde_json::from_str(r#"{"query": "lena"}"#).unwrap();
        assert_eq!(query.limit, 20);
        assert_eq!(query.entity_type, None);
    }

    #[test]
    fn search_query_parses_time_bounds() {
        let query: SearchQuery =
            serde_json::from_str(r#"{"query": "x", "from": "2026-01-01T00:00:00Z"}"#).unwrap();
        assert!(query.from.is_some());
        assert!(query.to.is_none());
    }

    #[test]
    fn create_capture_request_round_trips() {
        let req = CreateCaptureRequest {
            transcript_text: "test".into(),
            device: "unit-test".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateCaptureRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.transcript_text, req.transcript_text);
    }
}
