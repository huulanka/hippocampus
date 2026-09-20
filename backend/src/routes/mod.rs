pub mod captures;
pub mod search;

use axum::Router;
use axum::routing::{get, post};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/captures", post(captures::create).get(captures::list))
        .route("/search", get(search::search))
}
