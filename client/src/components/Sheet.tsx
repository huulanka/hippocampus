import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

/// How far the sheet has to be pulled down, from its handle, to close.
const DISMISS = 110;

/// A card that rises from the bottom over whatever was on screen, the way
/// a phone composes something new: the place you came from stays visible
/// above it, so closing it is obviously a return rather than a jump.
///
/// Only the phone uses it. On a Mac the capture sheet fills the window
/// beside the rail, which is already what a sheet is there.
///
/// Tapping the dimmed page above does not close it. A half-spoken note is
/// the one thing in this app that cannot be got back, and a stray tap
/// should not be what loses it; Cancel and pulling the handle down are
/// both deliberate.
export function Sheet({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose: () => void;
  /// Given the animated close, so a screen inside can leave the same way.
  children: (close: () => void) => ReactNode;
}) {
  const [leaving, setLeaving] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const drag = useRef<{ y: number; dy: number } | null>(null);

  /// Once only: Escape reaches both this and the screen inside, and two
  /// closes would pop the page underneath as well.
  const closing = useRef(false);
  const close = useCallback(() => {
    if (closing.current) return;
    closing.current = true;
    setLeaving(true);
    window.setTimeout(onClose, 220);
  }, [onClose]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  const follow = (dy: number, animate: boolean) => {
    const el = panel.current;
    if (!el) return;
    el.style.transition = animate ? "transform var(--dur-base) var(--ease-out)" : "none";
    el.style.transform = dy ? `translateY(${dy}px)` : "";
  };

  return (
    <div className={`sheet-layer${leaving ? " leaving" : ""}`}>
      <div className="sheet-scrim" aria-hidden="true" />
      <div ref={panel} className="sheet" role="dialog" aria-modal="true" aria-label={title}>
        <div
          className="sheet-head"
          onPointerDown={(e) => {
            if ((e.target as HTMLElement).closest("button")) return;
            drag.current = { y: e.clientY, dy: 0 };
            e.currentTarget.setPointerCapture(e.pointerId);
          }}
          onPointerMove={(e) => {
            if (!drag.current) return;
            drag.current.dy = Math.max(0, e.clientY - drag.current.y);
            follow(drag.current.dy, false);
          }}
          onPointerUp={() => {
            const dy = drag.current?.dy ?? 0;
            drag.current = null;
            if (dy > DISMISS) close();
            else follow(0, true);
          }}
          onPointerCancel={() => {
            drag.current = null;
            follow(0, true);
          }}
        >
          <span className="sheet-grabber" aria-hidden="true" />
          <button type="button" className="sheet-cancel" onClick={close}>
            Cancel
          </button>
          <h2 className="sheet-title">{title}</h2>
        </div>
        <div className="sheet-body">{children(close)}</div>
      </div>
    </div>
  );
}
