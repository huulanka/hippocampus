use axum::Json;
use axum::extract::{Query, State};
use contracts::{SearchQuery, SearchResult};

use crate::AppState;
use crate::error::AppError;

/// Hybrid search over raw capture transcripts: combines full-text rank
/// (`tsvector`/`plainto_tsquery`) with semantic similarity (`pgvector`
/// cosine distance) into a single score. Entity-based filtering/joins land
/// once the structuring worker (OpenRouter) populates `entities`.
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let query_embedding: pgvector::Vector = state.embedder.embed_query(&params.query).await?.into();
    let limit = i64::from(params.limit.min(200));

    let rows = sqlx::query!(
        r#"
        with scored as (
            select
                event_id,
                transcript,
                occurred_at,
                (
                    (1 - (embedding <=> $1)) * 0.7
                    + coalesce(ts_rank(tsv, plainto_tsquery('german', $2)), 0.0) * 0.3
                ) as score
            from capture_search
            where embedding is not null
        )
        select event_id, transcript, occurred_at, score as "score!"
        from scored
        order by score desc
        limit $3
        "#,
        query_embedding as _,
        params.query,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| SearchResult {
                capture_event_id: r.event_id,
                transcript_text: r.transcript,
                occurred_at: r.occurred_at,
                score: r.score as f32,
                related_entities: Vec::new(),
            })
            .collect(),
    ))
}
