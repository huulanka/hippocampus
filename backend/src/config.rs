use std::env;

pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Where downloaded ONNX embedding model files are cached, so they
    /// survive container restarts instead of re-downloading every time.
    pub model_cache_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            model_cache_dir: env::var("MODEL_CACHE_DIR")
                .unwrap_or_else(|_| "../data/models".to_string()),
        })
    }
}
