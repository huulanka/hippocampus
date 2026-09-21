//! How long the cross-encoder takes to rerank an echo candidate set.
//!
//! Echo runs inline, right after a capture — it is the one thing the user
//! is waiting to see. A reranker that improves quality but costs two
//! seconds has not improved anything, so this is measured before it goes
//! anywhere near the request path. BGE is the only reranker left after the
//! move to candle (see docs/adr/0008-candle-not-onnxruntime.md) — this
//! file used to compare it against Jina; there is nothing left to compare.
//!
//! Run with: `cargo run --release --example rerank_latency`

use std::time::Instant;

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::xlm_roberta::{Config, XLMRobertaForSequenceClassification};
use hf_hub::api::sync::ApiBuilder;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer};

const MODEL_ID: &str = "BAAI/bge-reranker-v2-m3";

/// Sized like a real echo lookup: the bi-encoder hands over its top N,
/// the cross-encoder scores them against the new capture.
const CANDIDATES: &[&str] = &[
    "Heut Abend war ich in der Sauna",
    "Mensch der finnische Aufguss heute Abend war wirklich zu heiß, die hat da zu viel Wasser gekippt und alle sind geflohen",
    "heute cardamom buns gemacht",
    "Ich habe gerade mit Lena über Tourenplanung gesprochen",
    "Rezept-Idee: Kaffee mit Kardamom rösten",
    "Go live bei Northwind für das Abrechnung",
    "Ich sollte morgen dringend in den Getränkemarkt",
    "Der Rasenmäher muss zum Service",
    "Idee für Hippocampus: ein wöchentlicher Rückblick",
    "Am Wochenende war ich lange draußen unterwegs",
];

const QUERY: &str = "Also Sauna heut war schon geil";

fn main() -> anyhow::Result<()> {
    let cache = std::env::var("MODEL_CACHE_DIR").unwrap_or_else(|_| "../data/models".into());
    let device = Device::Cpu;

    let started = Instant::now();
    let api = ApiBuilder::new().with_cache_dir(cache.into()).build()?;
    let repo = api.model(MODEL_ID.to_string());
    let config: Config = serde_json::from_str(&std::fs::read_to_string(repo.get("config.json")?)?)?;
    let mut tokenizer =
        Tokenizer::from_file(repo.get("tokenizer.json")?).map_err(anyhow::Error::msg)?;
    tokenizer
        .with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_id: config.pad_token_id,
            ..Default::default()
        }))
        .with_truncation(None)
        .map_err(anyhow::Error::msg)?;
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[repo.get("model.safetensors")?], DType::F32, &device)?
    };
    let model = XLMRobertaForSequenceClassification::new(1, &config, vb)?;
    println!("bge: loaded in {:.1?}", started.elapsed());

    // The first pass includes candle's own warm-up costs (weight mmap
    // paging in, kernel selection), which a long-running server pays once
    // at startup and never again. Both numbers matter, for different
    // reasons.
    for (label, count) in [("cold", 10usize), ("warm", 10), ("warm", 3), ("warm", 1)] {
        let documents: Vec<&str> = CANDIDATES.iter().take(count).copied().collect();
        let started = Instant::now();

        let pairs: Vec<(String, String)> = documents
            .iter()
            .map(|doc| (QUERY.to_string(), doc.to_string()))
            .collect();
        let encodings = tokenizer
            .encode_batch(pairs, true)
            .map_err(anyhow::Error::msg)?;
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

        let logits = model
            .forward(&input_ids, &attention_mask, &token_type_ids)?
            .squeeze(1)?
            .to_vec1::<f32>()?;

        let (best_idx, best_score) = logits
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, s)| (i, *s))
            .expect("at least one candidate");

        println!(
            "bge: {label} rerank of {count:2} candidates took {:>8.1?}  best: {:.3} {:?}",
            started.elapsed(),
            best_score,
            &documents[best_idx][..documents[best_idx].len().min(40)],
        );
    }

    Ok(())
}
