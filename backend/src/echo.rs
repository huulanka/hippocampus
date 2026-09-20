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

use crate::reranker::Reranker;

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

/// The same floor, used when a cross-encoder decides what is shown.
///
/// Much lower on purpose. 0.89 was tuned to be the *last* filter; here it
/// only has to keep the candidate set small enough to rerank in under
/// 200 ms, and anything it excludes the cross-encoder never gets to see.
/// Measured on the real corpus, similarity between unrelated German
/// sentences bottoms out around 0.82, so 0.80 is close to "everything".
pub const CANDIDATE_MIN_SIMILARITY: f32 = 0.80;

/// How many candidates the bi-encoder hands to the cross-encoder.
///
/// Ten costs 178 ms with the default reranker and is far more than three
/// good echoes ever need — the point of the surplus is that the
/// bi-encoder's *ranking* is untrustworthy (that is the whole reason a
/// reranker exists), so the right answer is often not in its top three.
pub const CANDIDATE_LIMIT: i64 = 10;

/// Echoes for a capture that is already in `capture_search`.
pub async fn for_capture(
    pool: &PgPool,
    capture_event_id: Uuid,
    reranker: Option<&Reranker>,
    thresholds: Thresholds,
    limit: i64,
) -> anyhow::Result<Vec<EchoItem>> {
    let target = sqlx::query!(
        r#"select transcript, embedding as "embedding: pgvector::Vector", occurred_at
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

    for_capture_text(
        pool,
        &embedding,
        &target.transcript,
        target.occurred_at,
        Some(capture_event_id),
        reranker,
        thresholds,
        limit,
    )
    .await
}

/// The two thresholds echo is filtered by, carried together so a caller
/// cannot accidentally pass one and forget the other.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Minimum cosine similarity for a capture to become a *candidate*.
    /// With a reranker in play this is a recall floor, not a quality bar,
    /// and should sit well below `DEFAULT_MIN_SIMILARITY`.
    pub min_similarity: f32,
    /// Minimum cross-encoder score for a candidate to actually be shown.
    /// Ignored when no reranker is loaded.
    pub min_rerank: f32,
}

/// Retrieval with the bi-encoder, judgement with the cross-encoder.
///
/// Splitting the two is the whole design: the embeddings already in the
/// database are good at recall and bad at ordering, so they choose the
/// candidates and something that reads both texts together decides which
/// of them the user actually sees.
#[allow(clippy::too_many_arguments)]
pub async fn for_capture_text(
    pool: &PgPool,
    embedding: &pgvector::Vector,
    text: &str,
    before: DateTime<Utc>,
    exclude: Option<Uuid>,
    reranker: Option<&Reranker>,
    thresholds: Thresholds,
    limit: i64,
) -> anyhow::Result<Vec<EchoItem>> {
    let Some(reranker) = reranker else {
        return for_embedding(
            pool,
            embedding,
            before,
            exclude,
            thresholds.min_similarity,
            limit,
        )
        .await;
    };

    let candidates = for_embedding(
        pool,
        embedding,
        before,
        exclude,
        thresholds.min_similarity,
        CANDIDATE_LIMIT.max(limit),
    )
    .await?;

    if candidates.is_empty() {
        return Ok(candidates);
    }

    let scores = reranker
        .score(
            text,
            candidates
                .iter()
                .map(|c| c.transcript_text.clone())
                .collect(),
        )
        .await?;

    let mut scored: Vec<(f32, EchoItem)> = scores
        .into_iter()
        .zip(candidates)
        .filter(|(score, _)| *score >= thresholds.min_rerank)
        .map(|(score, mut item)| {
            item.rerank_score = Some(score);
            (score, item)
        })
        .collect();

    // Descending by cross-encoder score. `total_cmp` rather than
    // `partial_cmp().unwrap()`: a NaN from the model would otherwise
    // panic inside a sort, taking the whole capture with it.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.truncate(limit.max(0) as usize);

    Ok(scored.into_iter().map(|(_, item)| item).collect())
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
            rerank_score: None,
        })
        .collect())
}
