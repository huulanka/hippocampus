-- The weekly review's written paragraph.
--
-- Everything else in the review is counted on every read. The paragraph
-- is the one part a model writes, so it is written once and kept: the
-- `review.written` event is the record, this is its projection — the
-- latest one per week, because a paragraph can be written again once the
-- week has seen more notes.
create table weekly_story (
    week_start     date primary key,
    timezone       text not null,
    -- [{"text": "...", "sources": ["<capture event id>", ...]}, ...]
    sentences      jsonb not null,
    model          text not null,
    -- How many of the week's captures the model was shown.
    captures_seen  integer not null,
    event_id       uuid not null references events (id),
    written_at     timestamptz not null default now()
);
