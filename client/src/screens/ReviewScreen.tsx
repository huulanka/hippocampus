import { useCallback, useEffect, useRef, useState } from "react";
import { getReview, writeStory, type ReviewTopic, type WeeklyReview } from "../api";
import { BackButton } from "../components/BackButton";
import { entityColor } from "../entityType";
import { calendarDaysBetween } from "../ago";
import { Upcoming } from "./TodayScreen";

/// One calendar week, looked back on.
///
/// Today answers "what is in front of me"; this answers "what was that
/// week". It is reached from Today, from the tray, and from the one
/// notification the app sends — at the time you chose for it, once a week
/// (see `docs/product.md`, P11, for why that one is allowed).
///
/// Everything below the paragraph is counted, not written: numbers and
/// your own sentences. The paragraph is the one written part, and every
/// sentence in it links the notes it came from.
export function ReviewScreen({
  week: initialWeek,
  onOpenCapture,
  onOpenEntity,
  onBack,
}: {
  /// Any day in the week to open on; none is this week.
  week?: string;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onBack: () => void;
}) {
  const [week, setWeek] = useState<string | undefined>(initialWeek);
  const [data, setData] = useState<WeeklyReview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [writing, setWriting] = useState(false);
  const [writeError, setWriteError] = useState<string | null>(null);
  /// Weeks a paragraph was already asked for in this sitting, so a week
  /// whose write-up failed is not asked again on every re-render.
  const asked = useRef(new Set<string>());

  useEffect(() => {
    let live = true;
    setData(null);
    setError(null);
    setWriteError(null);
    getReview(week)
      .then((loaded) => live && setData(loaded))
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
  }, [week]);

  const write = useCallback(async (weekStart: string) => {
    asked.current.add(weekStart);
    setWriting(true);
    setWriteError(null);
    try {
      const story = await writeStory(weekStart);
      setData((current) => (current && current.week_start === weekStart ? { ...current, story } : current));
    } catch (err) {
      setWriteError(String(err));
    } finally {
      setWriting(false);
    }
  }, []);

  // A finished week with notes and no paragraph gets one on first
  // opening: that is the moment it is read, and it is kept afterwards. A
  // week still running waits to be asked — written on Wednesday, it would
  // be missing half the week by Sunday.
  useEffect(() => {
    if (!data || data.story || !data.can_write || !data.complete || data.stock.captures === 0) return;
    if (asked.current.has(data.week_start)) return;
    void write(data.week_start);
  }, [data, write]);

  if (error) {
    return (
      <div className="column today review">
        <BackButton onBack={onBack} />
        <p className="today-note">Couldn't load that week: {error}</p>
      </div>
    );
  }
  if (!data) return <p className="today-note">Looking…</p>;

  const isThisWeek = !data.complete && data.week_start <= today() && today() <= data.week_end;
  const nothing = data.stock.captures === 0;

  return (
    <div className="column today review">
      <BackButton onBack={onBack} />
      <header className="today-head">
        <div className="review-pager">
          <button
            type="button"
            className="icon-btn"
            aria-label="The week before"
            onClick={() => setWeek(shift(data.week_start, -7))}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M14.5 5.5 8 12l6.5 6.5" />
            </svg>
          </button>
          <p className="label-micro">
            Week {isoWeek(data.week_start)} · {range(data.week_start, data.week_end)}
            {!data.complete && " · still running"}
          </p>
          <button
            type="button"
            className="icon-btn"
            aria-label="The week after"
            disabled={isThisWeek}
            onClick={() => setWeek(shift(data.week_start, 7))}
          >
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M9.5 5.5 16 12l-6.5 6.5" />
            </svg>
          </button>
        </div>
        <h1 className="today-lead">{lead(data)}</h1>
      </header>

      {!nothing && (
        <section className="today-section">
          <h2 className="label-micro">The week in a few sentences</h2>
          {data.story ? (
            <Story data={data} onOpenCapture={onOpenCapture} />
          ) : writing ? (
            <p className="today-note">Writing it up…</p>
          ) : !data.can_write ? (
            <p className="today-note">
              No model is set up on the backend, so the week is not written up. Everything
              below is counted straight from your notes.
            </p>
          ) : (
            <p className="today-note">
              {data.complete
                ? "Not written up yet."
                : "Written up once the week is over — or now, from what is there so far."}
            </p>
          )}
          {writeError && <p className="today-note">Couldn't write it up: {writeError}</p>}
          <StoryActions data={data} writing={writing} onWrite={() => void write(data.week_start)} />
        </section>
      )}

      <section className="today-section">
        <h2 className="label-micro">When you said things</h2>
        <div className="review-rhythm">
          <Bars
            label="Notes per day"
            values={data.stock.days}
            names={DAYS}
            highlight={isThisWeek ? weekdayIndex(today()) : null}
          />
          <Bars
            label="Notes per week, last eight"
            values={data.stock.recent_weeks.map((w) => w.captures)}
            names={data.stock.recent_weeks.map((w) => `W${isoWeek(w.week_start)}`)}
            highlight={data.stock.recent_weeks.length - 1}
          />
        </div>
      </section>

      <Topics title="Growing" hint="more than usual" topics={data.growing} onOpenEntity={onOpenEntity} />
      <Topics title="New this week" topics={data.new_topics} onOpenEntity={onOpenEntity} />

      {data.open_ends.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro">Open ends — announced, and nothing since</h2>
          <div className="stack stack-tight">
            {data.open_ends.map((item, index) => (
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

      {data.next_week.length > 0 && (
        <section className="today-section">
          <h2 className="label-micro">Coming the week after</h2>
          <div className="stack stack-tight">
            {data.next_week.map((item, index) => (
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

      <Topics
        title="Gone quiet"
        hint="used to come up, not for weeks"
        topics={data.quiet}
        onOpenEntity={onOpenEntity}
        quiet
      />
    </div>
  );
}

/// The paragraph, each sentence followed by the notes it rests on.
function Story({ data, onOpenCapture }: { data: WeeklyReview; onOpenCapture: (id: string) => void }) {
  const story = data.story!;
  // Numbered across the whole paragraph, in order of first citation, so
  // the same note is the same number wherever it is cited.
  const numbers = new Map<string, number>();
  for (const sentence of story.sentences) {
    for (const source of sentence.sources) {
      if (!numbers.has(source)) numbers.set(source, numbers.size + 1);
    }
  }
  return (
    <p className="prose review-story">
      {story.sentences.map((sentence, index) => (
        <span key={index}>
          {sentence.text}
          {sentence.sources.map((source) => (
            <button
              key={source}
              type="button"
              className="review-source"
              title="Open the note this rests on"
              onClick={() => onOpenCapture(source)}
            >
              {numbers.get(source)}
            </button>
          ))}{" "}
        </span>
      ))}
    </p>
  );
}

function StoryActions({
  data,
  writing,
  onWrite,
}: {
  data: WeeklyReview;
  writing: boolean;
  onWrite: () => void;
}) {
  const story = data.story;
  const unseen = story ? data.stock.captures - story.captures_seen : 0;
  if (writing || !data.can_write) return null;
  if (story && unseen <= 0) {
    return <p className="meta review-provenance">Written by {story.model}. Only what the notes say.</p>;
  }
  return (
    <div className="review-actions">
      {story && unseen > 0 && (
        <span className="meta">
          Written before {unseen === 1 ? "one more note" : `${unseen} more notes`}.
        </span>
      )}
      <button type="button" className="btn btn-secondary" onClick={onWrite}>
        {story ? "Write it again" : "Write it up"}
      </button>
    </div>
  );
}

/// A single series of bars: no legend (the heading names it), the
/// highlighted bar in the accent, every value in the tooltip and the
/// largest one written out.
function Bars({
  label,
  values,
  names,
  highlight,
}: {
  label: string;
  values: number[];
  names: string[];
  highlight: number | null;
}) {
  const max = Math.max(1, ...values);
  const peak = values.indexOf(Math.max(...values));
  return (
    <figure className="review-bars">
      <figcaption className="meta">{label}</figcaption>
      <div className="review-bars-plot" role="img" aria-label={`${label}: ${names.map((n, i) => `${n} ${values[i]}`).join(", ")}`}>
        {values.map((value, index) => (
          <div key={index} className="review-bar-slot" title={`${names[index]}: ${value} ${value === 1 ? "note" : "notes"}`}>
            <span className="review-bar-value">{index === peak && value > 0 ? value : ""}</span>
            <span
              className={`review-bar${index === highlight ? " review-bar-now" : ""}${value === 0 ? " review-bar-empty" : ""}`}
              style={{ height: `${Math.max(value === 0 ? 0 : 6, (value / max) * 100)}%` }}
            />
            <span className="review-bar-name">{names[index]}</span>
          </div>
        ))}
      </div>
    </figure>
  );
}

function Topics({
  title,
  hint,
  topics,
  onOpenEntity,
  quiet = false,
}: {
  title: string;
  hint?: string;
  topics: ReviewTopic[];
  onOpenEntity: (id: string) => void;
  quiet?: boolean;
}) {
  if (topics.length === 0) return null;
  return (
    <section className="today-section">
      <h2 className="label-micro">
        {title}
        {hint && <span className="review-hint"> — {hint}</span>}
      </h2>
      <div className="review-topics">
        {topics.map((topic) => (
          <button key={topic.entity_id} type="button" className="chip review-topic" onClick={() => onOpenEntity(topic.entity_id)}>
            <span className="chip-dot" style={{ background: entityColor(topic.entity_type) }} />
            {topic.name}
            <span className="chip-count">
              {quiet ? `last ${ago(topic.last_seen)}` : topic.before > 0 ? `${topic.this_week}× · ${topic.before} before` : `${topic.this_week}×`}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

/// The sentence at the top, from the counts — like Today's, it says only
/// what is there.
function lead(data: WeeklyReview): string {
  const { captures, spoken, typed } = data.stock;
  if (captures === 0) return data.complete ? "Nothing was captured that week." : "Nothing captured this week yet.";
  const notes = captures === 1 ? "One note" : `${captures} notes`;
  const how =
    spoken === 0 ? "all written" : typed === 0 ? "all spoken" : `${spoken} spoken, ${typed} written`;
  return data.complete ? `${notes} that week — ${how}.` : `${notes} so far — ${how}.`;
}

const DAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

function today(): string {
  return localDate(new Date());
}

function localDate(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/// Noon, so adding days never lands on the other side of a DST change.
function parse(day: string): Date {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d, 12);
}

function shift(day: string, days: number): string {
  const date = parse(day);
  date.setDate(date.getDate() + days);
  return localDate(date);
}

function weekdayIndex(day: string): number {
  return (parse(day).getDay() + 6) % 7;
}

/// ISO 8601 week number: the week containing the year's first Thursday
/// is week 1.
function isoWeek(day: string): number {
  const date = parse(day);
  const thursday = new Date(date);
  thursday.setDate(date.getDate() - weekdayIndex(day) + 3);
  const firstThursday = new Date(thursday.getFullYear(), 0, 4, 12);
  firstThursday.setDate(firstThursday.getDate() - ((firstThursday.getDay() + 6) % 7) + 3);
  return 1 + Math.round((thursday.getTime() - firstThursday.getTime()) / (7 * 86_400_000));
}

function range(start: string, end: string): string {
  const a = parse(start);
  const b = parse(end);
  const month = (d: Date) => d.toLocaleDateString(undefined, { month: "long" });
  return a.getMonth() === b.getMonth()
    ? `${a.getDate()}–${b.getDate()} ${month(b)}`
    : `${a.getDate()} ${month(a)} – ${b.getDate()} ${month(b)}`;
}

function ago(iso: string): string {
  const days = calendarDaysBetween(iso);
  if (days < 1) return "today";
  if (days === 1) return "yesterday";
  if (days < 14) return `${days} days ago`;
  return `${Math.round(days / 7)} weeks ago`;
}
