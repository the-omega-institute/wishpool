//! Browser sign-in through NyxID and request authentication.
//!
//! The server is the OAuth client (a backend-for-frontend): it runs the
//! authorization-code flow with PKCE, verifies the ID token against NyxID's
//! JWKS, and gives the browser only an opaque, HttpOnly session cookie whose
//! SHA-256 digest is stored. Machine clients present NyxID access tokens,
//! verified locally against the same JWKS.

mod cookies;
mod flow;
mod memory;
mod middleware;
pub mod nyxid;

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use wishpool_core::{CoreResult, app::App, ids::PersonId};

pub use flow::routes;
pub use memory::MemoryAuthStore;
pub use middleware::authenticate;

pub const SESSION_COOKIE: &str = "wp_session";
pub const LOGIN_COOKIE: &str = "wp_login";
pub const SESSION_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const ATTEMPT_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub subject: PersonId,
    pub expires_at: DateTime<Utc>,
}

/// An authorization request in flight, keyed by the digest of its `state`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginAttempt {
    /// Digest of the browser-binding cookie set when the attempt began.
    pub binding: String,
    pub verifier: String,
    pub nonce: String,
    pub return_to: String,
    pub expires_at: DateTime<Utc>,
    /// Set when the attempt authorizes donated quota rather than sign-in.
    pub donation: Option<DonationRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonationRequest {
    /// The signed-in person who asked to donate; the returned identity must match.
    pub donor: PersonId,
    pub monthly_cap: u64,
    pub model: String,
}

#[async_trait]
pub trait AuthStore: Send + Sync {
    async fn create_session(&self, digest: &str, session: &SessionRecord) -> CoreResult<()>;
    async fn session(&self, digest: &str) -> CoreResult<Option<SessionRecord>>;
    async fn delete_session(&self, digest: &str) -> CoreResult<()>;
    async fn put_attempt(&self, state_digest: &str, attempt: &LoginAttempt) -> CoreResult<()>;
    /// Remove and return the attempt; a state can be redeemed once.
    async fn take_attempt(&self, state_digest: &str) -> CoreResult<Option<LoginAttempt>>;
}

pub enum Provider {
    NyxId(Arc<nyxid::NyxIdClient>),
    /// Local development: sign in as any name.
    Dev,
}

pub struct AuthState {
    pub app: Arc<App>,
    pub store: Arc<dyn AuthStore>,
    pub provider: Provider,
    /// Browser origin, e.g. `https://wishpool.example.org`.
    pub public_origin: String,
    pub secure_cookies: bool,
    /// Present when donated quota is enabled.
    pub donations: Option<Arc<crate::donations::Donations>>,
}

pub(crate) fn random_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Domain-separated digest of a secret, as stored.
pub(crate) fn digest(domain: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(secret.as_bytes());
    hex(&hasher.finalize())
}

pub(crate) fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A same-origin path to return to after sign-in; anything else becomes `/`.
pub(crate) fn safe_return_path(value: Option<&str>) -> String {
    match value {
        Some(path)
            if path.starts_with('/')
                && !path.starts_with("//")
                && !path.contains('\\')
                && !path.chars().any(char::is_control)
                && path.len() <= 2048 =>
        {
            path.to_owned()
        }
        _ => "/".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_paths_stay_on_origin() {
        assert_eq!(safe_return_path(Some("/wishes/1?x=2")), "/wishes/1?x=2");
        assert_eq!(safe_return_path(Some("//evil.example")), "/");
        assert_eq!(safe_return_path(Some("https://evil.example")), "/");
        assert_eq!(safe_return_path(Some("/\\evil")), "/");
        assert_eq!(safe_return_path(None), "/");
    }

    #[test]
    fn pkce_matches_rfc7636_example() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn digests_are_domain_separated() {
        assert_ne!(digest("a", "x"), digest("b", "x"));
        assert_eq!(digest("a", "x").len(), 64);
    }
}
