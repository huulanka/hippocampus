mod audio;
mod auth;
mod config;
mod echo;
mod embedding;
mod error;
mod events;
mod openrouter;
mod reranker;
mod routes;
mod structuring;

use std::sync::Arc;

use axum::http::{HeaderValue, Method, header};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use embedding::Embedder;
use openrouter::OpenRouterClient;

#[derive(Clone)]
pub struct AppState {
    pool: sqlx::PgPool,
    embedder: Embedder,
    openrouter: Option<Arc<OpenRouterClient>>,
    echo_min_similarity: f32,
    echo_min_rerank: f32,
    reranker: Option<reranker::Reranker>,
    audio_dir: std::path::PathBuf,
    timezone: chrono_tz::Tz,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Dev convenience only: in production, real env vars are set directly
    // (e.g. via the Portainer stack), so a missing .env here is fine.
    dotenvy::dotenv().ok();

    // Read directly rather than through `Config`: logging has to be up
    // before `Config::from_env()` runs, so a bad env var still ends up
    // somewhere readable instead of only on a terminal that may already be
    // gone by the time anyone looks (this runs as a Portainer stack, not a
    // foreground process, once it leaves this machine).
    let log_dir = std::env::var("LOG_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "../data/logs".to_string());
    tokio::fs::create_dir_all(&log_dir).await?;
    let file_appender = tracing_appender::rolling::daily(&log_dir, "backend.log");
    let (file_writer, _log_guard) = tracing_appender::non_blocking(file_appender);

    // `from_default_env()` falls back to filtering out everything, not to
    // a sane level, when `RUST_LOG` is unset — which it always was here in
    // practice, so "the backend has logs" was previously untrue by
    // default. `info` is what every `tracing::info!` call in this crate
    // already assumes an operator wants to see.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_writer)
                .with_ansi(false),
        )
        .init();
    tracing::info!(dir = %log_dir, "file logging ready, rolls daily");

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

    let reranker =
        reranker::Reranker::load(config.reranker, config.model_cache_dir.clone().into()).await?;
    match &reranker {
        Some(loaded) => tracing::info!(reranker = ?loaded.choice(), "echo reranking enabled"),
        None => tracing::warn!(
            "echo reranking is off — echoes are ordered by embedding similarity alone, \
             which measurably ranks unrelated captures above related ones"
        ),
    }

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
        echo_min_rerank: config.echo_min_rerank,
        reranker,
        audio_dir: config.audio_dir.clone().into(),
        timezone: config.timezone,
    };

    tracing::info!(
        timezone = %config.timezone,
        "relative times in captures resolve against this timezone unless the device names its own"
    );

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
