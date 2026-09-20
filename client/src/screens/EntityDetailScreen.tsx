import { useEffect, useState } from "react";
import { getEntity, type EntityDetail } from "../api";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";

/// One thing, and everything ever said about it.
///
/// The observation the model wrote sits next to the sentence it came
/// from, never instead of it. A summary that cannot be checked against
/// the words it summarises is a rumour.
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

  if (error)
    return (
      <div className="detail">
        <Back onBack={onBack} />
        <p className="dim">Couldn't load that entity: {error}</p>
      </div>
    );
  if (!entity)
    return (
      <div className="detail">
        <Back onBack={onBack} />
        <p className="dim">Loading…</p>
      </div>
    );

  return (
    <div className="detail">
      <div className="detail-head">
        <Back onBack={onBack} />
        <span className="dim detail-stamp">
          // first noticed{" "}
          {new Date(entity.created_at).toLocaleDateString(undefined, {
            day: "numeric",
            month: "short",
            year: "numeric",
          })}
        </span>
      </div>

      <section className="panel detail-panel">
        <div className="entity-card-head">
          <span
            className="entity-dot"
            style={{ background: entityColor(entity.entity_type) }}
          />
          <span className="dim entity-type-label">{entity.entity_type.toUpperCase()}</span>
        </div>
        <h2 className="entity-title">{entity.name}</h2>
        {entity.current_summary && (
          <>
            {/* Not called a summary on purpose: `current_summary` is
                overwritten by whichever observation came last, so calling
                it a summary would promise a consolidation that has not
                happened. See docs/issues.md. */}
            <p className="dim entity-title-label">// most recently observed</p>
            <p className="detail-observation entity-title-summary">{entity.current_summary}</p>
          </>
        )}
      </section>

      {entity.relations.length > 0 && (
        <section className="panel detail-panel">
          <div className="kicker">
            [ WHAT IT STANDS WITH ]{" "}
            <span className="dim">— proposed by a model; follow one to see why</span>
          </div>
          <div className="detail-relations entity-edges">
            {entity.relations.map((edge, index) => (
              <div key={`${edge.source_event_id}-${index}`} className="detail-relation">
                {edge.outgoing ? (
                  <>
                    <span className="detail-relation-node">{entity.name}</span>
                    <span className="dim detail-relation-type">──{edge.relation_type}──▶</span>
                    <span
                      className="detail-relation-node link"
                      onClick={() => onOpenEntity(edge.other_id)}
                    >
                      {edge.other_name}
                    </span>
                  </>
                ) : (
                  <>
                    <span
                      className="detail-relation-node link"
                      onClick={() => onOpenEntity(edge.other_id)}
                    >
                      {edge.other_name}
                    </span>
                    <span className="dim detail-relation-type">──{edge.relation_type}──▶</span>
                    <span className="detail-relation-node">{entity.name}</span>
                  </>
                )}
                <span className="dim entity-edge-source" onClick={() => onOpenCapture(edge.source_event_id)}>
                  [why]
                </span>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="panel detail-panel">
        <div className="kicker">
          [ EVERYTHING SAID ABOUT IT ]{" "}
          <span className="dim">
            — {entity.mentions.length}{" "}
            {entity.mentions.length === 1 ? "capture" : "captures"}, newest first
          </span>
        </div>
        {entity.mentions.length === 0 ? (
          <p className="dim detail-note">
            Nothing is on record. This one exists only as the far end of a relation.
          </p>
        ) : (
          <div className="card-stack entity-mentions">
            {entity.mentions.map((mention) => (
              <div
                key={`${mention.capture_event_id}-${mention.observation}`}
                className="timeline-card clickable"
                onClick={() => onOpenCapture(mention.capture_event_id)}
              >
                <div className="timeline-card-meta">
                  <span className="dim">// {stamp(mention.occurred_at)}</span>
                  <span className="dim card-open-hint">[open]</span>
                </div>
                <p className="detail-observation entity-mention-observation">
                  {mention.observation}
                </p>
                {whenLabel(mention) && <p className="when-badge">◷ {whenLabel(mention)}</p>}
                <p className="timeline-transcript entity-mention-verbatim">
                  {mention.transcript_text || "(the words were removed)"}
                </p>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function Back({ onBack }: { onBack: () => void }) {
  return (
    <span className="dim link" onClick={onBack}>
      [ ← back ]
    </span>
  );
}

function stamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
