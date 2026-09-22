-- Consolidation: two rows may turn out to be one thing, and the graph has
-- to be able to say so without losing what it knew.
--
-- Three columns/tables, each answering a question the current schema
-- cannot:
--
--  * `merged_into` — "which entity is this one really?" Merging never
--    deletes. A deleted row takes its observations' history with it and
--    makes the merge irreversible; a pointer keeps both and makes
--    `entity.unmerged` a matter of clearing one field.
--
--  * `entity_alias` — "what else has this been called?" Without it a
--    merge trades fragmentation for unfindability: once the node is
--    called "Paul Hartmann", searching "Paul" stops finding him. The
--    surviving name is a display choice; every name it ever had stays a
--    key.
--
--  * `last_consolidated_at` — "has anything looked at this yet?" Null
--    means never. This is what makes the run cyclical rather than
--    one-off: it can always name the N entities that have waited
--    longest, so nothing sits unexamined forever just because it stopped
--    being mentioned.
--
--  * `entity_merge_block` — "which pairs must be left alone?" Two uses.
--    A merge the user took back must never be proposed again (the
--    oscillation named in docs/consolidation.md), and — later, once a
--    model judges — a pair ruled `different` must not be paid for twice.

alter table entities
    add column merged_into uuid references entities (id),
    add column last_consolidated_at timestamptz;

-- Every lookup of a live entity filters on this, so it earns an index
-- even at 78 rows.
create index entities_merged_into_idx on entities (merged_into)
    where merged_into is not null;

-- The run's own queue: never looked at first, then longest ago.
create index entities_consolidation_due_idx on entities (last_consolidated_at nulls first)
    where merged_into is null;

create table entity_alias (
    entity_id  uuid not null references entities (id) on delete cascade,
    name       text not null,
    -- Where the name came from: 'merge' when it arrived with a merged
    -- entity, 'observed' when an extraction used it for this entity.
    source     text not null,
    created_at timestamptz not null default now(),
    primary key (entity_id, name)
);

create index entity_alias_name_idx on entity_alias using gin (name gin_trgm_ops);

-- Every entity is an alias of itself. Written so that resolution and
-- search can read one table instead of a table and a column, and so that
-- a merge only has to move rows rather than invent them.
insert into entity_alias (entity_id, name, source)
select id, name, 'observed' from entities
on conflict do nothing;

create table entity_merge_block (
    -- Ordered pair, smaller id first, so a pair has exactly one row
    -- whichever direction it is discovered from.
    lower_id   uuid not null references entities (id) on delete cascade,
    higher_id  uuid not null references entities (id) on delete cascade,
    -- 'unmerged' (a person took a merge back) or 'different' (a judge
    -- read both and said no).
    reason     text not null,
    decided_at timestamptz not null default now(),
    primary key (lower_id, higher_id),
    check (lower_id < higher_id)
);
