# The interface

This document records **where every function ended up** and **why the
interface looks the way it does**. Unlike `docs/product.md`, it doesn't
come before the implementation; it writes down what was decided, so it can
be checked rather than taken on trust.

## Why it changed

The old look was a costume: one monospace font for everything, scanlines
over every surface, controls drawn in ASCII (`[ New Capture ]`,
`[x] depth`). Three problems prompted the rebuild.

1. **The hierarchy was upside down.** According to ADR 0004, what you said
   is the original. It rendered at 13px, muted and monospace, less
   noticeable than the gold labels around it.
2. **Nothing could be reached by keyboard.** Every control was a
   `<span onClick>`. Tab skipped over all of them.
3. **The graph was a second app.** Its own background, its own scanlines;
   the CSS literally said it committed to a visual world of its own. That
   is exactly why it didn't fit in.

## The places

Eight tabs became five, plus a button.

| Place | The question it answers |
|---|---|
| **Capture** (a button, not a place) | "Something just occurred to me." |
| **Today** | "What's on right now?" The only place that speaks first |
| **Search** | "Where was that again?" Empty, it shows everything, by day |
| **Write** | "I'm sitting in a four-hour workshop." |
| **Graph** | "What does this connect to?" |
| **Settings** | Everything that isn't a note |

**The weekly review isn't a place either.** It's a page you arrive at,
through "Look back on the week" on Today, the tray menu or the one
notification a week, and it closes like a detail view back to where you
were. A sixth tab for something opened once a week would be the sidebar
starting to grow again.

**Capture isn't a place.** The shortcut is the record button (ADR 0012),
so capturing opens over wherever you were and closes back to it. As a tab,
the action you take twenty times a day was also the only one that threw
away where you were. The button at the top of the sidebar is visible proof
that the shortcut exists, and the way in on the phone, in the browser
build, and when the key combination is taken.

## Inventory: where every function went

Nothing may disappear without an entry here. If something is missing, that
is a bug, not a decision.

| Before | Now | Note |
|---|---|---|
| Sidebar → `[ New Capture ]` | Capture button at the top of the sidebar | |
| Sidebar → pending work | unchanged, in the sidebar | |
| Sidebar → `[ lock now ]` | bottom of the sidebar; in Settings on the phone | |
| Capture: record, type, discard, save | unchanged, as an overlay | |
| Capture: echo straight afterwards | unchanged | |
| Resurface: "coming up" | **Today**, first section | |
| Resurface: "you keep coming back to this" | **Today**, second section | |
| Timeline: everything by day | **Search**, empty state | it was the same question a third time |
| Search: query, type filter, results | **Search**, unchanged | |
| Relations: graph, search, filter, merge | **Graph**: Map to start, click → Orbit | see "The graph" |
| Tidying: preview, proposals, apply | **Graph**, drawer on the right | it's about the graph |
| Tidying: log and undo | **Graph**, same drawer | |
| Chat | **removed** | it sat in the main navigation as "soon" |
| Capture detail (all of it) | same scope, redesigned | real buttons, audio as a range input |
| Entity detail (all of it) | same scope, redesigned | relations grouped, a timeline |
| Settings (all of it) | unchanged | |
| (nothing) | **Write**, new | see below |

## Write: a session

The case nothing covered before: a four-hour workshop, writing along the
whole time, sending once at the end. Two decisions carry the screen, and
both are visible on it.

**It stays several notes, not one.** A blank line separates blocks; each
block becomes its own capture. Four hours as one wall of text would drown
echo, which compares whole captures, and give extraction nothing to hold
on to.

**Each note keeps the time it was written**, not the time it was sent.
The times sit in the gutter while you type, so that it's a visible fact
and not a claim. If everything said 14:00, the timeline would lie about
when you thought something, and the timeline is most of what this system
is for.

The draft is kept on disk through `client/src-tauri/src/draft.rs`,
deliberately **not** in the outbox: the outbox holds finished captures that
can no longer change, while this is a document that changes with every
keystroke. It is written through a temporary file and a rename, so an
interrupted write leaves the previous draft untouched.

A session is **not an entity** and doesn't appear in the graph. Extraction
finds entities in what was *said*; a heading typed into a text field was
not said. Turning it into a node would mean the interface writing a guess
into the graph, which is the line P12 draws. The title lives in
`capture_sessions`, where it can be changed and deleted; only the ID goes
into the event payload (ADR 0005: the log is never changed).

## The foundation

`client/src/styles/tokens.css` is the only file allowed to name a colour,
a size or a duration. Before, the same view had twelve font sizes between
10 and 14px, not because anyone wanted that, but because there was no
place where the right value was written down.

The layers (`@layer tokens, base, components, screens`) are the second
part: a screen always beats a component, a component always beats the
base, and nobody has to out-specify anybody else. The old stylesheet
carried a `:not(.graph-ring)` around just so a general rule wouldn't paint
over a specific one.

### Three typefaces, three jobs

| Typeface | For |
|---|---|
| **Fraunces** | Your words, and the names of things |
| **Instrument Sans** | Everything the interface says |
| **JetBrains Mono** | Machine truth: timestamps, numbers, IDs |

Monospace on running text was why every screen had the same flat texture.
The fonts ship in the bundle (206 KB), not from a CDN: an app whose audio
never leaves the Mac shouldn't fetch a font on launch.

### Four accents, one job each

| Colour | Meaning |
|---|---|
| **Clay** | Actions, and what is in focus |
| **Ember** | Time: when something was said, when it's due |
| **Moss** | Derived by the machine, never your own words |
| **Plum** | People, and the fourth entity hue |

Entity colours come from these four, through an FNV-1a hash of the type
name. Before, the colour depended on the order in which the running
session first met each type: a different colour after every restart,
while the comment at the top of `entityType.ts` claimed the opposite.
That made colour the one thing on screen you couldn't learn.

### Controls

Real `<button>`, `<input>`, `<label>`, `<input type="checkbox">`. A 2px
Ember ring on `:focus-visible`, set off by the background colour.

A chip that *narrows* a list isn't selected, just quiet. A chip that
*hides* part of a picture (the type filters in the graph) is struck
through: the picture in front of you is missing something, and you need
to be able to see what.

## The mark

A brain in side profile, facing left. Both predecessors were built from a
mirrored half and came to a point at the bottom centre, which is how a
heart glyph is constructed, and both read like one. A brain is recognised
by its profile: asymmetric, twelve bulges along the outline (the gyri *are*
the silhouette, which is why they survive being scaled down), one long
sulcus, a cerebellum at the back and a stem off-centre.

The geometry lives in `client/src/brand/mark.json` and nowhere else.
`scripts/render-brand.py` renders the app icon, the `.icns` and the menu
bar template from it, so the window and the menu bar can no longer drift
apart.

The mark never moves on its own. It doesn't blink or bob; it only shows
when something is really happening: `listening` while the microphone is
open, `thinking` while the backend is working on something. In the menu
bar it stays completely still and a dot next to it breathes; the template
reserves the space for it, so the item never changes width.

## On the phone

Below 720px the app follows the phone's own apps rather than shrinking the
Mac window. All of it lives in `client/src/styles/phone.css` and
`client/src/phone.ts`.

- **Content runs edge to edge.** It scrolls under the Dynamic Island,
  where it fades and blurs out instead of running into the clock, and
  behind the tab bar, which leaves room for the last line to be scrolled
  clear of it.
- **The chrome floats, as glass.** The tab bar is a capsule of the five
  places, the back button a round glass button in the top corner that
  stays put while the page scrolls. A bar you can see through reads as
  above the page; a solid strip reads as part of it.
- **Capturing is a round button of its own**, beside the tab bar rather
  than in it: the one control a thumb finds without looking, shaped like
  every record button, and impossible to mistake for a sixth place.
- **It opens as a sheet** that rises over the page you were on, so closing
  it is visibly a return. Cancel or pulling the handle down closes it;
  tapping the dimmed page above does not, because a half-written note is
  the one thing here that cannot be got back.
- **Pages move like a navigation stack.** A detail slides in from the
  right, and pulling from the left edge takes it back, following the
  finger so the gesture can be abandoned halfway.
- **Type goes up a step**, towards the system's 17pt body; the tokens
  change, nothing else does. Every target is at least 44pt, and no text
  field is under 16px, below which iOS zooms the page on focus.
- **Settings is a grouped list**: rounded cards of rows, on-and-off as
  switches, text fields full width under their name. What only a Mac has
  (the shortcut, the login item, the weekly notification, the calendar,
  the log folder) is not shown at all.
- **Hover does nothing.** A finger has no hover, and on a phone it would
  stick to whatever was tapped last.

## The graph

**It opens on the Map, not on an Orbit.** The first version opened
centred on whatever was discussed last. The top then said only "Lena",
which read as though the graph was already filtered to one person before
you had touched anything. Now "Everything" is the first step of every path
and the way back; clicking a dot or a name on the Map opens its Orbit.

**Map:** every kind with at least two entries gets its own disc, and the
one-offs share "One-off kinds". The discs are packed against each other
(the largest in the middle, each further one in the free spot closest to
the middle) and the whole thing is scaled to the window. Names appear only
on what was said more than once, and only where they cover nothing. That
is a real collision check, not offsets.

**Orbit:** both rings always fit (`ringRadii` in `orbit.ts`), names point
radially outwards, and relations find a free spot above or below their
spoke. Two relations to the same neighbour make one node and one line, not
two.

## Still open

- The phone layout has been looked at in a phone-sized WebKit, not yet on
  a phone.
- The one-off kinds are a symptom of the type vocabulary coming out of
  extraction; consolidation (`docs/consolidation.md`) is the place to fix
  that, not the interface.
