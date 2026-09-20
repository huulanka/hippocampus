//! Shared HTTP API types between `backend` and `client`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Body of `POST /captures` — a raw, verbatim capture from a client.
///
/// This is the only write path for original knowledge. Once accepted, a
/// capture is never edited or deleted; corrections happen by adding new
/// derived data, never by mutating this record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCaptureRequest {
    pub transcript_text: String,
    pub device: String,
    /// Opaque reference to the audio file on the originating device/NAS,
    /// if one exists (e.g. a content hash or storage path). Audio itself
    /// is never uploaded to the backend.
    pub audio_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureAccepted {
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
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
    #[serde(default)]
    pub entity_type: Option<String>,
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
    fn create_capture_request_round_trips() {
        let req = CreateCaptureRequest {
            transcript_text: "test".into(),
            device: "unit-test".into(),
            audio_ref: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateCaptureRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.transcript_text, req.transcript_text);
    }
}
