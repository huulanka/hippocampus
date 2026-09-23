import { useEffect, useState, type ReactNode } from "react";
import { dismissIntention, fulfilIntention, reopenIntention, type Intention } from "../api";
import { foresightRefresh } from "../desktop";
import { entityColor } from "../entityType";
import { ago } from "../ago";

/// The mark of something you still mean to do: a four-pointed sparkle,
/// the same shape the menu bar lights up with. Ember, because ember is
/// the app's colour for time — when something was said, when it is due.
///
/// `twinkle` plays once, on arrival. Nothing here moves for as long as it
/// is on screen: that is the menu bar's lesson, and it holds on a page too.
export function Sparkle({ size = 14, twinkle = false }: { size?: number; twinkle?: boolean }) {
  return (
    <svg
      className={`sparkle${twinkle ? " sparkle-arrive" : ""}`}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
    >
      <path d="M12 0C12.6 6.6 17.4 11.4 24 12C17.4 12.6 12.6 17.4 12 24C11.4 17.4 6.6 12.6 0 12C6.6 11.4 11.4 6.6 12 0Z" />
    </svg>
  );
}

/// What a card offers to do about the intention it shows.
///
/// - `open`: the ordinary case — *Done*, and *Not one* for a misreading.
/// - `noted`: just heard in the capture on screen. Only *Not one*: it
///   cannot have been done in the second since it was said.
/// - `none`: the parent draws its own (the brief's "did you bring it up?").
export type IntentionActions = "open" | "noted" | "none";

/// One intention: your own words large, the machine's reading of them
/// small and marked as derived, who and what it is about, and when you
/// said it.
///
/// It changes its own status and says so in place, with an undo — a
/// card that vanished the moment it was ticked would leave you wondering
/// whether the click landed, and a dismissal without a way back punishes
/// the one mistake everybody makes.
export function IntentionCard({
  intention,
  actions = "open",
  arriving = false,
  onOpenEntity,
  onOpenCapture,
  onChange,
  children,
}: {
  intention: Intention;
  actions?: IntentionActions;
  /// Play the sparkle's arrival — for a card that has just appeared.
  arriving?: boolean;
  onOpenEntity?: (id: string) => void;
  onOpenCapture?: (id: string) => void;
  onChange?: (next: Intention) => void;
  children?: ReactNode;
}) {
  const [current, setCurrent] = useState(intention);
  // A parent that changed it (the brief's "yes") hands the new state down.
  useEffect(() => setCurrent(intention), [intention]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function act(call: () => Promise<Intention>) {
    setBusy(true);
    setError(null);
    try {
      const next = await call();
      setCurrent(next);
      onChange?.(next);
      // The menu bar may be lit for exactly this; let it go quiet now
      // rather than at its next look.
      void foresightRefresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  const settled = current.status !== "open";
  // Your words if they were found in what you said; otherwise the
  // machine's phrasing is all there is, and it is shown as the main line
  // rather than inventing quote marks around it.
  const headline = current.quote ?? current.text;

  return (
    <article className={`intention${settled ? " intention-settled" : ""}`} data-status={current.status}>
      <span className="intention-mark">
        <Sparkle size={15} twinkle={arriving} />
      </span>
      <div className="intention-body">
        <p className={current.quote ? "intention-quote" : "intention-text"}>
          {current.quote ? quoted(headline) : headline}
        </p>
        {current.quote && <p className="intention-reading derived">{current.text}</p>}

        <div className="intention-meta">
          {current.entities.map((entity) => (
            <button
              key={entity.id}
              type="button"
              className="btn-quiet intention-entity"
              onClick={() => onOpenEntity?.(entity.id)}
              disabled={!onOpenEntity}
            >
              <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
              {entity.name}
            </button>
          ))}
          <button
            type="button"
            className="btn-quiet intention-said"
            onClick={() => onOpenCapture?.(current.capture_event_id)}
            disabled={!onOpenCapture}
          >
            said {ago(current.said_at)}
          </button>
        </div>

        {children}
      </div>

      <div className="intention-actions">
        {settled ? (
          <>
            <span className="meta intention-outcome">
              {current.status === "fulfilled" ? "Done" : "Not one"}
            </span>
            <button
              type="button"
              className="btn-quiet"
              disabled={busy}
              onClick={() => act(() => reopenIntention(current.id))}
            >
              Undo
            </button>
          </>
        ) : (
          <>
            {actions === "open" && (
              <button
                type="button"
                className="btn btn-secondary intention-done"
                disabled={busy}
                onClick={() => act(() => fulfilIntention(current.id, "manual"))}
              >
                <Check /> Done
              </button>
            )}
            {actions !== "none" && (
              <button
                type="button"
                className="icon-btn intention-dismiss"
                disabled={busy}
                title="Not an intention — the machine misheard"
                aria-label="Not an intention"
                onClick={() => act(() => dismissIntention(current.id))}
              >
                <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                  <path d="M6 6l12 12M18 6 6 18" />
                </svg>
              </button>
            )}
          </>
        )}
      </div>
      {error && <p className="intention-error">{error}</p>}
    </article>
  );
}

/// Your words between the quote marks of the language they were said in.
/// The notes are mostly German and the interface is English; German words
/// between English marks read like a translation, which they are not.
export function quoted(words: string): string {
  const german = /[äöüß]|\b(ich|und|der|die|das|nicht|noch|mit|muss|mal|ein|eine)\b/i.test(words);
  return german ? `„${words}“` : `“${words}”`;
}

export function Check() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m5 12.5 4.5 4.5L19 7.5" />
    </svg>
  );
}
