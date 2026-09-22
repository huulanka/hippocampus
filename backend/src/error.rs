use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// A failed request.
///
/// Anything converted from an ordinary error is a 500 and tells the client
/// nothing beyond that — internal failures should not leak their shape.
/// Faults the caller can actually fix are constructed explicitly and do
/// carry a message, because a client that uploads the wrong audio format
/// deserves to be told which part was wrong rather than "internal error".
pub struct AppError {
    status: StatusCode,
    message: Option<String>,
    source: anyhow::Error,
}

/// Prints the cause, not the wrapper.
///
/// Written out rather than derived because the point of logging one of
/// these is always the underlying fault — the status and the sanitised
/// client message are the two things a reader already knows by the time
/// they are looking at the log line.
impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.source)
    }
}

impl AppError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::BAD_REQUEST,
            source: anyhow::anyhow!(message.clone()),
            message: Some(message),
        }
    }

    /// Rejects a request that did not prove who it is.
    ///
    /// The message deliberately says only that, never which part of the
    /// token was wrong: a caller probing the edge should learn nothing
    /// about what would have worked.
    pub fn unauthorized(reason: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            source: anyhow::anyhow!(reason.into()),
            message: Some("not authorised".to_string()),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            status: StatusCode::NOT_FOUND,
            source: anyhow::anyhow!(message.clone()),
            message: Some(message),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(error = ?self.source, "request failed");
        } else {
            tracing::debug!(error = ?self.source, status = %self.status, "request rejected");
        }

        let body = self.message.unwrap_or_else(|| "internal error".to_string());

        (self.status, Json(json!({ "error": body }))).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: None,
            source: err.into(),
        }
    }
}
