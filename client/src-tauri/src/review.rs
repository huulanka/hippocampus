//! The one notification this app sends: once a week, the review is ready.
//!
//! `docs/product.md` (P11) rules out notifications, and for the reason
//! that still holds — every notification is a habit that has to be built
//! on purpose. This one is the deliberate exception: it hangs on an
//! appointment they set themselves (a weekday and a time in Settings), it
//! comes once a week and never more, and it can be switched off.
//!
//! It says nothing about what is in the notes. Reading is behind the lock
//! ([`crate::lock`]); a notification banner is not, and would otherwise be
//! the one place the notes show up on screen without asking.

use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::settings::{parse_time, ReviewSchedule, SettingsState};

/// Tells the webview the week is ready. It opens the review the next time
/// the window is in front — now, if it already is.
pub const REVIEW_DUE_EVENT: &str = "hippocampus://review-due";
/// Tells the webview to open the review right away (the tray's menu item).
pub const OPEN_REVIEW_EVENT: &str = "hippocampus://open-review";

/// Coarse on purpose: the appointment is a time of day, and a minute late
/// is on time.
const CHECK_EVERY: Duration = Duration::from_secs(60);

/// Checks once a minute whether this week's review is due, and announces
/// it once.
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || loop {
        check(&app);
        std::thread::sleep(CHECK_EVERY);
    });
}

fn check(app: &AppHandle) {
    let settings = app.state::<SettingsState>();
    let schedule = settings.snapshot().review_schedule();
    let now = Local::now().naive_local();
    let Some(week) = due(&schedule, now) else {
        return;
    };
    if !settings.claim_review_announcement(&week) {
        return;
    }

    log::info!("announcing the weekly review for {week}");
    if let Err(err) = app
        .notification()
        .builder()
        .title("Your week is ready")
        .body("Look back on it in Hippocampus.")
        .show()
    {
        log::warn!("could not show the weekly review notification: {err}");
    }
    if let Err(err) = app.emit(REVIEW_DUE_EVENT, ()) {
        log::warn!("could not tell the webview the review is due: {err}");
    }
}

/// The ISO week to announce, if the appointment in this week has passed.
///
/// "Has passed" rather than "is now": a Mac that was asleep at four on
/// Friday should still say so when it wakes at six, and one that was off
/// all weekend on Monday should not — by then it is a new week, and last
/// week's review is one page back rather than news.
fn due(schedule: &ReviewSchedule, now: NaiveDateTime) -> Option<String> {
    if !schedule.enabled {
        return None;
    }
    let (hours, minutes) = parse_time(&schedule.time)?;
    let today = now.date();
    let monday = today - chrono::Days::new(u64::from(today.weekday().num_days_from_monday()));
    let day: NaiveDate = monday + chrono::Days::new(u64::from(schedule.weekday));
    // Compared as wall-clock times, so an appointment inside a DST gap
    // (02:30 on the night the clocks go forward) has simply passed once
    // the clock reads later than it.
    let appointment = day.and_hms_opt(hours, minutes, 0)?;
    (now >= appointment).then(|| today.format("%G-W%V").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap()
    }

    fn friday_at_four() -> ReviewSchedule {
        ReviewSchedule::default()
    }

    #[test]
    fn not_before_the_appointment() {
        // Wednesday 23 September 2026.
        assert_eq!(due(&friday_at_four(), at("2026-09-23 18:00")), None);
        assert_eq!(due(&friday_at_four(), at("2026-09-25 15:59")), None);
    }

    #[test]
    fn from_the_appointment_until_the_week_ends() {
        let week = Some("2026-W39".to_string());
        assert_eq!(due(&friday_at_four(), at("2026-09-25 16:00")), week);
        assert_eq!(due(&friday_at_four(), at("2026-09-27 23:59")), week);
        // Monday is a new week, and its appointment is still ahead.
        assert_eq!(due(&friday_at_four(), at("2026-09-28 09:00")), None);
    }

    #[test]
    fn switched_off_means_never() {
        let off = ReviewSchedule {
            enabled: false,
            ..ReviewSchedule::default()
        };
        assert_eq!(due(&off, at("2026-09-27 12:00")), None);
    }
}
