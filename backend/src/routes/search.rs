use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Utc};
use contracts::{EntitySummary, SearchQuery, SearchResult};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;

/// Reciprocal Rank Fusion constant. 60 is the value from the original RRF
/// paper and the de-facto default; it damps the influence of the very top
/// ranks just enough that one strong hit in a single retriever cannot
/// dominate the other entirely.
const RRF_K: f64 = 60.0;

/// How deep each retriever goes before fusion. Beyond this, contributions
/// are smaller than the difference between adjacent fused scores anyway.
const CANDIDATE_DEPTH: i64 = 100;

/// Hybrid search over capture transcripts, fusing full-text and semantic
/// retrieval with Reciprocal Rank Fusion.
///
/// The previous implementation added a weighted `ts_rank` to a weighted
/// cosine similarity. That silently degraded to pure vector search:
/// `ts_rank` returns roughly 0.01-0.1 while `1 - cosine` for
/// `multilingual-e5-small` sits at 0.7-0.9, so the lexical term was
/// numerically negligible. RRF compares *ranks* instead of scores, so the
/// two retrievers contribute on equal terms without any scale calibration
/// — which matters most for the case vector search is worst at: exact
/// proper nouns.
pub async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let hits = hybrid(
        &state,
        &params.query,
        params.from,
        params.to,
        params.entity_type.as_deref(),
        i64::from(params.limit.clamp(1, 200)),
    )
    .await?;

    let event_ids: Vec<Uuid> = hits.iter().map(|h| h.event_id).collect();
    let mut entities = related_entities(&state.pool, &event_ids).await?;

    Ok(Json(
        hits.into_iter()
            .map(|h| SearchResult {
                related_entities: entities.remove(&h.event_id).unwrap_or_default(),
                capture_event_id: h.event_id,
                transcript_text: h.transcript,
                occurred_at: h.occurred_at,
                score: h.score,
            })
            .collect(),
    ))
}

/// One capture found by [`hybrid`].
pub(crate) struct Hit {
    pub event_id: Uuid,
    pub transcript: String,
    pub occurred_at: DateTime<Utc>,
    pub score: f32,
}

/// The fused full-text and semantic ranking behind both the search field
/// and the answer to a question.
pub(crate) async fn hybrid(
    state: &AppState,
    query: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    entity_type: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<Hit>> {
    let query_embedding: pgvector::Vector = state.embedder.embed_query(query).await?.into();

    let rows = sqlx::query!(
        r#"
        with filtered as (
            select cs.event_id, cs.transcript, cs.occurred_at, cs.tsv, cs.embedding
            from capture_search cs
            where ($3::timestamptz is null or cs.occurred_at >= $3)
              and ($4::timestamptz is null or cs.occurred_at <= $4)
              and (
                $5::text is null
                or exists (
                    select 1
                    from observations o
                    join entities e on e.id = o.entity_id
                    where o.source_event_id = cs.event_id and e.entity_type = $5
                )
              )
        ),
        lexical as (
            select event_id, row_number() over (
                order by ts_rank(tsv, plainto_tsquery('german', $2)) desc, occurred_at desc
            ) as rank
            from filtered
            where tsv @@ plainto_tsquery('german', $2)
            limit $6
        ),
        semantic as (
            select event_id, row_number() over (order by embedding <=> $1) as rank
            from filtered
            where embedding is not null
            limit $6
        ),
        fused as (
            select
                coalesce(l.event_id, s.event_id) as event_id,
                coalesce(1.0 / ($7::float8 + l.rank), 0.0)
                + coalesce(1.0 / ($7::float8 + s.rank), 0.0) as score
            from lexical l
            full outer join semantic s on s.event_id = l.event_id
        )
        select
            f.event_id as "event_id!",
            fl.transcript as "transcript!",
            fl.occurred_at as "occurred_at!",
            f.score::real as "score!"
        from fused f
        join filtered fl on fl.event_id = f.event_id
        order by f.score desc, fl.occurred_at desc
        limit $8
        "#,
        query_embedding as _,
        query,
        from,
        to,
        entity_type,
        CANDIDATE_DEPTH,
        RRF_K,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| Hit {
            event_id: r.event_id,
            transcript: r.transcript,
            occurred_at: r.occurred_at,
            score: r.score,
        })
        .collect())
}

/// Entities that were extracted from each of the given captures. Fetched in
/// one query rather than per result, and grouped in Rust — the alternative
/// (a lateral join with `json_agg`) would trade a trivial amount of
/// application code for SQL that is markedly harder to read.
async fn related_entities(
    pool: &sqlx::PgPool,
    event_ids: &[Uuid],
) -> anyhow::Result<HashMap<Uuid, Vec<EntitySummary>>> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = sqlx::query!(
        r#"
        select distinct
            o.source_event_id,
            e.id,
            e.entity_type,
            e.name,
            e.current_summary
        from observations o
        join entities e on e.id = o.entity_id
        where o.source_event_id = any($1)
        order by e.name
        "#,
        event_ids,
    )
    .fetch_all(pool)
    .await?;

    let mut grouped: HashMap<Uuid, Vec<EntitySummary>> = HashMap::new();
    for row in rows {
        grouped
            .entry(row.source_event_id)
            .or_default()
            .push(EntitySummary {
                id: row.id,
                entity_type: row.entity_type,
                name: row.name,
                current_summary: row.current_summary,
            });
    }

    Ok(grouped)
}
