import { useEffect, useState } from "react";
import { getEcho, type EchoItem } from "./api";

/// How often to ask whether the echo has been judged yet, and for how
/// long to keep asking.
///
/// A judgement is one hosted model call — normally a second or two. The
/// ceiling is for the case where it failed and nobody is coming: after
/// this the UI stops claiming to be looking, and the next time this
/// capture is opened the backend starts a fresh attempt anyway.
const INTERVAL_MS = 1200;
const GIVE_UP_AFTER_MS = 45_000;

export interface EchoState {
  items: EchoItem[];
  /// True while the backend is still judging. An empty `items` with this
  /// set means "not yet"; empty without it means "nothing echoed", which
  /// is a real and common answer and must read differently.
  pending: boolean;
}

/// Follows a capture's echo until it is decided.
///
/// The echo is no longer part of the response that saves a capture — it
/// used to be, and waiting for it is what made saving a note take 28
/// seconds against a remote backend. It is computed behind the response
/// and collected here.
export function useEcho(eventId: string | null, startPending: boolean): EchoState {
  const [state, setState] = useState<EchoState>({ items: [], pending: startPending });

  useEffect(() => {
    if (!eventId) {
      setState({ items: [], pending: false });
      return;
    }

    setState({ items: [], pending: startPending });
    if (!startPending) return;

    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const startedAt = Date.now();

    async function ask() {
      if (!live || !eventId) return;
      try {
        const answer = await getEcho(eventId);
        if (!live) return;
        if (!answer.pending) {
          setState({ items: answer.items, pending: false });
          return;
        }
      } catch {
        // A failed poll is not a failed echo — the next one may land.
        // Only the deadline below ends the wait.
      }
      if (!live) return;
      if (Date.now() - startedAt > GIVE_UP_AFTER_MS) {
        setState({ items: [], pending: false });
        return;
      }
      timer = setTimeout(ask, INTERVAL_MS);
    }

    timer = setTimeout(ask, INTERVAL_MS);
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
    };
  }, [eventId, startPending]);

  return state;
}
