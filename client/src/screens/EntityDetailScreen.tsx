import { useEffect, useMemo, useState } from "react";
import { getEntity, type EntityCapture, type EntityDetail, type EntityEdge } from "../api";
import { BackButton } from "../components/BackButton";
import { IntentionCard, Sparkle } from "../components/Intention";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";

/// One thing, and everything ever said about it.
///
/// The observation the model wrote sits next to the sentence it came
/// from, never instead of it. A summary that cannot be checked against
/// the words it summarises is a rumour. So the spoken words are the large
/// type here, in the serif that means "yours" everywhere else in the app,
/// and the model's reading of them is the small green line underneath.
export function EntityDetailScreen({
  entityId,
  onOpenEntity,
  onOpenCapture,
  onBack,
}: {
  entityId: string;
  onOpenEntity: (id: string) => void;
  onOpenCapture: (eventId: string) => void;
  onBack: () => void;
}) {
  const [entity, setEntity] = useState<EntityDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setEntity(null);
    setError(null);
    getEntity(entityId)
      .then((e) => current && setEntity(e))
      .catch((err) => current && setError(String(err)));
    return () => {
      current = false;
    };
  }, [entityId]);

  const relations = useMemo(() => (entity ? mergeRelations(entity.relations) : []), [entity]);

  if (error || !entity) {
    return (
      <div className="column entity">
        <BackButton onBack={onBack} />
        <p className="entity-note">{error ? `Couldn't load that entity: ${error}` : "Loading…"}</p>
      </div>
    );
  }

  const count = entity.mentions.length;

  return (
    <div className="column entity">
      <BackButton onBack={onBack} />

      <header className="entity-head">
        <span className="entity-kind">
          <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
          <span className="label-micro">
            {entity.entity_type} · {count} {count === 1 ? "capture" : "captures"} · {relations.length}{" "}
            {relations.length === 1 ? "relation" : "relations"}
          </span>
        </span>
        <h1 className="entity-name-large">{entity.name}</h1>
        {entity.current_summary && (
          /* Not called a summary on purpose: `current_summary` is
             overwritten by whichever observation came last, so calling it
             a summary would promise a consolidation that has not happened.
             See docs/memory-model.md. It is labelled for what it is. */
          <p className="entity-latest">
            <span className="derived-dot" aria-hidden="true" />
            <span>
              <span className="sr-only">Most recently observed: </span>
              {entity.current_summary}
            </span>
          </p>
        )}
        <p className="meta">first noticed {longDate(entity.created_at)}</p>
      </header>

      {spansDays(entity.mentions) && <MentionStrip mentions={entity.mentions} />}

      {/* First after the header: of everything on this page, what you
          still mean to do about it is the one part that asks something of
          you. */}
      {(entity.intentions?.length ?? 0) > 0 && (
        <section className="entity-section">
          <h2 className="label-micro intention-label">
            <Sparkle size={10} /> You meant to
          </h2>
          <div className="stack stack-tight">
            {entity.intentions!.map((intention) => (
              <IntentionCard
                key={intention.id}
                intention={intention}
                onOpenEntity={(id) => id !== entity.id && onOpenEntity(id)}
                onOpenCapture={onOpenCapture}
              />
            ))}
          </div>
        </section>
      )}

      {relations.length > 0 && (
        <section className="entity-section">
          <h2 className="label-micro">How it hangs together — proposed by a model</h2>
          <ul className="entity-relations">
            {relations.map((relation) => (
              <li key={relation.key} className="entity-relation">
                <button
                  type="button"
                  className="btn-quiet entity-relation-open"
                  onClick={() => onOpenEntity(relation.otherId)}
                >
                  <span className="entity-relation-type">
                    {relation.outgoing ? `${relation.type} →` : `← ${relation.type}`}
                  </span>
                  <span className="entity-relation-name">
                    <span className="chip-dot" style={{ background: entityColor(relation.otherType) }} />
                    {relation.otherName}
                  </span>
                </button>
                {/* An edge with no single capture behind it was drawn by the
                    consolidation run from several notes at once. Saying so
                    is more honest than a "why" that opens nothing — and it
                    is the visible sign that the graph now links things that
                    were never said in one breath. */}
                {relation.source ? (
                  <button
                    type="button"
                    className="btn-quiet entity-relation-why"
                    onClick={() => onOpenCapture(relation.source!)}
                    title="Open the note this was read out of"
                  >
                    why
                  </button>
                ) : (
                  <span className="entity-relation-why derived" title="Drawn by the consolidation run from several notes together">
                    across notes
                  </span>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="entity-section">
        <h2 className="label-micro">What you actually said — newest first</h2>
        {count === 0 ? (
          <p className="entity-note">
            Nothing is on record. This one exists only as the far end of a relation.
          </p>
        ) : (
          <div className="stack stack-tight">
            {entity.mentions.map((mention) => (
              <button
                type="button"
                key={`${mention.capture_event_id}-${mention.observation}`}
                className="card entity-said"
                onClick={() => onOpenCapture(mention.capture_event_id)}
              >
                <span className="meta entity-said-when">
                  {stamp(mention.occurred_at)}
                  {whenLabel(mention) && <span className="entity-said-about"> · about {whenLabel(mention)}</span>}
                </span>
                <span className="entity-said-words">
                  {mention.transcript_text || "(the words were removed)"}
                </span>
                <span className="entity-said-reading">
                  <span className="derived-dot" aria-hidden="true" />
                  <span>
                    <span className="sr-only">Read as: </span>
                    {mention.observation}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

/// When it came up, as marks along a line.
///
/// A count says how often; this says *when* — a thing mentioned nine
/// times in one week and a thing mentioned nine times over a year are
/// different subjects, and only the spacing tells them apart.
function MentionStrip({ mentions }: { mentions: EntityCapture[] }) {
  const times = mentions.map((m) => Date.parse(m.occurred_at)).sort((a, b) => a - b);
  const first = times[0];
  const last = times[times.length - 1];
  const span = Math.max(last - first, 1);
  // A little air at either end so the first and last mark are not cut by
  // the edge of the box.
  const x = (t: number) => 1.5 + ((t - first) / span) * 97;

  return (
    <section className="entity-section">
      <div className="entity-strip-head">
        <h2 className="label-micro">When you talked about it</h2>
        <span className="meta">
          {shortDate(first)}
          {last - first > 86_400_000 && ` — ${shortDate(last)}`}
        </span>
      </div>
      <svg
        className="entity-strip"
        viewBox="0 0 100 30"
        preserveAspectRatio="none"
        role="img"
        aria-label={`${mentions.length} mentions between ${shortDate(first)} and ${shortDate(last)}`}
      >
        <line x1="0" y1="26" x2="100" y2="26" className="entity-strip-axis" />
        {times.map((t, index) => (
          <line key={index} x1={x(t)} x2={x(t)} y1="26" y2="6" className="entity-strip-mark" />
        ))}
      </svg>
    </section>
  );
}

/// Worth drawing only when there is a "when" to show. Four mentions in
/// the same ten minutes spread across the whole width would draw a
/// history that is not there.
function spansDays(mentions: EntityCapture[]): boolean {
  if (mentions.length < 2) return false;
  const times = mentions.map((m) => Date.parse(m.occurred_at));
  return Math.max(...times) - Math.min(...times) > 2 * 86_400_000;
}

interface MergedRelation {
  key: string;
  type: string;
  outgoing: boolean;
  otherId: string;
  otherName: string;
  otherType: string;
  /// One capture it was read from, if any was. Several notes saying the
  /// same thing are one relation, not several rows.
  source: string | null;
}

/// One row per thing and wording. The API returns an edge per capture it
/// was read out of, so the same "Lena — besprochen" came back once
/// for every note that said it and the list repeated itself.
function mergeRelations(edges: EntityEdge[]): MergedRelation[] {
  const merged = new Map<string, MergedRelation>();
  for (const edge of edges) {
    const key = `${edge.outgoing ? "out" : "in"}|${edge.relation_type}|${edge.other_id}`;
    const held = merged.get(key);
    if (held) {
      held.source ??= edge.source_event_id;
      continue;
    }
    merged.set(key, {
      key,
      type: edge.relation_type,
      outgoing: edge.outgoing,
      otherId: edge.other_id,
      otherName: edge.other_name,
      otherType: edge.other_type,
      source: edge.source_event_id,
    });
  }
  return Array.from(merged.values());
}

function stamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function shortDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

function longDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
}
