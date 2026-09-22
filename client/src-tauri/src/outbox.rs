//! Captures that are safe on this Mac but not yet at the backend.
//!
//! This is the module that makes the first line of the product's
//! non-functional requirements true rather than aspirational: *"Erfassung
//! schlägt nie fehl. Lokale Persistenz vor Netzwerk. Ein einziger
//! verlorener Gedanke kostet das Vertrauen dauerhaft."*
//!
//! Until now, speaking a note recorded it, transcribed it on-device, and
//! then uploaded it — and if that upload failed, because the NAS was
//! asleep or the Wi-Fi had dropped or the Access token had expired, the
//! function returned an error and both the audio and the transcript were
//! gone. The user was told "could not reach the backend", which is true
//! and beside the point: they had already said the thing.
//!
//! So nothing is uploaded that has not first been written down here. The
//! upload becomes a thing that either succeeds now or succeeds later,
//! never a thing that can lose a note.
//!
//! ## Why files and not a database
//!
//! One JSON file per waiting capture, with its WAV beside it, chosen over
//! SQLite deliberately. A queue of unsent thoughts is the most precious
//! and least redundant data this application ever holds — it is by
//! definition the part that exists in exactly one place. Plain files can
//! be read, copied and recovered with nothing but Finder, which is the
//! same argument the product makes for keeping raw captures as standard
//! audio on disk. It also means a half-written entry damages one file
//! rather than an index.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A capture waiting to be sent.
///
/// Everything the backend needs is here, including the things that are
/// only knowable at the moment of speaking — above all the timezone,
/// which is what lets "morgen" still resolve to the right date when the
/// note finally goes up three days and one flight later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Queued {
    pub id: Uuid,
    /// When it was actually said, not when it is finally sent. The
    /// backend stamps the event with its own clock, so this is currently
    /// for the user's eyes and for ordering the queue — but it is the one
    /// fact that cannot be recovered later, so it is written down.
    pub recorded_at: DateTime<Utc>,
    pub origin: Origin,
    pub transcript: String,
    pub device: String,
    /// The ASR model, for a spoken capture. Absent when it was typed:
    /// nothing derived that text, so naming a model would be a claim
    /// about provenance that is not true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    pub timezone: String,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Audio,
    Text,
}

/// What the sidebar puts on screen.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    /// Captures on this Mac that the backend has not acknowledged.
    pub waiting: usize,
    /// How many of those have already failed at least once. The
    /// difference matters: one waiting capture is a moment, one waiting
    /// capture that has failed eleven times is a problem.
    pub failing: usize,
    /// The most recent reason, so the tooltip can say what is wrong
    /// rather than only that something is.
    pub last_error: Option<String>,
}

pub struct Outbox {
    dir: PathBuf,
}

impl Outbox {
    pub fn new(dir: PathBuf) -> Self {
        if let Err(err) = std::fs::create_dir_all(&dir) {
            // Not fatal at startup: capturing has to keep working, and
            // the directory is created again on the first write. Loud,
            // though, because until it exists the safety net is not
            // there.
            log::error!(
                "could not create the outbox directory {}: {err}",
                dir.display()
            );
        }
        Self { dir }
    }

    fn entry_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    pub fn audio_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{id}.wav"))
    }

    /// Writes a spoken capture down before anything is attempted with it.
    ///
    /// Audio first, entry second, because the entry is the commit marker:
    /// a crash between the two leaves an orphaned WAV, which
    /// [`Self::sweep_orphans`] clears, whereas the other order would
    /// leave an entry pointing at audio that was never written.
    pub fn queue_audio(&self, entry: &Queued, wav: &[u8]) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        write_atomically(&self.audio_path(entry.id), wav)
            .with_context(|| format!("writing the recording for {}", entry.id))?;
        self.write_entry(entry)
    }

    pub fn queue_text(&self, entry: &Queued) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        self.write_entry(entry)
    }

    fn write_entry(&self, entry: &Queued) -> anyhow::Result<()> {
        // Pretty-printed on purpose. The argument for files over a
        // database is that a person can open one and see their own words;
        // a single long line would honour the letter of that and not the
        // point.
        let json = serde_json::to_vec_pretty(entry)?;
        write_atomically(&self.entry_path(entry.id), &json)
            .with_context(|| format!("writing outbox entry {}", entry.id))
    }

    /// Everything still waiting, oldest first, so the backend receives
    /// thoughts in the order they were had.
    pub fn pending(&self) -> Vec<Queued> {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };

        let mut entries: Vec<Queued> = dir
            .filter_map(Result::ok)
            .map(|item| item.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| match std::fs::read(&path) {
                Ok(bytes) => match serde_json::from_slice::<Queued>(&bytes) {
                    Ok(entry) => Some(entry),
                    Err(err) => {
                        // Left on disk rather than deleted. It is a file
                        // containing something the user said; a parser
                        // disagreement is not grounds for destroying it.
                        log::error!("outbox entry {} is unreadable: {err}", path.display());
                        None
                    }
                },
                Err(err) => {
                    log::error!("could not read outbox entry {}: {err}", path.display());
                    None
                }
            })
            .collect();

        entries.sort_by_key(|entry| entry.recorded_at);
        entries
    }

    pub fn counts(&self) -> Counts {
        let pending = self.pending();
        let failing = pending.iter().filter(|entry| entry.attempts > 0).count();
        let last_error = pending
            .iter()
            .filter(|entry| entry.last_error.is_some())
            .max_by_key(|entry| entry.last_attempt_at)
            .and_then(|entry| entry.last_error.clone());

        Counts {
            waiting: pending.len(),
            failing,
            last_error,
        }
    }

    /// Removes an entry once the backend has confirmed it.
    ///
    /// Entry first, audio second — the reverse of writing, and for the
    /// same reason. A crash in between leaves an orphaned WAV rather than
    /// an entry that would be uploaded a second time.
    pub fn done(&self, id: Uuid) {
        if let Err(err) = std::fs::remove_file(self.entry_path(id)) {
            log::warn!("could not remove outbox entry {id}: {err}");
        }
        let audio = self.audio_path(id);
        if audio.exists() {
            if let Err(err) = std::fs::remove_file(&audio) {
                log::warn!("could not remove queued audio for {id}: {err}");
            }
        }
    }

    /// Records that an attempt failed, so the next one can back off and
    /// the sidebar can say why.
    pub fn failed(&self, entry: &Queued, error: &str) {
        let updated = Queued {
            attempts: entry.attempts.saturating_add(1),
            last_error: Some(error.to_string()),
            last_attempt_at: Some(Utc::now()),
            ..entry.clone()
        };
        if let Err(err) = self.write_entry(&updated) {
            // The capture is still on disk and still queued; only the
            // failure count is lost, which costs a retry sooner than
            // planned.
            log::warn!("could not record a failed attempt for {}: {err}", entry.id);
        }
    }

    /// Deletes WAV files with no entry beside them.
    ///
    /// The only way one exists is a crash between writing the audio and
    /// writing its entry — which means nothing ever knew about that
    /// recording, and nothing ever will.
    pub fn sweep_orphans(&self) {
        let Ok(dir) = std::fs::read_dir(&self.dir) else {
            return;
        };

        for path in dir
            .filter_map(Result::ok)
            .map(|item| item.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
        {
            if path.with_extension("json").exists() {
                continue;
            }
            log::warn!("removing orphaned queued audio {}", path.display());
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Writes through a temporary file and renames it into place.
///
/// `rename` within one directory is atomic, so a reader — including this
/// process after a crash — sees either the old file or the whole new one,
/// never a half-written entry. The explicit `sync_all` is the part people
/// leave out: without it the rename can land before the bytes do, and a
/// power cut then leaves a correctly named, empty file.
fn write_atomically(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temporary = path.with_extension("tmp");

    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    std::fs::rename(&temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("hippocampus-outbox-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn spoken(transcript: &str) -> Queued {
        Queued {
            id: Uuid::new_v4(),
            recorded_at: Utc::now(),
            origin: Origin::Audio,
            transcript: transcript.to_string(),
            device: "test".into(),
            model: Some("parakeet".into()),
            language: Some("auto".into()),
            duration_ms: Some(1234),
            timezone: "Europe/Berlin".into(),
            attempts: 0,
            last_error: None,
            last_attempt_at: None,
        }
    }

    #[test]
    fn a_queued_capture_survives_being_written_and_read_back() {
        let outbox = Outbox::new(scratch("roundtrip"));
        let entry = spoken("die Sauna war gut");
        outbox.queue_audio(&entry, b"fake wav bytes").unwrap();

        let pending = outbox.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].transcript, "die Sauna war gut");
        assert_eq!(pending[0].timezone, "Europe/Berlin");
        assert!(outbox.audio_path(entry.id).exists());
    }

    #[test]
    fn the_queue_is_ordered_by_when_it_was_said() {
        let outbox = Outbox::new(scratch("order"));
        let mut first = spoken("first");
        first.recorded_at = Utc::now() - chrono::Duration::minutes(5);
        let second = spoken("second");

        // Written in the wrong order on purpose: the filesystem has no
        // opinion about which of these was thought first.
        outbox.queue_audio(&second, b"b").unwrap();
        outbox.queue_audio(&first, b"a").unwrap();

        let pending = outbox.pending();
        assert_eq!(pending[0].transcript, "first");
        assert_eq!(pending[1].transcript, "second");
    }

    #[test]
    fn a_failed_attempt_is_counted_and_the_capture_stays() {
        let outbox = Outbox::new(scratch("failure"));
        let entry = spoken("still here");
        outbox.queue_audio(&entry, b"wav").unwrap();

        outbox.failed(&entry, "could not connect");
        outbox.failed(&outbox.pending()[0], "could not connect");

        let pending = outbox.pending();
        assert_eq!(pending.len(), 1, "a failure must never drop the capture");
        assert_eq!(pending[0].attempts, 2);
        assert_eq!(pending[0].last_error.as_deref(), Some("could not connect"));

        let counts = outbox.counts();
        assert_eq!(counts.waiting, 1);
        assert_eq!(counts.failing, 1);
    }

    #[test]
    fn confirming_a_capture_removes_it_and_its_audio() {
        let outbox = Outbox::new(scratch("done"));
        let entry = spoken("sent");
        outbox.queue_audio(&entry, b"wav").unwrap();

        outbox.done(entry.id);

        assert!(outbox.pending().is_empty());
        assert!(!outbox.audio_path(entry.id).exists());
    }

    #[test]
    fn audio_without_an_entry_is_swept_away() {
        let outbox = Outbox::new(scratch("orphan"));
        let orphan = outbox.audio_path(Uuid::new_v4());
        std::fs::write(&orphan, b"wav").unwrap();

        let kept = spoken("keep me");
        outbox.queue_audio(&kept, b"wav").unwrap();

        outbox.sweep_orphans();

        assert!(
            !orphan.exists(),
            "a recording nothing knows about is not recoverable"
        );
        assert!(outbox.audio_path(kept.id).exists());
    }
}
