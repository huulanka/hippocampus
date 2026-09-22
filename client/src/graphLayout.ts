import type { GraphEdge, GraphNode } from "./api";

/// A force-directed layout, written here rather than pulled in.
///
/// `d3-force` would do this and more, but "more" is the problem: this
/// graph is at most a few hundred nodes, the simulation is a hundred
/// lines, and a dependency that ships a quadtree, three other force types
/// and its own event system to save those lines is not a trade this
/// project wants to make. The naive O(n²) repulsion below is fine at this
/// size — at 120 nodes it is 14,400 distance calculations per tick, which
/// a browser does in well under a millisecond.
///
/// The simulation is three-dimensional at all times, and flatness is a
/// force rather than a second code path: in flat mode `z` is pulled hard
/// toward zero, so the same tick that lays the graph out also performs
/// the transition between flat and deep. Two separate simulations would
/// mean the graph jumps when you switch, which destroys the one thing
/// that makes the switch worth having — seeing that it is the same graph.
///
/// Settling is deliberately visible. A graph that appears already
/// arranged tells you nothing about which nodes were pulled together;
/// watching it fall into place does.

export interface LaidOutNode {
  id: string;
  name: string;
  entityType: string;
  mentionCount: number;
  x: number;
  y: number;
  /// Toward the viewer is negative — the projection below divides by
  /// `FOCAL + z`, so a node in front has a smaller divisor and is drawn
  /// larger. Always present, always simulated, held at zero in flat mode.
  z: number;
  vx: number;
  vy: number;
  vz: number;
  /// Set while dragging, and kept afterwards: a node you moved stays
  /// where you put it, because you moved it for a reason.
  pinned: boolean;
  /// How many edges touch it. Used for sizing and for deciding which
  /// labels survive at low zoom.
  degree: number;
}

export interface LaidOutEdge {
  from: string;
  to: string;
  relationType: string;
  weight: number;
}

/// Where the graph is being looked at from. Two angles rather than a
/// camera position: the graph is always centred on the origin and always
/// looked at from the outside, so a full camera would be three numbers
/// that can only ever describe the same sphere.
export interface View {
  /// Rotation about the vertical axis, in radians.
  yaw: number;
  /// Rotation about the horizontal axis.
  pitch: number;
}

export const FLAT_VIEW: View = { yaw: 0, pitch: 0 };

/// How hard nodes push each other apart. Scaled by node size, so busy
/// entities claim more room than incidental ones.
const REPULSION = 5200;
/// Rest length of an edge. Long enough that labels have room.
const SPRING_LENGTH = 92;
const SPRING_STRENGTH = 0.035;
/// A weak pull between any two nodes that share a type, independent of
/// whether a relation connects them.
///
/// Without this, the only forces are isotropic repulsion and a pull to
/// the origin — and a graph with nothing else structuring it settles onto
/// a uniform shell around the centre (the Thomson problem: points that
/// only push each other apart spread out evenly over a sphere) regardless
/// of what the data actually says. In deep mode that shell is the "always
/// a ball" the graph was criticised for.
///
/// An order of magnitude weaker than `SPRING_STRENGTH`: a real, asserted
/// relation must still win every argument about where a node sits. This
/// only breaks the tie for everything a relation does not already decide
/// — same-type entities drift into loose neighbourhoods instead of
/// scattering uniformly, which is what turns the shell into something
/// that looks read rather than generated.
const TYPE_CLUSTER_STRENGTH = 0.0028;
/// Pull toward the middle, so disconnected islands do not drift away.
const GRAVITY = 0.012;
/// What holds the graph flat. Far stronger than `GRAVITY`, because this
/// is not a gentle tendency but the definition of the mode — and it is
/// the only thing standing between "2D" and a graph that slowly drifts
/// out of its own plane.
const FLATTENING = 0.16;
const DAMPING = 0.86;
/// Below this, the layout is called settled and the loop stops. Without
/// it the simulation would run forever at imperceptible amplitudes and
/// keep a core busy for nothing.
const SETTLED = 0.06;

/// Distance from the eye to the plane the graph is centred on. Large
/// enough that perspective reads as depth rather than as a fisheye: with
/// nodes spread roughly ±260 in z, the nearest is drawn about 1.4× the
/// size of the furthest, which is enough to tell them apart and not
/// enough to make the near ones grotesque.
const FOCAL = 900;

export function nodeRadius(node: { mentionCount: number; degree: number }): number {
  // Square root, not linear: an entity mentioned 40 times is not 40 times
  // more important than one mentioned once, and linear scaling makes the
  // busiest node swallow the picture.
  return 5 + Math.sqrt(node.mentionCount + node.degree) * 2.6;
}

export function layoutFrom(nodes: GraphNode[], edges: GraphEdge[]): LaidOutNode[] {
  const degree = new Map<string, number>();
  for (const edge of edges) {
    degree.set(edge.from, (degree.get(edge.from) ?? 0) + 1);
    degree.set(edge.to, (degree.get(edge.to) ?? 0) + 1);
  }

  // Seeded on a circle rather than at random: a random start occasionally
  // produces a knot that the simulation never fully unpicks, and a ring
  // gives every node room to be pushed outward from the first tick.
  //
  // The depth seed is deliberately not random either, and deliberately
  // not zero. Not random, because two visits to the same graph should lay
  // it out the same way. Not zero, because repulsion between two nodes at
  // exactly the same depth has no depth component to push along — switch
  // to deep mode from a perfectly flat start and the graph stays a sheet.
  return nodes.map((node, i) => {
    const angle = (i / Math.max(nodes.length, 1)) * Math.PI * 2;
    const radius = 120 + (i % 7) * 18;
    return {
      id: node.id,
      name: node.name,
      entityType: node.entity_type,
      mentionCount: node.mention_count,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      z: (((i * 37) % 21) - 10) * 9,
      vx: 0,
      vy: 0,
      vz: 0,
      pinned: false,
      degree: degree.get(node.id) ?? 0,
    };
  });
}

/// Advances the simulation one step, in place. Returns the total
/// movement, so the caller can stop when nothing is happening any more.
export function tick(nodes: LaidOutNode[], edges: LaidOutEdge[], deep: boolean): number {
  const byId = new Map(nodes.map((n) => [n.id, n]));

  for (let i = 0; i < nodes.length; i += 1) {
    const a = nodes[i];
    for (let j = i + 1; j < nodes.length; j += 1) {
      const b = nodes[j];
      let dx = b.x - a.x;
      let dy = b.y - a.y;
      let dz = b.z - a.z;
      let distanceSquared = dx * dx + dy * dy + dz * dz;
      if (distanceSquared < 0.01) {
        // Exactly coincident nodes have no direction to separate along,
        // so give them one rather than dividing by zero.
        dx = Math.random() - 0.5;
        dy = Math.random() - 0.5;
        dz = Math.random() - 0.5;
        distanceSquared = 0.01;
      }
      const distance = Math.sqrt(distanceSquared);
      const sizes = nodeRadius(a) + nodeRadius(b);
      const force = (REPULSION * (sizes / 24)) / distanceSquared;
      const fx = (dx / distance) * force;
      const fy = (dy / distance) * force;
      const fz = (dz / distance) * force;
      a.vx -= fx;
      a.vy -= fy;
      a.vz -= fz;
      b.vx += fx;
      b.vy += fy;
      b.vz += fz;

      // Same loop, same pair, so this costs nothing extra to compute —
      // a Hookean pull toward each other with no rest length, scaled by
      // how far apart they already are so it never fights the repulsion
      // above at close range.
      if (a.entityType === b.entityType) {
        const pull = distance * TYPE_CLUSTER_STRENGTH;
        const tfx = (dx / distance) * pull;
        const tfy = (dy / distance) * pull;
        const tfz = (dz / distance) * pull;
        a.vx += tfx;
        a.vy += tfy;
        a.vz += tfz;
        b.vx -= tfx;
        b.vy -= tfy;
        b.vz -= tfz;
      }
    }
  }

  for (const edge of edges) {
    const a = byId.get(edge.from);
    const b = byId.get(edge.to);
    if (!a || !b) continue;
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const dz = b.z - a.z;
    const distance = Math.sqrt(dx * dx + dy * dy + dz * dz) || 0.01;
    // Repeatedly asserted relations pull harder: things you keep saying
    // about the same pair should sit closer together.
    const strength = SPRING_STRENGTH * Math.min(1 + Math.log2(edge.weight), 3);
    const force = (distance - SPRING_LENGTH) * strength;
    const fx = (dx / distance) * force;
    const fy = (dy / distance) * force;
    const fz = (dz / distance) * force;
    a.vx += fx;
    a.vy += fy;
    a.vz += fz;
    b.vx -= fx;
    b.vy -= fy;
    b.vz -= fz;
  }

  let movement = 0;
  for (const node of nodes) {
    // A pinned node still takes part in every force above — it is the
    // rest of the graph making room for it that makes pinning useful —
    // it just does not move itself.
    if (node.pinned) {
      node.vx = 0;
      node.vy = 0;
      node.vz = 0;
      continue;
    }
    node.vx -= node.x * GRAVITY;
    node.vy -= node.y * GRAVITY;
    node.vz -= node.z * (deep ? GRAVITY : FLATTENING);
    node.vx *= DAMPING;
    node.vy *= DAMPING;
    node.vz *= DAMPING;
    node.x += node.vx;
    node.y += node.vy;
    node.z += node.vz;
    movement += Math.abs(node.vx) + Math.abs(node.vy) + Math.abs(node.vz);
  }

  return nodes.length ? movement / nodes.length : 0;
}

export interface Projected {
  x: number;
  y: number;
  /// How much perspective enlarged this point. Multiplied into radii and
  /// label sizes so that near and far read as near and far.
  scale: number;
  /// Rotated depth, larger meaning further away. Used only for sorting.
  depth: number;
}

/// Turns a point in the graph's own space into a point on the screen.
///
/// Yaw first, then pitch, then a perspective divide — the conventional
/// order, and the one that makes a horizontal drag feel like spinning a
/// globe rather than tumbling it. At `FLAT_VIEW` with `z` at zero this
/// reduces to the identity with a scale of exactly 1, which is why flat
/// mode needs no separate path through any of the drawing code.
export function project(node: { x: number; y: number; z: number }, view: View): Projected {
  const cosYaw = Math.cos(view.yaw);
  const sinYaw = Math.sin(view.yaw);
  const x = node.x * cosYaw + node.z * sinYaw;
  let z = node.z * cosYaw - node.x * sinYaw;

  const cosPitch = Math.cos(view.pitch);
  const sinPitch = Math.sin(view.pitch);
  const y = node.y * cosPitch - z * sinPitch;
  z = node.y * sinPitch + z * cosPitch;

  const scale = FOCAL / (FOCAL + z);
  return { x: x * scale, y: y * scale, scale, depth: z };
}

/// The scale a node at the near and far edge of the graph comes out at.
/// Both follow from `FOCAL` and from how far repulsion spreads the graph,
/// and they exist so that fading can be spread across the depth that is
/// actually there rather than across an arbitrary range.
const NEAREST_SCALE = FOCAL / (FOCAL - 260);
const FURTHEST_SCALE = FOCAL / (FOCAL + 260);

/// How solid a node at this scale should be drawn.
///
/// Size alone is ambiguous: a small circle is either far away or rarely
/// mentioned, and those are two very different facts. Fading resolves it,
/// but only if it is spread over the depth the graph actually occupies —
/// clamped at the midpoint, as this first was, every node in the near
/// half comes out identical and half the depth is thrown away.
export function depthOpacity(scale: number): number {
  const t = (scale - FURTHEST_SCALE) / (NEAREST_SCALE - FURTHEST_SCALE);
  return 0.45 + 0.55 * Math.max(0, Math.min(1, t));
}

/// The inverse of `project` for a point whose depth is already known —
/// which is what dragging needs. A pointer gives two numbers and a node
/// has three, so the missing one has to come from somewhere; taking the
/// node's current depth means a drag slides it across the screen without
/// also pushing it toward or away from the viewer, which is the only
/// interpretation of a two-dimensional gesture that does not surprise.
export function unproject(
  screen: { x: number; y: number },
  depth: number,
  view: View,
): { x: number; y: number; z: number } {
  const scale = FOCAL / (FOCAL + depth);
  const x = screen.x / scale;
  const y = screen.y / scale;

  const cosPitch = Math.cos(view.pitch);
  const sinPitch = Math.sin(view.pitch);
  const rotatedY = y * cosPitch + depth * sinPitch;
  const z = depth * cosPitch - y * sinPitch;

  const cosYaw = Math.cos(view.yaw);
  const sinYaw = Math.sin(view.yaw);
  return {
    x: x * cosYaw - z * sinYaw,
    y: rotatedY,
    z: z * cosYaw + x * sinYaw,
  };
}


export const SETTLED_THRESHOLD = SETTLED;

/// A label's rectangle, in the graph's own coordinate space, centred at
/// `(x, y)` — the anchor a hand-tuned offset used to point at without
/// ever checking whether anything else was already sitting there.
export interface LabelBox {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/// Roughly the advance width of one Space Mono character at 1px, which
/// is what the whole app is set in (`App.css`). Monospace is what makes
/// this worth doing at all: measuring real text needs a mounted DOM node
/// (`getBBox`), which means rendering once to find out where to render —
/// a fixed em-fraction is exact enough for a font where every glyph is
/// the same width, and costs nothing to compute ahead of the first paint.
const CHAR_WIDTH_EM = 0.62;

export function labelWidth(text: string, fontSize: number): number {
  return text.length * fontSize * CHAR_WIDTH_EM;
}

/// Separates overlapping label boxes by actually detecting and resolving
/// collisions, rather than by a formula that guesses how many labels
/// might land near each other. Each box is still pinned close to the
/// point it is labelling — `maxDrift` away at most — because a label
/// free to drift wherever there's room stops pointing at anything.
///
/// Axis-aligned rectangle separation, the same technique any 2D physics
/// engine uses for non-penetration: on each pass, two overlapping boxes
/// are pushed apart along whichever axis has the smaller overlap, which
/// is the cheapest way to stop overlapping. Repeated for a fixed budget
/// of iterations rather than until settled — with graphs busy enough
/// that not every overlap can be resolved inside `maxDrift`, "settled"
/// might never come, and a busy view is exactly where this must not hang.
export function declutterLabels(
  boxes: LabelBox[],
  maxDrift = 26,
  iterations = 40,
): Map<string, { x: number; y: number }> {
  const points = boxes.map((b) => ({ ...b, anchorX: b.x, anchorY: b.y }));

  for (let iter = 0; iter < iterations; iter += 1) {
    let moved = false;

    for (let i = 0; i < points.length; i += 1) {
      const a = points[i];
      for (let j = i + 1; j < points.length; j += 1) {
        const b = points[j];
        const overlapX = (a.width + b.width) / 2 - Math.abs(a.x - b.x);
        const overlapY = (a.height + b.height) / 2 - Math.abs(a.y - b.y);
        if (overlapX <= 0 || overlapY <= 0) continue;

        moved = true;
        if (overlapX < overlapY) {
          const push = overlapX / 2 + 0.5;
          const dir = a.x <= b.x ? -1 : 1;
          a.x += push * dir;
          b.x -= push * dir;
        } else {
          const push = overlapY / 2 + 0.5;
          const dir = a.y <= b.y ? -1 : 1;
          a.y += push * dir;
          b.y -= push * dir;
        }
      }
    }

    if (!moved) break;

    // Pulled back toward the anchor every pass, not only at the end —
    // otherwise a box that needed the whole budget to escape one
    // overlap has already spent it and can't also escape the next.
    for (const p of points) {
      const dx = p.x - p.anchorX;
      const dy = p.y - p.anchorY;
      const dist = Math.hypot(dx, dy);
      if (dist > maxDrift) {
        const scale = maxDrift / dist;
        p.x = p.anchorX + dx * scale;
        p.y = p.anchorY + dy * scale;
      }
    }
  }

  return new Map(points.map((p) => [p.id, { x: p.x, y: p.y }]));
}
