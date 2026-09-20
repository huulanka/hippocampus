use std::env;

pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Where downloaded ONNX embedding model files are cached, so they
    /// survive container restarts instead of re-downloading every time.
    pub model_cache_dir: String,
    /// If unset, structuring (entity/relation extraction) is skipped and
    /// captures stay raw-only — the rest of the system still works.
    pub openrouter_api_key: Option<String>,
    pub openrouter_model: String,
    /// Route only to providers with a Zero Data Retention policy. See
    /// https://openrouter.ai/docs/features/provider-routing.
    pub openrouter_zdr: bool,
    /// Minimum cosine similarity for a capture to be shown as an echo.
    /// Configurable because the right value depends on how the user
    /// actually speaks, and can only be found against real captures.
    pub echo_min_similarity: f32,
}

/// Reads an env var, treating both "unset" and "set but empty" as absent —
/// `.env.example` documents optional keys as blank (`KEY=`), and
/// `std::env::var` alone would otherwise treat that as a real value.
fn env_non_empty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: env_non_empty("DATABASE_URL")
                .ok_or_else(|| anyhow::anyhow!("DATABASE_URL must be set"))?,
            bind_addr: env_non_empty("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8080".to_string()),
            model_cache_dir: env_non_empty("MODEL_CACHE_DIR")
                .unwrap_or_else(|| "../data/models".to_string()),
            openrouter_api_key: env_non_empty("OPENROUTER_API_KEY"),
            openrouter_model: env_non_empty("OPENROUTER_MODEL")
                .unwrap_or_else(|| "google/gemini-3.5-flash-lite".to_string()),
            openrouter_zdr: env_non_empty("OPENROUTER_ZDR")
                .map(|v| v != "false")
                .unwrap_or(true),
            echo_min_similarity: env_non_empty("ECHO_MIN_SIMILARITY")
                .map(|v| v.parse())
                .transpose()
                .map_err(|err| anyhow::anyhow!("ECHO_MIN_SIMILARITY must be a number: {err}"))?
                .unwrap_or(crate::echo::DEFAULT_MIN_SIMILARITY),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression test: a blank `MODEL_CACHE_DIR=` in .env (the documented
    // way to leave an optional key at its default) once silently won out
    // over the code default, because `env::var` treats "set but empty" as
    // present — the model then downloaded straight into the crate's CWD.
    #[test]
    fn env_non_empty_treats_blank_value_as_absent() {
        // SAFETY: single-threaded access to a test-only env var name.
        unsafe {
            std::env::set_var("HIPPOCAMPUS_TEST_ENV_NON_EMPTY", "");
        }
        assert_eq!(env_non_empty("HIPPOCAMPUS_TEST_ENV_NON_EMPTY"), None);

        unsafe {
            std::env::set_var("HIPPOCAMPUS_TEST_ENV_NON_EMPTY", "value");
        }
        assert_eq!(
            env_non_empty("HIPPOCAMPUS_TEST_ENV_NON_EMPTY"),
            Some("value".to_string())
        );

        unsafe {
            std::env::remove_var("HIPPOCAMPUS_TEST_ENV_NON_EMPTY");
        }
        assert_eq!(env_non_empty("HIPPOCAMPUS_TEST_ENV_NON_EMPTY"), None);
    }
}
