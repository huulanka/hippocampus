//! On-device speech recognition.
//!
//! Transcription happens here rather than on the server on purpose: audio
//! is the most revealing thing this system holds — voice, background, other
//! people in the room — and it never leaves the machine it was recorded on.
//! Only the resulting text is sent anywhere, and only to the backend.
//!
//! The transcript is explicitly *not* the original (ADR 0004): the model id
//! travels with it so a later, better model's disagreement is visible
//! rather than silent.

use std::path::PathBuf;
use std::sync::Mutex;

use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

/// parakeet-tdt-0.6b-v3: multilingual (German included), int8-quantised,
/// about 670 MB on disk. Measured on this machine: loads in ~0.7 s and
/// transcribes 5.7 s of German speech in ~0.25 s.
pub const MODEL_ID: &str = "parakeet-tdt-0.6b-v3-int8";

/// Directory name the model files are expected to live in.
pub const MODEL_DIR_NAME: &str = "parakeet-tdt-0.6b-v3";

/// Files `transcribe-rs` needs to find in the model directory.
const REQUIRED_FILES: &[&str] = &[
    "encoder-model.int8.onnx",
    "decoder_joint-model.int8.onnx",
    "nemo128.onnx",
    "vocab.txt",
];

/// Loads the model on first use and keeps it in memory afterwards.
///
/// Loading is deferred rather than done at startup so the app still opens
/// (and typed capture still works) when the model has not been fetched yet.
#[derive(Default)]
pub struct Transcriber {
    model: Mutex<Option<ParakeetModel>>,
}

impl Transcriber {
    /// Transcribes 16 kHz mono samples. Blocking and CPU-bound — callers
    /// must not run this on an async runtime thread.
    pub fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        let mut guard = self
            .model
            .lock()
            .map_err(|_| anyhow::anyhow!("transcriber was poisoned by an earlier panic"))?;

        if guard.is_none() {
            let dir = model_dir()?;
            *guard = Some(ParakeetModel::load(&dir, &Quantization::Int8)?);
        }

        let model = guard.as_mut().expect("model loaded above");
        let result = model.transcribe(samples, &TranscribeOptions::default())?;
        Ok(result.text.trim().to_string())
    }
}

/// Where the ASR model lives.
///
/// `HIPPOCAMPUS_ASR_MODEL_DIR` wins when set, which is what development and
/// the fetch script use. Otherwise the model is expected next to the app's
/// own data, so a packaged build has somewhere sensible to put it.
pub fn model_dir() -> anyhow::Result<PathBuf> {
    let dir = match std::env::var("HIPPOCAMPUS_ASR_MODEL_DIR") {
        Ok(path) if !path.is_empty() => PathBuf::from(path),
        _ => default_model_dir()?,
    };

    let missing: Vec<&str> = REQUIRED_FILES
        .iter()
        .copied()
        .filter(|file| !dir.join(file).exists())
        .collect();

    if !missing.is_empty() {
        anyhow::bail!(
            "speech model incomplete in {}: missing {}. Run scripts/fetch-asr-model.sh",
            dir.display(),
            missing.join(", ")
        );
    }

    Ok(dir)
}

fn default_model_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME")?;
    Ok(PathBuf::from(home)
        .join("Library/Application Support/com.andreasbauer.hippocampus/models")
        .join(MODEL_DIR_NAME))
}
