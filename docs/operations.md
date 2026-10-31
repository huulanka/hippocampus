# Operations

## Backup and restore

Without a restore that has actually worked, "permanent memory" is an empty
promise. The procedure below has been run through once from start to
finish; what that showed is further down.

There are **two** things to back up, and they belong together:

| What | Where | Why |
| --- | --- | --- |
| Postgres | `data/postgres` (volume) | Events, projections, embeddings |
| Audio | `data/audio` | The original (ADR 0004). Not in the database. |

The embedding model under `data/models` is **not** worth backing up. It is
a download, not state.

### Order

**Copy the audio first, then back up the database.** Both are append-only,
but the database points at audio files, not the other way round. In this
order, the worst case is an audio file in the archive without a matching
row, which is harmless. The other way round you could get a capture whose
original is missing, which is exactly the loss this system exists to rule
out.

### Backing up

```sh
# 1. Audio first. It is content-addressed, so copying the same file again
#    always gives the same result.
tar -cf audio-$(date +%F).tar -C data audio

# 2. Then the database, in custom format (compressed, can be restored
#    selectively).
docker compose exec -T postgres \
  pg_dump -U hippocampus -Fc -d hippocampus > db-$(date +%F).dump
```

Both files belong on storage that outlives the machine.

### Restoring

```sh
# Into an empty instance:
docker compose up -d postgres
docker compose exec -T postgres \
  psql -U hippocampus -d postgres -c "create database hippocampus"
docker compose exec -T postgres \
  pg_restore -U hippocampus -d hippocampus --no-owner < db-2026-09-20.dump

tar -xf audio-2026-09-20.tar -C data
```

`--no-owner` is needed when the target instance has different roles from
the source. Migrations do **not** need to run afterwards: the dump contains
the schema, and `sqlx migrate` recognises the state from
`_sqlx_migrations`.

### What the run-through showed

Restored from a real archive into a fresh database:

- Row counts identical for every table: `events`, `capture_content`,
  `transcript_content`, `capture_search`, `entities`, `observations`,
  `relations`
- The `vector` extension comes along, embeddings keep their 384
  dimensions, and a similarity query returns the same values
- HNSW and GIN indexes are rebuilt (`capture_search_embedding_idx`,
  `capture_search_tsv_idx`, `entities_embedding_idx`)
- The generated `tsv` column keeps its German configuration;
  `plainto_tsquery('german', …)` finds the same matches
- **The append-only rules survive**: `update events …` reports `UPDATE 0`,
  `delete from events` reports `DELETE 0`, and every row is still there
  afterwards
- Audio through `tar` and back: the checksum over all files is identical
- Every `capture_content.audio_path` points at a file that exists after
  the restore

### When to check this again

After every migration that touches an extension, a generated column or a
rule. Those three things don't survive a `pg_dump` automatically just
because the rows do.

## Configuration in the container

`docker-compose.yml` passes through the environment variables the backend
reads. Two of them are not optional once the service is reachable:

- `CF_ACCESS_AUD` **and** `CF_ACCESS_TEAM_DOMAIN`: without both, every
  request is trusted. The backend says so loudly on startup, and it refuses
  to start at all if only one of them is set.
- `CORS_ALLOWED_ORIGINS`: defaults to the desktop client's origins. A
  wildcard is rejected.

`AUDIO_DIR` points at a mounted volume inside the container. Without that
volume, the archive of originals would end up in the container's own file
system and be gone after the next `docker compose up --force-recreate`.

## Deploying to a NAS

The NAS does not build the backend, it pulls it.
`.github/workflows/release-backend.yml` builds `backend/Dockerfile` for
every published release and pushes it to the GitHub Container Registry as
`ghcr.io/huulanka/hippocampus-backend:<version>` and `:latest`, which is
what `docker-compose.yml` refers to.

Building on the NAS used to be the setup, and it stopped working once the
build outgrew Portainer: a release build with full LTO compiles for well
over ten minutes on a NAS CPU, and Portainer gives a stack deploy a fixed
time limit it cannot be told to extend. Past it the deploy fails with
`DeadlineExceeded: context deadline exceeded` while the compiler is still
running. A Git stack in Portainer therefore needs no build at all now;
"Pull and redeploy" fetches the image of the release the stack points at
(`latest` unless `HIPPOCAMPUS_VERSION` is set in the stack's environment).

A second thing that only shows up when building on a Synology: BuildKit
resolves names during `RUN` steps through the host's own
`/etc/resolv.conf`, not through the DNS servers Docker hands to running
containers. When DSM's network settings leave that file pointing at a
server the build cannot reach, `apt-get update` fails with
`Temporary failure resolving 'deb.debian.org'` and the step exits with
code 100, while every running container resolves fine. Setting the DNS
servers by hand in DSM (Control Panel, Network, General) fixes it.

The image itself is built from the repository root, not `backend/`,
because `backend` is a workspace member and needs the root
`Cargo.toml`/`Cargo.lock` and the `contracts` crate next to it (see
[ADR 0007](adr/0007-separate-client-workspace.md) for the related reason
why the client, the other way round, is a workspace of its own).
`docker-compose.build.yml` does the same locally, for a change that has
not been released yet.

`fastembed` is pinned to `ort-download-binaries-rustls-tls` and
`hf-hub-rustls-tls` instead of the default native-tls features (otherwise
the slim Debian image has no OpenSSL at all), and the runtime stage needs
Debian **Trixie**, not Bookworm: the ONNX Runtime binary that `ort`
downloads needs glibc and libstdc++ symbols that Bookworm's glibc 2.36
doesn't have yet. The published image is linux/amd64 only, which is what
an x86_64 NAS runs.

Putting the backend behind Cloudflare Tunnel and Access is described in
[`docs/install.md`](install.md).

## What the backend says about itself

Logs roll daily into `LOG_DIR` (`/data/logs` in the container, so
`${HIPPOCAMPUS_DATA_DIR}/logs` on the NAS) and also go to stdout, where
Portainer or `docker logs` shows them. Everything below is at `INFO`, so no
`RUST_LOG` is needed.

Three kinds of line are the interesting ones day to day.

**Every request, with its duration.**

```
INFO http{method=GET path=/captures/{id}}: request finished status=200 ms=124
```

That's the quickest way to answer "is it the backend or the connection":
the number is pure server time, without tunnel or network.

**Every call to a hosted model.**

```
INFO model call finished purpose="echo-judge" model="google/gemini-3.5-flash-lite"
     provider="Google" served_by="google/gemini-3.5-flash-lite" ms=1388
     prompt_tokens=318 completion_tokens=59
```

`purpose` separates the two callers, `structuring` and `echo-judge`,
because they use different models for different reasons and their timings
have nothing to do with each other. `provider` is the most important value
when the same model is sometimes fast and sometimes slow: under
zero-retention routing OpenRouter picks among the matching providers, and
*which one it was* is the first question. `served_by` differs from `model`
when a route fell back. The token counts are the cost basis.

If a call fails, the same place logs `model call failed` with its duration
and the error, so a silent failure is ruled out. For an error answer the
error includes what OpenRouter said, and that matters most for a 429:

```
WARN model call failed purpose="echo-judge" model="…" ms=153
     error=OpenRouter answered 429 Too Many Requests: {"error":{"message":
     "Provider returned error",…"raw":"… is temporarily rate-limited upstream …"}}
```

"Rate-limited upstream" means one provider is throttling everybody on
that route. Waiting and retrying won't fix it, only another model or
provider will (ADR 0015). A limit on your own account reads differently.

**Every finished echo judgement.**

```
INFO echo judged and stored capture_event_id=… judged_by=google/gemini-3.5-flash-lite
     candidates=4 ms=1402 best=Some(1.0) runner_up=Some(0.0) shown=1
```

`best` and `runner_up` together are the real quality signal: far apart,
the judge separates cleanly; close together and near the threshold, it's
guessing. That is exactly what the still uncalibrated threshold of 0.5 has
to answer over time. `shown` is how many of them actually get shown.
`judged_by` is the model that answered: with a fallback configured it
can be the fallback, and then the default model was unreachable.

**Echoes judged later.** A judgement that failed is tried again in the
background, with a wait that doubles each time up to an hour:

```
WARN echo judging failed; it will be retried after a backoff err=… failures=3 retry_in_secs=80
INFO judged echoes that were left unjudged judged=4
```

A capture showing `failures` climbing while nothing is ever judged means
the judge is unreachable, and the `model call failed` line just above it
says why.

A few commands that have proven useful:

```sh
# How long do judgements take, and who made them?
grep "echo judged" backend.log

# Only the slow requests
grep "request finished" backend.log | awk -F'ms=' '$2 > 1000'

# What do the model calls cost in a day?
grep "model call finished" backend.log | grep -o 'prompt_tokens=[0-9]*'
```
