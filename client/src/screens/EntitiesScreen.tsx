import { useState } from "react";

// Preview only: there's no GET /entities endpoint yet, so this renders the
// design's sample data rather than real captures. Swap in a real fetch once
// that endpoint exists (structuring already writes to the `entities` table).
const SAMPLE_ENTITIES = [
  { id: "priya", name: "Priya", type: "person", count: 14, last: "Sep 18", related: ["Yesterday, 7:32pm — planning a weekend at Priya's in Lissabon", "Sep 12 — Priya recommended a book"] },
  { id: "okafor", name: "Dr. Okafor", type: "person", count: 3, last: "Sep 10", related: ["Yesterday, 3:45pm — talked with Dr. Okafor, appointment postponed"] },
  { id: "max", name: "Max (brother)", type: "person", count: 9, last: "Sep 15", related: [] },
  { id: "lissabon", name: "Lissabon", type: "place", count: 6, last: "Sep 18", related: ["Yesterday, 7:32pm — planning a weekend at Priya's in Lissabon"] },
  { id: "buero", name: "New Office", type: "place", count: 4, last: "Sep 16", related: ["Sep 16 — lease for a new office downtown", "Sep 9 — broken chair at the office"] },
  { id: "homeoffice", name: "Home Office Idea", type: "topic", count: 11, last: "Sep 19", related: ["Today, 9:14am — idea for home office setup, second monitor"] },
  { id: "laufen", name: "Running", type: "topic", count: 22, last: "Sep 19", related: ["Yesterday, 7:02am — morning run thoughts, new route along the river"] },
];

const TYPE_COLOR: Record<string, string> = { person: "#3f6e63", place: "#d6a94a", topic: "#8a4a5a" };

export function EntitiesScreen() {
  const [selected, setSelected] = useState<string | null>(null);
  const selectedEntity = SAMPLE_ENTITIES.find((e) => e.id === selected);

  return (
    <>
      <p className="dim">
        Preview data — wire this up once entities/relations get a read API (structuring already
        writes them).
      </p>
      <div className="entity-grid">
        {SAMPLE_ENTITIES.map((ent) => (
          <div
            key={ent.id}
            className={`panel entity-card${selected === ent.id ? " active" : ""}`}
            onClick={() => setSelected(ent.id)}
          >
            <div className="entity-card-head">
              <span className="entity-dot" style={{ background: TYPE_COLOR[ent.type] }} />
              <span className="dim entity-type-label">{ent.type.toUpperCase()}</span>
            </div>
            <div className="entity-name">{ent.name}</div>
            <div className="dim entity-meta">
              {ent.count} mentions · last {ent.last}
            </div>
          </div>
        ))}
      </div>
      {selectedEntity && (
        <div className="panel linked-captures">
          <div className="linked-captures-head">
            <span className="kicker">[ LINKED CAPTURES — {selectedEntity.name} ]</span>
          </div>
          {selectedEntity.related.length === 0 ? (
            <p className="dim">No linked captures.</p>
          ) : (
            selectedEntity.related.map((r) => (
              <p key={r} className="timeline-transcript">
                — {r}
              </p>
            ))
          )}
        </div>
      )}
    </>
  );
}
