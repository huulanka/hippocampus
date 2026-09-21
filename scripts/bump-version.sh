#!/usr/bin/env bash
# Sets the version in every place it is duplicated, in one step.
#
# There are six of them (backend, contracts and client are three separate
# Cargo/npm packages, plus Tauri's own tauri.conf.json), with no shared
# source between them, so they are only ever consistent because something
# set them all together. This is that something.
#
# Called two ways: by semantic-release's `exec` plugin during the release
# job (see .releaserc.json — prepareCmd, before the `git` plugin commits
# and tags), and directly by hand when you just want to see what a version
# would touch. `sed -i ''` is macOS syntax on purpose: local dev and the
# release job both run on macos-latest, same as the rest of this repo's CI.
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 <version>   (e.g. 0.2.0, no leading v)" >&2
  exit 1
fi

VERSION="$1"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: '$VERSION' is not a plain semver (X.Y.Z)" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

set_cargo_version() {
  local file="$1"
  sed -i '' -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$VERSION\"/" "$file"
  echo "  $file"
}

set_json_version() {
  local file="$1"
  sed -i '' -E "s/\"version\": \"[0-9]+\.[0-9]+\.[0-9]+\"/\"version\": \"$VERSION\"/" "$file"
  echo "  $file"
}

# `contracts` is a path dependency, pinned separately in the two crates
# that use it. A path dependency needs no version requirement at all to
# build, but since one is there, it has to move with the package's own
# version or a future minor bump (0.1.x -> 0.2.0) breaks the build on a
# stale `^0.1.0` requirement — not "unused", just easy to forget.
set_contracts_dependency_version() {
  local file="$1"
  sed -i '' -E "s/contracts = \{ version = \"[0-9]+\.[0-9]+\.[0-9]+\"/contracts = { version = \"$VERSION\"/" "$file"
  echo "  $file (contracts dependency)"
}

echo "Setting version to $VERSION in:"
set_cargo_version "Cargo.toml"
set_cargo_version "backend/Cargo.toml"
set_cargo_version "contracts/Cargo.toml"
set_cargo_version "client/src-tauri/Cargo.toml"
set_json_version "client/package.json"
set_json_version "client/src-tauri/tauri.conf.json"
set_contracts_dependency_version "backend/Cargo.toml"
set_contracts_dependency_version "client/src-tauri/Cargo.toml"

echo
echo "Refreshing lockfiles..."
cargo check --workspace --quiet
(cd client/src-tauri && cargo check --quiet)
(cd client && npm install --package-lock-only --silent)

echo
echo "Done. semantic-release commits, tags and pushes this automatically as"
echo "part of the release job — nothing further to do there. Running this by"
echo "hand instead: review with 'git diff', it does not commit anything itself."
