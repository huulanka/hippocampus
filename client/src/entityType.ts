// The extraction prompt invents entity types freely ("Erlebnis", "Backwerk",
// "Organisation"), so nothing can be keyed off a fixed list. A stable hash
// over the type name gives each one a consistent colour without anybody
// having to maintain a mapping — the same type is always the same colour,
// and a new type never arrives colourless.
//
// A hue rather than a slot in a small palette: a handful of fixed colours
// collide constantly once a dozen types exist, and two types sharing a
// colour is worse than no colour at all. Saturation and lightness stay
// fixed and low, which keeps every type dusty enough to sit on the same
// ground as the rest of the interface. Lightness comes from the theme, so
// the same hue stays legible on paper and on the dark ground.

const SATURATION = 38;

export function entityColor(entityType: string): string {
  let hash = 0;
  for (let i = 0; i < entityType.length; i += 1) {
    hash = (hash * 31 + entityType.charCodeAt(i)) >>> 0;
  }
  return `hsl(${hash % 360} ${SATURATION}% var(--hpo-entity-lightness))`;
}
