use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::request::Parts,
};
use wishpool_core::{CoreError, model::Caller};

use crate::problems::Problem;

/// The authenticated caller, resolved by the binary's middleware.
#[derive(Debug, Clone)]
pub struct AuthenticatedCaller(pub Caller);

impl<S: Send + Sync> FromRequestParts<S> for AuthenticatedCaller {
    type Rejection = Problem;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthenticatedCaller>()
            .cloned()
            .ok_or(Problem(CoreError::Unauthenticated))
    }
}

impl<S: Send + Sync> OptionalFromRequestParts<S> for AuthenticatedCaller {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(parts.extensions.get::<AuthenticatedCaller>().cloned())
    }
}

/// Set only by authentication middleware; a missing marker never permits author confirmation.
#[derive(Debug, Clone)]
pub struct RequestAuthentication(pub wishpool_core::model::AuthenticationMethod);
impl<S: Send + Sync> FromRequestParts<S> for RequestAuthentication {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts
            .extensions
            .get::<Self>()
            .cloned()
            .unwrap_or(Self(wishpool_core::model::AuthenticationMethod::Bearer)))
    }
}
