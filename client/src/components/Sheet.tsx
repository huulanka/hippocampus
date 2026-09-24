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
/// should not be what loses it; the close button and pulling the handle
/// down are both deliberate.
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
  const layer = useRef<HTMLDivElement>(null);
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

  /// The on-screen keyboard. iOS makes room for it by sliding the whole
  /// page up underneath, which carried the sheet's top edge off the screen
  /// with it. Instead the sheet is fitted to what is still visible — the
  /// visual viewport — so it simply gets shorter and sits on the keyboard.
  useEffect(() => {
    const viewport = window.visualViewport;
    const el = layer.current;
    if (!viewport || !el) return;
    const fit = () => {
      el.style.top = `${viewport.offsetTop}px`;
      el.style.height = `${viewport.height}px`;
      el.classList.toggle("keyboard", window.innerHeight - viewport.height > 120);
    };
    fit();
    viewport.addEventListener("resize", fit);
    viewport.addEventListener("scroll", fit);
    return () => {
      viewport.removeEventListener("resize", fit);
      viewport.removeEventListener("scroll", fit);
    };
  }, []);

  const follow = (dy: number, animate: boolean) => {
    const el = panel.current;
    if (!el) return;
    el.style.transition = animate ? "transform var(--dur-base) var(--ease-out)" : "none";
    el.style.transform = dy ? `translateY(${dy}px)` : "";
  };

  return (
    <div ref={layer} className={`sheet-layer${leaving ? " leaving" : ""}`}>
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
          <button type="button" className="glass-button sheet-close" aria-label="Close" onClick={close}>
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true">
              <path d="M6.5 6.5l11 11M17.5 6.5l-11 11" />
            </svg>
          </button>
          <h2 className="sheet-title">{title}</h2>
        </div>
        <div className="sheet-body">{children(close)}</div>
      </div>
    </div>
  );
}
