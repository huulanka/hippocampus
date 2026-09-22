import { useCallback, useEffect, useState } from "react";
import {
  getChangelog,
  getConsolidationPreview,
  runConsolidation,
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
  const [busy, setBusy] = useState<"preview" | "run" | null>(null);

  const load = useCallback(() => {
    getChangelog().then(setChanges).catch((err) => setError(String(err)));
  }, []);

  useEffect(load, [load]);

  async function look() {
    setBusy("preview");
    setError(null);
    try {
      setPreview(await getConsolidationPreview());
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  async function tidy() {
    setBusy("run");
    setError(null);
    try {
      await runConsolidation();
      setPreview(null);
      load();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(null);
    }
  }

  async function undo(id: string) {
    try {
      setChanges(await unmergeEntity(id));
    } catch (err) {
      setError(String(err));
    }
  }

  const nothingToDo =
    preview !== null &&
    preview.same_name.length === 0 &&
    preview.merges.length === 0 &&
    preview.retypes.length === 0 &&
    preview.relations.length === 0;

  return (
    <div className="changes-screen">
      <h3>How this got organised</h3>
      <p className="dim changes-lede">
        Your notes are never changed. This is everything that happened to the way they are
        arranged — and every bit of it can be taken back.
      </p>

      <div className="capture-actions changes-actions">
        <span className={`btn${busy ? " disabled" : ""}`} onClick={() => void look()}>
          [ {busy === "preview" ? "Looking…" : "What would it change?"} ]
        </span>
        <span className={`btn btn-accent${busy ? " disabled" : ""}`} onClick={() => void tidy()}>
          [ {busy === "run" ? "Tidying…" : "Tidy up now"} ]
        </span>
      </div>

      {error && <p className="dim">{error}</p>}

      {/* The dry run. Same reasoning as the real pass, applied and thrown
          away — which is what makes it safe to point at a database this
          has never run against. */}
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

          {[...preview.same_name, ...preview.merges].map((m, i) => (
            <p key={`m${i}`} className="changes-proposal">
              Fold <strong>{m.absorb.join(", ")}</strong> into <strong>{m.keep}</strong>
              {m.new_name && m.new_name !== m.keep && <> and call it <strong>{m.new_name}</strong></>}
              <span className="dim"> — {m.reason}</span>
            </p>
          ))}
          {preview.retypes.map((r, i) => (
            <p key={`t${i}`} className="changes-proposal">
              <strong>{r.entity}</strong>: {r.from} → {r.to}
              <span className="dim"> — {r.reason}</span>
            </p>
          ))}
          {preview.relations.map((r, i) => (
            <p key={`r${i}`} className="changes-proposal">
              <strong>{r.from}</strong> <span className="dim">──{r.relation_type}──▶</span>{" "}
              <strong>{r.to}</strong>
              <span className="dim"> — {r.reason}</span>
            </p>
          ))}
        </div>
      )}

      {changes === null ? (
        <p className="dim">Reading the log…</p>
      ) : changes.length === 0 ? (
        <p className="dim">
          Nothing has been rearranged yet. Once the tidying run has done something, it shows up
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
              {/* Only a merge has an undo: a name, a type or an edge is
                  changed again by changing it, but a merge has a thing on
                  the other side of it that has to be given back. */}
              {change.undo_id &&
                (change.undone ? (
                  <span className="dim change-undone">taken back</span>
                ) : (
                  <span className="dim link change-undo" onClick={() => void undo(change.undo_id!)}>
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
