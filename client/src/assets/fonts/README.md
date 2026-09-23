# Bundled typefaces

Shipped with the app rather than fetched at runtime. Hippocampus works
offline by design — the microphone, the transcription and the draft store
all do — so the interface must not need the network to render its own text.
Three families, one job each; see `docs/design.md`.

| File | Family | Role |
|---|---|---|
| `fraunces-*.woff2` | Fraunces (variable, 400–600) | Your words, and the names of things |
| `instrument-sans-*.woff2` | Instrument Sans (variable, 400–600) | Everything the interface says |
| `jetbrains-mono-*.woff2` | JetBrains Mono (variable, 400–500) | Machine truth: timestamps, counts, ids |

Each family ships as two subsets, `latin` and `latin-ext`, split on the
same unicode ranges Google Fonts uses. German needs only `latin`;
`latin-ext` is 82 KB for the rest of Europe and loads only when a
character actually calls for it.

All three are licensed under the SIL Open Font License 1.1 — the full text
is in [`OFL.txt`](OFL.txt).

- Fraunces — Copyright 2018 The Fraunces Project Authors, <https://github.com/undercasetype/Fraunces>
- Instrument Sans — Copyright 2023 The Instrument Sans Project Authors, <https://github.com/Instrument/instrument-sans>
- JetBrains Mono — Copyright 2020 The JetBrains Mono Project Authors, <https://github.com/JetBrains/JetBrainsMono>
