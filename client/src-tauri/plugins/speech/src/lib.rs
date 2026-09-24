//! Recording and speech recognition on the iPhone.
//!
//! The Mac records through `cpal` and transcribes with Parakeet through
//! ONNX, neither of which exists on iOS. There the same model runs on the
//! Neural Engine through FluidAudio, and recording goes through
//! AVAudioEngine — both Swift, in `ios/Sources/SpeechPlugin.swift`.
//! This crate is the thin door to that Swift code; the app's capture path
//! (outbox first, network second) stays in the app's Rust and is the same
//! on both.
//!
//! Every call here blocks until Swift answers, so none of them may be made
//! from the main thread: iOS draws the microphone permission prompt there,
//! and a blocked main thread would wait for an answer the user can never
//! give.

use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_speech);

/// A finished recording, already converted to what Parakeet expects:
/// 16 kHz mono, 16-bit WAV.
#[derive(Debug, Deserialize)]
pub struct Recorded {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct Transcript {
    pub text: String,
}

/// Whether the speech model is on the phone, and how far a download is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub installed: bool,
    pub downloading: bool,
    /// 0 to 1 while downloading.
    pub fraction: f64,
    /// Why the last download failed, until the next one starts.
    pub error: Option<String>,
}

#[derive(Serialize)]
struct TranscribeArgs<'a> {
    path: &'a str,
}

/// The Swift side, reached from Rust.
pub struct Speech<R: Runtime> {
    #[cfg(target_os = "ios")]
    handle: tauri::plugin::PluginHandle<R>,
    _runtime: PhantomData<fn() -> R>,
}

impl<R: Runtime> Clone for Speech<R> {
    fn clone(&self) -> Self {
        Self {
            #[cfg(target_os = "ios")]
            handle: self.handle.clone(),
            _runtime: PhantomData,
        }
    }
}

impl<R: Runtime> Speech<R> {
    /// Opens the microphone, asking for permission the first time.
    pub fn start_recording(&self) -> Result<(), String> {
        self.call::<serde_json::Value>("startRecording", ())
            .map(|_| ())
    }

    pub fn stop_recording(&self) -> Result<Recorded, String> {
        self.call("stopRecording", ())
    }

    /// Closes the microphone and throws the recording away.
    pub fn cancel_recording(&self) -> Result<(), String> {
        self.call::<serde_json::Value>("cancelRecording", ())
            .map(|_| ())
    }

    /// Runs Parakeet over a WAV file. Seconds, not milliseconds: call it
    /// off every thread that anything is waiting on.
    pub fn transcribe(&self, path: &str) -> Result<Transcript, String> {
        self.call("transcribe", TranscribeArgs { path })
    }

    /// Whether the Action Button asked for a recording since the last time
    /// this was asked, clearing the request. The App Intent that the
    /// button runs lives in the app target (`ios/RecordIntent.swift`) and
    /// can only leave a note; this is where the note is picked up.
    pub fn take_record_request(&self) -> Result<bool, String> {
        #[derive(Deserialize)]
        struct Requested {
            requested: bool,
        }
        self.call::<Requested>("takeRecordRequest", ())
            .map(|answer| answer.requested)
    }

    pub fn model_status(&self) -> Result<ModelStatus, String> {
        self.call("modelStatus", ())
    }

    /// Starts fetching the model and returns at once; `model_status`
    /// follows it.
    pub fn download_model(&self) -> Result<(), String> {
        self.call::<serde_json::Value>("downloadModel", ())
            .map(|_| ())
    }

    #[cfg(target_os = "ios")]
    fn call<T: DeserializeOwned>(
        &self,
        command: &str,
        payload: impl Serialize,
    ) -> Result<T, String> {
        self.handle
            .run_mobile_plugin(command, payload)
            .map_err(|err| err.to_string())
    }

    #[cfg(not(target_os = "ios"))]
    fn call<T: DeserializeOwned>(
        &self,
        _command: &str,
        _payload: impl Serialize,
    ) -> Result<T, String> {
        Err("recording through this plugin only exists on the iPhone".to_string())
    }
}

/// Registers the Swift plugin and puts a [`Speech`] in the app's state.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("hippocampus-speech")
        .setup(|app, _api| {
            #[cfg(target_os = "ios")]
            let handle = _api.register_ios_plugin(init_plugin_speech)?;
            app.manage(Speech::<R> {
                #[cfg(target_os = "ios")]
                handle,
                _runtime: PhantomData,
            });
            Ok(())
        })
        .build()
}
