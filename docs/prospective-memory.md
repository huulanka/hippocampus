# Remembering ahead: prospective memory

Like `docs/product.md`, this document says *why* and *what*; the *how* for
the calendar is in `docs/adr/0013-the-calendar-stays-on-the-mac.md`.

## The problem it solves

`docs/product.md` promises to bring thoughts back "at the moment they
matter, without you having to remember to ask". Until now there were
exactly three such moments, and all three belonged to the app:

1. You speak → echo.
2. You open the window → Today.
3. Friday, 16:00 → weekly review.

But the moment knowledge actually matters belongs to the world: in ten
minutes you have your regular meeting with Paul, and three weeks ago you
said you wanted to ask him about the Hafenportal deadline. Until now
nobody knew that.

There was a second gap as well: Hippocampus knew nothing about
**intentions**. Sentences like *"next time I talk to Paul, I'll ask him
about X"* have no date, so they never reached `upcoming` either. They hang
on a **person or a subject**, not on a time. Nothing handles that:
reminder apps know time and place, note apps know nothing at all.

## What it does

An intention is something you want to **do, say or ask later** that isn't
done yet. It's picked up in passing, with no keyword, and hangs on the
entities it's about.

It comes back at two moments:

| Trigger | Where | How |
|---|---|---|
| **Next mention**: you talk about Paul again | The capture sheet, right under the echo | "You meant to …", with *Done* |
| **Calendar**: a meeting whose title or attendees match an entity starts in *n* minutes | The menu bar (coloured, sparkling), optionally a banner, and the brief page for the meeting | Open intentions first, then your most recent sentences about the people involved |

After the meeting, Hippocampus asks once: **"Did you bring it up?"** *Yes*
closes the intention (as a new event, P7); *Not yet* leaves it open and
doesn't ask again for this meeting.

Right after you speak, the capture sheet shows **"Noted for later"** with
an [x] to dismiss it. Detection is a model's judgement, so it will
sometimes be wrong; the [x] is the safeguard, and it can be taken back.

## Decisions

| # | Decision | Why |
|---|---|---|
| F1 | **Picked up in passing**, no keyword | The notes are about whatever is on your mind. A keyword requires remembering to use it, which is exactly what the product is supposed to take off your hands. |
| F2 | **Shown right after speaking**, with an [x] | A judgement you can't see is a judgement you can't correct. The lesson from consolidation (automatic first, then made reviewable) applies here just the same. |
| F3 | **Triggers in version 1: calendar and next mention.** *Not* the active window | Watching the active window needs the Accessibility permission, feels like surveillance and produces the most false alarms. |
| F4 | **Calendars are ticked per Mac**, not guessed from the account | For example a personal Mac with iCloud and a work Mac with Exchange through Apple Calendar. A work meeting must never show up on the personal Mac. Without a tick, no calendar is read. |
| F5 | **The calendar stays on the Mac.** The title and attendee names of an upcoming meeting go once to your own backend, are matched there and are **not stored** | Entities, aliases and merges live in the backend; that's where matching is correct and testable. The meeting itself never becomes part of the memory. See ADR 0013. |
| F6 | **The icon changes clearly**, not just a dot: coloured, with sparkles | The icon is finely drawn; a dot gets lost in everyday use. Recording takes precedence over sparkling. |
| F7 | **The banner is optional** (on by default) and respects Focus and Do Not Disturb | This is the **second exception to P11**. Like the first, it hangs on an appointment you already have. macOS suppresses notifications in Focus mode by itself, as long as they come as system notifications. |
| F8 | **The banner reveals no note content**, only the meeting title, which comes from your own calendar | Reading is behind Touch ID, a banner is not. The menu bar knows *that* something is coming up, not *what*. |
| F9 | **Marked done by asking after the meeting**, not by a model | A model that closes intentions by itself will also close wrong ones, and you won't notice. |
| F10 | **Without an intention, nothing sparkles** | A meeting with known people but no open intention gets the brief page (Today shows it), but no sparkle and no banner. Otherwise it would sparkle before every meeting and the sparkle would become wallpaper. |
| F11 | **The text of an intention lives in content, not in the event payload** | Like the session title (`CaptureSession`): the log is never changed, so only what isn't in it can be redacted. `intention.noted` carries IDs; the wording lives in `intentions`. |
| F12 | **Every intention comes with the verbatim quote**, where one can be found | The short phrasing is the model's; the quote is yours. It's only kept if it really is in the transcript. An invented quote is worse than none. |

## Data model

```
events
  intention.noted      { intention_id, source_event_id, entity_ids }   (model)
  intention.dismissed  { intention_id }                                (you, [x])
  intention.fulfilled  { intention_id, via: "calendar" | "manual" }    (you)
  intention.reopened   { intention_id }                                (you, taking it back)

intentions              projection and content
  id, source_event_id, text, quote, status (open|fulfilled|dismissed),
  model, created_at, resolved_at

intention_entities      what it hangs on
  intention_id, entity_id
```

Merges are followed when reading (`coalesce(merged_into, id)`), not
rewritten when merging. That way an unmerge stays correct by itself.

Redacting a capture deletes its intentions along with its observations and
relations.

## Interface

- **Capture sheet:** under the echo, "Noted for later" (with [x]) and
  "You meant to …" (with *Done*).
- **Today:** the next matching meeting at the top ("In 8 minutes: Jour
  fixe Paul"), then **"Still on your mind"**, all open intentions.
- **Brief page for a meeting:** the meeting, its open intentions large,
  your most recent sentences about each entity involved. After the
  meeting: *Yes / Not yet*.
- **Entity page:** the open intentions that hang on it.
- **Settings → Meetings:** calendar access, calendars with account and
  colour to tick, lead time (5 / 10 / 15 / 30 minutes), banner on or off.
- **Menu bar:** at rest, the template image. Recording, a breathing dot.
  Something coming up, the brain in Clay with Ember sparkles.
- **The menu under the icon:** **both clicks open the native menu**, and it
  shows the meetings first: the lit one at the top, and below it only
  "Something to bring up" while locked, or the quote and who it concerns
  once unlocked; without a meeting, "Still on your mind". A meeting opens
  its brief, "Unlock to See…" asks for Touch ID and unlocks the window
  too, and **"Write a Note…"** opens the capture sheet for typing (without
  the microphone). A custom panel window was built and taken out again:
  on this status item AppKit hands the left click straight to the menu,
  so it never reaches the app. An input field *inside* the menu would need
  access to the `NSMenu`, which Tauri doesn't expose; noted for later.
  The rule from ADR 0012 still holds: no click ever starts a recording.
- **Nothing is written into the meeting description.** With Exchange, a
  change made as organiser sends an update to every attendee, the notes
  would then sit on the company's server, and the calendar stays
  read-only (ADR 0013).

## Deliberately left out

- Creating or changing meetings. Hippocampus only reads the calendar.
- Ticking off intentions by voice ("done that"). See F9.
- Location, the active window, email.
- Repetition ("every time …"). An intention is fulfilled once.

## How it's checked

- Backend: extraction with intentions (parser tests), the quote check,
  matching meetings to entities (a pure function, with tests), routes
  against Postgres (`db-tests`).
- Client: the phases of a meeting (before / running / after / expired) as
  a pure function, icon frames as tests like the dot.
- For real: the calendar permission and the banner only in the bundled
  build, like the microphone and Touch ID (`docs/verification.md`).
