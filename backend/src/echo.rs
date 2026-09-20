//! Echo: the earlier captures that are semantically closest to a given one.
//!
//! This is the product's core retrieval mechanism (ADR 0006). It runs on
//! embedding similarity alone — no LLM is involved, and nothing is ever
//! summarised or paraphrased. An echo can therefore be wrong about
//! *relevance*, but it can never put words into the user's mouth, which is
//! the one failure a memory system cannot afford.
//!
//! Only strictly earlier captures are returned: the experience being built
//! is "you have thought about this before", not "here is related material".

use chrono::{DateTime, Utc};
use contracts::EchoItem;
use sqlx::PgPool;
use uuid::Uuid;

/// How many echoes to show by default. Three is enough to recognise a
/// pattern and few enough to read without deciding to read them.
pub const DEFAULT_LIMIT: i64 = 3;

/// Minimum cosine similarity for an echo to be shown at all.
///
/// `multilingual-e5-small` compresses its similarity range badly. Measured
/// against the first real captures in this database:
///
/// - "Kardamom-Espresso probiert" vs. its two genuine earlier matches:
///   0.905 and 0.915; the best unrelated capture: 0.877.
/// - "Rasenmäher muss zum Service", related to nothing at all: best score
///   0.871, whole corpus spread 0.820-0.871.
///
/// So the noise floor reaches 0.877 while true matches start at 0.905 —
/// a gap of under 0.03. Anything below ~0.89 makes every capture echo
/// something, which trains the user to ignore echoes entirely; that is a
/// worse failure than showing nothing.
///
/// Purely relative criteria (median + k·MAD over the corpus) were tried
/// and rejected: they work for the coffee case but fire on all three top
/// results of the lawnmower case, because a homogeneous corpus has a tiny
/// MAD. Absolute threshold it is — tunable via `ECHO_MIN_SIMILARITY` and
/// per request, because the right value will drift as the corpus grows.
pub const DEFAULT_MIN_SIMILARITY: f32 = 0.89;

/// Echoes for a capture that is already in `capture_search`.
pub async fn for_capture(
    pool: &PgPool,
    capture_event_id: Uuid,
    min_similarity: f32,
    limit: i64,
) -> anyhow::Result<Vec<EchoItem>> {
    let target = sqlx::query!(
        r#"select embedding as "embedding: pgvector::Vector", occurred_at
           from capture_search where event_id = $1"#,
        capture_event_id,
    )
    .fetch_optional(pool)
    .await?;

    let Some(target) = target else {
        return Ok(Vec::new());
    };
    let Some(embedding) = target.embedding else {
        return Ok(Vec::new());
    };

    for_embedding(
        pool,
        &embedding,
        target.occurred_at,
        Some(capture_event_id),
        min_similarity,
        limit,
    )
    .await
}

/// Echoes for an embedding that may not be stored yet. `before` bounds the
/// result to captures older than the one being echoed; `exclude` drops the
/// capture itself, which would otherwise always rank first with
/// similarity 1.0.
pub async fn for_embedding(
    pool: &PgPool,
    embedding: &pgvector::Vector,
    before: DateTime<Utc>,
    exclude: Option<Uuid>,
    min_similarity: f32,
    limit: i64,
) -> anyhow::Result<Vec<EchoItem>> {
    let rows = sqlx::query!(
        r#"
        select
            event_id,
            transcript,
            occurred_at,
            (1 - (embedding <=> $1))::real as "similarity!"
        from capture_search
        where embedding is not null
          and occurred_at < $2
          and ($3::uuid is null or event_id <> $3)
          and (1 - (embedding <=> $1)) >= $4
        order by embedding <=> $1
        limit $5
        "#,
        embedding as _,
        before,
        exclude,
        f64::from(min_similarity),
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| EchoItem {
            capture_event_id: r.event_id,
            transcript_text: r.transcript,
            occurred_at: r.occurred_at,
            similarity: r.similarity,
        })
        .collect())
}
