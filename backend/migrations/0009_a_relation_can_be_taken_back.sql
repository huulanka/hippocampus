-- A relation the consolidation run drew can turn out to be wrong, the same
-- way a merge can. Taking it back needs the same oscillation guard
-- `entity_merge_block` gives merges: without it, the next pass reads the
-- same two entities, reaches the same conclusion, and redraws the edge it
-- was just told was wrong.
--
-- Keyed by the triple rather than the relation's own id, because the row
-- being blocked is deleted outright (a relation carries no observation of
-- its own to preserve — unlike a merged entity, there is nothing to give
-- back later) and the block has to survive that deletion.

create table relation_block (
    from_entity_id  uuid not null references entities (id) on delete cascade,
    to_entity_id    uuid not null references entities (id) on delete cascade,
    relation_type   text not null,
    decided_at      timestamptz not null default now(),
    primary key (from_entity_id, to_entity_id, relation_type)
);
