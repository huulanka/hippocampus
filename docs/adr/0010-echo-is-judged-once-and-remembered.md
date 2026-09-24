# ADR 0010: Echo is judged once, remembered, and judged off the machine

## Status
Accepted (2026-09-21)

## Context
The backend moved to the NAS (a Synology DS220+, Celeron J4025, no AVX2)
and the system became unusable. Measured through the Cloudflare Tunnel,
against a database holding five captures:

| Endpoint | Time |
| --- | --- |
| `/health`, `/captures`, `/entities`, `/resurface` | 120-135 ms |
| `/search` — embedding, no cross-encoder | 247 ms |
| `POST /captures` — 3 candidates | 27.9 s |
| `POST /captures` — 4 candidates | 36.6 s |
| `GET /captures/{id}` — 2 candidates | 17.7 s |
| `GET /captures/{id}/echo` — 2 candidates | 17.9 s |

Two facts fall out of that table. The database, the tunnel and the
bi-encoder are all healthy — everything that does not involve the
cross-encoder answers in about the time the network alone takes. And the
cost is linear in the number of candidates at **≈ 9.4 seconds per
candidate**: 17.7 for two, 27.9 for three, 36.6 for four.

Linear in the candidate count, not constant, means the model's weights are
not the bottleneck — they are read once per batch — so this is compute,
not memory. `bge-reranker-v2-m3` is 568M parameters in F32; one pair is
roughly 10 GFLOP; a J4025 in candle's CPU path manages roughly 1 GFLOP/s.
The 9.4 seconds is simply what that multiplication costs. More RAM would
not change it. The same model on an M-series Mac does ten candidates in
1.3-1.5 s.

`CANDIDATE_LIMIT` is 10. With five captures only three or four candidates
clear the recall floor; past ten captures every judgement is ten pairs, so
this was on its way to **~95 seconds** per capture *and* per detail view.

Two separate mistakes were hiding behind one symptom:

1. **The detail view recomputed the echo on every open.** Not a caching
   oversight — an echo looks only at captures strictly *earlier* than its
   own, and those never change. The answer is fixed the moment a capture
   is recorded, and it was being recomputed forever.
2. **Recording a capture waited for the judgement.** The capture itself
   was safe on disk after ~250 ms. The remaining 27 seconds were spent
   holding the response open for something the user had not asked to wait
   for. Structuring had already been moved behind the response for exactly
   this reason; echo had not, on the argument that it is "the one thing
   the user is waiting to see".

## Decision

### Judged once, stored, never recomputed
`capture_echo` holds the judged candidates and `capture_echo_judged`
records that a judgement happened and what made it. Two tables, because
"judged, and nothing echoed" is the common case and must not look like
"not judged yet".

Every candidate is stored with its score, including ones below the display
threshold. Retuning the threshold — or asking for everything with
`?min_rerank=-99` to calibrate it — then re-filters what is already known
instead of paying for a judgement again.

A correction is the one thing that invalidates a stored echo: it is a
verdict on a sentence that no longer exists. The correction path already
re-embeds and re-structures; it now forgets the echo too.

### Judged behind the response
`POST /captures` answers as soon as the capture is stored, with
`echo_pending: true`. The judgement runs in a spawned task, exactly as
structuring does. The client polls `GET /captures/{id}/echo` and shows
"looking for earlier thoughts" — which is honest, and different from
"nothing echoed".

A capture with no stored judgement starts one when it is next read. That
is both the backfill for captures recorded before this existed and the
retry when a judgement fails: the marker is only written on success. An
in-flight set keeps a refreshed page from starting the same judgement
twice, because each one is now a paid call.

That set alone only stops *concurrent* duplicates. A judgement that fails
fast — a provider rate limit answers in well under a second — clears the
in-flight set the moment it fails, and the client polls every 1.2s while
it waits. Left alone, that turns one failing call into a new paid call
several times a second for as long as anyone is looking at the note,
which both wastes money and keeps the rate limit from ever clearing. So a
failure now also starts a cooldown (`ECHO_JUDGE_RETRY_BACKOFF_SECS`,
default 20s) before the same capture may be retried — in-memory, like the
in-flight set itself, since a restart clearing it is harmless.

### Judged off the machine
`HIPPOCAMPUS_RERANKER` gains a third value, `remote`, and it is the
default. The candidates still come from the local bi-encoder — that part
is fast, private, and scales with an index. Only the judgement leaves,
over the same OpenRouter path and the same Zero Data Retention routing the
structuring step has always used.

*The model choice below was replaced by ADR 0015: that model's only
provider rate-limited nearly every call.*

The model is `mistralai/mistral-small-2603`, configurable via
`ECHO_JUDGE_MODEL`, and recorded on every stored judgement as provenance.
Of the 440 models in OpenRouter's catalogue, 312 have ZDR endpoints; after
filtering for current generation, reliable structured output and price,
53 remained. The deciding argument was **German**: these are two-sentence
German notes, and the one case this system is known to have got wrong
turned on German nuance. Cost did not decide it — at ~1000 input tokens
once per capture, the spread across every serious candidate was under 25
cents a month.

`bge` stays available for anyone running on hardware that can afford it,
and is the only option that keeps capture text on the machine. `off`
stays as it was. The 2.1 GB reranker download no longer happens on a
default install.

### On ADR 0006's "no LLM"
ADR 0006 says echo involves no LLM. That rule was written against one
specific failure: a memory system must never put words in the user's
mouth. A judge only ever *selects and orders* candidates — it returns
numbers, and what is displayed is still the verbatim transcript. The API
this system asks for cannot return anything else. The rule stands; this is
not an exception to it.

What does change, and should be said plainly: echo now depends on a
network call at capture time. A failed call means the echo arrives later,
never that the capture is lost or that a wrong echo is shown — there is no
fallback to similarity ordering, because that ordering is the bug this
mechanism exists to fix, and falling back into it silently would look like
it was working.

## Consequences
- Saving a capture is ~250 ms instead of 28 s. Opening a capture is
  ~130 ms instead of 27 s. Neither grows with the corpus.
- The number of judgements over the system's life is the number of
  captures — not captures × views. Ten thousand captures change nothing
  about the cost per capture; only candidate retrieval grows, and that is
  an indexed pgvector lookup.
- The echo appears a second or two after the capture rather than with it.
  This is a real loss against ADR 0006's "immediately", and the smaller
  one: it was already 28 seconds, which is not immediate either.
- Capture text now leaves the machine for a second purpose. Same provider,
  same ZDR routing, same trust boundary as structuring — but it is a
  second reason, and anyone who does not want it has `HIPPOCAMPUS_RERANKER=bge`.
- The thresholds are not comparable across judges (raw logits near -4 for
  the cross-encoder, 0-1 relevance for a hosted one), so the default now
  depends on which judge is in play. `ECHO_MIN_RERANK_SCORE` still
  overrides.
- The remote judge's threshold is calibrated against nothing yet. It is
  0.5 because the model is asked for a 0-1 relevance and half is the
  meaningful midpoint — not because it was measured. It needs the same
  treatment the cross-encoder thresholds got, once real captures have
  accumulated.
