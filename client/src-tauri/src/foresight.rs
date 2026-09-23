//! The moment before a meeting, and the moment after.
//!
//! Twice a minute this looks at the calendars ticked on this Mac
//! ([`crate::calendar`]) and asks the backend, once per meeting, which of
//! the things you know it is about — and whether you meant to bring any
//! of them up (`POST /brief`). From the answer it decides three things:
//!
//! - whether the menu bar should sparkle ([`attention`]),
//! - whether a banner is due, once per meeting and phase,
//! - what the webview's Today and brief pages list ([`ForesightStatus`]).
//!
//! **What stays here, and what does not.** The brief's answer carries
//! your own sentences. None of them is kept or shown by this side: only
//! the meeting (which is yours, from your calendar), how many known
//! things it touches, and the ids of the open intentions. Reading the
//! words is the webview's business, through the same gated request path
//! as everything else ([`crate::backend::api_request`]) — so the menu bar
//! knows *that* something is due, never *what*, and a locked Mac shows
//! nothing it should not (docs/prospective-memory.md, F8).
//!
//! One exception, for the menu-bar menu, which AppKit draws from text this
//! process already holds: while — and only while — the notes are
//! unlocked, the open intentions' words are fetched and kept in memory
//! ([`menu_view`]). Locking drops them, and the lock is re-read at every
//! draw.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::calendar::{self, Access, Meeting};
use crate::settings::SettingsState;

/// Pushed to the webview whenever what it would list changes.
pub const FORESIGHT_EVENT: &str = "hippocampus://foresight";
/// Tells the webview to open a meeting's brief (tray click, banner).
pub const OPEN_BRIEF_EVENT: &str = "hippocampus://open-brief";

/// How often the calendar is looked at. A meeting's lead is minutes, so
/// half a minute late is on time; any finer and it is a timer waking the
/// machine to find nothing changed.
const TICK: Duration = Duration::from_secs(30);

/// How long a brief is trusted before it is asked for again. Short enough
/// that ticking an intention off elsewhere clears the sparkle here within
/// a couple of minutes; long enough that one meeting is not a request
/// every tick.
const REBRIEF_AFTER: chrono::Duration = chrono::Duration::minutes(2);

/// How long after a meeting "did you bring it up?" still makes sense. The
/// rest of the working day, roughly — the next morning it is a question
/// about a meeting nobody remembers the details of.
const ASK_WINDOW: chrono::Duration = chrono::Duration::hours(12);

/// Bookkeeping older than this is forgotten.
const FORGET_AFTER: chrono::Duration = chrono::Duration::days(3);

const FILE_NAME: &str = "foresight.json";

/// Where a meeting stands, seen from now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Further off than the lead. Not shown.
    Later,
    /// Starts within the lead.
    Ahead,
    /// Running.
    Now,
    /// Over, within the window in which it is worth asking about.
    After,
    /// Over for longer than that.
    Gone,
}

pub fn phase(meeting: &Meeting, now: DateTime<Utc>, lead: chrono::Duration) -> Phase {
    if now < meeting.starts_at - lead {
        Phase::Later
    } else if now < meeting.starts_at {
        Phase::Ahead
    } else if now < meeting.ends_at {
        Phase::Now
    } else if now < meeting.ends_at + ASK_WINDOW {
        Phase::After
    } else {
        Phase::Gone
    }
}

/// What the backend said about one meeting, reduced to what may be kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Briefed {
    pub known: usize,
    pub intentions: Vec<Uuid>,
    pub at: DateTime<Utc>,
}

/// What survives a restart: which banners were shown and which meetings
/// were answered. Without it, relaunching the app during a meeting would
/// announce it a second time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// `"<key>#<phase>"` → when it was announced.
    #[serde(default)]
    pub announced: BTreeMap<String, DateTime<Utc>>,
    /// Meeting key → when "did you bring it up?" was answered.
    #[serde(default)]
    pub answered: BTreeMap<String, DateTime<Utc>>,
}

impl Memory {
    fn forget_before(&mut self, cutoff: DateTime<Utc>) {
        self.announced.retain(|_, at| *at >= cutoff);
        self.answered.retain(|_, at| *at >= cutoff);
    }
}

/// One meeting as the webview lists it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MeetingView {
    pub key: String,
    pub title: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub people: Vec<String>,
    pub phase: Phase,
    /// How many known entities it touches.
    pub known: usize,
    /// Ids of the open intentions about them. The webview reads their
    /// words itself, behind the lock.
    pub intentions: Vec<Uuid>,
    /// "Did you bring it up?" was answered.
    pub answered: bool,
}

/// Everything the webview needs to draw the foresight surfaces.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ForesightStatus {
    pub access: Access,
    /// How many calendars are ticked on this Mac.
    pub watching: usize,
    /// Meetings worth showing, soonest first.
    pub meetings: Vec<MeetingView>,
    /// Whether the menu bar is sparkling, and for which meeting.
    pub attention: Option<String>,
    /// The last thing that went wrong asking the backend, if the last
    /// attempt failed. A brief that silently never arrives looks exactly
    /// like a meeting about nothing.
    pub error: Option<String>,
}

/// A banner to show now.
#[derive(Debug, Clone, PartialEq)]
pub struct Announcement {
    pub id: String,
    pub key: String,
    pub title: String,
    pub body: String,
}

/// The decision, as a pure function of what is known: which meetings to
/// list, whether to sparkle, which banners are due. Everything the loop
/// does that is worth testing lives here.
pub fn plan(
    meetings: &[Meeting],
    briefs: &HashMap<String, Briefed>,
    memory: &Memory,
    now: DateTime<Utc>,
    lead: chrono::Duration,
) -> (Vec<MeetingView>, Option<String>, Vec<Announcement>) {
    let mut views = Vec::new();
    let mut announcements = Vec::new();

    for meeting in meetings {
        let phase = phase(meeting, now, lead);
        if !matches!(phase, Phase::Ahead | Phase::Now | Phase::After) {
            continue;
        }
        let Some(brief) = briefs.get(&meeting.key) else {
            continue;
        };
        let answered = memory.answered.contains_key(&meeting.key);
        let open = !brief.intentions.is_empty();

        // A meeting about nothing known is not worth a line. One that is
        // over is only worth one while there is still a question to ask.
        let shown = brief.known > 0 && (phase != Phase::After || (open && !answered));
        if !shown {
            continue;
        }

        if open {
            let (tag, title, body) = match phase {
                Phase::Ahead => {
                    let minutes = (meeting.starts_at - now).num_minutes().max(1);
                    (
                        "ahead",
                        format!("In {minutes} min: {}", meeting.title),
                        "Something you meant to bring up.".to_string(),
                    )
                }
                // Launched mid-meeting: still worth saying, once, and
                // under the same tag so it is never said twice.
                Phase::Now => (
                    "ahead",
                    format!("Now: {}", meeting.title),
                    "Something you meant to bring up.".to_string(),
                ),
                _ if !answered => (
                    "after",
                    format!("{} is over", meeting.title),
                    "Did you bring it up?".to_string(),
                ),
                _ => ("", String::new(), String::new()),
            };
            let id = format!("{}#{tag}", meeting.key);
            if !tag.is_empty() && !memory.announced.contains_key(&id) {
                announcements.push(Announcement {
                    id,
                    key: meeting.key.clone(),
                    title,
                    body,
                });
            }
        }

        views.push(MeetingView {
            key: meeting.key.clone(),
            title: meeting.title.clone(),
            starts_at: meeting.starts_at,
            ends_at: meeting.ends_at,
            people: meeting.people.clone(),
            phase,
            known: brief.known,
            intentions: brief.intentions.clone(),
            answered,
        });
    }

    views.sort_by_key(|v| v.starts_at);

    // The sparkle goes to the meeting that needs you soonest: one about
    // to start or running beats one waiting for an answer.
    let attention = views
        .iter()
        .filter(|v| !v.intentions.is_empty())
        .filter(|v| v.phase != Phase::After || !v.answered)
        .min_by_key(|v| (v.phase == Phase::After, v.starts_at))
        .map(|v| v.key.clone());

    (views, attention, announcements)
}

struct Inner {
    meetings: Vec<Meeting>,
    briefs: HashMap<String, Briefed>,
    memory: Memory,
    status: ForesightStatus,
    /// The words of the open intentions, for the menu-bar menu — held
    /// **only while the notes are unlocked**, and dropped the moment they
    /// are not ([`words_while_unlocked`]). The one exception to "this
    /// side keeps ids, not words": a native menu is drawn from text this
    /// process already has, it cannot ask the webview at click time.
    words: Vec<IntentionWords>,
}

/// An open intention as the menu shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct IntentionWords {
    pub id: Uuid,
    /// The quote if there is one, else the model's phrasing.
    pub line: String,
    /// Who and what it is about, "Paul · Northwind".
    pub about: String,
}

/// Everything the menu-bar menu is drawn from.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuView {
    pub meetings: Vec<MeetingView>,
    pub attention: Option<String>,
    /// Empty while locked, whatever was fetched before.
    pub words: Vec<IntentionWords>,
    pub locked: bool,
}

/// What the menu should show right now. The lock is read here, at the
/// moment of drawing, so a menu can never show words the lock has just
/// closed over — even before the next tick has had a chance to drop them.
pub fn menu_view(app: &AppHandle) -> MenuView {
    let locked = is_locked(app);
    let Some(state) = app.try_state::<ForesightState>() else {
        return MenuView {
            meetings: Vec::new(),
            attention: None,
            words: Vec::new(),
            locked,
        };
    };
    let inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
    MenuView {
        meetings: inner.status.meetings.clone(),
        attention: inner.status.attention.clone(),
        words: if locked {
            Vec::new()
        } else {
            inner.words.clone()
        },
        locked,
    }
}

fn is_locked(app: &AppHandle) -> bool {
    let armed = app.state::<SettingsState>().lock_armed();
    armed && !app.state::<crate::lock::LockState>().is_unlocked()
}

/// Fetches the open intentions' words while unlocked; forgets them when
/// not. Rust-side, like the brief, so it carries the Service Token.
async fn words_while_unlocked(app: &AppHandle) {
    if is_locked(app) {
        if let Ok(mut inner) = app.state::<ForesightState>().inner.lock() {
            inner.words.clear();
        }
        return;
    }
    let settings = app.state::<SettingsState>();
    let url = format!("{}/intentions", settings.backend_base());
    let mut request = app
        .state::<crate::backend::BackendClient>()
        .http()
        .get(&url)
        .timeout(Duration::from_secs(10));
    if let Some((id, secret)) = settings.credentials() {
        request = request
            .header("CF-Access-Client-Id", id)
            .header("CF-Access-Client-Secret", secret);
    }
    let intentions = match request.send().await {
        Ok(response) if response.status().is_success() => {
            response.json::<Vec<contracts::Intention>>().await.ok()
        }
        Ok(response) => {
            log::warn!(
                "could not read intentions for the menu: {}",
                response.status()
            );
            None
        }
        Err(err) => {
            log::warn!("could not read intentions for the menu: {err}");
            None
        }
    };
    let Some(intentions) = intentions else { return };
    // Checked again after the await: the lock may have closed while the
    // request was out, and then these words must not be kept.
    if is_locked(app) {
        return;
    }
    if let Ok(mut inner) = app.state::<ForesightState>().inner.lock() {
        inner.words = intentions
            .into_iter()
            .map(|i| IntentionWords {
                id: i.id,
                line: i.quote.unwrap_or(i.text),
                about: i
                    .entities
                    .iter()
                    .map(|e| e.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" · "),
            })
            .collect();
    }
}

pub struct ForesightState {
    inner: Mutex<Inner>,
    wake: Notify,
    path: PathBuf,
}

impl ForesightState {
    pub fn load(path: PathBuf) -> Self {
        let memory = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Memory>(&raw).ok())
            .unwrap_or_default();
        Self {
            inner: Mutex::new(Inner {
                meetings: Vec::new(),
                briefs: HashMap::new(),
                words: Vec::new(),
                memory,
                status: ForesightStatus {
                    access: calendar::access(),
                    watching: 0,
                    meetings: Vec::new(),
                    attention: None,
                    error: None,
                },
            }),
            wake: Notify::new(),
            path,
        }
    }

    pub fn status(&self) -> ForesightStatus {
        self.inner
            .lock()
            .map(|inner| inner.status.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().status.clone())
    }

    /// Something changed that the loop should act on now rather than at
    /// the next tick: a calendar was ticked, an answer was given.
    pub fn poke(&self) {
        self.wake.notify_one();
    }

    fn persist(&self, memory: &Memory) {
        let write = || -> anyhow::Result<()> {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = self.path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(memory)?)?;
            std::fs::rename(&tmp, &self.path)?;
            Ok(())
        };
        if let Err(err) = write() {
            log::warn!("could not write {}: {err}", self.path.display());
        }
    }
}

pub fn state_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join(FILE_NAME))
}

/// The meeting the menu bar is sparkling for, if it is.
pub fn attention(app: &AppHandle) -> Option<String> {
    app.try_state::<ForesightState>()?.status().attention
}

/// Asks the backend about one meeting. Rust-side, so it carries the
/// Service Token and needs no CORS (ADR 0009).
async fn brief(app: &AppHandle, meeting: &Meeting) -> Result<contracts::Brief, String> {
    let settings = app.state::<SettingsState>();
    let base = settings.backend_base();
    let credentials = settings.credentials();
    let url = format!("{base}/brief");

    let mut request = app
        .state::<crate::backend::BackendClient>()
        .http()
        .post(&url)
        .timeout(Duration::from_secs(10))
        .json(&contracts::BriefRequest {
            title: meeting.title.clone(),
            people: meeting.people.clone(),
        });
    if let Some((id, secret)) = credentials {
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
        .json::<contracts::Brief>()
        .await
        .map_err(|err| format!("the brief did not parse: {err}"))
}

/// Starts the loop.
pub fn watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tick(&app).await;
            let state = app.state::<ForesightState>();
            let _ = tokio::time::timeout(TICK, state.wake.notified()).await;
        }
    });
}

async fn tick(app: &AppHandle) {
    let settings = app.state::<SettingsState>();
    let foresight = settings.snapshot().foresight();
    let lead = chrono::Duration::minutes(i64::from(foresight.lead_minutes));
    let now = Utc::now();
    let access = calendar::access();

    // EventKit is synchronous and can take a moment on a large calendar.
    let ids = foresight.calendar_ids.clone();
    let meetings = if access == Access::Granted && !ids.is_empty() {
        tauri::async_runtime::spawn_blocking(move || {
            calendar::meetings(
                &ids,
                now - ASK_WINDOW,
                now + lead + chrono::Duration::minutes(1),
            )
        })
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Which meetings need (re-)asking. Decided under the lock, asked
    // outside it: a request held across an await under a std Mutex would
    // block every command that reads the status.
    let due: Vec<Meeting> = {
        let state = app.state::<ForesightState>();
        let inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
        meetings
            .iter()
            .filter(|m| {
                matches!(
                    phase(m, now, lead),
                    Phase::Ahead | Phase::Now | Phase::After
                )
            })
            .filter(|m| {
                inner.briefs.get(&m.key).is_none_or(|b| {
                    // A meeting that is over and had nothing open has
                    // nothing left to change about it; only one still
                    // waiting for "did you bring it up?" is worth asking
                    // about again.
                    let settled = phase(m, now, lead) == Phase::After && b.intentions.is_empty();
                    now - b.at >= REBRIEF_AFTER && !settled
                })
            })
            .cloned()
            .collect()
    };

    let mut fresh = HashMap::new();
    let mut error = None;
    for meeting in &due {
        match brief(app, meeting).await {
            Ok(brief) => {
                fresh.insert(
                    meeting.key.clone(),
                    Briefed {
                        known: brief.entities.len(),
                        intentions: brief.intentions.iter().map(|i| i.id).collect(),
                        at: now,
                    },
                );
            }
            Err(err) => {
                log::warn!("could not brief a meeting: {err}");
                error = Some(err);
                // The cause is almost always shared (NAS asleep, token
                // expired); the rest would only fail the same way.
                break;
            }
        }
    }

    let state = app.state::<ForesightState>();
    let (status, announcements, memory) = {
        let mut inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.meetings = meetings;
        inner.briefs.extend(fresh);
        let keys: Vec<String> = inner.meetings.iter().map(|m| m.key.clone()).collect();
        inner.briefs.retain(|key, _| keys.contains(key));
        inner.memory.forget_before(now - FORGET_AFTER);

        let (views, attention, announcements) =
            plan(&inner.meetings, &inner.briefs, &inner.memory, now, lead);
        let status = ForesightStatus {
            access,
            watching: foresight.calendar_ids.len(),
            meetings: views,
            attention,
            error: error.or_else(|| {
                // Keep saying it while nothing has succeeded since.
                if due.is_empty() {
                    inner.status.error.clone()
                } else {
                    None
                }
            }),
        };
        let changed = status != inner.status;
        inner.status = status.clone();

        if foresight.banner {
            for a in &announcements {
                inner.memory.announced.insert(a.id.clone(), now);
            }
        }
        (
            changed.then_some(status),
            announcements,
            inner.memory.clone(),
        )
    };

    if foresight.banner && !announcements.is_empty() {
        state.persist(&memory);
        for a in &announcements {
            log::info!("announcing a meeting ({})", a.id);
            if let Err(err) = app
                .notification()
                .builder()
                .title(&a.title)
                .body(&a.body)
                .show()
            {
                log::warn!("could not show the meeting notification: {err}");
            }
        }
    }

    if let Some(status) = status {
        if let Err(err) = app.emit(FORESIGHT_EVENT, &status) {
            log::warn!("could not tell the webview about meetings: {err}");
        }
    }

    words_while_unlocked(app).await;
}

#[tauri::command]
pub fn foresight_status(state: tauri::State<'_, ForesightState>) -> ForesightStatus {
    state.status()
}

/// "Did you bring it up?" answered for a meeting — yes or not yet. The
/// yes itself is sent by the webview (`POST /intentions/{id}/fulfil`); this
/// only records that the question has been dealt with.
#[tauri::command]
pub fn foresight_answered(state: tauri::State<'_, ForesightState>, key: String) {
    let memory = {
        let mut inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.memory.answered.insert(key, Utc::now());
        inner.memory.clone()
    };
    state.persist(&memory);
    state.poke();
}

/// Ask the backend again now — after an intention was ticked off, say.
#[tauri::command]
pub fn foresight_refresh(state: tauri::State<'_, ForesightState>) {
    if let Ok(mut inner) = state.inner.lock() {
        inner.briefs.clear();
    }
    state.poke();
}

#[tauri::command]
pub fn calendar_access() -> Access {
    calendar::access()
}

/// Puts up the system's calendar permission dialog.
#[tauri::command]
pub async fn request_calendar_access(
    state: tauri::State<'_, ForesightState>,
) -> Result<Access, String> {
    let access = tauri::async_runtime::spawn_blocking(calendar::request)
        .await
        .map_err(|err| format!("{err}"))??;
    state.poke();
    Ok(access)
}

/// Opens the calendar pane of System Settings — the only way back once
/// access was refused. A command of its own rather than a URL handed to
/// the opener plugin, whose permissions stop at http, mailto and tel; one
/// fixed destination is a narrower door than widening those.
#[tauri::command]
pub fn open_calendar_privacy() -> Result<(), String> {
    std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars")
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("could not open System Settings: {err}"))
}

#[tauri::command]
pub async fn list_calendars() -> Vec<calendar::CalendarInfo> {
    tauri::async_runtime::spawn_blocking(calendar::calendars)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 24, h, m, 0).unwrap()
    }

    fn meeting(key: &str, start: DateTime<Utc>, minutes: i64) -> Meeting {
        Meeting {
            key: key.into(),
            title: "Jour fixe Paul".into(),
            starts_at: start,
            ends_at: start + chrono::Duration::minutes(minutes),
            people: vec!["Paul Hartmann".into()],
            calendar_id: "work".into(),
        }
    }

    fn briefed(key: &str, known: usize, intentions: usize) -> HashMap<String, Briefed> {
        HashMap::from([(
            key.to_string(),
            Briefed {
                known,
                intentions: (0..intentions)
                    .map(|n| Uuid::from_u128(n as u128 + 1))
                    .collect(),
                at: at(0, 0),
            },
        )])
    }

    const LEAD: chrono::Duration = chrono::Duration::minutes(10);

    #[test]
    fn phases_run_from_later_to_gone() {
        let m = meeting("m", at(10, 0), 30);
        assert_eq!(phase(&m, at(9, 49), LEAD), Phase::Later);
        assert_eq!(phase(&m, at(9, 50), LEAD), Phase::Ahead);
        assert_eq!(phase(&m, at(10, 0), LEAD), Phase::Now);
        assert_eq!(phase(&m, at(10, 30), LEAD), Phase::After);
        assert_eq!(phase(&m, at(22, 30), LEAD), Phase::Gone);
    }

    #[test]
    fn an_open_intention_sparkles_and_announces_once() {
        let m = [meeting("m", at(10, 0), 30)];
        let briefs = briefed("m", 1, 1);
        let (views, attention, announcements) =
            plan(&m, &briefs, &Memory::default(), at(9, 52), LEAD);
        assert_eq!(views.len(), 1);
        assert_eq!(attention.as_deref(), Some("m"));
        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0].title, "In 8 min: Jour fixe Paul");

        let mut memory = Memory::default();
        memory
            .announced
            .insert(announcements[0].id.clone(), at(9, 52));
        let (_, attention, again) = plan(&m, &briefs, &memory, at(9, 55), LEAD);
        assert!(again.is_empty(), "announced twice: {again:?}");
        // Still sparkling — the banner is once, the signal is until it is done.
        assert_eq!(attention.as_deref(), Some("m"));
        // And running counts as the same announcement, not a second one.
        let (_, _, during) = plan(&m, &briefs, &memory, at(10, 5), LEAD);
        assert!(during.is_empty());
    }

    #[test]
    fn known_people_without_an_intention_are_listed_but_do_not_sparkle() {
        let m = [meeting("m", at(10, 0), 30)];
        let (views, attention, announcements) =
            plan(&m, &briefed("m", 2, 0), &Memory::default(), at(9, 55), LEAD);
        assert_eq!(views.len(), 1);
        assert_eq!(attention, None);
        assert!(announcements.is_empty());
    }

    #[test]
    fn a_meeting_about_nothing_known_is_not_listed() {
        let m = [meeting("m", at(10, 0), 30)];
        let (views, _, _) = plan(&m, &briefed("m", 0, 0), &Memory::default(), at(9, 55), LEAD);
        assert!(views.is_empty());
    }

    #[test]
    fn afterwards_it_asks_until_answered() {
        let m = [meeting("m", at(10, 0), 30)];
        let briefs = briefed("m", 1, 1);
        let (views, attention, announcements) =
            plan(&m, &briefs, &Memory::default(), at(10, 40), LEAD);
        assert_eq!(views[0].phase, Phase::After);
        assert_eq!(attention.as_deref(), Some("m"));
        assert_eq!(announcements[0].title, "Jour fixe Paul is over");
        assert_eq!(announcements[0].body, "Did you bring it up?");

        let mut memory = Memory::default();
        memory.answered.insert("m".into(), at(10, 41));
        let (views, attention, announcements) = plan(&m, &briefs, &memory, at(10, 45), LEAD);
        assert!(views.is_empty());
        assert_eq!(attention, None);
        assert!(announcements.is_empty());
    }

    #[test]
    fn done_elsewhere_means_nothing_left_to_ask() {
        // The intention was ticked off: the brief comes back without it.
        let m = [meeting("m", at(10, 0), 30)];
        let (views, attention, _) = plan(
            &m,
            &briefed("m", 1, 0),
            &Memory::default(),
            at(10, 40),
            LEAD,
        );
        assert!(views.is_empty());
        assert_eq!(attention, None);
    }

    #[test]
    fn the_sparkle_goes_to_what_is_coming_before_what_is_over() {
        let over = meeting("over", at(9, 0), 30);
        let next = meeting("next", at(10, 0), 30);
        let mut briefs = briefed("over", 1, 1);
        briefs.extend(briefed("next", 1, 1));
        let (_, attention, _) = plan(&[over, next], &briefs, &Memory::default(), at(9, 55), LEAD);
        assert_eq!(attention.as_deref(), Some("next"));
    }

    #[test]
    fn old_bookkeeping_is_forgotten() {
        let mut memory = Memory::default();
        memory
            .announced
            .insert("old#ahead".into(), at(0, 0) - chrono::Duration::days(4));
        memory.answered.insert("new".into(), at(0, 0));
        memory.forget_before(at(0, 0) - FORGET_AFTER);
        assert!(memory.announced.is_empty());
        assert_eq!(memory.answered.len(), 1);
    }
}
