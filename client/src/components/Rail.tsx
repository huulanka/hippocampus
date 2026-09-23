import { Mark } from "../brand/Mark";
import { PendingWork } from "./PendingWork";
import { lockNow, type LockStatus } from "../desktop";
import type { Place } from "../places";
import { PLACES } from "../places";

/// The navigation, as a rail you can ignore.
///
/// It used to be a 220px column carrying eight entries, one of which was
/// permanently marked "soon" — a filing cabinet, which is the shape this
/// app exists to argue against. Five now, in 76px, which hands most of a
/// 220px column back to the graph and the notes.
///
/// The names are always drawn. A version of this slid open on hover to
/// show them, which looked clever and was not: it laid a 208px panel over
/// the left of whatever you were reading every time the pointer crossed
/// the edge, and anyone who never hovered never learned what the icons
/// meant. Twelve pixels is a cheaper answer than a hidden one.
///
/// Below 720px it becomes a bottom tab bar. That is entirely a matter of
/// CSS; the markup below is the same either way.
export function Rail({
  place,
  onGo,
  onCapture,
  lock,
  onLockChange,
}: {
  place: Place;
  onGo: (place: Place) => void;
  /// Opens the capture sheet over whatever is on screen.
  onCapture: () => void;
  lock: LockStatus | null;
  onLockChange: (status: LockStatus) => void;
}) {
  /// Shown only when there is a guard to operate. On a machine that
  /// cannot authenticate, or with the guard switched off, this would be a
  /// control that does nothing — and the state it reports would be a
  /// claim about safety that is not true.
  const armed = lock !== null && lock.enabled && lock.mechanism !== "none";

  return (
    <div className="rail">
      <nav className="rail-inner" aria-label="Main">
        <div className="rail-brand">
          <Mark size={24} title="Hippocampus" />
        </div>

        {/* Not one of the places, and above them rather than among them.
            The shortcut is still the main way in (ADR 0012) — this is the
            visible proof that there is one, and the way in on a machine
            where the shortcut is taken, on the phone, and in the browser
            build where there is no global shortcut at all. Leaving it out
            was simply a mistake: it made capturing unreachable. */}
        <button type="button" className="rail-capture" onClick={onCapture}>
          <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <rect x="9" y="2.6" width="6" height="11" rx="3" />
            <path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3.4" />
          </svg>
          <span>Capture</span>
        </button>

        <ul className="rail-places">
          {PLACES.map((entry) => (
            <li key={entry.id}>
              <button
                type="button"
                className="rail-place"
                aria-current={place === entry.id ? "page" : undefined}
                onClick={() => onGo(entry.id)}
              >
                {entry.icon}
                <span>{entry.label}</span>
              </button>
            </li>
          ))}
        </ul>

        {/* Unfinished work, in the one place that is always on screen.
            Deliberately quiet: it is absent whenever there is nothing to
            say, so the fact that it is there at all is the whole signal. */}
        <PendingWork />

        {armed && !lock.locked && (
          <button
            type="button"
            className="rail-lock"
            title={`Locks by itself after ${Math.round(lock.idle_seconds / 60)} minutes unattended`}
            onClick={() => void lockNow().then(onLockChange)}
          >
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <rect x="5" y="10.5" width="14" height="9.5" rx="2.4" />
              <path d="M8.3 10.5V7.8a3.7 3.7 0 0 1 7.4 0v2.7" />
            </svg>
            <span>Lock</span>
          </button>
        )}
      </nav>
    </div>
  );
}
