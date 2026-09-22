cask "hippocampus" do
  version "1.8.0"
  sha256 "029d7dfe5b4fbcee2c06dff340bfb75587ee0d5396da0fb1ca1b45bff9d24432"

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
    One more step, and it is not optional:

      xattr -dr com.apple.quarantine /Applications/Hippocampus.app

    This build is ad-hoc signed, not notarised — there is no Apple
    Developer account behind it — so Gatekeeper refuses it until the
    quarantine attribute is gone. Homebrew attaches that attribute to the
    downloaded .dmg and the app inherits it; the --no-quarantine flag
    that used to prevent this was removed in Homebrew 7. Run the line
    again after every upgrade.

    If an older version was installed by hand rather than by Homebrew,
    remove it first — Homebrew will not take over an app it did not
    install:

      rm -rf /Applications/Hippocampus.app

    The microphone needs the bundle: speaking a capture does not work
    from a development build, only from an installed app like this one.
  EOS
end
