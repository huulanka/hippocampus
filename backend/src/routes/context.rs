//! Asking a question, and the calendar's side of a note's context.

use axum::Json;
use axum::extract::{Query, State};
use contracts::{Answer, AskRequest, OccasionsOffered, OfferOccasionsRequest, PendingOccasion};
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;

/// Enough for a Mac catching up on months of notes to get through them
/// in a handful of rounds, few enough that one round's calendar reads
/// stay quick.
const MAX_PENDING: i64 = 200;

/// `POST /ask`: a question, answered from the notes and nothing else.
pub async fn ask(
    State(state): State<AppState>,
    Json(request): Json<AskRequest>,
) -> Result<Json<Answer>, AppError> {
    if request.question.trim().is_empty() {
        return Err(AppError::bad_request("a question needs words"));
    }
    Ok(Json(crate::ask::ask(&state, &request).await?))
}

#[derive(Debug, Deserialize)]
pub struct PendingParams {
    pub checker: Uuid,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /occasions/pending`: the notes this Mac has not looked up in its
/// calendar yet.
pub async fn pending(
    State(state): State<AppState>,
    Query(params): Query<PendingParams>,
) -> Result<Json<Vec<PendingOccasion>>, AppError> {
    let limit = params.limit.unwrap_or(MAX_PENDING).clamp(1, MAX_PENDING);
    Ok(Json(
        crate::context::pending_for(&state.pool, params.checker, limit).await?,
    ))
}

/// `POST /occasions`: what this Mac found around those notes. Matched and
/// dropped; only the matched entities are kept (ADR 0016).
pub async fn offer(
    State(state): State<AppState>,
    Json(request): Json<OfferOccasionsRequest>,
) -> Result<Json<OccasionsOffered>, AppError> {
    Ok(Json(crate::context::offer(&state.pool, &request).await?))
}
