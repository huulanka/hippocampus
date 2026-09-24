# Consolidation and time awareness

Why the graph has to be reconciled after the fact, what the run is allowed
to change, and why it waits to be asked. How candidates are found and
judged is in [`entity-resolution.md`](entity-resolution.md).

## The problem

Every note is structured on its own. That has two consequences you can
see in any archive after a few dozen captures:

- **The type vocabulary is invented afresh for every note.** `Kunde`,
  `Unternehmen` and `Organisation` end up as one type under three names.
  The same thing can exist twice, once as a drink and once as an idea,
  with no edge between the two.
- **Relations only exist within a single extraction**, so only between
  things mentioned in the same sentence. The graph becomes what ADR 0006
  predicted: a scattering of unconnected stars.

Many things are related in ways that only show when several notes are
read together. One note at a time, the model can't see it.

## The wrong cut and the right one

The obvious design is a nightly run over the last 24 hours, then weekly
over seven days, then monthly over thirty, so the bigger picture doesn't
get lost without sending huge prompts every time.

That doesn't solve the problem, it only moves it: every tier reads the
same captures again, and the prompt grows with the history.

**The right cut is not a time window but the affected part of the
graph.** If a night's notes touch "Sauna", the run loads everything about
Sauna, whether from last week or last year, and consolidates that
neighbourhood. The cost then depends on the number of *entities touched*,
not on the length of the history. A time window is just a poor proxy for
that.

### Two triggers, both bounded

1. **The dirty set.** Every entity touched since the last run. Covers
   what just happened.
2. **Staleness.** Additionally the *N* entities that have gone longest
   without consolidation, in turn via `last_consolidated_at`. Covers what
   would otherwise lie untouched forever, and replaces the weekly and
   monthly tiers entirely.

Both have a fixed budget per run, so the run never gets more expensive
however large the archive grows.

### Incrementality comes from summaries

For an entity with two hundred captures to fit in a prompt, each entity
needs a **rolling summary**. Later runs read the neighbours' summaries,
not their raw notes.

The field already exists: `entities.current_summary`. It is currently
overwritten by the latest observation (last write wins), so it is the
newest note rather than a summary, which is why the interface labels it
"most recently observed". The consolidation run is what will turn it into
a real summary, and the label changes with it. This is still open; it
only becomes necessary once an entity no longer fits into a prompt
together with its observations.

## What the run may change

**It changes the view of the notes, never the notes.** The captures and
their transcripts are immutable (ADR 0003, ADR 0005). What the run
rewrites is how they are arranged, and every step of that is an event.
It may create relations, merge entities and unify the type and relation
vocabulary, and the state before must always be recoverable. It should
also be possible to see how the graph developed over time while the
sources stayed the same.

### Versioning comes almost for free

The run writes events, and nothing else:

| Event | Meaning |
| --- | --- |
| `relation.proposed` | a new edge |
| `relation.retracted` | an edge taken back |
| `entity.merged` | two entities are the same; target and source in the payload |
| `entity.unmerged` | a merge taken back |
| `entity.retyped` | the type changed; the old type in the payload |
| `entity.renamed` | the surviving name changed; every earlier name stays an alias |
| `consolidation.ran` | a run, with its scope, model and cost |

The graph is the projection of these events. "The graph as of X" simply
means replaying up to date X. Sources unchanged, view evolving: exactly
the property asked for, without a versioning apparatus of its own.

Entities are never deleted; they point at a target (`merged_into`).
Taking a merge back is then an event, not a restore.

### Why it waits to be asked

Being reversible is not the same as being noticed. A rearrangement made
while nobody was looking is still a surprise the moment it's found. So
`CONSOLIDATION_ENABLED` is off by default, and the run waits to be asked:
the preview shows what it would do without doing any of it, each proposal
can be unticked, and applying carries out exactly that judgement minus
whatever was removed. Everything applied appears in the log in the
Tidying drawer and can be taken back from there with one move.

### Three things the design has to handle

1. **Relations between entities never touched together.** The dirty set
   never finds them. That takes a candidate generator over the similarity
   of entities themselves, not just co-occurrence; see
   `entity-resolution.md`.
2. **Oscillation.** A run merges, you take it back, the next run merges
   again. The run reads the block list and never proposes the same pair
   again.
3. **The first pass over an existing archive** is a big run. With a few
   dozen entities that doesn't matter; with thousands it is a task of its
   own that has to be resumable, the same as re-derivation.

### Cycles, and the right word

Consolidation is not a one-off clean-up but recurring, because later
notes turn things into duplicates that weren't before. Two consequences:

- After a merge, edges have to hang on the surviving node, and the
  relation type has the same vocabulary problem as the entity type:
  `arbeitet_in` and `ist_angestellt_bei` are one edge with two names. The
  relation vocabulary is unified in the same step as the entity types.
- A consolidated graph that looks no different from before has missed its
  purpose. The run has to be thought of together with what it makes
  visible, which is why the graph, the Tidying drawer and the change log
  live in the same place.

## Time awareness

A note like "tomorrow I have an appointment" is meaningless three weeks
later unless "tomorrow" was resolved to a date when it was said. A
sentence such as "I'm meeting a friend on Tuesday" produced an
observation saying "meeting them on Tuesday", and nothing recorded which
Tuesday. In
three weeks that can't be read, and that is exactly when you read it.

The prompt to OpenRouter used to contain only the transcript: no date, no
time, no weekday, no time zone.

### What was decided

**Time context in, resolved date out.**

- Into the prompt: the recording time, weekday and time zone.
- Back from the model: an optional, normalised `when` for each statement
  (an ISO date, with a time if needed) plus its precision (day, week,
  month). "Tomorrow" becomes a real date; "next summer" stays vague and
  says so.
- Stored on the observation, shown on the entity and in the capture
  detail.

Explicitly **not** part of this: reminders, notifications, calendar
matching. That would be a calendar, not a memory. It later became its own
feature with its own rules; see `docs/prospective-memory.md`.

### A side effect that improves the rest

Time context doesn't only help resolve dates. "I was at the sauna this
evening" and "sauna today was great" are recognisably the same evening
once the model knows when both were said, which is exactly the connection
that was missing at first. So time context belongs in the consolidation
prompt as much as in the single-note extraction.

## Order

1. **Time context in the extraction prompt**, with the `when` field.
   Small, independent, improves every new note at once. **Done.**
2. **Resolving at write time**: the extraction sees what already exists.
   **Done** (`entity-resolution.md`, stage 1).
3. **Unifying the type vocabulary**, merging and cross-note relations,
   with a candidate generator and protection against oscillation.
   **Done**, run on request with a preview.
4. **Rolling summary per entity** (`entity.summarised`). **Open**, needed
   once an entity no longer fits in one prompt.
