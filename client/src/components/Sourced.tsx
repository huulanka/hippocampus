import type { SourcedSentence } from "../api";

/// Sentences a model wrote, each followed by the notes it rests on.
///
/// The one form every written part of the app takes — the week's
/// paragraph, an entity's gist, the answer to a question — so that
/// anything a model said can be followed back to what you said, one click
/// per sentence. Numbered across the whole passage in order of first
/// citation, so the same note is the same number wherever it is cited;
/// pass `numbers` to share the numbering with a list of the notes below.
export function Sourced({
  sentences,
  onOpenCapture,
  numbers = numberSources(sentences),
  className = "prose",
}: {
  sentences: SourcedSentence[];
  onOpenCapture: (id: string) => void;
  numbers?: Map<string, number>;
  className?: string;
}) {
  return (
    <p className={className}>
      {sentences.map((sentence, index) => (
        <span key={index}>
          {sentence.text}
          {sentence.sources.map((source) => (
            <button
              key={source}
              type="button"
              className="source-mark"
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

export function numberSources(sentences: SourcedSentence[]): Map<string, number> {
  const numbers = new Map<string, number>();
  for (const sentence of sentences) {
    for (const source of sentence.sources) {
      if (!numbers.has(source)) numbers.set(source, numbers.size + 1);
    }
  }
  return numbers;
}
