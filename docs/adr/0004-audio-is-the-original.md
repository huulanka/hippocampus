# ADR 0004: The audio is the original, the transcript an interpretation

## Status
Accepted (2026-09-20)

## Context
Until now the transcript counted as the immutable original: `POST /captures`
took `transcript_text` and stored it as `capture.recorded`, and the audio
explicitly stayed on the device ("Audio itself is never uploaded to the
backend", `contracts/src/lib.rs`).

That contradicts the core principle of the project. A transcript is the
output of a speech model; it is already the first AI-derived artefact in
the chain, with every mistake such models make. Speech recognition gets
proper names wrong most often, and proper names are exactly what matters
in a personal knowledge system ("Hafen Portal" instead of "HPortal"). If
only the transcript is kept, that mistake is permanent: nothing is left to
check it against.

Storage is no argument against it. At an assumed 15 captures a day of 30
seconds each, Opus compression (about 24 kbit/s) comes to roughly 1.5 GB a
year, which is irrelevant on the NAS.

## Decision
- Audio is kept permanently on the NAS, in a standard format (Opus in Ogg),
  under a content-addressed path.
- `capture.recorded` references the audio and contains **no** text.
- The transcript is its own event, `transcript.derived`, with the model
  named, which makes it explicitly an interpretation.
- Captures without audio (typed, or an iPhone Shortcut with Apple
  dictation) carry `origin: "text"`; there the text is the original and
  immutable.

## Consequences
- `CreateCaptureRequest` changes: audio is uploaded, and the comment in
  `contracts/src/lib.rs` no longer applies.
- Better speech models can later be run over the whole archive;
  re-transcription becomes the same operation as re-structuring. That is
  the second and stronger reason event sourcing pays off here.
- Bigger uploads, and a blob storage path that did not exist before.
- Audio is the most sensitive form of data in the system: a voice,
  background noise, other people overheard. It never leaves your own
  hardware; only text goes to OpenRouter.
