# Hippocampus

A personal knowledge system fed primarily by voice: speak a thought, and it
gets transcribed on-device, stored verbatim forever, and structured by an
LLM into entities, relations, and tasks — without ever altering the
original.

**Core principle:** original knowledge (what was said, and when) and
AI-derived knowledge (what was inferred from it) are stored separately and
are always traceable back to each other. See
[`docs/adr/0003-event-sourcing-without-event-store.md`](docs/adr/0003-event-sourcing-without-event-store.md).

## Architecture

```
Tauri v2 client (Rust)  --HTTPS-->  Axum backend (Rust)  -->  Postgres + pgvector
  on-device ASR                       event store,              (events, projections,
  (transcribe-rs)                     embeddings,                 full-text + vector
                                       OpenRouter for               search)
                                       structuring)
```

See [`docs/adr/`](docs/adr/) for the reasoning behind the stack, and the
project plan for the full concept and roadmap.

## Workspace layout

```
backend/      Axum HTTP service: ingest, event store, embeddings, search
contracts/    Shared DTOs between backend and client
client/       Tauri v2 desktop app (capture + browse/search)
docs/adr/     Architecture Decision Records
scripts/      One-off setup scripts (speech model download)
```

`backend/` and `contracts/` form the root Cargo workspace; `client/src-tauri/`
is a **separate** workspace, because the speech model and the embedding model
pin incompatible exact versions of `ort`. See
[`docs/adr/0007-separate-client-workspace.md`](docs/adr/0007-separate-client-workspace.md).
Cargo commands for the client must be run from `client/src-tauri/`.

## Installing it

Apple Silicon only, macOS Sonoma or newer:

```sh
brew tap huulanka/hippocampus https://github.com/huulanka/hippocampus
brew install --cask hippocampus
```

This build is **ad-hoc signed, not notarised** — there is no Apple
Developer account behind it. That used to mean installing with
`--no-quarantine`, and older writeups still say so, but Homebrew 7
removed the flag and stopped attaching the quarantine attribute at all,
so the app now opens as installed.

Downloading the `.dmg` from the Releases page by hand is a different
story: the browser does attach the attribute, and Gatekeeper will then
refuse to open the app until you remove it once with
`xattr -dr com.apple.quarantine /Applications/Hippocampus.app`.

Install it properly rather than running `npm run tauri dev` for daily use:
**the microphone only works from a bundled app.** macOS grants microphone
permission per bundle identity, and a development binary has none — it
records silence instead of failing, which is the worst possible way to
find out.

The cask and the `.dmg` it points at are produced by
`.github/workflows/release-app.yml` on every published release, so the
version you get is the version that was released.

## Development

Prerequisites: Rust (stable, via [rustup](https://rustup.rs)), Docker,
Node.js (for the Tauri frontend).

```sh
cp .env.example .env   # fill in values

# Start Postgres
docker compose up -d postgres

# Run migrations
cd backend && sqlx migrate run

# Run the backend
cargo run -p backend

# Fetch the on-device speech model (~670 MB, once)
./scripts/fetch-asr-model.sh

# Run the desktop client
cd client && npm install && npm run tauri dev
```

## Releases

Versioning is [semantic-release](https://semantic-release.org/), driven by
PR titles — every PR is squash-merged, so the PR title becomes the commit
header on `main`, and that header is what decides the next version. It
must follow [Conventional Commits](https://www.conventionalcommits.org/)
(`feat: ...`, `fix: ...`, `chore: ...`); a PR-title-lint check enforces
this before merge. `feat` bumps minor, `fix` bumps patch, a `BREAKING
CHANGE:` footer bumps major — anything else (`chore`, `docs`, `ci`, ...)
does not release at all.

On every push to `main` that passes CI, the release job in
`.github/workflows/ci.yml` runs `semantic-release`, which sets the version
in every place it is duplicated (via `scripts/bump-version.sh`), updates
`CHANGELOG.md`, commits, tags, and creates a GitHub Release — no manual
version bump or tag, ever. Preview what a release would do without
publishing anything:

```sh
npm install   # once, at the repo root — this is release tooling, not the app
npm run release:dry-run
```

## Who can read it

Everything ever captured is in here, on a laptop that spends its day open.
Reading it back asks for Touch ID — or the login password, on a Mac with no
sensor — and **capturing does not**. Speaking a note only ever adds to the
system; reading it back is the part worth a gate, and the global shortcut
stays a two-second act. See
[`docs/adr/0011-reading-is-guarded-capturing-is-not.md`](docs/adr/0011-reading-is-guarded-capturing-is-not.md).

The gate is enforced in Rust, not drawn in the webview: the requests behind
it are refused, not merely hidden. It re-locks after five minutes in which
the window was not in front (changeable in **Settings → Lock**, along with
switching it off — which authenticates first). A Mac that cannot
authenticate at all disarms it rather than shutting its owner out; if you
are ever stuck, `"lock_enabled": false` in `settings.json` is the way back.

## Voice capture

Speech recognition runs on this machine, never on the server: audio is the
most revealing thing the system holds, so it and the microphone stream stay
local, and only the resulting text is ever sent on. See
[`docs/adr/0004-audio-is-the-original.md`](docs/adr/0004-audio-is-the-original.md).

`scripts/fetch-asr-model.sh` downloads int8-quantised
[parakeet-tdt-0.6b-v3](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx)
(multilingual, German included) into the app's support directory, or into
`$HIPPOCAMPUS_ASR_MODEL_DIR` when that is set. Without the model the app still
runs and typed capture still works — the record button simply stays hidden.

macOS asks for microphone permission the first time you record.

## Talking to a backend that is not on this machine

The desktop client's HTTP requests are made in Rust, not by the webview —
see [`docs/adr/0009-the-webview-does-not-talk-to-the-backend.md`](docs/adr/0009-the-webview-does-not-talk-to-the-backend.md).
The short version: a `fetch` carrying Cloudflare Access headers needs a
CORS preflight, and Access answers an unauthenticated `OPTIONS` with a
login redirect, so the request never leaves the window. A request made in
Rust has no origin and no preflight.

Point the client at a backend in **Settings → Backend**. It is checked
against `/health` before it is saved, so a typo cannot strand the screen
that would let you fix it.

If that backend sits behind a Cloudflare Zero Trust Access application,
add a **Service Token** (not your own login) under **Settings → Cloudflare
Access**. The Client ID is stored in `settings.json`; the Client Secret is
stored in the **macOS Keychain** and is never written to disk in readable
form, never sent to the webview, and never logged. A secret left in
`settings.json` by version 1.2.0 or earlier is moved into the Keychain the
first time this version starts — rotate that token afterwards, since it
was on disk in the clear until then.

| Variable | Default | What it does |
| --- | --- | --- |
| `HIPPOCAMPUS_ASR_MODEL_DIR` | app support dir | Where the speech model lives |
| `HIPPOCAMPUS_API_BASE_URL` | `http://localhost:8080` | Backend the client talks to, when the settings screen has no value saved |

## Echo

After every capture, Hippocampus shows the earlier captures closest to it —
your own words, never a summary
([`docs/adr/0006-echo-before-graph.md`](docs/adr/0006-echo-before-graph.md)).

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
[`docs/adr/0010-echo-is-judged-once-and-remembered.md`](docs/adr/0010-echo-is-judged-once-and-remembered.md).

ADR 0006 says echo involves no LLM. A judge only ever *selects and orders*
candidates and returns numbers — what is displayed is still the verbatim
transcript. That rule is about never putting words in your mouth, and it
still holds.

Run `cargo sqlx prepare` (from `backend/`, with `DATABASE_URL` set and
migrations applied) after changing any `sqlx::query!` call, and commit the
resulting `.sqlx/` directory — CI builds offline and needs it up to date.

## Operations

Backup, restore and what the container needs configured:
[`docs/operations.md`](docs/operations.md). The restore procedure there
has been run end to end, not just written down.

## License

MIT, see [LICENSE](LICENSE).
