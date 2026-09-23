# How Hippocampus works

**Core principle:** original knowledge (what was said, and when) and
AI-derived knowledge (what was inferred from it) are stored separately and
are always traceable back to each other. See
[`adr/0003-event-sourcing-without-event-store.md`](adr/0003-event-sourcing-without-event-store.md).

That rule is behind most of what follows. This page covers the parts of the
system you notice from the outside; the decisions themselves, and the
alternatives that were turned down, are in the [ADRs](adr/).

## Architecture

```
Tauri v2 client (Rust)  --HTTPS-->  Axum backend (Rust)  -->  Postgres + pgvector
  on-device ASR                       event store,              (events, projections,
  (transcribe-rs)                     embeddings,                 full-text + vector
                                       OpenRouter for               search)
                                       structuring)
```

The reasoning behind each piece of the stack is in [`adr/`](adr/); what
the system is for, and in what order it gets built, is in
[`product.md`](product.md) and [`roadmap.md`](roadmap.md) (both in German).

## A capture cannot be lost

Once a capture is written down, it is kept — whatever the network, the
backend or the model does next. Two mechanisms, at the two places it used
to be possible to lose one.

**On this Mac: the outbox.** Speaking a note records it, transcribes it
on-device, and writes it to
`~/Library/Application Support/com.andreasbauer.hippocampus/outbox/` as a
JSON file with its WAV beside it — *before* the network is touched. Only
then is it uploaded. A sleeping NAS, a dropped Wi-Fi or an expired Access
token no longer costs a thought that was already spoken; the queue is
retried in the background, and the entry disappears once the backend has
acknowledged it. Typed captures take the same path.

Plain files rather than a database, deliberately: the queue is by
definition the part of the system that exists in exactly one place, so it
has to be readable and recoverable with nothing but the Finder. Writes go
through a temporary file and a rename, so a reader sees either the old
entry or the whole new one.

**On the backend: the pipeline.** A capture is permanent the moment it is
stored, but two things still have to happen — embedding it for search, and
having the model read it. Both used to fail with nothing but a log line.
An unstructured capture looks fine (it is in the timeline, findable by its
words) and simply never has entities, never a resolved date, and never
appears in Resurface. `capture_pipeline` records what has not finished and
a background loop comes back for it, with backoff, a per-pass budget, and
a point at which it gives up and waits for a person.

`GET /pipeline` counts what is waiting and what was given up on;
`POST /pipeline/retry` asks again. The client shows both in a single
sidebar line, which is absent whenever there is nothing to say — a status
line that is permanently green is one nobody reads on the day it turns
red. See `STRUCTURING_RETRY_*` in [`.env.example`](../.env.example).

## Echo

After every capture, Hippocampus shows the earlier captures closest to it —
your own words, never a summary
([`adr/0006-echo-before-graph.md`](adr/0006-echo-before-graph.md)).

Retrieval and judgement are two different jobs. The bi-encoder
(`multilingual-e5-small`, running locally via
[`candle`](https://github.com/huggingface/candle), embeddings already in
the database) finds candidates, because recall is what it is good at. It
turns each text into a vector **without ever seeing the other one**, so it
is blunt about ordering: it measurably ranked *cardamom buns* above
*finnischer Aufguss* for a note about a sauna, and a larger e5 did not fix
it. Something then has to read the new capture and each candidate
together.

That judgement is made once, when the capture is recorded, and written
down — an echo looks only at captures strictly *earlier* than its own, and
those never change, so there is nothing to recompute. It runs behind the
response: saving a capture is confirmed as soon as the capture is safe,
and the echo follows a second or two later. Reading it afterwards is a
single indexed query.

Who judges is `HIPPOCAMPUS_RERANKER`:

| Value | What it does |
| --- | --- |
| `remote` (default) | A small hosted model over OpenRouter, ZDR-routed, same path as structuring. `ECHO_JUDGE_MODEL` picks it; the default is `mistralai/mistral-small-2603`, chosen for German. |
| `bge` | `bge-reranker-v2-m3` locally through candle. The only option that keeps capture text on the machine — and the only one that needs hardware for it (see below). |
| `off` | Embedding similarity alone, which measurably ranks unrelated captures above related ones. |

The hardware caveat, measured rather than assumed: on an M-series Mac
`bge` scores ten candidates in 1.3-1.5 s. On the Synology DS220+ this
system is deployed to it costs **9.4 seconds per candidate** — 568M
parameters in F32 against a Celeron with no AVX2. That is why judging
moved off the machine by default, and why `bge` is still there for
machines that can afford it. Full reasoning and the measurements:
[`adr/0010-echo-is-judged-once-and-remembered.md`](adr/0010-echo-is-judged-once-and-remembered.md).

ADR 0006 says echo involves no LLM. A judge only ever *selects and orders*
candidates and returns numbers — what is displayed is still the verbatim
transcript. That rule is about never putting words in your mouth, and it
still holds.

## Who can read it

Everything ever captured is in here, on a laptop that spends its day open.
Reading it back asks for Touch ID — or the login password, on a Mac with no
sensor — and **capturing does not**. Speaking a note only ever adds to the
system; reading it back is the part worth a gate, and the global shortcut
stays a two-second act. See
[`adr/0011-reading-is-guarded-capturing-is-not.md`](adr/0011-reading-is-guarded-capturing-is-not.md).

The gate is enforced in Rust, not drawn in the webview: the requests behind
it are refused, not merely hidden. It re-locks after five minutes in which
the window was not in front (changeable in **Settings → Reading**, along with
switching it off — which authenticates first). A Mac that cannot
authenticate at all disarms it rather than shutting its owner out; if you
are ever stuck, `"lock_enabled": false` in `settings.json` is the way back.
