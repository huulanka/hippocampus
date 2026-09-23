import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { sendSession, type SentNote } from "../api";
import { clearDraft, loadDraft, saveDraft } from "../desktop";

/// A page you keep adding to for hours and send once.
///
/// The capture field answers "I just thought of something". It does not
/// answer "I am in a four-hour workshop", which is a different shape
/// entirely: you write, you stop, someone else talks, you write again,
/// and none of it should go anywhere until the end.
///
/// Two decisions carry the whole screen, and both are visible on it:
///
/// **It stays several notes, not one.** Four hours arriving as a single
/// wall of text would drown the echo — which compares whole captures —
/// and leave the extraction nothing to hold on to. Blocks are what gets
/// sent, one capture each, which is the same unit as everywhere else in
/// the app.
///
/// **Each note keeps the time it was written.** Not the time Send was
/// pressed. The timestamps run down the left edge while you type so that
/// this is a fact you can see rather than a claim in a changelog — if
/// they all said 14:00, the timeline would be lying about when you
/// thought something, and the timeline is most of what this app is for.
///
/// Nothing leaves the Mac until Send. Everything is on disk from the
/// first keystroke.

interface Block {
  id: string;
  text: string;
  /// Stamped on the first keystroke that puts something in this block,
  /// not when the empty block appeared. An empty block waiting at the
  /// bottom of the page for twenty minutes must not hand its note a time
  /// from twenty minutes ago.
  writtenAt: string | null;
}

interface Draft {
  version: 1;
  id: string;
  title: string;
  startedAt: string;
  blocks: Block[];
}

/// How long typing has to pause before the draft is written. Short enough
/// that "saved" is always nearly true, long enough that holding a key
/// down is not a hundred writes.
const SAVE_AFTER_MS = 400;

function id(): string {
  return crypto.randomUUID();
}

function fresh(): Draft {
  return {
    version: 1,
    id: id(),
    title: "",
    startedAt: new Date().toISOString(),
    blocks: [{ id: id(), text: "", writtenAt: null }],
  };
}

export function NotepadScreen({ onOpenCapture }: { onOpenCapture: (eventId: string) => void }) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saved, setSaved] = useState<Date | null>(null);
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState<SentNote[] | null>(null);
  /// Which block should take the caret after the next render, and where
  /// in it. Set by the two edits that move the caret between blocks.
  const focusNext = useRef<{ id: string; at: "start" | "end" } | null>(null);

  useEffect(() => {
    let live = true;
    loadDraft<Draft>()
      .then((stored) => {
        if (!live) return;
        setDraft(stored && stored.version === 1 && stored.blocks?.length ? stored : fresh());
      })
      .catch(() => live && setDraft(fresh()));
    return () => {
      live = false;
    };
  }, []);

  // Debounced, and it deliberately does not wait for the write to come
  // back before letting the next one start: the last write wins, and a
  // slow disk must never make typing feel slow.
  useEffect(() => {
    if (!draft) return;
    const timer = window.setTimeout(() => {
      void saveDraft(draft)
        .then(() => setSaved(new Date()))
        .catch(() => setSaved(null));
    }, SAVE_AFTER_MS);
    return () => window.clearTimeout(timer);
  }, [draft]);

  const edit = useCallback((blockId: string, text: string) => {
    setDraft((d) => {
      if (!d) return d;
      return {
        ...d,
        blocks: d.blocks.map((block) =>
          block.id === blockId
            ? {
                ...block,
                text,
                writtenAt: block.writtenAt ?? (text.trim() ? new Date().toISOString() : null),
              }
            : block,
        ),
      };
    });
  }, []);

  /// A blank line ends a note and starts the next one. Everything after
  /// the caret comes along, so splitting in the middle of a paragraph does
  /// what it looks like it does.
  const split = useCallback((blockId: string, before: string, after: string) => {
    const next: Block = { id: id(), text: after, writtenAt: after.trim() ? new Date().toISOString() : null };
    focusNext.current = { id: next.id, at: "start" };
    setDraft((d) => {
      if (!d) return d;
      const at = d.blocks.findIndex((block) => block.id === blockId);
      if (at < 0) return d;
      const blocks = [...d.blocks];
      blocks[at] = { ...blocks[at], text: before };
      blocks.splice(at + 1, 0, next);
      return { ...d, blocks };
    });
  }, []);

  /// Backspace at the very start of a note folds it back into the one
  /// above, the way every editor does.
  const merge = useCallback((blockId: string) => {
    setDraft((d) => {
      if (!d) return d;
      const at = d.blocks.findIndex((block) => block.id === blockId);
      if (at <= 0) return d;
      const previous = d.blocks[at - 1];
      focusNext.current = { id: previous.id, at: "end" };
      const blocks = [...d.blocks];
      blocks[at - 1] = { ...previous, text: previous.text + d.blocks[at].text };
      blocks.splice(at, 1);
      return { ...d, blocks };
    });
  }, []);

  const notes = (draft?.blocks ?? [])
    .filter((block) => block.text.trim() && block.writtenAt)
    .map((block) => ({ text: block.text.trim(), writtenAt: block.writtenAt as string }));

  const send = useCallback(async () => {
    if (!draft || notes.length === 0 || sending) return;
    setSending(true);
    try {
      const result = await sendSession(
        notes,
        {
          id: draft.id,
          title: draft.title.trim() || untitled(draft.startedAt),
          started_at: draft.startedAt,
        },
        "mac",
      );
      setSent(result);
      // Only the notes that actually landed leave the draft. Anything the
      // backend refused stays exactly where it was, still on disk, still
      // editable — after four hours of typing the one unacceptable
      // outcome is losing the part that failed.
      const kept = new Set(result.filter((note) => note.eventId).map((note) => note.text));
      const left = draft.blocks.filter((block) => !kept.has(block.text.trim()));
      if (left.length === 0) {
        await clearDraft();
        setDraft(fresh());
      } else {
        setDraft({ ...draft, blocks: left });
      }
    } finally {
      setSending(false);
    }
  }, [draft, notes, sending]);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        void send();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [send]);

  if (!draft) return <p className="today-note">Opening your page…</p>;

  const words = notes.reduce((total, note) => total + note.text.split(/\s+/).filter(Boolean).length, 0);
  const failed = sent?.filter((note) => note.error) ?? [];
  const landed = sent?.filter((note) => note.eventId) ?? [];

  return (
    <div className="notepad">
      <header className="notepad-head">
        <div className="notepad-title">
          <label className="sr-only" htmlFor="session-title">
            What this session is
          </label>
          <input
            id="session-title"
            className="notepad-title-input"
            value={draft.title}
            placeholder={untitled(draft.startedAt)}
            onChange={(event) => setDraft({ ...draft, title: event.currentTarget.value })}
          />
          <span className="meta">
            started {clock(draft.startedAt)} · {elapsed(draft.startedAt)}
          </span>
        </div>
        <span className="notepad-saved meta">
          {saved ? (
            <>
              <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="var(--moss)" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="m4.5 12.5 5 5 10-11" />
              </svg>
              Saved on this Mac
            </>
          ) : (
            "Not saved yet"
          )}
        </span>
      </header>

      <div className="notepad-body">
        <div className="notepad-page">
          {draft.blocks.map((block, index) => (
            <Note
              key={block.id}
              block={block}
              first={index === 0}
              focus={focusNext}
              onEdit={edit}
              onSplit={split}
              onMerge={merge}
            />
          ))}
          <p className="notepad-hint">
            A blank line starts the next note. Nothing is sent yet.
          </p>
        </div>

        <aside className="notepad-aside">
          <section className="card notepad-card">
            <h2 className="label-micro">When you send</h2>
            <p className="notepad-explain">
              {notes.length > 1
                ? `These go as ${notes.length} separate notes, not as one long one.`
                : "Each block becomes its own note, not one long one."} A four-hour wall of text would drown the echo and leave the
              extraction nothing to hold on to.
            </p>
            <p className="notepad-explain">
              Each one keeps <strong>the time you wrote it</strong>, not the time you pressed send
              — otherwise four hours collapse into one timestamp and the timeline starts lying.
            </p>
          </section>

          <section className="card notepad-card">
            <h2 className="label-micro">While you write</h2>
            <p className="notepad-explain">
              Every keystroke goes to disk. Quitting, losing power, or a meeting running an hour
              over changes nothing — the page is still here when you come back.
            </p>
          </section>

          {sent && (
            <section className="card notepad-card notepad-result">
              <h2 className="label-micro">{landed.length} kept</h2>
              {landed.length > 0 && (
                <div className="notepad-landed">
                  {landed.map((note) => (
                    <button
                      key={note.eventId}
                      type="button"
                      className="btn-quiet notepad-landed-link"
                      onClick={() => note.eventId && onOpenCapture(note.eventId)}
                    >
                      {clock(note.writtenAt)} — {excerpt(note.text)}
                    </button>
                  ))}
                </div>
              )}
              {failed.length > 0 && (
                <p className="notepad-failed">
                  {failed.length} could not be sent and {failed.length === 1 ? "is" : "are"} still
                  on the page above. Nothing was lost; try again when the backend is reachable.
                </p>
              )}
            </section>
          )}
        </aside>
      </div>

      <footer className="notepad-foot">
        <span className="notepad-count">
          <span className="meta">
            {notes.length} {notes.length === 1 ? "note" : "notes"} · {words}{" "}
            {words === 1 ? "word" : "words"}
          </span>
          <span className="notepad-nowhere">Nothing has left this Mac yet.</span>
        </span>
        <span className="notepad-actions">
          <kbd className="kbd">⌘ ↵</kbd>
          <button
            type="button"
            className="btn btn-secondary btn-danger"
            disabled={notes.length === 0 || sending}
            onClick={() => {
              void clearDraft();
              setDraft(fresh());
              setSent(null);
            }}
          >
            Discard
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={notes.length === 0 || sending}
            onClick={() => void send()}
          >
            {sending ? "Sending…" : notes.length === 0 ? "Send" : `Send ${notes.length} ${notes.length === 1 ? "note" : "notes"}`}
          </button>
        </span>
      </footer>
    </div>
  );
}

function Note({
  block,
  first,
  focus,
  onEdit,
  onSplit,
  onMerge,
}: {
  block: Block;
  first: boolean;
  focus: React.RefObject<{ id: string; at: "start" | "end" } | null>;
  onEdit: (id: string, text: string) => void;
  onSplit: (id: string, before: string, after: string) => void;
  onMerge: (id: string) => void;
}) {
  const field = useRef<HTMLTextAreaElement>(null);

  // Grow with the text. A textarea with a scrollbar in the middle of a
  // page of notes reads as a form field; this has to read as a page.
  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  }, [block.text]);

  useLayoutEffect(() => {
    const wanted = focus.current;
    if (!wanted || wanted.id !== block.id) return;
    focus.current = null;
    const element = field.current;
    if (!element) return;
    element.focus();
    const at = wanted.at === "end" ? element.value.length : 0;
    element.setSelectionRange(at, at);
  });

  return (
    <div className="notepad-note">
      <span className="notepad-stamp meta">{block.writtenAt ? clock(block.writtenAt) : "—"}</span>
      <textarea
        ref={field}
        className="notepad-field"
        rows={1}
        value={block.text}
        placeholder={first ? "Start writing. Nothing is sent until you say so." : ""}
        onChange={(event) => {
          const element = event.currentTarget;
          const value = element.value;
          const caret = element.selectionStart;
          // A blank line is the break. Looking at the caret rather than
          // the whole value means pasting a document with blank lines in
          // it does not explode into twenty notes.
          if (value.slice(0, caret).endsWith("\n\n")) {
            onSplit(block.id, value.slice(0, caret - 2), value.slice(caret));
            return;
          }
          onEdit(block.id, value);
        }}
        onKeyDown={(event) => {
          const element = event.currentTarget;
          if (
            event.key === "Backspace" &&
            !first &&
            element.selectionStart === 0 &&
            element.selectionEnd === 0
          ) {
            event.preventDefault();
            onMerge(block.id);
          }
        }}
      />
    </div>
  );
}

/// What a session is called before it is called anything. The date, not
/// "Untitled" — a page of notes from a day you can name is findable, and
/// a hundred sessions called "Untitled" are not.
function untitled(startedAt: string): string {
  return new Date(startedAt).toLocaleDateString(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
  });
}

function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

function elapsed(startedAt: string): string {
  const minutes = Math.max(0, Math.round((Date.now() - new Date(startedAt).getTime()) / 60_000));
  if (minutes < 1) return "just started";
  if (minutes < 60) return `${minutes} min`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

function excerpt(text: string): string {
  const oneLine = text.replace(/\s+/g, " ").trim();
  return oneLine.length > 44 ? `${oneLine.slice(0, 43)}…` : oneLine;
}
