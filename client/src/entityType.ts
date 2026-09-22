// The extraction prompt invents entity types freely ("Erlebnis", "Backwerk",
// "Organisation"), so nothing can be keyed off a fixed list. Every type
// gets a consistent colour without anybody having to maintain a mapping —
// the same type is always the same colour, and a new type never arrives
// colourless.
//
// The four hues are not generated — they are the app's own fixed brand
// accents (`--hpo-accent`, `--hpo-gold`, `--hpo-teal`, `--hpo-mauve` in
// App.css), so "coloured by type" reads as this app's palette wherever it
// shows up — a chip, a dot, a node in the relations graph — rather than
// as a separately invented scheme. A first attempt at the graph used a
// generated colour stepped by the golden angle across the full hue wheel;
// distinct, but it produced saturated blues and violets that had nothing
// to do with the brand, and answered "what colour is a type" differently
// from every other screen.
//
// A type's *hue* comes from `index % 4` — which of the four accents it
// is a shade of. Its *saturation* comes from `index / 4` — which shade of
// that accent, so the fifth type sharing an accent with the first is
// still visibly a different type. Sixteen combinations before anything
// repeats, comfortably past the ~30 types a real corpus actually carries
// at once (`docs/consolidation.md`).
//
// Lightness is deliberately left out of this and comes from
// `--hpo-entity-lightness` instead, which is the one part of an entity's
// colour that does have to track the theme: the same lightness that pops
// against the dark ground goes muddy on paper. Saturation does not need
// that same theme-tracking, which is what makes it the axis to vary here
// instead.
//
// The assignment is append-only and keyed to encounter order within the
// running session, not to anything persisted — the first type this
// function is ever called with becomes accent 0, the next accent 1, and
// so on wrapping through the four. Two sessions can therefore colour the
// same type differently; nothing in the app currently depends on a
// type's colour surviving a restart.

const ACCENT_HUES = [20, 40, 168, 344]; // orange, gold, teal, mauve
const SATURATION_STEPS = [38, 58, 26, 48];

const typeOrder = new Map<string, number>();

function typeSlot(entityType: string): { hue: number; saturation: number } {
  let index = typeOrder.get(entityType);
  if (index === undefined) {
    index = typeOrder.size;
    typeOrder.set(entityType, index);
  }
  return {
    hue: ACCENT_HUES[index % ACCENT_HUES.length],
    saturation: SATURATION_STEPS[Math.floor(index / ACCENT_HUES.length) % SATURATION_STEPS.length],
  };
}

export function entityColor(entityType: string): string {
  const { hue, saturation } = typeSlot(entityType);
  return `hsl(${hue} ${saturation}% var(--hpo-entity-lightness))`;
}
