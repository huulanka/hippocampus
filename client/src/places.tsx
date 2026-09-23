import type { ReactNode } from "react";

/// The five places in the app.
///
/// There were eight, and they described a filing cabinet: Capture,
/// Resurface, Timeline, Search, Relations, Tidying, Chat, Settings. Three
/// of those were the same question asked three ways — what have I said? —
/// and one of them ("Chat") was a permanent promise marked "soon" sitting
/// in the primary navigation.
///
/// What happened to each:
///   Capture    → not a place. The shortcut is the record button (ADR
///                0012), so speaking opens over wherever you already are.
///   Resurface  → Today, which is the same thing said first rather than
///   Timeline   → asked for.  Timeline folds in as Today's tail.
///   Relations  → Graph.
///   Tidying    → a drawer inside Graph. It is about the graph, and it was
///                never a destination anyone set out for.
///   Chat       → gone until it exists.
/// The order is Today, Search, Write, Graph, Settings. Search sits second
/// because after "what is going on today" it is the question asked most
/// often, and it is the one you arrive at already knowing what you want.
/// Write and Graph follow: both are places you settle into rather than
/// glance at.
export type Place = "today" | "search" | "write" | "graph" | "settings";

export interface PlaceEntry {
  id: Place;
  label: string;
  /// What this place calls itself on the lock screen, so the gate says
  /// what was being asked for rather than "this content".
  gatedName?: string;
  icon: ReactNode;
}

const stroke = {
  fill: "none" as const,
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const PLACES: PlaceEntry[] = [
  {
    id: "today",
    label: "Today",
    gatedName: "What today looks like",
    icon: (
      <svg width="19" height="19" viewBox="0 0 24 24" {...stroke} aria-hidden="true">
        <circle cx="12" cy="12" r="3.6" />
        <path d="M12 2.6v2.6M12 18.8v2.6M2.6 12h2.6M18.8 12h2.6M5.4 5.4l1.8 1.8M16.8 16.8l1.8 1.8M18.6 5.4l-1.8 1.8M7.2 16.8l-1.8 1.8" />
      </svg>
    ),
  },
  {
    id: "search",
    label: "Search",
    gatedName: "Search",
    icon: (
      <svg width="19" height="19" viewBox="0 0 24 24" {...stroke} aria-hidden="true">
        <circle cx="10.5" cy="10.5" r="6.4" />
        <path d="m15.2 15.2 4.4 4.4" />
      </svg>
    ),
  },
  {
    id: "write",
    label: "Write",
    // Not gated: a session only ever adds, and the draft is yours before
    // it is anything else. Same argument as the capture shortcut.
    icon: (
      <svg width="19" height="19" viewBox="0 0 24 24" {...stroke} aria-hidden="true">
        <path d="M4.5 19.5h3.6L19.8 7.8a2.55 2.55 0 0 0-3.6-3.6L4.5 15.9v3.6Z" />
        <path d="m14.7 5.7 3.6 3.6" />
      </svg>
    ),
  },
  {
    id: "graph",
    label: "Graph",
    gatedName: "Your graph",
    icon: (
      <svg width="19" height="19" viewBox="0 0 24 24" {...stroke} aria-hidden="true">
        <circle cx="12" cy="12" r="2.6" />
        <circle cx="5" cy="6" r="2" />
        <circle cx="19.2" cy="7.4" r="2" />
        <circle cx="17" cy="19" r="2" />
        <path d="M6.7 7.3 10 10.3M17.4 8.6 14 10.6M13.3 14l2.6 3.3" />
      </svg>
    ),
  },
  {
    id: "settings",
    label: "Settings",
    icon: (
      <svg width="19" height="19" viewBox="0 0 24 24" {...stroke} aria-hidden="true">
        <path d="M4 7h10M18 7h2M4 17h4M12 17h8" />
        <circle cx="16" cy="7" r="2.1" />
        <circle cx="10" cy="17" r="2.1" />
      </svg>
    ),
  },
];

/// The places that stay open while the notes are locked.
///
/// Writing and capturing, because they only ever add — and because the
/// global shortcut is the main path through this app, so a gate in front
/// of it would cost the one property the whole design is built on.
/// Settings, because it holds no notes, and because being shut out of the
/// screen that configures the backend by a guard you cannot reach to
/// switch off is a trap. Switching the guard off from there authenticates
/// first, so nothing is given away by letting you in.
export const OPEN_WHILE_LOCKED: Place[] = ["write", "settings"];
