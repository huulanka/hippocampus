//! The append-only event log. This is the single source of truth for
//! everything the system has ever recorded or derived — nothing here is
//! ever mutated. Projections (entities, observations, relations, search
//! indexes) are read-models built from these events and can always be
//! rebuilt by replaying them.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

pub struct StoredEvent {
    pub id: Uuid,
    pub occurred_at: DateTime<Utc>,
}

/// Appends a new event to a stream. Most capture-style events are the sole
/// event in their own stream (`version = 1`); derived/aggregate streams
/// (e.g. an entity accumulating observations) pass an existing `stream_id`
/// and rely on the `unique (stream_id, version)` constraint for optimistic
/// concurrency.
pub async fn append(
    pool: &PgPool,
    stream_id: Uuid,
    version: i64,
    event_type: &str,
    payload: &Value,
    source: &str,
) -> Result<StoredEvent, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        insert into events (stream_id, version, event_type, payload, source)
        values ($1, $2, $3, $4, $5)
        returning id, occurred_at
        "#,
        stream_id,
        version,
        event_type,
        payload,
        source,
    )
    .fetch_one(pool)
    .await?;

    Ok(StoredEvent {
        id: row.id,
        occurred_at: row.occurred_at,
    })
}

/// Appends inside a caller's transaction.
///
/// Needed wherever an event and the projection change it describes have
/// to be one atomic step — a merge, above all. A half-applied merge whose
/// event never landed would be a graph change with no record of why, and
/// nothing to undo it with.
pub async fn append_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    stream_id: Uuid,
    version: i64,
    event_type: &str,
    payload: &Value,
    source: &str,
) -> Result<StoredEvent, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        insert into events (stream_id, version, event_type, payload, source)
        values ($1, $2, $3, $4, $5)
        returning id, occurred_at
        "#,
        stream_id,
        version,
        event_type,
        payload,
        source,
    )
    .fetch_one(&mut **tx)
    .await?;

    Ok(StoredEvent {
        id: row.id,
        occurred_at: row.occurred_at,
    })
}
