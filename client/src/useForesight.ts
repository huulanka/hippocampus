import { useEffect, useState } from "react";
import { foresightStatus, onForesight, type ForesightStatus } from "./desktop";

/// The meetings this Mac is watching, kept current: read once, then
/// pushed by the Rust side whenever what it would list changes. `null`
/// until the first answer.
export function useForesight(): ForesightStatus | null {
  const [status, setStatus] = useState<ForesightStatus | null>(null);

  useEffect(() => {
    let live = true;
    let stop: (() => void) | undefined;
    foresightStatus()
      .then((s) => live && setStatus(s))
      .catch(() => live && setStatus(null));
    onForesight((s) => live && setStatus(s)).then((off) => {
      if (live) stop = off;
      else off();
    });
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  return status;
}

/// Re-renders once a minute, for countdowns. A meeting "in 8 min" that
/// still says so five minutes later is a clock that has stopped.
export function useMinute(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);
  return now;
}
