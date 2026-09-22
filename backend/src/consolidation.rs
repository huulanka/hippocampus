//! Making the graph say one thing once.
//!
//! The user, on the 22nd of September 2026:
//!
//! > „Ich erfasse einmal zum Beispiel Kaffee und ich erfasse einmal
//! > Espresso, da sind die beiden Knoten momentan getrennt … einmal nenne
//! > ich den Kollegen Paul, einmal nenne ich ihn Paul Hartmann."
//!
//! `structuring.rs` now prevents most of that at the source, by showing
//! the extraction what the graph already holds. This module deals with
//! the part prevention cannot reach: what is *already* fragmented, and
//! what drifts apart later anyway.
//!
//! ## Two passes, in this order
//!
//! **First, what needs no weighing up.** Two entities with the same name
//! under two different type words are one thing — `Sauna` as both `Ort`
//! and `Aktivität`, `Northwind` as both `Organisation` and `Kunde`.
//! Nothing is being judged there; the type vocabulary was simply invented
//! afresh per note. This runs whether or not a model is configured,
//! costs nothing, and is exact.
//!
//! **Then, what does.** A model reads a batch of entities together —
//! their names, their aliases and the sentences the person actually said
//! about them — and proposes merges, retypes and relations. This is the
//! part that can unify `Aufguss` with `Finnischer Aufguss`, collapse
//! `Kunde`/`Unternehmen`/`Organisation` onto one word without anyone
//! writing that mapping down, and draw the edge from `Kardamom-Espresso`
//! to `Espresso` instead of fusing them.
//!
//! The vocabulary is deliberately left open. The run is allowed to coin a
//! new type or a new relation word when nothing existing fits, because
//! the alternative — a fixed list decided once — stops describing a life
//! the moment that life changes.
//!
//! ## Why this is safe to do automatically
//!
//! Because it changes nothing the person said. The captures and their
//! transcripts are immutable (ADR 0003/0005); what this run rewrites is
//! only the *view* of them, and every step of that rewriting is an event.
//! `entity.merged`, `entity.retyped`, `relation.proposed` — each one
//! carries what it changed and why, each one is reversible, and together
//! they are a changelog of how the person's knowledge came to be
//! organised the way it is.
//!
//! The guards are in three places and all three are needed: the prompt
//! (what may be merged at all), this module (budgets, and a hard cap so
//! one bad pass cannot fold a graph), and the block list (a merge taken
//! back is never proposed again).
//!
//! ## But it does not, by default
//!
//! `CONSOLIDATION_ENABLED` defaults to off. Reversibility is not the same
//! as being noticed — a rearrangement made while nobody was looking is
//! still a surprise the moment it is found — so the run instead waits to
//! be asked: `preview` shows what it would do without doing any of it, and
//! `apply_selected` carries out exactly that judgement, minus whatever a
//! person removed from it. Nothing here runs unattended unless the
//! variable is explicitly switched on.

use std::time::Duration;

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::events;
use contracts::{ConsolidationPreview, PreviewMerge, PreviewRelation, PreviewRetype};

use crate::openrouter::{ConsolidationPlan, EntityDossier};

/// One entity folded into another.
#[derive(Debug, Clone)]
pub struct Merge {
    pub target_id: Uuid,
    pub target_name: String,
    pub source_id: Uuid,
    pub source_name: String,
    pub source_type: String,
    pub reason: String,
    /// What the survivor should be called afterwards, when the plan wants
    /// to rename it too. Carried on the merge itself rather than applied
    /// separately, so a stored plan needs nothing beyond its own list of
    /// merges to be replayed later.
    pub new_name: Option<String>,
}

/// One entity's type word changed for the same reason a merge would have
/// been wrong: the vocabulary drifted, not the thing.
#[derive(Debug, Clone)]
pub struct PlannedRetype {
    pub entity_id: Uuid,
    pub entity_name: String,
    pub from_type: String,
    pub to_type: String,
    pub reason: String,
}

/// One edge the run wants to draw between two entities read together.
#[derive(Debug, Clone)]
pub struct PlannedRelation {
    pub from_id: Uuid,
    pub from_name: String,
    pub to_id: Uuid,
    pub to_name: String,
    pub relation_type: String,
    pub reason: String,
}

/// A judged plan, validated and resolved to entity ids.
///
/// The output of [`resolve_plan`]: every guard (crossed types, a blocked
/// pair, the per-pass merge cap) has already been applied, so what is left
/// is safe to execute directly — either all of it, immediately
/// ([`run_once`]), or a person's chosen subset of it, later
/// ([`apply_selected`]).
#[derive(Debug, Clone, Default)]
pub struct ResolvedPlan {
    pub merges: Vec<Merge>,
    pub retypes: Vec<PlannedRetype>,
    pub relations: Vec<PlannedRelation>,
}

/// A preview, kept exactly as shown so that applying part of it later
/// replays that same judgement rather than asking the model again.
#[derive(Debug, Clone, Default)]
pub struct StoredPlan {
    pub same_name: Vec<Merge>,
    pub merges: Vec<Merge>,
    pub retypes: Vec<PlannedRetype>,
    pub relations: Vec<PlannedRelation>,
}

/// The most recent preview, waiting to be applied. One slot, not a map: this
/// is a single-user local app, and a second preview is meant to replace the
/// first rather than accumulate beside it.
pub type PreviewStore = std::sync::Mutex<Option<(Uuid, StoredPlan)>>;

/// Pairs that are the same thing by their name alone.
///
/// Case-insensitive, live entities only, and never a pair that has been
/// blocked. The survivor is the one carrying more observations — the row
/// that is actually being used — with the older row winning a tie, so the
/// outcome does not depend on which order the query happened to return.
///
/// This finds duplicates that *differ only by type*, which is the whole
/// point: an exact name match within the same type cannot exist, because
/// `resolve_existing` would have reused it.
pub async fn same_name_duplicates(pool: &PgPool) -> anyhow::Result<Vec<Merge>> {
    let rows = sqlx::query!(
        r#"
        with live as (
            select e.id, e.name, e.entity_type, e.created_at,
                   (select count(*) from observations o where o.entity_id = e.id) as "weight!"
            from entities e
            where e.merged_into is null
        ),
        ranked as (
            select *,
                   row_number() over (
                       partition by lower(name)
                       order by "weight!" desc, created_at
                   ) as rank
            from live
        )
        select
            keep.id as "target_id!", keep.name as "target_name!",
            drop_it.id as "source_id!", drop_it.name as "source_name!",
            drop_it.entity_type as "source_type!"
        from ranked keep
        join ranked drop_it
          on lower(drop_it.name) = lower(keep.name) and drop_it.rank > 1
        where keep.rank = 1
          and not exists (
            select 1 from entity_merge_block b
            where b.lower_id = least(keep.id, drop_it.id)
              and b.higher_id = greatest(keep.id, drop_it.id)
          )
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| Merge {
            target_id: row.target_id,
            target_name: row.target_name,
            source_id: row.source_id,
            source_name: row.source_name,
            source_type: row.source_type,
            reason: "same name".to_string(),
            new_name: None,
        })
        .collect())
}

/// Folds `source` into `target`.
///
/// Everything in one transaction, because a half-merged entity is worse
/// than either state: its observations would hang off a node that the
/// rest of the system no longer shows.
///
/// The source row is kept and pointed at the target. That is what makes
/// [`unmerge`] a matter of clearing a field rather than a restore from
/// backup — and it is why `entity.merged` can carry everything needed to
/// undo it.
pub async fn merge(pool: &PgPool, merge: &Merge, actor: &str) -> anyhow::Result<()> {
    if merge.target_id == merge.source_id {
        anyhow::bail!("an entity cannot be merged into itself");
    }

    let mut tx = pool.begin().await?;

    // Observations move wholesale: each one is a sentence about a thing,
    // and the thing is now the target.
    sqlx::query!(
        r#"update observations set entity_id = $1 where entity_id = $2"#,
        merge.target_id,
        merge.source_id,
    )
    .execute(&mut *tx)
    .await?;

    // Relations are redirected on both ends, and then any that have
    // become a self-loop are dropped: "Northwind (Kunde) gehört zu
    // Northwind (Organisation)" was only ever a statement about the
    // split, and it stops being true the moment the split does.
    sqlx::query!(
        r#"update relations set from_entity_id = $1 where from_entity_id = $2"#,
        merge.target_id,
        merge.source_id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        r#"update relations set to_entity_id = $1 where to_entity_id = $2"#,
        merge.target_id,
        merge.source_id,
    )
    .execute(&mut *tx)
    .await?;
    let self_loops = sqlx::query!(
        r#"delete from relations where from_entity_id = to_entity_id and from_entity_id = $1"#,
        merge.target_id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    // Every name the source answered to becomes a name the target
    // answers to. Without this the merge would make the source's wording
    // unfindable, which is the failure mode that makes people distrust
    // an automatic tidy-up.
    sqlx::query!(
        r#"
        insert into entity_alias (entity_id, name, source)
        select $1, a.name, 'merge' from entity_alias a where a.entity_id = $2
        on conflict (entity_id, name) do nothing
        "#,
        merge.target_id,
        merge.source_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"update entities set merged_into = $1, updated_at = now() where id = $2"#,
        merge.target_id,
        merge.source_id,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        merge.target_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    // The payload carries everything the undo needs, so reversing a merge
    // never has to reconstruct what was true beforehand.
    events::append_tx(
        &mut tx,
        merge.target_id,
        version,
        "entity.merged",
        &json!({
            "source_entity_id": merge.source_id,
            "source_name": merge.source_name,
            "source_entity_type": merge.source_type,
            "target_name": merge.target_name,
            "reason": merge.reason,
            "self_loops_removed": self_loops,
        }),
        actor,
    )
    .await?;

    tx.commit().await?;

    tracing::info!(
        target = %merge.target_name,
        source = %merge.source_name,
        source_type = %merge.source_type,
        reason = %merge.reason,
        "merged two entities that were the same thing"
    );

    Ok(())
}

/// Takes a merge back, and records that this pair is not to be merged
/// again.
///
/// The block is the point. Without it the next run would look at the same
/// two rows, reach the same conclusion, and undo the undo — the
/// oscillation `docs/consolidation.md` warns about.
pub async fn unmerge(pool: &PgPool, source_id: Uuid, actor: &str) -> anyhow::Result<bool> {
    let Some(row) = sqlx::query!(
        r#"select merged_into, name from entities where id = $1 and merged_into is not null"#,
        source_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    let target_id = row.merged_into.expect("filtered on not null");

    let mut tx = pool.begin().await?;

    // Observations go back to the entity that sourced them. The link is
    // `source_event_id` plus the event log: an observation belongs to
    // whichever entity the extraction that produced it named, and that is
    // recorded on the `entity.observed` event.
    sqlx::query!(
        r#"
        update observations o
        set entity_id = $1
        where o.entity_id = $2
          and exists (
            select 1 from events ev
            where ev.stream_id = $1
              and ev.event_type = 'entity.observed'
              and (ev.payload ->> 'source_event_id')::uuid = o.source_event_id
          )
        "#,
        source_id,
        target_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"delete from entity_alias where entity_id = $1 and source = 'merge'
           and name in (select name from entity_alias where entity_id = $2)"#,
        target_id,
        source_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"update entities set merged_into = null, updated_at = now() where id = $1"#,
        source_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        -- Cast so `least`/`greatest` stay uuid: without it Postgres
        -- resolves the pair through text and the insert type-mismatches.
        insert into entity_merge_block (lower_id, higher_id, reason)
        values (least($1::uuid, $2::uuid), greatest($1::uuid, $2::uuid), 'unmerged')
        on conflict (lower_id, higher_id) do update set reason = 'unmerged', decided_at = now()
        "#,
        source_id,
        target_id,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        target_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    events::append_tx(
        &mut tx,
        target_id,
        version,
        "entity.unmerged",
        &json!({ "source_entity_id": source_id, "source_name": row.name }),
        actor,
    )
    .await?;

    tx.commit().await?;
    tracing::info!(%source_id, "took a merge back; this pair will not be proposed again");

    Ok(true)
}

/// Changes the type of one entity, recording what it used to be.
///
/// The old type lives in the event payload, which is what makes this
/// reversible and what makes the changelog readable: "Northwind was a
/// Kunde, it is now an Organisation, because …".
pub async fn retype(
    pool: &PgPool,
    entity_id: Uuid,
    new_type: &str,
    reason: &str,
    actor: &str,
) -> anyhow::Result<bool> {
    let Some(row) = sqlx::query!(
        r#"select name, entity_type from entities where id = $1 and merged_into is null"#,
        entity_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    if row.entity_type == new_type {
        return Ok(false);
    }

    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"insert into entity_type_registry (entity_type) values ($1) on conflict do nothing"#,
        new_type,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"update entities set entity_type = $2, updated_at = now() where id = $1"#,
        entity_id,
        new_type,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        entity_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    events::append_tx(
        &mut tx,
        entity_id,
        version,
        "entity.retyped",
        &json!({
            "name": row.name,
            "from": row.entity_type,
            "to": new_type,
            "reason": reason,
        }),
        actor,
    )
    .await?;

    tx.commit().await?;
    tracing::info!(name = %row.name, from = %row.entity_type, to = %new_type, "retyped");
    Ok(true)
}

/// Records an edge the run proposes between two entities.
///
/// `source_event_id` is null here, and that is the point: unlike every
/// other relation in the graph, this one does not come from a single
/// capture. It comes from reading several of them together, which is
/// exactly the thing that was impossible before — `structuring.rs` could
/// only ever link two things that appeared in one sentence, which is why
/// the graph was a set of unconnected stars.
pub async fn relate(
    pool: &PgPool,
    from_id: Uuid,
    to_id: Uuid,
    relation_type: &str,
    reason: &str,
    actor: &str,
) -> anyhow::Result<bool> {
    if from_id == to_id {
        return Ok(false);
    }

    let exists = sqlx::query_scalar!(
        r#"
        select 1 from relations
        where from_entity_id = $1 and to_entity_id = $2 and relation_type = $3
        "#,
        from_id,
        to_id,
        relation_type,
    )
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
        return Ok(false);
    }
    if is_relation_blocked(pool, from_id, to_id, relation_type).await? {
        tracing::info!(
            %from_id, %to_id, %relation_type,
            "this edge was taken back before; leaving it alone"
        );
        return Ok(false);
    }

    let mut tx = pool.begin().await?;

    sqlx::query!(
        r#"
        insert into relations (from_entity_id, to_entity_id, relation_type, model)
        values ($1, $2, $3, $4)
        "#,
        from_id,
        to_id,
        relation_type,
        actor,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        from_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    events::append_tx(
        &mut tx,
        from_id,
        version,
        "relation.proposed",
        &json!({
            "to_entity_id": to_id,
            "relation_type": relation_type,
            "reason": reason,
            "across_captures": true,
        }),
        actor,
    )
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Takes a derived edge back, and records that this triple is not to be
/// proposed again.
///
/// Unlike a merge, there is nothing to give back later — a relation was
/// never an observation of its own, it was a reading of two others — so
/// this deletes the row outright rather than pointing it elsewhere. The
/// block is what makes that safe: without it, the next pass reads the same
/// two entities and redraws the exact edge just taken back.
pub async fn retract_relation(
    pool: &PgPool,
    relation_id: Uuid,
    actor: &str,
) -> anyhow::Result<bool> {
    let Some(row) = sqlx::query!(
        r#"select from_entity_id, to_entity_id, relation_type from relations where id = $1"#,
        relation_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };

    let mut tx = pool.begin().await?;

    sqlx::query!(r#"delete from relations where id = $1"#, relation_id)
        .execute(&mut *tx)
        .await?;

    sqlx::query!(
        r#"
        insert into relation_block (from_entity_id, to_entity_id, relation_type)
        values ($1, $2, $3)
        on conflict (from_entity_id, to_entity_id, relation_type)
        do update set decided_at = now()
        "#,
        row.from_entity_id,
        row.to_entity_id,
        row.relation_type,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        row.from_entity_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    events::append_tx(
        &mut tx,
        row.from_entity_id,
        version,
        "relation.retracted",
        &json!({
            "to_entity_id": row.to_entity_id,
            "relation_type": row.relation_type,
        }),
        actor,
    )
    .await?;

    tx.commit().await?;
    tracing::info!(%relation_id, "took a relation back; this pair will not be proposed again");

    Ok(true)
}

/// What one pass of the run did.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct RunReport {
    /// Folded together because the names were identical. No model
    /// involved.
    pub merged_by_name: usize,
    /// Folded together because a model read both and said so.
    pub merged_by_judgement: usize,
    pub retyped: usize,
    pub related: usize,
    pub examined: usize,
    /// How many entities the model was shown. Zero when no model is
    /// configured, or when nothing was due.
    pub considered: usize,
}

/// How long before an entity is looked at again even though nothing has
/// happened to it.
///
/// This is what makes the run *cyclical* instead of merely reactive. A
/// thing that stopped being mentioned is exactly the thing whose
/// duplicate will never be noticed by watching for changes — it comes
/// round again on its own.
///
/// Without it the run would settle into doing nothing: once every entity
/// carries a timestamp, "the oldest thirty" is still thirty entities, and
/// the pass would buy a model call every hour forever to be told that a
/// graph nobody has touched is unchanged.
const STALE_AFTER_DAYS: i32 = 30;

/// How many entities are shown to the model in one pass.
///
/// Bounded by the prompt, not by taste: every entity carries its
/// observations, so this is the number that decides what one call costs.
/// It is also the number that decides how much the model can *see at
/// once*, which is the whole reason this works — a duplicate is only
/// findable when both halves are in front of it.
const JUDGE_BUDGET: usize = 60;

/// The most entities one pass may fold away.
///
/// A backstop against the prompt failing in a way nobody anticipated. If
/// a single pass wants to merge more than this, something is wrong with
/// the answer rather than with the graph, and the right response is to
/// keep the graph and lose the pass. Reversibility is not a substitute
/// for this: undoing forty bad merges one at a time is not a remedy
/// anyone would use.
const MAX_MERGES_PER_PASS: usize = 12;

/// The entities a pass looks at, with everything the model needs to judge
/// them.
///
/// Two groups, and the second is what makes it work. The first is what is
/// *due*: never examined, or examined longest ago — that is the cyclical
/// part, and `last_consolidated_at` is what makes it possible to say so.
/// The second is each due entity's *neighbourhood*: whatever resembles it
/// by name or sits near it in embedding space. Without that, a pass would
/// show the model sixty unrelated things and ask it to find duplicates
/// among them, which is sixty tokens per entity spent on a question whose
/// answer is obviously "none".
async fn working_set(pool: &PgPool) -> anyhow::Result<Vec<(Uuid, EntityDossier)>> {
    let due = sqlx::query_scalar!(
        r#"
        select id from entities
        where merged_into is null
          and (
            -- Never looked at.
            last_consolidated_at is null
            -- Or something has been said about it since it was.
            or updated_at > last_consolidated_at
            -- Or it has simply gone stale: the part that keeps the run
            -- cyclical rather than reactive, so nothing sits unexamined
            -- forever merely because it stopped being mentioned.
            or last_consolidated_at < now() - make_interval(days => $2)
          )
        order by last_consolidated_at nulls first, updated_at desc
        limit $1
        "#,
        (JUDGE_BUDGET / 2) as i64,
        STALE_AFTER_DAYS,
    )
    .fetch_all(pool)
    .await?;

    if due.is_empty() {
        return Ok(Vec::new());
    }

    let rows = sqlx::query!(
        r#"
        with due as (select unnest($1::uuid[]) as id),
        neighbourhood as (
            select d.id from due d
            union
            -- Resembles a due entity by name, or sits near it in
            -- embedding space. Both, because they fail differently:
            -- trigrams miss "Sauna" next to "Aufguss", embeddings miss a
            -- typo in a proper noun.
            select n.id
            from due d
            join entities anchor on anchor.id = d.id
            join lateral (
                select e.id
                from entities e
                where e.merged_into is null and e.id <> anchor.id
                  and (
                    similarity(e.name, anchor.name) > 0.3
                    or (e.embedding is not null and anchor.embedding is not null
                        and e.embedding <=> anchor.embedding < 0.25)
                  )
                order by e.embedding <=> anchor.embedding
                limit 3
            ) n on true
        )
        select
            e.id,
            e.name,
            e.entity_type,
            coalesce(
                (select array_agg(a.name) from entity_alias a
                 where a.entity_id = e.id and a.name <> e.name),
                '{}'
            ) as "aliases!: Vec<String>",
            coalesce(
                (select array_agg(o.text order by o.created_at)
                 from observations o where o.entity_id = e.id),
                '{}'
            ) as "observations!: Vec<String>"
        from neighbourhood nb
        join entities e on e.id = nb.id
        where e.merged_into is null
        order by e.entity_type, e.name
        limit $2
        "#,
        &due,
        JUDGE_BUDGET as i64,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            (
                row.id,
                EntityDossier {
                    // A short tag rather than the UUID: models copy `e12`
                    // reliably and 36 hex characters unreliably, and a
                    // mistyped id here would act on the wrong entity.
                    tag: format!("e{index}"),
                    name: row.name,
                    entity_type: row.entity_type,
                    aliases: row.aliases,
                    observations: row.observations,
                },
            )
        })
        .collect())
}

/// Validates what the model proposed against every guard, and resolves it
/// to entity ids — without touching the database.
///
/// Every check here exists because the prompt cannot be relied on alone: a
/// tag that does not resolve, a merge of something into itself, a pair the
/// user has already pulled apart, a pass that wants to fold half the
/// graph. None of these should happen; all of them are cheap to refuse.
///
/// Split from execution ([`execute_resolved`]) so the same judgement can
/// either run immediately (the automatic pass) or be shown to a person and
/// applied later, minus whatever they removed (the preview). Read-only
/// database calls are still needed here — the block list, and following a
/// merge pointer — which is why this is `async` despite writing nothing.
async fn resolve_plan(
    pool: &PgPool,
    plan: &ConsolidationPlan,
    by_tag: &std::collections::HashMap<String, (Uuid, String, String)>,
) -> anyhow::Result<ResolvedPlan> {
    let mut resolved = ResolvedPlan::default();

    let total_merges: usize = plan.merges.iter().map(|m| m.absorb.len()).sum();
    if total_merges > MAX_MERGES_PER_PASS {
        tracing::error!(
            proposed = total_merges,
            cap = MAX_MERGES_PER_PASS,
            "consolidation proposed more merges than one pass may make; discarding all of them"
        );
    } else {
        for planned in &plan.merges {
            let Some((target_id, target_name, target_type)) = by_tag.get(&planned.keep) else {
                tracing::warn!(tag = %planned.keep, "merge names a tag that is not in this pass");
                continue;
            };
            let new_name = planned
                .name
                .as_deref()
                .map(|n| n.trim())
                .filter(|n| !n.is_empty() && *n != target_name)
                .map(|n| n.to_string());

            for absorbed in &planned.absorb {
                let Some((source_id, source_name, source_type)) = by_tag.get(absorbed) else {
                    tracing::warn!(tag = %absorbed, "merge names a tag that is not in this pass");
                    continue;
                };
                if source_id == target_id {
                    continue;
                }
                if crosses_types(target_name, source_name, source_type, target_type) {
                    tracing::warn!(
                        keep = %target_name,
                        absorb = %source_name,
                        keep_type = %target_type,
                        absorb_type = %source_type,
                        reason = %planned.reason,
                        "refusing a merge of two differently-named things of different types"
                    );
                    continue;
                }
                if is_blocked(pool, *target_id, *source_id).await? {
                    tracing::info!(
                        target = %target_name,
                        source = %source_name,
                        "this pair was pulled apart before; leaving it alone"
                    );
                    continue;
                }

                resolved.merges.push(Merge {
                    target_id: *target_id,
                    target_name: target_name.clone(),
                    source_id: *source_id,
                    source_name: source_name.clone(),
                    source_type: source_type.clone(),
                    reason: planned.reason.clone(),
                    new_name: new_name.clone(),
                });
            }
        }
    }

    for planned in &plan.retypes {
        let Some((id, name, from_type)) = by_tag.get(&planned.tag) else {
            continue;
        };
        if *from_type == planned.entity_type {
            continue;
        }
        resolved.retypes.push(PlannedRetype {
            entity_id: *id,
            entity_name: name.clone(),
            from_type: from_type.clone(),
            to_type: planned.entity_type.clone(),
            reason: planned.reason.clone(),
        });
    }

    for planned in &plan.relations {
        let (Some((from_id, from_name, _)), Some((to_id, to_name, _))) =
            (by_tag.get(&planned.from), by_tag.get(&planned.to))
        else {
            continue;
        };
        // Resolved through the merge pointer: a relation naming an entity
        // that this very pass folded away belongs on its survivor.
        let from_id = live_id(pool, *from_id).await?;
        let to_id = live_id(pool, *to_id).await?;
        if from_id == to_id {
            continue;
        }
        if is_relation_blocked(pool, from_id, to_id, &planned.relation_type).await? {
            continue;
        }
        resolved.relations.push(PlannedRelation {
            from_id,
            from_name: from_name.clone(),
            to_id,
            to_name: to_name.clone(),
            relation_type: planned.relation_type.clone(),
            reason: planned.reason.clone(),
        });
    }

    Ok(resolved)
}

/// Carries out a resolved plan, skipping whatever item ids appear in
/// `exclude`. Ids follow the same `"merges:{i}"` / `"retypes:{i}"` /
/// `"relations:{i}"` scheme `preview` hands to the client, so a person's
/// removals apply to exactly the items they saw.
async fn execute_resolved(
    pool: &PgPool,
    resolved: &ResolvedPlan,
    exclude: &std::collections::HashSet<String>,
    actor: &str,
    report: &mut RunReport,
) -> anyhow::Result<()> {
    // Collected rather than applied inline: several merges can share one
    // `keep`, and renaming once per unique survivor (after all of them
    // landed) avoids emitting a redundant `entity.renamed` per absorb.
    let mut renames: std::collections::HashMap<Uuid, String> = std::collections::HashMap::new();

    for (i, candidate) in resolved.merges.iter().enumerate() {
        if exclude.contains(&format!("merges:{i}")) {
            continue;
        }
        match merge(pool, candidate, actor).await {
            Ok(()) => {
                report.merged_by_judgement += 1;
                if let Some(name) = &candidate.new_name {
                    renames.insert(candidate.target_id, name.clone());
                }
            }
            Err(err) => tracing::warn!(?err, "could not apply a proposed merge"),
        }
    }
    for (target_id, name) in renames {
        if let Err(err) = rename(pool, target_id, &name, actor).await {
            tracing::warn!(?err, %name, "could not rename a merged entity");
        }
    }

    for (i, planned) in resolved.retypes.iter().enumerate() {
        if exclude.contains(&format!("retypes:{i}")) {
            continue;
        }
        match retype(
            pool,
            planned.entity_id,
            &planned.to_type,
            &planned.reason,
            actor,
        )
        .await
        {
            Ok(true) => report.retyped += 1,
            Ok(false) => {}
            Err(err) => tracing::warn!(?err, "could not retype"),
        }
    }

    for (i, planned) in resolved.relations.iter().enumerate() {
        if exclude.contains(&format!("relations:{i}")) {
            continue;
        }
        // Followed again rather than trusted from resolve time: an
        // excluded merge above can leave a relation's id stale between
        // resolving the plan and executing it.
        let from_id = live_id(pool, planned.from_id).await?;
        let to_id = live_id(pool, planned.to_id).await?;
        match relate(
            pool,
            from_id,
            to_id,
            &planned.relation_type,
            &planned.reason,
            actor,
        )
        .await
        {
            Ok(true) => report.related += 1,
            Ok(false) => {}
            Err(err) => tracing::warn!(?err, "could not add a proposed relation"),
        }
    }

    Ok(())
}

/// Whether a proposed merge is the shape that is usually a mistake.
///
/// Two entities of different types whose names are also different is the
/// combination that means "these belong together", not "these are one
/// thing" — and the model reaches for it. Observed on the real graph the
/// first time this ran: it proposed folding `Northwind
/// Abrechnungsprojekt` (Projekt) into `Northwind` (Organisation), with
/// the reason "beschreiben dasselbe Projekt beim Kunden", which is an
/// argument for an edge and against a merge in the same sentence. The
/// prompt forbids exactly that and it happened anyway, which is the
/// whole case for having this check in code as well.
///
/// The two legitimate shapes survive:
///
/// * same name, different type — `Sauna` as `Ort` and as `Aktivität`.
///   This is the type vocabulary having drifted, nothing more.
/// * different name, same type — `Hippocampus` and `Hippocampus
///   Projekt`, `Paul` and `Paul Hartmann`. This is a name having
///   drifted.
///
/// A genuine merge that needs both to change is refused here. That costs
/// one pass: the run can retype first and merge on the next round.
fn crosses_types(
    target_name: &str,
    source_name: &str,
    source_type: &str,
    target_type: &str,
) -> bool {
    let same_name = target_name.trim().eq_ignore_ascii_case(source_name.trim());
    let same_type = target_type.eq_ignore_ascii_case(source_type);
    !same_name && !same_type
}

/// Where an entity ended up, following the merge pointer.
async fn live_id(pool: &PgPool, id: Uuid) -> anyhow::Result<Uuid> {
    Ok(sqlx::query_scalar!(
        r#"select coalesce(merged_into, id) as "id!" from entities where id = $1"#,
        id,
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(id))
}

async fn is_blocked(pool: &PgPool, a: Uuid, b: Uuid) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar!(
        r#"
        select 1 from entity_merge_block
        where lower_id = least($1::uuid, $2::uuid) and higher_id = greatest($1::uuid, $2::uuid)
        "#,
        a,
        b,
    )
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// Whether this exact directed edge was taken back before. Directed and
/// typed, unlike [`is_blocked`]: "unfairly close to" and "close to" are
/// different claims, and retracting one must not silence the other.
async fn is_relation_blocked(
    pool: &PgPool,
    from_id: Uuid,
    to_id: Uuid,
    relation_type: &str,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar!(
        r#"
        select 1 from relation_block
        where from_entity_id = $1 and to_entity_id = $2 and relation_type = $3
        "#,
        from_id,
        to_id,
        relation_type,
    )
    .fetch_optional(pool)
    .await?
    .is_some())
}

/// Gives a merged entity the fuller name, keeping the old one findable.
async fn rename(pool: &PgPool, id: Uuid, name: &str, actor: &str) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    let previous = sqlx::query_scalar!(r#"select name from entities where id = $1"#, id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        r#"insert into entity_alias (entity_id, name, source) values ($1, $2, 'merge')
           on conflict (entity_id, name) do nothing"#,
        id,
        previous,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"update entities set name = $2, updated_at = now() where id = $1"#,
        id,
        name,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"insert into entity_alias (entity_id, name, source) values ($1, $2, 'observed')
           on conflict (entity_id, name) do nothing"#,
        id,
        name,
    )
    .execute(&mut *tx)
    .await?;

    let version = sqlx::query_scalar!(
        r#"select coalesce(max(version), 0) + 1 as "next!" from events where stream_id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    events::append_tx(
        &mut tx,
        id,
        version,
        "entity.renamed",
        &json!({ "from": previous, "to": name }),
        actor,
    )
    .await?;

    tx.commit().await?;
    Ok(())
}

/// One pass.
///
/// Exact first, then judged. The order matters: folding the obvious
/// duplicates away before the model sees the batch means it is not shown
/// the same thing twice and asked whether it is the same thing.
pub async fn run_once(state: &AppState, actor: &str) -> anyhow::Result<RunReport> {
    let pool = &state.pool;
    let mut report = RunReport::default();

    for candidate in same_name_duplicates(pool).await? {
        match merge(pool, &candidate, actor).await {
            Ok(()) => report.merged_by_name += 1,
            Err(err) => tracing::warn!(
                ?err,
                target = %candidate.target_name,
                "could not merge these two"
            ),
        }
    }

    let mut examined_ids: Vec<Uuid> = Vec::new();

    if let Some(client) = state.openrouter.clone() {
        let batch = working_set(pool).await?;
        report.considered = batch.len();
        examined_ids = batch.iter().map(|(id, _)| *id).collect();

        if batch.len() >= 2 {
            let by_tag: std::collections::HashMap<String, (Uuid, String, String)> = batch
                .iter()
                .map(|(id, dossier)| {
                    (
                        dossier.tag.clone(),
                        (*id, dossier.name.clone(), dossier.entity_type.clone()),
                    )
                })
                .collect();
            let dossiers: Vec<EntityDossier> =
                batch.into_iter().map(|(_, dossier)| dossier).collect();

            match client.consolidate(&dossiers).await {
                Ok(plan) => {
                    let resolved = resolve_plan(pool, &plan, &by_tag).await?;
                    execute_resolved(
                        pool,
                        &resolved,
                        &std::collections::HashSet::new(),
                        actor,
                        &mut report,
                    )
                    .await?;
                }
                // Not fatal and not retried here: the pass runs again on
                // the next tick, and the entities it did not reach keep
                // their old `last_consolidated_at`, so they stay at the
                // front of the queue.
                Err(err) => tracing::warn!(?err, "the consolidation model call failed"),
            }
        }
    }

    // Exactly the entities that were shown to the model, and no others.
    //
    // This used to mark a flat two hundred per pass while the model saw
    // thirty, which recorded an examination that had not happened — and
    // then hid those entities from the next pass for a month. The marking
    // is what the run's own queue is built on, so a marking that lies is
    // a queue that skips.
    if !examined_ids.is_empty() {
        sqlx::query!(
            r#"update entities set last_consolidated_at = now() where id = any($1)"#,
            &examined_ids,
        )
        .execute(pool)
        .await?;
    }
    report.examined = examined_ids.len();

    let changed =
        report.merged_by_name + report.merged_by_judgement + report.retyped + report.related;
    if changed > 0 {
        events::append(
            pool,
            Uuid::new_v4(),
            1,
            "consolidation.ran",
            &json!({
                "merged_by_name": report.merged_by_name,
                "merged_by_judgement": report.merged_by_judgement,
                "retyped": report.retyped,
                "related": report.related,
                "considered": report.considered,
                "examined": report.examined,
            }),
            actor,
        )
        .await?;
    }

    Ok(report)
}

/// A `Merge` as the thing a person reads and can remove.
fn preview_merge(id: String, m: &Merge) -> PreviewMerge {
    PreviewMerge {
        id,
        keep: m.target_name.clone(),
        absorb: vec![format!("{} ({})", m.source_name, m.source_type)],
        new_name: m.new_name.clone(),
        reason: m.reason.clone(),
    }
}

/// What a pass *would* do, without doing any of it.
///
/// Exists for two reasons now. First, the original one: the database this
/// first ran against in earnest was not the one it was developed against,
/// and being able to look before acting is the difference between an
/// automatic tidy-up and an automatic surprise. Second, and now the more
/// important one: the run is no longer automatic at all
/// (`CONSOLIDATION_ENABLED` defaults to off) — this is how a person starts
/// a pass, by reading exactly what it found before any of it happens.
///
/// What is proposed here is also *stored*, under the token this returns,
/// so that `POST /consolidation/apply` replays this exact judgement minus
/// whatever was removed rather than asking the model a second time and
/// risking a different answer.
pub async fn preview(state: &AppState) -> anyhow::Result<ConsolidationPreview> {
    let pool = &state.pool;

    let same_name = same_name_duplicates(pool).await?;
    let mut stored = StoredPlan {
        same_name: same_name.clone(),
        ..Default::default()
    };

    let mut out = ConsolidationPreview {
        same_name: same_name
            .iter()
            .enumerate()
            .map(|(i, m)| preview_merge(format!("same_name:{i}"), m))
            .collect(),
        ..Default::default()
    };

    let batch = working_set(pool).await?;
    out.considered = batch.len() as i64;

    if let Some(client) = state.openrouter.clone() {
        if batch.len() >= 2 {
            let by_tag: std::collections::HashMap<String, (Uuid, String, String)> = batch
                .iter()
                .map(|(id, dossier)| {
                    (
                        dossier.tag.clone(),
                        (*id, dossier.name.clone(), dossier.entity_type.clone()),
                    )
                })
                .collect();
            let dossiers: Vec<EntityDossier> =
                batch.into_iter().map(|(_, dossier)| dossier).collect();

            let plan = client.consolidate(&dossiers).await?;
            let resolved = resolve_plan(pool, &plan, &by_tag).await?;

            out.merges = resolved
                .merges
                .iter()
                .enumerate()
                .map(|(i, m)| preview_merge(format!("merges:{i}"), m))
                .collect();
            out.retypes = resolved
                .retypes
                .iter()
                .enumerate()
                .map(|(i, r)| PreviewRetype {
                    id: format!("retypes:{i}"),
                    entity: r.entity_name.clone(),
                    from: r.from_type.clone(),
                    to: r.to_type.clone(),
                    reason: r.reason.clone(),
                })
                .collect();
            out.relations = resolved
                .relations
                .iter()
                .enumerate()
                .map(|(i, r)| PreviewRelation {
                    id: format!("relations:{i}"),
                    from: r.from_name.clone(),
                    to: r.to_name.clone(),
                    relation_type: r.relation_type.clone(),
                    reason: r.reason.clone(),
                })
                .collect();

            stored.merges = resolved.merges;
            stored.retypes = resolved.retypes;
            stored.relations = resolved.relations;
        } else {
            out.note = Some("Nothing is due for consolidation right now.".into());
        }
    } else {
        out.note = Some("No structuring model configured — only exact-name merges run.".into());
    }

    let has_anything = !stored.same_name.is_empty()
        || !stored.merges.is_empty()
        || !stored.retypes.is_empty()
        || !stored.relations.is_empty();
    if has_anything {
        let token = Uuid::new_v4();
        *state.consolidation_preview.lock().unwrap() = Some((token, stored));
        out.token = Some(token);
    }

    Ok(out)
}

/// Applies a stored preview, skipping whatever item ids `exclude` names.
///
/// Never re-asks the model — the plan replayed is exactly the one
/// `preview` showed, which is the entire point of the token: a person
/// removing one bad merge must not risk getting a different judgement for
/// everything else too.
///
/// `Ok(None)` means the token did not match anything stored — expired,
/// already applied, or superseded by a later preview. That is a client
/// mistake to report plainly, not an internal failure, so it is a value
/// here rather than an error.
pub async fn apply_selected(
    state: &AppState,
    token: Uuid,
    exclude: &std::collections::HashSet<String>,
    actor: &str,
) -> anyhow::Result<Option<RunReport>> {
    let stored = {
        let mut slot = state.consolidation_preview.lock().unwrap();
        match slot.take() {
            // Consumed on read: applying the same token twice would redo
            // (or partially redo) work that already happened, and a stale
            // preview is worse than none.
            Some((stored_token, plan)) if stored_token == token => Some(plan),
            Some(other) => {
                *slot = Some(other);
                None
            }
            None => None,
        }
    };
    let Some(plan) = stored else {
        return Ok(None);
    };

    let pool = &state.pool;
    let mut report = RunReport::default();

    for (i, candidate) in plan.same_name.iter().enumerate() {
        if exclude.contains(&format!("same_name:{i}")) {
            continue;
        }
        match merge(pool, candidate, actor).await {
            Ok(()) => report.merged_by_name += 1,
            Err(err) => tracing::warn!(
                ?err,
                target = %candidate.target_name,
                "could not apply a selected same-name merge"
            ),
        }
    }

    let resolved = ResolvedPlan {
        merges: plan.merges,
        retypes: plan.retypes,
        relations: plan.relations,
    };
    execute_resolved(pool, &resolved, exclude, actor, &mut report).await?;

    let changed =
        report.merged_by_name + report.merged_by_judgement + report.retyped + report.related;
    if changed > 0 {
        events::append(
            pool,
            Uuid::new_v4(),
            1,
            "consolidation.ran",
            &json!({
                "merged_by_name": report.merged_by_name,
                "merged_by_judgement": report.merged_by_judgement,
                "retyped": report.retyped,
                "related": report.related,
            }),
            actor,
        )
        .await?;
    }

    Ok(Some(report))
}

/// Runs the pass on a timer.
///
/// Slow on purpose. Nothing here is urgent — a duplicate that survives
/// another hour costs nothing — and the deliberate pace is also a safety
/// property: a mistake in what this folds together has time to be noticed
/// before it has folded the whole graph.
pub fn watch(state: AppState, enabled: bool, every: Duration, actor: String) {
    if !enabled {
        tracing::info!(
            "consolidation is manual (the default) — GET /consolidation/preview shows what a \
             pass would do, and POST /consolidation/apply runs the parts a person kept"
        );
        return;
    }

    tracing::info!(
        every_secs = every.as_secs(),
        "consolidating the graph in the background; the first pass is one interval from now"
    );

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;

            match run_once(&state, &actor).await {
                Ok(report)
                    if report.merged_by_name
                        + report.merged_by_judgement
                        + report.retyped
                        + report.related
                        > 0 =>
                {
                    tracing::info!(?report, "consolidation pass changed something")
                }
                Ok(_) => tracing::debug!("consolidation pass found nothing to do"),
                Err(err) => tracing::warn!(?err, "consolidation pass failed"),
            }
        }
    });
}
