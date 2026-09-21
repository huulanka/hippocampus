//! Who decides which candidates are really echoes.
//!
//! The bi-encoder ([`crate::embedding`]) finds candidates and is good at
//! it; what it cannot do is *order* them, because it turns each text into
//! a vector without ever seeing the other one. Something has to read the
//! new capture and a candidate together. That is this module.
//!
//! There are two ways to do that, and which one is right depends entirely
//! on the machine:
//!
//! - [`Judge::Local`] is the cross-encoder from ADR 0008
//!   (`bge-reranker-v2-m3`). On an M-series Mac it scores ten candidates
//!   in 1.3-1.5 s. On the Synology DS220+ this system actually runs on it
//!   takes **9.4 seconds per candidate** — 568M parameters in F32 against
//!   a Celeron J4025 with no AVX2, which works out to roughly 10 GFLOP
//!   per pair at roughly 1 GFLOP/s. Measured, not estimated: 17.7 s for
//!   two candidates, 27.9 s for three, 36.6 s for four.
//! - [`Judge::Remote`] asks a small hosted model, over the same
//!   OpenRouter path and the same Zero Data Retention routing the
//!   structuring step already uses. Fractions of a second, and better at
//!   the case this whole mechanism exists for — recognising that
//!   *finnischer Aufguss* belongs with *Sauna* is world knowledge, not
//!   text similarity.
//!
//! ADR 0006 says echo involves no LLM. That rule stands, and it is about
//! one thing: never put words in the user's mouth. A judge only ever
//! *selects and orders* candidates — what is displayed is still the
//! verbatim transcript, and this module cannot return anything else. See
//! ADR 0010.

use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::reranker::{self, Reranker};

/// Model asked to judge echoes when no other is configured.
///
/// Chosen for German: these are two-sentence German notes, and the one
/// case this system is known to have got wrong turned on German nuance.
/// A European model's training mix weights that far more heavily than the
/// cheaper alternatives, and at this volume — around 1000 input tokens
/// once per capture — the price difference between every candidate was
/// under 25 cents a month, so it was never the deciding variable.
pub const DEFAULT_JUDGE_MODEL: &str = "mistralai/mistral-small-2603";

/// Minimum score for a remote judge's verdict to be shown.
///
/// The remote judge answers on a 0-1 relevance scale it was asked for in
/// so many words, so unlike the cross-encoder's raw logits this number
/// means something on its own: half. Tunable via
/// `ECHO_MIN_RERANK_SCORE`, and every candidate's score is stored
/// regardless, so retuning it never costs a judgement.
pub const DEFAULT_REMOTE_MIN_SCORE: f32 = 0.5;

const SYSTEM_PROMPT: &str = "\
You judge whether earlier personal notes are genuine echoes of a new one.

An echo is an earlier note that the person would recognise as being about
the same thing: the same topic, plan, person, place, problem or recurring
habit. Notes that merely share a language, a tone, or a everyday word are
NOT echoes. Related by real-world knowledge counts — a note about a sauna
ritual and a note about a sauna visit are about the same thing even when
they share no words.

You will get one NEW note and a numbered list of EARLIER notes.

Return JSON only, in exactly this shape:
{\"scores\": [{\"id\": <number>, \"score\": <0.0-1.0>}, ...]}

Include EVERY earlier note exactly once, by its number. The score is how
strongly it echoes the new note: 1.0 unmistakably the same subject, 0.5
genuinely related, 0.0 unrelated. Be strict — showing an unrelated note
teaches the person to ignore echoes entirely, which is worse than showing
none. Output no prose, no explanation, no other keys.";

/// Which judge to use, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// A small hosted model over OpenRouter. The default, because the
    /// machine this runs on is a NAS.
    Remote,
    /// The local cross-encoder. Right on hardware that can afford it, and
    /// the only option that keeps capture text on the machine.
    Bge,
    /// No judging at all: echoes are ordered by embedding similarity,
    /// which is measurably the wrong order.
    Off,
}

impl FromStr for Choice {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "remote" | "llm" | "openrouter" => Ok(Self::Remote),
            "bge" => Ok(Self::Bge),
            "off" | "none" | "false" => Ok(Self::Off),
            "jina" => anyhow::bail!(
                "jina is no longer available — its architecture has no candle implementation \
                 (see docs/adr/0008-candle-not-onnxruntime.md); use remote, bge or off"
            ),
            other => anyhow::bail!("unknown judge {other}, expected remote, bge or off"),
        }
    }
}

/// The display threshold a choice calls for, before any judge is loaded.
///
/// The scales are not comparable — raw logits around -4 for the
/// cross-encoder, a 0-1 relevance for the remote judge — which is exactly
/// why this is a function of the choice rather than one constant.
pub fn default_min_score(choice: Choice) -> f32 {
    match choice {
        Choice::Remote => DEFAULT_REMOTE_MIN_SCORE,
        Choice::Bge => reranker::default_min_score(reranker::Choice::Bge),
        Choice::Off => f32::NEG_INFINITY,
    }
}

/// The two ways a candidate can be judged.
pub enum Judge {
    Local(Reranker),
    Remote(RemoteJudge),
}

impl Judge {
    /// Prepares the configured judge, or `None` when judging is off.
    ///
    /// A remote judge with no API key is a configuration mistake worth
    /// saying out loud rather than silently degrading to similarity
    /// ordering — that ordering is the bug this whole mechanism exists to
    /// fix, and a system that quietly falls back into it looks like it is
    /// working.
    pub async fn load(
        choice: Choice,
        cache_dir: PathBuf,
        api_key: Option<String>,
        model: String,
        zdr: bool,
    ) -> anyhow::Result<Option<Self>> {
        match choice {
            Choice::Off => Ok(None),
            Choice::Bge => Ok(Reranker::load(reranker::Choice::Bge, cache_dir)
                .await?
                .map(Judge::Local)),
            Choice::Remote => {
                let api_key = api_key.ok_or_else(|| {
                    anyhow::anyhow!(
                        "HIPPOCAMPUS_RERANKER is remote but OPENROUTER_API_KEY is unset — \
                         set the key, or choose bge (local) or off explicitly"
                    )
                })?;
                Ok(Some(Judge::Remote(RemoteJudge::new(api_key, model, zdr))))
            }
        }
    }

    /// Scores every document against the query, one score per document,
    /// **in the order they were given**.
    pub async fn score(&self, query: &str, documents: Vec<String>) -> anyhow::Result<Vec<f32>> {
        match self {
            Judge::Local(reranker) => reranker.score(query, documents).await,
            Judge::Remote(remote) => remote.score(query, documents).await,
        }
    }

    /// What to record as provenance on the stored echo.
    pub fn name(&self) -> String {
        match self {
            Judge::Local(reranker) => reranker.choice().to_string(),
            Judge::Remote(remote) => remote.model.clone(),
        }
    }
}

pub struct RemoteJudge {
    http: reqwest::Client,
    api_key: String,
    model: String,
    zdr: bool,
}

#[derive(Deserialize)]
struct Verdict {
    id: usize,
    score: f32,
}

#[derive(Deserialize)]
struct Verdicts {
    #[serde(default)]
    scores: Vec<Verdict>,
}

impl RemoteJudge {
    pub fn new(api_key: String, model: String, zdr: bool) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
            model,
            zdr,
        }
    }

    async fn score(&self, query: &str, documents: Vec<String>) -> anyhow::Result<Vec<f32>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let listing = documents
            .iter()
            .enumerate()
            .map(|(i, doc)| format!("{i}. {doc}"))
            .collect::<Vec<_>>()
            .join("\n");

        let body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": SYSTEM_PROMPT},
                {"role": "user", "content": format!("NEW note:\n{query}\n\nEARLIER notes:\n{listing}")},
            ],
            "response_format": {"type": "json_object"},
            // The same Zero Data Retention routing the structuring step
            // uses. This is the second place capture text leaves the
            // machine, and it must not be the laxer of the two.
            "provider": {"zdr": self.zdr},
        });

        let response = self
            .http
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("the judge answered with no message content"))?;

        parse_scores(content, documents.len())
    }
}

/// Turns the model's answer into one score per document.
///
/// A missing verdict scores 0 rather than failing the whole judgement: a
/// model that forgets one of ten candidates should cost that candidate
/// its place, not cost the capture its echo. Split out from the request
/// so it can be tested without a network.
fn parse_scores(content: &str, expected: usize) -> anyhow::Result<Vec<f32>> {
    let verdicts: Verdicts = serde_json::from_str(content.trim())
        .map_err(|err| anyhow::anyhow!("the judge did not answer with usable JSON: {err}"))?;

    let mut scores = vec![0.0_f32; expected];
    for verdict in verdicts.scores {
        if let Some(slot) = scores.get_mut(verdict.id) {
            // Clamped rather than trusted: a model that answers 5 on a
            // 0-1 scale would otherwise outrank everything forever.
            *slot = verdict.score.clamp(0.0, 1.0);
        }
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_come_back_in_document_order() {
        let scores = parse_scores(
            r#"{"scores":[{"id":2,"score":0.9},{"id":0,"score":0.1}]}"#,
            3,
        )
        .unwrap();
        assert_eq!(scores, vec![0.1, 0.0, 0.9]);
    }

    /// A candidate the model forgot to mention is unrelated, not a
    /// failure. Ten candidates and nine answers still produce an echo.
    #[test]
    fn a_missing_verdict_is_a_zero_not_an_error() {
        let scores = parse_scores(r#"{"scores":[{"id":0,"score":1.0}]}"#, 3).unwrap();
        assert_eq!(scores, vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn out_of_range_scores_are_clamped() {
        let scores = parse_scores(
            r#"{"scores":[{"id":0,"score":5.0},{"id":1,"score":-2.0}]}"#,
            2,
        )
        .unwrap();
        assert_eq!(scores, vec![1.0, 0.0]);
    }

    /// An id the model invented must not panic or shift the others.
    #[test]
    fn an_unknown_id_is_ignored() {
        let scores = parse_scores(
            r#"{"scores":[{"id":9,"score":1.0},{"id":1,"score":0.8}]}"#,
            2,
        )
        .unwrap();
        assert_eq!(scores, vec![0.0, 0.8]);
    }

    #[test]
    fn prose_instead_of_json_is_an_error() {
        assert!(parse_scores("I think the second one matches.", 2).is_err());
    }

    /// An empty but valid answer is "nothing echoes", which is a real and
    /// common outcome — most notes echo nothing.
    #[test]
    fn an_empty_verdict_list_means_nothing_echoed() {
        assert_eq!(parse_scores(r#"{"scores":[]}"#, 2).unwrap(), vec![0.0, 0.0]);
    }
}
