//! Compares embedding models against real sentences.
//!
//! Exists because a threshold tuned on thirteen captures was wrong within
//! an hour of meeting real ones. Before any model change, run this on
//! sentences whose relatedness you already know, and look at the *order*
//! rather than the numbers: a model that ranks a wrong pair above a right
//! one cannot be rescued by a threshold.
//!
//!     cargo run --release --example embedding_compare -- small base large

use fastembed::{
    EmbeddingModel, RerankInitOptions, RerankerModel, TextEmbedding, TextInitOptions, TextRerank,
};

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
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let wanted = if wanted.is_empty() {
        vec!["small".to_string()]
    } else {
        wanted
    };

    let cache = std::env::var("MODEL_CACHE_DIR").unwrap_or_else(|_| "../data/models".to_string());

    for name in &wanted {
        let model = match name.as_str() {
            "small" => EmbeddingModel::MultilingualE5Small,
            "base" => EmbeddingModel::MultilingualE5Base,
            "large" => EmbeddingModel::MultilingualE5Large,
            // A different family: trained on paraphrase pairs rather than
            // query/passage retrieval, which is what an echo actually is.
            "paraphrase" => EmbeddingModel::ParaphraseMLMpnetBaseV2,
            "bgem3" => EmbeddingModel::BGEM3,
            "gemma" => EmbeddingModel::EmbeddingGemma300M,
            // Cross-encoders read both texts together instead of comparing
            // two vectors that never met.
            "rerank-bge" => {
                rerank(&cache, RerankerModel::BGERerankerV2M3, name)?;
                continue;
            }
            "rerank-jina" => {
                rerank(&cache, RerankerModel::JINARerankerV2BaseMultiligual, name)?;
                continue;
            }
            other => anyhow::bail!(
                "unknown model {other}; use small, base, large, paraphrase, bgem3, gemma, \
                 rerank-bge or rerank-jina"
            ),
        };

        eprintln!("loading {name} (first run downloads it)...");
        let started = std::time::Instant::now();
        let mut embedder = TextEmbedding::try_new(
            TextInitOptions::new(model).with_cache_dir(cache.clone().into()),
        )?;
        eprintln!("loaded in {:.1}s", started.elapsed().as_secs_f32());

        // E5 wants its prefixes; stored captures are passages.
        let passages: Vec<String> = SENTENCES.iter().map(|s| format!("passage: {s}")).collect();
        let vectors = embedder.embed(passages, None)?;

        println!("\n=== {name} ({} dims) ===", vectors[0].len());
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
    }

    Ok(())
}

/// Scores every sentence against the newest one, the way echo would.
fn rerank(cache: &str, model: RerankerModel, name: &str) -> anyhow::Result<()> {
    eprintln!("loading {name} (first run downloads it)...");
    let started = std::time::Instant::now();
    let mut reranker = TextRerank::try_new(
        RerankInitOptions::new(model).with_cache_dir(cache.to_string().into()),
    )?;
    eprintln!("loaded in {:.1}s", started.elapsed().as_secs_f32());

    // SENTENCES[1] is the newest capture; the other three are what it
    // would be compared against.
    let query = SENTENCES[1];
    let documents: Vec<&str> = vec![SENTENCES[0], SENTENCES[2], SENTENCES[3]];
    let labels = ["sauna   (ja) ", "aufguss (ja) ", "buns    (nein)"];

    let results = reranker.rerank(query, &documents, false, None)?;

    println!("\n=== {name} ===");
    let mut scores = [0.0f32; 3];
    for result in &results {
        scores[result.index] = result.score;
    }
    for (label, score) in labels.iter().zip(scores) {
        println!("{label}  {score:+.3}");
    }

    let worst_true = scores[0].min(scores[1]);
    println!(
        "{}  worst true match {worst_true:+.3} vs false match {:+.3}",
        if worst_true > scores[2] {
            "ORDER OK  "
        } else {
            "ORDER WRONG"
        },
        scores[2]
    );
    Ok(())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}
