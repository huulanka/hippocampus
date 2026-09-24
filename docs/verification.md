# What has been checked, and what hasn't

Automated tests cover most things, but not everything. Some things an
automated run fundamentally can't verify: a token Cloudflare only issues
once the tunnel exists, a shortcut that has to fire from inside another
app, and anything that only shows in the finished window. This list keeps
track of what is still open, so it doesn't get lost in pull request
descriptions.

## Open: needs a person

### Remembering ahead in the bundled build
Everything that depends on macOS only works in the real app (like the
microphone and Touch ID):

1. Settings → Meetings → *Allow…*. Expected: the system dialog with the
   text from `Info.plist`, then the calendars, grouped by account and in
   their colours. Tick only the calendars that belong on this Mac.
2. Create a meeting nine minutes from now, with a known name in the title
   ("Jour fixe Paul"), and beforehand say something about Paul that is an
   intention. Expected: the brain in the menu bar turns clay, two sparks
   twinkle for about 40 seconds and then stay still; a banner "In 9 min: …"
   (not in a Focus mode); clicking the icon opens the menu with the meeting
   at the top, showing only "Something to bring up" while locked and the
   quote after "Unlock…"; the meeting entry opens the brief page, "Write a
   Note…" opens the capture sheet for typing.
3. After the meeting ends: a banner "… is over. Did you bring it up?", and
   the icon stays lit until it's answered on the brief page.
4. Look at it on a light and a dark menu bar. The two palettes have only
   been checked as pixels (`tray::tests::tray_preview`), never in the real
   bar.

Checked so far: matching meetings to entities (`brief::tests`), the phases
and banner decisions (`foresight::tests`), the routes against Postgres
(`routes::intentions::db_tests`), and every page in the browser against a
running backend with a Tauri stub.

### Intention detection against the real model
The prompt has **never run against OpenRouter** from a development
machine. The parser (missing key, empty text, unknown `about`) and the
quote check are tested. Open: how often the model detects an intention
that isn't one, and how often it misses one. Look at a week of real notes
after deploying; the [x] in the interface is the measurement (count
`intention.dismissed` in the log).

### The recorded shortcut
Press `[ Change ]` in Settings, type a combination, then trigger it from
another app. Expected: it works immediately, without a restart, and
survives a restart.

### Connecting to the NAS from the finished app
Enter the backend URL and service token in Settings, save, then capture,
search and play a recording. Expected: "Saved, checked reachable just
now.", and every screen talks to the NAS instead of localhost. How the
client reacts to 200, 302 and 403, and that the headers actually go over
the wire, is tested against real sockets (`backend.rs`); the path through
the real tunnel isn't.

### The remote echo judge against the real API
The call to OpenRouter has the same shape as the one structuring has been
making all along, and parsing the answer is unit-tested with six cases
(missing verdict, invented ID, values outside 0–1, prose instead of JSON,
empty list, order). A real call from the deployed backend shows from the
outside: `GET /captures/{id}/echo` then fills `judged_by` with the model
name instead of `similarity`.

### The remote judge's threshold
0.5 is set because the model is explicitly asked for a 0–1 relevance and
half is the sensible midpoint, **not because it was measured**. It needs
the same calibration the cross-encoder thresholds got, once there are
enough real captures. `?min_rerank=-99` returns the values for it and
costs nothing, because every candidate is stored with its score.

### A real Cloudflare Access token
Signature checking is tested against self-generated key pairs (valid,
foreign `aud`, foreign team, expired, forged, `alg: none`). Whether a
token actually issued by Cloudflare passes can only be checked once the
tunnel is up.

### Correcting your own text
`[ fix a word ]` in the detail view, change the text, save. Expected: the
new version appears everywhere (timeline, search, entities), the old one
below it under `[ HOW THE WORDS CHANGED ]`. Run against a development
database, but not by a person in the app.

### Audio playback in the real app
The player has been checked in the browser against the running service,
not in the Tauri webview, and since ADR 0009 it no longer fetches the
recording through `<audio src>` but as a blob through Rust, because a
`src` can't carry the Access headers. Both deserve one listen in the
finished app.

### candle on the target hardware
The prebuilt ONNX Runtime binary from `ort` crashed with SIGILL on a
Celeron NAS without AVX2, before the first log line was written. The move
to `candle` (ADR 0008) is built, tested and verified locally, including
with `docker build --platform linux/amd64`, which confirms the container
build. What a local Docker build on Apple Silicon **cannot** check is
whether the SIGILL is really gone on that hardware, because QEMU emulation
doesn't reproduce the same (missing) CPU features.

## Checked, and how

| What | How |
| --- | --- |
| Recording from the real microphone | Checked by hand in the bundled app: the permission dialog appeared and speech was transcribed. |
| Echo threshold (cosine) | Measured on real captures: noise up to 0.877, real matches from 0.905. Threshold 0.89, which now only applies without a reranker. |
| Echo threshold (cross-encoder, Jina, since replaced) | Calibrated across the whole development archive. The threshold of −2.0 sat between a real match and a near miss, with a narrow gap of 0.12. Applies to a model that no longer runs (see ADR 0008). |
| Echo threshold (cross-encoder, BGE via candle) | Measured through the running service: a real match at +0.193, five unrelated candidates between −8.37 and −10.33. Threshold −4.0 in the middle of the gap. One real match so far; no corpus-wide calibration yet. |
| Reranker latency (Jina, since replaced) | Ten candidates: Jina 178 ms, BGE via `ort` 605 ms (`examples/rerank_latency.rs`). |
| Reranker latency (BGE via candle) | Ten candidates on an Apple Silicon Mac: 1.6 s cold, 1.3 s warm (`examples/rerank_latency.rs`). Slower than the `ort` version on the same machine. |
| Resolving time | A note at 00:24 Berlin time: "morgen Abend um halb acht" → 2026-09-22 17:30 UTC, "nächsten Dienstag" → 2026-09-29. Both right, including the subtlety that "tomorrow" is already Tuesday. |
| Correction chain | A typed capture corrected twice: versions `you, typed` → `you, corrected` → `you, corrected`, entities re-derived, `structuring.invalidated` written. |
| Entity pages | One entity brings together captures from different days and all of its edges on one page. |
| Hybrid search (RRF) | A proper noun found by both retrievers ranks ahead (0.0328) of single-retriever matches (0.0161). |
| Audio upload and storage | Content-addressed, deduplicated, size limit, format check: unit tests plus curl against the running service. |
| Error responses | 400 for an empty capture, a wrong MIME type, a missing transcript; 404 for an unknown capture. |
| Access control | 401 without a token and with a junk token, `/health` open, CORS only for the client origins, startup aborts on half a configuration. |
| Backup and restore | Restored once from start to finish; see `operations.md`. |
| Resampling, mono mixdown, WAV | Unit tests in the client. |
| Settings | The default parses, the accelerator round-trips, nonsense is rejected, a broken file falls back to the default. |
| Secret in the Keychain | Write, read, delete and delete again against the real macOS Keychain (`cargo test --lib -- --ignored`). A unit test also pins down that `persist` never writes the secret into `settings.json`. |
| Access errors can be told apart | Against real sockets: 200 is accepted, a 302 to the Cloudflare login page is reported as a rejected service token rather than a healthy backend, a 403 names the Access policy, a URL without a scheme says so. |
| Echo persistence, end to end | Against a local database: saving a capture 81 ms with `echo_pending: true`, detail view 22 ms, reading the echo again 2.5 ms, `?min_rerank=-99` returns all stored candidates without recomputing. The database holds a marker with provenance plus the ranked rows. |
| Catching up and re-judging | A capture without a stored judgement answers the first read with `pending: true` and the second with the result. A correction deletes the judgement and triggers a new one, confirmed through `judged_at`. |
| Reranker latency on the NAS (BGE via candle) | Measured against the running backend: 17.7 s for two candidates, 27.9 s for three, 36.6 s for four. Linear, **9.4 s per candidate**. On the same path, `/search` takes 247 ms, every read-only endpoint 120–135 ms, `/health` 160 ms. The bottleneck is purely the cross-encoder, and because time grows linearly with candidates (weights are read once per batch), it's compute, not memory. |
| Redaction, end to end | Against the running service: `DELETE` reports what was removed; the capture then disappears from timeline and search; opened by its ID it shows `redacted: true`, no text and no transcripts; a second `DELETE` answers 200 instead of an error. |
| Echo judge, real call | Against OpenRouter from a development machine: `mistralai/mistral-small-2603` through the provider Mistral, **ten candidates in 1.0 s**, 547 prompt and 147 completion tokens. From the NAS the same call with *five* candidates took 12.3 s: same model, same provider, so the difference is the NAS's route out, not the model. This is now in the logs. |
| Echo judge choice | `backend/examples/judge_compare.rs`, invented German notes, 36 labelled pairs, three runs each, production prompt. `gemini-3.5-flash-lite`: 27/27 answered, median 555 ms, 44/45 true echoes and 2/72 non-echoes shown, order never wrong. `mistral-small-3.2-24b-instruct`: 27/27, median 686 ms, 43/45 and 5/72, order wrong twice. `mistral-small-2603`: 0/29, all 429 "rate-limited upstream". See ADR 0015. |
| Judge fallback and catch-up, end to end | Against a local backend and a throwaway database. With an invalid key: a capture of over 5,000 characters is indexed (it used to fail at 512 tokens), every failed call logs OpenRouter's answer, and the background pass retries the newest capture at 5, 10, 20 and 40 s and stops at the first failure. With a real key and the throttled model as primary: OpenRouter falls back, `judged_by` names the fallback model, and one pass judges all four waiting captures. With the defaults: a new capture is judged by Gemini in 1.4 s. |
| What the backend says about itself | One line per request with its duration, one per model call with provider, duration and tokens, one per echo judgement with duration, best and runner-up. Checked against the running service; examples in `operations.md`. |
| Service token headers on the wire | Against a real socket: with credentials, `cf-access-client-id` and `cf-access-client-secret` are in the request; without credentials neither is (rather than empty headers). |

## Known state

- Observations from before time resolution was added have no resolved
  date. They only get one once re-derivation is built or the capture is
  corrected, so "you said this was coming" only fills up with new notes.
- The bi-encoder demonstrably still ranks wrongly: a note about a sauna
  evening ranked an unrelated note about cardamom espresso slightly above
  the one about the sauna infusion. That is exactly what the second stage
  is for.
- Since ADR 0009 the client talks to the backend from Rust, not from the
  webview. The backend's CORS configuration now only matters for the
  browser development build.
- Dependabot reports `glib` 0.18.5 (unsound `Iterator` implementation). It
  comes in through GTK from Tauri's Linux webview stack, isn't compiled on
  macOS, and Tauri 2.11 can't be moved to `glib` 0.20. Nothing that can
  be fixed here.
