import { useState } from "react";
import { search, type SearchResult } from "../api";

export function SearchScreen() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function runSearch(q: string) {
    if (!q.trim()) {
      setResults(null);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setResults(await search(q));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
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
      {/* Entity-type filtering needs the /search related_entities join
          (docs/adr backlog) — kept visible so the layout is ready, disabled
          until that lands. */}
      <div className="search-filters">
        <span className="filter-chip active">[x] All</span>
        <span className="filter-chip disabled" title="Coming soon">[ ] People</span>
        <span className="filter-chip disabled" title="Coming soon">[ ] Places</span>
        <span className="filter-chip disabled" title="Coming soon">[ ] Topics</span>
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
          </div>
        ))}
      </div>
    </>
  );
}
