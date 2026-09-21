//! Local semantic embeddings, computed in-process via candle — pure Rust,
//! no prebuilt ONNX Runtime binary. See
//! docs/adr/0008-candle-not-onnxruntime.md for why: `ort`'s prebuilt
//! binary for x86_64 Linux hard-requires AVX2 and crashes with SIGILL,
//! before any log line, on the low-power Atom-family CPUs common in NAS
//! boxes. Runs synchronously on a blocking thread since candle's forward
//! pass is CPU-bound and not async.

use std::path::PathBuf;
use std::sync::Arc;

use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig, DTYPE};
use hf_hub::api::sync::ApiBuilder;
use tokenizers::Tokenizer;
use tokio::sync::Mutex;

/// intfloat/multilingual-e5-small: 384 dims, handles German + English well
/// enough for a personal note archive, small enough to run on a weak NAS.
const DIMENSIONS: usize = 384;
const MODEL_ID: &str = "intfloat/multilingual-e5-small";

struct Loaded {
    model: BertModel,
    tokenizer: Tokenizer,
}

#[derive(Clone)]
pub struct Embedder(Arc<Mutex<Loaded>>);

impl Embedder {
    pub async fn load(cache_dir: PathBuf) -> anyhow::Result<Self> {
        let loaded = tokio::task::spawn_blocking(move || -> anyhow::Result<Loaded> {
            let api = ApiBuilder::new().with_cache_dir(cache_dir).build()?;
            let repo = api.model(MODEL_ID.to_string());

            let config_path = repo.get("config.json")?;
            let tokenizer_path = repo.get("tokenizer.json")?;
            let weights_path = repo.get("model.safetensors")?;

            let config: BertConfig = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
            let tokenizer = Tokenizer::from_file(tokenizer_path)
                .map_err(|err| anyhow::anyhow!("loading tokenizer: {err}"))?;

            // CPU only, deliberately: this is the code path that used to
            // crash on a NAS without a GPU, so a device that only exists
            // on this developer's Mac would prove nothing.
            let device = Device::Cpu;
            let vb =
                unsafe { VarBuilder::from_mmaped_safetensors(&[weights_path], DTYPE, &device)? };
            let model = BertModel::load(vb, &config)?;

            Ok(Loaded { model, tokenizer })
        })
        .await??;

        Ok(Self(Arc::new(Mutex::new(loaded))))
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
        let state = self.0.clone();
        let embedding = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<f32>> {
            let state = state.blocking_lock();
            let device = &state.model.device;

            let encoding = state
                .tokenizer
                .encode(prefixed, true)
                .map_err(|err| anyhow::anyhow!("tokenizing: {err}"))?;
            let token_ids = Tensor::new(encoding.get_ids(), device)?.unsqueeze(0)?;
            let token_type_ids = token_ids.zeros_like()?;

            // A single, unpadded sequence — there is no attention mask to
            // apply, every token is real, so plain mean pooling over all
            // of them is the masked mean (mirrors
            // candle's bert example, specialized to batch size 1).
            let output = state.model.forward(&token_ids, &token_type_ids, None)?;
            let (_batch, n_tokens, _hidden) = output.dims3()?;
            let pooled = (output.sum(1)? / (n_tokens as f64))?;
            let normalized = pooled.broadcast_div(&pooled.sqr()?.sum_keepdim(1)?.sqrt()?)?;

            Ok(normalized.get(0)?.to_vec1::<f32>()?)
        })
        .await??;

        debug_assert_eq!(embedding.len(), DIMENSIONS);
        Ok(embedding)
    }
}
