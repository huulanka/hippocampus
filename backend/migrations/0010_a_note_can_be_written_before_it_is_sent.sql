-- A writing session: hours of notes kept on the device and sent at the end.
--
-- Two things follow from that, and this migration is both of them.
--
-- First, a note has to be able to carry the moment it was *written*. Until
-- now `events.occurred_at` defaulted to the moment the row was inserted,
-- which is correct for a capture that is spoken and sent in the same
-- breath and wrong for every note in a four-hour workshop — all of them
-- would claim to have been thought at the moment Send was pressed. The
-- column already allows an explicit value; what was missing was a reason
-- to pass one, and a guard against the future (see the check below, which
-- is enforced in the route rather than here so the error can explain
-- itself).
--
-- Second, the session needs somewhere to keep its name. Deliberately NOT
-- the event payload: the event log is never modified, so a title typed by
-- hand — which is ordinary prose, and may name people — could never be
-- redacted afterwards (ADR 0005). The payload carries the session's id and
-- nothing else; the title lives here, where it can be changed and removed
-- like any other content.
--
-- A session is not an entity and does not appear in the graph. The
-- extraction finds entities in what was *said*; a heading typed into a
-- text field was not said, and inventing a node for it would be the
-- interface putting a guess into the graph — the line docs/product.md
-- draws at P12.

create table capture_sessions (
    id          uuid primary key,
    title       text not null,
    started_at  timestamptz not null,
    created_at  timestamptz not null default now()
);

alter table capture_content
    add column session_id uuid references capture_sessions (id);

-- Everything written in one sitting, in the order it was written.
create index capture_content_session_idx
    on capture_content (session_id)
    where session_id is not null;
