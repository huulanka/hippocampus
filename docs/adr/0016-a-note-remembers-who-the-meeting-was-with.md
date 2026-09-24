# ADR 0016: A note remembers who the meeting was with, not the meeting

## Status
Accepted (2026-09-24). Amends ADR 0013: the backend now keeps something
from the calendar, but only what the graph already knew.

## Context
`docs/finding-again.md` gives a note the meeting it was spoken around, so
that "what did I say in the meeting with Paul" becomes answerable. ADR
0013 decided that the calendar stays on the Mac and that `POST /brief`
stores nothing. Keeping a meeting with each note contradicts that unless
what is kept is not the meeting.

Three ways were on the table:

1. **Keep the title and attendees with the note.** Most to search by
   ("the Jour fixe"), but the calendar becomes part of the memory, and a
   work meeting's title would sit on the NAS next to every note — the
   mixing ADR 0013 was written to prevent.
2. **Keep the link on the Mac.** Nothing leaves the Mac, but the backend,
   which answers questions, could never use it.
3. **Keep only what matched.** The Mac matches each meeting against the
   graph exactly as `POST /brief` does, and the backend keeps the matched
   entities, the phase and the distance in minutes.

## Decision
**Option 3.**

- **The Macs look notes up; the backend never reads a calendar.**
  `client/src-tauri/src/occasions.rs` asks `GET /occasions/pending` which
  notes this installation has not looked up, reads the ticked calendars
  around them, and sends the meetings within the window to
  `POST /occasions`. Nothing is sent until a calendar is ticked, so ticking
  one later still finds the meetings around the notes from before.
- **Matched, then dropped.** The backend matches title and attendees with
  the brief's whole-word matching (`brief::find`) and keeps
  `capture_occasions` (phase, minutes, which Mac offered it) and
  `capture_occasion_entities`. A meeting that matches nothing known is not
  kept at all. The `occasion.offered` event carries ids only.
- **One look per note per Mac**, recorded in `capture_occasion_checked`
  whether a meeting was found or not. Every Mac is asked about every note,
  including the phone's: each answers for its own calendars. The id that
  names an installation is random and says nothing about the Mac.
- **A reading decides.** An offered meeting is shown on a note only after
  the context judge (`backend/src/context.rs`) has read the note against
  it and found it belongs. Until then, and when it does not, it is
  invisible.

## Consequences
- "The Jour fixe" cannot be asked for by name; "the meeting with Paul"
  can. A meeting nobody known was invited to, with a title that names
  nothing known, leaves no trace. Accepted, for the same reason ADR 0013
  accepted it.
- Title and attendee names reach your own backend once per note and
  meeting, as with `POST /brief`, and go no further: the judge sees the
  matched entity names, never the title.
- A calendar ticked after a note was looked up does not look it up again.
  Rarely matters; the same note was usually looked up by the Mac it was
  spoken on.
- Redacting a note removes its occasions; correcting it puts its occasions
  back in front of the judge.
