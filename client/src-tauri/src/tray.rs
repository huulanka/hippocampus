//! The menu-bar presence: an always-there brain, a click that brings the
//! window back, and a right-click that offers the way out.
//!
//! The mark comes from `icons/tray-template.png`, which
//! `scripts/render-brand.py` renders from `client/src/brand/mark.json` —
//! the same file the interface draws. It used to be a bitmap spelled out
//! as rows of `#` in this file, which meant the menu bar and the window
//! could drift apart, and both of them did: mirrored halves tapering to a
//! point at the bottom centre is the construction of a heart, and that is
//! what it had quietly become.
//!
//! The image is 48x36 and only its left 36 columns hold the brain.
//! `tray-icon` scales whatever it is handed to 18 points tall, so the
//! brain lands at the conventional 16 pt and the empty strip on the right
//! is reserved for the recording dot. Reserving it means the item never
//! changes width — so the mark itself never moves, however long a
//! recording runs.
//!
//! It also stands still. A menu bar item that moves when nothing is
//! happening is one that trains you to ignore it, so the brain never
//! animates at all: while a recording is in flight the dot beside it
//! breathes, and that is the whole of it. The shape changing used to be
//! the signal, which at menu-bar size is a flicker.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// The resting mark: black plus alpha, for macOS to tint once the icon is
/// marked as a template. Baked into the binary rather than read from disk
/// so a half-installed app cannot end up with no icon at all.
const TEMPLATE: &[u8] = include_bytes!("../icons/tray-template.png");

/// Where the recording dot sits, in template pixels, and how big it is.
/// Inside the strip the brain deliberately leaves empty.
const DOT_CENTRE: (f32, f32) = (42.0, 18.0);
const DOT_RADIUS: f32 = 3.4;

/// How many steps one breath is cut into, and how long the whole breath
/// takes. Twelve is enough that the fade reads as continuous and few
/// enough that the frames are built once at startup and then only cloned.
const PULSE_FRAMES: usize = 12;
const PULSE_PERIOD: Duration = Duration::from_millis(1600);

/// How far the dot fades at the bottom of a breath. Never to nothing: a
/// dot that disappears entirely reads as a glitch rather than as breathing,
/// and "am I still recording?" is the one question this must always answer.
const DOT_MIN_ALPHA: f32 = 0.30;

/// How often the loop looks for work while nothing is recording. Long
/// enough to be nearly free, short enough that the dot appears without a
/// noticeable wait after the shortcut is pressed.
const IDLE_POLL: Duration = Duration::from_millis(250);

/// Composites the recording dot onto a copy of the resting mark at the
/// given strength.
///
/// Coverage is the distance to the centre softened over one pixel, which
/// is all the antialiasing a disc this size needs. Only the alpha channel
/// carries the shape: a template image is drawn from its alpha alone, and
/// the colour underneath is discarded by macOS.
fn with_dot(rest: &Image<'_>, strength: f32) -> Image<'static> {
    let (width, height) = (rest.width() as usize, rest.height() as usize);
    let mut rgba = rest.rgba().to_vec();

    let (cx, cy) = DOT_CENTRE;
    let first_row = (cy - DOT_RADIUS - 1.0).floor().max(0.0) as usize;
    let last_row = ((cy + DOT_RADIUS + 1.0).ceil() as usize).min(height);
    let first_col = (cx - DOT_RADIUS - 1.0).floor().max(0.0) as usize;
    let last_col = ((cx + DOT_RADIUS + 1.0).ceil() as usize).min(width);

    for y in first_row..last_row {
        for x in first_col..last_col {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let coverage = (DOT_RADIUS + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let index = (y * width + x) * 4;
            let alpha = (coverage * strength * 255.0).round() as u8;
            // Never dim a pixel the mark already claimed. The dot sits in
            // empty space by construction, but the arithmetic should not
            // be the thing keeping that true.
            if alpha > rgba[index + 3] {
                rgba[index] = 0;
                rgba[index + 1] = 0;
                rgba[index + 2] = 0;
                rgba[index + 3] = alpha;
            }
        }
    }

    Image::new_owned(rgba, width as u32, height as u32)
}

/// How bright the dot is at `step` of a breath: a cosine, so it eases at
/// both ends instead of sawing back to the start.
fn breath(step: usize) -> f32 {
    let phase = step as f32 / PULSE_FRAMES as f32 * std::f32::consts::TAU;
    let swing = (1.0 - phase.cos()) / 2.0;
    DOT_MIN_ALPHA + (1.0 - DOT_MIN_ALPHA) * swing
}

/// The rendered frames, built once at startup rather than on every tick.
struct Frames {
    rest: Image<'static>,
    pulse: Vec<Image<'static>>,
}

impl Frames {
    fn render() -> tauri::Result<Self> {
        let rest = Image::from_bytes(TEMPLATE)?.to_owned();
        let pulse = (0..PULSE_FRAMES)
            .map(|step| with_dot(&rest, breath(step)))
            .collect();
        Ok(Self { rest, pulse })
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
    let frames = Frames::render()?;

    let show = MenuItem::with_id(app, "show", "Open Hippocampus", true, None::<&str>)?;
    let review = MenuItem::with_id(app, "review", "Look Back on the Week", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Hippocampus", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&show, &review, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(frames.rest.clone())
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => summon(app),
            "review" => {
                summon(app);
                if let Err(err) = app.emit(crate::review::OPEN_REVIEW_EVENT, ()) {
                    log::warn!("could not ask the webview to open the review: {err}");
                }
            }
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

/// Steps the breath forward while a recording is in flight, and puts the
/// resting mark back the moment it ends.
///
/// Every icon change goes through `set_icon_with_as_template`, never
/// `set_icon`. That is not a style preference: `set_icon` hands
/// `is_template: false` straight through to AppKit and never restores it,
/// so the first frame of the first recording turned the mark solid black
/// and it stayed black — on every menu bar, light or dark — until the app
/// was restarted. The flag being set once at build time is not enough.
///
/// The loop idles slowly and only wakes at frame rate while there is
/// something to draw; a menu-bar item has no business waking the machine
/// twelve times a second to redraw a picture that is not changing.
fn watch(app: AppHandle, tray: TrayIcon) {
    let frame_gap = PULSE_PERIOD / PULSE_FRAMES as u32;
    tauri::async_runtime::spawn(async move {
        let mut step = 0usize;
        let mut pulsing = false;
        loop {
            let active = is_active(&app);
            tokio::time::sleep(if active { frame_gap } else { IDLE_POLL }).await;

            let frames = app.state::<Frames>();
            if is_active(&app) {
                pulsing = true;
                let icon = frames.pulse[step % PULSE_FRAMES].clone();
                step = step.wrapping_add(1);
                if let Err(err) = tray.set_icon_with_as_template(Some(icon), true) {
                    log::warn!("could not step the tray pulse: {err}");
                }
            } else if pulsing {
                pulsing = false;
                step = 0;
                if let Err(err) = tray.set_icon_with_as_template(Some(frames.rest.clone()), true) {
                    log::warn!("could not still the tray icon: {err}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest() -> Image<'static> {
        Image::from_bytes(TEMPLATE)
            .expect("the bundled template decodes")
            .to_owned()
    }

    #[test]
    fn the_template_is_the_shape_the_menu_bar_expects() {
        let image = rest();
        assert_eq!(image.width(), 48);
        assert_eq!(image.height(), 36);
        assert_eq!(image.rgba().len(), 48 * 36 * 4);
    }

    #[test]
    fn the_mark_leaves_the_dot_strip_empty() {
        let image = rest();
        let rgba = image.rgba();
        for y in 0..36usize {
            for x in 36..48usize {
                let alpha = rgba[(y * 48 + x) * 4 + 3];
                assert_eq!(alpha, 0, "the mark reaches into the dot strip at {x},{y}");
            }
        }
    }

    #[test]
    fn breathing_never_disturbs_the_mark() {
        let base = rest();
        for step in 0..PULSE_FRAMES {
            let frame = with_dot(&base, breath(step));
            for y in 0..36usize {
                for x in 0..36usize {
                    let index = (y * 48 + x) * 4 + 3;
                    assert_eq!(
                        frame.rgba()[index],
                        base.rgba()[index],
                        "step {step} moved the mark at {x},{y}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_dot_fades_but_never_vanishes() {
        let base = rest();
        let centre = ((DOT_CENTRE.1 as usize) * 48 + DOT_CENTRE.0 as usize) * 4 + 3;
        let alphas: Vec<u8> = (0..PULSE_FRAMES)
            .map(|step| with_dot(&base, breath(step)).rgba()[centre])
            .collect();

        assert!(
            alphas.iter().all(|&a| a > 0),
            "the dot disappeared: {alphas:?}"
        );
        assert_eq!(
            *alphas.iter().max().unwrap(),
            255,
            "the dot never reaches full: {alphas:?}"
        );
        // Dimmest at the ends of the cycle, brightest in the middle — a
        // breath, not a sawtooth.
        assert_eq!(alphas.iter().copied().min().unwrap(), alphas[0]);
        assert_eq!(
            alphas.iter().copied().max().unwrap(),
            alphas[PULSE_FRAMES / 2]
        );
    }

    #[test]
    fn every_frame_is_the_same_size_as_the_mark() {
        let base = rest();
        for step in 0..PULSE_FRAMES {
            let frame = with_dot(&base, breath(step));
            assert_eq!(
                (frame.width(), frame.height()),
                (base.width(), base.height())
            );
        }
    }
}
