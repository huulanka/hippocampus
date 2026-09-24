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
    pub judge: crate::judge::Choice,
    /// Which hosted models judge echoes when `judge` is `Remote`, in the
    /// order they are tried: `ECHO_JUDGE_MODEL`, then
    /// `ECHO_JUDGE_FALLBACK_MODEL` unless that is `none`.
    pub echo_judge_models: Vec<String>,
    /// How long a failed echo judgement blocks a new attempt for the same
    /// capture, the first time; every further failure doubles it, up to
    /// an hour. Judging is triggered by every read of a not-yet-judged
    /// capture, and the client polls every 1.2s while it waits — without
    /// this, a single failing provider (a rate limit, say) turns into a
    /// new paid call several times a second for as long as anyone looks.
    pub echo_judge_retry_backoff: std::time::Duration,
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
    /// How the background loop finishes captures whose indexing or
    /// structuring did not succeed the first time.
    pub retry: crate::pipeline::RetrySettings,
    /// How often the graph is consolidated. Deliberately slow: a
    /// duplicate surviving another hour costs nothing, and the pace is
    /// itself a safety property — a mistake in what gets folded together
    /// has time to be noticed.
    pub consolidation_interval: std::time::Duration,
    /// Whether the consolidation pass runs by itself. Off by default: a
    /// rearrangement made while nobody was watching is a surprise no
    /// matter how reversible it is. `false` unless this is set to exactly
    /// `"true"`, so tidying stays a person clicking a button — preview,
    /// then apply — rather than something that happens on a timer.
    pub consolidation_enabled: bool,
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

/// A duration in whole seconds, or the default when unset.
fn parse_secs(key: &str, default: u64) -> anyhow::Result<u64> {
    env_non_empty(key)
        .map(|v| {
            v.parse::<u64>()
                .map_err(|err| anyhow::anyhow!("{key} must be a whole number of seconds: {err}"))
        })
        .transpose()
        .map(|parsed| parsed.unwrap_or(default))
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // Read first: both echo thresholds default differently depending
        // on whether a cross-encoder is doing the judging.
        let judge = env_non_empty("HIPPOCAMPUS_RERANKER")
            .map(|v| v.parse::<crate::judge::Choice>())
            .transpose()?
            .unwrap_or(crate::judge::Choice::Remote);

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
                .unwrap_or(match judge {
                    // A recall floor, not a quality bar: the judge
                    // decides. Keeping 0.89 here would hand it a candidate
                    // set already filtered by the thing it exists to fix.
                    crate::judge::Choice::Off => crate::echo::DEFAULT_MIN_SIMILARITY,
                    _ => crate::echo::CANDIDATE_MIN_SIMILARITY,
                }),
            echo_min_rerank: env_non_empty("ECHO_MIN_RERANK_SCORE")
                .map(|v| v.parse())
                .transpose()
                .map_err(|err| anyhow::anyhow!("ECHO_MIN_RERANK_SCORE must be a number: {err}"))?
                .unwrap_or_else(|| crate::judge::default_min_score(judge)),
            judge,
            echo_judge_models: {
                let primary = env_non_empty("ECHO_JUDGE_MODEL")
                    .unwrap_or_else(|| crate::judge::DEFAULT_JUDGE_MODEL.to_string());
                let fallback = env_non_empty("ECHO_JUDGE_FALLBACK_MODEL")
                    .unwrap_or_else(|| crate::judge::DEFAULT_JUDGE_FALLBACK_MODEL.to_string());
                let mut models = vec![primary];
                if !fallback.eq_ignore_ascii_case("none") && !models.contains(&fallback) {
                    models.push(fallback);
                }
                models
            },
            echo_judge_retry_backoff: std::time::Duration::from_secs(parse_secs(
                "ECHO_JUDGE_RETRY_BACKOFF_SECS",
                20,
            )?),
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
            retry: crate::pipeline::RetrySettings {
                enabled: env_non_empty("STRUCTURING_RETRY_ENABLED")
                    .map(|v| v != "false")
                    .unwrap_or(true),
                // Minutes, not seconds: this is catch-up work for a
                // provider that was down, and asking every few seconds
                // would only turn one outage into a lot of failed calls.
                interval: std::time::Duration::from_secs(parse_secs(
                    "STRUCTURING_RETRY_INTERVAL_SECS",
                    300,
                )?),
                backoff: std::time::Duration::from_secs(parse_secs(
                    "STRUCTURING_RETRY_BACKOFF_SECS",
                    900,
                )?),
                batch: env_non_empty("STRUCTURING_RETRY_BATCH")
                    .map(|v| v.parse())
                    .transpose()
                    .map_err(|err| {
                        anyhow::anyhow!("STRUCTURING_RETRY_BATCH must be a whole number: {err}")
                    })?
                    .unwrap_or(5),
                // Five, because each one is a paid call and a fault that
                // survives five attempts spread over more than an hour is
                // not a blip — it is something a person has to look at.
                max_attempts: env_non_empty("STRUCTURING_MAX_ATTEMPTS")
                    .map(|v| v.parse())
                    .transpose()
                    .map_err(|err| {
                        anyhow::anyhow!("STRUCTURING_MAX_ATTEMPTS must be a whole number: {err}")
                    })?
                    .unwrap_or(5),
            },
            consolidation_enabled: env_non_empty("CONSOLIDATION_ENABLED")
                .map(|v| v == "true")
                .unwrap_or(false),
            consolidation_interval: std::time::Duration::from_secs(parse_secs(
                "CONSOLIDATION_INTERVAL_SECS",
                3600,
            )?),
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
