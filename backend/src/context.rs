//! What a note was said next to: the meeting around it and the notes just
//! before it.
//!
//! People remember through context — "I said that in the meeting with
//! Paul", "that was the afternoon of the workshop" — and until now a note
//! knew neither. Both are cheap to offer and easy to get wrong, because
//! time alone proves nothing: a note spoken during a meeting can be about
//! the meeting or a reminder to buy milk, and two notes five minutes apart
//! can be one train of thought or two unrelated ones. So time only
//! nominates, and a model that reads the note decides — the same split
//! entity resolution settled on (docs/entity-resolution.md): similarity
//! finds candidates, a reader judges them.
//!
//! **Meetings** come from the Macs. Each one reads its own ticked
//! calendars around the notes it has not looked up yet, matches every
//! meeting against the graph through the brief's whole-word matching, and
//! the backend keeps only what matched: which entities, which phase, how
//! many minutes. The title and the attendee list are dropped here, the
//! same way `POST /brief` drops them (ADR 0016).
//!
//! **Episodes** need nothing from outside. A note is read against up to
//! three notes spoken in the half hour before it; each pair judged to
//! carry on from each other is an edge, and an episode is what those edges
//! connect.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use contracts::{
    CaptureOccasion, EntityRef, EpisodeNote, OccasionsOffered, OfferOccasionsRequest,
    PendingOccasion,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::brief;
use crate::events;

/// How far back a note looks for one it might continue.
///
/// Half an hour: long enough for a workshop's notes to chain up through
/// the gaps between them, short enough that "the same afternoon" does not
/// become "the same episode" by default. It only nominates; the reading
/// decides.
pub const EPISODE_WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(30);

/// How many earlier notes a note is read against. The nearest ones; a
/// fourth note back in half an hour is almost always reached through the
/// third anyway, since episodes chain.
const PREVIOUS_NOTES: i64 = 3;

/// A note is left alone this long after it arrived, so the Mac that
/// recorded it has had time to look it up in its calendar, and one call
/// can read both the note before it and the meeting around it.
const SETTLE: chrono::TimeDelta = chrono::TimeDelta::minutes(3);

const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const SWEEP_BATCH: i64 = 20;

/// Longest a note is shown to the judge. The question is what a note is
/// about, which the first few hundred characters answer.
const NOTE_CHARS: usize = 700;

/// Deepest an episode is walked. A chain longer than this is a day of
/// notes that each carry on from the one before, and showing all of it as
/// "this episode" would stop meaning anything.
const EPISODE_DEPTH: i32 = 12;

const PHASES: [&str; 3] = ["before", "during", "after"];

/// Notes this Mac has not looked up in its calendar yet, newest first —
/// the newest are the ones a meeting is most likely to still be in the
/// calendar for.
pub async fn pending_for(
    pool: &PgPool,
    checker: Uuid,
    limit: i64,
) -> Result<Vec<PendingOccasion>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        select cs.event_id, cs.occurred_at
        from capture_search cs
        where not exists (
            select 1 from capture_occasion_checked c
            where c.capture_event_id = cs.event_id and c.checker = $1
        )
        order by cs.occurred_at desc
        limit $2
        "#,
        checker,
        limit,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PendingOccasion {
            capture_event_id: row.event_id,
            occurred_at: row.occurred_at,
        })
        .collect())
}

/// Keeps what a Mac found around its notes, as far as the graph knows it.
///
/// A meeting that matches nothing known is dropped whole: "Abstimmung Q3"
/// with nobody invited says nothing about the note, and keeping it would
/// mean keeping the calendar. A note already looked up by this Mac is
/// skipped, so sending the same batch twice offers nothing twice.
pub async fn offer(
    pool: &PgPool,
    request: &OfferOccasionsRequest,
) -> anyhow::Result<OccasionsOffered> {
    let candidates = crate::routes::intentions::candidates(pool)
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    let mut report = OccasionsOffered::default();

    for capture in &request.captures {
        let exists = sqlx::query_scalar!(
            r#"select exists (select 1 from capture_search where event_id = $1) as "exists!""#,
            capture.capture_event_id,
        )
        .fetch_one(pool)
        .await?;
        // A redacted note, or an id this backend never had. Not an error:
        // a Mac's list can be a minute old.
        if !exists {
            continue;
        }

        let mut tx = pool.begin().await?;
        let fresh = sqlx::query!(
            r#"
            insert into capture_occasion_checked (capture_event_id, checker)
            values ($1, $2)
            on conflict do nothing
            "#,
            capture.capture_event_id,
            request.checker,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        if !fresh {
            tx.rollback().await?;
            continue;
        }
        report.checked += 1;

        for meeting in &capture.meetings {
            if !PHASES.contains(&meeting.phase.as_str()) {
                continue;
            }
            let mut entity_ids: Vec<Uuid> =
                brief::find(&candidates, &meeting.title, &meeting.people)
                    .into_iter()
                    .map(|m| m.id)
                    .collect();
            entity_ids.sort();
            entity_ids.dedup();
            if entity_ids.is_empty() {
                continue;
            }

            let occasion_id = Uuid::new_v4();
            let minutes = i32::try_from(meeting.minutes).unwrap_or(i32::MAX);
            // Ids only. The words that matched are the calendar's, and the
            // log is never modified — whatever went in could never come
            // out again.
            let event = events::append_tx(
                &mut tx,
                occasion_id,
                1,
                "occasion.offered",
                &json!({
                    "capture_event_id": capture.capture_event_id,
                    "phase": meeting.phase,
                    "minutes": minutes,
                    "entity_ids": entity_ids,
                    "checker": request.checker,
                }),
                "calendar",
            )
            .await?;

            sqlx::query!(
                r#"
                insert into capture_occasions
                    (id, capture_event_id, checker, phase, minutes, offered_event_id)
                values ($1, $2, $3, $4, $5, $6)
                "#,
                occasion_id,
                capture.capture_event_id,
                request.checker,
                meeting.phase,
                minutes,
                event.id,
            )
            .execute(&mut *tx)
            .await?;

            for entity_id in &entity_ids {
                sqlx::query!(
                    r#"insert into capture_occasion_entities (occasion_id, entity_id) values ($1, $2)"#,
                    occasion_id,
                    entity_id,
                )
                .execute(&mut *tx)
                .await?;
            }
            report.kept += 1;
        }

        tx.commit().await?;
    }

    Ok(report)
}

/// Notes with something left to read: neighbours in time not yet judged,
/// or a meeting offered since.
pub async fn due(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, sqlx::Error> {
    let settled = Utc::now() - SETTLE;
    sqlx::query_scalar!(
        r#"
        select cs.event_id as "event_id!"
        from capture_search cs
        left join capture_pipeline p on p.capture_event_id = cs.event_id
        where coalesce(p.indexed_at, cs.occurred_at) < $1
          and (
              not exists (select 1 from capture_context_judged j where j.capture_event_id = cs.event_id)
              or exists (
                  select 1 from capture_occasions o
                  where o.capture_event_id = cs.event_id and o.verdict is null
              )
          )
        order by cs.occurred_at desc
        limit $2
        "#,
        settled,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// An earlier note, as the judge sees it.
struct Previous {
    id: Uuid,
    text: String,
    minutes: i64,
}

/// A meeting, as the judge sees it: who and what it was with.
struct Offered {
    id: Uuid,
    phase: String,
    minutes: i32,
    with: Vec<(String, String)>,
}

#[derive(Debug, Default, Deserialize)]
struct Verdict {
    #[serde(default)]
    continues: Vec<String>,
    #[serde(default)]
    belongs_to: Vec<String>,
}

const JUDGE_PROMPT: &str = r#"You decide which circumstances around one personal voice note actually belong to it. The notes may be in German or English.

You get the note, possibly some notes the same person spoke shortly before it (tagged p1, p2, …), and possibly meetings from their calendar around the time it was spoken (tagged o1, o2, …), each described only by the people and subjects it was with.

"continues": the tags of earlier notes this note carries on from — the same situation, conversation, task, event or train of thought. Being on the same day, in the same mood or merely spoken soon after is not enough. A note about a customer project after a note about dinner plans does not continue it.

"belongs_to": the tags of meetings this note is part of — preparation for it, something said during it about it, or follow-up to it. Judge by what the note is about: it has to concern that meeting's people or subjects, or plainly the meeting itself. A note spoken during a meeting about something else entirely (an errand, a private thought, another project) does not belong to it.

When unsure, leave the tag out. Most notes continue nothing and belong to no meeting.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"continues": ["p1"], "belongs_to": ["o1"]}"#;

fn phase_line(phase: &str, minutes: i32) -> String {
    match phase {
        "before" => format!("the note was spoken {minutes} minutes before it began"),
        "after" => format!("the note was spoken {minutes} minutes after it ended"),
        _ => "the note was spoken while it was going on".to_string(),
    }
}

fn describe(text: &str, previous: &[Previous], offered: &[Offered]) -> String {
    let mut out = String::new();
    if !previous.is_empty() {
        out.push_str("Notes spoken shortly before it:\n");
        for (i, p) in previous.iter().enumerate() {
            let short: String = p.text.chars().take(NOTE_CHARS).collect();
            out.push_str(&format!(
                "- p{} ({} minutes earlier): {}\n",
                i + 1,
                p.minutes,
                short
            ));
        }
        out.push('\n');
    }
    if !offered.is_empty() {
        out.push_str("Meetings around it:\n");
        for (i, o) in offered.iter().enumerate() {
            let with: Vec<String> = o
                .with
                .iter()
                .map(|(name, kind)| format!("{name} ({kind})"))
                .collect();
            out.push_str(&format!(
                "- o{}: a meeting with {}; {}\n",
                i + 1,
                with.join(", "),
                phase_line(&o.phase, o.minutes)
            ));
        }
        out.push('\n');
    }
    let short: String = text.chars().take(NOTE_CHARS).collect();
    out.push_str(&format!("The note:\n{short}"));
    out
}

/// Reads one note against its neighbours in time and the meetings around
/// it. `Ok` once it has been read — with or without a call.
pub async fn judge(state: &AppState, capture_event_id: Uuid) -> anyhow::Result<()> {
    let pool = &state.pool;
    let Some(note) = sqlx::query!(
        r#"select transcript, occurred_at from capture_search where event_id = $1"#,
        capture_event_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };

    let already = sqlx::query_scalar!(
        r#"select exists (select 1 from capture_context_judged where capture_event_id = $1) as "exists!""#,
        capture_event_id,
    )
    .fetch_one(pool)
    .await?;

    let previous = if already {
        Vec::new()
    } else {
        earlier_notes(pool, capture_event_id, note.occurred_at).await?
    };
    let offered = unjudged_occasions(pool, capture_event_id).await?;

    // Nothing to read against: marked, and no call. This is most notes on
    // a quiet day, and the reason the sweep costs nothing when idle.
    if previous.is_empty() && offered.is_empty() {
        if !already {
            mark_judged(pool, capture_event_id, None).await?;
        }
        return Ok(());
    }

    let Some(model) = state.openrouter.as_ref() else {
        // No model, no reading. Left undecided rather than decided
        // "unrelated": a backend given a key later should still read it.
        return Ok(());
    };

    let verdict: Verdict = model
        .complete_json(
            "context",
            JUDGE_PROMPT,
            &describe(&note.transcript, &previous, &offered),
        )
        .await?;

    let continues: Vec<Uuid> = previous
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            verdict
                .continues
                .iter()
                .any(|t| t.trim() == format!("p{}", i + 1))
        })
        .map(|(_, p)| p.id)
        .collect();
    let belongs: Vec<Uuid> = offered
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            verdict
                .belongs_to
                .iter()
                .any(|t| t.trim() == format!("o{}", i + 1))
        })
        .map(|(_, o)| o.id)
        .collect();

    let mut tx = pool.begin().await?;
    for p in &previous {
        sqlx::query!(
            r#"
            insert into capture_continuations (capture_event_id, previous_event_id, continues, model)
            values ($1, $2, $3, $4)
            on conflict (capture_event_id, previous_event_id)
                do update set continues = excluded.continues, model = excluded.model, judged_at = now()
            "#,
            capture_event_id,
            p.id,
            continues.contains(&p.id),
            model.model_name(),
        )
        .execute(&mut *tx)
        .await?;
    }
    for o in &offered {
        let verdict = if belongs.contains(&o.id) {
            "belongs"
        } else {
            "unrelated"
        };
        sqlx::query!(
            r#"update capture_occasions set verdict = $2, model = $3, judged_at = now() where id = $1"#,
            o.id,
            verdict,
            model.model_name(),
        )
        .execute(&mut *tx)
        .await?;
    }
    if !already {
        sqlx::query!(
            r#"
            insert into capture_context_judged (capture_event_id, model) values ($1, $2)
            on conflict (capture_event_id) do update set model = excluded.model, judged_at = now()
            "#,
            capture_event_id,
            model.model_name(),
        )
        .execute(&mut *tx)
        .await?;
    }
    events::append_tx(
        &mut tx,
        Uuid::new_v4(),
        1,
        "context.judged",
        &json!({
            "capture_event_id": capture_event_id,
            "read_against": previous.iter().map(|p| p.id).collect::<Vec<_>>(),
            "continues": continues,
            "occasions": offered.iter().map(|o| o.id).collect::<Vec<_>>(),
            "belongs": belongs,
        }),
        model.model_name(),
    )
    .await?;
    tx.commit().await?;

    Ok(())
}

async fn mark_judged(pool: &PgPool, id: Uuid, model: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        insert into capture_context_judged (capture_event_id, model) values ($1, $2)
        on conflict (capture_event_id) do nothing
        "#,
        id,
        model,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn earlier_notes(
    pool: &PgPool,
    id: Uuid,
    at: DateTime<Utc>,
) -> Result<Vec<Previous>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        select cs.event_id, cs.transcript, cs.occurred_at
        from capture_search cs
        where cs.occurred_at < $2
          and cs.occurred_at >= $3
          and cs.event_id <> $1
        order by cs.occurred_at desc
        limit $4
        "#,
        id,
        at,
        at - EPISODE_WINDOW,
        PREVIOUS_NOTES,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| Previous {
            id: row.event_id,
            text: row.transcript,
            minutes: (at - row.occurred_at).num_minutes().max(0),
        })
        .collect())
}

async fn unjudged_occasions(pool: &PgPool, id: Uuid) -> Result<Vec<Offered>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        select o.id, o.phase, o.minutes, e.name, e.entity_type
        from capture_occasions o
        join capture_occasion_entities oe on oe.occasion_id = o.id
        join entities start on start.id = oe.entity_id
        join entities e on e.id = coalesce(start.merged_into, start.id)
        where o.capture_event_id = $1 and o.verdict is null
        order by o.created_at, o.id, e.name
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;

    let mut out: Vec<Offered> = Vec::new();
    for row in rows {
        match out.last_mut() {
            Some(last) if last.id == row.id => {
                let pair = (row.name, row.entity_type);
                if !last.with.contains(&pair) {
                    last.with.push(pair);
                }
            }
            _ => out.push(Offered {
                id: row.id,
                phase: row.phase,
                minutes: row.minutes,
                with: vec![(row.name, row.entity_type)],
            }),
        }
    }
    Ok(out)
}

/// The meetings a note was read as belonging to, merges followed.
pub async fn occasions_for(pool: &PgPool, id: Uuid) -> Result<Vec<CaptureOccasion>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        select o.id, o.phase, o.minutes, o.model, e.id as entity_id, e.name, e.entity_type
        from capture_occasions o
        join capture_occasion_entities oe on oe.occasion_id = o.id
        join entities start on start.id = oe.entity_id
        join entities e on e.id = coalesce(start.merged_into, start.id)
        where o.capture_event_id = $1 and o.verdict = 'belongs'
        order by o.minutes, o.id, e.name
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;

    let mut order: Vec<Uuid> = Vec::new();
    let mut by_id: HashMap<Uuid, CaptureOccasion> = HashMap::new();
    for row in rows {
        let occasion = by_id.entry(row.id).or_insert_with(|| {
            order.push(row.id);
            CaptureOccasion {
                phase: row.phase.clone(),
                minutes: row.minutes,
                entities: Vec::new(),
                model: row.model.clone(),
            }
        });
        if !occasion.entities.iter().any(|e| e.id == row.entity_id) {
            occasion.entities.push(EntityRef {
                id: row.entity_id,
                name: row.name,
                entity_type: row.entity_type,
            });
        }
    }
    Ok(order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}

/// The other notes of this note's episode, oldest first.
pub async fn episode(pool: &PgPool, id: Uuid) -> Result<Vec<EpisodeNote>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        with recursive edges as (
            select capture_event_id as a, previous_event_id as b
            from capture_continuations where continues
            union all
            select previous_event_id, capture_event_id
            from capture_continuations where continues
        ),
        walk (id, depth) as (
            select $1::uuid, 0
            union
            select e.b, w.depth + 1
            from walk w
            join edges e on e.a = w.id
            where w.depth < $2
        )
        select distinct cs.event_id as "event_id!", cs.transcript as "transcript!", cs.occurred_at as "occurred_at!"
        from walk w
        join capture_search cs on cs.event_id = w.id
        where w.id <> $1
        order by 3
        "#,
        id,
        EPISODE_DEPTH,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| EpisodeNote {
            capture_event_id: row.event_id,
            transcript_text: row.transcript,
            occurred_at: row.occurred_at,
        })
        .collect())
}

/// Episode neighbours for several notes at once: each note's direct
/// continuations either way. One hop is enough for widening an answer;
/// the whole episode is for the detail view.
pub async fn neighbours(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Uuid>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"
        select distinct other as "other!" from (
            select previous_event_id as other from capture_continuations
            where continues and capture_event_id = any($1)
            union
            select capture_event_id from capture_continuations
            where continues and previous_event_id = any($1)
        ) n
        where not (other = any($1))
        "#,
        ids,
    )
    .fetch_all(pool)
    .await
}

/// Everything read about a note's surroundings goes when its words go.
pub async fn forget(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"delete from capture_occasions where capture_event_id = $1"#,
        id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        r#"delete from capture_continuations where capture_event_id = $1 or previous_event_id = $1"#,
        id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        r#"delete from capture_context_judged where capture_event_id = $1"#,
        id
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A corrected note reads differently, so whatever was read from the old
/// wording is read again: its continuations go, its meetings wait for a
/// new verdict. The meetings themselves stay — the calendar did not
/// change.
pub async fn reread(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        r#"delete from capture_continuations where capture_event_id = $1 or previous_event_id = $1"#,
        id
    )
    .execute(&mut *tx)
    .await?;
    // The notes after it were read against the old wording too.
    sqlx::query!(
        r#"
        delete from capture_context_judged
        where capture_event_id = $1
           or capture_event_id in (
               select cs.event_id from capture_search cs, capture_search me
               where me.event_id = $1
                 and cs.occurred_at > me.occurred_at
                 and cs.occurred_at <= me.occurred_at + make_interval(mins => $2)
           )
        "#,
        id,
        EPISODE_WINDOW.num_minutes() as i32,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"update capture_occasions set verdict = null, model = null, judged_at = null where capture_event_id = $1"#,
        id
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Reads notes in the background, newest first, stopping at the first
/// failure: the next note would be asking the provider that just said no.
pub fn watch(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            let due = match due(&state.pool, SWEEP_BATCH).await {
                Ok(due) => due,
                Err(err) => {
                    tracing::warn!(?err, "could not look for notes to read in context");
                    continue;
                }
            };
            let mut read = 0;
            for id in due {
                match judge(&state, id).await {
                    Ok(()) => read += 1,
                    Err(err) => {
                        tracing::warn!(?err, capture_event_id = %id, "could not read a note in context");
                        break;
                    }
                }
            }
            if read > 0 {
                tracing::debug!(read, "read notes against what they were said next to");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_judge_sees_people_not_meeting_titles() {
        let text = describe(
            "Paul fragen, ob die Abnahme steht",
            &[],
            &[Offered {
                id: Uuid::nil(),
                phase: "before".into(),
                minutes: 20,
                with: vec![("Paul".into(), "Person".into())],
            }],
        );
        assert!(text.contains(
            "o1: a meeting with Paul (Person); the note was spoken 20 minutes before it began"
        ));
    }

    #[test]
    fn earlier_notes_are_tagged_nearest_first() {
        let text = describe(
            "and the second thing",
            &[
                Previous {
                    id: Uuid::nil(),
                    text: "first thing".into(),
                    minutes: 4,
                },
                Previous {
                    id: Uuid::nil(),
                    text: "older thing".into(),
                    minutes: 19,
                },
            ],
            &[],
        );
        assert!(text.contains("- p1 (4 minutes earlier): first thing"));
        assert!(text.contains("- p2 (19 minutes earlier): older thing"));
        assert!(!text.contains("Meetings around it"));
    }
}

/// Against a real database:
///
///     cargo test -p backend --features db-tests context
#[cfg(all(test, feature = "db-tests"))]
mod db_tests {
    use chrono::{DateTime, Utc};
    use contracts::{CaptureMeetings, NearbyMeeting, OfferOccasionsRequest};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{episode, forget, neighbours, occasions_for, offer, pending_for};

    async fn a_capture(pool: &PgPool, text: &str, at: DateTime<Utc>) -> Uuid {
        let event = sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source)
               values (gen_random_uuid(), 1, 'capture.recorded', '{}'::jsonb, 'test')
               returning id"#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into capture_search (event_id, transcript, occurred_at) values ($1, $2, $3)",
            event,
            text,
            at,
        )
        .execute(pool)
        .await
        .unwrap();
        event
    }

    async fn an_entity(pool: &PgPool, name: &str) -> Uuid {
        sqlx::query!(
            "insert into entity_type_registry (entity_type) values ('Person') on conflict do nothing"
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar!(
            "insert into entities (entity_type, name) values ('Person', $1) returning id",
            name
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn meeting(title: &str, people: &[&str], phase: &str, minutes: u32) -> NearbyMeeting {
        NearbyMeeting {
            title: title.into(),
            people: people.iter().map(|p| p.to_string()).collect(),
            phase: phase.into(),
            minutes,
        }
    }

    async fn continues(pool: &PgPool, note: Uuid, previous: Uuid, yes: bool) {
        sqlx::query!(
            "insert into capture_continuations (capture_event_id, previous_event_id, continues, model) values ($1, $2, $3, 'test-model')",
            note,
            previous,
            yes,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_meeting_is_kept_as_who_it_was_with_and_its_title_is_not(pool: PgPool) {
        let paul = an_entity(&pool, "Paul").await;
        let note = a_capture(&pool, "Abnahme vorbereiten", Utc::now()).await;
        let checker = Uuid::new_v4();

        let report = offer(
            &pool,
            &OfferOccasionsRequest {
                checker,
                captures: vec![CaptureMeetings {
                    capture_event_id: note,
                    meetings: vec![
                        meeting("Abnahme Hafenportal", &["Paul Hartmann"], "before", 20),
                        // Nothing known in it: dropped whole.
                        meeting("Abstimmung Q3", &[], "after", 5),
                    ],
                }],
            },
        )
        .await
        .unwrap();
        assert_eq!(report.checked, 1);
        assert_eq!(report.kept, 1);

        let kept = sqlx::query!("select phase, minutes, verdict from capture_occasions")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].phase.as_str(), kept[0].minutes), ("before", 20));
        assert!(
            kept[0].verdict.is_none(),
            "time only nominates; nothing is decided yet"
        );

        let payloads = sqlx::query_scalar!(
            "select payload::text as \"payload!\" from events where event_type = 'occasion.offered'"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(payloads.len(), 1);
        assert!(
            !payloads[0].contains("Hafenportal"),
            "the title must not reach the log"
        );
        assert!(payloads[0].contains(&paul.to_string()));

        // Not shown on the note until a reading says it belongs.
        assert!(occasions_for(&pool, note).await.unwrap().is_empty());
        sqlx::query!("update capture_occasions set verdict = 'belongs', model = 'test-model'")
            .execute(&pool)
            .await
            .unwrap();
        let shown = occasions_for(&pool, note).await.unwrap();
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].entities[0].name, "Paul");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn each_mac_looks_each_note_up_once(pool: PgPool) {
        an_entity(&pool, "Paul").await;
        let note = a_capture(&pool, "x", Utc::now()).await;
        let (work, home) = (Uuid::new_v4(), Uuid::new_v4());
        let ask = |checker| OfferOccasionsRequest {
            checker,
            captures: vec![CaptureMeetings {
                capture_event_id: note,
                meetings: vec![meeting("Paul", &[], "during", 0)],
            }],
        };

        assert_eq!(pending_for(&pool, work, 10).await.unwrap().len(), 1);
        assert_eq!(offer(&pool, &ask(work)).await.unwrap().kept, 1);
        assert!(pending_for(&pool, work, 10).await.unwrap().is_empty());
        // Sent twice, offered once.
        assert_eq!(offer(&pool, &ask(work)).await.unwrap().kept, 0);
        // The other Mac has its own calendar and is still asked.
        assert_eq!(pending_for(&pool, home, 10).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_episode_is_what_the_continuations_connect(pool: PgPool) {
        let now = Utc::now();
        let a = a_capture(&pool, "a", now - chrono::TimeDelta::minutes(20)).await;
        let b = a_capture(&pool, "b", now - chrono::TimeDelta::minutes(10)).await;
        let c = a_capture(&pool, "c", now).await;
        let unrelated = a_capture(&pool, "milk", now - chrono::TimeDelta::minutes(5)).await;
        continues(&pool, b, a, true).await;
        continues(&pool, c, b, true).await;
        continues(&pool, c, unrelated, false).await;

        let ids = |notes: Vec<contracts::EpisodeNote>| -> Vec<Uuid> {
            notes.iter().map(|n| n.capture_event_id).collect()
        };
        assert_eq!(
            ids(episode(&pool, c).await.unwrap()),
            vec![a, b],
            "oldest first, the note itself left out"
        );
        assert_eq!(
            ids(episode(&pool, a).await.unwrap()),
            vec![b, c],
            "walked both ways"
        );
        assert!(episode(&pool, unrelated).await.unwrap().is_empty());

        let mut near = neighbours(&pool, &[b]).await.unwrap();
        near.sort();
        let mut expected = vec![a, c];
        expected.sort();
        assert_eq!(near, expected);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_redacted_note_leaves_no_context_behind(pool: PgPool) {
        an_entity(&pool, "Paul").await;
        let now = Utc::now();
        let before = a_capture(&pool, "before", now - chrono::TimeDelta::minutes(3)).await;
        let note = a_capture(&pool, "note", now).await;
        continues(&pool, note, before, true).await;
        offer(
            &pool,
            &OfferOccasionsRequest {
                checker: Uuid::new_v4(),
                captures: vec![CaptureMeetings {
                    capture_event_id: note,
                    meetings: vec![meeting("Paul", &[], "during", 0)],
                }],
            },
        )
        .await
        .unwrap();

        let mut tx = pool.begin().await.unwrap();
        forget(&mut tx, note).await.unwrap();
        tx.commit().await.unwrap();

        assert!(episode(&pool, before).await.unwrap().is_empty());
        let left = sqlx::query_scalar!("select count(*) as \"n!\" from capture_occasions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }
}
