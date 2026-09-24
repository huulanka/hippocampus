import { useEffect, useMemo, useState } from "react";
import {
  listCaptures,
  listEntityTypes,
  search,
  type CaptureListItem,
  type EntityTypeCount,
  type SearchResult,
} from "../api";
import { entityColor } from "../entityType";
import { usePhoneLayout } from "../phone";

/// Finding something again, and — with the field empty — everything.
///
/// The empty state is the old Timeline. It was a place of its own, which
/// made it a third way of asking the one question this screen already
/// answers: where is the thing I said. Now the field starts empty and the
/// answer is your whole record, newest first, grouped by day; typing
/// narrows it. Nothing had to be dropped to get the tab count down.

/// How many type chips to offer. The extraction prompt invents types
/// freely, so the long tail is large and mostly one-off — showing every
/// type would bury the handful that are actually useful as filters.
const MAX_TYPE_CHIPS = 6;

export function SearchScreen({
  onOpenCapture,
  onOpenEntity,
}: {
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [everything, setEverything] = useState<CaptureListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [types, setTypes] = useState<EntityTypeCount[]>([]);
  const [activeType, setActiveType] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    listEntityTypes()
      .then((all) => live && setTypes(all.slice(0, MAX_TYPE_CHIPS)))
      .catch(() => live && setTypes([]));
    listCaptures()
      .then((all) => live && setEverything(all))
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
  }, []);

  async function run(q: string, entityType: string | null = activeType) {
    if (!q.trim()) {
      setResults(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setResults(await search(q, { entityType: entityType ?? undefined }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  const days = useMemo(() => groupByDay(everything ?? []), [everything]);
  const searching = results !== null;
  const phone = usePhoneLayout();

  return (
    <div className="column search">
      <div className="field search-field">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
          <circle cx="10.5" cy="10.5" r="6.4" />
          <path d="m15.2 15.2 4.4 4.4" />
        </svg>
        <label className="sr-only" htmlFor="search-field">
          Search everything you have said
        </label>
        <input
          id="search-field"
          className="input"
          type="search"
          // On a Mac the keyboard is already there; on a phone the cursor
          // would throw the on-screen keyboard over the list you came to
          // look at before you have asked for it.
          autoFocus={!phone}
          value={query}
          placeholder="Search everything you have said…"
          onChange={(event) => {
            const next = event.currentTarget.value;
            setQuery(next);
            if (!next.trim()) setResults(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") void run(query);
            if (event.key === "Escape") {
              setQuery("");
              setResults(null);
            }
          }}
        />
      </div>

      {types.length > 0 && (
        <div className="search-types">
          <button
            type="button"
            className="chip"
            aria-pressed={activeType === null}
            onClick={() => {
              setActiveType(null);
              void run(query, null);
            }}
          >
            Everything
          </button>
          {types.map((type) => (
            <button
              key={type.entity_type}
              type="button"
              className="chip"
              aria-pressed={activeType === type.entity_type}
              onClick={() => {
                const next = activeType === type.entity_type ? null : type.entity_type;
                setActiveType(next);
                void run(query, next);
              }}
            >
              <span className="chip-dot" style={{ background: entityColor(type.entity_type) }} />
              {type.entity_type}
              <span className="chip-count">({type.count})</span>
            </button>
          ))}
        </div>
      )}

      {loading && <p className="today-note">Looking…</p>}
      {error && <p className="today-note">That didn't work: {error}</p>}

      {searching ? (
        results.length === 0 ? (
          <p className="today-note">Nothing matches “{query}”.</p>
        ) : (
          <div className="stack stack-tight">
            {results.map((result, index) => (
              <article key={result.capture_event_id} className="card search-hit">
                <button
                  type="button"
                  className="search-hit-open"
                  onClick={() => onOpenCapture(result.capture_event_id)}
                >
                  <span className="search-hit-meta">
                    <span className="meta">{onlyDate(result.occurred_at)}</span>
                    {/* The rank, not the score. The score is a
                        reciprocal-rank-fusion sum — 0.03 is a *good* hit,
                        which reads as 3% to anyone who has ever seen a
                        percentage. A number nobody can interpret is worse
                        than no number. */}
                    <span className="meta">#{index + 1}</span>
                  </span>
                  <span className="prose search-hit-text">{result.transcript_text}</span>
                </button>
                {result.related_entities.length > 0 && (
                  <div className="search-hit-entities">
                    {result.related_entities.map((entity) => (
                      <button
                        key={entity.id}
                        type="button"
                        className="pill"
                        onClick={() => onOpenEntity(entity.id)}
                      >
                        <span
                          className="chip-dot"
                          style={{ background: entityColor(entity.entity_type) }}
                        />
                        {entity.name}
                        <span className="pill-type">{entity.entity_type}</span>
                      </button>
                    ))}
                  </div>
                )}
              </article>
            ))}
          </div>
        )
      ) : everything === null ? (
        !error && <p className="today-note">Fetching everything…</p>
      ) : everything.length === 0 ? (
        <p className="today-note">Nothing yet. Press Capture, or start a page under Write.</p>
      ) : (
        <>
          <p className="label-micro search-all">
            Everything you have said — {everything.length}{" "}
            {everything.length === 1 ? "note" : "notes"}
          </p>
          {days.map(([day, items]) => (
            <section key={day} className="search-day">
              <h2 className="label-micro search-day-label">{day}</h2>
              <div className="stack stack-tight">
                {items.map((item) => (
                  <button
                    key={item.event_id}
                    type="button"
                    className="card search-note"
                    onClick={() => onOpenCapture(item.event_id)}
                  >
                    <span className="meta">
                      {clock(item.occurred_at)} · {item.origin === "audio" ? "spoken" : "written"}
                    </span>
                    <span className="prose search-note-text">{item.transcript_text}</span>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </>
      )}
    </div>
  );
}

function dayLabel(iso: string): string {
  const date = new Date(iso);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(new Date()) - startOf(date)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: "long" });
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
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

function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

function onlyDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
