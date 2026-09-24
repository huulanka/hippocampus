# Product scope

This document says *why* things get built. It does not outrank the ADRs or
the code; it comes before them. Architecture lives in `docs/adr/`, the data
model in `docs/memory-model.md`, the order of work in `docs/roadmap.md`.

## Vision

A personal memory system that takes in thoughts with as little friction as
possible, keeps them unchanged, and brings them back at the moment they
matter, without you having to remember to ask.

## The problem

Thoughts come up all the time and get lost all the time. Note apps don't
fix that, because they carry three costs that together kill any habit:

1. **Capture cost**: open the app, pick a place, type, name it, file it.
2. **Filing cost**: knowing where something belongs at the moment you
   write it down.
3. **Recall cost**: remembering that you wrote it down at all.

The third one is what actually kills it. An archive that only answers when
asked is no help at exactly the moment you need it most.

## Who it is for

First of all, one person: someone who works at a Mac most of the day,
thinks in projects, people and recurring subjects, would rather speak than
type, wants to own their data and doesn't want running costs.

More generally: people with a lot of thoughts and little filing
discipline, who gave up on existing PKM tools (Obsidian, Notion) not
because of missing features but because of the upkeep.

## Core loop

```
SPEAK  →  ECHO  →  (now and then) SEARCH  →  (later) GRAPH
```

**Speak.** Shortcut, talk, stop. No category, no title, no save dialog.
Capturing always succeeds, even when the backend can't be reached.

**Echo.** Right after a capture, the system shows two or three earlier
captures that are close in meaning. Verbatim, with their date, never
summarised.

Echo is the core. It needs no new habit, because it happens inside the loop
you are already in. It is useful from the twentieth capture, not the
two-thousandth. And it cannot hallucinate, because it only ever shows your
own words.

**Search.** A deliberate query across all captures, hybrid (full text and
semantic), with time filters.

**Graph.** People, projects, subjects and how they relate. A stated
long-term goal, but *downstream*: it needs volume and working entity
resolution before it can show anything worthwhile. It has to earn its
place.

## Product decisions

| # | Decision | Why |
|---|---|---|
| P1 | Connection is the core of the product, not just finding things again | A searchable voice diary would count as a failure |
| P2 | Echo before graph | Echo is connection without any entity prerequisites |
| P3 | The capture path has absolute priority | Without real, messy speech data nothing downstream can be judged |
| P4 | Mac first, iPhone later | The Mac is where most of the day happens; the phone covers being out |
| P5 | Plan for 10–20 captures a day | About 5,000 a year, which makes entity fragmentation a real problem, not a theoretical one |
| P6 | The audio is the original, the transcript an interpretation | See ADR 0004 |
| P7 | Corrections are events, the original stays | See ADR 0005 |
| P8 | Redaction with a tombstone | See ADR 0005 |
| P9 | Merges follow the weight of evidence, not caution alone | What can be decided without weighing anything up (same name, inconsistent type) is merged automatically and stays reversible through an event. What needs judgement waits until there is something to read; see P12 |
| P10 | Structuring goes through OpenRouter with zero data retention, knowingly | The NAS (a Celeron, no GPU) can't run an LLM; the event log allows moving to a local model later |
| P11 | No morning brief, no push notifications, with **two** exceptions: the weekly review announces itself once a week at a day and time you set, and a banner appears before a meeting that an open intention fits (once before, once after). Both can be switched off | Every notification is a habit that has to be built on purpose. Both exceptions hang on an appointment you already have instead of inventing a new one. Neither says anything about what is in the notes; the second only names the meeting from your own calendar. Reading is behind the lock, a notification banner is not. See `docs/prospective-memory.md` |
| P12 | The threshold for automatic judgements is **observations per entity**, not the total number of captures | Early on, almost every entity had exactly one observation. A model deciding from one sentence per side whether two things are the same is guessing, and would write its guess into the graph automatically |
| P13 | Intentions ("I still need to ask Paul") are picked up in passing and hang on people and subjects, not on times; they come back the next time the subject comes up and before a matching meeting | The moment knowledge matters belongs to the world, not to the app. Detection is a model's judgement, so it is shown straight away and can be dismissed in one move; only you mark something as done (`docs/prospective-memory.md`, F1–F12) |

## What counts as an MVP

The MVP is reached when the system gets used **every day for a week,
voluntarily**, without anyone having to remind you.

1. A global shortcut on the Mac, push-to-talk, local transcription.
2. Audio is kept permanently; the transcript is derived from it.
3. A local queue: capturing works offline, syncing happens in the
   background.
4. Echo: two or three close earlier captures right after capturing.
5. Search across all captures, hybrid, with a time filter.
6. A timeline to browse chronologically.
7. Correcting a transcript, with the original kept.
8. The backend is authenticated and reachable from outside.
9. A backup and restore that has actually been run.

Points 8 and 9 are not features. They are the condition for trusting the
system with anything personal at all.

## Out of scope for version 1

| Not building | Why |
|---|---|
| Chat or RAG over the captures | A memory system must not confabulate about your own past; you cannot tell an invented memory from a real one. Dated quotes with a link to the original beat any summary. |
| A cryptographic hash chain | There is no threat model for it. Anyone with write access to the database rewrites the chain too. Without external anchoring it is theatre. A tested restore is worth more. |
| A model that **weighs up** which entities are the same, unsupervised | A process that invents relationships in your memory without supervision is a hallucination generator with write access. The threshold is P12, the weight of evidence per entity. *Deterministic* consolidation (same name, inconsistent type vocabulary) is **not** out of scope, because it weighs nothing and so cannot invent anything. |
| Documents, PDFs, email, screenshots | A different product (document extraction). It doesn't solve the problem above. |
| Dedicated hardware (ESP32 and the like) | The Mac already is the ambient device. |
| An MCP connection to ChatGPT | Only once the archive has substance. |
| Decay or forgetting as stored state | Nothing gets deleted from a personal archive. At most a ranking factor. |
| Modelling confidence or belief over time | Tempting, but impossible to calibrate without data. Revisit after phase 3. |

## Non-functional requirements

- **Capturing never fails.** Local persistence before the network. A
  single lost thought costs trust for good.
- **Capturing is ready in under two seconds**, from shortcut to recording.
- **You own the data.** Raw data (audio and transcript) leaves your own
  hardware only for structuring, as text, to zero-retention providers.
- **No running costs above roughly €5 a month.** Realistic with a cheap
  model at 5,000 extractions a year.
- **Formats that last.** The raw data has to stay readable without this
  program: audio as a standard format on disk, captures additionally as a
  JSONL mirror.
- **Authentication** as soon as the backend leaves the local network.

## Open questions

- At what volume does the graph pay off? For *consolidation*, P12 answers
  this: what counts is how much is known about an entity, not how much has
  been captured overall.
- How many echo matches are too many? Three to start with, the threshold
  found empirically.
- Does the iPhone need its own app, or is a Shortcut with Apple dictation
  enough? **Its own app**; keyboard dictation gets too much wrong with fast
  speech. See `docs/iphone.md`.
- Does `current_summary` stay a stored field or is it computed when read?
  See `docs/memory-model.md`.
