use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
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
    let query_embedding: pgvector::Vector = state.embedder.embed_query(&params.query).await?.into();
    let limit = i64::from(params.limit.clamp(1, 200));

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
        params.query,
        params.from,
        params.to,
        params.entity_type,
        CANDIDATE_DEPTH,
        RRF_K,
        limit,
    )
    .fetch_all(&state.pool)
    .await?;

    let event_ids: Vec<Uuid> = rows.iter().map(|r| r.event_id).collect();
    let mut entities = related_entities(&state.pool, &event_ids).await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| SearchResult {
                related_entities: entities.remove(&r.event_id).unwrap_or_default(),
                capture_event_id: r.event_id,
                transcript_text: r.transcript,
                occurred_at: r.occurred_at,
                score: r.score,
            })
            .collect(),
    ))
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
