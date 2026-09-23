# Documentation

Start with *Using it*. The pages under *Design and decisions* are the
reasoning behind everything else, and most of them are written in German,
the language the project is thought through in. Those are marked (de).

## Using it

- **[Installing Hippocampus](install.md)** — The Mac app, the speech model, the backend, and connecting the two
- **[How it works](how-it-works.md)** — Why a capture cannot be lost, how Echo picks what to show, who can read it
- **[Operations](operations.md)** (de) — Backup, a restore that has actually been run, deploying to a NAS

## Working on it

- **[Developing Hippocampus](development.md)** — Running from source, the two installs, releases, the brand assets
- **[Verification](verification.md)** (de) — What the tests cover, and what can only be checked by hand

## Design and decisions

- **[Product scope](product.md)** (de) — What the system is for, and the principles it is held to
- **[Roadmap](roadmap.md)** (de) — Phases, each with a criterion for going on or stopping
- **[Memory model](memory-model.md)** (de) — The six concepts the system knows, and the ones it refuses to
- **[Interface](design.md)** (de) — Where every function lives, and why it looks the way it does
- **[Consolidation](consolidation.md)** (de) — Reconciling the graph after the fact
- **[Entity resolution](entity-resolution.md)** (de) — Telling a duplicate from a relation
- **[Remembering ahead](prospective-memory.md)** (de) — Intentions that come back with the people they are about, before a meeting
- **[Work packages](issues.md)** (de) — The backlog, cut into pieces that can be built one at a time

## Architecture Decision Records

- **[0001](adr/0001-rust-stack.md)** — Full Rust stack: Tauri client, Axum backend
- **[0002](adr/0002-postgres-not-sqlite.md)** — Postgres and pgvector instead of a single SQLite file
- **[0003](adr/0003-event-sourcing-without-event-store.md)** — Event sourcing in Postgres, no dedicated event store
- **[0004](adr/0004-audio-is-the-original.md)** (de) — The audio is the original, the transcript an interpretation
- **[0005](adr/0005-corrections-and-redactions.md)** (de) — Corrections and redactions without touching the original
- **[0006](adr/0006-echo-before-graph.md)** (de) — Echo before graph
- **[0007](adr/0007-separate-client-workspace.md)** — The Tauri client is its own Cargo workspace
- **[0008](adr/0008-candle-not-onnxruntime.md)** — candle, not ONNX Runtime, for local inference on the backend
- **[0009](adr/0009-the-webview-does-not-talk-to-the-backend.md)** — The webview does not talk to the backend
- **[0010](adr/0010-echo-is-judged-once-and-remembered.md)** — Echo is judged once, remembered, and judged off the machine
- **[0011](adr/0011-reading-is-guarded-capturing-is-not.md)** — Reading is guarded, capturing is not
- **[0012](adr/0012-the-shortcut-is-the-record-button-nothing-else-summons-recording.md)** — The app lives in the menu bar; only the shortcut starts a recording
- **[0013](adr/0013-the-calendar-stays-on-the-mac.md)** — The calendar stays on the Mac; the backend only matches
