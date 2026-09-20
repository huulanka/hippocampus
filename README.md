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
```

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

# Run the desktop client
cd client && npm install && npm run tauri dev
```

Run `cargo sqlx prepare` (from `backend/`, with `DATABASE_URL` set and
migrations applied) after changing any `sqlx::query!` call, and commit the
resulting `.sqlx/` directory — CI builds offline and needs it up to date.

## License

MIT, see [LICENSE](LICENSE).
