import { useEffect, useState } from "react";
import { getCaptureIntentions, type Intention } from "./api";

/// How often to ask, and for how long. Structuring is one hosted model
/// call behind the response — a few seconds against the NAS. Past the
/// ceiling the sheet stops waiting; the intentions are still written, and
/// appear on Today and on their entities' pages as soon as they are.
const INTERVAL_MS = 1500;
const GIVE_UP_AFTER_MS = 60_000;

export interface IntentionsState {
  noted: Intention[];
  reminded: Intention[];
  /// Still waiting for the structuring. Nothing is drawn meanwhile: the
  /// moment worth having is the one where a sparkle *arrives*, and a
  /// "looking for intentions…" line under every single note would be
  /// noise for the nine notes in ten that have none.
  pending: boolean;
}

const NOTHING: IntentionsState = { noted: [], reminded: [], pending: false };

/// Follows a fresh capture until its structuring has said what, if
/// anything, you meant to do — and what it reminds you of.
export function useIntentions(eventId: string | null, enabled: boolean): IntentionsState {
  const [state, setState] = useState<IntentionsState>(NOTHING);

  useEffect(() => {
    if (!eventId || !enabled) {
      setState(NOTHING);
      return;
    }
    setState({ ...NOTHING, pending: true });

    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const startedAt = Date.now();

    async function ask() {
      if (!live || !eventId) return;
      try {
        const answer = await getCaptureIntentions(eventId);
        if (!live) return;
        if (!answer.pending) {
          setState({ noted: answer.noted, reminded: answer.reminded, pending: false });
          return;
        }
      } catch {
        // A failed poll is not a failed capture; only the deadline ends it.
      }
      if (!live) return;
      if (Date.now() - startedAt > GIVE_UP_AFTER_MS) {
        setState(NOTHING);
        return;
      }
      timer = setTimeout(ask, INTERVAL_MS);
    }

    timer = setTimeout(ask, INTERVAL_MS);
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
    };
  }, [eventId, enabled]);

  return state;
}
