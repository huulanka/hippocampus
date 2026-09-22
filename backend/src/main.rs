mod audio;
mod auth;
mod config;
mod consolidation;
mod echo;
mod embedding;
mod error;
mod events;
mod judge;
mod openrouter;
mod pipeline;
mod reranker;
mod routes;
mod structuring;
mod telemetry;

use std::sync::Arc;

use axum::http::{HeaderName, HeaderValue, Method, header};
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
    /// Shared rather than cloned: echo judging now happens in a spawned
    /// task, which outlives the request that started it.
    judge: Option<Arc<judge::Judge>>,
    /// Per-capture echo judging state: in flight, or cooling down after a
    /// failure. Without the in-flight half, opening a capture that has no
    /// stored echo three times would start three judgements of it — and
    /// each one is a paid call. Without the cooldown half, a judgement
    /// that fails fast (a provider rate limit, say) would start a new one
    /// on every read that follows — and the client reads every 1.2s while
    /// it waits.
    judging:
        Arc<std::sync::Mutex<std::collections::HashMap<uuid::Uuid, routes::captures::JudgeSlot>>>,
    /// How long a failed judgement blocks a new attempt for the same
    /// capture. See `judging`.
    echo_judge_retry_backoff: std::time::Duration,
    audio_dir: std::path::PathBuf,
    timezone: chrono_tz::Tz,
    /// How hard the background loop tries to finish a capture whose
    /// indexing or structuring failed. Read at the point of failure, not
    /// only by the loop, so the first attempt counts the same as the
    /// fifth.
    retry: pipeline::RetrySettings,
    /// The most recent `GET /consolidation/preview`, waiting for
    /// `POST /consolidation/apply` to say which parts of it to keep. See
    /// `consolidation::PreviewStore`.
    consolidation_preview: Arc<consolidation::PreviewStore>,
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

    let judge = judge::Judge::load(
        config.judge,
        config.model_cache_dir.clone().into(),
        config.openrouter_api_key.clone(),
        config.echo_judge_model.clone(),
        config.openrouter_zdr,
    )
    .await?
    .map(Arc::new);
    match &judge {
        Some(loaded) => tracing::info!(
            judge = %loaded.name(),
            min_score = config.echo_min_rerank,
            "echo judging enabled"
        ),
        None => tracing::warn!(
            "echo judging is off — echoes are ordered by embedding similarity alone, \
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
        judge,
        judging: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        echo_judge_retry_backoff: config.echo_judge_retry_backoff,
        audio_dir: config.audio_dir.clone().into(),
        timezone: config.timezone,
        retry: config.retry,
        consolidation_preview: Arc::new(std::sync::Mutex::new(None)),
    };

    tracing::info!(
        timezone = %config.timezone,
        "relative times in captures resolve against this timezone unless the device names its own"
    );

    // Started before the router: a backlog left by the last run should be
    // shrinking while the first request of this one arrives, not waiting
    // for someone to ask.
    pipeline::watch(state.clone(), config.retry);

    // Slow on purpose, and deliberately not on the same clock as the
    // capture pipeline: that one is catching up on work a user is
    // waiting for, this one is tidying a graph nobody is looking at.
    consolidation::watch(
        state.clone(),
        config.consolidation_enabled,
        config.consolidation_interval,
        config.openrouter_model.clone(),
    );

    // Entities have carried an `embedding` column and an HNSW index since
    // the first migration, and nothing ever wrote to either — every
    // entity in the database had a null. Spawned rather than awaited: it
    // is catch-up work on a local model, and nothing should wait to
    // listen for it.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                match structuring::backfill_entity_embeddings(&state.pool, &state.embedder).await {
                    Ok(0) => break,
                    Ok(filled) => tracing::info!(filled, "gave entities their embeddings"),
                    Err(err) => {
                        tracing::warn!(?err, "could not backfill entity embeddings");
                        break;
                    }
                }
            }
        });
    }

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
        // One INFO line per request, with how long it took. The default
        // `TraceLayer` logs at DEBUG, which meant that in practice this
        // service recorded nothing about its own behaviour — and the
        // first operational question asked of it was "why is this slow?".
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    tracing::info_span!(
                        "http",
                        method = %request.method(),
                        path = %request.uri().path(),
                    )
                })
                .on_request(())
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     _: &tracing::Span| {
                        tracing::info!(
                            status = response.status().as_u16(),
                            ms = latency.as_millis(),
                            "request finished"
                        );
                    },
                ),
        )
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

    // The Cloudflare Access Service Token headers: a preflight that does
    // not list them here comes back without them in
    // `Access-Control-Allow-Headers`, so the browser refuses to send the
    // real request with those headers attached.
    //
    // Only the browser development build needs this. The desktop client
    // makes its requests in Rust, which has no origin and therefore no
    // preflight at all — see ADR 0009, and note that a preflight would
    // not have survived Cloudflare Access in front of the backend anyway.
    let cf_access_client_id = HeaderName::from_static("cf-access-client-id");
    let cf_access_client_secret = HeaderName::from_static("cf-access-client-secret");

    Ok(CorsLayer::new()
        .allow_origin(parsed)
        // DELETE for redaction. Only the browser development build is
        // affected — the desktop client makes its requests in Rust and
        // never sends a preflight (ADR 0009).
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            cf_access_client_id,
            cf_access_client_secret,
        ]))
}
