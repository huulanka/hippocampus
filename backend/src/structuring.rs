//! Turns an `ExtractionResult` into projection rows (`entities`,
//! `observations`, `relations`), each backed by its own event on the
//! entity's/relation's stream — so every derived fact stays traceable to
//! the model call and source capture that produced it, per ADR 0003.

use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::events;
use crate::openrouter::{ExtractionResult, OpenRouterClient};

/// Runs structuring for a capture in the background: never blocks or fails
/// the ingest request. A failure here just means this capture stays
/// un-structured until the next attempt — acceptable at personal-note
/// volume, and always re-derivable from the untouched raw capture event.
pub async fn structure_capture_in_background(
    pool: PgPool,
    client: Option<std::sync::Arc<OpenRouterClient>>,
    source_event_id: Uuid,
    transcript: String,
) {
    let Some(client) = client else {
        tracing::debug!("OPENROUTER_API_KEY not set, skipping structuring");
        return;
    };

    if let Err(err) = structure_capture(&pool, &client, source_event_id, &transcript).await {
        tracing::error!(?err, %source_event_id, "structuring failed for capture");
    }
}

async fn structure_capture(
    pool: &PgPool,
    client: &OpenRouterClient,
    source_event_id: Uuid,
    transcript: &str,
) -> anyhow::Result<()> {
    let extraction = client.extract(transcript).await?;
    apply_extraction(pool, source_event_id, &extraction, client.model_name()).await
}

async fn apply_extraction(
    pool: &PgPool,
    source_event_id: Uuid,
    extraction: &ExtractionResult,
    model: &str,
) -> anyhow::Result<()> {
    let mut entity_ids = std::collections::HashMap::new();

    for entity in &extraction.entities {
        let id = resolve_or_create_entity(
            pool,
            &entity.entity_type,
            &entity.name,
            source_event_id,
            &entity.observation,
            model,
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

async fn resolve_or_create_entity(
    pool: &PgPool,
    entity_type: &str,
    name: &str,
    source_event_id: Uuid,
    observation: &str,
    model: &str,
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
        &json!({ "text": observation, "source_event_id": source_event_id }),
        model,
    )
    .await?;

    sqlx::query!(
        r#"insert into observations (entity_id, source_event_id, text, model) values ($1, $2, $3, $4)"#,
        entity_id,
        source_event_id,
        observation,
        model,
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
