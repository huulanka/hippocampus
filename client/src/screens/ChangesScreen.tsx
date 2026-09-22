import { useCallback, useEffect, useMemo, useState } from "react";
import {
  applyConsolidation,
  getChangelog,
  getConsolidationPreview,
  retractRelation,
  unmergeEntity,
  type ChangeRecord,
  type ConsolidationPreview,
} from "../api";

/// How the view of your knowledge came to look the way it does.
///
/// The user's own framing, and the reason this screen exists at all:
/// *"Ich habe unveränderliche Startprimitiven, Changelog auf die
/// Repräsentation meines Wissens."* Your notes are never touched. What
/// changes is how they are arranged — and an arrangement that rearranges
/// itself silently is not trustworthy however correct it happens to be.
///
/// So this is the other half of "the graph tidies itself". Without it,
/// "automatic and reversible" is only the first word: undoing something
/// requires noticing it, and nothing else in the app would ever have
/// mentioned it.
function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  const minutes = Math.round((Date.now() - then) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days < 30) return `${days} d ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

/// What each kind of change says, in the terms the person would use.
/// Deliberately not "entity 4f2a was merged": the whole value of a
/// changelog is that it can be checked against one's own memory.
function describe(change: ChangeRecord) {
  switch (change.kind) {
    case "merged":
      return (
        <>
          <strong>{change.before}</strong> is the same thing as{" "}
          <strong>{change.entity_name}</strong>, and was folded into it
        </>
      );
    case "renamed":
      return (
        <>
          <strong>{change.before}</strong> is now called{" "}
          <strong>{change.after}</strong>
        </>
      );
    case "retyped":
      return (
        <>
          <strong>{change.entity_name}</strong> was a {change.before}, now it is a{" "}
          {change.after}
        </>
      );
    default:
      return (
        <>
          <strong>{change.entity_name}</strong>{" "}
          <span className="dim">──{change.before}──▶</span>{" "}
          <strong>{change.after}</strong>
        </>
      );
  }
}

export function ChangesScreen({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [changes, setChanges] = useState<ChangeRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<ConsolidationPreview | null>(null);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"preview" | "apply" | null>(null);

  const load = useCallback(() => {
    getChangelog().then(setChanges).catch((err) => setError(String(err)));
  }, []);

  useEffect(load, [load]);

  async function look() {
    setBusy("preview");
    setError(null);
    try {
      setExcluded(new Set());
      setPreview(await getConsolidationPreview());
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  function exclude(id: string) {
    setExcluded((prev) => new Set(prev).add(id));
  }

  async function apply() {
    if (!preview?.token) return;
    setBusy("apply");
    setError(null);
    try {
      await applyConsolidation(preview.token, [...excluded]);
      setPreview(null);
      setExcluded(new Set());
      load();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  async function undo(change: ChangeRecord) {
    if (!change.undo_id) return;
    try {
      const updated =
        change.kind === "related"
          ? await retractRelation(change.undo_id)
          : await unmergeEntity(change.undo_id);
      setChanges(updated);
    } catch (err) {
      setError(String(err));
    }
  }

  const proposals = useMemo(() => {
    if (!preview) return [];
    return [
      ...preview.same_name.map((m) => ({ kind: "merge" as const, item: m })),
      ...preview.merges.map((m) => ({ kind: "merge" as const, item: m })),
      ...preview.retypes.map((r) => ({ kind: "retype" as const, item: r })),
      ...preview.relations.map((r) => ({ kind: "relation" as const, item: r })),
    ].filter((p) => !excluded.has(p.item.id));
  }, [preview, excluded]);

  const nothingToDo = preview !== null && proposals.length === 0 && excluded.size === 0;

  return (
    <div className="changes-screen">
      <h3>How this got organised</h3>
      <p className="dim changes-lede">
        Your notes are never changed. This is everything that happened to the way they are
        arranged — and every bit of it can be taken back.
      </p>

      <div className="capture-actions changes-actions">
        <span className={`btn${busy ? " disabled" : ""}`} onClick={() => void look()}>
          [ {busy === "preview" ? "Looking…" : "Check for changes"} ]
        </span>
        {preview?.token && (
          <span
            className={`btn btn-accent${busy || proposals.length === 0 ? " disabled" : ""}`}
            onClick={() => proposals.length > 0 && void apply()}
          >
            [ {busy === "apply" ? "Applying…" : `Apply ${proposals.length} change${proposals.length === 1 ? "" : "s"}`} ]
          </span>
        )}
      </div>

      {error && <p className="dim">{error}</p>}

      {/* The dry run. Same reasoning as the real pass, kept rather than
          thrown away this time — every item stays exactly as proposed
          until it is applied, so removing one never risks a second,
          different judgement of the rest. */}
      {preview && (
        <div className="panel transcript-panel">
          <div className="kicker">
            [ NOTHING HAS CHANGED YET ]{" "}
            <span className="dim">
              — {preview.considered} {preview.considered === 1 ? "entity" : "entities"} looked at
            </span>
          </div>

          {preview.note && <p className="dim">{preview.note}</p>}
          {nothingToDo && !preview.note && (
            <p className="dim">Nothing worth changing. That is a real answer, not an empty one.</p>
          )}
          {preview.token && proposals.length === 0 && excluded.size > 0 && (
            <p className="dim">Everything was removed. Nothing left to apply.</p>
          )}

          {proposals.map(({ kind, item }) => (
            <p key={item.id} className="changes-proposal">
              {kind === "merge" && "keep" in item && (
                <>
                  Fold <strong>{item.absorb.join(", ")}</strong> into <strong>{item.keep}</strong>
                  {item.new_name && item.new_name !== item.keep && (
                    <> and call it <strong>{item.new_name}</strong></>
                  )}
                  <span className="dim"> — {item.reason}</span>
                </>
              )}
              {kind === "retype" && "entity" in item && (
                <>
                  <strong>{item.entity}</strong>: {item.from} → {item.to}
                  <span className="dim"> — {item.reason}</span>
                </>
              )}
              {kind === "relation" && "relation_type" in item && (
                <>
                  <strong>{item.from}</strong> <span className="dim">──{item.relation_type}──▶</span>{" "}
                  <strong>{item.to}</strong>
                  <span className="dim"> — {item.reason}</span>
                </>
              )}
              <span className="dim link changes-proposal-remove" onClick={() => exclude(item.id)}>
                [x]
              </span>
            </p>
          ))}
        </div>
      )}

      {changes === null ? (
        <p className="dim">Reading the log…</p>
      ) : changes.length === 0 ? (
        <p className="dim">
          Nothing has been rearranged yet. Once a tidying pass has done something, it shows up
          here with its reasons.
        </p>
      ) : (
        <div className="card-stack">
          {changes.map((change, index) => (
            <div key={`${change.entity_id}-${change.changed_at}-${index}`} className="change-row">
              <span className={`change-kind ${change.kind}`}>{change.kind}</span>
              <div className="change-body">
                <p className="change-what">{describe(change)}</p>
                {change.reason && <p className="dim change-reason">{change.reason}</p>}
              </div>
              <span className="dim change-when">{relativeTime(change.changed_at)}</span>
              <span className="dim link change-open" onClick={() => onOpenEntity(change.entity_id)}>
                [open]
              </span>
              {/* A rename or a retype is changed again by changing it; a
                  merge or a derived edge has a thing on the other side of
                  it that has to be given back, which is what undo_id is
                  for. */}
              {change.undo_id &&
                (change.undone ? (
                  <span className="dim change-undone">taken back</span>
                ) : (
                  <span className="dim link change-undo" onClick={() => void undo(change)}>
                    [undo]
                  </span>
                ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
