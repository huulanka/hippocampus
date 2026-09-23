import { useEffect, useState } from "react";
import {
  getResurfaced,
  listCaptures,
  type CaptureListItem,
  type Resurfaced,
  type ThreadItem,
  type UpcomingItem,
} from "../api";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";
import type { Place } from "../places";

/// The only screen that speaks first.
///
/// Everything else in this app waits: capture waits for words, search
/// waits for a query, the graph waits for a name you already remember.
/// `docs/product.md` names the cost of *retrieval* as the thing that
/// actually kills a notes habit — an archive that only answers when asked
/// does not help on the day you needed it — and until now the app was
/// still an archive that only answered when asked.
///
/// This is deliberately not a notification (P11). Nothing buzzes and
/// nothing arrives; it is simply what is on screen when the window opens.
/// A habit that already exists carries it, instead of a new one having to
/// be built.
///
/// It also absorbs two of the old tabs. Resurface *was* this screen,
/// filed one click away where it could not do its job, and Timeline was
/// the same question a third time — so recent captures are its tail.
export function TodayScreen({
  onOpenCapture,
  onOpenEntity,
  onOpenReview,
  onGo,
}: {
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onOpenReview: () => void;
  onGo: (place: Place) => void;
}) {
  const [data, setData] = useState<Resurfaced | null>(null);
  const [recent, setRecent] = useState<CaptureListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    getResurfaced()
      .then((r) => live && setData(r))
      .catch((err) => live && setError(String(err)));
    listCaptures()
      .then((c) => live && setRecent(c.slice(0, 4)))
      .catch(() => live && setRecent([]));
    return () => {
      live = false;
    };
  }, []);

  if (error) return <p className="today-note">Couldn't load that: {error}</p>;
  if (!data) return <p className="today-note">Looking…</p>;

  return (
    <div className="column today">
      <header className="today-head">
        <p className="label-micro">{longDate()}</p>
        <h1 className="today-lead">{lead(data)}</h1>
        <button type="button" className="btn-quiet today-week" onClick={() => onOpenReview()}>
          Look back on the week
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M9.5 5.5 16 12l-6.5 6.5" />
          </svg>
        </button>
      </header>

      {data.upcoming.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro">Coming up — from your own notes</h2>
          <div className="stack stack-tight">
            {data.upcoming.map((item, index) => (
              <Upcoming
                key={`${item.capture_event_id}-${item.entity_id}-${index}`}
                item={item}
                onOpenCapture={onOpenCapture}
                onOpenEntity={onOpenEntity}
              />
            ))}
          </div>
        </section>
      )}

      {data.threads.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro">You keep coming back to this</h2>
          <div className="today-threads">
            {data.threads.map((thread) => (
              <Thread key={thread.entity_id} thread={thread} onOpenEntity={onOpenEntity} />
            ))}
          </div>
        </section>
      )}

      {recent && recent.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro">Lately</h2>
          <div className="stack stack-tight">
            {recent.map((item) => (
              <button
                key={item.event_id}
                type="button"
                className="card today-recent"
                onClick={() => onOpenCapture(item.event_id)}
              >
                <span className="meta">
                  {when(item.occurred_at)} · {item.origin === "audio" ? "spoken" : "written"}
                </span>
                <span className="prose today-recent-text">{item.transcript_text}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      {data.upcoming.length === 0 && data.threads.length === 0 && (
        <section className="today-empty">
          <p className="prose">
            Nothing has come up twice yet, and nothing you have said names a date.
          </p>
          <p className="today-note">
            This screen fills itself in as you keep talking — notes land in{" "}
            <em>coming up</em> when they mention a time, and a subject appears below once
            more than one capture has touched it. Nothing to set up.
          </p>
          <button type="button" className="btn btn-secondary" onClick={() => onGo("write")}>
            Write something instead
          </button>
        </section>
      )}
    </div>
  );
}

/// The sentence at the top. Assembled from what is actually there rather
/// than picked from a list of cheerful greetings — if it says two things
/// are coming, two things are coming.
function lead(data: Resurfaced): string {
  const ahead = data.upcoming.length;
  const threads = data.threads.length;
  if (ahead === 0 && threads === 0) return "Nothing is waiting for you.";
  if (ahead === 0) {
    return sentence(
      threads === 1 ? "one subject you keep circling." : `${count(threads)} subjects you keep circling.`,
    );
  }
  const first = ahead === 1 ? "one thing you said was coming" : `${count(ahead)} things you said were coming`;
  if (threads === 0) return sentence(`${first}.`);
  return sentence(
    `${first}, and ${threads === 1 ? "one subject" : `${count(threads)} subjects`} you keep circling.`,
  );
}

/// The lead is assembled from counts, and a count spelled out as a word
/// ("eight") starts a sentence as readily as "Two things" does — so the
/// capital goes on at the end rather than being baked into every branch.
function sentence(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

const WORDS = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"];
function count(n: number): string {
  return n < WORDS.length ? WORDS[n] : String(n);
}

function longDate(): string {
  return new Date().toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

export function Upcoming({
  item,
  onOpenCapture,
  onOpenEntity,
}: {
  item: UpcomingItem;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  return (
    <article className="card today-upcoming">
      <span className="today-when meta">{whenLabel(item) ?? item.happened_on}</span>
      <div className="today-upcoming-body">
        <p className="today-observation">{item.observation}</p>
        <div className="today-upcoming-meta">
          <button type="button" className="btn-quiet today-entity" onClick={() => onOpenEntity(item.entity_id)}>
            <span className="chip-dot" style={{ background: entityColor(item.entity_type) }} />
            {item.entity_name}
          </button>
          <button type="button" className="btn-quiet today-said" onClick={() => onOpenCapture(item.capture_event_id)}>
            said {ago(item.said_at)}
          </button>
        </div>
      </div>
    </article>
  );
}

function Thread({ thread, onOpenEntity }: { thread: ThreadItem; onOpenEntity: (id: string) => void }) {
  return (
    <button type="button" className="card today-thread" onClick={() => onOpenEntity(thread.entity_id)}>
      <span className="today-thread-head">
        <span className="chip-dot" style={{ background: entityColor(thread.entity_type) }} />
        <span className="label-micro">{thread.entity_type}</span>
      </span>
      <span className="name today-thread-name">{thread.name}</span>
      <span className="meta">
        {thread.capture_count} captures · {ago(thread.last_seen)}
      </span>
      {thread.current_summary && <span className="today-thread-summary">{thread.current_summary}</span>}
    </button>
  );
}

/// Relative rather than absolute: "three weeks ago" is the fact that
/// matters about a thread, and a date makes the reader do the arithmetic.
function ago(iso: string): string {
  const days = Math.round((Date.now() - new Date(iso).getTime()) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 31) return `${Math.round(days / 7)} weeks ago`;
  if (days < 365) return `${Math.round(days / 30)} months ago`;
  return new Date(iso).toLocaleDateString(undefined, { month: "short", year: "numeric" });
}

function when(iso: string): string {
  const date = new Date(iso);
  const days = Math.round((Date.now() - date.getTime()) / 86_400_000);
  const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (days <= 0) return `Today, ${time}`;
  if (days === 1) return `Yesterday, ${time}`;
  return `${date.toLocaleDateString(undefined, { day: "numeric", month: "short" })}, ${time}`;
}
