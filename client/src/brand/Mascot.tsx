import { useEffect, useMemo, useState } from "react";

export type MascotState = "idle" | "listening" | "thinking";

/// The mascot: a brain, in pixels, with a face.
///
/// Drawn the way the mark is built, not the way a heart is. The first two
/// attempts were symmetric — a left and a right lobe mirrored around a
/// groove, narrowing to a point at the bottom — and that is the
/// construction of a heart glyph, so both read as one. A brain is known by
/// its side profile: wider than tall, facing left like the mark, a bumpy
/// top where the gyri are the silhouette, one long lateral sulcus running
/// back from the front, the cerebellum tucked under the back and the stem
/// off-centre. Every one of those survives at 6px a cell; the face sits on
/// the frontal lobe, where a face would be.
///
/// Pixel art on purpose. The mark is the app's signature and stays
/// precise; the mascot is the part that is allowed to be fond of you, and
/// a hand-placed grid does that in a way a smooth vector cannot.
///
/// Unlike the mark, it *is* allowed to idle — it blinks and breathes,
/// because a character that holds perfectly still is furniture. It is
/// shown on one screen only, where you are waiting for something.

/// `O` outline and grooves, `H` the lit top, `F` the body, `D` the shaded
/// underside, `C` the cerebellum, `E` eyes and `W` the glint in them.
const REST = [
  "......OOOO..OOOOO.........",
  "....OOHHHHOOHHHHHOOOO.....",
  "...OHHHHHHHHHHHHHHHHHOO...",
  "..OHHHOOOHHHHHHOOOHHHHHO..",
  ".OHHHHHHHOHHHHOHHHHOHHHHO.",
  ".OHHHHHHHHOHHHOHHHHHOHHHHO",
  "OHHHOOOHHHHHHHHHOOHHHHHHHO",
  "OFFFFFFFFFFFFFFFFFOFFFFFFO",
  "OFFWEFWEFFFFFFFFFFFOFFFFFO",
  "OFFEEFEEFFOOOOOOFFFFOFFFFO",
  "OFFFFFFFFFFFFFFFOOOOFFFFFO",
  ".OFFFFFFFFFFFFFFFFFFFOOOO.",
  ".ODDDDDDDDDDDDDDDDDDOCCCCO",
  "..ODDDDDDDDDDDDDDDDOCCCCCO",
  "...OODDDDDDDDDDDOOOOCCCCO.",
  ".....OOOOOOOODDDO..OOOOO..",
  "............ODDO..........",
  "............ODO...........",
  ".............OO...........",
];

const INK: Record<string, string> = {
  O: "var(--mascot-line)",
  F: "var(--mascot-fill)",
  H: "var(--mascot-light)",
  D: "var(--mascot-shade)",
  C: "var(--mascot-shade)",
  E: "var(--mascot-line)",
  W: "var(--mascot-glint)",
};

function pixels(open: boolean) {
  const out: { key: string; left: number; top: number; ink: string }[] = [];
  REST.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      let cell = row[x];
      if (cell === ".") continue;
      // Shut eyes are the skin they sit on; the eyes are drawn on the
      // body, so that is always `F`.
      if (!open && (cell === "E" || cell === "W")) cell = "F";
      out.push({ key: `${y}-${x}`, left: x, top: y, ink: INK[cell] });
    }
  });
  return out;
}

export function Mascot({ state = "idle", cell = 6 }: { state?: MascotState; cell?: number }) {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    // Slow: a mascot that blinks on a fast timer reads as twitching, and
    // this one sits on screen for as long as a recording lasts.
    const id = window.setInterval(() => setTick((n) => n + 1), 900);
    return () => window.clearInterval(id);
  }, []);

  // Open almost always; shut for one beat in seven. Listening keeps its
  // eyes open — it is paying attention.
  const open = state === "listening" || tick % 7 !== 0;
  const drawn = useMemo(() => pixels(open), [open]);

  const width = REST[0].length * cell;
  const height = REST.length * cell;

  return (
    <span className={`mascot mascot-${state}`} style={{ width, height }}>
      {state === "listening" && (
        <>
          <span className="mascot-ring" />
          <span className="mascot-ring mascot-ring-late" />
        </>
      )}
      {state === "thinking" && (
        <span className="mascot-thoughts">
          <span style={{ background: "var(--ember)" }} />
          <span style={{ background: "var(--moss)" }} />
          <span style={{ background: "var(--plum)" }} />
        </span>
      )}
      <span className="mascot-body">
        {drawn.map((pixel) => (
          <span
            key={pixel.key}
            style={{
              left: pixel.left * cell,
              top: pixel.top * cell,
              width: cell,
              height: cell,
              background: pixel.ink,
            }}
          />
        ))}
      </span>
    </span>
  );
}
