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

/// Everything known about a single capture, gathered for the detail view.
///
/// Deliberately includes the derived material (entities, relations, the
/// event log) alongside the verbatim text: when the structuring gets
/// something wrong, the only way to see *why* is to see what it produced.
/// The one thing left out is the embedding — 384 floats tell a human
/// nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureDetail {
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    /// "audio" when a recording is the original, "text" when the typed
    /// words are (ADR 0004).
    pub origin: String,
    pub device: String,
    /// The transcript as it currently reads: the newest correction if one
    /// exists, otherwise the original. `None` once the content has been
    /// redacted — the capture's existence and time survive, its words do not.
    pub text: Option<String>,
    pub redacted: bool,
    pub audio: Option<AudioDetail>,
    /// Every transcript ever produced for this capture, oldest first.
    /// Usually one; more than one means it was corrected.
    pub transcripts: Vec<TranscriptVersion>,
    pub entities: Vec<EntityMention>,
    pub relations: Vec<RelationMention>,
    pub echo: Vec<EchoItem>,
    /// Everything this capture caused, in order — the capture event itself,
    /// its transcripts, and the entity/relation events derived from it.
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDetail {
    pub mime: String,
    pub duration_ms: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptVersion {
    pub event_id: Uuid,
    pub text: String,
    /// The ASR model that produced it, or "user" for a human correction.
    pub model: String,
    pub language: Option<String>,
    pub created_at: DateTime<Utc>,
    pub supersedes: Option<Uuid>,
}

/// An entity this capture spoke about, with the observation the model drew
/// from it. The observation, not the entity name, is what makes a wrong
/// extraction recognisable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMention {
    pub id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub observation: String,
    pub model: String,
    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationMention {
    pub id: Uuid,
    pub from_entity_id: Uuid,
    pub from_name: String,
    pub to_entity_id: Uuid,
    pub to_name: String,
    pub relation_type: String,
    pub model: String,
}

/// One row of the append-only log, passed through as stored. The payload
/// stays untyped on purpose: the detail view shows it raw, and inventing a
/// Rust enum over every event type would need changing every time a new
/// one is appended.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: Uuid,
    pub event_type: String,
    pub source: String,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
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

    /// The detail view's payload passes through untyped, which is the one
    /// place a serde mistake would not be caught by the compiler.
    #[test]
    fn event_record_keeps_its_payload_verbatim() {
        let record: EventRecord = serde_json::from_str(
            r#"{
                "id": "00000000-0000-0000-0000-000000000001",
                "event_type": "relation.proposed",
                "source": "gemini",
                "occurred_at": "2026-01-01T00:00:00Z",
                "payload": {"relation_type": "arbeitet_in", "nested": {"a": [1, 2]}}
            }"#,
        )
        .unwrap();

        assert_eq!(record.payload["relation_type"], "arbeitet_in");
        assert_eq!(record.payload["nested"]["a"][1], 2);
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
