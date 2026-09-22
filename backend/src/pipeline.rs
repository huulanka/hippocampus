//! What still has to happen to a capture after it is safely stored.
//!
//! A capture is written to `capture_content` in a few milliseconds and is
//! at that moment permanent. Two further things then have to happen
//! before it is *usable*: it has to be embedded into `capture_search`, and
//! it has to be read by the structuring model. Both of those talk to
//! something that can be down — a local model, a paid API — and both used
//! to fail the same way: a log line, and never again.
//!
//! The consequence was not visible and that is exactly what made it bad.
//! An unstructured capture is still in the timeline and still findable by
//! its words, so nothing looks broken; it simply never has entities,
//! never a resolved date, and never appears in Resurface. The note is
//! there and its meaning is quietly missing.
//!
//! So both steps are written down rather than merely attempted, and a
//! loop finishes what did not finish the first time. The rule the rest of
//! the code can now rely on: **once a capture is stored, storing it
//! succeeded** — anything after that is this module's problem, not the
//! caller's.

use std::time::Duration;

use chrono_tz::Tz;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::openrouter::SpokenAt;

/// Opens the bookkeeping for a capture, or reopens it.
///
/// Called before the work starts, including the very first time, so a
/// capture is on the unfinished list from the moment it exists rather
/// than only once something has already gone wrong.
pub async fn begin(pool: &PgPool, capture_event_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        insert into capture_pipeline (capture_event_id) values ($1)
        on conflict (capture_event_id) do update
            -- A correction re-derives everything, so whatever was true of
            -- the old wording stops being true here — including having
            -- been given up on.
            set structured_at = null,
                structured_by = null,
                abandoned_at = null,
                last_error = null
        "#,
        capture_event_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Records that the capture is embedded and findable.
pub async fn mark_indexed(pool: &PgPool, capture_event_id: Uuid) {
    let result = sqlx::query!(
        r#"
        insert into capture_pipeline (capture_event_id, indexed_at) values ($1, now())
        on conflict (capture_event_id) do update set indexed_at = now()
        "#,
        capture_event_id,
    )
    .execute(pool)
    .await;

    if let Err(err) = result {
        // The capture and its search row are both safe. Losing the note
        // that says so costs one redundant re-index, not a note.
        tracing::warn!(?err, %capture_event_id, "could not record that a capture was indexed");
    }
}

/// Records that a structuring run finished, whichever way it went.
///
/// Success and "the model had nothing to say about this one" are the same
/// outcome here, deliberately: both mean the capture has been read, and
/// neither is worth paying to read a second time.
pub async fn record_structuring(
    pool: &PgPool,
    capture_event_id: Uuid,
    model: &str,
    error: Option<&str>,
    max_attempts: i32,
) {
    let result = sqlx::query!(
        r#"
        insert into capture_pipeline
            (capture_event_id, structured_by, structured_at, attempts, last_attempt_at, last_error)
        values (
            $1,
            case when $3::text is null then $2 end,
            case when $3::text is null then now() end,
            1,
            now(),
            $3
        )
        on conflict (capture_event_id) do update set
            structured_by = case when $3::text is null then $2 end,
            structured_at = case when $3::text is null then now() end,
            attempts = capture_pipeline.attempts + 1,
            last_attempt_at = now(),
            last_error = $3,
            -- Given up on only while it is still failing, and only once
            -- it has failed often enough that the next call would be
            -- money spent on the same answer.
            abandoned_at = case
                when $3::text is not null and capture_pipeline.attempts + 1 >= $4 then now()
            end
        returning abandoned_at
        "#,
        capture_event_id,
        model,
        error,
        max_attempts,
    )
    .fetch_one(pool)
    .await;

    match result {
        Ok(row) if row.abandoned_at.is_some() => tracing::error!(
            %capture_event_id,
            attempts = max_attempts,
            "giving up on structuring this capture; it waits for a person now"
        ),
        Ok(_) => {}
        Err(err) => tracing::warn!(
            ?err,
            %capture_event_id,
            "could not record the structuring outcome"
        ),
    }
}

/// A capture that is stored but not yet finished, with everything needed
/// to finish it.
pub struct Unfinished {
    pub capture_event_id: Uuid,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub transcript: String,
    pub spoken_at: SpokenAt,
    pub needs_indexing: bool,
    pub needs_structuring: bool,
}

/// Finds captures that are stored but not yet usable.
///
/// Redacted captures are excluded: redaction empties the content on
/// purpose, so there is nothing left to embed and nothing worth paying a
/// model to read.
///
/// `backoff` keeps a permanently failing capture from being retried on
/// every tick — the first attempt is immediate, each one after it waits.
pub async fn unfinished(
    pool: &PgPool,
    fallback_timezone: Tz,
    backoff: Duration,
    limit: i64,
) -> anyhow::Result<Vec<Unfinished>> {
    let rows = sqlx::query!(
        r#"
        select
            p.capture_event_id,
            p.indexed_at,
            p.structured_at,
            e.occurred_at,
            e.payload,
            coalesce(
                (select tc.text from transcript_content tc
                 where tc.capture_event_id = p.capture_event_id and tc.redacted_at is null
                 order by tc.created_at desc limit 1),
                cc.text
            ) as "transcript?"
        from capture_pipeline p
        join events e on e.id = p.capture_event_id
        join capture_content cc on cc.event_id = p.capture_event_id
        where (p.indexed_at is null or p.structured_at is null)
          and p.abandoned_at is null
          and cc.redacted_at is null
          and (
            p.last_attempt_at is null
            or p.last_attempt_at < now() - make_interval(secs => $1)
          )
        order by p.last_attempt_at nulls first, e.occurred_at
        limit $2
        "#,
        backoff.as_secs_f64(),
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            // Neither transcript nor content is not a failure to retry —
            // there is no text to work with, and there never will be.
            let transcript = row.transcript?;
            if transcript.trim().is_empty() {
                return None;
            }
            let timezone = row
                .payload
                .get("timezone")
                .and_then(|value| value.as_str())
                .and_then(|name| name.parse::<Tz>().ok())
                .unwrap_or(fallback_timezone);
            Some(Unfinished {
                capture_event_id: row.capture_event_id,
                occurred_at: row.occurred_at,
                transcript,
                spoken_at: SpokenAt {
                    utc: row.occurred_at,
                    timezone,
                },
                needs_indexing: row.indexed_at.is_none(),
                needs_structuring: row.structured_at.is_none(),
            })
        })
        .collect())
}

/// Puts a capture that was given up on back on the list, so a person can
/// ask for one more try. False when there is no such capture, or when it
/// was already finished.
pub async fn revive(pool: &PgPool, capture_event_id: Uuid) -> Result<bool, sqlx::Error> {
    let affected = sqlx::query!(
        r#"
        update capture_pipeline
        set abandoned_at = null, attempts = 0, last_attempt_at = null, last_error = null
        where capture_event_id = $1
          and (indexed_at is null or structured_at is null)
        "#,
        capture_event_id,
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok(affected > 0)
}

/// How many captures are waiting, and how many have been given up on.
pub async fn counts(pool: &PgPool) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query!(
        r#"
        select
            count(*) filter (
                where (p.indexed_at is null or p.structured_at is null)
                  and p.abandoned_at is null
            ) as "waiting!",
            count(*) filter (where p.abandoned_at is not null) as "given_up!"
        from capture_pipeline p
        join capture_content cc on cc.event_id = p.capture_event_id
        where cc.redacted_at is null
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok((row.waiting, row.given_up))
}

/// Finishes one capture as far as it can get right now.
///
/// Indexing first: it is local, free, and everything else — search, echo,
/// the timeline's own ordering — depends on it. Structuring second,
/// because it is the part that costs money and the part that can be down.
pub async fn finish(state: &AppState, item: &Unfinished, max_attempts: i32) {
    if item.needs_indexing {
        match crate::routes::captures::index_for_search(
            state,
            item.capture_event_id,
            item.occurred_at,
            &item.transcript,
        )
        .await
        {
            Ok(()) => mark_indexed(&state.pool, item.capture_event_id).await,
            Err(err) => {
                tracing::warn!(?err, id = %item.capture_event_id, "re-indexing failed");
                // Structuring is not attempted on this pass: the two
                // failures would share one attempt counter, and a capture
                // could be given up on for a reason that was never about
                // the model.
                return;
            }
        }
    }

    if item.needs_structuring {
        crate::structuring::structure_capture_in_background(
            state.pool.clone(),
            state.openrouter.clone(),
            state.embedder.clone(),
            item.capture_event_id,
            item.transcript.clone(),
            item.spoken_at,
            max_attempts,
        )
        .await;
    }
}

/// Runs forever, finishing what did not finish the first time.
///
/// A loop rather than "next time someone opens it", which is how the echo
/// backfill works: an echo is only worth having when a capture is being
/// looked at, but a capture's *entities* are what make it findable from
/// somewhere else. Waiting for someone to open it would mean the note
/// nobody revisits — which is precisely the note this whole system exists
/// to hand back — stays meaningless forever.
pub fn watch(state: AppState, settings: RetrySettings) {
    if !settings.enabled {
        tracing::warn!(
            "STRUCTURING_RETRY_ENABLED=false — a capture whose structuring fails stays \
             unstructured until asked for by hand"
        );
        return;
    }

    tracing::info!(
        every_secs = settings.interval.as_secs(),
        batch = settings.batch,
        max_attempts = settings.max_attempts,
        "finishing unfinished captures in the background"
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(settings.interval).await;

            let items = match unfinished(
                &state.pool,
                state.timezone,
                settings.backoff,
                settings.batch,
            )
            .await
            {
                Ok(items) => items,
                Err(err) => {
                    tracing::warn!(?err, "could not look for unfinished captures");
                    continue;
                }
            };

            // Without a structuring model, a capture that only needs
            // structuring is not unfinished — it is finished as far as
            // this installation goes. Filtering here rather than in the
            // query keeps the meaning of the table the same whether or
            // not a key happens to be configured today: switch one on and
            // the backlog is picked up on the next tick, no backfill
            // needed.
            let actionable = state.openrouter.is_some();
            let items: Vec<Unfinished> = items
                .into_iter()
                .filter(|item| item.needs_indexing || actionable)
                .collect();

            if items.is_empty() {
                continue;
            }

            tracing::info!(
                count = items.len(),
                "finishing captures that were left unfinished"
            );
            // Sequentially: this is catch-up work behind a system that is
            // otherwise idle, and firing a batch of paid calls in parallel
            // at a provider that just failed is how a small outage becomes
            // a rate limit.
            for item in &items {
                finish(&state, item, settings.max_attempts).await;
            }
        }
    });
}

/// How hard to try, and how often.
#[derive(Debug, Clone, Copy)]
pub struct RetrySettings {
    pub enabled: bool,
    pub interval: Duration,
    pub backoff: Duration,
    pub batch: i64,
    pub max_attempts: i32,
}
