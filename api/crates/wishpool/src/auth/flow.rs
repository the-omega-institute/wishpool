//! `/auth` routes: sign-in, callback, session and sign-out.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use wishpool_core::{CoreError, ids::PersonId, model::VerifiedIdentity};
use wishpool_public::problem_response;

use super::{
    ATTEMPT_LIFETIME, AuthState, LOGIN_COOKIE, LoginAttempt, Provider, SESSION_COOKIE,
    SESSION_LIFETIME, SessionRecord, cookies, digest, middleware::origin_allowed, pkce_challenge,
    random_token, safe_return_path,
};

const STATE_DOMAIN: &str = "login-state";
const BINDING_DOMAIN: &str = "login-binding";
pub(crate) const SESSION_DOMAIN: &str = "session";

pub fn routes(state: Arc<AuthState>) -> Router {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
        .route("/auth/donate", get(donate))
        .with_state(state)
}

#[derive(Debug, Default, Deserialize)]
struct LoginParams {
    return_to: Option<String>,
    /// Dev mode only: the name to sign in as.
    #[serde(rename = "as")]
    as_name: Option<String>,
}

fn problem(error: CoreError) -> Response {
    let status = match error {
        CoreError::Unauthenticated => StatusCode::UNAUTHORIZED,
        CoreError::Forbidden(_) => StatusCode::FORBIDDEN,
        CoreError::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    let code = match status {
        StatusCode::UNAUTHORIZED => "not_authenticated",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::UNPROCESSABLE_ENTITY => "invalid",
        _ => "unavailable",
    };
    problem_response(status, code, error.to_string())
}

async fn login(State(state): State<Arc<AuthState>>, Query(params): Query<LoginParams>) -> Response {
    let return_to = safe_return_path(params.return_to.as_deref());
    match &state.provider {
        Provider::Dev => dev_login(&state, params.as_name, return_to).await,
        Provider::NyxId(client) => {
            let (oauth_state, binding, nonce, verifier) = (
                random_token(),
                random_token(),
                random_token(),
                random_token(),
            );
            let attempt = LoginAttempt {
                binding: digest(BINDING_DOMAIN, &binding),
                verifier: verifier.clone(),
                nonce: nonce.clone(),
                return_to,
                expires_at: Utc::now() + ATTEMPT_LIFETIME,
                donation: None,
            };
            if let Err(error) = state
                .store
                .put_attempt(&digest(STATE_DOMAIN, &oauth_state), &attempt)
                .await
            {
                return problem(error);
            }
            let url = match client.authorize_url(&oauth_state, &pkce_challenge(&verifier), &nonce) {
                Ok(url) => url,
                Err(error) => return problem(error),
            };
            let mut response = Redirect::to(&url).into_response();
            response.headers_mut().append(
                header::SET_COOKIE,
                cookies::set(
                    LOGIN_COOKIE,
                    &binding,
                    "/auth",
                    ATTEMPT_LIFETIME,
                    state.secure_cookies,
                ),
            );
            response
        }
    }
}

/// Dev mode: a form to choose a name, then a session for `dev:<name>`.
async fn dev_login(state: &AuthState, as_name: Option<String>, return_to: String) -> Response {
    let name = as_name
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty() && n.len() <= 64);
    let Some(name) = name else {
        let escaped = return_to
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;");
        return Html(format!(
            "<!doctype html><meta charset=utf-8><title>Dev sign-in</title>\
             <form method=get action=/auth/login style=\"font:16px system-ui;margin:4rem auto;max-width:24rem\">\
             <h1>Development sign-in</h1><p>NyxID is disabled. Sign in as <code>dev:&lt;name&gt;</code>.</p>\
             <input name=as placeholder=name autofocus required> <input type=hidden name=return_to value=\"{escaped}\">\
             <button>Sign in</button></form>"
        ))
        .into_response();
    };
    let identity = VerifiedIdentity {
        subject: PersonId(format!("dev:{name}")),
        name: Some(name),
        email: None,
        picture: None,
    };
    match establish_session(state, &identity).await {
        Ok(cookie) => {
            let mut response = Redirect::to(&return_to).into_response();
            response.headers_mut().append(header::SET_COOKIE, cookie);
            response
        }
        Err(error) => problem(error),
    }
}

async fn establish_session(
    state: &AuthState,
    identity: &VerifiedIdentity,
) -> Result<axum::http::HeaderValue, CoreError> {
    let person = state.app.sign_in(identity).await?;
    let token = random_token();
    let record = SessionRecord {
        subject: person.id,
        expires_at: Utc::now() + SESSION_LIFETIME,
    };
    state
        .store
        .create_session(&digest(SESSION_DOMAIN, &token), &record)
        .await?;
    Ok(cookies::set(
        SESSION_COOKIE,
        &token,
        "/",
        SESSION_LIFETIME,
        state.secure_cookies,
    ))
}

#[derive(Debug, Default, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(state): State<Arc<AuthState>>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Response {
    let Provider::NyxId(client) = &state.provider else {
        return problem(CoreError::forbidden("NyxID sign-in is not configured"));
    };
    let Some(oauth_state) = params.state else {
        return problem(CoreError::invalid("missing state"));
    };
    let attempt = match state
        .store
        .take_attempt(&digest(STATE_DOMAIN, &oauth_state))
        .await
    {
        Ok(Some(attempt)) if attempt.expires_at > Utc::now() => attempt,
        Ok(_) => return problem(CoreError::Unauthenticated),
        Err(error) => return problem(error),
    };
    let binding = cookies::read(&headers, LOGIN_COOKIE).map(|b| digest(BINDING_DOMAIN, &b));
    if binding.as_deref() != Some(attempt.binding.as_str()) {
        tracing::warn!("sign-in callback from a browser that did not start it");
        return problem(CoreError::Unauthenticated);
    }
    let clear_binding = cookies::clear(LOGIN_COOKIE, "/auth", state.secure_cookies);
    if let Some(error) = params.error {
        let mut response =
            Redirect::to(&format!("/?login_error={}", urlencode(&error))).into_response();
        response
            .headers_mut()
            .append(header::SET_COOKIE, clear_binding);
        return response;
    }
    let Some(code) = params.code else {
        return problem(CoreError::invalid("missing code"));
    };
    let result = async {
        let (identity, delegated) = client
            .complete_with_tokens(&code, &attempt.verifier, &attempt.nonce)
            .await?;
        if let Some(request) = &attempt.donation {
            record_donation(&state, request, &identity, delegated).await?;
        }
        establish_session(&state, &identity).await
    }
    .await;
    match result {
        Ok(session_cookie) => {
            let mut response = Redirect::to(&attempt.return_to).into_response();
            response
                .headers_mut()
                .append(header::SET_COOKIE, session_cookie);
            response
                .headers_mut()
                .append(header::SET_COOKIE, clear_binding);
            response
        }
        Err(error) => problem(error),
    }
}

fn urlencode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn anonymous(state: &AuthState) -> serde_json::Value {
    json!({
        "authenticated": false,
        "donations_enabled": state.donations.is_some(),
        "dev_sign_in": matches!(state.provider, Provider::Dev),
    })
}

async fn session(State(state): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let Some(token) = cookies::read(&headers, SESSION_COOKIE) else {
        return Json(anonymous(&state)).into_response();
    };
    let result = async {
        let Some(record) = state.store.session(&digest(SESSION_DOMAIN, &token)).await? else {
            return Ok(None);
        };
        let identity = VerifiedIdentity {
            subject: record.subject,
            name: None,
            email: None,
            picture: None,
        };
        let caller = state.app.caller(&identity).await?;
        state.app.me(&caller).await.map(Some)
    }
    .await;
    match result {
        Ok(Some(person)) => Json(json!({
            "authenticated": true,
            "person": person,
            "donations_enabled": state.donations.is_some(),
            "dev_sign_in": matches!(state.provider, Provider::Dev),
        }))
        .into_response(),
        Ok(None) => Json(anonymous(&state)).into_response(),
        Err(error) => problem(error),
    }
}

async fn logout(State(state): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    if !origin_allowed(&headers, &state.public_origin) {
        return problem(CoreError::forbidden("cross-origin sign-out refused"));
    }
    if let Some(token) = cookies::read(&headers, SESSION_COOKIE)
        && let Err(error) = state
            .store
            .delete_session(&digest(SESSION_DOMAIN, &token))
            .await
    {
        return problem(error);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookies::clear(SESSION_COOKIE, "/", state.secure_cookies),
    );
    response
}

#[derive(Debug, Default, Deserialize)]
struct DonateParams {
    /// Tokens per month the venue may spend.
    cap: Option<u64>,
    model: Option<String>,
    return_to: Option<String>,
}

/// Start incremental consent for donated quota. Requires a signed-in
/// browser; the identity NyxID returns must be that same person.
async fn donate(
    State(state): State<Arc<AuthState>>,
    headers: HeaderMap,
    Query(params): Query<DonateParams>,
) -> Response {
    let (Provider::NyxId(client), Some(donations)) = (&state.provider, &state.donations) else {
        return problem(CoreError::forbidden(
            "donations are not enabled on this deployment",
        ));
    };
    // Starting consent is a state-changing step: accept navigations from the
    // venue itself or typed by the user, not links on other sites.
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok())
        && !matches!(site, "same-origin" | "none")
    {
        return problem(CoreError::forbidden(
            "donations must be started from the venue",
        ));
    }
    let Some(token) = cookies::read(&headers, SESSION_COOKIE) else {
        return problem(CoreError::Unauthenticated);
    };
    let donor = match state.store.session(&digest(SESSION_DOMAIN, &token)).await {
        Ok(Some(session)) => session.subject,
        Ok(None) => return problem(CoreError::Unauthenticated),
        Err(error) => return problem(error),
    };
    let cap = params.cap.unwrap_or(200_000);
    if let Err(error) = wishpool_core::model::DonationGrant::validate_cap(cap) {
        return problem(error);
    }
    let model = params
        .model
        .unwrap_or_else(|| donations.default_model.clone());
    if !crate::donations::valid_model(&model) {
        return problem(CoreError::invalid("unsupported model name"));
    }
    let (oauth_state, binding, nonce, verifier) = (
        random_token(),
        random_token(),
        random_token(),
        random_token(),
    );
    let attempt = LoginAttempt {
        binding: digest(BINDING_DOMAIN, &binding),
        verifier: verifier.clone(),
        nonce: nonce.clone(),
        return_to: safe_return_path(params.return_to.as_deref().or(Some("/contribute"))),
        expires_at: Utc::now() + ATTEMPT_LIFETIME,
        donation: Some(super::DonationRequest {
            donor,
            monthly_cap: cap,
            model,
        }),
    };
    if let Err(error) = state
        .store
        .put_attempt(&digest(STATE_DOMAIN, &oauth_state), &attempt)
        .await
    {
        return problem(error);
    }
    let url = match client.donation_authorize_url(
        &oauth_state,
        &pkce_challenge(&verifier),
        &nonce,
        &donations.scope,
        &donations.service_ids,
    ) {
        Ok(url) => url,
        Err(error) => return problem(error),
    };
    let mut response = Redirect::to(&url).into_response();
    response.headers_mut().append(
        header::SET_COOKIE,
        cookies::set(
            LOGIN_COOKIE,
            &binding,
            "/auth",
            ATTEMPT_LIFETIME,
            state.secure_cookies,
        ),
    );
    response
}

async fn record_donation(
    state: &AuthState,
    request: &super::DonationRequest,
    identity: &VerifiedIdentity,
    delegated: Option<super::nyxid::DelegatedTokens>,
) -> Result<(), CoreError> {
    let donations = state
        .donations
        .as_ref()
        .ok_or_else(|| CoreError::forbidden("donations are not enabled"))?;
    if identity.subject != request.donor {
        tracing::warn!("donation consent returned a different identity than the signed-in donor");
        return Err(CoreError::Unauthenticated);
    }
    let refresh = delegated.and_then(|t| t.refresh_token).ok_or_else(|| {
        CoreError::invalid("NyxID returned no refresh token; offline access was not granted")
    })?;
    donations
        .tokens
        .put(&request.donor, &donations.cipher.seal(&refresh)?)
        .await?;
    state
        .app
        .create_or_renew_donation(&request.donor, request.monthly_cap, &request.model)
        .await?;
    Ok(())
}
