//! The entity index and an entity's own page.
//!
//! This is the half of retrieval that is not search: you get here by
//! clicking a name you already saw, not by typing one you hope exists.
//! Everything here is a projection of `events` and can be rebuilt.

use axum::Json;
use axum::extract::{Path as AxumPath, Query, State};
use contracts::{ChangeRecord, EntityCapture, EntityDetail, EntityEdge, EntityListItem};
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
        -- Merged entities are not rows anyone should see: their
        -- observations now hang off the surviving one, so listing them
        -- would show the same thing twice, once of it empty.
        where e.merged_into is null
          and ($1::text is null or e.entity_type = $1)
          -- Aliases as well as the current name, so a name that was
          -- merged away still finds what it became.
          and ($2::text is null or e.name ilike '%' || $2 || '%'
               or exists (select 1 from entity_alias a
                          where a.entity_id = e.id and a.name ilike '%' || $2 || '%'))
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
    // Follows the merge pointer rather than 404ing. A link to an entity
    // that has since been folded into another is not broken — it is a
    // link to the thing that entity turned out to be, and landing on it
    // is the right answer.
    let entity = sqlx::query!(
        r#"
        select e.id, e.entity_type, e.name, e.current_summary, e.created_at
        from entities start
        join entities e on e.id = coalesce(start.merged_into, start.id)
        where start.id = $1
        "#,
        id,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(entity) = entity else {
        return Err(AppError::not_found("no such entity"));
    };
    let id = entity.id;

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

/// How the view of this knowledge came to look the way it does.
///
/// The user's own framing, and the principle the system is built on:
/// *"Ich habe unveränderliche Startprimitiven, Changelog auf die
/// Repräsentation meines Wissens."* The captures never change. This is
/// everything that happened to their arrangement, newest first.
///
/// Read straight off the event log rather than from a projection: the
/// events *are* the record of what the run decided, and a second copy of
/// them could disagree with the first.
pub async fn changelog(State(state): State<AppState>) -> Result<Json<Vec<ChangeRecord>>, AppError> {
    let rows = sqlx::query!(
        r#"
        select
            ev.event_type,
            ev.stream_id,
            ev.payload,
            ev.occurred_at,
            subject.name as "subject_name!",
            -- For a merge: whether the absorbed entity still points here.
            absorbed.merged_into as "still_merged?",
            -- For a derived edge: what is at the other end.
            other.name as "other_name?"
        from events ev
        join entities subject on subject.id = ev.stream_id
        left join entities absorbed
            on ev.event_type = 'entity.merged'
           and absorbed.id = (ev.payload ->> 'source_entity_id')::uuid
        left join entities other
            on ev.event_type = 'relation.proposed'
           and other.id = (ev.payload ->> 'to_entity_id')::uuid
        where ev.event_type in
            ('entity.merged', 'entity.renamed', 'entity.retyped', 'relation.proposed')
          -- Edges that came out of a single capture are not a change to
          -- the arrangement; they are the arrangement being built in the
          -- first place, and they already show on the capture itself.
          and (ev.event_type <> 'relation.proposed'
               or ev.payload ->> 'across_captures' = 'true')
        order by ev.occurred_at desc
        limit 300
        "#,
    )
    .fetch_all(&state.pool)
    .await?;

    let text = |payload: &serde_json::Value, key: &str| {
        payload
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };

    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let p = &row.payload;
                let (kind, before, after, undo_id, undone) = match row.event_type.as_str() {
                    "entity.merged" => {
                        let source = p
                            .get("source_entity_id")
                            .and_then(|v| v.as_str())
                            .and_then(|v| v.parse::<Uuid>().ok());
                        (
                            "merged",
                            text(p, "source_name"),
                            String::new(),
                            source,
                            row.still_merged.is_none(),
                        )
                    }
                    "entity.renamed" => ("renamed", text(p, "from"), text(p, "to"), None, false),
                    "entity.retyped" => ("retyped", text(p, "from"), text(p, "to"), None, false),
                    _ => (
                        "related",
                        text(p, "relation_type"),
                        row.other_name.clone().unwrap_or_default(),
                        None,
                        false,
                    ),
                };

                ChangeRecord {
                    kind: kind.to_string(),
                    entity_id: row.stream_id,
                    entity_name: row.subject_name,
                    before,
                    after,
                    reason: text(p, "reason"),
                    changed_at: row.occurred_at,
                    undo_id,
                    undone,
                }
            })
            .collect(),
    ))
}

/// Body of `POST /entities/{id}/merge`.
#[derive(serde::Deserialize)]
pub struct MergeRequest {
    /// The entity that survives. `{id}` is folded into it.
    pub into: Uuid,
}

/// Folds one entity into another because a person said so.
///
/// The automatic run is deliberately timid — it refuses anything it is
/// not sure of, and the guard against merging differently-named things of
/// different types refuses some merges that are in fact correct. This is
/// the way through: none of those guards apply to a person who is looking
/// at both entities and has decided.
///
/// It also lifts an existing block on the pair. A block means "an
/// automatic run merged these and someone pulled them apart"; a person
/// now merging them on purpose is the later and better-informed decision.
pub async fn merge_entities(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    Json(req): Json<MergeRequest>,
) -> Result<Json<Vec<ChangeRecord>>, AppError> {
    if id == req.into {
        return Err(AppError::bad_request(
            "an entity cannot be merged into itself",
        ));
    }

    let pair = sqlx::query!(
        r#"
        select
            s.name as source_name, s.entity_type as source_type, s.merged_into as "source_merged?",
            t.name as target_name, t.merged_into as "target_merged?"
        from entities s, entities t
        where s.id = $1 and t.id = $2
        "#,
        id,
        req.into,
    )
    .fetch_optional(&state.pool)
    .await?;

    let Some(pair) = pair else {
        return Err(AppError::not_found("one of those entities does not exist"));
    };
    if pair.source_merged.is_some() || pair.target_merged.is_some() {
        return Err(AppError::bad_request(
            "one of those has already been merged into something else",
        ));
    }

    sqlx::query!(
        r#"delete from entity_merge_block
           where lower_id = least($1::uuid, $2::uuid) and higher_id = greatest($1::uuid, $2::uuid)"#,
        id,
        req.into,
    )
    .execute(&state.pool)
    .await?;

    crate::consolidation::merge(
        &state.pool,
        &crate::consolidation::Merge {
            target_id: req.into,
            target_name: pair.target_name,
            source_id: id,
            source_name: pair.source_name,
            source_type: pair.source_type,
            reason: "merged by hand".to_string(),
        },
        "user",
    )
    .await?;

    changelog(State(state)).await
}

/// Takes one merge back.
///
/// Addressed by the entity that was folded *in*, because that is the one
/// being given its own existence back.
pub async fn unmerge(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<Vec<ChangeRecord>>, AppError> {
    let undone = crate::consolidation::unmerge(&state.pool, id, "user")
        .await
        .map_err(AppError::from)?;

    if !undone {
        return Err(AppError::not_found(
            "no such entity, or it is not currently merged into another",
        ));
    }

    changelog(State(state)).await
}

/// What a consolidation pass would do, without doing any of it.
///
/// The endpoint to call before switching the automatic run on against a
/// database it has not seen before.
pub async fn consolidation_preview(
    State(state): State<AppState>,
) -> Result<Json<contracts::ConsolidationPreview>, AppError> {
    Ok(Json(crate::consolidation::preview(&state).await?))
}

/// Runs a consolidation pass now rather than waiting for the timer.
pub async fn consolidate_now(
    State(state): State<AppState>,
) -> Result<Json<crate::consolidation::RunReport>, AppError> {
    Ok(Json(crate::consolidation::run_once(&state, "user").await?))
}
