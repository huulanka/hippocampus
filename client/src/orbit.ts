import type { Graph, GraphEdge, GraphNode } from "./api";

/// The graph, laid out as orbits around one thing.
///
/// What this replaces was a force simulation over the whole corpus. At 72
/// entities that is a hairball: the layout has one stable shape once there
/// are enough nodes and few enough edges, a uniform shell around the
/// origin, and no amount of tuning changes what the picture says because
/// the picture is not saying anything. Sixty of the seventy-two circles
/// carried no name, because names could only be shown for a handful
/// before they collided. At five hundred entities it gets structurally
/// worse, not better.
///
/// So the graph is never shown whole. It is always centred on one entity;
/// the inner ring is everything one step away, the outer ring is two, and
/// everything beyond is a number on the horizon. Three consequences, and
/// all three are the point:
///
///   - It cannot degenerate. The inner ring holds at most a dozen nodes
///     however large the corpus grows.
///   - Every edge you can see is far enough from its neighbours to carry
///     its own name, so the relations the extraction found are finally
///     readable instead of being a tooltip.
///   - Distance means something exact — steps, not the equilibrium of a
///     physics simulation nobody can read backwards.
///
/// Nothing moves on its own. The old layout drifted continuously so that
/// depth would register, which is motion carrying no information; here
/// motion happens only when you re-centre, and then it says that you
/// moved.

/// How many nodes each ring will show before the rest becomes a count.
/// Twelve is about where a ring stops having room for names at a
/// comfortable reading size; the outer ring can take more because its
/// nodes are smaller and unlabelled until hovered.
const MAX_INNER = 12;
const MAX_OUTER = 20;

/// Past this, an entity has not come up in a while and is drawn hollow.
/// A month rather than a week: the point is to tell "still live" from
/// "was a thing in spring", not to mark everything older than Tuesday.
const WARM_DAYS = 30;

export interface Placed {
  node: GraphNode;
  /// 0 for the centre, 1 one step away, 2 two steps.
  hop: number;
  angle: number;
  x: number;
  y: number;
  r: number;
  /// Mentioned within the last month.
  warm: boolean;
  /// Which node this one hangs off, and under what name. The relation is
  /// what the spoke is labelled with.
  via: string | null;
  relation: string | null;
}

export interface Spoke {
  from: Placed;
  to: Placed;
  relation: string;
  weight: number;
  /// True for the ring-one spokes, which are the ones that get a name.
  labelled: boolean;
}

export interface Orbit {
  centre: Placed;
  /// The two ring radii, so the drawing paints its guide circles exactly
  /// where the nodes were put rather than repeating the arithmetic.
  radii: { one: number; two: number };
  ring: Placed[];
  spokes: Spoke[];
  /// Entities that are connected but did not fit on a ring, plus
  /// everything the graph request never fetched. Shown as a number: a
  /// partial picture must never pretend to be whole.
  beyond: number;
  /// Nothing is connected to the centre at all. Worth saying out loud
  /// rather than drawing an empty circle — with most entities carrying a
  /// single observation, this is a common and honest answer.
  lonely: boolean;
}

export interface Size {
  width: number;
  height: number;
}

function radius(node: GraphNode): number {
  // Square root, so twice the mentions is twice the *area* rather than
  // twice the width — a linear radius makes a node said ten times look
  // ten times more important than one said once, which is not what
  // "mentioned more often" means.
  return 9 + Math.sqrt(Math.max(node.mention_count, 1)) * 3.4;
}

function warm(node: GraphNode, now: number): boolean {
  if (!node.last_seen) return false;
  return now - new Date(node.last_seen).getTime() < WARM_DAYS * 86_400_000;
}

/// Who touches whom, and under what name.
function neighbours(edges: GraphEdge[]): Map<string, { id: string; relation: string; weight: number }[]> {
  const out = new Map<string, { id: string; relation: string; weight: number }[]>();
  const add = (from: string, to: string, relation: string, weight: number) => {
    const list = out.get(from) ?? [];
    list.push({ id: to, relation, weight });
    out.set(from, list);
  };
  for (const edge of edges) {
    add(edge.from, edge.to, edge.relation_type, edge.weight);
    add(edge.to, edge.from, edge.relation_type, edge.weight);
  }
  return out;
}

/// The entity the graph opens on: the most recent one that is actually
/// connected to something.
///
/// The same posture as Today — start from what is already on your mind
/// rather than hand over an empty search field. "Connected" is the part
/// that took a try to get right: sorting by recency alone landed on
/// whatever the extraction had produced last, which with most entities
/// carrying a single observation is usually something mentioned once and
/// joined to nothing. Opening a graph on an empty ring is the worst first
/// impression it could make, and it says nothing true about the corpus.
///
/// Ties go to whatever has more relations, then to whatever is said more
/// often. A graph where nothing is connected at all falls back to the most
/// recent thing in it, because an empty ring is then the honest answer.
export function mostRecent(graph: Graph): GraphNode | null {
  const degree = new Map<string, number>();
  for (const edge of graph.edges) {
    degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
    degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
  }

  /// Four keys, most important first: connected at all, then which day it
  /// last came up, then how connected, then how often said.
  ///
  /// Recency is rounded to the day on purpose. To the millisecond it picks
  /// whichever entity the extraction happened to write last, which is
  /// usually something said once and joined to one thing — technically the
  /// most recent, and a poor place to start. Rounded, "yesterday" is a tie
  /// and the best-connected thing you talked about yesterday wins it.
  const DAY = 86_400_000;
  const rank = (node: GraphNode): number[] => [
    degree.has(node.id) ? 1 : 0,
    node.last_seen ? Math.floor(Date.parse(node.last_seen) / DAY) : 0,
    degree.get(node.id) ?? 0,
    node.mention_count,
  ];

  const beats = (a: number[], b: number[]) => {
    for (let i = 0; i < a.length; i++) {
      if (a[i] !== b[i]) return a[i] > b[i];
    }
    return false;
  };

  let best: GraphNode | null = null;
  let bestRank: number[] | null = null;
  for (const node of graph.nodes) {
    const here = rank(node);
    if (!bestRank || beats(here, bestRank)) {
      best = node;
      bestRank = here;
    }
  }
  return best;
}

/// Both rings have to fit inside the box, with room left over for the
/// names that sit outside the outer nodes and the horizon line below.
/// The old proportions were taken from the shorter side alone, which on a
/// window wider than it is tall put the outer ring past the top and bottom
/// edges — two-step nodes were laid out and never seen. The inner ring
/// keeps the design's proportion to the outer one (178 : 310).
export function ringRadii(size: Size): { one: number; two: number } {
  const vertical = Math.min(centreY(size), size.height - centreY(size)) - 44;
  const horizontal = size.width / 2 - 90;
  const two = Math.max(120, Math.min(vertical, horizontal, 340));
  return { one: two * 0.575, two };
}

/// A little above the middle, as in the design: the focus card and the
/// legend sit along the bottom edge, so the picture leans away from them.
export function centreY(size: Size): number {
  return size.height * 0.47;
}

export function buildOrbit(graph: Graph, centreId: string, size: Size, now = Date.now()): Orbit | null {
  const byId = new Map(graph.nodes.map((node) => [node.id, node]));
  const centreNode = byId.get(centreId);
  if (!centreNode) return null;

  const adjacency = neighbours(graph.edges);
  const { one: r1, two: r2 } = ringRadii(size);

  const centre: Placed = {
    node: centreNode,
    hop: 0,
    angle: 0,
    x: size.width / 2,
    y: centreY(size),
    r: radius(centreNode) * 1.8,
    warm: warm(centreNode, now),
    via: null,
    relation: null,
  };

  // — one step away, grouped by what the relation is called ---------------
  //
  // Grouping is what makes the ring say something before it is read: four
  // nodes under "besprochen" sit together as an arc, so the shape of the
  // picture already carries the shape of the sentence.
  const first = (adjacency.get(centreId) ?? []).filter((link) => byId.has(link.id));
  const bestPerNeighbour = new Map<string, { id: string; relation: string; weight: number }>();
  for (const link of first) {
    const held = bestPerNeighbour.get(link.id);
    if (!held || link.weight > held.weight) bestPerNeighbour.set(link.id, link);
  }
  const ranked = Array.from(bestPerNeighbour.values()).sort(
    (a, b) => (byId.get(b.id)!.mention_count ?? 0) - (byId.get(a.id)!.mention_count ?? 0),
  );
  const innerLinks = ranked.slice(0, MAX_INNER);

  const groups = new Map<string, typeof innerLinks>();
  for (const link of innerLinks) {
    const list = groups.get(link.relation) ?? [];
    list.push(link);
    groups.set(link.relation, list);
  }

  const ring: Placed[] = [];
  const placedIds = new Set<string>([centreId]);
  const total = innerLinks.length;
  // Start at the top and run clockwise; the first group is the largest, so
  // the heaviest part of the answer is where the eye lands first.
  let cursor = -Math.PI / 2;
  const ordered = Array.from(groups.entries()).sort((a, b) => b[1].length - a[1].length);
  // A gap between groups, taken out of the circle before it is shared.
  const gap = ordered.length > 1 ? Math.min(0.28, (Math.PI * 2) / (ordered.length * 6)) : 0;
  const usable = Math.PI * 2 - gap * ordered.length;

  for (const [, links] of ordered) {
    const span = usable * (links.length / total);
    links.forEach((link, index) => {
      // Centre a group of one in its own span instead of pinning it to the
      // leading edge, or a lone node sits visibly off its arc.
      const step = links.length === 1 ? span / 2 : (span * index) / (links.length - 1 || 1);
      const angle = cursor + (links.length === 1 ? step : step);
      const node = byId.get(link.id)!;
      ring.push({
        node,
        hop: 1,
        angle,
        x: centre.x + Math.cos(angle) * r1,
        y: centre.y + Math.sin(angle) * r1,
        r: radius(node),
        warm: warm(node, now),
        via: centreId,
        relation: link.relation,
      });
      placedIds.add(link.id);
    });
    cursor += span + gap;
  }

  // — two steps away ------------------------------------------------------
  //
  // Each sits near whichever ring-one node brought it here, so following
  // the picture outward follows the actual chain.
  const outer: Placed[] = [];
  const seen = new Set(placedIds);
  for (const parent of ring) {
    // One entry per neighbour, as on the inner ring: two relations to the
    // same thing ("betrifft" and "benötigt_workflow_für") otherwise placed
    // it twice, on top of itself.
    const strongest = new Map<string, { id: string; relation: string; weight: number }>();
    for (const link of adjacency.get(parent.node.id) ?? []) {
      if (!byId.has(link.id) || seen.has(link.id)) continue;
      const held = strongest.get(link.id);
      if (!held || link.weight > held.weight) strongest.set(link.id, link);
    }
    const children = Array.from(strongest.values()).sort(
      (a, b) => byId.get(b.id)!.mention_count - byId.get(a.id)!.mention_count,
    );
    // A slice of sky centred on the parent, narrow enough that two busy
    // neighbours do not grow into each other.
    const window = Math.min(0.5, (Math.PI * 2) / Math.max(ring.length, 1) / 1.6);
    children.slice(0, 4).forEach((link, index, kept) => {
      const offset = kept.length === 1 ? 0 : (index / (kept.length - 1) - 0.5) * window * 2;
      const angle = parent.angle + offset;
      const node = byId.get(link.id)!;
      outer.push({
        node,
        hop: 2,
        angle,
        x: centre.x + Math.cos(angle) * r2,
        y: centre.y + Math.sin(angle) * r2,
        r: Math.max(radius(node) * 0.62, 5),
        warm: warm(node, now),
        via: parent.node.id,
        relation: link.relation,
      });
      seen.add(link.id);
    });
  }
  const keptOuter = outer.slice(0, MAX_OUTER);
  for (const placed of keptOuter) placedIds.add(placed.node.id);

  const all = [...ring, ...keptOuter];
  const index = new Map(all.map((placed) => [placed.node.id, placed]));
  index.set(centreId, centre);

  // One line per pair. The API returns a relation once per direction and
  // once per wording, and drawing each on top of the last gave the same
  // line — and the same React key — two or three times over.
  const drawn = new Set<string>();
  const pair = (a: string, b: string) => (a < b ? `${a}|${b}` : `${b}|${a}`);

  const spokes: Spoke[] = [];
  for (const placed of all) {
    const from = placed.via ? index.get(placed.via) : undefined;
    if (!from) continue;
    drawn.add(pair(from.node.id, placed.node.id));
    spokes.push({
      from,
      to: placed,
      relation: placed.relation ?? "",
      weight: 1,
      labelled: placed.hop === 1,
    });
  }
  // Relations *between* two things on the inner ring. Thin and unlabelled:
  // they are not the answer to "what is this connected to", but leaving
  // them out would draw a fan where the data says a cluster.
  for (const edge of graph.edges) {
    const a = index.get(edge.from);
    const b = index.get(edge.to);
    if (!a || !b || a.hop === 0 || b.hop === 0) continue;
    const key = pair(a.node.id, b.node.id);
    if (drawn.has(key)) continue;
    drawn.add(key);
    spokes.push({ from: a, to: b, relation: edge.relation_type, weight: edge.weight, labelled: false });
  }

  return {
    centre,
    radii: { one: r1, two: r2 },
    ring: all,
    spokes,
    beyond: graph.nodes.length - placedIds.size + graph.omitted_nodes,
    lonely: ring.length === 0,
  };
}

export interface Cluster {
  /// The kind, or null for the disc that gathers the one-off kinds.
  entityType: string | null;
  /// How many kinds the disc holds — one, except for the gathered one.
  kinds: number;
  label: { x: number; y: number };
  members: Placed[];
  /// Members whose name is written next to them, and where.
  names: MapName[];
}

export interface MapName {
  id: string;
  text: string;
  x: number;
  y: number;
  anchor: "start" | "middle" | "end";
}

/// A kind needs this many entities before it gets a disc of its own.
/// The extraction invents a type per capture, so a real corpus carries
/// thirty kinds and twenty of them hold a single thing. Thirty discs is a
/// legend, not a picture; the one-offs share a disc instead, and each
/// still says its kind on hover.
const MIN_CLUSTER = 2;

/// Everything at once, as labelled clusters rather than a cloud.
///
/// "How much is there, and what kinds of thing" is a real question, and
/// it is the question the graph opens on. Entities of a kind are packed
/// into a disc by phyllotaxis; the discs are packed against each other,
/// largest first, each one at the free spot closest to the middle, so no
/// two ever overlap — the first version put them on a ring by index, and
/// with thirty kinds the ring folded over itself. The finished picture is
/// scaled to the box. It is deterministic: the same corpus gives the same
/// picture every time you open it.
export function buildMap(graph: Graph, size: Size, now = Date.now()): Cluster[] {
  const byType = new Map<string, GraphNode[]>();
  for (const node of graph.nodes) {
    const list = byType.get(node.entity_type) ?? [];
    list.push(node);
    byType.set(node.entity_type, list);
  }

  const groups: { entityType: string | null; kinds: number; nodes: GraphNode[] }[] = [];
  const oneOffs: GraphNode[] = [];
  let oneOffKinds = 0;
  for (const [entityType, nodes] of byType) {
    if (nodes.length >= MIN_CLUSTER) groups.push({ entityType, kinds: 1, nodes });
    else {
      oneOffs.push(...nodes);
      oneOffKinds += 1;
    }
  }
  groups.sort((a, b) => b.nodes.length - a.nodes.length || a.entityType!.localeCompare(b.entityType!));
  if (oneOffKinds === 1 && oneOffs.length > 0) {
    groups.push({ entityType: oneOffs[0].entity_type, kinds: 1, nodes: oneOffs });
  } else if (oneOffs.length > 0) {
    groups.push({ entityType: null, kinds: oneOffKinds, nodes: oneOffs });
  }

  // — each disc on its own, around the origin ------------------------------
  const golden = Math.PI * (3 - Math.sqrt(5));
  const SPACING = 20;
  const discs = groups.map((group) => {
    const members = group.nodes
      .slice()
      .sort((a, b) => b.mention_count - a.mention_count || a.name.localeCompare(b.name))
      .map((node, index) => {
        const spiral = index * golden;
        const out = SPACING * Math.sqrt(index + (index > 0 ? 0.5 : 0));
        return {
          node,
          hop: 1,
          angle: spiral,
          x: Math.cos(spiral) * out,
          y: Math.sin(spiral) * out,
          r: Math.max(radius(node) * 0.62, 4.5),
          warm: warm(node, now),
          via: null,
          relation: null,
        } satisfies Placed;
      });
    const reach = Math.max(...members.map((m) => Math.hypot(m.x, m.y) + m.r));
    // Room for the kind's name above and the members' names around it.
    return { group, members, radius: reach + 26, x: 0, y: 0 };
  });

  // — pack the discs --------------------------------------------------------
  //
  // Greedy: the largest sits in the middle, every next one is tried
  // against each placed disc at a ring of angles, touching it, and takes
  // whichever free spot is closest to the middle. Widened slightly so the
  // pack leans to the shape of the window rather than a circle.
  const aspect = Math.max(1, Math.min(2.2, size.width / Math.max(size.height, 1)));
  const distance = (x: number, y: number) => Math.hypot(x / aspect, y);
  const placed: typeof discs = [];
  for (const disc of discs) {
    if (placed.length === 0) {
      placed.push(disc);
      continue;
    }
    let best: { x: number; y: number; d: number } | null = null;
    for (const other of placed) {
      for (let k = 0; k < 36; k++) {
        const angle = (k / 36) * Math.PI * 2;
        const x = other.x + Math.cos(angle) * (other.radius + disc.radius);
        const y = other.y + Math.sin(angle) * (other.radius + disc.radius);
        const clear = placed.every((p) => Math.hypot(p.x - x, p.y - y) >= p.radius + disc.radius - 0.5);
        if (!clear) continue;
        const d = distance(x, y);
        if (!best || d < best.d) best = { x, y, d };
      }
    }
    disc.x = best!.x;
    disc.y = best!.y;
    placed.push(disc);
  }

  // — fit the pack into the box ---------------------------------------------
  const left = Math.min(...placed.map((d) => d.x - d.radius));
  const right = Math.max(...placed.map((d) => d.x + d.radius));
  const top = Math.min(...placed.map((d) => d.y - d.radius));
  const bottom = Math.max(...placed.map((d) => d.y + d.radius));
  const margin = 32;
  const scale = Math.min(
    1.6,
    (size.width - margin * 2) / Math.max(right - left, 1),
    (size.height - margin * 2) / Math.max(bottom - top, 1),
  );
  const ox = size.width / 2 - ((left + right) / 2) * scale;
  const oy = size.height / 2 - ((top + bottom) / 2) * scale;
  // Nodes grow with the picture only a little, so a sparse corpus does not
  // turn into beach balls and a dense one does not into dust.
  const nodeScale = Math.max(0.8, Math.min(1.25, scale));

  const clusters = placed.map((disc) => {
    const cx = ox + disc.x * scale;
    const cy = oy + disc.y * scale;
    const members = disc.members.map((m) => ({
      ...m,
      x: cx + m.x * scale,
      y: cy + m.y * scale,
      r: m.r * nodeScale,
    }));
    return {
      entityType: disc.group.entityType,
      kinds: disc.group.kinds,
      label: { x: cx, y: cy - (disc.radius - 12) * scale },
      members,
      names: [] as MapName[],
    };
  });
  placeNames(clusters);
  return clusters;
}

/// Roughly how wide a name is at the map's 11px — close enough to keep
/// names apart without measuring the DOM, which would mean laying the
/// picture out twice.
const CHAR_WIDTH = 6.1;
const NAME_HEIGHT = 13;

interface Box {
  left: number;
  right: number;
  top: number;
  bottom: number;
}

const overlaps = (a: Box, b: Box) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;

/// Which names the map writes, and where.
///
/// Every dot and every kind label is an obstacle. Candidates go in order
/// of importance — said most often first, across the whole map — and each
/// tries below, above, right and left of its dot; the first spot that
/// touches nothing already there wins, and a name with no free spot is
/// simply not written (its dot still says it on hover). The first version
/// put every name under its dot and let them fall on each other; this is
/// the fix that held up for the old graph too, a real collision check
/// instead of tuned offsets.
function placeNames(clusters: Cluster[]) {
  const taken: Box[] = [];
  for (const cluster of clusters) {
    for (const m of cluster.members) {
      taken.push({ left: m.x - m.r, right: m.x + m.r, top: m.y - m.r, bottom: m.y + m.r });
    }
    const title = cluster.entityType ?? "one-off kinds";
    const half = ((title.length + 4) * 7.4) / 2;
    taken.push({
      left: cluster.label.x - half,
      right: cluster.label.x + half,
      top: cluster.label.y - 11,
      bottom: cluster.label.y + 3,
    });
  }

  const candidates = clusters
    .flatMap((cluster) =>
      cluster.members.filter((m) => m.node.mention_count > 1).map((m) => ({ cluster, m })),
    )
    .sort(
      (a, b) =>
        b.m.node.mention_count - a.m.node.mention_count || a.m.node.name.localeCompare(b.m.node.name),
    );

  for (const { cluster, m } of candidates) {
    const text = m.node.name.length > 26 ? `${m.node.name.slice(0, 25)}…` : m.node.name;
    const width = text.length * CHAR_WIDTH;
    const spots: [number, number, MapName["anchor"]][] = [
      [m.x, m.y + m.r + 11, "middle"],
      [m.x, m.y - m.r - 4, "middle"],
      [m.x + m.r + 5, m.y + 4, "start"],
      [m.x - m.r - 5, m.y + 4, "end"],
    ];
    for (const [x, y, anchor] of spots) {
      const left = anchor === "start" ? x : anchor === "end" ? x - width : x - width / 2;
      const box = { left: left - 2, right: left + width + 2, top: y - NAME_HEIGHT + 3, bottom: y + 3 };
      if (taken.some((t) => overlaps(t, box))) continue;
      taken.push(box);
      cluster.names.push({ id: m.node.id, text, x, y, anchor });
      break;
    }
  }
}
