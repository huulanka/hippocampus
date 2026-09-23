import { useEffect, useMemo, useState } from "react";
import { fulfilIntention, getBrief, type Brief, type BriefEntity, type Intention } from "../api";
import { BackButton } from "../components/BackButton";
import { Check, IntentionCard, Sparkle, quoted } from "../components/Intention";
import { foresightAnswered, foresightRefresh, type MeetingView } from "../desktop";
import { entityColor } from "../entityType";
import { ago, clock, until } from "../ago";
import { useForesight, useMinute } from "../useForesight";

/// A meeting, and what you know about it.
///
/// What the lit menu bar opens. Three parts, in the order they matter
/// ten minutes before walking in: what you meant to bring up, who and
/// what the meeting is about, and your own last sentences about each —
/// verbatim, dated, one click from the note they came from. No summary,
/// for the same reason as everywhere else in this app: a briefing you
/// cannot check against what you actually said is a rumour.
///
/// After the meeting the first part turns into the one question that
/// closes the loop: did you bring it up? Yes ends the intention; not yet
/// leaves it open for the next time, and either answer stops the asking
/// for this meeting.
export function BriefScreen({
  meetingKey,
  onOpenCapture,
  onOpenEntity,
  onBack,
}: {
  meetingKey: string;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onBack: () => void;
}) {
  const status = useForesight();
  const now = useMinute();
  // Held once found: a meeting that drops out of the watcher's list while
  // you are reading about it (answered, or simply over) must not blank
  // the page under you.
  const [meeting, setMeeting] = useState<MeetingView | null>(null);
  const [brief, setBrief] = useState<Brief | null>(null);
  const [error, setError] = useState<string | null>(null);
  /// Per intention: the answer given here, after the meeting.
  const [answers, setAnswers] = useState<Record<string, "yes" | "not-yet">>({});

  useEffect(() => {
    const found = status?.meetings.find((m) => m.key === meetingKey);
    if (found) setMeeting(found);
  }, [status, meetingKey]);

  useEffect(() => {
    if (!meeting) return;
    let live = true;
    getBrief(meeting.title, meeting.people)
      .then((b) => live && setBrief(b))
      .catch((err) => live && setError(String(err)));
    return () => {
      live = false;
    };
    // The brief is asked for once per meeting, not on every status push.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meeting?.key]);

  const phase = meeting ? phaseAt(meeting, now) : null;
  const after = phase === "after";

  const intentions = brief?.intentions ?? [];
  const unanswered = intentions.filter((i) => !answers[i.id]);

  async function answer(intention: Intention, value: "yes" | "not-yet") {
    setError(null);
    try {
      if (value === "yes") {
        const done = await fulfilIntention(intention.id, "calendar");
        setBrief((b) =>
          b ? { ...b, intentions: b.intentions.map((i) => (i.id === done.id ? done : i)) } : b,
        );
      }
      const next = { ...answers, [intention.id]: value };
      setAnswers(next);
      if (meeting && intentions.every((i) => next[i.id])) {
        await foresightAnswered(meeting.key);
      }
      void foresightRefresh();
    } catch (err) {
      setError(String(err));
    }
  }

  const said = useMemo(() => groupSaid(brief), [brief]);

  if (!meeting) {
    return (
      <div className="column brief">
        <BackButton onBack={onBack} />
        <p className="brief-note">
          {status ? "That meeting is no longer coming up." : "Looking at your calendar…"}
        </p>
      </div>
    );
  }

  return (
    <div className="column brief">
      <BackButton onBack={onBack} />

      <header className="brief-head">
        <p className="label-micro brief-when" data-phase={phase ?? undefined}>
          {whenLine(meeting, phase, now)}
        </p>
        <h1 className="brief-title">{meeting.title}</h1>
        {meeting.people.length > 0 && (
          <People people={meeting.people} known={brief?.entities ?? []} onOpenEntity={onOpenEntity} />
        )}
      </header>

      {error && <p className="brief-note">Couldn't load that: {error}</p>}
      {!brief && !error && <p className="brief-note">Remembering…</p>}

      {intentions.length > 0 && (
        <section className={`brief-section brief-bring${after ? " brief-ask" : ""}`}>
          <h2 className="label-micro intention-label">
            <Sparkle size={10} />
            {after ? " Did you bring it up?" : " You meant to bring up"}
          </h2>
          <div className="stack stack-tight">
            {intentions.map((intention) => (
              <IntentionCard
                key={intention.id}
                intention={intention}
                actions={after ? "none" : "open"}
                onOpenEntity={onOpenEntity}
                onOpenCapture={onOpenCapture}
              >
                {after && (
                  <div className="brief-answer">
                    {answers[intention.id] ? (
                      <span className="meta">
                        {answers[intention.id] === "yes" ? "Done — closed." : "Kept open for next time."}
                      </span>
                    ) : (
                      <>
                        <button type="button" className="btn btn-primary" onClick={() => answer(intention, "yes")}>
                          <Check /> Yes
                        </button>
                        <button type="button" className="btn btn-secondary" onClick={() => answer(intention, "not-yet")}>
                          Not yet
                        </button>
                      </>
                    )}
                  </div>
                )}
              </IntentionCard>
            ))}
          </div>
          {after && intentions.length > 0 && unanswered.length === 0 && (
            <p className="brief-closed">
              <Sparkle size={12} twinkle /> That's that. Nothing more to ask about this one.
            </p>
          )}
        </section>
      )}

      {said.length > 0 && (
        <section className="brief-section">
          <h2 className="label-micro">What you've said lately — your own words</h2>
          <div className="brief-entities">
            {said.map(({ entity, quotes }) => (
              <div key={entity.id} className="brief-entity">
                <div className="brief-entity-head">
                  <button type="button" className="btn-quiet brief-entity-name" onClick={() => onOpenEntity(entity.id)}>
                    <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
                    <span className="name">{entity.name}</span>
                  </button>
                  <span className="meta brief-why">
                    {entity.matched_on === "attendee" ? `invited · ${entity.matched_text}` : `in the title · “${entity.matched_text}”`}
                  </span>
                </div>
                <div className="stack stack-tight">
                  {quotes.map((quote) => (
                    <button
                      key={`${quote.capture_event_id}-${entity.id}`}
                      type="button"
                      className="card brief-said"
                      onClick={() => onOpenCapture(quote.capture_event_id)}
                    >
                      <span className="meta">{ago(quote.occurred_at)}</span>
                      <span className="brief-said-words">
                        <Marked
                          text={quote.transcript_text}
                          passages={intentions.map((i) => i.quote).filter((q): q is string => !!q)}
                        />
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {brief && intentions.length === 0 && said.length === 0 && (
        <p className="brief-note">Nothing you've said touches this meeting yet.</p>
      )}
    </div>
  );
}

/// A note with the passage an intention was heard in marked — so the
/// sentence at the top of the page can be found in the note it came from
/// at a glance. Only passages found verbatim are marked; the backend kept
/// a quote only if it was (`structuring::verbatim`), so this is a lookup,
/// not a guess.
function Marked({ text, passages }: { text: string; passages: string[] }) {
  const lower = text.toLowerCase();
  const hit = passages
    .map((p) => ({ at: lower.indexOf(p.toLowerCase()), length: p.length }))
    .find((h) => h.at >= 0);
  const open = /[äöüß]|\b(ich|und|der|die|das)\b/i.test(text) ? "„" : "“";
  const close = open === "„" ? "“" : "”";
  if (!hit) return <>{quoted(text)}</>;
  return (
    <>
      {open}
      {text.slice(0, hit.at)}
      <mark className="said-mark">{text.slice(hit.at, hit.at + hit.length)}</mark>
      {text.slice(hit.at + hit.length)}
      {close}
    </>
  );
}

/// The attendees, with the ones Hippocampus knows lit and clickable.
function People({
  people,
  known,
  onOpenEntity,
}: {
  people: string[];
  known: BriefEntity[];
  onOpenEntity: (id: string) => void;
}) {
  return (
    <ul className="brief-people">
      {people.map((person) => {
        const entity = known.find((e) => e.matched_on === "attendee" && e.matched_text === person);
        return (
          <li key={person}>
            {entity ? (
              <button type="button" className="pill brief-person brief-person-known" onClick={() => onOpenEntity(entity.id)}>
                <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
                {person}
              </button>
            ) : (
              <span className="pill brief-person">{person}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/// Recomputed from the clock rather than taken from the last push, so a
/// page left open across the start of the meeting says so.
function phaseAt(meeting: MeetingView, now: number): "ahead" | "now" | "after" {
  if (now < new Date(meeting.starts_at).getTime()) return "ahead";
  if (now < new Date(meeting.ends_at).getTime()) return "now";
  return "after";
}

function whenLine(meeting: MeetingView, phase: string | null, now: number): string {
  const span = `${clock(meeting.starts_at)}–${clock(meeting.ends_at)}`;
  if (phase === "ahead") return `${until(meeting.starts_at, now)} · ${span}`;
  if (phase === "now") return `now · until ${clock(meeting.ends_at)}`;
  return `earlier today · ${span}`;
}

/// Quotes grouped under the entity they are about, in the order the
/// backend ranked the entities.
function groupSaid(brief: Brief | null) {
  if (!brief) return [];
  return brief.entities
    .map((entity) => ({
      entity,
      quotes: brief.said.filter((q) => q.entity_id === entity.id),
    }))
    .filter((group) => group.quotes.length > 0);
}
