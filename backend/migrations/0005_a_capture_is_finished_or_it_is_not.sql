-- Everything that still has to happen to a capture after it is safely
-- stored, written down instead of merely attempted.
--
-- Two silent holes closed at once, both of the same shape: work that ran
-- behind a capture, failed, said so only in a log line, and was never
-- tried again.
--
-- 1. **Structuring.** A failed OpenRouter call produced exactly one log
--    line (`structuring.rs`: "structuring failed for capture"). The
--    capture stayed safe — but it would never have entities, never an
--    observation, never a resolved date, and would therefore never appear
--    on an entity page or in Resurface. The note was there and its
--    meaning was silently gone.
--
-- 2. **Indexing.** `index_capture` was called with `?`, so a failed
--    embedding returned 500 *after* the capture had been durably written.
--    The caller was told the capture failed when it had not — which, the
--    moment a client retries on failure, means a duplicate note.
--
-- Same shape as `capture_echo_judged`, for the same reason: "structured,
-- and it yielded nothing" has to be distinguishable from "never
-- structured". Without the marker, a capture the model had nothing to say
-- about looks exactly like one whose call timed out — and only one of the
-- two is worth paying to retry.

create table capture_pipeline (
    capture_event_id uuid primary key references events (id) on delete cascade,
    -- Searchable and embedded: the `capture_search` row exists.
    indexed_at       timestamptz,
    -- The model that produced the structure. Null while this capture has
    -- only ever failed, which is part of what makes a row "unfinished".
    structured_by    text,
    structured_at    timestamptz,
    attempts         integer not null default 0,
    last_attempt_at  timestamptz,
    last_error       text,
    -- Set once the retry loop has given up. A capture here is never
    -- retried automatically again; it waits for a person, because past
    -- this point every further attempt is a paid call that has already
    -- failed the same way several times over.
    abandoned_at     timestamptz
);

-- The retry loop's one query: what is still unfinished, longest-waiting
-- first. Partial, because the answer is almost always a handful of rows
-- out of the whole archive.
create index capture_pipeline_unfinished_idx
    on capture_pipeline (last_attempt_at nulls first)
    where (indexed_at is null or structured_at is null) and abandoned_at is null;

-- Backfill. Indexing is knowable exactly: the `capture_search` row either
-- exists or it does not.
insert into capture_pipeline (capture_event_id, indexed_at, structured_by, structured_at, attempts)
select
    cc.event_id,
    (select cs.occurred_at from capture_search cs where cs.event_id = cc.event_id),
    -- Structuring is only knowable by its traces, and naming a model here
    -- would be inventing one: `observations.model` knows, but a capture
    -- has several and picking one would be a guess dressed as a fact.
    case when exists (
        select 1 from observations o where o.source_event_id = cc.event_id
        union all
        select 1 from relations r where r.source_event_id = cc.event_id
    ) then 'before this was recorded' end,
    case when exists (
        select 1 from observations o where o.source_event_id = cc.event_id
        union all
        select 1 from relations r where r.source_event_id = cc.event_id
    ) then now() end,
    0
from capture_content cc
where cc.redacted_at is null;

-- What that leaves unfinished is picked up by the retry loop on its next
-- tick. Two outcomes, both right: a capture that failed back then finally
-- gets structured, and a capture the model genuinely had nothing to say
-- about costs one call and is then done for good.
