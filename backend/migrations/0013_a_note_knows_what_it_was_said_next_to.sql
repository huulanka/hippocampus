-- What a note was said next to: the meeting around it, the note before
-- it, and — one level up — what is known about an entity as a whole.
--
-- All three are readings, not facts. Time only offers a candidate (a note
-- spoken ten minutes before a meeting with Paul *may* be preparation for
-- it; it may just as well be a shopping list), and a model that reads the
-- note decides. Every row here names that model, and none of it is ever
-- shown as more than what it is (docs/finding-again.md).

-- A meeting a note was spoken around, as far as the graph knows it.
--
-- Deliberately without the meeting: no title, no attendees, no times. The
-- Mac that read its calendar matched the meeting against the graph and
-- sends what matched; the calendar itself stays on the Mac (ADR 0013,
-- amended by ADR 0016). What is kept is "a meeting with Paul and the
-- harbour portal, 20 minutes after this note".
create table capture_occasions (
    id                uuid primary key,
    capture_event_id  uuid not null references events (id),
    -- Which Mac offered it. Two Macs with two calendars can each offer
    -- their own meeting for the same note from the phone.
    checker           uuid not null,
    phase             text not null check (phase in ('before', 'during', 'after')),
    -- Minutes between the note and the meeting's nearer edge; 0 during.
    minutes           integer not null check (minutes >= 0),
    -- Null until a model has read the note against it.
    verdict           text check (verdict in ('belongs', 'unrelated')),
    model             text,
    judged_at         timestamptz,
    offered_event_id  uuid not null references events (id),
    created_at        timestamptz not null default now()
);

create index capture_occasions_capture_idx on capture_occasions (capture_event_id);
create index capture_occasions_unjudged_idx on capture_occasions (created_at) where verdict is null;

-- Who and what the meeting was with. A later merge is followed when read,
-- the same as `intention_entities`.
create table capture_occasion_entities (
    occasion_id  uuid not null references capture_occasions (id) on delete cascade,
    entity_id    uuid not null references entities (id) on delete cascade,
    primary key (occasion_id, entity_id)
);

create index capture_occasion_entities_entity_idx on capture_occasion_entities (entity_id);

-- Which Mac has already looked in its calendar for which note, meeting
-- found or not. Without it every Mac would ask about every note forever.
create table capture_occasion_checked (
    capture_event_id  uuid not null references events (id),
    checker           uuid not null,
    checked_at        timestamptz not null default now(),
    primary key (capture_event_id, checker)
);

-- Whether a note carries on from one spoken shortly before it.
--
-- One row per pair that was read, `continues` either way: a pair judged
-- unrelated is not asked about again. An episode is what these edges
-- connect.
create table capture_continuations (
    capture_event_id   uuid not null references events (id),
    previous_event_id  uuid not null references events (id),
    continues          boolean not null,
    model              text not null,
    judged_at          timestamptz not null default now(),
    primary key (capture_event_id, previous_event_id)
);

create index capture_continuations_previous_idx on capture_continuations (previous_event_id)
    where continues;

-- A note whose neighbours in time have been read. Set even when there
-- were none, so the sweep does not keep finding it.
create table capture_context_judged (
    capture_event_id  uuid primary key references events (id),
    model             text,
    judged_at         timestamptz not null default now()
);

-- What is known about an entity, in a few sentences that each name the
-- notes they rest on.
--
-- The `entity.gist_written` event is the record; the words live only
-- here, so they can go when a note they rest on is redacted. Rewritten
-- when the entity has been observed again since.
create table entity_gists (
    entity_id          uuid primary key references entities (id) on delete cascade,
    -- [{"text": "...", "sources": ["<capture event id>", ...]}, ...]
    sentences          jsonb not null,
    model              text not null,
    observations_seen  integer not null,
    event_id           uuid not null references events (id),
    written_at         timestamptz not null default now()
);
