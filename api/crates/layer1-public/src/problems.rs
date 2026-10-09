//! RFC 9457 problem documents. Each [`CoreError`] variant maps to exactly one
//! status and code.

use axum::{
    Json,
    extract::rejection::{JsonRejection, PathRejection, QueryRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use wishpool_core::CoreError;

const PROBLEM_BASE: &str = "urn:wishpool:problem";

#[derive(Serialize)]
struct ProblemDocument<'a> {
    #[serde(rename = "type")]
    problem_type: String,
    title: &'a str,
    status: u16,
    detail: String,
    code: &'a str,
}

pub fn problem_response(status: StatusCode, code: &str, detail: impl Into<String>) -> Response {
    let document = ProblemDocument {
        problem_type: format!("{PROBLEM_BASE}:{code}"),
        title: code,
        status: status.as_u16(),
        detail: detail.into(),
        code,
    };
    let mut response = (status, Json(document)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

/// A Layer 2 error on its way to the wire.
#[derive(Debug)]
pub struct Problem(pub CoreError);

impl From<CoreError> for Problem {
    fn from(error: CoreError) -> Self {
        Self(error)
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            CoreError::Unauthenticated => (StatusCode::UNAUTHORIZED, "not_authenticated"),
            CoreError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            CoreError::NotFound { .. } => (StatusCode::NOT_FOUND, "not_found"),
            CoreError::Invalid(_) => (StatusCode::UNPROCESSABLE_ENTITY, "invalid"),
            CoreError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            CoreError::StaleRevision { .. } => (StatusCode::CONFLICT, "stale_revision"),
            CoreError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
        };
        if status.is_server_error() {
            tracing::error!(error = %self.0, "request failed");
        }
        problem_response(status, code, self.0.to_string())
    }
}

impl From<JsonRejection> for Problem {
    fn from(rejection: JsonRejection) -> Self {
        Self(CoreError::Invalid(rejection.body_text()))
    }
}

impl From<QueryRejection> for Problem {
    fn from(rejection: QueryRejection) -> Self {
        Self(CoreError::Invalid(rejection.body_text()))
    }
}

impl From<PathRejection> for Problem {
    fn from(rejection: PathRejection) -> Self {
        Self(CoreError::Invalid(rejection.body_text()))
    }
}
