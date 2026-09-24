//! Speech recognition on the phone, which does not exist yet.
//!
//! The Mac runs Parakeet through ONNX, and `ort` has no prebuilt binaries
//! for iOS. The phone will record and transcribe in Swift instead
//! (`docs/iphone.md`, I4); until then this stands in for [`crate::asr`]
//! with the same surface, so the capture commands build unchanged and
//! report speech as unavailable, which leaves typed capture working.

use std::path::PathBuf;

pub const MODEL_ID: &str = "none";

// A private field, so it is built the way the real one is:
// `Transcriber::default()` in `capture.rs` must compile for both.
#[derive(Default)]
pub struct Transcriber {
    _private: (),
}

impl Transcriber {
    pub fn transcribe(&self, _samples: &[f32]) -> anyhow::Result<String> {
        anyhow::bail!("speech recognition is not available on this device yet")
    }
}

pub fn model_dir() -> anyhow::Result<PathBuf> {
    anyhow::bail!("speech recognition is not available on this device yet")
}
