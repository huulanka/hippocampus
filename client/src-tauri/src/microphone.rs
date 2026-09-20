//! Microphone permission.
//!
//! This exists because of a failure that looked like nothing at all.
//! cpal opens the input device through AudioUnit, which — unlike
//! `AVCaptureDeviceInput` — never triggers the system's permission
//! dialog. Apple's own documentation is explicit about what happens
//! then: "Until access has been granted, any AVCaptureDevices for the
//! media type will vend silent audio samples."
//!
//! So an unauthorised recording is not an error. It is a perfect,
//! correctly-sized buffer of zeros, which transcribes to nothing, which
//! surfaced to the user as "nothing was recognised in that recording" —
//! true, useless, and pointing at the wrong thing entirely.
//!
//! Permission is therefore asked for explicitly, before the microphone
//! is opened, and its absence is reported as itself.

/// What the system currently thinks about us and the microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Never asked. Asking will show the dialog.
    Undecided,
    Granted,
    /// Refused, or forbidden by policy. Only System Settings can undo it.
    Denied,
    /// Not macOS, or the status could not be read. Assume the best and
    /// let the recording itself fail if it must.
    Unknown,
}

impl Permission {
    pub fn blocks_recording(self) -> bool {
        matches!(self, Permission::Denied)
    }
}

/// What to tell the user, in their terms rather than the API's.
pub fn explain(permission: Permission) -> &'static str {
    match permission {
        Permission::Denied => {
            "macOS is blocking the microphone. System Settings → Privacy & Security → \
             Microphone, switch Hippocampus on, then try again."
        }
        _ => "the microphone could not be opened",
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    use super::Permission;

    pub fn current() -> Permission {
        // SAFETY: AVMediaTypeAudio is a framework constant, and the
        // class method takes it by reference without retaining it.
        let raw = unsafe {
            let Some(media_type) = AVMediaTypeAudio else {
                return Permission::Unknown;
            };
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        };

        match raw {
            AVAuthorizationStatus::Authorized => Permission::Granted,
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => Permission::Denied,
            AVAuthorizationStatus::NotDetermined => Permission::Undecided,
            _ => Permission::Unknown,
        }
    }

    /// Asks the system, showing the dialog if it has never been asked.
    ///
    /// Blocks until the user answers, because the alternative is opening
    /// a microphone that quietly records silence while they decide.
    pub fn request() -> Permission {
        let existing = current();
        if existing != Permission::Undecided {
            return existing;
        }

        let (tx, rx) = mpsc::channel::<bool>();
        let handler = RcBlock::new(move |granted: objc2::runtime::Bool| {
            // The receiver may be gone if the wait below timed out; a
            // failed send just means nobody is listening any more.
            let _ = tx.send(granted.as_bool());
        });

        // SAFETY: the block outlives the call — `request_access` blocks on
        // the channel until the completion handler has run, or until the
        // timeout, and `RcBlock` keeps it alive for the duration.
        unsafe {
            let Some(media_type) = AVMediaTypeAudio else {
                return Permission::Unknown;
            };
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }

        // Generous, because a human has to read a dialog and decide. If
        // they take longer than this, the status is read again below and
        // will simply be whatever they eventually chose.
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(true) => Permission::Granted,
            Ok(false) => Permission::Denied,
            Err(_) => current(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::Permission;

    pub fn request() -> Permission {
        Permission::Unknown
    }

    pub fn current() -> Permission {
        Permission::Unknown
    }
}

pub use platform::{current, request};

/// Whether a finished recording is digital silence.
///
/// Not "quiet": exactly zero, sample for sample. A real microphone in a
/// silent room still produces a noise floor, so an all-zero buffer means
/// no signal ever arrived — a device that went away, or a permission
/// that was refused after recording had already started.
pub fn is_digital_silence(samples: &[f32]) -> bool {
    !samples.is_empty() && samples.iter().all(|s| *s == 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zeros_is_digital_silence() {
        assert!(is_digital_silence(&[0.0; 512]));
    }

    #[test]
    fn a_noise_floor_is_not_silence() {
        let mut samples = vec![0.0f32; 512];
        samples[300] = -0.0001;
        assert!(!is_digital_silence(&samples));
    }

    #[test]
    fn an_empty_recording_is_not_reported_as_silence() {
        // It has its own, more specific message.
        assert!(!is_digital_silence(&[]));
    }

    #[test]
    fn only_denial_blocks_recording() {
        assert!(Permission::Denied.blocks_recording());
        assert!(!Permission::Granted.blocks_recording());
        assert!(!Permission::Undecided.blocks_recording());
        assert!(!Permission::Unknown.blocks_recording());
    }
}
