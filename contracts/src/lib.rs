//! Shared HTTP API types between `backend` and `client`.

use chrono::{DateTime, NaiveDate, Utc};
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
    /// IANA timezone of the device that recorded this, e.g.
    /// `Europe/Berlin`. Sent by the client because "tomorrow" means a
    /// different day depending on where the speaker was standing — and
    /// the answer has to stay right after they fly somewhere. Falls back
    /// to the server's configured timezone when absent.
    #[serde(default)]
    pub timezone: Option<String>,
    /// When the note was *written*, if that is not now.
    ///
    /// A capture spoken and sent in one breath leaves this out and gets
    /// the moment it arrived. A note written during a four-hour session
    /// and sent at the end must carry its own moment, or every note in
    /// that session claims to have been thought when Send was pressed —
    /// and the timeline, which is most of what this system promises, ends
    /// up full of times that never happened.
    ///
    /// Refused if it is in the future: a note cannot have been written
    /// later than it arrived, and a device whose clock says otherwise is
    /// a device whose timestamps cannot be trusted at all.
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
    /// The sitting this note was written in, if it was written in one.
    #[serde(default)]
    pub session: Option<CaptureSession>,
}

/// A writing session: one sitting, many notes, one Send at the end.
///
/// A label on the captures and nothing more. A session is deliberately not
/// an entity and never appears in the graph — the extraction finds
/// entities in what was *said*, and a heading typed into a text field was
/// not said. Inventing a node for it would be the interface writing a
/// guess into the graph, which is the line `docs/product.md` draws at P12.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSession {
    pub id: Uuid,
    /// What the writer called it. Ordinary prose, possibly naming people,
    /// so it is stored as content rather than in the event payload — the
    /// log is never modified and anything written there could never be
    /// redacted (ADR 0005).
    pub title: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureAccepted {
    pub event_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    /// Earlier captures that are semantically close to the one just
    /// recorded. Empty when nothing clears the threshold — which is the
    /// common case early on — **and** while `echo_pending` is true.
    pub echo: Vec<EchoItem>,
    /// The echo is still being judged and will arrive shortly; ask
    /// `GET /captures/{id}/echo` for it.
    ///
    /// Judging used to happen inline, which meant the capture was not
    /// confirmed as stored until it finished: 28 seconds against the NAS,
    /// for a note that had been safely written after 250 ms. It now runs
    /// behind the response, so this flag is how the client knows to show
    /// "looking for echoes" rather than "nothing echoed".
    #[serde(default)]
    pub echo_pending: bool,
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
    /// Cosine similarity in [0, 1]; higher is closer. This is the
    /// retrieval score, not the one the echo was judged by.
    pub similarity: f32,
    /// The judge's verdict, when one ran. Not comparable between judges —
    /// raw logits for the local cross-encoder, a 0-1 relevance for a
    /// hosted one — which is why it is never shown to the user. It is
    /// here so the threshold can be tuned by looking at real captures.
    #[serde(default)]
    pub rerank_score: Option<f32>,
}

/// The entity graph, in one piece.
///
/// Sent whole rather than walked entity by entity: a graph is the one
/// view whose whole point is what it looks like *together*, and fetching
/// it a node at a time would mean the layout settles while the data is
/// still arriving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Entities that exist but were left out because the graph was
    /// capped. Shown as a number rather than hidden, so a graph that is
    /// only part of the picture never pretends to be all of it.
    pub omitted_nodes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: Uuid,
    pub name: String,
    pub entity_type: String,
    /// How often this entity has been observed. Drives how large it is
    /// drawn — the things you keep coming back to should be the things
    /// you see first.
    pub mention_count: i64,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: Uuid,
    pub to: Uuid,
    pub relation_type: String,
    /// How many separate captures assert this same relation. One is a
    /// passing remark; five is something you keep saying.
    pub weight: i64,
}

/// What a redaction actually removed.
///
/// Returned so the confirmation can say something true rather than a
/// generic "deleted" — a capture that had no entities derived from it and
/// a capture that anchored half the graph are very different deletions,
/// and the person doing it deserves to know which one just happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Redacted {
    pub event_id: Uuid,
    pub redacted_at: DateTime<Utc>,
    /// Derived observations that went with it.
    pub observations_removed: i64,
    /// Derived relations that went with it.
    pub relations_removed: i64,
    /// Entities that existed only because of this capture and are now
    /// gone too. Entities mentioned elsewhere are untouched.
    pub entities_removed: i64,
    /// Whether an original recording was deleted from disk.
    pub audio_removed: bool,
}

/// A capture's echo, and whether it is final.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoResponse {
    pub items: Vec<EchoItem>,
    /// True while the judgement is still running. An empty `items` with
    /// this set means "not yet"; an empty `items` without it means
    /// "nothing echoed", which is a real and common answer.
    pub pending: bool,
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
    /// As on `CaptureAccepted`: the echo is still being judged. Opening a
    /// capture recorded before this mechanism existed sets it once, while
    /// the backfill runs.
    #[serde(default)]
    pub echo_pending: bool,
    /// Everything this capture caused, in order — the capture event itself,
    /// its transcripts, and the entity/relation events derived from it.
    pub events: Vec<EventRecord>,
}

/// Body of `POST /captures/{id}/transcript` — a human fixing what the
/// machine (or their own typing) got wrong.
///
/// The correction never overwrites anything: it is appended as one more
/// transcript, superseding the previous one. The original stays readable
/// forever, which is the whole reason the transcript is a separate thing
/// from the capture (ADR 0004).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectTranscriptRequest {
    pub text: String,
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
    /// The day this observation is *about*, when it is about one —
    /// "morgen" resolved against the moment the note was spoken. Distinct
    /// from `occurred_at`, which is when it was said.
    pub happened_on: Option<NaiveDate>,
    /// Only set when a time of day was actually named.
    pub happened_at: Option<DateTime<Utc>>,
    /// How precise the above really is: "time", "day", "week", "month"
    /// or "year".
    pub happened_precision: Option<String>,
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

/// One row of the entity index: a thing the system has noticed, with
/// enough weight attached to tell a passing mention from a recurring
/// subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityListItem {
    pub id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub current_summary: Option<String>,
    /// How many captures have said something about it.
    pub mention_count: i64,
    /// When it was last spoken about. `None` for an entity that exists
    /// only as the far end of a relation.
    pub last_seen: Option<DateTime<Utc>>,
}

/// An entity's own page: everything ever observed about it, and what it
/// stands in relation to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDetail {
    pub id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub current_summary: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Newest first: an entity is read from what was last said about it.
    pub mentions: Vec<EntityCapture>,
    pub relations: Vec<EntityEdge>,
}

/// A capture that said something about an entity, with the observation
/// the model drew from it. Both are shown: the observation is the
/// model's reading, the transcript is what was actually said.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityCapture {
    pub capture_event_id: Uuid,
    pub transcript_text: String,
    pub occurred_at: DateTime<Utc>,
    pub observation: String,
    pub model: String,
    /// The day this observation is *about*, when it is about one —
    /// "morgen" resolved against the moment the note was spoken. Distinct
    /// from `occurred_at`, which is when it was said.
    pub happened_on: Option<NaiveDate>,
    /// Only set when a time of day was actually named.
    pub happened_at: Option<DateTime<Utc>>,
    /// How precise the above really is: "time", "day", "week", "month"
    /// or "year".
    pub happened_precision: Option<String>,
}

/// An edge from this entity's point of view. `outgoing` is false when
/// this entity is the target — the relation still reads the right way
/// round, it just points the other way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEdge {
    pub relation_type: String,
    pub outgoing: bool,
    pub other_id: Uuid,
    pub other_name: String,
    pub other_type: String,
    /// The capture this edge was read out of. `None` when it was drawn
    /// across several captures by the consolidation run — which is the
    /// only way an edge between two things that were never mentioned in
    /// one breath can exist at all.
    pub source_event_id: Option<Uuid>,
}

/// What the system puts in front of you without being asked.
///
/// Everything else in the app answers a question. This answers none: it
/// is the only surface where knowledge arrives rather than being
/// retrieved, which is the difference between a memory and an archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resurfaced {
    /// Things you said were coming, that have not happened yet.
    pub upcoming: Vec<UpcomingItem>,
    /// Subjects you keep returning to across separate captures. Ordered
    /// by when they were last spoken about, so a thread that has gone
    /// quiet is visible as such.
    pub threads: Vec<ThreadItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingItem {
    pub entity_id: Uuid,
    pub entity_name: String,
    pub entity_type: String,
    pub observation: String,
    pub happened_on: NaiveDate,
    pub happened_at: Option<DateTime<Utc>>,
    pub happened_precision: Option<String>,
    pub capture_event_id: Uuid,
    /// When you said it — as opposed to when it is about.
    pub said_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadItem {
    pub entity_id: Uuid,
    pub entity_type: String,
    pub name: String,
    pub current_summary: Option<String>,
    /// How many separate captures have touched it. Two is the threshold
    /// for being a thread at all — one is just a note.
    pub capture_count: i64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
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

/// What the backend has not finished yet — the answer to
/// `GET /pipeline`.
///
/// Exists so the client can say so out loud. Both numbers are normally
/// zero, and a surface that only ever shows zero is a surface nobody
/// reads; the point is the moment they are not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStatus {
    /// Captures stored but not yet embedded or structured. Usually a
    /// number that is briefly one and then zero again.
    pub waiting: i64,
    /// Captures the retry loop gave up on. These never resolve by
    /// themselves — that is the whole meaning of the number.
    pub given_up: i64,
}

/// One change the consolidation run made to how knowledge is organised —
/// the answer to `GET /consolidation`.
///
/// This is the changelog the whole design rests on. The captures are
/// immutable; what changes is the *view* of them, and a view that
/// rearranges itself silently is not trustworthy however correct it is.
/// So every merge, rename, retype and derived edge shows up here, in the
/// person's own terms, with the reason the run gave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    /// "merged", "renamed", "retyped" or "related".
    pub kind: String,
    /// The entity the change is about — the surviving one, for a merge.
    pub entity_id: Uuid,
    pub entity_name: String,
    /// What it was before: the absorbed entity's name, the old name, the
    /// old type. Empty for a new edge.
    pub before: String,
    /// What it is now: the relation's other end, the new name, the new
    /// type. Empty for a merge, where the survivor is `entity_name`.
    pub after: String,
    pub reason: String,
    pub changed_at: DateTime<Utc>,
    /// The id to hand to the undo endpoint, when this change can still be
    /// taken back: the absorbed entity's id for a merge
    /// (`POST /entities/{id}/unmerge`), or the relation's own id for a
    /// derived edge (`DELETE /relations/{id}`).
    #[serde(default)]
    pub undo_id: Option<Uuid>,
    /// True once this change has been taken back.
    pub undone: bool,
}

/// What a consolidation pass would do, if it ran — the answer to
/// `GET /consolidation/preview`.
///
/// The same reasoning the real pass performs, applied and then thrown
/// away. It exists because the pass is automatic: being able to look
/// before it acts is the difference between an automatic tidy-up and an
/// automatic surprise, especially the first time it meets a database it
/// was not developed against.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidationPreview {
    /// Entities that are one thing under two type words. No model
    /// involved; these are exact.
    pub same_name: Vec<PreviewMerge>,
    /// How many entities the model was shown.
    pub considered: i64,
    pub merges: Vec<PreviewMerge>,
    pub retypes: Vec<PreviewRetype>,
    pub relations: Vec<PreviewRelation>,
    /// Set when there was nothing to preview, and why.
    #[serde(default)]
    pub note: Option<String>,
    /// Names this exact preview for `POST /consolidation/apply`. Absent
    /// when nothing was proposed. What gets applied is this stored plan
    /// minus whatever the person removed — never a fresh judgement, which
    /// is what makes removing an item and applying the rest a coherent
    /// thing to do: a second call to the model could easily propose
    /// something different from the first.
    #[serde(default)]
    pub token: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMerge {
    /// This item's key within the preview, for `POST /consolidation/apply`'s
    /// `exclude` list.
    pub id: String,
    pub keep: String,
    pub absorb: Vec<String>,
    /// What the survivor would be called afterwards, when the run would
    /// rename it.
    #[serde(default)]
    pub new_name: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRetype {
    pub id: String,
    pub entity: String,
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewRelation {
    pub id: String,
    pub from: String,
    pub to: String,
    pub relation_type: String,
    pub reason: String,
}

/// Body of `POST /consolidation/apply`.
///
/// Applies the plan `GET /consolidation/preview` stored under `token`,
/// skipping whatever item ids are named in `exclude`. Never re-asks the
/// model — that is the entire point of the token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationApplyRequest {
    pub token: Uuid,
    #[serde(default)]
    pub exclude: Vec<String>,
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
        let written = DateTime::parse_from_rfc3339("2026-09-23T10:31:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let req = CreateCaptureRequest {
            transcript_text: "test".into(),
            device: "unit-test".into(),
            timezone: Some("Europe/Berlin".into()),
            occurred_at: Some(written),
            session: Some(CaptureSession {
                id: Uuid::nil(),
                title: "Workshop".into(),
                started_at: written,
            }),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CreateCaptureRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.transcript_text, req.transcript_text);
        assert_eq!(back.occurred_at, Some(written));
        assert_eq!(back.session.map(|s| s.title).as_deref(), Some("Workshop"));
    }

    /// A client that predates the session — and every spoken capture, which
    /// has no written time and no sitting — must still be accepted.
    #[test]
    fn a_capture_without_a_written_time_still_parses() {
        let req: CreateCaptureRequest =
            serde_json::from_str(r#"{"transcript_text":"test","device":"mac"}"#).unwrap();
        assert!(req.occurred_at.is_none());
        assert!(req.session.is_none());
    }
}
