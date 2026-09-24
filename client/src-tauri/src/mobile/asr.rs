//! Speech recognition on the phone: Parakeet v3 on the Neural Engine,
//! through the Swift plugin in `plugins/speech` (`docs/iphone.md`, I4).
//!
//! The same surface as [`crate::asr`] on the Mac, so the capture commands
//! are the same code on both. The model is not bundled — it is several
//! hundred megabytes — and is downloaded once from Settings; until it is,
//! [`model_dir`] fails and the phone offers typing only, as the Mac does
//! without its model.

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{anyhow, Context};
use tauri::Manager;
use tauri_plugin_hippocampus_speech::Speech;

/// What `transcript.derived.model` records for a capture heard on the
/// phone. The same weights as the Mac's, in a different container, so it
/// is named apart: a difference between the two would otherwise be
/// impossible to trace back.
pub const MODEL_ID: &str = "parakeet-tdt-0.6b-v3-coreml";

static SPEECH: OnceLock<Speech<tauri::Wry>> = OnceLock::new();

/// Keeps a handle to the plugin for the recorder and the transcriber,
/// which are called from places that have no `AppHandle` to ask.
pub fn install(app: &tauri::AppHandle) {
    if let Some(speech) = app.try_state::<Speech<tauri::Wry>>() {
        let _ = SPEECH.set(speech.inner().clone());
    }
}

pub(crate) fn speech() -> anyhow::Result<&'static Speech<tauri::Wry>> {
    SPEECH
        .get()
        .ok_or_else(|| anyhow!("the speech plugin is not running"))
}

// A private field, so it is built the way the real one is:
// `Transcriber::default()` in `capture.rs` must compile for both.
#[derive(Default)]
pub struct Transcriber {
    _private: (),
}

impl Transcriber {
    /// Hands the samples to Parakeet as a file, which is how the Swift
    /// side reads them, and removes the file afterwards whatever happened.
    pub fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        let wav = crate::recorder::to_wav(samples)?;
        let path = std::env::temp_dir().join(format!("transcribe-{}.wav", uuid::Uuid::new_v4()));
        std::fs::write(&path, wav).context("could not write the recording for the model")?;
        let result = speech()?.transcribe(&path.to_string_lossy());
        let _ = std::fs::remove_file(&path);
        Ok(result.map_err(|err| anyhow!(err))?.text)
    }
}

/// Succeeds when the model is on the phone. There is no directory the rest
/// of the app needs to know about — FluidAudio keeps its own — so the path
/// is empty; the result is what matters.
pub fn model_dir() -> anyhow::Result<PathBuf> {
    let status = speech()?.model_status().map_err(|err| anyhow!(err))?;
    if status.installed {
        Ok(PathBuf::new())
    } else {
        Err(anyhow!(
            "the speech model is not on this phone yet — download it in Settings; typing works meanwhile"
        ))
    }
}
