//! Recording on the phone, through AVAudioEngine in the Swift plugin
//! (`plugins/speech`). The same surface as [`crate::recorder`] on the Mac.
//!
//! Swift writes the audio to a file as it arrives and hands back a 16 kHz
//! mono WAV at the end, which is read into samples here so the rest of the
//! capture path — the silence check, the outbox, the upload — is the one
//! the Mac uses.

use anyhow::{anyhow, Context};

pub const TARGET_RATE: u32 = 16_000;

pub struct Recording {
    _private: (),
}

impl Recording {
    /// Opens the microphone. Blocks until Swift has, which the first time
    /// includes the permission prompt — so never call this on the main
    /// thread, which has to be free to draw it.
    pub fn start() -> anyhow::Result<Self> {
        crate::asr::speech()?
            .start_recording()
            .map_err(|err| anyhow!(err))?;
        Ok(Self { _private: () })
    }

    pub fn finish(self) -> anyhow::Result<Vec<f32>> {
        let recorded = crate::asr::speech()?
            .stop_recording()
            .map_err(|err| anyhow!(err))?;
        let samples = read_wav(&recorded.path);
        let _ = std::fs::remove_file(&recorded.path);
        samples
    }
}

fn read_wav(path: &str) -> anyhow::Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path).context("could not open the recording")?;
    reader
        .samples::<i16>()
        .map(|sample| Ok(f32::from(sample?) / f32::from(i16::MAX)))
        .collect()
}

/// The same encoding as on the Mac, so the backend receives one format.
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
