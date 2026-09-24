# ADR 0013: The calendar stays on the Mac; the backend only matches

## Status
Accepted (2026-09-23)

## Context
"Zukunft erinnern" (docs/prospective-memory.md) brings an intention back
shortly before a meeting with the people it is about. That needs two
things that live in different places: the calendar, which is on each Mac,
and the entities, aliases and merges, which are in the backend.

The user has two Macs — a private one (Apple Calendar, iCloud) and a work
one (Outlook, with the Exchange account also in Apple Calendar). A work
meeting must never surface on the private machine.

Three ways to join the two were on the table:

1. **Sync the calendar to the backend.** One place to match, one place to
   run the timer. But the calendar would become part of the memory — every
   meeting title, every attendee, stored on the NAS next to the notes —
   and both Macs would feed the same store, which is exactly the mixing
   the user ruled out.
2. **Pull the whole graph to the client and match there.** Keeps the
   calendar local, but the client would need entities, aliases and merge
   pointers, kept in step with the backend, only to answer "is Paul in
   this title".
3. **Ask the backend once per meeting.** The client reads its own ticked
   calendars, and for a meeting inside the lead sends the title and the
   attendee names to `POST /brief`. The backend matches them against the
   graph, answers with what it knows, and keeps nothing.

## Decision
**Option 3.**

- **EventKit on the client, read-only, per Mac.** `calendar.rs` reads only
  the calendars ticked in this Mac's Settings (`watched_calendars` in its
  own `settings.json`). Nothing is ticked by default. The two decisions —
  may the app read the calendar, and which calendars — are separate.
- **`POST /brief` stores nothing.** No table, no event, no log line with
  the title. The meeting is matched and forgotten.
- **Matching is whole words, not similarity** (`backend/src/brief.rs`):
  a name or alias appearing as a run of whole words in the title or in an
  attendee's name, case and accents folded, with a short list of meeting
  words ("Jour fixe", "Review", "Team") that never match on their own. The
  answer drives a signal in the menu bar, and a signal has to be
  explainable in one line — *because "Paul" is an attendee* — or it will
  stop being trusted.
- **The menu bar knows *that*, never *what*.** The Rust watcher
  (`foresight.rs`) keeps the meeting, a count and the ids of open
  intentions — nothing it could show on a locked screen. The words are
  fetched by the webview through the gated request path (ADR 0011).
- **The lit icon is not a template.** It is the only coloured state of the
  tray icon (clay brain, ember sparkles), in two palettes chosen by the
  system appearance, because macOS would tint a template back to
  monochrome and monochrome is what "nothing to say" already looks like.

## Consequences
- The backend has to be reachable for a meeting to light up. If it is
  not, `ForesightStatus.error` says so, and nothing sparkles — a missed
  sparkle, never a wrong one.
- A meeting is only as matchable as its title and attendees. "Abstimmung
  Q3" with nobody invited matches nothing. Accepted: a fuzzy match would
  light the bar for meetings it has no business lighting it for.
- Title and attendee names reach your own NAS in the request body.
  They are not written anywhere there, and do not reach OpenRouter —
  matching involves no model call.
- Calendar permission, like the microphone and Touch ID, only exists for a
  bundled app. `Info.plist` must carry `NSCalendarsFullAccessUsageDescription`;
  without it macOS terminates the app instead of asking.
