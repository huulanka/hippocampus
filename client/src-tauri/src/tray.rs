//! The menu-bar presence: an always-there black-and-white pixel-art
//! brain, a click that brings the window back, and a right-click that
//! offers the way out.
//!
//! Doubles as this project's first drawn icon — there has never been one
//! besides Tauri's default double-O. Built as a literal bitmap in source
//! rather than a PNG asset: a glyph this small is a handful of pixels
//! either way, and a bitmap spelled out as text is one that can be tuned
//! without an image editor. `icon_as_template` leaves the actual colour
//! to macOS, which is what lets the same brain sit correctly on a light
//! or a dark menu bar.
//!
//! It also stands still. A menu bar item that moves when nothing is
//! happening is a menu bar item that trains you to ignore it — so the
//! brain only pulses while a recording is actually in progress, one
//! frame of a synapse travelling down the groove between the two halves,
//! and holds still the rest of the time.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

/// Logical pixels per side of the bitmap below.
const SIZE: usize = 16;
/// How many real pixels each logical pixel becomes. Kept low deliberately
/// — this is meant to read as pixel art, not as a smoothed-out icon that
/// happens to be small.
const SCALE: usize = 2;

/// The resting brain. `#` is ink, `.` is transparent; read top to bottom.
/// Two lobes, a groove between them, and a short stem.
///
/// Earlier version of this bitmap carved single-pixel gyri notches into
/// the outline, which read fine blown up in an ASCII preview and turned
/// to mush at the size a menu bar actually renders it — a 20×20 grid of
/// one-pixel details survives neither the downscale nor the antialiasing
/// macOS applies. Every stroke here is at least two logical pixels thick
/// for exactly that reason: it has to still be a brain at 16 real pixels
/// tall, not just at this file's zoom level.
const REST: [&str; SIZE] = [
    "................",
    "................",
    "...####..####...",
    "..#####..#####..",
    "..#####..#####..",
    ".######..######.",
    ".##############.",
    ".##############.",
    "..############..",
    "...##########...",
    ".....######.....",
    "......####......",
    "......####......",
    "................",
    "................",
    "................",
];

/// Where the groove between the two lobes runs — the two columns and the
/// row range that [`REST`] deliberately leaves blank there. Two columns
/// wide for the same reason as everything else here: a one-pixel groove
/// disappears at real size, a two-pixel one does not. Short on purpose,
/// too: a groove that ran the full height of the mass read as two hearts
/// pinched together rather than one brain with a dimple at the top.
const GROOVE_COLUMNS: (usize, usize) = (7, 8);
/// Row pairs the pulse lights up, one step at a time, top to bottom — an
/// impulse two rows tall rather than one, again so it stays visible after
/// the downscale.
const GROOVE_STEPS: [(usize, usize); 2] = [(2, 3), (4, 5)];

/// How often the pulse steps forward while something is recording.
/// Slow enough to read as a heartbeat, not a spinner.
const PULSE_INTERVAL: Duration = Duration::from_millis(450);

/// One frame of the pulse: the resting brain with one step of the groove
/// lit, standing in for an impulse partway down it.
fn pulse_frame(step: usize) -> [String; SIZE] {
    let (top, bottom) = GROOVE_STEPS[step % GROOVE_STEPS.len()];
    std::array::from_fn(|y| {
        if y != top && y != bottom {
            return REST[y].to_string();
        }
        let mut row: Vec<char> = REST[y].chars().collect();
        row[GROOVE_COLUMNS.0] = '#';
        row[GROOVE_COLUMNS.1] = '#';
        row.into_iter().collect()
    })
}

/// Turns a `SIZE`×`SIZE` grid of `#`/`.` into an RGBA image, `SCALE`
/// times larger, nearest-neighbour — the upscale that keeps pixel art
/// looking like pixel art instead of blurring it. Alpha is what actually
/// draws the shape once the tray icon is marked as a template; colour is
/// opaque black and macOS ignores it in that mode.
fn render(rows: &[impl AsRef<str>; SIZE]) -> Image<'static> {
    let out = SIZE * SCALE;
    let mut rgba = vec![0u8; out * out * 4];
    for (y, row) in rows.iter().enumerate() {
        for (x, cell) in row.as_ref().chars().enumerate() {
            if cell != '#' {
                continue;
            }
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let idx = ((y * SCALE + dy) * out + (x * SCALE + dx)) * 4;
                    rgba[idx] = 0;
                    rgba[idx + 1] = 0;
                    rgba[idx + 2] = 0;
                    rgba[idx + 3] = 255;
                }
            }
        }
    }
    Image::new_owned(rgba, out as u32, out as u32)
}

/// The rendered frames, built once at startup rather than on every tick
/// of the pulse — the bitmap never changes, only which frame is shown.
struct Frames {
    rest: Image<'static>,
    pulse: [Image<'static>; 2],
}

impl Frames {
    fn render() -> Self {
        Self {
            rest: render(&REST),
            pulse: std::array::from_fn(|step| render(&pulse_frame(step))),
        }
    }
}

/// How many recordings are in flight. A count rather than a bool because
/// `start_recording` and `stop_recording`/`cancel_recording` are separate
/// commands with no guarantee only one is ever outstanding at once — a
/// count that never goes negative is simpler than reasoning about which
/// call is allowed to turn the pulse off.
#[derive(Default)]
struct Activity(AtomicUsize);

/// Marks a recording as having started. Pair with [`activity_end`] —
/// every path out of `start_recording` that returns `Ok` must eventually
/// call it, including cancellation.
pub fn activity_begin(app: &AppHandle) {
    app.state::<Activity>().0.fetch_add(1, Ordering::SeqCst);
}

/// Marks a recording as finished. Saturates at zero instead of
/// underflowing: a stray extra call here should be a no-op, not a pulse
/// that never turns off because the counter wrapped around.
pub fn activity_end(app: &AppHandle) {
    let _ = app
        .state::<Activity>()
        .0
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
}

fn is_active(app: &AppHandle) -> bool {
    app.state::<Activity>().0.load(Ordering::SeqCst) > 0
}

/// Brings the window back, nothing else. Deliberately *not*
/// `crate::summon_capture`: that also tells the webview a press of the
/// capture shortcut just happened, which starts (or stops) a recording —
/// exactly the bug this used to have, where clicking the tray icon
/// silently opened the microphone. A click here should only ever mean
/// "show me the window."
fn summon(app: &AppHandle) {
    crate::reveal_window(app);
}

/// Builds the tray icon, its menu, and starts the pulse watcher.
///
/// Registered from `setup()`, after the window it points back at
/// already exists.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    app.manage(Activity::default());
    let frames = Frames::render();

    let show = MenuItem::with_id(app, "show", "Open Hippocampus", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hippocampus", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &PredefinedMenuItem::separator(app)?, &quit])?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(frames.rest.clone())
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => summon(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                summon(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(frames);
    watch(app.clone(), tray);
    Ok(())
}

/// Steps the pulse forward while a recording is in flight, and puts the
/// resting frame back the moment it ends. Ticks at [`PULSE_INTERVAL`]
/// regardless of activity — coarser than that would make the first frame
/// of a pulse lag behind the recording noticeably; a tighter interval
/// would just be extra wakeups with nothing to show for them, the same
/// tradeoff the idle-lock clock already makes.
fn watch(app: AppHandle, tray: TrayIcon) {
    tauri::async_runtime::spawn(async move {
        let mut step = 0usize;
        let mut pulsing = false;
        loop {
            tokio::time::sleep(PULSE_INTERVAL).await;
            let frames = app.state::<Frames>();
            if is_active(&app) {
                pulsing = true;
                let icon = frames.pulse[step % frames.pulse.len()].clone();
                step = step.wrapping_add(1);
                if let Err(err) = tray.set_icon(Some(icon)) {
                    log::warn!("could not step the tray pulse: {err}");
                }
            } else if pulsing {
                pulsing = false;
                step = 0;
                if let Err(err) = tray.set_icon(Some(frames.rest.clone())) {
                    log::warn!("could not still the tray icon: {err}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_of_the_bitmap_is_the_declared_width() {
        for row in REST {
            assert_eq!(row.chars().count(), SIZE);
        }
    }

    #[test]
    fn a_pulse_frame_lights_up_only_its_own_groove_rows() {
        for (step, &(top, bottom)) in GROOVE_STEPS.iter().enumerate() {
            let frame = pulse_frame(step);
            for (y, (rest_row, frame_row)) in REST.iter().zip(frame.iter()).enumerate() {
                if y == top || y == bottom {
                    assert_ne!(
                        *frame_row, *rest_row,
                        "row {y} should differ from rest at the lit step"
                    );
                    let mut expected: Vec<char> = rest_row.chars().collect();
                    expected[GROOVE_COLUMNS.0] = '#';
                    expected[GROOVE_COLUMNS.1] = '#';
                    let expected: String = expected.into_iter().collect();
                    assert_eq!(*frame_row, expected);
                } else {
                    assert_eq!(*frame_row, *rest_row, "row {y} should be untouched");
                }
            }
        }
    }

    #[test]
    fn rendering_scales_up_and_only_ever_writes_opaque_ink_or_transparency() {
        let image = render(&REST);
        let out = SIZE * SCALE;
        assert_eq!(image.width(), out as u32);
        assert_eq!(image.height(), out as u32);

        let rgba = image.rgba();
        assert_eq!(rgba.len(), out * out * 4);
        for pixel in rgba.as_chunks::<4>().0 {
            match pixel {
                [0, 0, 0, 255] => {}
                [0, 0, 0, 0] => {}
                other => panic!("unexpected pixel {other:?} — ink must be opaque black or absent"),
            }
        }
    }

    #[test]
    fn a_lit_groove_pixel_survives_the_upscale() {
        let frame = pulse_frame(0);
        let image = render(&frame);
        let out = SIZE * SCALE;
        let rgba = image.rgba();

        let (gx, gy) = (GROOVE_COLUMNS.0 * SCALE, GROOVE_STEPS[0].0 * SCALE);
        let idx = (gy * out + gx) * 4;
        assert_eq!(&rgba[idx..idx + 4], &[0, 0, 0, 255]);
    }
}
