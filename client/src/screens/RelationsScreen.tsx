import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { getGraph, mergeEntities, type Graph, type GraphNode } from "../api";
import { entityColor } from "../entityType";
import { buildMap, buildOrbit, mostRecent, type Cluster, type Orbit, type Placed } from "../orbit";
import { FoldPicker, type FoldTarget } from "../components/FoldPicker";
import { ChangesScreen } from "./ChangesScreen";

/// The graph, as one thing at a time.
///
/// See `orbit.ts` for why it is not drawn whole any more. This file is the
/// screen around that layout: what you can walk, filter, fold together and
/// take back.
///
/// It opens on the map — everything, grouped by kind — and a click on any
/// entity walks into its orbit. Opening straight into an orbit around
/// whatever came up last was tried first, and read as if the graph had
/// been filtered down to one person before you had touched anything: the
/// walk at the top showed a single name and nothing it was a part of.
/// "Everything" is now the first step of every walk, and the way back.

/// How far the pointer may travel between press and release and still
/// count as a click rather than a drag.
const CLICK_SLOP = 4;
const TOUCH_SLOP = 10;
/// How far past its drawn edge a node still takes a tap. A dot the size of
/// a letter is fine under a pointer and a guess under a fingertip; the
/// invisible ring around it is what makes it a target.
const HIT_MARGIN = 12;
/// How many entities the search offers at once.
const MAX_MATCHES = 6;
/// How many type chips before the rest are folded away. The extraction
/// invents a type per capture, so a real corpus carries thirty and twenty
/// of them are one-offs — all at once is three lines of legend above a
/// picture, which is the wrong way round.
const MAX_TYPE_CHIPS = 8;
/// How far out the stand-in for a fold target sits, between the centre's
/// halo and the first ring.
const FOLD_GHOST_AT = 0.5;
/// Past this a relation name is cut, with the whole thing in the tooltip.
const MAX_RELATION_LABEL = 22;

type View = "orbit" | "map";

export function RelationsScreen({ onOpenEntity }: { onOpenEntity: (id: string) => void }) {
  const [graph, setGraph] = useState<Graph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<View>("map");
  const [tidyOpen, setTidyOpen] = useState(false);

  /// The walk. The last entry is what the picture is centred on; earlier
  /// ones are how you got here, and clicking one goes back to it. The old
  /// screen kept a single focused id, so walking three hops in left no way
  /// back but starting again.
  const [path, setPath] = useState<GraphNode[]>([]);

  const [query, setQuery] = useState("");
  const [hiddenTypes, setHiddenTypes] = useState<Set<string>>(new Set());
  const [allTypes, setAllTypes] = useState(false);
  const [onlyRepeated, setOnlyRepeated] = useState(false);

  const [mergeSource, setMergeSource] = useState<string | null>(null);
  const [mergeTarget, setMergeTarget] = useState<FoldTarget | null>(null);
  const [merging, setMerging] = useState(false);

  const sky = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 900, height: 600 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });

  useEffect(() => {
    let live = true;
    getGraph({ limit: 400 })
      .then((loaded) => {
        if (!live) return;
        setGraph(loaded);
      })
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
  }, []);

  // The layout is built for the box it will be drawn in, so the rings use
  // whatever room there actually is rather than a guessed constant.
  useEffect(() => {
    const element = sky.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      const box = entry.contentRect;
      if (box.width > 0 && box.height > 0) setSize({ width: box.width, height: box.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
    // `graph` is in the dependencies because the sky does not exist until
    // there is one — the screen shows a line of text while loading. Without
    // it the observer attached to nothing on first run and the layout kept
    // the placeholder size for the life of the screen.
  }, [graph]);

  /// The graph with the filters applied. Rebuilding the layout from a
  /// filtered copy — rather than hiding nodes after the fact — is what
  /// makes a filtered ring close its gaps instead of leaving holes where
  /// something used to be.
  const visible = useMemo<Graph | null>(() => {
    if (!graph) return null;
    const keep = graph.nodes.filter(
      (node) =>
        !hiddenTypes.has(node.entity_type) && (!onlyRepeated || node.mention_count > 1),
    );
    const ids = new Set(keep.map((node) => node.id));
    return {
      nodes: keep,
      edges: graph.edges.filter((edge) => ids.has(edge.from) && ids.has(edge.to)),
      omitted_nodes: graph.omitted_nodes + (graph.nodes.length - keep.length),
    };
  }, [graph, hiddenTypes, onlyRepeated]);

  const centreId = path.length > 0 ? path[path.length - 1].id : null;
  const orbit = useMemo(
    () => (visible && centreId ? buildOrbit(visible, centreId, size) : null),
    [visible, centreId, size],
  );
  const clusters = useMemo(
    () => (visible && view === "map" ? buildMap(visible, size) : null),
    [visible, view, size],
  );

  const settled = useSettle(view === "orbit" ? orbit : null, size);

  /// Centring on something is the one move this screen is for, so it is
  /// also the only thing that animates: the ring re-forms around what you
  /// clicked, and the motion says you moved.
  const centreOn = useCallback((node: GraphNode) => {
    setView("orbit");
    setPath((walked) => {
      const at = walked.findIndex((step) => step.id === node.id);
      if (at >= 0) return walked.slice(0, at + 1);
      return [...walked, node];
    });
    setQuery("");
  }, []);

  const pick = useCallback(
    (node: GraphNode) => {
      if (mergeSource && node.id !== mergeSource) {
        setMergeTarget(node);
        return;
      }
      if (node.id === centreId) {
        onOpenEntity(node.id);
        return;
      }
      centreOn(node);
    },
    [mergeSource, centreId, centreOn, onOpenEntity],
  );

  const matches = useMemo(() => {
    const term = query.trim().toLowerCase();
    if (!term || !visible) return [];
    return visible.nodes
      .filter((node) => node.name.toLowerCase().includes(term))
      .sort((a, b) => b.mention_count - a.mention_count)
      .slice(0, MAX_MATCHES);
  }, [query, visible]);

  const types = useMemo(() => {
    if (!graph) return [] as [string, number][];
    const counts = new Map<string, number>();
    for (const node of graph.nodes) counts.set(node.entity_type, (counts.get(node.entity_type) ?? 0) + 1);
    return Array.from(counts.entries()).sort((a, b) => b[1] - a[1]);
  }, [graph]);
  const shownTypes = allTypes ? types : types.slice(0, MAX_TYPE_CHIPS);

  /// Back to the whole picture. The walk is dropped with it: "Everything"
  /// is its first step, so going there is going back to the start.
  const showEverything = () => {
    setView("map");
    setPath([]);
    setMergeSource(null);
    setMergeTarget(null);
  };

  const cancelMerge = () => {
    setMergeSource(null);
    setMergeTarget(null);
  };

  // A fold belongs to the entity it was started on. Walking to another
  // one leaves it behind rather than quietly carrying it along — the
  // picture would then show one thing and the panel fold another.
  useEffect(() => {
    if (mergeSource && centreId !== mergeSource) {
      setMergeSource(null);
      setMergeTarget(null);
    }
  }, [centreId, mergeSource]);

  async function confirmMerge() {
    if (!mergeSource || !mergeTarget) return;
    setMerging(true);
    try {
      await mergeEntities(mergeSource, mergeTarget.id);
      const reloaded = await getGraph({ limit: 400 });
      setGraph(reloaded);
      // The folded-away entity is gone; stand on what absorbed it.
      const target = reloaded.nodes.find((node) => node.id === mergeTarget.id);
      setPath((walked) => {
        const kept = walked.filter((step) => step.id !== mergeSource);
        return target ? [...kept.filter((step) => step.id !== target.id), target] : kept;
      });
      cancelMerge();
    } catch (err) {
      setError(String(err));
    } finally {
      setMerging(false);
    }
  }

  // — panning and zooming -------------------------------------------------
  //
  // One pointer pans, two pinch. Every pointer on the canvas is tracked,
  // because a pinch is two touches that arrive one after the other: the
  // first starts a pan, and the second turns it into a zoom about the
  // point between the two fingers, the way a map does.
  const drag = useRef<{ x: number; y: number; from: { x: number; y: number }; moved: boolean } | null>(null);
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const pinch = useRef<{ distance: number; zoom: number; mid: { x: number; y: number }; pan: { x: number; y: number } } | null>(null);
  // Mirrors of the two, so a gesture reads where it started from rather
  // than a render behind.
  const view2 = useRef({ zoom, pan });
  view2.current = { zoom, pan };

  const local = (x: number, y: number) => {
    const box = sky.current?.getBoundingClientRect();
    return { x: x - (box?.left ?? 0), y: y - (box?.top ?? 0) };
  };
  /// The zoom changed about a fixed point on screen: whatever was under
  /// that point before is still under it after.
  const zoomAbout = (
    point: { x: number; y: number },
    from: { zoom: number; pan: { x: number; y: number } },
    next: number,
  ) => {
    const clamped = Math.min(3.2, Math.max(0.45, next));
    const k = clamped / from.zoom;
    setZoom(clamped);
    setPan({ x: point.x - (point.x - from.pan.x) * k, y: point.y - (point.y - from.pan.y) * k });
  };

  function onPointerDown(event: React.PointerEvent) {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (pointers.current.size === 2) {
      const [a, b] = [...pointers.current.values()];
      pinch.current = {
        distance: Math.hypot(a.x - b.x, a.y - b.y),
        zoom: view2.current.zoom,
        mid: local((a.x + b.x) / 2, (a.y + b.y) / 2),
        pan: view2.current.pan,
      };
      // Two fingers are never a tap on whatever the first one landed on.
      if (drag.current) drag.current.moved = true;
      return;
    }
    drag.current = { x: event.clientX, y: event.clientY, from: pan, moved: false };
  }
  function onPointerMove(event: React.PointerEvent) {
    if (!pointers.current.has(event.pointerId)) return;
    pointers.current.set(event.pointerId, { x: event.clientX, y: event.clientY });

    const pinching = pinch.current;
    if (pinching && pointers.current.size >= 2) {
      const [a, b] = [...pointers.current.values()];
      const mid = local((a.x + b.x) / 2, (a.y + b.y) / 2);
      const moved = { x: pinching.pan.x + mid.x - pinching.mid.x, y: pinching.pan.y + mid.y - pinching.mid.y };
      zoomAbout(mid, { zoom: pinching.zoom, pan: moved }, pinching.zoom * (Math.hypot(a.x - b.x, a.y - b.y) / pinching.distance));
      return;
    }

    const held = drag.current;
    if (!held) return;
    const dx = event.clientX - held.x;
    const dy = event.clientY - held.y;
    // A fingertip wobbles further than a mouse does while it taps.
    const slop = event.pointerType === "touch" ? TOUCH_SLOP : CLICK_SLOP;
    if (!held.moved && (Math.abs(dx) > slop || Math.abs(dy) > slop)) {
      held.moved = true;
      // Captured only once it is a drag. Capturing on press sent the
      // release — and so the click — to the canvas instead of the node
      // under the pointer, and no node on either view could be clicked.
      (event.currentTarget as Element).setPointerCapture(event.pointerId);
    }
    if (held.moved) setPan({ x: held.from.x + dx, y: held.from.y + dy });
  }
  function onPointerUp(event: React.PointerEvent) {
    pointers.current.delete(event.pointerId);
    if (pointers.current.size < 2) pinch.current = null;
    if (pointers.current.size === 1) {
      // One finger lifted from a pinch: the other carries on as a pan
      // from where it is now, rather than jumping back to where it began.
      const [rest] = [...pointers.current.values()];
      drag.current = { x: rest.x, y: rest.y, from: view2.current.pan, moved: true };
      return;
    }
    if (pointers.current.size === 0) {
      // Cleared after the click that follows this release has been seen,
      // so a node can still tell a tap from the end of a drag.
      window.setTimeout(() => {
        if (pointers.current.size === 0) drag.current = null;
      }, 0);
    }
  }
  /// A wheel or a trackpad pinch zooms about the pointer, not the corner.
  function onWheel(event: React.WheelEvent) {
    const step = event.ctrlKey ? Math.exp(-event.deltaY / 100) : event.deltaY < 0 ? 1.09 : 1 / 1.09;
    zoomAbout(local(event.clientX, event.clientY), view2.current, view2.current.zoom * step);
  }
  const framed = zoom !== 1 || pan.x !== 0 || pan.y !== 0;

  if (error) return <p className="graph-message">Couldn't load the graph: {error}</p>;
  if (!graph) return <p className="graph-message">Drawing it…</p>;
  if (graph.nodes.length === 0) {
    return (
      <p className="graph-message">
        Nothing is connected yet. The graph fills in as the extraction finds the same things
        coming up more than once.
      </p>
    );
  }

  return (
    <div className="graph">
      <header className="graph-bar">
        <nav className="graph-walk" aria-label="Where you have been">
          {view === "orbit" && path.length > 0 && (
            <button
              type="button"
              className="icon-btn"
              aria-label="Back one step"
              onClick={() => (path.length > 1 ? setPath((walked) => walked.slice(0, -1)) : showEverything())}
            >
              <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M14.5 5.5 8 12l6.5 6.5" />
              </svg>
            </button>
          )}
          <button
            type="button"
            className="btn-quiet graph-walk-name"
            aria-current={view === "map" ? "true" : undefined}
            onClick={showEverything}
          >
            Everything
          </button>
          {view === "orbit" && path.map((step, index) => (
            <span key={step.id} className="graph-walk-step">
              <span className="graph-walk-sep">/</span>
              <button
                type="button"
                className="btn-quiet graph-walk-name"
                aria-current={index === path.length - 1 ? "true" : undefined}
                onClick={() => setPath((walked) => walked.slice(0, index + 1))}
              >
                {step.name}
              </button>
            </span>
          ))}
        </nav>

        <div className="graph-tools">
          <div className="field graph-find">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
              <circle cx="10.5" cy="10.5" r="6.4" />
              <path d="m15.2 15.2 4.4 4.4" />
            </svg>
            <label className="sr-only" htmlFor="graph-find">
              Centre the graph on an entity
            </label>
            <input
              id="graph-find"
              className="input"
              type="search"
              value={query}
              placeholder="Centre on…"
              onChange={(event) => setQuery(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") setQuery("");
                if (event.key === "Enter" && matches.length > 0) centreOn(matches[0]);
              }}
            />
          </div>

          <div className="segmented" role="group" aria-label="How to draw the graph">
            <button
              type="button"
              aria-pressed={view === "orbit"}
              onClick={() => {
                // Nothing walked into yet: start from what came up last,
                // rather than showing an orbit around nothing.
                if (path.length === 0 && visible) {
                  const start = mostRecent(visible);
                  if (start) setPath([start]);
                }
                setView("orbit");
              }}
              title="One entity, and what is a step or two from it"
            >
              Orbit
            </button>
            <button
              type="button"
              aria-pressed={view === "map"}
              onClick={showEverything}
              title="Everything at once, clustered by kind"
            >
              Map
            </button>
          </div>

          <button
            type="button"
            className="btn btn-secondary graph-tidy-toggle"
            aria-label="Tidying"
            aria-expanded={tidyOpen}
            onClick={() => setTidyOpen((open) => !open)}
            title="Duplicates, merges, and everything that has been done to the arrangement"
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <circle cx="5.5" cy="12" r="3" />
              <circle cx="18.5" cy="12" r="3" fill="currentColor" />
              <path d="M9 12h6M13 9.6 15.6 12 13 14.4" />
            </svg>
            <span className="graph-tidy-word">Tidying</span>
          </button>
        </div>
      </header>

      {matches.length > 0 && (
        <div className="graph-matches">
          {matches.map((match) => (
            <button key={match.id} type="button" className="chip" onClick={() => centreOn(match)}>
              <span className="chip-dot" style={{ background: entityColor(match.entity_type) }} />
              {match.name}
            </button>
          ))}
        </div>
      )}

      <div className="graph-sky" ref={sky}>
        <svg
          className="graph-canvas"
          width="100%"
          height="100%"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onWheel={onWheel}
          role="img"
          aria-label={
            view === "orbit" && orbit
              ? `${orbit.centre.node.name}, with ${orbit.ring.filter((p) => p.hop === 1).length} entities one step away and ${orbit.beyond} further out`
              : `Every entity, grouped into ${clusters?.length ?? 0} kinds`
          }
        >
          <g transform={`translate(${pan.x} ${pan.y}) scale(${zoom})`}>
            {view === "orbit" && orbit && (
              <OrbitDrawing
                orbit={orbit}
                at={settled}
                size={size}
                mergeSource={mergeSource}
                foldTarget={mergeTarget}
                onPick={(node) => {
                  if (!drag.current?.moved) pick(node);
                }}
              />
            )}
            {view === "map" && clusters && (
              <MapDrawing
                clusters={clusters}
                onPick={(node) => {
                  if (drag.current?.moved) return;
                  centreOn(node);
                }}
              />
            )}
          </g>
        </svg>

        {framed && (
          <button
            type="button"
            className="btn btn-secondary graph-recentre"
            onClick={() => {
              setPan({ x: 0, y: 0 });
              setZoom(1);
            }}
          >
            Fit to the window
          </button>
        )}
      {view === "orbit" && orbit && (
        <div className="graph-overlays">
          <article className="graph-focus">
            <span className="graph-focus-head">
              <span
                className="chip-dot"
                style={{ background: entityColor(orbit.centre.node.entity_type) }}
              />
              <span className="label-micro">{orbit.centre.node.entity_type}</span>
            </span>
            <h2 className="name graph-focus-name">{orbit.centre.node.name}</h2>
            <p className="meta">
              {orbit.centre.node.mention_count}{" "}
              {orbit.centre.node.mention_count === 1 ? "mention" : "mentions"} ·{" "}
              {orbit.ring.filter((placed) => placed.hop === 1).length} one step away
              {orbit.beyond > 0 && ` · ${orbit.beyond} further out`}
            </p>
            {orbit.lonely && (
              <p className="graph-lonely">
                Nothing is connected to this yet — it has been mentioned, but never alongside
                anything else.
              </p>
            )}
            <div className="graph-focus-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => onOpenEntity(orbit.centre.node.id)}
              >
                Read it
              </button>
              <button
                type="button"
                className="btn btn-secondary"
                title="Fold this into another entity. Reversible, and nothing is deleted."
                onClick={() => {
                  setMergeSource(orbit.centre.node.id);
                  setMergeTarget(null);
                }}
              >
                Fold into…
              </button>
            </div>
          </article>

          {mergeSource === orbit.centre.node.id && (
            <FoldPicker
              source={orbit.centre.node}
              target={mergeTarget}
              merging={merging}
              onTarget={setMergeTarget}
              onConfirm={() => void confirmMerge()}
              onCancel={cancelMerge}
            />
          )}
        </div>
      )}

      {view === "orbit" && (
        <aside className="graph-legend">
          <span className="label-micro">How to read it</span>
          <span className="graph-legend-row">
            <svg width="24" height="12" aria-hidden="true">
              <circle cx="6" cy="6" r="5.4" fill="var(--clay)" />
              <circle cx="18" cy="6" r="3.2" fill="var(--clay)" />
            </svg>
            Size is how often you said it
          </span>
          <span className="graph-legend-row">
            <svg width="24" height="12" aria-hidden="true">
              <circle cx="6" cy="6" r="5.4" fill="var(--clay)" />
              <circle cx="18" cy="6" r="4.6" fill="none" stroke="var(--clay)" strokeWidth="1.4" />
            </svg>
            Hollow has gone quiet
          </span>
          <span className="graph-legend-row">
            <svg width="24" height="12" aria-hidden="true">
              <path d="M2 6h20" stroke="var(--line-strong)" strokeWidth="1.7" />
            </svg>
            Distance is steps, not similarity
          </span>
        </aside>
      )}
      </div>

      <div className="graph-filters">
        <label className="check">
          <input
            type="checkbox"
            checked={onlyRepeated}
            onChange={() => setOnlyRepeated((only) => !only)}
          />
          Said more than once
        </label>
        {shownTypes.map(([type, count]) => {
          const off = hiddenTypes.has(type);
          return (
            <button
              key={type}
              type="button"
              className="chip chip-hides"
              aria-pressed={!off}
              onClick={() =>
                setHiddenTypes((previous) => {
                  const next = new Set(previous);
                  if (next.has(type)) next.delete(type);
                  else next.add(type);
                  return next;
                })
              }
            >
              <span className="chip-dot" style={{ background: entityColor(type) }} />
              {type}
              <span className="chip-count">({count})</span>
            </button>
          );
        })}
        {types.length > MAX_TYPE_CHIPS && (
          <button type="button" className="btn-quiet graph-more-types" onClick={() => setAllTypes((a) => !a)}>
            {allTypes ? "Fewer kinds" : `${types.length - MAX_TYPE_CHIPS} more kinds`}
          </button>
        )}
        {(hiddenTypes.size > 0 || onlyRepeated) && (
          <button
            type="button"
            className="btn-quiet"
            onClick={() => {
              setHiddenTypes(new Set());
              setOnlyRepeated(false);
            }}
          >
            Show everything
          </button>
        )}
      </div>

      {tidyOpen && (
        <aside className="graph-tidy" aria-label="Tidying">
          <header className="graph-tidy-head">
            <span className="label-micro">Tidying</span>
            <button type="button" className="icon-btn" aria-label="Close tidying" onClick={() => setTidyOpen(false)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </header>
          <div className="graph-tidy-body">
            <ChangesScreen onOpenEntity={onOpenEntity} />
          </div>
        </aside>
      )}
    </div>
  );
}

/// Where every node is *right now*, as against where the layout says it
/// belongs.
///
/// Re-centring is the one move this screen is for, so it is the one thing
/// that animates. Nodes ease toward their new places; ones that were not
/// in the previous picture start at the middle and travel out, which reads
/// as the ring forming rather than as a new image being pasted over the
/// old one. Anyone who has asked for less movement gets the new positions
/// immediately.
function useSettle(orbit: Orbit | null, size: { width: number; height: number }) {
  const at = useRef(new Map<string, { x: number; y: number; r: number }>());
  const [, repaint] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    if (!orbit) {
      at.current.clear();
      return;
    }
    const targets = new Map<string, { x: number; y: number; r: number }>();
    const place = (placed: Placed) => targets.set(placed.node.id, { x: placed.x, y: placed.y, r: placed.r });
    place(orbit.centre);
    orbit.ring.forEach(place);

    for (const id of Array.from(at.current.keys())) {
      if (!targets.has(id)) at.current.delete(id);
    }
    for (const [id, target] of targets) {
      if (!at.current.has(id)) {
        at.current.set(id, { x: size.width / 2, y: size.height / 2, r: 0 });
      }
      void target;
    }

    const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (reduced) {
      for (const [id, target] of targets) at.current.set(id, { ...target });
      repaint();
      return;
    }

    let frame = 0;
    let last = performance.now();
    const step = (now: number) => {
      const elapsed = Math.min((now - last) / 1000, 0.05);
      last = now;
      // Exponential approach rather than a fixed duration: the distance
      // decides how long it takes, so a small shuffle is quick and a jump
      // across the picture is not.
      const k = 1 - Math.exp(-elapsed * 11);
      let moving = false;
      for (const [id, target] of targets) {
        const current = at.current.get(id)!;
        current.x += (target.x - current.x) * k;
        current.y += (target.y - current.y) * k;
        current.r += (target.r - current.r) * k;
        if (
          Math.abs(target.x - current.x) > 0.4 ||
          Math.abs(target.y - current.y) > 0.4 ||
          Math.abs(target.r - current.r) > 0.3
        ) {
          moving = true;
        } else {
          current.x = target.x;
          current.y = target.y;
          current.r = target.r;
        }
      }
      repaint();
      if (moving) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [orbit, size.width, size.height]);

  return at.current;
}

function OrbitDrawing({
  orbit,
  at,
  size,
  mergeSource,
  foldTarget,
  onPick,
}: {
  orbit: Orbit;
  at: Map<string, { x: number; y: number; r: number }>;
  size: { width: number; height: number };
  mergeSource: string | null;
  foldTarget: FoldTarget | null;
  onPick: (node: GraphNode) => void;
}) {
  const where = (placed: Placed) => at.get(placed.node.id) ?? placed;
  const centre = where(orbit.centre);
  const { one, two } = orbit.radii;

  return (
    <>
      {/* The two rings, drawn faintly and named once. Without them
          "distance is steps" is a claim in a legend; with them it is
          visible in the picture. */}
      <circle className="graph-ring" cx={centre.x} cy={centre.y} r={one} />
      <circle className="graph-ring graph-ring-far" cx={centre.x} cy={centre.y} r={two} />
      <text className="graph-ring-label" x={centre.x + one + 8} y={centre.y - 4}>
        ONE STEP
      </text>
      <text className="graph-ring-label graph-ring-label-far" x={centre.x + two + 8} y={centre.y - 4}>
        TWO STEPS
      </text>

      <g className="graph-spokes">
        {orbit.spokes.map((spoke) => {
          const a = where(spoke.from);
          const b = where(spoke.to);
          return (
            <line
              key={`${spoke.from.node.id}-${spoke.to.node.id}-${spoke.relation}`}
              className={spoke.labelled ? "graph-spoke" : "graph-spoke graph-spoke-thin"}
              x1={a.x}
              y1={a.y}
              x2={b.x}
              y2={b.y}
            />
          );
        })}
      </g>

      {/* Only the inner spokes are named, and there are at most a dozen of
          them — which is exactly why they can be. */}
      <g className="graph-relations">
        {relationLabels(orbit, where, centre).map((label) => (
          <text key={label.key} className="graph-relation" x={label.x} y={label.y} dy={label.dy}>
            {label.text}
            <title>{label.title}</title>
          </text>
        ))}
      </g>

      <g className="graph-nodes">
        {orbit.ring.map((placed) => {
          const position = where(placed);
          const colour = entityColor(placed.node.entity_type);
          const label = nameSpot(placed, position);
          return (
            <g
              key={placed.node.id}
              className={`graph-node${placed.hop === 2 ? " graph-node-far" : ""}${
                mergeSource === placed.node.id || foldTarget?.id === placed.node.id
                  ? " graph-node-merging"
                  : ""
              }`}
              onClick={() => onPick(placed.node)}
            >
              <circle className="graph-hit" cx={position.x} cy={position.y} r={position.r + HIT_MARGIN} />
              <circle
                cx={position.x}
                cy={position.y}
                r={position.r}
                fill={placed.warm ? colour : "transparent"}
                fillOpacity={placed.warm ? 1 : 0.22}
                stroke={colour}
                strokeWidth={placed.warm ? 0 : 1.6}
              />
              <text
                className={placed.hop === 1 ? "graph-name" : "graph-name graph-name-far"}
                x={label.x}
                y={label.y}
                style={{ textAnchor: label.anchor }}
              >
                {placed.node.name}
              </text>
              <title>{`${placed.node.name} — ${placed.node.entity_type}, ${placed.node.mention_count}×`}</title>
            </g>
          );
        })}
      </g>

      {foldTarget && foldTarget.id !== orbit.centre.node.id && (
        <FoldLine orbit={orbit} centre={centre} where={where} target={foldTarget} />
      )}

      <g className="graph-node graph-node-centre" onClick={() => onPick(orbit.centre.node)}>
        <circle
          className="graph-halo"
          cx={centre.x}
          cy={centre.y}
          r={centre.r * 1.34}
          fill={entityColor(orbit.centre.node.entity_type)}
        />
        <circle
          cx={centre.x}
          cy={centre.y}
          r={centre.r}
          fill={entityColor(orbit.centre.node.entity_type)}
        />
        <text className="graph-centre-count" x={centre.x} y={centre.y + 5}>
          {orbit.centre.node.mention_count}×
        </text>
        {/* The thing everything else is drawn around carries its name in
            the picture, not only in the card below — otherwise the one
            node that matters most is the only one without a label. */}
        <text className="graph-centre-name" x={centre.x} y={centre.y - centre.r * 1.34 - 12}>
          {orbit.centre.node.name}
        </text>
        <title>{`${orbit.centre.node.name} — click to open it`}</title>
      </g>

      {orbit.beyond > 0 && (
        <text className="graph-horizon" x={centre.x} y={Math.min(centre.y + two + 34, size.height - 14)}>
          {orbit.beyond} MORE FURTHER OUT — CENTRE ON ONE TO SEE ITS OWN ORBIT
        </text>
      )}
    </>
  );
}

/// Where a node's name goes: outward from the centre, the way the ring
/// itself points. Names on the right start beside their dot and run away
/// from the middle, names on the left end beside it, and only the ones
/// near the top and bottom sit centred above or below. Every name used to
/// be centred under its dot, and two neighbours on the upper ring —
/// "Freibad" and "Kaffee mit Kardamom rösten" — wrote over each other.
function nameSpot(
  placed: Placed,
  at: { x: number; y: number; r: number },
): { x: number; y: number; anchor: "start" | "middle" | "end" } {
  const cos = Math.cos(placed.angle);
  const sin = Math.sin(placed.angle);
  if (cos > 0.35) return { x: at.x + at.r + 6, y: at.y + 4, anchor: "start" };
  if (cos < -0.35) return { x: at.x - at.r - 6, y: at.y + 4, anchor: "end" };
  return sin >= 0
    ? { x: at.x, y: at.y + at.r + (placed.hop === 1 ? 15 : 13), anchor: "middle" }
    : { x: at.x, y: at.y - at.r - 7, anchor: "middle" };
}

/// Where each relation name sits on its spoke.
///
/// Horizontal, not along the spoke. Rotated labels were the first thing
/// tried and the first thing thrown out: a spoke pointing straight down
/// turns its name on its side, and sideways text is not read, it is
/// decoded. Level text needs room, though, and the centre's own name sits
/// right where the spokes leave it — so each label tries a few points
/// along its spoke and takes the first that touches neither the centre's
/// name nor a label already placed. One that finds none is left to its
/// tooltip rather than drawn on top of something.
function relationLabels(
  orbit: Orbit,
  where: (placed: Placed) => { x: number; y: number; r: number },
  centre: { x: number; y: number; r: number },
) {
  type Box = { left: number; right: number; top: number; bottom: number };
  const hit = (a: Box, b: Box) =>
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;

  const nameWidth = orbit.centre.node.name.length * 11.5;
  const nameY = centre.y - centre.r * 1.34 - 12;
  const taken: Box[] = [
    { left: centre.x - nameWidth / 2, right: centre.x + nameWidth / 2, top: nameY - 22, bottom: nameY + 6 },
    { left: centre.x - centre.r * 1.4, right: centre.x + centre.r * 1.4, top: centre.y - centre.r * 1.4, bottom: centre.y + centre.r * 1.4 },
  ];
  // The inner ring's dots and names are obstacles too — the same offsets
  // the drawing uses, so a relation never lands on the name it points at.
  for (const placed of orbit.ring) {
    if (placed.hop !== 1) continue;
    const at = where(placed);
    const spot = nameSpot(placed, at);
    const width = placed.node.name.length * 7;
    const left = spot.anchor === "start" ? spot.x : spot.anchor === "end" ? spot.x - width : spot.x - width / 2;
    taken.push({ left: at.x - at.r, right: at.x + at.r, top: at.y - at.r, bottom: at.y + at.r });
    taken.push({ left, right: left + width, top: spot.y - 13, bottom: spot.y + 3 });
  }

  const out: { key: string; text: string; title: string; x: number; y: number; dy: number }[] = [];
  for (const spoke of orbit.spokes) {
    if (!spoke.labelled) continue;
    const a = where(spoke.from);
    const b = where(spoke.to);
    const text =
      spoke.relation.length > MAX_RELATION_LABEL
        ? `${spoke.relation.slice(0, MAX_RELATION_LABEL - 1)}…`
        : spoke.relation;
    const width = text.length * 5.6;
    // Above the spoke first, then below it, at each point along it.
    const spots = [0.52, 0.64, 0.4, 0.74, 0.84, 0.3].flatMap((t) => [
      { t, dy: -5 },
      { t, dy: 13 },
    ]);
    for (const { t, dy } of spots) {
      const x = a.x + (b.x - a.x) * t;
      const y = a.y + (b.y - a.y) * t;
      const box = { left: x - width / 2 - 2, right: x + width / 2 + 2, top: y + dy - 12, bottom: y + dy + 4 };
      if (taken.some((other) => hit(other, box))) continue;
      taken.push(box);
      out.push({
        key: `label-${spoke.to.node.id}`,
        text,
        title: `${spoke.from.node.name} — ${spoke.relation} → ${spoke.to.node.name}`,
        x,
        y,
        dy,
      });
      break;
    }
  }
  return out;
}

function MapDrawing({
  clusters,
  onPick,
}: {
  clusters: Cluster[];
  onPick: (node: GraphNode) => void;
}) {
  return (
    <>
      {clusters.map((cluster) => (
        <g key={cluster.entityType ?? "other"}>
          <text className="graph-cluster-label" x={cluster.label.x} y={cluster.label.y}>
            {cluster.entityType ? cluster.entityType.toUpperCase() : "ONE-OFF KINDS"}
            <tspan className="graph-cluster-count"> {cluster.members.length}</tspan>
          </text>
          {cluster.members.map((placed) => (
            <g key={placed.node.id} className="graph-node" onClick={() => onPick(placed.node)}>
              <circle className="graph-hit" cx={placed.x} cy={placed.y} r={placed.r + HIT_MARGIN} />
              <circle
                cx={placed.x}
                cy={placed.y}
                r={placed.r}
                fill={placed.warm ? entityColor(placed.node.entity_type) : "transparent"}
                fillOpacity={placed.warm ? 1 : 0.22}
                stroke={entityColor(placed.node.entity_type)}
                strokeWidth={placed.warm ? 0 : 1.4}
              />
              <title>{`${placed.node.name} — ${placed.node.entity_type}, ${placed.node.mention_count}×`}</title>
            </g>
          ))}
          {/* Names after all the dots of the disc, so no dot is painted
              over a name that was placed to avoid it. */}
          {cluster.names.map((name) => (
            <text
              key={`name-${name.id}`}
              className="graph-name graph-name-map"
              x={name.x}
              y={name.y}
              style={{ textAnchor: name.anchor }}
              onClick={() => {
                const member = cluster.members.find((placed) => placed.node.id === name.id);
                if (member) onPick(member.node);
              }}
            >
              {name.text}
            </text>
          ))}
        </g>
      ))}
    </>
  );
}

/// What the fold being chosen will do, drawn where it will happen.
///
/// A target already in the orbit gets a line to it. One that is not — the
/// usual case for a duplicate — is stood in for by a dashed ghost in the
/// widest gap of the inner ring, so the picture shows both halves of the
/// fold without walking anywhere.
function FoldLine({
  orbit,
  centre,
  where,
  target,
}: {
  orbit: Orbit;
  centre: { x: number; y: number; r: number };
  where: (placed: Placed) => { x: number; y: number; r: number };
  target: FoldTarget;
}) {
  const placed = orbit.ring.find((candidate) => candidate.node.id === target.id);
  const colour = entityColor(target.entity_type);

  if (placed) {
    const to = where(placed);
    return <line className="graph-fold-line" x1={centre.x} y1={centre.y} x2={to.x} y2={to.y} />;
  }

  const angle = widestGap(
    orbit.ring
      .filter((candidate) => candidate.hop === 1)
      .map((candidate) => {
        const at = where(candidate);
        return Math.atan2(at.y - centre.y, at.x - centre.x);
      }),
  );
  const halo = centre.r * 1.34;
  const distance = halo + (orbit.radii.one - halo) * FOLD_GHOST_AT + 18;
  const ghost = { x: centre.x + Math.cos(angle) * distance, y: centre.y + Math.sin(angle) * distance };
  const right = Math.cos(angle) >= 0;

  return (
    <g className="graph-fold">
      <line className="graph-fold-line" x1={centre.x} y1={centre.y} x2={ghost.x} y2={ghost.y} />
      <circle className="graph-fold-ghost" cx={ghost.x} cy={ghost.y} r={11} stroke={colour} />
      <text
        className="graph-name graph-fold-name"
        x={ghost.x + (right ? 17 : -17)}
        y={ghost.y + 4}
        style={{ textAnchor: right ? "start" : "end" }}
      >
        {target.name}
      </text>
    </g>
  );
}

/// The middle of the widest empty arc between the given angles. Upper
/// right when there is nothing to avoid, where the centre's own name is
/// not.
function widestGap(angles: number[]): number {
  if (angles.length === 0) return -Math.PI / 4;
  const sorted = [...angles].sort((a, b) => a - b);
  let best = { size: -1, middle: 0 };
  sorted.forEach((angle, index) => {
    const next = index + 1 < sorted.length ? sorted[index + 1] : sorted[0] + Math.PI * 2;
    if (next - angle > best.size) best = { size: next - angle, middle: angle + (next - angle) / 2 };
  });
  return best.middle;
}
