-- Trigram matching, so a capture can be resolved against the graph that
-- already exists instead of adding to it blindly.
--
-- The extraction invents a name and a type per note with no knowledge of
-- what it has already called things. Measured on 78 real entities, that
-- produces exactly the fragmentation the user described:
--
--   Sauna (Ort)              ↔ Sauna (Aktivität)         1.00
--   Northwind (Organisation) ↔ Northwind (Kunde)     1.00
--   Kardamom-Espresso (Idee) ↔ Kardamom-Espresso (Getränk) 1.00
--   Espresso mit Kardamom    ↔ Kardamom-Espresso         0.82
--   Hippocampus              ↔ Hippocampus Projekt       0.60
--
-- `word_similarity(name, transcript)` is the operator this is for: it
-- asks how well a short string matches the best-matching *substring* of a
-- longer one, which is precisely "is this entity being talked about in
-- this note, however sloppily spelled".
--
-- Note what this does NOT decide. Similarity cannot tell a duplicate from
-- a relation: in the same data, `Kardamom-Espresso ↔ Espresso` scores
-- 0.50 — higher than `Aufguss ↔ Finnischer Aufguss` at 0.42 — and the
-- first must never merge while the second must. Similarity generates
-- candidates; a reader decides. See docs/entity-resolution.md.

create extension if not exists pg_trgm;

-- GIN rather than GiST: this index is read on every capture and written
-- only when an entity is created, which is the ratio GIN is for.
create index entities_name_trgm_idx on entities using gin (name gin_trgm_ops);
