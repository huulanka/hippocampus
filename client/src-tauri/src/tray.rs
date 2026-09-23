//! The menu-bar presence: an always-there brain, and a menu — on either
//! click — that says what is coming up before it offers the way in and out.
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
//!
//! There is one more state, and it is the only one in colour: **lit**,
//! when a meeting is coming up that you meant to bring something up in
//! ([`crate::foresight`]). A dot was the first idea and was turned down
//! for a good reason — this mark is drawn fine, and a dot beside it is
//! exactly what nobody notices on an ordinary day. So the brain turns
//! clay and two sparkles appear in the strip, twinkle for a moment to
//! arrive, and then hold still until the meeting is dealt with. Recording
//! wins over lit: while you speak, the only question is "am I recording?".

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
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

/// The two sparkles of the "something is due" state, in template pixels:
/// centre and radius (tip to centre). Both inside the strip the brain
/// leaves empty, so the mark stays exactly where it always is — the
/// recording dot's strip, borrowed while nothing is recording.
const SPARKLES: [((f32, f32), f32); 2] = [((42.0, 10.5), 6.2), ((43.4, 26.2), 4.3)];

/// How strongly the inside of the brain is filled while lit. The outline
/// alone is the resting mark in another colour, and a colour change on a
/// hairline is still a hairline; a faint fill makes it glow without
/// turning it into a blob.
const GLOW_ALPHA: f32 = 0.34;

/// One twinkle, and how long it takes. Slower than the recording breath:
/// this is news arriving, not a machine working.
const SPARKLE_FRAMES: usize = 16;
const SPARKLE_PERIOD: Duration = Duration::from_millis(2400);

/// How long the sparkles twinkle before they hold still. A signal that
/// moves for as long as it is up — which can be all afternoon, while a
/// meeting waits for "did you bring it up?" — is the one that trains you
/// to stop seeing it. It twinkles to arrive, then stays lit, in colour,
/// until it is dealt with.
const TWINKLE_FOR: Duration = Duration::from_secs(40);

/// The colours of the lit state, which cannot be a template: macOS would
/// tint it back to monochrome, and monochrome is what the resting state
/// already is. `--clay` and `--ember` from tokens.css on a dark menu bar;
/// their `-deep` variants on a light one, where the bright pair drops
/// under 3:1 against the bar.
#[derive(Clone, Copy)]
struct Palette {
    brain: [u8; 3],
    spark: [u8; 3],
}

const ON_DARK: Palette = Palette {
    brain: [0xd0, 0x78, 0x50],
    spark: [0xf0, 0xb4, 0x5c],
};
const ON_LIGHT: Palette = Palette {
    brain: [0xb2, 0x60, 0x3c],
    spark: [0xc4, 0x80, 0x22],
};

/// How much of a pixel a four-pointed sparkle covers — the ✦ shape, whose
/// sides curve inwards between the tips. Supersampled 4×4: the curve is
/// what makes it read as a sparkle rather than a diamond, and at this
/// size a single sample per pixel flattens it back into one.
fn sparkle_coverage(px: usize, py: usize, centre: (f32, f32), radius: f32) -> f32 {
    const N: usize = 4;
    let mut inside = 0usize;
    for sy in 0..N {
        for sx in 0..N {
            let x = px as f32 + (sx as f32 + 0.5) / N as f32 - centre.0;
            let y = py as f32 + (sy as f32 + 0.5) / N as f32 - centre.1;
            // An astroid: |x|^½ + |y|^½ ≤ r^½.
            if x.abs().sqrt() + y.abs().sqrt() <= radius.sqrt() {
                inside += 1;
            }
        }
    }
    inside as f32 / (N * N) as f32
}

/// The lit mark: the brain in `palette.brain` wherever the template has
/// coverage, and the sparkles at the given strengths (0–1, each scaling
/// its size and opacity together).
fn lit(rest: &Image<'_>, palette: Palette, strengths: [f32; 2]) -> Image<'static> {
    let (width, height) = (rest.width() as usize, rest.height() as usize);
    let mut rgba = rest.rgba().to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&palette.brain);
    }
    let glow = (GLOW_ALPHA * 255.0).round() as u8;
    for index in interior(rest) {
        let i = index * 4;
        if rgba[i + 3] < glow {
            rgba[i + 3] = glow;
        }
    }

    for (((cx, cy), radius), strength) in SPARKLES.into_iter().zip(strengths) {
        if strength <= 0.0 {
            continue;
        }
        let r = radius * (0.72 + 0.28 * strength);
        let first_row = (cy - r - 1.0).floor().max(0.0) as usize;
        let last_row = ((cy + r + 1.0).ceil() as usize).min(height);
        let first_col = (cx - r - 1.0).floor().max(36.0) as usize;
        let last_col = ((cx + r + 1.0).ceil() as usize).min(width);
        for y in first_row..last_row {
            for x in first_col..last_col {
                let coverage = sparkle_coverage(x, y, (cx, cy), r);
                if coverage <= 0.0 {
                    continue;
                }
                let index = (y * width + x) * 4;
                let alpha = (coverage * (0.55 + 0.45 * strength) * 255.0).round() as u8;
                if alpha > rgba[index + 3] {
                    rgba[index..index + 3].copy_from_slice(&palette.spark);
                    rgba[index + 3] = alpha;
                }
            }
        }
    }

    Image::new_owned(rgba, width as u32, height as u32)
}

/// The pixels inside the brain's outline: everything a flood fill from
/// the image's border cannot reach without crossing the mark. Worked out
/// from the template itself, so it follows the mark if `mark.json` ever
/// changes rather than a second drawing of it having to be kept in step.
fn interior(rest: &Image<'_>) -> Vec<usize> {
    let (width, height) = (rest.width() as usize, rest.height() as usize);
    let rgba = rest.rgba();
    // "Wall" is generous on purpose: an antialiased edge pixel at low
    // alpha would otherwise be a gap the fill leaks through.
    let wall = |i: usize| rgba[i * 4 + 3] >= 40;
    let mut outside = vec![false; width * height];
    let mut queue: Vec<usize> = Vec::new();
    for x in 0..width {
        queue.push(x);
        queue.push((height - 1) * width + x);
    }
    for y in 0..height {
        queue.push(y * width);
        queue.push(y * width + width - 1);
    }
    while let Some(i) = queue.pop() {
        if outside[i] || wall(i) {
            continue;
        }
        outside[i] = true;
        let (x, y) = (i % width, i / width);
        if x > 0 {
            queue.push(i - 1);
        }
        if x + 1 < width {
            queue.push(i + 1);
        }
        if y > 0 {
            queue.push(i - width);
        }
        if y + 1 < height {
            queue.push(i + width);
        }
    }
    (0..width * height)
        .filter(|&i| !outside[i] && !wall(i))
        .collect()
}

/// Both sparkles' strength at `step` of a twinkle. Half a cycle apart, so
/// one is always coming up as the other goes down and the pair never
/// blinks off together.
fn twinkle(step: usize) -> [f32; 2] {
    let phase = step as f32 / SPARKLE_FRAMES as f32 * std::f32::consts::TAU;
    let wave = |offset: f32| (1.0 - (phase + offset).cos()) / 2.0;
    [wave(0.0), wave(std::f32::consts::PI)]
}

/// Whether the menu bar is dark right now. Read from the system setting
/// rather than cached: it changes at sunset for anyone on Auto.
#[cfg(target_os = "macos")]
fn menu_bar_is_dark() -> bool {
    use objc2_foundation::{NSString, NSUserDefaults};
    NSUserDefaults::standardUserDefaults()
        .stringForKey(&NSString::from_str("AppleInterfaceStyle"))
        .is_some_and(|style| style.to_string().eq_ignore_ascii_case("dark"))
}

#[cfg(not(target_os = "macos"))]
fn menu_bar_is_dark() -> bool {
    true
}

/// The rendered frames, built once at startup rather than on every tick.
struct Frames {
    rest: Image<'static>,
    pulse: Vec<Image<'static>>,
    /// The twinkle on a dark and on a light menu bar. The held-still frame
    /// is the last of each: both sparkles at full.
    lit_dark: Vec<Image<'static>>,
    lit_light: Vec<Image<'static>>,
    still_dark: Image<'static>,
    still_light: Image<'static>,
}

impl Frames {
    fn render() -> tauri::Result<Self> {
        let rest = Image::from_bytes(TEMPLATE)?.to_owned();
        let pulse = (0..PULSE_FRAMES)
            .map(|step| with_dot(&rest, breath(step)))
            .collect();
        let frames = |palette| {
            (0..SPARKLE_FRAMES)
                .map(|step| lit(&rest, palette, twinkle(step)))
                .collect()
        };
        Ok(Self {
            lit_dark: frames(ON_DARK),
            lit_light: frames(ON_LIGHT),
            still_dark: lit(&rest, ON_DARK, [1.0, 0.85]),
            still_light: lit(&rest, ON_LIGHT, [1.0, 0.85]),
            rest,
            pulse,
        })
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

/// One line of the menu, before it becomes an AppKit item. Worked out by
/// [`entries`], a pure function of what is known, so what the menu says
/// can be tested without a menu bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// Clickable, with the id [`on_menu`] acts on.
    Item {
        id: String,
        text: String,
    },
    /// A line that only says something — a quote, who it is about.
    Note(String),
    Separator,
}

/// How long a quote may run in the menu before it is cut. A menu item is
/// one line; past this it is wider than most screens are comfortable with.
const MENU_QUOTE_CHARS: usize = 60;

/// How often the menu's contents are worked out again. Its finest unit is
/// a minute; a second is plenty, and it is a comparison of a few strings.
const MENU_CHECK: Duration = Duration::from_secs(1);

/// Indent for the lines under a meeting, so they read as belonging to it.
const UNDER: &str = "      ";

fn quoted(words: &str) -> String {
    let cut: String = if words.chars().count() > MENU_QUOTE_CHARS {
        let head: String = words.chars().take(MENU_QUOTE_CHARS - 1).collect();
        format!("{}…", head.trim_end())
    } else {
        words.to_string()
    };
    let lower = words.to_lowercase();
    let german = lower.contains(['ä', 'ö', 'ü', 'ß'])
        || [
            " ich ", " und ", " der ", " die ", " das ", " noch ", " mit ", " muss ",
        ]
        .iter()
        .any(|w| format!(" {lower} ").contains(w));
    if german {
        format!("„{cut}“")
    } else {
        format!("“{cut}”")
    }
}

/// "In 8 min", "In 1 h 20", "Now", "Earlier".
fn when(meeting: &crate::foresight::MeetingView, now: chrono::DateTime<chrono::Utc>) -> String {
    if now >= meeting.ends_at {
        return "Earlier".to_string();
    }
    if now >= meeting.starts_at {
        return "Now".to_string();
    }
    let minutes = (meeting.starts_at - now).num_minutes().max(1);
    if minutes < 60 {
        format!("In {minutes} min")
    } else if minutes % 60 == 0 {
        format!("In {} h", minutes / 60)
    } else {
        format!("In {} h {}", minutes / 60, minutes % 60)
    }
}

/// The menu, top to bottom.
///
/// Meetings first, because they are why anyone clicks: the one the icon
/// is lit for leads, with what you meant to bring up under it — your words
/// while unlocked, only a count while locked. With nothing coming up, the
/// same place holds what is still on your mind. The app's own ways in
/// and out follow, as before.
pub fn entries(
    view: &crate::foresight::MenuView,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<Entry> {
    let mut out = Vec::new();

    let mut meetings: Vec<&crate::foresight::MeetingView> = view.meetings.iter().collect();
    // The lit one first; the rest keep their order (soonest first).
    meetings.sort_by_key(|m| view.attention.as_deref() != Some(m.key.as_str()));

    let mut something_locked = false;
    for meeting in &meetings {
        let lit = !meeting.intentions.is_empty();
        let mark = if lit { "✦  " } else { "    " };
        out.push(Entry::Item {
            id: format!("brief:{}", meeting.key),
            text: format!("{mark}{} · {}", when(meeting, now), meeting.title),
        });
        if !lit {
            continue;
        }
        let over = now >= meeting.ends_at;
        if view.locked {
            something_locked = true;
            let n = meeting.intentions.len();
            out.push(Entry::Note(format!(
                "{UNDER}{}",
                match (over, n) {
                    (true, _) => "Did you bring it up?".to_string(),
                    (false, 1) => "Something to bring up".to_string(),
                    (false, n) => format!("{n} things to bring up"),
                }
            )));
            continue;
        }
        if over {
            out.push(Entry::Item {
                id: format!("brief:{}", meeting.key),
                text: format!("{UNDER}Did you bring it up?"),
            });
        }
        for id in &meeting.intentions {
            let Some(words) = view.words.iter().find(|w| w.id == *id) else {
                continue;
            };
            out.push(Entry::Note(format!("{UNDER}{}", quoted(&words.line))));
            if !words.about.is_empty() {
                out.push(Entry::Note(format!("{UNDER}{}", words.about)));
            }
        }
    }

    if meetings.is_empty() {
        out.push(Entry::Note(
            "Nothing coming up you've talked about".to_string(),
        ));
        if !view.locked && !view.words.is_empty() {
            out.push(Entry::Separator);
            out.push(Entry::Note("Still on your mind".to_string()));
            for words in view.words.iter().take(3) {
                out.push(Entry::Item {
                    id: "show".to_string(),
                    text: format!("✦  {}", quoted(&words.line)),
                });
            }
        }
    }

    if something_locked {
        out.push(Entry::Separator);
        out.push(Entry::Item {
            id: "unlock".to_string(),
            text: "Unlock to See What You Meant to Say…".to_string(),
        });
    }

    out.push(Entry::Separator);
    out.push(Entry::Item {
        id: "note".to_string(),
        text: "Write a Note…".to_string(),
    });
    out.push(Entry::Item {
        id: "show".to_string(),
        text: "Open Hippocampus".to_string(),
    });
    out.push(Entry::Item {
        id: "review".to_string(),
        text: "Look Back on the Week".to_string(),
    });
    out.push(Entry::Separator);
    out.push(Entry::Item {
        id: "quit".to_string(),
        text: "Quit Hippocampus".to_string(),
    });
    out
}

fn build_menu(app: &AppHandle, entries: &[Entry]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            Entry::Item { id, text } => {
                menu.append(&MenuItem::with_id(
                    app,
                    id.as_str(),
                    text,
                    true,
                    None::<&str>,
                )?)?;
            }
            Entry::Note(text) => {
                // Disabled, which AppKit draws dimmed — a line that says
                // something rather than one that does something. Ids only
                // have to be unique; nothing acts on these.
                menu.append(&MenuItem::with_id(
                    app,
                    format!("note:{index}"),
                    text,
                    false,
                    None::<&str>,
                )?)?;
            }
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

/// Opens the capture sheet for typing — the menu's "Write a Note…".
pub const WRITE_NOTE_EVENT: &str = "hippocampus://write-note";

/// What a menu item does.
fn on_menu(app: &AppHandle, id: &str) {
    if let Some(key) = id.strip_prefix("brief:") {
        summon(app);
        if let Err(err) = app.emit_to("main", crate::foresight::OPEN_BRIEF_EVENT, key.to_string()) {
            log::warn!("could not ask the webview to open the brief: {err}");
        }
        return;
    }
    match id {
        "show" => summon(app),
        "review" => {
            summon(app);
            if let Err(err) = app.emit(crate::review::OPEN_REVIEW_EVENT, ()) {
                log::warn!("could not ask the webview to open the review: {err}");
            }
        }
        // Typing, not speaking: the capture sheet with the cursor in the
        // field. Deliberately not `summon_capture`, which would start the
        // microphone (ADR 0012).
        "note" => {
            summon(app);
            if let Err(err) = app.emit_to("main", WRITE_NOTE_EVENT, ()) {
                log::warn!("could not ask the webview for a note: {err}");
            }
        }
        "unlock" => unlock_from_menu(app.clone()),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// "Unlock…" from the menu: the same Touch ID the window's gate asks for,
/// and afterwards the window is open too.
fn unlock_from_menu(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let settings = app.state::<crate::settings::SettingsState>();
        let lock = app.state::<crate::lock::LockState>();
        let granted = if settings.lock_armed() {
            tauri::async_runtime::spawn_blocking(crate::lock::authenticate)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or(false)
        } else {
            true
        };
        if granted {
            lock.unlock();
            crate::lock::announce_unlocked(&app, settings.lock_status(&lock));
        }
    });
}

/// Builds the tray icon, its menu, and starts the pulse watcher.
///
/// Registered from `setup()`, after the window it points back at
/// already exists.
///
/// Both clicks open the menu. A custom panel was tried and taken out
/// again: on this status item AppKit hands a left click straight to the
/// menu, so the click never reaches the app to open anything else — and
/// the user found the native menu the better answer anyway.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    app.manage(Activity::default());
    let frames = Frames::render()?;

    let first = entries(&crate::foresight::menu_view(app), chrono::Utc::now());
    let menu = build_menu(app, &first)?;

    let tray = TrayIconBuilder::with_id("main")
        .icon(frames.rest.clone())
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;

    app.manage(frames);
    watch(app.clone(), tray, first);
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
fn watch(app: AppHandle, tray: TrayIcon, first: Vec<Entry>) {
    let pulse_gap = PULSE_PERIOD / PULSE_FRAMES as u32;
    let twinkle_gap = SPARKLE_PERIOD / SPARKLE_FRAMES as u32;
    tauri::async_runtime::spawn(async move {
        let mut step = 0usize;
        let mut shown = Shown::Rest;
        // When the current lit spell began, and for which meeting: a
        // different meeting lighting up is news again, and twinkles again.
        let mut lit_since: Option<(std::time::Instant, String)> = None;
        // The menu as last set. Rebuilt only when what it would say has
        // changed — a countdown ticking over, a meeting arriving, the lock
        // closing — never just because a second passed.
        let mut menu_shown = first;
        let mut menu_checked = std::time::Instant::now();
        loop {
            if menu_checked.elapsed() >= MENU_CHECK {
                menu_checked = std::time::Instant::now();
                let wanted = entries(&crate::foresight::menu_view(&app), chrono::Utc::now());
                if wanted != menu_shown {
                    match build_menu(&app, &wanted) {
                        Ok(menu) => {
                            if let Err(err) = tray.set_menu(Some(menu)) {
                                log::warn!("could not update the tray menu: {err}");
                            }
                        }
                        Err(err) => log::warn!("could not build the tray menu: {err}"),
                    }
                    menu_shown = wanted;
                }
            }

            let want = if is_active(&app) {
                Want::Pulse
            } else {
                match crate::foresight::attention(&app) {
                    Some(key) => {
                        if lit_since.as_ref().is_none_or(|(_, k)| *k != key) {
                            lit_since = Some((std::time::Instant::now(), key));
                            step = 0;
                        }
                        let twinkling = lit_since
                            .as_ref()
                            .is_some_and(|(since, _)| since.elapsed() < TWINKLE_FOR);
                        Want::Lit {
                            twinkling,
                            dark: menu_bar_is_dark(),
                        }
                    }
                    None => {
                        lit_since = None;
                        Want::Rest
                    }
                }
            };

            let frames = app.state::<Frames>();
            let (icon, template, next) = match want {
                Want::Pulse => {
                    let icon = frames.pulse[step % PULSE_FRAMES].clone();
                    step = step.wrapping_add(1);
                    (Some(icon), true, Shown::Moving)
                }
                Want::Lit {
                    twinkling: true,
                    dark,
                } => {
                    let set = if dark {
                        &frames.lit_dark
                    } else {
                        &frames.lit_light
                    };
                    let icon = set[step % SPARKLE_FRAMES].clone();
                    step = step.wrapping_add(1);
                    (Some(icon), false, Shown::Moving)
                }
                Want::Lit {
                    twinkling: false,
                    dark,
                } => {
                    let still = Shown::Still { dark };
                    let icon = (shown != still).then(|| {
                        if dark {
                            frames.still_dark.clone()
                        } else {
                            frames.still_light.clone()
                        }
                    });
                    (icon, false, still)
                }
                Want::Rest => {
                    let icon = (shown != Shown::Rest).then(|| frames.rest.clone());
                    if shown != Shown::Rest {
                        step = 0;
                    }
                    (icon, true, Shown::Rest)
                }
            };
            if let Some(icon) = icon {
                if let Err(err) = tray.set_icon_with_as_template(Some(icon), template) {
                    log::warn!("could not change the tray icon: {err}");
                }
            }
            shown = next;

            tokio::time::sleep(match want {
                Want::Pulse => pulse_gap,
                Want::Lit {
                    twinkling: true, ..
                } => twinkle_gap,
                _ => IDLE_POLL,
            })
            .await;
        }
    });
}

/// What the loop is being asked to draw.
#[derive(Clone, Copy)]
enum Want {
    Pulse,
    Lit { twinkling: bool, dark: bool },
    Rest,
}

/// What the loop last drew, so an unchanging state is set once rather
/// than on every poll.
#[derive(Clone, Copy, PartialEq)]
enum Shown {
    Rest,
    Moving,
    Still { dark: bool },
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

    fn menu(locked: bool, meetings: bool) -> Vec<Entry> {
        use crate::foresight::{IntentionWords, MeetingView, MenuView, Phase};
        use chrono::TimeZone;
        let now = chrono::Utc.with_ymd_and_hms(2026, 9, 24, 9, 52, 0).unwrap();
        let id = uuid::Uuid::from_u128(1);
        let view = MenuView {
            meetings: if meetings {
                vec![MeetingView {
                    key: "m".into(),
                    title: "Jour fixe Paul".into(),
                    starts_at: now + chrono::Duration::minutes(8),
                    ends_at: now + chrono::Duration::minutes(38),
                    people: vec![],
                    phase: Phase::Ahead,
                    known: 1,
                    intentions: vec![id],
                    answered: false,
                }]
            } else {
                vec![]
            },
            attention: meetings.then(|| "m".to_string()),
            words: if locked {
                vec![]
            } else {
                vec![IntentionWords {
                    id,
                    line: "Da fehlt noch irgendwo eine Route".into(),
                    about: "Paul · Northwind".into(),
                }]
            },
            locked,
        };
        entries(&view, now)
    }

    fn texts(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .filter_map(|e| match e {
                Entry::Item { text, .. } | Entry::Note(text) => Some(text.trim().to_string()),
                Entry::Separator => None,
            })
            .collect()
    }

    #[test]
    fn unlocked_the_menu_shows_the_words_under_the_meeting() {
        let t = texts(&menu(false, true));
        assert_eq!(t[0], "✦  In 8 min · Jour fixe Paul");
        assert_eq!(t[1], "„Da fehlt noch irgendwo eine Route“");
        assert_eq!(t[2], "Paul · Northwind");
        assert!(t.contains(&"Write a Note…".to_string()));
        assert!(!t.iter().any(|x| x.starts_with("Unlock")));
    }

    #[test]
    fn locked_the_menu_says_that_not_what() {
        let t = texts(&menu(true, true));
        assert_eq!(t[1], "Something to bring up");
        assert!(!t.iter().any(|x| x.contains("Route")));
        assert!(t.iter().any(|x| x.starts_with("Unlock")));
    }

    #[test]
    fn with_nothing_coming_up_it_shows_what_is_still_on_your_mind() {
        let t = texts(&menu(false, false));
        assert_eq!(t[0], "Nothing coming up you've talked about");
        assert!(t.iter().any(|x| x.contains("Route")));
    }

    #[test]
    fn lit_keeps_the_mark_where_it_is_and_only_recolours_it() {
        let base = rest();
        let inside = interior(&base);
        // The fill has to be *inside*: a leak through a gap in the
        // outline would flood the whole icon.
        assert!(
            !inside.is_empty() && inside.len() < 36 * 36 / 2,
            "{}",
            inside.len()
        );
        for step in 0..SPARKLE_FRAMES {
            for palette in [ON_DARK, ON_LIGHT] {
                let frame = lit(&base, palette, twinkle(step));
                for y in 0..36usize {
                    for x in 0..36usize {
                        let index = (y * 48 + x) * 4;
                        let glowing = inside.contains(&(y * 48 + x));
                        let (before, after) = (base.rgba()[index + 3], frame.rgba()[index + 3]);
                        assert!(
                            after == before || (glowing && after > before),
                            "step {step} changed the mark at {x},{y}: {before} → {after}"
                        );
                        if base.rgba()[index + 3] > 0 {
                            assert_eq!(frame.rgba()[index..index + 3], palette.brain);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_sparkles_are_visible_and_twinkle_out_of_step() {
        let base = rest();
        let ink = |frame: &Image<'_>| -> [u32; 2] {
            let mut sums = [0u32; 2];
            for y in 0..36usize {
                for x in 36..48usize {
                    let alpha = u32::from(frame.rgba()[(y * 48 + x) * 4 + 3]);
                    // The big one lives in the top half, the small one below.
                    sums[usize::from(y >= 18)] += alpha;
                }
            }
            sums
        };
        let still = ink(&lit(&base, ON_DARK, [1.0, 0.85]));
        assert!(
            still[0] > 255 * 12,
            "the big sparkle is too faint: {still:?}"
        );
        assert!(
            still[1] > 255 * 4,
            "the small sparkle is too faint: {still:?}"
        );

        // Half a cycle apart: when one is brightest the other is dimmest.
        let start = ink(&lit(&base, ON_DARK, twinkle(0)));
        let middle = ink(&lit(&base, ON_DARK, twinkle(SPARKLE_FRAMES / 2)));
        assert!(
            start[1] > middle[1] && middle[0] > start[0],
            "{start:?} {middle:?}"
        );
    }

    /// Writes the lit frames out as raw RGBA, for looking at at real size
    /// on a menu-bar coloured ground — the lesson of the first tray icon
    /// was that pixel art judged zoomed in is not the pixel art you get.
    ///
    ///     TRAY_PREVIEW_DIR=/tmp/tray cargo test tray_preview -- --ignored
    #[test]
    #[ignore]
    fn tray_preview() {
        let dir = PathBuf::from(std::env::var("TRAY_PREVIEW_DIR").expect("TRAY_PREVIEW_DIR"));
        std::fs::create_dir_all(&dir).unwrap();
        let base = rest();
        let write = |name: &str, image: &Image<'_>| {
            std::fs::write(dir.join(format!("{name}.rgba")), image.rgba()).unwrap();
        };
        write("rest", &base);
        write("still-dark", &lit(&base, ON_DARK, [1.0, 0.85]));
        write("still-light", &lit(&base, ON_LIGHT, [1.0, 0.85]));
        for step in [0, SPARKLE_FRAMES / 4, SPARKLE_FRAMES / 2] {
            write(
                &format!("twinkle-{step}"),
                &lit(&base, ON_DARK, twinkle(step)),
            );
        }
    }

    use std::path::PathBuf;

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
