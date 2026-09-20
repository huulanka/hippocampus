import { useEffect, useState } from "react";
import { listCaptures, type CaptureListItem } from "../api";

function dayLabel(iso: string): string {
  const date = new Date(iso);
  const now = new Date();
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const diffDays = Math.round((startOf(now) - startOf(date)) / 86_400_000);
  if (diffDays === 0) return "TODAY";
  if (diffDays === 1) return "YESTERDAY";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function timeLabel(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

function groupByDay(items: CaptureListItem[]): [string, CaptureListItem[]][] {
  const groups = new Map<string, CaptureListItem[]>();
  for (const item of items) {
    const label = dayLabel(item.occurred_at);
    const group = groups.get(label) ?? [];
    group.push(item);
    groups.set(label, group);
  }
  return Array.from(groups.entries());
}

// Deleting a raw capture isn't offered here on purpose: original transcripts
// are append-only by design (see docs/adr/0003) — there's no "delete event"
// yet, only a possible future redaction event. Export-only for now.
export function TimelineScreen() {
  const [captures, setCaptures] = useState<CaptureListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listCaptures()
      .then(setCaptures)
      .catch((err) => setError(String(err)));
  }, []);

  if (error) return <p className="dim">Couldn't load captures: {error}</p>;
  if (!captures) return <p className="dim">Loading…</p>;
  if (captures.length === 0) return <p className="dim">No captures yet. Start with [ New Capture ].</p>;

  return (
    <>
      {groupByDay(captures).map(([label, items]) => (
        <div key={label}>
          <h4 className="section-label">{label}</h4>
          <div className="card-stack">
            {items.map((item) => (
              <div key={item.event_id} className="panel timeline-card">
                <div className="timeline-card-meta">
                  <span className="dim">
                    // {timeLabel(item.occurred_at)} · verbatim
                  </span>
                </div>
                <p className="timeline-transcript">{item.transcript_text}</p>
                <div className="timeline-card-actions">
                  <span className="dim link">[Export]</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </>
  );
}
