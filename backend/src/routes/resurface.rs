//! The one surface that answers a question nobody asked.
//!
//! Capture, timeline, search and the entity pages all wait to be used.
//! That is the gap that showed after the first real week: capturing is
//! effortless, but getting anything back still means going and searching.
//!
//! Two things are worth pushing, and only two, because a surface that
//! shows everything is a surface nobody reads:
//!
//! 1. **What you said was coming.** Only possible since the extraction
//!    resolves "morgen" into a date — before that, a future was just a
//!    word in a sentence.
//! 2. **What you keep coming back to.** A subject touched by two separate
//!    captures is a thread; one capture is a note.

use axum::Json;
use axum::extract::State;
use contracts::{Resurfaced, ThreadItem, UpcomingItem};

use crate::AppState;
use crate::error::AppError;

/// How far ahead to look. Beyond a few weeks it stops being a reminder
/// and starts being a calendar, which this is not.
const HORIZON_DAYS: i32 = 30;

/// Threads are for recognising a pattern, not for browsing — that is what
/// the entity index is for.
const MAX_THREADS: i64 = 8;

pub async fn resurface(State(state): State<AppState>) -> Result<Json<Resurfaced>, AppError> {
    let upcoming = sqlx::query!(
        r#"
        select
            en.id as entity_id,
            en.name as entity_name,
            en.entity_type,
            o.text as observation,
            o.happened_on as "happened_on!",
            o.happened_at,
            o.happened_precision,
            o.source_event_id,
            o.created_at,
            cs.occurred_at as "said_at?"
        from observations o
        join entities en on en.id = o.entity_id
        left join capture_search cs on cs.event_id = o.source_event_id
        where o.happened_on is not null
          and o.happened_on >= current_date
          and o.happened_on <= current_date + ($1 || ' days')::interval
        order by o.happened_on, o.happened_at nulls last
        "#,
        HORIZON_DAYS.to_string(),
    )
    .fetch_all(&state.pool)
    .await?;

    // Two *captures*, not two observations: a single sentence often
    // yields several observations about the same entity, and counting
    // those would make every capture look like a recurring theme.
    let threads = sqlx::query!(
        r#"
        select
            en.id,
            en.entity_type,
            en.name,
            en.current_summary,
            count(distinct o.source_event_id) as "capture_count!",
            min(o.created_at) as "first_seen!",
            max(o.created_at) as "last_seen!"
        from entities en
        join observations o on o.entity_id = en.id
        where en.merged_into is null
        group by en.id, en.entity_type, en.name, en.current_summary
        having count(distinct o.source_event_id) >= 2
        order by max(o.created_at) desc
        limit $1
        "#,
        MAX_THREADS,
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(Resurfaced {
        upcoming: upcoming
            .into_iter()
            .map(|u| UpcomingItem {
                entity_id: u.entity_id,
                entity_name: u.entity_name,
                entity_type: u.entity_type,
                observation: u.observation,
                happened_on: u.happened_on,
                happened_at: u.happened_at,
                happened_precision: u.happened_precision,
                capture_event_id: u.source_event_id,
                // Falls back to when the observation was written, which is
                // only different for a capture whose content was redacted.
                said_at: u.said_at.unwrap_or(u.created_at),
            })
            .collect(),
        threads: threads
            .into_iter()
            .map(|t| ThreadItem {
                entity_id: t.id,
                entity_type: t.entity_type,
                name: t.name,
                current_summary: t.current_summary,
                capture_count: t.capture_count,
                first_seen: t.first_seen,
                last_seen: t.last_seen,
            })
            .collect(),
    }))
}
