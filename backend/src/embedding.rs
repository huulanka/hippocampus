//! Local semantic embeddings, computed in-process via ONNX (no separate
//! service/container). Runs synchronously on a blocking thread since
//! `fastembed`'s `embed()` call is CPU-bound and not async.

use std::path::PathBuf;
use std::sync::Arc;

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use tokio::sync::Mutex;

/// intfloat/multilingual-e5-small: 384 dims, handles German + English well
/// enough for a personal note archive, small enough to run on a weak NAS.
const DIMENSIONS: usize = 384;

#[derive(Clone)]
pub struct Embedder(Arc<Mutex<TextEmbedding>>);

impl Embedder {
    pub async fn load(cache_dir: PathBuf) -> anyhow::Result<Self> {
        let model = tokio::task::spawn_blocking(move || {
            TextEmbedding::try_new(
                TextInitOptions::new(EmbeddingModel::MultilingualE5Small).with_cache_dir(cache_dir),
            )
        })
        .await??;

        Ok(Self(Arc::new(Mutex::new(model))))
    }

    /// Embeds a single piece of stored content ("passage") for later
    /// retrieval, per the E5 model's expected input prefixing.
    pub async fn embed_passage(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        self.embed_one(format!("passage: {text}")).await
    }

    /// Embeds a search query, per the E5 model's expected input prefixing.
    pub async fn embed_query(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        self.embed_one(format!("query: {text}")).await
    }

    async fn embed_one(&self, prefixed: String) -> anyhow::Result<Vec<f32>> {
        let model = self.0.clone();
        let embedding = tokio::task::spawn_blocking(move || {
            let mut model = model.blocking_lock();
            model.embed(vec![prefixed], None)
        })
        .await??
        .pop()
        .ok_or_else(|| anyhow::anyhow!("fastembed returned no embedding"))?;

        debug_assert_eq!(embedding.len(), DIMENSIONS);
        Ok(embedding)
    }
}
