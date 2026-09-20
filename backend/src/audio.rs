//! Content-addressed storage for capture audio.
//!
//! Audio is the original (ADR 0004), so it is stored whole and never
//! rewritten. Files are named by the SHA-256 of their own bytes and fanned
//! out one level, which gives deduplication for free (a retried upload
//! writes the same path) and keeps any single directory from filling up
//! with thousands of entries.
//!
//! The path recorded in `capture_content.audio_path` is relative to the
//! configured audio directory, so moving the archive to a different disk
//! or NAS share does not invalidate a single row.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Audio formats accepted from clients, with the file extension used on
/// disk. Kept to an allowlist so a mislabelled upload cannot decide what
/// lands in the archive.
const ACCEPTED: &[(&str, &str)] = &[
    ("audio/wav", "wav"),
    ("audio/x-wav", "wav"),
    ("audio/wave", "wav"),
    ("audio/ogg", "ogg"),
    ("audio/opus", "opus"),
    ("audio/mpeg", "mp3"),
    ("audio/mp4", "m4a"),
    ("audio/aac", "aac"),
];

/// Largest accepted recording. At 16 kHz mono 16-bit WAV — what on-device
/// ASR wants — this is roughly 25 minutes, far beyond a spoken note, while
/// still bounding what one request can write to disk.
pub const MAX_BYTES: usize = 48 * 1024 * 1024;

pub fn extension_for(mime: &str) -> Option<&'static str> {
    let mime = mime.split(';').next().unwrap_or(mime).trim();
    ACCEPTED
        .iter()
        .find(|(accepted, _)| accepted.eq_ignore_ascii_case(mime))
        .map(|(_, ext)| *ext)
}

/// Writes `bytes` into `dir` and returns the path relative to `dir`.
///
/// Writing is skipped when the file already exists: identical bytes always
/// hash to the same name, so a client that retries an upload after a
/// timeout cannot create a second copy.
pub async fn store(dir: &Path, bytes: &[u8], extension: &str) -> anyhow::Result<String> {
    let digest = Sha256::digest(bytes);
    let hex = hex_encode(&digest);
    let relative = format!("{}/{}.{}", &hex[..2], hex, extension);

    let absolute = dir.join(&relative);
    if tokio::fs::try_exists(&absolute).await? {
        return Ok(relative);
    }

    if let Some(parent) = absolute.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Write to a temporary name first and rename into place: a crash
    // mid-write would otherwise leave a truncated file under a hash that
    // claims to describe complete content, and the dedup check above would
    // then happily reuse it forever.
    let temp = absolute.with_extension(format!("{extension}.partial"));
    tokio::fs::write(&temp, bytes).await?;
    tokio::fs::rename(&temp, &absolute).await?;

    Ok(relative)
}

pub fn resolve(dir: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    // The stored path is generated here, never supplied by a client, but
    // this is the one place a bad value would turn into a filesystem read
    // anywhere on the host — so it is checked rather than trusted.
    if relative.contains("..") || Path::new(relative).is_absolute() {
        anyhow::bail!("refusing to resolve suspicious audio path: {relative}");
    }
    Ok(dir.join(relative))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_known_types_and_ignores_parameters() {
        assert_eq!(extension_for("audio/wav"), Some("wav"));
        assert_eq!(extension_for("audio/wav; codecs=1"), Some("wav"));
        assert_eq!(extension_for("AUDIO/OGG"), Some("ogg"));
        assert_eq!(extension_for("application/zip"), None);
    }

    #[tokio::test]
    async fn stores_by_content_hash_and_deduplicates() {
        let dir = std::env::temp_dir().join(format!("hippocampus-audio-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let first = store(&dir, b"some audio bytes", "wav").await.unwrap();
        let second = store(&dir, b"some audio bytes", "wav").await.unwrap();
        let other = store(&dir, b"different bytes", "wav").await.unwrap();

        assert_eq!(first, second, "identical bytes must reuse the same path");
        assert_ne!(first, other);
        assert!(
            first.starts_with(&first[..2]),
            "path is fanned out by prefix"
        );
        assert!(resolve(&dir, &first).unwrap().exists());

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[test]
    fn rejects_paths_that_escape_the_audio_directory() {
        let dir = Path::new("/tmp/audio");
        assert!(resolve(dir, "../../etc/passwd").is_err());
        assert!(resolve(dir, "/etc/passwd").is_err());
        assert!(resolve(dir, "ab/abcd.wav").is_ok());
    }
}
