//! Resolve the caller of every `/api` request.
//!
//! - `Authorization: Bearer` must verify, or the request is refused; it never
//!   falls back to the cookie or to anonymous.
//! - The session cookie authenticates browsers. Unsafe methods authenticated
//!   by cookie must come from the public origin.
//! - Without either, the request proceeds anonymously and Layer 2 decides.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};
use wishpool_core::{CoreError, ids::PersonId, model::VerifiedIdentity};
use wishpool_public::{AuthenticatedCaller, RequestAuthentication, problem_response};

use super::{AuthState, Provider, SESSION_COOKIE, cookies, digest, flow::SESSION_DOMAIN};

pub(crate) fn origin_allowed(headers: &HeaderMap, public_origin: &str) -> bool {
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) => origin == public_origin,
        // Browsers send Origin on every unsafe cross-site request; without it,
        // accept only when fetch metadata says the request is same-origin.
        None => headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) == Some("same-origin"),
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

fn refuse(status: StatusCode, error: CoreError) -> Response {
    let code = if status == StatusCode::UNAUTHORIZED {
        "not_authenticated"
    } else if status == StatusCode::FORBIDDEN {
        "forbidden"
    } else {
        "unavailable"
    };
    problem_response(status, code, error.to_string())
}

pub async fn authenticate(
    State(state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let method = if bearer(headers).is_some() {
        wishpool_core::model::AuthenticationMethod::Bearer
    } else {
        wishpool_core::model::AuthenticationMethod::CookieSession
    };
    let identity = if let Some(token) = bearer(headers) {
        let verified = match &state.provider {
            Provider::NyxId(client) => client.verify_access_token(token).await,
            // Dev mode accepts `Bearer dev:<name>` for scripting.
            Provider::Dev => match token.strip_prefix("dev:") {
                Some(name) if !name.is_empty() => Ok(VerifiedIdentity {
                    subject: PersonId(token.to_owned()),
                    name: Some(name.to_owned()),
                    email: None,
                    picture: None,
                }),
                _ => Err(CoreError::Unauthenticated),
            },
        };
        match verified {
            Ok(identity) => Some(identity),
            Err(CoreError::Unauthenticated) => {
                return refuse(StatusCode::UNAUTHORIZED, CoreError::Unauthenticated);
            }
            Err(error) => return refuse(StatusCode::SERVICE_UNAVAILABLE, error),
        }
    } else if let Some(token) = cookies::read(headers, SESSION_COOKIE) {
        let safe = matches!(
            *request.method(),
            Method::GET | Method::HEAD | Method::OPTIONS
        );
        if !safe && !origin_allowed(headers, &state.public_origin) {
            return refuse(
                StatusCode::FORBIDDEN,
                CoreError::forbidden("cross-origin request refused"),
            );
        }
        match state.store.session(&digest(SESSION_DOMAIN, &token)).await {
            Ok(Some(session)) => Some(VerifiedIdentity {
                subject: session.subject,
                name: None,
                email: None,
                picture: None,
            }),
            // An expired or unknown cookie is anonymous; the browser re-signs in.
            Ok(None) => None,
            Err(error) => return refuse(StatusCode::SERVICE_UNAVAILABLE, error),
        }
    } else {
        None
    };

    if let Some(identity) = identity {
        match state.app.caller(&identity).await {
            Ok(caller) => {
                request.extensions_mut().insert(AuthenticatedCaller(caller));
                request
                    .extensions_mut()
                    .insert(RequestAuthentication(method));
            }
            Err(error) => return refuse(StatusCode::SERVICE_UNAVAILABLE, error),
        }
    }
    next.run(request).await
}
