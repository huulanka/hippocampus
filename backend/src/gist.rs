//! What is known about an entity, in a few sentences.
//!
//! The missing half of the memory model. The notes are episodes; an entity
//! page listing twenty of them is an archive, not knowledge. After twenty
//! mentions of something a person has a sense of what it *is* — the gist —
//! and until now the only thing standing in for that was
//! `current_summary`, which is whatever was observed last.
//!
//! A gist is written by a model from the entity's observations and every
//! sentence names the notes it rests on, the same contract as the weekly
//! story: a sentence that cannot be traced back is dropped, not shown. It
//! is rewritten only when the entity has been observed again, and only
//! once there are enough observations for a summary to say more than the
//! one note it would summarise (P12: the weight of evidence per entity).
//!
//! It is also where the memory admits that things change. "Paul works at
//! Northwind" and, three weeks later, "Paul has moved to Contoso" are two
//! observations of equal standing; nothing in the graph says the second
//! replaces the first. The gist does: the later note is the current state,
//! the earlier one is what it used to be, both with their dates.

use std::collections::HashMap;
use std::time::Duration;

use contracts::{EntityGist, StorySentence};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::events;
use crate::openrouter::WrittenSentence;

/// Fewer observations than this and there is nothing to summarise: the
/// gist of two notes is the two notes.
pub const MIN_OBSERVATIONS: i64 = 3;

/// The newest observations shown to the model. An entity mentioned two
/// hundred times is summarised from its recent history; the old part is
/// still on its page.
const MAX_NOTES: i64 = 40;

const NOTE_CHARS: usize = 500;
const MAX_SENTENCES: usize = 4;

const SWEEP_INTERVAL: Duration = Duration::from_secs(300);
const SWEEP_BATCH: i64 = 10;

#[derive(Debug, Serialize)]
struct GistNote {
    tag: String,
    said: String,
    /// The model's reading of the note for this entity.
    observation: String,
    /// What was actually said, so the reading can be checked.
    text: String,
}

#[derive(Deserialize)]
struct Written {
    #[serde(default)]
    sentences: Vec<WrittenSentence>,
}

const GIST_PROMPT: &str = r#"You write what one person's memory holds about one thing — a person, project, place, topic, anything — from the notes in which they mentioned it. They will read it at the top of that thing's page, to know at a glance what it is and where it stands.

Write 1 to 4 short sentences about the thing itself ("Paul leads the harbour portal project…"), not about the notes and not addressed to the person. Start with what it is and where it stands now. Only what the notes say; no general knowledge, no advice, no guesses about feelings.

Things change, and the notes are in time order. When a later note updates, contradicts or replaces an earlier one — a new employer, a moved date, a decision taken back — the later note is the current state. Say what it was and what it is now, each with its date ("until mid-September …, since 18 September …"). Never blend the two into one statement, and never silently drop the earlier one.

Every sentence must rest on specific notes; cite them by tag. Write in the language the notes are written in.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"sentences": [{"text": "...", "sources": ["n3", "n7"]}]}"#;

/// Entities whose gist is missing or older than their last observation.
pub async fn due(pool: &PgPool, limit: i64) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        select e.id as "id!"
        from entities e
        join observations o on o.entity_id = e.id
        left join entity_gists g on g.entity_id = e.id
        where e.merged_into is null
        group by e.id, g.observations_seen
        having count(*) >= $1
           and (g.observations_seen is null or g.observations_seen <> count(*))
        order by max(o.created_at) desc
        limit $2
        "#,
        MIN_OBSERVATIONS,
        limit,
    )
    .fetch_all(pool)
    .await
}

/// Writes one entity's gist from its observations and keeps it.
///
/// A model that writes nothing it can source still leaves a (blank) row
/// behind with the count it saw, so the entity is not asked about again
/// until it is observed again.
pub async fn write(state: &AppState, entity_id: Uuid) -> anyhow::Result<()> {
    let Some(model) = state.openrouter.as_ref() else {
        return Ok(());
    };
    let pool = &state.pool;

    let Some(entity) = sqlx::query!(
        r#"select name, entity_type from entities where id = $1 and merged_into is null"#,
        entity_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };

    let total = sqlx::query_scalar!(
        r#"select count(*) as "count!" from observations where entity_id = $1"#,
        entity_id,
    )
    .fetch_one(pool)
    .await?;

    let mut rows = sqlx::query!(
        r#"
        select o.source_event_id, o.text as observation,
               cs.transcript, cs.occurred_at
        from observations o
        join capture_search cs on cs.event_id = o.source_event_id
        where o.entity_id = $1
        order by cs.occurred_at desc
        limit $2
        "#,
        entity_id,
        MAX_NOTES,
    )
    .fetch_all(pool)
    .await?;
    rows.reverse();

    let tz = state.timezone;
    let notes: Vec<GistNote> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| GistNote {
            tag: format!("n{}", i + 1),
            said: row
                .occurred_at
                .with_timezone(&tz)
                .format("%d.%m.%Y")
                .to_string(),
            observation: row.observation.clone(),
            text: row.transcript.chars().take(NOTE_CHARS).collect(),
        })
        .collect();
    let by_tag: HashMap<&str, Uuid> = notes
        .iter()
        .zip(&rows)
        .map(|(note, row)| (note.tag.as_str(), row.source_event_id))
        .collect();

    let sentences = if notes.is_empty() {
        Vec::new()
    } else {
        let user = format!(
            "The thing: {} ({})\n\nNotes, oldest first:\n{}",
            entity.name,
            entity.entity_type,
            serde_json::to_string_pretty(&notes)?
        );
        let written: Written = model.complete_json("gist", GIST_PROMPT, &user).await?;
        crate::routes::review::sourced(written.sentences, &by_tag, MAX_SENTENCES)
    };

    let seen = i32::try_from(total).unwrap_or(i32::MAX);
    let mut tx = pool.begin().await?;
    // Ids and counts only; the sentences are the model's reading of the
    // notes, and live where they can be deleted with them.
    let event = events::append_tx(
        &mut tx,
        Uuid::new_v4(),
        1,
        "entity.gist_written",
        &json!({
            "entity_id": entity_id,
            "observations_seen": seen,
            "sentences": sentences.len(),
        }),
        model.model_name(),
    )
    .await?;
    sqlx::query!(
        r#"
        insert into entity_gists (entity_id, sentences, model, observations_seen, event_id)
        values ($1, $2, $3, $4, $5)
        on conflict (entity_id) do update set
            sentences = excluded.sentences,
            model = excluded.model,
            observations_seen = excluded.observations_seen,
            event_id = excluded.event_id,
            written_at = now()
        "#,
        entity_id,
        serde_json::to_value(&sentences)?,
        model.model_name(),
        seen,
        event.id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The gist on an entity's page. `None` until there is one worth showing.
pub async fn for_entity(pool: &PgPool, entity_id: Uuid) -> Result<Option<EntityGist>, sqlx::Error> {
    let row = sqlx::query!(
        r#"select sentences, model, written_at, observations_seen from entity_gists where entity_id = $1"#,
        entity_id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|row| {
        let sentences: Vec<StorySentence> = serde_json::from_value(row.sentences).ok()?;
        (!sentences.is_empty()).then_some(EntityGist {
            sentences,
            model: row.model,
            written_at: row.written_at,
            observations_seen: row.observations_seen,
        })
    }))
}

/// Gists of several entities at once, as plain text — orientation for
/// the answer to a question.
pub async fn texts(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query!(
        r#"select entity_id, sentences from entity_gists where entity_id = any($1)"#,
        ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let sentences: Vec<StorySentence> = serde_json::from_value(row.sentences).ok()?;
            let text = sentences
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            (!text.is_empty()).then_some((row.entity_id, text))
        })
        .collect())
}

/// Gists that rest on a note go with its words. The entity is written
/// again on the next sweep, from what is left.
pub async fn forget_citing(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    capture_event_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let removed = sqlx::query!(
        r#"
        delete from entity_gists g
        where exists (
            select 1
            from jsonb_array_elements(g.sentences) s,
                 jsonb_array_elements_text(s -> 'sources') src
            where src = $1::text
        )
        "#,
        capture_event_id.to_string(),
    )
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(removed)
}

/// Writes gists in the background, most recently observed first, stopping
/// at the first failure.
pub fn watch(state: AppState) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            if state.openrouter.is_none() {
                continue;
            }
            let due = match due(&state.pool, SWEEP_BATCH).await {
                Ok(due) => due,
                Err(err) => {
                    tracing::warn!(?err, "could not look for entities whose gist is stale");
                    continue;
                }
            };
            let mut written = 0;
            for id in due {
                match write(&state, id).await {
                    Ok(()) => written += 1,
                    Err(err) => {
                        tracing::warn!(?err, entity_id = %id, "could not write an entity's gist");
                        break;
                    }
                }
            }
            if written > 0 {
                tracing::info!(written, "wrote entity gists");
            }
        }
    });
}

/// Against a real database:
///
///     cargo test -p backend --features db-tests gist
#[cfg(all(test, feature = "db-tests"))]
mod db_tests {
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{due, for_entity, forget_citing};

    async fn a_capture(pool: &PgPool) -> Uuid {
        let event = sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source)
               values (gen_random_uuid(), 1, 'capture.recorded', '{}'::jsonb, 'test')
               returning id"#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into capture_search (event_id, transcript, occurred_at) values ($1, 'x', now())",
            event,
        )
        .execute(pool)
        .await
        .unwrap();
        event
    }

    async fn observe(pool: &PgPool, entity: Uuid, note: Uuid) {
        sqlx::query!(
            "insert into observations (entity_id, source_event_id, text, model) values ($1, $2, 'seen', 'test-model')",
            entity,
            note,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn paul_seen(pool: &PgPool, times: usize) -> (Uuid, Vec<Uuid>) {
        sqlx::query!(
            "insert into entity_type_registry (entity_type) values ('Person') on conflict do nothing"
        )
        .execute(pool)
        .await
        .unwrap();
        let paul = sqlx::query_scalar!(
            "insert into entities (entity_type, name) values ('Person', 'Paul') returning id"
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let mut notes = Vec::new();
        for _ in 0..times {
            let note = a_capture(pool).await;
            observe(pool, paul, note).await;
            notes.push(note);
        }
        (paul, notes)
    }

    async fn written(pool: &PgPool, entity: Uuid, cites: Uuid, seen: i32) {
        let event = sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source)
               values (gen_random_uuid(), 1, 'entity.gist_written', '{}'::jsonb, 'test')
               returning id"#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into entity_gists (entity_id, sentences, model, observations_seen, event_id) values ($1, $2, 'test-model', $3, $4)",
            entity,
            json!([{"text": "Paul leads it.", "sources": [cites]}]),
            seen,
            event,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn two_mentions_have_no_gist_and_three_do(pool: PgPool) {
        let (paul, _) = paul_seen(&pool, 2).await;
        assert!(due(&pool, 10).await.unwrap().is_empty());

        let note = a_capture(&pool).await;
        observe(&pool, paul, note).await;
        assert_eq!(due(&pool, 10).await.unwrap(), vec![paul]);

        written(&pool, paul, note, 3).await;
        assert!(
            due(&pool, 10).await.unwrap().is_empty(),
            "nothing new since it was written"
        );
        assert_eq!(
            for_entity(&pool, paul)
                .await
                .unwrap()
                .unwrap()
                .sentences
                .len(),
            1
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_gist_goes_with_a_note_it_rests_on(pool: PgPool) {
        let (paul, notes) = paul_seen(&pool, 3).await;
        written(&pool, paul, notes[1], 3).await;

        let mut tx = pool.begin().await.unwrap();
        assert_eq!(
            forget_citing(&mut tx, notes[0]).await.unwrap(),
            0,
            "not cited, kept"
        );
        assert_eq!(forget_citing(&mut tx, notes[1]).await.unwrap(), 1);
        tx.commit().await.unwrap();

        assert!(for_entity(&pool, paul).await.unwrap().is_none());
        assert_eq!(
            due(&pool, 10).await.unwrap(),
            vec![paul],
            "written again from what is left"
        );
    }
}
