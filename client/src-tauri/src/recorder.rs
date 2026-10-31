//! Microphone capture.
//!
//! The recording is the original (ADR 0004), so this module's job is to
//! produce an honest 16 kHz mono signal and nothing else — no trimming, no
//! noise gate, no silence removal. Whatever was said is what gets kept.
//!
//! cpal's `Stream` is not `Send` on macOS, so the stream lives on its own
//! thread for its whole life and is only ever reached through channels.
//! Samples accumulate in a shared buffer that the stopping side takes over.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

/// What on-device ASR models expect, Parakeet included.
pub const TARGET_RATE: u32 = 16_000;

pub struct Recording {
    stop: mpsc::Sender<()>,
    samples: Arc<Mutex<Vec<f32>>>,
    /// Set by the audio callback if the input stream errors out, so the
    /// stopping side can tell "silence because nobody spoke" apart from
    /// "silence because the device went away".
    failed: Arc<AtomicBool>,
    source_rate: u32,
    channels: u16,
}

impl Recording {
    /// Opens the default input device and starts recording immediately.
    pub fn start() -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("no microphone available"))?;
        let config = device.default_input_config()?;

        let source_rate = config.sample_rate();
        let channels = config.channels();
        let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
        let failed = Arc::new(AtomicBool::new(false));

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<anyhow::Result<()>>();

        let thread_samples = Arc::clone(&samples);
        let thread_failed = Arc::clone(&failed);

        std::thread::spawn(move || {
            let error_flag = Arc::clone(&thread_failed);
            let stream = device.build_input_stream(
                config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buffer) = thread_samples.lock() {
                        buffer.extend_from_slice(data);
                    }
                },
                move |err| {
                    log::error!("input stream error: {err}");
                    error_flag.store(true, Ordering::Relaxed);
                },
                None,
            );

            let stream = match stream {
                Ok(stream) => stream,
                Err(err) => {
                    let _ = ready_tx.send(Err(err.into()));
                    return;
                }
            };

            if let Err(err) = stream.play() {
                let _ = ready_tx.send(Err(err.into()));
                return;
            }
            let _ = ready_tx.send(Ok(()));

            // Hold the stream here until asked to stop. A disconnected
            // channel means the app is going down, which stops it too.
            let _ = stop_rx.recv();
            drop(stream);
        });

        ready_rx.recv()??;

        Ok(Self {
            stop: stop_tx,
            samples,
            failed,
            source_rate,
            channels,
        })
    }

    /// Stops the stream and returns the recording as 16 kHz mono samples.
    pub fn finish(self) -> anyhow::Result<Vec<f32>> {
        let _ = self.stop.send(());

        if self.failed.load(Ordering::Relaxed) {
            anyhow::bail!("the microphone stream failed during recording");
        }

        // Taken rather than copied: at 48 kHz stereo a minute is 23 MB,
        // and the callback may still append a last buffer or two to the
        // emptied one before the stream thread drops it, which is harmless.
        let raw = std::mem::take(
            &mut *self
                .samples
                .lock()
                .map_err(|_| anyhow::anyhow!("recording buffer was poisoned"))?,
        );

        let mono = to_mono(&raw, self.channels);
        resample(&mono, self.source_rate, TARGET_RATE)
    }

    /// Stops the stream and throws the recording away, without the
    /// resampling [`Self::finish`] would spend on it.
    pub fn cancel(self) {
        let _ = self.stop.send(());
    }
}

/// Averages interleaved channels down to one. Averaging rather than taking
/// the first channel: on a stereo interface the speaker may well be on the
/// other side.
fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let channels = usize::from(channels);
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect()
}

/// Band-limited resampling to the ASR model's rate.
///
/// Mac input devices generally run at 48 kHz and cannot be asked for 16 kHz
/// directly, so this always has work to do. Naive decimation would alias
/// everything above 8 kHz straight down into the speech band, which is
/// exactly where consonants live.
fn resample(samples: &[f32], from: u32, to: u32) -> anyhow::Result<Vec<f32>> {
    if from == to {
        return Ok(samples.to_vec());
    }

    // `process_all` cuts the clip into chunks of the size given here, trims
    // the resampler's startup delay and stops at the end of the input, so
    // the result lines up with the recording without padding or a leading
    // stretch of silence.
    const CHUNK: usize = 1024;
    let mut resampler = Fft::<f32>::new(from as usize, to as usize, CHUNK, 1, FixedSync::Both)?;
    let input = InterleavedSlice::new(samples, 1, samples.len())?;
    let resampled = resampler.process_all(&input, samples.len(), None)?;

    Ok(resampled.take_data())
}

/// Encodes samples as a 16-bit mono WAV in memory, which is what the
/// backend archives and what every ASR tool can read without ceremony.
pub fn to_wav(samples: &[f32]) -> anyhow::Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buffer, spec)?;
        for sample in samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_stereo_into_mono() {
        let stereo = [1.0, 0.0, 0.5, 0.5, -1.0, 1.0];
        assert_eq!(to_mono(&stereo, 2), vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn mono_passes_through_untouched() {
        let mono = [0.1, 0.2, 0.3];
        assert_eq!(to_mono(&mono, 1), mono.to_vec());
    }

    #[test]
    fn resampling_48k_to_16k_thirds_the_length() {
        let input = vec![0.0f32; 48_000];
        let out = resample(&input, 48_000, 16_000).unwrap();

        // A 2% band catches a wrong ratio (the mistake that would matter)
        // without pinning how the crate rounds the length at the end of
        // the clip.
        let expected = 16_000i64;
        let drift = (out.len() as i64 - expected).abs();
        assert!(
            drift < expected / 50,
            "expected ~{expected} samples, got {}",
            out.len()
        );
    }

    #[test]
    fn resampling_keeps_a_tone_intact_from_the_first_sample() {
        // One second of a 440 Hz tone at half scale, at both rates Mac input
        // devices commonly run at. Speech sits well below the 8 kHz cutoff,
        // so a tone like this must come through unchanged in pitch and
        // level, and must not start late.
        for source_rate in [44_100u32, 48_000] {
            let freq = 440.0f32;
            let input: Vec<f32> = (0..source_rate)
                .map(|n| {
                    0.5 * (2.0 * std::f32::consts::PI * freq * n as f32 / source_rate as f32).sin()
                })
                .collect();
            let out = resample(&input, source_rate, 16_000).unwrap();

            let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
            let expected_rms = 0.5 / 2f32.sqrt();
            let middle = &out[1_000..15_000];
            assert!(
                (rms(middle) - expected_rms).abs() < expected_rms * 0.02,
                "{source_rate} Hz: level changed, rms {}",
                rms(middle)
            );

            // Two zero crossings per period: about 880 in one second.
            let crossings = out
                .windows(2)
                .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                .count() as i64;
            assert!(
                (crossings - 880).abs() < 18,
                "{source_rate} Hz: pitch changed, {crossings} crossings"
            );

            // The startup delay is trimmed: the first 10 ms already carry the tone.
            assert!(
                rms(&out[..160]) > expected_rms * 0.5,
                "{source_rate} Hz: the output starts with silence"
            );
        }
    }

    #[test]
    fn wav_round_trips_through_hound() {
        let wav = to_wav(&[0.0, 0.5, -0.5]).unwrap();
        let reader = hound::WavReader::new(std::io::Cursor::new(wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, TARGET_RATE);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len(), 3);
    }
}
