#!/bin/sh
# Brings the generated Xcode project (client/src-tauri/gen/apple, not in
# the repository) up to what the app needs, after `npx tauri ios init`:
#
# - the Swift that has to be in the app target itself, not in a plugin:
#   the Action Button's App Intent (client/src-tauri/ios/*.swift), because
#   iOS only offers intents it finds in the app's own binary;
# - the app icon (scripts/render-brand.py);
# - a regenerated project, so Xcode sees the added files.
#
# Safe to run again; it only copies and regenerates.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
apple="$root/client/src-tauri/gen/apple"

if [ ! -d "$apple" ]; then
  echo "no generated project at $apple — run 'npx tauri ios init' in client/ first" >&2
  exit 1
fi

cp "$root"/client/src-tauri/ios/*.swift "$apple/Sources/client/"
python3 "$root/scripts/render-brand.py" >/dev/null
(cd "$apple" && xcodegen generate --quiet)
echo "prepared $apple"
