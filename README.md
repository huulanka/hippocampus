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

| Variable | Default | What it does |
| --- | --- | --- |
| `HIPPOCAMPUS_ASR_MODEL_DIR` | app support dir | Where the speech model lives |
| `HIPPOCAMPUS_API_BASE_URL` | `http://localhost:8080` | Backend the client talks to |

## Echo

After every capture, Hippocampus shows the earlier captures closest to it —
your own words, never a summary
([`docs/adr/0006-echo-before-graph.md`](docs/adr/0006-echo-before-graph.md)).

Retrieval and judgement are two different models. The bi-encoder
(`multilingual-e5-small`, already in the database) finds candidates, because
recall is what it is good at. A cross-encoder then reads the new capture and
each candidate **together** and decides which survive — a bi-encoder cannot,
because it compares two vectors that have never met, and measurably ranked
*cardamom buns* above *finnischer Aufguss* for a note about a sauna.

The default reranker is `jina-reranker-v2-base-multilingual`: 178 ms for ten
candidates, against 605 ms for `bge-reranker-v2-m3`. Echo runs inline, right
after a capture, so the cheaper of two correct models wins. Set
`HIPPOCAMPUS_RERANKER=off` to fall back to similarity alone.

The model files are downloaded on first start into `MODEL_CACHE_DIR`
(1.1 GB for the default reranker, on top of the 465 MB embedding model).

Run `cargo sqlx prepare` (from `backend/`, with `DATABASE_URL` set and
migrations applied) after changing any `sqlx::query!` call, and commit the
resulting `.sqlx/` directory — CI builds offline and needs it up to date.

## Operations

Backup, restore and what the container needs configured:
[`docs/operations.md`](docs/operations.md). The restore procedure there
has been run end to end, not just written down.

## License

MIT, see [LICENSE](LICENSE).
