//! Turns an `ExtractionResult` into projection rows (`entities`,
//! `observations`, `relations`), each backed by its own event on the
//! entity's/relation's stream — so every derived fact stays traceable to
//! the model call and source capture that produced it, per ADR 0003.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::embedding::Embedder;
use crate::events;
use crate::openrouter::{ExtractionResult, KnownEntity, KnownGraph, OpenRouterClient, SpokenAt};

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
/// the ingest request.
///
/// A failure here no longer ends the matter. The outcome is written to
/// `capture_pipeline` either way, and [`crate::pipeline::watch`] comes
/// back for whatever did not succeed — which is the difference between a
/// capture that is temporarily unstructured and one that is permanently
/// meaningless.
pub async fn structure_capture_in_background(
    pool: PgPool,
    client: Option<std::sync::Arc<OpenRouterClient>>,
    embedder: Embedder,
    source_event_id: Uuid,
    transcript: String,
    spoken_at: SpokenAt,
    max_attempts: i32,
) {
    let Some(client) = client else {
        // Not recorded as a failure: there is no model configured, so
        // this capture is not waiting on anything that a retry would fix.
        tracing::debug!("OPENROUTER_API_KEY not set, skipping structuring");
        return;
    };

    match structure_capture(
        &pool,
        &client,
        &embedder,
        source_event_id,
        &transcript,
        spoken_at,
    )
    .await
    {
        Ok(()) => {
            crate::pipeline::record_structuring(
                &pool,
                source_event_id,
                client.model_name(),
                None,
                max_attempts,
            )
            .await;
        }
        Err(err) => {
            tracing::error!(?err, %source_event_id, "structuring failed for capture");
            crate::pipeline::record_structuring(
                &pool,
                source_event_id,
                client.model_name(),
                // `{:#}` so the chain reads "sending the request failed:
                // connection refused" rather than only the outermost
                // sentence, which is usually the least useful one.
                Some(&format!("{err:#}")),
                max_attempts,
            )
            .await;
        }
    }
}

/// How many known entities are offered to the extraction.
///
/// Capped because this is sent on every capture and paid for by the
/// token. Forty is roughly 600 tokens of names and one-line summaries —
/// cheap next to the cost of finding and merging the duplicate it
/// prevents.
const KNOWN_ENTITY_BUDGET: i64 = 40;

/// How much of an entity's name has to match somewhere in the transcript
/// before it is worth showing the model.
///
/// Low on purpose: this decides what the model gets to *consider*, not
/// what it resolves to. A missed candidate is a duplicate that has to be
/// merged later; a spurious one costs a few tokens and is ignored.
const KNOWN_ENTITY_MIN_SIMILARITY: f64 = 0.5;

/// How many recently touched entities are offered regardless of whether
/// they resemble the transcript.
///
/// Recency is a strong prior in a personal note system: notes come in
/// bursts about the same subject, and the second note of a burst often
/// names the thing differently from the first ("Hippocampus" after
/// "Hippocampus Projekt"). Trigram matching alone would miss exactly
/// those, because the wording that needs resolving is the wording that
/// does not match.
const RECENT_ENTITY_BUDGET: i64 = 12;

/// What the graph already holds that could bear on this transcript.
///
/// Two sources, because they catch different failures — see the constants
/// above. Failing to build this is not fatal: extraction then behaves as
/// it did before, producing a duplicate that the consolidation run picks
/// up later. A capture is never lost over it.
async fn known_graph(pool: &PgPool, transcript: &str) -> KnownGraph {
    let types = sqlx::query_scalar!(
        r#"
        select r.entity_type as "entity_type!"
        from entity_type_registry r
        left join entities e on e.entity_type = r.entity_type
        group by r.entity_type
        order by count(e.id) desc, r.entity_type
        "#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(?err, "could not read the entity type vocabulary");
        Vec::new()
    });

    let entities = sqlx::query!(
        r#"
        -- Column overrides because sqlx cannot see through a UNION to
        -- the NOT NULL on the underlying columns.
        (
            select e.id, e.name as "name!", e.entity_type as "entity_type!", e.current_summary
            from entities e
            where word_similarity(e.name, $1) > $2
            order by word_similarity(e.name, $1) desc
            limit $3
        )
        union
        (
            select e.id, e.name as "name!", e.entity_type as "entity_type!", e.current_summary
            from entities e
            order by e.updated_at desc
            limit $4
        )
        "#,
        transcript,
        KNOWN_ENTITY_MIN_SIMILARITY as f32,
        KNOWN_ENTITY_BUDGET,
        RECENT_ENTITY_BUDGET,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(
            ?err,
            "could not look for entities this capture may be about"
        );
        Vec::new()
    });

    KnownGraph {
        types,
        entities: entities
            .into_iter()
            .map(|row| KnownEntity {
                name: row.name,
                entity_type: row.entity_type,
                summary: row.current_summary,
            })
            .collect(),
    }
}

async fn structure_capture(
    pool: &PgPool,
    client: &OpenRouterClient,
    embedder: &Embedder,
    source_event_id: Uuid,
    transcript: &str,
    spoken_at: SpokenAt,
) -> anyhow::Result<()> {
    let known = known_graph(pool, transcript).await;
    tracing::debug!(
        types = known.types.len(),
        candidates = known.entities.len(),
        "extraction knows this much of the existing graph"
    );
    let extraction = client.extract(transcript, spoken_at, &known).await?;
    apply_extraction(
        pool,
        embedder,
        source_event_id,
        transcript,
        &extraction,
        client.model_name(),
        spoken_at.timezone,
    )
    .await
}

async fn apply_extraction(
    pool: &PgPool,
    embedder: &Embedder,
    source_event_id: Uuid,
    transcript: &str,
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
            embedder,
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

    record_intentions(
        pool,
        source_event_id,
        transcript,
        &extraction.intentions,
        &entity_ids,
        model,
    )
    .await?;

    Ok(())
}

/// Writes down what the speaker said they still mean to do.
///
/// Once per capture. A capture is structured again only after a failure,
/// and a failure partway through can leave its intentions already
/// written; noting them a second time would put every one of them in
/// front of the user twice. So a capture that already has intentions is
/// left alone.
///
/// The words go into `intentions`, the event carries ids only — the log
/// is never modified, so nothing written into it could be redacted later
/// (see migration 0012).
pub(crate) async fn record_intentions(
    pool: &PgPool,
    source_event_id: Uuid,
    transcript: &str,
    intentions: &[crate::openrouter::ExtractedIntention],
    entity_ids: &std::collections::HashMap<String, Uuid>,
    model: &str,
) -> anyhow::Result<()> {
    if intentions.is_empty() {
        return Ok(());
    }
    let already = sqlx::query_scalar!(
        r#"select exists (select 1 from intentions where source_event_id = $1) as "exists!""#,
        source_event_id,
    )
    .fetch_one(pool)
    .await?;
    if already {
        tracing::info!(%source_event_id, "intentions already noted for this capture; not noting them twice");
        return Ok(());
    }

    for intention in intentions {
        let id = Uuid::new_v4();
        // Only names from this extraction's own list, the same rule
        // relations follow: an `about` the model did not also extract as
        // an entity is a name with nothing behind it.
        let mut about: Vec<Uuid> = intention
            .about
            .iter()
            .filter_map(|name| entity_ids.get(name).copied())
            .collect();
        about.sort();
        about.dedup();
        let quote = intention
            .quote
            .as_deref()
            .and_then(|quote| verbatim(quote, transcript));

        // One step: an intention on screen whose event never landed
        // could not be dismissed, because dismissing appends to its
        // stream.
        let mut tx = pool.begin().await?;
        let noted = events::append_tx(
            &mut tx,
            id,
            1,
            "intention.noted",
            &json!({
                "intention_id": id,
                "source_event_id": source_event_id,
                "entity_ids": about,
            }),
            model,
        )
        .await?;
        sqlx::query!(
            r#"
            insert into intentions (id, source_event_id, text, quote, model, noted_event_id)
            values ($1, $2, $3, $4, $5, $6)
            "#,
            id,
            source_event_id,
            intention.text.trim(),
            quote,
            model,
            noted.id,
        )
        .execute(&mut *tx)
        .await?;
        for entity_id in &about {
            sqlx::query!(
                r#"insert into intention_entities (intention_id, entity_id) values ($1, $2)"#,
                id,
                entity_id,
            )
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        tracing::info!(%source_event_id, about = about.len(), quoted = quote.is_some(), "intention noted");
    }

    Ok(())
}

/// The passage of `transcript` that `quote` claims to be, if it really is
/// one — returned as it stands in the transcript, not as the model wrote
/// it.
///
/// Forgiving about what a model changes without meaning to (case, runs of
/// whitespace, the punctuation at either end, typographic quote marks) and
/// about nothing else. A quote that is not in the transcript is a
/// paraphrase, and a paraphrase shown between quote marks is the one
/// thing this system must never do: it would put words in the speaker's
/// mouth that they can check and find they never said.
pub fn verbatim(quote: &str, transcript: &str) -> Option<String> {
    let trim = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\''
                    | '„'
                    | '“'
                    | '”'
                    | '‚'
                    | '‘'
                    | '’'
                    | '«'
                    | '»'
                    | '.'
                    | ','
                    | '!'
                    | '?'
                    | ';'
                    | ':'
                    | '…'
            )
    };
    let needle: Vec<char> = fold(quote.trim_matches(trim)).0;
    if needle.len() < 3 {
        return None;
    }
    let (hay, positions) = fold(transcript);

    let start = hay
        .windows(needle.len())
        .position(|window| window == needle.as_slice())?;
    let end = start + needle.len() - 1;
    let from = positions[start];
    let to = positions[end] + transcript[positions[end]..].chars().next()?.len_utf8();
    Some(transcript[from..to].to_string())
}

/// Lowercases and collapses whitespace, remembering for every folded
/// character the byte offset it came from — so a match found in the folded
/// text can be cut out of the original.
fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut positions = Vec::with_capacity(text.len());
    let mut in_space = false;
    for (offset, c) in text.char_indices() {
        if c.is_whitespace() {
            if !in_space && !chars.is_empty() {
                chars.push(' ');
                positions.push(offset);
            }
            in_space = true;
            continue;
        }
        in_space = false;
        for lower in c.to_lowercase() {
            chars.push(lower);
            positions.push(offset);
        }
    }
    (chars, positions)
}

/// The vector for an entity, or `None` when the model could not be asked.
///
/// A failure is not propagated: the entity is worth having without its
/// embedding, and the alternative would be losing an observation because
/// a local model hiccuped. A row with a null embedding is also exactly
/// what a later backfill looks for.
async fn embed_entity(
    embedder: &Embedder,
    entity_type: &str,
    name: &str,
) -> Option<pgvector::Vector> {
    match embedder
        .embed_passage(&format!("{name} ({entity_type})"))
        .await
    {
        Ok(vector) => Some(vector.into()),
        Err(err) => {
            tracing::warn!(?err, %name, "could not embed this entity; leaving it unembedded");
            None
        }
    }
}

/// How alike two names have to be before they are taken to be the same
/// thing without anyone reading them.
///
/// High, and deliberately so. This runs unsupervised on every capture,
/// and the measured data says similarity alone cannot tell a duplicate
/// from a relation: `Kardamom-Espresso ↔ Espresso` scores 0.50 and must
/// never merge, while `Aufguss ↔ Finnischer Aufguss` scores 0.42 and
/// must. There is no threshold between them, so this one sits far above
/// both and only catches what is nearly a spelling difference —
/// "Hippocampus Projekt" against "Hippocampus-Projekt". Everything in the
/// ambiguous middle is left to the consolidation run, which reads both
/// entities before deciding (docs/entity-resolution.md).
const SAME_NAME_SIMILARITY: f64 = 0.9;

/// Finds the entity an extracted name refers to, if the graph already
/// holds it.
///
/// Three attempts, widening:
///
/// 1. Exact name and exact type — what this used to do, and nothing else.
/// 2. **Exact name, any type.** This is the one that matters most. The
///    type vocabulary was invented per note, so the graph held `Sauna` as
///    both `Ort` and `Aktivität`, `Northwind` as both `Organisation`
///    and `Kunde`, `Kardamom-Espresso` as both `Idee` and `Getränk` —
///    four of the six duplicate pairs in the real data, all of them the
///    same thing under two type names.
/// 3. Near-identical name, same type, above [`SAME_NAME_SIMILARITY`].
///
/// The type of the surviving entity is left alone. Re-typing is a
/// decision about the whole vocabulary, not about one capture, and it
/// belongs to the consolidation run.
///
/// The risk this accepts is step 2: two genuinely different things with
/// the same name ("Golf" the sport, "Golf" the car) resolve to one. For a
/// single speaker's own memory that is rare, and the alternative —
/// guaranteed fragmentation of everything whose type wording drifts — is
/// both certain and worse.
async fn resolve_existing(
    pool: &PgPool,
    entity_type: &str,
    name: &str,
) -> anyhow::Result<Option<Uuid>> {
    let exact = sqlx::query_scalar!(
        r#"select id from entities where entity_type = $1 and lower(name) = lower($2)"#,
        entity_type,
        name,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(id) = exact {
        return Ok(Some(id));
    }

    // Any type. Ordered by how many observations already hang off the
    // entity, so a name that exists twice resolves onto the one that is
    // actually being used rather than whichever row came first.
    let by_name = sqlx::query!(
        r#"
        select e.id, e.entity_type
        from entities e
        left join observations o on o.entity_id = e.id
        where lower(e.name) = lower($1)
        group by e.id, e.entity_type
        order by count(o.id) desc
        limit 1
        "#,
        name,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(row) = by_name {
        tracing::info!(
            %name,
            extracted_type = %entity_type,
            existing_type = %row.entity_type,
            "same name under a different type; resolved onto the existing entity"
        );
        return Ok(Some(row.id));
    }

    let near = sqlx::query!(
        r#"
        select e.id, e.name
        from entities e
        where e.entity_type = $1 and similarity(e.name, $2) >= $3
        order by similarity(e.name, $2) desc
        limit 1
        "#,
        entity_type,
        name,
        SAME_NAME_SIMILARITY as f32,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(row) = near {
        tracing::info!(
            extracted = %name,
            existing = %row.name,
            "near-identical name; resolved onto the existing entity"
        );
        return Ok(Some(row.id));
    }

    Ok(None)
}

#[allow(clippy::too_many_arguments)]
async fn resolve_or_create_entity(
    pool: &PgPool,
    embedder: &Embedder,
    entity_type: &str,
    name: &str,
    source_event_id: Uuid,
    observation: &str,
    model: &str,
    happened: Option<Happened>,
) -> anyhow::Result<Uuid> {
    let existing = resolve_existing(pool, entity_type, name).await?;

    let entity_id = match existing {
        Some(row) => row,
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

            // Embedded on creation. The column and its HNSW index have
            // existed since migration 0001 and nothing ever wrote to them
            // — 0 of 78 entities had one — so the semantic half of
            // finding "is this thing already in here somewhere" simply
            // did not work. Name and type together, because the name
            // alone is often a single word with no context ("Aufguss").
            let embedding = embed_entity(embedder, entity_type, name).await;

            sqlx::query!(
                r#"insert into entities (id, entity_type, name, embedding) values ($1, $2, $3, $4)"#,
                id,
                entity_type,
                name,
                embedding as _,
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

/// How many entities one backfill pass embeds.
///
/// Local and free, so the only reason to bound it at all is to keep one
/// pass from holding the embedder for a long stretch while captures are
/// arriving.
const EMBEDDING_BACKFILL_BATCH: i64 = 200;

/// Gives an embedding to entities that have none.
///
/// Two populations need this and they are the same shape: every entity
/// created before entities were embedded at all (78 of 78 when this was
/// written), and the occasional later one whose embedding failed. Returns
/// how many it filled, so a caller can tell "done" from "more to do".
pub async fn backfill_entity_embeddings(
    pool: &PgPool,
    embedder: &Embedder,
) -> anyhow::Result<usize> {
    let rows = sqlx::query!(
        r#"
        select id, name, entity_type
        from entities
        where embedding is null
        order by updated_at desc
        limit $1
        "#,
        EMBEDDING_BACKFILL_BATCH,
    )
    .fetch_all(pool)
    .await?;

    let mut filled = 0usize;
    for row in rows {
        let Some(embedding) = embed_entity(embedder, &row.entity_type, &row.name).await else {
            continue;
        };
        sqlx::query!(
            r#"update entities set embedding = $2 where id = $1"#,
            row.id,
            embedding as _,
        )
        .execute(pool)
        .await?;
        filled += 1;
    }

    Ok(filled)
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

    const SPOKEN: &str = "Heute war viel los. Beim Paul  muss ich noch die Hafenportal-Deadline ansprechen, sonst rutscht das wieder.";

    #[test]
    fn a_quote_is_cut_out_of_the_transcript_as_it_stands() {
        // Case and a double space differ; the answer is the transcript's
        // own spelling, not the model's.
        assert_eq!(
            verbatim(
                "beim paul muss ich noch die hafenportal-deadline ansprechen",
                SPOKEN
            )
            .as_deref(),
            Some("Beim Paul  muss ich noch die Hafenportal-Deadline ansprechen")
        );
    }

    #[test]
    fn quote_marks_and_end_punctuation_are_forgiven() {
        assert_eq!(
            verbatim("„muss ich noch die Hafenportal-Deadline ansprechen.“", SPOKEN).as_deref(),
            Some("muss ich noch die Hafenportal-Deadline ansprechen")
        );
    }

    #[test]
    fn a_paraphrase_is_not_a_quote() {
        assert_eq!(verbatim("Paul nach der Deadline fragen", SPOKEN), None);
        assert_eq!(
            verbatim("muss ich noch die Deadline ansprechen", SPOKEN),
            None
        );
    }

    #[test]
    fn a_fragment_too_short_to_mean_anything_is_refused() {
        assert_eq!(verbatim("„ ab “", SPOKEN), None);
    }

    #[test]
    fn umlauts_survive_the_cut() {
        let transcript = "Ich sollte Günter fragen, ob die Größe passt.";
        assert_eq!(
            verbatim("GÜNTER FRAGEN, OB DIE GRÖSSE PASST", transcript),
            // "ß" lowercases to itself, not "ss": no match, and no
            // pretending there was one.
            None
        );
        assert_eq!(
            verbatim("günter fragen, ob die größe passt", transcript).as_deref(),
            Some("Günter fragen, ob die Größe passt")
        );
    }

    #[test]
    fn nonsense_resolves_to_nothing_rather_than_to_today() {
        assert!(resolve_when("next tuesday", None, BERLIN).is_none());
        assert!(resolve_when("", Some("day"), BERLIN).is_none());
        assert!(resolve_when("2026-13-40", None, BERLIN).is_none());
    }
}
