//! Turns an `ExtractionResult` into projection rows (`entities`,
//! `observations`, `relations`), each backed by its own event on the
//! entity's/relation's stream — so every derived fact stays traceable to
//! the model call and source capture that produced it, per ADR 0003.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::events;
use crate::openrouter::{ExtractionResult, OpenRouterClient, SpokenAt};

/// When an observation is about, once the model's ISO string has been
/// pinned down. `at` is only set when a time of day was actually named —
/// a date with a fabricated midnight would read as a fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Happened {
    pub on: NaiveDate,
    pub at: Option<DateTime<Utc>>,
    pub precision: Precision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    Time,
    Day,
    Week,
    Month,
    Year,
}

impl Precision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Time => "time",
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "time" => Some(Self::Time),
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
}

/// Pins the model's `when` string to a real date.
///
/// Deliberately forgiving about the shape (`2026-09`, `2026`, a trailing
/// `Z`, a space instead of `T`) and deliberately strict about precision:
/// a value the model made up is replaced by what the string actually
/// supports, never the other way round. A date can always be stored; a
/// time can only be claimed when one was written.
pub fn resolve_when(when: &str, precision: Option<&str>, timezone: Tz) -> Option<Happened> {
    let raw = when.trim();
    if raw.is_empty() {
        return None;
    }

    let (date_part, time_part) = match raw.split_once(['T', ' ']) {
        Some((date, time)) => (date, Some(time)),
        None => (raw, None),
    };

    let stated = precision.and_then(Precision::parse);

    let (on, implied) = match date_part.split('-').collect::<Vec<_>>()[..] {
        [year, month, day] => (
            NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)?,
            Precision::Day,
        ),
        [year, month] => (
            NaiveDate::from_ymd_opt(year.parse().ok()?, month.parse().ok()?, 1)?,
            Precision::Month,
        ),
        [year] => (
            NaiveDate::from_ymd_opt(year.parse().ok()?, 1, 1)?,
            Precision::Year,
        ),
        _ => return None,
    };

    let at = time_part
        .and_then(|time| {
            let time = time.trim_end_matches('Z');
            let time = time.split(['+', '.']).next().unwrap_or(time);
            let mut parts = time.split(':');
            let hour: u32 = parts.next()?.parse().ok()?;
            let minute: u32 = parts.next().unwrap_or("0").parse().ok()?;
            on.and_hms_opt(hour, minute, 0)
        })
        .and_then(|naive| timezone.from_local_datetime(&naive).single())
        .map(|local| local.with_timezone(&Utc));

    // The stated precision is honoured only where the string backs it up.
    let precision = match stated {
        Some(Precision::Time) if at.is_some() => Precision::Time,
        Some(Precision::Time) => Precision::Day,
        // A coarser claim than the string is fine — "2026-09-22" with
        // precision "week" means the speaker said "next week" and the
        // model picked a day inside it.
        Some(other) => other,
        None if at.is_some() => Precision::Time,
        None => implied,
    };

    Some(Happened { on, at, precision })
}

/// Runs structuring for a capture in the background: never blocks or fails
/// the ingest request. A failure here just means this capture stays
/// un-structured until the next attempt — acceptable at personal-note
/// volume, and always re-derivable from the untouched raw capture event.
pub async fn structure_capture_in_background(
    pool: PgPool,
    client: Option<std::sync::Arc<OpenRouterClient>>,
    source_event_id: Uuid,
    transcript: String,
    spoken_at: SpokenAt,
) {
    let Some(client) = client else {
        tracing::debug!("OPENROUTER_API_KEY not set, skipping structuring");
        return;
    };

    if let Err(err) =
        structure_capture(&pool, &client, source_event_id, &transcript, spoken_at).await
    {
        tracing::error!(?err, %source_event_id, "structuring failed for capture");
    }
}

async fn structure_capture(
    pool: &PgPool,
    client: &OpenRouterClient,
    source_event_id: Uuid,
    transcript: &str,
    spoken_at: SpokenAt,
) -> anyhow::Result<()> {
    let extraction = client.extract(transcript, spoken_at).await?;
    apply_extraction(
        pool,
        source_event_id,
        &extraction,
        client.model_name(),
        spoken_at.timezone,
    )
    .await
}

async fn apply_extraction(
    pool: &PgPool,
    source_event_id: Uuid,
    extraction: &ExtractionResult,
    model: &str,
    timezone: Tz,
) -> anyhow::Result<()> {
    let mut entity_ids = std::collections::HashMap::new();

    for entity in &extraction.entities {
        let happened = entity
            .when
            .as_deref()
            .and_then(|when| resolve_when(when, entity.when_precision.as_deref(), timezone));

        let id = resolve_or_create_entity(
            pool,
            &entity.entity_type,
            &entity.name,
            source_event_id,
            &entity.observation,
            model,
            happened,
        )
        .await?;
        entity_ids.insert(entity.name.clone(), id);
    }

    for relation in &extraction.relations {
        let (Some(&from_id), Some(&to_id)) =
            (entity_ids.get(&relation.from), entity_ids.get(&relation.to))
        else {
            tracing::warn!(
                from = %relation.from,
                to = %relation.to,
                "skipping relation referencing an entity not in this extraction's own list"
            );
            continue;
        };

        record_relation(
            pool,
            from_id,
            to_id,
            &relation.relation_type,
            source_event_id,
            model,
        )
        .await?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn resolve_or_create_entity(
    pool: &PgPool,
    entity_type: &str,
    name: &str,
    source_event_id: Uuid,
    observation: &str,
    model: &str,
    happened: Option<Happened>,
) -> anyhow::Result<Uuid> {
    let existing = sqlx::query!(
        r#"select id from entities where entity_type = $1 and lower(name) = lower($2)"#,
        entity_type,
        name,
    )
    .fetch_optional(pool)
    .await?;

    let entity_id = match existing {
        Some(row) => row.id,
        None => {
            let id = Uuid::new_v4();

            sqlx::query!(
                r#"insert into entity_type_registry (entity_type) values ($1) on conflict do nothing"#,
                entity_type,
            )
            .execute(pool)
            .await?;

            events::append(
                pool,
                id,
                1,
                "entity.created",
                &json!({ "entity_type": entity_type, "name": name }),
                model,
            )
            .await?;

            sqlx::query!(
                r#"insert into entities (id, entity_type, name) values ($1, $2, $3)"#,
                id,
                entity_type,
                name,
            )
            .execute(pool)
            .await?;

            id
        }
    };

    let next_version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        entity_id,
    )
    .fetch_one(pool)
    .await?;

    events::append(
        pool,
        entity_id,
        next_version,
        "entity.observed",
        &json!({
            "text": observation,
            "source_event_id": source_event_id,
            "happened_on": happened.map(|h| h.on.to_string()),
            "happened_at": happened.and_then(|h| h.at).map(|at| at.to_rfc3339()),
            "happened_precision": happened.map(|h| h.precision.as_str()),
        }),
        model,
    )
    .await?;

    sqlx::query!(
        r#"
        insert into observations
            (entity_id, source_event_id, text, model, happened_on, happened_at, happened_precision)
        values ($1, $2, $3, $4, $5, $6, $7)
        "#,
        entity_id,
        source_event_id,
        observation,
        model,
        happened.map(|h| h.on),
        happened.and_then(|h| h.at),
        happened.map(|h| h.precision.as_str()),
    )
    .execute(pool)
    .await?;

    sqlx::query!(
        r#"update entities set current_summary = $1, updated_at = now() where id = $2"#,
        observation,
        entity_id,
    )
    .execute(pool)
    .await?;

    Ok(entity_id)
}

async fn record_relation(
    pool: &PgPool,
    from_id: Uuid,
    to_id: Uuid,
    relation_type: &str,
    source_event_id: Uuid,
    model: &str,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4();

    events::append(
        pool,
        id,
        1,
        "relation.proposed",
        &json!({
            "from_entity_id": from_id,
            "to_entity_id": to_id,
            "relation_type": relation_type,
            // Also on the payload, not only on the projection row: the
            // capture detail view reads the log by this key, and a
            // relation that cannot be traced back to the sentence that
            // proposed it is not reviewable.
            "source_event_id": source_event_id,
        }),
        model,
    )
    .await?;

    sqlx::query!(
        r#"
        insert into relations (id, from_entity_id, to_entity_id, relation_type, source_event_id, model)
        values ($1, $2, $3, $4, $5, $6)
        "#,
        id,
        from_id,
        to_id,
        relation_type,
        source_event_id,
        model,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BERLIN: Tz = chrono_tz::Europe::Berlin;

    #[test]
    fn a_plain_date_is_a_day() {
        let happened = resolve_when("2026-09-22", Some("day"), BERLIN).unwrap();
        assert_eq!(happened.on, NaiveDate::from_ymd_opt(2026, 9, 22).unwrap());
        assert_eq!(happened.at, None);
        assert_eq!(happened.precision, Precision::Day);
    }

    #[test]
    fn a_named_time_is_stored_in_utc() {
        let happened = resolve_when("2026-09-22T19:30", Some("time"), BERLIN).unwrap();
        assert_eq!(happened.precision, Precision::Time);
        // 19:30 in Berlin during summer time is 17:30 UTC.
        assert_eq!(
            happened.at.unwrap().to_rfc3339(),
            "2026-09-22T17:30:00+00:00"
        );
    }

    #[test]
    fn a_claimed_time_without_one_in_the_string_falls_back_to_day() {
        // The model occasionally says "time" while writing only a date.
        // Believing it would put a fabricated midnight into the record.
        let happened = resolve_when("2026-09-22", Some("time"), BERLIN).unwrap();
        assert_eq!(happened.precision, Precision::Day);
        assert_eq!(happened.at, None);
    }

    #[test]
    fn a_coarser_claim_than_the_string_is_kept() {
        // "nächste Woche" resolved to a day inside it: the day is the best
        // anchor available, but the precision is the honest part.
        let happened = resolve_when("2026-09-22", Some("week"), BERLIN).unwrap();
        assert_eq!(happened.precision, Precision::Week);
        assert_eq!(happened.on, NaiveDate::from_ymd_opt(2026, 9, 22).unwrap());
    }

    #[test]
    fn a_month_or_a_year_resolves_to_its_first_day() {
        let month = resolve_when("2026-09", None, BERLIN).unwrap();
        assert_eq!(month.on, NaiveDate::from_ymd_opt(2026, 9, 1).unwrap());
        assert_eq!(month.precision, Precision::Month);

        let year = resolve_when("2027", None, BERLIN).unwrap();
        assert_eq!(year.on, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        assert_eq!(year.precision, Precision::Year);
    }

    #[test]
    fn tolerates_the_shapes_models_actually_emit() {
        for raw in [
            "2026-09-22T19:30:00Z",
            "2026-09-22 19:30",
            "2026-09-22T19:30:00+02:00",
            "2026-09-22T19:30:00.000Z",
        ] {
            let happened = resolve_when(raw, None, BERLIN).unwrap_or_else(|| panic!("{raw}"));
            assert_eq!(happened.on, NaiveDate::from_ymd_opt(2026, 9, 22).unwrap());
            assert!(happened.at.is_some(), "{raw}");
        }
    }

    #[test]
    fn nonsense_resolves_to_nothing_rather_than_to_today() {
        assert!(resolve_when("next tuesday", None, BERLIN).is_none());
        assert!(resolve_when("", Some("day"), BERLIN).is_none());
        assert!(resolve_when("2026-13-40", None, BERLIN).is_none());
    }
}
