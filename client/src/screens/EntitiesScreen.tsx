import { useEffect, useState } from "react";
import { listEntities, listEntityTypes, type EntityListItem, type EntityTypeCount } from "../api";
import { entityColor } from "../entityType";

/// How many type chips to offer. The extraction prompt invents types
/// freely, so the long tail is large and mostly one-off.
const MAX_TYPE_CHIPS = 6;

/// The things the system has noticed, most-spoken-about first.
///
/// This is the half of retrieval that is not search: you arrive by
/// recognising a name, not by guessing one. The ordering matters more
/// than it looks — an entity mentioned once is noise, an entity mentioned
/// nine times is a subject.
export function EntitiesScreen({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [entities, setEntities] = useState<EntityListItem[] | null>(null);
  const [types, setTypes] = useState<EntityTypeCount[]>([]);
  const [activeType, setActiveType] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listEntityTypes()
      .then((all) => setTypes(all.slice(0, MAX_TYPE_CHIPS)))
      .catch(() => setTypes([]));
  }, []);

  useEffect(() => {
    let current = true;
    listEntities({ entityType: activeType ?? undefined })
      .then((all) => current && setEntities(all))
      .catch((err) => current && setError(String(err)));
    return () => {
      current = false;
    };
  }, [activeType]);

  if (error) return <p className="dim">Couldn't load entities: {error}</p>;
  if (!entities) return <p className="dim">Loading…</p>;

  const needle = filter.trim().toLowerCase();
  const shown = needle
    ? entities.filter((e) => e.name.toLowerCase().includes(needle))
    : entities;

  return (
    <>
      <div className="search-box">
        <span className="kicker">&gt;</span>
        <input
          className="search-input"
          value={filter}
          placeholder="Narrow the list by name…"
          onChange={(e) => setFilter(e.currentTarget.value)}
        />
      </div>
      <div className="search-filters">
        <span
          className={`filter-chip${activeType === null ? " active" : ""}`}
          onClick={() => setActiveType(null)}
        >
          [{activeType === null ? "x" : " "}] All
        </span>
        {types.map((t) => (
          <span
            key={t.entity_type}
            className={`filter-chip${activeType === t.entity_type ? " active" : ""}`}
            onClick={() => setActiveType(t.entity_type)}
          >
            [{activeType === t.entity_type ? "x" : " "}] {t.entity_type}
          </span>
        ))}
      </div>

      {shown.length === 0 ? (
        <p className="dim">
          {entities.length === 0
            ? "Nothing has been noticed yet. Entities appear once captures have been structured."
            : "No entity by that name."}
        </p>
      ) : (
        <div className="entity-grid">
          {shown.map((entity) => (
            <div
              key={entity.id}
              className="panel entity-card clickable"
              onClick={() => onOpenEntity(entity.id)}
            >
              <div className="entity-card-head">
                <span
                  className="entity-dot"
                  style={{ background: entityColor(entity.entity_type) }}
                />
                <span className="dim entity-type-label">{entity.entity_type.toUpperCase()}</span>
              </div>
              <div className="entity-name">{entity.name}</div>
              <div className="dim entity-meta">
                {entity.mention_count} {entity.mention_count === 1 ? "mention" : "mentions"}
                {entity.last_seen && ` · last ${shortDate(entity.last_seen)}`}
              </div>
              {entity.current_summary && (
                <p className="dim entity-summary">{entity.current_summary}</p>
              )}
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function shortDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}
