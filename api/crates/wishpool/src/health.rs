//! `/healthz` (liveness) and `/readyz` (readiness: the database answers).

use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use serde_json::json;

use crate::store::MongoStore;

#[derive(Clone)]
pub struct Readiness {
    pub mongo: Option<MongoStore>,
}

pub fn routes(readiness: Arc<Readiness>) -> Router {
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(json!({ "status": "ok" })) }),
        )
        .route("/readyz", get(ready))
        .with_state(readiness)
}

async fn ready(State(readiness): State<Arc<Readiness>>) -> impl IntoResponse {
    match &readiness.mongo {
        Some(store) => match store.ping().await {
            Ok(()) => (StatusCode::OK, Json(json!({ "status": "ready" }))),
            Err(error) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable", "detail": error.to_string() })),
            ),
        },
        None => (
            StatusCode::OK,
            Json(json!({ "status": "ready", "storage": "memory" })),
        ),
    }
}
