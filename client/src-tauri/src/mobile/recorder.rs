//! Recording on the phone, which does not exist yet.
//!
//! Stands in for [`crate::recorder`] until the phone records through a
//! Swift plugin (`docs/iphone.md`, I4). Never reached in practice: the
//! capture commands check [`crate::asr::model_dir`] first, which fails
//! here.

pub const TARGET_RATE: u32 = 16_000;

pub struct Recording;

impl Recording {
    pub fn start() -> anyhow::Result<Self> {
        anyhow::bail!("recording is not available on this device yet")
    }

    pub fn finish(self) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }
}

pub fn to_wav(_samples: &[f32]) -> anyhow::Result<Vec<u8>> {
    anyhow::bail!("recording is not available on this device yet")
}
