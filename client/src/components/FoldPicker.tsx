import { useEffect, useRef, useState } from "react";
import { getEntity, getFoldCandidates, type EntityDetail, type EntityListItem } from "../api";
import { entityColor } from "../entityType";

/// Something an entity can be folded into. The graph's own nodes carry
/// this much, and so does a row from the candidate list — the target does
/// not have to be anywhere in the picture.
export type FoldTarget = { id: string; name: string; entity_type: string };

/// How long typing has to pause before the candidates are asked again.
const TYPE_PAUSE_MS = 160;
/// How much of each side the comparison quotes. Enough to recognise the
/// thing; the entity page is one click away for the rest.
const QUOTED = 2;

/// Choosing what to fold an entity into, and then being shown both.
///
/// Used to be "click the target in the graph", which only worked for a
/// target already on screen — and a duplicate almost never is: two names
/// for one thing were never said together, so they are not neighbours.
/// Walking to it re-centred the graph and the entity being folded left the
/// picture. The choice is made here now and the graph stays where it is.
export function FoldPicker({
  source,
  target,
  merging,
  onTarget,
  onConfirm,
  onCancel,
}: {
  source: FoldTarget;
  target: FoldTarget | null;
  merging: boolean;
  onTarget: (target: FoldTarget | null) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <article className="graph-merge">
      {target ? (
        <Compare source={source} target={target} merging={merging} onTarget={onTarget} onConfirm={onConfirm} onCancel={onCancel} />
      ) : (
        <Choose source={source} onTarget={onTarget} onCancel={onCancel} />
      )}
    </article>
  );
}

function Choose({
  source,
  onTarget,
  onCancel,
}: {
  source: FoldTarget;
  onTarget: (target: FoldTarget) => void;
  onCancel: () => void;
}) {
  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<EntityListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => input.current?.focus(), []);

  useEffect(() => {
    let live = true;
    const timer = window.setTimeout(
      () => {
        getFoldCandidates(source.id, query)
          .then((found) => {
            if (!live) return;
            setCandidates(found);
            setError(null);
          })
          .catch((err) => live && setError(String(err)));
      },
      // The first answer — the look-alikes — should not wait for a pause
      // in typing nobody has started.
      query ? TYPE_PAUSE_MS : 0,
    );
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [source.id, query]);

  return (
    <>
      <p className="graph-merge-ask">
        Fold <strong>{source.name}</strong> into…
      </p>
      <div className="field">
        <label className="sr-only" htmlFor="fold-into">
          Find what {source.name} should be folded into
        </label>
        <input
          id="fold-into"
          ref={input}
          className="input"
          type="search"
          value={query}
          placeholder="Any name"
          autoComplete="off"
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") onCancel();
            if (event.key === "Enter" && candidates && candidates.length > 0) onTarget(candidates[0]);
          }}
        />
      </div>
      {error && <p className="graph-merge-why">Couldn't look: {error}</p>}
      {candidates && candidates.length === 0 && (
        <p className="graph-merge-why">
          {query ? "Nothing by that name." : "Nothing looks like it. Type a name."}
        </p>
      )}
      {candidates && candidates.length > 0 && (
        <ul className="fold-candidates" aria-label={query ? "Found" : "Looks like it"}>
          {candidates.map((candidate) => (
            <li key={candidate.id}>
              <button type="button" className="fold-candidate" onClick={() => onTarget(candidate)}>
                <span className="chip-dot" style={{ background: entityColor(candidate.entity_type) }} />
                <span className="fold-candidate-name">{candidate.name}</span>
                <span className="meta">
                  {candidate.entity_type} · {candidate.mention_count}×
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="graph-focus-actions">
        <button type="button" className="btn btn-quiet" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </>
  );
}

function Compare({
  source,
  target,
  merging,
  onTarget,
  onConfirm,
  onCancel,
}: {
  source: FoldTarget;
  target: FoldTarget;
  merging: boolean;
  onTarget: (target: FoldTarget | null) => void;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <>
      <p className="graph-merge-ask">Are these the same thing?</p>
      <Side entity={source} />
      <p className="fold-arrow label-micro" aria-hidden="true">
        ↓ folds into
      </p>
      <Side entity={target} />
      <p className="graph-merge-why">
        Everything said about “{source.name}” moves across, and the name stays searchable. You
        can take this back.
      </p>
      <div className="graph-focus-actions">
        <button type="button" className="btn btn-primary" disabled={merging} onClick={onConfirm}>
          {merging ? "Folding…" : `Yes, keep ${target.name}`}
        </button>
        <button type="button" className="btn btn-secondary" onClick={() => onTarget(null)}>
          Pick another
        </button>
        <button type="button" className="btn btn-quiet" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </>
  );
}

/// One side of the comparison: the name, and what was actually said
/// about it. A name alone is what made the duplicate in the first place.
function Side({ entity }: { entity: FoldTarget }) {
  const [detail, setDetail] = useState<EntityDetail | null>(null);

  useEffect(() => {
    let live = true;
    setDetail(null);
    getEntity(entity.id)
      .then((loaded) => live && setDetail(loaded))
      .catch(() => {
        // The comparison is a help, not a gate: without it the name and
        // type are still there and the fold still works.
      });
    return () => {
      live = false;
    };
  }, [entity.id]);

  return (
    <section className="fold-side">
      <span className="graph-focus-head">
        <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
        <span className="fold-side-name">{entity.name}</span>
        <span className="meta">
          {entity.entity_type}
          {detail && ` · ${detail.mentions.length}×`}
        </span>
      </span>
      {detail?.mentions.slice(0, QUOTED).map((mention) => (
        <p key={mention.capture_event_id} className="fold-quote">
          {mention.observation}
        </p>
      ))}
    </section>
  );
}
