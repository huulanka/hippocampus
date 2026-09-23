# Developing Hippocampus

## Workspace layout

```
backend/      Axum HTTP service: ingest, event store, embeddings, search
contracts/    Shared DTOs between backend and client
client/       Tauri v2 desktop app (capture, browse, search, graph)
docs/         Product scope, design, ADRs, operations
scripts/      Speech model download, brand rendering, release helpers
Casks/        The Homebrew cask, rewritten on every release
```

`backend/` and `contracts/` form the root Cargo workspace; `client/src-tauri/`
is a **separate** workspace, because the speech model and the embedding model
pin incompatible exact versions of `ort`. See
[`adr/0007-separate-client-workspace.md`](adr/0007-separate-client-workspace.md).
Cargo commands for the client must be run from `client/src-tauri/`.

## Running it from source

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

# Run the desktop client, under its own bundle identity
cd client && npm install && npm run dev:app
```

`npm run dev:app` rather than `npm run tauri dev`: the plain one runs as
the installed app and writes over its settings. See below.

## The app you use and the app you are changing

These are two different installs, deliberately.

The one you use is the one [installed with Homebrew](install.md#the-mac-app).
The one you are changing runs from source **under its own bundle identity**:

```sh
cd client && npm run dev:app
```

That is `tauri dev` with `src-tauri/tauri.dev.conf.json` merged over the
real config: identifier `com.andreasbauer.hippocampus.dev`, product name
`Hippocampus (dev)`. Everything macOS keys off the identifier therefore
splits — `settings.json`, the log directory, the microphone permission —
so a development run cannot write over the configuration the installed
app is reading. It also means the development build starts with **no
backend configured** and so talks to `http://localhost:8080`, which is
the right default for it: the installed app keeps pointing at the NAS,
and an experiment cannot reach real captures without being told to.

What is *not* split, and why:

- **The speech model.** `asr::default_model_dir` names the stable
  identifier outright rather than deriving it, so both builds find the
  same 640 MB rather than keeping a copy each.
- **The Keychain entry.** One Cloudflare Service Token for one backend;
  splitting it would only mean typing it twice.
- **The global capture shortcut.** It is a system-wide hotkey, not an
  app-scoped one: with both running, whichever registered first keeps it
  and the other logs a warning and carries on without one. Give the
  development build a different combination in its own settings — it has
  its own `settings.json` now, so it sticks.

## Queries are checked offline

Run `cargo sqlx prepare` (from `backend/`, with `DATABASE_URL` set and
migrations applied) after changing any `sqlx::query!` call, and commit the
resulting `.sqlx/` directory — CI builds offline and needs it up to date.

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

## The brand

The mark — the brain in the Dock, the menu bar and the window — has exactly
one source, [`client/src/brand/mark.json`](../client/src/brand/mark.json),
and every raster form of it is rendered from there by
`scripts/render-brand.py`. The pixel mascot is drawn from the grid in
[`client/src/brand/Mascot.tsx`](../client/src/brand/Mascot.tsx);
`scripts/render-readme-art.py` turns that same grid into the SVG at the top
of the README, so the two cannot drift apart.
