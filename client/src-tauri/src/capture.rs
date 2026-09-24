//! The voice capture command surface exposed to the webview.
//!
//! Recording, transcription and upload all happen in Rust so the audio
//! never has to cross into JavaScript. The webview only ever learns the
//! resulting text and its echoes.

use std::sync::{Arc, Mutex};

use chrono::Utc;
use contracts::CaptureAccepted;
use serde::Serialize;
use uuid::Uuid;

use tauri::Manager;

use crate::asr::{self, Transcriber};
use crate::microphone;
use crate::outbox::{Origin, Queued};
use crate::recorder::{self, Recording, TARGET_RATE};
use crate::sync::{self, SyncState};

/// The device's IANA timezone, or an empty string when the platform
/// cannot say. The backend falls back to its own configured zone then,
/// which is the right behaviour: a wrong guess here would silently date
/// things to the wrong day.
fn local_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_default()
}

/// Identifies where a capture came from, recorded as the event's source.
#[cfg(desktop)]
const DEVICE: &str = "mac-desktop";
#[cfg(mobile)]
const DEVICE: &str = "iphone";

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
    /// The backend's answer, when it was reachable. `None` means the
    /// capture is safe on this Mac and queued — which is a success, not a
    /// failure, and the UI says so in those words.
    pub capture: Option<CaptureAccepted>,
    /// Why it is still queued, when it is. Shown rather than swallowed:
    /// "waiting to sync" with no reason is how a broken Access token goes
    /// unnoticed for a week.
    pub queued_reason: Option<String>,
}

/// Whether speech recognition is usable, so the UI can offer the recording
/// button only when pressing it would actually work.
#[tauri::command]
pub fn speech_available() -> bool {
    asr::model_dir().is_ok()
}

#[tauri::command]
pub fn start_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, CaptureState>,
) -> Result<(), String> {
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
    // The one visible sign, from the menu bar, that the mic is actually
    // listening — see `crate::tray`.
    crate::tray::activity_begin(&app);
    Ok(())
}

/// Throws the recording away without transcribing or storing it.
#[tauri::command]
pub fn cancel_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, CaptureState>,
) -> Result<(), String> {
    let mut slot = state.recording.lock().map_err(|_| "recorder is wedged")?;
    if let Some(recording) = slot.take() {
        let _ = recording.finish();
        crate::tray::activity_end(&app);
    }
    Ok(())
}

/// Stops recording, transcribes on-device, writes the capture down, and
/// then tries to send it.
///
/// The order is the whole point. This used to record, transcribe and
/// upload, and return an error if the upload failed — at which moment the
/// audio and the transcript both ceased to exist. The backend being
/// asleep, the Wi-Fi having dropped or an Access token having expired
/// were each enough to lose a thought the user had already had and
/// already spoken.
///
/// Now the capture is on this Mac before the network is touched at all,
/// and the upload is something that either happens now or happens later.
/// A failure to send is no longer a failure to capture, and this command
/// only returns `Err` for things that happened *before* there was
/// anything to keep.
#[tauri::command]
pub async fn stop_recording(
    app: tauri::AppHandle,
    state: tauri::State<'_, CaptureState>,
) -> Result<VoiceCapture, String> {
    let recording = state
        .recording
        .lock()
        .map_err(|_| "recorder is wedged")?
        .take()
        .ok_or("not recording")?;
    crate::tray::activity_end(&app);

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

    let entry = Queued {
        id: Uuid::new_v4(),
        recorded_at: Utc::now(),
        origin: Origin::Audio,
        transcript: transcript.clone(),
        device: DEVICE.to_string(),
        model: Some(asr::MODEL_ID.to_string()),
        language: Some("auto".to_string()),
        duration_ms: Some(duration_ms),
        // Where the speaker was standing, so the backend can turn
        // "morgen" into a real date. Recorded here rather than at send
        // time, because a capture that waits three days and one flight
        // has to still mean the day it meant when it was spoken.
        timezone: local_timezone(),
        attempts: 0,
        last_error: None,
        last_attempt_at: None,
    };

    // The one line this whole module exists for. Past it, the thought is
    // kept whatever happens next.
    queue(&app, &entry, Some(&wav))?;

    let accepted = sync::send_now(&app, entry.id).await;
    let queued_reason = match &accepted {
        Some(_) => None,
        None => Some(last_reason(&app, entry.id)),
    };

    Ok(VoiceCapture {
        transcript,
        duration_ms,
        model: asr::MODEL_ID.to_string(),
        capture: accepted,
        queued_reason,
    })
}

/// Writes a typed capture down and then tries to send it.
///
/// Typed captures used to go straight out of the webview through
/// `api_request`, which meant they had the same hole as spoken ones: a
/// backend that was not there turned a written thought into an error
/// message. They come through here now for the same reason and with the
/// same guarantee.
#[tauri::command]
pub async fn capture_text(
    app: tauri::AppHandle,
    transcript: String,
) -> Result<VoiceCapture, String> {
    let transcript = transcript.trim().to_string();
    if transcript.is_empty() {
        return Err("a capture cannot be empty".to_string());
    }

    let entry = Queued {
        id: Uuid::new_v4(),
        recorded_at: Utc::now(),
        origin: Origin::Text,
        transcript: transcript.clone(),
        device: DEVICE.to_string(),
        // Nothing derived this text — the user typed it — so there is no
        // model to name (ADR 0004).
        model: None,
        language: None,
        duration_ms: None,
        timezone: local_timezone(),
        attempts: 0,
        last_error: None,
        last_attempt_at: None,
    };

    queue(&app, &entry, None)?;

    let accepted = sync::send_now(&app, entry.id).await;
    let queued_reason = match &accepted {
        Some(_) => None,
        None => Some(last_reason(&app, entry.id)),
    };

    Ok(VoiceCapture {
        transcript,
        duration_ms: 0,
        model: String::new(),
        capture: accepted,
        queued_reason,
    })
}

/// Puts a capture in the outbox, or explains why it could not be kept.
///
/// This is the only error in the capture path that is worth returning to
/// the user, because it is the only one that means the thought is
/// genuinely not safe anywhere. Everything downstream of it is a delay.
fn queue(app: &tauri::AppHandle, entry: &Queued, wav: Option<&[u8]>) -> Result<(), String> {
    let sync = app.state::<SyncState>();
    let result = match wav {
        Some(bytes) => sync.outbox().queue_audio(entry, bytes),
        None => sync.outbox().queue_text(entry),
    };

    result.map_err(|err| {
        log::error!("could not write a capture to the outbox: {err:#}");
        format!(
            "could not keep this capture on disk: {err:#}. Nothing was saved —              the text is still on screen."
        )
    })
}

/// Why a capture is still queued, read back from the entry the failed
/// attempt just updated. Falls back to a plain statement rather than an
/// invented cause.
fn last_reason(app: &tauri::AppHandle, id: Uuid) -> String {
    app.state::<SyncState>()
        .outbox()
        .pending()
        .into_iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.last_error)
        .unwrap_or_else(|| "the backend could not be reached".to_string())
}
