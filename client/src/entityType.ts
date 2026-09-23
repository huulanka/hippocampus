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
// `--entity-lightness` instead, which is the one part of an entity's
// colour that does have to track the theme: the same lightness that pops
// against the dark ground goes muddy on paper. Saturation does not need
// that same theme-tracking, which is what makes it the axis to vary here
// instead.
//
// The assignment comes from a hash of the type's own name, so it is the
// same in every session, on every machine, forever — which is what the
// paragraph at the top of this file always claimed and, until this was
// written, was not true. It used to be keyed to the order the running
// session happened to meet each type: the first type ever passed in
// became accent 0, the next accent 1. A restart reshuffled the lot.
//
// That made colour the one thing on screen nobody could learn. You cannot
// come to know that Personen are teal if Personen were teal yesterday and
// are mauve today, and the graph leans on colour harder than any other
// screen. FNV-1a because it is four lines, has no dependencies and
// scatters short similar strings ("Thema" / "Theme") into different
// buckets, which is the entire requirement.

const ACCENT_HUES = [20, 40, 168, 344]; // orange, gold, teal, mauve
const SATURATION_STEPS = [38, 58, 26, 48];

/// FNV-1a, 32-bit. `>>> 0` after the multiply keeps the result an unsigned
/// 32-bit integer — without it JavaScript's doubles start losing the low
/// bits, which are the ones being used.
function hash(text: string): number {
  let value = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    value ^= text.charCodeAt(i);
    value = Math.imul(value, 0x01000193) >>> 0;
  }
  return value;
}

function typeSlot(entityType: string): { hue: number; saturation: number } {
  // Two independent digits out of one hash: which accent, and which shade
  // of it. Dividing before the second modulo is what keeps them from
  // moving together — otherwise every type on accent 0 would also land on
  // the same saturation.
  const digest = hash(entityType.trim().toLowerCase());
  return {
    hue: ACCENT_HUES[digest % ACCENT_HUES.length],
    saturation:
      SATURATION_STEPS[Math.floor(digest / ACCENT_HUES.length) % SATURATION_STEPS.length],
  };
}

export function entityColor(entityType: string): string {
  const { hue, saturation } = typeSlot(entityType);
  return `hsl(${hue} ${saturation}% var(--entity-lightness))`;
}
