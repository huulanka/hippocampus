# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Hippocampus is a macOS memory-capture app: a Tauri v2 desktop client (Rust + React/Vite webview) talking to an Axum/Postgres backend that runs anywhere Docker does. Speech-to-text happens on-device; an LLM (via OpenRouter) derives structured entities/places/dates without ever overwriting the verbatim capture.

## Two Cargo workspaces — not one

- Root workspace (`Cargo.toml`): `backend` + `contracts` only.
- `client/src-tauri/` is its **own separate** Cargo workspace. Reason: `transcribe-rs` (client speech model) and the backend's embedding stack (`candle`/`fastembed`) pin incompatible exact `ort` versions — one lockfile can't hold both (see `docs/adr/0007-separate-client-workspace.md`).
- Run client Cargo commands from inside `client/src-tauri/`, not the repo root.

## Running from source: two installs must not collide

The Homebrew-installed app and the dev build from source are deliberately separate identities:

- Always run the dev client with `cd client && npm run dev:app` (not plain `npm run tauri dev`) — the plain command runs under the *installed* app's bundle identity (`com.andreasbauer.hippocampus`) and overwrites its real `settings.json`/log dir/mic permission.
- The dev build's identity is `com.andreasbauer.hippocampus.dev`, defaults to backend `http://localhost:8080` (installed app points at a NAS in production).
- NOT split between the two builds (deliberately shared): the 640MB on-device speech model directory, the Keychain entry (one Cloudflare Service Token), and the global capture hotkey (a true system-wide shortcut — whichever build registers first keeps it).

Local dev setup order (`docs/development.md`):
```sh
cp .env.example .env
docker compose up -d postgres
cd backend && sqlx migrate run
cargo run -p backend
./scripts/fetch-asr-model.sh   # ~670MB, once
cd client && npm install && npm run dev:app
```

Note: `docker-compose.yml` maps Postgres to host port **5433**, matching `.env.example`'s `DATABASE_URL`. CI's `rust-db` job maps it to `5432` instead — don't copy CI's `DATABASE_URL` into local `.env`.

## sqlx is checked offline in CI

After changing any `sqlx::query!` call, run `cargo sqlx prepare` from `backend/` (with `DATABASE_URL` set and migrations applied) and commit the regenerated `backend/.sqlx/` directory. CI builds with `SQLX_OFFLINE=true` against that committed directory, so a stale `.sqlx/` breaks CI without a local repro.

## CI shape (`.github/workflows/ci.yml`)

Four independent jobs gate `release`, which only runs on push to `main`:
- `rust` (macOS): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — no database.
- `rust-db` (Linux, separate because GitHub service containers are Linux-only): `cargo test -p backend --features db-tests` against a real `pgvector/pgvector:pg17` container. `#[sqlx::test]` migrates a fresh DB per test; no manual migration step.
- `client` (macOS, `working-directory: client/src-tauri`): fmt/clippy/test for the Tauri workspace.
- `frontend` (Linux, `working-directory: client`): `npm ci && npm run build --if-present`.

No formatter/linter config files exist anywhere (no `.rustfmt.toml`, `clippy.toml`, `.eslintrc*`) — everything runs on tool defaults.

## Commits, PR titles, and releases

- Every PR is squash-merged; the **PR title** becomes the commit on `main` and must follow Conventional Commits (`pr-title-lint.yml` enforces this against `commitlint.config.mjs`) — commit body text doesn't matter for this, the title does.
- `feat:` → minor release, `fix:` → patch, a `BREAKING CHANGE:` footer → major. Anything else (`chore:`, `docs:`, `ci:`, ...) does not trigger a release.
- Versioning, changelog, tag, and GitHub Release are fully automated via `semantic-release` on merge to `main` — never bump a version or tag by hand. Preview with `npm run release:dry-run` from the repo root (root `package.json` is release tooling only, not the app).
- Commit/PR subjects in this repo read as descriptive prose, not terse imperatives (e.g. "the graph is the only view now, and it fills the window") — match that voice.

## Env vars worth knowing (`.env.example`, backend)

`OPENROUTER_API_KEY`, `OPENROUTER_MODEL`, `OPENROUTER_ZDR` (zero-data-retention routing, default strict), `CF_ACCESS_AUD`/`CF_ACCESS_TEAM_DOMAIN` (backend refuses to start if only one is set), `CORS_ALLOWED_ORIGINS` (wildcard rejected outright), `ECHO_MIN_SIMILARITY`, `HIPPOCAMPUS_RERANKER`, `CONSOLIDATION_ENABLED` (off by default — consolidation is deliberately manual via `/consolidation/preview` + `/consolidation/apply`).

## Docs are mostly in German

Only `docs/install.md`, `docs/how-it-works.md`, `docs/development.md`, and the ADRs under `docs/adr/` are in English. The deeper design docs (`docs/product.md`, `docs/memory-model.md`, `docs/consolidation.md`, `docs/entity-resolution.md`, etc.) are in German — check `docs/README.md` for the index before assuming a doc doesn't exist just because it doesn't parse as English.
