import AppIntents
import Foundation

// The Action Button's "Record" (docs/iphone.md, I5).
//
// Lives in the app target, not in the speech plugin: iOS only offers an
// App Intent it finds in the app's own binary, and the plugin is a library
// built outside Xcode. `scripts/prepare-ios.sh` copies this file into the
// generated project. All it does is leave a note for the plugin, which
// hands it to the webview; recording itself goes the usual way.

struct RecordIntent: AppIntent {
  static let title: LocalizedStringResource = "Record a Note"
  static let description = IntentDescription("Opens Hippocampus and starts recording.")
  static let openAppWhenRun = true

  @MainActor
  func perform() async throws -> some IntentResult {
    // Spelled the same in the plugin (`SpeechPlugin.swift`).
    UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: "hippocampus.recordRequested")
    NotificationCenter.default.post(name: Notification.Name("hippocampus.recordRequested"), object: nil)
    return .result()
  }
}

/// Makes the intent offerable without the user building a Shortcut: it
/// appears under Settings, Action Button, Shortcut, and to Siri.
struct HippocampusShortcuts: AppShortcutsProvider {
  static var appShortcuts: [AppShortcut] {
    AppShortcut(
      intent: RecordIntent(),
      phrases: [
        "Record a note in \(.applicationName)",
        "Capture with \(.applicationName)",
      ],
      shortTitle: "Record",
      systemImageName: "mic.fill"
    )
  }
}
