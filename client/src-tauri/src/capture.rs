//! The voice capture command surface exposed to the webview.
//!
//! Recording, transcription and upload all happen in Rust so the audio
//! never has to cross into JavaScript. The webview only ever learns the
//! resulting text and its echoes.

use std::sync::{Arc, Mutex};

use contracts::CaptureAccepted;
use serde::Serialize;

use crate::asr::{self, Transcriber};
use crate::backend::BackendClient;
use crate::microphone;
use crate::recorder::{self, Recording, TARGET_RATE};
use crate::settings::SettingsState;

/// The device's IANA timezone, or an empty string when the platform
/// cannot say. The backend falls back to its own configured zone then,
/// which is the right behaviour: a wrong guess here would silently date
/// things to the wrong day.
fn local_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_default()
}

/// Identifies where a capture came from, recorded as the event's source.
const DEVICE: &str = "mac-desktop";

pub struct CaptureState {
    recording: Mutex<Option<Recording>>,
    transcriber: Arc<Transcriber>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self {
            recording: Mutex::new(None),
            transcriber: Arc::new(Transcriber::default()),
        }
    }
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
pub struct VoiceCapture {
    /// What the model heard. Shown to the user immediately, because a
    /// transcript they never see is a transcript they can never correct.
    pub transcript: String,
    pub duration_ms: u32,
    pub model: String,
    pub capture: CaptureAccepted,
}

/// Whether speech recognition is usable, so the UI can offer the recording
/// button only when pressing it would actually work.
#[tauri::command]
pub fn speech_available() -> bool {
    asr::model_dir().is_ok()
}

#[tauri::command]
pub fn start_recording(state: tauri::State<'_, CaptureState>) -> Result<(), String> {
    let mut slot = state.recording.lock().map_err(|_| "recorder is wedged")?;
    if slot.is_some() {
        return Err("already recording".to_string());
    }

    // Checked before opening the microphone: failing after the user has
    // spoken would throw the recording away, which is the one outcome this
    // system must never produce.
    asr::model_dir().map_err(|err| err.to_string())?;

    // Asked for, not assumed. An unauthorised microphone does not fail —
    // it records silence, and the user only finds out once they have
    // already said the thing.
    let permission = microphone::request();
    if permission.blocks_recording() {
        return Err(microphone::explain(permission).to_string());
    }

    *slot = Some(Recording::start().map_err(|err| err.to_string())?);
    Ok(())
}

/// Throws the recording away without transcribing or storing it.
#[tauri::command]
pub fn cancel_recording(state: tauri::State<'_, CaptureState>) -> Result<(), String> {
    let mut slot = state.recording.lock().map_err(|_| "recorder is wedged")?;
    if let Some(recording) = slot.take() {
        let _ = recording.finish();
    }
    Ok(())
}

/// Stops recording, transcribes on-device, and stores audio and transcript.
#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, CaptureState>,
    client: tauri::State<'_, BackendClient>,
    settings: tauri::State<'_, SettingsState>,
) -> Result<VoiceCapture, String> {
    // Where this capture is going, and what it needs to get past
    // Cloudflare Access — read from the same settings the rest of the app
    // uses. This used to be an environment variable read once at startup,
    // which meant the backend URL the user had configured in the settings
    // screen applied to every screen *except* the one path that matters
    // most: speaking a capture.
    let api_base = settings.backend_base();
    let credentials = settings.credentials();
    let recording = state
        .recording
        .lock()
        .map_err(|_| "recorder is wedged")?
        .take()
        .ok_or("not recording")?;

    let samples = recording.finish().map_err(|err| err.to_string())?;
    let duration_ms = (samples.len() as u64 * 1000 / u64::from(TARGET_RATE)) as u32;

    if samples.is_empty() {
        return Err("the recording was empty".to_string());
    }

    // A real microphone in a silent room still has a noise floor, so
    // sample-for-sample zero means no signal ever arrived.
    if microphone::is_digital_silence(&samples) {
        return Err(microphone::explain(microphone::Permission::Denied).to_string());
    }

    let transcriber = Arc::clone(&state.transcriber);
    let for_asr = samples.clone();
    let transcript = tauri::async_runtime::spawn_blocking(move || transcriber.transcribe(&for_asr))
        .await
        .map_err(|err| format!("transcription task failed: {err}"))?
        .map_err(|err| err.to_string())?;

    if transcript.is_empty() {
        return Err("nothing was recognised in that recording".to_string());
    }

    let wav = recorder::to_wav(&samples).map_err(|err| err.to_string())?;

    let form = reqwest::multipart::Form::new()
        .part(
            "audio",
            reqwest::multipart::Part::bytes(wav)
                .file_name("capture.wav")
                .mime_str("audio/wav")
                .map_err(|err| err.to_string())?,
        )
        .text("device", DEVICE)
        .text("transcript", transcript.clone())
        .text("model", asr::MODEL_ID)
        .text("language", "auto")
        .text("duration_ms", duration_ms.to_string())
        // Where the speaker was standing, so the backend can turn
        // "morgen" into a real date. Sent rather than assumed: the answer
        // has to stay right after a flight.
        .text("timezone", local_timezone());

    let mut request = client
        .http()
        .post(format!("{api_base}/captures/audio"))
        .multipart(form);
    if let Some((id, secret)) = credentials {
        request = request
            .header("CF-Access-Client-Id", id)
            .header("CF-Access-Client-Secret", secret);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach the backend at {api_base}: {err}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        // An Access challenge is a redirect, not a rejection by the
        // backend — saying so beats printing a login page as if it were
        // the backend's answer.
        if let Some(explained) = crate::backend::describe_status(status, location.as_deref()) {
            return Err(explained);
        }
        let body = response.text().await.unwrap_or_default();
        return Err(format!("backend rejected the capture ({status}): {body}"));
    }

    let capture: CaptureAccepted = response
        .json()
        .await
        .map_err(|err| format!("could not read the backend's answer: {err}"))?;

    Ok(VoiceCapture {
        transcript,
        duration_ms,
        model: asr::MODEL_ID.to_string(),
        capture,
    })
}
