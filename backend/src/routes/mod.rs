pub mod captures;
mod context;
pub mod entities;
mod graph;
pub mod intentions;
pub mod resurface;
pub mod review;
pub mod search;

use axum::Json;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::AppState;
use crate::audio;

/// Reachable without a Cloudflare Access token, because the thing asking
/// is a health check (or the client's About screen, before it necessarily
/// has one configured) that has no way to obtain one.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route(
            "/version",
            get(|| async { Json(env!("CARGO_PKG_VERSION")) }),
        )
}

/// Everything that touches what the user has said. Behind access
/// verification whenever it is configured.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/captures", post(captures::create).get(captures::list))
        .route(
            "/captures/audio",
            // Axum's default body limit is 2 MB, which a spoken note
            // exceeds within seconds; the real bound lives in `audio`.
            post(captures::create_from_audio).layer(DefaultBodyLimit::max(audio::MAX_BYTES)),
        )
        .route(
            "/captures/{id}",
            get(captures::detail).delete(captures::redact),
        )
        .route(
            "/captures/{id}/transcript",
            post(captures::correct_transcript),
        )
        .route("/captures/{id}/echo", get(captures::echo_for))
        .route("/captures/{id}/retry", post(captures::retry_capture))
        .route("/captures/{id}/audio", get(captures::audio_for))
        .route("/captures/{id}/intentions", get(intentions::for_capture))
        .route("/intentions", get(intentions::open))
        .route("/intentions/{id}/dismiss", post(intentions::dismiss))
        .route("/intentions/{id}/fulfil", post(intentions::fulfil))
        .route("/intentions/{id}/reopen", post(intentions::reopen))
        .route("/brief", post(intentions::brief))
        .route("/entities", get(entities::list))
        .route("/entities/{id}", get(entities::detail))
        .route("/entities/{id}/merge", post(entities::merge_entities))
        .route(
            "/entities/{id}/fold-candidates",
            get(entities::fold_candidates),
        )
        .route("/entities/{id}/unmerge", post(entities::unmerge))
        .route("/consolidation", get(entities::changelog))
        .route("/consolidation/run", post(entities::consolidate_now))
        .route(
            "/consolidation/preview",
            get(entities::consolidation_preview),
        )
        .route("/consolidation/apply", post(entities::consolidation_apply))
        .route(
            "/relations/{id}",
            axum::routing::delete(entities::retract_relation),
        )
        .route("/entity-types", get(captures::entity_types))
        .route("/graph", get(graph::graph))
        .route("/resurface", get(resurface::resurface))
        .route("/review", get(review::week))
        .route("/review/story", post(review::write_story))
        .route("/search", get(search::search))
        .route("/ask", post(context::ask))
        .route("/occasions/pending", get(context::pending))
        .route("/occasions", post(context::offer))
        .route("/pipeline", get(captures::pipeline_status))
        .route("/pipeline/retry", post(captures::retry_all))
}
