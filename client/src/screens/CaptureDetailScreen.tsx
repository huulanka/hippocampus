import { useCallback, useEffect, useState } from "react";
import { correctTranscript, getCapture, redactCapture, type CaptureDetail } from "../api";
import { AudioPlayer } from "../components/AudioPlayer";
import { BackButton } from "../components/BackButton";
import { useEcho } from "../useEcho";
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
  /// Two-step, never a browser confirm(): a modal dialog inside a Tauri
  /// webview blocks every subsequent event, and this is the one action
  /// in the app that cannot be undone.
  const [confirmRedact, setConfirmRedact] = useState(false);
  const [redacting, setRedacting] = useState(false);

  /// A capture recorded before its echo was judged — or one whose
  /// judgement is still running — gets it filled in here rather than
  /// making the whole page wait for a model.
  const live = useEcho(
    detail?.echo_pending ? detail.event_id : null,
    detail?.echo_pending ?? false,
  );
  const echoItems = detail?.echo_pending ? live.items : (detail?.echo ?? []);
  const echoPending = detail?.echo_pending ? live.pending : false;

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

  /// Takes the words back. Reloads rather than navigating away: the
  /// capture still exists as a tombstone, and seeing it sit there empty
  /// is a truer picture of what just happened than the timeline silently
  /// being one shorter.
  async function redact() {
    if (redacting) return;
    setRedacting(true);
    setSaveError(null);
    try {
      await redactCapture(eventId);
      await reload();
      setConfirmRedact(false);
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setRedacting(false);
    }
  }

  if (error) return <DetailFrame onBack={onBack}>Couldn't load that capture: {error}</DetailFrame>;
  if (!detail) return <DetailFrame onBack={onBack}>Loading…</DetailFrame>;

  const spoken = detail.origin === "audio";
  const corrections = detail.transcripts.filter((t) => t.model === "user");

  return (
    <div className="column said">
      <BackButton onBack={onBack} />

      <header className="said-head">
        <p className="meta said-when">
          {fullStamp(detail.occurred_at)} · {spoken ? "spoken" : "typed"} on {detail.device}
        </p>

        {draft === null ? (
          <p className={detail.redacted ? "said-words said-words-gone" : "said-words"}>
            {detail.text ?? "The words were taken back."}
          </p>
        ) : (
          <>
            <label className="sr-only" htmlFor="said-edit">
              The corrected wording
            </label>
            <textarea
              id="said-edit"
              className="input said-edit"
              autoFocus
              rows={Math.max(3, Math.ceil(draft.length / 60))}
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
          </>
        )}

        <p className="said-note">
          {detail.redacted
            ? "The content was removed; the fact that you said something stays."
            : draft !== null
              ? spoken
                ? "Fixing the transcript. The recording stays untouched, and the old wording is kept."
                : "Fixing the text. The old wording is never deleted — search and the entities are rebuilt from the new one."
              : spoken
                ? "A transcript. The recording below is the original."
                : "Exactly as you typed it."}
          {draft === null && corrections.length > 0 && (
            <> Corrected {corrections.length === 1 ? "once" : `${corrections.length} times`}; every earlier wording is kept below.</>
          )}
        </p>

        {detail.audio && <AudioPlayer eventId={detail.event_id} mime={detail.audio.mime} />}

        {!detail.redacted &&
          (draft === null ? (
            <div className="said-actions">
              <button type="button" className="btn btn-secondary" onClick={() => setDraft(detail.text ?? "")}>
                Fix a word
              </button>
              {confirmRedact ? (
                <>
                  <button type="button" className="btn btn-secondary btn-danger" onClick={redact} disabled={redacting}>
                    {redacting ? "Taking it back…" : "Yes, take it back"}
                  </button>
                  <button type="button" className="btn btn-quiet" onClick={() => setConfirmRedact(false)}>
                    Keep it
                  </button>
                </>
              ) : (
                <button type="button" className="btn btn-quiet" onClick={() => setConfirmRedact(true)}>
                  Take this back…
                </button>
              )}
            </div>
          ) : (
            <div className="said-actions">
              <button
                type="button"
                className="btn btn-primary"
                onClick={saveCorrection}
                disabled={!(draft.trim() && !saving)}
              >
                {saving ? "Fixing…" : "Keep the fix"}
              </button>
              <button type="button" className="btn btn-secondary" onClick={() => setDraft(null)}>
                Cancel
              </button>
              <span className="kbd said-kbd">⌘↵</span>
            </div>
          ))}

        {confirmRedact && !detail.redacted && (
          <p className="said-warning">
            This removes the words, the recording, and everything derived from them — and the
            capture leaves search and other captures' echoes. The event that it happened stays.
            It cannot be undone, and backups made before now still hold the original.
          </p>
        )}

        {saveError && <p className="said-warning">Couldn't save that: {saveError}</p>}
      </header>

      <section className="said-section">
        <h2 className="label-micro">What it means — drawn out by a model, not written by you</h2>
        {detail.entities.length === 0 ? (
          <p className="said-note">
            Nothing was extracted from this one. Either the structuring hasn't run yet, or there
            was nothing in it worth remembering as a thing.
          </p>
        ) : (
          <ul className="said-entities">
            {detail.entities.map((entity) => (
              <li key={`${entity.id}-${entity.observation}`}>
                <button type="button" className="btn-quiet said-entity" onClick={() => onOpenEntity(entity.id)}>
                  <span className="said-entity-head">
                    <span className="chip-dot" style={{ background: entityColor(entity.entity_type) }} />
                    <span className="name said-entity-name">{entity.name}</span>
                    <span className="label-micro">{entity.entity_type}</span>
                  </span>
                  <span className="said-entity-reading">
                    {entity.observation}
                    {whenLabel(entity) && <span className="said-about"> · about {whenLabel(entity)}</span>}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}

        {detail.relations.length > 0 && (
          <ul className="said-relations">
            {detail.relations.map((relation) => (
              <li key={relation.id} className="said-relation">
                <button type="button" className="btn-quiet said-relation-end" onClick={() => onOpenEntity(relation.from_entity_id)}>
                  {relation.from_name}
                </button>
                <span className="said-relation-type">{relation.relation_type} →</span>
                <button type="button" className="btn-quiet said-relation-end" onClick={() => onOpenEntity(relation.to_entity_id)}>
                  {relation.to_name}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="said-section">
        <h2 className="label-micro">You've been here before — your own earlier words</h2>
        {echoPending ? (
          <p className="said-note">Looking through your earlier captures…</p>
        ) : echoItems.length === 0 ? (
          <p className="said-note">Nothing close enough among your earlier captures.</p>
        ) : (
          <div className="stack stack-tight">
            {echoItems.map((item) => (
              <button
                type="button"
                key={item.capture_event_id}
                className="card said-echo"
                onClick={() => onOpenCapture(item.capture_event_id)}
              >
                <span className="meta said-when">{shortStamp(item.occurred_at)}</span>
                <span className="said-echo-words">{item.transcript_text}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      {detail.transcripts.length > 1 && (
        <section className="said-section">
          <h2 className="label-micro">How the words changed — oldest first; nothing was overwritten</h2>
          <ol className="said-versions">
            {detail.transcripts.map((version) => (
              <li key={version.event_id} className="said-version">
                <span className="meta">
                  {shortStamp(version.created_at)} · {authorOf(version.model)}
                </span>
                <span className="said-echo-words">{version.text}</span>
              </li>
            ))}
          </ol>
        </section>
      )}

      <details className="said-raw">
        <summary className="label-micro">
          Under the hood — {detail.events.length} events, exactly as stored
        </summary>
        {detail.events.map((event) => (
          <div key={event.id} className="said-event">
            <span className="meta">
              <span className="derived">{event.event_type}</span> · {shortStamp(event.occurred_at)} · {event.source}
            </span>
            <pre className="said-payload">{JSON.stringify(event.payload, null, 2)}</pre>
          </div>
        ))}
      </details>
    </div>
  );
}

function DetailFrame({ children, onBack }: { children: React.ReactNode; onBack: () => void }) {
  return (
    <div className="column said">
      <BackButton onBack={onBack} />
      <p className="said-note">{children}</p>
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
