//! What you still mean to do, and the moments it comes back in.
//!
//! Two of those moments are served from here: a capture that mentions
//! something an open intention is about (`GET /captures/{id}/intentions`),
//! and a meeting about to happen (`POST /brief`). The third place an
//! intention shows up — an entity's own page — reads through
//! [`for_entity`]. See docs/prospective-memory.md.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path as AxumPath, State};
use chrono::{DateTime, Utc};
use contracts::{
    Brief, BriefEntity, BriefQuote, BriefRequest, CaptureIntentions, FulfilIntentionRequest,
    Intention, IntentionEntity,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::brief;
use crate::error::AppError;
use crate::events;

/// Who a dismissal or a confirmation is recorded as coming from. Not the
/// model: these are the user's own decisions, and the log should say so.
const USER: &str = "user";

/// How many entities a brief speaks about. A meeting title rarely names
/// more than two things; past three, the page is a list rather than a
/// briefing.
const BRIEF_ENTITIES: usize = 3;

/// How many of your own sentences a brief quotes per entity. The newest
/// ones — a brief is what you said *lately*, not a biography.
const BRIEF_QUOTES_PER_ENTITY: i64 = 2;

/// Reads intentions by id, with their entities, newest said first.
///
/// The one place an `Intention` is assembled, so every route shows the
/// same shape: merges followed, the words from `capture_search` (where a
/// correction lands) for the date.
async fn load(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Intention>, AppError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query!(
        r#"
        select i.id, i.text, i.quote, i.status, i.source_event_id, i.created_at, i.resolved_at,
               cs.occurred_at as "said_at?"
        from intentions i
        left join capture_search cs on cs.event_id = i.source_event_id
        where i.id = any($1)
        order by coalesce(cs.occurred_at, i.created_at) desc
        "#,
        ids,
    )
    .fetch_all(pool)
    .await?;

    // Merges are followed here rather than rewritten into the table, so
    // an unmerge puts the intention back on the right entity by itself.
    let about = sqlx::query!(
        r#"
        select distinct ie.intention_id, e.id, e.name, e.entity_type
        from intention_entities ie
        join entities start on start.id = ie.entity_id
        join entities e on e.id = coalesce(start.merged_into, start.id)
        where ie.intention_id = any($1)
        order by e.name
        "#,
        ids,
    )
    .fetch_all(pool)
    .await?;

    let mut entities: HashMap<Uuid, Vec<IntentionEntity>> = HashMap::new();
    for row in about {
        entities
            .entry(row.intention_id)
            .or_default()
            .push(IntentionEntity {
                id: row.id,
                name: row.name,
                entity_type: row.entity_type,
            });
    }

    Ok(rows
        .into_iter()
        .map(|row| Intention {
            id: row.id,
            text: row.text,
            quote: row.quote,
            status: row.status,
            capture_event_id: row.source_event_id,
            said_at: row.said_at.unwrap_or(row.created_at),
            resolved_at: row.resolved_at,
            entities: entities.remove(&row.id).unwrap_or_default(),
        })
        .collect())
}

/// Open intentions about any of these entities (already merge-resolved).
async fn open_about(pool: &PgPool, entity_ids: &[Uuid]) -> Result<Vec<Uuid>, AppError> {
    Ok(sqlx::query_scalar!(
        r#"
        select distinct i.id
        from intentions i
        join intention_entities ie on ie.intention_id = i.id
        join entities start on start.id = ie.entity_id
        where i.status = 'open'
          and coalesce(start.merged_into, start.id) = any($1)
        "#,
        entity_ids,
    )
    .fetch_all(pool)
    .await?)
}

/// The open intentions on an entity's page.
pub async fn for_entity(pool: &PgPool, entity_id: Uuid) -> Result<Vec<Intention>, AppError> {
    let ids = open_about(pool, &[entity_id]).await?;
    load(pool, &ids).await
}

/// `GET /intentions` — everything still open, newest first.
pub async fn open(State(state): State<AppState>) -> Result<Json<Vec<Intention>>, AppError> {
    let ids = sqlx::query_scalar!(r#"select id from intentions where status = 'open'"#)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(load(&state.pool, &ids).await?))
}

/// `GET /captures/{id}/intentions` — what this capture noted, and what it
/// reminds you of.
///
/// Asked for right after speaking, so most of the time the honest answer
/// at first is "not yet": structuring is a model call behind the
/// response. `pending` says so, and the client asks again.
pub async fn for_capture(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<CaptureIntentions>, AppError> {
    // No model configured means no structuring is coming, which is not
    // the same as "still waiting" — a client that polled for it would
    // poll until its own deadline, every time.
    Ok(Json(
        capture_intentions(&state.pool, id, state.openrouter.is_some()).await?,
    ))
}

pub(crate) async fn capture_intentions(
    pool: &PgPool,
    id: Uuid,
    structuring_configured: bool,
) -> Result<CaptureIntentions, AppError> {
    let pipeline = sqlx::query!(
        r#"select structured_at, abandoned_at from capture_pipeline where capture_event_id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?;
    let pending = structuring_configured
        && pipeline.is_some_and(|p| p.structured_at.is_none() && p.abandoned_at.is_none());

    let noted = sqlx::query_scalar!(
        r#"select id from intentions where source_event_id = $1"#,
        id,
    )
    .fetch_all(pool)
    .await?;

    // Observations already point at the surviving entity after a merge,
    // so they need no resolving of their own.
    let reminded = sqlx::query_scalar!(
        r#"
        select distinct i.id
        from intentions i
        join intention_entities ie on ie.intention_id = i.id
        join entities start on start.id = ie.entity_id
        where i.status = 'open'
          and i.source_event_id <> $1
          and coalesce(start.merged_into, start.id) in (
              select entity_id from observations where source_event_id = $1
          )
        "#,
        id,
    )
    .fetch_all(pool)
    .await?;

    Ok(CaptureIntentions {
        pending,
        noted: load(pool, &noted).await?,
        reminded: load(pool, &reminded).await?,
    })
}

/// Moves an intention to `to`, provided it is currently in one of `from`,
/// and records why.
///
/// Every change is an event on the intention's own stream; the status
/// column is its projection. A change that would not change anything
/// answers with the intention as it stands rather than an error — two
/// windows ticking the same thing off is not a conflict worth reporting.
async fn transition(
    pool: &PgPool,
    id: Uuid,
    from: &[&str],
    to: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<Json<Intention>, AppError> {
    let current = sqlx::query_scalar!(r#"select status from intentions where id = $1"#, id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("no such intention"))?;

    if current != to && from.contains(&current.as_str()) {
        let mut tx = pool.begin().await?;
        let version = sqlx::query_scalar!(
            r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
            id,
        )
        .fetch_one(&mut *tx)
        .await?;
        events::append_tx(&mut tx, id, version, event_type, &payload, USER).await?;
        let resolved_at: Option<DateTime<Utc>> = (to != "open").then(Utc::now);
        sqlx::query!(
            r#"update intentions set status = $2, resolved_at = $3 where id = $1"#,
            id,
            to,
            resolved_at,
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(%id, from = %current, %to, "intention changed");
    }

    let mut loaded = load(pool, &[id]).await?;
    loaded
        .pop()
        .map(Json)
        .ok_or_else(|| AppError::not_found("no such intention"))
}

/// `POST /intentions/{id}/dismiss` — "that was not an intention".
pub async fn dismiss(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<Intention>, AppError> {
    transition(
        &state.pool,
        id,
        &["open"],
        "dismissed",
        "intention.dismissed",
        json!({ "intention_id": id }),
    )
    .await
}

/// `POST /intentions/{id}/fulfil` — "done", after a meeting or by hand.
pub async fn fulfil(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
    body: Option<Json<FulfilIntentionRequest>>,
) -> Result<Json<Intention>, AppError> {
    let via = body
        .and_then(|Json(b)| b.via)
        .filter(|via| via == "calendar" || via == "manual")
        .unwrap_or_else(|| "manual".to_string());
    transition(
        &state.pool,
        id,
        &["open"],
        "fulfilled",
        "intention.fulfilled",
        json!({ "intention_id": id, "via": via }),
    )
    .await
}

/// `POST /intentions/{id}/reopen` — taking back a dismissal or a "done".
/// Reversible because the person pressing the button is sometimes wrong.
pub async fn reopen(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Json<Intention>, AppError> {
    transition(
        &state.pool,
        id,
        &["dismissed", "fulfilled"],
        "open",
        "intention.reopened",
        json!({ "intention_id": id }),
    )
    .await
}

/// Everything the graph knows by name, for matching against a meeting.
pub(crate) async fn candidates(pool: &PgPool) -> Result<Vec<brief::Candidate>, AppError> {
    let rows = sqlx::query!(
        r#"
        select e.id, e.name, e.entity_type,
               coalesce(array_agg(a.name) filter (where a.name is not null), '{}') as "aliases!"
        from entities e
        left join entity_alias a on a.entity_id = e.id
        where e.merged_into is null
        group by e.id, e.name, e.entity_type
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let mut names = vec![row.name.clone()];
            names.extend(row.aliases.into_iter().filter(|a| a != &row.name));
            brief::Candidate {
                id: row.id,
                entity_type: row.entity_type,
                names,
            }
        })
        .collect())
}

/// `POST /brief` — what you know about the people and things a meeting is
/// about.
///
/// The meeting itself is not stored and not logged beyond a count: the
/// calendar belongs to the person, not to their memory (ADR 0013).
pub async fn brief(
    State(state): State<AppState>,
    Json(request): Json<BriefRequest>,
) -> Result<Json<Brief>, AppError> {
    Ok(Json(brief_for(&state.pool, &request).await?))
}

pub(crate) async fn brief_for(pool: &PgPool, request: &BriefRequest) -> Result<Brief, AppError> {
    let known = candidates(pool).await?;
    let matches = brief::find(&known, &request.title, &request.people);
    if matches.is_empty() {
        tracing::debug!(
            people = request.people.len(),
            "a meeting mentioned nothing known"
        );
        return Ok(Brief::default());
    }

    let by_id: HashMap<Uuid, &brief::Candidate> = known.iter().map(|c| (c.id, c)).collect();
    let matched_ids: Vec<Uuid> = matches.iter().map(|m| m.id).collect();
    let intentions = load(pool, &open_about(pool, &matched_ids).await?).await?;

    // The entities worth a place on the page: those with something open
    // first, then the strongest matches, three at most.
    let mut chosen: Vec<&brief::Match> = matches.iter().collect();
    chosen.sort_by_key(|m| {
        let open = intentions
            .iter()
            .any(|i| i.entities.iter().any(|e| e.id == m.id));
        !open
    });
    chosen.truncate(BRIEF_ENTITIES);

    let mut said = Vec::new();
    for m in &chosen {
        let rows = sqlx::query!(
            r#"
            select o.source_event_id, o.text as observation, cs.transcript, cs.occurred_at
            from observations o
            join capture_search cs on cs.event_id = o.source_event_id
            where o.entity_id = $1
            order by cs.occurred_at desc
            limit $2
            "#,
            m.id,
            BRIEF_QUOTES_PER_ENTITY,
        )
        .fetch_all(pool)
        .await?;
        said.extend(rows.into_iter().map(|row| BriefQuote {
            entity_id: m.id,
            capture_event_id: row.source_event_id,
            transcript_text: row.transcript,
            observation: row.observation,
            occurred_at: row.occurred_at,
        }));
    }

    tracing::info!(
        matched = matches.len(),
        intentions = intentions.len(),
        "briefed a meeting"
    );

    Ok(Brief {
        entities: chosen
            .into_iter()
            .filter_map(|m| {
                let candidate = by_id.get(&m.id)?;
                Some(BriefEntity {
                    id: m.id,
                    name: candidate.names[0].clone(),
                    entity_type: candidate.entity_type.clone(),
                    matched_on: m.on.as_str().to_string(),
                    matched_text: m.text.clone(),
                })
            })
            .collect(),
        intentions,
        said,
    })
}

/// The whole round trip against a real database: noted in one capture,
/// reminded by the next, briefed before a meeting, closed afterwards.
///
///     cargo test -p backend --features db-tests intentions
#[cfg(all(test, feature = "db-tests"))]
mod db_tests {
    use std::collections::HashMap;

    use contracts::BriefRequest;
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use super::{brief_for, capture_intentions, transition};
    use crate::openrouter::ExtractedIntention;
    use crate::structuring::record_intentions;

    async fn a_capture(pool: &PgPool, text: &str) -> Uuid {
        let event = sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source)
               values (gen_random_uuid(), 1, 'capture.recorded', '{}'::jsonb, 'test')
               returning id"#,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        sqlx::query!(
            "insert into capture_search (event_id, transcript, occurred_at) values ($1, $2, now())",
            event,
            text,
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

    async fn observe(pool: &PgPool, entity: Uuid, capture: Uuid) {
        sqlx::query!(
            "insert into observations (entity_id, source_event_id, text, model) values ($1, $2, 'observed', 'test-model')",
            entity,
            capture,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    fn intention(text: &str, quote: &str, about: &[&str]) -> ExtractedIntention {
        ExtractedIntention {
            text: text.into(),
            quote: Some(quote.into()),
            about: about.iter().map(|s| s.to_string()).collect(),
        }
    }

    const SPOKEN: &str = "Beim Paul muss ich noch die Hafenportal-Deadline ansprechen.";

    /// Paul, with one capture that noted an intention about him.
    async fn paul_with_an_intention(pool: &PgPool) -> (Uuid, Uuid) {
        let paul = an_entity(pool, "Paul").await;
        let capture = a_capture(pool, SPOKEN).await;
        observe(pool, paul, capture).await;
        let ids = HashMap::from([("Paul".to_string(), paul)]);
        record_intentions(
            pool,
            capture,
            SPOKEN,
            &[intention(
                "Paul nach der Hafenportal-Deadline fragen",
                "muss ich noch die Hafenportal-Deadline ansprechen",
                &["Paul"],
            )],
            &ids,
            "test-model",
        )
        .await
        .unwrap();
        (paul, capture)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_later_mention_is_reminded_and_the_first_is_not(pool: PgPool) {
        let (paul, first) = paul_with_an_intention(&pool).await;

        let own = capture_intentions(&pool, first, false).await.unwrap();
        assert_eq!(own.noted.len(), 1);
        assert_eq!(
            own.noted[0].quote.as_deref(),
            Some("muss ich noch die Hafenportal-Deadline ansprechen")
        );
        // A capture does not remind you of what it has just said.
        assert!(own.reminded.is_empty());

        let later = a_capture(&pool, "Paul war heute krank.").await;
        observe(&pool, paul, later).await;
        let next = capture_intentions(&pool, later, false).await.unwrap();
        assert!(next.noted.is_empty());
        assert_eq!(next.reminded.len(), 1);
        assert_eq!(next.reminded[0].entities[0].name, "Paul");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_quote_that_is_not_in_the_transcript_is_not_kept(pool: PgPool) {
        let paul = an_entity(&pool, "Paul").await;
        let capture = a_capture(&pool, SPOKEN).await;
        let ids = HashMap::from([("Paul".to_string(), paul)]);
        record_intentions(
            &pool,
            capture,
            SPOKEN,
            &[intention(
                "Paul fragen",
                "Paul nach der Deadline fragen",
                &["Paul", "Nobody"],
            )],
            &ids,
            "test-model",
        )
        .await
        .unwrap();

        let noted = capture_intentions(&pool, capture, false)
            .await
            .unwrap()
            .noted;
        assert_eq!(noted[0].quote, None);
        // "Nobody" was not in the extraction's own entities.
        assert_eq!(noted[0].entities.len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn structuring_twice_does_not_note_twice(pool: PgPool) {
        let (paul, capture) = paul_with_an_intention(&pool).await;
        let ids = HashMap::from([("Paul".to_string(), paul)]);
        record_intentions(
            &pool,
            capture,
            SPOKEN,
            &[intention("Paul fragen", "Paul", &["Paul"])],
            &ids,
            "test-model",
        )
        .await
        .unwrap();
        let noted = capture_intentions(&pool, capture, false)
            .await
            .unwrap()
            .noted;
        assert_eq!(noted.len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_meeting_with_paul_brings_it_up_until_it_is_done(pool: PgPool) {
        paul_with_an_intention(&pool).await;
        let meeting = BriefRequest {
            title: "Jour fixe".into(),
            people: vec!["Paul Hartmann".into()],
        };

        let before = brief_for(&pool, &meeting).await.unwrap();
        assert_eq!(before.entities.len(), 1);
        assert_eq!(before.entities[0].matched_on, "attendee");
        assert_eq!(before.intentions.len(), 1);
        assert_eq!(before.said.len(), 1);
        assert_eq!(before.said[0].transcript_text, SPOKEN);

        let id = before.intentions[0].id;
        let done = transition(
            &pool,
            id,
            &["open"],
            "fulfilled",
            "intention.fulfilled",
            json!({ "intention_id": id, "via": "calendar" }),
        )
        .await
        .unwrap();
        assert_eq!(done.status, "fulfilled");
        assert!(done.resolved_at.is_some());

        // Paul is still on the page — only the thing to bring up is gone.
        let after = brief_for(&pool, &meeting).await.unwrap();
        assert_eq!(after.entities.len(), 1);
        assert!(after.intentions.is_empty());

        let events = sqlx::query_scalar!(
            "select event_type from events where stream_id = $1 order by version",
            id
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(events, ["intention.noted", "intention.fulfilled"]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_dismissal_can_be_taken_back(pool: PgPool) {
        let (_, capture) = paul_with_an_intention(&pool).await;
        let id = capture_intentions(&pool, capture, false)
            .await
            .unwrap()
            .noted[0]
            .id;
        let dismiss = || {
            transition(
                &pool,
                id,
                &["open"],
                "dismissed",
                "intention.dismissed",
                json!({}),
            )
        };

        assert_eq!(dismiss().await.unwrap().status, "dismissed");
        // Dismissing twice changes nothing and is not an error.
        assert_eq!(dismiss().await.unwrap().status, "dismissed");

        let back = transition(
            &pool,
            id,
            &["dismissed", "fulfilled"],
            "open",
            "intention.reopened",
            json!({}),
        )
        .await
        .unwrap();
        assert_eq!(back.status, "open");
        assert!(back.resolved_at.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_merge_is_followed_to_the_surviving_entity(pool: PgPool) {
        let (paul, _) = paul_with_an_intention(&pool).await;
        let full = an_entity(&pool, "Paul Hartmann").await;
        sqlx::query!(
            "update entities set merged_into = $1 where id = $2",
            full,
            paul
        )
        .execute(&pool)
        .await
        .unwrap();

        let meeting = BriefRequest {
            title: "1:1 Paul Hartmann".into(),
            people: vec![],
        };
        let brief = brief_for(&pool, &meeting).await.unwrap();
        assert_eq!(
            brief.entities.len(),
            1,
            "the merged-away Paul is not a second match"
        );
        assert_eq!(brief.entities[0].id, full);
        assert_eq!(brief.intentions.len(), 1);
        assert_eq!(brief.intentions[0].entities[0].name, "Paul Hartmann");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_meeting_about_nothing_known_is_an_empty_brief(pool: PgPool) {
        paul_with_an_intention(&pool).await;
        let meeting = BriefRequest {
            title: "Zahnarzt".into(),
            people: vec!["Dr. Brandt".into()],
        };
        let brief = brief_for(&pool, &meeting).await.unwrap();
        assert!(brief.entities.is_empty());
        assert!(brief.intentions.is_empty());
    }
}
