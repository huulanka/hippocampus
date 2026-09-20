import { useEffect, useState } from "react";
import { getResurfaced, type Resurfaced, type ThreadItem, type UpcomingItem } from "../api";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";

/// The only screen that shows you something you did not ask for.
///
/// Everything else in the app waits: capture waits for words, search
/// waits for a query, the entity index waits for a name you already
/// remember. This is where the system speaks first.
export function ResurfaceScreen({
  onOpenCapture,
  onOpenEntity,
}: {
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  const [data, setData] = useState<Resurfaced | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getResurfaced()
      .then(setData)
      .catch((err) => setError(String(err)));
  }, []);

  if (error) return <p className="dim">Couldn't load that: {error}</p>;
  if (!data) return <p className="dim">Loading…</p>;

  return (
    <div className="detail">
      <section className="panel detail-panel">
        <div className="kicker">
          [ YOU SAID THIS WAS COMING ]{" "}
          <span className="dim">— the next 30 days, from your own notes</span>
        </div>
        {data.upcoming.length === 0 ? (
          <p className="dim detail-note">
            Nothing ahead. Notes only land here when they name a time — "morgen", "nächsten
            Dienstag", "im Oktober".
          </p>
        ) : (
          <div className="upcoming">
            {data.upcoming.map((item, index) => (
              <Upcoming
                key={`${item.capture_event_id}-${item.entity_id}-${index}`}
                item={item}
                onOpenCapture={onOpenCapture}
                onOpenEntity={onOpenEntity}
              />
            ))}
          </div>
        )}
      </section>

      <section className="panel detail-panel">
        <div className="kicker">
          [ YOU KEEP COMING BACK TO THIS ]{" "}
          <span className="dim">— subjects more than one capture has touched</span>
        </div>
        {data.threads.length === 0 ? (
          <p className="dim detail-note">
            Nothing has come up twice yet. This fills itself in as you keep capturing.
          </p>
        ) : (
          <div className="detail-entities threads">
            {data.threads.map((thread) => (
              <Thread key={thread.entity_id} thread={thread} onOpenEntity={onOpenEntity} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

function Upcoming({
  item,
  onOpenCapture,
  onOpenEntity,
}: {
  item: UpcomingItem;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  return (
    <div className="upcoming-row">
      <span className="when-badge upcoming-when">{whenLabel(item) ?? item.happened_on}</span>
      <div className="upcoming-body">
        <p className="detail-observation upcoming-observation">{item.observation}</p>
        <div className="upcoming-meta">
          <span className="link" onClick={() => onOpenEntity(item.entity_id)}>
            <span
              className="entity-dot"
              style={{ background: entityColor(item.entity_type) }}
            />{" "}
            {item.entity_name}
          </span>
          <span className="dim link" onClick={() => onOpenCapture(item.capture_event_id)}>
            said {agoLabel(item.said_at)}
          </span>
        </div>
      </div>
    </div>
  );
}

function Thread({
  thread,
  onOpenEntity,
}: {
  thread: ThreadItem;
  onOpenEntity: (id: string) => void;
}) {
  return (
    <div className="detail-entity detail-entity-link" onClick={() => onOpenEntity(thread.entity_id)}>
      <div className="entity-card-head">
        <span className="entity-dot" style={{ background: entityColor(thread.entity_type) }} />
        <span className="entity-name detail-entity-name">{thread.name}</span>
        <span className="dim entity-type-label">{thread.entity_type.toUpperCase()}</span>
      </div>
      <p className="dim entity-meta thread-meta">
        {thread.capture_count} captures · last {agoLabel(thread.last_seen)}
      </p>
      {thread.current_summary && (
        <p className="detail-observation thread-summary">{thread.current_summary}</p>
      )}
    </div>
  );
}

/// Relative rather than absolute: "three weeks ago" is the fact that
/// matters about a thread, and a date makes the reader do the arithmetic.
function agoLabel(iso: string): string {
  const days = Math.round((Date.now() - new Date(iso).getTime()) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 31) return `${Math.round(days / 7)} weeks ago`;
  if (days < 365) return `${Math.round(days / 30)} months ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", year: "numeric" });
}
