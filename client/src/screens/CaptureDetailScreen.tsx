import { useEffect, useState } from "react";
import { audioUrl, getCapture, type CaptureDetail } from "../api";
import { AudioPlayer } from "../components/AudioPlayer";
import { entityColor } from "../entityType";

/// Everything known about one capture, on one page.
///
/// The ordering is deliberate: the user's own words first, then what the
/// machine made of them, then what it connects to, and only at the end —
/// folded away — the log that proves it. Anything derived is labelled with
/// the model that derived it, so a wrong entity is recognisable as the
/// model's mistake rather than the user's memory.
export function CaptureDetailScreen({
  eventId,
  onOpen,
  onBack,
}: {
  eventId: string;
  onOpen: (eventId: string) => void;
  onBack: () => void;
}) {
  const [detail, setDetail] = useState<CaptureDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setDetail(null);
    setError(null);
    getCapture(eventId)
      .then((d) => current && setDetail(d))
      .catch((err) => current && setError(String(err)));
    return () => {
      current = false;
    };
  }, [eventId]);

  if (error) return <DetailFrame onBack={onBack}>Couldn't load that capture: {error}</DetailFrame>;
  if (!detail) return <DetailFrame onBack={onBack}>Loading…</DetailFrame>;

  const spoken = detail.origin === "audio";
  const corrections = detail.transcripts.filter((t) => t.model === "user");

  return (
    <div className="detail">
      <div className="detail-head">
        <span className="dim link" onClick={onBack}>
          [ ← back ]
        </span>
        <span className="dim detail-stamp">
          // {fullStamp(detail.occurred_at)} · {spoken ? "spoken" : "typed"} on {detail.device}
        </span>
      </div>

      <section className="panel detail-panel">
        <div className="kicker">
          [ CAPTURED ]{" "}
          <span className="dim">
            {detail.redacted
              ? "— the content was removed; the capture itself stays"
              : spoken
                ? "— transcript; the recording below is the original"
                : "— raw, verbatim, never edited"}
          </span>
        </div>
        <p className="detail-transcript">{detail.text ?? "(redacted)"}</p>
        {detail.audio && <AudioPlayer src={audioUrl(detail.event_id)} />}
        {corrections.length > 0 && (
          <p className="dim detail-note">
            Corrected {corrections.length === 1 ? "once" : `${corrections.length} times`} — the
            original is kept below.
          </p>
        )}
      </section>

      <section className="panel detail-panel">
        <div className="kicker">
          [ WHAT IT MEANS ]{" "}
          <span className="dim">— drawn out by a model, not written by you</span>
        </div>
        {detail.entities.length === 0 ? (
          <p className="dim detail-note">
            Nothing was extracted from this one. Either the structuring hasn't run yet, or
            there was nothing in it worth remembering as a thing.
          </p>
        ) : (
          <div className="detail-entities">
            {detail.entities.map((entity) => (
              <div key={`${entity.id}-${entity.observation}`} className="detail-entity">
                <div className="entity-card-head">
                  <span
                    className="entity-dot"
                    style={{ background: entityColor(entity.entity_type) }}
                  />
                  <span className="entity-name detail-entity-name">{entity.name}</span>
                  <span className="dim entity-type-label">{entity.entity_type.toUpperCase()}</span>
                </div>
                <p className="detail-observation">{entity.observation}</p>
              </div>
            ))}
          </div>
        )}
        {detail.relations.length > 0 && (
          <div className="detail-relations">
            {detail.relations.map((relation) => (
              <div key={relation.id} className="detail-relation">
                <span className="detail-relation-node">{relation.from_name}</span>
                <span className="dim detail-relation-type">──{relation.relation_type}──▶</span>
                <span className="detail-relation-node">{relation.to_name}</span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="panel detail-panel">
        <div className="kicker">
          [ YOU'VE BEEN HERE BEFORE ]{" "}
          <span className="dim">— your own earlier words, not a summary</span>
        </div>
        {detail.echo.length === 0 ? (
          <p className="dim detail-note">
            Nothing close enough among your earlier captures.
          </p>
        ) : (
          <div className="card-stack detail-echo">
            {detail.echo.map((item) => (
              <div
                key={item.capture_event_id}
                className="timeline-card clickable"
                onClick={() => onOpen(item.capture_event_id)}
              >
                <div className="timeline-card-meta">
                  <span className="dim">// {shortStamp(item.occurred_at)}</span>
                </div>
                <p className="timeline-transcript">{item.transcript_text}</p>
              </div>
            ))}
          </div>
        )}
      </section>

      {detail.transcripts.length > 1 && (
        <section className="panel detail-panel">
          <div className="kicker">
            [ HOW THE WORDS CHANGED ]{" "}
            <span className="dim">— oldest first; nothing was overwritten</span>
          </div>
          {detail.transcripts.map((version) => (
            <div key={version.event_id} className="detail-version">
              <div className="timeline-card-meta">
                <span className="dim">// {shortStamp(version.created_at)}</span>
                <span className="dim">{version.model === "user" ? "you" : version.model}</span>
              </div>
              <p className="timeline-transcript">{version.text}</p>
            </div>
          ))}
        </section>
      )}

      <details className="panel detail-panel detail-raw">
        <summary className="kicker">
          [ UNDER THE HOOD ]{" "}
          <span className="dim">— {detail.events.length} events, exactly as stored</span>
        </summary>
        {detail.events.map((event) => (
          <div key={event.id} className="detail-event">
            <div className="timeline-card-meta">
              <span className="detail-event-type">{event.event_type}</span>
              <span className="dim">
                {shortStamp(event.occurred_at)} · {event.source}
              </span>
            </div>
            <pre className="detail-payload">{JSON.stringify(event.payload, null, 2)}</pre>
          </div>
        ))}
      </details>
    </div>
  );
}

function DetailFrame({ children, onBack }: { children: React.ReactNode; onBack: () => void }) {
  return (
    <div className="detail">
      <div className="detail-head">
        <span className="dim link" onClick={onBack}>
          [ ← back ]
        </span>
      </div>
      <p className="dim">{children}</p>
    </div>
  );
}

function fullStamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function shortStamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
