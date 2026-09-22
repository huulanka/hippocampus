//! The entity graph as one object.
//!
//! Everything else in this API is walked: a capture, then its entities,
//! then one entity's neighbours. A graph is the one view whose point is
//! what it looks like *together*, so it is fetched together — walking it
//! would mean the layout settles while the data is still arriving.
//!
//! Bounded by mention count rather than returned whole. A graph with a
//! thousand nodes is not a picture of anything, and the entities worth
//! seeing first are the ones that keep coming back. What was left out is
//! reported as a number instead of quietly dropped.

use axum::Json;
use axum::extract::{Query, State};
use contracts::{Graph, GraphEdge, GraphNode};
use sqlx::PgPool;

use crate::AppState;
use crate::error::AppError;

/// How many entities are drawn unless asked otherwise. Chosen as roughly
/// what stays readable on a laptop screen once labels are on it, not as a
/// performance limit.
const DEFAULT_LIMIT: i64 = 120;

#[derive(serde::Deserialize)]
pub struct GraphParams {
    pub limit: Option<i64>,
    /// Only entities of this type, for looking at one layer of the graph
    /// on its own.
    pub entity_type: Option<String>,
}

pub async fn graph(
    State(state): State<AppState>,
    Query(params): Query<GraphParams>,
) -> Result<Json<Graph>, AppError> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, 1000);
    let entity_type = params.entity_type.filter(|t| !t.trim().is_empty());

    Ok(Json(load(&state.pool, limit, entity_type).await?))
}

/// Split out from the handler so it can be tested against a real
/// database: what is worth testing here is the SQL — the cap, what
/// happens to an edge whose other end did not make the cut, and whether
/// `omitted_nodes` tells the truth — and none of that is reachable
/// through an `AppState`, which would have to load an embedding model
/// first.
pub async fn load(
    pool: &PgPool,
    limit: i64,
    entity_type: Option<String>,
) -> Result<Graph, sqlx::Error> {
    let nodes = sqlx::query!(
        r#"
        select
            e.id,
            e.name,
            e.entity_type,
            count(o.id) as "mention_count!",
            max(o.created_at) as "last_seen?"
        from entities e
        left join observations o on o.entity_id = e.id
        where ($1::text is null or e.entity_type = $1)
        group by e.id, e.name, e.entity_type
        order by count(o.id) desc, max(o.created_at) desc nulls last, e.name
        limit $2
        "#,
        entity_type,
        limit,
    )
    .fetch_all(pool)
    .await?;

    let total = sqlx::query_scalar!(
        r#"select count(*) as "count!" from entities e
           where ($1::text is null or e.entity_type = $1)"#,
        entity_type,
    )
    .fetch_one(pool)
    .await?;

    let ids: Vec<uuid::Uuid> = nodes.iter().map(|n| n.id).collect();

    // Both ends must be on screen. An edge to something that was cut is
    // not a hint that something is missing — it is a line to nowhere.
    //
    // Grouped by type as well as by pair, because "Sam works at Acme"
    // said five times and "Sam founded Acme" said once are two different
    // facts about the same two entities, and averaging them into one line
    // would lose the more interesting of the two.
    let edges = sqlx::query!(
        r#"
        select
            r.from_entity_id as "from!",
            r.to_entity_id as "to!",
            r.relation_type,
            count(*) as "weight!"
        from relations r
        where r.from_entity_id = any($1) and r.to_entity_id = any($1)
        group by r.from_entity_id, r.to_entity_id, r.relation_type
        order by count(*) desc
        "#,
        &ids,
    )
    .fetch_all(pool)
    .await?;

    Ok(Graph {
        omitted_nodes: (total - nodes.len() as i64).max(0),
        nodes: nodes
            .into_iter()
            .map(|n| GraphNode {
                id: n.id,
                name: n.name,
                entity_type: n.entity_type,
                mention_count: n.mention_count,
                last_seen: n.last_seen,
            })
            .collect(),
        edges: edges
            .into_iter()
            .map(|e| GraphEdge {
                from: e.from,
                to: e.to,
                relation_type: e.relation_type,
                weight: e.weight,
            })
            .collect(),
    })
}

/// These need a real Postgres, so they are behind a feature rather than
/// compiled into every `cargo test`. The SQL is the whole substance of
/// this module — a mock pool would only assert that the string was not
/// mistyped — and the questions worth asking (does the cap keep the
/// busiest entities, does an edge survive when one end was cut, does
/// `omitted_nodes` count what it claims) can only be answered by the
/// database itself.
///
///     docker compose up -d postgres
///     DATABASE_URL=postgres://hippocampus:hippocampus@localhost:5433/hippocampus \
///       cargo test -p backend --features db-tests
///
/// `#[sqlx::test]` gives each test its own freshly migrated database and
/// drops it afterwards, so they neither see each other nor the dev data.
#[cfg(all(test, feature = "db-tests"))]
mod tests {
    use sqlx::PgPool;
    use uuid::Uuid;

    /// One event for every observation and relation to hang off. The
    /// foreign keys require it; nothing in this module reads it.
    async fn an_event(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!(
            r#"insert into events (stream_id, version, event_type, payload, source)
               values (gen_random_uuid(), 1, 'capture.recorded', '{}'::jsonb, 'test')
               returning id"#
        )
        .fetch_one(pool)
        .await
        .expect("insert event")
    }

    /// An entity mentioned `mentions` times. The count is what the cap
    /// orders by, so it is the interesting parameter.
    async fn an_entity(pool: &PgPool, name: &str, entity_type: &str, mentions: i32) -> Uuid {
        sqlx::query!(
            "insert into entity_type_registry (entity_type) values ($1)
             on conflict do nothing",
            entity_type
        )
        .execute(pool)
        .await
        .expect("register type");

        let id = sqlx::query_scalar!(
            "insert into entities (entity_type, name) values ($1, $2) returning id",
            entity_type,
            name
        )
        .fetch_one(pool)
        .await
        .expect("insert entity");

        for _ in 0..mentions {
            let event = an_event(pool).await;
            sqlx::query!(
                "insert into observations (entity_id, source_event_id, text, model)
                 values ($1, $2, $3, 'test-model')",
                id,
                event,
                name
            )
            .execute(pool)
            .await
            .expect("insert observation");
        }

        id
    }

    async fn a_relation(pool: &PgPool, from: Uuid, to: Uuid, relation_type: &str) {
        let event = an_event(pool).await;
        sqlx::query!(
            "insert into relations (from_entity_id, to_entity_id, relation_type, source_event_id, model)
             values ($1, $2, $3, $4, 'test-model')",
            from,
            to,
            relation_type,
            event
        )
        .execute(pool)
        .await
        .expect("insert relation");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_empty_database_is_an_empty_graph(pool: PgPool) {
        let graph = super::load(&pool, 120, None).await.expect("load");

        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
        assert_eq!(graph.omitted_nodes, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_cap_keeps_the_most_mentioned_and_counts_the_rest(pool: PgPool) {
        an_entity(&pool, "often", "Person", 5).await;
        an_entity(&pool, "sometimes", "Person", 3).await;
        an_entity(&pool, "once", "Person", 1).await;

        let graph = super::load(&pool, 2, None).await.expect("load");

        assert_eq!(
            graph
                .nodes
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            ["often", "sometimes"],
            "the cap should keep the busiest entities, in that order"
        );
        assert_eq!(graph.omitted_nodes, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_entity_nobody_mentioned_is_still_a_node(pool: PgPool) {
        an_entity(&pool, "silent", "Thema", 0).await;

        let graph = super::load(&pool, 120, None).await.expect("load");

        assert_eq!(graph.nodes.len(), 1, "a left join, not an inner one");
        assert_eq!(graph.nodes[0].mention_count, 0);
        assert!(graph.nodes[0].last_seen.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn an_edge_whose_other_end_was_cut_is_dropped(pool: PgPool) {
        let busy = an_entity(&pool, "busy", "Person", 9).await;
        let rare = an_entity(&pool, "rare", "Person", 1).await;
        a_relation(&pool, busy, rare, "kennt").await;

        let both = super::load(&pool, 120, None).await.expect("load");
        assert_eq!(both.edges.len(), 1, "both ends drawn, so the edge is drawn");

        let cut = super::load(&pool, 1, None).await.expect("load");
        assert_eq!(cut.nodes.len(), 1);
        assert!(
            cut.edges.is_empty(),
            "an edge to something that was cut is a line to nowhere"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn the_same_relation_said_twice_is_one_edge_of_weight_two(pool: PgPool) {
        let sam = an_entity(&pool, "Sam", "Person", 4).await;
        let acme = an_entity(&pool, "Acme", "Organisation", 4).await;
        a_relation(&pool, sam, acme, "arbeitet_in").await;
        a_relation(&pool, sam, acme, "arbeitet_in").await;
        // Same pair, different claim: this must stay its own edge, or the
        // more interesting of the two facts disappears into the other.
        a_relation(&pool, sam, acme, "hat_gegründet").await;

        let graph = super::load(&pool, 120, None).await.expect("load");

        assert_eq!(graph.edges.len(), 2);
        let arbeitet = graph
            .edges
            .iter()
            .find(|e| e.relation_type == "arbeitet_in")
            .expect("the repeated relation");
        assert_eq!(arbeitet.weight, 2);
        assert_eq!(
            graph
                .edges
                .iter()
                .find(|e| e.relation_type == "hat_gegründet")
                .expect("the single relation")
                .weight,
            1
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn filtering_by_type_counts_only_that_type_as_omitted(pool: PgPool) {
        an_entity(&pool, "Sam", "Person", 3).await;
        an_entity(&pool, "Priya", "Person", 2).await;
        an_entity(&pool, "Lissabon", "Ort", 2).await;

        let graph = super::load(&pool, 1, Some("Person".to_string()))
            .await
            .expect("load");

        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].entity_type, "Person");
        assert_eq!(
            graph.omitted_nodes, 1,
            "the Ort was never a candidate, so it is not 'left out'"
        );
    }
}
