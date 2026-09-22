cask "hippocampus" do
  version "1.5.2"
  sha256 "746947c3f0fe1ff8d10b6da43f428e0e46bbb7913c6e2f96b007445bd6009d3c"

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
