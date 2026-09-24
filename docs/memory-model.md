# Memory model

Which concepts the system knows, which it deliberately does *not* know, and
how they map onto events and projections.

## Concepts

Only these six. Everything else is derived or presentation.

| Concept | Definition | Can it change? |
|---|---|---|
| **Capture** | One act of capturing: the recording itself (audio) or text typed in directly. | Never |
| **Transcript** | Text for a capture. The output of a speech model, and therefore already an interpretation. | New versions; old ones stay |
| **Entity** | Something that gets talked about repeatedly: a person, project, subject, place, recipe. | Its identity can be corrected |
| **Observation** | What a single capture says about an entity. Always tied to its source and the model that read it. | Never, only added to |
| **Relation** | A connection between two entities, with its source and model. | Proposed, then kept or taken back |
| **Echo** | The earlier captures close in meaning to a new one. | Judged once and remembered (ADR 0010) |

### Deliberately not modelled

- **Knowledge as a separate layer.** "What do I currently believe" is the
  sum of an entity's observations, in time order. An extra belief layer
  with confidence values would be guesswork without calibration data.
- **Decay.** Nothing fades or disappears. If needed, as a ranking factor.
- **Reconsolidation.** It means memory changes when it is recalled, which
  is the exact opposite of the core principle.
- **A state machine `NEW → CAPTURED → INTERPRETED → …`.** That is a
  boolean ("structured or not") dressed up as neuroscience.

## Layers

```
capture.recorded         audio or text: the original, never touched
      ↓
transcript.derived       speech model output: first interpretation, with model
transcript.corrected     a person overrides it; the original stays next to it
      ↓
entity.* / relation.*    LLM output: second interpretation, with model
      ↓
projections              entities, observations, relations, capture_search
```

Each layer knows its source and the model that produced it. Every layer
except the first can be rebuilt entirely from the one above. That is the
only reason this system uses event sourcing; without a re-derivation path
that actually exists, the `events` table would just be an audit log (see
ADR 0003).

## Event types

### Capturing
- `capture.recorded`: `{ origin: "audio"|"text", content_ref, device, duration_ms }`.
  Contains **no** text (ADR 0004).
- `transcript.derived`: `{ capture_event_id, model, language }`
- `transcript.corrected`: `{ capture_event_id, supersedes, corrected_by: "user" }`
- `capture.redacted`: `{ capture_event_id, scope: "content"|"all", reason? }`

### Derivation
- `entity.created`: `{ entity_type, name }`
- `entity.observed`: `{ text, source_event_id }`
- `relation.proposed`: `{ from_entity_id, to_entity_id, relation_type }`
- `structuring.invalidated`: written when a correction throws away the
  derived rows of the old wording. A full replay has to respect it, or it
  brings back a reading of a sentence that no longer exists.

### Identity
- `entity.merged`: `{ from_entity_id, into_entity_id, decided_by }`
- `entity.unmerged`: `{ merge_event_id }`. Every merge can be undone.

Consolidation adds its own events on top; see `docs/consolidation.md`.

## Where content lives, and why not in the event log

A database rule blocks UPDATE and DELETE on `events`. That would make
redaction (deliberately removing content) impossible, unless the rule is
loosened and every claim about immutability goes with it.

**The way out:** the event log holds only *structure and references*,
never content. Content lives in its own tables that point at the event ID:

```
events                 (immutable: what happened, when, by whom or which model)
capture_content        (event_id → audio_path | text)          ← redactable
transcript_content     (event_id → text, model, superseded_by)  ← redactable
```

Redaction empties the content in `capture_content` and
`transcript_content`, removes derived observations and appends a
`capture.redacted` event. The chain of events stays complete and
checkable; the content is gone. See ADR 0005.

## Projections

`entities`, `observations`, `relations`, `capture_search`,
`entity_type_registry`, plus the consolidation tables (`entity_alias`,
`entity_merge_block`).

- `capture_search.transcript` mirrors the **current** transcript version
  and is re-embedded on every correction.
- `entities.current_summary` is overwritten by the latest observation
  (last write wins). That is not a consolidated view, so the interface
  labels it "most recently observed" rather than as a summary. Either the
  field goes and the view is built from the observations when read, or it
  gets a real consolidation step.

## Entity identity

Exact string matching on name and type makes names fall apart into
unconnected fragments ("Lena", "Lena M.", speech recognition noise) at a
few thousand captures a year. The graph then isn't wrong, it's empty.

How it works now:

1. The extraction sees what already exists and reuses those names
   (`docs/entity-resolution.md`, stage 1).
2. The same name under a different type word resolves to the same entity.
3. Everything else is found through name similarity (`pg_trgm`) and
   embedding proximity, and decided by a model that reads both sides.
4. Every merge is an event and can be undone; a merge that was taken back
   is never proposed again.
