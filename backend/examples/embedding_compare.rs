//! Sanity check for the embedder: confirms the bi-encoder alone still
//! cannot separate these four sentences correctly, the way `ort`'s
//! multilingual-e5-small could not either — a regression check that the
//! candle port behaves like its predecessor, not a new investigation.
//!
//! Exists because a threshold tuned on thirteen captures was wrong within
//! an hour of meeting real ones. Before trusting a model, run this on
//! sentences whose relatedness you already know, and look at the *order*
//! rather than the numbers: a model that ranks a wrong pair above a right
//! one cannot be rescued by a threshold.
//!
//!     cargo run --release --example embedding_compare

use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use hf_hub::api::sync::ApiBuilder;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer};

const MODEL_ID: &str = "intfloat/multilingual-e5-small";

/// Four captures from a real evening. The sauna pair is obvious, the
/// Aufguss belongs with them without sharing a word, and the buns belong
/// nowhere near any of them.
const SENTENCES: &[&str] = &[
    "Heut Abend war ich in der Sauna",
    "Also Sauna heut war schon geil",
    "Mensch der finnische Aufguss heute Abend war wirkich zu heiss, die hat da zu viel Wasser gekippt und alle sind geflohen - lol",
    "heute cardamom buns gemacht",
];

/// Which pairs a useful model must rank above which others.
const EXPECTED_ORDER: &[(&str, usize, usize)] = &[
    ("sauna ~ sauna      ", 0, 1),
    ("sauna ~ aufguss    ", 1, 2),
    ("sauna ~ aufguss    ", 0, 2),
    ("sauna ~ buns  (nein)", 1, 3),
    ("sauna ~ buns  (nein)", 0, 3),
    ("aufguss ~ buns (nein)", 2, 3),
];

fn main() -> anyhow::Result<()> {
    let cache = std::env::var("MODEL_CACHE_DIR").unwrap_or_else(|_| "../data/models".to_string());

    eprintln!("loading {MODEL_ID} (first run downloads it)...");
    let started = std::time::Instant::now();

    let api = ApiBuilder::new().with_cache_dir(cache.into()).build()?;
    let repo = api.model(MODEL_ID.to_string());
    let config: Config = serde_json::from_str(&std::fs::read_to_string(repo.get("config.json")?)?)?;
    let mut tokenizer =
        Tokenizer::from_file(repo.get("tokenizer.json")?).map_err(anyhow::Error::msg)?;
    tokenizer.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::BatchLongest,
        pad_id: config.pad_token_id as u32,
        ..Default::default()
    }));
    let device = Device::Cpu;
    let vb = unsafe {
        VarBuilder::from_mmaped_safetensors(&[repo.get("model.safetensors")?], DTYPE, &device)?
    };
    let model = BertModel::load(vb, &config)?;
    eprintln!("loaded in {:.1}s", started.elapsed().as_secs_f32());

    // E5 wants its prefixes; stored captures are passages.
    let passages: Vec<String> = SENTENCES.iter().map(|s| format!("passage: {s}")).collect();
    let encodings = tokenizer
        .encode_batch(passages, true)
        .map_err(anyhow::Error::msg)?;

    let token_ids = encodings
        .iter()
        .map(|e| Tensor::new(e.get_ids(), &device))
        .collect::<candle_core::Result<Vec<_>>>()?;
    let token_ids = Tensor::stack(&token_ids, 0)?;
    let attention_mask = encodings
        .iter()
        .map(|e| Tensor::new(e.get_attention_mask(), &device))
        .collect::<candle_core::Result<Vec<_>>>()?;
    let attention_mask = Tensor::stack(&attention_mask, 0)?;
    let token_type_ids = token_ids.zeros_like()?;

    let output = model.forward(&token_ids, &token_type_ids, Some(&attention_mask))?;
    // Masked mean: several sentences are batched together now, so shorter
    // ones are padded — including those padding embeddings in the mean
    // would pull every vector toward whatever the pad token encodes as.
    let mask_f32 = attention_mask.to_dtype(DTYPE)?.unsqueeze(2)?;
    let sum_mask = mask_f32.sum(1)?;
    let pooled = (output.broadcast_mul(&mask_f32)?.sum(1)?).broadcast_div(&sum_mask)?;
    let normalized = pooled.broadcast_div(&pooled.sqr()?.sum_keepdim(1)?.sqrt()?)?;
    let vectors = normalized.to_vec2::<f32>()?;

    println!("\n=== e5-small via candle ({} dims) ===", vectors[0].len());
    for (label, a, b) in EXPECTED_ORDER {
        println!("{label}  {:.3}", cosine(&vectors[*a], &vectors[*b]));
    }

    let right = cosine(&vectors[1], &vectors[2]).min(cosine(&vectors[0], &vectors[2]));
    let wrong = cosine(&vectors[1], &vectors[3]).max(cosine(&vectors[0], &vectors[3]));
    println!(
        "{}  worst true match {right:.3} vs best false match {wrong:.3}",
        if right > wrong {
            "ORDER OK  "
        } else {
            "ORDER WRONG"
        }
    );

    Ok(())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}
