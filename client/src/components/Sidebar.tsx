import { Mascot } from "../mascot";
import type { TabId } from "../App";

interface NavItem {
  id: TabId;
  label: string;
  icon: React.ReactNode;
  /// Shown, but not reachable. These screens exist as layout and nothing
  /// else; leaving them clickable would mean showing invented data next
  /// to real data, which teaches the wrong thing about the system.
  pending?: string;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "capture",
    label: "Capture",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <rect x="5" y="1" width="5" height="9" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
        <line x1="7.5" y1="10" x2="7.5" y2="13" stroke="currentColor" strokeWidth="1.3" />
        <line x1="4.5" y1="13" x2="10.5" y2="13" stroke="currentColor" strokeWidth="1.3" />
      </svg>
    ),
  },
  {
    id: "timeline",
    label: "Timeline",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <line x1="1" y1="3" x2="14" y2="3" stroke="currentColor" strokeWidth="1.3" />
        <line x1="1" y1="7.5" x2="14" y2="7.5" stroke="currentColor" strokeWidth="1.3" />
        <line x1="1" y1="12" x2="14" y2="12" stroke="currentColor" strokeWidth="1.3" />
      </svg>
    ),
  },
  {
    id: "search",
    label: "Search",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <circle cx="6.5" cy="6.5" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
        <line x1="10" y1="10" x2="14" y2="14" stroke="currentColor" strokeWidth="1.3" />
      </svg>
    ),
  },
  {
    id: "entities",
    label: "Entities",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <circle cx="4" cy="4" r="2.4" fill="currentColor" />
        <circle cx="11" cy="4" r="2.4" fill="currentColor" />
        <circle cx="7.5" cy="11" r="2.4" fill="currentColor" />
      </svg>
    ),
  },
  {
    id: "relations",
    pending: "Every relation is already on its entity's page — the graph view itself is not built yet",
    label: "Relations",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <circle cx="3" cy="3" r="1.8" fill="currentColor" />
        <circle cx="12" cy="4" r="1.8" fill="currentColor" />
        <circle cx="7.5" cy="12" r="1.8" fill="currentColor" />
        <line x1="3" y1="3" x2="7.5" y2="12" stroke="currentColor" strokeWidth="1" />
        <line x1="12" y1="4" x2="7.5" y2="12" stroke="currentColor" strokeWidth="1" />
      </svg>
    ),
  },
  {
    id: "chat",
    pending: "Not built yet",
    label: "Chat",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <rect x="1" y="2" width="13" height="8.5" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.3" />
        <rect x="4" y="11.3" width="3" height="3" fill="currentColor" transform="rotate(45 5.5 12.8)" />
      </svg>
    ),
  },
  {
    id: "settings",
    label: "Settings",
    icon: (
      <svg width="14" height="14" viewBox="0 0 15 15">
        <circle cx="7.5" cy="7.5" r="3" fill="none" stroke="currentColor" strokeWidth="1.3" />
        <rect x="6.7" y="0.5" width="1.6" height="2.6" fill="currentColor" />
        <rect x="6.7" y="11.9" width="1.6" height="2.6" fill="currentColor" />
        <rect x="0.5" y="6.7" width="2.6" height="1.6" fill="currentColor" />
        <rect x="11.9" y="6.7" width="2.6" height="1.6" fill="currentColor" />
      </svg>
    ),
  },
];

export function Sidebar({
  activeTab,
  onSelectTab,
}: {
  activeTab: TabId;
  onSelectTab: (tab: TabId) => void;
}) {
  return (
    <div className="sidebar">
      <div className="sidebar-brand">
        <Mascot state="idle" cell={1.5} animated={false} />
        <span>Hippocampus</span>
      </div>

      <nav className="sidebar-nav">
        {NAV_ITEMS.map((item) => (
          <div
            key={item.id}
            className={`sidebar-nav-item${activeTab === item.id ? " active" : ""}${
              item.pending ? " pending" : ""
            }`}
            title={item.pending}
            onClick={() => !item.pending && onSelectTab(item.id)}
          >
            {item.icon}
            <span>{item.label}</span>
            {item.pending && <span className="sidebar-nav-soon">soon</span>}
          </div>
        ))}
      </nav>

      <div className="sidebar-new-capture" onClick={() => onSelectTab("capture")}>
        [ New Capture ]
      </div>
    </div>
  );
}
