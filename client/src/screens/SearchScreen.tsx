import { useEffect, useMemo, useRef, useState } from "react";
import {
  ask,
  listCaptures,
  listEntityTypes,
  search,
  type Answer,
  type CaptureListItem,
  type EntityTypeCount,
  type SearchResult,
} from "../api";
import { entityColor } from "../entityType";
import { usePhoneLayout } from "../phone";
import { numberSources, Sourced } from "../components/Sourced";

/// Finding something again, and — with the field empty — everything.
///
/// The empty state is the old Timeline. It was a place of its own, which
/// made it a third way of asking the one question this screen already
/// answers: where is the thing I said. Now the field starts empty and the
/// answer is your whole record, newest first, grouped by day; typing
/// narrows it. Nothing had to be dropped to get the tab count down.

/// How many type chips to offer. The extraction prompt invents types
/// freely, so the long tail is large and mostly one-off — showing every
/// type would bury the handful that are actually useful as filters.
const MAX_TYPE_CHIPS = 6;

/// Words a question starts with, in the two languages the notes are in.
const QUESTION_WORDS = new Set([
  "wer", "was", "wann", "wo", "wie", "warum", "wieso", "weshalb", "welche", "welcher", "welches",
  "wem", "wen", "wessen", "woher", "wohin", "hab", "habe", "hatte", "hatten", "gibt", "gab", "ist",
  "sind", "war", "waren", "kann", "soll", "sollte", "muss", "wollte",
  "who", "what", "when", "where", "how", "why", "which", "did", "do", "does", "is", "are",
  "was", "were", "have", "has", "had", "can", "should",
]);

/// Whether the field holds a question rather than words to look for. A
/// question gets an answer above the matches; words get only the matches.
/// Deliberately simple and visible — "Ask" is always one click away for
/// anything this misses.
export function looksLikeQuestion(query: string): boolean {
  const q = query.trim();
  if (q.endsWith("?")) return q.split(/\s+/).length >= 2;
  const words = q.toLowerCase().split(/\s+/);
  return words.length >= 3 && QUESTION_WORDS.has(words[0]);
}

const QUOTES_ONLY_KEY = "hippocampus.answer.quotesOnly";

function readQuotesOnly(): boolean {
  try {
    return window.localStorage.getItem(QUOTES_ONLY_KEY) === "1";
  } catch {
    return false;
  }
}

function writeQuotesOnly(value: boolean) {
  try {
    window.localStorage.setItem(QUOTES_ONLY_KEY, value ? "1" : "0");
  } catch {
    // A convenience; the toggle still works for this visit.
  }
}

export function SearchScreen({
  onOpenCapture,
  onOpenEntity,
}: {
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[] | null>(null);
  const [everything, setEverything] = useState<CaptureListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [types, setTypes] = useState<EntityTypeCount[]>([]);
  const [activeType, setActiveType] = useState<string | null>(null);
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [asking, setAsking] = useState(false);
  const [askError, setAskError] = useState<string | null>(null);
  const [quotesOnly, setQuotesOnly] = useState(readQuotesOnly);
  // Only the latest question may land: an answer takes seconds, and one
  // arriving for a question that has since been replaced would sit above
  // matches for something else.
  const askSeq = useRef(0);

  useEffect(() => {
    let live = true;
    listEntityTypes()
      .then((all) => live && setTypes(all.slice(0, MAX_TYPE_CHIPS)))
      .catch(() => live && setTypes([]));
    listCaptures()
      .then((all) => live && setEverything(all))
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
  }, []);

  function clearAnswer() {
    askSeq.current += 1;
    setAnswer(null);
    setAsking(false);
    setAskError(null);
  }

  async function runAsk(q: string) {
    const question = q.trim();
    if (!question) return;
    const seq = ++askSeq.current;
    setAsking(true);
    setAskError(null);
    setAnswer(null);
    try {
      const got = await ask(question);
      if (askSeq.current === seq) setAnswer(got);
    } catch (err) {
      if (askSeq.current === seq) setAskError(String(err));
    } finally {
      if (askSeq.current === seq) setAsking(false);
    }
  }

  async function run(q: string, entityType: string | null = activeType) {
    if (!q.trim()) {
      setResults(null);
      clearAnswer();
      return;
    }
    setLoading(true);
    setError(null);
    try {
      setResults(await search(q, { entityType: entityType ?? undefined }));
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  const days = useMemo(() => groupByDay(everything ?? []), [everything]);
  const searching = results !== null;
  const phone = usePhoneLayout();

  return (
    <div className="column search">
      <div className="field search-field">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true">
          <circle cx="10.5" cy="10.5" r="6.4" />
          <path d="m15.2 15.2 4.4 4.4" />
        </svg>
        <label className="sr-only" htmlFor="search-field">
          Search everything you have said
        </label>
        <input
          id="search-field"
          className="input"
          type="search"
          // On a Mac the keyboard is already there; on a phone the cursor
          // would throw the on-screen keyboard over the list you came to
          // look at before you have asked for it.
          autoFocus={!phone}
          value={query}
          placeholder="Search everything you have said…"
          onChange={(event) => {
            const next = event.currentTarget.value;
            setQuery(next);
            if (!next.trim()) {
              setResults(null);
              clearAnswer();
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              void run(query);
              if (looksLikeQuestion(query)) void runAsk(query);
              else clearAnswer();
            }
            if (event.key === "Escape") {
              setQuery("");
              setResults(null);
              clearAnswer();
            }
          }}
        />
        {query.trim() && (
          <button
            type="button"
            className="chip search-ask"
            disabled={asking}
            title="Answer this from your notes"
            onClick={() => {
              void run(query);
              void runAsk(query);
            }}
          >
            Ask
          </button>
        )}
      </div>

      {types.length > 0 && (
        <div className="search-types">
          <button
            type="button"
            className="chip"
            aria-pressed={activeType === null}
            onClick={() => {
              setActiveType(null);
              void run(query, null);
            }}
          >
            Everything
          </button>
          {types.map((type) => (
            <button
              key={type.entity_type}
              type="button"
              className="chip"
              aria-pressed={activeType === type.entity_type}
              onClick={() => {
                const next = activeType === type.entity_type ? null : type.entity_type;
                setActiveType(next);
                void run(query, next);
              }}
            >
              <span className="chip-dot" style={{ background: entityColor(type.entity_type) }} />
              {type.entity_type}
              <span className="chip-count">({type.count})</span>
            </button>
          ))}
        </div>
      )}

      {(asking || answer || askError) && (
        <AnswerCard
          answer={answer}
          asking={asking}
          error={askError}
          quotesOnly={quotesOnly}
          onQuotesOnly={(value) => {
            setQuotesOnly(value);
            writeQuotesOnly(value);
          }}
          onOpenCapture={onOpenCapture}
        />
      )}

      {loading && <p className="today-note">Looking…</p>}
      {error && <p className="today-note">That didn't work: {error}</p>}

      {searching ? (
        results.length === 0 ? (
          <p className="today-note">Nothing matches “{query}”.</p>
        ) : (
          <div className="stack stack-tight">
            {results.map((result, index) => (
              <article key={result.capture_event_id} className="card search-hit">
                <button
                  type="button"
                  className="search-hit-open"
                  onClick={() => onOpenCapture(result.capture_event_id)}
                >
                  <span className="search-hit-meta">
                    <span className="meta">{onlyDate(result.occurred_at)}</span>
                    {/* The rank, not the score. The score is a
                        reciprocal-rank-fusion sum — 0.03 is a *good* hit,
                        which reads as 3% to anyone who has ever seen a
                        percentage. A number nobody can interpret is worse
                        than no number. */}
                    <span className="meta">#{index + 1}</span>
                  </span>
                  <span className="prose search-hit-text">{result.transcript_text}</span>
                </button>
                {result.related_entities.length > 0 && (
                  <div className="search-hit-entities">
                    {result.related_entities.map((entity) => (
                      <button
                        key={entity.id}
                        type="button"
                        className="pill"
                        onClick={() => onOpenEntity(entity.id)}
                      >
                        <span
                          className="chip-dot"
                          style={{ background: entityColor(entity.entity_type) }}
                        />
                        {entity.name}
                        <span className="pill-type">{entity.entity_type}</span>
                      </button>
                    ))}
                  </div>
                )}
              </article>
            ))}
          </div>
        )
      ) : everything === null ? (
        !error && <p className="today-note">Fetching everything…</p>
      ) : everything.length === 0 ? (
        <p className="today-note">Nothing yet. Press Capture, or start a page under Write.</p>
      ) : (
        <>
          <p className="label-micro search-all">
            Everything you have said — {everything.length}{" "}
            {everything.length === 1 ? "note" : "notes"}
          </p>
          {days.map(([day, items]) => (
            <section key={day} className="search-day">
              <h2 className="label-micro search-day-label">{day}</h2>
              <div className="stack stack-tight">
                {items.map((item) => (
                  <button
                    key={item.event_id}
                    type="button"
                    className="card search-note"
                    onClick={() => onOpenCapture(item.event_id)}
                  >
                    <span className="meta">
                      {clock(item.occurred_at)} · {item.origin === "audio" ? "spoken" : "written"}
                    </span>
                    <span className="prose search-note-text">{item.transcript_text}</span>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </>
      )}
    </div>
  );
}

/// The answer, above the matches.
///
/// Sentences first, each with the numbers of the notes it rests on; then
/// those notes, numbered the same way, so a claim and its source are one
/// glance apart. "Quotes only" leaves the model's sentences out and shows
/// just your own words. When the notes do not say, it says so, and shows
/// the nearest notes instead of a guess.
function AnswerCard({
  answer,
  asking,
  error,
  quotesOnly,
  onQuotesOnly,
  onOpenCapture,
}: {
  answer: Answer | null;
  asking: boolean;
  error: string | null;
  quotesOnly: boolean;
  onQuotesOnly: (value: boolean) => void;
  onOpenCapture: (id: string) => void;
}) {
  const numbers = useMemo(() => numberSources(answer?.sentences ?? []), [answer]);
  const said = answer !== null && answer.sentences.length > 0;

  return (
    <section className="card answer" aria-live="polite" aria-busy={asking}>
      <header className="answer-head">
        <h2 className="label-micro">From your notes</h2>
        {said && (
          <label className="check answer-toggle">
            <input
              type="checkbox"
              checked={quotesOnly}
              onChange={() => onQuotesOnly(!quotesOnly)}
            />
            <span className="meta">Quotes only</span>
          </label>
        )}
      </header>

      {asking && <p className="today-note answer-reading">Reading your notes…</p>}
      {error && <p className="today-note">That didn't work: {error}</p>}

      {answer && !answer.can_answer && (
        <p className="today-note">
          No model is configured on the backend, so there is no answer — only the closest notes.
        </p>
      )}
      {answer && answer.can_answer && !said && (
        <p className="prose answer-text">
          {answer.considered === 0 ? "Nothing in your notes comes close to that." : "Your notes don't say."}
        </p>
      )}
      {said && !quotesOnly && (
        <Sourced
          sentences={answer.sentences}
          numbers={numbers}
          onOpenCapture={onOpenCapture}
          className="prose answer-text"
        />
      )}

      {answer && answer.notes.length > 0 && (
        <ol className="answer-notes">
          {!said && <li className="label-micro answer-nearest">Closest notes</li>}
          {answer.notes.map((note) => (
            <li key={note.capture_event_id}>
              <button
                type="button"
                className="answer-note"
                onClick={() => onOpenCapture(note.capture_event_id)}
              >
                <span className="answer-note-meta">
                  {numbers.has(note.capture_event_id) && (
                    <span className="source-mark" aria-hidden="true">
                      {numbers.get(note.capture_event_id)}
                    </span>
                  )}
                  <span className="meta">{onlyDate(note.occurred_at)}</span>
                </span>
                <span className="prose answer-note-text">{note.transcript_text}</span>
              </button>
            </li>
          ))}
        </ol>
      )}

      {answer && answer.model && answer.considered > 0 && (
        <p className="meta answer-provenance">
          {said
            ? `Written by ${answer.model} from ${answer.considered} ${answer.considered === 1 ? "note" : "notes"}, each sentence checked against the notes it cites.`
            : `Read ${answer.considered} ${answer.considered === 1 ? "note" : "notes"}.`}
          {answer.dropped > 0 &&
            ` ${answer.dropped} ${answer.dropped === 1 ? "sentence was" : "sentences were"} left out: the notes did not carry ${answer.dropped === 1 ? "it" : "them"}.`}
        </p>
      )}
    </section>
  );
}

function dayLabel(iso: string): string {
  const date = new Date(iso);
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(new Date()) - startOf(date)) / 86_400_000);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return date.toLocaleDateString(undefined, { weekday: "long" });
  return date.toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
}

function groupByDay(items: CaptureListItem[]): [string, CaptureListItem[]][] {
  const groups = new Map<string, CaptureListItem[]>();
  for (const item of items) {
    const label = dayLabel(item.occurred_at);
    const group = groups.get(label) ?? [];
    group.push(item);
    groups.set(label, group);
  }
  return Array.from(groups.entries());
}

function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

function onlyDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}
