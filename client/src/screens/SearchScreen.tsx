import { useEffect, useState } from "react";
import { listEntityTypes, search, type EntityTypeCount, type SearchResult } from "../api";

/// How many type chips to offer. The extraction prompt invents types
/// freely, so the long tail is large and mostly one-off — showing every
/// type would bury the handful that are actually useful as filters.
const MAX_TYPE_CHIPS = 5;

export function SearchScreen() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [types, setTypes] = useState<EntityTypeCount[]>([]);
  const [activeType, setActiveType] = useState<string | null>(null);

  useEffect(() => {
    listEntityTypes()
      .then((all) => setTypes(all.slice(0, MAX_TYPE_CHIPS)))
      .catch(() => setTypes([]));
  }, []);

  async function runSearch(q: string, entityType: string | null = activeType) {
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

  function selectType(entityType: string | null) {
    setActiveType(entityType);
    runSearch(query, entityType);
  }

  return (
    <>
      <div className="search-box">
        <span className="kicker">&gt;</span>
        <input
          className="search-input"
          autoFocus
          value={query}
          placeholder="Search your captures…"
          onChange={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") runSearch(query);
          }}
        />
      </div>
      <div className="search-filters">
        <span
          className={`filter-chip${activeType === null ? " active" : ""}`}
          onClick={() => selectType(null)}
        >
          [{activeType === null ? "x" : " "}] All
        </span>
        {types.map((t) => (
          <span
            key={t.entity_type}
            className={`filter-chip${activeType === t.entity_type ? " active" : ""}`}
            onClick={() => selectType(t.entity_type)}
          >
            [{activeType === t.entity_type ? "x" : " "}] {t.entity_type}
          </span>
        ))}
      </div>

      {loading && <p className="dim">Searching…</p>}
      {error && <p className="dim">Search failed: {error}</p>}
      {results && results.length === 0 && <p className="dim">No matches.</p>}

      <div className="card-stack">
        {results?.map((r) => (
          <div key={r.capture_event_id} className="panel timeline-card">
            <div className="timeline-card-meta">
              <span className="dim">// {new Date(r.occurred_at).toLocaleDateString()}</span>
              <span className="dim">score {r.score.toFixed(2)}</span>
            </div>
            <p className="timeline-transcript">{r.transcript_text}</p>
            {r.related_entities.length > 0 && (
              <div className="timeline-card-actions">
                {r.related_entities.map((e) => (
                  <span key={e.id} className="dim">
                    {e.name} <span className="entity-type-label">{e.entity_type}</span>
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
      </div>
    </>
  );
}
