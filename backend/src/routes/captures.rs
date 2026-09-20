use axum::Json;
use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use contracts::{CaptureAccepted, CreateCaptureRequest, EchoItem};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::audio;
use crate::echo;
use crate::error::AppError;
use crate::events;
use crate::structuring;

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

    let stored = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "capture.recorded",
        // The text itself deliberately does not go into the payload: the
        // event log is never modified, so anything written here could never
        // be redacted later (ADR 0005).
        &json!({ "origin": "text", "device": req.device }),
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

    let echo = index_capture(&state, stored.id, stored.occurred_at, transcript).await?;

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
        echo,
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

    let echo = index_capture(&state, stored.id, stored.occurred_at, &transcript).await?;

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
        echo,
    }))
}

/// Everything that happens to a capture's text once it exists, whatever
/// produced it: make it findable, structure it, and answer with its echoes.
async fn index_capture(
    state: &AppState,
    capture_event_id: Uuid,
    occurred_at: chrono::DateTime<chrono::Utc>,
    transcript: &str,
) -> Result<Vec<EchoItem>, AppError> {
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

    // Structuring is best-effort and never blocks or fails the capture —
    // the original is already safely stored and stays re-derivable.
    tokio::spawn(structuring::structure_capture_in_background(
        state.pool.clone(),
        state.openrouter.clone(),
        capture_event_id,
        transcript.to_string(),
    ));

    // Echo, by contrast, is computed inline: it is the one thing the user
    // is waiting to see and needs no network call. A failure still must not
    // cost the capture, so it degrades to an empty list.
    Ok(echo::for_embedding(
        &state.pool,
        &embedding,
        occurred_at,
        Some(capture_event_id),
        state.echo_min_similarity,
        echo::DEFAULT_LIMIT,
    )
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(?err, %capture_event_id, "echo lookup failed, returning capture without it");
        Vec::new()
    }))
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
    /// Override the configured similarity threshold, for tuning against
    /// real captures without restarting the backend.
    pub min_similarity: Option<f32>,
    pub limit: Option<i64>,
}

/// Echoes for an existing capture. `POST /captures` already returns these
/// inline; this endpoint exists so they can be re-viewed later from the
/// timeline, and so the threshold can be tuned interactively.
pub async fn echo_for(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Query(params): Query<EchoParams>,
) -> Result<Json<Vec<EchoItem>>, AppError> {
    let items = echo::for_capture(
        &state.pool,
        id,
        params.min_similarity.unwrap_or(state.echo_min_similarity),
        params.limit.unwrap_or(echo::DEFAULT_LIMIT).clamp(1, 20),
    )
    .await?;

    Ok(Json(items))
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
