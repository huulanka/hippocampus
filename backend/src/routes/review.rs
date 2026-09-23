//! The week, looked back on.
//!
//! Today answers "what is in front of me"; this answers "what was that
//! week". It is the second surface that comes to you rather than waiting
//! to be searched (the client raises a notification at a time you chose),
//! and the only one bounded by a calendar: Monday to Sunday, in your own
//! timezone, so weeks line up with each other and can be paged through.
//!
//! Everything but the paragraph is counted on every read. The paragraph
//! is written by a model once, stored as `review.written`, and every
//! sentence in it has to name the notes it came from — one it cannot
//! source is dropped, not shown.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone, Utc};
use contracts::{
    ReviewTopic, StorySentence, UpcomingItem, WeekCount, WeekStock, WeeklyReview, WeeklyStory,
    WriteStoryRequest,
};
use serde_json::json;
use uuid::Uuid;

use sqlx::PgPool;

use crate::AppState;
use crate::error::AppError;
use crate::events;
use crate::openrouter::StoryNote;

/// How far back "before" reaches when deciding whether a subject grew.
const BEFORE_DAYS: u64 = 56;
/// A subject is growing when this week has more than this many times its
/// usual weekly share of the eight weeks before.
const GROWTH_FACTOR: i64 = 2;
/// Quiet means: talked about at least this often, and not for this many
/// days — but within `QUIET_WITHIN_DAYS`, past which it is not quiet, it
/// is history.
const QUIET_MIN_CAPTURES: i64 = 3;
const QUIET_AFTER_DAYS: u64 = 14;
const QUIET_WITHIN_DAYS: u64 = 84;
/// How far back an announcement can lie and still count as an open end.
const OPEN_ENDS_DAYS: u64 = 28;
const MAX_TOPICS: usize = 8;
const MAX_QUIET: usize = 6;
const MAX_OPEN_ENDS: i64 = 10;
/// How many of the week's notes the model is shown, and how much of each.
/// A week of a few dozen notes fits whole; past that it is the newest.
const STORY_NOTES: i64 = 60;
const STORY_NOTE_CHARS: usize = 700;
const MAX_SENTENCES: usize = 5;

#[derive(serde::Deserialize)]
pub struct WeekParams {
    /// Any day in the week wanted. Defaults to today.
    pub week: Option<NaiveDate>,
    pub timezone: Option<String>,
}

/// A week's edges, local and as instants.
struct Week {
    tz: chrono_tz::Tz,
    start: NaiveDate,
    end: NaiveDate,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
    today: NaiveDate,
}

impl Week {
    fn of(tz: chrono_tz::Tz, day: Option<NaiveDate>) -> Self {
        Self::seen_from(tz, day, Utc::now().with_timezone(&tz).date_naive())
    }

    /// The same, with "today" given rather than read off the clock.
    fn seen_from(tz: chrono_tz::Tz, day: Option<NaiveDate>, today: NaiveDate) -> Self {
        let day = day.unwrap_or(today);
        let start = day - Days::new(u64::from(day.weekday().num_days_from_monday()));
        let end = start + Days::new(6);
        Self {
            tz,
            start,
            end,
            from: midnight(tz, start),
            until: midnight(tz, end + Days::new(1)),
            today,
        }
    }
}

/// The first instant of a local day. `earliest` because a day can begin
/// inside a DST gap; the fallback is for a zone that skips midnight
/// entirely, which a few have.
fn midnight(tz: chrono_tz::Tz, day: NaiveDate) -> DateTime<Utc> {
    let naive = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    tz.from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| tz.from_utc_datetime(&naive))
        .with_timezone(&Utc)
}

pub async fn week(
    State(state): State<AppState>,
    Query(params): Query<WeekParams>,
) -> Result<Json<WeeklyReview>, AppError> {
    let tz = crate::routes::captures::timezone_for(&state, params.timezone.as_deref());
    let week = Week::of(tz, params.week);
    Ok(Json(
        review(&state.pool, &week, state.openrouter.is_some()).await?,
    ))
}

/// Everything the week has to show, counted fresh.
async fn review(pool: &PgPool, week: &Week, can_write: bool) -> Result<WeeklyReview, AppError> {
    let tz = week.tz;
    let zone = tz.name();

    let stock = sqlx::query!(
        r#"
        select
            count(*) filter (where cs.occurred_at >= $1) as "captures!",
            count(*) filter (where cs.occurred_at >= $1 and cc.origin = 'audio') as "spoken!",
            count(*) filter (where cs.occurred_at >= $1 and cc.origin = 'text') as "typed!",
            count(*) as "total!"
        from capture_search cs
        left join capture_content cc on cc.event_id = cs.event_id
        where cs.occurred_at < $2
        "#,
        week.from,
        week.until,
    )
    .fetch_one(pool)
    .await?;

    let by_day = sqlx::query!(
        r#"
        select extract(isodow from cs.occurred_at at time zone $3)::int as "weekday!", count(*) as "n!"
        from capture_search cs
        where cs.occurred_at >= $1 and cs.occurred_at < $2
        group by 1
        "#,
        week.from,
        week.until,
        zone,
    )
    .fetch_all(pool)
    .await?;
    let mut days = vec![0; 7];
    for row in by_day {
        if let Some(slot) = usize::try_from(row.weekday - 1)
            .ok()
            .and_then(|i| days.get_mut(i))
        {
            *slot = row.n;
        }
    }

    let first_week = week.start - Days::new(49);
    let by_week = sqlx::query!(
        r#"
        select date_trunc('week', cs.occurred_at at time zone $3)::date as "week_start!", count(*) as "n!"
        from capture_search cs
        where cs.occurred_at >= $1 and cs.occurred_at < $2
        group by 1
        "#,
        midnight(tz, first_week),
        week.until,
        zone,
    )
    .fetch_all(pool)
    .await?;
    let counted: HashMap<NaiveDate, i64> = by_week
        .into_iter()
        .map(|row| (row.week_start, row.n))
        .collect();
    let recent_weeks = (0..8)
        .map(|i| {
            let start = first_week + Days::new(7 * i);
            WeekCount {
                week_start: start,
                captures: counted.get(&start).copied().unwrap_or(0),
            }
        })
        .collect();

    let (growing, new_topics, quiet) = topics(pool, week).await?;
    let open_ends = open_ends(pool, week).await?;
    let next_week = next_week(pool, week).await?;
    let story = stored_story(pool, week.start).await?;

    Ok(WeeklyReview {
        week_start: week.start,
        week_end: week.end,
        timezone: zone.to_string(),
        complete: week.today > week.end,
        stock: WeekStock {
            captures: stock.captures,
            spoken: stock.spoken,
            typed: stock.typed,
            total: stock.total,
            days,
            recent_weeks,
        },
        growing,
        new_topics,
        open_ends,
        next_week,
        quiet,
        story,
        can_write,
    })
}

/// Growing, new and quiet, from one pass over every subject.
///
/// Counted in *captures*, not observations, for the same reason as the
/// threads on Today: one sentence can yield three observations about the
/// same thing, and that is not three times the interest.
///
/// And dated by when the capture was *said*, not when its observations
/// were written: a capture whose structuring was retried days later would
/// otherwise land in the wrong week.
async fn topics(
    pool: &PgPool,
    week: &Week,
) -> Result<(Vec<ReviewTopic>, Vec<ReviewTopic>, Vec<ReviewTopic>), AppError> {
    let rows = sqlx::query!(
        r#"
        select
            en.id,
            en.name,
            en.entity_type,
            count(distinct o.source_event_id) filter (where cs.occurred_at >= $1) as "this_week!",
            count(distinct o.source_event_id)
                filter (where cs.occurred_at < $1 and cs.occurred_at >= $3) as "before!",
            count(distinct o.source_event_id) filter (where cs.occurred_at < $1) as "ever_before!",
            max(cs.occurred_at) as "last_seen!"
        from entities en
        join observations o on o.entity_id = en.id
        join capture_search cs on cs.event_id = o.source_event_id
        where en.merged_into is null and cs.occurred_at < $2
        group by en.id, en.name, en.entity_type
        "#,
        week.from,
        week.until,
        week.from - chrono::Duration::days(BEFORE_DAYS as i64),
    )
    .fetch_all(pool)
    .await?;

    let quiet_after = week.from - chrono::Duration::days(QUIET_AFTER_DAYS as i64);
    let quiet_within = week.from - chrono::Duration::days(QUIET_WITHIN_DAYS as i64);

    let mut growing = Vec::new();
    let mut new_topics = Vec::new();
    let mut quiet = Vec::new();
    for row in rows {
        let topic = ReviewTopic {
            entity_id: row.id,
            name: row.name,
            entity_type: row.entity_type,
            this_week: row.this_week,
            before: row.before,
            last_seen: row.last_seen,
        };
        if row.this_week > 0 && row.ever_before == 0 {
            new_topics.push(topic);
        } else if row.this_week >= 2 && is_growing(row.this_week, row.before) {
            growing.push(topic);
        } else if row.this_week == 0
            && row.ever_before >= QUIET_MIN_CAPTURES
            && row.last_seen < quiet_after
            && row.last_seen >= quiet_within
        {
            quiet.push(topic);
        }
    }

    let busiest = |a: &ReviewTopic, b: &ReviewTopic| {
        b.this_week
            .cmp(&a.this_week)
            .then(b.last_seen.cmp(&a.last_seen))
    };
    growing.sort_by(busiest);
    new_topics.sort_by(busiest);
    quiet.sort_by(|a, b| b.before.cmp(&a.before).then(b.last_seen.cmp(&a.last_seen)));
    growing.truncate(MAX_TOPICS);
    new_topics.truncate(MAX_TOPICS);
    quiet.truncate(MAX_QUIET);
    Ok((growing, new_topics, quiet))
}

/// More than `GROWTH_FACTOR` times the usual weekly share of the eight
/// weeks before. Integer arithmetic: `this_week > factor * before / 8`.
fn is_growing(this_week: i64, before: i64) -> bool {
    let weeks = (BEFORE_DAYS / 7) as i64;
    this_week * weeks > GROWTH_FACTOR * before
}

/// What you announced, whose day has come and gone without another word.
///
/// Only *announcements*: the note has to have been said before the day it
/// is about. "Gestern war der Aufguss zu heiß" is dated yesterday too, but
/// it closed itself the moment it was said.
async fn open_ends(pool: &PgPool, week: &Week) -> Result<Vec<UpcomingItem>, AppError> {
    let last_day = week.end.min(week.today - Days::new(1));
    let rows = sqlx::query!(
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
            cs.occurred_at as said_at
        from observations o
        join entities en on en.id = o.entity_id
        join capture_search cs on cs.event_id = o.source_event_id
        where en.merged_into is null
          and o.happened_on between $1 and $2
          and (cs.occurred_at at time zone $3)::date < o.happened_on
          and not exists (
              select 1
              from observations later
              join capture_search lcs on lcs.event_id = later.source_event_id
              where later.entity_id = o.entity_id
                and later.source_event_id <> o.source_event_id
                and (lcs.occurred_at at time zone $3)::date >= o.happened_on
          )
        order by o.happened_on desc, o.happened_at desc nulls last
        limit $4
        "#,
        week.start - Days::new(OPEN_ENDS_DAYS),
        last_day,
        week.tz.name(),
        MAX_OPEN_ENDS,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UpcomingItem {
            entity_id: r.entity_id,
            entity_name: r.entity_name,
            entity_type: r.entity_type,
            observation: r.observation,
            happened_on: r.happened_on,
            happened_at: r.happened_at,
            happened_precision: r.happened_precision,
            capture_event_id: r.source_event_id,
            said_at: r.said_at,
        })
        .collect())
}

async fn next_week(pool: &PgPool, week: &Week) -> Result<Vec<UpcomingItem>, AppError> {
    let rows = sqlx::query!(
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
            cs.occurred_at as said_at
        from observations o
        join entities en on en.id = o.entity_id
        join capture_search cs on cs.event_id = o.source_event_id
        where en.merged_into is null
          and o.happened_on between $1 and $2
          -- Said by the end of this week. "Heute Mittag Leberkäse", said
          -- on the Monday it is about, is dated next week too — but it
          -- was never something this week knew was coming.
          and cs.occurred_at < $3
        order by o.happened_on, o.happened_at nulls last
        "#,
        week.end + Days::new(1),
        week.end + Days::new(7),
        week.until,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| UpcomingItem {
            entity_id: r.entity_id,
            entity_name: r.entity_name,
            entity_type: r.entity_type,
            observation: r.observation,
            happened_on: r.happened_on,
            happened_at: r.happened_at,
            happened_precision: r.happened_precision,
            capture_event_id: r.source_event_id,
            said_at: r.said_at,
        })
        .collect())
}

async fn stored_story(
    pool: &PgPool,
    week_start: NaiveDate,
) -> Result<Option<WeeklyStory>, AppError> {
    let row = sqlx::query!(
        r#"
        select sentences, model, captures_seen, written_at
        from weekly_story
        where week_start = $1
        "#,
        week_start,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| WeeklyStory {
        sentences: serde_json::from_value(r.sentences).unwrap_or_default(),
        model: r.model,
        written_at: r.written_at,
        captures_seen: i64::from(r.captures_seen),
    }))
}

/// Writes the week's paragraph, or writes it again.
///
/// Asked for by the client, not run on a timer: the client knows when you
/// look at the week, and a paragraph for a week nobody opened is a model
/// call nobody read. Writing again replaces the paragraph shown; the old
/// one stays in the event log.
pub async fn write_story(
    State(state): State<AppState>,
    Json(req): Json<WriteStoryRequest>,
) -> Result<Json<WeeklyStory>, AppError> {
    let Some(model) = state.openrouter.clone() else {
        return Err(AppError::bad_request(
            "no model is configured (OPENROUTER_API_KEY), so the week cannot be written up",
        ));
    };
    let tz = crate::routes::captures::timezone_for(&state, req.timezone.as_deref());
    let week = Week::of(tz, Some(req.week));

    let rows = sqlx::query!(
        r#"
        select cs.event_id, cs.occurred_at, cs.transcript
        from capture_search cs
        where cs.occurred_at >= $1 and cs.occurred_at < $2
        order by cs.occurred_at desc
        limit $3
        "#,
        week.from,
        week.until,
        STORY_NOTES,
    )
    .fetch_all(&state.pool)
    .await?;

    if rows.is_empty() {
        return Err(AppError::bad_request(
            "nothing was captured that week, so there is nothing to write about",
        ));
    }

    // Oldest first for the model — a week is told in the order it
    // happened — and tagged n1, n2, … so the answer can cite a short tag
    // rather than copy a UUID it might garble.
    let mut rows = rows;
    rows.reverse();
    let notes: Vec<StoryNote> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| StoryNote {
            tag: format!("n{}", i + 1),
            said: row
                .occurred_at
                .with_timezone(&tz)
                .format("%A %d.%m. %H:%M")
                .to_string(),
            text: row.transcript.chars().take(STORY_NOTE_CHARS).collect(),
        })
        .collect();
    let by_tag: HashMap<&str, Uuid> = notes
        .iter()
        .zip(&rows)
        .map(|(note, row)| (note.tag.as_str(), row.event_id))
        .collect();

    let written = model
        .write_week(&notes)
        .await
        .map_err(|err| AppError::from(err.context("writing the week up")))?;

    let sentences = sourced(written, &by_tag);
    if sentences.is_empty() {
        return Err(AppError::bad_request(
            "the model wrote nothing it could source to a note, so nothing was kept",
        ));
    }

    let captures_seen = rows.len() as i64;
    let payload = json!({
        "week_start": week.start,
        "timezone": tz.name(),
        "sentences": sentences,
        "model": model.model_name(),
        "captures_seen": captures_seen,
    });
    let event = events::append(
        &state.pool,
        Uuid::new_v4(),
        1,
        "review.written",
        &payload,
        model.model_name(),
    )
    .await?;

    sqlx::query!(
        r#"
        insert into weekly_story (week_start, timezone, sentences, model, captures_seen, event_id, written_at)
        values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (week_start) do update set
            timezone = excluded.timezone,
            sentences = excluded.sentences,
            model = excluded.model,
            captures_seen = excluded.captures_seen,
            event_id = excluded.event_id,
            written_at = excluded.written_at
        "#,
        week.start,
        tz.name(),
        serde_json::to_value(&sentences).map_err(anyhow::Error::from)?,
        model.model_name(),
        captures_seen as i32,
        event.id,
        event.occurred_at,
    )
    .execute(&state.pool)
    .await?;

    Ok(Json(WeeklyStory {
        sentences,
        model: model.model_name().to_string(),
        written_at: event.occurred_at,
        captures_seen,
    }))
}

/// Keeps the sentences that cite at least one real note, with their tags
/// turned back into capture ids. A tag the model invented is dropped from
/// its sentence; a sentence left with no source is dropped whole.
fn sourced(
    written: Vec<crate::openrouter::WrittenSentence>,
    by_tag: &HashMap<&str, Uuid>,
) -> Vec<StorySentence> {
    written
        .into_iter()
        .filter_map(|sentence| {
            let text = sentence.text.trim().to_string();
            let mut sources: Vec<Uuid> = Vec::new();
            for tag in &sentence.sources {
                if let Some(id) = by_tag.get(tag.trim())
                    && !sources.contains(id)
                {
                    sources.push(*id);
                }
            }
            (!text.is_empty() && !sources.is_empty()).then_some(StorySentence { text, sources })
        })
        .take(MAX_SENTENCES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrouter::WrittenSentence;

    #[test]
    fn a_week_runs_monday_to_sunday_in_the_local_zone() {
        let tz: chrono_tz::Tz = "Europe/Berlin".parse().unwrap();
        let week = Week::of(tz, NaiveDate::from_ymd_opt(2026, 9, 23));
        assert_eq!(week.start, NaiveDate::from_ymd_opt(2026, 9, 21).unwrap());
        assert_eq!(week.end, NaiveDate::from_ymd_opt(2026, 9, 27).unwrap());
        // Monday 00:00 in Berlin (summer time) is Sunday 22:00 UTC.
        assert_eq!(week.from.to_rfc3339(), "2026-09-20T22:00:00+00:00");
        // The week that contains the switch back to winter time is an
        // hour longer, and ends at 23:00 UTC.
        let autumn = Week::of(tz, NaiveDate::from_ymd_opt(2026, 10, 25));
        assert_eq!(autumn.until.to_rfc3339(), "2026-10-25T23:00:00+00:00");
    }

    #[test]
    fn growing_means_well_above_the_usual_week() {
        // Nothing before: two this week is growth.
        assert!(is_growing(2, 0));
        // Eight in eight weeks is one a week; two is only double, not more.
        assert!(!is_growing(2, 8));
        assert!(is_growing(3, 8));
    }

    #[test]
    fn an_unsourced_sentence_is_not_kept() {
        let id = Uuid::new_v4();
        let by_tag = HashMap::from([("n1", id)]);
        let kept = sourced(
            vec![
                WrittenSentence {
                    text: "Sauna twice.".into(),
                    sources: vec!["n1".into(), "n1".into(), "n9".into()],
                },
                WrittenSentence {
                    text: "A good week overall.".into(),
                    sources: vec!["n42".into()],
                },
            ],
            &by_tag,
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].sources, vec![id]);
    }
}

/// The review's SQL against a real database, on a constructed history.
/// The dev data has no announcements at all (nothing is dated after the
/// day it was said), so open ends and next week would otherwise never be
/// exercised. Run as in `graph.rs`:
///
///     cargo test -p backend --features db-tests review
#[cfg(all(test, feature = "db-tests"))]
mod db_tests {
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{Week, review};

    fn berlin() -> chrono_tz::Tz {
        "Europe/Berlin".parse().unwrap()
    }

    /// Noon on a given day in Berlin.
    fn noon(day: &str) -> DateTime<Utc> {
        let day: NaiveDate = day.parse().unwrap();
        berlin()
            .from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc)
    }

    /// A capture said at `at`, typed.
    async fn a_capture(pool: &PgPool, at: DateTime<Utc>) -> Uuid {
        let event = sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source, occurred_at)
               values (gen_random_uuid(), 1, 'capture.recorded', '{}'::jsonb, 'test', $1)
               returning id"#,
            at,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into capture_content (event_id, origin, text) values ($1, 'text', 'note')",
            event
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into capture_search (event_id, transcript, occurred_at) values ($1, 'note', $2)",
            event,
            at,
        )
        .execute(pool)
        .await
        .unwrap();
        event
    }

    async fn an_entity(pool: &PgPool, name: &str) -> Uuid {
        sqlx::query!(
            "insert into entity_type_registry (entity_type) values ('Thema') on conflict do nothing"
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query_scalar!(
            "insert into entities (entity_type, name) values ('Thema', $1) returning id",
            name
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Something said about `entity` on `said`, optionally about `about`.
    async fn said(pool: &PgPool, entity: Uuid, said: &str, about: Option<&str>) {
        let capture = a_capture(pool, noon(said)).await;
        let about: Option<NaiveDate> = about.map(|d| d.parse().unwrap());
        sqlx::query!(
            "insert into observations (entity_id, source_event_id, text, model, happened_on, happened_precision)
             values ($1, $2, 'observed', 'test-model', $3, case when $3::date is null then null else 'day' end)",
            entity,
            capture,
            about,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn names(topics: &[contracts::ReviewTopic]) -> Vec<&str> {
        topics.iter().map(|t| t.name.as_str()).collect()
    }

    /// The week of Monday 14 September 2026, looked at on Wednesday 23rd.
    fn week() -> Week {
        Week::seen_from(
            berlin(),
            Some("2026-09-16".parse().unwrap()),
            "2026-09-23".parse().unwrap(),
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn subjects_are_sorted_into_growing_new_and_quiet(pool: PgPool) {
        let growing = an_entity(&pool, "growing").await;
        said(&pool, growing, "2026-08-10", None).await;
        for day in ["2026-09-14", "2026-09-15", "2026-09-17"] {
            said(&pool, growing, day, None).await;
        }

        let fresh = an_entity(&pool, "new").await;
        said(&pool, fresh, "2026-09-18", None).await;

        let quiet = an_entity(&pool, "quiet").await;
        for day in ["2026-08-01", "2026-08-05", "2026-08-12"] {
            said(&pool, quiet, day, None).await;
        }

        // Talked about as often, but half a year ago: history, not quiet.
        let old = an_entity(&pool, "history").await;
        for day in ["2026-03-01", "2026-03-02", "2026-03-03"] {
            said(&pool, old, day, None).await;
        }

        // Steady at one a week: neither growing nor new nor quiet.
        let steady = an_entity(&pool, "steady").await;
        for day in ["2026-08-24", "2026-08-31", "2026-09-07", "2026-09-15"] {
            said(&pool, steady, day, None).await;
        }

        let review = review(&pool, &week(), false).await.unwrap();
        assert_eq!(names(&review.growing), ["growing"]);
        assert_eq!(names(&review.new_topics), ["new"]);
        assert_eq!(names(&review.quiet), ["quiet"]);
        assert_eq!(review.growing[0].this_week, 3);
        assert_eq!(review.stock.captures, 5);
        // Monday, Tuesday twice, Thursday, Friday.
        assert_eq!(review.stock.days, [1, 2, 0, 1, 1, 0, 0]);
        assert!(review.complete);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_open_end_is_an_announcement_nothing_followed(pool: PgPool) {
        // Announced on the 10th for the 16th, never mentioned again.
        let dentist = an_entity(&pool, "dentist").await;
        said(&pool, dentist, "2026-09-10", Some("2026-09-16")).await;

        // Announced, and then talked about on the day: closed.
        let workshop = an_entity(&pool, "workshop").await;
        said(&pool, workshop, "2026-09-10", Some("2026-09-17")).await;
        said(&pool, workshop, "2026-09-17", None).await;

        // Said on the day it is about: never an announcement.
        let lunch = an_entity(&pool, "lunch").await;
        said(&pool, lunch, "2026-09-16", Some("2026-09-16")).await;

        // Announced during the week for the week after.
        let trip = an_entity(&pool, "trip").await;
        said(&pool, trip, "2026-09-19", Some("2026-09-22")).await;

        // About the week after, but said in it.
        let later = an_entity(&pool, "said later").await;
        said(&pool, later, "2026-09-21", Some("2026-09-22")).await;

        let review = review(&pool, &week(), false).await.unwrap();
        let open: Vec<&str> = review
            .open_ends
            .iter()
            .map(|u| u.entity_name.as_str())
            .collect();
        let next: Vec<&str> = review
            .next_week
            .iter()
            .map(|u| u.entity_name.as_str())
            .collect();
        assert_eq!(open, ["dentist"]);
        assert_eq!(next, ["trip"]);
    }
}
