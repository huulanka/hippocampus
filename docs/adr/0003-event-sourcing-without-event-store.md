# ADR 0003: Postgres-native event sourcing, no dedicated event store

## Status
Accepted (2026-09-20)

## Context
The core product requirement — original knowledge is never mutated, every
derived fact is traceable to what produced it — maps naturally onto event
sourcing. Two ways to build this properly (not hand-rolled) were
considered: a dedicated event store (e.g. EventStoreDB) alongside Postgres,
or an append-only `events` table directly in Postgres.

A dedicated event store is the more recognizable "industry standard"
artifact, but it means synchronizing two databases (the event store for
raw events, Postgres for projections/search) — more moving parts to
operate on an already resource-constrained NAS, which directly works
against the stated priority of minimizing what can break in production.

Rust also does not have an event-sourcing framework as established as, say,
the JS/.NET ecosystems have — so pulling in a small/niche crate would not
actually buy more "professionalism" than implementing the well-documented
append-only-table-with-optimistic-concurrency pattern directly.

## Decision
Single `events` table in Postgres: `(stream_id, version, event_type,
payload jsonb, source, occurred_at)`, unique on `(stream_id, version)` for
optimistic concurrency, `UPDATE`/`DELETE` blocked at the database level via
`RULE ... DO INSTEAD NOTHING`. `entities`, `observations`, `relations`, and
`capture_search` are projections built from these events and are, in
principle, always rebuildable by replaying the log.

## Consequences
- Two containers total (backend + Postgres), matching the operational
  simplicity goal.
- No dedicated event-sourcing library to learn or depend on; the pattern is
  implemented directly and kept intentionally small (see
  `backend/src/events.rs`).
- If this ever needs to scale beyond a single-user personal tool, migrating
  to a dedicated event store remains possible without changing the
  application-level event shape.
