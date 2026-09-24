# The iPhone

Like `docs/prospective-memory.md`, this document says *why* and *what*.
The question of where structuring runs once models on the device are good
enough is in `docs/adr/0014-structuring-is-a-job-any-device-may-claim.md`.

## The problem it solves

Every capture so far was made at a Mac. A thought you have on the way
home, in a queue or on a walk is gone by the time you sit down again, and
the product exists to stop exactly that. P4 put the Mac first because
that is where most of the day happens. The phone covers the rest.

A Shortcut with Apple's keyboard dictation was the cheap answer in
`docs/product.md`. It is not good enough: with fast, spontaneous speech it
is slow and gets too much wrong, and proper names, the words that matter
most here, suffer first. Reading matters just as much as capturing. Before
meeting someone you want to see what you meant to ask them, and that
happens away from the desk.

## What it is

An app of its own, built from the same client: Tauri on iOS, with the
React interface and the Rust core reused, and Swift where only Swift can
do it (recording, speech recognition, Face ID, App Intents).

| Capture | Read |
|---|---|
| The Action Button starts a recording; a large button in the app does the same | Today, the timeline and a capture's detail with its audio |
| Typed notes | Search |
| An outbox, so a capture made without network is sent later | Open intentions, and the brief for a meeting |
| | The weekly review |
| | The graph, adapted for touch |

## Decisions

| # | Decision | Why |
|---|---|---|
| I1 | **Tauri on iOS with Swift plugins**, not a native SwiftUI app | Timeline, detail, graph, intentions and review already exist as React pages, and the request path, the outbox and the keychain handling already exist in Rust. A second code base is too much for a one-person project. The price is a mixed project and a generated Xcode project to maintain. |
| I2 | **No paid developer account.** The app is signed with a free Apple ID (SideStore, or Xcode directly) | It is a private app for one person. Alternative app marketplaces in the EU do not help: every app distributed there has to be notarised by Apple, which needs the paid membership. |
| I3 | **Built within the limits of free signing** | A free signature lasts 7 days and has to be renewed, at most 3 apps can be installed at once, and at most 10 app IDs can be registered a week. Every extension (a widget, a control) needs its own app ID. There is no push and no iCloud. Version 1 therefore has no extension at all. |
| I4 | **Parakeet v3 on the phone too**, the model the Mac uses, through FluidAudio on the Neural Engine | Spike c) ran Apple's `SpeechTranscriber` and `DictationTranscriber` against it on recordings of fast, spontaneous speech. Both got proper names wrong about as often as Parakeet does, and on top of that made clearly more mistakes with ordinary words. A list of known names handed to Apple's model as context did not change that. The price is a model of about 600 MB to download once, which Apple's built-in models would have avoided. One engine on every device also keeps the archive consistent: `transcript.derived` names its model either way (ADR 0004). |
| I5 | **The Action Button and a button in the app** start a recording. Opening the app does not | An App Intent in the main target can be put on the Action Button without an extension. Opening the app to read must never start a recording; the Mac learned that from the menu bar icon (ADR 0012). |
| I6 | **Capturing works offline, reading does not** | The outbox is the same promise as on the Mac: nothing is lost. Reading needs the backend, and no copy of the archive lives on the phone, so a lost phone gives away less. |
| I7 | **Reading is behind Face ID, capturing is not** | ADR 0011, unchanged. |
| I8 | **Set up by scanning a QR code** from the Mac's Settings | The Cloudflare Access client ID and secret would otherwise have to be typed or pasted on the phone. The Mac shows the code only after Touch ID and only briefly; the phone keeps the secret in its keychain. Keychain sync through iCloud is out, because free signing has no iCloud entitlement. |
| I9 | **The audio goes to the NAS**, like the Mac's | ADR 0004. The phone keeps a recording only until the outbox has delivered it. |
| I10 | **Structuring stays on the backend**, through OpenRouter, like the echo judge and the reranking | Spike d) measured Apple's on-device models against it and neither was good enough (ADR 0014). The only model the phone runs is speech recognition (I4). |
| I11 | **No Watch app.** The Watch is reached through a control on the iPhone | Since watchOS 26, a Control Center control of an iPhone app appears on the Watch without a Watch app of its own: in Control Center, in the Smart Stack and on the Action Button of an Ultra. The action runs on the iPhone. A Watch app would need signing that is not realistic with a free account. |
| I12 | **Recording from a control uses `AudioRecordingIntent`** and shows a Live Activity with a stop button | A control whose action brings the app to the foreground does not appear on the Watch, so the recording has to start in the background, and iOS requires a Live Activity while it runs. The Live Activity is mirrored into the Watch's Smart Stack. The recording uses the iPhone's microphone, which is worse from a pocket. |

## Phases

Each phase ends with a criterion for going on or stopping, as in
`docs/roadmap.md`.

### 0. Spikes

Four throwaway experiments, independent of each other. None of their code
is meant to stay.

- **a) Skeleton.** A Tauri iOS app, signed for free, installed on the
  phone, reaching the backend through Cloudflare Access with a service
  token from the keychain, and surviving a renewal of its signature with
  its data intact.
  *Go on when* a request from the phone gets a real answer from the NAS.
  *Stop when* free signing cannot carry a Tauri app; then the question of
  a paid account comes back.
- **b) Extension under free signing.** A widget extension with a control
  and an App Group, next to the skeleton.
  *Decides* whether phase 3 is possible without paying.
- **c) Speech.** Recordings of your own, spoken the way you actually
  speak, run through `SpeechAnalyzer` and through Parakeet v3, compared
  by word error rate and by how the proper names come out. The
  recordings stay on your own machine.
  *Decides* the default engine. Decided: Parakeet, see I4. Neither
  engine is good with proper names, so correcting a transcript stays
  part of the app.
- **d) Structuring on the device.** The existing structuring prompt, run
  over your own notes with Apple's smaller on-device model, its larger
  one, and the current OpenRouter model, compared on entities, types,
  dates and intentions. Run locally; neither the notes nor the results go
  into the repository.
  *Decides* whether ADR 0014 is built at all.

### 1. Capture and read

The table under *What it is*, without the graph: Action Button, button,
typed notes, outbox, Face ID, QR setup, Today, timeline, detail, search,
intentions and brief, review. The layouts are the ones from the design
canvas's *Mobile* artboard, reviewed at real phone size.

*Go on when* a week of captures has been made from the phone without one
being lost or a renewal breaking the app.

### 2. The graph on a phone

Map and Orbit rebuilt for touch: a finger is not a pointer, there is no
hover, and the canvas has to stay smooth in a webview on a small screen.

*Stop when* it cannot be made to feel good; then the phone links from a
capture to its entities as a list, and the graph stays a desk view.

### 3. Control, Live Activity, Watch

Only if spike b) says free signing carries it. One control, "Record", on
the iPhone's Control Center, Lock Screen and Action Button, and through
watchOS on the Watch as well.

### 4. Structuring on the devices

Not planned. Spike d) found the on-device models not good enough; ADR 0014
records what was measured and what would reopen it.

## Out of scope

| Not building | Why |
|---|---|
| A Watch app | See I11. |
| A local copy of the archive on the phone | See I6. Revisit if reading in a dead zone turns out to matter. |
| The calendar on the phone | ADR 0013 keeps the calendar on each Mac. The phone shows open intentions and can ask for a brief, but it does not watch meetings. |
| Push notifications | Free signing has none. Local notifications need no entitlement and stay possible if the review or a meeting should announce itself on the phone as well. |
| Android | Nobody here uses it. |

## Open questions

- Does Tauri's webview graph run well enough on a phone? Phase 2 finds out.
- Do free signing and SideStore keep an App Group intact across a
  renewal? Spike b) finds out.
- How much does a capture from a control lose by using the iPhone's
  microphone in a pocket, compared to taking the phone out?
