import { useEffect, useState } from "react";
import {
  getResurfaced,
  listCaptures,
  listIntentions,
  type CaptureListItem,
  type Intention,
  type Resurfaced,
  type ThreadItem,
  type UpcomingItem,
} from "../api";
import { IntentionCard, Sparkle } from "../components/Intention";
import type { MeetingView } from "../desktop";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";
import { ago, clock, until } from "../ago";
import { useForesight, useMinute } from "../useForesight";
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
  onOpenBrief,
  onGo,
}: {
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onOpenReview: () => void;
  onOpenBrief: (key: string) => void;
  onGo: (place: Place) => void;
}) {
  const [data, setData] = useState<Resurfaced | null>(null);
  const [recent, setRecent] = useState<CaptureListItem[] | null>(null);
  const [intentions, setIntentions] = useState<Intention[]>([]);
  const [error, setError] = useState<string | null>(null);
  const foresight = useForesight();
  const now = useMinute();

  useEffect(() => {
    let live = true;
    getResurfaced()
      .then((r) => live && setData(r))
      .catch((err) => live && setError(String(err)));
    listCaptures()
      .then((c) => live && setRecent(c.slice(0, 4)))
      .catch(() => live && setRecent([]));
    // Quiet on failure: a backend from before intentions has no such
    // route, and Today without this section is still Today.
    listIntentions()
      .then((i) => live && setIntentions(i))
      .catch(() => live && setIntentions([]));
    return () => {
      live = false;
    };
  }, []);

  if (error) return <p className="today-note">Couldn't load that: {error}</p>;
  if (!data) return <p className="today-note">Looking…</p>;

  const next = nextMeeting(foresight?.meetings ?? []);
  const shownIntentions = intentions.slice(0, MAX_INTENTIONS);

  return (
    <div className="column today">
      <header className="today-head">
        <p className="label-micro">{longDate()}</p>
        <h1 className="today-lead">{next ? meetingLead(next, now) : lead(data, intentions.length)}</h1>
        <button type="button" className="btn-quiet today-week" onClick={() => onOpenReview()}>
          Look back on the week
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M9.5 5.5 16 12l-6.5 6.5" />
          </svg>
        </button>
      </header>

      {(foresight?.meetings.length ?? 0) > 0 && (
        <section className="today-section">
          <h2 className="label-micro">From your calendar</h2>
          <div className="stack stack-tight">
            {foresight!.meetings.map((meeting) => (
              <MeetingCard key={meeting.key} meeting={meeting} now={now} onOpen={() => onOpenBrief(meeting.key)} />
            ))}
          </div>
        </section>
      )}

      {shownIntentions.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro intention-label">
            <Sparkle size={10} /> Still on your mind
          </h2>
          <div className="stack stack-tight">
            {shownIntentions.map((intention) => (
              <IntentionCard
                key={intention.id}
                intention={intention}
                onOpenEntity={onOpenEntity}
                onOpenCapture={onOpenCapture}
              />
            ))}
          </div>
          {intentions.length > MAX_INTENTIONS && (
            <p className="today-note">
              And {intentions.length - MAX_INTENTIONS} more — each is on the page of whoever it is about.
            </p>
          )}
        </section>
      )}

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

      {data.upcoming.length === 0 && data.threads.length === 0 && intentions.length === 0 && (
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

/// How many open intentions Today lists. The rest live on their entities'
/// pages; a to-do list that grows without bound is the thing this app
/// exists to not be.
const MAX_INTENTIONS = 5;

/// The meeting the lead should be about: the soonest one about to start
/// or running, with something to bring up. Only then does a meeting
/// outrank everything else on the screen — one with nothing to say is a
/// line further down, not the headline.
function nextMeeting(meetings: MeetingView[]): MeetingView | null {
  return (
    meetings.find((m) => (m.phase === "ahead" || m.phase === "now") && m.intentions.length > 0) ??
    null
  );
}

function meetingLead(meeting: MeetingView, now: number): string {
  const n = meeting.intentions.length;
  const what = n === 1 ? "something" : `${count(n)} things`;
  const when =
    new Date(meeting.starts_at).getTime() > now ? until(meeting.starts_at, now) : "now";
  return `${meeting.title}, ${when} — you meant to bring ${what} up.`;
}

/// One meeting from the calendar, as a line you can open.
function MeetingCard({
  meeting,
  now,
  onOpen,
}: {
  meeting: MeetingView;
  now: number;
  onOpen: () => void;
}) {
  const lit = meeting.intentions.length > 0 && !(meeting.phase === "after" && meeting.answered);
  const started = new Date(meeting.starts_at).getTime() <= now;
  const ended = new Date(meeting.ends_at).getTime() <= now;
  const when = ended ? "earlier" : started ? "now" : until(meeting.starts_at, now);
  const n = meeting.intentions.length;
  const detail = ended
    ? "Did you bring it up?"
    : n > 0
      ? sentence(n === 1 ? "something to bring up" : `${count(n)} things to bring up`)
      : sentence(
          meeting.known === 1
            ? "something you've talked about before"
            : `${count(meeting.known)} things you've talked about before`,
        );
  return (
    <button type="button" className={`card today-meeting${lit ? " today-meeting-lit" : ""}`} onClick={onOpen}>
      <span className="today-when meta">
        {when}
        <span className="today-meeting-clock">{clock(meeting.starts_at)}</span>
      </span>
      <span className="today-meeting-body">
        <span className="name today-meeting-title">{meeting.title}</span>
        <span className="today-meeting-detail">
          {lit && <Sparkle size={11} />}
          {detail}
        </span>
      </span>
    </button>
  );
}

/// The sentence at the top. Assembled from what is actually there rather
/// than picked from a list of cheerful greetings — if it says two things
/// are coming, two things are coming.
function lead(data: Resurfaced, intentions: number): string {
  const ahead = data.upcoming.length;
  const threads = data.threads.length;
  if (ahead === 0 && threads === 0 && intentions > 0) {
    return intentions === 1
      ? "One thing you meant to do, and it will come back when it matters."
      : `${sentence(count(intentions))} things you meant to do, each waiting for its moment.`;
  }
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

function when(iso: string): string {
  const date = new Date(iso);
  const days = Math.round((Date.now() - date.getTime()) / 86_400_000);
  const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  if (days <= 0) return `Today, ${time}`;
  if (days === 1) return `Yesterday, ${time}`;
  return `${date.toLocaleDateString(undefined, { day: "numeric", month: "short" })}, ${time}`;
}
