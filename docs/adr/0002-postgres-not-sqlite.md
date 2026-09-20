# ADR 0002: Postgres + pgvector instead of a single SQLite file

## Status
Accepted (2026-09-20)

## Context
An early design proposed a single SQLite file (with FTS5 + `sqlite-vec`) as
the entire data store, for operational simplicity on the NAS. This was
rejected: a database file sitting on NAS storage, potentially accessed by
more than one process over time, is a real source of lock contention and
corruption risk, and "a database is just a file" was explicitly not
considered acceptable for what is meant to be a permanent personal memory
store.

## Decision
Run Postgres (`pgvector/pgvector` image, which bundles the `pgvector`
extension) as its own container, alongside the backend, in one Portainer
stack. Postgres provides full-text search (`tsvector`/`tsquery`), vector
similarity search (`pgvector`), and JSONB in one place, so no additional
services are needed beyond the two containers.

## Consequences
- Two containers instead of one, but both are ordinary, well-understood
  services with official images — low operational risk.
- Real concurrent-access guarantees, proper backups via `pg_dump` /
  filesystem snapshot of the volume (already covered by the existing NAS
  backup job).
- Requires a running Postgres for local development; the project uses
  `sqlx`'s offline query cache (`.sqlx/`, committed to the repo) so CI and
  quick builds don't require a live database connection.
