//! Every request to the backend, made from this side of the webview.
//!
//! It used to be the webview's `fetch` that talked to the backend. That
//! works against `localhost` and stops working the moment the backend
//! moves behind Cloudflare Access, for a reason that has nothing to do
//! with the backend: attaching `CF-Access-Client-Id` makes the request
//! non-simple, so the browser sends a CORS preflight first, and an
//! `OPTIONS` carrying no credentials is exactly what Access answers with
//! a login redirect. The real request never goes out, and the webview
//! reports the only thing it knows — `TypeError: Load failed`.
//!
//! A request made here has no origin, so there is no preflight, no CORS,
//! and nothing to keep in sync between two systems. It also means the
//! Service Token secret never has to be handed to JavaScript to be sent:
//! the webview asks for a path, this module decides what credentials that
//! deserves.

use std::time::Duration;

use serde::Serialize;

use crate::settings::SettingsState;

const CLIENT_ID_HEADER: &str = "CF-Access-Client-Id";
const CLIENT_SECRET_HEADER: &str = "CF-Access-Client-Secret";

/// How long the settings screen waits for `/health` before calling it
/// unreachable. Long enough for a cold NAS container, short enough that a
/// typo does not feel like a hang.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

pub struct BackendClient {
    http: reqwest::Client,
}

impl BackendClient {
    pub fn new() -> Self {
        Self { http: client() }
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }
}

impl Default for BackendClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Redirects are deliberately **not** followed.
///
/// Cloudflare Access answers an unauthenticated request with a 302 to its
/// login page. Followed, that lands on an HTML page with status 200 —
/// which would make a rejected request look like a successful one whose
/// body merely failed to parse. Left unfollowed, it is unmistakable, and
/// [`describe_status`] can say what actually happened.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default()
}

/// One HTTP exchange, flattened for the webview. A non-2xx status comes
/// back as a value rather than an error: the caller wants the status *and*
/// the body, and only a request that never completed is an error here.
#[derive(Serialize)]
pub struct ApiResponse {
    pub status: u16,
    pub status_text: String,
    pub body: String,
}

/// Turns a transport failure into something worth reading. `reqwest`'s own
/// `Display` nests three levels of cause and leads with the least useful
/// one.
fn describe_transport(err: &reqwest::Error, url: &str) -> String {
    if err.is_timeout() {
        return "no answer within 5s".to_string();
    }
    if err.is_connect() {
        return format!("could not connect to {url} ({err})");
    }
    if err.is_builder() {
        return format!("{url} is not a usable address");
    }
    format!("{err}")
}

/// Says what a status means when the meaning is not obvious, because the
/// two that matter here are both easy to misread.
pub(crate) fn describe_status(
    status: reqwest::StatusCode,
    location: Option<&str>,
) -> Option<String> {
    if status.is_redirection() {
        let target = location.unwrap_or("a login page");
        return Some(format!(
            "Cloudflare Access sent a login redirect ({status} → {target}) instead of \
             answering — the Service Token was not accepted. Check that the Client ID and \
             Secret belong to a service token that is listed in the Access policy."
        ));
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Some(
            "403 — the request reached Access but was refused. The Service Token exists \
             but the Access application's policy does not include it."
                .to_string(),
        );
    }
    None
}

/// Performs a request against the configured backend, carrying the stored
/// Service Token when there is one.
///
/// `method` and `path` come from the webview; the base URL and the
/// credentials never do.
#[tauri::command]
pub async fn api_request(
    client: tauri::State<'_, BackendClient>,
    settings: tauri::State<'_, SettingsState>,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<ApiResponse, String> {
    // Read out of the locks before the first await: a `MutexGuard` held
    // across one would make this future non-Send and the command would
    // not compile.
    let base = settings.backend_base();
    let credentials = settings.credentials();

    let url = format!("{base}{path}");
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|_| format!("{method} is not an HTTP method"))?;

    let mut request = client.http().request(method, &url);
    if let Some((id, secret)) = credentials {
        request = request
            .header(CLIENT_ID_HEADER, id)
            .header(CLIENT_SECRET_HEADER, secret);
    }
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(body);
    }

    let response = request
        .send()
        .await
        .map_err(|err| describe_transport(&err, &url))?;

    let status = response.status();
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response.text().await.unwrap_or_default();

    Ok(ApiResponse {
        status: status.as_u16(),
        status_text: describe_status(status, location.as_deref())
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("").to_string()),
        body,
    })
}

/// The bytes of a capture's original recording.
///
/// An `<audio src>` cannot carry the Access headers, so behind Access the
/// element would load a login page instead of a recording. Fetching the
/// bytes here and handing the webview a blob trades streaming and seeking
/// on a long file for playback that works at all — and captures are
/// seconds long, not hours.
///
/// Returned as a raw IPC response rather than a `Vec<u8>`, which would be
/// serialised to a JSON array of numbers: roughly a tenfold blow-up of
/// every recording on its way through the bridge.
#[tauri::command]
pub async fn api_audio(
    client: tauri::State<'_, BackendClient>,
    settings: tauri::State<'_, SettingsState>,
    event_id: String,
) -> Result<tauri::ipc::Response, String> {
    let base = settings.backend_base();
    let credentials = settings.credentials();

    let url = format!("{base}/captures/{event_id}/audio");
    let mut request = client.http().get(&url);
    if let Some((id, secret)) = credentials {
        request = request
            .header(CLIENT_ID_HEADER, id)
            .header(CLIENT_SECRET_HEADER, secret);
    }

    let response = request
        .send()
        .await
        .map_err(|err| describe_transport(&err, &url))?;

    let status = response.status();
    if !status.is_success() {
        return Err(describe_status(status, None)
            .unwrap_or_else(|| format!("the backend answered {status}")));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("the recording could not be read: {err}"))?;

    Ok(tauri::ipc::Response::new(bytes.to_vec()))
}

/// Asks a backend whether it is there, with credentials that have not
/// been saved yet.
///
/// This is what lets the settings screen prove a URL and a token work
/// *before* committing either — including the case where the backend was
/// already behind Access when the screen loaded, so that neither value
/// can be checked in isolation. An empty `client_secret` with a
/// `client_id` present means "use the one already in the Keychain", which
/// is how the screen can re-check without ever holding the secret.
#[tauri::command]
pub async fn check_backend(
    client: tauri::State<'_, BackendClient>,
    settings: tauri::State<'_, SettingsState>,
    url: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<String, String> {
    let base = url
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| settings.backend_base());

    let id = client_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let secret = client_secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| settings.secret());

    let credentials = match (id, secret) {
        (Some(id), Some(secret)) => Some((id, secret)),
        _ => None,
    };

    health_check(client.http(), &base, credentials).await
}

/// The part of [`check_backend`] that has no Tauri in it, so it can be
/// tested against a real socket.
async fn health_check(
    http: &reqwest::Client,
    base: &str,
    credentials: Option<(String, String)>,
) -> Result<String, String> {
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(format!(
            "{base} has no scheme — it needs to start with http:// or https://"
        ));
    }

    let health = format!("{base}/health");
    let mut request = http.get(&health).timeout(CHECK_TIMEOUT);
    if let Some((id, secret)) = credentials {
        request = request
            .header(CLIENT_ID_HEADER, id)
            .header(CLIENT_SECRET_HEADER, secret);
    }

    let response = request
        .send()
        .await
        .map_err(|err| describe_transport(&err, &health))?;

    let status = response.status();
    if status.is_success() {
        return Ok(base.to_string());
    }

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    Err(describe_status(status, location.as_deref())
        .unwrap_or_else(|| format!("the backend answered {status}")))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::sync::mpsc;

    use super::*;

    /// A socket that answers one request with `response` and hands the
    /// request it received back over a channel.
    ///
    /// Hand-rolled rather than pulling in a mock-server crate: what needs
    /// proving is how this module reacts to raw status lines Cloudflare
    /// Access actually sends, and that is exactly what a raw socket can
    /// say.
    fn serve_once(response: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let read = stream.read(&mut buf).unwrap_or(0);
                let _ = tx.send(String::from_utf8_lossy(&buf[..read]).to_string());
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        (format!("http://{addr}"), rx)
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tauri::async_runtime::block_on(future)
    }

    #[test]
    fn a_backend_that_answers_is_accepted() {
        let (base, _rx) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");
        let result = block_on(health_check(&client(), &base, None));
        assert_eq!(result, Ok(base));
    }

    /// The failure this whole module exists for. Cloudflare Access
    /// answers an unauthenticated request with a redirect to its login
    /// page; followed, that would arrive as a perfectly healthy 200 and
    /// the check would pass against a backend it never reached.
    #[test]
    fn an_access_login_redirect_is_not_mistaken_for_a_healthy_backend() {
        let (base, _rx) = serve_once(
            "HTTP/1.1 302 Found\r\nlocation: https://team.cloudflareaccess.com/cdn-cgi/access/login\r\ncontent-length: 0\r\n\r\n",
        );
        let err = block_on(health_check(&client(), &base, None)).unwrap_err();
        assert!(err.contains("Service Token was not accepted"), "{err}");
        assert!(err.contains("cloudflareaccess.com"), "{err}");
    }

    /// The headers have to be on the wire, not merely configured. This is
    /// the assertion the old `fetch` path could never make from inside
    /// the webview.
    #[test]
    fn the_service_token_is_sent_as_headers() {
        let (base, rx) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");
        block_on(health_check(
            &client(),
            &base,
            Some(("abc.access".to_string(), "shh".to_string())),
        ))
        .unwrap();

        let request = rx.recv().unwrap().to_lowercase();
        assert!(
            request.contains("cf-access-client-id: abc.access"),
            "{request}"
        );
        assert!(
            request.contains("cf-access-client-secret: shh"),
            "{request}"
        );
    }

    /// No credentials means no headers — not empty ones, which Access
    /// would treat as a malformed token rather than as an anonymous
    /// request.
    #[test]
    fn no_credentials_means_no_headers() {
        let (base, rx) = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");
        block_on(health_check(&client(), &base, None)).unwrap();

        let request = rx.recv().unwrap().to_lowercase();
        assert!(!request.contains("cf-access-client-id"), "{request}");
    }

    /// A URL typed without a scheme is the likeliest typo of all, and
    /// `reqwest`'s own message for it ("builder error: relative URL
    /// without a base") explains nothing to the person who typed it.
    #[test]
    fn a_url_without_a_scheme_says_so() {
        let err = block_on(health_check(&client(), "hippocampus.example.com", None)).unwrap_err();
        assert!(err.contains("http://"), "{err}");
    }

    /// The whole point of `Policy::none()`: an Access challenge must stay
    /// visible as a redirect instead of being followed into a login page
    /// that returns 200.
    #[test]
    fn a_redirect_is_reported_as_an_access_challenge() {
        let message = describe_status(
            reqwest::StatusCode::FOUND,
            Some("https://team.cloudflareaccess.com/cdn-cgi/access/login"),
        )
        .expect("a redirect must be explained");
        assert!(message.contains("Service Token was not accepted"));
        assert!(message.contains("cloudflareaccess.com"));
    }

    #[test]
    fn a_forbidden_says_which_side_refused() {
        let message =
            describe_status(reqwest::StatusCode::FORBIDDEN, None).expect("403 must be explained");
        assert!(message.contains("policy"));
    }

    /// An ordinary failure keeps its own wording rather than being
    /// dressed up as an Access problem.
    #[test]
    fn other_statuses_are_left_alone() {
        assert!(describe_status(reqwest::StatusCode::NOT_FOUND, None).is_none());
        assert!(describe_status(reqwest::StatusCode::OK, None).is_none());
    }
}
