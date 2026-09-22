//! Getting the outbox empty again.
//!
//! Every upload in the app goes through here, including the one that
//! happens immediately after speaking. That is the point: there is no
//! "fast path" that skips the queue, because a fast path that skips the
//! queue is exactly the code that used to lose notes. Speaking writes to
//! the outbox and then asks this module to try; if it works the entry is
//! gone a moment later and nobody notices there was a queue at all.

use std::sync::Arc;
use std::time::Duration;

use contracts::CaptureAccepted;
use tauri::{Emitter, Manager};
use uuid::Uuid;

use crate::backend::BackendClient;
use crate::outbox::{Counts, Origin, Outbox, Queued};
use crate::settings::SettingsState;

/// Event the webview listens for to update the sidebar. Must match
/// `OUTBOX_EVENT` in `src/desktop.ts`.
pub const OUTBOX_EVENT: &str = "hippocampus://outbox";

/// How often the queue is retried when something is in it.
///
/// Not a per-entry exponential backoff: the failure this exists for is
/// almost always one shared cause — the backend is unreachable — so
/// twenty entries backing off independently would just be twenty clocks
/// telling the same time. The whole queue waits together, and waits
/// longer the longer it has been failing.
const RETRY_STEPS: [Duration; 5] = [
    Duration::from_secs(10),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(180),
    Duration::from_secs(600),
];

/// How long a single upload may take before it is treated as failed.
///
/// Generous, because the backend is a NAS that may be spinning up its
/// container, and because a timeout here costs only a retry — the capture
/// is on disk either way.
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SyncState {
    outbox: Arc<Outbox>,
    /// Serialises uploads, so the timer and the "I just finished
    /// speaking" path cannot send the same entry twice. Duplicate notes
    /// are the failure mode a naive outbox introduces, and they are worse
    /// than the problem it solves — a lost note is at least visibly lost.
    gate: tokio::sync::Mutex<()>,
}

impl SyncState {
    pub fn new(outbox: Arc<Outbox>) -> Self {
        Self {
            outbox,
            gate: tokio::sync::Mutex::new(()),
        }
    }

    pub fn outbox(&self) -> &Outbox {
        &self.outbox
    }
}

/// What one attempt at the queue achieved.
#[derive(Debug, Default, Clone, Copy)]
pub struct Drained {
    pub sent: usize,
    pub failed: usize,
}

/// Uploads one queued capture.
///
/// Spoken and typed captures take different endpoints because they are
/// different things: for a spoken one the recording is the original and
/// the transcript is an interpretation of it (ADR 0004), so it goes as
/// multipart with the audio attached.
pub(crate) async fn upload(
    client: &BackendClient,
    api_base: &str,
    credentials: Option<(String, String)>,
    entry: &Queued,
    audio: Option<Vec<u8>>,
) -> Result<CaptureAccepted, String> {
    let mut request = match audio {
        Some(wav) => {
            let mut form = reqwest::multipart::Form::new()
                .part(
                    "audio",
                    reqwest::multipart::Part::bytes(wav)
                        .file_name("capture.wav")
                        .mime_str("audio/wav")
                        .map_err(|err| err.to_string())?,
                )
                .text("device", entry.device.clone())
                .text("transcript", entry.transcript.clone())
                .text("timezone", entry.timezone.clone());
            if let Some(model) = &entry.model {
                form = form.text("model", model.clone());
            }
            if let Some(language) = &entry.language {
                form = form.text("language", language.clone());
            }
            if let Some(duration) = entry.duration_ms {
                form = form.text("duration_ms", duration.to_string());
            }
            client
                .http()
                .post(format!("{api_base}/captures/audio"))
                .multipart(form)
        }
        None => client.http().post(format!("{api_base}/captures")).json(
            &contracts::CreateCaptureRequest {
                transcript_text: entry.transcript.clone(),
                device: entry.device.clone(),
                timezone: Some(entry.timezone.clone()),
            },
        ),
    };

    if let Some((id, secret)) = credentials {
        request = request
            .header("CF-Access-Client-Id", id)
            .header("CF-Access-Client-Secret", secret);
    }

    let response = request
        .timeout(UPLOAD_TIMEOUT)
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

    response
        .json()
        .await
        .map_err(|err| format!("could not read the backend's answer: {err}"))
}

/// Tries to send everything in the queue, oldest first.
///
/// Stops at the first failure. Failures here are nearly always one shared
/// cause, so working through the rest of the queue would only mean the
/// same error repeated n times, n retry counters incremented, and — with
/// a backend that is up but refusing — n pointless round trips.
pub async fn drain(app: &tauri::AppHandle) -> Drained {
    let sync = app.state::<SyncState>();
    let _held = sync.gate.lock().await;

    let outbox = Arc::clone(&sync.outbox);
    let pending = outbox.pending();
    if pending.is_empty() {
        return Drained::default();
    }

    let settings = app.state::<SettingsState>();
    let api_base = settings.backend_base();
    let credentials = settings.credentials();
    let client = app.state::<BackendClient>();

    let mut result = Drained::default();

    for entry in &pending {
        // A spoken capture whose audio has gone missing is still sent —
        // as text, through the other endpoint. The recording is the
        // original and losing it is a real loss, but the words are what
        // the user actually said, and the alternative is losing those
        // too.
        let audio = match entry.origin {
            Origin::Audio => match std::fs::read(outbox.audio_path(entry.id)) {
                Ok(bytes) => Some(bytes),
                Err(err) => {
                    log::error!(
                        "queued recording for {} is unreadable ({err}); sending the transcript alone",
                        entry.id
                    );
                    None
                }
            },
            Origin::Text => None,
        };

        match upload(&client, &api_base, credentials.clone(), entry, audio).await {
            Ok(accepted) => {
                log::info!("sent queued capture {} as {}", entry.id, accepted.event_id);
                outbox.done(entry.id);
                result.sent += 1;
            }
            Err(err) => {
                log::warn!("queued capture {} did not go through: {err}", entry.id);
                outbox.failed(entry, &err);
                result.failed += 1;
                break;
            }
        }
    }

    announce(app, outbox.counts());
    result
}

/// Sends one specific capture, for the path where the user has just
/// finished speaking and is looking at the screen.
///
/// Returns the backend's answer when it goes through, so the echo can be
/// shown immediately, and `None` when it did not — in which case the
/// capture is still queued and this is not an error the caller should
/// report as a failure to save.
pub async fn send_now(app: &tauri::AppHandle, id: Uuid) -> Option<CaptureAccepted> {
    let sync = app.state::<SyncState>();
    let _held = sync.gate.lock().await;

    let outbox = Arc::clone(&sync.outbox);
    let Some(entry) = outbox.pending().into_iter().find(|entry| entry.id == id) else {
        // Already sent by the timer while the user was still looking at
        // the transcript. Nothing to do and nothing wrong.
        return None;
    };

    let settings = app.state::<SettingsState>();
    let api_base = settings.backend_base();
    let credentials = settings.credentials();
    let client = app.state::<BackendClient>();

    let audio = match entry.origin {
        Origin::Audio => std::fs::read(outbox.audio_path(entry.id)).ok(),
        Origin::Text => None,
    };

    let sent = match upload(&client, &api_base, credentials, &entry, audio).await {
        Ok(accepted) => {
            outbox.done(entry.id);
            Some(accepted)
        }
        Err(err) => {
            log::warn!("capture {id} stays queued: {err}");
            outbox.failed(&entry, &err);
            None
        }
    };

    announce(app, outbox.counts());
    sent
}

/// Tells the webview what the queue looks like now.
fn announce(app: &tauri::AppHandle, counts: Counts) {
    if let Err(err) = app.emit(OUTBOX_EVENT, counts) {
        log::warn!("could not tell the webview about the outbox: {err}");
    }
}

/// Retries the queue on a timer for as long as anything is in it.
///
/// Sleeps briefly between passes while the queue is empty, which is the
/// normal state — the loop exists for the rare stretch when it is not.
pub fn watch(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Cleared once at startup rather than on every pass: the only
        // thing that produces an orphan is a crash, and a crash is
        // necessarily before this point.
        app.state::<SyncState>().outbox().sweep_orphans();

        let mut consecutive_failures = 0usize;

        loop {
            let waiting = app.state::<SyncState>().outbox().counts().waiting;

            let wait = if waiting == 0 {
                consecutive_failures = 0;
                RETRY_STEPS[0]
            } else {
                RETRY_STEPS[consecutive_failures.min(RETRY_STEPS.len() - 1)]
            };
            tokio::time::sleep(wait).await;

            if app.state::<SyncState>().outbox().counts().waiting == 0 {
                continue;
            }

            let drained = drain(&app).await;
            if drained.failed > 0 {
                consecutive_failures = consecutive_failures.saturating_add(1);
            } else {
                consecutive_failures = 0;
            }
        }
    });
}

/// What the sidebar asks for on first render, before any event has fired.
#[tauri::command]
pub fn outbox_status(sync: tauri::State<'_, SyncState>) -> Counts {
    sync.outbox().counts()
}

/// Retries now, because the user asked rather than because a timer said
/// so. The whole queue, not one entry: they are almost always stuck on
/// the same thing.
#[tauri::command]
pub async fn sync_now(app: tauri::AppHandle) -> Result<Counts, String> {
    drain(&app).await;
    Ok(app.state::<SyncState>().outbox().counts())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::Origin;
    use chrono::Utc;
    use uuid::Uuid;

    /// The round trip this whole module exists for, against a real
    /// backend.
    ///
    /// Ignored by default because it needs one running — CI has no
    /// Postgres and no embedding model. Run it with:
    ///
    /// ```text
    /// HIPPOCAMPUS_TEST_BACKEND=http://127.0.0.1:8080 \
    ///   cargo test --lib sync:: -- --ignored --nocapture
    /// ```
    ///
    /// What it proves is the one thing the unit tests around `Outbox`
    /// cannot: that a queued capture is sent, is accepted, and is then
    /// gone from the queue — and, in the second half, that a capture
    /// aimed at a backend that is not there stays.
    #[tokio::test]
    #[ignore = "needs a running backend; see the doc comment"]
    async fn a_queued_capture_goes_up_and_leaves_the_queue() {
        let Ok(base) = std::env::var("HIPPOCAMPUS_TEST_BACKEND") else {
            panic!("set HIPPOCAMPUS_TEST_BACKEND to a running backend");
        };

        let dir = std::env::temp_dir().join(format!("hippocampus-sync-test-{}", Uuid::new_v4()));
        let outbox = Outbox::new(dir);
        let client = BackendClient::new();

        let entry = Queued {
            id: Uuid::new_v4(),
            recorded_at: Utc::now(),
            origin: Origin::Text,
            transcript: "Outbox round trip test — safe to redact".to_string(),
            device: "integration-test".to_string(),
            model: None,
            language: None,
            duration_ms: None,
            timezone: "Europe/Berlin".to_string(),
            attempts: 0,
            last_error: None,
            last_attempt_at: None,
        };
        outbox.queue_text(&entry).unwrap();
        assert_eq!(outbox.counts().waiting, 1);

        let accepted = upload(&client, &base, None, &entry, None)
            .await
            .expect("the backend should have accepted it");
        outbox.done(entry.id);

        assert_eq!(
            outbox.counts().waiting,
            0,
            "an accepted capture leaves the queue"
        );
        println!("accepted as {}", accepted.event_id);

        // Now the half that matters more: a backend that is not there.
        let stranded = Queued {
            id: Uuid::new_v4(),
            transcript: "this one must survive a dead backend".to_string(),
            ..entry.clone()
        };
        outbox.queue_text(&stranded).unwrap();

        let result = upload(
            &client,
            // Nothing listens here.
            "http://127.0.0.1:9",
            None,
            &stranded,
            None,
        )
        .await;

        let err = result.expect_err("a dead backend cannot accept anything");
        outbox.failed(&stranded, &err);

        let pending = outbox.pending();
        assert_eq!(
            pending.len(),
            1,
            "a failed upload must never drop the capture"
        );
        assert_eq!(
            pending[0].transcript,
            "this one must survive a dead backend"
        );
        assert_eq!(pending[0].attempts, 1);
        assert!(pending[0].last_error.is_some(), "and it must say why");

        println!("stranded capture kept, reason: {err}");
        std::fs::remove_dir_all(outbox.audio_path(stranded.id).parent().unwrap()).ok();
    }
}
