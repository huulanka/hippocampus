mod audio;
mod auth;
mod config;
mod echo;
mod embedding;
mod error;
mod events;
mod openrouter;
mod routes;
mod structuring;

use std::sync::Arc;

use axum::http::{HeaderValue, Method, header};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use embedding::Embedder;
use openrouter::OpenRouterClient;

#[derive(Clone)]
pub struct AppState {
    pool: sqlx::PgPool,
    embedder: Embedder,
    openrouter: Option<Arc<OpenRouterClient>>,
    echo_min_similarity: f32,
    audio_dir: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Dev convenience only: in production, real env vars are set directly
    // (e.g. via the Portainer stack), so a missing .env here is fine.
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = config::Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    tokio::fs::create_dir_all(&config.audio_dir).await?;
    tracing::info!(dir = %config.audio_dir, "audio archive ready");

    tracing::info!("loading local embedding model (first run downloads it)...");
    let embedder = Embedder::load(config.model_cache_dir.clone().into()).await?;
    tracing::info!("embedding model ready");

    let openrouter = config.openrouter_api_key.clone().map(|key| {
        tracing::info!(model = %config.openrouter_model, "structuring via OpenRouter enabled");
        Arc::new(OpenRouterClient::new(
            key,
            config.openrouter_model.clone(),
            config.openrouter_zdr,
        ))
    });
    if openrouter.is_none() {
        tracing::warn!("OPENROUTER_API_KEY not set — captures will be stored but not structured");
    }

    let state = AppState {
        pool,
        embedder,
        openrouter,
        echo_min_similarity: config.echo_min_similarity,
        audio_dir: config.audio_dir.clone().into(),
    };

    let protected = routes::router().with_state(state.clone());
    let protected = match config.cf_access_aud.clone() {
        Some(aud) => {
            let team = config
                .cf_access_team_domain
                .clone()
                .expect("config rejects an aud without a team domain");
            tracing::info!(team = %team, "verifying Cloudflare Access tokens");
            let verifier = Arc::new(auth::AccessVerifier::new(&team, aud));
            protected.layer(axum::middleware::from_fn_with_state(
                verifier,
                auth::require_access,
            ))
        }
        None => {
            // Said plainly, because the difference between "local
            // development" and "exposed with no front door" is one
            // unset variable and nothing else.
            tracing::warn!(
                "CF_ACCESS_AUD not set — every request is trusted. Fine on localhost, \
                 not fine anywhere reachable."
            );
            protected
        }
    };

    let app = routes::public_router()
        .with_state(state)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(&config.cors_allowed_origins)?);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "hippocampus backend listening");
    axum::serve(listener, app).await?;

    Ok(())
}

/// Browser access, restricted to the origins the client actually uses.
///
/// Deliberately not `CorsLayer::permissive()`: any page in any tab could
/// otherwise read every capture through a logged-in browser.
fn cors_layer(origins: &[String]) -> anyhow::Result<CorsLayer> {
    let parsed = origins
        .iter()
        .map(|origin| {
            origin
                .parse::<HeaderValue>()
                .map_err(|_| anyhow::anyhow!("{origin} is not a usable origin"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    tracing::info!(origins = ?origins, "CORS restricted");

    Ok(CorsLayer::new()
        .allow_origin(parsed)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]))
}
