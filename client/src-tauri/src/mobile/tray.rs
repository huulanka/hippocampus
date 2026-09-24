//! A phone has no menu bar. The recording pulse that [`crate::tray`]
//! shows on the Mac has no equivalent here until a Live Activity takes
//! its place (`docs/iphone.md`, I12).

use tauri::AppHandle;

pub fn activity_begin(_app: &AppHandle) {}

pub fn activity_end(_app: &AppHandle) {}
