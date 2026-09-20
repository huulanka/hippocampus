use axum::Json;
use axum::extract::State;
use contracts::{CaptureAccepted, CreateCaptureRequest};
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;
use crate::events;

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

    sqlx::query!(
        r#"
        insert into capture_search (event_id, transcript, occurred_at)
        values ($1, $2, $3)
        "#,
        stored.id,
        req.transcript_text,
        stored.occurred_at,
    )
    .execute(&state.pool)
    .await?;

    Ok(Json(CaptureAccepted {
        event_id: stored.id,
        occurred_at: stored.occurred_at,
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
