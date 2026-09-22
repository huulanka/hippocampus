import { useEffect, useRef, useState } from "react";
import { unlock, type LockStatus } from "../desktop";

/// What stands in front of a screen that reads notes, while the notes are
/// locked.
///
/// It is not what protects them. The Rust side refuses the requests
/// ([`lock::allows`]); this is the part that says so in words instead of
/// leaving a screen full of failed loads. That division matters: if this
/// component were removed the notes would still be unreadable, which is
/// the only arrangement worth calling a lock.
///
/// The prompt goes up by itself on arrival. Asking someone to click a
/// button whose only possible purpose is the thing they just asked for is
/// a step that exists only to be got past — but it goes up *once*, and
/// after that there is a button, because a dialog that reappears every
/// time you cancel it is a dialog you cannot get out of.
export function LockGate({
  status,
  onUnlocked,
  what,
}: {
  status: LockStatus;
  onUnlocked: (status: LockStatus) => void;
  /// What was being asked for, so the screen can name it rather than say
  /// "this content".
  what: string;
}) {
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const askedOnArrival = useRef(false);

  const mechanism = status.mechanism === "touchid" ? "Touch ID" : "your password";

  async function ask() {
    if (asking) return;
    setAsking(true);
    setError(null);
    try {
      const next = await unlock();
      if (next.locked) setCancelled(true);
      else onUnlocked(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setAsking(false);
    }
  }

  useEffect(() => {
    if (askedOnArrival.current) return;
    askedOnArrival.current = true;
    void ask();
    // Deliberately once per mount, and deliberately with no dependencies:
    // navigating between two locked screens must not put the dialog up a
    // second time. `askedOnArrival` is what makes that true even under
    // React's double-invoked effects in development.
  }, []);

  return (
    <div className="lock-gate">
      <div className="lock-gate-mark" aria-hidden="true">
        <svg width="34" height="34" viewBox="0 0 24 24">
          <rect
            x="4.5"
            y="10.5"
            width="15"
            height="10"
            rx="1.5"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
          />
          <path
            d="M8 10.5V7.5a4 4 0 0 1 8 0v3"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
          />
        </svg>
      </div>

      <p className="lock-gate-line">{what} is locked.</p>
      <p className="dim lock-gate-why">
        Everything you have ever said is in here, on a laptop that spends its day open.
        Capturing stays open — speaking a note only ever adds to this. Reading it back
        asks for {mechanism}.
      </p>

      {error ? (
        <p className="dim lock-gate-error">{error}</p>
      ) : (
        cancelled && <p className="dim lock-gate-error">Not unlocked.</p>
      )}

      <span className={`lock-gate-button${asking ? " disabled" : ""}`} onClick={() => void ask()}>
        {asking ? "[ waiting… ]" : `[ unlock with ${mechanism} ]`}
      </span>
    </div>
  );
}
