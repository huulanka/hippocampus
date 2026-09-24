# ADR 0005: Corrections and redactions without breaking immutability

## Status
Accepted (2026-09-20)

## Context
Two real requirements collide with "nothing is ever changed":

1. **Correction.** A transcript is wrong ("Hafen Portal" instead of
   "HPortal"). Left as it is, the memory is as good as lost, because
   searching for the right word doesn't find it.
2. **Redaction.** Sooner or later something gets captured that you don't
   want kept for good, or that you can't justify keeping when other people
   appear in it.

At the same time, `events` is protected against UPDATE and DELETE by a
`RULE ... DO INSTEAD NOTHING` (ADR 0003). Content in the event log would
therefore be irrevocable. The lock meant to create trust would turn the
system into a place you can't trust with anything delicate.

## Decision

**Separate structure from content.** The event log holds only *what
happened when, done by whom or by which model*, plus references. Content
(audio paths, transcript text) lives in its own tables, referenced by event
ID and writable as usual.

**Correction** is additive: `transcript.corrected` creates a new version
and points at the one it replaces. All versions stay readable and can be
seen side by side in the interface. Search, embeddings and structuring work
on the current version; a correction recomputes the embedding and the
extraction for that capture.

**Redaction** is destructive but recorded: the content in `capture_content`
and `transcript_content` is emptied, derived observations and relations are
removed, and `capture.redacted` is appended. What remains is a tombstone:
the time, the device, and the fact that something was here and was
deliberately removed.

## Consequences
- "The event log is never changed" stays literally true.
- Redaction does **not** reach into existing backups. That is accepted on
  purpose; removing something completely also means discarding old
  backups.
- Corrections cause follow-up work (re-embedding, re-extraction). At the
  expected frequency that is not a concern.
- `capture_search` mirrors the current version, not the first.
- Transcript corrections and entity merges are reviewed in the same way:
  in both cases the system is unsure, a person decides, and the decision is
  recorded as an event.
