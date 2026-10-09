//! NyxID as the OpenID Connect provider.
//!
//! Endpoints come from `/.well-known/openid-configuration`. ID tokens are
//! verified with `aud = client_id` and the request nonce; access tokens
//! presented by machine clients are verified with `aud = issuer` and must be
//! user access tokens (`token_type = "access"`) or service-account tokens
//! (`sa = true`). Signing keys are cached and refetched when an unknown `kid`
//! appears, at most once a minute.

use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;
use wishpool_core::{CoreError, CoreResult, ids::PersonId, model::VerifiedIdentity};

const JWKS_REFRESH_FLOOR: Duration = Duration::from_secs(60);
const LEEWAY_SECONDS: u64 = 60;

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

pub struct NyxIdClient {
    http: reqwest::Client,
    discovery: Discovery,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    keys: RwLock<KeyCache>,
}

struct KeyCache {
    set: JwkSet,
    fetched_at: Option<Instant>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Tokens returned by NyxID for delegated use (donated quota).
#[derive(Clone)]
pub struct DelegatedTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

impl std::fmt::Debug for DelegatedTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DelegatedTokens(<redacted>)")
    }
}

#[derive(Deserialize)]
struct IdClaims {
    sub: String,
    nonce: Option<String>,
    name: Option<String>,
    email: Option<String>,
    picture: Option<String>,
}

#[derive(Deserialize)]
struct AccessClaims {
    sub: String,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    sa: bool,
}

impl NyxIdClient {
    /// Discover the provider and load its signing keys.
    pub async fn discover(
        base_url: &Url,
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    ) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;
        let discovery_url = base_url.join(".well-known/openid-configuration")?;
        let discovery: Discovery = http
            .get(discovery_url.clone())
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let client = Self {
            http,
            discovery,
            client_id,
            client_secret,
            redirect_uri,
            keys: RwLock::new(KeyCache {
                set: JwkSet { keys: vec![] },
                fetched_at: None,
            }),
        };
        client
            .refresh_keys()
            .await
            .map_err(|e| anyhow::anyhow!("load NyxID JWKS: {e}"))?;
        Ok(client)
    }

    pub fn authorize_url(&self, state: &str, challenge: &str, nonce: &str) -> CoreResult<String> {
        let mut url = Url::parse(&self.discovery.authorization_endpoint)
            .map_err(|e| CoreError::Unavailable(format!("authorization endpoint: {e}")))?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", "openid profile email")
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("nonce", nonce);
        Ok(url.into())
    }

    /// An authorization request that adds delegated scopes to an existing
    /// grant (incremental consent), for donated model quota.
    pub fn donation_authorize_url(
        &self,
        state: &str,
        challenge: &str,
        nonce: &str,
        scope: &str,
        service_ids: &[String],
    ) -> CoreResult<String> {
        let mut url = Url::parse(&self.authorize_url(state, challenge, nonce)?)
            .map_err(|e| CoreError::Unavailable(format!("authorization endpoint: {e}")))?;
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| k != "scope")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        url.query_pairs_mut()
            .clear()
            .extend_pairs(pairs)
            .append_pair("scope", scope)
            .append_pair("include_granted_scopes", "true");
        if !service_ids.is_empty() {
            url.query_pairs_mut()
                .append_pair("requested_service_ids", &service_ids.join(" "));
        }
        Ok(url.into())
    }

    /// Exchange a refresh token for fresh delegated tokens.
    pub async fn refresh(&self, refresh_token: &str) -> CoreResult<DelegatedTokens> {
        let response = self
            .http
            .post(&self.discovery.token_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
            ])
            .send()
            .await
            .map_err(|e| CoreError::Unavailable(format!("NyxID token endpoint: {e}")))?;
        if !response.status().is_success() {
            return Err(CoreError::Unauthenticated);
        }
        let tokens: TokenResponse = response
            .json()
            .await
            .map_err(|e| CoreError::Unavailable(format!("NyxID token response: {e}")))?;
        Ok(DelegatedTokens {
            access_token: tokens.access_token.ok_or(CoreError::Unauthenticated)?,
            refresh_token: tokens.refresh_token,
        })
    }

    /// Redeem an authorization code, verify the returned ID token, and keep
    /// any delegated tokens it came with.
    pub async fn complete_with_tokens(
        &self,
        code: &str,
        verifier: &str,
        nonce: &str,
    ) -> CoreResult<(VerifiedIdentity, Option<DelegatedTokens>)> {
        let response = self
            .http
            .post(&self.discovery.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &self.redirect_uri),
                ("client_id", &self.client_id),
                ("client_secret", &self.client_secret),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|e| CoreError::Unavailable(format!("NyxID token endpoint: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            tracing::warn!(%status, "NyxID refused the authorization code");
            return Err(CoreError::Unauthenticated);
        }
        let tokens: TokenResponse = response
            .json()
            .await
            .map_err(|e| CoreError::Unavailable(format!("NyxID token response: {e}")))?;
        let id_token = tokens.id_token.ok_or(CoreError::Unauthenticated)?;
        let claims: IdClaims = self.verify(&id_token, &self.client_id).await?;
        if claims.nonce.as_deref() != Some(nonce) {
            tracing::warn!("ID token nonce mismatch");
            return Err(CoreError::Unauthenticated);
        }
        let delegated = tokens.access_token.map(|access_token| DelegatedTokens {
            access_token,
            refresh_token: tokens.refresh_token,
        });
        Ok((
            VerifiedIdentity {
                subject: PersonId(claims.sub),
                name: claims.name,
                email: claims.email,
                picture: claims.picture,
            },
            delegated,
        ))
    }

    /// Verify a bearer access token presented by an API client.
    pub async fn verify_access_token(&self, token: &str) -> CoreResult<VerifiedIdentity> {
        let claims: AccessClaims = self.verify(token, &self.discovery.issuer).await?;
        if !(claims.sa || claims.token_type.as_deref() == Some("access")) {
            return Err(CoreError::Unauthenticated);
        }
        Ok(VerifiedIdentity {
            subject: PersonId(claims.sub),
            name: None,
            email: None,
            picture: None,
        })
    }

    async fn verify<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        audience: &str,
    ) -> CoreResult<T> {
        let header = decode_header(token).map_err(|_| CoreError::Unauthenticated)?;
        if header.alg != Algorithm::RS256 {
            return Err(CoreError::Unauthenticated);
        }
        let kid = header.kid.ok_or(CoreError::Unauthenticated)?;
        let key = match self.key(&kid).await {
            Some(key) => key,
            None => {
                self.refresh_keys().await?;
                self.key(&kid).await.ok_or(CoreError::Unauthenticated)?
            }
        };
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.discovery.issuer]);
        validation.set_audience(&[audience]);
        validation.leeway = LEEWAY_SECONDS;
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        decode::<T>(token, &key, &validation)
            .map(|data| data.claims)
            .map_err(|error| {
                tracing::debug!(%error, "token verification failed");
                CoreError::Unauthenticated
            })
    }

    async fn key(&self, kid: &str) -> Option<DecodingKey> {
        let cache = self.keys.read().await;
        cache
            .set
            .find(kid)
            .and_then(|jwk| DecodingKey::from_jwk(jwk).ok())
    }

    async fn refresh_keys(&self) -> CoreResult<()> {
        let mut cache = self.keys.write().await;
        if cache
            .fetched_at
            .is_some_and(|at| at.elapsed() < JWKS_REFRESH_FLOOR)
        {
            return Ok(());
        }
        let set: JwkSet = self
            .http
            .get(&self.discovery.jwks_uri)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| CoreError::Unavailable(format!("NyxID JWKS: {e}")))?
            .json()
            .await
            .map_err(|e| CoreError::Unavailable(format!("NyxID JWKS body: {e}")))?;
        cache.set = set;
        cache.fetched_at = Some(Instant::now());
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
