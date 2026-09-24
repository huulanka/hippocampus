# ADR 0017: An answer is written from chosen notes, and checked sentence by sentence

## Status
Accepted (2026-09-24). Replaces the "Chat or RAG over the captures" line
in `docs/product.md`'s out-of-scope list with P15.

## Context
The search field finds notes. The question behind a search is often one a
note does not answer on its own: "when does the harbour portal go live?"
is spread over three notes, the latest of which contradicts the first.
Chat was excluded because a model that talks about your life will, now
and then, invent part of it, and an invented memory cannot be told from a
real one.

What an answer can be trusted for depends on three things: what the model
was allowed to read, whether each claim can be traced to a note, and
whether anyone checked that the note says it.

## Decision
- **The backend chooses what may be read** (`backend/src/ask.rs`). A
  planning call turns the question into search words, named things and an
  optional time range. The notes are then collected in priority order and
  capped at thirty: the hybrid search hits, the latest notes about each
  named entity, notes that belonged to a meeting with them, the episode
  neighbours of the best hits, and the latest notes about the entities
  most often related to them. The model gets those notes, each with its
  date and what it was said next to, and the gists of the named entities
  marked as orientation that must not be cited. It gets no tool and no
  other access.
- **Every sentence cites tags**, which are turned back into note ids; a
  sentence whose tags were never shown, or that cites none, is dropped
  (`routes::review::sourced`, shared with the weekly story).
- **A second call reads each remaining sentence against exactly the notes
  it cites** and names the ones they support. The rest are dropped; the
  answer reports how many.
- **No sentences is a valid answer**, returned with the nearest notes.
- **Nothing is stored.** No table, no event, no question in the log.
- Three calls per question (plan, answer, check), on the structuring
  model, with zero data retention.

## Consequences
- An answer takes a few seconds, most of it the three calls.
- The check is a model too, and can wave through a sentence it should
  not. The notes are therefore always shown beneath the answer, numbered
  like the sentences, and "Quotes only" removes the model's words
  altogether.
- On a small archive the search returns nearly every note, so the thirty
  are mostly everything; the cap starts to matter at a few hundred.
- Without a model configured, the endpoint returns the closest notes and
  says it cannot answer.
