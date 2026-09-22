use axum::Json;
use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use contracts::{
    AudioDetail, CaptureAccepted, CaptureDetail, CorrectTranscriptRequest, CreateCaptureRequest,
    EchoItem, EchoResponse, EntityMention, EventRecord, Redacted, RelationMention,
    TranscriptVersion,
};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::audio;
use crate::echo;
use crate::error::AppError;
use crate::events;
use crate::openrouter::SpokenAt;
use crate::pipeline;
use crate::structuring;

/// Resolves which timezone a capture's relative time expressions should be
/// read in: what the device said, or the server's fallback when the device
/// said nothing or something unparseable.
fn timezone_for(state: &AppState, claimed: Option<&str>) -> chrono_tz::Tz {
    claimed
        .and_then(|name| match name.parse::<chrono_tz::Tz>() {
            Ok(tz) => Some(tz),
            Err(_) => {
                tracing::warn!(%name, "capture claimed an unknown timezone, using the server's");
                None
            }
        })
        .unwrap_or(state.timezone)
}

/// Records a typed or dictated capture. Here the text *is* the original —
/// nothing derived it — so it is stored as capture content directly, with
/// no transcript event above it (ADR 0004).
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateCaptureRequest>,
) -> Result<Json<CaptureAccepted>, AppError> {
    let transcript = req.transcript_text.trim();
    if transcript.is_empty() {
        return Err(AppError::bad_request("a capture cannot be empty"));
    }

    let timezone = timezone_for(&state, req.timezone.as_deref());

    let stored = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "capture.recorded",
        // The text itself deliberately does not go into the payload: the
        // event log is never modified, so anything written here could never
        // be redacted later (ADR 0005).
        &json!({
            "origin": "text",
            "device": req.device,
            "timezone": timezone.name(),
        }),
        &req.device,
    )
    .await?;

    sqlx::query!(
        r#"insert into capture_content (event_id, origin, text) values ($1, 'text', $2)"#,
        stored.id,
        transcript,
    )
    .execute(&state.pool)
    .await?;

    index_capture(
        &state,
        stored.id,
        stored.occurred_at,
        transcript,
        SpokenAt {
            utc: stored.occurred_at,
            timezone,
        },
    )
    .await;

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
        echo: Vec::new(),
        echo_pending: true,
    }))
}

/// Records a spoken capture: the audio plus the transcript the client
/// produced from it on-device.
///
/// The audio is the original and is kept whole; the transcript arrives as
/// a separate `transcript.derived` event carrying the model that produced
/// it, because it is already an interpretation and a better model may
/// disagree with it later (ADR 0004).
pub async fn create_from_audio(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<CaptureAccepted>, AppError> {
    let mut audio_bytes: Option<Vec<u8>> = None;
    let mut audio_mime = String::new();
    let mut device = String::new();
    let mut transcript = String::new();
    let mut model = String::new();
    let mut language: Option<String> = None;
    let mut duration_ms: Option<i32> = None;
    let mut timezone_name: Option<String> = None;

    while let Some(field) = multipart.next_field().await? {
        match field.name().unwrap_or_default() {
            "audio" => {
                audio_mime = field
                    .content_type()
                    .map(str::to_owned)
                    .unwrap_or_else(|| "audio/wav".to_string());
                let bytes = field.bytes().await?;
                if bytes.len() > audio::MAX_BYTES {
                    return Err(AppError::bad_request(format!(
                        "recording is {} bytes, over the {} byte limit",
                        bytes.len(),
                        audio::MAX_BYTES
                    )));
                }
                audio_bytes = Some(bytes.to_vec());
            }
            "device" => device = field.text().await?,
            "transcript" => transcript = field.text().await?,
            "model" => model = field.text().await?,
            "language" => language = Some(field.text().await?).filter(|l| !l.is_empty()),
            "duration_ms" => duration_ms = field.text().await?.parse().ok(),
            "timezone" => timezone_name = Some(field.text().await?).filter(|t| !t.is_empty()),
            other => tracing::debug!(field = %other, "ignoring unexpected multipart field"),
        }
    }

    let Some(bytes) = audio_bytes else {
        return Err(AppError::bad_request("no audio part in the upload"));
    };
    let Some(extension) = audio::extension_for(&audio_mime) else {
        return Err(AppError::bad_request(format!(
            "unsupported audio type: {audio_mime}"
        )));
    };
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        return Err(AppError::bad_request(
            "audio capture arrived without a transcript",
        ));
    }
    if device.is_empty() {
        device = "unknown".to_string();
    }

    let timezone = timezone_for(&state, timezone_name.as_deref());

    // Written to disk before any row references it, so a failure here
    // cannot leave a capture pointing at audio that does not exist.
    let audio_path = audio::store(&state.audio_dir, &bytes, extension).await?;

    let stored = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "capture.recorded",
        &json!({
            "origin": "audio",
            "device": device,
            "audio_path": audio_path,
            "audio_mime": audio_mime,
            "duration_ms": duration_ms,
            "timezone": timezone.name(),
        }),
        &device,
    )
    .await?;

    sqlx::query!(
        r#"
        insert into capture_content (event_id, origin, audio_path, audio_mime, duration_ms)
        values ($1, 'audio', $2, $3, $4)
        "#,
        stored.id,
        audio_path,
        audio_mime,
        duration_ms,
    )
    .execute(&state.pool)
    .await?;

    let model = if model.is_empty() {
        "unknown-asr".to_string()
    } else {
        model
    };

    let transcript_event = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "transcript.derived",
        &json!({
            "capture_event_id": stored.id,
            "model": model,
            "language": language,
        }),
        &model,
    )
    .await?;

    sqlx::query!(
        r#"
        insert into transcript_content (event_id, capture_event_id, text, model, language)
        values ($1, $2, $3, $4, $5)
        "#,
        transcript_event.id,
        stored.id,
        transcript,
        model,
        language,
    )
    .execute(&state.pool)
    .await?;

    index_capture(
        &state,
        stored.id,
        stored.occurred_at,
        &transcript,
        SpokenAt {
            utc: stored.occurred_at,
            timezone,
        },
    )
    .await;

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
        echo: Vec::new(),
        echo_pending: true,
    }))
}

/// Embeds a capture and puts it in the search index.
///
/// Split out so the retry loop can do exactly this much and no more: it
/// is the only part of finishing a capture that is local and free, and it
/// is the part everything else — search, echo, the entity pages' links
/// back — is built on.
pub(crate) async fn index_for_search(
    state: &AppState,
    capture_event_id: Uuid,
    occurred_at: chrono::DateTime<chrono::Utc>,
    transcript: &str,
) -> Result<(), AppError> {
    let embedding: pgvector::Vector = state.embedder.embed_passage(transcript).await?.into();

    sqlx::query!(
        r#"
        insert into capture_search (event_id, transcript, occurred_at, embedding)
        values ($1, $2, $3, $4)
        on conflict (event_id) do update
            set transcript = excluded.transcript, embedding = excluded.embedding
        "#,
        capture_event_id,
        transcript,
        occurred_at,
        embedding as _,
    )
    .execute(&state.pool)
    .await?;

    Ok(())
}

/// Everything that happens to a capture's text once it exists, whatever
/// produced it: make it findable, structure it, and start judging its
/// echoes.
///
/// Nothing here waits for a model. Structuring never did; echo used to,
/// and that inline wait was the whole of the 28 seconds a capture took
/// against the NAS — for a note that was already safely stored after a
/// quarter of a second. The echo now arrives behind the response, and
/// [`super::AppState`] remembers it so it is never computed twice.
///
/// And nothing here can fail the caller, which is the other half of the
/// point. This used to be called with `?`, so a failed embedding answered
/// a stored capture with a 500 — telling the client its note had not been
/// saved when it had. A client that retries on failure turns that into a
/// duplicate note, and a client that does not turns it into a note the
/// user believes is lost. Both are worse than a capture that is briefly
/// not yet searchable, which is what happens now: the work stays on
/// `capture_pipeline` and [`crate::pipeline::watch`] comes back for it.
async fn index_capture(
    state: &AppState,
    capture_event_id: Uuid,
    occurred_at: chrono::DateTime<chrono::Utc>,
    transcript: &str,
    spoken_at: SpokenAt,
) {
    if let Err(err) = pipeline::begin(&state.pool, capture_event_id).await {
        // Worth saying loudly: without this row the retry loop cannot
        // see the capture, so a failure below would go back to being
        // silent — the exact thing this is here to stop.
        tracing::error!(
            ?err,
            %capture_event_id,
            "could not record that this capture still needs finishing"
        );
    }

    match index_for_search(state, capture_event_id, occurred_at, transcript).await {
        Ok(()) => pipeline::mark_indexed(&state.pool, capture_event_id).await,
        Err(err) => {
            tracing::error!(
                ?err,
                %capture_event_id,
                "could not index this capture for search; it will be retried"
            );
            // Structuring is not started either. It reads the same text
            // through the same database, so a failure here almost always
            // means the next call fails too — and it would spend money
            // finding that out.
            return;
        }
    }

    // Structuring never blocks the capture — the original is already
    // safely stored and stays re-derivable. What is new is that failing
    // is recorded rather than only logged.
    tokio::spawn(structuring::structure_capture_in_background(
        state.pool.clone(),
        state.openrouter.clone(),
        state.embedder.clone(),
        capture_event_id,
        transcript.to_string(),
        spoken_at,
        state.retry.max_attempts,
    ));

    spawn_judging(state, capture_event_id);
}

/// A capture's echo-judging slot: either a judgement is running for it, or
/// one just failed and is cooling down before it may be retried.
#[derive(Clone, Copy)]
pub enum JudgeSlot {
    InFlight,
    FailedAt(std::time::Instant),
}

/// Judges a capture's echo behind whatever response is being sent, unless
/// a judgement for it is already running or a previous failure is still
/// within its backoff window.
///
/// A failure is logged and otherwise ignored: the marker is only written
/// on success, so the next read of this capture starts a fresh attempt —
/// once the backoff has passed. That is the retry, and it costs nothing
/// when nobody looks. The backoff is what keeps it from costing something
/// on every read while a provider is failing fast (a rate limit, say):
/// without it, the client's 1.2s poll would turn one failing call into
/// several paid calls a second for as long as anyone is watching.
fn spawn_judging(state: &AppState, capture_event_id: Uuid) {
    {
        let Ok(mut slots) = state.judging.lock() else {
            return;
        };
        match slots.get(&capture_event_id) {
            Some(JudgeSlot::InFlight) => return,
            Some(JudgeSlot::FailedAt(at)) if at.elapsed() < state.echo_judge_retry_backoff => {
                return;
            }
            Some(JudgeSlot::FailedAt(_)) | None => {}
        }
        slots.insert(capture_event_id, JudgeSlot::InFlight);
    }

    let pool = state.pool.clone();
    let judge = state.judge.clone();
    let slots = state.judging.clone();
    let thresholds = echo::Thresholds {
        min_similarity: state.echo_min_similarity,
        min_score: state.echo_min_rerank,
    };

    tokio::spawn(async move {
        let result =
            echo::judge_stored_capture(&pool, judge.as_deref(), capture_event_id, thresholds).await;
        let Ok(mut slots) = slots.lock() else {
            return;
        };
        match result {
            Ok(()) => {
                slots.remove(&capture_event_id);
            }
            Err(err) => {
                tracing::warn!(
                    ?err,
                    %capture_event_id,
                    "echo judging failed; it will be retried after a backoff"
                );
                slots.insert(
                    capture_event_id,
                    JudgeSlot::FailedAt(std::time::Instant::now()),
                );
            }
        }
    });
}

/// The stored echo for a capture, starting a judgement if there is none.
///
/// Returns the items and whether a judgement is still outstanding, which
/// is what lets the client say "looking for echoes" instead of showing
/// the empty result as if it were the answer.
async fn stored_echo(state: &AppState, capture_event_id: Uuid) -> (Vec<EchoItem>, bool) {
    match echo::stored(
        &state.pool,
        capture_event_id,
        state.echo_min_rerank,
        echo::DEFAULT_LIMIT,
    )
    .await
    {
        Ok(Some(items)) => (items, false),
        Ok(None) => {
            spawn_judging(state, capture_event_id);
            (Vec::new(), true)
        }
        Err(err) => {
            tracing::warn!(?err, %capture_event_id, "reading the stored echo failed");
            (Vec::new(), false)
        }
    }
}

#[derive(serde::Serialize)]
pub struct CaptureListItem {
    pub event_id: Uuid,
    pub transcript_text: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    /// "audio" or "text" — whether a recording exists to play back.
    pub origin: String,
}

/// Lists recent captures, newest first.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<CaptureListItem>>, AppError> {
    let rows = sqlx::query!(
        r#"
        select cs.event_id, cs.transcript, cs.occurred_at, cc.origin as "origin?"
        from capture_search cs
        left join capture_content cc on cc.event_id = cs.event_id
        order by cs.occurred_at desc
        limit 50
        "#
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| CaptureListItem {
                event_id: r.event_id,
                transcript_text: r.transcript,
                occurred_at: r.occurred_at,
                origin: r.origin.unwrap_or_else(|| "text".to_string()),
            })
            .collect(),
    ))
}

/// Streams back the original recording, so the transcript can always be
/// checked against what was actually said.
pub async fn audio_for(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Response, AppError> {
    let row = sqlx::query!(
        r#"select audio_path, audio_mime, redacted_at from capture_content where event_id = $1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        return Err(AppError::not_found("no such capture"));
    };
    if row.redacted_at.is_some() {
        return Err(AppError::not_found("this capture's content was redacted"));
    }
    let Some(path) = row.audio_path else {
        return Err(AppError::not_found("this capture has no recording"));
    };

    let bytes = tokio::fs::read(audio::resolve(&state.audio_dir, &path)?).await?;
    let mime = row.audio_mime.unwrap_or_else(|| "audio/wav".to_string());

    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[derive(serde::Deserialize)]
pub struct EchoParams {
    /// Override the display threshold, for tuning against real captures
    /// without restarting the backend. The right value drifts as the
    /// corpus grows, and it can only be found by looking:
    /// `?min_rerank=-99` returns every candidate with its score.
    ///
    /// There is deliberately no `min_similarity` any more. That one chose
    /// the *candidate* set, and candidates are settled when a capture is
    /// judged — honouring it per request would mean re-judging, which is
    /// the cost this endpoint exists to avoid.
    pub min_rerank: Option<f32>,
    pub limit: Option<i64>,
}

/// A capture's echo. `POST /captures` starts it; this is where it is
/// collected, and where the timeline goes to re-read it later.
///
/// Thresholds can still be overridden per request, and that now costs
/// nothing: every candidate was stored with its score, so tuning
/// `min_rerank` — or asking for everything with `?min_rerank=-99` to
/// calibrate — re-filters what is already known instead of paying a judge
/// again.
pub async fn echo_for(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Query(params): Query<EchoParams>,
) -> Result<Json<EchoResponse>, AppError> {
    let min_score = params.min_rerank.unwrap_or(state.echo_min_rerank);
    let limit = params.limit.unwrap_or(echo::DEFAULT_LIMIT).clamp(1, 20);

    if let Some(items) = echo::stored(&state.pool, id, min_score, limit).await? {
        return Ok(Json(EchoResponse {
            items,
            pending: false,
        }));
    }

    spawn_judging(&state, id);
    Ok(Json(EchoResponse {
        items: Vec::new(),
        pending: true,
    }))
}

#[derive(serde::Serialize)]
pub struct EntityTypeCount {
    pub entity_type: String,
    pub count: i64,
}

/// Entity types that actually occur, most common first. Drives the search
/// filter chips: the type vocabulary is open (the extraction prompt invents
/// types freely), so the client cannot hard-code a list.
pub async fn entity_types(
    State(state): State<AppState>,
) -> Result<Json<Vec<EntityTypeCount>>, AppError> {
    let rows = sqlx::query!(
        r#"
        select entity_type, count(*) as "count!"
        from entities
        where merged_into is null
        group by entity_type
        order by count(*) desc, entity_type
        "#
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| EntityTypeCount {
                entity_type: r.entity_type,
                count: r.count,
            })
            .collect(),
    ))
}

/// Everything known about one capture, in a single round trip.
///
/// The detail view is where a wrong extraction becomes visible, so this
/// returns the derived material next to the verbatim text rather than a
/// tidied-up summary of it. The embedding is the one thing left out.
pub async fn detail(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<CaptureDetail>, AppError> {
    let head = sqlx::query!(
        r#"
        select
            e.occurred_at,
            e.payload,
            cc.origin as "origin?",
            cc.text as "text?",
            cc.audio_mime as "audio_mime?",
            cc.duration_ms as "duration_ms?",
            cc.redacted_at as "redacted_at?"
        from events e
        left join capture_content cc on cc.event_id = e.id
        where e.id = $1 and e.event_type = 'capture.recorded'
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(head) = head else {
        return Err(AppError::not_found("no such capture"));
    };

    let transcripts = sqlx::query!(
        r#"
        select event_id, text, model, language, created_at, supersedes
        from transcript_content
        where capture_event_id = $1 and redacted_at is null
        order by created_at
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    let redacted = head.redacted_at.is_some();

    let mut versions: Vec<TranscriptVersion> = Vec::with_capacity(transcripts.len() + 1);
    // A typed capture has no transcript row: the text *is* the original and
    // lives on the capture itself (ADR 0004). It still belongs at the head
    // of the chain, otherwise correcting a typed capture would appear to
    // delete what was first written — the one thing this system promises
    // never to do.
    if let Some(original) = head.text.clone().filter(|_| !redacted) {
        versions.push(TranscriptVersion {
            event_id: id,
            text: original,
            model: TYPED_AUTHOR.to_string(),
            language: None,
            created_at: head.occurred_at,
            supersedes: None,
        });
    }
    versions.extend(transcripts.into_iter().map(|t| TranscriptVersion {
        event_id: t.event_id,
        text: t.text,
        model: t.model,
        language: t.language,
        created_at: t.created_at,
        supersedes: t.supersedes,
    }));

    // The newest reading wins.
    let text = if redacted {
        None
    } else {
        versions.last().map(|t| t.text.clone())
    };

    let entities = sqlx::query!(
        r#"
        select
            en.id, en.entity_type, en.name,
            o.text as observation, o.model, o.confidence,
            o.happened_on, o.happened_at, o.happened_precision
        from observations o
        join entities en on en.id = o.entity_id
        where o.source_event_id = $1
        order by o.created_at
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    let relations = sqlx::query!(
        r#"
        select
            r.id,
            r.from_entity_id, ef.name as from_name,
            r.to_entity_id, et.name as to_name,
            r.relation_type, r.model
        from relations r
        join entities ef on ef.id = r.from_entity_id
        join entities et on et.id = r.to_entity_id
        where r.source_event_id = $1
        order by r.created_at
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    // Everything this capture caused: its own stream, the transcripts
    // pointing at it, and the entity/relation events derived from it.
    let events = sqlx::query!(
        r#"
        select id, event_type, source, occurred_at, payload
        from events
        where id = $1
           -- Later events on the capture's own stream, e.g. a redaction.
           -- The stream id is not the event id: `events::append` opens a
           -- fresh stream and Postgres generates the row id separately.
           or stream_id = (select stream_id from events where id = $1)
           or payload ->> 'capture_event_id' = $1::text
           or payload ->> 'source_event_id' = $1::text
        order by occurred_at, version
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    let (echo, echo_pending) = stored_echo(&state, id).await;

    let origin = head.origin.unwrap_or_else(|| "text".to_string());
    let audio = (origin == "audio" && !redacted).then(|| AudioDetail {
        mime: head
            .audio_mime
            .clone()
            .unwrap_or_else(|| "audio/wav".to_string()),
        duration_ms: head.duration_ms,
    });

    Ok(Json(CaptureDetail {
        event_id: id,
        occurred_at: head.occurred_at,
        origin,
        device: head
            .payload
            .get("device")
            .and_then(|d| d.as_str())
            .unwrap_or("unknown")
            .to_string(),
        text,
        redacted,
        audio,
        transcripts: versions,
        entities: entities
            .into_iter()
            .map(|e| EntityMention {
                id: e.id,
                entity_type: e.entity_type,
                name: e.name,
                observation: e.observation,
                model: e.model,
                confidence: e.confidence,
                happened_on: e.happened_on,
                happened_at: e.happened_at,
                happened_precision: e.happened_precision,
            })
            .collect(),
        relations: relations
            .into_iter()
            .map(|r| RelationMention {
                id: r.id,
                from_entity_id: r.from_entity_id,
                from_name: r.from_name,
                to_entity_id: r.to_entity_id,
                to_name: r.to_name,
                relation_type: r.relation_type,
                model: r.model,
            })
            .collect(),
        echo,
        echo_pending,
        events: events
            .into_iter()
            .map(|e| EventRecord {
                id: e.id,
                event_type: e.event_type,
                source: e.source,
                occurred_at: e.occurred_at,
                payload: e.payload,
            })
            .collect(),
    }))
}

/// Corrects a capture's text.
///
/// Nothing is overwritten. The correction is appended as one more
/// transcript, superseding the previous one, and the original stays
/// readable in the detail view forever (ADR 0005). What *does* change is
/// everything derived: the search index is re-embedded and the structuring
/// runs again, because a correction that leaves the typo in the search
/// index has not corrected anything the user can feel.
pub async fn correct_transcript(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<CorrectTranscriptRequest>,
) -> Result<Json<TranscriptVersion>, AppError> {
    let text = req.text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::bad_request(
            "a correction cannot be empty — redaction is a separate thing",
        ));
    }

    let capture = sqlx::query!(
        r#"
        select e.occurred_at, e.payload, cc.redacted_at as "redacted_at?"
        from events e
        left join capture_content cc on cc.event_id = e.id
        where e.id = $1 and e.event_type = 'capture.recorded'
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(capture) = capture else {
        return Err(AppError::not_found("no such capture"));
    };
    if capture.redacted_at.is_some() {
        return Err(AppError::bad_request(
            "this capture's content was removed; there is nothing left to correct",
        ));
    }

    let previous = sqlx::query_scalar!(
        r#"
        select event_id from transcript_content
        where capture_event_id = $1
        order by created_at desc
        limit 1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let stored = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "transcript.corrected",
        &json!({
            "capture_event_id": id,
            "model": CORRECTION_AUTHOR,
            "supersedes": previous,
        }),
        CORRECTION_AUTHOR,
    )
    .await?;

    sqlx::query!(
        r#"
        insert into transcript_content (event_id, capture_event_id, text, model, supersedes)
        values ($1, $2, $3, $4, $5)
        "#,
        stored.id,
        id,
        text,
        CORRECTION_AUTHOR,
        previous,
    )
    .execute(&state.pool)
    .await?;

    // Everything derived from the old wording is now wrong, so it goes
    // before the new derivation runs. These are projection rows — derived
    // read-models, not history — which is exactly why they may be dropped
    // while the event log stays untouched (ADR 0003).
    //
    // Only when something can actually re-derive them, though: without a
    // structuring model configured this would silently strip a capture of
    // its entities and never put them back.
    if state.openrouter.is_some() {
        invalidate_derived(&state, id, stored.id).await?;
    } else {
        tracing::warn!(
            %id,
            "corrected without a structuring model configured — the entities still \
             describe the old wording"
        );
    }

    // The correction is read in the timezone the capture was *recorded*
    // in, not the one the correction is typed in: "tomorrow" meant a day
    // relative to when it was said.
    let timezone = timezone_for(
        &state,
        capture.payload.get("timezone").and_then(|t| t.as_str()),
    );

    // The stored echo judged the sentence that was just replaced, so it
    // is no longer an answer about this capture. Dropping the marker is
    // what makes `index_capture` below judge it again.
    if let Err(err) = echo::forget(&state.pool, id).await {
        tracing::warn!(?err, %id, "could not clear the echo of a corrected capture");
    }

    index_capture(
        &state,
        id,
        capture.occurred_at,
        &text,
        SpokenAt {
            utc: capture.occurred_at,
            timezone,
        },
    )
    .await;

    Ok(Json(TranscriptVersion {
        event_id: stored.id,
        text,
        model: CORRECTION_AUTHOR.to_string(),
        language: None,
        created_at: stored.occurred_at,
        supersedes: previous,
    }))
}

/// Recorded as the `model` of a human correction, so a transcript written
/// by a person is never mistaken for one an ASR model produced.
const CORRECTION_AUTHOR: &str = "user";

/// Stands in as the `model` of the original text of a typed capture, which
/// has no transcript row of its own to carry one.
const TYPED_AUTHOR: &str = "typed";

/// Drops the entities, observations and relations this capture produced,
/// so the next structuring run starts from a clean slate instead of
/// stacking a second reading on top of the first.
///
/// An entity is only removed when this capture was the last thing holding
/// it up: other captures' observations keep it alive, and so does any
/// relation that does not come from here.
async fn invalidate_derived(
    state: &AppState,
    capture_event_id: Uuid,
    reason_event_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = state.pool.begin().await?;

    sqlx::query!(
        r#"delete from relations where source_event_id = $1"#,
        capture_event_id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"delete from observations where source_event_id = $1"#,
        capture_event_id
    )
    .execute(&mut *tx)
    .await?;

    let orphaned = sqlx::query_scalar!(
        r#"
        delete from entities
        where not exists (select 1 from observations o where o.entity_id = entities.id)
          and not exists (
              select 1 from relations r
              where r.from_entity_id = entities.id or r.to_entity_id = entities.id
          )
        returning id
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    // The events that produced those rows are still in the log and always
    // will be. A future replay has to honour this marker, or it would
    // resurrect the reading of a sentence that no longer exists.
    events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "structuring.invalidated",
        &json!({
            "capture_event_id": capture_event_id,
            "because_of_event_id": reason_event_id,
            "orphaned_entities": orphaned,
        }),
        CORRECTION_AUTHOR,
    )
    .await?;

    Ok(())
}

/// Redacts a capture: the words go, the fact that something was said
/// stays.
///
/// This is the one deletion this system has, and it is deliberately not a
/// deletion of the event log. ADR 0003 and ADR 0005: the events are the
/// record of what happened and are never rewritten, so what is removed
/// here is everything *derived* from the capture plus the content itself.
/// A `capture.redacted` event is appended saying so, which is what a
/// future replay has to honour in order not to resurrect words that were
/// deliberately taken back.
///
/// The `capture_search` row goes too. Leaving it would keep the full
/// transcript in the search index — redaction that leaves the text
/// findable is not redaction. The consequence is that a redacted capture
/// disappears from the timeline, from search, from entity pages and from
/// everyone else's echoes; opening it by id still shows the tombstone.
///
/// Older backups are untouched, by definition. The caller is expected to
/// say so out loud before asking for this.
pub async fn redact(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<Redacted>, AppError> {
    let content = sqlx::query!(
        r#"select audio_path, redacted_at from capture_content where event_id = $1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(content) = content else {
        return Err(AppError::not_found("no such capture"));
    };
    if let Some(redacted_at) = content.redacted_at {
        // Already gone. Reporting success rather than an error keeps a
        // double click from looking like a failure — there is nothing
        // left to do and nothing left to remove.
        return Ok(Json(Redacted {
            event_id: id,
            redacted_at,
            observations_removed: 0,
            relations_removed: 0,
            entities_removed: 0,
            audio_removed: false,
        }));
    }

    let mut tx = state.pool.begin().await?;

    let redacted_at = sqlx::query_scalar!(
        r#"
        update capture_content
        set text = null, audio_path = null, audio_mime = null, redacted_at = now()
        where event_id = $1
        returning redacted_at as "redacted_at!"
        "#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    // `text` is NOT NULL here, so the words are replaced rather than
    // nulled; every reader already filters on `redacted_at is null`, so
    // the empty string is never shown to anyone.
    sqlx::query!(
        r#"update transcript_content set text = '', redacted_at = now()
           where capture_event_id = $1 and redacted_at is null"#,
        id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(r#"delete from capture_search where event_id = $1"#, id)
        .execute(&mut *tx)
        .await?;

    // Both directions: this capture's own judged echo, and its place in
    // anybody else's.
    sqlx::query!(
        r#"delete from capture_echo where capture_event_id = $1 or echo_event_id = $1"#,
        id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"delete from capture_echo_judged where capture_event_id = $1"#,
        id,
    )
    .execute(&mut *tx)
    .await?;

    let relations_removed = sqlx::query!(r#"delete from relations where source_event_id = $1"#, id)
        .execute(&mut *tx)
        .await?
        .rows_affected() as i64;

    let observations_removed =
        sqlx::query!(r#"delete from observations where source_event_id = $1"#, id)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;

    // Only entities this capture was the last reason for. One mentioned
    // anywhere else stays exactly as it was.
    let orphaned = sqlx::query_scalar!(
        r#"
        delete from entities
        where not exists (select 1 from observations o where o.entity_id = entities.id)
          and not exists (
              select 1 from relations r
              where r.from_entity_id = entities.id or r.to_entity_id = entities.id
          )
        returning id
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    // After the commit, not inside it: a failed unlink must not roll back
    // a redaction the database has already recorded. The row no longer
    // points at the file either way, so the worst case is an orphaned
    // blob on disk, and the log says which one.
    let mut audio_removed = false;
    if let Some(path) = content.audio_path.as_deref() {
        match audio::resolve(&state.audio_dir, path) {
            Ok(resolved) => match tokio::fs::remove_file(&resolved).await {
                Ok(()) => audio_removed = true,
                Err(err) => tracing::warn!(
                    ?err, %id, file = %resolved.display(),
                    "capture redacted but its recording could not be deleted"
                ),
            },
            Err(err) => tracing::warn!(?err, %id, "redacted capture had an unusable audio path"),
        }
    }

    events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "capture.redacted",
        &json!({
            "capture_event_id": id,
            "observations_removed": observations_removed,
            "relations_removed": relations_removed,
            "entities_removed": orphaned.len(),
            "audio_removed": audio_removed,
        }),
        REDACTION_AUTHOR,
    )
    .await?;

    tracing::info!(
        %id,
        observations_removed,
        relations_removed,
        entities_removed = orphaned.len(),
        audio_removed,
        "capture redacted"
    );

    Ok(Json(Redacted {
        event_id: id,
        redacted_at,
        observations_removed,
        relations_removed,
        entities_removed: orphaned.len() as i64,
        audio_removed,
    }))
}

/// Recorded as the source of a redaction event, for the same reason
/// `CORRECTION_AUTHOR` exists: a deletion a person asked for must be
/// distinguishable from anything a model did.
const REDACTION_AUTHOR: &str = "user";

/// What the backend has not finished yet.
///
/// Cheap on purpose — two counts over one indexed table — because the
/// client asks for it on a timer so that a stalled capture surfaces on
/// its own rather than waiting to be discovered.
pub async fn pipeline_status(
    State(state): State<AppState>,
) -> Result<Json<contracts::PipelineStatus>, AppError> {
    let (waiting, given_up) = pipeline::counts(&state.pool).await?;
    Ok(Json(contracts::PipelineStatus { waiting, given_up }))
}

/// Asks for one more try at a capture the retry loop gave up on.
///
/// Runs it right away rather than only clearing the flag: someone pressing
/// this wants to know whether it works now, and a button whose entire
/// effect is invisible for the next five minutes teaches people not to
/// trust it.
pub async fn retry_capture(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<contracts::PipelineStatus>, AppError> {
    if !pipeline::revive(&state.pool, id).await? {
        return Err(AppError::not_found(
            "no such capture, or there is nothing left to finish for it",
        ));
    }

    let items = pipeline::unfinished(
        &state.pool,
        state.timezone,
        // Zero: the wait is exactly what is being overridden here.
        std::time::Duration::ZERO,
        // One row is expected — `revive` just made this capture the
        // longest-waiting one — but the query is by age, not by id, so
        // the guard below is what makes sure this acts on the right one.
        16,
    )
    .await?;

    if let Some(item) = items.iter().find(|item| item.capture_event_id == id) {
        pipeline::finish(&state, item, state.retry.max_attempts).await;
    }

    let (waiting, given_up) = pipeline::counts(&state.pool).await?;
    Ok(Json(contracts::PipelineStatus { waiting, given_up }))
}

/// Asks for one more try at everything that was given up on.
///
/// The sidebar's one action. Per-capture retry exists too, but the
/// failure this is for is almost always one shared cause — a key that
/// expired, a provider that was down — so the useful gesture is "try
/// again", not "try again, twelve times, one capture at a time".
pub async fn retry_all(
    State(state): State<AppState>,
) -> Result<Json<contracts::PipelineStatus>, AppError> {
    let revived = sqlx::query!(
        r#"
        update capture_pipeline
        set abandoned_at = null, attempts = 0, last_attempt_at = null, last_error = null
        where abandoned_at is not null
        "#,
    )
    .execute(&state.pool)
    .await?
    .rows_affected();

    tracing::info!(revived, "asked to try the given-up captures again");

    let items = pipeline::unfinished(
        &state.pool,
        state.timezone,
        std::time::Duration::ZERO,
        // Bounded even here. Someone pressing a button should not be able
        // to start an unbounded run of paid calls; the loop picks up
        // whatever this pass does not reach.
        state.retry.batch,
    )
    .await?;

    for item in &items {
        pipeline::finish(&state, item, state.retry.max_attempts).await;
    }

    let (waiting, given_up) = pipeline::counts(&state.pool).await?;
    Ok(Json(contracts::PipelineStatus { waiting, given_up }))
}
