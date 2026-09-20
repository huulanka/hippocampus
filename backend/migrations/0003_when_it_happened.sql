-- Time awareness: an observation can be about a moment other than the one
-- it was spoken in.
--
-- "Ich treffe Lea am Dienstag" used to be stored exactly like that, and
-- in three weeks "Dienstag" means nothing — which is precisely when the
-- note gets read. The extraction now resolves relative expressions against
-- the moment the note was recorded, and the result lands here.
--
-- Two columns rather than one because precision is part of the fact:
-- "next summer" is not a timestamp and pretending otherwise would invent
-- an accuracy the speaker never had.

alter table observations
    add column happened_on date,
    -- Only set when the speaker actually named a time of day.
    add column happened_at timestamptz,
    add column happened_precision text
        check (happened_precision in ('time', 'day', 'week', 'month', 'year'));

-- "What did I say was happening this week" is a question a memory system
-- has to answer quickly.
create index observations_happened_on_idx on observations (happened_on)
    where happened_on is not null;
