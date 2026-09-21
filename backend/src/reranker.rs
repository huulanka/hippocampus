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
//! **BGE is now the only cross-encoder, not by choice of quality or
//! speed.** Jina was the default through 2026-09-21 — faster (178 ms vs
//! BGE's 605 ms for ten candidates, `examples/rerank_latency.rs`), same
//! order-correctness. It was dropped when the backend moved off `ort`
//! (ONNX Runtime) onto `candle` (see docs/adr/0008), because
//! jina-reranker-v2's architecture (custom modeling code, ALiBi-adjacent
//! attention, `trust_remote_code`) has no candle implementation, while
//! bge-reranker-v2-m3 is a plain `XLMRobertaForSequenceClassification` —
//! directly supported. Slower echo, but it runs at all on hardware `ort`
//! could not touch. Re-measure `examples/rerank_latency.rs` against the
//! candle implementation before trusting the 605 ms figure going forward;
//! it was measured against `ort`.
//!
//! Retrieval stays with the bi-encoder: it is cheap, it is already in the
//! database, and recall is the one thing it is good at. The cross-encoder
//! only re-sorts and filters what it hands over — so nothing had to be
//! re-embedded to get this.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::xlm_roberta::{
    Config as XlmRobertaConfig, XLMRobertaForSequenceClassification,
};
use hf_hub::api::sync::ApiBuilder;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer};
use tokio::sync::Mutex;

const BGE_MODEL_ID: &str = "BAAI/bge-reranker-v2-m3";

/// Which cross-encoder to load, or none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Bge,
    Off,
}

impl FromStr for Choice {
    type Err = anyhow::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "bge" => Ok(Self::Bge),
            "off" | "none" | "false" => Ok(Self::Off),
            "jina" => anyhow::bail!(
                "jina is no longer available — its architecture has no candle implementation \
                 (see docs/adr/0008-candle-not-onnxruntime.md); use bge or off"
            ),
            other => anyhow::bail!("unknown reranker {other}, expected bge or off"),
        }
    }
}

/// Score below which an echo is not worth showing.
///
/// Raw logits, not probabilities. Jina's calibration (below) is history —
/// kept because it is the methodology, not because the number still
/// applies to anything running today. Jina scored roughly -2 to +0.1 for a
/// true match; BGE, measured against real captures after the candle
/// migration, scores on a much wider, more positive scale. The two are
/// not comparable, which is the whole reason this is a function of
/// `Choice` rather than one constant.
///
/// **Jina, retired 2026-09-21** — calibrated against the real 38-capture
/// corpus, one echo lookup per capture (`?min_rerank=-99` on the echo
/// endpoint prints the scores for exactly this purpose). The decisive
/// pairs:
///
/// | pair | score | should show |
/// | --- | --- | --- |
/// | "Also Sauna heut war schon geil" ↔ "Heut Abend war ich in der Sauna" | +0.02 | yes |
/// | "Sauna war ein guter Abend" ↔ "finnischer Aufguss war zu heiß" | -2.02 | yes |
/// | "Also Sauna heut war schon geil" ↔ "finnischer Aufguss" | -2.03 | yes |
/// | "heute cardamom buns gemacht" ↔ "Heut Abend war ich in der Sauna" | -2.15 | **no** |
/// | "finnischer Aufguss" ↔ "Rasenmäher muss zum Service" | -3.23 | no |
///
/// -2.0 sat in the gap, and the gap is where the user's own complaint
/// lived: with cosine similarity, *cardamom buns ↔ sauna* (0.871) outranked
/// *Aufguss ↔ sauna* (0.849). It no longer did.
///
/// **BGE, measured 2026-09-21** against real captures through the actual
/// `/captures/{id}/echo?min_rerank=-99` endpoint, post-migration:
///
/// | query | candidate | score | should show |
/// | --- | --- | --- | --- |
/// | "Heute war der Go Live des Northwind Abrechnungsprojektes..." | "Go live bei Northwind für das Abrechnungs Projekt war heute..." (same event, same day) | +0.193 | yes |
/// | same query | "Ich muss morgen für Northwind das Mock Up machen..." (different topic, shares only the client name) | -9.321 | **no** |
/// | same query | "Ich habe heute an dem Hippocampus Projekt gearbeitet..." (unrelated) | -10.329 | no |
/// | a note about the weekend | "Go live bei Northwind..." (unrelated) | -8.373 | no |
/// | same query | "Kardamom-Espresso probiert..." (unrelated) | -9.057 | no |
/// | same query | "...Hippocampus Projekt gearbeitet..." (unrelated) | -9.412 | no |
///
/// One true match so far, not five — this is a start, not the corpus-wide
/// calibration Jina got. But five false matches now cluster tightly in
/// -8.4 to -10.3, against the one true match at +0.19: -4.0 sits in the
/// middle of that gap with room either direction. Re-run this against more
/// captures — especially more true matches — as they accumulate, the way
/// -2.0 was tightened for Jina.
///
/// (Testing this locally: use `cargo run --release`, not plain `cargo
/// run`. candle's matmul is compiled unoptimized in debug builds — a
/// single ten-candidate rerank that takes ~1.3s in release took over
/// three minutes in debug, indistinguishable from a hang from the
/// outside. Not a bug; just don't chase it as one again.)
pub fn default_min_score(choice: Choice) -> f32 {
    match choice {
        Choice::Bge => -4.0,
        Choice::Off => f32::NEG_INFINITY,
    }
}

struct Loaded {
    model: XLMRobertaForSequenceClassification,
    tokenizer: Tokenizer,
}

#[derive(Clone)]
pub struct Reranker {
    state: Arc<Mutex<Loaded>>,
    choice: Choice,
}

impl Reranker {
    /// Loads the model, or returns `None` when reranking is switched off.
    pub async fn load(choice: Choice, cache_dir: PathBuf) -> anyhow::Result<Option<Self>> {
        if choice == Choice::Off {
            return Ok(None);
        }

        let loaded = tokio::task::spawn_blocking(move || -> anyhow::Result<Loaded> {
            let api = ApiBuilder::new().with_cache_dir(cache_dir).build()?;
            let repo = api.model(BGE_MODEL_ID.to_string());

            let config_path = repo.get("config.json")?;
            let tokenizer_path = repo.get("tokenizer.json")?;
            let weights_path = repo.get("model.safetensors")?;

            let config: XlmRobertaConfig =
                serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
            let mut tokenizer = Tokenizer::from_file(tokenizer_path)
                .map_err(|err| anyhow::anyhow!("loading tokenizer: {err}"))?;
            tokenizer
                .with_padding(Some(PaddingParams {
                    strategy: PaddingStrategy::BatchLongest,
                    pad_id: config.pad_token_id,
                    ..Default::default()
                }))
                .with_truncation(None)
                .map_err(|err| anyhow::anyhow!("configuring tokenizer padding: {err}"))?;

            let device = Device::Cpu;
            // F32, matching the checkpoint's own `torch_dtype` — unlike
            // candle's xlm-roberta example, which loads as F16 to halve
            // memory. Keeping the model's native precision means one
            // fewer thing to account for while re-deriving the
            // calibration this reranker still needs.
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &device)?
            };
            let model = XLMRobertaForSequenceClassification::new(1, &config, vb)?;

            Ok(Loaded { model, tokenizer })
        })
        .await??;

        Ok(Some(Self {
            state: Arc::new(Mutex::new(loaded)),
            choice,
        }))
    }

    pub fn choice(&self) -> Choice {
        self.choice
    }

    /// Scores every document against the query, returning one score per
    /// document **in the order they were given**.
    pub async fn score(&self, query: &str, documents: Vec<String>) -> anyhow::Result<Vec<f32>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let state = self.state.clone();
        let query = query.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<f32>> {
            let state = state.blocking_lock();
            let device = Device::Cpu;

            let pairs: Vec<(String, String)> = documents
                .into_iter()
                .map(|doc| (query.clone(), doc))
                .collect();
            let encodings = state
                .tokenizer
                .encode_batch(pairs, true)
                .map_err(|err| anyhow::anyhow!("tokenizing: {err}"))?;

            let input_ids = encodings
                .iter()
                .map(|e| Tensor::new(e.get_ids(), &device))
                .collect::<candle_core::Result<Vec<_>>>()?;
            let input_ids = Tensor::stack(&input_ids, 0)?;

            let attention_mask = encodings
                .iter()
                .map(|e| Tensor::new(e.get_attention_mask(), &device))
                .collect::<candle_core::Result<Vec<_>>>()?;
            let attention_mask = Tensor::stack(&attention_mask, 0)?;

            let token_type_ids = input_ids.zeros_like()?;

            // One raw logit per pair — deliberately not passed through
            // sigmoid, so it stays comparable to the thresholds this
            // module has always used (see `default_min_score`).
            let logits = state
                .model
                .forward(&input_ids, &attention_mask, &token_type_ids)?
                .to_dtype(candle_core::DType::F32)?;

            Ok(logits.squeeze(1)?.to_vec1::<f32>()?)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_names_a_person_would_write() {
        assert_eq!("BGE".parse::<Choice>().unwrap(), Choice::Bge);
        assert_eq!(" off ".parse::<Choice>().unwrap(), Choice::Off);
        assert!("gpt".parse::<Choice>().is_err());
    }

    #[test]
    fn jina_is_rejected_with_an_explanation_rather_than_silently_misread() {
        let err = "jina".parse::<Choice>().unwrap_err().to_string();
        assert!(err.contains("no longer available"), "{err}");
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
