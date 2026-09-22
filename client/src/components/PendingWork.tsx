import { useCallback, useEffect, useState } from "react";
import { getPipelineStatus, retryStuck, type PipelineStatus } from "../api";
import {
  onOutboxChange,
  outboxStatus,
  runningInDesktopApp,
  syncNow,
  type OutboxCounts,
} from "../desktop";

/// How often the backend is asked what it has not finished.
///
/// Slow on purpose. The answer is almost always "nothing", and the case
/// it exists for — a capture whose structuring failed — is something the
/// retry loop is already working on in minutes, not seconds. A faster
/// poll would only be a heartbeat against someone's NAS.
const POLL_MS = 60_000;

/// What is not finished yet, in one line.
///
/// It exists because both kinds of unfinished work were previously
/// invisible. A capture stuck in the outbox is on this Mac and nowhere
/// else; a capture the backend never structured is in the timeline and
/// findable by its words and simply has no meaning attached to it. In
/// neither case did anything on screen say so — which is the worst
/// property a memory system can have, because the user's only signal
/// that something is wrong would be noticing, months later, that
/// something they said is not there.
///
/// Silent when there is nothing to say. A status line that is permanently
/// green is a status line nobody reads on the day it turns red.
export function PendingWork() {
  const [outbox, setOutbox] = useState<OutboxCounts>({
    waiting: 0,
    failing: 0,
    last_error: null,
  });
  const [backend, setBackend] = useState<PipelineStatus>({ waiting: 0, given_up: 0 });
  const [retrying, setRetrying] = useState(false);

  useEffect(() => {
    void outboxStatus().then(setOutbox);
    return onOutboxChange(setOutbox);
  }, []);

  useEffect(() => {
    let live = true;
    const ask = () =>
      getPipelineStatus()
        .then((status) => live && setBackend(status))
        // Silently: the backend being unreachable is already said by the
        // outbox line, and saying it twice in the same corner of the
        // screen would be noise about one fact.
        .catch(() => {});

    void ask();
    const timer = setInterval(ask, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  const tryAgain = useCallback(async () => {
    if (retrying) return;
    setRetrying(true);
    try {
      // The queue first: a capture that has not arrived cannot be
      // structured, so sending is always the more useful half.
      if (runningInDesktopApp()) setOutbox(await syncNow());
      if (backend.given_up > 0) setBackend(await retryStuck());
      else setBackend(await getPipelineStatus());
    } catch {
      // Whatever went wrong is the same thing that is already on screen.
    } finally {
      setRetrying(false);
    }
  }, [backend.given_up, retrying]);

  const parts: string[] = [];
  if (outbox.waiting > 0) {
    parts.push(`${outbox.waiting} waiting to sync`);
  }
  if (backend.given_up > 0) {
    parts.push(`${backend.given_up} not processed`);
  }

  if (parts.length === 0) return null;

  // The reason, when there is one. This is the part that turns "something
  // is wrong" into something a person can act on — an expired service
  // token and a sleeping NAS look identical without it.
  const detail = outbox.last_error
    ? `${outbox.last_error}\n\nClick to try again now.`
    : "Click to try again now.";

  return (
    <div
      className={`sidebar-pending${outbox.failing > 0 || backend.given_up > 0 ? " stuck" : ""}`}
      title={detail}
      onClick={() => void tryAgain()}
    >
      <span className="sidebar-pending-count">{parts.join(" · ")}</span>
      <span className="sidebar-pending-action">
        {retrying ? "[ trying… ]" : "[ try now ]"}
      </span>
    </div>
  );
}
