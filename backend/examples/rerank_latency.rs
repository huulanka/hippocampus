//! How long a cross-encoder takes to rerank an echo candidate set.
//!
//! Echo runs inline, right after a capture — it is the one thing the user
//! is waiting to see. A reranker that improves quality but costs two
//! seconds has not improved anything, so this is measured before it goes
//! anywhere near the request path.
//!
//! Run with: `cargo run --release --example rerank_latency [jina|bge]`

use std::time::Instant;

use fastembed::{RerankInitOptions, RerankerModel, TextRerank};

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
    let which = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "jina".to_string());
    let model = match which.as_str() {
        "bge" => RerankerModel::BGERerankerV2M3,
        _ => RerankerModel::JINARerankerV2BaseMultiligual,
    };

    let cache_dir = std::env::var("MODEL_CACHE_DIR").unwrap_or_else(|_| "../data/models".into());

    let started = Instant::now();
    let mut model =
        TextRerank::try_new(RerankInitOptions::new(model).with_cache_dir(cache_dir.into()))?;
    println!("{which}: loaded in {:.1?}", started.elapsed());

    // The first pass includes ONNX graph warm-up, which a long-running
    // server pays once at startup and never again. Both numbers matter,
    // for different reasons.
    for (label, count) in [("cold", 10usize), ("warm", 10), ("warm", 3), ("warm", 1)] {
        let candidates: Vec<&str> = CANDIDATES.iter().take(count).copied().collect();
        let started = Instant::now();
        let results = model.rerank(QUERY, candidates.clone(), false, None)?;
        println!(
            "{which}: {label} rerank of {count:2} candidates took {:>8.1?}  best: {:.3} {:?}",
            started.elapsed(),
            results[0].score,
            &candidates[results[0].index][..candidates[results[0].index].len().min(40)],
        );
    }

    Ok(())
}
