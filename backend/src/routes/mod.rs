pub mod captures;
pub mod search;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::AppState;
use crate::audio;

/// Reachable without a Cloudflare Access token, because the thing asking
/// is a health check that has no way to obtain one.
pub fn public_router() -> Router<AppState> {
    Router::new().route("/health", get(|| async { "ok" }))
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
        .route("/captures/{id}/echo", get(captures::echo_for))
        .route("/captures/{id}/audio", get(captures::audio_for))
        .route("/entity-types", get(captures::entity_types))
        .route("/search", get(search::search))
}
