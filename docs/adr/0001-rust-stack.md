# ADR 0001: Full Rust stack (Tauri client + Axum backend)

## Status
Accepted (2026-09-20)

## Context
The backend needs to run on a resource-constrained Synology DS220+ (Celeron,
no GPU, 16GB RAM, already running other services) alongside a desktop client
that must do on-device speech-to-text. An initial Node/Fastify backend +
native SwiftUI client design was considered.

[Handy](https://github.com/cjpais/handy), an open-source local-transcription
app the project owner already uses daily, proved that Rust + Tauri v2 is a
production-viable way to build exactly this kind of app: it uses
`transcribe-rs` (ONNX: Parakeet, Moonshine, SenseVoice, Canary) and
`transcribe-cpp` (Whisper/GGML) behind a swappable model abstraction, plus
mature Tauri plugins for tray icon, global shortcuts, and autostart.

## Decision
- Client: Tauri v2 (Rust) with a Vite + React/TypeScript frontend in the
  webview, reusing the `transcribe-rs`/`transcribe-cpp` pattern for
  swappable local ASR.
- Backend: Rust + Axum.
- Both live in one Cargo workspace, sharing a `contracts` crate for HTTP
  API DTOs.

## Consequences
- Very low RAM/CPU footprint for the backend container, which matters given
  the weak NAS.
- One language across client and backend; steeper learning curve than
  Node/TypeScript, accepted as a deliberate, enjoyed trade-off for this
  hobby project.
- Smaller ecosystem for some conveniences (e.g. local embeddings use
  `fastembed`/`ort` instead of a one-line JS library).
