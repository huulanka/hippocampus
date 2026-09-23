//! The calendars this Mac is told to watch, read through EventKit.
//!
//! Read-only, and only ever the calendars ticked in Settings on *this*
//! Mac. That is the whole of the privacy model and it is deliberately
//! simple: a work Mac ticks the Exchange calendar, a private one ticks
//! iCloud, and a work meeting can then never surface on the private
//! machine, because the private machine never reads it (docs/
//! prospective-memory.md, F4). Nothing is ticked by default — access to
//! the calendar and choosing what to watch are two separate decisions.
//!
//! A new `EKEventStore` per call rather than one kept for the life of the
//! app. A store created before access was granted never sees any
//! calendars, and one checked twice a minute costs nothing worth saving.

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Whether this app may read the calendar, as macOS sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Never asked. The Settings button asks.
    NotDetermined,
    /// Asked and refused, or switched off later in System Settings. Only
    /// System Settings can change it now; asking again shows nothing.
    Denied,
    /// A device policy forbids it.
    Restricted,
    Granted,
    /// Allowed to add events but not to read them — useless here, and
    /// said so rather than shown as granted.
    WriteOnly,
}

/// One calendar, for the list in Settings.
#[derive(Debug, Clone, Serialize)]
pub struct CalendarInfo {
    pub id: String,
    pub title: String,
    /// The account it belongs to — "iCloud", "Exchange", an address.
    /// Shown because two calendars called "Kalender" are otherwise
    /// indistinguishable, and that is the exact choice F4 hinges on.
    pub account: String,
    /// `#rrggbb`, as Calendar.app draws it.
    pub color: Option<String>,
}

/// One meeting, as far as Hippocampus needs to know it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Meeting {
    /// Stable for one occurrence of one event. A recurring event keeps its
    /// identifier across occurrences, so the start is part of the key —
    /// otherwise answering "did you bring it up?" for Monday's Jour fixe
    /// would answer it for every Monday after.
    pub key: String,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    /// Display names of the other people invited; the owner and rooms
    /// are left out.
    pub people: Vec<String>,
    pub calendar_id: String,
}

pub fn occurrence_key(event_id: &str, starts_at: DateTime<Utc>) -> String {
    format!("{event_id}@{}", starts_at.timestamp())
}

/// An attendee's name for matching: the display name if the calendar has
/// one, otherwise the address without `mailto:`.
pub fn attendee_name(name: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) {
        return Some(name.to_string());
    }
    let url = url?.trim();
    let address = url.strip_prefix("mailto:").unwrap_or(url);
    (!address.is_empty()).then(|| address.to_string())
}

#[cfg(target_os = "macos")]
mod platform {
    use chrono::{DateTime, TimeZone, Utc};
    use objc2::rc::Retained;
    use objc2_app_kit::NSColorSpace;
    use objc2_event_kit::{
        EKAuthorizationStatus, EKCalendar, EKEntityType, EKEventStore, EKParticipantType,
    };
    use objc2_foundation::{NSArray, NSDate};

    use super::{attendee_name, occurrence_key, Access, CalendarInfo, Meeting};

    fn store() -> Retained<EKEventStore> {
        // SAFETY: a plain `init`, no requirements beyond being called.
        unsafe { EKEventStore::new() }
    }

    pub fn access() -> Access {
        // SAFETY: a class method with no preconditions.
        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        match status {
            EKAuthorizationStatus::FullAccess => Access::Granted,
            EKAuthorizationStatus::Denied => Access::Denied,
            EKAuthorizationStatus::Restricted => Access::Restricted,
            EKAuthorizationStatus::WriteOnly => Access::WriteOnly,
            _ => Access::NotDetermined,
        }
    }

    /// Blocks until the user answers the system dialog, the same way
    /// [`crate::lock`] waits for Touch ID.
    ///
    /// Needs `NSCalendarsFullAccessUsageDescription` in Info.plist; without
    /// it macOS does not ask, it terminates the app.
    pub fn request() -> Result<Access, String> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let store = store();
        // SAFETY: the block is called once and only sends on the channel,
        // which outlives it because this function blocks on it below.
        unsafe {
            let reply = block2::RcBlock::new(
                move |_granted: objc2::runtime::Bool, error: *mut objc2_foundation::NSError| {
                    let outcome = if error.is_null() {
                        Ok(())
                    } else {
                        Err((*error).localizedDescription().to_string())
                    };
                    let _ = tx.send(outcome);
                },
            );
            store.requestFullAccessToEventsWithCompletion(block2::RcBlock::as_ptr(&reply));
        }
        rx.recv()
            .unwrap_or_else(|_| Err("the calendar permission dialog went away".to_string()))?;
        Ok(access())
    }

    fn hex(calendar: &EKCalendar) -> Option<String> {
        // SAFETY: `color` is always set for an existing calendar; the
        // conversion returns nil for a colour that has no sRGB form, which
        // is handled.
        unsafe {
            let color = calendar.color();
            let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
            let channel = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            Some(format!(
                "#{:02x}{:02x}{:02x}",
                channel(srgb.redComponent()),
                channel(srgb.greenComponent()),
                channel(srgb.blueComponent())
            ))
        }
    }

    pub fn calendars() -> Vec<CalendarInfo> {
        if access() != Access::Granted {
            return Vec::new();
        }
        let store = store();
        // SAFETY: plain property reads on objects EventKit just handed us.
        unsafe {
            store
                .calendarsForEntityType(EKEntityType::Event)
                .iter()
                .map(|calendar| CalendarInfo {
                    id: calendar.calendarIdentifier().to_string(),
                    title: calendar.title().to_string(),
                    account: calendar
                        .source()
                        .map(|source| source.title().to_string())
                        .unwrap_or_default(),
                    color: hex(&calendar),
                })
                .collect()
        }
    }

    fn date(at: DateTime<Utc>) -> Retained<NSDate> {
        NSDate::dateWithTimeIntervalSince1970(at.timestamp_millis() as f64 / 1000.0)
    }

    fn utc(date: &NSDate) -> Option<DateTime<Utc>> {
        let millis = (date.timeIntervalSince1970() * 1000.0).round() as i64;
        Utc.timestamp_millis_opt(millis).single()
    }

    /// Timed meetings overlapping `[from, to)` in the given calendars.
    /// All-day events are left out: "Urlaub" is not a meeting anyone walks
    /// into ten minutes from now.
    pub fn meetings(
        calendar_ids: &[String],
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<Meeting> {
        if calendar_ids.is_empty() || access() != Access::Granted {
            return Vec::new();
        }
        let store = store();
        // SAFETY: plain reads on objects EventKit just handed us; the
        // predicate is built by the store it is used with.
        unsafe {
            let chosen: Vec<Retained<EKCalendar>> = store
                .calendarsForEntityType(EKEntityType::Event)
                .iter()
                .filter(|calendar| {
                    let id = calendar.calendarIdentifier().to_string();
                    calendar_ids.contains(&id)
                })
                .collect();
            // An empty array here would mean "every calendar" to EventKit,
            // which is exactly what must never happen.
            if chosen.is_empty() {
                return Vec::new();
            }
            let chosen = NSArray::from_retained_slice(&chosen);
            let predicate = store.predicateForEventsWithStartDate_endDate_calendars(
                &date(from),
                &date(to),
                Some(&chosen),
            );

            store
                .eventsMatchingPredicate(&predicate)
                .iter()
                .filter(|event| !event.isAllDay())
                .filter_map(|event| {
                    let starts_at = utc(&event.startDate())?;
                    let ends_at = utc(&event.endDate())?;
                    let id = event
                        .eventIdentifier()
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| event.calendarItemIdentifier().to_string());
                    let people = event
                        .attendees()
                        .map(|attendees| {
                            attendees
                                .iter()
                                .filter(|p| !p.isCurrentUser())
                                .filter(|p| {
                                    let kind = p.participantType();
                                    kind != EKParticipantType::Room
                                        && kind != EKParticipantType::Resource
                                })
                                .filter_map(|p| {
                                    let name = p.name().map(|n| n.to_string());
                                    let url = p.URL().absoluteString().map(|u| u.to_string());
                                    attendee_name(name.as_deref(), url.as_deref())
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(Meeting {
                        key: occurrence_key(&id, starts_at),
                        title: event.title().to_string(),
                        starts_at,
                        ends_at,
                        people,
                        calendar_id: event
                            .calendar()
                            .map(|c| c.calendarIdentifier().to_string())
                            .unwrap_or_default(),
                    })
                })
                .collect()
        }
    }
}

/// Everywhere else there is no calendar to read. Kept so the crate still
/// builds and says so honestly.
#[cfg(not(target_os = "macos"))]
mod platform {
    use chrono::{DateTime, Utc};

    use super::{Access, CalendarInfo, Meeting};

    pub fn access() -> Access {
        Access::Restricted
    }
    pub fn request() -> Result<Access, String> {
        Err("this platform has no calendar to read".to_string())
    }
    pub fn calendars() -> Vec<CalendarInfo> {
        Vec::new()
    }
    pub fn meetings(_: &[String], _: DateTime<Utc>, _: DateTime<Utc>) -> Vec<Meeting> {
        Vec::new()
    }
}

pub use platform::{access, calendars, meetings, request};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attendee_is_named_by_display_name_first() {
        assert_eq!(
            attendee_name(Some("Paul Hartmann"), Some("mailto:paul@x.de")).as_deref(),
            Some("Paul Hartmann")
        );
        assert_eq!(
            attendee_name(Some("  "), Some("mailto:paul@x.de")).as_deref(),
            Some("paul@x.de")
        );
        assert_eq!(attendee_name(None, None), None);
    }

    #[test]
    fn each_occurrence_of_a_recurring_meeting_has_its_own_key() {
        let monday = Utc.with_ymd_and_hms(2026, 9, 28, 8, 0, 0).unwrap();
        let next = Utc.with_ymd_and_hms(2026, 10, 5, 8, 0, 0).unwrap();
        assert_ne!(occurrence_key("E1", monday), occurrence_key("E1", next));
    }

    use chrono::TimeZone;
}
