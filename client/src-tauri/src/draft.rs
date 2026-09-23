//! The unsent draft of a writing session.
//!
//! A four-hour workshop is not twenty separate thoughts pressed into the
//! capture field one at a time, and it is not one wall of text either. It
//! is a page you keep adding to and send once, at the end — which means
//! there is a document sitting on this machine, unsent, for hours, that
//! has to survive a crash, a battery, and a meeting that runs long.
//!
//! This is deliberately *not* the outbox. The outbox holds captures that
//! have been committed and are waiting for the network; nothing in it can
//! be lost and nothing in it is still being edited. A draft is the
//! opposite on both counts: it is not a capture yet, and every keystroke
//! changes it. Mixing the two would mean either drafts that sync
//! half-written or captures that can still be edited away.
//!
//! The shape of the document belongs to the interface, not here — this
//! module's whole job is that whatever it was handed comes back byte for
//! byte after a hard power-off. Writes go to a temporary file and are
//! renamed over the real one, so an interrupted save leaves the previous
//! draft intact rather than a half-written file that parses as nothing.

use std::path::PathBuf;

use serde_json::Value;
use tauri::Manager;

const FILE_NAME: &str = "draft.json";

fn draft_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join(FILE_NAME))
        .map_err(|err| format!("no application data directory: {err}"))
}

/// The draft as it was last saved, or `None` when there is not one.
///
/// A file that cannot be parsed is reported and treated as absent rather
/// than as an error the interface has to handle: refusing to open the
/// writing screen because of a damaged draft would be trading a small
/// loss for a total one.
#[tauri::command]
pub fn draft_load(app: tauri::AppHandle) -> Option<Value> {
    let path = draft_path(&app).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(err) => {
            log::warn!("the draft at {} could not be read: {err}", path.display());
            None
        }
    }
}

/// Writes the draft, atomically.
#[tauri::command]
pub fn draft_save(app: tauri::AppHandle, draft: Value) -> Result<(), String> {
    let path = draft_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    // Same directory as the target, because a rename across filesystems is
    // a copy and stops being atomic.
    let staging = path.with_extension("json.writing");
    std::fs::write(
        &staging,
        serde_json::to_vec(&draft).map_err(|err| err.to_string())?,
    )
    .map_err(|err| err.to_string())?;
    std::fs::rename(&staging, &path).map_err(|err| err.to_string())
}

/// Forgets the draft. Called once its notes have been accepted, and by
/// the interface when the writer discards it.
///
/// A draft that is already gone is a success, not a failure: the only
/// thing the caller wanted is for it not to be there.
#[tauri::command]
pub fn draft_clear(app: tauri::AppHandle) -> Result<(), String> {
    let path = draft_path(&app)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}
