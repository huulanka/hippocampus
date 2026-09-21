//! What the system says about itself while it runs.
//!
//! The backend stopped being a terminal session the moment it moved to
//! the NAS: nobody watches it, and the only way to know how it behaves is
//! what it wrote down. Until now it wrote down almost nothing —
//! `TraceLayer` ran with its defaults, which log at DEBUG, and the model
//! calls were entirely silent. The first real question anyone asked of it
//! ("why does a judgement take twelve seconds?") could not be answered
//! from the logs at all.
//!
//! Everything here is one line per event at INFO, with fields rather than
//! prose, so `grep` and `jq`-style reading both work and so the rolling
//! file stays readable by a person.

use std::time::Duration;

use serde_json::Value;

/// One completed call to a hosted model.
///
/// `purpose` separates the two callers — structuring and echo judging —
/// because they use different models for different reasons and their
/// latencies have nothing to do with each other.
///
/// The provider is read out of the response rather than assumed: with ZDR
/// routing OpenRouter picks from whichever providers qualify, and *which
/// one it picked* is the first thing worth knowing when the same model is
/// fast one minute and slow the next.
pub fn model_call(purpose: &str, model: &str, response: &Value, elapsed: Duration) {
    let usage = &response["usage"];
    tracing::info!(
        purpose,
        model,
        // OpenRouter names the upstream it routed to; absent on some
        // responses, which is worth seeing as "unknown" rather than
        // silently omitting the field.
        provider = response["provider"].as_str().unwrap_or("unknown"),
        // The model that actually answered can differ from the one asked
        // for, when a route falls back.
        served_by = response["model"].as_str().unwrap_or(model),
        ms = elapsed.as_millis(),
        prompt_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0),
        completion_tokens = usage["completion_tokens"].as_u64().unwrap_or(0),
        "model call finished"
    );
}

/// A call that never produced an answer.
pub fn model_call_failed(purpose: &str, model: &str, elapsed: Duration, err: &dyn std::fmt::Debug) {
    tracing::warn!(
        purpose,
        model,
        ms = elapsed.as_millis(),
        error = ?err,
        "model call failed"
    );
}
