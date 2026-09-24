# Entity resolution

How the system tells a duplicate from a relation, and in what order it
tries. [`consolidation.md`](consolidation.md) says *why* and *what the run
may change*; this document says *how*.

The problem in one sentence: you capture "coffee" once and "espresso"
once, and the two nodes stay separate; the same happens with typos, or
when a colleague is "Paul" in one note and "Paul Hartmann" in the next.

## No threshold separates the cases

Trigram similarity (`pg_trgm`) for a handful of typical pairs:

| Pair | Similarity | What it is |
| --- | --- | --- |
| Sauna (place) ↔ Sauna (activity) | 1.00 | duplicate |
| Hippocampus-Projekt ↔ Hippocampus Projekt | 1.00 | duplicate |
| Espresso mit Kardamom ↔ Kardamom-Espresso | 0.82 | duplicate |
| Hippocampus ↔ Hippocampus Projekt | 0.60 | duplicate |
| **Kardamom-Espresso ↔ Espresso** | **0.50** | **relation** (a kind of) |
| **Kardamom-Espresso ↔ Kardamom** | **0.50** | **relation** (ingredient) |
| **Hafenportal ↔ HPortal** | **0.43** | **nothing** (two projects) |
| Aufguss ↔ Finnischer Aufguss | 0.42 | duplicate |
| **Cardamom Buns ↔ Kardamom** | **0.35** | **relation** (ingredient) |
| **Northwind Abrechnungsprojekt ↔ Northwind** | **0.34** | **relation** (project for a customer) |

**This is the central design decision.** No threshold separates these
groups: `Kardamom-Espresso ↔ Espresso` (0.50) must *not* be merged and sits
**above** `Aufguss ↔ Finnischer Aufguss` (0.42), which must be. And
`Hafenportal ↔ HPortal`, two unrelated projects, sits above both.

It has the same shape as the echo problem this project already had: with
cosine similarity, a note about cardamom buns ranked above a note about a
sauna infusion when the query was about the sauna. The fix then was not a
better threshold but a model **that reads both texts** (ADR 0010). The
same applies here:

> **Similarity produces candidates. A reader has to decide.**

### Four verdicts, not two

The model gets both entities with their observations and answers with one
of four verdicts:

| Verdict | Consequence | Example |
| --- | --- | --- |
| `same` | merge, the other name stays as an alias | Aufguss ↔ Finnischer Aufguss |
| `narrower` | an `is_a` edge, both stay | Kardamom-Espresso → Espresso |
| `related` | a generic edge, both stay | Northwind Abrechnungsprojekt → Northwind |
| `different` | **recorded**, never asked again | Hafenportal ↔ HPortal |

`different` is stored, not discarded; otherwise every run pays again to
find out the same non-result.

### Why coffee and espresso do *not* merge

Because afterwards you would no longer know whether an espresso or filter
coffee was meant. In a memory system that is a worse loss than the
fragmentation it fixes, and practically irreversible, because a wrongly
merged graph *looks coherent*. The same applies to Sauna and Finnischer
Aufguss: they belong together, which makes them an edge, not one node.

## Further decisions

**Every merge is an event and can be undone.** Undoing only helps if the
mistake is noticed, so whatever the run changed has to be visible and
reversible in one move. That is why the run shows a preview first and
keeps a change log (see `consolidation.md`).

**Naming:** the same verdict that merges also chooses the surviving name,
usually the fuller form. **Every earlier spelling stays searchable as an
alias.** Otherwise fragmentation is traded for unfindability: "Paul" would
no longer find the colleague once the node is called "Paul Hartmann".

**Order: prevent first, then repair.** Several duplicate pairs only exist
because of the freely invented type vocabulary, and "Espresso mit
Kardamom" appears next to "Kardamom-Espresso" because the extraction
doesn't know what already exists. What doesn't fall apart in the first
place doesn't need merging.

## Stage 1: resolving at write time

*Prevents new fragmentation. Takes effect from the next note.*

Before this stage, `structuring.rs` looked up `entity_type = $1 and
lower(name) = lower($2)` before creating anything: exact name **and**
exact type. The extraction prompt contained nothing about the existing
graph, so the model invented names and types afresh for each note.

1. **The type vocabulary goes into the prompt.** The existing entries from
   `entity_type_registry` are sent along, with the instruction to use one
   of them if it fits. This scales: the type list stays small even with
   thousands of entities.
2. **Known neighbours go into the prompt.** Candidates via
   `word_similarity(entity.name, transcript)`, which is exactly the
   `pg_trgm` operator for "does this name appear somewhere in this text,
   even misspelled", plus the most recently touched entities, because one
   sitting tends to be about the same things. Capped at about forty
   entries (name, type, short summary).
3. **Fuzzy resolution when creating.** The same name under a *different*
   type resolves to the same entity, and so does anything above a high
   trigram threshold. Below it, a new entity is created and the later
   stages take care of it.
4. **Entity embeddings are actually written.** `entities.embedding` had
   been declared with an HNSW index since migration 0001 but was never
   filled. The later stages need it, and the candidates in step 2 get
   better with it.

*Done when:* two notes about the same thing in different wording land on
one entity, and a new note doesn't invent a type that already exists.

### What it showed

Test notes worded so that the old exact comparison would certainly have
created a new node ("Paul Hartmann" for an existing "Paul", "Kardamom
Espresso" without the hyphen, "Hippocampus-Projekt" with one) all resolved
to the existing entities. No new nodes.

The notable part is *which* mechanism did it: none of the fuzzy fallbacks
in `resolve_existing` fired. The model itself returned the existing names
because it saw them in the prompt. Resolution happens at the source, not
in the repair, which is also the cheaper place.

The fallback is still calibrated correctly: pure spelling differences
reach a trigram similarity of 1.00 (`Hippocampus-Projekt` ↔ `Hippocampus
Projekt`) and resolve. Reordered words like `Espresso mit Kardamom` ↔
`Kardamom Espresso` stay below at 0.82 and go to the verdict in the later
stages, which is right: a reordered phrase is a decision, not a spelling
variant.

Stage 1 deliberately does **not** repair the existing archive. That is
what the following stages are for.

## Stages 2 to 6: the consolidation run

Rather than a fixed list of pairs or a hand-written type mapping, a run
looks at the entities periodically and merges them on its own, guided by a
careful system prompt. A fixed list describes a life only until that life
changes; the run derives the vocabulary itself and may coin new types and
new relation words.

### What exists

| | |
| --- | --- |
| `entity_alias` | every name an entity ever had stays searchable |
| `entities.merged_into` | merging never deletes, it points |
| `entities.last_consolidated_at` | `null` means never looked at |
| `entity_merge_block` | a merge that was taken back is never proposed again |
| `consolidation.rs` | an exact pass, then a model pass, then the marker |
| `GET /consolidation` | the change log |
| `GET /consolidation/preview` | the same run, changing nothing |
| `POST /consolidation/apply` | apply the preview, minus what was unticked |
| `POST /consolidation/run` | an immediate pass without preview, for scripts |
| `GET /entities/{id}/fold-candidates` | likely duplicates, or a search by name and alias |
| `POST /entities/{id}/merge` | by hand |
| `POST /entities/{id}/unmerge` | take it back |
| `DELETE /relations/{id}` | take an edge back |

The working set is not a time window but the affected part of the graph,
as `consolidation.md` requires: the entities that are due
(`last_consolidated_at` null first, then oldest) plus their neighbourhood
by name similarity *and* embedding proximity. Both, because they fail
differently: trigrams miss "Sauna" next to "Aufguss", embeddings miss a
typo in a proper noun.

### The first dry run found a mistake, which is what it was for

The model wanted to merge a customer's project into the customer itself,
reasoning that both "describe the same project at the customer", which is
an argument for an edge and against a merge in the same sentence. The
prompt forbids exactly that, and it happened anyway.

That led to a rule that lives in code and not just in the prompt
(`crosses_types`): **two differently named things of different types are
never merged automatically.** The two legitimate forms survive: same name,
different type (`Sauna` as `Ort` and `Aktivität`), and different name,
same type (`Hippocampus` and `Hippocampus Projekt`). The rejected merge
became the right answer instead: an edge from the project to the customer.

The real gain is the edges across notes, because they used to be
*impossible*: `structuring.rs` can only connect what was said in one
sentence. For that, `relations.source_event_id` became nullable
(migration 0008). An edge read out of several notes has no single source,
and inventing one would be a lie about provenance in exactly the table
where provenance is the point. The interface shows `[across notes]` there
instead of `[why]`.

### By hand

In the graph: select a node, choose "Fold into…", pick the target from a
list (the likely duplicates are suggested without typing, and typing
searches names and aliases), check both sides, confirm. The target is
chosen in the panel rather than by clicking in the graph, because a
duplicate is almost never in the same orbit: two names for the same thing
were never said together, so they aren't neighbours.

The automatic run's safeguards deliberately do **not** apply to a person:
`crosses_types` also rejects some correct merges, and whoever has both
entities in front of them has better information. An existing block is
lifted by a manual merge.

The candidate list sorts by name, then type, with embedding proximity only
breaking ties: the embedding model puts almost any pair of short names at
around 0.9 cosine, so it can't be the primary order.

## Still open

- **Stage 7**, a rolling summary per entity, is built as the gist
  (`docs/finding-again.md`). The consolidation run does not read it yet;
  that only matters once an entity no longer fits into a prompt with its
  observations.
- **Re-derivation** of the archive with the new extraction prompt (phase 2
  of the roadmap). Decided, not built yet.
