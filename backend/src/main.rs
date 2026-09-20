mod config;
mod embedding;
mod error;
mod events;
mod openrouter;
mod routes;
mod structuring;

use std::sync::Arc;

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
    };

    let app = routes::router()
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "hippocampus backend listening");
    axum::serve(listener, app).await?;

    Ok(())
}
