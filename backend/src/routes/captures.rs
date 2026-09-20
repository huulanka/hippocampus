use axum::Json;
use axum::extract::{Path, Query, State};
use contracts::{CaptureAccepted, CreateCaptureRequest, EchoItem};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::echo;
use crate::error::AppError;
use crate::events;
use crate::structuring;

/// Accepts a raw capture and appends it to the event log, unmodified and
/// forever. This is the only place original knowledge enters the system.
pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateCaptureRequest>,
) -> Result<Json<CaptureAccepted>, AppError> {
    let stream_id = Uuid::new_v4();
    let payload = json!({
        "transcript_text": req.transcript_text,
        "device": req.device,
        "audio_ref": req.audio_ref,
    });

    let stored = events::append(
        &state.pool,
        stream_id,
        1,
        "capture.recorded",
        &payload,
        &req.device,
    )
    .await?;

    let embedding: pgvector::Vector = state
        .embedder
        .embed_passage(&req.transcript_text)
        .await?
        .into();

    sqlx::query!(
        r#"
        insert into capture_search (event_id, transcript, occurred_at, embedding)
        values ($1, $2, $3, $4)
        "#,
        stored.id,
        req.transcript_text,
        stored.occurred_at,
        embedding as _,
    )
    .execute(&state.pool)
    .await?;

    // Structuring is best-effort and never blocks or fails the capture
    // itself — the raw event above is already safely stored.
    tokio::spawn(structuring::structure_capture_in_background(
        state.pool.clone(),
        state.openrouter.clone(),
        stored.id,
        req.transcript_text.clone(),
    ));

    // Echo is computed inline, unlike structuring: it is the one thing the
    // user is waiting to see, it needs no network call, and a failure here
    // must still not cost the capture — so it degrades to an empty list.
    let echo = echo::for_embedding(
        &state.pool,
        &embedding,
        stored.occurred_at,
        Some(stored.id),
        state.echo_min_similarity,
        echo::DEFAULT_LIMIT,
    )
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(?err, %stored.id, "echo lookup failed, returning capture without it");
        Vec::new()
    });

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
        echo,
    }))
}

#[derive(serde::Serialize)]
pub struct CaptureListItem {
    pub event_id: Uuid,
    pub transcript_text: String,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

/// Lists recent raw captures. Exists mainly to verify end-to-end ingestion
/// during development; the real read path is the search/browse API.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<CaptureListItem>>, AppError> {
    let rows = sqlx::query!(
        r#"
        select event_id, transcript, occurred_at
        from capture_search
        order by occurred_at desc
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
            })
            .collect(),
    ))
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
    Path(id): Path<Uuid>,
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
