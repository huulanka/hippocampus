#!/usr/bin/env bash
# Rewrites the Homebrew cask for a released version.
#
# Called by the release-app workflow once the DMG exists and has been
# attached to the GitHub Release: the cask has to name both the version
# and the artifact's checksum, and neither is known until then.
#
# Usage: scripts/write-cask.sh <version> <path-to-dmg>
set -euo pipefail

version="${1:?version required}"
dmg="${2:?path to the built dmg required}"

sha="$(shasum -a 256 "$dmg" | awk '{print $1}')"

# Git does not track empty directories, so on a fresh checkout — which is
# what the release workflow always has — `Casks/` does not exist until the
# first cask is written into it.
mkdir -p Casks

cat > Casks/hippocampus.rb <<CASK
cask "hippocampus" do
  version "${version}"
  sha256 "${sha}"

  url "https://github.com/huulanka/hippocampus/releases/download/v#{version}/Hippocampus_#{version}_aarch64.dmg"
  name "Hippocampus"
  desc "Personal knowledge system fed by voice, structured without losing the original"
  homepage "https://github.com/huulanka/hippocampus"

  # Apple Silicon only. The build is aarch64, and cross-building an x86
  # bundle would ship something nobody has tested.
  depends_on arch: :arm64
  # A bare symbol already means "this release or newer" — the older
  # ">= :sonoma" spelling says the same thing and is now a style offence.
  depends_on macos: :sonoma

  app "Hippocampus.app"

  # The app's own data: the settings file, the downloaded speech model
  # (~670 MB) and the log directory. The Cloudflare Access secret is NOT
  # here — it lives in the macOS Keychain and is deliberately left alone,
  # because an uninstall should not silently revoke a credential that may
  # still be in use on another machine.
  zap trash: [
    "~/Library/Application Support/com.andreasbauer.hippocampus",
    "~/Library/Logs/com.andreasbauer.hippocampus",
    "~/Library/Saved Application State/com.andreasbauer.hippocampus.savedState",
  ]

  caveats <<~EOS
    This build is ad-hoc signed, not notarised — there is no Apple
    Developer account behind it. Homebrew no longer attaches the
    quarantine attribute to what it installs, so the app opens as
    installed and nothing extra is needed here.

    If you instead download the .dmg from the Releases page by hand, the
    browser does attach it, and Gatekeeper will refuse to open the app
    until you remove it once:

      xattr -dr com.apple.quarantine /Applications/Hippocampus.app

    The microphone needs the bundle: speaking a capture does not work
    from a development build, only from an installed app like this one.
  EOS
end
CASK

echo "wrote Casks/hippocampus.rb for ${version} (${sha})"
