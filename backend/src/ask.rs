//! Asking your notes a question, and getting an answer only they can give.
//!
//! The product refused chat for a long time, for a reason that still
//! holds: a memory system must not confabulate about your own past,
//! because an invented memory cannot be told from a real one. What changed
//! is not the risk but the shape of the answer (docs/finding-again.md):
//!
//! 1. **The backend chooses what the model may read.** Not the archive,
//!    not a tool it can call — at most thirty notes this code picked:
//!    the search hits for the question, the notes about the people and
//!    subjects it names, one step further along the graph, the notes
//!    that belonged to a meeting with them, and the notes those hits
//!    continue. The same zero-retention route structuring already takes.
//! 2. **Every sentence cites its notes**, and one that cites nothing, or
//!    only a tag that was never shown, is dropped — the weekly story's
//!    contract.
//! 3. **A second reading checks each sentence** against exactly the notes
//!    it cites. What they do not carry is dropped, and the answer says how
//!    many were.
//! 4. **"The notes do not say" is an answer.** It comes with the nearest
//!    notes, so there is still something to read.
//!
//! Nothing about a question is stored. It is asked, answered and gone.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use contracts::{Answer, AnswerNote, AskRequest, StorySentence};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::brief;
use crate::openrouter::WrittenSentence;

const MAX_QUESTION_CHARS: usize = 500;

/// The most notes one answer is written from. Enough for "what's the
/// state of the harbour portal" across a few weeks; few enough that the
/// model reads all of them rather than skimming.
const MAX_NOTES: usize = 30;

const SEARCH_HITS: i64 = 12;
const ENTITIES: usize = 4;
const NOTES_PER_ENTITY: i64 = 8;
const NEIGHBOURS: i64 = 4;
const NOTES_PER_NEIGHBOUR: i64 = 3;
const OCCASION_NOTES: i64 = 6;
/// How many of the top hits have their episode neighbours pulled in.
const EPISODE_SEEDS: usize = 5;
/// Shown when the notes answer nothing.
const NEAREST: usize = 5;

const NOTE_CHARS: usize = 900;
const MAX_SENTENCES: usize = 6;

#[derive(Debug, Default, Deserialize)]
struct Plan {
    #[serde(default)]
    search: String,
    #[serde(default)]
    names: Vec<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

#[derive(Debug, Serialize)]
struct Note {
    tag: String,
    said: String,
    /// "during a meeting with Paul", "continues n4" — what the note was
    /// said next to, when that is known.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    context: Vec<String>,
    text: String,
}

#[derive(Deserialize)]
struct Written {
    #[serde(default)]
    sentences: Vec<WrittenSentence>,
}

#[derive(Deserialize)]
struct Checked {
    #[serde(default)]
    supported: Vec<String>,
}

const PLAN_PROMPT: &str = r#"You prepare a search of one person's own notes for a question they ask about them. You do not answer it.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"search": "...", "names": ["..."], "from": null, "to": null}

"search": the words to look for in the notes, in the language of the question, without question words or filler ("Hafenportal Deadline" for "Wann war nochmal die Deadline beim Hafenportal?").
"names": people, projects, places, organisations or other things the question names, written as in the question but in their base form ("Paul" for "Pauls").
"from" / "to": ISO dates (YYYY-MM-DD) only when the question itself limits the time ("letzte Woche", "im August", "since Monday"), resolved against the date given below. Otherwise null."#;

const ANSWER_PROMPT: &str = r#"You answer a person's question about their own life, from their own notes and from nothing else.

The notes are listed oldest first. Each has a tag, when it was said, sometimes what it was said next to (a meeting it belonged to, an earlier note it continues), and the words. Lines under "Known about" are earlier summaries, for orientation only: never cite them and never state anything that rests on them alone.

Rules:
- Every sentence must rest on specific notes; cite their tags. A sentence you cannot cite, do not write.
- Only what the notes say. No general knowledge, no advice, no guesses about feelings or reasons the notes do not state.
- If the notes do not answer the question, return an empty list. That is a correct answer, and better than a near miss.
- Things change. When notes disagree, the later one is the current state: say what it was before and what it is now, each with its date. Never blend them into one statement.
- Say when something was said whenever the time matters to the question ("am 12.09. …").
- Short: 1 to 5 sentences. Write in the language of the question and address the person as "du" in German, "you" in English.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"sentences": [{"text": "...", "sources": ["n3", "n7"]}]}"#;

const CHECK_PROMPT: &str = r#"You check an answer that was written from a person's notes. For each sentence you get the notes it cites.

A sentence is supported only if the notes it cites state what it says — names, dates, numbers, who did what, and whether something is still the case. A sentence that adds anything those notes do not say, or presents as current what a later cited note contradicts, is not supported. Wording may differ; the facts may not.

Respond with ONLY this JSON object, no prose, no markdown fences:
{"supported": ["s1", "s3"]}"#;

/// Whether a date the planner returned is a date at all.
fn day(raw: Option<&str>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(raw?.trim(), "%Y-%m-%d").ok()
}

pub async fn ask(state: &AppState, request: &AskRequest) -> anyhow::Result<Answer> {
    let question: String = request
        .question
        .trim()
        .chars()
        .take(MAX_QUESTION_CHARS)
        .collect();
    let tz = crate::routes::captures::timezone_for(state, request.timezone.as_deref());
    let pool = &state.pool;

    let Some(model) = state.openrouter.as_ref() else {
        // No model: the notes are still worth having.
        let hits =
            crate::routes::search::hybrid(state, &question, None, None, None, NEAREST as i64)
                .await?;
        return Ok(Answer {
            sentences: Vec::new(),
            notes: hits
                .into_iter()
                .map(|h| AnswerNote {
                    capture_event_id: h.event_id,
                    transcript_text: h.transcript,
                    occurred_at: h.occurred_at,
                    cited: false,
                })
                .collect(),
            considered: 0,
            dropped: 0,
            model: None,
            can_answer: false,
        });
    };

    let today = Utc::now().with_timezone(&tz);
    let plan: Plan = model
        .complete_json(
            "ask.plan",
            PLAN_PROMPT,
            &format!(
                "Today is {} ({}).\n\nQuestion:\n{question}",
                today.format("%A, %-d %B %Y"),
                today.format("%Y-%m-%d")
            ),
        )
        .await
        .unwrap_or_else(|err| {
            // A plan is a help, not a precondition: without one the
            // question itself is searched for, unbounded.
            tracing::warn!(?err, "could not plan the search for a question");
            Plan::default()
        });

    let midnight = |d: NaiveDate| {
        tz.from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight exists"))
            .earliest()
            .map(|t| t.with_timezone(&Utc))
    };
    let from = day(plan.from.as_deref()).and_then(midnight);
    let to = day(plan.to.as_deref())
        .and_then(|d| d.succ_opt())
        .and_then(midnight);

    let search_text = if plan.search.trim().is_empty() {
        question.clone()
    } else {
        plan.search.trim().to_string()
    };
    let hits =
        crate::routes::search::hybrid(state, &search_text, from, to, None, SEARCH_HITS).await?;

    // The people and subjects it names, found the way a meeting finds
    // them: whole words, names and aliases, nothing fuzzier.
    let candidates = crate::routes::intentions::candidates(pool)
        .await
        .map_err(|err| anyhow::anyhow!("{err:?}"))?;
    let named: Vec<Uuid> = brief::find(&candidates, &question, &plan.names)
        .into_iter()
        .map(|m| m.id)
        .take(ENTITIES)
        .collect();

    let mut chosen: Vec<Uuid> = Vec::new();
    let add = |ids: Vec<Uuid>, chosen: &mut Vec<Uuid>| {
        for id in ids {
            if chosen.len() >= MAX_NOTES {
                break;
            }
            if !chosen.contains(&id) {
                chosen.push(id);
            }
        }
    };

    // Priority order, because the cap cuts from the end: what the search
    // found, what was said about what the question names, the meetings
    // with them, the notes the best hits continue, and last the graph's
    // next step out.
    add(hits.iter().map(|h| h.event_id).collect(), &mut chosen);
    add(entity_notes(pool, &named, from, to).await?, &mut chosen);
    add(occasion_notes(pool, &named).await?, &mut chosen);
    let seeds: Vec<Uuid> = hits
        .iter()
        .take(EPISODE_SEEDS)
        .map(|h| h.event_id)
        .collect();
    add(crate::context::neighbours(pool, &seeds).await?, &mut chosen);
    add(neighbour_notes(pool, &named, from, to).await?, &mut chosen);

    let nearest: Vec<Uuid> = hits.iter().take(NEAREST).map(|h| h.event_id).collect();
    if chosen.is_empty() {
        return Ok(Answer {
            sentences: Vec::new(),
            notes: Vec::new(),
            considered: 0,
            dropped: 0,
            model: Some(model.model_name().to_string()),
            can_answer: true,
        });
    }

    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        r#"
        select event_id, transcript, occurred_at
        from capture_search
        where event_id = any($1)
        order by occurred_at
        "#,
        &chosen,
    )
    .fetch_all(pool)
    .await?;

    let contexts = note_contexts(pool, &chosen).await?;
    let tag_of: HashMap<Uuid, String> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| (row.event_id, format!("n{}", i + 1)))
        .collect();

    let notes: Vec<Note> = rows
        .iter()
        .map(|row| {
            let mut context: Vec<String> = contexts
                .occasions
                .get(&row.event_id)
                .cloned()
                .unwrap_or_default();
            for previous in contexts.continues.get(&row.event_id).into_iter().flatten() {
                if let Some(tag) = tag_of.get(previous) {
                    context.push(format!("continues {tag}"));
                }
            }
            Note {
                tag: tag_of[&row.event_id].clone(),
                said: row
                    .occurred_at
                    .with_timezone(&tz)
                    .format("%a %d.%m.%Y %H:%M")
                    .to_string(),
                context,
                text: row.transcript.chars().take(NOTE_CHARS).collect(),
            }
        })
        .collect();
    let by_tag: HashMap<&str, Uuid> = notes
        .iter()
        .zip(&rows)
        .map(|(note, row)| (note.tag.as_str(), row.event_id))
        .collect();

    let mut user = format!("Today is {}.\n\n", today.format("%A, %-d %B %Y"));
    let names: HashMap<Uuid, &str> = candidates
        .iter()
        .map(|c| (c.id, c.names[0].as_str()))
        .collect();
    let gists = crate::gist::texts(pool, &named).await?;
    if !gists.is_empty() {
        user.push_str("Known about (orientation only, never cite):\n");
        for (id, text) in &gists {
            user.push_str(&format!(
                "- {}: {text}\n",
                names.get(id).copied().unwrap_or("?")
            ));
        }
        user.push('\n');
    }
    user.push_str(&format!(
        "Notes, oldest first:\n{}\n\nQuestion:\n{question}",
        serde_json::to_string_pretty(&notes)?
    ));

    let written: Written = model
        .complete_json("ask.answer", ANSWER_PROMPT, &user)
        .await?;
    let sourced = crate::routes::review::sourced(written.sentences, &by_tag, MAX_SENTENCES);

    let (sentences, dropped) = if sourced.is_empty() {
        (Vec::new(), 0)
    } else {
        check(model, &sourced, &rows_text(&rows, &tag_of)).await?
    };

    let answer_notes = cited_or_nearest(&sentences, &nearest, &rows);

    Ok(Answer {
        sentences,
        notes: answer_notes,
        considered: rows.len(),
        dropped,
        model: Some(model.model_name().to_string()),
        can_answer: true,
    })
}

/// One note the answer may be written from.
struct Row {
    event_id: Uuid,
    transcript: String,
    occurred_at: DateTime<Utc>,
}

/// Tag and text per note, for the second reading.
fn rows_text(rows: &[Row], tag_of: &HashMap<Uuid, String>) -> HashMap<Uuid, (String, String)> {
    rows.iter()
        .map(|row| {
            let text = row.transcript.chars().take(NOTE_CHARS).collect();
            (row.event_id, (tag_of[&row.event_id].clone(), text))
        })
        .collect()
}

/// The second reading: each sentence against exactly the notes it cites.
async fn check(
    model: &crate::openrouter::OpenRouterClient,
    sentences: &[StorySentence],
    notes: &HashMap<Uuid, (String, String)>,
) -> anyhow::Result<(Vec<StorySentence>, usize)> {
    let mut listing = String::new();
    for (i, sentence) in sentences.iter().enumerate() {
        listing.push_str(&format!("s{}: {}\n", i + 1, sentence.text));
        for source in &sentence.sources {
            if let Some((tag, text)) = notes.get(source) {
                listing.push_str(&format!("  {tag}: {text}\n"));
            }
        }
        listing.push('\n');
    }
    let checked: Checked = model
        .complete_json("ask.check", CHECK_PROMPT, &listing)
        .await?;
    Ok(keep_supported(sentences, &checked.supported))
}

/// The sentences the second reading vouched for, and how many it did not.
fn keep_supported(
    sentences: &[StorySentence],
    supported: &[String],
) -> (Vec<StorySentence>, usize) {
    let kept: Vec<StorySentence> = sentences
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            supported
                .iter()
                .any(|tag| tag.trim() == format!("s{}", i + 1))
        })
        .map(|(_, s)| s.clone())
        .collect();
    let dropped = sentences.len() - kept.len();
    (kept, dropped)
}

/// The notes to show under the answer: those cited, in order of first
/// citation — or, when nothing could be said, the nearest ones found.
fn cited_or_nearest(
    sentences: &[StorySentence],
    nearest: &[Uuid],
    rows: &[Row],
) -> Vec<AnswerNote> {
    let by_id: HashMap<Uuid, &Row> = rows.iter().map(|r| (r.event_id, r)).collect();
    let mut order: Vec<(Uuid, bool)> = Vec::new();
    for sentence in sentences {
        for source in &sentence.sources {
            if !order.iter().any(|(id, _)| id == source) {
                order.push((*source, true));
            }
        }
    }
    if order.is_empty() {
        order = nearest.iter().map(|id| (*id, false)).collect();
    }
    order
        .into_iter()
        .filter_map(|(id, cited)| {
            let row = by_id.get(&id)?;
            Some(AnswerNote {
                capture_event_id: id,
                transcript_text: row.transcript.clone(),
                occurred_at: row.occurred_at,
                cited,
            })
        })
        .collect()
}

/// What was said about the named things, newest first.
async fn entity_notes(
    pool: &PgPool,
    named: &[Uuid],
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if named.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"
        select event_id as "event_id!" from (
            select cs.event_id, cs.occurred_at,
                   row_number() over (partition by o.entity_id order by cs.occurred_at desc) as n
            from observations o
            join capture_search cs on cs.event_id = o.source_event_id
            where o.entity_id = any($1)
              and ($2::timestamptz is null or cs.occurred_at >= $2)
              and ($3::timestamptz is null or cs.occurred_at < $3)
        ) ranked
        where n <= $4
        order by occurred_at desc
        "#,
        named,
        from,
        to,
        NOTES_PER_ENTITY,
    )
    .fetch_all(pool)
    .await
}

/// Notes that belonged to a meeting with one of the named things.
async fn occasion_notes(pool: &PgPool, named: &[Uuid]) -> Result<Vec<Uuid>, sqlx::Error> {
    if named.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"
        select id as "id!" from (
            select distinct o.capture_event_id as id, cs.occurred_at
            from capture_occasions o
            join capture_occasion_entities oe on oe.occasion_id = o.id
            join entities start on start.id = oe.entity_id
            join capture_search cs on cs.event_id = o.capture_event_id
            where o.verdict = 'belongs'
              and coalesce(start.merged_into, start.id) = any($1)
        ) found
        order by occurred_at desc
        limit $2
        "#,
        named,
        OCCASION_NOTES,
    )
    .fetch_all(pool)
    .await
}

/// One step out along the graph: the things most often related to what
/// the question names, and the latest said about each.
async fn neighbour_notes(
    pool: &PgPool,
    named: &[Uuid],
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if named.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(
        r#"
        with near as (
            select other, count(*) as weight from (
                select to_entity_id as other from relations where from_entity_id = any($1)
                union all
                select from_entity_id from relations where to_entity_id = any($1)
            ) r
            where not (other = any($1))
            group by other
            order by weight desc
            limit $2
        )
        select event_id as "event_id!" from (
            select cs.event_id, cs.occurred_at,
                   row_number() over (partition by o.entity_id order by cs.occurred_at desc) as n
            from near
            join observations o on o.entity_id = near.other
            join capture_search cs on cs.event_id = o.source_event_id
            where ($3::timestamptz is null or cs.occurred_at >= $3)
              and ($4::timestamptz is null or cs.occurred_at < $4)
        ) ranked
        where n <= $5
        order by occurred_at desc
        "#,
        named,
        NEIGHBOURS,
        from,
        to,
        NOTES_PER_NEIGHBOUR,
    )
    .fetch_all(pool)
    .await
}

/// What each note was said next to, as the model is told it.
#[derive(Default)]
struct Contexts {
    occasions: HashMap<Uuid, Vec<String>>,
    continues: HashMap<Uuid, Vec<Uuid>>,
}

async fn note_contexts(pool: &PgPool, ids: &[Uuid]) -> Result<Contexts, sqlx::Error> {
    let mut out = Contexts::default();

    let meetings = sqlx::query!(
        r#"
        select o.capture_event_id, o.id, o.phase, o.minutes,
               array_agg(distinct e.name) as "names!"
        from capture_occasions o
        join capture_occasion_entities oe on oe.occasion_id = o.id
        join entities start on start.id = oe.entity_id
        join entities e on e.id = coalesce(start.merged_into, start.id)
        where o.verdict = 'belongs' and o.capture_event_id = any($1)
        group by o.capture_event_id, o.id, o.phase, o.minutes
        "#,
        ids,
    )
    .fetch_all(pool)
    .await?;
    for row in meetings {
        let with = row.names.join(", ");
        let line = match row.phase.as_str() {
            "before" => format!("{} min before a meeting with {with}", row.minutes),
            "after" => format!("{} min after a meeting with {with}", row.minutes),
            _ => format!("during a meeting with {with}"),
        };
        out.occasions
            .entry(row.capture_event_id)
            .or_default()
            .push(line);
    }

    let links = sqlx::query!(
        r#"
        select capture_event_id, previous_event_id
        from capture_continuations
        where continues and capture_event_id = any($1)
        "#,
        ids,
    )
    .fetch_all(pool)
    .await?;
    for row in links {
        out.continues
            .entry(row.capture_event_id)
            .or_default()
            .push(row.previous_event_id);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentence(text: &str) -> StorySentence {
        StorySentence {
            text: text.into(),
            sources: vec![Uuid::nil()],
        }
    }

    fn row(id: Uuid, text: &str) -> Row {
        Row {
            event_id: id,
            transcript: text.into(),
            occurred_at: Utc::now(),
        }
    }

    #[test]
    fn only_what_the_second_reading_vouched_for_is_kept() {
        let sentences = vec![sentence("a"), sentence("b"), sentence("c")];
        let (kept, dropped) =
            keep_supported(&sentences, &["s1".into(), " s3 ".into(), "s9".into()]);
        assert_eq!(
            kept.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["a", "c"]
        );
        assert_eq!(dropped, 1);
    }

    #[test]
    fn a_second_reading_that_vouches_for_nothing_leaves_nothing() {
        let (kept, dropped) = keep_supported(&[sentence("a")], &[]);
        assert!(kept.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn with_nothing_said_the_nearest_notes_are_shown_uncited() {
        let near = Uuid::new_v4();
        let notes = cited_or_nearest(&[], &[near, Uuid::new_v4()], &[row(near, "x")]);
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].cited);
    }

    #[test]
    fn cited_notes_come_in_the_order_they_are_first_cited() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let sentences = vec![
            StorySentence {
                text: "one".into(),
                sources: vec![b],
            },
            StorySentence {
                text: "two".into(),
                sources: vec![a, b],
            },
        ];
        let notes = cited_or_nearest(&sentences, &[], &[row(a, "a"), row(b, "b")]);
        assert_eq!(
            notes.iter().map(|n| n.capture_event_id).collect::<Vec<_>>(),
            vec![b, a]
        );
        assert!(notes.iter().all(|n| n.cited));
    }

    #[test]
    fn a_planned_date_that_is_not_one_is_ignored() {
        assert_eq!(
            day(Some("2026-09-14")),
            NaiveDate::from_ymd_opt(2026, 9, 14)
        );
        assert_eq!(day(Some("last week")), None);
        assert_eq!(day(None), None);
    }
}
