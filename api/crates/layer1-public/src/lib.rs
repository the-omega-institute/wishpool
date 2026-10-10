//! Layer 1 of wishpool: the public REST projection of the Layer 2 services.
//!
//! Every route maps one request onto one application-service call. Routes
//! carry no business rules; authorization lives in Layer 2. Authentication is
//! established by middleware in the binary, which places an
//! [`AuthenticatedCaller`] in the request extensions.

mod auth;
mod problems;
mod rest;

use std::sync::Arc;

use axum::Router;
use wishpool_core::app::App;

pub use auth::{AuthenticatedCaller, RequestAuthentication};
pub use problems::{Problem, problem_response};

/// The `/api/v1` router.
pub fn router(app: Arc<App>) -> Router {
    rest::routes().with_state(app)
}
