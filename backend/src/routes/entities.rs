//! The entity index and an entity's own page.
//!
//! This is the half of retrieval that is not search: you get here by
//! clicking a name you already saw, not by typing one you hope exists.
//! Everything here is a projection of `events` and can be rebuilt.

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use contracts::{EntityCapture, EntityDetail, EntityEdge, EntityListItem};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;

#[derive(serde::Deserialize)]
pub struct ListParams {
    /// Restrict to one entity type.
    pub entity_type: Option<String>,
    /// Substring of the name, case-insensitive. For finding a known name
    /// in a long list, not for retrieval — that is what search is.
    pub name: Option<String>,
    pub limit: Option<i64>,
}

/// How many entities to hand back by default. The list is ordered by how
/// often something has been spoken about, so the tail is one-off mentions
/// that are better reached by searching for the capture itself.
const DEFAULT_LIMIT: i64 = 200;

pub async fn list(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<EntityListItem>>, AppError> {
    let name = params.name.filter(|n| !n.trim().is_empty());

    let rows = sqlx::query!(
        r#"
        select
            e.id,
            e.entity_type,
            e.name,
            e.current_summary,
            count(o.id) as "mention_count!",
            max(o.created_at) as "last_seen?"
        from entities e
        left join observations o on o.entity_id = e.id
        where ($1::text is null or e.entity_type = $1)
          and ($2::text is null or e.name ilike '%' || $2 || '%')
        group by e.id, e.entity_type, e.name, e.current_summary
        order by count(o.id) desc, max(o.created_at) desc nulls last, e.name
        limit $3
        "#,
        params.entity_type,
        name,
        params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 1000),
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| EntityListItem {
                id: r.id,
                entity_type: r.entity_type,
                name: r.name,
                current_summary: r.current_summary,
                mention_count: r.mention_count,
                last_seen: r.last_seen,
            })
            .collect(),
    ))
}

pub async fn detail(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<EntityDetail>, AppError> {
    let entity = sqlx::query!(
        r#"select id, entity_type, name, current_summary, created_at from entities where id = $1"#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(entity) = entity else {
        return Err(AppError::not_found("no such entity"));
    };

    // Joined against `capture_search` rather than `capture_content`
    // because that is where the *current* reading of a capture lives: a
    // corrected capture must read as corrected here too.
    let mentions = sqlx::query!(
        r#"
        select
            o.source_event_id,
            o.text as observation,
            o.model,
            o.happened_on,
            o.happened_at,
            o.happened_precision,
            cs.transcript as "transcript?",
            cs.occurred_at as "occurred_at?",
            o.created_at
        from observations o
        left join capture_search cs on cs.event_id = o.source_event_id
        where o.entity_id = $1
        order by coalesce(cs.occurred_at, o.created_at) desc
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    let relations = sqlx::query!(
        r#"
        select
            r.relation_type,
            r.source_event_id,
            (r.from_entity_id = $1) as "outgoing!",
            other.id as other_id,
            other.name as other_name,
            other.entity_type as other_type
        from relations r
        join entities other
            on other.id = case when r.from_entity_id = $1 then r.to_entity_id else r.from_entity_id end
        where r.from_entity_id = $1 or r.to_entity_id = $1
        order by r.created_at
        "#,
        id,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(EntityDetail {
        id: entity.id,
        entity_type: entity.entity_type,
        name: entity.name,
        current_summary: entity.current_summary,
        created_at: entity.created_at,
        mentions: mentions
            .into_iter()
            .map(|m| EntityCapture {
                capture_event_id: m.source_event_id,
                // A capture whose content was redacted keeps its place in
                // the entity's history; only its words are gone.
                transcript_text: m.transcript.unwrap_or_default(),
                occurred_at: m.occurred_at.unwrap_or(m.created_at),
                observation: m.observation,
                model: m.model,
                happened_on: m.happened_on,
                happened_at: m.happened_at,
                happened_precision: m.happened_precision,
            })
            .collect(),
        relations: relations
            .into_iter()
            .map(|r| EntityEdge {
                relation_type: r.relation_type,
                outgoing: r.outgoing,
                other_id: r.other_id,
                other_name: r.other_name,
                other_type: r.other_type,
                source_event_id: r.source_event_id,
            })
            .collect(),
    }))
}
