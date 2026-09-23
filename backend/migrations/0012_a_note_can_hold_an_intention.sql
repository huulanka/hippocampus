-- Something the speaker means to do, say or ask later — and has not yet.
--
-- The `intention.noted` event is the record that the model found one; this
-- table is its projection *and* the only place its words live. The event
-- carries ids and nothing else, for the same reason a session title is
-- kept out of the log (see `CaptureSession`): the log is never modified, so
-- the only words that can ever be redacted are words that were never
-- written into it. Redacting a capture deletes its rows here.
create table intentions (
    id               uuid primary key,
    -- The capture it was heard in.
    source_event_id  uuid not null references events (id),
    -- The model's short phrasing, in the speaker's language.
    text             text not null,
    -- The speaker's own words, when the model's quote was found verbatim
    -- in the transcript. Null rather than a paraphrase dressed as a quote.
    quote            text,
    status           text not null default 'open'
                     check (status in ('open', 'fulfilled', 'dismissed')),
    model            text not null,
    noted_event_id   uuid not null references events (id),
    created_at       timestamptz not null default now(),
    resolved_at      timestamptz
);

create index intentions_open_idx on intentions (created_at) where status = 'open';
create index intentions_source_idx on intentions (source_event_id);

-- What an intention is about. Stored against the entity it was noted on;
-- a later merge is followed when read (`coalesce(merged_into, id)`), not
-- rewritten here, so an unmerge needs no bookkeeping of its own.
create table intention_entities (
    intention_id  uuid not null references intentions (id) on delete cascade,
    entity_id     uuid not null references entities (id) on delete cascade,
    primary key (intention_id, entity_id)
);

create index intention_entities_entity_idx on intention_entities (entity_id);
