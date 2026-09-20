# ADR 0007: The Tauri client is its own Cargo workspace

## Status
Accepted (2026-09-20)

## Context
ADR 0001 put the client and backend in one Cargo workspace, sharing a
`contracts` crate and one lock file.

Adding on-device speech recognition broke that. Both halves of the system
now run ONNX models, and their respective wrapper crates pin the ONNX
runtime to exact, different versions:

- backend: `fastembed` 7.0.1 → `ort =2.0.0-rc.13`
- client: `transcribe-rs` 0.3.x → `ort =2.0.0-rc.12`

Both pins are exact and hold across every published version of either crate
(`fastembed` last used rc.10, at 5.0.0). One lock file cannot satisfy them,
and Cargo refuses the resolution outright.

Three ways out were considered:

1. **Use `transcribe-rs`'s whisper-cpp backend instead**, which does not
   depend on `ort` at all. Rejected: it builds whisper.cpp from source, so
   cmake becomes a hard build dependency for every developer and for CI,
   every clean build gets much slower, and the GGUF models already on the
   machine are not verified to load through it.
2. **Replace `fastembed` with direct `ort` usage** in the backend, pinned
   to rc.12. Rejected as a large rewrite of working code to serve a build
   constraint.
3. **Split the workspaces.** Chosen.

## Decision
`client/src-tauri` declares its own `[workspace]` and carries its own lock
file and release profile. The repository root workspace keeps `backend` and
`contracts`.

`contracts` is still shared, by path dependency across the workspace
boundary — the API types stay single-sourced, which was the actual point of
ADR 0001's shared crate.

## Consequences
- CI builds, lints and tests the two workspaces separately.
- Two target directories, so a full clean build compiles shared
  dependencies twice. Accepted: the two halves never link together, and
  neither does the ONNX runtime they disagree about.
- The version skew is contained rather than fixed. When `fastembed` and
  `transcribe-rs` agree on an `ort` version again, merging the workspaces
  back is a two-line change.
- ADR 0001's "both live in one Cargo workspace" no longer holds; its
  reasoning about language choice and the shared contracts crate does.
