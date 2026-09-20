import { useEffect, useMemo, useState } from "react";

export type MascotState = "idle" | "listening" | "thinking";

// Half of the 8-bit pixel-art brain; mirrored horizontally to get the full
// 16-wide grid. O = outline, F = fill, H = highlight, D = shadow.
const LEFT_HALF = [
  "......OO",
  "....OHHH",
  "..OHOHHH",
  ".OHHFFFF",
  "OHFFFFFF",
  "OFFFFFFF",
  "OFFFFFFF",
  "OFFFFDDD",
  "OFFFDDDD",
  ".ODDDDDD",
  ".ODDDDDD",
  "..ODDDDD",
  "....ODDD",
  "......OD",
];

const COLORS: Record<string, string> = {
  O: "#160f0a",
  F: "#c9663a",
  H: "#d6a94a",
  D: "#833a1d",
  E: "#160f0a",
};

function mirror(half: string[]): string[] {
  return half.map((row) => row + row.split("").reverse().join(""));
}

function addEyes(grid: string[]): string[] {
  const rows = grid.map((row) => row.split(""));
  for (const r of [5, 6]) {
    rows[r][5] = "E";
    rows[r][10] = "E";
  }
  return rows.map((row) => row.join(""));
}

function shiftUp(grid: string[]): string[] {
  return [grid[0].replace(/./g, "."), ...grid.slice(0, -1)];
}

interface Pixel {
  key: string;
  style: React.CSSProperties;
}

function toPixels(grid: string[], cell: number): Pixel[] {
  const out: Pixel[] = [];
  grid.forEach((row, r) => {
    for (let c = 0; c < row.length; c++) {
      const ch = row[c];
      if (ch === ".") continue;
      out.push({
        key: `${r}-${c}`,
        style: {
          position: "absolute",
          left: c * cell,
          top: r * cell,
          width: cell,
          height: cell,
          background: COLORS[ch],
        },
      });
    }
  });
  return out;
}

const EYES_OPEN = addEyes(mirror(LEFT_HALF));
const EYES_CLOSED = mirror(LEFT_HALF);

/**
 * The Hippocampus brain mark. Blinks and bobs at rest; `listening` adds
 * pulsing rings, `thinking` adds three orbiting dots. Pass `animated={false}`
 * for static uses (app icon, sidebar brand mark).
 */
export function Mascot({
  state = "idle",
  cell = 9,
  animated = true,
}: {
  state?: MascotState;
  cell?: number;
  animated?: boolean;
}) {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (!animated) return;
    const id = setInterval(() => setTick((t) => t + 1), 750);
    return () => clearInterval(id);
  }, [animated]);

  const pixels = useMemo(() => {
    if (!animated) return toPixels(EYES_OPEN, cell);
    const blinking = tick % 6 === 0;
    const base = blinking ? EYES_CLOSED : EYES_OPEN;
    const frame = tick % 2 === 0 ? base : shiftUp(base);
    return toPixels(frame, cell);
  }, [animated, tick, cell]);

  const width = 16 * cell;
  const height = LEFT_HALF.length * cell;
  const ringSize = cell * 11.5;

  return (
    <div style={{ position: "relative", width, height }}>
      {state === "listening" && (
        <>
          <span
            style={{
              position: "absolute",
              left: "50%",
              top: "50%",
              width: ringSize,
              height: ringSize,
              margin: -ringSize / 2,
              borderRadius: "50%",
              border: "1.5px solid #c9663a",
              animation: "hpo-pulse 1.6s ease-out infinite",
            }}
          />
          <span
            style={{
              position: "absolute",
              left: "50%",
              top: "50%",
              width: ringSize,
              height: ringSize,
              margin: -ringSize / 2,
              borderRadius: "50%",
              border: "1.5px solid #c9663a",
              animation: "hpo-pulse 1.6s ease-out infinite",
              animationDelay: "0.8s",
            }}
          />
        </>
      )}
      {state === "thinking" && (
        <div
          style={{
            position: "absolute",
            left: "50%",
            top: "50%",
            width: ringSize * 1.3,
            height: ringSize * 1.3,
            margin: (-ringSize * 1.3) / 2,
            animation: "hpo-orbit 1.8s linear infinite",
          }}
        >
          <span style={{ position: "absolute", left: "50%", top: 0, width: 7, height: 7, marginLeft: -3.5, background: "#d6a94a" }} />
          <span style={{ position: "absolute", left: 0, top: "50%", width: 6, height: 6, marginTop: -3, background: "#3f6e63" }} />
          <span style={{ position: "absolute", right: 0, top: "50%", width: 6, height: 6, marginTop: -3, background: "#8a4a5a" }} />
        </div>
      )}
      {pixels.map((p) => (
        <div key={p.key} style={p.style} />
      ))}
    </div>
  );
}
