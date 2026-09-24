//! Looking notes up in this Mac's calendar.
//!
//! "I said that in the meeting with Paul" is how people find things again,
//! and a note did not know which meeting it was said around. This loop
//! asks the backend which notes this Mac has not looked up yet, reads the
//! ticked calendars around each one, and sends back the meetings it found
//! near them — before, during or after, and how far. The backend matches
//! each meeting against the graph and keeps only who and what it was with;
//! the title and attendee list go no further (ADR 0016). Whether a note
//! actually belongs to the meeting is then read from what it says, on the
//! backend: a note spoken during a meeting can just as well be about
//! something else entirely.
//!
//! Every note, not only this Mac's own: a note spoken on the phone during
//! a meeting in this Mac's calendar was spoken during that meeting all the
//! same. Each Mac answers for its own calendars, once per note.
//!
//! Nothing happens until a calendar is ticked, and then the notes from
//! before are looked up too — the calendar remembers last month's
//! meetings, so last month's notes can learn theirs.

use std::time::Duration;

use chrono::{DateTime, Utc};
use contracts::{CaptureMeetings, NearbyMeeting, OfferOccasionsRequest, PendingOccasion};
use tauri::{AppHandle, Manager};

use crate::calendar::{self, Access, Meeting};
use crate::settings::SettingsState;

const TICK: Duration = Duration::from_secs(120);
/// Asked for per round, and at most this many rounds per tick, so months
/// of notes are caught up on within a few minutes without one tick
/// holding the calendar for long.
const BATCH: usize = 200;
const ROUNDS: usize = 10;

/// The meetings near one moment, from that moment's point of view.
///
/// A meeting the note falls inside is "during"; one that begins within the
/// window after it is "before" (the note is preparation); one that ended
/// within the window before it is "after" (follow-up). A note between two
/// meetings can be both.
pub fn nearby(
    meetings: &[Meeting],
    at: DateTime<Utc>,
    window: chrono::Duration,
) -> Vec<NearbyMeeting> {
    meetings
        .iter()
        .filter_map(|meeting| {
            let (phase, gap) = if at < meeting.starts_at {
                ("before", meeting.starts_at - at)
            } else if at > meeting.ends_at {
                ("after", at - meeting.ends_at)
            } else {
                ("during", chrono::Duration::zero())
            };
            (gap <= window).then(|| NearbyMeeting {
                title: meeting.title.clone(),
                people: meeting.people.clone(),
                phase: phase.to_string(),
                minutes: u32::try_from(gap.num_minutes()).unwrap_or(u32::MAX),
            })
        })
        .collect()
}

pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if let Err(err) = tick(&app).await {
                log::warn!("could not look notes up in the calendar: {err}");
            }
        }
    });
}

async fn tick(app: &AppHandle) -> Result<(), String> {
    let settings = app.state::<SettingsState>();
    let foresight = settings.snapshot().foresight();
    // Not looked up is not the same as looked up and found nothing: until
    // a calendar is ticked, nothing is sent, so ticking one later still
    // finds the meetings around every note from before.
    if foresight.calendar_ids.is_empty() || calendar::access() != Access::Granted {
        return Ok(());
    }
    let checker = settings.checker();
    let window = chrono::Duration::minutes(i64::from(foresight.window_minutes));

    for _ in 0..ROUNDS {
        let pending = pending(app, checker).await?;
        if pending.is_empty() {
            break;
        }
        let full = pending.len() >= BATCH;

        let earliest = pending
            .iter()
            .map(|p| p.occurred_at)
            .min()
            .unwrap_or_else(Utc::now);
        let latest = pending
            .iter()
            .map(|p| p.occurred_at)
            .max()
            .unwrap_or_else(Utc::now);
        let ids = foresight.calendar_ids.clone();
        let meetings = tauri::async_runtime::spawn_blocking(move || {
            calendar::meetings(&ids, earliest - window, latest + window)
        })
        .await
        .unwrap_or_default();

        let captures = pending
            .iter()
            .map(|p| CaptureMeetings {
                capture_event_id: p.capture_event_id,
                meetings: nearby(&meetings, p.occurred_at, window),
            })
            .collect();
        offer(app, OfferOccasionsRequest { checker, captures }).await?;

        if !full {
            break;
        }
    }
    Ok(())
}

async fn pending(app: &AppHandle, checker: uuid::Uuid) -> Result<Vec<PendingOccasion>, String> {
    let settings = app.state::<SettingsState>();
    let url = format!(
        "{}/occasions/pending?checker={checker}&limit={BATCH}",
        settings.backend_base()
    );
    let mut request = app
        .state::<crate::backend::BackendClient>()
        .http()
        .get(&url)
        .timeout(Duration::from_secs(20));
    if let Some((id, secret)) = settings.credentials() {
        request = request
            .header("CF-Access-Client-Id", id)
            .header("CF-Access-Client-Secret", secret);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach the backend: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(crate::backend::describe_status(status, None)
            .unwrap_or_else(|| format!("the backend answered {status}")));
    }
    response
        .json()
        .await
        .map_err(|err| format!("the list of notes did not parse: {err}"))
}

async fn offer(app: &AppHandle, body: OfferOccasionsRequest) -> Result<(), String> {
    let settings = app.state::<SettingsState>();
    let url = format!("{}/occasions", settings.backend_base());
    let mut request = app
        .state::<crate::backend::BackendClient>()
        .http()
        .post(&url)
        .timeout(Duration::from_secs(30))
        .json(&body);
    if let Some((id, secret)) = settings.credentials() {
        request = request
            .header("CF-Access-Client-Id", id)
            .header("CF-Access-Client-Secret", secret);
    }
    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach the backend: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(crate::backend::describe_status(status, None)
            .unwrap_or_else(|| format!("the backend answered {status}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 28, h, m, 0).unwrap()
    }

    fn meeting(start: DateTime<Utc>, end: DateTime<Utc>) -> Meeting {
        Meeting {
            key: format!("E@{}", start.timestamp()),
            title: "Abnahme".into(),
            starts_at: start,
            ends_at: end,
            people: vec!["Paul Hartmann".into()],
            calendar_id: "work".into(),
        }
    }

    #[test]
    fn a_note_between_two_meetings_follows_one_and_prepares_the_next() {
        let meetings = [meeting(at(9, 0), at(10, 0)), meeting(at(11, 0), at(12, 0))];
        let found = nearby(&meetings, at(10, 20), chrono::Duration::minutes(60));
        let phases: Vec<(&str, u32)> = found
            .iter()
            .map(|m| (m.phase.as_str(), m.minutes))
            .collect();
        assert_eq!(phases, vec![("after", 20), ("before", 40)]);
    }

    #[test]
    fn during_is_zero_minutes_and_outside_the_window_is_nothing() {
        let meetings = [meeting(at(9, 0), at(10, 0))];
        assert_eq!(
            nearby(&meetings, at(9, 30), chrono::Duration::minutes(30))[0].phase,
            "during"
        );
        assert!(nearby(&meetings, at(11, 1), chrono::Duration::minutes(60)).is_empty());
        assert!(nearby(&meetings, at(8, 29), chrono::Duration::minutes(30)).is_empty());
    }
}
