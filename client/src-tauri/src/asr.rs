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

/// The longest stretch of audio the model is given in one piece.
///
/// Not a tuning knob. The encoder attends over everything it is handed at
/// once, so its working memory grows with the square of the length, and
/// ONNX Runtime keeps its peak afterwards. Measured with synthetic German
/// speech on an M-series Mac: 6 s peaked at 1.2 GB, 103 s at 2.6 GB, and
/// 7 minutes did not transcribe at all — the model has no positions past
/// about 5,000 frames and fails outright, which used to take the recording
/// down with it.
const MAX_CHUNK: usize = 30 * 16_000;

/// How far back from [`MAX_CHUNK`] a cut may move to land in a pause
/// rather than in the middle of a word.
const CUT_SEARCH: usize = 8 * 16_000;

/// The stretch whose loudness decides where the pause is: 50 ms, shorter
/// than any pause between words and longer than a single plosive.
const CUT_FRAME: usize = 16_000 / 20;

/// Loads the model when it is needed and lets it go again when it is not.
///
/// Loading is deferred rather than done at startup so the app still opens
/// (and typed capture still works) when the model has not been fetched yet.
/// It is not kept for the life of the app either: loaded, it is over a
/// gigabyte, for something used a few times a day. [`Self::warm`] at the
/// start of a recording hides the load (~0.7 s) behind the speaking, and
/// the capture commands call [`Self::release`] once it has gone unused
/// for a while.
#[derive(Default)]
pub struct Transcriber {
    model: Mutex<Option<ParakeetModel>>,
}

impl Transcriber {
    /// Loads the model if it is not loaded yet. Blocking.
    pub fn warm(&self) -> anyhow::Result<()> {
        let mut guard = self.lock()?;
        Self::loaded(&mut guard)?;
        Ok(())
    }

    /// Transcribes 16 kHz mono samples. Blocking and CPU-bound — callers
    /// must not run this on an async runtime thread.
    ///
    /// Long recordings are cut into pieces of at most [`MAX_CHUNK`], each
    /// at the quietest moment near the limit, and the pieces' text joined.
    pub fn transcribe(&self, samples: &[f32]) -> anyhow::Result<String> {
        let mut guard = self.lock()?;
        let model = Self::loaded(&mut guard)?;

        let mut text = String::new();
        for chunk in chunks(samples) {
            let result = model.transcribe(chunk, &TranscribeOptions::default())?;
            let part = result.text.trim();
            if part.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(part);
        }
        Ok(text)
    }

    /// Drops the model, unless a transcription is holding it right now.
    /// Returns whether it was loaded.
    pub fn release(&self) -> bool {
        match self.model.try_lock() {
            Ok(mut guard) => guard.take().is_some(),
            Err(_) => false,
        }
    }

    fn lock(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Option<ParakeetModel>>> {
        self.model
            .lock()
            .map_err(|_| anyhow::anyhow!("transcriber was poisoned by an earlier panic"))
    }

    fn loaded(slot: &mut Option<ParakeetModel>) -> anyhow::Result<&mut ParakeetModel> {
        if slot.is_none() {
            let dir = model_dir()?;
            *slot = Some(ParakeetModel::load(&dir, &Quantization::Int8)?);
        }
        Ok(slot.as_mut().expect("model loaded above"))
    }
}

/// Cuts a recording into pieces the model can take, each ending in the
/// quietest 50 ms of the last [`CUT_SEARCH`] before the limit. Nothing is
/// dropped or repeated: the pieces are the recording, end to end.
fn chunks(samples: &[f32]) -> Vec<&[f32]> {
    let mut out = Vec::new();
    let mut rest = samples;
    while rest.len() > MAX_CHUNK {
        let from = MAX_CHUNK - CUT_SEARCH;
        let (head, tail) = rest.split_at(from + quietest(&rest[from..MAX_CHUNK]));
        out.push(head);
        rest = tail;
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// The middle of the quietest [`CUT_FRAME`] in `window`.
fn quietest(window: &[f32]) -> usize {
    window
        .chunks(CUT_FRAME)
        .map(|frame| frame.iter().map(|s| s * s).sum::<f32>())
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(window.len(), |(index, _)| index * CUT_FRAME + CUT_FRAME / 2)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone, loud enough to never be taken for a pause.
    fn speech(len: usize) -> Vec<f32> {
        (0..len).map(|i| (i as f32 * 0.05).sin() * 0.5).collect()
    }

    #[test]
    fn a_short_recording_is_one_piece() {
        let samples = speech(10 * 16_000);
        let pieces = chunks(&samples);
        assert_eq!(pieces.len(), 1);
        assert_eq!(pieces[0].len(), samples.len());
    }

    #[test]
    fn a_long_recording_is_cut_into_pieces_that_add_up_to_it() {
        let samples = speech(7 * 60 * 16_000);
        let pieces = chunks(&samples);

        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|piece| piece.len() <= MAX_CHUNK));
        let joined: Vec<f32> = pieces.concat();
        assert_eq!(joined, samples, "nothing dropped, nothing repeated");
    }

    #[test]
    fn the_cut_lands_in_the_pause() {
        let mut samples = speech(2 * MAX_CHUNK);
        // A second of silence inside the search window, away from its edges.
        let pause = MAX_CHUNK - CUT_SEARCH / 2;
        samples[pause..pause + 16_000].fill(0.0);

        let first = chunks(&samples)[0].len();
        assert!(
            (pause..pause + 16_000).contains(&first),
            "cut at {first}, pause is {pause}..{}",
            pause + 16_000
        );
    }
}
