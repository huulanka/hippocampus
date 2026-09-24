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

use crate::judge::Judge;

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

/// Judges a capture's candidates once and writes the result down.
///
/// This is the only place a judge is ever asked anything. Everything that
/// *reads* an echo reads [`stored`] — on the NAS the difference is 27
/// seconds against a single indexed query, and the recomputation was pure
/// waste in any case: an echo looks only at captures earlier than its
/// own, and those never change.
///
/// Every candidate is stored, including ones the threshold would hide, so
/// that retuning the threshold — or answering a calibration request —
/// never costs a judgement again.
/// The two thresholds a judgement is bounded by, carried together so a
/// caller cannot pass one and forget the other.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Minimum cosine similarity for a capture to become a *candidate*.
    /// A recall floor, not a quality bar: the judge decides what is
    /// shown, so this should sit well below `DEFAULT_MIN_SIMILARITY`.
    pub min_similarity: f32,
    /// Minimum judge score for a candidate to be shown. Deliberately not
    /// applied when storing — every candidate is kept with its score —
    /// so this only decides what a read returns, and what the log says
    /// the user would have seen.
    pub min_score: f32,
}

pub async fn judge_and_store(
    pool: &PgPool,
    judge: Option<&Judge>,
    capture_event_id: Uuid,
    embedding: &pgvector::Vector,
    text: &str,
    before: DateTime<Utc>,
    thresholds: Thresholds,
) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let candidates = for_embedding(
        pool,
        embedding,
        before,
        Some(capture_event_id),
        thresholds.min_similarity,
        CANDIDATE_LIMIT,
    )
    .await?;

    // Without a judge the bi-encoder's own similarity is the score. It is
    // the wrong order — that is why a judge exists — but it is an honest
    // record of what produced it, which the marker then names.
    let (scores, judged_by) = match judge {
        Some(judge) if !candidates.is_empty() => {
            let judgement = judge
                .score(
                    text,
                    candidates
                        .iter()
                        .map(|c| c.transcript_text.clone())
                        .collect(),
                )
                .await?;
            (judgement.scores, judgement.judged_by)
        }
        Some(judge) => (Vec::new(), judge.name()),
        None => (
            candidates.iter().map(|c| c.similarity).collect(),
            "similarity".to_string(),
        ),
    };

    let mut scored: Vec<(f32, &EchoItem)> = scores.iter().copied().zip(candidates.iter()).collect();
    // Descending by score. `total_cmp` rather than `partial_cmp().unwrap()`:
    // a NaN from a model would otherwise panic inside a sort.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    // One transaction, so a half-written judgement can never be mistaken
    // for a complete one by the marker that follows it.
    let mut tx = pool.begin().await?;

    sqlx::query!(
        "delete from capture_echo where capture_event_id = $1",
        capture_event_id
    )
    .execute(&mut *tx)
    .await?;

    for (rank, (score, item)) in scored.iter().enumerate() {
        sqlx::query!(
            r#"insert into capture_echo
                   (capture_event_id, echo_event_id, rank, score, similarity)
               values ($1, $2, $3, $4, $5)"#,
            capture_event_id,
            item.capture_event_id,
            rank as i32,
            score,
            item.similarity,
        )
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query!(
        r#"insert into capture_echo_judged (capture_event_id, judged_by, judged_at)
           values ($1, $2, now())
           on conflict (capture_event_id) do update
               set judged_by = excluded.judged_by, judged_at = excluded.judged_at"#,
        capture_event_id,
        judged_by,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // The shape of the verdict, not just that there was one. `best` and
    // `runner_up` together are the evidence for whether the judge is
    // separating cleanly or guessing near the threshold — which is the
    // question the 0.5 default still has to answer.
    let best = scored.first().map(|(score, _)| *score);
    let runner_up = scored.get(1).map(|(score, _)| *score);
    tracing::info!(
        %capture_event_id,
        judged_by = %judged_by,
        candidates = scored.len(),
        ms = started.elapsed().as_millis(),
        best = ?best,
        runner_up = ?runner_up,
        shown = scored
            .iter()
            .filter(|(score, _)| *score >= thresholds.min_score)
            .count(),
        "echo judged and stored"
    );
    Ok(())
}

/// The stored echo, or `None` when this capture has never been judged.
///
/// The distinction matters: "judged, and nothing echoed" is the common
/// case — most notes echo nothing — and must not look like "not computed
/// yet", or every such capture would be re-judged forever.
pub async fn stored(
    pool: &PgPool,
    capture_event_id: Uuid,
    min_score: f32,
    limit: i64,
) -> anyhow::Result<Option<Vec<EchoItem>>> {
    let judged = sqlx::query_scalar!(
        "select judged_by from capture_echo_judged where capture_event_id = $1",
        capture_event_id
    )
    .fetch_optional(pool)
    .await?;

    if judged.is_none() {
        return Ok(None);
    }

    // Joined against `capture_search` rather than stored alongside: the
    // transcript has one home, so a correction shows through here without
    // this table knowing anything about corrections.
    let rows = sqlx::query!(
        r#"
        select e.echo_event_id, e.score, e.similarity, s.transcript, s.occurred_at
        from capture_echo e
        join capture_search s on s.event_id = e.echo_event_id
        where e.capture_event_id = $1 and e.score >= $2
        order by e.rank
        limit $3
        "#,
        capture_event_id,
        min_score,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(Some(
        rows.into_iter()
            .map(|row| EchoItem {
                capture_event_id: row.echo_event_id,
                transcript_text: row.transcript,
                occurred_at: row.occurred_at,
                similarity: row.similarity,
                rerank_score: Some(row.score),
            })
            .collect(),
    ))
}

/// Forgets a capture's judged echo, so it will be judged again.
///
/// Used when the capture's own text changes: a correction makes the old
/// judgement a verdict on a sentence that no longer exists.
pub async fn forget(pool: &PgPool, capture_event_id: Uuid) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "delete from capture_echo where capture_event_id = $1",
        capture_event_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "delete from capture_echo_judged where capture_event_id = $1",
        capture_event_id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Captures that can be judged and have not been, newest first.
///
/// Newest first because an echo matters most while its capture is fresh:
/// that is when it is looked at. A backlog of old ones still drains, a
/// few per pass.
pub async fn unjudged(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        select cs.event_id
        from capture_search cs
        left join capture_echo_judged j on j.capture_event_id = cs.event_id
        where j.capture_event_id is null
          and cs.embedding is not null
        order by cs.occurred_at desc
        limit $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// Judges a capture that is already in `capture_search`.
///
/// The backfill path: a capture recorded before judgements were stored,
/// or one whose judgement failed, gets one the next time anybody looks at
/// it, or when [`crate::pipeline::watch_echoes`] comes by. Everything it needs is already in the database, so unlike
/// [`judge_and_store`] it takes no embedding or text.
pub async fn judge_stored_capture(
    pool: &PgPool,
    judge: Option<&Judge>,
    capture_event_id: Uuid,
    thresholds: Thresholds,
) -> anyhow::Result<()> {
    let target = sqlx::query!(
        r#"select transcript, embedding as "embedding: pgvector::Vector", occurred_at
           from capture_search where event_id = $1"#,
        capture_event_id,
    )
    .fetch_optional(pool)
    .await?;

    // A capture with no searchable row — redacted, or never indexed — has
    // nothing to echo against and nothing to be echoed by.
    let Some(target) = target else {
        return Ok(());
    };
    let Some(embedding) = target.embedding else {
        return Ok(());
    };

    judge_and_store(
        pool,
        judge,
        capture_event_id,
        &embedding,
        &target.transcript,
        target.occurred_at,
        thresholds,
    )
    .await
}
