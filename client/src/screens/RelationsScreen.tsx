import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getGraph, type Graph } from "../api";
import { entityColor } from "../entityType";
import {
  FLAT_VIEW,
  SETTLED_THRESHOLD,
  depthOpacity,
  layoutFrom,
  nodeRadius,
  project,
  tick,
  unproject,
  type LaidOutEdge,
  type LaidOutNode,
  type View,
} from "../graphLayout";

/// The graph, as something you can walk rather than a picture of one.
///
/// It used to be a mock with six hard-coded nodes, because there was no
/// read API over `relations`. There is now, and this renders it: every
/// entity you have mentioned, every relation the extraction found between
/// them, laid out by a force simulation that you can watch settle.
///
/// Deliberately SVG rather than canvas or WebGL. At this size the
/// performance difference is nil, and SVG keeps every node a real
/// element — which is what makes hover, focus, click and the labels work
/// without reimplementing hit testing and text rendering. The depth in
/// deep mode is real: the simulation runs in three dimensions and the
/// result is projected here, rather than the whole drawing layer being
/// swapped for a 3D engine to arrive at the same picture.

/// How far the pointer may travel between press and release and still
/// count as a click rather than a drag.
const CLICK_SLOP = 4;
/// How fast a drag on the background turns the graph, in radians per
/// pixel. Tuned so that dragging across the width of the view is a little
/// more than half a turn.
const TURN_PER_PIXEL = 0.007;
/// Looking at a graph from directly above tells you nothing, and past
/// vertical it turns upside down. Pitch stops short of both.
const MAX_PITCH = 1.2;
/// How far the graph turns on its own each frame while nobody is doing
/// anything. Without it a projected graph is a still image and reads as a
/// flat mess of overlapping circles; slow motion is what makes depth
/// legible at all. Stopped the moment there is something to look at —
/// while dragging, and while a node is focused.
const DRIFT_PER_FRAME = 0.0016;
/// Relation types come out of the extraction as free text, and some are
/// whole clauses ("ist betroffen von unzureichender Datenqualität
/// in"). Past this they are cut, with the whole thing still in the
/// tooltip.
const MAX_EDGE_LABEL = 26;
/// How far a relation name sits off its own edge, perpendicular to it.
const EDGE_LABEL_OFFSET = 9;
/// How many entities the search offers at once.
const MAX_MATCHES = 6;
/// How many entity types get a chip before the rest are folded away. The
/// extraction invents a type per capture, so a real corpus has thirty of
/// them and twenty are one-offs — all of them at once is three lines of
/// legend above a picture, which is the wrong way round. The same cap
/// exists on the entities screen, for the same reason.
const MAX_TYPE_CHIPS = 10;

export function RelationsScreen({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [graph, setGraph] = useState<Graph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [nodes, setNodes] = useState<LaidOutNode[]>([]);
  const [hovered, setHovered] = useState<string | null>(null);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  /// The pan again, readable synchronously. A `setState` updater does not
  /// run when it is called but when React next renders, so a frame loop
  /// that tried to decide "have we arrived yet?" inside one always read
  /// the answer from the previous frame — and never stopped.
  const panRef = useRef({ x: 0, y: 0 });

  /// Where the graph is being looked at from, and whether it has any
  /// depth to look at. Held in state for rendering and in a ref for the
  /// animation loop, which must not re-subscribe every frame.
  const [deep, setDeep] = useState(false);
  const [view, setView] = useState<View>(FLAT_VIEW);
  const viewRef = useRef<View>(FLAT_VIEW);
  const deepRef = useRef(false);

  /// The node being looked at. Clicking a node focuses it rather than
  /// navigating away: the question you have in front of a graph is almost
  /// never "show me this entity's page", it is "what is this one next
  /// to". Opening the page is then one more click, on the same node.
  const [focused, setFocused] = useState<string | null>(null);
  const focusedRef = useRef<string | null>(null);
  const [query, setQuery] = useState("");

  /// Filters. A graph of everything is a graph of nothing: the
  /// extraction invents an entity for every passing remark, so a single
  /// note about someone's dog arrives as its own node and its own edge
  /// and sits in the picture forever. These take it back out without
  /// deleting anything.
  const [hiddenTypes, setHiddenTypes] = useState<Set<string>>(new Set());
  const [minMentions, setMinMentions] = useState(1);
  const [connectedOnly, setConnectedOnly] = useState(false);
  const [allTypes, setAllTypes] = useState(false);

  const frame = useRef<number | null>(null);
  const nodesRef = useRef<LaidOutNode[]>([]);
  const svgRef = useRef<SVGSVGElement | null>(null);
  /// The zoomed and panned group. Pointer coordinates are mapped through
  /// its own screen matrix rather than by recomputing the transform by
  /// hand — the browser already knows where everything ended up, and
  /// re-deriving it is how drag-under-zoom bugs happen.
  const groupRef = useRef<SVGGElement | null>(null);
  const dragging = useRef<{ id: string | null; travelled: number } | null>(null);
  /// A drag that ends on a node must not also focus it. Without this,
  /// moving a node to a better spot selects it the moment you let go.
  const draggedFar = useRef(false);
  /// Where the view wants to be. While a node is focused this tracks it,
  /// so a focused node stays centred while the layout is still settling
  /// underneath it.
  const panTarget = useRef<{ x: number; y: number } | null>(null);
  /// Whether the view is still following the focused node. Dropped the
  /// moment the view is moved by hand — having put the graph where you
  /// want it, you should not have to fight it back there every frame.
  const tracking = useRef(false);

  useEffect(() => {
    let live = true;
    getGraph()
      .then((g) => live && setGraph(g))
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
  }, []);

  /// Every type present, with how many entities carry it. Built from the
  /// graph rather than fetched, so the chips always match what is on
  /// screen.
  const types = useMemo(() => {
    const counts = new Map<string, number>();
    for (const node of graph?.nodes ?? []) {
      counts.set(node.entity_type, (counts.get(node.entity_type) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [graph]);

  /// What survives the filters. Edges go when either end does — an edge
  /// to something that is not drawn is a line to nowhere.
  const visible = useMemo(() => {
    const nodes = (graph?.nodes ?? []).filter(
      (n) => !hiddenTypes.has(n.entity_type) && n.mention_count >= minMentions,
    );
    let ids = new Set(nodes.map((n) => n.id));
    let edges = (graph?.edges ?? []).filter((e) => ids.has(e.from) && ids.has(e.to));

    if (connectedOnly) {
      const touched = new Set<string>();
      for (const edge of edges) {
        touched.add(edge.from);
        touched.add(edge.to);
      }
      const kept = nodes.filter((n) => touched.has(n.id));
      ids = new Set(kept.map((n) => n.id));
      edges = edges.filter((e) => ids.has(e.from) && ids.has(e.to));
      return { nodes: kept, edges };
    }

    return { nodes, edges };
  }, [graph, hiddenTypes, minMentions, connectedOnly]);

  const edges: LaidOutEdge[] = useMemo(
    () =>
      visible.edges.map((e) => ({
        from: e.from,
        to: e.to,
        relationType: e.relation_type,
        weight: e.weight,
      })),
    [visible],
  );

  /// One animation loop for the whole screen, driving three things that
  /// all have to agree with each other every frame: the simulation, the
  /// drift, and the pan that keeps a focused node centred.
  ///
  /// It deliberately does not stop when the simulation settles. Settling
  /// only means the nodes have stopped moving — the view may still be
  /// turning, or still easing toward a node that was just focused. What
  /// it stops is *ticking*, which is the part that costs anything; it
  /// goes on drawing until nothing is in motion either.
  const animate = useCallback(() => {
    if (frame.current !== null) cancelAnimationFrame(frame.current);

    let settling = true;
    function step() {
      let moving = false;

      if (settling) {
        // Several ticks per frame: the layout reaches its shape in about
        // a second instead of ten, while still visibly falling into
        // place.
        let movement = 0;
        for (let i = 0; i < 3; i += 1) {
          movement = tick(nodesRef.current, edges, deepRef.current);
        }
        if (movement > SETTLED_THRESHOLD) moving = true;
        else settling = false;
      }

      if (deepRef.current && !dragging.current && !panTarget.current) {
        viewRef.current = {
          ...viewRef.current,
          yaw: viewRef.current.yaw + DRIFT_PER_FRAME,
        };
        setView(viewRef.current);
        moving = true;
      } else if (!deepRef.current) {
        // Coming back to flat: the angles have to return to zero as well,
        // or the graph lands flat but askew.
        const { yaw, pitch } = viewRef.current;
        if (Math.abs(yaw) > 0.001 || Math.abs(pitch) > 0.001) {
          viewRef.current = { yaw: yaw * 0.88, pitch: pitch * 0.88 };
          setView(viewRef.current);
          moving = true;
        } else if (yaw !== 0 || pitch !== 0) {
          viewRef.current = FLAT_VIEW;
          setView(FLAT_VIEW);
        }
      }

      // Re-aimed every frame rather than once when the node was clicked:
      // the layout is usually still settling underneath a focused node,
      // and a target computed once leaves it drifting off centre.
      const centre = tracking.current ? focusedRef.current : null;
      if (centre && !dragging.current) {
        const node = nodesRef.current.find((n) => n.id === centre);
        if (node) {
          const at = project(node, viewRef.current);
          panTarget.current = { x: -at.x, y: -at.y };
        }
      }

      const target = panTarget.current;
      if (target) {
        const dx = target.x - panRef.current.x;
        const dy = target.y - panRef.current.y;
        if (Math.abs(dx) < 0.3 && Math.abs(dy) < 0.3) {
          if (panRef.current.x !== target.x || panRef.current.y !== target.y) {
            panRef.current = target;
            setPan(target);
          }
          if (!centre) panTarget.current = null;
        } else {
          panRef.current = {
            x: panRef.current.x + dx * 0.18,
            y: panRef.current.y + dy * 0.18,
          };
          setPan(panRef.current);
          moving = true;
        }
      }

      setNodes([...nodesRef.current]);
      frame.current = moving ? requestAnimationFrame(step) : null;
    }

    frame.current = requestAnimationFrame(step);
  }, [edges]);

  // Lay out from scratch whenever the set of nodes changes, then run.
  // Switching between flat and deep is deliberately *not* in here: the
  // same nodes carry on from where they are and the flattening force does
  // the rest, which is what makes the switch legible.
  useEffect(() => {
    if (!graph) return;
    nodesRef.current = layoutFrom(visible.nodes, visible.edges);
    setNodes([...nodesRef.current]);
    animate();
    return () => {
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
    };
  }, [graph, visible, animate]);

  /// Focus follows the node: while one is focused the view eases to keep
  /// it centred, and letting go of focus leaves the view where it is
  /// rather than snapping back to somewhere nobody asked for.
  useEffect(() => {
    focusedRef.current = focused;
    tracking.current = focused !== null;
    if (!focused) {
      panTarget.current = null;
      return;
    }
    animate();
  }, [focused, animate]);

  const setDepth = useCallback(
    (next: boolean) => {
      deepRef.current = next;
      setDeep(next);
      // The nodes have to be let go of, or a graph that settled flat
      // stays flat: pinning survives the switch, and a pinned node has no
      // depth to gain.
      for (const node of nodesRef.current) node.pinned = false;
      animate();
    },
    [animate],
  );

  const neighbours = useMemo(() => {
    const centre = focused ?? hovered;
    if (!centre) return null;
    const near = new Set<string>([centre]);
    for (const edge of edges) {
      if (edge.from === centre) near.add(edge.to);
      if (edge.to === centre) near.add(edge.from);
    }
    return near;
  }, [focused, hovered, edges]);

  /// Entities whose name contains what was typed. The graph is not
  /// filtered down to them — at a hundred nodes you cannot find a
  /// particular one by eye, but you also do not want the rest to vanish,
  /// because where it sits among the rest is the answer you came for.
  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return [];
    return visible.nodes
      .filter((n) => n.name.toLowerCase().includes(needle))
      .slice(0, MAX_MATCHES);
  }, [query, visible]);
  const matchIds = useMemo(() => new Set(matches.map((m) => m.id)), [matches]);

  /// Screen coordinates to the graph's own space, through the group's own
  /// matrix so it stays right at any zoom and pan. The depth has to be
  /// supplied: a pointer has two numbers and the graph has three.
  function toGraphSpace(event: React.PointerEvent, depth: number) {
    const svg = svgRef.current;
    const matrix = groupRef.current?.getScreenCTM();
    if (!svg || !matrix) return null;
    const point = svg.createSVGPoint();
    point.x = event.clientX;
    point.y = event.clientY;
    const mapped = point.matrixTransform(matrix.inverse());
    return unproject({ x: mapped.x, y: mapped.y }, depth, viewRef.current);
  }

  function onPointerDown(event: React.PointerEvent, id: string | null) {
    (event.currentTarget as Element).setPointerCapture?.(event.pointerId);
    dragging.current = { id, travelled: 0 };
    draggedFar.current = false;
  }

  function onPointerMove(event: React.PointerEvent) {
    const drag = dragging.current;
    if (!drag) return;

    // A few pixels of travel is a click with a shaky hand, not a drag.
    // Nothing at all happens below that threshold: pinning the node and
    // waking the simulation on the way down meant that merely *selecting*
    // something made the whole graph shift under the pointer.
    drag.travelled += Math.abs(event.movementX) + Math.abs(event.movementY);
    if (drag.travelled <= CLICK_SLOP) return;
    if (!draggedFar.current) {
      draggedFar.current = true;
      // A drag takes over from whatever the view was doing on its own.
      panTarget.current = null;
      if (drag.id === null) tracking.current = false;
      if (drag.id) {
        const node = nodesRef.current.find((n) => n.id === drag.id);
        if (node) node.pinned = true;
      }
    }

    if (drag.id === null) {
      if (deepRef.current) {
        // With depth on, the background is the thing you turn rather than
        // the thing you slide: once the graph has a shape, looking at it
        // from another side is the gesture you reach for, and zoom plus
        // focus already get you anywhere panning would have.
        viewRef.current = {
          yaw: viewRef.current.yaw + event.movementX * TURN_PER_PIXEL,
          pitch: Math.max(
            -MAX_PITCH,
            Math.min(MAX_PITCH, viewRef.current.pitch + event.movementY * TURN_PER_PIXEL),
          ),
        };
        setView(viewRef.current);
        setNodes([...nodesRef.current]);
        return;
      }
      // Panning works in screen deltas divided by the group's current
      // scale, not in graph coordinates: moving the thing you are
      // measuring against feeds back on itself.
      const scale = groupRef.current?.getScreenCTM()?.a ?? 1;
      panRef.current = {
        x: panRef.current.x + event.movementX / scale,
        y: panRef.current.y + event.movementY / scale,
      };
      setPan(panRef.current);
      return;
    }

    const node = nodesRef.current.find((n) => n.id === drag.id);
    if (!node) return;
    const { depth } = project(node, viewRef.current);
    const point = toGraphSpace(event, depth);
    if (!point) return;
    node.x = point.x;
    node.y = point.y;
    node.z = point.z;
    node.vx = 0;
    node.vy = 0;
    node.vz = 0;
    setNodes([...nodesRef.current]);
  }

  function onPointerUp() {
    const movedANode = draggedFar.current && dragging.current?.id;
    dragging.current = null;
    // A node that was actually moved deserves the rest of the graph
    // making room for where it ended up — and with depth on, the drift
    // picks up again from here either way.
    if (movedANode || deepRef.current) animate();
  }

  /// A node answers a click differently depending on whether you are
  /// already looking at it: the first click says "this one", the second
  /// says "open it". Nothing is ever one click away from navigating off
  /// the graph, which is the whole difference between a picture and
  /// something you can walk.
  function onNodeClick(id: string) {
    if (draggedFar.current) {
      draggedFar.current = false;
      return;
    }
    if (focused === id) onOpenEntity(id);
    else setFocused(id);
  }

  if (error) return <p className="dim">Couldn't load the graph: {error}</p>;
  if (!graph) return <p className="dim">Laying out your graph…</p>;

  if (graph.nodes.length === 0) {
    return (
      <p className="dim">
        Nothing to draw yet. Entities and the relations between them are derived from what you
        capture — a handful of notes and there will be something here.
      </p>
    );
  }

  const hidden = graph.nodes.length - visible.nodes.length;
  // The long tail stays reachable, but folded: a type that carries one
  // entity is a legend entry for a single dot. A type you switched off
  // stays on the row whatever its rank — a switch you cannot find again
  // is a switch that only goes one way.
  const shownTypes = allTypes
    ? types
    : types.filter(([type], i) => i < MAX_TYPE_CHIPS || hiddenTypes.has(type));

  /// Everything the drawing needs, projected once per frame and sorted
  /// back to front: SVG has no depth buffer, so the order elements are
  /// written in *is* the depth order.
  const placed = nodes
    .map((node) => ({ node, at: project(node, view) }))
    .sort((a, b) => b.at.depth - a.at.depth);
  const byId = new Map(placed.map((p) => [p.node.id, p]));
  const focusedNode = focused ? byId.get(focused) : undefined;

  // Labels for everything at 120 nodes is unreadable. The busiest keep
  // theirs; the rest get one when hovered, focused, next to what is
  // focused, or found by the search — which is every occasion you
  // actually want to read a name.
  const alwaysLabelled = new Set(
    [...nodes]
      .sort((a, b) => b.mentionCount + b.degree - (a.mentionCount + a.degree))
      .slice(0, 24)
      .map((n) => n.id),
  );

  return (
    <>
      <div className="graph-toolbar">
        <span className="dim">
          {visible.nodes.length} {visible.nodes.length === 1 ? "entity" : "entities"},{" "}
          {visible.edges.length} {visible.edges.length === 1 ? "relation" : "relations"}
          {hidden > 0 && ` — ${hidden} filtered out`}
          {graph.omitted_nodes > 0 && `, ${graph.omitted_nodes} rarer ones never fetched`}
        </span>
        <span className="dim graph-hint">
          {deep ? "drag to turn" : "drag to move"} · scroll to zoom · click a node, then click
          it again to open
        </span>
      </div>

      <div className="graph-search">
        <span className="kicker">&gt;</span>
        <input
          className="search-input"
          value={query}
          placeholder="Find an entity in the graph…"
          onChange={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setQuery("");
            if (e.key === "Enter" && matches.length > 0) {
              setFocused(matches[0].id);
              setQuery("");
            }
          }}
        />
        {matches.length > 0 && (
          <span className="graph-matches">
            {matches.map((match) => (
              <span
                key={match.id}
                className="filter-chip graph-chip"
                style={{ borderColor: entityColor(match.entity_type) }}
                onClick={() => {
                  setFocused(match.id);
                  setQuery("");
                }}
              >
                <span
                  className="graph-chip-dot"
                  style={{ background: entityColor(match.entity_type) }}
                />
                {match.name}
              </span>
            ))}
          </span>
        )}
      </div>

      {/* What the graph is, on one row, always in the same place — the
          type chips below it are a legend that grows with the corpus, and
          a switch that moves is a switch you have to look for. */}
      <div className="graph-modes">
        <span
          className={`filter-chip${minMentions > 1 ? " active" : ""}`}
          onClick={() => setMinMentions((m) => (m > 1 ? 1 : 2))}
          title="Entities mentioned only once are usually a passing remark"
        >
          {minMentions > 1 ? "[x]" : "[ ]"} said more than once
        </span>

        <span
          className={`filter-chip${connectedOnly ? " active" : ""}`}
          onClick={() => setConnectedOnly((c) => !c)}
          title="Hide entities with no relation to anything else"
        >
          {connectedOnly ? "[x]" : "[ ]"} connected only
        </span>

        <span
          className={`filter-chip${deep ? " active" : ""}`}
          onClick={() => setDepth(!deep)}
          title="Let the layout use depth as well. Drag the background to turn it."
        >
          {deep ? "[x]" : "[ ]"} depth
        </span>

        {(hiddenTypes.size > 0 || minMentions > 1 || connectedOnly) && (
          <span
            className="dim link graph-reset"
            onClick={() => {
              setHiddenTypes(new Set());
              setMinMentions(1);
              setConnectedOnly(false);
            }}
          >
            [ show everything ]
          </span>
        )}
      </div>

      <div className="graph-filters">
        {shownTypes.map(([type, count]) => {
          const off = hiddenTypes.has(type);
          return (
            <span
              key={type}
              className={`filter-chip graph-chip${off ? " graph-chip-off" : ""}`}
              onClick={() =>
                setHiddenTypes((previous) => {
                  const next = new Set(previous);
                  if (next.has(type)) next.delete(type);
                  else next.add(type);
                  return next;
                })
              }
            >
              <span className="graph-chip-dot" style={{ background: entityColor(type) }} />
              {type} {count}
            </span>
          );
        })}

        {types.length > MAX_TYPE_CHIPS && (
          <span className="filter-chip dim" onClick={() => setAllTypes((a) => !a)}>
            {allTypes ? "[ fewer types ]" : `[ ${types.length - MAX_TYPE_CHIPS} more types ]`}
          </span>
        )}
      </div>

      <svg
        ref={svgRef}
        className="relations-graph"
        viewBox="-420 -300 840 600"
        preserveAspectRatio="xMidYMid meet"
        onPointerDown={(e) => onPointerDown(e, null)}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={onPointerUp}
        onClick={() => {
          // A click on the background means "nothing in particular", which
          // is a thing you have to be able to say.
          if (!draggedFar.current) setFocused(null);
          draggedFar.current = false;
        }}
        onWheel={(e) => {
          setZoom((z) => Math.min(3, Math.max(0.25, z * (e.deltaY < 0 ? 1.1 : 0.9))));
        }}
      >
        <g ref={groupRef} transform={`scale(${zoom}) translate(${pan.x} ${pan.y})`}>
          {edges.map((edge) => {
            const a = byId.get(edge.from);
            const b = byId.get(edge.to);
            if (!a || !b) return null;
            const dimmed =
              neighbours && !(neighbours.has(edge.from) && neighbours.has(edge.to));
            const title = `${a.node.name} — ${edge.relationType} → ${b.node.name}${
              edge.weight > 1 ? ` (said ${edge.weight}×)` : ""
            }`;
            return (
              <g
                key={`${edge.from}-${edge.to}-${edge.relationType}`}
                className={dimmed ? "graph-dimmed" : undefined}
              >
                <line
                  className="graph-edge"
                  x1={a.at.x}
                  y1={a.at.y}
                  x2={b.at.x}
                  y2={b.at.y}
                  strokeWidth={
                    Math.min(1 + Math.log2(edge.weight), 3) * ((a.at.scale + b.at.scale) / 2)
                  }
                >
                  <title>{title}</title>
                </line>
              </g>
            );
          })}

          {placed.map(({ node, at }) => {
            const radius = nodeRadius(node) * at.scale;
            const dimmed = neighbours && !neighbours.has(node.id);
            const isFocused = focused === node.id;
            const isMatch = matchIds.has(node.id);
            const showLabel =
              alwaysLabelled.has(node.id) ||
              hovered === node.id ||
              isMatch ||
              (neighbours?.has(node.id) ?? false);
            return (
              <g
                key={node.id}
                className={`graph-node${dimmed ? " graph-dimmed" : ""}${
                  isFocused ? " graph-node-focused" : ""
                }`}
                // Depth as fading on top of depth as size. Either alone is
                // ambiguous at a glance — a small circle is either far
                // away or rarely mentioned — and together they read as
                // distance. Only with depth on: flat, every node is at
                // the same scale, and fading them all equally would just
                // make the whole graph paler.
                opacity={dimmed || !deep ? undefined : depthOpacity(at.scale)}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  onPointerDown(e, node.id);
                }}
                onPointerEnter={() => setHovered(node.id)}
                onPointerLeave={() => setHovered(null)}
                onClick={(e) => {
                  e.stopPropagation();
                  onNodeClick(node.id);
                }}
              >
                {(isFocused || isMatch) && (
                  <circle
                    className="graph-ring"
                    cx={at.x}
                    cy={at.y}
                    r={radius + 5}
                    fill="none"
                    stroke={entityColor(node.entityType)}
                  />
                )}
                <circle cx={at.x} cy={at.y} r={radius} fill={entityColor(node.entityType)}>
                  <title>{`${node.name} · ${node.entityType} · mentioned ${node.mentionCount}×`}</title>
                </circle>
                {showLabel && (
                  <text
                    className="graph-label"
                    x={at.x}
                    y={at.y + radius + 12 * at.scale}
                    fontSize={10 * at.scale}
                  >
                    {node.name}
                  </text>
                )}
              </g>
            );
          })}

          {/* Relation names last, so they sit above the nodes. They are
              written only for the edges touching the focused node: every
              edge labelled at once is a wall of text over a picture,
              where the one you asked about is a sentence.

              Offset perpendicular to the edge rather than sitting on it.
              Several relations leaving one node run close together near
              that node, and a label centred on each line lands on the
              others and on the neighbours' own names; pushing each one
              sideways off its own line separates them by the one thing
              that is guaranteed to differ — direction. */}
          {focused &&
            edges
              .filter((edge) => edge.from === focused || edge.to === focused)
              .map((edge, i, all) => {
                const a = byId.get(edge.from);
                const b = byId.get(edge.to);
                if (!a || !b) return null;
                const dx = b.at.x - a.at.x;
                const dy = b.at.y - a.at.y;
                const length = Math.hypot(dx, dy) || 1;
                // Two edges leaving the same node in almost the same
                // direction cannot be separated sideways — they would just
                // be pushed the same way. Sliding each along its own line
                // by a different amount separates them anyway.
                const along = all.length > 1 ? 0.5 + ((i % 3) - 1) * 0.11 : 0.5;
                const label =
                  edge.relationType.length > MAX_EDGE_LABEL
                    ? `${edge.relationType.slice(0, MAX_EDGE_LABEL - 1)}…`
                    : edge.relationType;
                return (
                  <text
                    key={`label-${edge.from}-${edge.to}-${edge.relationType}`}
                    className="graph-edge-label"
                    x={a.at.x + dx * along + (-dy / length) * EDGE_LABEL_OFFSET}
                    y={a.at.y + dy * along + (dx / length) * EDGE_LABEL_OFFSET}
                  >
                    {label}
                    <title>{`${a.node.name} — ${edge.relationType} → ${b.node.name}`}</title>
                  </text>
                );
              })}
        </g>
      </svg>

      {focusedNode && (
        <div className="graph-focus">
          <span
            className="graph-chip-dot"
            style={{ background: entityColor(focusedNode.node.entityType) }}
          />
          <strong>{focusedNode.node.name}</strong>
          <span className="dim">
            {focusedNode.node.entityType} · mentioned {focusedNode.node.mentionCount}× ·{" "}
            {focusedNode.node.degree}{" "}
            {focusedNode.node.degree === 1 ? "relation" : "relations"}
          </span>
          <span className="link" onClick={() => onOpenEntity(focusedNode.node.id)}>
            [ open ]
          </span>
          <span className="dim link" onClick={() => setFocused(null)}>
            [ back to everything ]
          </span>
        </div>
      )}
    </>
  );
}
