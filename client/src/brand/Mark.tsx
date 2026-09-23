import mark from "./mark.json";

export type MarkState = "idle" | "listening" | "thinking";

/// The Hippocampus mark.
///
/// A brain in side profile, facing left. That direction is the whole
/// design: the two marks this replaces were each built by mirroring one
/// half and tapering to a point at the bottom centre, which is the
/// construction of a heart glyph — and they both read as one. A brain is
/// recognised from its profile, so this one is asymmetric, lumpy along the
/// top (the gyri *are* the silhouette, which is why they survive being
/// shrunk), and carries the two cues that do most of the recognising: a
/// cerebellum tucked in at the back and a stem leaving off-centre.
///
/// Geometry lives in `mark.json` and nowhere else. `scripts/render-brand.py`
/// reads the same file to produce the app icon and the menu-bar template,
/// so the mark cannot drift between the window and the menu bar.
///
/// It does not blink, bob or move at rest. The thing it replaced had eyes
/// and a two-frame hop, which made it a character rather than a mark — and
/// a logo that is animating when nothing is happening teaches you to stop
/// looking at it. Motion here only ever reports something real:
/// `listening` while the microphone is open, `thinking` while the backend
/// is still working on what you said.
export function Mark({
  size = 20,
  state = "idle",
  tone = "var(--accent)",
  title,
}: {
  size?: number;
  state?: MarkState;
  /// Any CSS colour. Defaults to the accent; pass `currentColor` to let it
  /// take the colour of whatever it sits inside.
  tone?: string;
  /// Given only where the mark is the sole content of a link or button.
  /// Anywhere it sits beside the word "Hippocampus" it is decoration, and
  /// naming it twice is noise for anyone listening to the screen.
  title?: string;
}) {
  // Below this a 1.3 stroke stops resolving and the folds turn to mush —
  // the tray bitmap this replaced learned the same lesson with
  // single-pixel notches that looked fine zoomed in and vanished at
  // real size.
  const small = size < mark.smallSize;
  const stroke = small ? mark.strokeSmall : mark.strokeOutline;
  const inner = small ? mark.strokeSmall : mark.strokeInner;

  return (
    <span
      className={`mark mark-${state}`}
      style={{ width: size, height: size, color: tone }}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <svg
        width={size}
        height={size}
        viewBox={mark.viewBox}
        fill="none"
        stroke="currentColor"
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        {state === "listening" && (
          <>
            <circle className="mark-ring" cx="12" cy="12" r="11" strokeWidth="0.8" />
            <circle className="mark-ring mark-ring-late" cx="12" cy="12" r="11" strokeWidth="0.8" />
          </>
        )}

        <path d={mark.outline} />
        {mark.essential.map((d) => (
          <path key={d} d={d} strokeWidth={inner} />
        ))}

        {/* An impulse running down the central sulcus. `pathLength="1"`
            normalises the path so the dash maths is the same whatever the
            curve actually measures. */}
        {state === "thinking" && (
          <path
            className="mark-impulse"
            d={mark.essential[0]}
            pathLength="1"
            strokeWidth={inner + 0.5}
          />
        )}
      </svg>
    </span>
  );
}
