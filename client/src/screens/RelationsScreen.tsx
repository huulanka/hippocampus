// Preview only, same reasoning as EntitiesScreen — no read API for
// `relations` yet, so this renders the design's sample graph.
const NODES = [
  { id: "priya", label: "Priya", x: 120, y: 70, type: "person" },
  { id: "lissabon", label: "Lissabon", x: 280, y: 40, type: "place" },
  { id: "homeoffice", label: "Home Office Idea", x: 420, y: 150, type: "topic" },
  { id: "max", label: "Max", x: 130, y: 260, type: "person" },
  { id: "buero", label: "New Office", x: 360, y: 280, type: "place" },
  { id: "laufen", label: "Running", x: 500, y: 90, type: "topic" },
];
const EDGES: [string, string][] = [
  ["priya", "lissabon"],
  ["priya", "max"],
  ["homeoffice", "buero"],
  ["max", "laufen"],
];
const TYPE_COLOR: Record<string, string> = { person: "#3f6e63", place: "#d6a94a", topic: "#8a4a5a" };
const NODE_R = 8;

export function RelationsScreen() {
  return (
    <>
      <p className="dim">Preview data — needs a read API over the `relations` table.</p>
      <div className="relations-graph">
        {EDGES.map(([fromId, toId]) => {
          const a = NODES.find((n) => n.id === fromId)!;
          const b = NODES.find((n) => n.id === toId)!;
          const dx = b.x - a.x;
          const dy = b.y - a.y;
          const len = Math.hypot(dx, dy);
          const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
          return (
            <div
              key={`${fromId}-${toId}`}
              className="relations-edge"
              style={{
                left: a.x,
                top: a.y,
                width: len,
                transform: `rotate(${angle}deg)`,
              }}
            />
          );
        })}
        {NODES.map((n) => (
          <div key={n.id}>
            <div
              className="relations-node"
              style={{
                left: n.x - NODE_R,
                top: n.y - NODE_R,
                width: NODE_R * 2,
                height: NODE_R * 2,
                background: TYPE_COLOR[n.type],
              }}
            />
            <div
              className="relations-label"
              style={{ left: n.x - 60, top: n.y + NODE_R + 4 }}
            >
              {n.label}
            </div>
          </div>
        ))}
      </div>
    </>
  );
}
