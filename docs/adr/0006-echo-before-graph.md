# ADR 0006: Echo before graph

## Status
Accepted (2026-09-20)

## Context
The stated product goal is connection, not just finding things again. There
are two mechanisms for it, at very different cost.

**Echo** shows earlier captures close in meaning right after capturing. It
rests on embedding similarity alone. Its core already runs (`embedding.rs`,
`pgvector`, `capture_search`). It works from about twenty captures on and
cannot invent anything, because it only ever shows your own words.

**Graph** means typed entities and relations you can navigate. It depends
entirely on reliable entity resolution. At this point that is an exact
string comparison, and relations only come out of a single transcript, so
the graph is a scattering of unconnected stars.

## Decision
Echo is built first and has to carry the product on its own. The graph
remains a goal, but is only developed further once there is capture volume
and a working review step, and is re-evaluated at around a thousand
captures.

In practice: no further work on entity or relation interfaces until the
capture path and echo are in daily use.

## Consequences
- The fastest route to value you can actually feel; echo is days away, not
  months.
- The existing structuring keeps running in the background and collects
  data on which the quality of extraction can later be judged against real
  material.
- Risk: echo may turn out to be enough and the graph may never get built.
  That would not be a failure but a result, established on real data
  rather than assumed in advance.

## Later

The graph did get built, once consolidation could connect things across
notes (`docs/consolidation.md`).
