import { useCallback, useEffect, useState } from "react";
import { audioUrl, correctTranscript, getCapture, type CaptureDetail } from "../api";
import { AudioPlayer } from "../components/AudioPlayer";
import { entityColor } from "../entityType";
import { whenLabel } from "../whenLabel";

/// Everything known about one capture, on one page.
///
/// The ordering is deliberate: the user's own words first, then what the
/// machine made of them, then what it connects to, and only at the end —
/// folded away — the log that proves it. Anything derived is labelled with
/// the model that derived it, so a wrong entity is recognisable as the
/// model's mistake rather than the user's memory.
export function CaptureDetailScreen({
  eventId,
  onOpenCapture,
  onOpenEntity,
  onBack,
}: {
  eventId: string;
  onOpenCapture: (eventId: string) => void;
  onOpenEntity: (id: string) => void;
  onBack: () => void;
}) {
  const [detail, setDetail] = useState<CaptureDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const reload = useCallback(
    () => getCapture(eventId).then(setDetail),
    [eventId],
  );

  useEffect(() => {
    let current = true;
    setDetail(null);
    setError(null);
    setDraft(null);
    setSaveError(null);
    getCapture(eventId)
      .then((d) => current && setDetail(d))
      .catch((err) => current && setError(String(err)));
    return () => {
      current = false;
    };
  }, [eventId]);

  async function saveCorrection() {
    if (draft === null || !draft.trim() || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      await correctTranscript(eventId, draft);
      // Refetch rather than patch: the correction also re-embeds the
      // capture and restarts the structuring, so the echoes and entities
      // on screen are about the old wording.
      await reload();
      setDraft(null);
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  }

  if (error) return <DetailFrame onBack={onBack}>Couldn't load that capture: {error}</DetailFrame>;
  if (!detail) return <DetailFrame onBack={onBack}>Loading…</DetailFrame>;

  const spoken = detail.origin === "audio";
  const corrections = detail.transcripts.filter((t) => t.model === "user");

  return (
    <div className="detail">
      <div className="detail-head">
        <span className="dim link" onClick={onBack}>
          [ ← back ]
        </span>
        <span className="dim detail-stamp">
          // {fullStamp(detail.occurred_at)} · {spoken ? "spoken" : "typed"} on {detail.device}
        </span>
      </div>

      <section className="panel detail-panel">
        <div className="kicker">
          [ CAPTURED ]{" "}
          <span className="dim">
            {detail.redacted
              ? "— the content was removed; the capture itself stays"
              : draft !== null
                ? spoken
                  ? "— fixing the transcript; the recording stays untouched"
                  : "— fixing the text; the original is kept"
                : spoken
                  ? "— transcript; the recording below is the original"
                  : "— raw, verbatim"}
          </span>
        </div>

        {draft === null ? (
          <p className="detail-transcript">{detail.text ?? "(redacted)"}</p>
        ) : (
          <textarea
            className="capture-input detail-edit"
            autoFocus
            rows={Math.max(3, Math.ceil(draft.length / 70))}
            value={draft}
            disabled={saving}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void saveCorrection();
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setDraft(null);
              }
            }}
          />
        )}

        {detail.audio && <AudioPlayer src={audioUrl(detail.event_id)} />}

        {!detail.redacted &&
          (draft === null ? (
            <div className="detail-actions">
              <span className="dim link" onClick={() => setDraft(detail.text ?? "")}>
                [ fix a word ]
              </span>
            </div>
          ) : (
            <div className="detail-actions">
              <span
                className={`btn btn-accent${draft.trim() && !saving ? "" : " disabled"}`}
                onClick={saveCorrection}
              >
                [ {saving ? "Fixing…" : "Keep The Fix"} ]
              </span>
              <span className="btn" onClick={() => setDraft(null)}>
                [ Cancel ]
              </span>
            </div>
          ))}

        {saveError && <p className="dim detail-note">Couldn't save that: {saveError}</p>}

        {draft !== null && (
          <p className="dim detail-note">
            The old wording is never deleted — every version is kept below. Search and the
            entities are rebuilt from the new one.
          </p>
        )}

        {draft === null && corrections.length > 0 && (
          <p className="dim detail-note">
            Corrected {corrections.length === 1 ? "once" : `${corrections.length} times`} — every
            earlier wording is kept below.
          </p>
        )}
      </section>

      <section className="panel detail-panel">
        <div className="kicker">
          [ WHAT IT MEANS ]{" "}
          <span className="dim">— drawn out by a model, not written by you</span>
        </div>
        {detail.entities.length === 0 ? (
          <p className="dim detail-note">
            Nothing was extracted from this one. Either the structuring hasn't run yet, or
            there was nothing in it worth remembering as a thing.
          </p>
        ) : (
          <div className="detail-entities">
            {detail.entities.map((entity) => (
              <div
                key={`${entity.id}-${entity.observation}`}
                className="detail-entity detail-entity-link"
                onClick={() => onOpenEntity(entity.id)}
              >
                <div className="entity-card-head">
                  <span
                    className="entity-dot"
                    style={{ background: entityColor(entity.entity_type) }}
                  />
                  <span className="entity-name detail-entity-name">{entity.name}</span>
                  <span className="dim entity-type-label">{entity.entity_type.toUpperCase()}</span>
                </div>
                <p className="detail-observation">{entity.observation}</p>
                <WhenBadge of={entity} />
              </div>
            ))}
          </div>
        )}
        {detail.relations.length > 0 && (
          <div className="detail-relations">
            {detail.relations.map((relation) => (
              <div key={relation.id} className="detail-relation">
                <span
                  className="detail-relation-node link"
                  onClick={() => onOpenEntity(relation.from_entity_id)}
                >
                  {relation.from_name}
                </span>
                <span className="dim detail-relation-type">──{relation.relation_type}──▶</span>
                <span
                  className="detail-relation-node link"
                  onClick={() => onOpenEntity(relation.to_entity_id)}
                >
                  {relation.to_name}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="panel detail-panel">
        <div className="kicker">
          [ YOU'VE BEEN HERE BEFORE ]{" "}
          <span className="dim">— your own earlier words, not a summary</span>
        </div>
        {detail.echo.length === 0 ? (
          <p className="dim detail-note">
            Nothing close enough among your earlier captures.
          </p>
        ) : (
          <div className="card-stack detail-echo">
            {detail.echo.map((item) => (
              <div
                key={item.capture_event_id}
                className="timeline-card clickable"
                onClick={() => onOpenCapture(item.capture_event_id)}
              >
                <div className="timeline-card-meta">
                  <span className="dim">// {shortStamp(item.occurred_at)}</span>
                  <span className="dim card-open-hint">[open]</span>
                </div>
                <p className="timeline-transcript">{item.transcript_text}</p>
              </div>
            ))}
          </div>
        )}
      </section>

      {detail.transcripts.length > 1 && (
        <section className="panel detail-panel">
          <div className="kicker">
            [ HOW THE WORDS CHANGED ]{" "}
            <span className="dim">— oldest first; nothing was overwritten</span>
          </div>
          {detail.transcripts.map((version) => (
            <div key={version.event_id} className="detail-version">
              <div className="timeline-card-meta">
                <span className="dim">// {shortStamp(version.created_at)}</span>
                <span className="dim">{authorOf(version.model)}</span>
              </div>
              <p className="timeline-transcript">{version.text}</p>
            </div>
          ))}
        </section>
      )}

      <details className="panel detail-panel detail-raw">
        <summary className="kicker">
          [ UNDER THE HOOD ]{" "}
          <span className="dim">— {detail.events.length} events, exactly as stored</span>
        </summary>
        {detail.events.map((event) => (
          <div key={event.id} className="detail-event">
            <div className="timeline-card-meta">
              <span className="detail-event-type">{event.event_type}</span>
              <span className="dim">
                {shortStamp(event.occurred_at)} · {event.source}
              </span>
            </div>
            <pre className="detail-payload">{JSON.stringify(event.payload, null, 2)}</pre>
          </div>
        ))}
      </details>
    </div>
  );
}

/// The date an observation is *about*, shown only when there is one.
/// Most observations are not about a point in time, and a badge on every
/// card would train the eye to ignore it.
function WhenBadge({ of }: { of: Parameters<typeof whenLabel>[0] }) {
  const label = whenLabel(of);
  if (!label) return null;
  return <p className="when-badge">◷ {label}</p>;
}

function DetailFrame({ children, onBack }: { children: React.ReactNode; onBack: () => void }) {
  return (
    <div className="detail">
      <div className="detail-head">
        <span className="dim link" onClick={onBack}>
          [ ← back ]
        </span>
      </div>
      <p className="dim">{children}</p>
    </div>
  );
}

/// Who wrote a given version. The backend uses two sentinel values where
/// an ASR model name would otherwise stand.
function authorOf(model: string): string {
  if (model === "user") return "you, corrected";
  if (model === "typed") return "you, typed";
  return model;
}

function fullStamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function shortStamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
