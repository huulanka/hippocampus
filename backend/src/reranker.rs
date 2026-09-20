//! Cross-encoder reranking for echo.
//!
//! The bi-encoder in `embedding.rs` compares two vectors that have never
//! seen each other. Measured against real captures on 2026-09-20, that is
//! not good enough for echo: every E5 variant scored *cardamom buns ↔
//! sauna* **above** *finnischer Aufguss ↔ sauna*, and no threshold
//! separates them (`examples/embedding_compare.rs`). A bigger embedding
//! model did not fix it — e5-large was marginally worse.
//!
//! A cross-encoder reads both texts together, and the same four sentences
//! separate by more than an order of magnitude more:
//!
//! | candidate | worst true | best false | margin |
//! | --- | --- | --- | --- |
//! | e5 small/base/large | 0.830 | 0.892-0.899 | wrong order |
//! | ParaphraseMLMpnetBaseV2 | 0.562 | 0.561 | +0.001 |
//! | EmbeddingGemma300M | 0.517 | 0.503 | +0.014 |
//! | JINARerankerV2BaseMultiligual | -2.156 | -3.513 | **+1.357** |
//! | BGERerankerV2M3 | -6.529 | -8.338 | **+1.809** |
//!
//! Jina is the default rather than BGE because echo runs inline, right
//! after a capture, and the user is waiting for it. Measured on this
//! machine (`examples/rerank_latency.rs`), ten candidates:
//! **Jina 178 ms, BGE 605 ms** — with model files of 1.1 GB and 2.1 GB.
//! Both order the sentences correctly; only one of them is free enough to
//! sit in the request path.
//!
//! Retrieval stays with the bi-encoder: it is cheap, it is already in the
//! database, and recall is the one thing it is good at. The cross-encoder
//! only re-sorts and filters what it hands over — so nothing had to be
//! re-embedded to get this.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use fastembed::{RerankInitOptions, RerankerModel, TextRerank};
use tokio::sync::Mutex;

/// Which cross-encoder to load, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Jina,
    Bge,
    Off,
}

impl FromStr for Choice {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "jina" => Ok(Self::Jina),
            "bge" => Ok(Self::Bge),
            "off" | "none" | "false" => Ok(Self::Off),
            other => anyhow::bail!("unknown reranker {other}, expected jina, bge or off"),
        }
    }
}

impl Choice {
    fn model(self) -> Option<RerankerModel> {
        match self {
            Self::Jina => Some(RerankerModel::JINARerankerV2BaseMultiligual),
            Self::Bge => Some(RerankerModel::BGERerankerV2M3),
            Self::Off => None,
        }
    }
}

/// Score below which an echo is not worth showing.
///
/// Raw logits, not probabilities, and not comparable between the two
/// models — which is why each carries its own.
///
/// The Jina value is calibrated against the real 38-capture corpus, one
/// echo lookup per capture (`?min_rerank=-99` on the echo endpoint prints
/// the scores for exactly this purpose). The decisive pairs:
///
/// | pair | score | should show |
/// | --- | --- | --- |
/// | "Also Sauna heut war schon geil" ↔ "Heut Abend war ich in der Sauna" | +0.02 | yes |
/// | "Sauna war ein guter Abend" ↔ "finnischer Aufguss war zu heiß" | -2.02 | yes |
/// | "Also Sauna heut war schon geil" ↔ "finnischer Aufguss" | -2.03 | yes |
/// | "heute cardamom buns gemacht" ↔ "Heut Abend war ich in der Sauna" | -2.15 | **no** |
/// | "finnischer Aufguss" ↔ "Rasenmäher muss zum Service" | -3.23 | no |
///
/// -2.0 sits in the gap, and the gap is where the user's own complaint
/// lived: with cosine similarity, *cardamom buns ↔ sauna* (0.871) outranked
/// *Aufguss ↔ sauna* (0.849). It no longer does.
///
/// The gap is 0.12 wide, which is not much. Some unrelated pairs still
/// clear -2.0, so the honest claim is that the *ordering* is now
/// trustworthy and the cut-off is approximately right — not that it is
/// clean. `ECHO_MIN_RERANK_SCORE` overrides it per deployment and
/// `?min_rerank=` per request, because the right value will drift as the
/// corpus grows.
///
/// The BGE value is **not** calibrated against real captures; it is scaled
/// from the four-sentence comparison in `examples/embedding_compare.rs`.
/// Re-measure before relying on it.
pub fn default_min_score(choice: Choice) -> f32 {
    match choice {
        Choice::Jina => -2.0,
        Choice::Bge => -7.5,
        Choice::Off => f32::NEG_INFINITY,
    }
}

#[derive(Clone)]
pub struct Reranker {
    model: Arc<Mutex<TextRerank>>,
    choice: Choice,
}

impl Reranker {
    /// Loads the model, or returns `None` when reranking is switched off.
    pub async fn load(choice: Choice, cache_dir: PathBuf) -> anyhow::Result<Option<Self>> {
        let Some(model) = choice.model() else {
            return Ok(None);
        };

        let model = tokio::task::spawn_blocking(move || {
            TextRerank::try_new(RerankInitOptions::new(model).with_cache_dir(cache_dir))
        })
        .await??;

        Ok(Some(Self {
            model: Arc::new(Mutex::new(model)),
            choice,
        }))
    }

    pub fn choice(&self) -> Choice {
        self.choice
    }

    /// Scores every document against the query, returning one score per
    /// document **in the order they were given**.
    ///
    /// `fastembed` returns them sorted by score with the original index
    /// attached; putting them back in input order here keeps the caller
    /// from having to care, and makes a mis-indexed result impossible to
    /// paper over.
    pub async fn score(&self, query: &str, documents: Vec<String>) -> anyhow::Result<Vec<f32>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let model = self.model.clone();
        let query = query.to_string();
        let count = documents.len();

        let results = tokio::task::spawn_blocking(move || {
            let mut model = model.blocking_lock();
            model.rerank(query, &documents, false, None)
        })
        .await??;

        let mut scores = vec![f32::NEG_INFINITY; count];
        for result in results {
            let slot = scores
                .get_mut(result.index)
                .ok_or_else(|| anyhow::anyhow!("reranker returned an out-of-range index"))?;
            *slot = result.score;
        }

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_names_a_person_would_write() {
        assert_eq!("jina".parse::<Choice>().unwrap(), Choice::Jina);
        assert_eq!("BGE".parse::<Choice>().unwrap(), Choice::Bge);
        assert_eq!(" off ".parse::<Choice>().unwrap(), Choice::Off);
        assert!("gpt".parse::<Choice>().is_err());
    }

    #[test]
    fn a_disabled_reranker_never_filters_anything_out() {
        // NEG_INFINITY rather than a low number: a threshold that happens
        // to sit above some real score would silently drop echoes on a
        // deployment that asked for no reranking at all.
        assert_eq!(default_min_score(Choice::Off), f32::NEG_INFINITY);
        assert!(-99.0_f32 > default_min_score(Choice::Off));
    }
}
