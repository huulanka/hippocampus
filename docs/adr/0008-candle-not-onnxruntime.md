# ADR 0008: candle, not ONNX Runtime, for the backend's local inference

## Status
Accepted (2026-09-21)

## Context
The backend ran its embedding model (`intfloat/multilingual-e5-small`) and
its cross-encoder reranker through `fastembed`, a wrapper around `ort`
(ONNX Runtime bindings). Deployed to the user's Synology DS220+ — an
Intel Celeron J4025, Goldmont Plus microarchitecture — the backend
container exited immediately with code 132 (SIGILL) on every start, before
a single log line was written.

`ort`'s prebuilt ONNX Runtime binary for `x86_64-unknown-linux-gnu` comes
in exactly one variant (`ort-sys`'s `dist.tsv` lists no AVX-less
alternative), and Microsoft's official builds assume AVX2. Goldmont Plus —
like every Atom-derived Intel core up through this generation — has no
AVX2, and in some code paths not even AVX. The crash happens loading the
shared library itself, which is why nothing reached the log.

Three ways out were considered:

1. **Build ONNX Runtime from source without AVX2 kernels.** Stays on
   `ort`, but a from-source onnxruntime build is a multi-hour CMake
   project even on capable hardware, brittle across updates, and would
   need redoing for every dependency bump.
2. **Move the embedding/reranking compute off the NAS** (this Mac, or a
   cloud API). Rejected on request: the user's explicit goal was to keep
   as much as possible local, and OpenRouter already carries the one
   external dependency the project accepts (transcript text, for
   structuring) — widening that to cover every embedding and every
   reranked candidate pair was a real, not hypothetical, privacy
   trade-off the user did not want to make by default.
3. **Replace `ort` with [`candle`](https://github.com/huggingface/candle)**,
   Hugging Face's pure-Rust ML framework. Chosen.

candle's default (non-`mkl`/`accelerate`/`cuda`) CPU backend goes through
the `gemm` crate, which does one-time runtime CPU-feature detection
(cached in an `AtomicPtr`) and dispatches to the best kernel the actual
CPU supports, scalar fallback included — no compile-time ISA assumption.
The one caveat found in research: candle's own *quantized/GGUF* kernels
(`candle-core/src/quantized/avx.rs`) use hand-written AVX2 intrinsics with
no runtime guard and are known to SIGILL on non-AVX2 CPUs (upstream issues
#1818, #2140) — irrelevant here, since both models load from safetensors
in f32, never GGUF.

## Decision
`backend/src/embedding.rs` and `backend/src/reranker.rs` now load models
directly via `candle-core`/`candle-nn`/`candle-transformers`, fetching
weights from the Hugging Face Hub via `hf-hub` (pinned to `0.4`, not the
newly-major-bumped `1.0`, which dropped the `api::sync` module the
candle example code and this port both depend on) exactly as `fastembed`
did — the existing `MODEL_CACHE_DIR` volume and its already-downloaded
files are reused without change.

**The embedder** (`intfloat/multilingual-e5-small`) ports directly:
its `config.json` is a plain `"model_type": "bert"`, and
`candle_transformers::models::bert::BertModel` loads it as-is. Verified
against `examples/embedding_compare.rs`: the candle port reproduces the
exact same cosine similarities (to three decimal places) as the retired
`ort` implementation did, on the same four sentences.

**The reranker could not port as-is.** jina-reranker-v2-base-multilingual
— the default through 2026-09-21 — ships custom modeling code
(`modeling_xlm_roberta.py`, flash-attention, `trust_remote_code: true`,
`bfloat16`), not the standard architecture its name suggests. candle has
no implementation of it and porting one was out of scope. `BAAI/bge-reranker-v2-m3`
— the project's own already-configured fallback — is a plain
`XLMRobertaForSequenceClassification`, which `candle_transformers::models::xlm_roberta`
implements directly. **BGE is now the only reranker candle can run, so it
is now the only reranker, full stop** — `Choice::Jina` no longer exists as
a variant; `HIPPOCAMPUS_RERANKER=jina` fails fast with an explanation
rather than silently falling back to something else.

## Consequences
- **Slower echo.** BGE via candle measures ~1.3-1.6s for ten candidates on
  this Apple Silicon Mac, against `ort`'s measured 605ms for the same
  model on the same machine — roughly 2-3x slower, on top of BGE already
  being the slower of the two rerankers Jina was chosen over. Echo runs
  inline, right after a capture; the user feels this. Accepted: the
  alternative was a reranker that could not run on the target hardware at
  all. Not yet measured on the actual NAS.
- **The BGE threshold (`reranker::default_min_score`) needed
  recalibrating**, not just re-measuring — raw logits are not comparable
  between models, and BGE's scale runs far more negative than Jina's did.
  Done against two real captures so far (documented in
  `reranker.rs`), not the 38-capture corpus Jina's calibration used.
  Revisit as more real echoes accumulate.
- **Debug builds are unusably slow for this code locally.** candle's
  matmul is compiled unoptimized without `--release`; a rerank that takes
  ~1.3s in release took over three minutes in debug during this
  migration's own testing, indistinguishable from a hang. Always test
  echo/reranking against `cargo run --release`.
- **`ort` is gone from the backend entirely**, which quietly undercuts
  half of [ADR 0007](0007-separate-client-workspace.md)'s stated
  reasoning — the client's `transcribe-rs` still pins its own exact `ort`
  version for on-device ASR, but the backend no longer has one to
  conflict with. Not acted on here: merging the workspaces back is a
  separate decision, out of scope for a reranker migration.
- **Not yet proven on the actual NAS.** Everything above is verified on
  this Apple Silicon Mac and via `docker build --platform linux/amd64`
  locally (which confirms the image builds, not that AVX2 is actually
  the fix — QEMU emulation on Apple Silicon does not reproduce a real
  CPU's missing instruction set). The original crash was only ever
  observed on the real hardware; the fix needs to be too.
