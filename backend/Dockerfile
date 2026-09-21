# Build context is the repo root, not this directory — `backend` is a
# workspace member and cannot build without its siblings (root Cargo.toml,
# Cargo.lock, and the `contracts` path dependency). See
# docs/adr/0007-separate-client-workspace.md for why the *client* is a
# separate workspace; backend/contracts are not, and share this image's
# build.
# Trixie, not Bookworm: the prebuilt ONNX Runtime binary `ort` downloads
# needs glibc/libstdc++ symbols (e.g. __isoc23_strtoll, ISO C23 additions)
# newer than Bookworm's glibc 2.36 ships — linking fails with "undefined
# symbol" otherwise. Both stages must match, or the runtime image can't
# load what the builder produced.
FROM rust:1-slim-trixie AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY contracts contracts
COPY backend backend

# sqlx's query! macros check against the committed query cache at compile
# time (backend/.sqlx) — there is no DATABASE_URL here, only what
# `cargo sqlx prepare` already captured. See the README's Echo section for
# when that cache needs regenerating.
ENV SQLX_OFFLINE=true

RUN cargo build --release -p backend

FROM debian:trixie-slim

# ca-certificates: outbound TLS to OpenRouter and to Hugging Face for the
# embedding/reranker model downloads. libgomp1: the ONNX Runtime CPU
# execution provider (via `ort`/`fastembed`) links against it.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libgomp1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /workspace/target/release/backend /usr/local/bin/backend

# Migrations are embedded into the binary at compile time by
# `sqlx::migrate!()` in main.rs — nothing else to copy for them to run.

EXPOSE 8080
CMD ["backend"]
