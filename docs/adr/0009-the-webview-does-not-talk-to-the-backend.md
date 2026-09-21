# ADR 0009: The webview does not talk to the backend

## Status
Accepted (2026-09-21)

## Context
Until now the desktop client's HTTP requests were made by the webview, with
`fetch`, exactly as the browser build makes them. The Cloudflare Access
Service Token was read out of `settings.json` on startup, handed to
JavaScript, and attached as `CF-Access-Client-Id` / `CF-Access-Client-Secret`
headers on every request.

That worked against `localhost` and stopped working the moment the backend
moved behind Cloudflare Access on the NAS. Every save in the settings screen
came back as:

```
Not saved: TypeError: Load failed
```

The cause is not the backend. Attaching those two headers makes the request
non-simple, so WebKit sends a CORS preflight first — and an `OPTIONS`
carrying no credentials is precisely what Access answers with a redirect to
its login page. The real request is never sent. `TypeError: Load failed` is
all the webview can say about it: no status, no body, and no way to tell a
wrong URL from a wrong token from a backend that is simply down.

Fixing it inside the browser model would have meant configuring CORS in two
places that must agree — the Access application's own CORS settings *and*
the backend's `Access-Control-Allow-Headers` — to permit a preflight that
exists only because of a same-origin policy that protects nothing here.
There is no other origin: this is a desktop app whose "page" is its own
bundle.

A second problem sat in the same code path. The Client Secret was in
`settings.json` in plain text, and the design required it to be: the
webview cannot send a header it has not been given. So a credential that
opens every capture ever recorded was readable by anything that could read
a file in the app's config directory, and it went into every Time Machine
backup in the clear.

## Decision
The webview asks for a path. Rust performs the request.

`client/src-tauri/src/backend.rs` owns a `reqwest::Client` and exposes three
commands — `api_request`, `api_audio`, `check_backend`. It reads the backend
URL and the Service Token from settings itself; neither is a parameter the
webview supplies.

The Client Secret moves to the macOS Keychain
(`client/src-tauri/src/keychain.rs`). `settings.json` keeps the Client ID,
which is an identifier rather than a credential. The webview is told
`cf_access_configured: true` and never the secret. A secret found in an
older `settings.json` is moved into the Keychain on first start and removed
from the file.

Redirects are not followed (`redirect::Policy::none()`), so an Access
challenge stays a 302 instead of arriving as a 200 with a login page in the
body.

The browser build keeps using `fetch`. It exists for development against a
local backend, which has no Access in front of it.

## Consequences
- No preflight, no CORS, nothing to keep in sync across two systems for the
  desktop app. The backend's CORS configuration now only concerns the
  browser development build.
- Failures can be named. A login redirect, a 403 from an Access policy, a
  URL with no scheme and a connection that timed out are four different
  sentences instead of one `TypeError`.
- The secret is not on disk in readable form, and is not in the webview's
  memory at all. The Keychain read happens at most once per run, and only
  when a Service Token is actually configured — a local install never
  triggers a Keychain prompt.
- Audio playback changed shape. An `<audio src>` cannot carry the Access
  headers, so the bytes are fetched in Rust and handed over as a blob. That
  gives up streaming and seeking into a file that has not finished
  downloading; captures are seconds long, so the cost is theoretical.
- The voice path now uses the configured backend. It previously read
  `HIPPOCAMPUS_API_BASE_URL` once at startup and ignored the settings
  screen entirely — so the one path that matters most was the one that
  could not be pointed at the NAS.
- Every request is one IPC hop slower. Unmeasurable next to a network
  round trip to a NAS.
- Unlike a `fetch` in a webview, this code is testable from a test binary:
  the Access-challenge and header-on-the-wire cases are covered against a
  real socket in `backend.rs`.
