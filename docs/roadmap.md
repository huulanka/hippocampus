# Roadmap

An order, not dates. Each phase has a criterion for going on or stopping.

## Phase 0: make it trustworthy (prerequisite). Done

Nothing personal goes into the system without this.

- The backend verifies the Cloudflare Access JWT.
- CORS is limited to an explicit list of origins.
- Backup **and a tested restore**, run through once from start to finish
  (`docs/operations.md`).

*Go on when:* a restore from backup has demonstrably worked.

## Phase 1: capture and echo (the actual MVP). Done

- Audio is recorded and kept as the original (ADR 0004); the event model
  records `capture.recorded` without text and `transcript.derived` next
  to it.
- Local transcription in the Tauri client, global shortcut, push-to-talk.
- A local outbox: capturing works offline and syncs in the background,
  with retry. Structuring and indexing are retried the same way on the
  backend. From here on, once a capture is stored, storing it succeeded;
  everything after that is a delay, not a failure.
- `GET /captures/{id}/echo`, shown straight after capturing.
- Search uses Reciprocal Rank Fusion instead of adding weighted scores, and
  has time filters.

*Go on when:* a week of voluntary daily use without reminders.
*Otherwise:* find out why before building anything else.

## Phase 2: trust in everyday use

- Transcript correction (ADR 0005), including re-embedding and
  re-extraction. **Done.**
- Redaction with a tombstone. **Done.**
- Re-derivation: re-structure the whole archive. This is where event
  sourcing earns its keep. **Open.**
- A JSONL mirror of the raw data, so it outlives this program. **Open.**

## Phase 3: the graph earns its place

Brought forward: the automatically created connections need consolidating
after the fact. That means spotting duplicates, merging the same subject
under different spellings, and filing edges under the right word. Scope,
triggers and order are in [`docs/consolidation.md`](consolidation.md).

The volume threshold is still right as an argument about *value*, but it
is not a ban on building: the mechanics are meant for hundreds of entities
and get tested on dozens along the way.

- Entity resolution with a candidate search (`pg_trgm` and embeddings).
  **Done.**
- A review surface for merges and corrections. **Done**, as a preview
  with a checkbox per proposal.
- Relations across capture boundaries. **Done.**
- Settle `entities.current_summary`: remove it or consolidate for real.
  **Open.**

*Go on when:* the graph has at least once shown something echo would not
have.

## Phase 4: reaching further

- iPhone: an app of its own, Tauri on iOS, signed for free. Scope and
  phases in [`docs/iphone.md`](iphone.md); structuring on the devices is
  proposed in ADR 0014.
- Weekly review, tied to a review appointment you set anyway. **Done.**
- Remembering ahead: intentions that hang on people and subjects, and a
  context echo before meetings. **Done**; scope in
  [`docs/prospective-memory.md`](prospective-memory.md), architecture in
  ADR 0013. Next in line: the iPhone app, then "how my thinking changes
  over time".
- Revisit chat, MCP and documents, each with the question of whether it
  solves a problem that by then actually exists.

## Smaller open items

- **Re-evaluate the search cut-off.** On a small archive the semantic
  retriever returns every capture, so a tail of noise follows the real
  matches. The ranking is right, only the length is not. Deliberately not
  solved yet: a threshold calibrated on a handful of captures is
  overfitting, and at a few hundred captures the candidate depth filters
  on its own. Measure again on real volume.
- **Resurface "a year ago today"**, once the archive is old enough for it
  to mean anything.
- **Querying by time** ("what is on this week"). The index for it exists.
