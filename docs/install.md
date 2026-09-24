# Installing Hippocampus

Hippocampus is two things: a Mac app that listens and shows you what you
have said, and a small backend that stores it. The app is installed with
Homebrew. The backend runs wherever you like — the same Mac, a home
server, a NAS — as long as the app can reach it over HTTP.

## The Mac app

Apple Silicon only, macOS Sonoma or newer:

```sh
brew tap huulanka/hippocampus https://github.com/huulanka/hippocampus
brew trust --cask huulanka/hippocampus/hippocampus
brew install --cask hippocampus
xattr -dr com.apple.quarantine /Applications/Hippocampus.app
```

`brew trust` is not optional and is easy to mistake for a broken tap.
Homebrew 7 refuses to load a cask from a third-party tap until it has
been trusted once, and says so with an error that reads like the cask is
missing. It is not — it is Homebrew asking whether you meant it.

If an older version was installed by hand, remove it first
(`rm -rf /Applications/Hippocampus.app`); Homebrew will not take over an
app it did not install. macOS asks for microphone permission again
afterwards, because the signature changed.

The `xattr` line is not optional either, and it is the step most likely
to be skipped because older instructions promised it away. This build is
**ad-hoc signed, not notarised** — there is no Apple Developer account
behind it — so Gatekeeper refuses it outright (`spctl` calls it "code has
no resources but signature indicates they must be present"). Removing the
quarantine attribute is what lets it open.

It used to be possible to prevent the attribute instead, with
`brew install --cask --no-quarantine`. Homebrew 7 removed that flag, and
it does still attach the attribute: it lands on the downloaded `.dmg`
with `Homebrew Cask` named as the agent, and the installed app inherits
it. Reading `Cask::Quarantine.check_quarantine_support` — which returns
`:quarantine_unavailable` unconditionally — suggests otherwise and is
about the staging step, not the download. Checked by installing:
`xattr -p com.apple.quarantine /Applications/Hippocampus.app` comes back
with a value.

Repeat the `xattr` line after every `brew upgrade --cask hippocampus`,
for the same reason.

Install it properly rather than running the app from source for daily
use: **the microphone only works from a bundled app.** macOS grants
microphone permission per bundle identity, and a development binary has
none — it records silence instead of failing, which is the worst possible
way to find out.

The cask and the `.dmg` it points at are produced by
`.github/workflows/release-app.yml` on every published release, so the
version you get is the version that was released.

## The speech model

Speech recognition runs on this machine, never on the server: audio is the
most revealing thing the system holds, so it and the microphone stream stay
local, and only the resulting text is ever sent on. See
[`adr/0004-audio-is-the-original.md`](adr/0004-audio-is-the-original.md).

`scripts/fetch-asr-model.sh` downloads int8-quantised
[parakeet-tdt-0.6b-v3](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx)
(multilingual, German included) into the app's support directory, or into
`$HIPPOCAMPUS_ASR_MODEL_DIR` when that is set. Without the model the app still
runs and typed capture still works — the record button simply stays hidden.

macOS asks for microphone permission the first time you record.

## The backend

The backend and its Postgres run from the repository's
[`docker-compose.yml`](../docker-compose.yml):

```sh
git clone https://github.com/huulanka/hippocampus && cd hippocampus
cp .env.example .env   # at least OPENROUTER_API_KEY, see below
docker compose up -d
```

It listens on port 8080. Every setting is described where it is set, in
[`.env.example`](../.env.example). The one that matters on day one is
`OPENROUTER_API_KEY`: without it captures are still stored, transcribed and
searchable, but nothing reads them, so there are no entities, no dates and
no graph. Only text is ever sent, never audio, and requests are routed to
providers with zero data retention.

Backups, restoring them, and what a deployment on a NAS needs are in
[`operations.md`](operations.md).

## Pointing the app at your backend

The desktop client's HTTP requests are made in Rust, not by the webview —
see [`adr/0009-the-webview-does-not-talk-to-the-backend.md`](adr/0009-the-webview-does-not-talk-to-the-backend.md).
The short version: a `fetch` carrying Cloudflare Access headers needs a
CORS preflight, and Access answers an unauthenticated `OPTIONS` with a
login redirect, so the request never leaves the window. A request made in
Rust has no origin and no preflight.

Point the client at a backend in **Settings → Backend**. It is checked
against `/health` before it is saved, so a typo cannot strand the screen
that would let you fix it.

If that backend sits behind a Cloudflare Zero Trust Access application,
add a **Service Token** (not your own login) under **Settings → Cloudflare
Access**. The Client ID is stored in `settings.json`; the Client Secret is
stored in the **macOS Keychain** and is never written to disk in readable
form, never sent to the webview, and never logged. A secret left in
`settings.json` by version 1.2.0 or earlier is moved into the Keychain the
first time this version starts — rotate that token afterwards, since it
was on disk in the clear until then.

| Variable | Default | What it does |
| --- | --- | --- |
| `HIPPOCAMPUS_ASR_MODEL_DIR` | app support dir | Where the speech model lives |
| `HIPPOCAMPUS_API_BASE_URL` | `http://localhost:8080` | Backend the client talks to, when the settings screen has no value saved |
