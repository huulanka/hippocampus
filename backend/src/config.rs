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
    /// Where capture audio is kept. This is the archive of originals
    /// (ADR 0004) — it must be on backed-up storage, not a scratch disk.
    pub audio_dir: String,
    /// Minimum cosine similarity for a capture to be considered as an
    /// echo at all. With a reranker loaded this is a recall floor rather
    /// than a quality bar — the cross-encoder decides what is shown — so
    /// it defaults lower in that case.
    pub echo_min_similarity: f32,
    /// Minimum cross-encoder score for a candidate to survive. Ignored
    /// when no reranker is loaded.
    pub echo_min_rerank: f32,
    /// Which cross-encoder reranks echo candidates: "jina", "bge" or
    /// "off".
    pub reranker: crate::reranker::Choice,
    /// Expected `aud` of the Cloudflare Access token. Unset means access
    /// verification is switched off, which is how local development runs.
    pub cf_access_aud: Option<String>,
    /// Zero Trust team, as a name or a full hostname. Required whenever
    /// `cf_access_aud` is set — it is where the signing keys come from.
    pub cf_access_team_domain: Option<String>,
    /// Origins the browser side is allowed to call from. Never a wildcard:
    /// this service answers with someone's entire memory.
    pub cors_allowed_origins: Vec<String>,
    /// Fallback timezone for resolving "tomorrow" and friends, used only
    /// when a capture did not say where it was recorded.
    pub timezone: chrono_tz::Tz,
}

/// Where the desktop client calls from: `tauri://localhost` is the packaged
/// app's origin on macOS, the other is `npm run tauri dev`.
const DEFAULT_ALLOWED_ORIGINS: &str = "tauri://localhost,http://localhost:1420";

/// Reads an env var, treating both "unset" and "set but empty" as absent —
/// `.env.example` documents optional keys as blank (`KEY=`), and
/// `std::env::var` alone would otherwise treat that as a real value.
fn env_non_empty(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // Read first: both echo thresholds default differently depending
        // on whether a cross-encoder is doing the judging.
        let reranker = env_non_empty("HIPPOCAMPUS_RERANKER")
            .map(|v| v.parse::<crate::reranker::Choice>())
            .transpose()?
            .unwrap_or(crate::reranker::Choice::Bge);

        let config = Self {
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
            audio_dir: env_non_empty("AUDIO_DIR").unwrap_or_else(|| "../data/audio".to_string()),
            echo_min_similarity: env_non_empty("ECHO_MIN_SIMILARITY")
                .map(|v| v.parse())
                .transpose()
                .map_err(|err| anyhow::anyhow!("ECHO_MIN_SIMILARITY must be a number: {err}"))?
                .unwrap_or(match reranker {
                    // A recall floor, not a quality bar: the cross-encoder
                    // decides. Keeping 0.89 here would hand it a candidate
                    // set already filtered by the thing it exists to fix.
                    crate::reranker::Choice::Off => crate::echo::DEFAULT_MIN_SIMILARITY,
                    _ => crate::echo::CANDIDATE_MIN_SIMILARITY,
                }),
            echo_min_rerank: env_non_empty("ECHO_MIN_RERANK_SCORE")
                .map(|v| v.parse())
                .transpose()
                .map_err(|err| anyhow::anyhow!("ECHO_MIN_RERANK_SCORE must be a number: {err}"))?
                .unwrap_or_else(|| crate::reranker::default_min_score(reranker)),
            reranker,
            cf_access_aud: env_non_empty("CF_ACCESS_AUD"),
            cf_access_team_domain: env_non_empty("CF_ACCESS_TEAM_DOMAIN"),
            timezone: env_non_empty("HIPPOCAMPUS_TIMEZONE")
                .map(|name| {
                    name.parse::<chrono_tz::Tz>().map_err(|_| {
                        anyhow::anyhow!("HIPPOCAMPUS_TIMEZONE is not an IANA timezone: {name}")
                    })
                })
                .transpose()?
                .unwrap_or(chrono_tz::Europe::Berlin),
            cors_allowed_origins: env_non_empty("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|| DEFAULT_ALLOWED_ORIGINS.to_string())
                .split(',')
                .map(|origin| origin.trim().to_string())
                .filter(|origin| !origin.is_empty())
                .collect(),
        };

        // Half-configured access control is the worst of both worlds: it
        // looks protected and is not. Refuse to start rather than fall
        // back to letting everything through.
        if config.cf_access_aud.is_some() && config.cf_access_team_domain.is_none() {
            anyhow::bail!("CF_ACCESS_AUD is set but CF_ACCESS_TEAM_DOMAIN is not");
        }

        if config.cors_allowed_origins.iter().any(|o| o == "*") {
            anyhow::bail!(
                "CORS_ALLOWED_ORIGINS must name real origins; this service answers with \
                 everything you have ever said"
            );
        }

        Ok(config)
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
