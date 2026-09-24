import { useEffect, useState, type RefObject } from "react";

/// The width below which the rail becomes a tab bar. Must match the
/// `max-width: 720px` queries in the stylesheets.
const PHONE_QUERY = "(max-width: 720px)";

/// Whether the phone layout is on. A layout question, not a platform one:
/// a Mac window dragged narrow gets the tab bar too, which is also what
/// makes the phone layout something you can look at in a browser.
export function usePhoneLayout(): boolean {
  const [phone, setPhone] = useState(() => window.matchMedia(PHONE_QUERY).matches);
  useEffect(() => {
    const query = window.matchMedia(PHONE_QUERY);
    const update = () => setPhone(query.matches);
    query.addEventListener("change", update);
    return () => query.removeEventListener("change", update);
  }, []);
  return phone;
}

/// How far in from the left edge a touch has to start to be a swipe back
/// rather than a scroll or a tap on something near the edge.
const EDGE = 24;
/// How far, or how fast, the page has to be pulled for letting go to count.
const DISTANCE = 0.33;
const VELOCITY = 0.5; // px per ms

/// Back by pulling the page off to the right from the left edge, the way
/// every navigation stack on the phone works. Without it a detail page has
/// exactly one way out, a 44pt target in the top corner, which is the one
/// place on a large phone a thumb does not reach.
///
/// The page follows the finger, so the gesture can be abandoned halfway;
/// only letting go past a third of the width, or with a flick, goes back.
export function useEdgeSwipeBack(
  area: RefObject<HTMLElement | null>,
  page: RefObject<HTMLElement | null>,
  enabled: boolean,
  onBack: () => void,
) {
  useEffect(() => {
    const el = area.current;
    if (!el || !enabled) return;

    let start: { x: number; y: number; t: number } | null = null;
    let dragging = false;
    let dx = 0;

    const set = (x: number, animate: boolean) => {
      const target = page.current;
      if (!target) return;
      target.style.transition = animate ? "transform var(--dur-base) var(--ease-out)" : "none";
      target.style.transform = x ? `translateX(${x}px)` : "";
    };

    const down = (e: TouchEvent) => {
      const touch = e.touches[0];
      if (e.touches.length !== 1 || touch.clientX > EDGE) return;
      start = { x: touch.clientX, y: touch.clientY, t: e.timeStamp };
      dragging = false;
      dx = 0;
    };
    const move = (e: TouchEvent) => {
      if (!start) return;
      const touch = e.touches[0];
      const x = touch.clientX - start.x;
      const y = touch.clientY - start.y;
      if (!dragging) {
        // Decide once, on the first real movement: mostly sideways is a
        // swipe back, anything else is left to scroll.
        if (Math.abs(x) < 8 && Math.abs(y) < 8) return;
        if (Math.abs(y) > Math.abs(x) || x < 0) {
          start = null;
          return;
        }
        dragging = true;
      }
      e.preventDefault();
      dx = Math.max(0, x);
      set(dx, false);
    };
    const up = (e: TouchEvent) => {
      if (!start) return;
      const width = el.clientWidth;
      const speed = dx / Math.max(1, e.timeStamp - start.t);
      start = null;
      if (!dragging) return;
      if (dx > width * DISTANCE || speed > VELOCITY) {
        set(width, true);
        window.setTimeout(() => {
          const target = page.current;
          if (target) {
            target.style.transition = "none";
            target.style.transform = "";
          }
          onBack();
        }, 180);
      } else {
        set(0, true);
      }
    };

    el.addEventListener("touchstart", down, { passive: true });
    el.addEventListener("touchmove", move, { passive: false });
    el.addEventListener("touchend", up);
    el.addEventListener("touchcancel", up);
    return () => {
      el.removeEventListener("touchstart", down);
      el.removeEventListener("touchmove", move);
      el.removeEventListener("touchend", up);
      el.removeEventListener("touchcancel", up);
    };
  }, [area, page, enabled, onBack]);
}

/// Whether you are reading down a page: the last scroll went down and you
/// are past the top. The phone's tab bar steps back while this is true,
/// the way the system's own bars get out of the way of a long read.
export function useReading(area: RefObject<HTMLElement | null>, enabled: boolean, page: string): boolean {
  const [reading, setReading] = useState(false);
  useEffect(() => {
    setReading(false);
    const el = area.current;
    if (!el || !enabled) return;
    let last = el.scrollTop;
    const onScroll = () => {
      const y = el.scrollTop;
      if (y < 40) setReading(false);
      else if (y > last + 6) setReading(true);
      else if (y < last - 6) setReading(false);
      if (Math.abs(y - last) > 6) last = y;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, [area, enabled, page]);
  return reading;
}
